# AIQA — Phase 1 Remediation Report — D2 ONLY

**Document:** `AIQA_D2_FIX_REPORT.md`  
**Date:** 2026-08-17  
**Scope:** Remediation of Defect D2 in `services/orchestrator`  
**Final Status:** **D2 — PASS**  

---

## 1. D2 Root Cause

When a persistence failure was recorded via `RunRecord::note_persistence_failure()`, the engine marked the run `FAILED` in memory and formatted an auditable diagnostic note into `record.policy_notes` (`engine.rs`).

However, the finalisation writer `set_run_finished` (`store.rs:192`) executed:
```sql
UPDATE runs SET state = ?, finished_at = ? WHERE id = ?
```

The `policy_notes` column in the SQLite `runs` table was not updated. As a consequence:
- While the orchestrator process lived, `GET /runs/:id` returned `policy_notes` from memory.
- In SQLite, `runs.policy_notes` was `NULL`.
- After an orchestrator process restart, `get_run_summary` loaded `runs.policy_notes` as `NULL`, permanently losing the failure reason and leaving the run marked `FAILED` with no recorded explanation.

---

## 2. Exact Source Locations

- `services/orchestrator/src/store.rs`: lines 189–208 (`set_run_finished`)
- `services/orchestrator/src/engine.rs`: lines 312 (`recover_incomplete_runs`) & lines 539–546 (`spawn_run` finalisation)

---

## 3. Reproduction Evidence

### Reproduction Method
1. Seeded a run in SQLite.
2. Injected a real SQLite write error on event sequence 1 via trigger:
   ```sql
   CREATE TRIGGER fail_event_persistence BEFORE INSERT ON run_events WHEN NEW.sequence = 1
   BEGIN SELECT RAISE(ABORT, 'injected disk write failure on event sequence 1'); END;
   ```
3. Executed the run to completion.

### Database State Before Fix
- **In-memory summary:** `state = FAILED`, `policy_notes = "durable persistence failed during this run; recorded state may be incomplete — first failure: insert_event(...): injected disk write failure on event sequence 1"`
- **Database row (`runs`):** `state = 'FAILED'`, `finished_at = <ts>`, `policy_notes = NULL`
- **Database row after process restart:** `state = 'FAILED'`, `policy_notes = NULL` (diagnostic lost)

### Database State After Fix
- **In-memory summary:** `state = FAILED`, `policy_notes = "durable persistence failed during this run; recorded state may be incomplete — first failure: insert_event(...): ..."`
- **Database row (`runs`):** `state = 'FAILED'`, `finished_at = <ts>`, `policy_notes = "durable persistence failed during this run; recorded state may be incomplete — first failure: insert_event(...): ..."`
- **Database row after process restart & recovery:** `state = 'FAILED'`, `policy_notes = "durable persistence failed during this run; recorded state may be incomplete — first failure: insert_event(...): ..."` (exact match preserved)

---

## 4. Files Changed

1. `services/orchestrator/src/store.rs`:
   - Updated `set_run_finished` to take `policy_notes: Option<&str>`.
   - Updated query to:
     ```sql
     UPDATE runs SET state = ?, finished_at = ?, policy_notes = COALESCE(?, policy_notes) WHERE id = ?
     ```
2. `services/orchestrator/src/engine.rs`:
   - In `spawn_run`: read `policy_notes` from `record.policy_notes` and passed `policy_notes_to_persist.as_deref()` into `store.set_run_finished`.
   - In `recover_incomplete_runs`: passed `None` for `policy_notes` so `COALESCE(?, policy_notes)` preserves any existing diagnostic.
3. `services/orchestrator/src/engine_tests.rs`:
   - Added `d2_persistence_failure_reason_is_durable_and_survives_restart`.

---

## 5. Schema & Migration Changes

None required. The `runs` table already contained `policy_notes TEXT` (added in migration `0003_auth_governance_and_audit.sql`).

---

## 6. Regression Test Details

Test: `engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart`
1. Creates a real SQLite test database file on disk.
2. Injects an SQLite trigger write failure on event insertion.
3. Drives run to completion.
4. Asserts in-memory `record.summary().policy_notes` contains the failure message.
5. Queries the database directly via `store.get_run_summary()` and asserts `db_summary.policy_notes == memory_reason`.
6. Closes the connection pool, reconnects fresh via `Store::connect` to simulate orchestrator process restart, and runs `recover_incomplete_runs`.
7. Asserts `recovered_summary.policy_notes == memory_reason == db_summary.policy_notes`.

---

## 7. Test Results

### D2 Regression Test
```
running 1 test
test engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.22s
```

### D1 Regression Tests
```
running 4 tests
test engine_tests::d1_successful_rejection_persists_and_mutates_state ... ok
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::d1_successful_approval_persists_and_spawns ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.13s
```

### `cargo check --all-targets`
```
Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.93s
Result: 0 errors, 0 warnings
```

### Full Backend Test Suite
```
running 29 tests
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::b2_recovery_marks_orphaned_tasks_and_preserves_terminal_ones ... ok
test engine_tests::b2_recovery_of_run_with_product_failure_yields_failed ... ok
test engine_tests::b2_recovery_of_all_passed_run_yields_completed_not_failed ... ok
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
test store_tests::b14_bug_drafts_are_project_scoped ... ok
test store_tests::b14_bug_draft_project_is_resolvable_for_authorization ... ok
test store_tests::b1_wal_and_busy_timeout_are_configured ... ok
test store_tests::b3_task_history_does_not_cross_projects ... ok
test store_tests::b6_insert_run_rolls_back_completely_on_failure ... ok
test store_tests::b6_insert_run_handles_large_plans ... ok
test engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart ... ok
test store_tests::b1_concurrent_writes_do_not_silently_disappear ... ok
test engine_tests::b4_started_at_reflects_real_execution_start_under_concurrency_1 ... ok
test engine_tests::b5_hung_executor_times_out_releases_permit_and_run_continues ... ok

test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.24s
```

---

## 8. Scope Verification

- **Frontend code modified:** None (`forentend/` and `apps/web/` untouched).
- **API Contract & Acceptance Matrix:** Unmodified (`AIQA_API_CONTRACT_v1.0.md`, `AIQA_ACCEPTANCE_MATRIX_v1.0.md` untouched).
- **API Route Paths & Schemas:** Unmodified.
- **Unrelated Refactoring:** None.
- **Defects addressed:** D2 ONLY.

---

## 9. Remaining Phase 1 Defects (Pending Separate Remediation)

- **D3:** Orphaned `RUNNING` tasks and event-sequence gaps post write-failure reconciliation.
- **D4:** Live finalisation state divergence from recovery.
- **D5:** Broader persistence test coverage for run-level fault injection.

---

## Final Status

# D2 — PASS
