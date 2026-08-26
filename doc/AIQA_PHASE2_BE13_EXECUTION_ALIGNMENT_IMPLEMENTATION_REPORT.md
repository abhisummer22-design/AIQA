# AIQA — Phase 2 — BE-13 Implementation Report

**Unit:** BE-13 — Execution API Contract Alignment + Cancel (Integration Plan milestone table: `BE-13 | Execution + cancel | §12.1–12.5 incl. auditable cancel`)
**Mode:** Implementation sprint (no large independent verification pass, per instruction)
**Status: COMPLETE**

## 1. Final status

**COMPLETE.** §12.2–12.5 are implemented (§12.2 already existed pre-BE-13); §12.1's response is aligned, its request deliberately is not (see §3 — a resolved scope decision, not a blocker). Cancellation is real (interrupts in-flight subprocess-backed tasks, not a label flip), auditable, project-isolated, and restart-safe for any state it actually persisted. 79/79 tests pass (69 pre-existing + 10 new BE-13 tests), zero regressions.

## 2. Exact BE-13 source sections implemented

- **§12.1 Start Execution** — response field aligned (`run_id`/`status`) for the scoped endpoint only; request left as-is (see §3).
- **§12.2 Get Run** — already existed (`get_run_scoped`, from "Actual BE-9"); unchanged.
- **§12.3 Cancel Run** — `POST /api/v1/projects/{project_id}/executions/{run_id}/cancel`, new. "Cancellation must be policy-aware and auditable" is the contract's entire spec for this endpoint.
- **§12.4 Run Tasks** — `GET .../executions/{run_id}/tasks`, new (previously only reachable embedded in `GET .../executions/{run_id}`'s `tasks` array).
- **§12.5 Task Detail** — `GET .../executions/{run_id}/tasks/{task_id}`, new.

## 3. Contract gaps found / scope decisions

**Request schema (`test_case_ids`, `test_suite_id`, `execution_mode`) — deliberately NOT implemented, per Stop Condition #3.**
The contract's §12.1 request is explicitly "Request concept" (illustrative), referencing entities from §8 (Test Case Generator) and §10 (Suites/Plans). Per the Integration Plan's own gap matrix: §8 is **MISSING** ("No LLM integration of any kind") and §10 is **PARTIAL** ("Plans not persisted as reusable entities" — no retrievable id exists for a "test suite" at all). Neither `test_case_ids` nor `test_suite_id` can reference anything real in this backend. The gap matrix's own terse description of §12's gap — "Path + scope mismatch; no cancel endpoint (§12.3)" — does not list a request-schema mismatch as something to fix, and a prior session's own comment in `api.rs` (written during "Actual BE-9") independently reached the same conclusion. Accepting those fields and silently ignoring them, or inventing a fake validation path, would be exactly the "accept anything and continue" / fake-ID behavior this unit is told never to do. The existing `RunPlan` (`name`/`concurrency`/`tasks[]`) request shape is unchanged.

**Response field naming — resolved, not a literal reproduction.**
The contract's example response is `{"run_id": "run_123", "status": "QUEUED"}`. This backend's run-level state machine (`RunState`) never produces `QUEUED` — that value belongs to §13's canonical list, which is task-level (`TaskStatus`), not run-level (confirmed by the gap matrix: "10 statuses" = `TaskStatus`'s exact variant count). Reproducing `"QUEUED"` literally would be a fabricated status. Resolved as: rename the field to `status` (contract's name), keep the *value* as the run's real initial state (`SCHEDULED` or `PENDING_APPROVAL`, whichever the existing policy gate produced).

**"Policy-aware" cancellation — resolved as the existing authorization gate.**
`policy.rs` has no separate "cancel policy" concept (it only gates run *creation*). Inventing one would be undefined-semantics guessing. Resolved as: cancellation goes through the same `require_scoped_project` + `auth::require_role(Operator)` gate every other mutating execution route already uses.

**Cancellable states — resolved conservatively.**
`PendingApproval` is excluded from `/cancel` (rejected with `409`, message pointing at `/reject`, its existing dedicated stop action) since it has never entered the execution pipeline. `Created`/`Scheduled`/`Running` are cancellable; every other state is already terminal (`RunState::is_terminal`, new).

## 4. Request schema changes

None (see §3).

## 5. Response schema changes

- **`POST /api/v1/projects/{project_id}/executions`** (scoped only): new `CreateExecutionResponse { run_id, status, policy_notes }` replaces the previously-shared `CreateRunResponse { run_id, state, policy_notes }` for this route. The legacy `POST /runs` response is byte-for-byte unchanged (locked by a new regression test, §15).
- **`POST .../executions/{run_id}/cancel`**: new `CancelRunResponse { run_id, status }` — `status` is the run's real state at the instant of acceptance (often still `RUNNING`; execution is asynchronous per Contract §12, so this does not claim the run has already stopped).
- **`GET .../executions/{run_id}/tasks`**: `Vec<TaskState>` (existing type, reused).
- **`GET .../executions/{run_id}/tasks/{task_id}`**: `TaskState` (existing type, reused), `404` if not found.

## 6. Endpoints added/changed

Added: `POST .../executions/:run_id/cancel`, `GET .../executions/:run_id/tasks`, `GET .../executions/:run_id/tasks/:task_id`.
Changed (response shape only): `POST /api/v1/projects/:project_id/executions`.
Unchanged: everything else, including all legacy `/runs/...` routes and `/metrics`.

## 7. Cancel behavior and state transitions

- Cancellable: `Created`/`Scheduled`/`Running` → signals `RunRecord::cancel` (a `tokio::sync::watch<bool>`) and records the requesting actor.
- Not cancellable: `PendingApproval` → `409`, points to `/reject`. Any terminal state → `409`.
- Once signalled, `spawn_run`'s dispatch loop: (a) stops dispatching new work — every still-`QUEUED`-and-eligible task is driven straight to `CANCELLED` through the same `complete_task`/persist/emit path a real outcome uses (so cascade-`BLOCKED` for their dependents happens automatically, identical to today's failed-prerequisite behavior); (b) every in-flight task's executor future races the cancel signal via `tokio::select!` — if cancellation wins, the executor future is **dropped**.
- The run's final state is derived by `derive_run_state`, which now checks `CANCELLED` **first**: any task ending `CANCELLED` makes the whole run report `CANCELLED`, overriding what would otherwise be `PARTIAL`/`FAILED`/`COMPLETED`.

## 8. How running work is actually stopped/safely handled

Every real executor (Playwright/Newman/Pytest/Maestro/Schemathesis) already calls `.kill_on_drop(true)` on its `tokio::process::Command` — this is the exact, already-proven mechanism B5's own timeout relies on to guarantee a dropped future's subprocess actually dies rather than being orphaned. Cancellation reuses this identically: dropping the executor future (the losing side of the `select!`) sends the same kill signal a timeout would. This was verified directly, not assumed: `be13_cancel_actually_interrupts_an_in_flight_task_rather_than_waiting_it_out` gives a task a real 2-second delay, cancels it after ~30ms, and asserts the whole run reaches a terminal state in under 1 second — a fake cancel (relabel while the task ran to completion in the background) would take ~2s instead. The permit (`tokio::sync::Semaphore`) is released via the same `drop(permit)` that already existed on every exit path (normal/timeout/now also cancel), so concurrency capacity is never leaked.

## 9. Persistence/recovery behavior

A task's `CANCELLED` status, once actually written to the DB, is durable and survives a restart. `derive_run_state`'s new priority check runs identically inside `recover_incomplete_runs` (B2's recovery path — the same function, no second copy), so a restart correctly reports a partially-cancelled run as `CANCELLED`, not `COMPLETED`/`PARTIAL` (verified: `be13_recovery_reports_cancelled_run_as_cancelled_not_completed`). The in-memory cancel *signal* itself is not durable — a cancel request that raced an unclean shutdown before any task status was actually persisted is not guaranteed to "stick" after restart, which is the same class of already-documented, already-accepted limitation as B1's own persistence tradeoffs (no distributed work queue to redeliver in-flight state). This is disclosed here rather than silently assumed away.

## 10. Authorization/project isolation behavior

Cancel and the two tasks endpoints all reuse `require_scoped_project` (project exists + caller has access) and `scoped_run_summary`/`require_resource_in_project` (run genuinely belongs to the URL's project) — the exact same helpers every other scoped execution route already uses. Cancel additionally requires `Role::Operator`. Verified with real cross-project and cross-role attempts (`403` in both cases, and the run's state confirmed unchanged afterward), plus nonexistent-project/nonexistent-run `404`s.

## 11. Audit/event behavior

- **Audit**: a `run.cancel` entry is written via the existing generic `audit()` helper (same mechanism as `run.create`/`run.approve`/`run.reject`) — verified directly against `store.list_audit`.
- **Events**: the run's `RunCompleted` event summary says `"run cancelled by <actor>"` (or `"run cancelled"`) instead of the generic `"run finished"` when cancellation caused the outcome — verified via `GET .../events`.

## 12. Real entities/data models used for any IDs

None new. `run_id`/`task_id` are the existing real identities (`runs.id`, `tasks.id` scoped by `run_id`). No `test_case_ids`/`test_suite_id` acceptance was added (§3).

## 13. Files changed

- `services/orchestrator/src/domain.rs` — added `RunState::is_terminal()`.
- `services/orchestrator/src/engine.rs` — added `RunRecord::cancel`/`cancel_requested_by`/`is_cancel_requested`/`request_cancel`; `derive_run_state` checks `Cancelled` first; `spawn_run`'s dispatch loop branches on cancellation and races each in-flight executor future against it; factored the existing terminal-persist/emit block into `persist_and_emit_terminal` (reused by both the normal-completion and cancel-before-dispatch paths, no duplicated logic); finalization block's `completion_summary`/`policy_notes` reflect cancellation.
- `services/orchestrator/src/api.rs` — new `cancel_run_scoped`, `list_run_tasks_scoped`, `get_run_task_detail_scoped` handlers + `CancelRunResponse`/`CreateExecutionResponse` DTOs; factored `get_run_scoped`'s inline task-fetch into a shared `scoped_run_tasks` helper (reused by the two new task endpoints); `create_run_scoped` now returns `CreateExecutionResponse` (legacy `create_run`/`CreateRunResponse` untouched); 3 new routes mounted in the existing `protected` router; updated the stale module comment above the project-scoped-executions section.
- `services/orchestrator/src/api_tests.rs` — 7 new `be13_*` HTTP tests; one pre-existing test (`actual_be9_scoped_approve_and_reject_through_router`) updated to check the new `status` field instead of the removed-from-that-response `state` field (see §3's response-alignment change).
- `services/orchestrator/src/engine_tests.rs` — 3 new `be13_*` engine-level tests (real interruption timing, never-started-task cancellation, recovery durability).

## 14. Migrations added

None — every field used already existed (`runs`/`tasks` columns, `audit_log`, `run_events`).

## 15. Tests added (10)

**Engine-level** (`engine_tests.rs`):
1. `be13_cancel_actually_interrupts_an_in_flight_task_rather_than_waiting_it_out` — timing proof of real interruption (cascade/timing requirement 10, safety rule).
2. `be13_cancel_prevents_a_not_yet_started_queued_task_from_ever_running` — a task behind a held permit never transitions to RUNNING once cancelled (requirement 15).
3. `be13_recovery_reports_cancelled_run_as_cancelled_not_completed` — restart-safety (requirement 12).

**HTTP-level** (`api_tests.rs`):
4. `be13_cancel_running_execution_succeeds_and_is_auditable` — success path, response schema, no false success, event + audit behavior (requirements 1, 4, 9, 13, 14).
5. `be13_cancel_rejects_already_terminal_run` — requirement 11.
6. `be13_cancel_rejects_pending_approval_run_pointing_to_reject` — undefined-state resolution, documented.
7. `be13_cancel_rejects_cross_project_and_insufficient_role` — requirements 7, 8, plus confirms no state change occurred.
8. `be13_cancel_nonexistent_run_and_project_return_structured_not_found` — requirement 6 (as applied to cancel).
9. `be13_run_tasks_list_and_detail_endpoints` — §12.4/12.5 real data, response schema, cross-project isolation (requirement 6), nonexistent task.
10. `be13_legacy_create_response_shape_is_unchanged` — requirement 16.

Not added: fake `test_case_ids`/`test_suite_id` validation tests (§3 — no such behavior was implemented, so none was tested) and a duplicate "valid contract-shaped creation" test (already covered by the existing `actual_be9_scoped_execution_lifecycle_through_router`, now additionally exercising the new response shape via test #4 above).

## 16. Targeted test result

```
running 10 tests
test api_tests::be13_cancel_nonexistent_run_and_project_return_structured_not_found ... ok
test api_tests::be13_legacy_create_response_shape_is_unchanged ... ok
test api_tests::be13_cancel_rejects_pending_approval_run_pointing_to_reject ... ok
test api_tests::be13_cancel_rejects_cross_project_and_insufficient_role ... ok
test engine_tests::be13_recovery_reports_cancelled_run_as_cancelled_not_completed ... ok
test api_tests::be13_cancel_rejects_already_terminal_run ... ok
test api_tests::be13_run_tasks_list_and_detail_endpoints ... ok
test engine_tests::be13_cancel_prevents_a_not_yet_started_queued_task_from_ever_running ... ok
test api_tests::be13_cancel_running_execution_succeeds_and_is_auditable ... ok
test engine_tests::be13_cancel_actually_interrupts_an_in_flight_task_rather_than_waiting_it_out ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 69 filtered out
```

## 17. `cargo check --all-targets`

Clean — no warnings, no errors.

## 18. Full `cargo test` result

```
running 79 tests
...
test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.75-2.89s
```
69 pre-existing (D1–D5, B-series, BE-8, BE-9 errors, BE-10, Actual BE-9, BE-12) + 10 new BE-13 tests, all passing. Re-ran 4 times consecutively to rule out timing flakiness in the interruption-timing test — stable every run. Exactly one pre-existing test was *modified* (not deleted): `actual_be9_scoped_approve_and_reject_through_router` now asserts the intentionally-renamed `status` field instead of `state`, matching §3/§5's documented response-alignment change to that same endpoint.

## 19. Explicit out-of-scope (not touched)

BE-14 (project realtime event stream), BE-15 (evidence/findings/audit contract migration), G-AUTHN, G-WS, any frontend code (`forentend/`, `apps/web/`). `/metrics` unchanged. `AIQA_API_CONTRACT_v1.0.md`/`AIQA_ACCEPTANCE_MATRIX_v1.0.md` not modified. §13's `INFRA_ERROR` vs `INFRASTRUCTURE_ERROR` naming gap (flagged separately in the gap matrix, not part of BE-13's own milestone description) was left alone. No `test_case_ids`/`test_suite_id`/`environment_id`/`execution_mode` request-field support was added (§3).

## 20. Remaining blocker or limitation

None blocking. One disclosed, non-blocking limitation: a cancel request that races an unclean process shutdown before any task status is actually persisted is not guaranteed to survive that restart (§9) — this is an existing, already-accepted class of limitation in this codebase's overall persistence model (no distributed work queue), not something introduced or newly exposed by this unit.
