use crate::domain::{SimOutcome, SimSpec, TaskDef, TaskStatus};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

    pub fn resolve(&self, task: &TaskDef) -> Arc<dyn Executor> {
        match &task.executor {
            Some(kind) => self.by_kind.get(kind).cloned().unwrap_or_else(|| self.default.clone()),
            None => self.default.clone(),
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
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_' | '/' | '\\' | ':'));
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
    std_cmd.env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1");
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
async fn validate_target_url(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|e| format!("rejected target URL {raw:?}: {e}"))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(format!("rejected target URL {raw:?}: only http/https schemes are allowed, got {:?}", url.scheme()));
    }
    let host = url.host_str().ok_or_else(|| format!("rejected target URL {raw:?}: no host"))?;
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
        return Err(format!("rejected target {host}:{port}: host resolved to no addresses"));
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
            info.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS | JOB_OBJECT_LIMIT_JOB_MEMORY;
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
async fn run_contained(mut cmd: tokio::process::Command, label: &str) -> Result<std::process::Output, std::io::Error> {
    // `Command::output()` (what every call site here used before this
    // session) pipes stdout/stderr automatically as part of that one call.
    // `spawn()` does not — without this, `wait_with_output` below would
    // return empty output for a real, successful process, silently
    // breaking every adapter's actual result reporting.
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let child = cmd.spawn()?;

    #[cfg(windows)]
    {
        match job_containment::ProcessJob::new(EXECUTOR_JOB_MAX_PROCESSES, EXECUTOR_JOB_MAX_MEMORY_BYTES) {
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
                tracing::warn!("{label}: failed to create containment job, running un-contained: {e}");
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
    let evidence = if combined.trim().is_empty() { None } else { Some(combined.clone()) };
    if output.status.success() {
        return ExecutionOutcome {
            status: TaskStatus::Passed,
            reason: None,
            evidence,
        };
    }
    let lower = combined.to_lowercase();
    let looks_like_missing_tool = lower.contains("is not recognized as an internal or external command")
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
    let raw = task.params.get(key).and_then(|v| v.as_str()).unwrap_or(default);
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

/// Web executor (SRD §17: "Playwright handles web"). `params.spec` selects
/// the spec file, `params.grep` selects one test title within it (both
/// relative to `project_dir`, which points at executors/playwright).
pub struct PlaywrightExecutor {
    pub project_dir: PathBuf,
}

#[async_trait]
impl Executor for PlaywrightExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let spec = param_or_return!(safe_param(task, "spec", "tests/sample.spec.ts"));
        let grep = param_or_return!(safe_param(task, "grep", ""));
        let mut args = vec!["playwright", "test", spec.as_str(), "--reporter=line"];
        if !grep.is_empty() {
            args.push("--grep");
            args.push(grep.as_str());
        }
        match run_contained(shell_command("npx", &args, &self.project_dir), "playwright").await {
            Ok(output) => outcome_from_output(output, "playwright"),
            Err(e) => spawn_error_outcome("playwright", e),
        }
    }
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
        let args = vec!["newman", "run", "collection.json", "--folder", folder.as_str()];
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
            return ExecutionOutcome { status: TaskStatus::InfraError, reason: Some(reason), evidence: None };
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

#[async_trait]
impl Executor for MaestroExecutor {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome {
        let flow = param_or_return!(safe_param(task, "flow", "flow.yaml"));
        let args = vec!["test", flow.as_str()];
        match run_contained(shell_command("maestro", &args, &self.project_dir), "maestro").await {
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
            return ExecutionOutcome { status: TaskStatus::InfraError, reason: Some(reason), evidence: None };
        }
        let args = vec!["run", schema.as_str(), "--url", url.as_str()];
        match run_contained(shell_command("schemathesis", &args, &self.project_dir), "schemathesis").await {
            Ok(output) => outcome_from_output(output, "schemathesis"),
            Err(e) => spawn_error_outcome("schemathesis", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_token_accepts_realistic_params() {
        assert!(sanitize_token("tests/sample.spec.ts").is_ok());
        assert!(sanitize_token("homepage has correct title and heading").is_ok());
        assert!(sanitize_token("health check passes").is_ok());
        assert!(sanitize_token("test_health.py::test_health_returns_200_and_ok_body").is_ok());
        assert!(sanitize_token("http://localhost:8080").is_ok());
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
            assert!(sanitize_token(bad).is_err(), "expected rejection for {bad:?}");
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
        let err = validate_target_url("http://169.254.169.254/latest/meta-data/").await.unwrap_err();
        assert!(err.contains("disallowed"), "expected a disallowed-range rejection, got: {err}");
    }

    #[tokio::test]
    async fn validate_target_url_rejects_rfc1918_private_ranges() {
        for target in ["http://10.0.0.5:80", "http://172.16.0.5:80", "http://192.168.1.1:80"] {
            match validate_target_url(target).await {
                Err(err) => assert!(err.contains("disallowed"), "expected a disallowed-range rejection for {target}, got: {err}"),
                Ok(()) => panic!("expected {target} to be rejected as a private range"),
            }
        }
    }

    #[tokio::test]
    async fn validate_target_url_rejects_non_http_schemes() {
        let err = validate_target_url("file:///etc/passwd").await.unwrap_err();
        assert!(err.contains("scheme"), "expected a scheme rejection, got: {err}");
    }

    #[tokio::test]
    async fn validate_target_url_rejects_loopback_on_a_non_orchestrator_port() {
        // Loopback is allowed *only* on the orchestrator's own real port —
        // otherwise this guard could be used to probe whatever else is
        // quietly listening on 127.0.0.1 on this host.
        let err = validate_target_url("http://127.0.0.1:9999/").await.unwrap_err();
        assert!(err.contains("disallowed"), "expected a disallowed-range rejection, got: {err}");
    }

    #[tokio::test]
    async fn validate_target_url_allows_the_documented_orchestrator_target() {
        validate_target_url("http://127.0.0.1:8080").await.expect("the documented default target must remain allowed");
        validate_target_url("http://localhost:8080").await.expect("localhost resolves to loopback and must remain allowed");
    }

    /// End-to-end proof, not just the isolated guard function: a real
    /// `PytestExecutor`/`SchemathesisExecutor` given a metadata-range
    /// `base_url`/`url` must reject it and return, never reaching (let
    /// alone completing) a subprocess spawn — proven by returning
    /// `INFRA_ERROR` immediately rather than hanging on a real process
    /// launch/timeout.
    #[tokio::test]
    async fn pytest_executor_rejects_metadata_range_base_url_before_spawning() {
        let executor = PytestExecutor { project_dir: PathBuf::from(".") };
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
        let cmd = shell_command("echo", &["hello-from-contained-process"], std::path::Path::new("."));
        let output = run_contained(cmd, "test").await.expect("spawn must succeed");
        assert!(output.status.success(), "status={:?} stderr={}", output.status, String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello-from-contained-process"), "stdout={stdout:?} stderr={}", String::from_utf8_lossy(&output.stderr));
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
        cmd.args(["/C", "ping -n 30 127.0.0.1 >NUL"]).kill_on_drop(false);
        let mut child = cmd.spawn().expect("spawn must succeed");

        let job = job_containment::ProcessJob::new(EXECUTOR_JOB_MAX_PROCESSES, EXECUTOR_JOB_MAX_MEMORY_BYTES)
            .expect("job creation must succeed on any Windows version this targets");
        let handle = job_containment::raw_handle(&child).expect("a freshly spawned child must have a raw handle");
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
        let executor = SchemathesisExecutor { project_dir: PathBuf::from(".") };
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
