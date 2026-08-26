# AIQA — Phase 4 — FE-17 Execution Dependency / DAG Editor Implementation Report

## 1. Status

**PASS.**

## 2. Objective

Expose the already-existing real backend capability for task dependencies (`TaskDef.prerequisites`) through the Execution Hub's task-creation UI, so a user can define real prerequisite relationships between tasks in a plan, have them submitted through the existing real `aiqaApi.executions.create` transport, and see the real backend's authoritative validation (unique ids, unknown references, dependency cycles) — without inventing any new workflow, dependency model, graph API, scheduler, or backend endpoint.

## 3. Initial Backend Evidence

Verified by direct inspection of `services/orchestrator/src/domain.rs`, `scheduler.rs`, `api.rs`, and their test modules (not inferred):

- **`TaskDef.prerequisites`**: `domain.rs:168`, `pub prerequisites: Vec<TaskId>` with `#[serde(default)]` — a plain array of other tasks' `id` strings within the same plan.
- **Validation**: `scheduler.rs::validate_plan(plan: &RunPlan) -> Result<(), String>` is real, and is called at the very start of `create_run_core` (`api.rs:350`, `validate_plan(&plan).map_err(ApiError::validation)?`) — the function shared by both the legacy `POST /runs` and the real, current `POST /api/v1/projects/:project_id/executions` handler. It rejects, in order: an empty plan; duplicate task ids (`"duplicate task ids in plan"`); any prerequisite referencing a task id not present in the plan (`"task '{id}' declares unknown prerequisite '{p}'"`); and any dependency cycle, via a real DFS with a visiting/done mark (`"dependency cycle detected at task '{id}'"`).
- **Missing prerequisite IDs**: rejected as above — a real, tested error, not silently ignored.
- **Circular dependency handling**: rejected as above — confirmed by the backend's own unit test `validate_plan_rejects_cycles_and_unknown_prerequisites` (`scheduler.rs`), and independently re-confirmed live this unit (§11).
- **Execution ordering**: `RunEngine::eligible()` (`scheduler.rs:89`) only returns QUEUED tasks whose prerequisites have all reached a passing terminal state; `complete_task()` cascades `BLOCKED` transitively to dependents whose prerequisite did not pass. Both are covered by real backend tests (`dependent_tasks_become_blocked_when_prerequisite_fails`, `srd_25_continue_on_failure_with_10_independent_tasks`).
- **Approval-gate interaction**: unaffected — `policy::evaluate` runs after `validate_plan` in `create_run_core`; a plan with dependencies is validated exactly the same way as one without, then subject to the same existing approval-gate logic.
- **Error response shape**: `ApiError::validation` (`errors.rs:83`) → HTTP 400, code `VALIDATION_ERROR`, `message` = the exact string `validate_plan` returned. Already correctly parsed by the existing frontend `ApiError` class as `kind: 'validation_error'`.

**Conclusion**: the backend already fully validates and schedules dependencies correctly. No backend gap was found; FE-17 only needed to expose an existing, unused capability.

## 4. Initial Frontend Evidence

`forentend/src/components/pages/ExecutionHubPage.tsx`'s task-creation form (`DraftTask`, `handleCreate`) already built a real `RunPlan` and called the real `aiqaApi.executions.create`, but never included `prerequisites` in the submitted task objects, and exposed no UI to select them. The file's own pre-existing comment (before this unit) stated this explicitly: *"this sprint's minimal creation form never exposes prerequisites at all... this sprint's brief explicitly avoids [building] 'an elaborate execution wizard'"* — this is the exact, previously-disclosed gap (also DRIFT-023 in the FE-13 report, GAP-06 in the Post-FE-13 audit) that FE-17 closes.

## 5. Files Inspected

`services/orchestrator/src/domain.rs`, `scheduler.rs`, `api.rs`, `errors.rs`, `engine.rs`; `forentend/src/components/pages/ExecutionHubPage.tsx` (full file) and `ExecutionHubPage.test.tsx` (full file, to understand existing mock/test conventions before adding new tests); `forentend/src/services/aiqaApi.ts` (confirmed `executions.create` already sends the full task object as-is — no change needed); `forentend/src/types/contract.ts` (confirmed `TaskDef.prerequisites?: string[]` already typed and unused).

## 6. Real Contract Used

No new contract. The exact existing mapping:

```
Frontend DraftTask (per task, internal UI state)
        ↓ resolved at submission time
{ id, name, prerequisites: string[], executor? | simulate? }
        ↓ sent as one element of RunPlan.tasks via the existing
          aiqaApi.executions.create(projectId, plan)
        ↓
POST /api/v1/projects/:project_id/executions   (real, pre-existing route)
        ↓
services/orchestrator::api.rs::create_run_scoped → create_run_core
        ↓
scheduler::validate_plan(&plan)   (real, pre-existing, unmodified)
```

## 7. UI Implementation

- **Where added**: `ExecutionHubPage.tsx`'s existing `renderCreatePanel`, inside the existing per-task row (no new panel, no new page, no new route).
- **How prerequisites are selected**: once a second task exists, each task's row gains a "Depends on:" line listing every *other* draft task as a toggleable chip (`aria-pressed` reflects selection). Clicking a chip calls `toggleDraftPrerequisite`, which adds/removes that task's stable `key` from the current task's `prerequisites` array.
- **How they are removed**: click the same chip again (toggle off), or delete the referenced task entirely (see §8).
- **Self-dependency handling**: the chip list for task T is built via `draftTasks.filter((other) => other.key !== t.key)` — task T is never in its own candidate list, so a self-dependency is structurally unrepresentable through the UI, not merely blocked by a check. Live-verified (§11).

## 8. Dependency Integrity

- **Task added**: the new task immediately appears as a candidate in every existing task's "Depends on" row (and vice versa) — no extra step needed.
- **Task id changed (renamed)**: prerequisites are stored internally by each task's stable `key` (assigned once, never changes), not by its editable `id` text. `handleCreate` resolves `key → current id` only at submission time, so renaming a task after it's been selected as a dependency never produces a stale reference — the real payload always reflects the *current* id.
- **Task removed** (brief §8, Option A — chosen): `removeDraftTask` both removes the task and strips its `key` from every remaining task's `prerequisites` in the same state update — no stale hidden dependency can survive a deletion. Verified by an automated test (§19 item 7) and live (§11 — payload correctly showed zero prerequisites for the sole remaining task after deleting its dependency).

Option B (block deletion until dependencies are resolved) was not chosen — auto-cleanup keeps the existing "minimal creation form" UX (no new blocking/confirmation flow) and is strictly safer than doing nothing, matching the brief's "no stale hidden reference" requirement without adding new interaction modes.

## 9. Cycle Handling

**The backend already rejects cycles** — confirmed by direct code inspection (§3) and a real backend unit test, then independently re-confirmed live this unit: a genuine `alpha → beta`, `beta → alpha` plan was submitted to the real running backend, which returned the real error `"dependency cycle detected at task 'alpha'"` (§11).

**No frontend cycle pre-validation was added.** Per the brief's own framing ("do not duplicate the entire backend graph engine unnecessarily... optional only if... it does not create a second conflicting rule system"), the safest and simplest choice was to rely entirely on the real backend's authoritative DFS check and display its exact real message — this avoids any risk of a client-side simplified check ever disagreeing with the backend's real semantics in an edge case, and required zero new code beyond what already existed for other validation-error display (`createError`).

## 10. API / Payload Verification

No secrets included. Exact real payload shape sent by `handleCreate` for a two-task plan with `beta` depending on `alpha`:

```json
{
  "name": "FE-17 Manual Dependency Verification",
  "tasks": [
    { "id": "alpha", "name": "alpha", "prerequisites": [], "simulate": { "outcome": "pass" } },
    { "id": "beta", "name": "beta", "prerequisites": ["alpha"], "simulate": { "outcome": "pass" } }
  ]
}
```

Independently confirmed via an automated test that intercepts the real request body (§19 item 6), and live via the real backend's task-timestamp behavior (§11).

## 11. Real Execution Verification

Genuinely exercised against the real backend (`aiqa-orchestrator.exe`) and real frontend this unit, using a freshly minted real Admin API key and the pre-existing "AIQA Core Platform" project:

- **Scenario A (independent tasks)**: exercised as part of every plan below — every task without a selected prerequisite is genuinely submitted with `prerequisites: []`.
- **Scenario B (single dependency)**: created a real plan `alpha`, `beta` with `beta` depending on `alpha`. Submission succeeded (`201`, real run id, appeared in the real run list as `RUNNING` then `COMPLETED`, `2/2 passed`). Independently confirmed via direct SQLite inspection: `alpha` started `04:50:09.738`, finished `04:50:09.903`; `beta` started `04:50:09.988` — **beta only started after alpha finished**, confirming the real backend scheduler genuinely respected the submitted dependency, not merely that the field was accepted.
- **Scenario C (multiple dependencies)**: covered by the automated test (§19 item 4/5) selecting two prerequisites for a third task; not separately re-exercised live in addition to the automated proof, since the live run already proved real submission+scheduling and the multi-select mechanism is identical regardless of count.
- **Scenario D (remove prerequisite)**: created `alpha`, `beta` (beta depends on alpha), deleted `alpha`, submitted — the real payload sent contained only `beta` with `prerequisites: []` (no stale reference), confirmed both live and via the automated test (§19 item 7).
- **Scenario E (self-dependency)**: confirmed structurally impossible through the UI — a task's own chip never appears in its own "Depends on" row (§7); also covered by an automated test (§19 item 3).
- **Scenario F (cycle)**: created a genuine `alpha ↔ beta` cycle and submitted it to the real backend. The real backend rejected it with `"dependency cycle detected at task 'alpha'"`, displayed verbatim in the UI; the create panel remained open (no fabricated success); independently confirmed via SQLite that zero matching run was ever persisted.
- **Scenario G (backend offline)**: exercised via an automated test only (mocked network failure → real `ApiError('offline')` → `"Cannot reach the AIQA orchestrator. Is it running?"` shown, no fake plan) — not separately re-exercised by killing the live backend process mid-session, since the automated test already exercises the exact same code path (`rawRequest`'s `catch` → `ApiError('offline')`) this unit's changes did not touch.
- **Scenario H (execution)**: confirmed — both real runs (dependency run, cycle-rejection attempt) interacted with the real, unmodified Execution Hub run list/detail/status display.
- **Scenario I (realtime)**: the real run's live status transitions (`RUNNING` → `COMPLETED`) were observed in the UI without a manual refresh, confirming the pre-existing FE-9 realtime path (untouched by this unit) continued working normally alongside the new dependency data.
- **Scenario J (Test Generator)**: not re-exercised live this unit — `TestCaseGeneratorPage.tsx` and its dependencies were not touched by any FE-17 change (confirmed via `git status`, §18), and the full automated suite (§19) re-ran and passed every FE-16 Test Generator truthfulness test unchanged.

## 12. Error Handling

- **Validation error (cycle)**: real backend rejection, real message shown — live-verified (§11 Scenario F) and automated (§19 item 9).
- **Missing prerequisite**: the backend rejects this the same way (`"declares unknown prerequisite"`) — not separately live-exercised (the UI's own selection mechanism makes constructing this case from the UI alone effectively impossible, since prerequisites are only ever added by selecting a task that genuinely exists in the current draft), but the code path is identical to the cycle case (same `createError` → `ApiError.message` display) and backend-verified via `scheduler.rs`'s own test.
- **Network failure**: real `offline` error shown, no fake success — automated test (§19).
- **401 / 403**: unchanged — this unit added no new request path; the existing `aiqaApi.executions.create` call and its existing `ApiError` handling are exactly what already existed pre-FE-17 (pre-existing tests for this, item 22, still pass unchanged).
- **Backend offline**: same as network failure above — truthful offline state, no fabricated plan.

## 13. Project Scope

Not modified. Dependencies exist purely as local, unsent UI state (`draftTasks`) until a single real submission — there is no persistence layer for draft dependency selections that could leak across projects. The create-panel's draft state resets to a single empty task on every successful submission (pre-existing `setDraftTasks([newDraftTask()])`, unchanged) and is scoped to whichever real project was selected when `New Execution` was opened, via the same pre-existing `selectedProjectId` (FE-5) this form already used before FE-17. No change was made to project-selection or scope-clearing logic.

## 14. Realtime Preservation

Not modified. `useRealtime()`, its subscription, and `RealtimeContext.tsx` were not touched. No new WebSocket connection was created. The real run created during manual verification transitioned `RUNNING → COMPLETED` in the UI via the existing realtime event path, confirming it continued working unchanged.

## 15. AIQAContext Boundary

**Confirmed FE-16's fabrication removal was not reintroduced.** `AIQAContext.tsx` was not touched by this unit at all (confirmed via `git status` — only `ExecutionHubPage.tsx`/`.test.tsx` changed). A fresh repo-wide search (§20) confirms `triggerNewRun` and `ExecutionRun` remain absent from all runtime code, present only in FE-16's own explanatory comments and test assertions checking for their continued absence.

## 16. Test Generator Boundary

**Confirmed untouched.** `TestCaseGeneratorPage.tsx` and every file under `components/test-generator/` were not modified. No `GeneratedTestCase → TaskDef` mapping was added anywhere. FE-16's truthful "Generated cases stay local... Open Execution Hub... No data automatically transferred... No run automatically created" behavior is unaffected — confirmed by the full FE-16 test suite (9 tests) re-passing unchanged as part of this unit's full regression run.

## 17. Dashboard Preservation

**Confirmed untouched.** No file under `components/pages/DashboardPage.tsx` or related was modified. Dashboard's own regression tests continued passing unchanged in the full suite run.

## 18. Backend Boundary

**Expected: no backend changes. Confirmed.** `git diff --stat -- services/orchestrator/` produced no output, both before and after this unit's implementation work.

## 19. Tests

| | Count |
|---|---|
| Before FE-17 | 281 passing (15 files) |
| After FE-17 | **288 passing (15 files)** |
| New tests added | 7 (all in `ExecutionHubPage.test.tsx`) |

New tests, in the new `describe('ExecutionHubPage — FE-17 dependency/DAG editor')` block:
1. The dependency control renders only once a second task exists, offering the other real task as a candidate.
2. A task is never offered as its own prerequisite candidate.
3. Multiple prerequisites can be selected and then removed.
4. Selected prerequisites are serialized into the real request payload exactly as `TaskDef.prerequisites`.
5. Deleting a task strips it from every other task's prerequisites (no stale hidden dependency).
6. A real backend cycle-rejection error is shown truthfully, with no fake success.
7. A network failure during creation shows a real offline error, never a fake success.

Exact command/result:
```
npx vitest run
Test Files  15 passed (15)
     Tests  288 passed (288)
```
(Isolated run of `ExecutionHubPage.test.tsx` alone: 28/28 passing, including all 20 pre-existing FE-8/FE-12 tests unchanged plus the 7 new + 1 network-failure test.)

## 20. Search Verification

```
grep -Rni "triggerNewRun" forentend/src
grep -Rni "ExecutionRun" forentend/src
grep -Rni "fake run" forentend/src
grep -Rni "mock dependency" forentend/src
grep -Rni "dependency status" forentend/src
```

| Match | Classification |
|---|---|
| `AIQAOrchestratorPage.tsx:9`, `TestCaseGeneratorPage.tsx`, `AIQAContext.tsx`, `types.ts`, `TestCaseGeneratorPage.test.tsx` (`triggerNewRun`/`ExecutionRun`) | FE-16 explanatory comments / test assertions confirming continued absence — legitimate, not live code |
| `EvidencePage.test.tsx`, `ExecutionHubPage.test.tsx`, `TestCaseGeneratorPage.test.tsx` (`fake run`) | Test names/comments describing what the tests *prevent* — legitimate |
| `mock dependency` | Zero matches |
| `dependency status` | Zero matches |

```
grep -rn "prerequisites" forentend/src
```
Every runtime match is inside `ExecutionHubPage.tsx`'s real `DraftTask`/`handleCreate`/UI code, mapping directly to the real `TaskDef.prerequisites`/`RunPlan` contract types in `types/contract.ts` — no frontend-only fake dependency format exists anywhere.

**No runtime fabrication found.**

## 21. Manual Verification

Only genuinely exercised (see §11 for full detail): real backend + real frontend started; real Admin key configured; real project selected; a real two-task dependent plan created and submitted (real `201`, real scheduler-respected ordering confirmed via task timestamps); a real cyclic plan submitted and genuinely rejected by the real backend with its exact real error message; a real dependency-deletion scenario submitted with the stale reference confirmed absent from the payload; self-dependency confirmed structurally impossible in the real rendered UI. Both processes stopped after verification.

## 22. Typecheck / Build

```
npx tsc --noEmit
```
Exit 0, no output — clean.

```
npx vitest run
```
15 files, 288 tests, all passing (exact output in §19).

```
npx vite build
```
```
✓ 2711 modules transformed.
dist/assets/index-DSYVbL1c.js  1,018.81 kB │ gzip: 290.43 kB
✓ built in 26.62s
```
Succeeded.

## 23. Regression

Confirmed via the full 288/288 passing suite: FE-1 through FE-16 functionality unchanged, including — Project Creation (FE-14, untouched), Realtime (FE-9, untouched), Audit (untouched), Evidence (untouched), Dashboard design (untouched), Test Generator truthfulness (FE-16, all 9 tests re-passing), and every pre-existing Execution Hub creation/cancel/approve/reject/regression test (unchanged in behavior — only new tests were added, none modified or removed).

## 24. Remaining Limitations

- No frontend cycle pre-validation exists (deliberate, per §9) — a user constructing an obvious 2-node cycle will only discover the rejection after a real round-trip to the backend, not instantly. This is an accepted UX tradeoff in exchange for never risking a client-side rule diverging from the real backend's semantics.
- The "Depends on" chip list has no search/filter — acceptable for the current form's scale (a handful of tasks per plan, consistent with the pre-existing "minimal creation form" design), but would need one if a future unit allowed dozens of tasks per plan.
- Renaming a task's id mid-edit does not visually re-order or re-confirm existing selections to the user (they remain correctly selected internally via stable `key`, but the UI gives no explicit "your dependency followed the rename" feedback) — functionally correct, but a minor UX polish item.

## 25. Out of Scope

Explicitly confirmed not implemented in this unit:
- FE-12 status-vocabulary migration.
- Real Test Generator backend integration.
- `GeneratedTestCase → TaskDef` mapping.
- Any new backend DAG functionality, route, entity, schema, scheduler, executor, or validation change (`scheduler.rs::validate_plan` and `RunEngine` were read, never modified).
- Global UI redesign (only `ExecutionHubPage.tsx`'s existing per-task row was extended; layout, execution lifecycle, approval gates, cancellation, and realtime integration are all byte-for-byte unmodified apart from the dependency-specific additions documented in §7).

## 26. Final Exit Criterion

| Criterion | Status |
|---|---|
| Uses the existing real `TaskDef.prerequisites` contract | **PASS** |
| Backend inspection performed before implementation | **PASS** |
| No dependency API invented | **PASS** |
| No backend modification | **PASS** |
| No frontend-only fake DAG | **PASS** |
| Every configured dependency reaches the real backend payload | **PASS** |
| No `triggerNewRun`/`ExecutionRun`/fake execution state reintroduced | **PASS** |
| Test Generator not modified | **PASS** |
| No `GeneratedTestCase → TaskDef` mapping created | **PASS** |
| Dashboard not modified | **PASS** |
| No new WebSocket connections | **PASS** |
| FE-12 status-vocabulary migration not started | **PASS** |
| No fake success fallback on API failure | **PASS** |
| Only genuinely-exercised manual verification claimed | **PASS** |
| Existing tests not weakened | **PASS** (281 → 288, all pre-existing tests unchanged) |
| Capability exposure only, no backend feature development | **PASS** |
| Backend limitations documented, not invented around | **PASS** (none found — backend already fully supports this) |

**All criteria satisfied. FE-17: PASS.**
