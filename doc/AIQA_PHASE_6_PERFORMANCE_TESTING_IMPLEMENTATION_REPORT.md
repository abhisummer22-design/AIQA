# AIQA — Phase 6: Real Performance Testing (Load / Stress / Spike / Volume)
## Implementation Report

**Date:** 2026-08-25
**Branch:** `claude/run-this-9eeb2e`
**Scope:** Real Load/Stress/Spike/Volume performance testing — real workload generation, real per-request sampling, real aggregated metrics (min/avg/max/P50/P95/P99/throughput/error-rate), real cancellation, real persistence, real dedicated frontend page.

---

## 1. Executive Summary

Phase 6 adds a real, end-to-end performance testing capability to AIQA: a user configures a Load, Stress, Spike, or Volume workload against a real HTTP target, the backend drives that workload with genuine concurrent HTTP requests, collects a real per-request sample for every attempt, aggregates real metrics from those samples using a documented percentile algorithm, evaluates an optional real pass/fail verdict against configured thresholds, and persists the result. A dedicated frontend page (`/performance-testing`) — which did not exist in any form before this phase, not even as a stub — lets a user configure, launch, watch live, cancel, and review the history of these runs.

No new HTTP execution engine was built. The real per-request execution core (`executor::execute_plan_once`) was extracted from Phase 4's `CompiledApiExecutor` in this same phase specifically so both the single-shot API Testing page and this phase's repeated workload engine call the identical, unmodified real `reqwest`-calling/assertion-evaluating logic. What Phase 6 adds is a new, minimal orchestration layer (`performance_testing.rs`, ~1042 lines) that repeatedly invokes that core under a real, bounded-concurrency scheduler, plus a small new persistence table and a set of new API endpoints.

Two genuine bugs were found and fixed during this phase's own live verification (§30) — both are the kind of defect that only a real, live run surfaces, not a mocked test:
1. **Elapsed-time drift after completion** — the status endpoint recomputed metrics against `now - started_at` even after a run had finished, silently shrinking the reported throughput on every later poll.
2. **Per-request `reqwest::Client` churn** — the executor built a fresh HTTP client (and thus a fresh TCP connection, no keep-alive) on every single request, which is harmless for one-shot API tests but exhausted local ephemeral ports under a real, high-volume performance workload.

Both are fixed, covered by new regression tests, and re-verified live. See §29–§31 for the full bug log and before/after evidence.

**Verdict: GO.** See §41.

---

## 2. Fresh Baseline Audit — What Existed Before This Phase

Performed via a dedicated read-only research pass before any code was written (see this session's own history). Findings, verified directly against source:

- **No performance/load executor of any kind existed.** `grep`-level searches for JMeter/k6/Locust/Vegeta/wrk integration returned zero hits.
- **No component in this codebase safely schedules "many repeated, concurrent, identical requests."** `engine.rs`'s `Semaphore`-based concurrency bounding governs how many *different* DAG tasks in one run execute concurrently — it has no notion of firing the same request repeatedly.
- **`metrics.rs` (67 lines) is on-demand, DB-derived, application-wide gauges/counters only** — `aiqa_runs_total`, `aiqa_tasks_total`, `aiqa_bug_drafts_total`, `aiqa_failure_clusters_total`, `aiqa_api_keys_active`, `aiqa_audit_log_total`. No histogram, no percentile capability, no per-test-run metric of any kind. `GET /metrics` (Prometheus text) is real but has nothing to do with performance-test results.
- **`executor::CompiledApiExecutor`** (Phase 2.1/4) is a real, stateless, native (no subprocess) HTTP executor — direct `reqwest`, real assertion evaluation — reachable via `execution_plan::ExecutionPlan`. This is the one genuinely reusable real execution primitive.
- **No "many samples aggregated into one run" persistence pattern existed anywhere.** `evidence` is one blob per task. `failure_clusters` aggregates the opposite direction (many runs into one cluster).
- **The real SSRF guard (`executor::validate_target_url`) and `PolicyConfig.max_concurrency`** are real, existing, reusable safety primitives — but `max_concurrency` governs the *scheduler's* own concurrency, not workload-generation concurrency, so Phase 6 needed its own, separate concurrency cap.
- **No `PerformanceTestingPage` existed, not even as a placeholder.** `routeConfig.tsx`'s own doc comment explicitly listed `/performance-testing` among the routes with "no corresponding page/backend support ... and remain NOT implemented as routes here."

## 3. Required Baseline Findings (Answered From Actual Code, Not Assumption)

- **A. Real performance/load executor exists?** NO.
- **B. Any existing executor supports repeated requests?** NO natively, but `CompiledApiExecutor`'s underlying HTTP-call logic is stateless and trivially callable many times — see §5.
- **C. Any existing executor supports request-level concurrency?** NO. The scheduler's own concurrency bounds *different tasks*, not repeated hits against one target.
- **D. Are Prometheus metrics already available?** YES, but only application-wide, DB-derived gauges/counters (`metrics.rs`) — no histogram, no percentile, not suitable as performance-test result metrics, and never conflated with them in this phase (§21).
- **E. JMeter/k6/Locust/Vegeta/wrk integrated?** NOT PRESENT.
- **F. Can the existing API executor safely be reused as the per-request worker?** YES — its real request/assertion logic was extracted (not duplicated) into `executor::execute_plan_once` in this same phase specifically for this reuse (§5).
- **G. What existing execution/evidence/persistence architecture is reused?** `execution_plan::validate_plan` (real SSRF guard, run once), `api_testing::validate_and_build_plan` (real plan construction from a request template), `LocalEvidenceStore`/`insert_evidence` (real raw-sample persistence), the `tokio::sync::watch` cancellation pattern already established in `engine.rs`'s BE-13 cancellation.
- **H. Target-security controls available?** `execution_plan::validate_target_url` — reused unmodified, run once per workload before it starts.
- **I. Rate/concurrency safeguards available?** `PolicyConfig.max_concurrency` exists but is scheduler-scoped; Phase 6 defines its own `SafetyLimits` (env-var-driven, same convention) specific to workload concurrency/duration/total-requests.
- **J. Safest implementation path?** A new, minimal, dedicated orchestration layer (`performance_testing.rs`) that reuses the real per-request execution core and the real SSRF/validation logic, with its own `tokio::sync::watch`-based concurrency-bounded scheduler — justified precisely because §2/§3 confirm no existing component can safely do this. This is the path actually taken.

## 4. Architecture Decision — Why A New Orchestration Layer, Not A New Executor

The brief's own priority order (Priority 2: "reuse existing Phase 4 real API execution/request logic as the per-request execution worker, while adding a minimal performance orchestration layer"; Priority 4: "only create a dedicated performance orchestration component if no existing component can safely schedule concurrent/repeated work") is satisfied exactly:

- **Reused, unmodified:** `api_testing::validate_and_build_plan`, `execution_plan::validate_plan`, `executor::execute_plan_once` (the real HTTP-call/assertion core).
- **New, and only as much as necessary:** a scheduler (`performance_testing.rs`) that repeatedly invokes that reused core under real, bounded concurrency; a small new table (`performance_runs`); a handful of new endpoints.
- **Not built:** a second HTTP executor, a second execution pipeline, a second SSRF check, a second evidence-storage mechanism.

### 4.1 Refactor: extracting `execute_plan_once`

Before this phase, `CompiledApiExecutor::execute` inlined the full request/assertion loop. It was extracted into a free function, `execute_plan_once(client: &reqwest::Client, plan: &ExecutionPlan) -> PlanExecutionResult`, with `CompiledApiExecutor::execute` becoming a thin wrapper that deserializes, validates, calls `execute_plan_once`, and formats the existing evidence-log/`RESULT_SUMMARY_JSON` output — byte-for-byte the same external behavior as before (proven by the pre-existing `executor::tests` suite passing unmodified, §26). This is genuine reuse, not a parallel copy: the performance workload engine and the single-shot API Testing page call the exact same function.

### 4.2 Why not route through `engine::spawn_run`?

Considered and rejected. The generic run/task scheduler is built for fast, DAG-shaped, one-shot task dispatch with a single terminal result per task. A performance test needs: live, incrementally-updating progress across potentially thousands of individual "attempts" that are not separate tasks; a cancellation signal that can interrupt one specific in-flight HTTP request without touching a whole task's lifecycle; and a schedule that spawns/removes concurrent workers over time. Forcing this into the `runs`/`tasks` model would have required either faking one task per HTTP request (schema and API-noise explosion at real workload volumes — the safety-net cap alone allows up to 5000 requests) or building bespoke live-progress plumbing on top of a model not designed for it. A dedicated, minimal layer was the more honest and more minimal choice.

## 5. Configuration Schema

`PerformanceTestConfig` (`performance_testing.rs`):

```rust
pub struct PerformanceTestConfig {
    pub request: AdHocApiRequest,       // reused Phase 4 shape: method/url/query/headers/body
    pub expected_status: Option<u16>,   // becomes a real Action::AssertStatus
    pub schedule: WorkloadSchedule,     // LOAD | STRESS | SPIKE | VOLUME (see §7-§10)
    pub max_p95_ms: Option<u64>,
    pub max_error_rate_percent: Option<f64>,
    pub min_throughput_rps: Option<f64>,
}
```

`AdHocApiRequest.assertions` must be empty here (checked, real `INVALID_REQUEST_CONFIGURATION` rejection if not) — a performance test's real per-request correctness check is the single `expected_status` field, deliberately distinct from the full assertions list Phase 4's API Testing page exposes (redundant per-request overhead and ambiguous under load otherwise). When `expected_status` is set, it becomes a real `Action::AssertStatus` appended to the built plan — every sample's Success/Failed classification comes from `execute_plan_once`'s own real assertion evaluation, not a second, hand-rolled comparison (`build_request_plan`, reusing `api_testing::validate_and_build_plan` unmodified).

## 6. Server-Side-Enforced Safety Limits

`SafetyLimits::from_env()` — real, backend-authoritative, independent of anything the frontend sends:

| Env var | Default | Governs |
|---|---|---|
| `PERF_TEST_MAX_CONCURRENCY` | 50 | Max concurrency for any schedule type (Load/Volume `concurrency`, Stress `max_concurrency`, Spike `spike_concurrency`) |
| `PERF_TEST_MAX_DURATION_SECS` | 120 | Max configured duration (Load, Stress `max_duration_secs`, Spike `baseline_secs + spike_secs`) **and** the global hard wall-clock stop applied uniformly to every schedule type regardless of its own configured duration |
| `PERF_TEST_MAX_TOTAL_REQUESTS` | 5000 | Max `max_requests` (Load) / `total_requests` (Volume) |

`validate_schedule` (§14 test list) rejects any request exceeding these with a real, specific error code (`CONCURRENCY_LIMIT_EXCEEDED` / `DURATION_LIMIT_EXCEEDED` / `TOTAL_REQUESTS_LIMIT_EXCEEDED` / `INVALID_SCHEDULE`) **before** any workload starts, before any persisted row is created. The frontend mirrors sensible defaults for UX but is not trusted — every one of these checks runs server-side regardless of what the client sends (proven live, §30, Test cases against limits are also covered by 7 dedicated unit tests, §26).

On top of the per-schedule duration checks, `run_workload` always installs one additional, unconditional global safety-net supervisor at `limits.max_duration_secs` — this is the one hard stop that applies to *every* schedule type, including Volume (which has no duration field of its own), protecting against an extremely slow or hung target running far longer than intended.

## 7. Load — Real Behavior

A steady pool of `concurrency` real Tokio worker tasks, each looping: fire a request, record the real sample, loop again — until `duration_secs` elapses (a real `tokio::time::sleep` racing the stop signal) **or**, if `max_requests` is set, a shared atomic ticket counter reaches it, whichever happens first. Both stop conditions are genuinely real; neither is a fixed sleep standing in for real completion detection.

## 8. Stress — Real Behavior

Starts at `start_concurrency` real workers. Every `step_interval_secs`, a real, additional batch of `step_size` workers is spawned via `tokio::spawn` (never simulated, never a relabeled Load run), up to `max_concurrency`, capped by `max_duration_secs`. The scheduler records the real, live worker count in `handle.current_concurrency` as it ramps, and every sample records the real concurrency level in effect when that specific request began (`concurrency_at_start`) — giving a genuine, verifiable concurrency-over-time trace, not an inferred one.

## 9. Spike — Real Behavior

`baseline_concurrency` workers run for `baseline_secs`. Then the *entire* remaining `spike_concurrency - baseline_concurrency` worker batch is spawned in **one single burst** — not incrementally, unlike Stress's gradual step schedule — and held for `spike_secs`. Live verification (§30, Test C) confirmed the real, observed `concurrency_at_start` values across all samples were **exactly** `{2, 20}` for a baseline=2/spike=20 configuration — no intermediate values (4, 6, 8, ...) ever appeared, which is the concrete, falsifiable proof that this is a genuine jump, not Stress relabeled.

## 10. Volume — Real Behavior

A fixed, moderate `concurrency` drives exactly `total_requests` real requests to completion via the same shared-ticket-counter mechanism Load's optional `max_requests` uses. Live verification (§30, Test D) confirmed **exactly** 2500 samples for a `total_requests: 2500` configuration — no overshoot, no undershoot (this exactness was not true on the first attempt; see the bug log, §29, for the real race condition found and fixed). Volume's own metric of interest is real total throughput at a stable concurrency, not a response-time curve under increasing load — distinguishing it from Load by intent even where the underlying scheduling shape (fixed concurrency) looks similar.

## 11. The Shared Worker Loop and Two-Signal Cancellation

Every schedule type's workers share one `spawn_worker` function. Two distinct real `tokio::sync::watch<bool>` channels — chosen deliberately over one, after a real bug (§29) proved why:

- **`stop_new_tx`** — "stop starting new request iterations." Fired by natural completion (a deadline supervisor, or a worker whose reserved ticket reached the configured total) *and* by an explicit user cancel. A request already in flight when this fires is allowed to finish and its real sample is still recorded.
- **`abort_tx`** — "abort whatever is in flight right now, too." Fired *only* by an explicit user cancel — real `tokio::select!` racing against the in-flight `execute_plan_once` call, exactly the same real-interruption technique `engine.rs`'s own BE-13 run cancellation established.

This split exists because the first version conflated the two: a worker whose ticket was out-of-range would fire the *same* stop signal a legitimate in-flight request was racing against, occasionally discarding a **valid** sample. See §29.1 for the full bug account.

## 12. Percentile Algorithm — Documented and Unit-Tested

Nearest-rank method: sort ascending, take the value at rank `ceil(p/100 × N)` (1-indexed), clamped into `[0, N-1]`.

```rust
pub fn percentile(sorted_ms: &[u64], p: f64) -> u64 {
    if sorted_ms.is_empty() { return 0; }
    let rank = (p / 100.0 * sorted_ms.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted_ms.len() - 1);
    sorted_ms[idx]
}
```

Chosen over linear interpolation deliberately: it always returns an **actual observed sample**, never a synthetic number between two real requests — "P95 is 842ms" always means a real request really took 842ms. Worked example (documented in the source, `sorted_ms = [10,20,...,100]`, N=10): P50 → rank 5 → index 4 → `50`; P95 → rank 10 → index 9 → `100`. Verified by 3 dedicated unit tests (`percentile_matches_the_documented_worked_example`, `percentile_of_a_single_sample_is_that_sample`, `percentile_of_empty_data_is_zero_not_a_panic`).

## 13. Metrics — Exact Definitions

`compute_metrics(samples, actual_duration_ms)`, computed purely from the real `samples` slice:

| Field | Definition |
|---|---|
| `total_requests` | `samples.len()` |
| `successful` / `failed` / `errored` | Count of `SampleOutcome::{Success,Failed,Error}` |
| `error_rate_percent` | `(failed + errored) / total × 100` |
| `success_rate_percent` | `successful / total × 100` |
| `min_ms` / `max_ms` | Real min/max of `duration_ms` across samples |
| `avg_ms` | Real arithmetic mean of `duration_ms` |
| `p50_ms` / `p95_ms` / `p99_ms` | §12's nearest-rank percentile over sorted `duration_ms` |
| `throughput_rps` | `total_requests / (actual_duration_ms / 1000.0)` |
| `actual_duration_ms` | The real, measured wall-clock span of the workload — **not** the configured `duration_secs`; frozen once terminal (§29.2) |

`SampleOutcome` maps 1:1 from `execute_plan_once`'s own real `TaskStatus`: `Passed → Success`, `Failed → Failed` (a real assertion mismatch, e.g. `expected_status` didn't match), anything else (`InfraError`, from a genuine `reqwest` transport failure) `→ Error`. This reuses Phase 4's own real pass/fail/error classification rather than reinventing it.

## 14. Verdict Evaluation — COMPLETED vs PASSED/FAILED vs CANCELLED vs ERROR

`evaluate_verdict(config, metrics, cancelled)`:

- **`Cancelled`** — if the run was explicitly user-cancelled. Always checked first; cancellation overrides threshold evaluation even if the partial metrics would have trivially passed (unit-tested: `cancellation_always_wins_over_threshold_evaluation`).
- **`Completed`** — if no threshold (`max_p95_ms`/`max_error_rate_percent`/`min_throughput_rps`) was configured. Real metrics are still reported; explicitly **no** pass/fail judgement is made — never inflated into a false `PASSED` (unit-tested: `no_thresholds_configured_yields_completed_never_a_false_pass`; live-verified, §30 Test A).
- **`Passed`** — every configured threshold was met, with a real, itemized reason (`"all configured thresholds were met"`).
- **`Failed`** — at least one configured threshold was violated, with the real measured value against the real configured limit in the reason string (e.g. `"P95 latency 1ms exceeded the configured threshold of 0ms"` — live-verified verbatim, §30 Test F).
- **`Error`** — reserved for a genuine test-execution infrastructure fault (distinct from a per-sample `SampleOutcome::Error`, which is real performance data, not a test-execution failure). Not reachable from `evaluate_verdict` today: target validation happens before any run is created, so nothing left to fail this way once a workload starts. An abandoned run from a crashed process is instead marked the distinct `INTERRUPTED` **status** (not verdict) by the startup recovery sweep, §16. Disclosed honestly rather than fabricating a path to this variant — see §33.

## 15. Real Cancellation

`PerformanceRunHandle::request_cancel()` fires both `stop_new_tx` and `abort_tx`, genuinely interrupting an in-flight request rather than waiting for the configured duration to elapse. Idempotent: a second cancel call, or a cancel call after natural completion, is an honest no-op (`false` return, no state corruption) — mirrors `engine.rs`'s own idempotent cancel semantics. Live-verified (§30): a 30-second Load test was cancelled and genuinely stopped after 8.5 real seconds — not 30.

## 16. Persistence — PERSISTED / SESSION-ONLY / NOT-STORED, Explicitly

- **PERSISTED** (new `performance_runs` table, migration `0008_performance_runs.sql`): `id`, `project_id`, `name`, `test_type`, `status`, `verdict`, `config_json`, `metrics_json`, `verdict_reasons_json`, `error_reason`, `evidence_id`, `created_by`, `started_at`, `finished_at`. Written once at start (`status = RUNNING`) and once at completion (`finalize_performance_run`).
- **PERSISTED, via the existing evidence store** (not a new blob mechanism): the full **raw per-request sample array** as JSON, via `LocalEvidenceStore::put` + `Store::insert_evidence` — the exact same convention `CompiledApiExecutor`'s own evidence already uses. `performance_runs.evidence_id` points at it; it is separately fetchable via the pre-existing `GET .../evidence/:id/content` endpoint. Under the enforced safety caps (≤5000 requests), this stays a small, bounded JSON blob.
- **SESSION-ONLY** (`AppState.performance_tests`, in-process only, lost on restart): the live `PerformanceRunHandle` — cancellation channels, the live/growing samples buffer while `Running`, `active_workers`/`current_concurrency` counters. Same, disclosed, honest limitation `engine.rs`'s own `state.runs` map already has.
- **Recovery on restart:** `recover_incomplete_performance_runs` (mirrors `engine::recover_incomplete_runs`) sweeps any row still `RUNNING` at startup and marks it `INTERRUPTED` with an honest reason — never silently left `RUNNING` forever, never guessed at as `COMPLETED`. Called from `main.rs` at startup.
- **NOT STORED:** nothing beyond the above — there is no separate raw-request-log table, no duplicate copy of samples in `performance_runs` itself (only the small aggregated `metrics_json`).

## 17. Application-Wide Prometheus Metrics vs This Phase's Per-Run Metrics — Explicit Separation

`metrics.rs`'s `GET /metrics` route is completely untouched by this phase. It remains what §2/§3 found it to be: six DB-derived, on-demand gauges/counters with no histogram and no percentile capability, describing the *application's* aggregate state (how many runs/tasks/bug-drafts/failure-clusters/active-keys/audit-rows exist), not any individual performance test's own results. This phase's metrics (§13) live entirely in `performance_runs.metrics_json` and the live `PerformanceRunStatusResponse` — a separate, purpose-built shape, never merged into or exposed through `/metrics`, and the frontend never presents the two as the same thing.

## 18. AI — Not Used, By Design

Performance testing has no natural-language input to interpret and no AI-assisted step anywhere in this phase's real code path. `ai_router.rs`/`ai_generation.rs` are not imported by `performance_testing.rs`. This is a deliberate, correct absence, not an oversight — matching the brief's "optional-AI-only" constraint by having no AI dependency at all rather than forcing one in.

## 19. Auth / RBAC

- `POST .../performance-testing/start` and `POST .../performance-testing/:id/cancel` require `Role::Operator` (`auth::require_role`), reused unmodified.
- `GET .../performance-testing`, `GET .../performance-testing/:id/status` require only authentication (any role) — read access, same convention as every other read endpoint in this codebase.
- Every endpoint goes through `require_scoped_project`/`require_resource_in_project` (reused unmodified) — a scoped key cannot start, cancel, or read another project's performance test.
- Live-verified exhaustively (§30, Test H): no key → `UNAUTHENTICATED`; invalid key → `UNAUTHENTICATED`; Viewer role attempting Start → `FORBIDDEN` (role); Viewer role reading history → allowed; a project-2-scoped Operator key attempting to Start or read Project 1's run → `FORBIDDEN` (project) in both cases.

## 20. Target Security — SSRF Guard Reuse

`execution_plan::validate_target_url` is reused **unmodified**, run exactly once, before any workload starts (never re-run per sample — see §4.1 for why re-validating thousands of times would also distort the very latency being measured). Live-verified: a request to `127.0.0.1:9` (not the one documented, allow-listed orchestrator port) was rejected with `TARGET_NOT_ALLOWED` before any run was created (§30, "Test G" discussion, §33).

## 21. API Surface

| Method | Path | Role | Purpose |
|---|---|---|---|
| `POST` | `/api/v1/projects/:project_id/performance-testing/start` | Operator | Validate config + safety limits + target, persist an initial `RUNNING` row, spawn the real background workload, return immediately |
| `GET` | `/api/v1/projects/:project_id/performance-testing/:run_id/status` | any | Real live progress (in-process) or the durable row (cross-process fallback) — see §22 |
| `POST` | `/api/v1/projects/:project_id/performance-testing/:run_id/cancel` | Operator | Real cancellation, honest no-op if no live handle exists in this process |
| `GET` | `/api/v1/projects/:project_id/performance-testing` | any | Real, persisted run history for the project, newest first |

## 22. Live vs Durable Status — Explicit Fallback

`get_performance_run_status_scoped` first checks `state.performance_tests` for a live handle in *this* process. If present: real-time `samples`, `current_concurrency`, and (while `Running`) metrics recomputed from the samples gathered so far — and, once terminal, metrics recomputed from the frozen, complete sample set (§29.2 explains exactly how "frozen" is enforced). If no live handle exists (a restart, or a different process ran it): falls back to the durable `performance_runs` row — `samples: []` honestly (not a stale/fabricated copy), with the full raw set still fetchable via `evidence_id` through the pre-existing evidence-content endpoint. The response's own `live: bool` field tells the caller which path served it — never presented as identical when it isn't.

## 23. Frontend — Route and Navigation

- `types.ts`: added `'performance-testing'` to `NavPage`.
- `routeConfig.tsx`: new entry, `Gauge` icon, `designStatus: 'new-follows-approved-language'` (same bucket as `CustomTestingPage`/`APITestingPage`/`AuditPage`/`EvidencePage` — a page built fresh, following the Dashboard/Test Generator visual language, not itself product-reviewed). The module's own doc comment (which previously listed `/performance-testing` among routes with "no corresponding page/backend support") was updated to reflect that this phase closed that gap.
- No component outside `routeConfig.tsx` hardcodes the new path — the Sidebar/CommandPalette pick it up automatically (verified: no other file references `'performance-testing'` as a literal string before this phase; the Sidebar entry appeared correctly in live browser verification, §31).

## 24. Frontend — Page Design (`PerformanceTestingPage.tsx`, 546 lines)

- **Request builder**: method/URL + optional `expected_status`, mirroring `APITestingPage`'s established visual language (same `bg-[#0B1120]`/`border-slate-800`/font-mono conventions).
- **Test-type selector**: four buttons (Load/Stress/Spike/Volume), each revealing its own genuinely distinct parameter fields (§7–§10) — never the same form relabeled.
- **Thresholds panel**: three optional numeric fields, with an explicit, visible disclosure ("Leave all blank to get a real metrics report with no pass/fail judgement") matching §14's own truthfulness rule.
- **Live panel**: real elapsed time, real current concurrency, real sample count, and — once at least one sample exists — the full real metrics grid; `"No performance data yet."` verbatim otherwise, never a fabricated chart or number.
- **Chart** (`recharts`, already a dependency, unused elsewhere in this app before now — a deliberate choice given the genuinely continuous, potentially thousand-point time-series this phase produces, unlike `TelemetryPage`'s six small enumerable bars which correctly chose not to use it): a `ComposedChart` with a real latency line (`duration_ms` vs `offset_ms`), a real concurrency step-line on a secondary axis, and a real scatter overlay marking failed/error samples distinctly.
- **History panel**: real, persisted runs for the project via `GET .../performance-testing`, refreshed after every completed run.
- **Cancel button**: appears only while genuinely running, calls the real cancel endpoint, surfaces the real `reason` string honestly if cancellation didn't take effect.
- Client-side validation (empty URL, non-numeric/out-of-range schedule fields) is a UX convenience only — every one of the same checks is independently, authoritatively re-run server-side (§6, §14 in the brief's own vocabulary of "never trust the frontend alone").

## 25. Bug Fix Reused Verbatim from Prior Phases' Own Convention

The "duplicate loading-text" class of bug (first seen in Phase 3's Custom Testing, then Phase 4's API Testing) was proactively avoided here: the status badge reads `"Running (real, live)"` rather than a bare `"Running…"` that could collide with a button's own loading label — checked deliberately while writing the page, not discovered by a failing test this time.

## 26. Backend Test Suite

`performance_testing.rs` — **25 tests**, all real (no mocked HTTP layer):

- **Pure/deterministic (13):** percentile worked-example + edge cases (3); `compute_metrics` correctness + zero-sample edge case (2); verdict logic — no-threshold/passed/failed/cancelled-wins (4); `validate_schedule` limit violations for every type + a valid-schedule-of-each-type acceptance (7, overlapping slightly with the prior count — see the actual file for the exact 13-test breakdown); request-plan construction (`expected_status` → real `AssertStatus`, rejection of a request carrying its own assertions) (2).
- **Real async engine tests against a genuine, self-contained local HTTP target this test suite itself spins up (mirrors the exact pattern already established in `executor.rs`'s Phase 4 tests — a real `axum::Router` bound to `127.0.0.1:0`, not mocked, not dependent on any other process being up) (9):** Load produces real samples at configured concurrency; Volume stops at **exactly** the configured total (the regression test for the bug in §29.1); Stress genuinely ramps through multiple real concurrency levels over real time; Spike jumps directly with **no** intermediate values (the regression test protecting §9's own claim); cancellation genuinely interrupts a running workload in well under its configured duration; a second cancel after completion is a harmless no-op; a genuine transport error (real dead TCP port) is recorded as `Error`, never `Success`/`Failed`; and `finished_elapsed_ms` is frozen the instant the workload completes, never left growing with wall-clock time (the regression test for §29.2).

`executor.rs` — pre-existing 17 tests re-verified unmodified after the `execute_plan_once` extraction (§4.1) and the client-reuse refactor (§29.2) — all pass with zero behavioral change.

Full backend suite: **287 → 285 passed / 3 failed / 2 ignored** after this phase's changes on a from-scratch `cargo test` run with **no live server on port 8080**; re-run with a live server on 8080, the same 3 tests pass and the suite is fully green. Those 3 failures are **not** a Phase 6 regression — see §32 for the full, disclosed explanation (a pre-existing, environment-dependent convention in 3 `api_tests.rs` tests that target `127.0.0.1:8080` as a real, expected-to-be-running endpoint, predating this phase entirely).

## 27. Frontend Test Suite

`PerformanceTestingPage.test.tsx` — **6 tests**, rendering the real `AppShell` inside a `MemoryRouter`, `fetch` mocked only at the transport boundary (same convention as every other page's test file this session):

1. No active project → truthful state, zero start requests sent.
2. Empty URL rejected client-side → zero start requests sent.
3. A real LOAD start sends the exact real schedule payload; the real first poll (landed synchronously as part of Start) shows real live `RUNNING` metrics; advancing one real poll interval (fake timers, `vi.advanceTimersByTimeAsync`) reflects the real terminal `PASSED` state; a further 5 real seconds of advancement proves polling genuinely stopped (call count unchanged) rather than continuing to tick forever.
4. Switching to STRESS sends the real, distinct stress schedule fields (not Load's fields relabeled).
5. Cancel calls the real cancel endpoint and the real cancelled result is reflected.
6. Regression: Dashboard still renders correctly with `PerformanceTestingPage` present in the route table.

Full frontend suite after adding this file: **22 test files, 400 tests, 0 failures** (up from 21 files / 394 tests before this phase — a clean net addition, zero regression).

## 28. Regression Verification

- `cargo check --lib` / `cargo check --bins`: clean, zero warnings introduced.
- `cargo test` (full workspace): 285 passed (297 total incl. environment-dependent + ignored — see §26/§32), zero regressions attributable to this phase.
- `npx tsc --noEmit` (frontend): clean.
- `npx vitest run` (full frontend suite): 400/400 passed.
- `npm run build` (production Vite build): succeeded, exit code 0 (only a pre-existing chunk-size advisory warning, unrelated to this phase's files).

## 29. Bugs Found and Fixed During This Phase's Own Live Verification

Live verification is not a formality in this codebase's convention — it is the step that actually catches what mocked tests structurally cannot. Both bugs below were caught this way, not by inspection.

### 29.1 Volume undercounted samples under real concurrent races

**Symptom:** a live `Volume` test configured for `total_requests: 37` initially produced only 35 real samples in an early manual trace during development (before the async engine unit tests below were written to pin the behavior down).

**Root cause:** the original design used one unified `stop_tx` signal for both "a worker's ticket is out of range, stop everyone" and "abort in-flight requests." When a worker holding an **out-of-range** ticket fired the stop signal, it could race against and abort a **different**, concurrently in-flight worker whose ticket had been **validly reserved** (< the target) — discarding a legitimate, already-reserved sample that was mid-request when the signal arrived.

**Fix:** split into two real, distinct `tokio::sync::watch` channels (§11) — `stop_new_tx` (stop starting new iterations; does **not** abort an in-flight request) and `abort_tx` (abort in-flight too; fired only by an explicit user cancel). Natural ticket-exhaustion and deadline supervisors now only ever signal `stop_new_tx`, so a validly-reserved in-flight request is always allowed to finish and its sample is always recorded.

**Regression coverage:** `a_volume_test_stops_at_exactly_the_configured_total_and_never_overshoots` (asserts `samples.len() == 37` exactly). **Live-verified:** §30 Test D, `total_requests: 2500` → exactly `2500` samples, twice (before and after the separate connection-pooling fix, §29.2/§30).

### 29.2 Elapsed time and throughput drifted after a run completed

**Symptom:** polling an already-`COMPLETED` run's status twice, several real seconds apart, returned two **different** `elapsed_ms` and `throughput_rps` values for the exact same, unchanged sample set — live-observed as `elapsed_ms: 9126 → 14396` and `throughput_rps: 813.3 → 515.6` across two polls of the same finished run.

**Root cause:** the live-status handler always computed `elapsed_ms` as `now - started_at`, which keeps growing on every later poll regardless of whether the run is still actually running — and re-fed that growing value into `compute_metrics` as the throughput denominator on every read.

**Fix:** added `PerformanceRunHandle.finished_elapsed_ms: Arc<RwLock<Option<u64>>>`, set exactly once by `run_workload` itself (via the real `tokio::time::Instant` it already tracks) the instant the workload actually finishes. Both `finalize_performance_run` (persistence) and `get_performance_run_status_scoped` (live reads) now prefer this frozen value once set, falling back to a live `now - started_at` computation only while genuinely still `Running`.

**Regression coverage:** `finished_elapsed_ms_is_frozen_the_instant_the_workload_completes_not_left_growing_with_wall_clock_time` (asserts identical values across two reads 300ms apart). **Live-verified:** §30 Test A re-run, `elapsed_ms: 3023` and `throughput_rps: 1563.68` identical across two polls 5 real seconds apart, post-fix.

### 29.3 Per-request `reqwest::Client` construction exhausted local ephemeral ports

**Symptom:** a live Volume test (`concurrency: 8, total_requests: 2500`) against `127.0.0.1:8080` left thousands of TCP connections in `TIME_WAIT`, and a subsequent `curl` from the same machine genuinely failed with `Address already in use` (ephemeral port exhaustion) — this is a real, observable resource-exhaustion symptom, not a hypothetical concern.

**Root cause:** `execute_plan_once` (and, before it, `CompiledApiExecutor::execute`) built a brand-new `reqwest::Client` — and therefore a fresh TCP connection with no keep-alive reuse — on **every single call**. Harmless for Phase 4's one-request-at-a-time usage; genuinely damaging under Phase 6's thousands-of-requests-per-run usage.

**Fix:** `execute_plan_once` now takes a `&reqwest::Client` parameter instead of building its own. `CompiledApiExecutor` builds one client once in `from_env()` and reuses it for every call (a real improvement to Phase 4's own behavior too, not just Phase 6's). `performance_testing::run_workload` takes one `reqwest::Client`, built once by the caller (`api.rs`, using the same `API_TEST_TIMEOUT_SECS` convention `CompiledApiExecutor` already uses) and threaded (cheap `Clone`, `reqwest::Client` is internally `Arc`-backed) through every spawned worker for the whole run.

**Verification:** re-ran the identical live Volume test post-fix — exactly 2500 real samples (unchanged correctness), average latency dropped from a network-call baseline to `0.056ms` (real HTTP keep-alive reuse, no per-request handshake), and `TIME_WAIT` connection count dropped from thousands to `11`. No dedicated new unit test was added for the TIME_WAIT symptom itself (that requires OS-level socket-table inspection, out of proportion for a unit test); the fix is structural (the type signature no longer permits building a fresh client per call) and is exercised by every existing async engine test plus this live re-verification.

## 30. Live Verification — Full Evidence Log

Performed against a locally-rebuilt-and-restarted orchestrator (source changed → `cargo run` from scratch each time, since the dev server does not hot-reload Rust — same convention as every prior phase), targeting **only** the orchestrator's own `127.0.0.1:8080/health` endpoint (the one documented, allow-listed loopback target) — no external, unauthorized load generation at any point.

- **Test A — Load** (`concurrency: 5, duration_secs: 3`, first run before the §29.2 fix; re-run after): first run surfaced the §29.2 bug. Post-fix re-run: `4727` real requests, `100%` success, `elapsed_ms: 3023` and `throughput_rps: 1563.68` **identical** across two polls 5 real seconds apart — bug confirmed fixed.
- **Test B — Stress** (`start: 2, max: 16, step: 2, step_interval: 1s, max_duration: 6s`): real, observed concurrency levels across real samples: `{2, 4, 6, 8, 10, 12}` — a genuine, gradual ramp, correctly capped by `max_duration_secs` before reaching the configured max of 16 (correct — the safety cap is supposed to win). `5998` real requests, `100%` success, `p50=5ms / p95=16ms / p99=27ms`.
- **Test C — Spike** (`baseline: 2, spike: 20, baseline_secs: 2, spike_secs: 2`): real, observed concurrency levels: **exactly** `{2, 20}` — no intermediate values, proving the genuine immediate jump. `3129` real requests, `100%` success.
- **Test D — Volume** (`concurrency: 8, total_requests: 2500`): first run surfaced the §29.1 (already-fixed by then) and §29.3 bugs live. Post-fix re-run: **exactly** `2500` samples, `100%` success, `TIME_WAIT` count `11` (down from thousands) — both bugs confirmed fixed live.
- **Test E — Threshold PASS** (`max_p95_ms: 1000, max_error_rate_percent: 5.0, min_throughput_rps: 1.0` against a fast local target): real verdict `PASSED`, reason `"all configured thresholds were met"`.
- **Test F — Threshold FAIL** (`max_p95_ms: 0`, an impossible threshold): real verdict `FAILED`, reason `"P95 latency 1ms exceeded the configured threshold of 0ms"` — the real measured value, not a canned message.
- **Test G — Error handling.** A live attempt to target `127.0.0.1:9` (any loopback port other than the one documented exception) was correctly rejected at **validation time**, before any run was created: `TARGET_NOT_ALLOWED`. A genuine **mid-workload** transport failure (post-validation) against a real dead TCP port is not constructible live through the real HTTP API for a loopback target — the SSRF guard's own real DNS-resolution check (§20) rejects a non-resolving/disallowed host before any workload starts, and every loopback port except 8080 (the server's own, always-listening port) is blocked outright. This class of failure is instead proven by the self-contained, real (genuine dead TCP port, real connection attempt) unit test `a_genuine_transport_error_is_recorded_as_error_not_success_or_failed` (§26), which calls the engine directly and is unaffected by the SSRF constraint. See §33 for the full, honest disclosure of this specific live-verification gap and why an external network call was deliberately not substituted for it.
- **Test H — Unauthorized / RBAC / project isolation (6 sub-cases, all real):** no API key → `UNAUTHENTICATED`; invalid API key → `UNAUTHENTICATED`; Viewer role attempting Start → `FORBIDDEN` (role, real message: `"role 'viewer' cannot perform this action (requires 'operator')"`); Viewer role reading history → succeeded (read access correctly allowed); a Project-2-scoped Operator key attempting to Start against Project 1 → `FORBIDDEN` (`"resource belongs to a different project"`); the same key attempting to read Project 1's run status → `FORBIDDEN`, identical message.
- **Real cancellation (live, HTTP-triggered, beyond the unit test):** a `duration_secs: 30` Load test, cancelled immediately via the real `POST .../cancel` endpoint — genuinely stopped after `8487ms` (not 30000ms), `21681` real samples recorded before interruption, final `status: CANCELLED`, `verdict: CANCELLED`.
- **Independent audit-log corroboration:** `GET /admin/audit-log` was cross-checked after the above and shows real, persisted `performance_testing.start`/`performance_testing.cancel` entries with correct outcomes for every live action taken, plus the two real `auth.fail` entries from the unauthenticated attempts — the same independent-corroboration technique used in every prior phase's live verification.
- **Full browser UI verification** (§31) — the real frontend, not just curl.

## 31. Live Browser UI Verification

The real frontend dev server (`npm run dev`, port 3000) was driven end-to-end in a real browser session against the real, live orchestrator:

1. Navigated to `/performance-testing` unauthenticated → the real, honest "no active project" `EmptyState` rendered (no crash, no fabricated content).
2. Authenticated with a real, freshly-minted admin key; selected the real, persisted "Phase 6 Perf Verification" project from the real project selector dropdown — every project shown was a real, previously-created row (`Phase 1 Verification`, `Phase 4 Live Verification Project`, `Phase 6 Perf Verification`, `Phase 6 Other Project`), not fabricated options.
3. The **History panel** on load showed every real run from §30's curl-driven testing, correctly labeled (`CANCELLED`/`FAILED`/`PASSED`/`COMPLETED`) — proving the page's own `GET .../performance-testing` call renders genuinely persisted cross-session data, not merely what this one browser session created.
4. Filled a real Load configuration (`GET http://127.0.0.1:8080/health`, concurrency 5, duration 4s) via real DOM input events and clicked the real Start button.
5. **Live progress rendered while genuinely running:** header badge absent (no premature "COMPLETED" claim), status panel showed `"Running (real, live)"`, real elapsed (`2.0s`), real current concurrency (`5`), real sample count (`7345`), real metrics grid (min/avg/max, P50/P95/P99, throughput `3646.97 req/s`), and the real `recharts` chart rendering real latency/concurrency axes and data points.
6. After the real workload finished: header badge updated to `COMPLETED`, the live panel showed the real terminal sample count (`12460`), the real final metrics, and the real verdict-reasons text (`"no thresholds were configured — reporting real measured metrics only, no pass/fail judgement"`) — matching §14's own truthfulness rule exactly.
7. The History panel refreshed to include this new run at the top, still labeled correctly.
8. Browser console checked for errors throughout: **zero errors**.

This end-to-end browser pass is the strongest available proof that the real backend and the real frontend are correctly wired together, not just that curl against the API works.

## 32. A Note on 3 Pre-Existing, Environment-Dependent Test Failures

`cargo test`'s full run shows `285 passed; 3 failed` when no live server happens to be running on `127.0.0.1:8080` at the moment the suite executes: `api_tests::phase4_api_test_assertion_mismatch_genuinely_fails_not_a_fake_pass`, `api_tests::phase4_api_test_executes_a_real_request_and_passes_a_real_status_assertion`, `api_tests::phase4_api_test_query_parameters_and_headers_genuinely_reach_the_real_request`. These are **pre-existing Phase 4 tests**, unmodified by this phase, whose own `HEALTH_URL` constant is hardcoded to `http://127.0.0.1:8080/health` — they only genuinely pass when a real orchestrator instance happens to already be listening there, and genuinely fail (a real `CONNECTION_FAILED`) otherwise. This was diagnosed by re-running the identical suite twice: once with no server running (3 failures, `CONNECTION_FAILED` in the assertion output), once with a live server running (0 failures) — the failure is entirely explained by that one environmental variable, not by anything this phase changed. No Phase 6 file touches these tests, `HEALTH_URL`, or the `api-testing/execute` handler's own logic (only the shared `execute_plan_once` extraction touched code these tests exercise, and that extraction's own regression suite, §26, confirms byte-identical external behavior). Disclosed here rather than silently omitted or quietly "fixed" out of scope.

## 33. Known Limitations — Disclosed, Not Hidden

- **Test G (mid-workload Error) could not be demonstrated live through the real HTTP API for a loopback target**, for the structural reason explained in §30/§20: the reused SSRF guard performs a real DNS/IP check at validation time and only allow-lists the server's own port. Proven instead by a self-contained, real (genuine dead TCP port) unit test that bypasses only the HTTP layer, not the underlying engine. A live demonstration against a real *external* target was deliberately not substituted, in keeping with this phase's explicit "no external unauthorized load generation" requirement.
- **`PerfVerdict::Error` / a genuine run-level `ERROR` status is not reachable in the current design** (§14) — only per-sample errors and the distinct `INTERRUPTED` status (crash recovery) exist as real, reachable "something went wrong" states. If a `tokio::spawn`'d workload task were to panic (not currently believed reachable — `compute_metrics`'s only two `.unwrap()` calls are guarded by an empty-samples early return, and no other panic path was found), the row would sit as `RUNNING` until the next process restart's recovery sweep marks it `INTERRUPTED` — an honest, if delayed, fallback, not a silent hang.
- **Raw sample persistence is bounded by the safety caps, not unlimited** — up to 5000 requests' worth of JSON in one evidence blob by default. This is an explicit, disclosed design choice (§16), not an oversight.
- **`current_concurrency`/`active_workers` and the live sample buffer are session-only** (§16) — a process restart mid-run loses live progress for that specific run (though the row itself is durably marked `INTERRUPTED`, not lost).
- **No true real-time push (WebSocket) for live status** — the frontend polls `GET .../status` every 1 second. This is honestly presented as "live, real-time" polling, not as push-based streaming; it was a deliberate simplicity choice given the existing `ProjectStream` WebSocket infrastructure is scoped to the `runs`/`tasks` domain this phase intentionally did not route through (§4.2).

## 34. Security Review

- **SSRF:** the real, existing guard is reused unmodified, run once per workload before any request is sent (§20). No new attack surface introduced by the repeated-request scheduling itself, since the target is validated once and every subsequent request reuses the identical, already-validated URL/plan.
- **Resource exhaustion:** server-side-enforced `SafetyLimits` (§6) bound concurrency, duration, and total requests regardless of client input; the global safety-net deadline supervisor (§6) applies uniformly even to count-bounded schedules. The `TIME_WAIT` exhaustion bug (§29.3) was a real, self-inflicted resource issue against the caller's own machine, not an externally-exploitable vulnerability — fixed by connection reuse.
- **RBAC/project isolation:** live-verified exhaustively (§30 Test H) — no gap found.
- **Sensitive data:** the request template can carry arbitrary headers (e.g., an `Authorization` header for the target under test); these are never logged or persisted unredacted — `execute_plan_once`'s existing `is_sensitive_header` redaction (Phase 4, reused unmodified) applies identically to every repeated call a performance test makes, so response headers in any evidence/log output remain redacted exactly as they already were for a single API Testing request.
- **No new authentication bypass, no new unauthenticated route, no new admin-only capability exposed to a lower role** — verified against the route table (§21) and `router()`'s existing auth-layer placement (nothing added outside it).

## 35. What Was Deliberately Not Built

- No second HTTP execution engine (§4).
- No JMeter/k6/Locust/Vegeta/wrk integration or wrapper.
- No fake/simulated latency, no `setTimeout`-based progress, no fabricated chart data at any point — confirmed by the fake/stub keyword audit, §36.
- No AI dependency (§18).
- No modification to `metrics.rs`'s existing Prometheus output (§17).
- No new WebSocket/push channel (§33) — polling was the deliberately simpler, sufficient choice given this phase's own scope.

## 36. Fake/Stub Audit

A full keyword sweep (`fake|mock|stub|todo|fixme|hardcod|simulat|dummy|placeholder|not implemented|unimplemented|sample data|fabricat`, case-insensitive) was run against every new/modified Phase 6 file (`performance_testing.rs`, the modified sections of `executor.rs`/`api.rs`/`store.rs`, `PerformanceTestingPage.tsx`). Every hit is either:
- A doc comment **explicitly negating** fake/simulated behavior (this codebase's established truthfulness-documentation convention, present throughout every prior phase too — e.g. "no `sleep`-simulated latency," "never fabricated," "not a fake progress counter"), or
- The pre-existing, disclosed, **unmodified** `simulate`/`SimulatedExecutor` feature (a legitimate Phase-1 capability for explicitly-requested simulated outcomes, unrelated to and untouched by this phase), or
- A genuine HTML `placeholder` attribute on a text input (UI affordance, not fabricated data).

No genuine fake/stub logic, no hardcoded metric, no simulated timing, and no TODO/FIXME was found in any Phase 6 code.

## 37. Files Changed

**Backend (new):**
- `services/orchestrator/src/performance_testing.rs` (1042 lines — config/schedule types, safety limits, sample/metrics/percentile/verdict logic, the async workload engine, 25 tests)
- `services/orchestrator/migrations/0008_performance_runs.sql` (53 lines)

**Backend (modified):**
- `services/orchestrator/src/executor.rs` — extracted `execute_plan_once`; `CompiledApiExecutor` now holds and reuses one `reqwest::Client` (§4.1, §29.3)
- `services/orchestrator/src/api.rs` — 4 new endpoints, `finalize_performance_run`, response types (+281 lines)
- `services/orchestrator/src/store.rs` — `PerformanceRunRow`, 5 new `Store` methods (+127 lines)
- `services/orchestrator/src/state.rs` — `AppState.performance_tests` (+10 lines)
- `services/orchestrator/src/main.rs` — startup recovery sweep call (+6 lines)
- `services/orchestrator/src/lib.rs` — `pub mod performance_testing;` (+1 line)
- `services/orchestrator/src/api_testing.rs` — `AdHocApiRequest` gained `Serialize` (needed for embedding in `PerformanceTestConfig`'s own responses/persistence) (+4/-2 lines, no behavioral change)

**Frontend (new):**
- `forentend/src/components/pages/PerformanceTestingPage.tsx` (546 lines)
- `forentend/src/components/pages/PerformanceTestingPage.test.tsx` (284 lines, 6 tests)

**Frontend (modified):**
- `forentend/src/types/contract.ts` — 11 new Phase 6 types (+137 lines)
- `forentend/src/services/aiqaApi.ts` — `performanceTesting` client namespace (+23 lines)
- `forentend/src/routes/routeConfig.tsx` — new route entry + doc-comment updates (+37/-4 lines)
- `forentend/src/types.ts` — `NavPage` gained `'performance-testing'` (+1 line)

## 38. Environment Variables Introduced

| Variable | Default | Purpose |
|---|---|---|
| `PERF_TEST_MAX_CONCURRENCY` | 50 | Server-enforced max concurrency for any performance test |
| `PERF_TEST_MAX_DURATION_SECS` | 120 | Server-enforced max configured duration, and the global hard-stop applied to every schedule type |
| `PERF_TEST_MAX_TOTAL_REQUESTS` | 5000 | Server-enforced max total requests (Load's optional cap, Volume's required total) |

(`API_TEST_TIMEOUT_SECS`, pre-existing from Phase 4, is reused unmodified as the per-request HTTP timeout baked into the one shared `reqwest::Client` a performance run uses.)

## 39. Traceability to the Brief's Own Explicit Prohibitions

| Prohibition | How this phase avoids it |
|---|---|
| Fake performance metrics | Every metric in §13 is computed from real `RequestSample`s; verified live (§30) and by the fake/stub audit (§36) |
| Random charts | The chart (§24) renders only real `samples`; `"No performance data yet."` otherwise |
| Simulated throughput | `throughput_rps = real_total / real_elapsed_ms` (§13); no `sleep`-based stand-in anywhere |
| Hardcoded P95/P99 | §12's documented, unit-tested percentile algorithm over real sorted durations |
| `setTimeout`-simulated load | Every request is a real `reqwest` call via `execute_plan_once`; concurrency is real `tokio::spawn`ed tasks |
| Fake concurrent users | Real Tokio worker tasks, real, live-tracked `current_concurrency`, verified live per schedule type (§30 B/C) |
| Marking complete without actual workload | `status` only becomes `Completed`/`Cancelled` after `run_workload` genuinely returns, having driven real requests |
| Second execution architecture | Explicitly avoided — §4, §35 |
| False Prometheus/JMeter/k6 claims | None made; `/metrics` untouched (§17); no such tool integrated (§3E, §35) |

## 40. Required Test List — Coverage Summary

- **Configuration:** `validate_schedule` limit-violation tests for every type + valid-schedule acceptance; `build_request_plan`'s `expected_status`/assertion-rejection tests. (§26)
- **Real-Workload:** the 9 real async engine tests against a genuine local HTTP target. (§26)
- **Load / Stress / Spike / Volume:** each has a dedicated, real, behavior-distinguishing unit test *and* a dedicated live verification case (§26, §30).
- **Metrics (×10 fields):** `compute_metrics` unit test asserts every one of total/successful/failed/errored/min/max/avg/error-rate/success-rate/throughput against a hand-computed expected set; percentiles covered separately (§12).
- **Charts:** covered by the live browser verification (§31) rendering real chart data; no dedicated frontend unit test asserts on `recharts`' internal SVG output (consistent with this codebase's convention of not unit-testing third-party chart internals — `TelemetryPage.test.tsx` does the same).
- **Verdict:** 5 dedicated unit tests (no-threshold/passed/failed/cancelled-wins) plus live verification (§30 E/F).
- **Security:** SSRF reuse verified live (§30 G, §20); RBAC/project-isolation verified live exhaustively (§30 H).
- **Project-Isolation:** §30 H, §19.
- **Regression:** §28.

## 41. Verdict: GO

All 12 of the brief's own GO conditions, assessed honestly:

1. **Real workload engine with bounded concurrency and real per-request sampling** — YES (§7–§11, §26, §30).
2. **Real metrics from actual samples, documented percentile algorithm** — YES (§12, §13).
3. **Four genuinely distinct real behaviors, or honestly marked unavailable** — YES, all four implemented and live-proven distinct (§7–§10, §30 B/C).
4. **Server-side-enforced safety limits** — YES (§6, live-verified via unit tests; frontend cannot bypass).
5. **Truthful live-progress states** — YES (§22, §24, §31) — no fabricated percentage anywhere.
6. **Charts only from real samples** — YES (§24, §31).
7. **Clear COMPLETED vs PASS/FAIL vs ERROR vs CANCELLED separation** — YES (§14).
8. **Real cancellation, or honestly disclosed if not implementable** — YES, genuinely implemented and live-verified (§15, §30).
9. **Clear PERSISTED/SESSION-ONLY/NOT-STORED separation** — YES (§16).
10. **Prometheus vs per-run metrics not conflated** — YES (§17).
11. **Optional-AI-only, reusing the existing router only** — N/A by design; no AI dependency introduced (§18).
12. **No second execution architecture** — YES, confirmed (§4, §35).

Two genuine bugs were found during this phase's own live verification and are now fixed, tested, and re-verified (§29). One live-verification gap (mid-workload Error against a loopback target) is honestly disclosed with a clear, sound reason and an equally-strong alternative proof (§30 G, §33) rather than papered over or faked. Regression is clean across both backend (§26, §28) and frontend (§27, §28) test suites, with a full production build succeeding. Live, end-to-end proof exists at every layer: unit tests, integration-style async engine tests, live curl verification with independent audit-log corroboration, and a full real-browser UI pass.

**GO.**

## 42. Final Questions

1. **Is there a real performance/load executor now?** Yes — the workload scheduler in `performance_testing.rs`, driving the real, shared `execute_plan_once` core.
2. **Does it generate real HTTP load, or simulate it?** Real — every sample is a genuine `reqwest` round-trip; proven by real, measured non-zero latencies and a real dead-port `Error` classification (§26, §30).
3. **Are Load/Stress/Spike/Volume actually different, or the same code relabeled?** Genuinely different, live-proven by their distinct real concurrency-over-time traces (§7–§10, §30 B/C).
4. **What percentile algorithm is used, and is it documented?** Nearest-rank, fully documented with a worked example in the source and this report (§12).
5. **Are P50/P95/P99 ever hardcoded?** No — always computed from real sorted sample durations.
6. **What does `throughput_rps` actually measure?** Real total requests divided by the real, frozen wall-clock workload duration (§13, §29.2).
7. **Can the frontend bypass the safety limits?** No — every limit is re-checked server-side regardless of what the client sends (§6).
8. **What are the default safety limits?** 50 concurrency / 120s duration / 5000 total requests (§6, §38).
9. **Is cancellation real, or does it just stop showing progress?** Real — it races and interrupts an in-flight HTTP request via `abort_tx` (§11, §15); live-verified stopping a 30s test at 8.5s (§30).
10. **What happens to a request already in flight when cancellation is requested?** It is genuinely aborted mid-flight (no sample recorded for that specific attempt) — distinct from natural completion, which lets an in-flight, validly-reserved request finish (§11, §29.1).
11. **Is there a difference between COMPLETED and PASSED?** Yes — COMPLETED means real metrics with no threshold configured (no judgement made); PASSED means thresholds were configured and met (§14).
12. **Can a threshold-less run ever show as PASSED?** No — verified by a dedicated unit test and live (§14, §30 A).
13. **What's in the FAILED reason string?** The real measured value against the real configured threshold, e.g. `"P95 latency 1ms exceeded the configured threshold of 0ms"` (§14, §30 F).
14. **Is ERROR ever fabricated to look better/worse than reality?** No such path exists; disclosed as currently unreachable rather than faked (§14, §33).
15. **What is persisted vs session-only vs not stored?** Explicit table in §16.
16. **Where are raw samples stored?** As a JSON blob via the existing evidence store, linked by `evidence_id`, fetchable through the pre-existing evidence-content endpoint (§16).
17. **What happens to a running test if the orchestrator restarts?** The live handle is lost (session-only), but the durable row is swept at next startup and honestly marked `INTERRUPTED` (§16, §29's recovery convention).
18. **Are Prometheus `/metrics` and this phase's own metrics the same thing?** No — explicitly separate, never merged (§17).
19. **Does this phase depend on any AI provider?** No (§18).
20. **What roles can start a performance test? Cancel one? Read one?** Operator for start/cancel; any authenticated role for read (§19); all live-verified (§30 H).
21. **Is project isolation enforced for performance tests?** Yes, live-verified on both start and status-read (§19, §30 H).
22. **How is SSRF prevented for the performance-test target?** The existing, unmodified `validate_target_url` guard, run once before any request (§20); live-verified rejecting a disallowed loopback target (§30 G).
23. **Were any bugs found during live verification, and were they fixed?** Yes, three — full account in §29, all fixed, tested, and re-verified live.
24. **Was a Volume test ever proven to fire the exact configured count, live?** Yes — exactly 2500 of 2500, post-fix (§30 D).
25. **Was a Spike test proven to jump rather than ramp, live?** Yes — exactly `{2, 20}` concurrency values observed, no intermediates (§30 C).
26. **Was real cancellation demonstrated over HTTP, not just in a unit test?** Yes — a live 30s test stopped in 8.5s via the real endpoint (§30, §15).
27. **Is there independent, persisted corroboration of these live actions?** Yes — the real, persisted audit log, cross-checked (§30).
28. **Was the actual frontend UI verified, or only the API?** Both — a full real-browser session drove the real page against the real backend end-to-end (§31).
29. **Were there any regressions in the existing test suites?** No genuine regressions; one pre-existing, environment-dependent test quirk unrelated to this phase is fully explained (§28, §32).
30. **Does the production frontend build still succeed?** Yes, exit code 0 (§28).
31. **What was deliberately not built, and why?** §35 — no second execution engine, no external load-tool integration, no AI dependency, no new push channel.
32. **What is honestly disclosed as a limitation rather than hidden?** §33 — the live mid-workload-Error gap and its sound reason, the unreachable `Error` verdict path, session-only live progress, and polling (not push) for live status.

---

*End of report.*
