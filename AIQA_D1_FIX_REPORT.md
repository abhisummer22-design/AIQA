# AIQA — Phase 1 Remediation Report — D1 ONLY

**Document:** `AIQA_D1_FIX_REPORT.md`  
**Date:** 2026-08-17  
**Scope:** Remediation of Defect D1 in `services/orchestrator`  
**Final Status:** **D1 — PASS**  

---

## 1. D1 Root Cause

In `services/orchestrator/src/engine.rs`, the governance functions `approve_and_spawn` and `reject_run` invoked durable store operations (`store.approve_run` and `store.reject_run`) within `if let Err(e) = ...` blocks that only logged the error via `tracing::error!` and then swallowed the failure:

1. **Approval Path (`engine.rs`):** If `store.approve_run` failed (e.g. database lock contention, closed connection pool, disk failure), the error was logged and ignored. The function proceeded to mutate in-memory state (`approved_by`, `approved_at`), called `spawn_run` (which dispatched background tasks), and `api.rs` audited the approval as `"ok"` and returned `HTTP 200 OK` with an approved summary. The database remained in `PENDING_APPROVAL` with `approved_by = NULL`, creating an authorization gap where a run executed in memory without durable approval on disk.
2. **Rejection Path (`engine.rs`):** If `store.reject_run` failed, the error was logged and ignored. The function proceeded to mutate in-memory state (`state = RunState::Rejected`, `finished_at`, `approved_by`, `policy_notes`), emitted a `RunCompleted` event, and `api.rs` returned `HTTP 200 OK`. The database remained in `PENDING_APPROVAL`, leaving the durable store in conflict with the in-memory state.

Neither site invoked `record.note_persistence_failure()`, neither propagated the error to the calling API handler, and neither prevented erroneous in-memory state transitions.

---

## 2. Exact Source Locations

- `services/orchestrator/src/engine.rs`: lines 231–233 (`approve_and_spawn`) & lines 244–246 (`reject_run`)
- `services/orchestrator/src/api.rs`: lines 298–307 (`approve_run` handler) & lines 340–342 (`reject_run` handler)

---

## 3. Failure Reproduction

To reproduce the defect:
1. Seeded a run in `PENDING_APPROVAL` state.
2. Closed the database connection pool (`store.pool.close().await`) to simulate an unrecoverable database/persistence failure condition.
3. Invoked `approve_and_spawn` and `reject_run`.

**Observed behavior before fix:**
- `approve_and_spawn` returned `()` (unit) successfully.
- `record.approved_by` was mutated to `Some("approver_alice")`.
- `record.approved_at` was mutated to `Some(timestamp)`.
- Background task execution was spawned.
- `record.persistence_error().await` was `None` (failure was completely swallowed).
- `reject_run` returned `()` (unit) successfully.
- `record.state` was mutated to `RunState::Rejected`.
- `record.policy_notes` was mutated.
- `RunCompleted` event was broadcast and emitted.
- `record.persistence_error().await` was `None`.

---

## 4. Files Changed

Only files strictly required for D1 in `services/orchestrator` were modified:

1. `services/orchestrator/src/engine.rs`:
   - Updated `approve_and_spawn` signature to return `anyhow::Result<()>`.
   - On `store.approve_run` error: records persistence failure via `record.note_persistence_failure("approve_run", &e).await` and returns `Err(e)` immediately without mutating in-memory `approved_by`/`approved_at` or spawning background execution.
   - Updated `reject_run` signature to return `anyhow::Result<()>`.
   - On `store.reject_run` error: records persistence failure via `record.note_persistence_failure("reject_run", &e).await` and returns `Err(e)` immediately without mutating in-memory state or emitting `RunCompleted`.

2. `services/orchestrator/src/api.rs`:
   - In `approve_run` handler: appended `.map_err(to_500)?` to `engine::approve_and_spawn(...).await`, causing API to return `500 Internal Server Error` on persistence failure and prevent fraudulent `"ok"` audit log entries.
   - In `reject_run` handler: appended `.map_err(to_500)?` to `engine::reject_run(...).await`, causing API to return `500 Internal Server Error` on persistence failure.

3. `services/orchestrator/src/engine_tests.rs`:
   - Added focused regression tests for approval and rejection persistence failures as well as valid persistence paths.

---

## 5. Behavior Comparison

| Scenario | Behavior Before Fix | Behavior After Fix |
|---|---|---|
| `store.approve_run` fails | Swallowed with `error!`; in-memory marked approved; tasks spawned; API returns `200 OK` | Recorded via `note_persistence_failure`; returns `Err`; in-memory stays `PENDING_APPROVAL`; no tasks spawned; API returns `500 Internal Server Error` |
| `store.reject_run` fails | Swallowed with `error!`; in-memory marked `REJECTED`; `RunCompleted` emitted; API returns `200 OK` | Recorded via `note_persistence_failure`; returns `Err`; in-memory stays `PENDING_APPROVAL`; no `RunCompleted` event; API returns `500 Internal Server Error` |
| Successful approval | Updates DB, sets in-memory state, spawns run, returns `200 OK` | Updates DB, sets in-memory state, spawns run, returns `200 OK` (preserved) |
| Successful rejection | Updates DB, sets in-memory state, emits `RunCompleted`, returns `200 OK` | Updates DB, sets in-memory state, emits `RunCompleted`, returns `200 OK` (preserved) |

---

## 6. Regression Tests Added

Added 4 dedicated test cases in `services/orchestrator/src/engine_tests.rs`:

1. `d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory`:
   - Closes store pool to inject persistence failure.
   - Calls `approve_and_spawn`.
   - Asserts `res.is_err()`, `record.persistence_error()` is `Some("approve_run: ...")`, `approved_by` is `None`, `approved_at` is `None`, `state` is `PendingApproval`, and tasks remain `Queued` (not spawned).
2. `d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state`:
   - Closes store pool to inject persistence failure.
   - Calls `reject_run`.
   - Asserts `res.is_err()`, `record.persistence_error()` is `Some("reject_run: ...")`, `state` remains `PendingApproval`, `finished_at` is `None`, `approved_by` is `None`, `policy_notes` is unchanged, and event log length is `0`.
3. `d1_successful_approval_persists_and_spawns`:
   - Validates that the happy path for `approve_and_spawn` persists to SQLite and runs tasks to completion.
4. `d1_successful_rejection_persists_and_mutates_state`:
   - Validates that the happy path for `reject_run` persists `state = REJECTED` and rejection details to SQLite.

---

## 7. Test and Validation Results

### `cargo check --all-targets`
```
Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.42s
Result: 0 errors, 0 warnings
```

### Regression Tests for D1
```
running 4 tests
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::d1_successful_rejection_persists_and_mutates_state ... ok
test engine_tests::d1_successful_approval_persists_and_spawns ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 24 filtered out; finished in 0.15s
```

### Full Backend Test Suite
```
running 28 tests
test engine_tests::b2_recovery_of_all_passed_run_yields_completed_not_failed ... ok
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::b2_recovery_of_run_with_product_failure_yields_failed ... ok
test engine_tests::b2_recovery_marks_orphaned_tasks_and_preserves_terminal_ones ... ok
test executor::tests::safe_param_allows_empty_default_for_optional_grep ... ok
test executor::tests::safe_param_rejects_unsafe_value_from_params ... ok
test executor::tests::sanitize_token_accepts_realistic_params ... ok
test executor::tests::sanitize_token_rejects_shell_metacharacters_and_traversal ... ok
test intelligence::tests::classify_failure_recognizes_common_categories ... ok
test policy::tests::concurrency_over_max_is_denied_even_if_also_gated ... ok
test policy::tests::maestro_requires_approval_by_default ... ok
test policy::tests::unrestricted_executor_is_allowed ... ok
test scheduler::tests::dependent_tasks_become_blocked_when_prerequisite_fails ... ok
test scheduler::tests::srd_25_continue_on_failure_with_10_independent_tasks ... ok
test scheduler::tests::validate_plan_rejects_cycles_and_unknown_prerequisites ... ok
test intelligence::tests::normalize_signature_collapses_numbers_and_hex_ids ... ok
test engine_tests::d1_successful_rejection_persists_and_mutates_state ... ok
test engine_tests::d1_successful_approval_persists_and_spawns ... ok
test store_tests::b14_bug_draft_project_is_resolvable_for_authorization ... ok
test store_tests::b1_wal_and_busy_timeout_are_configured ... ok
test store_tests::b14_bug_drafts_are_project_scoped ... ok
test store_tests::b3_task_history_does_not_cross_projects ... ok
test store_tests::b6_insert_run_rolls_back_completely_on_failure ... ok
test store_tests::b1_concurrent_writes_do_not_silently_disappear ... ok
test store_tests::b6_insert_run_handles_large_plans ... ok
test engine_tests::b4_started_at_reflects_real_execution_start_under_concurrency_1 ... ok
test engine_tests::b5_hung_executor_times_out_releases_permit_and_run_continues ... ok

test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.19s
```

---

## 8. Scope Verification

- **Frontend code modified:** None (`forentend/` and `apps/web/` untouched during this task).
- **API Contract & Acceptance Matrix:** Unmodified (`AIQA_API_CONTRACT_v1.0.md`, `AIQA_ACCEPTANCE_MATRIX_v1.0.md` intact).
- **API Route Paths & Schemas:** Unmodified.
- **Unrelated Refactoring:** None performed.
- **Defects addressed:** D1 ONLY.

---

## 9. Remaining Phase 1 Defects (Pending Separate Remediation)

- **D2:** Failure-reason durability (`policy_notes` not persisted on run degradation).
- **D3:** Orphaned `RUNNING` tasks and event-sequence gaps post write-failure reconciliation.
- **D4:** Live finalisation state divergence from recovery.
- **D5:** Broader persistence test coverage for run-level fault injection.

---

## Final Status

# D1 — PASS
