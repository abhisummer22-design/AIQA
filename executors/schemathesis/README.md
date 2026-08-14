# Schemathesis executor fixture

`openapi.yaml` is a real (minimal) OpenAPI 3.0 document describing this
orchestrator's own `GET /health`. The Rust `SchemathesisExecutor` shells out
to the real `schemathesis` CLI (`pip install -r requirements.txt`, or just
`pip install schemathesis` — it registers a `schemathesis` console-script
entry point on PATH) against whatever `--url` the task's `params.url` names,
default `http://localhost:8080` — this orchestrator's own address, same
pattern as the Newman/Pytest fixtures.

Unlike the other adapters, this one is property-based: Schemathesis reads
the OpenAPI schema and *generates* request cases per operation rather than
running examples someone wrote by hand. Verified live against this
orchestrator: it correctly passes the happy-path `GET /health` case, and —
without anyone crafting a "deliberately wrong" fixture for it, unlike the
Newman/Playwright/Pytest suites — its default `unsupported_method` check
generates a `TRACE /health` request and finds a real API-contract gap this
codebase actually has: an unlisted method returns whatever the framework's
default is instead of `405 Method Not Allowed`. That's a genuine finding
from genuine fuzzing, not a simulated one.

Windows note: the `schemathesis` CLI prints Unicode box-drawing/emoji
output. When its stdout is a pipe (not a real console — which is what
`std::process::Command::output()` always gives it), Python picks the OS
ANSI codepage instead of UTF-8 and crashes with `UnicodeEncodeError` unless
`PYTHONIOENCODING=utf-8`/`PYTHONUTF8=1` are set — `shell_command` in
`src/executor.rs` sets both unconditionally for every adapter it runs.
