# AIQA — Backend Test-Suite Timing Flakiness: Root-Cause Investigation & Fix

## 1. Status

**PASS — Test Timing Flakiness Hardened.** Two distinct, provable, reproducible test-harness races were found and fixed, both **Outcome A** (test-only timing/condition margin — production logic is correct in both cases). Zero production files were touched.

## 2. Root Cause Investigation

**Q1 — Exact flaky tests.** Reproduced directly this session: `api_tests::phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields` and `api_tests::phase4_be10_intelligence_isolated_across_projects` — matching the audit's own report exactly. A **third**, previously-undetected flaky test was also discovered during this unit's own required repeated full-suite runs: `api_tests::be12b_avg_run_duration_is_exact_over_two_real_runs` (same helper family, different root cause — see below).

**Q2 — Exact polling helpers used.** The two originally-reported tests both call `poll_intelligence_row` (`api_tests.rs`). The newly-discovered third test calls `poll_run_state` followed by a direct-SQL `set_run_timestamps` helper.

**Q3/Q4/Q5 — Retry count / interval / max wait (both helpers, before fix).** `poll_intelligence_row` and `poll_run_state` are identical in shape: `for _ in 0..100 { ...; sleep(30ms) }` → 100 retries × 30ms = **3000ms (3s) maximum wait**.

**Q6 — Exact condition each helper waits for (before fix).**
- `poll_intelligence_row`: `rows.iter().find(|r| r["name"] == task_name)` — waits only for **a row to exist with the right name**, nothing about its field values.
- `poll_run_state`: `detail["state"] == want` — waits for the run's API-visible `state` field to equal the target string.

**Q7 — Is intelligence data written synchronously or asynchronously relative to run completion?** **Asynchronously**, and in **multiple separate sequential DB writes**. Direct read of `engine.rs` (lines 689–707): the run's terminal state is set in-memory and `RUN_COMPLETED` is emitted, and **only after that** does `intel.ingest_run(...)` run. Inside `ingest_run` → `update_test_intelligence` (`intelligence.rs:148-249`), each task gets **three separate, sequential upserts**: `upsert_quality_node` (creates the row, name now matchable) → `upsert_risk_assessment` → `upsert_test_value` → `upsert_flakiness`. Each is a separate awaited DB call.

**Q8 — Can the run become terminal before intelligence persistence finishes?** **Yes, always, by design** — confirmed directly in `engine.rs:689-707`: `record.state`/`RUN_COMPLETED` happen strictly before `intel.ingest_run(...)` is even invoked.

**Q9 — Can `RUN_COMPLETED` be emitted before intelligence persistence finishes?** **Yes** — same evidence as Q8; this ordering is not a bug, it's the intended architecture (intelligence is a post-processing step over a completed run's snapshot), and it is already disclosed elsewhere in this codebase (the FE-25/prior-audit "known, disclosed, low-severity" realtime-ordering note) and mitigated there by the frontend's own debounce. This audit's new finding is a **second, previously-undocumented sub-race one level deeper**: *within* `ingest_run` itself, `quality_nodes` (name-visible) is written before `risk_assessments`/`test_value_assessments`/`flakiness_metrics` (score-visible) — confirmed directly via `store.rs`'s `list_test_intelligence` query (`store.rs:1012-1028`), which `LEFT JOIN`s `quality_nodes` against the three score tables. A `LEFT JOIN` means a node with no matching score row yet still returns as a row — with `risk_score`/`value_score`/`flakiness_score` all `NULL`. This is exactly, precisely what the two flaky tests observed: real, reproduced failure output this session showed a row with `risk_score: 0.00008` (populated) but `flakiness_score: null, window_runs: null, window_failures: null` (not yet populated) — proving the row was captured squarely mid-sequence between the second and fourth upsert.

**Q10 — Can parallel tests interfere with each other?** No cross-test interference found or suspected: every test uses `test_state()`, its own fresh temp-dir SQLite file (confirmed in the earlier audit and re-confirmed by this session's isolated single-test reruns reproducing the same failures with zero other tests running concurrently).

**Q11 — Are DB locks/contention possible under parallel execution?** Not implicated here: the failures reproduced readily in **isolated, filtered, single-test runs** (`cargo test phase4_be10_intelligence -- --test-threads=1`, no other tests in the binary running), which rules out cross-test lock contention as the mechanism. The race is entirely internal to a single test's own two concurrent tasks (its HTTP-handling test code vs. the run's own background `tokio::spawn`ed driver task).

**Q12 — Any fixed sleep, bounded retry, race-prone ordering, or shared-state dependency elsewhere in these tests?** Yes, newly found: `be12b_avg_run_duration_is_exact_over_two_real_runs` (and 3 sibling BE-12b tests) call `poll_run_state(..., "COMPLETED")` and then immediately overwrite `runs.started_at`/`finished_at` via a raw SQL `set_run_timestamps` helper — a second, distinct race, detailed in §4/§5 below.

**Q13 — Is the observed failure genuinely consistent with insufficient polling time, or is another failure mode possible?** **Another failure mode** — confirmed by direct, repeated reproduction (§3). Both races are **wait-on-the-wrong-condition** bugs, not "didn't wait long enough in total": the helpers return success as soon as *a* signal appears (name match / API-visible state), when the caller actually needs a *later*, more specific signal (fully-scored row / durably-persisted DB timestamp) that can appear a short, variable, real time afterward — a duration that is *not* bounded by any of the existing sleep/retry parameters, so no amount of *total* wait time fixes it; only waiting on the *correct* condition does. This was proven directly: the intelligence-row race reproduced in isolated runs finishing in 0.3–0.7s (no load at all), and reproduced on the very first poll attempt in the captured failure output — i.e., attempt 1 of 100 already returned the wrong thing.

## 3. Reproduction Results

| Run set | Mode | Attempts | Failures | Failing test(s) | Failure message |
|---|---|---|---|---|---|
| Full suite | Single-threaded (`--test-threads=1`) | 1 | 1 | `phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields` | `flakiness_score must be a real computed value; row={...,"flakiness_score":null,...,"window_failures":null,"window_runs":null,...}` at `api_tests.rs:1837` |
| Filtered (`phase4_be10_intelligence`), single-threaded, before fix | Isolated (~101 other tests filtered out) | 8 | 3 | Mix of `phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields` and `phase4_be10_intelligence_isolated_across_projects` | One capture: panic at `api_tests.rs:1872:47` — `row_a["risk_score"].as_f64().unwrap()` on a `null` |
| Full suite | Single-threaded, before fix (2nd family) | 3 | 1 | `be12b_avg_run_duration_is_exact_over_two_real_runs` | `expected exactly (10s+30s)/2 = 20000ms, got <wrong value>` at `api_tests.rs:943` |
| Filtered (`phase4_be10_intelligence`), single-threaded, **after fix** | Isolated | 8 | 0 | — | — |
| Filtered (`be12b`), single-threaded, **after fix** | Isolated | 8 | 0 | — | — |
| Full suite, single-threaded, **after fix** | Full | 5 | 0 | — | 105/105 every run, 18.9–20.3s each |
| Full suite, default parallel, **after fix** | Full | 5 | 0 | — | 105/105 every run, 3.2–3.5s each |

The failures occurred **before intelligence data existed in full** (the row existed but not every scored field did) and, separately, **before the run's own row-level durable completion** (the DB row's `finished_at` had not yet landed when a test's own direct write raced it) — not before run completion itself, and not any other failure mode (no panics, no connection errors, no deadlocks were observed in any run).

The original flake **was** reproduced this session (both before-fix failure classes), and both are now unreproducible across 21 combined post-fix runs of the specific affected tests plus 10 full-suite runs.

## 4. Root Cause Classification

**Outcome A — Test Timing Margin (Condition) Confirmed**, for both findings:

- Production logic is correct in both cases: `ingest_run`'s sequential upserts always converge to a fully-scored row (no data loss, no dropped writes, no incorrect final value — confirmed by the fully-passing, repeated post-fix runs asserting the real, deterministic computed values); `engine.rs`'s completion path always durably persists `finished_at` shortly after the in-memory flip (confirmed the same way).
- No incorrect ordering exists relative to any documented contract: `TestIntelRow`'s fields are explicitly, deliberately nullable (`contract.ts`'s own doc comment: "a test with no scoreable history yet is `null`, never a fabricated `0`") — a transiently-null field while ingestion is mid-flight is consistent with, not a violation of, this documented behavior. Nothing in the API contract promises "when a row is name-matchable, all its fields are already populated," or "the instant `GET .../executions/:id` reports a terminal state, the DB row for that run is already updated."
- The failure is specifically caused by the test **polling on the wrong condition** — not by an insufficient total timeout. Both bugs were proven reproducible in isolated, unloaded, sub-second runs, and in the captured intelligence-row failure, on the very first poll attempt — meaning no realistic timeout increase would have reliably fixed either one; only correcting *what* is being waited for does.

Neither finding rises to Outcome B: no evidence of data loss, no evidence of an incorrect final value ever being served to a real caller, and no violation of any documented contract guarantee. Both are confined to test-harness assumptions that don't match the real (correct, intentional, already-partially-disclosed) asynchronous-persistence architecture.

## 5. Fix Applied

**Finding 1 — `poll_intelligence_row` (the two originally-reported tests).** Tightened the wait condition from "a row named X exists" to "a row named X exists **and** `risk_score`/`value_score`/`flakiness_score` are all populated numbers." This directly targets the proven mechanism (the `LEFT JOIN`-visible, not-yet-fully-written row) rather than the total wait budget, which was left unchanged (still 100 × 30ms = 3s, since the previous failures show the *condition*, not the *budget*, was the defect).

**Finding 2 — `set_run_timestamps` / the BE-12b average-duration tests (found during this unit's own required stress-testing).** Root cause: `poll_run_state` reads `GET .../executions/:id`, which — while a run is held in the in-memory `RunRecord` registry — serves `record.summary()` (`engine.rs:214-241`). That function's `state`/`finished_at` fields are set **in-memory** (`engine.rs:689-690`) strictly before the corresponding `store.set_run_finished(...)` DB `UPDATE` (line 691) is even issued, let alone committed. Every BE-12b duration test polls for the terminal state via this API-visible signal, then immediately performs its own **direct SQL** overwrite of `started_at`/`finished_at` to get an exact, assertable duration — racing the engine's own real, not-yet-committed `UPDATE`. If the engine's write lands *after* the test's overwrite, it silently clobbers the test's deterministic values back to a real (near-zero) duration, breaking the exact-average assertion. Fix: added `wait_for_run_row_finished`, a small bounded poll (same 100×30ms shape) that waits for the real DB row's `finished_at` column to be non-null — i.e., waits for the actual signal the test is about to conflict with — before `set_run_timestamps` performs its overwrite. This is called once, inside `set_run_timestamps` itself, so all 4 existing call sites are fixed uniformly without touching any of the other ~8 tests that use `poll_run_state` correctly for their own (non-conflicting) purposes.

Both fixes preserve immediate success when the condition is already met (a `for` loop that checks the condition before ever sleeping) and use the project's existing bounded-retry idiom rather than introducing a new pattern.

## 6. Files Changed

- `services/orchestrator/src/api_tests.rs` — **only file changed.**

Production files (`engine.rs`, `intelligence.rs`, `api.rs`, `store.rs`) remain untouched because both confirmed root causes are test-harness assumptions that don't match an intentional, non-buggy, already-consistent-with-the-documented-contract asynchronous-persistence design — there is no proven production defect to fix, per the task's own Outcome-A criteria and the non-negotiable "prove before changing" rule.

## 7. Before vs After Polling Behavior

| Helper | Retry count | Interval | Max wait | Condition (before) | Condition (after) | Immediate-success preserved |
|---|---|---|---|---|---|---|
| `poll_intelligence_row` | 100 (unchanged) | 30ms (unchanged) | 3s (unchanged) | name match only | name match **and** `risk_score`/`value_score`/`flakiness_score` all numeric | Yes |
| `set_run_timestamps` (new: `wait_for_run_row_finished`) | 100 (new helper, same shape as project convention) | 30ms | 3s | *(none — no wait existed before this fix)* | `runs.finished_at IS NOT NULL` for the target run | Yes |
| `poll_run_state` | 100 (unchanged) | 30ms (unchanged) | 3s (unchanged) | `state == want` (unchanged — correct as-is for its other ~8 callers) | unchanged | Yes |

## 8. Production Behavior Impact

**Zero.** No production file was modified. No API route, response shape, database schema, intelligence algorithm, execution lifecycle, realtime event behavior, RBAC behavior, or project-isolation behavior changed. Confirmed by `git diff --stat` (§12).

## 9. Tests

- Isolated `phase4_be10_intelligence_*` (before fix): 8 runs, 3 failures (both originally-reported tests, mixed).
- Isolated `phase4_be10_intelligence_*` (after fix): 8 runs, **0 failures**.
- Isolated `be12b` duration tests (after fix): 8 runs, **0 failures**.
- Full suite, single-threaded (after fix): 5 runs, **105/105 every run** (18.93s, 19.18s, 19.98s, 20.23s, 20.29s).
- Full suite, default parallel (after fix): 5 runs, **105/105 every run** (3.38s, 3.35s, 3.46s, 3.23s, 3.45s).
- Total post-fix evidence: **26 runs across both affected test groups plus 10 full-suite runs, 0 failures.**

## 10. Build Result

```
cd services/orchestrator && cargo build
```
`Finished dev profile [unoptimized + debuginfo] target(s)` — clean, both before and after the fix. (Note: `api_tests.rs` is a `#[cfg(test)]`-only module, so `cargo build` alone does not recompile it — `cargo test --no-run` was used to positively confirm the test binary itself compiles clean: `Finished test profile [unoptimized + debuginfo] target(s) in 15.92s`, no errors.)

## 11. Fake / Masking Audit

Confirmed explicitly, this session did **not**:
- Add any arbitrary fixed sleep (`tokio::time::sleep(Duration::from_secs(N))` before an assertion) — the only sleeps are the existing bounded-retry idiom's per-iteration 30ms, unchanged in count or interval.
- Weaken any assertion — both fixed tests still assert the exact same real values (`is_number()`, `.as_f64()`, exact millisecond averages) as before.
- Ignore or swallow any error — `wait_for_run_row_finished` still `panic!`s if its bounded wait is exceeded, exactly like every other polling helper in this file.
- Retry a whole test to hide a failure — no `#[test]`-level retry/flake-retry mechanism was added anywhere.
- Hide a production race with a longer timeout — the total wait budget (3s / 100×30ms) was left **unchanged** in every existing helper; only the *condition* being waited for was corrected, and a *new*, narrowly-scoped wait was added exactly where a real signal was missing (not as a blanket delay).

## 12. Regression Audit

`git diff --stat` (from repo root) confirms:
```
services/orchestrator/src/api.rs          | (unchanged from before this session's fix work — still the pre-existing +56 BE-12b diff)
services/orchestrator/src/store.rs        | (unchanged — still the pre-existing +32 BE-12b diff)
services/orchestrator/src/engine.rs       | (no diff at all — never touched)
services/orchestrator/src/intelligence.rs | (no diff at all — never touched)
services/orchestrator/src/api_tests.rs    | only file with new changes this session
```
1. No production source behavior changed. ✅ (engine.rs/intelligence.rs untouched; api.rs/store.rs diffs identical in size to the pre-existing, already-reported BE-12b diff)
2. No API route changed. ✅
3. No API response changed. ✅
4. No database schema changed. ✅ (no migration files touched)
5. No intelligence algorithm changed. ✅ (`intelligence.rs` untouched)
6. No execution lifecycle changed. ✅ (`engine.rs` untouched)
7. No realtime event behavior changed. ✅
8. No RBAC behavior changed. ✅
9. No project-isolation behavior changed. ✅
10. Only test infrastructure changed. ✅ (`api_tests.rs` only)

## 13. Limitations

Repeated runs (26 targeted + 10 full-suite, all clean) are **strong evidence of stability**, not a mathematical proof of absolute elimination. Both fixed races were proven to depend on real async-scheduling timing that varies with system load; a sufficiently extreme load spike could in principle still make even the corrected, longer/more-specific wait conditions time out at their existing 3-second bound (a starvation scenario, not a re-emergence of the original wrong-condition bug). What *is* eliminated is the specific defect proven this session — returning a wrong-but-present intermediate state as if it were the final one — because both helpers now wait on the actual condition the caller needs, not a proxy for it.

## 14. Final Decision

**PASS — Test Timing Flakiness Hardened.**

## 15. Recommended Next Step

Per the project's own established discipline (do not invent a feature unit merely to keep a sequence moving): a fresh, evidence-based audit should be performed again before any further implementation unit is chosen, exactly as the prior full-project audit itself already concluded. This unit found and fixed two genuine test-infrastructure defects but changed nothing about the product surface, so the prior audit's own conclusion — no Category-A feature gap currently justified — stands unchanged and should simply be re-confirmed fresh at that time, not assumed.
