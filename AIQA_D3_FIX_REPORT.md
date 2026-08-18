# AIQA — Phase 1 Remediation Report — D3 ONLY

**Document:** `AIQA_D3_FIX_REPORT.md`
**Date:** 2026-08-17
**Scope:** Remediation of Defect D3 in `services/orchestrator`
**Final Status:** **D3 — PASS**

---

## 1. D3 Root Cause

`recover_incomplete_runs` (`engine.rs:285`) only ever considered run ids returned by `Store::list_incomplete_runs` (`store.rs:441`, before this fix):

```sql
SELECT id FROM runs WHERE state IN ('CREATED','SCHEDULED','RUNNING','PENDING_APPROVAL')
```

This selects by the **run row's own state**, not by whether any of its **tasks** are still non-terminal.

A mid-run durable-write failure can put those two things out of sync. In `spawn_run` (`engine.rs:352`), when a task reaches a terminal outcome the in-memory scheduler (`engine.complete_task`) transitions it immediately; the durable write (`store.set_task_status(..., false, true)` at `engine.rs:485`) happens *after*, as a separate step. If that specific write fails, `note_persistence_failure` is recorded (`engine.rs:489`) but the loop continues — by design, so one bad write doesn't stall the rest of the run. At finalisation, `persistence_error().await` being `Some(..)` forces `final_state = RunState::Failed` and that is written durably via `set_run_finished` (`engine.rs:538`, fixed under D2 to also persist `policy_notes`).

The consequence: the **run** row correctly and durably becomes `FAILED`, while the **task** row whose write failed never left `RUNNING`. Once the run row is `FAILED`, it no longer matches `list_incomplete_runs`'s state list, so `recover_incomplete_runs` never looks at its tasks again — on this restart or any future one. The task is orphaned permanently.

Answering the brief's specific questions:

| Question | Answer |
|---|---|
| Why can a task remain `RUNNING`? | Its terminal-status `UPDATE` (`store.rs` `set_task_status`, the `set_finished` branch) failed, and there is no retry or later reconciliation for that specific write. |
| Why are events missing? | A separate, already-understood tradeoff (`engine.rs:99-107` doc comment): `emit` appends to the in-memory event log and broadcasts before attempting the durable `insert_event`; if that insert fails, the event stays visible in memory (and to any live WebSocket subscriber) but is missing from `run_events` on disk. Not part of D3's fix — see §11. |
| Why does recovery fail to repair the task? | `list_incomplete_runs` is scoped by the run's own `state` column. A `FAILED` run is terminal by that column even though a task beneath it is not. |
| Does terminal run state prevent recovery? | Yes — that is the entire defect. Terminal run state was being used as a proxy for "every task under it is also settled," which the B1/D2 escalation path (correctly) breaks. |
| Which persistence operation fails? | `UPDATE tasks SET status = ?, reason = ?, finished_at = ? WHERE run_id = ? AND id = ?` (`store.rs`, `set_task_status`, `set_finished` branch) for the specific task whose write lost the race/lock. |
| Which state transition happens in memory but not durably? | The task's `Running → <terminal>` transition. `engine.complete_task` applies it in the pure in-memory scheduler unconditionally; the durable mirror of that same transition is the write that failed. |

---

## 2. Reproduction

### 2.1 Live, against the running binary (real fault, real process, real restart)

Using the production `infra/data/orchestrator.sqlite3` (preserved before the test and restored after):

1. Submitted a 30-task run (`concurrency=2`, ~1.2s/task).
2. 2.5s in, opened a **second, independent SQLite connection** to the same file and issued `BEGIN EXCLUSIVE`, holding it for 20 seconds — 4× the orchestrator's configured `busy_timeout(5s)`. This is a genuine `SQLITE_BUSY`/`database is locked` condition, not a simulated one.
3. Released the lock, waited for the run to settle, then queried both the HTTP API and the database file directly.
4. Restarted the orchestrator process and re-inspected.

This is the exact method used in the prior independent Phase 1 verification, replayed against the current codebase (including the already-applied D1/D2 fixes) to confirm the defect was still live before touching any source.

### 2.2 Regression test, against the persistence layer directly (no bypass)

`engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery` drives a real `spawn_run` over a real on-disk SQLite file, with a `CREATE TRIGGER ... BEFORE UPDATE ON tasks WHEN NEW.id = 'target' AND NEW.finished_at IS NOT NULL BEGIN SELECT RAISE(ABORT, ...) END` that makes SQLite itself reject exactly the one `UPDATE` that finishes task `target` — not a mock, not a manually-inserted row. The task's *start* update (which never sets `finished_at`) is untouched, so `target` genuinely begins executing.

---

## 3. Before Fix

### 3.1 Live reproduction (§2.1), captured directly from the database

| Point in time | `runs.state` | `runs.policy_notes` | task `t5` status | Stuck `RUNNING` |
|---|---|---|---|---|
| Before failure | `RUNNING` | — | (in progress) | — |
| Immediately after lock release | `FAILED` | *(durable — D2 fix)* "durable persistence failed during this run; recorded state may be incomplete — first failure: `set_task_status(final, t5)`: … database is locked" | `RUNNING` | **t5** |
| After orchestrator restart | `FAILED` (unchanged) | unchanged (unchanged) | `RUNNING` (unchanged) | **t5 — still stuck** |

Orchestrator boot log after restart: **no** "recovered N run(s)" line — the run was never even considered.

### 3.2 Regression test, before the fix (recorded failure)

```
running 1 test
test engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery ... FAILED

thread 'engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery' panicked at src\engine_tests.rs:762:5:
assertion `left == right` failed: recovery must find this run even though its OWN row is already terminal (FAILED), because one of its tasks is still stuck RUNNING
  left: 0
 right: 1

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 29 filtered out
```

`recover_incomplete_runs` returned `0` — it did not even see the run, exactly matching the live reproduction.

---

## 4. Fix

**File:** `services/orchestrator/src/store.rs`, function `list_incomplete_runs` (the only change).

```sql
-- before
SELECT id FROM runs WHERE state IN ('CREATED','SCHEDULED','RUNNING','PENDING_APPROVAL')

-- after
SELECT id FROM runs WHERE state IN ('CREATED','SCHEDULED','RUNNING','PENDING_APPROVAL')
UNION
SELECT DISTINCT run_id AS id FROM tasks WHERE status IN ('QUEUED','RUNNING')
```

**Why this is the whole fix.** `recover_incomplete_runs` (`engine.rs:285`) already, for every run id it is given: lists that run's tasks, marks any still `QUEUED`/`RUNNING` as `INFRA_ERROR` with an explanatory reason, re-reads the tasks, calls the existing `derive_run_state` (B2) over that fresh snapshot, and durably finalises via `set_run_finished` (which — since D2 — preserves any existing `policy_notes` via `COALESCE`). That logic is correct and already tested (B2, D2) for the "run itself non-terminal" case. D3's gap was purely in *which run ids reach that logic* — a run with an orphaned task but an already-terminal row was never being handed to it. Broadening the query is therefore sufficient; the reconciliation, derivation, and persistence logic needed no changes.

No new `TaskStatus` or `RunState` variant was introduced (`INFRA_ERROR` and `derive_run_state`'s existing rules are reused as-is). No function signature changed. No API route, request shape, or response shape changed. No migration was needed — both columns queried (`runs.state`, `tasks.status`) already exist.

**File:** `services/orchestrator/src/engine_tests.rs` — added one regression test (§6). No existing test was modified.

---

## 5. After Fix

### 5.1 Live reproduction, replayed against the fixed binary

Exact same method as §2.1, same production database, fresh EXCLUSIVE-lock fault:

| Point in time | `runs.state` | task statuses | Stuck `RUNNING` |
|---|---|---|---|
| Pre-restart (unchanged from before — D3 does not touch live finalisation) | `FAILED` | `{PASSED: 29, RUNNING: 1}` | t5 |
| **After restart** | **`PARTIAL`** | **`{PASSED: 29, INFRA_ERROR: 1}`** | **none** |

Boot log: `WARN recovered 1 run(s) left non-terminal by a prior unclean shutdown` — the run is now found. `policy_notes` (the original "durable persistence failed…" diagnostic) was confirmed still present after recovery — D2's guarantee is not disturbed, because recovery passes `None` for the notes parameter and `set_run_finished`'s `COALESCE(?, policy_notes)` (D2) keeps the existing value.

### 5.2 Regression test, after the fix

```
running 1 test
test engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.21s
```

---

## 6. D3 Regression Test

**Name:** `engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery`

**What it proves**, in one continuous run against a real on-disk SQLite file:

1. A real 2-task run executes under `spawn_run`; a SQLite trigger makes the terminal `UPDATE` for task `target` genuinely fail (`RAISE(ABORT, ...)`), while task `ok` completes normally.
2. In memory, `target` reached `Passed` — proving the memory/disk divergence is real, not assumed.
3. Directly against the database (no in-memory shortcut): the run is durably `FAILED` with a durable `policy_notes` diagnostic (B1/D2, unaffected by this fix), and `target` is still `RUNNING` — the exact defect.
4. The trigger is dropped (the live fault was transient — a lock, not a permanent condition — and a permanent trigger would also block recovery's own repair write).
5. The connection pool is closed and a **fresh** `Store::connect` is opened against the **same file**, genuinely simulating a process restart (not a reused connection).
6. `recover_incomplete_runs` is invoked on that fresh connection.
7. Assertions: `recover_incomplete_runs` returns `1` (the run is found); `target` is no longer `Running` and is specifically `InfraError`; `ok` (already correct) is untouched; the run's derived state is `Partial` (one passed + one infra-error, via the existing `derive_run_state` rules); the original `policy_notes` diagnostic is byte-identical to before this second recovery pass; recovery's own reconciliation is itself durably recorded as a `RunCompleted` event with no duplicate/renumbered event sequence introduced.

The test was run and observed to **fail** against the pre-fix code (§3.2) before the fix was written, and to **pass** after (§5.2), satisfying the "must fail against old behavior, pass after fix" requirement.

---

## 7. D1 Regression

```
running 4 tests
test engine_tests::d1_successful_rejection_persists_and_mutates_state ... ok
test engine_tests::d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory ... ok
test engine_tests::d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state ... ok
test engine_tests::d1_successful_approval_persists_and_spawns ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 0.15s
```

**Result: D1 — PASS (unchanged).**

---

## 8. D2 Regression

```
running 1 test
test engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.23s
```

**Result: D2 — PASS (unchanged).**

---

## 9. Full Test Suite

```
running 30 tests
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

test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.17s
```

No unrelated test failed. **30/30 pass** (the pre-existing 24, plus the 4 D1 + 1 D2 tests already present from earlier remediation, plus this session's 1 new D3 test).

---

## 10. Build

```
$ cargo check --all-targets
    Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.05s
```

**0 errors, 0 warnings.**

---

## 11. Scope Verification

| Constraint | Status |
|---|---|
| Frontend (`apps/web/`) untouched by this session | ✅ Confirmed — `git diff --stat -- apps/web/` shows no changes from this D3 session (any pre-existing modifications predate this task) |
| `forentend/` untouched | ✅ Confirmed — not referenced or opened |
| `AIQA_API_CONTRACT_v1.0.md` untouched | ✅ Confirmed — file not opened or modified |
| `AIQA_ACCEPTANCE_MATRIX_v1.0.md` untouched | ✅ Confirmed — file not opened or modified |
| API routes / request / response shapes unchanged | ✅ Confirmed — no `api.rs` handler signature or route changed; `list_incomplete_runs`'s return type (`Vec<Uuid>`) is unchanged and it is not part of any HTTP-facing contract |
| No new statuses invented | ✅ Confirmed — reuses existing `TaskStatus::InfraError` and the existing `derive_run_state` rules verbatim |
| No unrelated refactoring | ✅ Confirmed — one query change in `store.rs`, one new test in `engine_tests.rs`; nothing else touched |
| D4 untouched | ✅ Confirmed — live finalisation (`engine.rs:499-506`) was not modified; D4 is unaffected by this fix (it is entirely about the *live* completion path, which D3 does not touch) |
| D5 untouched | ✅ Confirmed — no broader test-coverage initiative was undertaken beyond the one D3-scoped test required by this task |
| Phase 2 untouched | ✅ Confirmed — no `/api/v1` prefix, project-scoping, migration, or error-envelope work was started |

Files changed this session: `services/orchestrator/src/store.rs` (1 function), `services/orchestrator/src/engine_tests.rs` (1 new test appended), and this report.

---

## 12. Remaining Phase 1 Issues (not addressed — listed only)

- **D4** — Live finalisation (`engine.rs:499-506`) still uses its own inline `InfraError`-only check instead of `derive_run_state`, so a live (not-yet-restarted) run containing `FAILED`, `BLOCKED`, or `TIMEOUT` tasks can report `COMPLETED` even though recovery would derive `FAILED`/`PARTIAL` for the identical task set. Not fixed here — out of scope for D3, and changes externally visible run states, so it needs its own sign-off.
- **D5** — Test coverage for the persistence/recovery layer is now broader than before this session (D1: 4 tests, D2: 1 test, D3: 1 test), but no systematic audit of remaining gaps (e.g., fault injection on `insert_evidence`, `set_run_state`, `set_run_started`, or concurrent recovery races) was performed. Not fixed here.

---

## Final Status

# D3 — PASS
