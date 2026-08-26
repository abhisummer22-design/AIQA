# AIQA — Security Remediation: The Four Deferred Findings, Closed

**Date:** 22 August 2026
**Scope:** The four Critical/Medium security findings Phase 1's own audits raised on 17 August,
deferred with a named destination ("Phase 2"), and never touched, fixed, or formally retracted in any
of the ~55 documents written since — reconciling `AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md` §7/§10's
**R2** risk ("Unretracted Critical/Medium security findings … sitting under a recent 'no security
defect found' verdict"). No unrelated module was touched; no existing architecture was redesigned; no
working code was replaced with something narrower.

---

## 1. What changed, in one paragraph

All four findings are closed as of this session, three by a real code fix with a real test that
exercises the specific thing being closed, and the fourth (executor sandboxing) by a genuine, working,
real OS-level containment mechanism plus an explicit, dated acceptance of the part of that finding this
environment cannot fully close today. CORS is now an explicit origin allow-list, not a wildcard. The
`pytest`/`schemathesis` executors now reject link-local, cloud-metadata, and private-range targets
before any subprocess is ever spawned. A `PENDING_APPROVAL` run now survives a real process restart and
remains approvable/rejectable — and a second, more severe bug was found and fixed alongside the
documented one: startup recovery was silently destroying every pending approval on *every* restart, not
merely making it unreachable. Every executor subprocess now runs inside a real Windows Job Object, which
closes a real gap `kill_on_drop` alone did not (process-tree cleanup on timeout/cancel/shutdown), while
filesystem/network isolation remains a named, accepted residual risk — no container runtime is
available in this environment, and enabling one is a system-level decision outside this session's
authority to make unilaterally.

Fresh backend baseline before any change: **177 passed, 0 failed, 1 ignored.** After: **190 passed, 0
failed, 1 ignored** (13 new tests, all closing a specific, named gap). Frontend, re-run for symmetry
though untouched: **339 passed, 17 files**, unchanged.

---

## 2. The four findings, in their own words

Quoted exactly from where each was first raised, per the governing prompt's own citations.

**Executor sandboxing** — `AIQA_ARCHITECTURE_AUDIT_REPORT.md`:
> **CRITICAL SECURITY RISK:** Executors (`Playwright`, `Newman`, `Maestro`, `Schemathesis`) launch as
> direct host subprocesses (via `cmd /C` on Windows). There is **no sandboxing or container isolation**.
> While basic parameter sanitization exists, running arbitrary test workloads directly on the host
> orchestrator machine is highly vulnerable to container escape and host compromise.

> | **Critical** | Missing Executor Isolation | `executor.rs` uses `cmd /C` and `tokio::process::Command`
> without sandboxing. | Direct host compromise. Remote Code Execution via parameter injection. | Migrate
> executors to isolated Docker containers or microVMs (e.g., Firecracker). Remove `cmd /C` shelling. |

**M2 — SSRF via executor URL parameters** — `AIQA_BACKEND_AUDIT_REPORT.md`:
> `executor.rs:309, 363`. `pytest`'s `base_url` and `schemathesis`'s `url` come straight from the
> `POST /runs` body. `sanitize_token` permits `:` and `/`, so `http://169.254.169.254/…` or any internal
> host passes. Schemathesis will then actively fuzz that target from the orchestrator's network
> position. Requires an operator key, but operator is not intended to be a network-pivot role.
> **Fix:** allow-list schemes and hosts, or require targets to be pre-registered.

**B12 — CORS `allow_origin(Any)`** — `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md` /
`AIQA_DEEP_AUDIT_REPORT.md` §15.2:
> `CorsLayer::new().allow_origin(Any)` (`main.rs:101`) — Permissive; every route still Bearer-gated.
>
> Permissive CORS on the orchestrator. `main.rs:101` sets `allow_origin(Any)`. Every route is
> Bearer-gated so this is not directly exploitable, but it should be restricted to known origins before
> any non-local deployment.

**B11/L4 — approve/reject are process-local** — `AIQA_BACKEND_AUDIT_REPORT.md` / `AIQA_DEEP_AUDIT_REPORT.md`:
> `api.rs:270-282`. Both return 404 if the run's `RunRecord` is not in this process's memory, so a
> restart voids every pending approval. Documented in the code; a real constraint on multi-process
> deployment.

And the consolidated review's own framing of why all four still matter (§7):
> The corpus therefore contains an unretracted Critical finding and an unretracted SSRF finding sitting
> underneath a recent clean bill of health. **This should be treated as an open reconciliation item, not
> as evidence that the findings were resolved.**

---

## 3. Current-source re-verification (before any change)

Per the governing instruction — checked directly, this session, before writing a line of code:

- `services/orchestrator/src/main.rs:127-132` (the file has moved slightly since 17 Aug; content
  unchanged): `CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)`. **Confirmed
  still present.**
- `executor.rs`: `PytestExecutor::execute` and `SchemathesisExecutor::execute` read `base_url`/`url` via
  `safe_param`, which calls `sanitize_token` — a character-class check (`alphanumeric | ' .-_/\:'`) that
  has no concept of "host" at all. **Confirmed still exploitable** — `http://169.254.169.254/` passes
  every character in `sanitize_token`'s allowed set.
- `api.rs::approve_run`/`reject_run`/`approve_run_scoped`/`reject_run_scoped`: all four looked up the run
  via `state.runs.read().await.get(&id)` only, 404-ing with the literal message *"run not held in this
  process's memory (see README)"* on a miss. **Confirmed still true**, and worse than documented — see §6.
- `executor.rs`: every real adapter (`PlaywrightExecutor`, `NewmanExecutor`, `PytestExecutor`,
  `MaestroExecutor`, `SchemathesisExecutor`) called `.output()` directly on a `tokio::process::Command`
  with no job/container/restricted-account wrapper of any kind. **Confirmed still true.**

---

## 4. Fix 1 — CORS (B12)

**Before:** a wildcard (`Any`) that echoes back whatever `Origin` header a request carries.

**After:** `api::cors_layer_from_env()` — an explicit allow-list. `AIQA_CORS_ALLOWED_ORIGINS`
(comma-separated) overrides it entirely when set; unset, it defaults to `forentend`'s own documented dev
origin (`http://localhost:3000`, `http://127.0.0.1:3000` — see `forentend/package.json`'s `dev` script,
`vite --port=3000`). `apps/web` (unmaintained since 18 Aug — see
`AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md` §6.1) is deliberately not in the default list.

No other real deployment origin was named as needed right now, so none was guessed into the default —
per the governing instruction, that decision is the env var, not a guess baked into source.

**Test:** `cors_rejects_a_disallowed_origin` / `cors_allows_the_configured_origin`
(`api_tests.rs`). Drives a real request through the real router wrapped in the real
`cors_layer_for_origins` construction (the same function `main.rs` calls) and inspects the actual
`Access-Control-Allow-Origin` response header — CORS is a browser-enforced header, not a server-rejected
status code, so the header is the only thing that actually proves the fix. A disallowed origin never
gets its value echoed back; the configured one does, exactly.

---

## 5. Fix 2 — SSRF (M2)

**Before:** `sanitize_token` (a shell-injection guard, not a network-destination guard) was the only
check on `base_url`/`url`.

**After:** `executor::validate_target_url` — parses the target, rejects any scheme but `http`/`https`,
and resolves the host (literal IPs checked directly; hostnames resolved via `tokio::net::lookup_host` and
*every* resolved address checked, closing the "friendly hostname that happens to resolve to a metadata
address" variant, not just the literal-IP one) against `is_blocked_ip`: link-local (169.254.0.0/16 — the
cloud metadata range), RFC1918 private ranges, unspecified/broadcast/documentation/multicast, and the
IPv6 equivalents are all rejected. The one explicit allowed exception is loopback **on this
orchestrator's own real port (8080)** — the one destination these two adapters are actually documented
to target — so this guard cannot be repurposed to probe whatever else happens to be listening on
`127.0.0.1` on a different port.

Called in both `PytestExecutor::execute` and `SchemathesisExecutor::execute` immediately after
`safe_param`, before any `Command`/`shell_command` construction — a rejection returns `INFRA_ERROR`
straight from the handler, so no subprocess is ever spawned for a disallowed target.

**Tests:** 7, in `executor.rs`:
- `validate_target_url_rejects_cloud_metadata_address` — the exact M2 reproduction
  (`169.254.169.254`).
- `validate_target_url_rejects_rfc1918_private_ranges` (10/8, 172.16/12, 192.168/16).
- `validate_target_url_rejects_non_http_schemes` (`file://`).
- `validate_target_url_rejects_loopback_on_a_non_orchestrator_port`.
- `validate_target_url_allows_the_documented_orchestrator_target` (`127.0.0.1:8080`, `localhost:8080` —
  the fix must not break the documented default).
- `pytest_executor_rejects_metadata_range_base_url_before_spawning` /
  `schemathesis_executor_rejects_metadata_range_url_before_spawning` — end-to-end through the real
  `Executor::execute`, proving the guard is actually wired in, not just correct in isolation.

---

## 6. Fix 3 — Approval durability (B11/L4)

**Before, and worse than documented.** The finding as written says a restart makes a pending approval
*inaccessible* (404). Reading `engine::recover_incomplete_runs` this session found it is actually worse:
`store.list_incomplete_runs()`'s SQL explicitly includes `PENDING_APPROVAL` runs (their tasks sit
`QUEUED` by design, since they were never spawned) — and the recovery loop treated *every* run it
returned identically to an orphaned in-flight run, forcing those `QUEUED` tasks to `INFRA_ERROR` and the
run itself to `FAILED`. **Every clean restart was silently destroying every pending approval**, not
merely making it unreachable in memory. This is a second, more severe defect this session found and
fixed alongside the one that was actually documented — exactly the kind of thing "verify the current
source yourself" is for.

**After, two parts:**
1. `engine::recover_incomplete_runs` now checks each incomplete run's real current state before touching
   its tasks, and skips (`continue`s past) anything already `PendingApproval` entirely — untouched, not
   force-failed.
2. `store::get_run_for_rehydration` + `api::rehydrate_run_record`/`find_or_rehydrate_run`: the in-memory
   lookup (`state.runs`) is tried first as before; on a miss, the full `RunPlan` — persisted verbatim as
   JSON by `insert_run` since the durability work that predates this session — is read back and used to
   rebuild a live `Arc<RunRecord>`, which is inserted into `state.runs` so every subsequent operation
   sees it exactly as if this process had created it. All four handlers (`approve_run`, `reject_run`,
   `approve_run_scoped`, `reject_run_scoped`) now go through this shared path.

**Tests:** `approval_survives_a_process_restart_and_can_still_be_approved` and
`rejection_survives_a_process_restart_and_can_still_be_rejected` (`api_tests.rs`) — "restart" simulated
the same way this codebase's own existing durability tests do it
(`ai_settings_survive_a_fresh_store_reload_real_durable_persistence`): a completely independent
`AppState`/`Store`/registry reconnected to the same real sqlite file, `state.runs` starting empty, which
is exactly what a real restarted process has. The approval test additionally runs the real
`recover_incomplete_runs` pass in between and asserts the run's state is still `PendingApproval`
afterward — proving the more severe bug is actually fixed, not just the in-memory-lookup half of it.

---

## 7. Fix 4 — Executor sandboxing (Critical) — the hardest one

### What was investigated

The finding's own suggested fix — Docker containers or microVMs — was checked fresh, this session, not
assumed from the README: `docker`/`podman` are both absent from `PATH`, and `wsl --status` reports WSL
itself is not installed (enabling it is a Windows-feature change requiring elevation and, typically, a
reboot). A `Get-WindowsOptionalFeature` probe for Windows Sandbox failed with *"The requested operation
requires elevation"* — this session does not have admin rights on this machine. None of the three
container/VM-based options the original finding's fix direction named are achievable right now without
either a heavy new install (Docker Desktop) or a system-level feature change requiring elevation this
session does not have — exactly the kind of decision that should be asked for explicitly, not made
unilaterally mid-session, the same principle already applied earlier when installing the Rust toolchain
itself required asking first.

### What was actually built

A real, working Win32 Job Object around every executor invocation (`executor.rs::job_containment`,
Windows-only — no executor is wired up on any other target). This is a genuinely different *kind* of
protection from `sanitize_token`/`validate_target_url` above, not a bigger version of the same thing:
`tokio::process::Command::kill_on_drop(true)` (already present, B5) only guarantees the *direct* child
dies when its handle drops — it does not reach grandchildren. A `cmd /C npx playwright test …` chain can
spawn a real browser process underneath `npx`; killing `cmd.exe`/`npx` alone does not obligate Windows to
also kill that browser. A Job Object does: every process ever assigned to it dies together the instant
the job's last handle closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), which now happens automatically via
`Drop` — including on the existing B5 timeout path, where the executor future is dropped without any
explicit kill call anywhere today. It also imposes a hard ceiling on how many processes
(`JOB_OBJECT_LIMIT_ACTIVE_PROCESS`, 64) and how much memory (`JOB_OBJECT_LIMIT_JOB_MEMORY`, 2 GiB) one
invocation's entire process tree may ever use — a real backstop against a runaway or fork-bombing tool
that no amount of parameter sanitization touches.

`run_contained` replaces the bare `cmd.output().await` every real adapter used to call: it spawns,
assigns the child to a fresh job, waits, then drops the job. If job creation or assignment itself fails
for any reason, the process is still awaited rather than abandoned — a containment failure must never
leave a process running *less* supervised than the pre-fix code path did.

**A real bug this session's own test caught before it shipped:** the first version of `run_contained`
called `.spawn()` directly, which — unlike `.output()`, the method every call site used before —
does **not** automatically pipe stdout/stderr. The happy-path test
(`run_contained_still_returns_real_process_output_on_success`) failed with empty `stdout`/`stderr` on the
first real run, which is exactly what it exists to catch: every executor's actual pass/fail reason text
and evidence capture would have silently gone empty in production, on every successful run, with no
compile error and no other existing test positioned to notice (the executor structs are not otherwise
exercised against real subprocesses anywhere else in this suite). Fixed by explicitly configuring
`Stdio::piped()` before spawn; re-verified passing.

### What this does NOT claim

No filesystem or network isolation. A process inside this job can still read/write anywhere its own
Windows ACLs allow, and reach any network destination the M2 fix (§5) doesn't already block. Closing that
fully needs a real container runtime or Windows Sandbox — both blocked on the elevation/install decision
above. This is not a new gap introduced by this session; it is the exact remainder the original Critical
finding named, now with everything achievable *without* a system-level decision actually built, real,
and tested, and everything that still needs one named precisely rather than silently dropped.

**Tests:**
- `run_contained_still_returns_real_process_output_on_success` — the happy path (real output correctly
  captured; caught the stdout/stderr bug above).
- `job_object_kill_on_close_actually_terminates_a_running_process` — the actual mechanism, not a proxy
  for it: spawns a real, genuinely long-running process (`ping -n 30 127.0.0.1`) with `kill_on_drop(false)`
  set explicitly so tokio's own child-drop machinery cannot be what kills it, confirms it is really still
  running, drops only the `ProcessJob` (never the `Child` itself, which stays in scope and alive
  throughout), and polls for up to 2 seconds confirming the process actually exits. This is the one test
  in this whole unit that would fail if the Job Object mechanism were cosmetic rather than real — it
  passed.

---

## 8. Regression Verification

**Backend**, fresh from a clean checkout of the current source, before and after:

```
cd services/orchestrator && cargo test
```
Before: **177 passed, 0 failed, 1 ignored.**
After: **190 passed, 0 failed, 1 ignored.** (+13: 7 SSRF, 2 CORS, 2 approval-durability, 2 sandboxing —
every one of them new, none replacing or weakening an existing assertion.)

No pre-existing test was deleted, skipped, or had an assertion loosened to make this pass.

**Frontend**, re-run for symmetry though nothing in `forentend`/`apps/web` was touched by this unit:

```
cd forentend && npx vitest run
```
**339 passed, 17 files, 0 failed** — unchanged, as expected.

---

## 9. Fake/Stub Audit

Grepped every changed file for `fake|mock|demo|sample|placeholder|stub|simulated|TODO|FIXME`. Every hit
is a legitimate test scenario name (e.g. `job_object_kill_on_close_actually_terminates_a_*process*`,
which contains no such word but is listed here for completeness of method) or absent entirely — this
unit added no test doubles at all; every new test in §4-§7 drives real code (a real router, a real CORS
layer, a real DNS resolution, a real sqlite file, a real spawned Windows process) rather than a mock of
any of them.

## 10. Security Notes on the Fixes Themselves

- The SSRF guard resolves hostnames before checking them specifically so a friendly-looking name pointing
  at a blocked address cannot bypass a literal-string check — this was a deliberate design choice, not an
  oversight the tests happened to cover.
- The CORS default is additive-only: unset, behavior is *more* restrictive than before (a real list
  instead of `Any`); nothing about the default can accidentally widen access beyond what was already
  possible.
- The Job Object's raw Win32 handles are never exposed outside `job_containment`; `ProcessJob` owns its
  handle exclusively and closes it exactly once, in `Drop`.
- No credential, API key, or other secret was read, logged, or touched by any of this work.

## 11. Files Changed

`services/orchestrator/`: `Cargo.toml` (+17, the Windows-only `windows-sys` dependency), `Cargo.lock`
(dependency-resolution side effect of the above, not hand-edited), `src/main.rs` (CORS layer now built by
`api::cors_layer_from_env()`), `src/api.rs` (`cors_layer_from_env`/`cors_layer_for_origins`,
`rehydrate_run_record`/`find_or_rehydrate_run`, all four approve/reject handlers updated),
`src/executor.rs` (`validate_target_url`/`is_blocked_ip`, `job_containment` module, `run_contained`, all
five adapters updated), `src/engine.rs` (`recover_incomplete_runs`'s `PendingApproval` skip),
`src/store.rs` (`get_run_for_rehydration`), `src/api_tests.rs` (13 new tests). 8 files, 806
insertions, 46 deletions — every changed line traces to one of the four findings above; nothing else in
the repository was touched.

## 12. Remaining Limitations

- **Executor sandboxing is process-containment, not filesystem/network isolation.** Named explicitly in
  §7 — this is the one finding not fully closed, and it is not silently dropped: closing it needs a
  container runtime (Docker) or Windows Sandbox, both blocked on a system-level decision (a heavy install
  or admin elevation this session does not have) that belongs to the user, not to a mid-session
  unilateral choice. **Dated risk acceptance:** as of 22 August 2026, real process-tree containment
  (kill-on-close, process-count and memory ceilings) is in place and tested; filesystem and network
  access from within an executor subprocess are constrained only by the host's own ACLs and the M2 SSRF
  guard, not by a sandbox boundary. **Concrete follow-up, when wanted:** install Docker Desktop, or run
  `wsl --install` (both require the user's explicit go-ahead and, for WSL, likely a reboot), then migrate
  `executor.rs`'s five adapters to spawn inside that boundary instead of (or in addition to) the Job
  Object — the Job Object code does not need to be removed to do this; the two mechanisms are
  complementary, not alternatives.
- **CORS's default origin list is exactly one entry pair** (`forentend`'s dev server, `localhost`/`127.0.0.1`).
  No other real deployment origin was named as needed, so none was added — `AIQA_CORS_ALLOWED_ORIGINS` is
  the mechanism for that when a real one exists.
- **The SSRF guard's loopback exception is pinned to port 8080.** If the orchestrator's own listening
  port ever becomes configurable (it is currently hardcoded in `main.rs`), this allow-list needs updating
  alongside it — flagged in code, not just here.
- Nothing else about the four findings' own original scope was expanded or narrowed. M3 (no bound on
  plan task count) and the other findings the consolidated review lists as "also unclosed" were left
  exactly as they were — out of scope for this unit by the governing instruction's own boundary.

## 13. Per-Finding Final Status

| Finding | Original severity | Status |
|---|---|---|
| Executor sandboxing | **Critical** | **Partially fixed, remainder formally accepted with a dated statement and a concrete follow-up (§12)** — real process-tree containment shipped and tested; filesystem/network isolation named as the explicit residual risk, not silently dropped |
| M2 — SSRF | Medium | **Fixed** — real destination validation, 7 tests, one of which reproduces the exact `169.254.169.254` case from the original finding |
| B12 — CORS `Any` | Medium | **Fixed** — explicit allow-list, real header-level test |
| B11/L4 — process-local approval | Low→structural (a more severe variant found and fixed alongside it) | **Fixed** — durable across a real restart, both approve and reject paths tested, the more severe recovery-sweep bug closed too |

Three of four findings are fully closed. The fourth is genuinely, substantially improved with a real,
tested mechanism, and its remaining gap is named, dated, and given a concrete path forward rather than
left to be rediscovered — which is the actual failure mode this whole unit exists to correct (§1, §7 of
the consolidated review: findings dropped silently, not reconciled).
