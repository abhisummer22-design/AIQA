# AIQA — Phase 1 Remediation Report — D5 ONLY

**Document:** `AIQA_D5_FIX_REPORT.md`
**Date:** 2026-08-17
**Scope:** Systematic persistence/recovery coverage audit for `services/orchestrator`
**Final Status:** **D5 — PASS**

---

## 1. D5 Objective

D1–D4 fixed and tested four specific defects in the persistence/recovery layer, but each was scoped to the one code path it targeted. The D4 verification flagged that no *systematic* audit of the remaining persistence surface had been done — specifically calling out `insert_evidence`, `set_run_state`, `set_run_started`, and concurrent recovery as paths with no fault-injection coverage at all. D5's objective is to inventory every persistence operation in the run/recovery lifecycle, determine — from source, not assumption — which already propagate errors correctly and which don't, add real fault-injection tests closing genuine gaps, and fix only what a gap turns out to be an actual defect rather than just an untested-but-correct path.

---

## 2. Persistence/Recovery Inventory

Every persistence operation reachable from run creation, execution, approval/rejection, evidence capture, eventing, and recovery, found by reading `store.rs`, `engine.rs`, and `api.rs` in full (not grep-driven spot checks):

| Operation | Defined in | Called from |
|---|---|---|
| `insert_run` | `store.rs:86` | `api.rs::create_run` |
| `set_run_state` | `store.rs:174` | `engine.rs::spawn_run` (transition to RUNNING) |
| `set_run_started` | `store.rs:183` | `engine.rs::spawn_run` |
| `set_run_finished` | `store.rs:192` | `engine.rs::spawn_run` (live finalisation), `engine.rs::recover_incomplete_runs` |
| `approve_run` | `store.rs:210` | `engine.rs::approve_and_spawn` |
| `reject_run` | `store.rs:222` | `engine.rs::reject_run` |
| `set_task_status` | `store.rs:236` | `engine.rs::spawn_run` (start + finish), `engine.rs::recover_incomplete_runs` (orphan repair) |
| `insert_event` | `store.rs:279` | `RunRecord::emit` (every event, live), `engine.rs::recover_incomplete_runs` (recovery-tail event) |
| `insert_evidence` | `store.rs:469` | `engine.rs::spawn_run` (after `evidence.put` succeeds) |
| `list_incomplete_runs` | `store.rs:452` | `engine.rs::recover_incomplete_runs` |
| `list_tasks_for_run` | `store.rs:370` | recovery, `get_run_summary`, API handlers |

Also inspected and confirmed out of D5's persistence-correctness surface: `insert_audit` (best-effort, fire-and-forget by design — `api.rs::audit()` discards its `Result` with `let _ =`, and audit-log completeness was never a Phase 1 correctness requirement), `intelligence.rs`'s upserts (errors propagate as `Result` but the call site in `spawn_run` intentionally logs-and-continues, because intelligence is scored *after* the run's own state is already durably finalised — a lost intelligence write cannot cause a run/task to misreport its own outcome), and `webhook.rs` (no database write at all).

---

## 3. Coverage Matrix — Before

| Persistence Operation | Failure Tested? | Error Propagated? | Recovery Tested? | Race Tested? | Gap |
|---|---|---|---|---|---|
| `insert_run` | Yes (B6, real lock contention) | Yes (`?` in API handler → 500) | N/A (atomic transaction) | No | — |
| `set_run_state` | **No** | Yes (source: `note_persistence_failure`) | N/A | No | **Untested** |
| `set_run_started` | **No** | Yes (source: `note_persistence_failure`) | N/A | No | **Untested** |
| `set_run_finished` | Indirectly (D2's trigger targets `run_events`, not this call; its own failure path is untested) | Yes (`note_persistence_failure`) | N/A | No | **Untested** |
| `approve_run` | Yes (D1) | Yes (D1: `Err` propagates, no in-memory mutation) | N/A | No | — |
| `reject_run` | Yes (D1) | Yes (D1) | N/A | No | — |
| `set_task_status` (finish) | Yes (D3, real trigger) | Yes | Yes (D3) | No | — |
| `insert_event` (live) | Yes (D2, real trigger) | Yes (`note_persistence_failure`) | N/A | No | — |
| `insert_event` (recovery tail) | **No** | Yes (`?`, propagates out of `recover_incomplete_runs`) | N/A | **No** | **Untested — and this is exactly where concurrency matters** |
| `insert_evidence` | **No** | Yes (source: `note_persistence_failure`) | N/A | No | **Untested** |
| `list_incomplete_runs` | Yes (D3) | N/A (read-only) | Yes (D3) | **No** | **Concurrency untested** |
| Concurrent recovery (two passes, same DB) | N/A | N/A | N/A | **No** | **Untested — explicitly requested** |

Four genuine gaps, matching exactly the ones flagged: `insert_evidence`, `set_run_state`, `set_run_started`, concurrent recovery. `set_run_finished`'s own failure path is also untested by name (D2 exercises `note_persistence_failure`'s downstream effects via a different call site) but was not added as a fifth mandatory case since it isn't in the brief's explicit minimum list; it's called out under §16 as a remaining gap.

---

## 4. Findings

### A. Genuine test gaps (no source change required)

1. **`insert_evidence`** — `spawn_run` already wraps this call in `if let Err(e) = ... { note_persistence_failure(...) }` (`engine.rs`, evidence-handling block). The escalation mechanism (B1) was already correctly wired here; it had simply never been exercised by a real fault.
2. **`set_run_state`** — same pattern: `if let Err(e) = store.set_run_state(...).await { record.note_persistence_failure("set_run_state(Running)", e).await; }`. Already correct, untested.
3. **`set_run_started`** — same pattern, already correct, untested.

For all three, reading the source before writing any test showed the escalation call was already present and already routes into the same B1 mechanism that D1/D2/D3/D4 already prove works end-to-end. This made the predicted classification, *before* running anything: Category A (test gap only). Step 4's fault-injection tests were written to confirm that prediction empirically rather than accept it on faith — see §5.

### B. Genuine product defects discovered

**None.** No source-level defect was found in `insert_evidence`, `set_run_state`, or `set_run_started` — the fault-injection tests in §5 confirmed the existing code already handles all three correctly.

### C. Out-of-scope / documented findings (not fixed)

**Concurrent recovery: one racer can return a spurious top-level `Err` despite full, correct data recovery.**

Two concurrent calls to `recover_incomplete_runs` against the same database (constructed as the literal scenario the brief asked for: `tokio::join!(recover_incomplete_runs(&store), recover_incomplete_runs(&store))` against two orphaned runs) reliably produce this pattern, confirmed across 20 total executions (15 standalone stress runs plus 5 full-suite runs):

- Exactly one of the two calls returns `Ok(2)` (both runs fully recovered).
- The other returns `Err(... UNIQUE constraint failed: run_events.run_id, run_events.sequence ...)`.

Root cause: `recover_incomplete_runs` iterates its run list and, for each run, ends its per-run work with `store.insert_event(&ev).await?;` — a "run recovered at startup" audit event at a **deterministic** sequence number (`tasks.len() as u64 + 1000`). `run_events` has `UNIQUE (run_id, sequence)` (migration `0001_core_execution.sql`). When two callers race on the *same* run, whichever's `INSERT` commits second hits that constraint and gets a real SQLite error, which `?` then propagates out of the entire loop — ending that caller's pass immediately, before it can move on to any other run still needing recovery.

This was traced through to its actual consequence rather than assumed dangerous: because both callers derive their run list from the same query and process it in the same order, the loser of the *first* contested run's event-insert race always aborts on that first run — meaning it never has the chance to also be racing for any later run. Only the winner of the first race ever reaches subsequent runs, and having eliminated its only competitor, it wins every race after that by default. Every state-mutating write (`set_task_status`, `set_run_finished`) for a given run completes and commits *before* that run's tail-event insert is attempted, so the loser's abort never discards a task or run state write that already landed — it only ever discards a *redundant* audit-event insert that would have collided with one already durably present. This is why every data-correctness assertion (task status, run state, and — critically — exactly one, not zero and not two, "recovered at startup" event per run) held in all 20 runs, even though the top-level `Ok`/`Err` split was essentially a coin flip every time.

**Why this was not fixed:** every literal pass criterion Step 4.D asked this test to verify — no double recovery, no duplicate task execution, no duplicate terminal events, no state corruption, consistent final durable state — holds unconditionally. The only actual defect is cosmetic: the losing caller's return value (and the `tracing::error!("startup run-recovery pass failed: ...")` line `main.rs` would log for it) is misleading, since the underlying data was in fact fully recovered. Two concurrent orchestrator processes against one database file is also already documented elsewhere in this codebase as an unsupported configuration (`api.rs`'s `approve_run` handler doc comment: "approving a run created by a different (or since-restarted) orchestrator process isn't supported in this slice — there's no cross-process work queue"), so this is a graceful-degradation characteristic of an already out-of-architecture scenario, not a defect in the architecture Phase 1 actually targets. Silently changing `recover_incomplete_runs`'s error handling to swallow this specific collision was considered and rejected: it isn't required by any stated pass criterion, and the scope lock explicitly says D5 is "primarily a coverage/remediation task, not permission to redesign unrelated behavior." Documented here instead, per Step 3's instruction for exactly this situation.

---

## 5. Tests Added

All four added to `services/orchestrator/src/engine_tests.rs`, under a new `// D5` section. Every test drives a real `spawn_run` (or `recover_incomplete_runs`) over a real on-disk SQLite file (via the existing `temp_store()` helper) and injects failures with `CREATE TRIGGER ... RAISE(ABORT, ...)` — genuine SQLite errors, not mocked `Result`s.

### `d5_insert_evidence_failure_is_escalated_with_no_false_success_row`

- **Purpose:** close the `insert_evidence` gap.
- **Failure simulated:** `CREATE TRIGGER fail_evidence_insert BEFORE INSERT ON evidence BEGIN SELECT RAISE(ABORT, ...) END` — every write to the `evidence` table genuinely fails. A new `EvidenceProducingExecutor` was added (the existing `SimulatedExecutor` always returns `evidence: None` and never reaches this code path at all) so the test task genuinely produces log text and genuinely calls `evidence.put` + `store.insert_evidence`.
- **Why meaningful:** proves the failure doesn't corrupt the task's own already-decided outcome, is escalated via `note_persistence_failure` (context string checked to contain `"insert_evidence"`), and — the specific "no false success" requirement — that the run is not reported `COMPLETED` over evidence that never reached the database.
- **Database verification:** `store.list_evidence_for_run(run_id)` asserted to return **zero rows** (not a partial or dangling row); separately, the evidence log file on disk is asserted to actually exist (`std::fs::read_dir` on the evidence directory) — proving the divergence is real: content exists on disk that the database has no record of, which is exactly the class of silent inconsistency B1's escalation exists to catch.

### `d5_set_run_state_running_failure_is_escalated_and_run_finishes_failed`

- **Purpose:** close the `set_run_state` gap.
- **Failure simulated:** `CREATE TRIGGER ... BEFORE UPDATE ON runs WHEN NEW.state = 'RUNNING' ...` — fails only the specific UPDATE that transitions a run to RUNNING. (`set_run_finished`'s own UPDATE never sets `state` to `'RUNNING'`, so it's untouched by this trigger — confirmed by the test's later assertion that finalisation still succeeds.)
- **Why meaningful:** `set_run_state` is fire-and-forget by design (the task dispatch loop doesn't wait on it), so the risk is specifically that execution proceeds *silently* as if the write succeeded. The test proves the task still genuinely executes (best-effort semantics preserved) **and** that the loss is not silent — the run still ends up durably `FAILED`, not a clean `COMPLETED`.
- **Database verification:** `store.get_run_summary(run_id).state` asserted equal to the live state (`Failed`), confirming durable agreement, not just an in-memory flag.

### `d5_set_run_started_failure_is_escalated_and_started_at_stays_null`

- **Purpose:** close the `set_run_started` gap.
- **Failure simulated:** `CREATE TRIGGER ... BEFORE UPDATE ON runs WHEN NEW.started_at IS NOT NULL ...` — isolates this exact write (`set_run_finished` never touches `started_at`, so `NEW.started_at` during that later UPDATE just carries over the still-NULL old value and passes through fine).
- **Why meaningful:** directly answers the brief's specific concern for this operation — "execution does not silently proceed as if durable start succeeded."
- **Database verification:** queries `SELECT started_at FROM runs WHERE id = ?` directly (not through `get_run_summary`'s parsed/defaulted view) and asserts it is genuinely `NULL`, even though the run demonstrably executed its task to `Passed` and finalised. Live and DB run state both asserted `Failed`.

### `d5_concurrent_recovery_of_multiple_orphaned_runs_is_consistent_and_non_blocking`

- **Purpose:** the explicitly requested concurrency test — "test two recovery attempts against the same recoverable run... do not assume a race exists; test it."
- **Race simulated:** two independent orphaned runs (each with one already-terminal task and one left `RUNNING`, seeded directly via `store.insert_run` + `store.set_task_status`, matching the existing B2/D3 seeding pattern) are recovered by `tokio::join!(recover_incomplete_runs(&store), recover_incomplete_runs(&store))` — two real concurrent calls against the same connection pool/file, not a simulated interleaving.
- **Why meaningful:** two runs, not one, so the test can distinguish "this run recovered correctly" from "one caller's collision on run A silently cost run B its recovery" — the actual failure mode a naive implementation could have.
- **Database verification, per run:** both tasks' final status (`Passed` preserved, `RUNNING` orphan repaired to `InfraError`) read back from `list_tasks_for_run`; run state read back from `get_run_summary` and asserted `Partial` (single consistent derived value); event count for the "recovered at startup" marker read back from `list_events_since` and asserted **exactly 1** — proving the `UNIQUE(run_id, sequence)` constraint genuinely prevented a duplicate terminal event from landing, not merely that no test happened to observe one.
- The test deliberately does **not** assert both `tokio::join!` results are `Ok` — see §4C for why, and what it asserts instead (the durable outcome, which is what actually matters).

Every test was run against the actual current source (no revert-then-restore cycle was needed here, unlike D4, because — per §4A — no defect existed to hide behind a "must fail before, pass after" comparison for the three fault-injection tests; they exercise an escalation mechanism that was already correct). The concurrency test's meaningfulness instead comes from what it empirically discovered (§4C) despite passing on first write — it was stress-run 15 additional times specifically to confirm the "always self-heals" result wasn't a lucky single observation, and it wasn't: 20/20.

---

## 6. Code Fixes

**None.** §4 found no genuine product defect within D5's fixable scope. `services/orchestrator/src/engine.rs` and `store.rs` were read in full but not modified this session.

---

## 7. Coverage Matrix — After

| Operation / Path | Failure Covered | Recovery Covered | Concurrency Covered | Result |
|---|---|---|---|---|
| `insert_run` | Yes (B6) | N/A (atomic) | No | Adequate |
| `set_run_state` | **Yes (D5)** | N/A | No | Adequate |
| `set_run_started` | **Yes (D5)** | N/A | No | Adequate |
| `set_run_finished` (live path) | Indirect (D5's own two tests reach it on the success side of the same run) | N/A | No | Its *own* failure path remains untested by name — see §16 |
| `approve_run` / `reject_run` | Yes (D1) | N/A | No | Adequate |
| `set_task_status` (start + finish) | Yes (D3 for finish; start-branch failure untested) | Yes (D3) | No | Finish-branch adequate; start-branch untested — §16 |
| `insert_event` (live) | Yes (D2) | N/A | No | Adequate |
| `insert_event` (recovery tail) | **Yes (D5, via the concurrency test)** | Yes | **Yes (D5)** | Adequate — race characterised, documented (§4C) |
| `insert_evidence` | **Yes (D5)** | N/A | No | Adequate |
| `evidence.put` (filesystem write) | No | N/A | No | Untested — §16 |
| `list_incomplete_runs` / recovery orphan repair | Yes (D3) | Yes (D3) | **Yes (D5)** | Adequate |
| Concurrent recovery, same DB | N/A | Yes | **Yes (D5)** | Characterised; safe on every measured invariant; one cosmetic gap documented, not fixed |

Not claiming 100% coverage: §16 lists what remains genuinely untested, honestly, rather than rounding up.

---

## 8. D1 Regression

```
running 4 tests
test engine_tests::d1_successful_rejection_persists_and_mutates_state ... ok
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::d1_successful_approval_persists_and_spawns ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 33 filtered out
```
**Result: D1 — PASS (unchanged).**

## 9. D2 Regression

```
running 1 test
test engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out
```
**Result: D2 — PASS (unchanged).**

## 10. D3 Regression

```
running 1 test
test engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out
```
**Result: D3 — PASS (unchanged).**

## 11. D4 Regression

```
running 3 tests
test engine_tests::d4_live_run_with_failed_task_matches_recovery_derived_state ... ok
test engine_tests::d4_live_run_with_blocked_task_matches_recovery_derived_state ... ok
test engine_tests::d4_live_run_with_timeout_task_matches_recovery_derived_state ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out
```
**Result: D4 — PASS (unchanged).**

## 12. B2/B3/B4/B5/B6/B14 Regression

```
b2_*  — 3 passed, 0 failed
b3_*  — 1 passed, 0 failed
b4_*  — 1 passed, 0 failed
b5_*  — 1 passed, 0 failed
b6_*  — 2 passed, 0 failed
b14_* — 2 passed, 0 failed
```
**Result: all PASS (unchanged).**

---

## 13. Full Test Suite

```
running 37 tests
... (all listed individually in terminal output, including the 4 new D5 tests)
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.3x s
```

Re-run **5 consecutive times** in full (37/37 every time) plus the concurrency test standalone **15 additional times** (`--nocapture`, confirming the race genuinely occurs and every data-correctness assertion holds regardless of which caller "wins") — **37/37, 0 failed, 20/20 stress executions of the racy test with correct invariants**, no unrelated failure.

---

## 14. Build

```
$ cargo check --all-targets
    Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.68s
```
**0 errors, 0 warnings.**

---

## 15. Scope Verification

| Constraint | Status |
|---|---|
| Frontend (`apps/web/`) untouched | ✅ Confirmed — no file under `apps/web/` opened or written this session |
| `forentend/` untouched | ✅ Confirmed |
| `AIQA_API_CONTRACT_v1.0.md` untouched | ✅ Confirmed — not opened |
| `AIQA_ACCEPTANCE_MATRIX_v1.0.md` untouched | ✅ Confirmed — not opened |
| Phase 2 untouched | ✅ Confirmed — no `/api/v1` prefix, migration, or scope work started |
| No new API endpoints | ✅ Confirmed — `api.rs` was read for inventory purposes only, never edited |
| No UI functionality added | ✅ Confirmed |
| No unrelated refactoring | ✅ Confirmed — `engine.rs` and `store.rs` were read but not modified; the only file changed this session is `engine_tests.rs` (four tests appended) |
| No "clean up" of unrelated code | ✅ Confirmed |
| D1/D2/D3/D4 preserved | ✅ Confirmed (§8–11) |

Files changed this session: `services/orchestrator/src/engine_tests.rs` (four tests appended) and this report. No production source file was modified, because no genuine product defect requiring remediation was found.

---

## 16. Remaining Findings (explicitly untested or intentionally not fixed)

- **Concurrent recovery's cosmetic false-alarm (§4C)** — one racer can return a spurious top-level `Err` (and would log a misleading "recovery pass failed" line in `main.rs`) despite full, correct data recovery. All measured data-correctness invariants hold; not fixed, for the reasons given in §4C.
- **`set_run_finished`'s own failure path** — D5's `set_run_state`/`set_run_started` tests exercise the *success* side of `set_run_finished` (it's what durably records their `Failed` escalation), but no test injects a failure directly on `set_run_finished`'s own `UPDATE`. That specific write already routes into `note_persistence_failure` on error (`engine.rs`), consistent with every other site audited in this pass, but that has not been proven by fault injection the way the four D5 tests prove it for their targets.
- **`set_task_status`'s start-branch failure** — D3 fault-injects the *finish* branch (`set_finished: true`). The *start* branch (`set_started: true`, marking a task `Running`) is not separately fault-injected; its error handling is identical in shape (`note_persistence_failure`) and was read as part of this audit, but not empirically proven.
- **`evidence.put`'s filesystem-write failure** — distinct from `insert_evidence` (the DB write, now covered). A filesystem-level fault (e.g. a read-only evidence root) would exercise the sibling `Err(e) => note_persistence_failure(&format!("evidence.put({id})"), e)` arm, which was read but not fault-injected, since doing so portably (Windows dev environment) is materially more involved than a SQL trigger and was judged lower priority than the four items the brief explicitly named.
- **True multi-process concurrency** — the D5 concurrency test uses two concurrent async calls against one `Store` (one connection pool), which is the correct and sufficient way to test race safety of the *code*, and is what actually raced at the SQLite level. It does not launch two separate OS processes; given `Store`'s pool is the actual point of contention, this is not expected to change the result, but is noted for completeness.

---

## Final Status

# D5 — PASS
