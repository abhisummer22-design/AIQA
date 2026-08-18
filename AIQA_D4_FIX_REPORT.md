# AIQA — Phase 1 Remediation Report — D4 ONLY

**Document:** `AIQA_D4_FIX_REPORT.md`
**Date:** 2026-08-17
**Scope:** Remediation of Defect D4 in `services/orchestrator`
**Final Status:** **D4 — PASS**

---

## 1. D4 Root Cause

Two independent places computed a run's terminal state from its tasks, and they used different rules.

**Recovery** (`engine.rs`, `recover_incomplete_runs`) has always used the single canonical function:

```rust
fn derive_run_state(tasks: &[TaskState]) -> RunState {
    let has = |s: TaskStatus| tasks.iter().any(|t| t.status == s);
    if has(TaskStatus::InfraError) || has(TaskStatus::Timeout) {
        RunState::Partial
    } else if has(TaskStatus::Failed) {
        RunState::Failed
    } else if has(TaskStatus::Blocked) {
        RunState::Partial
    } else {
        RunState::Completed
    }
}
```

**Live finalisation**, at the end of `spawn_run`'s dispatch loop, did not call it. It had its own inline check, written before `derive_run_state` existed:

```rust
let mut final_state = {
    let engine = record.engine.read().await;
    if engine.tasks.values().any(|t| t.status == TaskStatus::InfraError) {
        RunState::Partial
    } else {
        RunState::Completed
    }
};
```

This inline check tests for exactly one outcome — `InfraError` — out of the five `derive_run_state` considers. Any run whose only non-passing outcome was `FAILED`, `BLOCKED`, or `TIMEOUT` fell through to the `else` branch and was reported `Completed`, live, by the exact same process that just ran it — before any restart, before recovery ever touches it.

Both functions read from the same kind of data (`domain::TaskState`), at a point where every task is guaranteed terminal (`RunEngine::is_finished()` — `self.tasks.values().all(|t| t.status.is_terminal())` — is what ends the dispatch loop just above this code). So the two rules were never looking at different information; they were just different rules over the same information, with the live one asking a narrower question than the canonical one.

---

## 2. Contract Semantics

Per Step 2's instruction, `AIQA_API_CONTRACT_v1.0.md` was searched before any code was touched. It defines, in §13 "Canonical Execution Status", the **task-level** statuses (`QUEUED`, `RUNNING`, `PASSED`, `FAILED`, `BLOCKED`, `SKIPPED`, `CANCELLED`, `NEEDS_REVIEW`, `INFRASTRUCTURE_ERROR`) and says nothing else prescriptive about them. The contract contains **no section defining run-level state values or run-state derivation rules** — no mention of `COMPLETED`/`PARTIAL`/`FAILED` as run states, and no rule for how a task-status set maps to one.

Since the contract is silent on run-state derivation, there is no conflict to report and no invented semantics were needed. `derive_run_state` is the only place this logic is established in the codebase; it already carries a documented rationale (its own doc comment: infra outcomes are worse than a clean pass, a product failure outranks a blocked dependent because the dependent's block is usually just the failure's consequence) and is already relied on and tested by B2, D2, and D3. Per the task's own instruction — *"the fix must make live finalisation consistent with the already-established recovery/state-derivation behavior unless the API Contract explicitly says otherwise"* — that established behavior is what this fix reuses.

---

## 3. Reproduction

All three cases were run live against the current (pre-fix) binary and the real production database (preserved before, restored after), then confirmed durable and cross-checked against the canonical derivation.

### Case A — FAILED

Run: 2 passing tasks + 1 `simulate.outcome=fail` task.

| | Value |
|---|---|
| Task states | `bad → FAILED`, `ok1 → PASSED`, `ok2 → PASSED` |
| **Live API state** | **`COMPLETED`** |
| **Durable DB state** | **`COMPLETED`** |
| Canonical `derive_run_state` (via a real recovery pass over the identical task snapshot, seeded as a separate `RUNNING` run) | **`FAILED`** |

### Case B — BLOCKED

Run: one task fails (`root`), a dependent (`dependent`, `prerequisites: [root]`) is blocked.

| | Value |
|---|---|
| Task states | `root → FAILED`, `dependent → BLOCKED` |
| **Live API state** | **`COMPLETED`** |
| **Durable DB state** | **`COMPLETED`** |
| Canonical derivation | **`FAILED`** (per `derive_run_state`'s documented precedence: a product `FAILED` outranks `BLOCKED`) |

### Case C — TIMEOUT

Run: one fast passing task, one task bound to a hung executor with `AIQA_TASK_TIMEOUT_SECS=2`.

| | Value |
|---|---|
| Task states | `slow → TIMEOUT`, `fast → PASSED` |
| **Live API state** | **`COMPLETED`** |
| **Durable DB state** | **`COMPLETED`** |
| Canonical derivation | **`PARTIAL`** |

All three were captured directly from the database (not only from logs), and all three durably persisted the wrong verdict — this was not a display-only bug limited to one in-flight HTTP response; `COMPLETED` was what `set_run_finished` wrote to disk in every case.

---

## 4. Fix

**File:** `services/orchestrator/src/engine.rs`, inside `spawn_run`'s finalisation block. One change.

```rust
// before
let mut final_state = {
    let engine = record.engine.read().await;
    if engine.tasks.values().any(|t| t.status == TaskStatus::InfraError) {
        RunState::Partial
    } else {
        RunState::Completed
    }
};

// after
let tasks_snapshot: Vec<TaskState> = { record.engine.read().await.tasks.values().cloned().collect() };
let mut final_state = derive_run_state(&tasks_snapshot);
```

No new function was written and no rule was duplicated — `derive_run_state` (already `fn`-private within `engine.rs`, already used by `recover_incomplete_runs` at line 310) is called directly from the live path, in the same module. `RunEngine::tasks` is a `HashMap<TaskId, domain::TaskState>`, the exact type `derive_run_state` already accepts, so no adapter or new type was needed.

A duplicate collection of the same snapshot slightly further down the function (originally taken a second time, after finalisation, to feed the intelligence layer) was removed and replaced with a comment explaining the earlier snapshot is safe to reuse — no task can change between the two points, since the dispatch loop has already exited and nothing remains in flight. This is not new behavior; it removes one redundant read/clone that existed only because the two use sites were written at different times.

Nothing about *when* finalisation runs was changed — it still only executes once the same `is_finished()` guarantee (all tasks terminal) that always gated it. This directly satisfies the brief's caution against "accidentally marking an incomplete run as terminal": the fix changes only *how* the already-fully-terminal snapshot is interpreted, not *when* that snapshot is taken.

**File:** `services/orchestrator/src/engine_tests.rs` — three new regression tests appended (§6). No existing test was modified.

---

## 5. After-Fix Verification

### 5.1 Unit tests (real on-disk SQLite, real `spawn_run`)

| Case | Live state | DB state | Recovery-derived (same snapshot) |
|---|---|---|---|
| FAILED | `Failed` | `Failed` | `Failed` |
| BLOCKED | `Failed` | `Failed` | `Failed` |
| TIMEOUT | `Partial` | `Partial` | `Partial` |

All three match exactly — see §6 for how "recovery-derived" is obtained without reaching into a private function.

### 5.2 Live, against the rebuilt binary and the real HTTP API

Same three cases, replayed against the fixed binary on the real (preserved/restored) production database:

| Case | Live API state | Durable DB state |
|---|---|---|
| A — FAILED | `FAILED` | `FAILED` |
| B — BLOCKED (+FAILED) | `FAILED` | `FAILED` |
| C — TIMEOUT | `PARTIAL` | `PARTIAL` |

No case reports `COMPLETED` any longer; live and durable state agree with each other and with what recovery would independently derive for the same task set.

---

## 6. Regression Tests

Three tests added to `engine_tests.rs`, one per case, each asserting all four of the brief's required points (task reaches expected terminal state; live finalisation derives the expected run state; the database contains the same state; recovery derives the same result):

- **`d4_live_run_with_failed_task_matches_recovery_derived_state`** — 2 passed + 1 failed task. Asserts `bad` reaches `Failed`; live `record.summary().state == RunState::Failed`; DB `get_run_summary` agrees; and a helper (`recovery_derived_state`, below) confirms recovery independently derives `Failed` for the same three-task snapshot.
- **`d4_live_run_with_blocked_task_matches_recovery_derived_state`** — a failed prerequisite and its blocked dependent. Also exercises `derive_run_state`'s FAILED-over-BLOCKED precedence: the expected (and asserted) state is `Failed`, not `Partial`.
- **`d4_live_run_with_timeout_task_matches_recovery_derived_state`** — reuses the existing `HangingExecutor` (from the B5 test) with a 2-second task timeout. Asserts `Partial`.

**`recovery_derived_state` helper** — `derive_run_state` is a private function in `engine.rs` with no test-only visibility carve-out (`engine_tests.rs` is a sibling module, not a child of `engine.rs`, so private items aren't reachable from it, and no visibility change was made to keep this fix minimal). Rather than loosening that visibility, the helper seeds a **separate** run with the exact task-status snapshot under test as a fresh `RUNNING` row, runs the real `recover_incomplete_runs` over it, and reads back the state it derived — exercising the canonical function through its one real production entry point, the same way the live reproduction in §3 did.

Each test was run against the pre-fix code (§ below) and observed to fail with the exact old, wrong verdict, then against the fixed code and observed to pass.

**Before fix** (inline logic temporarily restored to reproduce, then reverted — see §4; no test was written to bypass the persistence layer):

```
test engine_tests::d4_live_run_with_blocked_task_matches_recovery_derived_state ... FAILED
  assertion `left == right` failed: a run with a FAILED prerequisite and a BLOCKED dependent must not be reported COMPLETED live
  left: Completed
 right: Failed

test engine_tests::d4_live_run_with_failed_task_matches_recovery_derived_state ... FAILED
  assertion `left == right` failed: a run containing a FAILED task must not be reported COMPLETED live
  left: Completed
 right: Failed

test engine_tests::d4_live_run_with_timeout_task_matches_recovery_derived_state ... FAILED
  assertion `left == right` failed: a run containing a TIMEOUT task must not be reported COMPLETED live
  left: Completed
 right: Partial

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 30 filtered out
```

**After fix:**

```
running 3 tests
test engine_tests::d4_live_run_with_blocked_task_matches_recovery_derived_state ... ok
test engine_tests::d4_live_run_with_failed_task_matches_recovery_derived_state ... ok
test engine_tests::d4_live_run_with_timeout_task_matches_recovery_derived_state ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out; finished in 2.30s
```

---

## 7. D1 Regression

```
running 4 tests
test engine_tests::d1_successful_rejection_persists_and_mutates_state ... ok
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::d1_successful_approval_persists_and_spawns ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out
```

**Result: D1 — PASS (unchanged).**

---

## 8. D2 Regression

```
running 1 test
test engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out
```

**Result: D2 — PASS (unchanged).**

---

## 9. D3 Regression

```
running 1 test
test engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out
```

**Result: D3 — PASS (unchanged).**

Also individually re-run: `b2_*` (3/3), `b3_*` (1/1), `b4_*` (1/1), `b5_*` (1/1), `b6_*` (2/2), `b14_*` (2/2) — all pass, unaffected by this fix.

---

## 10. Full Test Suite

```
running 33 tests
test engine_tests::b2_recovery_of_all_passed_run_yields_completed_not_failed ... ok
test engine_tests::b2_recovery_marks_orphaned_tasks_and_preserves_terminal_ones ... ok
test engine_tests::b2_recovery_of_run_with_product_failure_yields_failed ... ok
test engine_tests::b4_started_at_reflects_real_execution_start_under_concurrency_1 ... ok
test engine_tests::b5_hung_executor_times_out_releases_permit_and_run_continues ... ok
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::d1_successful_approval_persists_and_spawns ... ok
test engine_tests::d1_successful_rejection_persists_and_mutates_state ... ok
test engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart ... ok
test engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery ... ok
test engine_tests::d4_live_run_with_blocked_task_matches_recovery_derived_state ... ok
test engine_tests::d4_live_run_with_failed_task_matches_recovery_derived_state ... ok
test engine_tests::d4_live_run_with_timeout_task_matches_recovery_derived_state ... ok
test executor::tests::safe_param_allows_empty_default_for_optional_grep ... ok
test executor::tests::safe_param_rejects_unsafe_value_from_params ... ok
test executor::tests::sanitize_token_accepts_realistic_params ... ok
test executor::tests::sanitize_token_rejects_shell_metacharacters_and_traversal ... ok
test intelligence::tests::classify_failure_recognizes_common_categories ... ok
test intelligence::tests::normalize_signature_collapses_numbers_and_hex_ids ... ok
test policy::tests::concurrency_over_max_is_denied_even_if_also_gated ... ok
test policy::tests::maestro_requires_approval_by_default ... ok
test policy::tests::unrestricted_executor_is_allowed ... ok
test scheduler::tests::dependent_tasks_become_blocked_when_prerequisite_fails ... ok
test scheduler::tests::srd_25_continue_on_failure_with_10_independent_tasks ... ok
test scheduler::tests::validate_plan_rejects_cycles_and_unknown_prerequisites ... ok
test store_tests::b1_concurrent_writes_do_not_silently_disappear ... ok
test store_tests::b1_wal_and_busy_timeout_are_configured ... ok
test store_tests::b14_bug_draft_project_is_resolvable_for_authorization ... ok
test store_tests::b14_bug_drafts_are_project_scoped ... ok
test store_tests::b3_task_history_does_not_cross_projects ... ok
test store_tests::b6_insert_run_handles_large_plans ... ok
test store_tests::b6_insert_run_rolls_back_completely_on_failure ... ok

test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.35s
```

**33/33 pass** (the pre-D4 30, plus this session's 3 new D4 tests). No unrelated test failed.

---

## 11. Build

```
$ cargo check --all-targets
    Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.10s
```

**0 errors, 0 warnings.**

---

## 12. Scope Verification

| Constraint | Status |
|---|---|
| Frontend (`apps/web/`) untouched by this session | ✅ Confirmed — no `apps/web/` file was opened, read, or written during D4 |
| `forentend/` untouched | ✅ Confirmed — not referenced |
| `AIQA_API_CONTRACT_v1.0.md` untouched | ✅ Confirmed — read-only inspection in Step 2 (grep/sed), never opened with a write tool |
| `AIQA_ACCEPTANCE_MATRIX_v1.0.md` untouched | ✅ Confirmed — not opened at all this session |
| No unrelated persistence issues touched | ✅ Confirmed — `set_task_status`, `set_run_finished`, `list_incomplete_runs` (D1/D2/D3's fixes) were not modified |
| No authentication/RBAC changes | ✅ Confirmed |
| No second copy of run-state logic created | ✅ Confirmed — `derive_run_state` remains the single definition; the live path now calls it instead of re-implementing it |
| No unrelated refactoring | ✅ Confirmed — one call-site change plus removal of one now-redundant duplicate snapshot collection, both inside the same finalisation block |
| D1 intact | ✅ Confirmed (§7) |
| D2 intact | ✅ Confirmed (§8) |
| D3 intact | ✅ Confirmed (§9) |
| D5 untouched | ✅ Confirmed — no broader test-coverage initiative beyond the three D4-scoped tests required by this task |
| Phase 2 untouched | ✅ Confirmed — no `/api/v1` prefix, project-scoping, migration, or error-envelope work was started |

Files changed this session: `services/orchestrator/src/engine.rs` (one finalisation block), `services/orchestrator/src/engine_tests.rs` (three new tests appended), and this report.

---

## 13. Remaining Phase 1 Issue (not addressed — listed only)

- **D5** — Test coverage for the persistence/recovery layer now includes D1 (4 tests), D2 (1), D3 (1), and D4 (3), but no systematic audit of remaining gaps (e.g., fault injection on `insert_evidence`, `set_run_state`, `set_run_started`, or concurrent-recovery races) was performed. Not fixed here.

---

## Final Status

# D4 — PASS
