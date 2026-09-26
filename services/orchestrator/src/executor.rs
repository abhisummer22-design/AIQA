use crate::domain::{SimOutcome, SimSpec, TaskDef, TaskStatus};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ExecutionOutcome {
    pub status: TaskStatus,
    pub reason: Option<String>,
    /// Full combined stdout+stderr from a real tool invocation, if any —
    /// `reason` only carries a 15-line tail (enough for an event summary);
    /// this is what `engine.rs` writes to the evidence store (SRD §15) so
    /// the complete output survives. `None` for the SimulatedExecutor and
    /// for outcomes that never produced process output (bad params,
    /// failure to launch).
    pub evidence: Option<String>,
}

/// Common Executor contract (SRD §16-17): "All use a common API Executor
/// contract" / "Each executor declares capabilities and safety requirements."
#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome;
}

/// Routes each task to the right Executor by its `executor` field
/// (PRD §21 "capability routing"), falling back to the SimulatedExecutor for
/// tasks that don't name one — this is what keeps the SRD §25 scheduler
/// tests/demo working unchanged alongside real executors.
pub struct ExecutorRegistry {
    by_kind: HashMap<String, Arc<dyn Executor>>,
    default: Arc<dyn Executor>,
}

impl ExecutorRegistry {
    pub fn new(default: Arc<dyn Executor>) -> Self {
        Self {
            by_kind: HashMap::new(),
            default,
        }
    }

    pub fn register(mut self, kind: &str, executor: Arc<dyn Executor>) -> Self {
        self.by_kind.insert(kind.to_string(), executor);
        self
    }

    /// The real capability-routing table: every executor kind a task may
    /// name and actually be routed to. Sorted so the orchestrator view
    /// (`api.rs::get_orchestrator_state`) renders a stable list rather than
    /// HashMap iteration order. A task naming none of these runs on the
    /// default (`SimulatedExecutor`); a task naming something outside this
    /// list is a real error, never a silent substitution (see `resolve`).
    pub fn registered_kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.by_kind.keys().cloned().collect();
        kinds.sort();
        kinds
    }

    /// Phase 2.1 (brief §13) — a task naming *no* executor at all still
    /// means "run the default (`SimulatedExecutor`)" (an explicit,
    /// disclosed, pre-existing convention every simulate-only task and
    /// every existing test in this repository relies on) — but a task
    /// naming an executor that genuinely is not registered is now a real
    /// error, never a silent substitution. Before this fix, a typo'd or
    /// unrecognized `executor` string (reachable only via a direct API
    /// call — the real UI only ever sends one of the registry's own known
    /// names) would silently run `SimulatedExecutor` and could produce a
    /// fabricated `PASSED` for a request that named a real tool. See
    /// `phase2_handoff_with_an_unregistered_executor_name_silently_falls_back_to_simulated`
    /// (`api_tests.rs`) for the pre-fix behavior this replaces, and its
    /// Phase 2.1 successor test for the new one.
    pub fn resolve(&self, task: &TaskDef) -> Result<Arc<dyn Executor>, String> {
        match &task.executor {
            None => Ok(self.default.clone()),
            Some(kind) => self
                .by_kind
                .get(kind)
                .cloned()
                .ok_or_else(|| format!("no executor is registered for {kind:?}")),
        }
    }
}

pub struct SimulatedExecutor;

#[async_trait]
impl Executor for SimulatedExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let spec = task.simulate.clone().unwrap_or(SimSpec {
            outcome: SimOutcome::Pass,
            delay_ms: 150,
        });
        tokio::time::sleep(std::time::Duration::from_millis(spec.delay_ms)).await;
        match spec.outcome {
            SimOutcome::Pass => ExecutionOutcome {
                status: TaskStatus::Passed,
                reason: None,
                evidence: None,
            },
            SimOutcome::Fail => ExecutionOutcome {
                status: TaskStatus::Failed,
                reason: Some("simulated failure".into()),
                evidence: None,
            },
            SimOutcome::Timeout => ExecutionOutcome {
                status: TaskStatus::Timeout,
                reason: Some("simulated timeout".into()),
                evidence: None,
            },
            SimOutcome::InfraError => ExecutionOutcome {
                status: TaskStatus::InfraError,
                reason: Some("simulated infrastructure error".into()),
                evidence: None,
            },
        }
    }
}

// --- shared process-execution helpers ------------------------------------

/// Task `params` arrive straight from the (network-reachable) `POST /runs`
/// body and end up as arguments to `npx`/`newman`/`maestro`. On Windows
/// those are `.cmd`/shim launchers that only run under a shell (see
/// `shell_command` below), and `cmd /C` re-parses its whole command line —
/// so an unsanitized param is a command-injection vector (e.g. a `grep`
/// value of `x" & calc.exe & "`). Reject anything outside a conservative
/// safe set instead of trying to escape shell metacharacters; also reject
/// `..` to keep path-like params from walking out of `project_dir`.
fn sanitize_token(s: &str) -> Result<String, String> {
    let ok = !s.is_empty()
        && !s.contains("..")
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_' | '/' | '\\' | ':')
        });
    if ok {
        Ok(s.to_string())
    } else {
        Err(format!("rejected unsafe executor parameter: {s:?}"))
    }
}

/// `npx`/`newman`/`maestro` resolve to shell shims on Windows (`.cmd`/entry
/// points) that `std::process::Command` can't launch directly — it needs a
/// shell in front of them. Real binaries (`python`) don't need this and
/// should be invoked directly. `args` must already be sanitized
/// (`sanitize_token`) — they're concatenated into one command-line string
/// for `cmd /C`, which does not respect argv boundaries the way a direct
/// exec does.
///
/// Building that line: only args containing a space are quoted (npx/newman
/// are themselves .cmd batch shims that forward args onward via `%*`, which
/// does not re-strip quotes — quoting every arg unconditionally was observed
/// to make npx see literal quote characters and mangle its own argument
/// parsing). `sanitize_token` already forbids `"` and shell metacharacters
/// in `a`, so this quoting is unambiguous. The line is then passed via
/// `raw_arg` — `Command`'s normal `.arg()` would itself re-quote the whole
/// line (it contains spaces), which doubles up with the quotes already
/// embedded in it and breaks cmd.exe's parsing.
#[cfg(windows)]
fn shell_command(program: &str, args: &[&str], cwd: &Path) -> tokio::process::Command {
    use std::os::windows::process::CommandExt;

    let mut line = format!("chcp 65001 >nul && {program}");
    for a in args {
        if a.contains(' ') {
            line.push_str(&format!(" \"{a}\""));
        } else {
            line.push(' ');
            line.push_str(a);
        }
    }
    let mut std_cmd = std::process::Command::new("cmd");
    std_cmd.arg("/C").raw_arg(line.as_str()).current_dir(cwd);
    // Output here is always captured through a pipe (`.output()`), never a
    // real console — `chcp` above only affects an interactive console, not
    // a pipe's encoding. Python-based tools (schemathesis) pick their
    // stdout encoding from the OS ANSI codepage in that case and crash on
    // their own emoji/box-drawing output unless told explicitly to use
    // UTF-8; Node-based tools (npx/newman/maestro) ignore these vars, so
    // it's harmless to set them unconditionally for every adapter here
    // rather than special-casing Python ones.
    std_cmd
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");
    let mut cmd = tokio::process::Command::from(std_cmd);
    // B5 — the engine bounds every invocation with `tokio::time::timeout`,
    // which drops this future on expiry. Without kill_on_drop the child
    // process would outlive the task that was supposed to own it, leaking a
    // hung tool for the lifetime of the orchestrator.
    cmd.kill_on_drop(true);
    cmd
}

#[cfg(not(windows))]
fn shell_command(program: &str, args: &[&str], cwd: &Path) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    c.args(args).current_dir(cwd);
    // B5 — see the Windows branch: a timed-out invocation is dropped, and
    // the child must die with it rather than outliving the run.
    c.kill_on_drop(true);
    c
}

// --- M2 remediation: SSRF guard for URL-shaped executor parameters -------

/// `pytest`'s `base_url` and `schemathesis`'s `url` used to reach the
/// subprocess boundary validated only by `sanitize_token`'s character-class
/// check, which permits `:` and `/` and therefore any host —
/// `http://169.254.169.254/...` (cloud metadata) passes it, per the Backend
/// Audit's own confirmed reproduction (M2). This is a different kind of
/// check from `sanitize_token`: that one stops shell/command injection;
/// this one stops the orchestrator itself being used as a network pivot
/// against link-local, private, or otherwise internal infrastructure —
/// Schemathesis in particular will *actively fuzz* whatever target it's
/// given from the orchestrator's own network position.
///
/// The one destination these two adapters are actually documented to
/// target is this orchestrator itself (`PytestExecutor`/`SchemathesisExecutor`'s
/// own doc comments: "default matches this orchestrator's own address") —
/// loopback, on this orchestrator's real port. That is the explicit allowed
/// exception; everything else in a private/internal range is rejected,
/// including loopback on any *other* port (so this guard cannot be used to
/// probe some other service quietly listening on 127.0.0.1).
const ORCHESTRATOR_PORT: u16 = 8080;

/// Real destination classification, not a string/prefix match — an IP is
/// what a request actually reaches, regardless of what hostname a caller
/// wrote. `is_link_local` alone would already reject the cloud metadata
/// address (`169.254.169.254`); the rest of this list closes the other
/// private/internal ranges the M2 finding's own fix direction named
/// ("allow-list schemes and hosts").
fn is_blocked_ip(ip: &std::net::IpAddr, port: u16) -> bool {
    use std::net::IpAddr;
    if ip.is_loopback() {
        // The one explicit allowed exception — and only on the real port,
        // so this never becomes a general "any local port" allowance.
        return port != ORCHESTRATOR_PORT;
    }
    match ip {
        IpAddr::V4(v4) => {
            v4.is_link_local() // 169.254.0.0/16 — includes the cloud metadata address
                || v4.is_private() // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_unspecified() // 0.0.0.0
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
        }
        IpAddr::V6(v6) => {
            v6.is_unicast_link_local() // fe80::/10
                || v6.is_unique_local() // fc00::/7
                || v6.is_multicast()
                || v6.is_unspecified()
        }
    }
}

/// Validates a caller-supplied target URL before it is ever built into a
/// subprocess invocation. Only `http`/`https` are allowed (blocks
/// `file://`, `gopher://`, and similar scheme-smuggling tricks). A literal
/// IP is checked directly; a hostname is resolved and *every* address it
/// resolves to is checked — a hostname's spelling proves nothing about
/// where it actually points (DNS rebinding, or an internal name that
/// simply resolves to a metadata/private address), so the check has to
/// happen after resolution, against the real address, not the string.
pub(crate) async fn validate_target_url(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|e| format!("rejected target URL {raw:?}: {e}"))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(format!(
            "rejected target URL {raw:?}: only http/https schemes are allowed, got {:?}",
            url.scheme()
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("rejected target URL {raw:?}: no host"))?;
    let port = url.port_or_known_default().unwrap_or(80);

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return if is_blocked_ip(&ip, port) {
            Err(format!("rejected target {host}:{port}: address is in a disallowed private/internal/link-local range"))
        } else {
            Ok(())
        };
    }

    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("rejected target {host}:{port}: could not resolve host ({e})"))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!(
            "rejected target {host}:{port}: host resolved to no addresses"
        ));
    }
    for addr in &addrs {
        if is_blocked_ip(&addr.ip(), port) {
            return Err(format!(
                "rejected target {host}:{port}: resolves to {}, which is in a disallowed private/internal/link-local range",
                addr.ip()
            ));
        }
    }
    Ok(())
}

// --- Critical-finding remediation: executor process containment ----------

/// Architecture Audit "PRIORITY 1" — executors are, and remain, plain host
/// subprocesses; that finding's own suggested fix ("migrate to Docker
/// containers or microVMs") is not achievable in this dev environment,
/// checked fresh this session: `docker`/`podman` are absent from `PATH`
/// (same fact `store.rs`'s module doc comment already recorded for
/// Postgres), and `wsl --status` reports WSL itself is not installed —
/// enabling it is a Windows-feature change requiring elevation this
/// process does not have, and is exactly the kind of system-level change
/// that should be an explicit, asked-for decision, not something silently
/// done mid-session. This module is the real, different-in-kind mechanism
/// that *is* achievable here, with no new system install and no
/// elevation: a genuine Win32 Job Object around every executor invocation.
///
/// What it actually closes: `tokio::process::Command::kill_on_drop(true)`
/// (already in `shell_command`, B5) only guarantees the *direct* child
/// dies when its handle is dropped — it does not reach grandchildren. A
/// `cmd /C npx playwright test ...` chain can spawn a real browser process
/// underneath `npx`; a single `TerminateProcess` on the `cmd.exe`/`npx`
/// process does not obligate Windows to also kill that browser. A Job
/// Object does: every process ever assigned to it dies together the
/// moment the job's last handle closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`),
/// which happens automatically when `ProcessJob` is dropped — including on
/// the B5 timeout path, where the executor future (and everything it
/// owns) is dropped without an explicit kill call anywhere. It also caps
/// how many processes and how much memory one invocation's entire tree may
/// ever use (`JOB_OBJECT_LIMIT_ACTIVE_PROCESS` / `_JOB_MEMORY`) — a real
/// backstop against a runaway or fork-bombing tool.
///
/// What it deliberately does **not** claim: filesystem or network
/// isolation. A process in this job can still read/write anywhere its own
/// Windows ACLs allow and reach any network destination `executor.rs`'s
/// SSRF guard (M2, above) doesn't already block. That gap is real, is not
/// hidden, and is recorded with a dated acceptance in this session's own
/// report rather than silently dropped a second time — closing it fully
/// needs a container runtime or Windows Sandbox, both of which need a
/// system-level decision this session explicitly did not make unilaterally
/// (see this module's own doc comment above and the report's §"Executor
/// Sandboxing").
#[cfg(windows)]
mod job_containment {
    use std::ffi::c_void;
    use std::os::windows::io::RawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// One unnamed Job Object per executor invocation. Dropping this is
    /// the actual containment mechanism (see module doc comment) — closing
    /// the last handle to a `KILL_ON_JOB_CLOSE` job terminates every
    /// process still assigned to it. Every field/method here is a thin,
    /// direct wrapper around the documented Win32 contract; the only
    /// judgment calls are the limit values `new`'s caller picks.
    pub struct ProcessJob(RawHandle);

    // An opaque Win32 kernel-object handle is not a pointer into this
    // process's own memory and carries no thread-affinity — safe to move
    // and share across threads/tasks (this job outlives the single
    // `.await` that owns the child it was created for, per `run_contained`
    // below, but never needs concurrent access from two threads at once).
    unsafe impl Send for ProcessJob {}
    unsafe impl Sync for ProcessJob {}

    impl ProcessJob {
        pub fn new(max_processes: u32, max_memory_bytes: usize) -> std::io::Result<Self> {
            // SAFETY: `CreateJobObjectW(null, null)` is the documented call
            // shape for an unnamed job with default security attributes;
            // the returned handle is checked for the documented failure
            // value (NULL) before being trusted.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error());
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.BasicLimitInformation.ActiveProcessLimit = max_processes;
            info.JobMemoryLimit = max_memory_bytes;

            // SAFETY: `info` is a `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`
            // fully zero-initialized before the fields this call actually
            // reads are set, matching `SetInformationJobObject`'s
            // documented contract for the `JobObjectExtendedLimitInformation`
            // info class; the size passed is that struct's real size.
            let ok = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                let err = std::io::Error::last_os_error();
                unsafe { CloseHandle(handle) };
                return Err(err);
            }
            Ok(Self(handle))
        }

        /// Assigns a process (by its raw OS handle) to this job. A process
        /// can belong to only one job at a time on the Windows versions
        /// this targets; a `tokio::process::Child` fresh from `.spawn()`
        /// has never been assigned to any job, so this is always a clean
        /// first assignment, never a re-parent.
        pub fn assign(&self, process_handle: RawHandle) -> std::io::Result<()> {
            // SAFETY: `process_handle` comes from `Child::raw_handle()` on
            // a process this same call site just spawned and still owns —
            // a live, valid handle for the duration of this call.
            let ok = unsafe { AssignProcessToJobObject(self.0, process_handle) };
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }
    }

    impl Drop for ProcessJob {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a job handle this struct exclusively
            // owns (never cloned, never handed out) — closing it here is
            // the one and only place it's ever closed.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub(super) fn raw_handle(child: &tokio::process::Child) -> Option<RawHandle> {
        child.raw_handle()
    }
}

/// Real limits, not aspirational ones: a real Playwright/Newman/Maestro/
/// Schemathesis invocation is one tool plus a handful of real child
/// processes (a browser and its renderer/GPU helper processes, chiefly) —
/// 64 is generous headroom over that, not a number picked to never
/// matter. 2 GiB is enough for a real headless-Chromium run with margin;
/// still a real ceiling against a genuinely runaway process.
#[cfg(windows)]
const EXECUTOR_JOB_MAX_PROCESSES: u32 = 64;
#[cfg(windows)]
const EXECUTOR_JOB_MAX_MEMORY_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Spawns `cmd`, contained in a fresh Job Object for the duration of the
/// call (Windows) or run directly (every other target — see this module's
/// header comment: no executor is wired up on a non-Windows target today,
/// so there is nothing this fallback needs to contain yet). Replaces the
/// bare `cmd.output().await` every adapter used to call directly: `.output()`
/// itself spawns *and* waits, leaving no window to assign the child to a
/// job between those two steps, so this does `.spawn()` — assign — `.wait_with_output()`
/// instead. On a spawn failure, or if job creation/assignment itself
/// fails, the process (if one was spawned) is still awaited rather than
/// abandoned — a job-object failure must never leave an *unquarantined*
/// process running for longer than the ordinary, un-contained path would
/// have.
async fn run_contained(
    mut cmd: tokio::process::Command,
    label: &str,
) -> Result<std::process::Output, std::io::Error> {
    // `Command::output()` (what every call site here used before this
    // session) pipes stdout/stderr automatically as part of that one call.
    // `spawn()` does not — without this, `wait_with_output` below would
    // return empty output for a real, successful process, silently
    // breaking every adapter's actual result reporting.
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = cmd.spawn()?;

    #[cfg(windows)]
    {
        match job_containment::ProcessJob::new(
            EXECUTOR_JOB_MAX_PROCESSES,
            EXECUTOR_JOB_MAX_MEMORY_BYTES,
        ) {
            Ok(job) => {
                if let Some(handle) = job_containment::raw_handle(&child) {
                    if let Err(e) = job.assign(handle) {
                        tracing::warn!("{label}: failed to assign process to containment job, continuing un-contained: {e}");
                    }
                }
                // The job must outlive the process it contains, or closing
                // it (which kills everything still assigned) would kill
                // the very process this call is trying to wait on. Held
                // until after `wait_with_output` below, then dropped.
                let output = child.wait_with_output().await;
                drop(job);
                return output;
            }
            Err(e) => {
                tracing::warn!(
                    "{label}: failed to create containment job, running un-contained: {e}"
                );
            }
        }
    }

    child.wait_with_output().await
}

fn tail_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// Turn a completed subprocess `Output` into an ExecutionOutcome. A handful
/// of well-known "the tool itself is missing" signatures are classified as
/// INFRA_ERROR rather than FAILED, per FRD §2's requirement to keep product
/// and infrastructure failures distinct; anything else non-zero is treated
/// as a genuine test failure. This heuristic is necessarily incomplete (a
/// crashing third-party tool can still be misread as a product failure) —
/// see executors/maestro/README.md for a live example.
fn outcome_from_output(output: std::process::Output, label: &str) -> ExecutionOutcome {
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence = if combined.trim().is_empty() {
        None
    } else {
        Some(combined.clone())
    };
    if output.status.success() {
        return ExecutionOutcome {
            status: TaskStatus::Passed,
            reason: None,
            evidence,
        };
    }
    let lower = combined.to_lowercase();
    let looks_like_missing_tool = lower
        .contains("is not recognized as an internal or external command")
        || lower.contains("command not found")
        || lower.contains("enoent");
    let status = if looks_like_missing_tool {
        TaskStatus::InfraError
    } else {
        TaskStatus::Failed
    };
    ExecutionOutcome {
        status,
        reason: Some(format!(
            "{label} exited with {:?}\n{}",
            output.status.code(),
            tail_lines(&combined, 15)
        )),
        evidence,
    }
}

/// The selector a real, compiled browser test failed on, read from the
/// structured line `executors/playwright/helpers/healing.ts` prints when its
/// runtime self-healing gives up on a selector:
/// `AIQA_HEALING_EVENT {"...","primarySelector":"<selector>","resolved":false}`.
///
/// That line lives only in a task's *full* captured output
/// (`ExecutionOutcome.evidence`), never in the 15-line tail `reason` keeps.
/// Events with `"resolved": true` (a fallback strategy worked) are not failures
/// and are ignored. A test stops at its first throw, so if several unresolved
/// events appear the last one is the one that ended the run. Returns `None` for
/// anything else - a selector is never guessed.
pub fn unresolved_healing_selector(evidence: &str) -> Option<String> {
    const MARKER: &str = "AIQA_HEALING_EVENT ";
    evidence
        .lines()
        .filter_map(|line| {
            let json = &line[line.find(MARKER)? + MARKER.len()..];
            let event: serde_json::Value = serde_json::Deserializer::from_str(json)
                .into_iter::<serde_json::Value>()
                .next()?
                .ok()?;
            if event.get("resolved")?.as_bool()? {
                return None;
            }
            event
                .get("primarySelector")?
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(ToString::to_string)
        })
        .next_back()
}

fn spawn_error_outcome(label: &str, err: std::io::Error) -> ExecutionOutcome {
    ExecutionOutcome {
        status: TaskStatus::InfraError,
        reason: Some(format!("failed to launch {label}: {err}")),
        evidence: None,
    }
}

/// Fetches `task.params[key]` (or `default`), rejecting anything
/// `sanitize_token` flags as unsafe. An empty value always passes through —
/// several adapters use "" to mean "no filter" (e.g. Playwright's `grep`).
fn safe_param(task: &TaskDef, key: &str, default: &str) -> Result<String, ExecutionOutcome> {
    let raw = task
        .params
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default);
    if raw.is_empty() {
        return Ok(String::new());
    }
    sanitize_token(raw).map_err(|msg| ExecutionOutcome {
        status: TaskStatus::InfraError,
        reason: Some(msg),
        evidence: None,
    })
}

/// Extracts `Ok`, or returns the `Err` `ExecutionOutcome` directly from the
/// enclosing `execute()` — every adapter below is `async fn execute(...) ->
/// ExecutionOutcome`, so a bare `return` is always valid at the call site.
macro_rules! param_or_return {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(outcome) => return outcome,
        }
    };
}

// --- real adapters ---------------------------------------------------------

// --- run-time secrets ------------------------------------------------------

/// The variables a task says its spec reads (`params.secrets`: `AIQA_LOGIN_*` / `AIQA_SECRET_*` names — names only,
/// never values). The spec reads them itself (`helpers/secrets.ts`); this is what lets the executor refuse to start
/// a run whose secrets are not set, and scrub their values from what the run prints.
fn secret_names_param(task: &TaskDef) -> Result<Vec<String>, ExecutionOutcome> {
    let Some(raw) = task.params.get("secrets") else {
        return Ok(Vec::new());
    };
    let rejected = |why: &str| ExecutionOutcome {
        status: TaskStatus::InfraError,
        reason: Some(format!("rejected executor parameter secrets: {why}")),
        evidence: None,
    };
    let array = raw
        .as_array()
        .ok_or_else(|| rejected("expected an array of variable names"))?;
    let mut names = Vec::with_capacity(array.len());
    for item in array {
        let name = item
            .as_str()
            .filter(|n| crate::execution_plan::is_valid_secret_name(n))
            .ok_or_else(|| {
                rejected("every entry must be a variable name starting AIQA_LOGIN_ or AIQA_SECRET_")
            })?;
        names.push(name.to_string());
    }
    Ok(names)
}

/// The values of `names` from the orchestrator's environment, or the `INFRA_ERROR` outcome that names the ones that
/// are not set (names only).
fn read_secrets(names: &[String]) -> Result<Vec<String>, ExecutionOutcome> {
    let missing = crate::execution_plan::missing_secrets(names);
    if !missing.is_empty() {
        return Err(ExecutionOutcome {
            status: TaskStatus::InfraError,
            reason: Some(crate::execution_plan::missing_secrets_message(&missing)),
            evidence: None,
        });
    }
    Ok(names
        .iter()
        .filter_map(|name| crate::ai_router::env_value(name))
        .collect())
}

/// The shortest secret that is scrubbed: a shorter one would also match ordinary words in the output.
const MIN_REDACTED_SECRET_LEN: usize = 4;

/// `text` with every secret value replaced by `***`. What a run prints — an error's code frame, a failed assertion —
/// can quote what was typed; the value must reach neither the stored evidence nor a run's reason.
pub fn redact_secrets(text: &str, values: &[String]) -> String {
    let mut forms: Vec<String> = Vec::new();
    for value in values.iter().filter(|v| v.len() >= MIN_REDACTED_SECRET_LEN) {
        forms.push(value.clone());
        // The way a JSON string or a source line would spell it.
        if let Ok(quoted) = serde_json::to_string(value) {
            let escaped = quoted[1..quoted.len() - 1].to_string();
            if escaped != *value {
                forms.push(escaped);
            }
        }
    }
    // Longest first, so a secret that contains another is not left half-scrubbed.
    forms.sort_by_key(|form| std::cmp::Reverse(form.len()));
    forms.iter().fold(text.to_string(), |out, form| {
        out.replace(form.as_str(), "***")
    })
}

fn redact_outcome(mut outcome: ExecutionOutcome, values: &[String]) -> ExecutionOutcome {
    if values.is_empty() {
        return outcome;
    }
    outcome.reason = outcome.reason.map(|r| redact_secrets(&r, values));
    outcome.evidence = outcome.evidence.map(|e| redact_secrets(&e, values));
    outcome
}

/// Web executor (SRD §17: "Playwright handles web"). `params.spec` selects
/// the spec file, `params.grep` selects one test title within it (both
/// relative to `project_dir`, which points at executors/playwright).
/// `params.secrets` (optional) names the environment variables the spec reads
/// at run time (`env:NAME` values): the run is refused if one is not set, and
/// their values are scrubbed from the evidence.
pub struct PlaywrightExecutor {
    pub project_dir: PathBuf,
}

#[async_trait]
impl Executor for PlaywrightExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let spec = param_or_return!(safe_param(task, "spec", "tests/sample.spec.ts"));
        let grep = param_or_return!(safe_param(task, "grep", ""));
        let secret_names = param_or_return!(secret_names_param(task));
        let secret_values = param_or_return!(read_secrets(&secret_names));
        let mut args = vec!["playwright", "test", spec.as_str(), "--reporter=line"];
        if !grep.is_empty() {
            args.push("--grep");
            args.push(grep.as_str());
        }
        match run_contained(shell_command("npx", &args, &self.project_dir), "playwright").await {
            Ok(output) => finish_playwright(output, &secret_values),
            Err(e) => spawn_error_outcome("playwright", e),
        }
    }
}

/// What a finished Playwright run reports: its outcome, with every secret value scrubbed from the reason and from
/// the evidence that is stored.
fn finish_playwright(output: std::process::Output, secret_values: &[String]) -> ExecutionOutcome {
    redact_outcome(outcome_from_output(output, "playwright"), secret_values)
}

/// API executor, Postman collections (SRD §16: "Newman is the primary
/// Postman collection adapter"). `params.folder` selects which folder of
/// `collection.json` (in executors/newman) to run.
pub struct NewmanExecutor {
    pub project_dir: PathBuf,
}

#[async_trait]
impl Executor for NewmanExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let folder = param_or_return!(safe_param(task, "folder", "health check passes"));
        let args = vec![
            "newman",
            "run",
            "collection.json",
            "--folder",
            folder.as_str(),
        ];
        match run_contained(shell_command("npx", &args, &self.project_dir), "newman").await {
            Ok(output) => outcome_from_output(output, "newman"),
            Err(e) => spawn_error_outcome("newman", e),
        }
    }
}

/// Native API executor (SRD §16: "HTTPX/Pytest is the native API executor").
/// `params.node_id` selects a specific test (pytest node id syntax, e.g.
/// `test_health.py::test_health_returns_200_and_ok_body`); omitted runs the
/// whole file. `params.base_url` overrides the target (default matches this
/// orchestrator's own address, since no separate product-under-test exists
/// yet). Invoked directly (no `cmd /C`), so this one isn't exposed to the
/// shell-reparsing risk `shell_command` guards against — params are still
/// sanitized for consistency and to block pytest flag/path-argument injection
/// (e.g. a node_id of `--maxfail=1`).
pub struct PytestExecutor {
    pub project_dir: PathBuf,
}

#[async_trait]
impl Executor for PytestExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let node_id = param_or_return!(safe_param(task, "node_id", "test_health.py"));
        let base_url = param_or_return!(safe_param(task, "base_url", "http://localhost:8080"));
        if let Err(reason) = validate_target_url(&base_url).await {
            return ExecutionOutcome {
                status: TaskStatus::InfraError,
                reason: Some(reason),
                evidence: None,
            };
        }
        let mut cmd = tokio::process::Command::new("python");
        cmd.args(["-m", "pytest", "-q", node_id.as_str()])
            .current_dir(&self.project_dir)
            .env("AIQA_TARGET_BASE_URL", base_url)
            // B5 — invoked directly rather than through `shell_command`, so
            // it needs the same kill-on-timeout guarantee.
            .kill_on_drop(true);
        match run_contained(cmd, "pytest").await {
            Ok(output) => outcome_from_output(output, "pytest"),
            Err(e) => spawn_error_outcome("pytest", e),
        }
    }
}

/// Mobile executor (SRD §17: "Maestro handles primary mobile flows").
/// `params.flow` selects the flow file (relative to executors/maestro). See
/// executors/maestro/README.md — there is no device/emulator in this dev
/// environment, so this adapter is exercised for real subprocess handling
/// but not for a genuine mobile flow result.
pub struct MaestroExecutor {
    pub project_dir: PathBuf,
}

/// Phase 2.1 (brief §14) — real identity verification, not a fake fix.
/// Confirmed live this session (see the Phase 2 implementation report,
/// §21/§30): this dev machine's `maestro` on `PATH` genuinely resolves to
/// an unrelated Python/Typer-based ML tool (its own real, observed output:
/// `WARNING cannot import recipe for Florence-2/PaliGemma 2/Qwen2.5-VL`,
/// then `Usage: maestro [OPTIONS] COMMAND [ARGS]...` / `No such command
/// 'test'.` when asked to run a flow), not mobile.dev's Maestro automation
/// CLI. Run *before* the real `test <flow>` invocation, using the same
/// real, contained subprocess mechanism (`run_contained`) — a real,
/// distinguishable failure here is reported as an infrastructure/tooling
/// problem (`EXECUTOR_UNAVAILABLE` in the `reason` text, `InfraError`
/// status — this project's existing status vocabulary has no separate
/// "unavailable" variant; `InfraError` is the correct existing category
/// for "not a product/test failure"), never silently attempted and
/// misattributed to the test case as a `FAILED` result.
async fn verify_maestro_identity(project_dir: &Path) -> Result<(), String> {
    let output = match run_contained(
        shell_command("maestro", &["--version"], project_dir),
        "maestro-identity-check",
    )
    .await
    {
        Ok(o) => o,
        Err(e) => {
            return Err(format!(
                "could not invoke the 'maestro' binary on PATH to verify its identity: {e}"
            ))
        }
    };
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_lowercase();

    // Real, directly-observed signature of the specific unrelated tool
    // confirmed occupying `maestro` on PATH in this dev environment.
    const KNOWN_WRONG_TOOL_MARKERS: [&str; 4] =
        ["florence", "paligemma", "qwen", "cannot import recipe"];
    if KNOWN_WRONG_TOOL_MARKERS
        .iter()
        .any(|m| combined.contains(m))
    {
        return Err(
            "the 'maestro' binary on PATH is a different, unrelated tool (detected real ML-recipe warnings characteristic of a Python vision-model CLI, not the mobile.dev Maestro automation CLI)"
                .to_string(),
        );
    }
    // A real mobile.dev Maestro `--version` prints a bare version string.
    // This environment's confirmed-wrong tool instead prints a multi-line
    // Click/Typer "Usage: ... [OPTIONS] COMMAND [ARGS]..." block for any
    // invocation it doesn't recognize — a real, generalizable signature of
    // "wrong CLI family", not specific to today's exact wrong tool.
    if combined.contains("usage:") && combined.contains("[options]") {
        return Err(
            "the 'maestro' binary on PATH did not return a real version string — it returned a command-line usage/help block, inconsistent with the mobile.dev Maestro automation CLI"
                .to_string(),
        );
    }
    Ok(())
}

#[async_trait]
impl Executor for MaestroExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        if let Err(reason) = verify_maestro_identity(&self.project_dir).await {
            return ExecutionOutcome {
                status: TaskStatus::InfraError,
                reason: Some(format!("EXECUTOR_UNAVAILABLE: {reason}")),
                evidence: None,
            };
        }
        let flow = param_or_return!(safe_param(task, "flow", "flow.yaml"));
        let args = vec!["test", flow.as_str()];
        match run_contained(
            shell_command("maestro", &args, &self.project_dir),
            "maestro",
        )
        .await
        {
            Ok(output) => outcome_from_output(output, "maestro"),
            Err(e) => spawn_error_outcome("maestro", e),
        }
    }
}

/// API-contract fuzzing executor (build report: "Schemathesis — Not
/// built"). `params.schema` selects the OpenAPI document (relative to
/// executors/schemathesis, default `openapi.yaml`, which describes this
/// orchestrator's own `/health`); `params.url` is the base URL under test.
/// Property-based, not example-based like the other adapters — a single run
/// generates multiple request cases per operation and can turn up findings
/// (e.g. wrong status code for an unlisted HTTP method) that nobody wrote a
/// test for, which is the actual point of it. Windows: the `schemathesis`
/// console-script entry point is a real `.exe` launcher (not a `.cmd`
/// shim like npx-based tools), but it's still routed through
/// `shell_command` for the `chcp`/`PYTHONIOENCODING` handling — direct
/// invocation crashes on Schemathesis's own Unicode output when stdout is a
/// pipe (see `shell_command`'s doc comment).
pub struct SchemathesisExecutor {
    pub project_dir: PathBuf,
}

#[async_trait]
impl Executor for SchemathesisExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let schema = param_or_return!(safe_param(task, "schema", "openapi.yaml"));
        let url = param_or_return!(safe_param(task, "url", "http://localhost:8080"));
        if let Err(reason) = validate_target_url(&url).await {
            return ExecutionOutcome {
                status: TaskStatus::InfraError,
                reason: Some(reason),
                evidence: None,
            };
        }
        let args = vec!["run", schema.as_str(), "--url", url.as_str()];
        match run_contained(
            shell_command("schemathesis", &args, &self.project_dir),
            "schemathesis",
        )
        .await
        {
            Ok(output) => outcome_from_output(output, "schemathesis"),
            Err(e) => spawn_error_outcome("schemathesis", e),
        }
    }
}

// --- Phase 2.1: compiled-plan executors -----------------------------------

/// Real, native (no subprocess) API executor for a compiled `ExecutionPlan`
/// (brief §10 "API / Newman / Existing HTTP Executor"). Deliberately does
/// NOT reuse `NewmanExecutor` — Newman only ever runs a fixed, pre-existing
/// `collection.json` folder (`executors/newman/`), which has no way to
/// accept a dynamically compiled method/url/headers/body at all without
/// either mutating that shared fixture file at runtime (unsafe, racy across
/// concurrent runs) or writing one out per run (real but needlessly heavy
/// next to a native HTTP call). This executor performs the plan's actual
/// `REQUEST` against its actual `url` via `reqwest` directly, then
/// evaluates the plan's actual `ASSERT_*` actions against the real
/// response — the test case's own compiled content, not a fixture.
/// Phase 4 (API Testing page) — real, per-assertion result data, appended to
/// this executor's own evidence log as one final, clearly-prefixed JSON line
/// (`RESULT_SUMMARY_JSON:`), not a second, parallel result channel. Every
/// field here is a genuine, real value observed during the one real HTTP
/// call this executor makes — never computed independently of it. Consumed
/// by `api_testing.rs`'s own `parse_result_summary`, which does nothing
/// `CompiledApiExecutor` itself couldn't have returned directly if
/// `ExecutionOutcome` carried structured data — this is the minimal, honest
/// way to expose that same real data without changing every executor's
/// shared result type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiAssertionResult {
    pub description: String,
    pub actual: String,
    pub passed: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiResultSummary {
    pub status: u16,
    /// Real, measured wall-clock duration of the one `reqwest` call this
    /// executor made (`Instant` around `.send().await` only) — distinct
    /// from the task's own `started_at`/`finished_at` (which also includes
    /// real scheduling/dispatch overhead). See the Phase 4 report §22 for
    /// which one the frontend shows as "network request duration" vs.
    /// "end-to-end operation duration".
    pub request_duration_ms: u64,
    /// Real response headers, with a fixed, small set of known-sensitive
    /// names (Authorization/Cookie/Set-Cookie/Proxy-Authorization/API-key
    /// variants) redacted before this ever leaves the process — never sent
    /// to the frontend, never persisted in evidence, unredacted.
    pub response_headers: Vec<(String, String)>,
    pub assertions: Vec<ApiAssertionResult>,
}

const SENSITIVE_HEADER_NAMES: [&str; 6] = [
    "authorization",
    "cookie",
    "set-cookie",
    "proxy-authorization",
    "x-api-key",
    "api-key",
];

fn is_sensitive_header(name: &str) -> bool {
    SENSITIVE_HEADER_NAMES
        .iter()
        .any(|s| name.eq_ignore_ascii_case(s))
}

/// Real, distinct failure-category classification of a genuine `reqwest`
/// transport error (Phase 4 brief §16's own required vocabulary) — every
/// branch here reflects a real `reqwest`/`hyper` error classification
/// method (`is_timeout`/`is_connect`/`is_request` etc.), never a guess from
/// string matching on the error's own `Display` text.
pub(crate) fn classify_reqwest_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        format!("REQUEST_TIMEOUT: {e}")
    } else if e.is_connect() {
        let text = e.to_string();
        if text.to_lowercase().contains("dns") {
            format!("DNS_FAILED: {e}")
        } else {
            format!("CONNECTION_FAILED: {e}")
        }
    } else if e.is_request() && e.to_string().to_lowercase().contains("tls") {
        format!("TLS_FAILED: {e}")
    } else {
        format!("EXECUTION_FAILED: {e}")
    }
}

/// Real, native (no subprocess) API executor for a compiled `ExecutionPlan`
/// (Phase 2.1) — reused unmodified as the execution engine behind the Phase
/// 4 API Testing page's own ad-hoc requests (`api_testing.rs`), which
/// construct an `ExecutionPlan` directly instead of via the compiler. `timeout`
/// (Phase 4, `API_TEST_TIMEOUT_SECS`, default 30s) gives a genuinely
/// unresponsive target a real, distinguishable `REQUEST_TIMEOUT` outcome
/// instead of hanging the task indefinitely — the one real gap found while
/// auditing this executor for the API Testing page's own explicit
/// REQUEST_TIMEOUT/DNS_FAILED/CONNECTION_FAILED/TLS_FAILED requirement.
pub struct CompiledApiExecutor {
    pub timeout: Duration,
    /// Built once and reused across every `execute()` call — real HTTP
    /// keep-alive connection pooling, not a fresh TCP connection (and, for
    /// Phase 6's repeated-workload use of `execute_plan_once`, fresh local
    /// ephemeral port) per single request. A prior version built a new
    /// `reqwest::Client` inside `execute_plan_once` on every call; that was
    /// harmless for one request at a time but genuinely exhausted local
    /// ephemeral ports (`TIME_WAIT` pileup) under a real, high-volume
    /// performance test against `127.0.0.1` — caught during this phase's
    /// own live verification (see the report's bug log).
    client: reqwest::Client,
}

impl CompiledApiExecutor {
    pub fn from_env() -> Self {
        let timeout_secs: u64 = std::env::var("API_TEST_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        let timeout = Duration::from_secs(timeout_secs);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { timeout, client }
    }
}

/// Real, structured result of running every `Action` in a single API
/// `ExecutionPlan` exactly once — the same real request/assertion logic
/// `CompiledApiExecutor::execute` has always used, extracted so Phase 6's
/// performance workload engine (`performance_testing.rs`) can reuse this
/// *exact* real reqwest-calling/assertion-evaluating core as its
/// per-request worker for many repeated samples, without re-running the
/// (redundant, once a target is already validated and unchanging across a
/// whole workload) structural/SSRF `validate_plan` check on every single
/// request — that check still runs once, up front, in both callers
/// (`CompiledApiExecutor::execute` below, and the performance orchestrator
/// before it starts a workload).
pub struct PlanExecutionResult {
    pub status: TaskStatus,
    pub reason: Option<String>,
    pub http_status: Option<u16>,
    pub duration_ms: u64,
    pub response_headers: Vec<(String, String)>,
    pub assertions: Vec<ApiAssertionResult>,
    pub log: String,
}

pub async fn execute_plan_once(
    client: &reqwest::Client,
    plan: &crate::execution_plan::ExecutionPlan,
) -> PlanExecutionResult {
    use crate::execution_plan::Action;

    let mut log = String::new();
    let mut last_response: Option<(reqwest::StatusCode, reqwest::header::HeaderMap, String)> = None;
    let mut failures: Vec<String> = Vec::new();
    let mut assertion_results: Vec<ApiAssertionResult> = Vec::new();
    let mut last_status: u16 = 0;
    let mut last_duration_ms: u64 = 0;
    let mut last_redacted_headers: Vec<(String, String)> = Vec::new();

    for action in &plan.actions {
        match action {
            Action::Request {
                method,
                url,
                headers,
                body,
            } => {
                log.push_str(&format!("REQUEST {method} {url}\n"));
                let m = match reqwest::Method::from_bytes(method.as_bytes()) {
                    Ok(m) => m,
                    Err(_) => {
                        return PlanExecutionResult {
                            status: TaskStatus::InfraError,
                            reason: Some(format!(
                                "invalid HTTP method in compiled plan: {method:?}"
                            )),
                            http_status: None,
                            duration_ms: 0,
                            response_headers: Vec::new(),
                            assertions: Vec::new(),
                            log,
                        };
                    }
                };
                let mut req = client.request(m, url);
                for (k, v) in headers {
                    req = req.header(k, v);
                }
                if let Some(b) = body {
                    req = req.json(b);
                }
                let started = std::time::Instant::now();
                match req.send().await {
                    Ok(resp) => {
                        last_duration_ms = started.elapsed().as_millis() as u64;
                        let status = resp.status();
                        let resp_headers = resp.headers().clone();
                        let text = resp.text().await.unwrap_or_default();
                        log.push_str(&format!("RESPONSE {status}\n{}\n", tail_lines(&text, 30)));
                        last_status = status.as_u16();
                        last_redacted_headers = resp_headers
                            .iter()
                            .map(|(k, v)| {
                                (
                                    k.to_string(),
                                    if is_sensitive_header(k.as_str()) {
                                        "***REDACTED***".to_string()
                                    } else {
                                        v.to_str().unwrap_or("").to_string()
                                    },
                                )
                            })
                            .collect();
                        last_response = Some((status, resp_headers, text));
                    }
                    Err(e) => {
                        let classified = classify_reqwest_error(&e);
                        log.push_str(&format!("REQUEST FAILED: {classified}\n"));
                        return PlanExecutionResult {
                            status: TaskStatus::InfraError,
                            reason: Some(classified),
                            http_status: None,
                            duration_ms: 0,
                            response_headers: Vec::new(),
                            assertions: Vec::new(),
                            log,
                        };
                    }
                }
            }
            Action::AssertStatus { status } => {
                let Some((actual, _, _)) = &last_response else {
                    failures.push("ASSERT_STATUS with no prior REQUEST".to_string());
                    continue;
                };
                let passed = actual.as_u16() == *status;
                if !passed {
                    failures.push(format!(
                        "expected HTTP status {status}, got real status {actual}"
                    ));
                }
                log.push_str(&format!("ASSERT_STATUS {status} -> actual {actual}\n"));
                assertion_results.push(ApiAssertionResult {
                    description: format!("Status equals {status}"),
                    actual: actual.to_string(),
                    passed,
                });
            }
            Action::AssertHeader { name, value } => {
                let Some((_, headers, _)) = &last_response else {
                    failures.push("ASSERT_HEADER with no prior REQUEST".to_string());
                    continue;
                };
                let actual = headers
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                let passed = actual == value;
                if !passed {
                    failures.push(format!(
                        "expected header {name:?} = {value:?}, got real value {actual:?}"
                    ));
                }
                log.push_str(&format!(
                    "ASSERT_HEADER {name}={value} -> actual {actual:?}\n"
                ));
                let display_actual = if is_sensitive_header(name) {
                    "***REDACTED***".to_string()
                } else {
                    actual.to_string()
                };
                assertion_results.push(ApiAssertionResult {
                    description: format!("Header {name} equals {value}"),
                    actual: display_actual,
                    passed,
                });
            }
            Action::AssertBodyField { path, equals } => {
                let Some((_, _, text)) = &last_response else {
                    failures.push("ASSERT_BODY_FIELD with no prior REQUEST".to_string());
                    continue;
                };
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(text);
                match parsed {
                    Ok(json) => {
                        let actual = path.split('.').try_fold(&json, |acc, key| acc.get(key));
                        let passed = actual == Some(equals);
                        if !passed {
                            failures.push(format!(
                                "expected body field {path:?} == {equals}, got real value {:?}",
                                actual
                            ));
                        }
                        log.push_str(&format!(
                            "ASSERT_BODY_FIELD {path} == {equals} -> actual {:?}\n",
                            actual
                        ));
                        assertion_results.push(ApiAssertionResult {
                            description: format!("Body field {path} equals {equals}"),
                            actual: actual
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "null".to_string()),
                            passed,
                        });
                    }
                    Err(e) => {
                        failures.push(format!(
                            "response body is not valid JSON, cannot check field {path:?}: {e}"
                        ));
                        assertion_results.push(ApiAssertionResult {
                            description: format!("Body field {path} equals {equals}"),
                            actual: format!("(not valid JSON: {e})"),
                            passed: false,
                        });
                    }
                }
            }
            Action::AssertStatusIn { statuses } => {
                let Some((actual, _, _)) = &last_response else {
                    failures.push("ASSERT_STATUS_IN with no prior REQUEST".to_string());
                    continue;
                };
                let list = statuses
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let passed = statuses.contains(&actual.as_u16());
                if !passed {
                    failures.push(format!(
                        "expected an HTTP status among {list}, got real status {actual}"
                    ));
                }
                log.push_str(&format!("ASSERT_STATUS_IN [{list}] -> actual {actual}\n"));
                assertion_results.push(ApiAssertionResult {
                    description: format!("Status is one of {list}"),
                    actual: actual.to_string(),
                    passed,
                });
            }
            Action::AssertBodySchema { schema } => {
                let Some((_, _, text)) = &last_response else {
                    failures.push("ASSERT_BODY_SCHEMA with no prior REQUEST".to_string());
                    continue;
                };
                match serde_json::from_str::<serde_json::Value>(text) {
                    Ok(json) => {
                        let problems = crate::json_schema::violations(schema, &json);
                        let passed = problems.is_empty();
                        let summary = if passed {
                            "fits".to_string()
                        } else {
                            problems.join("; ")
                        };
                        if !passed {
                            failures.push(format!(
                                "the response body does not fit its schema: {summary}"
                            ));
                        }
                        log.push_str(&format!("ASSERT_BODY_SCHEMA -> {summary}\n"));
                        assertion_results.push(ApiAssertionResult {
                            description: "Body fits its schema".to_string(),
                            actual: summary,
                            passed,
                        });
                    }
                    Err(e) => {
                        failures.push(format!(
                            "response body is not valid JSON, cannot check it against its schema: {e}"
                        ));
                        log.push_str(&format!("ASSERT_BODY_SCHEMA -> not valid JSON: {e}\n"));
                        assertion_results.push(ApiAssertionResult {
                            description: "Body fits its schema".to_string(),
                            actual: format!("(not valid JSON: {e})"),
                            passed: false,
                        });
                    }
                }
            }
            // Browser-only actions never reach this executor —
            // `execution_plan::materialize_task_def` only ever routes
            // an `ExecutionType::Api` plan here.
            Action::Navigate { .. }
            | Action::Click { .. }
            | Action::Fill { .. }
            | Action::AssertVisible { .. }
            | Action::AssertText { .. }
            | Action::AssertUrl { .. } => {
                failures.push(format!("browser action {action:?} cannot run in the API executor — this plan was misrouted"));
            }
        }
    }

    let (status, reason) = if failures.is_empty() {
        (TaskStatus::Passed, None)
    } else {
        log.push_str(&format!("\nFAILED ASSERTIONS:\n{}\n", failures.join("\n")));
        (TaskStatus::Failed, Some(tail_lines(&log, 15)))
    };

    PlanExecutionResult {
        status,
        reason,
        http_status: if last_status == 0 {
            None
        } else {
            Some(last_status)
        },
        duration_ms: last_duration_ms,
        response_headers: last_redacted_headers,
        assertions: assertion_results,
        log,
    }
}

#[async_trait]
impl Executor for CompiledApiExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let plan: crate::execution_plan::ExecutionPlan =
            match serde_json::from_value(task.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return ExecutionOutcome {
                        status: TaskStatus::InfraError,
                        reason: Some(format!(
                            "compiled-api executor received a malformed execution plan: {e}"
                        )),
                        evidence: None,
                    }
                }
            };

        // Re-validated here, not just trusted from compile time — this
        // executor is reachable via the exact same generic `executor`/
        // `params` fields any direct API call can set (`TaskDef`, unchanged
        // contract), so a plan that reached this point via a path other
        // than `execution_plan::compile` (e.g. a replayed/tampered request)
        // still gets the real SSRF/structural check, not an assumed-safe
        // shortcut.
        if let Err(reasons) = crate::execution_plan::validate_plan(&plan).await {
            return ExecutionOutcome {
                status: TaskStatus::InfraError,
                reason: Some(format!(
                    "compiled plan failed re-validation at execution time: {}",
                    reasons.join("; ")
                )),
                evidence: None,
            };
        }

        run_validated_plan(&self.client, plan).await
    }
}

/// Runs a plan that has already passed `validate_plan`: reads the secrets its request headers refer to, makes the request, checks
/// what it expects of the answer and writes the evidence.
///
/// A header value written `env:NAME` is read from the environment now, when the request is made: the plan, the run's parameters and
/// the stored evidence carry only the name, and the value is scrubbed from what the run prints.
async fn run_validated_plan(
    client: &reqwest::Client,
    plan: crate::execution_plan::ExecutionPlan,
) -> ExecutionOutcome {
    let secret_values = match read_secrets(&crate::execution_plan::secret_names_of(&plan.actions))
    {
        Ok(values) => values,
        Err(outcome) => return outcome,
    };
    let plan = with_resolved_header_secrets(plan);

    let result = execute_plan_once(client, &plan).await;
    let mut log = redact_secrets(&result.log, &secret_values);
    let result_reason = result.reason.map(|r| redact_secrets(&r, &secret_values));

    // Appended last, deliberately after `reason` is derived above, so
    // the human-readable reason/evidence tail a task list already shows
    // is never dominated by this machine-readable summary line.
    let summary = ApiResultSummary {
        status: result.http_status.unwrap_or(0),
        request_duration_ms: result.duration_ms,
        response_headers: result.response_headers,
        assertions: result.assertions,
    };
    if let Ok(summary_json) = serde_json::to_string(&summary) {
        log.push_str(&format!("RESULT_SUMMARY_JSON:{summary_json}\n"));
    }

    ExecutionOutcome {
        status: result.status,
        reason: result_reason,
        evidence: Some(log),
    }
}

/// `plan` with each request header value written `env:NAME` replaced by what the variable holds now. Only names that pass
/// `execution_plan::secret_name` are read, so a plan cannot ask for any variable of the orchestrator's environment.
fn with_resolved_header_secrets(mut plan: crate::execution_plan::ExecutionPlan) -> crate::execution_plan::ExecutionPlan {
    for action in &mut plan.actions {
        if let crate::execution_plan::Action::Request { headers, .. } = action {
            for value in headers.values_mut() {
                if let Some(name) = crate::execution_plan::secret_name(value) {
                    *value = crate::ai_router::env_value(name).unwrap_or_default();
                }
            }
        }
    }
    plan
}

/// One step of a compiled browser test as a line of the run's own output (`AIQA_STEP {...}`), written just before the
/// step runs — so a failed run's evidence ends at the step that failed, and a passing one lists what was done.
/// Never a value: a FILL names its selector and, if it reads one, the variable (a secret's name is not a secret).
fn step_log_line(index: usize, action: &crate::execution_plan::Action) -> String {
    use crate::execution_plan::Action;
    let step = match action {
        Action::Navigate { url } => {
            serde_json::json!({ "index": index, "type": "NAVIGATE", "url": url })
        }
        Action::Click { selector } => {
            serde_json::json!({ "index": index, "type": "CLICK", "selector": selector })
        }
        Action::Fill { selector, value } => {
            let mut step =
                serde_json::json!({ "index": index, "type": "FILL", "selector": selector });
            if let Some(name) = crate::execution_plan::secret_name(value) {
                step["secret"] = serde_json::json!(name);
            }
            step
        }
        Action::AssertVisible { selector } => {
            serde_json::json!({ "index": index, "type": "ASSERT_VISIBLE", "selector": selector })
        }
        Action::AssertText { selector, .. } => {
            serde_json::json!({ "index": index, "type": "ASSERT_TEXT", "selector": selector })
        }
        Action::AssertUrl { url_contains } => {
            serde_json::json!({ "index": index, "type": "ASSERT_URL", "url_contains": url_contains })
        }
        // API actions never reach a browser plan (`materialize_playwright_spec` refuses them, and a request's
        // headers or body could hold a secret: nothing of it is formatted here).
        _ => serde_json::json!({ "index": index, "type": "UNSUPPORTED" }),
    };
    let line = format!("AIQA_STEP {step}");
    format!(
        "  console.log({});\n",
        serde_json::to_string(&line).unwrap_or_default()
    )
}

/// Turns a validated `Browser`-type `ExecutionPlan` into a real, safe
/// Playwright `.spec.ts` file under `executors/playwright/tests/generated/`,
/// then returns the path (relative to `executors/playwright`, exactly what
/// `PlaywrightExecutor`'s existing `spec` param already expects — no change
/// to that executor was needed). Every action's *string values* (URLs,
/// selectors, text) are embedded via `serde_json::to_string`, which
/// produces a valid, safely-escaped JS/TS string literal — never raw
/// string interpolation of untrusted content into source code. The action
/// vocabulary itself is closed (matched exhaustively below); there is no
/// path from a plan to arbitrary generated code.
///
/// Self-healing (Phase 3): CLICK/FILL/ASSERT_VISIBLE/ASSERT_TEXT — the four
/// action kinds that name a `selector` an AI compiler had to *invent* from
/// prose (this codebase has no crawler/real-DOM access feeding the
/// compiler yet, so that selector is an educated guess, not an observed
/// one) — are routed through `executors/playwright/helpers/healing.ts`'s
/// `healingLocator` instead of a raw `page.locator(selector)`. The exact
/// same selector is still tried first; only on a real, complete miss does
/// it reach for a resilient alternative. NAVIGATE/ASSERT_URL never involve
/// a selector at all, so they are untouched.
pub fn materialize_playwright_spec(
    task_id: &str,
    plan: &crate::execution_plan::ExecutionPlan,
) -> Result<String, String> {
    use crate::execution_plan::Action;

    fn js_str(s: &str) -> String {
        serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
    }

    let mut body = String::new();
    let mut uses_secret = false;
    for (index, action) in plan.actions.iter().enumerate() {
        body.push_str(&step_log_line(index, action));
        match action {
            Action::Navigate { url } => {
                body.push_str(&format!("  await page.goto({});\n", js_str(url)))
            }
            Action::Click { selector } => body.push_str(&format!(
                "  await (await healingLocator(page, {}, {index}, \"CLICK\")).click();\n",
                js_str(selector)
            )),
            Action::Fill { selector, value } => {
                // A value written `env:NAME` is read from the environment when the test runs: the spec holds the
                // name, never the text.
                let text = match crate::execution_plan::secret_name(value) {
                    Some(name) => {
                        uses_secret = true;
                        format!("secret({})", js_str(name))
                    }
                    None => js_str(value),
                };
                body.push_str(&format!(
                    "  await (await healingLocator(page, {}, {index}, \"FILL\")).fill({text});\n",
                    js_str(selector),
                ));
            }
            Action::AssertVisible { selector } => body.push_str(&format!(
                "  await expect(await healingLocator(page, {}, {index}, \"ASSERT_VISIBLE\")).toBeVisible();\n",
                js_str(selector)
            )),
            Action::AssertText { selector, text } => body.push_str(&format!(
                "  await expect(await healingLocator(page, {}, {index}, \"ASSERT_TEXT\")).toHaveText({});\n",
                js_str(selector),
                js_str(text)
            )),
            Action::AssertUrl { url_contains } => body.push_str(&format!(
                "  await expect(page).toHaveURL(new RegExp({}));\n",
                js_str(&regex::escape(url_contains))
            )),
            Action::Request { .. }
            | Action::AssertStatus { .. }
            | Action::AssertHeader { .. }
            | Action::AssertBodyField { .. }
            | Action::AssertStatusIn { .. }
            | Action::AssertBodySchema { .. } => {
                return Err(format!("API action {action:?} cannot appear in a BROWSER plan — this plan was misrouted"));
            }
        }
    }

    // Where the run ended, as one line of its own output: the evidence of a passing run says which page it reached.
    body.push_str(
        "  console.log(\"AIQA_FINAL \" + JSON.stringify({ url: page.url(), title: await page.title() }));\n",
    );

    let safe_id: String = task_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if safe_id.is_empty() {
        return Err(
            "task id produced no safe characters for a generated spec filename".to_string(),
        );
    }
    let file_name = format!("compiled-{safe_id}.spec.ts");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../executors/playwright/tests/generated");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create generated-spec directory: {e}"))?;
    let full_path = dir.join(&file_name);
    // Only pulled in when this plan actually uses a selector-based action —
    // a NAVIGATE/ASSERT_URL-only plan (the deterministic compiler's typical
    // output) has no real use for it, and an always-present unused import
    // is exactly the kind of dead surface this codebase's own FE-13 audit
    // unit already flagged elsewhere.
    let healing_import = if body.contains("healingLocator(") {
        "import { healingLocator } from \"../../helpers/healing\";\n"
    } else {
        ""
    };
    let secret_import = if uses_secret {
        "import { secret } from \"../../helpers/secrets\";\n"
    } else {
        ""
    };
    let source = format!(
        "// Generated by execution_plan::materialize_playwright_spec from a real, compiled\n\
         // test case's ExecutionPlan — every value above was embedded via a safe JSON string\n\
         // escape, never raw-interpolated. Values written env:NAME are read from the environment\n\
         // when the test runs (helpers/secrets.ts); this file holds only the name.\n\
         // Not hand-edited; safe to delete/regenerate.\n\
         import {{ test, expect }} from \"@playwright/test\";\n\
         {healing_import}{secret_import}\n\
         test(\"compiled plan for test case {safe_id}\", async ({{ page }}) => {{\n{body}}});\n"
    );
    std::fs::write(&full_path, source)
        .map_err(|e| format!("failed to write generated spec file: {e}"))?;

    Ok(format!("tests/generated/{file_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------ unresolved_healing_selector

    /// The shape a real `--reporter=line` Playwright run printed when a compiled
    /// test's selector matched nothing - captured from an actual run of
    /// `helpers/healing.ts`, not hand-written - with the lines it really sits
    /// between.
    const REAL_SELECTOR_MISS_OUTPUT: &str = r##"Running 1 test using 1 worker
AIQA_HEALING_EVENT {"event":"heal","actionIndex":1,"actionKind":"CLICK","primarySelector":"#login-btn-that-does-not-exist","strategy":"none","resolved":false}

  1) tests\generated\tmp.spec.ts:4:5 › verify healing event format

    Error: AIQA self-healing exhausted every real fallback strategy for selector "#login-btn-that-does-not-exist" (action #1, CLICK) — no matching element found.
"##;

    #[test]
    fn a_real_selector_miss_yields_the_failed_selector() {
        assert_eq!(
            unresolved_healing_selector(REAL_SELECTOR_MISS_OUTPUT).as_deref(),
            Some("#login-btn-that-does-not-exist")
        );
    }

    #[test]
    fn a_heal_that_worked_is_not_a_failure() {
        let out = r##"AIQA_HEALING_EVENT {"event":"heal","actionIndex":0,"actionKind":"CLICK","primarySelector":"#old","strategy":"text-from-id","resolved":true}"##;

        assert_eq!(unresolved_healing_selector(out), None);
    }

    #[test]
    fn output_without_the_marker_has_no_selector() {
        assert_eq!(
            unresolved_healing_selector("Running 1 test\n  1 passed (1.2s)\n"),
            None
        );
        assert_eq!(unresolved_healing_selector(""), None);
    }

    #[test]
    fn malformed_or_incomplete_events_are_ignored_not_guessed() {
        let out = "AIQA_HEALING_EVENT {not json\n\
                   AIQA_HEALING_EVENT {\"resolved\":false}\n\
                   AIQA_HEALING_EVENT {\"primarySelector\":\"#x\"}\n\
                   AIQA_HEALING_EVENT {\"resolved\":false,\"primarySelector\":\"   \"}\n";

        assert_eq!(unresolved_healing_selector(out), None);
    }

    #[test]
    fn the_last_unresolved_event_wins_and_a_line_prefix_is_tolerated() {
        let out = "AIQA_HEALING_EVENT {\"resolved\":false,\"primarySelector\":\"#first\"}\n\
                   [chromium] stdout: AIQA_HEALING_EVENT {\"resolved\":false,\"primarySelector\":\"#second\"} trailing text\n";

        assert_eq!(unresolved_healing_selector(out).as_deref(), Some("#second"));
    }

    #[test]
    fn sanitize_token_accepts_realistic_params() {
        assert!(sanitize_token("tests/sample.spec.ts").is_ok());
        assert!(sanitize_token("homepage has correct title and heading").is_ok());
        assert!(sanitize_token("health check passes").is_ok());
        assert!(sanitize_token("test_health.py::test_health_returns_200_and_ok_body").is_ok());
        assert!(sanitize_token("http://localhost:8080").is_ok());
    }

    // ------------------------------- Phase 3: self-healing spec materialization

    use crate::execution_plan::{Action, CompilerMethod, ExecutionPlan, ExecutionType};

    fn browser_plan(test_case_id: &str, actions: Vec<Action>) -> ExecutionPlan {
        ExecutionPlan {
            test_case_id: test_case_id.to_string(),
            execution_type: ExecutionType::Browser,
            target: None,
            actions,
            compiler: CompilerMethod::Deterministic,
            diagnostics: vec![],
            provider: None,
            model: None,
        }
    }

    #[test]
    fn a_navigate_only_plan_never_imports_or_calls_the_healing_helper() {
        let plan = browser_plan(
            "navonly",
            vec![
                Action::Navigate {
                    url: "https://example.com/login".to_string(),
                },
                Action::AssertUrl {
                    url_contains: "example.com".to_string(),
                },
            ],
        );
        let rel_path = materialize_playwright_spec("test-navonly", &plan).expect("materialize");
        let source = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../executors/playwright")
                .join(&rel_path),
        )
        .expect("read generated spec");
        assert!(
            !source.contains("healingLocator"),
            "a NAVIGATE/ASSERT_URL-only plan has no selector at all — must not pull in the healing helper: {source}"
        );
    }

    #[test]
    fn selector_based_actions_are_routed_through_healing_locator_with_the_right_kind_and_index() {
        let plan = browser_plan(
            "selectors",
            vec![
                Action::Navigate {
                    url: "https://example.com/login".to_string(),
                },
                Action::Fill {
                    selector: "#email".to_string(),
                    value: "user@example.com".to_string(),
                },
                Action::Click {
                    selector: "#submit-button".to_string(),
                },
                Action::AssertVisible {
                    selector: ".dashboard".to_string(),
                },
                Action::AssertText {
                    selector: "#welcome".to_string(),
                    text: "Welcome".to_string(),
                },
            ],
        );
        let rel_path = materialize_playwright_spec("test-selectors", &plan).expect("materialize");
        let source = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../executors/playwright")
                .join(&rel_path),
        )
        .expect("read generated spec");

        assert!(
            source.contains("import { healingLocator } from \"../../helpers/healing\";"),
            "source={source}"
        );
        // Action indices match each action's real position in plan.actions
        // (Navigate is index 0 and carries no selector, so the first real
        // healing call is index 1, matching Fill's real position).
        assert!(
            source.contains("healingLocator(page, \"#email\", 1, \"FILL\")"),
            "source={source}"
        );
        assert!(
            source.contains("healingLocator(page, \"#submit-button\", 2, \"CLICK\")"),
            "source={source}"
        );
        assert!(
            source.contains("healingLocator(page, \".dashboard\", 3, \"ASSERT_VISIBLE\")"),
            "source={source}"
        );
        assert!(
            source.contains("healingLocator(page, \"#welcome\", 4, \"ASSERT_TEXT\")"),
            "source={source}"
        );
        // The primary selector is still tried first inside the helper —
        // this call site does not change what selector is attempted, only
        // how a miss is handled.
        assert!(source.contains(".click();"));
        assert!(source.contains(".fill(\"user@example.com\");"));
    }

    #[test]
    fn an_api_action_inside_a_browser_plan_is_still_rejected() {
        let plan = browser_plan("misrouted", vec![Action::AssertStatus { status: 200 }]);
        let err = materialize_playwright_spec("test-misrouted", &plan)
            .expect_err("an API action in a BROWSER plan must be rejected");
        assert!(err.contains("misrouted"), "err={err}");
    }

    #[test]
    fn selector_and_text_values_are_safely_json_escaped_never_raw_interpolated() {
        // A selector/value containing a quote and a backslash must not be
        // able to break out of the generated string literal.
        let plan = browser_plan(
            "escaping",
            vec![Action::Fill {
                selector: "input[data-x=\"a\\b\"]".to_string(),
                value: "quote\" and backslash\\ here".to_string(),
            }],
        );
        let rel_path = materialize_playwright_spec("test-escaping", &plan).expect("materialize");
        let source = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../executors/playwright")
                .join(&rel_path),
        )
        .expect("read generated spec");
        // A real JSON-escaped round trip: the generated source, re-parsed
        // as JS string literals, must reproduce the exact original values —
        // proof this is a safe escape, not raw string concatenation.
        assert!(source.contains("input[data-x=\\\"a\\\\b\\\"]"));
        assert!(source.contains("quote\\\" and backslash\\\\ here"));
    }

    // ------------------------------------------------------ run-time secrets

    fn spec_source(rel_path: &str) -> String {
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../executors/playwright")
                .join(rel_path),
        )
        .expect("read generated spec")
    }

    fn playwright_task(params: serde_json::Value) -> TaskDef {
        TaskDef {
            id: "t".to_string(),
            name: "n".to_string(),
            prerequisites: Vec::new(),
            simulate: None,
            executor: Some("playwright".to_string()),
            params,
        }
    }

    #[test]
    fn a_secret_fill_is_read_at_run_time_and_the_spec_never_holds_its_value() {
        std::env::set_var("AIQA_LOGIN_T5A_SPEC_PW", "pa55-w0rd-value");
        let plan = browser_plan(
            "secrets",
            vec![
                Action::Navigate {
                    url: "https://example.com/login".to_string(),
                },
                Action::Fill {
                    selector: "#username".to_string(),
                    value: "admin".to_string(),
                },
                Action::Fill {
                    selector: "#password".to_string(),
                    value: "env:AIQA_LOGIN_T5A_SPEC_PW".to_string(),
                },
            ],
        );
        let rel_path = materialize_playwright_spec("test-5a-secrets", &plan).expect("materialize");
        let source = spec_source(&rel_path);
        assert!(
            source.contains("import { secret } from \"../../helpers/secrets\";"),
            "{source}"
        );
        assert!(
            source.contains(".fill(secret(\"AIQA_LOGIN_T5A_SPEC_PW\"));"),
            "{source}"
        );
        assert!(!source.contains("pa55-w0rd-value"), "{source}");
        assert!(
            !source.contains("env:AIQA_LOGIN_T5A_SPEC_PW"),
            "the reference is not left in the spec as text either: {source}"
        );
        // An ordinary value is still typed as written.
        assert!(source.contains(".fill(\"admin\");"), "{source}");
    }

    /// The `AIQA_STEP` lines of a generated spec, parsed: each is `console.log("AIQA_STEP {json}")`.
    fn logged_steps(source: &str) -> Vec<serde_json::Value> {
        source
            .lines()
            .filter(|line| line.contains("AIQA_STEP"))
            .map(|line| {
                let literal: String =
                    serde_json::from_str(&line[line.find('"').unwrap()..=line.rfind('"').unwrap()])
                        .expect("a JS string literal");
                serde_json::from_str(literal.strip_prefix("AIQA_STEP ").expect("the marker"))
                    .expect("the step is JSON")
            })
            .collect()
    }

    #[test]
    fn a_compiled_spec_logs_each_step_before_it_runs_and_where_it_ended_and_never_a_value() {
        std::env::set_var("AIQA_LOGIN_T5B_STEP_PW", "step-pw-value-9");
        let plan = browser_plan(
            "steps",
            vec![
                Action::Navigate {
                    url: "https://example.com/login".to_string(),
                },
                Action::Fill {
                    selector: "#username".to_string(),
                    value: "typed-literal-value".to_string(),
                },
                Action::Fill {
                    selector: "#password".to_string(),
                    value: "env:AIQA_LOGIN_T5B_STEP_PW".to_string(),
                },
                Action::Click {
                    selector: "#go".to_string(),
                },
                Action::AssertText {
                    selector: "h1".to_string(),
                    text: "expected heading text".to_string(),
                },
                Action::AssertUrl {
                    url_contains: "/welcome".to_string(),
                },
            ],
        );
        let rel_path = materialize_playwright_spec("test-5b-steps", &plan).expect("materialize");
        let source = spec_source(&rel_path);
        let steps = logged_steps(&source);

        let kinds: Vec<&str> = steps.iter().map(|s| s["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            [
                "NAVIGATE",
                "FILL",
                "FILL",
                "CLICK",
                "ASSERT_TEXT",
                "ASSERT_URL"
            ]
        );
        let indexes: Vec<u64> = steps.iter().map(|s| s["index"].as_u64().unwrap()).collect();
        assert_eq!(indexes, [0, 1, 2, 3, 4, 5]);
        assert_eq!(steps[0]["url"], "https://example.com/login");
        assert_eq!(steps[3]["selector"], "#go");
        // A secret's step names the variable; a literal's says nothing of the value; neither logs a value.
        assert_eq!(steps[2]["secret"], "AIQA_LOGIN_T5B_STEP_PW");
        assert!(steps[1].get("secret").is_none() && steps[1].get("value").is_none());
        let log_lines: String = source
            .lines()
            .filter(|l| l.contains("AIQA_STEP"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!log_lines.contains("typed-literal-value"), "{log_lines}");
        assert!(!log_lines.contains("step-pw-value-9"), "{log_lines}");
        assert!(!log_lines.contains("expected heading text"), "{log_lines}");

        // Each step is logged before it runs (so a failed run's evidence ends at the step that failed).
        let position = |needle: &str| {
            source
                .find(needle)
                .unwrap_or_else(|| panic!("{needle} not in {source}"))
        };
        for (index, action_code) in [
            (3, "healingLocator(page, \"#go\", 3, \"CLICK\")"),
            (2, "healingLocator(page, \"#password\", 2, \"FILL\")"),
        ] {
            let logged = source
                .lines()
                .position(|l| {
                    l.contains("AIQA_STEP") && l.contains(&format!("\\\"index\\\":{index},"))
                })
                .expect("the step is logged");
            let action = source
                .lines()
                .position(|l| l.contains(action_code))
                .unwrap();
            assert_eq!(
                logged + 1,
                action,
                "step {index} is logged on the line before it runs"
            );
        }
        // And where the run ended is logged last.
        let final_line = position("AIQA_FINAL");
        assert!(final_line > position("toHaveURL"), "{source}");
        assert!(source[final_line..].contains("page.url()"));
    }

    #[test]
    fn a_plan_with_no_secret_never_imports_the_secret_helper() {
        let plan = browser_plan(
            "nosecret",
            vec![Action::Fill {
                selector: "#q".to_string(),
                value: "text".to_string(),
            }],
        );
        let rel_path = materialize_playwright_spec("test-5a-nosecret", &plan).expect("materialize");
        let source = spec_source(&rel_path);
        assert!(!source.contains("import { secret }"), "{source}");
        assert!(!source.contains("secret("), "{source}");
    }

    #[test]
    fn redaction_scrubs_every_spelling_of_a_secret_and_leaves_short_values_alone() {
        let values = vec![
            "pa55-w0rd".to_string(),
            "q\"uote\\d".to_string(),
            "abc".to_string(),
        ];
        let text = "typed pa55-w0rd; as JSON \"q\\\"uote\\\\d\" and raw q\"uote\\d; abc stays";
        let out = redact_secrets(text, &values);
        assert!(!out.contains("pa55-w0rd"), "{out}");
        assert!(!out.contains("q\\\"uote\\\\d"), "{out}");
        assert!(!out.contains("q\"uote\\d"), "{out}");
        assert!(
            out.contains("abc stays"),
            "a value under {MIN_REDACTED_SECRET_LEN} characters is not scrubbed: {out}"
        );
        assert_eq!(redact_secrets("nothing here", &values), "nothing here");
        assert_eq!(redact_secrets("anything", &[]), "anything");
    }

    #[test]
    fn a_secret_that_contains_another_is_scrubbed_whole() {
        let out = redact_secrets(
            "value: super-secret-pass",
            &["secret".to_string(), "super-secret-pass".to_string()],
        );
        assert_eq!(out, "value: ***");
    }

    #[test]
    fn redact_outcome_scrubs_both_the_reason_and_the_evidence() {
        let outcome = ExecutionOutcome {
            status: TaskStatus::Failed,
            reason: Some("failed typing s3cr3t-v4lue".to_string()),
            evidence: Some("log: s3cr3t-v4lue\nmore".to_string()),
        };
        let out = redact_outcome(outcome, &["s3cr3t-v4lue".to_string()]);
        assert_eq!(out.reason.as_deref(), Some("failed typing ***"));
        assert_eq!(out.evidence.as_deref(), Some("log: ***\nmore"));
        assert!(matches!(out.status, TaskStatus::Failed));
    }

    /// A real `Output` (the exit status comes from a real, trivial process) carrying the given stdout.
    fn finished_process_output(stdout: &str, exit_code: i32) -> std::process::Output {
        #[cfg(windows)]
        let mut output = std::process::Command::new("cmd")
            .args(["/C", &format!("exit {exit_code}")])
            .output()
            .expect("run a trivial process");
        #[cfg(not(windows))]
        let mut output = std::process::Command::new("sh")
            .args(["-c", &format!("exit {exit_code}")])
            .output()
            .expect("run a trivial process");
        output.stdout = stdout.as_bytes().to_vec();
        output.stderr = Vec::new();
        output
    }

    #[test]
    fn a_finished_run_has_its_secrets_scrubbed_from_the_reason_and_the_stored_evidence() {
        let output = finished_process_output(
            "Error: locator.fill: expected s3cr3t-value\n  at login.spec.ts:7\n",
            1,
        );
        let outcome = finish_playwright(output, &["s3cr3t-value".to_string()]);
        assert!(matches!(outcome.status, TaskStatus::Failed));
        let reason = outcome.reason.expect("a failed run has a reason");
        let evidence = outcome.evidence.expect("the output is the evidence");
        assert!(!reason.contains("s3cr3t-value"), "{reason}");
        assert!(!evidence.contains("s3cr3t-value"), "{evidence}");
        assert!(evidence.contains("expected ***"), "{evidence}");
        // A passing run is scrubbed too.
        let passed = finish_playwright(
            finished_process_output("typed s3cr3t-value\n", 0),
            &["s3cr3t-value".to_string()],
        );
        assert!(matches!(passed.status, TaskStatus::Passed));
        assert_eq!(passed.evidence.as_deref(), Some("typed ***\n"));
    }

    #[test]
    fn secret_names_come_from_the_task_params_and_must_be_reserved_names() {
        assert!(secret_names_param(&playwright_task(serde_json::json!({})))
            .unwrap()
            .is_empty());
        let ok = secret_names_param(&playwright_task(
            serde_json::json!({"secrets": ["AIQA_LOGIN_A_USER", "AIQA_SECRET_B"]}),
        ))
        .unwrap();
        assert_eq!(
            ok,
            vec!["AIQA_LOGIN_A_USER".to_string(), "AIQA_SECRET_B".to_string()]
        );
        for bad in [
            serde_json::json!("AIQA_LOGIN_A"),
            serde_json::json!(["GROQ_API_KEY"]),
            serde_json::json!([1]),
            serde_json::json!(["AIQA_LOGIN_A; calc"]),
        ] {
            let err = secret_names_param(&playwright_task(serde_json::json!({ "secrets": bad })))
                .expect_err("rejected");
            assert!(matches!(err.status, TaskStatus::InfraError));
        }
    }

    #[test]
    fn read_secrets_returns_the_trimmed_values_of_set_variables() {
        std::env::set_var("AIQA_LOGIN_T5A_READ", "  read-me-123  ");
        assert_eq!(
            read_secrets(&["AIQA_LOGIN_T5A_READ".to_string()]).unwrap(),
            vec!["read-me-123".to_string()]
        );
    }

    #[tokio::test]
    async fn a_run_whose_secret_is_not_set_is_refused_before_anything_is_launched() {
        std::env::remove_var("AIQA_LOGIN_T5A_NEVER_SET");
        // A directory that does not exist: if the executor got as far as launching, the outcome would say it
        // failed to launch — not that a secret is missing.
        let executor = PlaywrightExecutor {
            project_dir: PathBuf::from("this-directory-does-not-exist"),
        };
        let outcome = executor
            .execute(&playwright_task(serde_json::json!({
                "spec": "tests/generated/x.spec.ts",
                "secrets": ["AIQA_LOGIN_T5A_NEVER_SET"],
            })))
            .await;
        assert!(matches!(outcome.status, TaskStatus::InfraError));
        let reason = outcome.reason.expect("a reason");
        assert!(
            reason.contains("AIQA_LOGIN_T5A_NEVER_SET is not set"),
            "{reason}"
        );
        assert!(outcome.evidence.is_none(), "nothing was launched");
    }

    // ------------------------------------------- Phase 4: API Testing page

    #[test]
    fn sensitive_response_header_names_are_recognized_case_insensitively() {
        for name in [
            "Authorization",
            "authorization",
            "AUTHORIZATION",
            "Set-Cookie",
            "cookie",
            "X-Api-Key",
            "Proxy-Authorization",
        ] {
            assert!(
                is_sensitive_header(name),
                "{name} must be treated as sensitive"
            );
        }
    }

    // ---------------------------------------------------------------------- Phase 8B: what an API answer must look like

    /// A local server that answers every request with `status`, `content_type` and `body`, and keeps the text of each request it got.
    async fn canned_server(
        status: u16,
        content_type: &'static str,
        body: String,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let keep = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let keep = keep.clone();
                let body = body.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buffer = vec![0u8; 8192];
                    let n = socket.read(&mut buffer).await.unwrap_or(0);
                    keep.lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&buffer[..n]).to_string());
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        (format!("http://{addr}"), seen)
    }

    fn get_plan(url: &str, headers: &[(&str, &str)], expect: Vec<crate::execution_plan::Action>) -> crate::execution_plan::ExecutionPlan {
        use crate::execution_plan::{Action, CompilerMethod, ExecutionPlan, ExecutionType};
        let mut actions = vec![Action::Request {
            method: "GET".to_string(),
            url: url.to_string(),
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: None,
        }];
        actions.extend(expect);
        ExecutionPlan {
            test_case_id: "t".to_string(),
            execution_type: ExecutionType::Api,
            target: Some(url.to_string()),
            actions,
            compiler: CompilerMethod::Deterministic,
            diagnostics: Vec::new(),
            provider: None,
            model: None,
        }
    }

    #[tokio::test]
    async fn assert_status_in_passes_for_any_status_on_its_list_and_fails_with_the_list_named_otherwise() {
        use crate::execution_plan::Action::AssertStatusIn;
        let client = reqwest::Client::new();
        let (url, _) = canned_server(201, "application/json", "{}".to_string()).await;
        let ok = execute_plan_once(&client, &get_plan(&url, &[], vec![AssertStatusIn { statuses: vec![200, 201] }])).await;
        assert_eq!(ok.status, TaskStatus::Passed, "{:?}", ok.reason);
        assert_eq!(ok.assertions.len(), 1);
        assert_eq!(ok.assertions[0].description, "Status is one of 200, 201");
        assert!(ok.assertions[0].passed);
        assert!(ok.log.contains("ASSERT_STATUS_IN [200, 201] -> actual 201"), "{}", ok.log);

        let (url, _) = canned_server(404, "application/json", "{}".to_string()).await;
        let failed = execute_plan_once(&client, &get_plan(&url, &[], vec![AssertStatusIn { statuses: vec![200, 201] }])).await;
        assert_eq!(failed.status, TaskStatus::Failed);
        assert!(failed.log.contains("expected an HTTP status among 200, 201, got real status 404"), "{}", failed.log);
        assert!(!failed.assertions[0].passed);
        assert_eq!(failed.http_status, Some(404));
    }

    #[tokio::test]
    async fn an_assertion_with_no_request_before_it_fails_rather_than_passing() {
        use crate::execution_plan::Action::{AssertBodySchema, AssertStatusIn};
        let mut plan = get_plan("http://127.0.0.1:1/", &[], vec![]);
        plan.actions = vec![AssertStatusIn { statuses: vec![200] }, AssertBodySchema { schema: serde_json::json!({}) }];
        let result = execute_plan_once(&reqwest::Client::new(), &plan).await;
        assert_eq!(result.status, TaskStatus::Failed);
        assert!(result.log.contains("ASSERT_STATUS_IN with no prior REQUEST"), "{}", result.log);
        assert!(result.log.contains("ASSERT_BODY_SCHEMA with no prior REQUEST"), "{}", result.log);
    }

    #[tokio::test]
    async fn assert_body_schema_passes_a_body_that_fits() {
        use crate::execution_plan::Action::AssertBodySchema;
        let schema = serde_json::json!({"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}, "name": {"type": "string"}}});
        let (url, _) = canned_server(200, "application/json", r#"{"id": 3, "name": "Rex"}"#.to_string()).await;
        let result = execute_plan_once(&reqwest::Client::new(), &get_plan(&url, &[], vec![AssertBodySchema { schema }])).await;
        assert_eq!(result.status, TaskStatus::Passed, "{:?} {}", result.reason, result.log);
        assert_eq!(result.assertions[0].description, "Body fits its schema");
        assert_eq!(result.assertions[0].actual, "fits");
        assert!(result.log.contains("ASSERT_BODY_SCHEMA -> fits"), "{}", result.log);
    }

    #[tokio::test]
    async fn assert_body_schema_fails_a_body_that_does_not_fit_and_says_what_is_wrong() {
        use crate::execution_plan::Action::AssertBodySchema;
        let schema = serde_json::json!({"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}, "name": {"type": "string"}}});
        let (url, _) = canned_server(200, "application/json", r#"{"name": 5}"#.to_string()).await;
        let result = execute_plan_once(&reqwest::Client::new(), &get_plan(&url, &[], vec![AssertBodySchema { schema }])).await;
        assert_eq!(result.status, TaskStatus::Failed);
        assert!(!result.assertions[0].passed);
        let said = &result.assertions[0].actual;
        assert!(said.contains("required property \"id\" is missing"), "{said}");
        assert!(said.contains("$.name: expected string, got integer"), "{said}");
        assert!(result.log.contains("the response body does not fit its schema"), "{}", result.log);
    }

    #[tokio::test]
    async fn assert_body_schema_fails_a_body_that_is_not_json() {
        use crate::execution_plan::Action::AssertBodySchema;
        let (url, _) = canned_server(200, "text/html", "<html>nope</html>".to_string()).await;
        let result = execute_plan_once(&reqwest::Client::new(), &get_plan(&url, &[], vec![AssertBodySchema { schema: serde_json::json!({"type": "object"}) }])).await;
        assert_eq!(result.status, TaskStatus::Failed);
        assert!(result.assertions[0].actual.starts_with("(not valid JSON"), "{}", result.assertions[0].actual);
        assert!(result.log.contains("cannot check it against its schema"), "{}", result.log);
    }

    #[tokio::test]
    async fn a_header_written_env_name_is_read_when_the_request_is_made_and_kept_out_of_the_evidence() {
        use crate::execution_plan::Action::AssertStatusIn;
        std::env::set_var("AIQA_SECRET_T8B_TOKEN", "  tok-8b-secret-value  ");
        // The server repeats the token in its answer, as a badly written one may.
        let (url, seen) = canned_server(200, "application/json", r#"{"echo": "tok-8b-secret-value"}"#.to_string()).await;
        let plan = get_plan(&url, &[("Authorization", "env:AIQA_SECRET_T8B_TOKEN")], vec![AssertStatusIn { statuses: vec![200] }]);
        let outcome = run_validated_plan(&reqwest::Client::new(), plan).await;
        assert_eq!(outcome.status, TaskStatus::Passed, "{:?}", outcome.reason);
        let requests = seen.lock().unwrap().clone();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].to_lowercase().contains("authorization: tok-8b-secret-value\r\n"), "the server got the value, trimmed: {}", requests[0]);
        assert!(!requests[0].contains("env:AIQA_SECRET_T8B_TOKEN"), "and not the reference");
        let evidence = outcome.evidence.unwrap();
        assert!(!evidence.contains("tok-8b-secret-value"), "the value is scrubbed from what the run keeps: {evidence}");
        assert!(evidence.contains("***"), "{evidence}");
        assert!(evidence.contains("RESULT_SUMMARY_JSON:"), "{evidence}");
    }

    #[tokio::test]
    async fn a_header_that_names_a_variable_that_is_not_set_stops_the_run_before_any_request() {
        std::env::remove_var("AIQA_SECRET_T8B_MISSING");
        let (url, seen) = canned_server(200, "application/json", "{}".to_string()).await;
        let plan = get_plan(&url, &[("X-Api-Key", "env:AIQA_SECRET_T8B_MISSING")], vec![]);
        let outcome = run_validated_plan(&reqwest::Client::new(), plan).await;
        assert_eq!(outcome.status, TaskStatus::InfraError);
        let reason = outcome.reason.unwrap();
        assert!(reason.contains("AIQA_SECRET_T8B_MISSING"), "{reason}");
        assert!(seen.lock().unwrap().is_empty(), "nothing was sent");
    }

    #[test]
    fn only_a_variable_a_plan_may_name_is_ever_read_for_a_header() {
        use crate::execution_plan::Action;
        std::env::set_var("GROQ_API_KEY_T8B", "must-not-leak");
        let plan = get_plan("http://x.test/", &[("Authorization", "env:GROQ_API_KEY_T8B"), ("X-Plain", "plain")], vec![]);
        let resolved = with_resolved_header_secrets(plan);
        let Action::Request { headers, .. } = &resolved.actions[0] else { panic!("no request") };
        assert_eq!(headers.get("Authorization").map(String::as_str), Some("env:GROQ_API_KEY_T8B"), "left as written, never read");
        assert_eq!(headers.get("X-Plain").map(String::as_str), Some("plain"));
    }

    #[test]
    fn ordinary_response_headers_are_not_flagged_sensitive() {
        for name in [
            "Content-Type",
            "Content-Length",
            "X-Request-Id",
            "Cache-Control",
        ] {
            assert!(!is_sensitive_header(name), "{name} must not be redacted");
        }
    }

    #[tokio::test]
    async fn a_genuine_connection_failure_is_classified_as_connection_failed_not_a_generic_error() {
        // A real, ephemeral TCP port this test itself binds and then
        // immediately releases — genuinely nothing listens on it
        // afterward, so the failure is a real transport-level connection
        // refusal, not a validation rejection or a dependency on any
        // other process being up.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let err = client
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .await
            .unwrap_err();
        let classified = classify_reqwest_error(&err);
        assert!(
            classified.starts_with("CONNECTION_FAILED:"),
            "classified={classified}"
        );
    }

    #[tokio::test]
    async fn a_genuine_timeout_is_classified_as_request_timeout() {
        // A real TCP listener this test spins up that accepts the
        // connection but deliberately never writes a response — the
        // client's own request genuinely times out waiting for one.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((socket, _)) = listener.accept().await {
                let _socket = socket; // held open, nothing ever written
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let err = client
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap_err();
        assert!(err.is_timeout(), "err={err}");
        let classified = classify_reqwest_error(&err);
        assert!(
            classified.starts_with("REQUEST_TIMEOUT:"),
            "classified={classified}"
        );
    }

    /// This is the regression test for the fix: `params` are attacker-
    /// reachable via `POST /runs`, and get concatenated into a `cmd /C
    /// "..."` line for the npx/newman/maestro adapters (see
    /// `shell_command`). Anything that could break out of that line must be
    /// rejected before it's ever built into a command string.
    #[test]
    fn sanitize_token_rejects_shell_metacharacters_and_traversal() {
        for bad in [
            "x\" & calc.exe & \"",
            "x`whoami`",
            "x$(whoami)",
            "x; rm -rf /",
            "x | curl evil.example",
            "x > out.txt",
            "x\nrogue",
            "../../etc/passwd",
            "",
        ] {
            assert!(
                sanitize_token(bad).is_err(),
                "expected rejection for {bad:?}"
            );
        }
    }

    #[test]
    fn safe_param_allows_empty_default_for_optional_grep() {
        let task = TaskDef {
            id: "t".into(),
            name: "t".into(),
            ..Default::default()
        };
        assert_eq!(safe_param(&task, "grep", "").unwrap(), "");
    }

    #[test]
    fn safe_param_rejects_unsafe_value_from_params() {
        let task = TaskDef {
            id: "t".into(),
            name: "t".into(),
            params: serde_json::json!({ "grep": "x\" & calc.exe & \"" }),
            ..Default::default()
        };
        let err = safe_param(&task, "grep", "").unwrap_err();
        assert_eq!(err.status, TaskStatus::InfraError);
    }

    // ------------------------------------------------------ M2 — SSRF guard

    /// The exact reproduction the Backend Audit's M2 finding names:
    /// `sanitize_token` alone lets `http://169.254.169.254/...` straight
    /// through (it's just letters/digits/`.`/`:`/`/`), and that address is
    /// the AWS/GCP/Azure instance-metadata endpoint — a real SSRF target,
    /// not a hypothetical one.
    #[tokio::test]
    async fn validate_target_url_rejects_cloud_metadata_address() {
        let err = validate_target_url("http://169.254.169.254/latest/meta-data/")
            .await
            .unwrap_err();
        assert!(
            err.contains("disallowed"),
            "expected a disallowed-range rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn validate_target_url_rejects_rfc1918_private_ranges() {
        for target in [
            "http://10.0.0.5:80",
            "http://172.16.0.5:80",
            "http://192.168.1.1:80",
        ] {
            match validate_target_url(target).await {
                Err(err) => assert!(
                    err.contains("disallowed"),
                    "expected a disallowed-range rejection for {target}, got: {err}"
                ),
                Ok(()) => panic!("expected {target} to be rejected as a private range"),
            }
        }
    }

    #[tokio::test]
    async fn validate_target_url_rejects_non_http_schemes() {
        let err = validate_target_url("file:///etc/passwd").await.unwrap_err();
        assert!(
            err.contains("scheme"),
            "expected a scheme rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn validate_target_url_rejects_loopback_on_a_non_orchestrator_port() {
        // Loopback is allowed *only* on the orchestrator's own real port —
        // otherwise this guard could be used to probe whatever else is
        // quietly listening on 127.0.0.1 on this host.
        let err = validate_target_url("http://127.0.0.1:9999/")
            .await
            .unwrap_err();
        assert!(
            err.contains("disallowed"),
            "expected a disallowed-range rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn validate_target_url_allows_the_documented_orchestrator_target() {
        validate_target_url("http://127.0.0.1:8080")
            .await
            .expect("the documented default target must remain allowed");
        validate_target_url("http://localhost:8080")
            .await
            .expect("localhost resolves to loopback and must remain allowed");
    }

    /// End-to-end proof, not just the isolated guard function: a real
    /// `PytestExecutor`/`SchemathesisExecutor` given a metadata-range
    /// `base_url`/`url` must reject it and return, never reaching (let
    /// alone completing) a subprocess spawn — proven by returning
    /// `INFRA_ERROR` immediately rather than hanging on a real process
    /// launch/timeout.
    #[tokio::test]
    async fn pytest_executor_rejects_metadata_range_base_url_before_spawning() {
        let executor = PytestExecutor {
            project_dir: PathBuf::from("."),
        };
        let task = TaskDef {
            id: "t".into(),
            name: "t".into(),
            params: serde_json::json!({ "base_url": "http://169.254.169.254/" }),
            ..Default::default()
        };
        let outcome = executor.execute(&task).await;
        assert_eq!(outcome.status, TaskStatus::InfraError);
        assert!(outcome.reason.unwrap_or_default().contains("disallowed"));
    }

    // -------------------------------------------- Critical finding — sandboxing

    /// The happy path must still work — the whole point of "smallest
    /// version that does not break the existing adapters" is that a
    /// successful invocation looks exactly like it did before this
    /// session, just contained.
    #[cfg(windows)]
    #[tokio::test]
    async fn run_contained_still_returns_real_process_output_on_success() {
        // Through the same `shell_command` every real adapter above
        // already uses (not a hand-rolled invocation) — this proves
        // `run_contained` doesn't change how a real executor's command
        // actually runs, only that it's now contained while doing so.
        let cmd = shell_command(
            "echo",
            &["hello-from-contained-process"],
            std::path::Path::new("."),
        );
        let output = run_contained(cmd, "test")
            .await
            .expect("spawn must succeed");
        assert!(
            output.status.success(),
            "status={:?} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("hello-from-contained-process"),
            "stdout={stdout:?} stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The actual attack this closes: a process that `kill_on_drop` alone
    /// would not reliably kill (this test doesn't even give `kill_on_drop`
    /// the chance to fire — the `Child` stays alive and in scope the whole
    /// test; only the *job* is dropped) must still die, because the job
    /// itself was told to kill everything in it on close
    /// (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). This is real: a genuine,
    /// otherwise long-running process is spawned, confirmed alive, and
    /// then confirmed dead within 2 seconds of the job (not the process)
    /// being dropped.
    #[cfg(windows)]
    #[tokio::test]
    async fn job_object_kill_on_close_actually_terminates_a_running_process() {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.args(["/C", "ping -n 30 127.0.0.1 >NUL"])
            .kill_on_drop(false);
        let mut child = cmd.spawn().expect("spawn must succeed");

        let job = job_containment::ProcessJob::new(
            EXECUTOR_JOB_MAX_PROCESSES,
            EXECUTOR_JOB_MAX_MEMORY_BYTES,
        )
        .expect("job creation must succeed on any Windows version this targets");
        let handle = job_containment::raw_handle(&child)
            .expect("a freshly spawned child must have a raw handle");
        job.assign(handle).expect("assignment must succeed");

        assert!(
            matches!(child.try_wait(), Ok(None)),
            "the process must still be running before the job is dropped, or this test proves nothing"
        );

        drop(job); // <-- the actual mechanism under test

        let mut exited = false;
        for _ in 0..20 {
            if matches!(child.try_wait(), Ok(Some(_))) {
                exited = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(
            exited,
            "dropping the ProcessJob must terminate every process still assigned to it \
             (JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE) — the process was still running 2s later"
        );
    }

    #[tokio::test]
    async fn schemathesis_executor_rejects_metadata_range_url_before_spawning() {
        let executor = SchemathesisExecutor {
            project_dir: PathBuf::from("."),
        };
        let task = TaskDef {
            id: "t".into(),
            name: "t".into(),
            params: serde_json::json!({ "url": "http://169.254.169.254/" }),
            ..Default::default()
        };
        let outcome = executor.execute(&task).await;
        assert_eq!(outcome.status, TaskStatus::InfraError);
        assert!(outcome.reason.unwrap_or_default().contains("disallowed"));
    }
}
