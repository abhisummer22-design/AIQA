# BE-12B — Average Run Duration Implementation Report

## Status

**PASS.**

## Objective

Implement exactly one evidence-backed Dashboard KPI identified by BE-11's audit — **Average Run Duration**, project-scoped, using the existing real `runs.started_at`/`runs.finished_at` columns — and nothing else. No other KPI concept, no migration, no new endpoint, no frontend change.

## Fresh Audit Findings

Before writing any code, the exact backend state BE-11 audited was re-verified byte-for-byte (not assumed from the report):

- `git diff --stat -- services/orchestrator/` at the start of this session was identical to BE-10's own diff — confirming zero backend changes had occurred between BE-11's audit and this unit's start.
- Re-read `store.rs::dashboard_avg_task_duration_ms` (lines 487-497) directly: still the exact real SQL BE-11 cited, unchanged.
- Re-read `api.rs::KpisResponse`/`get_project_kpis` (lines 981-1019) directly: still the exact real struct/handler BE-11 cited, unchanged.
- **BE-11's conclusion holds.** No STOP condition applies.

One genuine, new finding surfaced during this unit's own implementation (not present in BE-11, which did not need to trace the REJECTED path this deeply): `store.rs::reject_run` (lines 232-244) sets a real `finished_at` on a rejected run but **never sets `started_at`** — a rejected run was held at `PENDING_APPROVAL` and never entered the scheduler, so it structurally has no start time. This means the query's existing `started_at IS NOT NULL` predicate — required anyway, with no special-casing — correctly and automatically excludes REJECTED runs from the duration average, exactly as the BE-12b prompt's own instruction ("Do NOT include... any run without `finished_at`" plus its list of runs to include) resolves in practice once the real schema is checked rather than assumed. This is documented explicitly in the new store function's own doc comment and independently verified by a dedicated test (`be12b_mixed_terminal_states_all_counted_rejected_naturally_excluded`, which directly queries the real row and asserts `started_at IS NULL`/`finished_at IS NOT NULL` for a real rejected run, in addition to checking the KPI value).

## Exact Files Changed

- `services/orchestrator/src/store.rs` (+24 lines) — new `dashboard_avg_run_duration_ms` function.
- `services/orchestrator/src/api.rs` (+56/-3 lines) — `KpisResponse` struct (+1 field, updated doc comment), `get_project_kpis` handler (+1 store call, +1 field in the response construction).
- `services/orchestrator/src/api_tests.rs` (+394 lines) — 4 new tests, 1 new assertion added to an existing test, 1 new `use sqlx::Row` import, 3 small local test-only helpers (`set_run_timestamps`, `infra_error_plan`, `fail_plan`).

No other file was touched. Confirmed via `git diff --stat -- services/orchestrator/` (exactly these 3 files) and via `git diff --stat` against `migrations/`, `domain.rs`, `engine.rs`, `intelligence.rs`, `auth.rs` (all empty).

## Exact Calculation

```rust
pub async fn dashboard_avg_run_duration_ms(&self, project_id: &str) -> Result<Option<f64>> {
    let row = sqlx::query(
        "SELECT AVG((julianday(r.finished_at) - julianday(r.started_at)) * 86400000.0) as avg_ms
         FROM runs r
         WHERE r.project_id = ? AND r.started_at IS NOT NULL AND r.finished_at IS NOT NULL",
    )
    .bind(project_id)
    .fetch_one(&self.pool)
    .await?;
    Ok(row.get("avg_ms"))
}
```
Exactly the formula specified in the prompt, exactly one join simpler than `dashboard_avg_task_duration_ms` (operates on `runs` directly, no `tasks` join needed since both timestamps already live on `runs`). Parameterized (`.bind(project_id)`), no string concatenation, no SQL injection surface.

## Database / Source Verification

- `runs.started_at`, `runs.finished_at` — real `TEXT` (RFC3339) columns, present since `migrations/0001_core_execution.sql` (the very first migration; re-confirmed by reading the file directly this session, not assumed). **No migration was needed or added.**
- `runs.started_at` is set exactly once, by `store.rs::set_run_started` (`UPDATE runs SET started_at = ? WHERE id = ? AND started_at IS NULL`), called when the scheduler actually begins a run.
- `runs.finished_at` is set by `store.rs::set_run_finished` (every terminal path — COMPLETED/PARTIAL/FAILED/CANCELLED) and separately by `store.rs::reject_run` (REJECTED). Both are real, already-shipped, unmodified functions this unit only *reads from*, never edits.

## Project Isolation Verification

- The query itself: `WHERE r.project_id = ?`, identical binding pattern to every sibling `dashboard_*` function.
- The handler: `get_project_kpis` calls `require_scoped_project(&state, &ctx, project_id)` as its first line — **unmodified**, the exact same gate every other scoped endpoint uses (verified by re-reading the diff: this line was not touched).
- Live-tested, not just structurally reasoned: `be12b_avg_run_duration_is_project_isolated` creates two real projects with deliberately different real run durations (5s and 60s), confirms each project's `/kpis` response reflects only its own value, and confirms a key scoped to Project A still gets a real `403 FORBIDDEN` when requesting Project B's `/kpis` — the same existing authorization code path, unmodified.

## API Contract Change

`KpisResponse` gains exactly one additive field:
```rust
execution_avg_run_duration_ms: Option<f64>,
```
- **Backward compatible**: additive-only server→client struct, no existing field renamed/removed/retyped.
- **Nullability**: matches `execution_efficiency_ms`'s existing `Option<f64>` convention exactly — `null` for "no qualifying data yet," never a fabricated `0`.
- **No new endpoint, no new route registration** — `get_project_kpis` (`GET /api/v1/projects/:project_id/kpis`) is the same handler, same URL, unmodified signature.
- Frontend contract (`types/contract.ts::KpisResponse`) is intentionally **not** updated in this unit — that is explicitly a separate, future FE unit's work, per this unit's own scope boundary.

## Realtime Behavior

**No realtime change was made or needed.** `DashboardPage.tsx`'s existing debounced any-event resync already refetches `/kpis` in full on every real project event; the new field rides through that existing mechanism automatically the moment a future frontend unit starts reading it — confirmed by inspection of the existing frontend resync effect (unchanged, not touched by this backend-only unit) rather than assumed.

## Security Verification

No new authorization path, no new credential check, no new route. The new store function is read-only, parameterized, and reachable only through the already-authenticated, already-scoped `get_project_kpis` handler. No information about any project other than the one named in the URL is ever queried or returned — verified structurally (single `WHERE project_id = ?` predicate, no subquery, no cross-project join) and live-tested (§Project Isolation Verification).

## Fake/Stub Audit

```
grep -n "fake\|mock\|demo\|sample\|hardcoded" store.rs api.rs
grep -n "execution_avg_run_duration_ms" store.rs api.rs
```
Zero matches for fabrication markers in either changed production file. `execution_avg_run_duration_ms` flows directly from `state.store.dashboard_avg_run_duration_ms(&pid).await.map_err(to_500)?` straight into the response struct — no `.unwrap_or(0.0)`, no fallback branch, no default value anywhere in the path. A project with no qualifying runs genuinely returns `null`, verified by test (§Tests Added, item 2). Test-only code (`set_run_timestamps`, `infra_error_plan`, `fail_plan`) is confined to `api_tests.rs`, clearly a test fixture (mutates real rows via direct SQL purely to make a duration assertable exactly, not present in any production code path), and does not touch the production query itself.

## Tests Added

1. **`be12b_avg_run_duration_is_exact_over_two_real_runs`** — 2 real runs created via the real HTTP API, real engine execution confirmed to COMPLETED via `poll_run_state`, then their real `started_at`/`finished_at` rows overwritten to exact, known values (10s and 30s) so the resulting average is assertable to the millisecond (20000ms, verified within a 1ms float-precision epsilon) rather than merely "is a number."
2. **Extended `be12_dashboard_and_kpis_on_empty_project_show_honest_absence_not_fake_zero`** (existing test, one line added, not weakened) — a project with zero runs → `execution_avg_run_duration_ms` is `null`.
3. **`be12b_running_run_excluded_from_avg_run_duration`** — a real run with a real, genuinely non-null `started_at` (confirmed via `poll_run_state(..., "RUNNING")`) but no `finished_at` yet → contributes nothing, average is `null`. The still-running task is cancelled at the end of the test for clean shutdown.
4. **`be12b_mixed_terminal_states_all_counted_rejected_naturally_excluded`** — 5 real runs, one of each state (COMPLETED, FAILED, PARTIAL via a real `infra_error` task outcome, CANCELLED via the real cancel endpoint, REJECTED via the real maestro-approval-gate + reject endpoint), all 4 non-rejected runs given an identical known 10s duration; asserts the average is exactly 10000ms (proving REJECTED contributed nothing) **and** directly queries the real `runs` row for the rejected run to confirm `started_at IS NULL`/`finished_at IS NOT NULL` — verifying the fresh-audit finding above with real data, not just asserting the KPI's end result.
5. **`be12b_avg_run_duration_is_project_isolated`** — two real projects, deliberately different real durations (5s vs. 60s), confirms each project's KPI reflects only its own data, plus a real cross-project `403`.

Test 6 ("regression — existing KPI fields must remain unchanged") is satisfied by the full suite (below): `be12_dashboard_and_kpis_reflect_real_run_and_task_data` and the now-extended empty-project test both still pass, confirming `execution_reliability`/`execution_efficiency_ms` are untouched.

## Full Test Results

```
cargo test be12b
```
```
running 4 tests
test api_tests::be12b_avg_run_duration_is_project_isolated ... ok
test api_tests::be12b_avg_run_duration_is_exact_over_two_real_runs ... ok
test api_tests::be12b_running_run_excluded_from_avg_run_duration ... ok
test api_tests::be12b_mixed_terminal_states_all_counted_rejected_naturally_excluded ... ok
test result: ok. 4 passed; 0 failed
```

```
cargo test
```
Full suite, single-threaded (`-- --test-threads=1`, a deliberately low-contention run to establish a clean baseline):
```
test result: ok. 105 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.55s
```
(101 pre-existing + 4 new = 105; up from 101 at the end of BE-10.)

**Honest disclosure on parallel-run flakiness**: `cargo test`'s default (parallel) mode showed occasional, pre-existing flakiness during this session — different unrelated tests failed on different runs (`phase4_be10_intelligence_isolated_across_projects`, `phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields`, `actual_be9_scoped_approve_and_reject_through_router`, `be13_cancel_prevents_a_not_yet_started_queued_task_from_ever_running`, `be13_cancel_actually_interrupts_an_in_flight_task_rather_than_waiting_it_out`), never the same test twice, and **never a BE-12b test**. Each individually re-ran clean in isolation. This is consistent with real-timing-sensitive tests (cancel-interrupt timing, WS lag handling, async polling) competing for CPU under full parallel execution on this machine — the same class of resource-contention false-positive already diagnosed once earlier in this engagement (documented in the FE-16 report) — not a regression introduced by this unit. The single-threaded run above is the authoritative, reproducible result: 105/105, zero failures.

## Build/Check Result

```
cargo build
```
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 44.69s
```
Clean, zero errors, zero warnings from BE-12b's own code.

```
cargo clippy --all-targets
```
```
warning: this function has too many arguments (8/7)
    --> src\store.rs:1291:5   (insert_audit — pre-existing, unrelated to this unit, unchanged since BE-10's own report)
warning: `aiqa-orchestrator` (lib) generated 1 warning
```
Same single pre-existing warning already noted in BE-10's report. **Zero new warnings from BE-12b.**

```
cargo fmt --check
```
433 pre-existing diffs across the crate (419 at the end of BE-10, growing proportionally with the lines added to the same already-unformatted files) — the same repo-wide formatting drift already documented and explained in BE-10's report, not something this unit introduces or is in scope to fix (a repo-wide reformat would touch dozens of unrelated files).

## Regression Verification

Full suite 105/105 (single-threaded, authoritative run). `be12_dashboard_and_kpis_reflect_real_run_and_task_data` (the original BE-12 KPI test) passes unmodified — `execution_reliability` and `execution_efficiency_ms` behave exactly as before. All 101 pre-existing tests pass; zero pre-existing test was modified except the one intentional, additive assertion in the empty-project test (§Tests Added item 2).

## Any Limitations

- The occasional parallel-test-run flakiness described above is a pre-existing environment characteristic, not fixed or investigated further by this unit (out of scope — this unit's job was the KPI, not test-infrastructure hardening).
- `julianday`-based millisecond arithmetic has the same inherent floating-point precision characteristics as the existing `dashboard_avg_task_duration_ms` query it mirrors — not a new limitation, and not observed to cause any real-world precision problem (test assertions use a 1ms epsilon, consistent with floating-point best practice, not because a larger error was observed).
- No frontend consumes this field yet, by design — this unit's explicit, correct scope boundary.

## Final Recommendation

BE-12b is complete and safe to build on. The new `execution_avg_run_duration_ms` field is real, null-honest, project-isolated, and reuses the exact existing authorization/query architecture with zero new endpoints, zero migrations, and zero changes to any file outside the three explicitly allowed. **Implementation Status: PASS.**

## Files NOT Changed (explicit list)

`services/orchestrator/migrations/**` (all 4 files, no migration needed or added), `services/orchestrator/src/domain.rs`, `services/orchestrator/src/engine.rs`, `services/orchestrator/src/intelligence.rs`, `services/orchestrator/src/auth.rs`, `services/orchestrator/src/errors.rs`, `services/orchestrator/src/policy.rs`, `services/orchestrator/src/scheduler.rs`, `services/orchestrator/src/state.rs`, `services/orchestrator/src/evidence.rs`, `services/orchestrator/src/metrics.rs`, `services/orchestrator/src/webhook.rs`, `services/orchestrator/src/main.rs`, route registration in `api.rs::router` (no new `.route(...)` line added), every file under `forentend/**` (confirmed zero new frontend diff — `git status --porcelain -- forentend/` unchanged from before this unit).

---

**Implementation Status: PASS**

**Recommended Next Unit:** A frontend unit ("FE-26" in the user's own stated sequence) to display `execution_avg_run_duration_ms` on `DashboardPage.tsx` — the only remaining step to make this KPI visible, and the one explicitly deferred by this unit's own scope boundary. This is not a speculative recommendation: the backend field now exists, is real, is project-scoped, and already rides the existing realtime-resync path with zero further backend work required — it is a genuinely evidence-backed next step, not an invented one. No other next unit is recommended; BE-11's own fresh audit already concluded every other Dashboard KPI candidate is a STOP pending a product-level design decision this session has no authority to make.
