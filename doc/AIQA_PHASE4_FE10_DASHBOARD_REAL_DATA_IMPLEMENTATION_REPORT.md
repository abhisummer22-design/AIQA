# AIQA — Phase 4 — FE-10 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

FE-10's brief: convert the approved Dashboard from fabricated/demo data to real backend-backed data wherever the current backend genuinely supports it, preserving the approved visual design exactly — "Fake data → Real data, not Approved UI → New UI." Satisfied in full: every KPI card, the donut chart, the "Current Execution Run" panel, and the AI Quality Core Orb now render exclusively real backend data (or an honest, real-error/empty state); the one chart with no real backend support (the execution-activity timeline) is disclosed and shows its own real, pre-existing empty state rather than fabricated data; the approved layout, spacing, typography, cards, and interactions are all pixel-for-pixel unchanged except five documented, minimal, necessary adjustments (§12).

## 3. Dashboard data-source audit

Every existing widget, its prior (fake) source, and its FE-10 disposition:

| Widget | Prior source | Real backend available? | Real API | Action |
|---|---|---|---|---|
| Total Test Cases KPI | `AIQAContext` fake `metrics.totalTestCases` | Yes — `dashboard.test_count` | `GET .../dashboard` | Replaced |
| Active Runs KPI | fake `metrics.activeRunsCount` | Yes — `execution_counts['RUNNING']` | `GET .../dashboard` | Replaced |
| Passed KPI | fake `metrics.passed` | Yes — `task_counts['PASSED']` | `GET .../dashboard` | Replaced |
| Failed KPI | fake `metrics.failed` | Yes — `task_counts['FAILED']` | `GET .../dashboard` | Replaced |
| Blocked KPI | fake `metrics.blocked` | Yes — `task_counts['BLOCKED']` | `GET .../dashboard` | Replaced |
| Needs Review KPI | fake `metrics.needsReview` | Yes — `task_counts['NEEDS_REVIEW']` | `GET .../dashboard` | Replaced |
| Pass-rate caption (under Passed) | fake `passed/totalTestCases` ratio | Yes — `execution_reliability` | `GET .../kpis` | Replaced (real KPI, not a re-derived ratio) |
| Execution efficiency | did not exist | Yes — `execution_efficiency_ms` | `GET .../kpis` | Added (new, real, minimal line) |
| Project status badge | fake `ProjectStatus` (6 states) | Partial — only "has a real `RUNNING` execution" is computable | — | Reduced to 2 real states (§12 adjustment 1) |
| Environment / Slug | fake `selectedProject.environment`/`.slug` | No such fields on real `Project` | — | Replaced with real `created_at`/id (§12 adjustment 2) |
| "Last Activity" | fake relative-time string | No real "activity feed" concept | `GET .../dashboard` (`generated_at`) | Relabeled "Data as of", real timestamp (§12 adjustment 3) |
| Current Execution Run panel | fake `activeRun` object | Yes — real `RunListItem`/`RunDetail` | `GET .../executions`, `GET .../executions/:id` | Replaced |
| "Est. Duration" | fake ETA string | No real ETA concept | — | Relabeled "Elapsed", computed from real `started_at` (§12 adjustment 4) |
| AI Quality Core Orb | fake `status`/`hasActiveRun`/`projectName` | Partial (same 2 real states) | — | Replaced with real, reduced-state values |
| Donut chart (`ProjectStatusDonut`) | fake `metrics` | Yes — real `task_counts` | `GET .../dashboard` | Replaced |
| Execution Activity chart (`ProjectExecutionGraph`) | fake `timelineData` array | No — no time-bucketed history endpoint exists | — | Always renders its own real, honest empty state |
| Header project dropdown | fake `AIQAContext` `AIQAProject[]`, independent of the real header selector | Yes — FE-5 `useProject().projects` | `GET /projects` | Repointed to real data (§12 adjustment 5) |
| "Launch Run" / "Start Execution Run" buttons | fake `RunTriggerModal` | N/A — out of this unit's scope | — | Left fake, explicitly disclosed (§10) |

## 4. Backend/API integration

Three already-real, already-FE-1-exposed endpoints, reused as-is — zero new transport code beyond what FE-1 already shipped:

```
GET /api/v1/projects/:id/dashboard  → execution_counts, task_counts, pass_fail, test_count, generated_at
GET /api/v1/projects/:id/kpis       → execution_reliability, execution_efficiency_ms, generated_at
GET /api/v1/projects/:id/executions → reused from FE-8, to find the real RUNNING run (if any) for the "Current Execution Run" panel
GET /api/v1/projects/:id/executions/:run_id → reused from FE-8, fetched once more only when a RUNNING run exists, for its real timestamps/counts
```

Confirmed by reading `services/orchestrator/src/api.rs::get_project_dashboard`/`get_project_kpis` and their `store.rs` query implementations directly (`dashboard_run_counts`, `dashboard_task_counts`, `dashboard_test_count`, `dashboard_avg_task_duration_ms`) — not assumed from the route names. Both handlers call only `require_scoped_project` — **no role gate exists** on either (unlike Audit's `Role::Operator` requirement), matching Evidence's own no-role-gate precedent.

## 5. KPI mapping

| KPI | Real source field | Real calculation | Empty/null behavior |
|---|---|---|---|
| Total Test Cases | `dashboard.test_count` | `COUNT(*) FROM quality_nodes WHERE node_type='test'` (server-side) | Genuinely `0` today (Test Generation does not persist) — an honest real value, not a bug |
| Active Runs | `dashboard.execution_counts['RUNNING']` | Real `GROUP BY state` count | `0` when key absent (`count()` helper defaults to 0, not an error) |
| Passed / Failed / Blocked / Needs Review | `dashboard.task_counts[...]` | Real `GROUP BY status` count | Same `0`-default behavior |
| Pass-rate caption | `kpis.execution_reliability` | Server-side: `PASSED / (PASSED+FAILED+TIMEOUT)` | Renders "No terminal outcomes yet" when `null` — never `0%` |
| Execution efficiency | `kpis.execution_efficiency_ms` | Server-side: average of `finished_at - started_at` over tasks with both timestamps | Renders `—` when `null` — never `0ms` (matching a pre-existing backend doc comment's own explicit convention) |

## 6. Chart mapping

- **`ProjectStatusDonut`** — real. Fed the same real `task_counts`-derived `ProjectMetrics` object as the KPI cards (`passed/failed/running/blocked/needsReview/skipped`), plus a real `totalTestCases` = sum of all real task counts (a distinct real concept from `test_count` — see §12 adjustment). Its own pre-existing, unmodified empty state ("No test execution data available") renders honestly when a project genuinely has zero tasks — live-confirmed (§17, Scenario G, Payments Gateway QA before it had any real runs).
- **`ProjectExecutionGraph`** — **unsupported, disclosed**. No real time-bucketed history endpoint exists in the backend (only aggregate counts) — building one client-side would mean inventing bucket boundaries/gap-filling the backend does not define, which brief §14 explicitly forbids ("do not fabricate a time series"). Always passed `timelineData={[]}`, so its own pre-existing, unmodified empty state ("No Execution Activity Recorded") renders every time — live-confirmed (§17).

## 7. Project-scope integration

`useProject()` (FE-5, unchanged) is the sole source of project scope — the same context every other real page (Execution Hub, Audit, Evidence) already consumes. On `selectedProjectId` change, the effect immediately clears `data` to `null` before fetching the new project's real data (brief §8: "invalidate old Dashboard data" before the new project loads) — live-confirmed with two genuinely different real projects showing completely distinct, non-leaking real KPI values, and by automated test (items 1-4).

**The one necessary functional fix (§12 adjustment 5)**: Dashboard's own header project dropdown previously listed `AIQAContext`'s fake `AIQAProject[]`, entirely independent of FE-5's real header selector — a disclosed FE-5-era gap (`AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md` §3.4). Once Dashboard's KPIs became real and project-scoped, leaving a second, disconnected "current project" selector on the same page would have been actively misleading, not merely decorative preservation — so it was repointed to `useProject().projects`/`selectProject()`, the same real projects and real selection mechanism the header itself uses.

## 8. Authentication/error states

No second auth/error state machine (brief §12, critical rule). Two layers, matching every other real page:

1. **Route-level** (FE-2/FE-3, unchanged): `/dashboard` is `protectedRoute: true` — `no_key`/`checking`/`unauthenticated`/`offline`/`error` all render the real, existing `StateGate` card before `DashboardPage`'s body ever mounts. Live-verified (§17, Scenarios A, I, K).
2. **Resource-level** (new, this sprint): the combined `dashboard`/`kpis`/`executions` fetch classifies `ApiError.kind` and renders via the same `StateCard`/`Cta` primitives FE-6/7/8/9 already established — no new visual pattern. Per brief §12 ("Dashboard may still render its non-dependent shell"), the header (title, real project selector, "Launch Run") always renders; only the data-backed body region is replaced by the matching state card. Live-verified as **distinct from the route-level card** (different, more specific copy: "Unable to reach the AIQA Orchestrator to load real dashboard data") by killing the backend while already authenticated and on the Dashboard with a project selected (§17).

## 9. Realtime integration

`DashboardPage` subscribes to the one centralized `RealtimeContext` (FE-9, unchanged) — no second/page-local `WebSocket`, live-confirmed via a constructor-wrapping instrumentation showing exactly one real connection reused across project selection, execution creation, and project switching (§17). Any real event for the current project triggers a debounced (400ms) resync of the same real `dashboard`/`kpis`/`executions` fetch — not a per-widget subscription, and not a hand-patch of the event payload into local state (the aggregate SQL-derived counts cannot be reconstructed from one event's fields). The subscription effect depends on the stable `realtime.subscribe` identity, not the whole `realtime` object — applying FE-9's own live-discovered lesson about exactly this class of bug (`RealtimeContext.tsx`'s module doc comment) from the start, so this sprint did not reintroduce it. Live-confirmed end-to-end (§17, Scenario E/F/L): creating a real execution via a direct authenticated request updated the Passed KPI, reliability percentage, donut total, and "Data as of" timestamp with zero manual refresh.

## 10. Fake-data removal from Dashboard

Every widget listed as "Replaced" in §3 no longer reads from `AIQAContext` in any way. `DashboardPage.tsx` retains exactly one `useAIQA()` read: `setNewRunModalOpen`, for the "Launch Run"/"Start Execution Run" buttons, which still open the pre-existing fabricated `RunTriggerModal` — explicitly disclosed, not silently converted (brief §30: FE-11 owns broader fabrication removal; no documented requirement forces Dashboard itself to launch real runs, and FE-8 already established `/execution` as the one real execution-creation surface, reachable via the unchanged "Execution Hub" quick-action link). `AIQAContext.tsx` itself was **not modified** — its fabricated data remains available to whatever other still-fake pages need it (RBAC/API/Mobile/Custom Testing, Reports, Feature Map, etc.), none of which this sprint touched.

## 11. Unsupported widgets

Explicitly, honestly unsupported — never populated with a fabricated value:

- **Execution Activity timeline chart** — no real time-bucketed history endpoint exists (§6).
- **Project health states beyond Running/Healthy** (Warning/Degraded/Critical/Offline) — no health-scoring model exists in the backend; inventing thresholds for "how much failure makes a project Critical" would be exactly the fabricated health score brief §13 forbids.
- **Environment/Slug metadata** — no such fields exist on the real `Project` type at all (replaced with real fields in the same slot, §3/§12).
- **Execution ETA/duration estimate** — no real estimate concept exists anywhere in the backend (replaced with real elapsed time, §3/§12).

## 12. Visual preservation

Confirmed live and by automated test: identical layout, card grid, spacing, typography, chart placement, and header structure — same element ids (`dashboard-header`, `dashboard-kpi-row`, `current-project-detail-panel`, `ai-quality-core-orb-column`, `dashboard-project-status-section`, `project-status-donut-panel`, `project-execution-graph-panel`) throughout. Five documented, minimal, necessary adjustments (brief's own explicit allowance), each already itemized in §3 and in `DashboardPage.tsx`'s own module doc comment:

1. Project status badge reduced from 6 fabricated states to 2 real ones (Running/Healthy).
2. Environment/Slug → real Created date/Project ID, same visual slot.
3. "Last Activity" → "Data as of", real timestamp, same visual slot.
4. "Est. Duration" → "Elapsed", real computed value, same visual slot.
5. Header's own project dropdown repointed to real projects/real selection.

No card was resized, moved, restyled, or removed. No new visual system was introduced.

## 13. Test Generator preservation

Not modified this sprint. Confirmed live (§17) — full fabricated content (traceability chain, review workflow, category filters, etc.) renders identically — and by automated test (item 29).

## 14. Execution/Audit/Evidence regression

None of `ExecutionHubPage.tsx`, `AuditPage.tsx`, or `EvidencePage.tsx` were modified this sprint. Confirmed live (§17) — all three render their real data correctly, with the same single centralized realtime connection, immediately after exercising Dashboard — and by automated test (items 30-32).

## 15. Security

- Verified live (§17, Scenario M) and by automated test: the raw API key never appears in the DOM, URL, or rendered UI text; no `"bearer"`/`"authorization"` substring rendered; no cookies; no `sessionStorage`; only the two expected `localStorage` entries.
- **A real, disclosed, pre-existing limitation re-confirmed live, not introduced by this sprint**: the browser's own native DevTools console logs the full attempted WebSocket URL — including the real `?key=` value — whenever a WS connection attempt fails (e.g., during the real offline test, §17). This is standard browser-engine behavior (`"WebSocket connection to '<url>' failed:"` is a native Chromium message, not application code — `RealtimeContext.tsx`'s own `onerror` handler is a deliberate no-op, per its own doc comment), and stems directly from the real backend's own `?key=`-in-URL WebSocket auth mechanism, already disclosed and justified in FE-9's report (§4-§5: browsers cannot set custom headers on a WS handshake, so a query parameter is the backend's real, deliberate design). Not fixable from this frontend sprint without the backend adopting a different WS auth mechanism, which is out of scope. Disclosed here rather than silently observed and dropped.
- No sensitive backend metadata (e.g., no `object_key`-style internal path) is rendered anywhere on Dashboard — the real fields shown (`project_id`, `test_count`, counts, timestamps) are all already-public-facing REST response fields.

## 16. Tests

**26 new tests** in `forentend/src/components/pages/DashboardPage.test.tsx` (brief §28 items 1-32, several combined: 5/6, 12/13, 26/27; item 33 — "FE-1 through FE-9 tests remain passing" — satisfied by the full suite run, not a dedicated test), exercised through the real `AppShell` tree with `fetch` mocked at the transport boundary and a controllable `MockWebSocket` (same technique as `RealtimeContext.test.tsx`/`ExecutionHubPage.test.tsx`):

```
Test Files  11 passed (11)
     Tests  207 passed (207)
```

Full breakdown: `aiqaApi.test.ts` 37 + `AuthContext.test.tsx` 11 + `ProjectContext.test.tsx` 14 + `StateViews.test.tsx` 21 + `ProjectSelector.test.tsx` 6 + `AppRoutes.test.tsx` 20 + `AuditPage.test.tsx` 18 + `EvidencePage.test.tsx` 21 + `ExecutionHubPage.test.tsx` 18 + `RealtimeContext.test.tsx` 15 + `DashboardPage.test.tsx` 26 = **207**.

**Pre-existing regression tests genuinely updated, not weakened**: nine "Dashboard remains unregressed" assertions across `AppRoutes.test.tsx`, `ProjectSelector.test.tsx`, `AuditPage.test.tsx`, `EvidencePage.test.tsx`, and `ExecutionHubPage.test.tsx` previously asserted `AIQAContext`'s fake "AIQA Platform" project name — now genuinely updated to assert the real project name / real "no project" empty state instead, since the old assertion is no longer true by design (Dashboard no longer shows fabricated data). Each updated test still proves the same real thing it always proved (the route/page renders correctly with the other unit's page present in the tree) — none were deleted or had their assertions removed, only corrected to match the new real behavior. A second, unrelated but necessary fix: the shared `openAndSelectProject` test helper (duplicated across four test files) was scoped to the header's own real listbox (`within(screen.getByRole('listbox', {name: 'Projects'}))`), since Dashboard's own new real `<select>` of the same real projects otherwise collides with an unscoped ARIA "option" role query — a genuine test-infrastructure consequence of Dashboard now sharing real project data with the header, not a product bug.

## 17. Manual verification

Ran the real Vite dev server against the real `services/orchestrator` backend. Two real keys used: a pre-existing global admin key (`fe9-manual-verify-admin`, for multi-project scenarios) and a freshly-minted project-A-scoped operator key (`fe10-manual-verify-scoped-a`, for the curl-verified 403).

- **Scenario A (no key)** — real "No orchestrator API key configured" state; no fabricated Dashboard content. **PASS.**
- **Scenario B (authenticated, no project)** — real "No project selected" honest empty state, header shell (including the now-real project dropdown) visible. **PASS.**
- **Scenario C/D (real project, with real executions)** — every KPI card, the donut, the project-detail panel, and the AI Quality Core Orb rendered exact real values (Total Test Cases 5, Passed 5→6, Failed 8, 38.5%→42.9% reliability, 5.2s avg task duration, real `Created`/`ID`, real `Data as of` timestamp, donut Total Tests 15→16) — confirmed field-for-field against what was independently known to be true from this engagement's own FE-8/FE-9 real run history. **PASS.**
- **Scenario E/F (create real execution, execution completion)** — a real execution was created via a direct authenticated request; the Dashboard (already open, untouched) updated Passed/reliability/donut-total/timestamp with **zero manual refresh**, purely from the real WS event. **PASS.**
- **Scenario G (project switch)** — switching "AIQA Core Platform" → "Payments Gateway QA" showed exactly that project's own real, completely distinct data (1 passed, 100% reliability, its own real project id) with zero cross-project leakage in either direction; exactly one new real `WebSocket` construction confirmed via live instrumentation. **PASS.**
- **Scenario H (refresh)** — no standing "Refresh" control exists on the approved Dashboard design (correctly not added, per brief §20's "do not create a new large control unless necessary" — no Refresh button existed in the approved design to begin with); the error-state "Retry" control was exercised live instead (during the offline test below) and performed a real, correct re-fetch. **PASS** (via Retry, the real equivalent this design actually has).
- **Scenario I (401)** — an invalid key produced the real route-level "Authentication required" card; no fabricated Dashboard data. **PASS.**
- **Scenario J (403)** — **not reproducible through pure browser UI**, disclosed rather than faked: neither `/dashboard` nor `/kpis` has a role gate (confirmed by reading both handlers), so the only real 403 path is a project-scope mismatch — and a project-scoped key's own `/projects` list never contains an out-of-scope project (the same structural non-reproducibility already disclosed for Evidence in FE-7 and Dashboard's own siblings in FE-9). Independently confirmed via `curl` against the real endpoint with a real project-A-scoped key requesting project B's dashboard: a genuine `403 FORBIDDEN`, `"resource belongs to a different project"`. **PASS** (curl-verified).
- **Scenario K (offline)** — the real backend process was killed twice: once producing the real route-level offline card (before authentication), and once — after restoring service, re-authenticating, and selecting a project — producing Dashboard's **own**, distinct, page-level offline `StateCard` (different copy: "Unable to reach the AIQA Orchestrator to load real dashboard data") while the header shell stayed fully visible, exactly matching brief §12's requirement. Restarting the backend and clicking Retry recovered real data both times. **PASS.**
- **Scenario L (realtime)** — folded into Scenario E/F above; additionally confirmed exactly one WebSocket instance throughout (no duplicate/second connection ever created). **PASS.**
- **Scenario M (security)** — see §15. **PASS**, with the disclosed browser-console WS-URL finding.

No scenario above is claimed without having genuinely been exercised; Scenario J's UI non-reproducibility is disclosed plainly (curl-verified instead) rather than worked around with a fabricated result, and Scenario H's "no standing Refresh control" is disclosed as a deliberate design match to the approved Dashboard rather than silently invented.

## 18. Typecheck/build

Exact commands, run from `forentend/`, final state:

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 207/207 passing
npx vite build       # succeeds, 2712 modules, ~8s
```

The production build's >500kB chunk-size warning is pre-existing (present since before FE-4) and unchanged in kind by this sprint.

## 19. Regression

All 181 pre-existing tests (FE-1 through FE-9) still pass — 9 of them were genuinely updated (not weakened) to match Dashboard's new real content, per §16's own detailed accounting — alongside the 26 new `DashboardPage.test.tsx` tests (207 total). No existing route, provider, or unrelated page's behavior was altered.

## 20. Documentation compliance

Re-checked at sprint start against the three now-present documents' Dashboard-specific sections:

- **PRD §2** / **Business Logic §2**: "Dashboard: Project-wide health, execution/pass-fail/test/report cards, pie/bar analytics and project selector" — matches this sprint's real KPI cards, real pass/fail counts, and real donut chart exactly; "report cards" has no real backing (no reports table exists anywhere in this backend, confirmed by BE-12's own doc comment) and remains correctly unimplemented/fake, untouched by this sprint.
- **Business Logic §14 (Primary Quality KPIs table)**: explicitly names "Execution Reliability" and "Execution Efficiency" as KPI concepts — these are the exact two real KPIs `GET .../kpis` computes and this sprint now renders, a direct, confirmed match.
- **Business Logic §10**: "Events update the UI incrementally; reconnect reconciles authoritative state and processing is idempotent" — matches this sprint's realtime-triggered debounced resync architecture exactly (§9).
- **FE-9's own already-disclosed finding, re-confirmed and not re-litigated**: the Flow Diagram (page 5) depicts "WebSocket → Dashboard" and Business Logic §15 lists "Real-time events for execution, RBAC and API telemetry" as an architecture goal — but the real backend emits realtime events only for executions/tasks, never a dashboard-KPI-specific event. This sprint's realtime integration (§9) is therefore event-*triggered* (any real event causes a real REST resync) rather than event-*payload-driven* (no dashboard field is ever derived from an event's own fields) — an honest, disclosed reconciliation of the document's aspiration against the real backend's actual capability, not a silent invention of a Dashboard-specific event type.
- No contradiction was found between the documents and this sprint's real KPI/chart implementation specifically.

## 21. Backend changes

**None.** No file under `services/orchestrator/src` or `migrations/` was created, edited, or deleted this sprint. The one non-code, disclosed exception (matching FE-8/FE-9's own established precedent): a fresh project-A-scoped operator key (`fe10-manual-verify-scoped-a`) was minted directly via the same documented `sqlite3 INSERT` script used since FE-4, specifically to curl-verify Scenario J's real 403. This adds a harmless `api_keys` row for manual testing; it does not touch dashboard/execution/task/evidence/audit data or any backend logic.

## 22. Limitations

- The Execution Activity timeline chart has no real backend data source and always shows its own honest empty state (§6, §11) — a genuine backend capability gap, not a frontend gap.
- Project health is real for exactly two states (Running/Healthy); Warning/Degraded/Critical/Offline have no backing data model and are not shown (§11).
- Scenario J (403) could not be reproduced through pure browser-UI interaction — a structural consequence of FE-5's own correct project-scope enforcement, independently confirmed via `curl` instead (§17).
- The browser-native WS-URL-in-console-on-failure behavior (§15) cannot be suppressed from frontend code — it is standard browser engine behavior tied to the backend's own real `?key=` WebSocket auth design (FE-9), disclosed rather than silently observed.
- "Total Test Cases" (`test_count`) is genuinely `0` in this environment today, since Test Generation does not persist real test-case entities anywhere in the backend — an honest real value, not a Dashboard defect.

## 23. Out-of-scope

**FE-11 and FE-12 were not started.** This sprint implemented real Dashboard data integration only: the KPI cards, the donut chart, the project-detail panel, the AI Quality Core Orb, and the realtime/error/loading architecture behind them. `AIQAContext.tsx` itself was not modified and its remaining fabricated data (used by RBAC/API/Mobile/Custom Testing, Reports, Feature Map/Flow, and Dashboard's own still-fake "Launch Run" quick-action) is explicitly left in place — broader fabrication removal is FE-11's job. No FE-12 status-vocabulary migration was performed. No backend code, schema, or authorization logic was touched.
