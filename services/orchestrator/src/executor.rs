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
        match shell_command("npx", &args, &self.project_dir).output().await {
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
        match shell_command("npx", &args, &self.project_dir).output().await {
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
        let mut cmd = tokio::process::Command::new("python");
        cmd.args(["-m", "pytest", "-q", node_id.as_str()])
            .current_dir(&self.project_dir)
            .env("AIQA_TARGET_BASE_URL", base_url)
            // B5 — invoked directly rather than through `shell_command`, so
            // it needs the same kill-on-timeout guarantee.
            .kill_on_drop(true);
        match cmd.output().await {
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
        match shell_command("maestro", &args, &self.project_dir).output().await {
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
        let args = vec!["run", schema.as_str(), "--url", url.as_str()];
        match shell_command("schemathesis", &args, &self.project_dir).output().await {
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
}
