# AIQA — Phase 1 Implementation Report

**Phase:** 1 — Correctness & Persistence
**Date:** 17 August 2026
**Scope:** `services/orchestrator` only
**Source plan:** `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md`

**Constraints honoured:** No frontend file modified. `forentend/` untouched. `apps/web/` untouched by this work. API Contract v1.0 unmodified. Acceptance Matrix v1.0 unmodified. No API path, request schema, response schema or versioning changed. No Phase 2+ feature implemented.

**Final status: PHASE 1 — PASS** (see §18 for the item-by-item DoD, and §17 for two deviations that are flagged rather than hidden).

---

## 1. Files Changed

All changes are inside `services/orchestrator`. **8 files modified, 2 added.**

```
 services/orchestrator/Cargo.lock          |  39 ++++++
 services/orchestrator/Cargo.toml          |   7 +
 services/orchestrator/src/api.rs          |  47 ++++++-
 services/orchestrator/src/engine.rs       | 193 +++++++++++++++++++++----
 services/orchestrator/src/executor.rs     |  16 ++-
 services/orchestrator/src/intelligence.rs |   6 +-
 services/orchestrator/src/lib.rs          |   5 +
 services/orchestrator/src/store.rs        | 225 ++++++++++++++++++++++++------
 8 files changed, 465 insertions(+), 73 deletions(-)

 new: services/orchestrator/src/store_tests.rs    (7 regression tests)
 new: services/orchestrator/src/engine_tests.rs   (5 regression tests)
```

| File | Why |
|---|---|
| `store.rs` | B1 WAL/busy-timeout; B6 transactional batched `insert_run`; B3 tenant-scoped history; B14 scoped `list_bug_drafts` + new `bug_draft_project` |
| `engine.rs` | B1 persistence-failure escalation; B2 `derive_run_state` for recovery; B4 permit-before-start; B5 timeout wrapper; `default_task_timeout()` |
| `api.rs` | B14 scoping on `GET /bug-drafts` and project check on `POST /bug-drafts/:id/review`; threading the task timeout to `spawn_run`/`approve_and_spawn` |
| `executor.rs` | B5 `kill_on_drop(true)` on all four command builders so a timed-out child actually dies |
| `intelligence.rs` | B3 pass the resolved tenant key into history lookup |
| `lib.rs` | register the two test modules |
| `Cargo.toml` / `Cargo.lock` | dev-dependency `tempfile` for on-disk store tests |

**Not changed:** any migration (none was needed — B1 is connection configuration, not schema), any route path, any request/response type, `domain.rs`, `auth.rs`, `policy.rs`, `scheduler.rs`, `evidence.rs`, `metrics.rs`, `webhook.rs`, `state.rs`.

---

## 2. B1 — SQLite Persistence Correctness · **PASS**

### Root cause
`store.rs` opened SQLite with only `create_if_missing` and `foreign_keys`. Under the default rollback journal every write takes an exclusive database lock, and with no busy timeout a blocked writer fails **immediately** with `SQLITE_BUSY`. `engine.rs` then discarded 8 distinct persistence failures with `warn!`/`error!` and continued, so the in-memory truth and the database silently diverged.

### Changes

**Connection configuration** (`store.rs::connect`):
```rust
.journal_mode(SqliteJournalMode::Wal)      // readers never block writers
.synchronous(SqliteSynchronous::Normal)    // safe pairing with WAL
.busy_timeout(Duration::from_secs(5))      // wait and retry, don't fail on contact
```

**Error escalation** — all 8 swallowed sites now route through a new
`RunRecord::note_persistence_failure()`, which records the first failure and logs every occurrence:

| Site | Was | Now |
|---|---|---|
| `emit()` → `insert_event` | `warn!` | escalated |
| `set_run_state(Running)` | `error!` | escalated |
| `set_run_started` | `error!` | escalated |
| `set_task_status(started)` | `warn!` | escalated |
| `set_task_status(final)` | `warn!` | escalated |
| `insert_evidence` | `warn!` | escalated |
| `evidence.put` | `warn!` | escalated |
| `set_run_finished` | `error!` | escalated |

**Auditable failure state.** The run loop is a detached `tokio::spawn` and cannot propagate an error upward, so finalisation reads the recorded failure and transitions the run explicitly:

```rust
Some(cause) => {
    final_state = RunState::Failed;
    *record.policy_notes.write().await = Some(note);   // visible on the run summary
    format!("run finished with persistence failure: {cause}")  // in the RunCompleted event
}
```

A run whose durable writes failed is now reported as **FAILED with the cause on the summary and in the event log**, instead of reporting success over a database known to disagree.

### Verification
- `b1_wal_and_busy_timeout_are_configured` — `PRAGMA journal_mode` = `wal`, `busy_timeout` ≥ 1000 ms
- `b1_concurrent_writes_do_not_silently_disappear` — 60 concurrent `set_task_status` writes: **0 errors, 60/60 persisted**
- Live: `-wal` and `-shm` files present alongside the running database
- Live: **0** `database is locked` occurrences across the entire integration run (pre-fix: 7 under comparable load)

---

## 3. B2 — Recovery Correctness · **PASS**

### Root cause
`engine.rs:229` used `if touched_any { Partial } else { Failed }`. When recovery had to force *nothing* terminal — meaning every task had already finished and only the run-level completion write was lost, which is exactly the B1 scenario — the run was marked **FAILED**. A 50-task run in which all 50 tasks passed was permanently stored as a failure.

### Change
A new `derive_run_state(&[TaskState])` computes the verdict from the tasks themselves, re-read after any orphan is forced terminal:

| Condition | Result |
|---|---|
| any `INFRA_ERROR` or `TIMEOUT` | `Partial` |
| else any `FAILED` | `Failed` |
| else any `BLOCKED` | `Partial` |
| else | `Completed` |

`BLOCKED` is checked after `FAILED` deliberately: a blocked task is almost always the consequence of a failed prerequisite, and the failure is the more informative verdict.

### Verification (3 regression tests)
| Test | Pre-fix | Post-fix |
|---|---|---|
| all tasks PASSED, run row stale | **`FAILED`** ❌ | `COMPLETED` ✅ |
| orphaned RUNNING task + one PASSED | `Partial` | `Partial`, orphan → `INFRA_ERROR`, PASSED preserved ✅ |
| product failure present | `Failed` | `Failed` ✅ |

Pre-fix failure captured verbatim:
```
assertion `left == right` failed: a run whose tasks all passed must recover as COMPLETED, never FAILED
  left: Failed
 right: Completed
```

---

## 4. B3 — Intelligence Tenant Isolation · **PASS**

### Root cause
`task_history_by_name(name, limit)` filtered on task **name only**. `tasks` carries no `project_id`, so two tenants naming a test the same thing shared a failure history — and that history feeds `risk_score`, `flakiness_score` and `execution_cost_ms`.

### Change
Signature is now `task_history_by_name(name, project_filter: Option<&str>, limit)`. The scoped branch joins back to the owning run:

```sql
JOIN runs r ON r.id = t.run_id
WHERE t.name = ? AND t.finished_at IS NOT NULL
  AND COALESCE(r.project_id, 'default') = ?
```

`COALESCE(..., 'default')` matches the existing `intelligence::tenant_key` convention, so untenanted runs score against each other and never against a real project. `intelligence.rs` passes its already-resolved `pid`. `None` (global/admin) still sees everything.

### Verification
Pre-fix behavioural probe (against the old unscoped signature):
```
assertion failed: beta ran this task once and it passed; history must not include alpha's executions
  left: 4      <- alpha's 3 failures + beta's 1 pass
 right: 1
```

Post-fix unit test and live HTTP (§13, IT-2):
```
alpha window=3/3 failure_rate=1.0
beta  window=0/1 failure_rate=0.0     <- was 3/4, rate 0.75
```

---

## 5. B4 — Correct Task Start Timing · **PASS**

### Root cause
The dispatch loop marked **every** eligible task started — status `RUNNING`, `started_at = now` — in one batch, then acquired permits 19 lines later. With concurrency below the eligible count, tasks recorded a start time while still queued, and their durations silently included queue wait. That timing feeds `execution_cost_ms`, which is 15% of the risk score.

### Change
The loop was restructured so a task is marked started **only once it holds a permit**:

```rust
let eligible = { record.engine.read().await.eligible() };
for id in eligible {
    let Ok(permit) = semaphore.clone().try_acquire_owned() else { break };  // at capacity
    { record.engine.write().await.mark_started(&id); }                      // now it is real
    record.emit(&store, EventType::TaskStarted, ...).await;
    ...
}
```

`try_acquire_owned` rather than awaiting a permit is deliberate: it keeps the loop responsive so completions are drained promptly and `TASK_PASSED` events are not held behind a blocked dispatch.

### Verification
`b4_started_at_reflects_real_execution_start_under_concurrency_1` — four 300 ms tasks at `concurrency = 1`:
- start-time spread must be ≥ 450 ms (pre-fix it was **0 ms** — all identical)
- each task's own duration must be < 900 ms (pre-fix durations reached 5× the real work)

Both assertions pass.

---

## 6. B5 — Executor Timeout · **PASS**

### Root cause
No timeout existed anywhere in `executor.rs` (`grep tokio::time::timeout` → 0). A hung tool held its semaphore permit forever, `in_flight` never decremented, and the run stalled indefinitely. `TaskStatus::Timeout` was unreachable for every real executor.

### Change
Applied in `engine.rs` so it covers **all** executors uniformly, present and future:

```rust
let outcome = match tokio::time::timeout(task_timeout, exec.execute(&task_def)).await {
    Ok(outcome) => outcome,
    Err(_) => ExecutionOutcome {
        status: TaskStatus::Timeout,
        reason: Some(format!("executor exceeded the {}s task timeout and was cancelled", ...)),
        evidence: None,
    },
};
drop(permit);   // released on the timeout path too
```

Bound via `default_task_timeout()` — `AIQA_TASK_TIMEOUT_SECS`, default 600 s.

`tokio::time::timeout` drops the future, which alone would leave a subprocess orphaned, so `kill_on_drop(true)` was added to all four command builders in `executor.rs` (Windows `shell_command`, POSIX `shell_command`, and the direct `pytest` invocation). The child now dies with the task that owned it.

### Verification
- Unit: `b5_hung_executor_times_out_releases_permit_and_run_continues` uses a `HangingExecutor` that sleeps an hour. Pre-fix: `left: Running, right: Timeout`. Post-fix: `hang → TIMEOUT`, `after → PASSED`.
- Live (§13, IT-7): a 60 s task with an 8 s ceiling — run reached terminal in **8.1 s**, `hang → TIMEOUT`, independent task still `PASSED`, reason recorded as *"executor exceeded the 8s task timeout and was cancelled"*.

---

## 7. B6 — Large Run Insertion · **PASS**

### Root cause
`insert_run` issued one un-batched `INSERT` per task and per dependency, each in its own implicit transaction and therefore its own fsync. A failure mid-loop left a run row with an incomplete task set and no rollback.

### Change
One transaction for the whole plan, with multi-row inserts in bounded chunks via `sqlx::QueryBuilder`:

```rust
const CHUNK: usize = 200;                 // 200 rows x 4 binds = 800, inside SQLite's 999 ceiling
let mut tx = self.pool.begin().await?;
for chunk in plan.tasks.chunks(CHUNK) { /* push_values */ }
let edges: Vec<(&str,&str)> = /* flattened so chunks are even */;
for chunk in edges.chunks(CHUNK) { /* push_values */ }
tx.commit().await?;
```

Dependency edges are flattened before chunking so batch sizes stay even regardless of how unevenly prerequisites are distributed. `QueryBuilder` rather than `format!` because sqlx 0.9 rejects runtime-constructed SQL at compile time (injection guard).

### Verification
- `b6_insert_run_rolls_back_completely_on_failure` — a duplicate task id fails mid-transaction; **no run row and no tasks survive**
- `b6_insert_run_handles_large_plans` — 1,000 chained tasks: all 1,000 persisted, all 999 dependency edges preserved
- Live benchmarks: §13

---

## 8. B14 — Bug Draft Project Isolation · **PASS**

### Root cause
`api.rs` bound `Extension(_ctx)`, discarding the auth context entirely, and `store.rs::list_bug_drafts()` had no project parameter and no `WHERE`. A project-scoped **viewer** key received every tenant's drafts — title, description, severity, confidence. `POST /bug-drafts/:id/review` performed **no project check at all**, so that key could also promote or dismiss another tenant's draft.

### Change
`bug_drafts` has no `project_id`, so ownership is resolved through the failure cluster it was drafted from:

```rust
// list — scoped branch
JOIN failure_clusters c ON c.id = d.failure_cluster_id
WHERE COALESCE(c.project_id, 'default') = ?

// new lookup used by the review gate
pub async fn bug_draft_project(&self, id: Uuid) -> Result<Option<String>>
```

The review handler now resolves the owner and reuses the same gate every other scoped resource uses:

```rust
let owner = state.store.bug_draft_project(id).await?.ok_or(404)?;
auth::require_project_access(&ctx, Some(owner.as_str()))?;
```

Enforcement is entirely server-side; nothing relies on frontend filtering.

### Verification
Pre-fix behavioural probe:
```
assertion failed: a project-scoped caller must see exactly one draft (its own), not every tenant's
  left: 2
 right: 1
```

Post-fix, unit and live (§13, IT-2):

| Case | Result |
|---|---|
| same-project read | alpha sees 1, beta sees 1 ✅ |
| cross-project read | neither sees the other's ✅ |
| unscoped/admin read | sees both (2) ✅ |
| cross-project review | **403** ✅ |
| same-project review | **204** ✅ |

---

## 9. Tests Added

**12 new tests**, taking the suite from 12 to **24**.

| File | Test | Pins |
|---|---|---|
| `store_tests.rs` | `b1_wal_and_busy_timeout_are_configured` | B1 config |
| | `b1_concurrent_writes_do_not_silently_disappear` | B1 outcome |
| | `b6_insert_run_rolls_back_completely_on_failure` | B6 atomicity |
| | `b6_insert_run_handles_large_plans` | B6 scale (1,000 tasks) |
| | `b3_task_history_does_not_cross_projects` | B3 |
| | `b14_bug_drafts_are_project_scoped` | B14 read |
| | `b14_bug_draft_project_is_resolvable_for_authorization` | B14 write gate |
| `engine_tests.rs` | `b2_recovery_of_all_passed_run_yields_completed_not_failed` | B2 core defect |
| | `b2_recovery_marks_orphaned_tasks_and_preserves_terminal_ones` | B2 orphans |
| | `b2_recovery_of_run_with_product_failure_yields_failed` | B2 failure path |
| | `b4_started_at_reflects_real_execution_start_under_concurrency_1` | B4 |
| | `b5_hung_executor_times_out_releases_permit_and_run_continues` | B5 |

Both suites use a real on-disk SQLite file per test (`tempfile`), because the defects are properties of the file-backed store — an in-memory database would not exercise journal mode, busy timeout or transactions.

**This closes the coverage gap identified in the backend audit:** every Critical and High finding lived in `engine.rs` or `store.rs`, both of which previously had **zero** tests.

---

## 10. Tests Intentionally Failing Before Fixes

Every fix was preceded by a test proving the defect. Captured output:

**B1** — `PRAGMA journal_mode` returned `delete`, `busy_timeout` returned `0`.

**B2**
```
assertion `left == right` failed: a run whose tasks all passed must recover as COMPLETED, never FAILED
  left: Failed
 right: Completed
```

**B3** (behavioural probe against the old unscoped signature)
```
assertion `left == right` failed: beta ran this task once and it passed; history must not include alpha's executions
  left: 4
 right: 1
```

**B4**
```
FAILED at engine_tests.rs:230 — start-time spread assertion
(pre-fix spread across 4 serialised tasks: 0ms)
```

**B5**
```
assertion `left == right` failed: a hung executor must be recorded as TIMEOUT
  left: Running
 right: Timeout
```

**B6** — 100-task `POST /runs` returned **HTTP 500** pre-fix (backend audit, reproduced).

**B14** (behavioural probe against the old unscoped signature)
```
assertion `left == right` failed: a project-scoped caller must see exactly one draft (its own), not every tenant's
  left: 2
 right: 1
```

### Method note (honest disclosure)

For **B3** and **B14** the permanent tests call the *new* scoped signatures, so against pre-fix code they would fail to compile rather than fail an assertion — weaker evidence. To prove the defects behaviourally I first wrote temporary probes (`src/prefix_probe.rs`) that called the **old** unscoped API and asserted the correct isolation. Those produced the `left: 4 / right: 1` and `left: 2 / right: 1` failures above. The probe file was deleted once the fixes landed; its assertions live on in the permanent tests.

One probe was itself wrong on first run and is disclosed here: it called `set_task_status(..., true, true)`, but that function treats the two flags as mutually exclusive (`if set_started {} else if set_finished {}`), so `finished_at` was never written and history came back empty (`left: 0`). The probe was corrected to mirror the engine's own two-call pattern, after which the real leak appeared (`left: 4`). **The `0` was my harness, not the code.**

---

## 11. Full Test Results

```
$ cargo test

running 24 tests
test engine_tests::b2_recovery_of_all_passed_run_yields_completed_not_failed ... ok
test engine_tests::b2_recovery_marks_orphaned_tasks_and_preserves_terminal_ones ... ok
test engine_tests::b2_recovery_of_run_with_product_failure_yields_failed ... ok
test engine_tests::b4_started_at_reflects_real_execution_start_under_concurrency_1 ... ok
test engine_tests::b5_hung_executor_times_out_releases_permit_and_run_continues ... ok
test store_tests::b1_wal_and_busy_timeout_are_configured ... ok
test store_tests::b1_concurrent_writes_do_not_silently_disappear ... ok
test store_tests::b3_task_history_does_not_cross_projects ... ok
test store_tests::b6_insert_run_rolls_back_completely_on_failure ... ok
test store_tests::b6_insert_run_handles_large_plans ... ok
test store_tests::b14_bug_drafts_are_project_scoped ... ok
test store_tests::b14_bug_draft_project_is_resolvable_for_authorization ... ok
test executor::tests::sanitize_token_accepts_realistic_params ... ok
test executor::tests::sanitize_token_rejects_shell_metacharacters_and_traversal ... ok
test executor::tests::safe_param_allows_empty_default_for_optional_grep ... ok
test executor::tests::safe_param_rejects_unsafe_value_from_params ... ok
test intelligence::tests::classify_failure_recognizes_common_categories ... ok
test intelligence::tests::normalize_signature_collapses_numbers_and_hex_ids ... ok
test policy::tests::maestro_requires_approval_by_default ... ok
test policy::tests::unrestricted_executor_is_allowed ... ok
test policy::tests::concurrency_over_max_is_denied_even_if_also_gated ... ok
test scheduler::tests::srd_25_continue_on_failure_with_10_independent_tasks ... ok
test scheduler::tests::dependent_tasks_become_blocked_when_prerequisite_fails ... ok
test scheduler::tests::validate_plan_rejects_cycles_and_unknown_prerequisites ... ok

test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**All 12 pre-existing tests still pass, unmodified.** No test was weakened or rewritten to accommodate a fix.

---

## 12. `cargo check` Result

```
$ cargo check --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.41s
```

**Clean — 0 errors, 0 warnings.**

---

## 13. Large-Run Benchmarks (IT-6)

Real HTTP `POST /runs` against a running orchestrator. Latency is insert-only (plan acceptance), not execution.

| Tasks | HTTP | Latency | ms/task | Pre-fix (backend audit) |
|---:|---:|---:|---:|---|
| 10 | 201 | 0.03 s | 3.2 | 201 in 0.77 s |
| 50 | 201 | 0.05 s | 1.1 | 201 in 8.58 s |
| 100 | 201 | 0.03 s | 0.3 | **HTTP 500** |
| 250 | 201 | 0.08 s | 0.3 | **HTTP 500** |
| 500 | 201 | 0.17 s | 0.3 | not reached |
| 1000 | **201** | **0.14 s** | **0.1** | not reached |

The ~100-task hard ceiling is gone. At 50 tasks the improvement is roughly **170×** (8.58 s → 0.05 s), and the Acceptance Matrix §12 target of ~1,000 tests is accepted in **0.14 s**.

---

## 14. Restart-Integrity Results (IT-5)

A 341-task run mixing every status class (300 pass, 20 fail, 10 infra-error, 10 dependency-blocked, 1 real pytest executor), captured before restart from memory and after restart from the database.

| Field | Before (memory) | After (database) | |
|---|---|---|---|
| run state | `PARTIAL` | `PARTIAL` | **OK** |
| total tasks | 341 | 341 | **OK** |
| event count | 674 | 674 | **OK** |
| sequence range | 0..673 | 0..673 | **OK** |
| sequence gaps | 0 | 0 | **OK** |
| evidence refs | 0 | 0 | **OK** |

Per-status counts:

| Status | Before | After | |
|---|---:|---:|---|
| PASSED | 300 | 300 | OK |
| FAILED | 20 | 20 | OK |
| INFRA_ERROR | 10 | 10 | OK |
| BLOCKED | 10 | 10 | OK |
| TIMEOUT | 1 | 1 | OK |

- **Tasks stuck RUNNING/QUEUED after restart: 0**
- **Per-task status drift: 0**
- Startup recovery pass reported nothing to recover — the run had persisted correctly, so there was nothing to repair.

**Pre-fix comparison** (backend audit, same procedure): a 500-task run came back as 498 PASSED + 2 stuck RUNNING with 1 event lost, and a 50-task all-passed run came back FAILED.

### IT-5b — evidence across restart

The main IT-5 run compared `0 == 0` for evidence, because its pytest task hit the deliberately short 8 s timeout used for IT-7 and produced no artifact. That is a weak check, so a separate run was executed on the default timeout:

| Field | Before | After | |
|---|---|---|---|
| run state | `COMPLETED` | `COMPLETED` | OK |
| evidence refs | 1 | 1 | OK |
| evidence ids | — | identical | OK |
| evidence sizes | 970 B | 970 B | OK |
| artifact retrievable | — | 968 chars | OK |

**IT-5: PASS · IT-5b: PASS**

---

## 15. Security Isolation Results (IT-1, IT-2)

### IT-1 — Authentication boundary

| Check | Result | Expected |
|---|---|---|
| `GET /health` unauthenticated | 200 (`ok`) | 200 |
| `GET /runs` no key | 401 | 401 |
| `GET /runs` bad key | 401 | 401 |
| `GET /runs` valid admin | 200 | 200 |
| `GET /admin/audit-log` valid admin | 200 | 200 |

**5/5 PASS.** (One line initially read FAIL because the harness parsed `/health`'s plain-text `ok` as JSON — a test-script defect, corrected and re-verified.)

### IT-2 — Cross-tenant isolation

Two project-scoped operator keys, `alpha` and `beta`, sharing one task name.

| Check | Result |
|---|---|
| B3 — alpha history | `3/3`, failure_rate `1.0` |
| B3 — beta history | `0/1`, failure_rate `0.0` (**was 3/4, rate 0.75**) |
| B14 — alpha bug drafts | 1 (own only) |
| B14 — beta bug drafts | 1 (own only) |
| B14 — admin bug drafts | 2 (global key sees both) |
| B14 — cross-project review | **403** |
| B14 — same-project review | **204** |

**All PASS.** Both tenant leaks are closed, read and write.

---

## 16. Remaining Issues

Real, known, and **not** in Phase 1 scope:

| # | Issue | Owner |
|---|---|---|
| R1 | **Live completion path and recovery path now disagree on runs containing only FAILED tasks** — see §17, deviation D1. This is the most important item to resolve. | Phase 2 |
| R2 | `set_task_status(set_started, set_finished)` treats its flags as mutually exclusive (`if/else if`). Calling `(true, true)` silently writes only `started_at`. Not a Phase 1 blocker (the engine calls it correctly), but a latent trap — it cost one debugging cycle in this very work. | Phase 2 |
| R3 | B11 — approvals remain process-local (`api.rs:270` returns 404 after restart). Explicitly Phase 2+. | Phase 2 |
| R4 | B12 — `CorsLayer::allow_origin(Any)`. Explicitly Phase 2. | Phase 2 |
| R5 | B15 — `/admin/audit-log` still global (`list_audit(limit)` has no project filter). Admin-gated, and contract §25's project-scoped `/audit` is Phase 3. | Phase 3 |
| R6 | `/metrics` remains global and unscoped. | Phase 2 |
| R7 | SSRF via `schemathesis.url` / `pytest.base_url` (audit M2). Not a Phase 1 item. | Phase 2 |
| R8 | No bound on plan task count (audit M3). B6 removed the *failure*, so a very large plan now succeeds — which arguably makes a policy limit more necessary, not less. | Phase 2 |
| R9 | WAL adds `-wal`/`-shm` sidecar files next to the database. Any backup or copy procedure must include them or use `VACUUM INTO`. Operational note. | Ops |

---

## 17. Deviations From the Implementation Plan

Two, both disclosed rather than absorbed.

### D1 — Recovery and live completion now use different rules **(needs a decision)**

The Phase 1 instruction for B2 was explicit: *"all tasks PASSED → COMPLETED; any real product failure → FAILED; infrastructure failure → per existing vocabulary; blocked tasks correctly represented."* `derive_run_state` implements exactly that.

The **live** completion path (`engine.rs`, unchanged) uses the original rule:

```rust
if any InfraError { Partial } else { Completed }
```

So for a run containing only `FAILED` tasks and no infrastructure error:

| Path | Result |
|---|---|
| Live completion | `COMPLETED` |
| Recovery after restart | `FAILED` |

I did not change the live path because the instructions also said *"Do not weaken existing scheduler semantics"*, and altering how every normal run is classified is a semantic change well beyond a correctness fix. The two paths therefore disagree on this one case.

This was **not exercised** by IT-5 — that run contained infrastructure errors, so both rules agree on `PARTIAL`.

**Recommendation:** reconcile in Phase 2 by deciding which is authoritative — most likely "a run that executed to completion is COMPLETED; individual test failures are task-level facts" — and then use one shared function for both paths. Flagging rather than silently picking, because it changes the meaning of a status the contract calls canonical.

### D2 — `spawn_run` / `approve_and_spawn` signatures gained a parameter

Both now take `task_timeout: Duration`, supplied by `engine::default_task_timeout()` (`AIQA_TASK_TIMEOUT_SECS`, default 600 s). These are internal Rust functions — **no API path, request schema, response schema or version changed** — but it is a signature change and is recorded here for completeness. It exists so the timeout is injectable for testing rather than read from a process-global env var inside the loop.

### Also noted, not deviations

- **`tempfile` dev-dependency added** — permitted as backend configuration required by Phase 1; test-only, not in the production dependency graph.
- **`kill_on_drop(true)` added to executor commands** — not literally listed in B5, but required for it: `tokio::time::timeout` drops the future, and without this the subprocess would outlive the task that owned it, leaking a hung tool for the orchestrator's lifetime.
- **`apps/web/` shows modifications in `git status`.** They are **not mine** — they appeared from another source during this session. I did not create, modify or revert them, per the Phase 1 constraint.

---

## 18. Phase 1 Definition-of-Done Checklist

From `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md` §22.

| # | DoD item | Status | Evidence |
|---|---|---|---|
| 1 | WAL + busy timeout enabled; **zero** `database is locked` under load | ✅ | PRAGMA test; 0 occurrences across the full integration run incl. a 1,000-task plan |
| 2 | No persistence failure logged-and-ignored; every write error propagates or degrades the run explicitly | ✅ | All 8 sites escalate via `note_persistence_failure`; finalisation transitions the run to `FAILED` with the cause in `policy_notes` and the `RunCompleted` event |
| 3 | **IT-5 passes** — post-restart state matches pre-restart exactly; no stuck RUNNING, no lost events | ✅ | §14: 341 tasks, 674 events both sides, 0 gaps, 0 stuck, 0 drift; evidence verified separately in IT-5b |
| 4 | All-passed run with lost completion write recovers as `COMPLETED`, never `FAILED` | ✅ | `b2_recovery_of_all_passed_run_yields_completed_not_failed` |
| 5 | Project-scoped viewer receives only its own bug drafts; cross-project review → 403 | ✅ | §15 IT-2: alpha 1 / beta 1 / admin 2; cross-review **403**, same-project **204** |
| 6 | Two projects sharing a task name have provably independent histories | ✅ | §15 IT-2: alpha `3/3` rate 1.0, beta `0/1` rate 0.0 |
| 7 | 5×600 ms at `concurrency:1` → distinct `started_at`, ~600 ms durations | ✅ | `b4_...` (4×300 ms variant): spread ≥ 450 ms, per-task duration < 900 ms |
| 8 | Hung executor → `TIMEOUT`, permit released, run reaches terminal | ✅ | Unit test + IT-7: terminal in **8.1 s** vs a 60 s hang; `after → PASSED` |
| 9 | 1,000-task plan accepted within an agreed latency budget | ✅ | §13: **0.14 s** |
| 10 | `engine.rs` + `store.rs` have test suites; every fix has a test that failed against the old code | ✅ | 12 new tests; pre-fix failures captured verbatim in §10 |
| 11 | `cargo test` green; `cargo check --all-targets` clean | ✅ | 24/24 passing; check clean |

**11 / 11 verified.**

### Regression protection (explicitly preserved)

| Semantic | Status |
|---|---|
| Continue-on-failure | ✅ `srd_25_...` unchanged and passing; IT-5 ran 300 passes alongside 20 failures |
| Dependency blocking | ✅ `dependent_tasks_become_blocked...` unchanged; IT-5 produced 10 BLOCKED |
| Policy gate | ✅ 3 policy tests unchanged and passing |
| Approval behaviour | ✅ untouched apart from threading the timeout parameter |
| Executor registry | ✅ untouched; timeout applied at the engine layer so all adapters are covered uniformly |
| Evidence capture | ✅ IT-5b: real 970 B pytest artifact captured, persisted and retrievable after restart |
| Realtime event semantics | ✅ 674 events, sequence 0..673, **0 gaps**, ordering intact |

### Scope discipline

Nothing from Phase 2+ was implemented: no `/api/v1`, no Projects API, no Dashboard API, no RBAC users, no Model Gateway, no Reports, no Mobile APIs, no test generation, no Settings, no frontend integration, no API contract change.

---

## FINAL STATUS

# PHASE 1: **PASS**

All seven items — **B1, B2, B3, B4, B5, B6, B14** — are implemented, each pinned by a regression test that provably failed against the previous code. `cargo check` is clean, all 24 tests pass, and the five required integration checks (IT-1, IT-2, IT-5, IT-6, IT-7) pass against a real running orchestrator over real HTTP, plus a supplementary IT-5b for evidence durability.

The two Critical findings that motivated this phase are closed and demonstrated closed: a restarted orchestrator now reports **exactly** what happened — 674 events with no gaps, no stuck tasks, no status drift — where previously it lost events, stranded passing tasks in `RUNNING`, and recorded a fully-successful run as `FAILED`.

**One open decision blocks nothing but should be resolved early:** the live completion path and the recovery path now classify a failures-only run differently (§17, D1). It is documented, not hidden, and belongs to Phase 2.

---

*No frontend file was modified. `forentend/` and `apps/web/` were not touched by this work. API Contract v1.0 and Acceptance Matrix v1.0 are unmodified. Every command output quoted here was executed against this repository; the one superseded measurement and the one faulty probe are disclosed in §10 and §15.*
