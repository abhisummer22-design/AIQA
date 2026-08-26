# AIQA — Phase 4 — FE-22 Implementation Report

## 1. Status

**PASS.**

## 2. Objective

Expose BE-10's real, project-scoped Test Intelligence data (`GET /api/v1/projects/:project_id/intelligence/tests` → `TestIntelRow[]`) in the frontend, using the currently-selected project, with no legacy-endpoint fallback and no fabricated data anywhere in the chain.

## 3. Fresh Frontend Audit

Inspected before writing any UI code:
- `forentend/src/types/contract.ts` — confirmed no `TestIntelRow` type existed yet; confirmed the existing `Finding`/`FailureCluster` types as the closest analogous shape/naming convention.
- `forentend/src/services/aiqaApi.ts` — confirmed the `findings`/`audit`/`evidence` namespaces as the established project-scoped-method pattern (`fn(projectId, ..., signal) => requestJson<T>('/projects/${projectId}/...', { signal })`); confirmed zero references to `/intelligence/*` anywhere.
- `ProjectContext.tsx`/project-switcher — confirmed `useProject()` exposes `selectedProjectId`/`selectedProject`, already the single source of truth every scoped page (`AuditPage`, `EvidencePage`, `ExecutionHubPage`, `ReportsPage`) reads from.
- **`ReportsPage.tsx`** — read in full. Its own `SectionHeader` title is **`"Findings & Quality Intelligence"`** — the "Quality Intelligence" half of that title had no real content behind it until now (the page only ever rendered Findings). This is a strong, pre-existing, in-repo signal that Test Intelligence belongs on this exact page.
- **`TestCaseValidatorPage.tsx`** — read in full and deliberately **not** used. Its own disclosure text explicitly scopes itself to *pre-execution* "requirement traceability, quality scoring, and duplicate detection" — a different concept from `TestIntelRow`'s real *post-hoc* (historical-run-derived) risk/value/flakiness scoring. Reusing that page would have misrepresented what the data actually is — the exact reasoning this same repository already applies to keep Findings (post-hoc failure clustering) distinct from pre-execution validation.
- **`DashboardPage.tsx`** — read for its `formatPct`/`formatMs` null-safe formatters (reused conceptually — see §5) and confirmed as explicitly out of scope (§30/§36 of the prompt: "do not redesign Dashboard").
- Existing data-fetching/loading/error/empty-state patterns (`StateCard`, `EmptyState`, `renderResourceErrorState`, `ApiError`, abort-controller-per-fetch, `null` = not-yet-loaded vs `[]` = real-empty) — all reused verbatim from `ReportsPage.tsx`'s own existing Findings implementation, not reinvented.
- Existing test patterns in `ReportsPage.test.tsx` — reused `mockBackend`/`renderApp`/`verifyKey`/`openAndSelectProject` helpers exactly as FE-19 and prior units did.

**Decision**: add a new "Test Intelligence" section to the existing `ReportsPage.tsx`, below the existing Findings section, as an independent resource (own fetch, own state, own loading/error/empty handling) rather than creating a new page or repurposing `TestCaseValidatorPage.tsx`.

## 4. Backend Contract Used

```
GET /api/v1/projects/:project_id/intelligence/tests
```
Response: `TestIntelRow[]` — `id`, `name`, `risk_score`, `failure_rate`, `dependency_count`, `execution_cost_ms`, `value_score`, `flakiness_score`, `window_runs`, `window_failures` (every numeric field `number | null`). Verified live against the real running backend this session (§17) — confirmed the actual response shape matches this exactly, field-for-field.

## 5. Frontend Contract

Added to `forentend/src/types/contract.ts`, immediately after the `Finding`/`FailureCluster`/`ReviewFindingRequest` block:
```ts
export interface TestIntelRow {
  id: string;
  name: string;
  risk_score: number | null;
  failure_rate: number | null;
  dependency_count: number | null;
  execution_cost_ms: number | null;
  value_score: number | null;
  flakiness_score: number | null;
  window_runs: number | null;
  window_failures: number | null;
}
```
Exact field set, exact nullability, no renaming, no added/invented fields.

## 6. API Client

Added to `forentend/src/services/aiqaApi.ts`, a new `intelligence` namespace mirroring `audit`/`findings`'s exact shape:
```ts
intelligence: {
  tests: (projectId: string, signal?: AbortSignal) =>
    requestJson<TestIntelRow[]>(`/projects/${projectId}/intelligence/tests`, { signal }),
},
```
No new request helper, no new error handling — reuses the existing `requestJson` transport every other method already uses. No reference to the legacy `/intelligence/tests` route exists anywhere in this file (verified by search, §20).

## 7. UI Implementation

`ReportsPage.tsx`: added a new section below the existing Findings panel, with its own heading ("Test Intelligence", a live test count badge, and a one-line disclosure: *"Real, deterministic risk/value/flakiness scoring computed from this project's actual task execution history — not an AI prediction."*) and a compact table (`Test | Risk | Failure Rate | Value | Flakiness | Exec. Cost | Dependencies | Window`), styled with the exact same `#0B1120`/`border-slate-800` panel language, `text-xs font-mono` table conventions, and `StateCard`/`EmptyState`/`renderResourceErrorState` components already used by the Findings section and by `ExecutionHubPage.tsx`'s own task table. No new component was created; no existing component was modified beyond `ReportsPage.tsx` itself.

## 8. Project Switching

`selectedProjectId` (from the existing `useProject()`) drives a dedicated `useEffect` — structurally identical to the pre-existing Findings effect — that: aborts any in-flight test-intelligence request, resets `testIntel` to `null`, and (if a project is selected) calls `fetchTestIntelligence(selectedProjectId, false)`, which calls `aiqaApi.intelligence.tests(selectedProjectId, controller.signal)`. A late-arriving response for an old project is discarded via the same `if (controller.signal.aborted) return;` guard the Findings fetch already uses, so a stale project's rows can never overwrite the newly-selected project's state — live-verified in §17 (A → B → A).

## 9. Loading State

`testIntelLoading`/`testIntelRefreshing` drive the exact same `StateCard` component Findings uses (spinning icon, "LOADING…" badge) — no new loading primitive, no fake/skeleton rows.

## 10. Error State

`testIntelError` (an `ApiError`, same type Findings uses) is passed to the existing `renderResourceErrorState` helper with test-intelligence-specific copy (`forbiddenTitle`, `offlineDescriptionSuffix`, `genericTitle`) — the same function, the same 401/403/offline/generic branches `AuditPage.tsx`/`EvidencePage.tsx`/`ExecutionHubPage.tsx` already use. Live-verified against a real stopped backend (§17, Scenario E): "Orchestrator unreachable — Unable to reach the AIQA Orchestrator to load test intelligence," with a real Retry button, not a fake fallback.

## 11. Empty State

`testIntel !== null && testIntel.length === 0` renders the existing `EmptyState` component with: *"No test intelligence available for this project yet"* / *"Risk, value, and flakiness scoring is computed after a task reaches a terminal state (pass/fail/timeout/etc.) during a completed run — none has happened yet for this project."* Live-verified against a real project with zero `quality_nodes` rows (§17, Scenario D) — no sample rows, no placeholder scores.

## 12. Null Handling

Two local, null-safe formatters were added (matching `DashboardPage.tsx::formatPct`/`formatMs`'s exact logic, duplicated per this codebase's established per-page-formatter convention rather than extracted into a new shared util):
```ts
function formatPct(ratio: number | null): string { if (ratio === null) return '—'; return `${(ratio * 100).toFixed(1)}%`; }
function formatMs(ms: number | null): string { if (ms === null) return '—'; if (ms < 1000) return `${Math.round(ms)}ms`; return `${(ms / 1000).toFixed(1)}s`; }
```
`dependency_count`/`window_runs`/`window_failures` use `?? '—'`/an explicit `=== null` check, never a coerced `0`. Verified by a dedicated test with every field `null` (§13, item 6) asserting every cell renders `'—'` and the row's text never contains `NaN`, `0%`, or `0ms`.

## 13. Tests Added

New `describe('ReportsPage — FE-22 real Test Intelligence exposure', ...)` block in `ReportsPage.test.tsx`, 6 tests:
1. *item 1* — the API client calls the real scoped URL (`/api/v1/projects/{id}/intelligence/tests`) with the selected project's real id, and never calls the legacy unscoped `/intelligence/tests`.
2. *item 2* — a real `TestIntelRow` renders all 8 columns with the correct formatted values (`82.0%`, `60.0%`, `71.0%`, `25.0%`, `4.5s`, `3`, `10 runs, 6 failed`).
3. *item 3* — a real empty `[]` response renders the truthful empty state, not sample rows.
4. *item 4* — a real `403` renders the existing forbidden state, not fake intelligence.
5. *item 5* — switching Project A → Project B calls the API with each project's own id and never leaves Project A's row visible under Project B.
6. *item 6* — an all-`null` row renders `'—'` for every numeric cell, never `NaN`/`0%`/`0ms`, and never crashes.

A `testIntelByProjectId` fixture option was added to the file's existing `mockBackend` helper (mirroring `findingsByProjectId`'s exact shape) so `/intelligence/tests` is explicitly stubbed rather than silently falling through to the mock's project-list catch-all (which was returning the wrong shape for this endpoint before this fix — a test-fixture gap, not a production issue, corrected as part of writing these tests).

## 14. Test Results

Isolated file:
```
npx vitest run src/components/pages/ReportsPage.test.tsx
```
```
Test Files  1 passed (1)
     Tests  23 passed (23)
```
(17 pre-existing + 6 new, all passing on the first run — zero pre-existing test modified in a way that changed its assertions.)

Full suite:
```
npx vitest run
```
```
Test Files  15 passed (15)
     Tests  305 passed (305)
```
(up from 299 at the end of FE-21/BE-10 — net +6.)

## 15. Typecheck

```
npx tsc --noEmit
```
Clean, no output, run after the contract/API-client change, after the `ReportsPage.tsx` change, and after the test additions. No `any` was introduced.

## 16. Build

```
npx vite build
```
Succeeded: `2711 modules transformed`, `dist/assets/index-BYMFoEii.js` (1,025.29 kB, gzip 291.58 kB) — the pre-existing chunk-size warning is unchanged and unrelated to this change. No build configuration was modified.

## 17. Manual Verification

All scenarios genuinely performed, live, against the real backend and real data (no seeding — real projects and real `quality_nodes` rows already present from prior units' own manual verification sessions):

1. Minted a fresh real Operator API key directly in `api_keys`, started the up-to-date debug backend (`services/orchestrator/target/debug/aiqa-orchestrator.exe` — rebuilt during BE-10, already includes the new route; the stale `release` binary was checked and correctly *not* used), confirmed via `curl` that the real scoped endpoint returns real `TestIntelRow[]` data for "AIQA Core Platform" (`project_id=9860d454-...`).
2. **Scenario A** — started the frontend, selected "AIQA Core Platform" in the real UI: the page rendered **"TEST INTELLIGENCE — 8 TESTS"** with 8 real rows (`mobile-task-1`, `task-1`, `Health check via real pytest executor`, `task-17`, `Task One`, `task-34`, `beta`, `alpha`), each with real, distinct risk/failure-rate/value/flakiness/cost/dependency/window values matching the live API response exactly.
3. **Scenario B** — switched to "Payments Gateway QA" in the real UI: the table changed to **"1 TEST"** (`Task One`, `risk 0.1%`), and none of Project A's 8 rows remained visible.
4. **Scenario C** — switched back to "AIQA Core Platform": the real 8-row dataset returned, byte-identical to Scenario A's output.
5. **Scenario D** — selected "First Project For A New Admin" (a real project with zero `quality_nodes` rows, confirmed directly against the live SQLite database beforehand): the UI rendered **"0 TESTS"** and the real truthful empty-state message — no sample rows.
6. **Scenario E** — stopped the real backend process, selected "AIQA Core Platform" again: the UI rendered the real "Orchestrator unreachable" error state for the Test Intelligence section (distinct copy: *"Unable to reach the AIQA Orchestrator to load test intelligence."*), with a real Retry button — not a fake fallback, not silently swallowed.

All five scenarios were **live verified**, not merely automated-only. Backend process and frontend dev server were both stopped afterward; the manually-minted API key was revoked.

## 18. Backend Boundary

```
git diff --stat -- services/orchestrator/
```
Output is exactly BE-10's own diff (`api.rs` +41, `api_tests.rs` +186), byte-identical before and after this unit's work — **zero backend files were touched by FE-22.**

## 19. Regression Verification

- **FE-16** (Test Generator truthfulness): `TestCaseGeneratorPage.tsx` untouched; its 9 tests pass.
- **FE-17** (dependency/DAG editor): `ExecutionHubPage.tsx` untouched; its tests pass.
- **FE-18** (approval/rejection attribution): untouched; its tests pass.
- **FE-19** (finding review attribution): `ReportsPage.tsx`'s *existing* Findings behavior (list/detail/review/attribution) was read but not altered — only new, additive code was inserted; all 17 pre-existing `ReportsPage.test.tsx` tests pass unmodified.
- **FE-20** (Execution Hub count grid): untouched; its tests pass.
- **FE-21** (audit conclusions): the conclusion that the legacy `/intelligence/tests` route is unsafe for this frontend's multi-project model was not reopened — this unit builds on top of it, using BE-10's new scoped route instead, exactly as FE-21 recommended.
- Full suite: 305/305 passing, 15 files — confirms all of the above collectively.

## 20. Fabrication Audit

```
grep -niE "fake|mock|demo|sample|fabricated|placeholder|hardcoded" ReportsPage.tsx aiqaApi.ts contract.ts
grep -niE "Math\.random|setTimeout" ReportsPage.tsx
grep -Rni "'/intelligence/tests'" forentend/src   (excluding test files)
```
Every `fake|mock|demo|...` match is either a pre-existing/new doc-comment explaining that fake data was *not* used, or the pre-existing, legitimate `placeholder` HTML attribute on the (unrelated, pre-existing) Findings search input. The `setTimeout` match is the pre-existing Findings realtime-resync debounce timer, untouched. The legacy-route search returned **zero** matches in production code — no fallback to `/intelligence/tests` exists anywhere. No fake runtime intelligence was introduced.

## 21. Files Changed

- `forentend/src/types/contract.ts` (+19 lines — `TestIntelRow`)
- `forentend/src/services/aiqaApi.ts` (+9 lines — `intelligence.tests` method + type import)
- `forentend/src/components/pages/ReportsPage.tsx` (new state/fetch/effect/render for the Test Intelligence section, plus doc-comment updates)
- `forentend/src/components/pages/ReportsPage.test.tsx` (+~130 lines — 6 new tests, `testIntelByProjectId` mock fixture option, `TestIntelRow` import)

## 22. Files Not Changed

**`services/orchestrator/**` — confirmed untouched (§18).** Also not changed: `DashboardPage.tsx`, `TestCaseGeneratorPage.tsx`, `TestCaseValidatorPage.tsx`, `AuditPage.tsx`, `EvidencePage.tsx`, `ExecutionHubPage.tsx`, `AIQAContext.tsx`, `RealtimeContext.tsx`, `ProjectContext.tsx`, `routeConfig.tsx`, `Sidebar.tsx` — no new route, no new nav entry, no unrelated page touched.

## 23. Remaining Limitations

- No realtime auto-refresh was wired for the Test Intelligence section (only initial-load-on-project-select and the existing page-level manual "Refresh" button, now extended to refresh both Findings and Test Intelligence together). Findings on this same page does auto-resync on realtime `RUN_COMPLETED` events; Test Intelligence does not. This was a deliberate "smallest truthful UI" choice, not an oversight — reusing the realtime resync for a second, independent resource was judged an unnecessary scope increase for this unit, and the manual Refresh button is a real, working substitute, not a missing capability.
- `window_runs`/`window_failures` reaching `0`/`0` (a test with a completed run but no matching-status history yet) renders `"0 runs, 0 failed"` rather than `'—'` — this is intentionally correct, not a bug: `0` here is a genuine real count (confirmed live for `task-34`, §17), not a null/missing value.
- No sorting/filtering was added to the Test Intelligence table (Findings' existing search box was not extended to it) — out of scope for "smallest truthful UI"; the table shows the real backend's own already-sorted order (`ORDER BY r.score DESC` in `store.rs::list_test_intelligence`).

## 24. Final Recommendation

**Test Intelligence is now genuinely exposed**, end-to-end, with no fake layer: real backend computation → BE-10's real project-scoped API → the real selected project id → the real frontend API client → the real HTTP response → real React state → real rendered UI, verified both by automated tests (23/23 in this file, 305/305 overall) and by live manual verification across all 5 required scenarios. No further backend work is required for this specific capability. Any future enhancement (realtime auto-refresh, sorting/filtering, a dedicated risk-focused view) would be a genuinely new, separate unit — not a gap in what FE-22 was scoped to deliver.
