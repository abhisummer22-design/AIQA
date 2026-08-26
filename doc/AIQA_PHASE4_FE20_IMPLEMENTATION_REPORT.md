# AIQA — Phase 4 — FE-20 Implementation Report

## 1. Status

**PASS.**

## 2. Objective

Expose the full real per-status task-count breakdown that the backend already computes and already returns on every `RunDetail` fetch (`GET /api/v1/projects/:project_id/executions/:run_id`), but which the Execution Hub's run-detail summary grid only partially rendered (5 of 11 real fields).

## 3. Fresh Capability Inventory

Performed independently this session — no field, gap, or conclusion below was copied from FE-18/FE-19 without re-reading the current source.

| Area | Fresh check performed | Category |
|---|---|---|
| **Execution Hub run-summary grid** | Read `ExecutionHubPage.tsx` lines 746-767 directly: grid rendered `total`, `passed`, `failed`, `blocked`, `needs_review` only. Read `types/contract.ts` `RunSummary`: 11 real fields exist (`total`, `queued`, `running`, `passed`, `failed`, `blocked`, `skipped`, `cancelled`, `timeout`, `infra_error`, `needs_review`). Read `services/aiqaApi.ts::executions.get`: already fetches the full `RunDetail` (which extends `RunSummary`) — no new request needed. | **A — eligible, selected** |
| Findings/Reports (`ReportsPage.tsx`) | Re-grepped for `promoted_at`/`promoted_by`/other `Finding` fields post-FE-19: all fields now rendered (FE-19 already closed this gap this session). | B — already complete |
| Audit (`AuditPage.tsx`) | Re-grepped `AuditRow` field usage: all 8 fields (`occurred_at`, `actor`, `role`, `action`, `target`, `detail`, `outcome`, `project_id`) still rendered, unchanged since FE-19's fresh check. | B — already complete |
| Evidence (`EvidencePage.tsx`) | Re-grepped `EvidenceRow` field usage: all fields still rendered; `evidence.get` still deliberately unused per the file's own documented redundancy with `list_evidence_for_run`. | B — already complete |
| Dashboard (`DashboardPage.tsx`) | Re-read: independently already sums `passed+failed+blocked+skipped+cancelled+timeout+infra_error+needs_review` from `RunDetail` for its own "tests executed"/progress-percent computation — confirming these same fields are real, already-relied-upon data elsewhere in the app, not fields being invented a use for. Dashboard's remaining 9/11 undocumented KPI concepts are still genuinely backend-missing. | D (KPI gaps) / not touched |
| Project data (`aiqaApi.ts::projects.*`) | Re-confirmed `update` has no PRD/FRD/SRD citation found; `list`/`get`/`create` all consumed. | C (update) / B (rest) |
| `aiqaApi.ts` full function inventory | Re-read all 21 exported call sites (`projects`, `executions`, `evidence`, `findings`, `dashboard`, `audit`). `executions.task`/`evidence.get` remain Category E (documented redundant); `executions.events` remains Category C (superseded by realtime resync, per FE-19's still-valid finding). | Mixed, no new candidate |

**Conclusion:** exactly one fresh Category-A candidate exists — the Execution Hub's incomplete count grid.

## 4. Selected Capability

Extend the existing run-detail summary grid in `ExecutionHubPage.tsx` to render all 11 real `RunSummary` count fields instead of 5, using the exact real backend field names and the exact real backend vocabulary (no renaming, no merging).

## 5. Requirement Evidence

Direct quotes from the current repository's own code, written against the product documents:

- `services/orchestrator/src/api.rs` (line 476-478): *"GET /runs/:id — FRD §7 Live Dashboard UX data source."* — the very endpoint this grid is fed by (`get_run_scoped`, its project-scoped equivalent, built on the same `RunSummary`) is documented as the FRD §7 "Live Dashboard UX" data source.
- `services/orchestrator/src/domain.rs` (line 233): *"SRD §4 event types. TaskStarted/…/TaskCompleted map onto FRD §2 statuses"* — establishing that the full 10-value `TaskStatus` vocabulary (`QUEUED/RUNNING/PASSED/FAILED/BLOCKED/SKIPPED/CANCELLED/TIMEOUT/INFRA_ERROR/NEEDS_REVIEW`) is itself an FRD §2 product-defined concept, not an internal implementation detail.
- `ExecutionHubPage.tsx`'s own doc comment (lines 28-36, pre-existing, re-verified unchanged) already states this page uses "Status vocabulary (`domain.rs::RunState`/`TaskStatus`) ... exactly as the backend returns it" for the full 10-value set — establishing this page as the intended FRD §7 surface for that exact vocabulary.

Together: a page documented as the product's "Live Dashboard UX" surface for a product-defined 10-status vocabulary, that silently drops 6 of those 10 real, already-fetched states from its summary, is not a truthful implementation of that requirement.

No claim is made that "the PRD requires every backend field to be displayed" — the requirement basis is specific to this page's own documented FRD §7 role and the FRD §2 status vocabulary it already partially surfaces.

## 6. Backend Evidence

- **Domain model** (`domain.rs:201-231`, `RunSummary`): `total: usize`, plus one `usize` field per real `TaskStatus` value — `queued`, `running`, `passed`, `failed`, `blocked`, `skipped`, `cancelled`, `timeout`, `infra_error`, `needs_review`. All non-nullable — a real 0 is always a genuine count, never an absent value.
- **Persistence/computation** (`store.rs::get_run_summary`, lines 343-383): real SQL fetch of the run row plus every real persisted `tasks` row for that run (`list_tasks_for_run`), then a real in-process group-by-status count (`HashMap<TaskStatus, usize>`) over those real rows — not a frontend-derived estimate, not a fake calculation, not an inferred value. The in-memory fast path (`engine.rs::summary`, used while a run is still active) computes the identical count via `scheduler.rs::RunEngine::counts()`, the same real engine that drives actual task scheduling.
- **Route**: `GET /api/v1/projects/:project_id/executions/:run_id` (`api.rs::get_run_scoped`, line 792) — the same, already-in-use endpoint (no new route).
- **Authorization**: `require_scoped_project(&state, &ctx, project_id)` (any authenticated role can view) then `require_resource_in_project(summary.project_id.as_deref(), project_id)` inside `scoped_run_summary` — unchanged, already enforced.
- **Project scope**: `RunDetail`'s `summary.project_id` is checked against the URL's `project_id` before the response is ever built — cross-project access is rejected before the frontend sees any data.
- **Backend tests**: `engine_tests.rs` (grepped fresh) contains assertions against `summary.blocked`, `summary.skipped`, `summary.cancelled` etc. for `RunEngine`-driven scenarios (e.g. `dependent_tasks_become_blocked_when_prerequisite_fails`, `srd_25_continue_on_failure_with_10_independent_tasks`) — the count-computation logic itself is already backend-tested, independent of this frontend change.

## 7. API Contract

Request: `GET /api/v1/projects/{project_id}/executions/{run_id}` (already called by `aiqaApi.ts::executions.get`, already invoked by `ExecutionHubPage.tsx`'s existing detail-fetch effect — no new call site).

Response (`api.rs::RunDetail`, `#[serde(flatten)]` over `RunSummary` plus `tasks`) — real example captured live this session (§17):
```json
{
  "run_id": "fa8883c9-...", "name": "FE-8 cancel test", "state": "CANCELLED",
  "total": 1, "queued": 0, "running": 0, "passed": 0, "failed": 0,
  "blocked": 0, "skipped": 0, "cancelled": 1, "timeout": 0,
  "infra_error": 0, "needs_review": 0,
  "started_at": "...", "finished_at": "...", "project_id": "...",
  "requested_by": "...", "policy_notes": "...", "approved_by": null, "approved_at": null,
  "tasks": [...]
}
```
`forentend/src/types/contract.ts` (lines 141-167) already types every one of these fields correctly on `RunSummary`/`RunDetail` — no contract change.

## 8. Initial Frontend Gap

`ExecutionHubPage.tsx`, run-detail summary grid (before this unit):
```jsx
<div className="grid grid-cols-3 sm:grid-cols-5 gap-2 text-[10px] font-mono">
  {/* TOTAL, PASSED, FAILED, BLOCKED, NEEDS REVIEW only */}
</div>
```
`detail.queued`, `detail.running`, `detail.skipped`, `detail.cancelled`, `detail.timeout`, `detail.infra_error` were already present on the fetched `detail` object (confirmed via the type contract and the live network response) and never read anywhere in this component.

## 9. Candidates Not Selected

No other Category-A candidate was found in this session's fresh audit (§3) — Findings/Reports was closed by FE-19 this same session, Audit and Evidence remain fully exposed, Dashboard's remaining gaps are backend-missing (Category D, out of scope), and `projects.update` remains Category C (no requirement citation found on fresh re-check). This unit did not need to choose between competing Category-A candidates.

## 10. Implementation

Extended the existing grid (same container pattern, same cell markup, same color language) from 5 to 11 cells, reusing:
- The file's own pre-existing `formatStatusLabel` utility (`import { formatStatusLabel } from '../../utils/statusLabel'`, already imported and used elsewhere in this same file for task/run status badges) for every new label, instead of hand-typing the underscore-to-space transformation a second time.
- Color hues already established by the file's own `TASK_STATUS_STYLE` map (`RUNNING`→cyan, `TIMEOUT`/`INFRA_ERROR`→orange, `QUEUED`/`SKIPPED`/`CANCELLED`→neutral slate) for visual consistency with the per-task status badges already shown in the table below the grid.
- The grid's `grid-cols-3 sm:grid-cols-4 lg:grid-cols-6` responsive breakpoints were widened from `sm:grid-cols-5` to accommodate 11 cells without redesigning the panel — same card styling, same font, same spacing.
- Added `id="run-detail-count-grid"` to the wrapping `<div>` for stable, structure-based test/verification queries (matching the `id`-based pattern already established in FE-18/FE-19).

No new state, no new fetch, no new component, no new file.

## 11. Real Data Verification

- **Automated verified**: all 6 newly-exposed fields (§16), plus regression of the 5 previously-exposed fields, plus a real-zero-rendering check, plus a run-switch check proving the grid re-renders from the newly-fetched `RunDetail` rather than stale state.
- **Live verified** (§17): the real `cancelled` field and real-zero rendering of the other 5 newly-exposed fields, against a real backend response and a real database row.
- **Not live verified**: `queued`, `running`, `skipped`, `timeout` with a genuinely non-zero real value — no run in the live database currently has a persisted task in any of these states with a real `project_id` (checked directly against `infra/data/orchestrator.sqlite3`; every diverse-status run found — `pipeline-with-dependencies`, `srd-25-continue-on-failure`, `size-probe-500` — has `project_id IS NULL`, i.e. was created via the legacy, pre-project-scoping `/runs` endpoint and is not reachable through the scoped Execution Hub UI at all). Per the brief's explicit instruction not to fabricate or seed a state artificially for this unit, this was not manufactured. These 4 fields are automated-verified only, honestly distinguished from the live-verified fields.

## 12. Error Handling

No fake fallback was introduced. All 11 fields are non-nullable `number` in both the Rust struct and the TypeScript contract — a rendered `0` is always a real backend-computed count, never a null-coalesced placeholder. Loading, error, offline, 401, and 403 states are handled entirely upstream of the grid (unchanged `detail === null` / `error` branches already gate the whole detail panel, including this grid) — not touched by this change.

## 13. Project Scope

Unchanged. The grid renders fields off the same `detail: RunDetail | null` state already populated by the existing, unmodified, project-scoped `aiqaApi.executions.get(projectId, runId)` call. No new request, no new scope check, no legacy `/runs` usage.

## 14. Realtime

Not applicable as a new mechanism — reused entirely. The grid re-renders whenever `detail` updates, and `detail` is refreshed by FE-9's existing debounced realtime resync (`detailResyncTimeoutRef`) exactly as it already was for the 5 previously-shown fields. No new WebSocket connection, no new event subscription, no new polling interval.

## 15. Fabrication Search

```
grep -niE "fake|mock|demo|sample|placeholder|hardcoded|Math\.random" forentend/src/components/pages/ExecutionHubPage.tsx
grep -Rni "RunDetail" forentend/src/components/pages/
grep -RniE "QUEUED|RUNNING|PASSED|FAILED|SKIPPED|CANCELLED|TIMEOUT|INFRA_ERROR" forentend/src/components/pages/ExecutionHubPage.tsx
```
Classified:
- `placeholder` matches: one pre-existing, unrelated doc comment ("fabricated '—' placeholder", documenting a prior truthful-fallback decision) and three legitimate HTML `placeholder` attributes on real form inputs (task name/id fields in the creation panel) — not fake-data markers.
- `RunDetail` matches in `DashboardPage.tsx`: confirms Dashboard already independently sums the same `skipped`/`cancelled`/`timeout`/`infra_error` fields for its own real progress computation — corroborating evidence these are real, already-relied-upon fields, not newly invented ones. `DashboardPage.tsx` was not modified.
- Status-literal matches in `ExecutionHubPage.tsx`: all are either the pre-existing `TASK_STATUS_STYLE`/`RUN_STATE_STYLE` color maps (reused, not duplicated), the pre-existing `SIMULATE_OUTCOMES` picker (unrelated, real `simulate` executor feature), or the new grid's own `formatStatusLabel(...)` calls — every label matches its real backend field 1:1, none renamed or merged.

No new fabrication was introduced.

## 16. Tests

New `describe('ExecutionHubPage — FE-20 real full run-summary count-grid exposure', ...)` block, 5 tests:
1. *item 1 — real non-zero QUEUED/RUNNING/SKIPPED/CANCELLED/TIMEOUT/INFRA ERROR values from RunDetail render in the grid* — a new fixture (`RUN_B_DETAIL`) with distinct non-zero values per field, asserts each renders its own real value.
2. *item 2 — semantically distinct states render distinct real counts, never merged* — asserts `TIMEOUT`/`INFRA ERROR`/`FAILED` remain three separate real values even where two happen to share the same count, and `SKIPPED` ≠ `CANCELLED`.
3. *item 3 — a real backend zero renders as the literal count 0, never blank or omitted* — reuses the pre-existing `RUN_A_DETAIL` fixture's genuine zeros for the 5 newly-exposed fields that are zero there.
4. *item 4 — previously-exposed real counts remain correct alongside the new fields* — regression check on `TOTAL`/`PASSED`/`FAILED`/`BLOCKED`/`NEEDS REVIEW`.
5. *item 5 — switching the selected run re-renders the grid from the newly-fetched real RunDetail* — selects `RUN_A` (queued=0) then `RUN_B` (queued=3), confirming the grid tracks the real fetch path rather than stale state.

No pre-existing fixture was modified. No pre-existing test was weakened.

**Isolated file**: `npx vitest run src/components/pages/ExecutionHubPage.test.tsx` → 36 tests passed (31 pre-existing + 5 new), first attempt, zero fixes needed.

**Full suite**: `npx vitest run` → **299 tests passed, 15 test files** (up from 294 at the end of FE-19 — net +5, all new).

## 17. Manual Verification

Genuinely performed this session:
1. Queried the live `infra/data/orchestrator.sqlite3` directly for every run's real per-status task distribution. Found the only diverse-status runs (`pipeline-with-dependencies`, `srd-25-continue-on-failure`, two `size-probe-500` runs) all have `project_id IS NULL` (legacy/unscoped, unreachable via the scoped Execution Hub) — reported honestly in §11 rather than seeded around.
2. Selected a real, project-scoped run instead: `fa8883c9-3649-4cd1-af42-59ad7f606588` ("FE-8 cancel test", project `9860d454-...` "AIQA Core Platform"), a genuine `CANCELLED` run left over from FE-8's own manual verification, with one real `CANCELLED` task.
3. Minted a fresh real Operator API key directly in `api_keys` (`SHA-256(raw)`), started the pre-compiled backend (`aiqa-orchestrator.exe`, no rebuild) against the same live database, and confirmed via `curl` that the real API returns `"cancelled":1` with all other 10 count fields at real `0`.
4. Started the frontend dev server, set the real key in `localStorage`, verified the connection, confirmed "AIQA Core Platform" was the active project, and clicked into "FE-8 cancel test" in the real UI.
5. Read `#run-detail-count-grid`'s real rendered cells via the browser: **`TOTAL=1, QUEUED=0, RUNNING=0, PASSED=0, FAILED=0, BLOCKED=0, SKIPPED=0, CANCELLED=1, TIMEOUT=0, INFRA ERROR=0, NEEDS REVIEW=0`** — matching the real API response and the real SQLite row exactly.
6. Stopped the backend (`taskkill`) and frontend dev server, and revoked the manually-minted API key as cleanup.

**Live verified**: `cancelled` (real non-zero) and real-zero rendering of `queued`/`running`/`skipped`/`timeout`/`infra_error` in the same check. **Automated-verified only**: non-zero `queued`/`running`/`skipped`/`timeout` (no real project-scoped run currently exists in that state — honestly reported in §11, not fabricated).

## 18. Typecheck

```
npx tsc --noEmit
```
Clean, no output, run after both the implementation edit and the new test additions.

## 19. Build

```
npx vite build
```
Succeeded: `2711 modules transformed`, `dist/assets/index-H7xF6vUN.js` (1,020.67 kB, gzip 290.67 kB) — the pre-existing chunk-size warning is unchanged and unrelated to this change.

## 20. Backend Boundary

```
git diff --stat -- services/orchestrator/
```
Empty, verified both before this unit's changes and after. No backend file was modified; the manually-minted API key and its later revocation were both direct `UPDATE`/`INSERT` operations against the git-untracked runtime database file (`infra/data/orchestrator.sqlite3`, confirmed via `git status --porcelain -- infra/` returning nothing), not a source change.

## 21. FE-16 Through FE-19 Regression

- **FE-16** (Test Generator truthfulness): `TestCaseGeneratorPage.tsx` untouched; its 9 tests still pass.
- **FE-17** (dependency/DAG editor): the create-panel dependency-chip UI in `ExecutionHubPage.tsx` untouched; all 8 FE-17 tests still pass unmodified.
- **FE-18** (approval/rejection attribution): the `run-approval-attribution` span untouched; all 3 FE-18 tests still pass unmodified.
- **FE-19** (finding review timestamp): `ReportsPage.tsx` untouched this unit; all 3 FE-19 tests still pass.
- **FE-12** (status vocabulary migration): not started. The existing, already-shipped `formatStatusLabel` utility (a prior, already-complete space-for-underscore display transformation, not a semantic rename) was reused as-is for the new labels — no raw backend value was renamed, merged, or reinterpreted.

## 22. Files Changed

- `forentend/src/components/pages/ExecutionHubPage.tsx` (summary grid: 5 → 11 cells, `id` added, `grid-cols` breakpoints widened)
- `forentend/src/components/pages/ExecutionHubPage.test.tsx` (+123 lines, new describe block, 5 tests)

## 23. Files Explicitly Not Changed

`services/orchestrator/**` (all files), `DashboardPage.tsx`, `TestCaseGeneratorPage.tsx`, `AuditPage.tsx`, `EvidencePage.tsx`, `ReportsPage.tsx`, `AIQAContext.tsx`, `RealtimeContext.tsx`, `types/contract.ts`, `services/aiqaApi.ts`, `utils/statusLabel.ts`.

## 24. Remaining Limitations

No real, project-scoped run in the current live database currently sits in `QUEUED`, `RUNNING`, `SKIPPED`, or `TIMEOUT` with a non-zero count — those 4 fields' non-zero rendering is automated-verified only, not live-verified, honestly disclosed in §11/§17 rather than papered over.

## 25. Open Candidates

None identified this session beyond what is already documented as Category C/D (`projects.update` — no requirement citation; Dashboard's 9 backend-missing KPI concepts).

## 26. Truthfulness Statement

No fabricated runtime data, endpoint, field, workflow, metric, or execution state was introduced. All 11 rendered values are the real backend's own computed counts, retrieved through the existing, unmodified, project-scoped fetch path. **Expected: None. Actual: None.**

## 27. Evidence Chain Summary

Requirement (`api.rs`'s own "FRD §7 Live Dashboard UX data source" comment + `domain.rs`'s "FRD §2 statuses" comment, both re-verified fresh this session) → Real backend capability (`store.rs::get_run_summary`'s real SQL-backed group-by-status count, `engine.rs`/`scheduler.rs`'s identical live-run equivalent, both already backend-tested) → Real API contract (`GET .../executions/:run_id`, already called, already typed, all 11 fields already declared on `RunSummary`) → Concrete frontend gap (5 of 11 fetched fields never rendered) → Smallest truthful implementation (6 new grid cells reusing existing label/color/layout patterns, no new request) → Automated verification (5 new tests, 299/299 full suite, clean typecheck, clean build) → Regression verification (294→299, zero pre-existing tests broken, FE-16/17/18/19 boundaries confirmed intact) → Live verification (real SQLite row → real API response → real rendered DOM cells, for the fields where real diverse data actually exists in the live scoped database).

## 28. Final Exit Criterion

| Criterion | Result |
|---|---|
| Fresh audit performed | Yes |
| Valid Category-A candidate found | Yes — Execution Hub full count-grid |
| Requirement verified | Yes (§5) |
| Backend verified | Yes (§6) |
| Existing contract reused | Yes — no contract change |
| No backend API invented | Confirmed |
| No backend modification | Confirmed — `git diff --stat -- services/orchestrator/` empty |
| No fake fallback | Confirmed — all fields non-nullable, real 0 is real data |
| No new WebSocket | Confirmed — reuses FE-9 realtime resync |
| Existing tests preserved | Confirmed — 0 modified, 0 weakened |
| Typecheck passes | Clean |
| Full suite passes | 299/299 (15 files), up from 294 |
| Build passes | Yes |
| FE-16 preserved | Yes |
| FE-17 preserved | Yes |
| FE-18 preserved | Yes |
| FE-19 preserved | Yes |
| **Overall** | **PASS** |
