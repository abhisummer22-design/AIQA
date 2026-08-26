# FE-26 — Average Run Duration KPI Implementation Report

## Status

**PASS.**

## Objective

Integrate the real, already-shipped backend field `execution_avg_run_duration_ms` (BE-12b) into `DashboardPage.tsx` so Average Run Duration displays using real, project-scoped API data — nothing calculated client-side, no new request, no new KPI concept invented.

## Fresh Audit Findings

Performed by direct re-inspection before any edit, answering each mandated question:

1. **Does `GET /api/v1/projects/:project_id/kpis` currently return `execution_avg_run_duration_ms`?** Yes — re-read `api.rs` lines 981-1028 directly: `KpisResponse` has the field, `get_project_kpis` populates it via `state.store.dashboard_avg_run_duration_ms(&pid)`.
2. **Is it still `Option<f64>`/nullable?** Yes, unchanged.
3. **Is the calculation still based on real `runs.started_at`/`runs.finished_at`?** Yes — re-read `store.rs::dashboard_avg_run_duration_ms` directly: `AVG((julianday(r.finished_at)-julianday(r.started_at))*86400000.0) FROM runs r WHERE r.project_id=? AND r.started_at IS NOT NULL AND r.finished_at IS NOT NULL`.
4. **Does the calculation remain project-scoped?** Yes — same `WHERE r.project_id = ?` binding, same unmodified `require_scoped_project` gate on the handler.
5. **`types/contract.ts::KpisResponse`** — confirmed genuinely **missing** the field before this unit (only had `execution_reliability`, `execution_efficiency_ms`, `project_id`, `generated_at`) — matches the fresh audit's expectation exactly.
6. **`aiqaApi.ts::dashboard.kpis(...)`** — confirmed it already calls `GET /projects/${projectId}/kpis` and returns `Promise<KpisResponse>` — no new method needed; adding the field to the TS interface is sufficient for the existing call to receive and type it.
7. **`DashboardPage.tsx`** — confirmed the 6-card KPI grid (`#dashboard-kpi-row`) holds only integer counts; the existing "Execution efficiency" line (a single small text row below the middle section, using `formatMs(kpis.execution_efficiency_ms)`) is the exact architectural match for a second nullable-millisecond KPI — same shape, same formatter, not a count. `formatMs(ms: number | null): string` confirmed already exists (lines 164-168) and already returns `'—'` for `null`, `${Math.round(ms)}ms` under 1000, `${(ms/1000).toFixed(1)}s` at or above 1000.
8. **`DashboardPage.test.tsx`** — confirmed existing patterns: `kpisFor(project, overrides)` fixture helper, `mockBackend` with `kpisByProjectId`, `MockWebSocket`/`realEvent` for realtime, `openAndSelectProject` for project switching, `kpiRow()`/`dashboardRoot()` scoped query helpers, and item 5/6's existing assertion `within(dashboardRoot()).getByText('1.5s')` as the established pattern for asserting a formatted KPI value outside the card grid.
9. **Realtime**: confirmed Dashboard's existing debounced any-event resync (`resyncTimeoutRef`, `EVENT_RESYNC_DEBOUNCE_MS = 400`, subscribing via `realtime.subscribe`) already refetches the full `/kpis` response on any real project event — the new field needs no dedicated wiring.
10. **No fake/hardcoded data** was found anywhere in current Dashboard production code (re-confirmed via search, §Fake/Stub Audit).

**No STOP condition applies** — the backend field exists, is correctly named, correctly nullable, correctly project-scoped, and the frontend contract/architecture can truthfully receive and display it with the smallest possible change.

## Backend Contract Verification

Exact field verified: `execution_avg_run_duration_ms: Option<f64>` on `KpisResponse`, returned by `GET /api/v1/projects/:project_id/kpis`. `null` when no qualifying run exists; a real millisecond `f64` otherwise. Unchanged since BE-12b — this unit made **zero backend changes** (confirmed, §Files NOT Changed).

## Files Changed

1. `forentend/src/types/contract.ts` — `KpisResponse` gains exactly one additive field: `execution_avg_run_duration_ms: number | null;` (with a doc comment citing the real backend source). No existing field renamed or modified.
2. `forentend/src/components/pages/DashboardPage.tsx` — the existing "Execution efficiency" row is extended with a second `<span>` for "Average run duration", reusing the existing `formatMs` helper unmodified; module doc comment updated to document the new `/kpis` field. No new component, no new state, no new request.
3. `forentend/src/components/pages/DashboardPage.test.tsx` — `kpisFor`'s default fixture gains `execution_avg_run_duration_ms: null` (required for the now-non-optional TS field); one new `describe` block, 6 new tests.

No other file was changed. `aiqaApi.ts` was confirmed unmodified (`git diff --stat` for that file is identical before and after this unit — the existing +11 lines are FE-22's own prior `intelligence.tests` addition, not new).

## Frontend Data Flow

```
runs.started_at / runs.finished_at (real SQLite columns, unmodified)
  → store.rs::dashboard_avg_run_duration_ms (real SQL AVG, unmodified, BE-12b)
  → api.rs::get_project_kpis (unmodified handler, BE-12b)
  → GET /api/v1/projects/:id/kpis (unmodified route)
  → KpisResponse (Rust struct, unmodified)
  → aiqaApi.dashboard.kpis(projectId, signal) (unmodified call, existing since FE-1)
  → types/contract.ts::KpisResponse (THIS UNIT — added the missing field)
  → DashboardPage.tsx's existing `data.kpis` state (unmodified fetch logic in `fetchAll`)
  → THIS UNIT — new <span> rendering formatMs(kpis.execution_avg_run_duration_ms)
```
Every link except the two marked "THIS UNIT" was already real and already wired before this session began.

## KPI Rendering Logic

```jsx
<span>Average run duration: <strong className="text-slate-300">{formatMs(kpis.execution_avg_run_duration_ms)}</strong></span>
```
No calculation, no derivation, no client-side averaging — the value passed to `formatMs` is exactly the real backend field, unmodified in transit.

## Null Handling

`formatMs(null)` returns `'—'` (pre-existing behavior, not touched). Verified by dedicated test (§Tests Added item 2) that the rendered text contains `'—'` and does **not** match `/0ms|0\.0s|\b0\b/` — i.e., not `0ms`, not `0.0s`, not a bare `0` anywhere in that KPI's own text.

## Project Isolation

Reuses the existing Dashboard request lifecycle entirely — no parallel state management was added (none was needed): the same `fetchAll(projectId, isRefresh)` call that already fetches `dashboard`/`kpis`/`executions` together, the same `abortRef`-based stale-response guard, the same project-switch `useEffect` that resets `data` to `null` before the new project's fetch begins. Live-verified (Scenario B below) and automated-tested (§Tests Added item 4) that switching projects shows only the newly-selected project's real value, with the previous project's formatted string absent from the DOM.

## Realtime Behavior

No new realtime infrastructure was added — confirmed by diff: `RealtimeContext.tsx` untouched, no new `useRealtime()` call, no new subscription, no new timer. The existing debounced any-event resync effect (unchanged) already refetches the full `KpisResponse` object on any real project event, and the new field rides along automatically since it's part of that same object. Live-verified (Scenario C) and automated-tested (§Tests Added item 5).

## Duplicate Request Audit

`GET .../kpis` is called exactly once per Dashboard load/resync cycle, same as before this unit — verified by a dedicated test (§Tests Added item 6) counting `fetchMock` calls matching `/kpis` after a full load and asserting exactly `1`. No second, independent request was introduced for this field.

## Fake/Stub/Fabrication Audit

```
grep -niE "fake|mock|demo|sample|fabricated|hardcoded" DashboardPage.tsx   (production only)
grep -n "execution_avg_run_duration_ms" DashboardPage.tsx contract.ts aiqaApi.ts
grep -n "Math\.random|?? 0|\|\| 0" DashboardPage.tsx
```
Every `fake|mock|...` match is a pre-existing doc comment documenting an already-removed prior fabrication (health score, ETA, `AIQAContext` project list, `RunTriggerModal`) — none new, none near the new field. The one `?? 0` match in the file (`count(map, key) { return map[key] ?? 0; }`) is the pre-existing, unmodified helper for real integer **count** lookups (task/execution counts) — a legitimate "0 if this status key is absent from the map" pattern, structurally unrelated to and never applied to the new nullable-duration field. **Zero** `?? 0`, `|| 0`, `Math.random`, or any other fallback pattern exists anywhere near `execution_avg_run_duration_ms` in any production file — confirmed by direct grep, not assumed. `aiqaApi.ts` diff confirmed byte-identical to before this unit (no new method, no fallback logic added).

## Tests Added

New `describe('DashboardPage — FE-26 real Average Run Duration KPI', ...)` block in `DashboardPage.test.tsx`, 6 tests:
1. A real `execution_avg_run_duration_ms: 20_000` renders as `'20.0s'` — the exact same `formatMs` behavior already proven for `execution_efficiency_ms` in the pre-existing item 5/6 test, not a new formatting expectation invented for this field.
2. `null` renders the existing `'—'` convention; explicitly asserts the text does **not** match `0ms`/`0.0s`/a bare `0`.
3. Existing `execution_reliability`/`execution_efficiency_ms` render correctly and unchanged alongside the new field in the same response.
4. Project-switch isolation: Project A (`5_000` → `'5.0s'`) → Project B (`60_000` → `'60.0s'`) → confirm `'5.0s'` is no longer present anywhere.
5. The existing debounced realtime resync (same `MockWebSocket`/`realEvent` mechanism `RealtimeContext.test.tsx`/the pre-existing Dashboard realtime tests already use) delivers an updated `execution_avg_run_duration_ms` (`5_000` → `45_000`) with no manual action, using a real `RUN_COMPLETED`-typed event; confirms exactly one live `MockWebSocket` connection (unchanged from the pre-existing item 21/22 test).
6. Exactly one `/kpis` request occurs per load — no duplicate request introduced by this field.

## Full Test Results

Isolated file:
```
npx vitest run src/components/pages/DashboardPage.test.tsx
```
```
Test Files  1 passed (1)
     Tests  32 passed (32)
```
(26 pre-existing + 6 new, all passing on the first run.)

Full suite:
```
npx vitest run
```
```
Test Files  16 passed (16)
     Tests  325 passed (325)
```
(up from 319 at the end of FE-25 — net +6, zero pre-existing test modified except the one required, additive `kpisFor` default field.)

## Typecheck Result

```
npx tsc --noEmit
```
Clean, no output. One expected, immediately-fixed type error occurred mid-implementation (`kpisFor`'s object-literal default was missing the now-required field) — resolved by adding `execution_avg_run_duration_ms: null` to that fixture's defaults; the final state is fully clean.

## Build Result

```
npx vite build
```
Succeeded: `2711 modules transformed`, `dist/assets/index-D0C9SqLq.js` (1,026.24 kB, gzip 291.96 kB) — the pre-existing chunk-size warning is unchanged and unrelated.

## Manual Verification

All 4 scenarios genuinely performed live, against the real running backend (the same debug binary rebuilt during BE-12b, already including the field) and two real browser tabs, using a freshly-minted real Operator API key:

- **Scenario A**: opened "Payments Gateway QA" — real Dashboard text showed *"Execution efficiency: 155ms avg task duration • Average run duration: 1.3s"* — a genuine real backend value, correctly formatted.
- **Scenario B**: switched to "AIQA Core Platform" via the real project dropdown — the display changed to a genuinely different real value (*"5.5s"*), with no trace of Payments Gateway QA's *"1.3s"* remaining anywhere on the page.
- **Scenario C**: with tab "seed" left idle on Dashboard (no click, no reload), created a real execution (`FE-26 dashboard avg run duration check`) in a separate tab (`tab-1`, same project). Without any manual action on tab "seed", its displayed Average Run Duration changed from the pre-run baseline (*"5.5s"*) to a new real value (*"5.3s"*) reflecting the just-completed run's real contribution to the average — driven entirely by the existing realtime → debounced-resync → `/kpis`-refetch chain.
- **Scenario D**: stopped the real backend process, then forced a real reconnection attempt (reload + re-click Verify Connection). The app rendered the real, pre-existing "Orchestrator unreachable" error state — no fabricated duration value, no fake fallback, appeared anywhere.

Cleanup performed: second tab closed, backend process and frontend dev server both stopped, the manually-minted API key revoked.

**Live-verified**: Scenarios A, B, C, D as described above. **Automated-verified only**: the exact debounce-timing/no-duplicate-request assertions (test items 5/6) — inherently more precise to prove via controlled mocks than via live wall-clock observation, and already covered with full rigor there.

## Regression Verification

Full suite 325/325 passing (319 → 325, +6 new, 0 broken). The pre-existing item 5/6 test (which already asserted `execution_efficiency_ms`'s `'1.5s'` rendering) passes unmodified, confirming the new field's addition to the same response/render path did not disturb it. All 26 pre-existing Dashboard tests pass unmodified.

## Limitations

None specific to this unit. The Average Run Duration KPI is now genuinely real, project-scoped, null-honest, and realtime-updating end to end — no further frontend work is required to make this specific field visible and correct.

## Final Recommendation

FE-26 successfully closes the loop BE-11 → BE-12b → FE-26 for exactly one evidence-backed KPI: Average Run Duration is now real, visible, correctly formatted, correctly null-safe, project-isolated, and rides the existing realtime path with zero new requests or infrastructure.

---

**Implementation Status: PASS**

**Recommended Next Unit:** A fresh audit at this point finds no further evidence-backed Dashboard KPI gap. BE-11's own audit (re-confirmed unchanged by this session's fresh checks) already classified every other candidate concept — pass/fail/execution/failure trend series, project-level flakiness/risk aggregation, run frequency, test coverage, project health score, API latency, model/worker health, UI stability, report counts — as requiring either a missing data model or an unresolved product-design decision (bucket size, aggregation methodology, time window) that this session has no authority to invent. **No Category-A next implementation unit identified. STOP pending a new product requirement or backend capability.**
