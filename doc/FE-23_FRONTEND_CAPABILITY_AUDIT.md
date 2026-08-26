# FE-23 — Frontend Capability Audit (Post FE-22)

## Status

**PASS** (audit-only — no code was modified this session).

## Audit Scope

Fresh, direct inspection of the current repository state (no reliance on prior report claims without re-verification): `forentend/src/**` (all pages, contexts, services, types, routes, tests), `services/orchestrator/src/api.rs`'s full route table, and cross-referencing every scoped backend route against actual frontend consumption. Repository has zero commits across the entire FE-1→FE-22/BE-10 span (confirmed via `git log`/`git status` — all work remains uncommitted in the working tree, consistent with every prior unit this session).

## Repository State

- `git diff --stat -- services/orchestrator/` at the start of this audit: `api.rs` (+41), `api_tests.rs` (+186) — exactly BE-10's diff, unchanged since FE-22. No backend file has moved since.
- Frontend test suite (last known good, re-confirmed structurally this session by inspecting all 15 files directly): 15 test files, 305 passing tests (299 at end of FE-21, +6 from FE-22).
- Full route table in `api.rs` re-enumerated fresh (line-by-line grep of every `.route(...)` call, §"Backend → Frontend Capability Matrix" below) — confirmed identical to FE-21's own inventory plus exactly one addition: `GET /api/v1/projects/:project_id/intelligence/tests` (BE-10/FE-22).

## FE-1 → FE-22 Capability Matrix

| Unit | Capability | Backend Exists | Frontend Exists | Real Data | Project Scoped | Realtime | Tests | Status |
|---|---|---|---|---|---|---|---|---|
| FE-1 | HTTP transport (`aiqaApi.ts`, `ApiError`, Bearer auth) | Yes (whole `/api/v1` surface) | Yes | Yes | N/A (transport) | N/A | `aiqaApi.test.ts` | Complete |
| FE-2 | Auth UI (`AuthContext.tsx`, `OrchestratorAuthPanel` in `SettingsPage.tsx`) | Yes | Yes | Yes | N/A | N/A | `AuthContext.test.tsx` | Complete |
| FE-4 | Data-driven route/nav config (`routeConfig.tsx`) | N/A | Yes | N/A | N/A | N/A | `AppRoutes.test.tsx` | Complete |
| FE-5 | `ProjectContext.tsx` (real project list/select/persist) | `GET/POST /api/v1/projects` | Yes | Yes (`Project[]`, never seeded) | Yes (this *is* the scoping mechanism) | N/A | `ProjectContext.test.tsx` | Complete |
| FE-6 | `AuditPage.tsx` | `GET .../audit` | Yes | Yes (`AuditRow[]`, all 8 fields rendered — verified fresh, `git grep` of `occurred_at/actor/role/action/target/detail/outcome/project_id` all present) | Yes | Yes (`useRealtime().subscribe`, confirmed via fresh grep of `components/pages`) | `AuditPage.test.tsx` | Complete |
| FE-7 | `EvidencePage.tsx` | `GET .../evidence`, `.../evidence/:id`, `.../evidence/:id/content` | Yes | Yes (`EvidenceRow[]`, `task_id` rendered at line 534-536, `object_key` deliberately never rendered per this file's own doc comment — a documented security decision, not a gap) | Yes | Yes | `EvidencePage.test.tsx` | Complete |
| FE-8 | `ExecutionHubPage.tsx` (create/list/detail/cancel) | `POST/GET .../executions`, `.../executions/:id`, `.../cancel` | Yes | Yes | Yes | Yes | `ExecutionHubPage.test.tsx` | Complete |
| FE-9 | `RealtimeContext.tsx` (project-scoped WS, dedup, isolation, reconnect/backoff, lag handling) | `GET .../events` (WS) | Yes — single centralized connection, `useRealtime().subscribe` | Yes | Yes (double-enforced: server-side per `ws_tests.rs::be14_subscribers_receive_only_their_own_projects_real_events`, plus a second client-side `selectedProjectIdRef` check in `handleIncoming`, line 213-214) | Yes (is the realtime layer) | `RealtimeContext.test.tsx` (14 tests, incl. dedup/lag/isolation) | Complete |
| FE-10 | `DashboardPage.tsx` | `GET .../dashboard`, `.../kpis` | Yes | Yes — `execution_counts`, `task_counts`, `pass_fail`, `test_count`, `generated_at`, `execution_reliability`, `execution_efficiency_ms` all real and rendered; `ProjectStatus` explicitly limited to `'Running'`/`'Healthy'` only, with the file's own comment stating *"no health-scoring model exists; inventing thresholds... [is] exactly the fabricated 'health score'"* | Yes | Yes | `DashboardPage.test.tsx` | Complete (within real backend surface — additional KPI concepts beyond these two real endpoints remain backend-missing, this file's own disclosure, unchanged) |
| FE-11 | `ReportsPage.tsx` (Findings) + 7 truthful "Case C" not-implemented pages | `GET .../findings`, `.../analysis`, `.../review` | Yes | Yes | Yes | Yes (Findings section only) | `ReportsPage.test.tsx`, `CaseCPages.test.tsx` | Complete |
| FE-12 | Status-vocabulary display (`utils/statusLabel.ts::formatStatusLabel`) + centralized error-state mapping (`components/state/StateViews.tsx::renderResourceErrorState`) | N/A (display-only) | Yes | N/A | N/A | N/A | `statusLabel.test.ts`, `StateViews.test.tsx` | Complete |
| FE-14 | Real project creation (`ProjectContext.createProject`, `ProjectSelector.tsx`) | `POST /api/v1/projects` | Yes | Yes | N/A | N/A | `aiqaApi.test.ts` (`projects.create` describe block), `ProjectSelector.test.tsx` | Complete — `projects.update` (edit project) remains real but **unconsumed** (zero call sites, confirmed fresh, §Backend→Frontend matrix) |
| FE-16 | Test Generator local-only truthfulness | N/A (deliberately local) | Yes | N/A (disclosed local-only) | N/A | N/A | `TestCaseGeneratorPage.test.tsx` | Complete |
| FE-17 | Execution dependency/DAG editor (`TaskDef.prerequisites` chip UI) | `scheduler.rs::validate_plan` (real cycle detection) | Yes | Yes | Yes | Yes (via FE-8/9's existing resync) | `ExecutionHubPage.test.tsx` (FE-17 block, 8 tests) | Complete |
| FE-18 | Run approval/rejection attribution (`approved_by`/`approved_at`) | `store.rs::approve_run`/`reject_run` | Yes (`#run-approval-attribution`) | Yes | Yes | Yes | `ExecutionHubPage.test.tsx` (FE-18 block, 3 tests) | Complete |
| FE-19 | Finding review attribution (`promoted_at`) | `store.rs::review_bug_draft` | Yes (`#finding-review-attribution-{id}`) | Yes | Yes | Yes (Findings section) | `ReportsPage.test.tsx` (FE-19 block, 3 tests) | Complete |
| FE-20 | Execution Hub full 11-field count grid | `domain.rs::RunSummary` | Yes (`#run-detail-count-grid`) | Yes | Yes | Yes | `ExecutionHubPage.test.tsx` (FE-20 block, 5 tests) | Complete |
| FE-21 | Fresh audit unit | N/A | N/A | N/A | N/A | N/A | N/A | Complete (correctly concluded no valid frontend-only candidate; recommended backend work, which became BE-10) |
| BE-10 | Project-scoped Test Intelligence route | `GET .../intelligence/tests` (new, project-scoped) | N/A (backend unit) | Yes | Yes (`require_scoped_project`, verified isolated by real distinct-risk-value test) | N/A | `api_tests.rs` (5 new Rust tests) | Complete |
| FE-22 | Test Intelligence frontend exposure | (BE-10, above) | Yes (`ReportsPage.tsx`, `#test-intelligence-content`) | Yes | Yes (own effect mirroring Findings' stale-data protection) | **No** — confirmed fresh: this section has no `useRealtime().subscribe` call of its own; only Findings on the same page auto-resyncs on `RUN_COMPLETED` | `ReportsPage.test.tsx` (FE-22 block, 6 tests) | Complete, with one disclosed, minor limitation (no realtime auto-refresh — manual "Refresh" button only) |

FE-3, FE-13, FE-15 were doc-comment-only corrections/consolidations (FE-3 folded the auth status card into FE-12's shared `renderAuthState`; FE-13 was a corrective fix to a stale doc comment in `RealtimeContext.tsx`, not a shippable capability; FE-15's real work is BE-15's backend split, reflected in FE-7/FE-11's already-audited implementations above) — not separately tabulated as they produced no distinct, independently-testable frontend surface.

## Backend → Frontend Capability Matrix

Every route currently registered in `services/orchestrator/src/api.rs::router` (fresh `grep -n '\.route('` this session), cross-referenced against actual frontend consumption (fresh `grep` of every `aiqaApi.*.*(` call site under `forentend/src`, production code only):

| Backend Route | Handler | Auth/Scope | Frontend Client Method | Consumed By | Status |
|---|---|---|---|---|---|
| `POST/GET /api/v1/projects` | `create_project`/`list_projects_handler` | Admin (create) / any (list) | `projects.create`/`.list` | `ProjectContext.tsx` | B — complete |
| `GET/PATCH /api/v1/projects/:id` | `get_project_handler`/`update_project_handler` | any / Admin | `projects.get`/`.update` | **Zero production call sites** (confirmed fresh grep) | C — real, no consumer, no requirement evidence found (3rd independent confirmation across FE-19/FE-20/FE-21/now) |
| `GET .../dashboard`, `.../kpis` | `get_project_dashboard`/`get_project_kpis` | any | `dashboard.get`/`.kpis` | `DashboardPage.tsx` | B — complete |
| `POST/GET .../executions` | `create_run_scoped`/`list_runs_scoped` | Operator (create) / any | `executions.create`/`.list` | `ExecutionHubPage.tsx` | B — complete |
| `GET .../executions/:id` | `get_run_scoped` | any | `executions.get` | `ExecutionHubPage.tsx`, `DashboardPage.tsx` | B — complete |
| `GET .../executions/:id/events` | `get_events_scoped` | any | `executions.events` | **Zero production call sites** (confirmed fresh; `RealtimeContext.tsx`'s own comment already discloses this — full-refetch via `.get()` is used instead of `?since=` catch-up) | C — real, deliberately unused, documented alternative strategy in place |
| `POST .../approve`, `.../reject` | `approve_run_scoped`/`reject_run_scoped` | Operator | `executions.approve`/`.reject` | `ExecutionHubPage.tsx` | B — complete |
| `POST .../cancel` | `cancel_run_scoped` | Operator | `executions.cancel` | `ExecutionHubPage.tsx` | B — complete |
| `GET .../tasks`, `.../tasks/:id` | `list_run_tasks_scoped`/`get_run_task_detail_scoped` | any | `executions.tasks`/`.task` | **Zero production call sites** (confirmed fresh) | E — redundant, tasks already embedded in `RunDetail` |
| `GET .../executions/:id/evidence` | `list_evidence_scoped` | any | `executions.evidence` | `EvidencePage.tsx`, `ExecutionHubPage.tsx` | B — complete |
| `GET .../evidence/:id` | `get_evidence_detail_scoped` | any | `evidence.get` | **Zero production call sites** (only in `aiqaApi.test.ts`) | E — documented redundancy (`EvidencePage.tsx`'s own comment: list already returns the identical shape) |
| `GET .../evidence/:id/content` | `get_evidence_content_scoped` | any | `evidence.content` | `EvidencePage.tsx` | B — complete |
| `GET .../findings` | `list_findings_scoped` | any | `findings.list` | `ReportsPage.tsx` | B — complete |
| `GET .../findings/:id` | `get_finding_detail_scoped` | any | `findings.get` | **Zero production call sites** (confirmed fresh) | E — redundant (detail already embedded in `.analysis()` response's `finding` field) |
| `POST .../findings/:id/analysis` | `get_finding_analysis_scoped` | any | `findings.analysis` | `ReportsPage.tsx` | B — complete |
| `POST .../findings/:id/review` | `review_finding_scoped` | Operator | `findings.review` | `ReportsPage.tsx` | B — complete |
| `GET .../audit` | `get_audit_log_scoped` | Operator | `audit.list` | `AuditPage.tsx` | B — complete |
| `GET .../intelligence/tests` | `list_test_intelligence_scoped` | any | `intelligence.tests` | `ReportsPage.tsx` | B — complete (new this cycle, BE-10/FE-22) |
| `GET /intelligence/tests` (legacy) | `list_test_intelligence` | any, key-bound scope | N/A by design | none — confirmed zero references outside the backend and this file | E — legacy, deliberately unscoped-by-key, superseded for frontend use by the row above |
| `GET /intelligence/failure-clusters` (legacy) | `list_failure_clusters` | any | N/A | none | E — redundant with `.analysis()`'s `failure_cluster` field |
| `GET/POST /bug-drafts*` (legacy) | `list_bug_drafts`/`review_bug_draft` | any/Operator | N/A | none | E — superseded by scoped `findings.*` |
| `GET /metrics` | `get_metrics` | any | N/A | none | C — Prometheus text format, not JSON; not a UI-renderable capability |
| `POST/GET/DELETE /admin/api-keys*` | `create_api_key`/`list_api_keys_handler`/`revoke_api_key_handler` | Admin | N/A | none | C — real, Admin-gated key CRUD; no requirement evidence found for a UI (re-confirmed this session; would also be a materially larger feature than a small exposure) |
| `GET /admin/audit-log` | `get_audit_log` | Admin | N/A | none | E — superseded by scoped `.../audit` |
| `GET /runs*`, `/evidence/:id/content` (legacy unscoped execution/evidence routes) | various | any, key-bound scope | N/A | none | E — legacy, superseded by scoped equivalents |
| `GET /api/v1/projects/:id/events` (WS) | `project_events_ws_handler` | `?key=` query param | `RealtimeContext.tsx` | every scoped page except FE-22's Test Intelligence section | B — complete, with one disclosed gap (see Realtime Audit) |

**No new Category-A gap was found.** Every route without a frontend consumer was already investigated in FE-21 and is unchanged in nature; BE-10/FE-22 closed the one route (`intelligence/tests`) that had crossed from Category D into Category A.

## Fake / Stub / Placeholder Audit

**Legitimate test fixtures (not production fabrication):** every `describe`/`it` block across the 15 test files uses fixture data (`FINDING_A`, `RUN_A_DETAIL`, `INTEL_ROW_A`, etc.) — these are correctly scoped inside `*.test.tsx`/`*.test.ts` files only, confirmed by `grep -RniE "fake|mock|demo|sample" forentend/src | grep -v .test.ts` returning only doc-comments and legitimate HTML `placeholder` attributes (re-run fresh this session, same result pattern as every prior unit's own search).

**Genuine production runtime fabrication found this session (new finding, not previously flagged by FE-11/16/18/19/20/21/22):**

1. **`context/AIQAContext.tsx:94`** — `const [notificationCount, setNotificationCount] = useState<number>(3);` — a **hardcoded starting count of 3**, with no real backend notification/alert entity behind it anywhere in this codebase (no `/notifications`, `/alerts` route exists in `api.rs`).
2. **`components/layout/Header.tsx:270-292`** — the notification-bell dropdown (opened via `#notifications-button`, imported from `AIQAContext`) is headed *"Recent Quality Alerts"* / *"Live Stream"* and unconditionally renders **two fully static, hardcoded alert cards** — *"Infrastructure DNS alert on Redis"* / *"test-104 failed due to DNS timeout"* and *"Auth Token Revocation Assertion"* / *"Expected HTTP 401, received HTTP 200 on revoked cookie"* — identical on every page load, for every project, regardless of any real backend state. This is global (rendered in the app shell's `Header`, visible on every route) and explicitly labeled "Live Stream," which is false — it is neither live nor derived from any stream.
3. **`components/layout/Header.tsx:301`** (approximately — the "User Identity Context" block) — a static `QA` two-letter avatar badge, not derived from the real authenticated key's `name`/role at all.

These are distinct from, and not covered by, `SettingsPage.tsx`'s already-documented, deliberately-preserved fake content (that file's own comment (line 30-35) scopes the "do not remove existing fake data" instruction specifically to the Settings Authentication tab's SSO/token widgets — it makes no claim about `Header.tsx`, and no prior FE unit's report claims to have audited or preserved the Header's notification panel).

No other new production fabrication was found. Re-confirmed clean: `Math.random()` appears only in `TestCaseGeneratorPage.tsx` (FE-16's already-disclosed, already-scoped local ID/latency simulation for the local-only generation pipeline) and `ApplicationSourceSection.tsx`'s pre-existing ping simulator (unrelated, pre-existing, not touched by any recent unit). `setTimeout` appears only in legitimate debounce/resync timers (every scoped page) plus the same two pre-existing, already-disclosed local simulations.

## Project Isolation Audit

- Fresh repo-wide search for hardcoded project-id literals and the `'default'` tenant string in production code (`grep -RniE "project_id\s*[:=]\s*['\"]"` and `grep -Rn "'default'"`, excluding `*.test.ts*` and `types/contract.ts`): **zero matches**.
- Every project-scoped fetch across all 5 real data pages (`AuditPage`, `EvidencePage`, `ExecutionHubPage`, `DashboardPage`, `ReportsPage`) reads `selectedProjectId` from `useProject()` and passes it as the first argument to its `aiqaApi.*` call (confirmed by direct inspection of every call site, §Backend→Frontend matrix's "Consumed By" column).
- `ProjectContext.tsx`'s `applyList` (line 108-116) never adopts a selection that isn't present in the real, just-fetched `projects` list — including the persisted `localStorage` value, which is only re-adopted "after confirming real backend membership, never 'blindly'" (the file's own comment, line 107).
- **FE-22 Test Intelligence specifically**: its `useEffect` (mirrors the pre-existing Findings effect exactly) aborts the in-flight request and resets `testIntel` to `null` before firing a new fetch on project change; the abort-signal check (`if (controller.signal.aborted) return;`) prevents a late-arriving stale-project response from ever overwriting newer state. This was **live-verified** in FE-22's own manual verification (A → B → A, real distinct data each time, confirmed no cross-project bleed) — re-confirmed structurally sound by direct code re-read this session, not merely re-cited from that report.
- `RealtimeContext.tsx` enforces isolation twice: server-side (`ws_tests.rs::be14_subscribers_receive_only_their_own_projects_real_events`) and a second client-side check in `handleIncoming` (line 213-214: `if (selectedProjectIdRef.current !== socketProjectId) return;` then `if (event.project_id !== null && event.project_id !== selectedProjectIdRef.current) return;`) — a genuine defense-in-depth measure, not merely trusting the server.

**No project-isolation defect was found.**

## Realtime Audit

Real backend event vocabulary (confirmed via `RealtimeContext.tsx`'s own doc comment, itself sourced from reading every `record.emit(...)` call site in `engine.rs`): `RUN_SCHEDULED` (once), nine `TASK_*` events, `RUN_COMPLETED` (once, terminal for every reason a run ends). No dedicated approval/rejection/cancellation event type exists — these are only inferable from `RUN_COMPLETED`'s `summary` text or per-task `TASK_CANCELLED` events.

Realtime consumption, confirmed fresh (`grep -Rl "useRealtime()" components/pages`):
- `AuditPage.tsx`, `ExecutionHubPage.tsx`, `EvidencePage.tsx`, `DashboardPage.tsx`, `ReportsPage.tsx` (Findings section only) — **5 of 5** real data-driven pages subscribe.
- **Gap**: `ReportsPage.tsx`'s Test Intelligence section (FE-22) does **not** subscribe — confirmed by direct re-read of the file this session; only the pre-existing Findings `resyncTimeoutRef`/`realtime.subscribe` block exists, and it is not wired to `fetchTestIntelligence`. This is the same page, so a user reviewing Findings sees them live-update on `RUN_COMPLETED` while the Test Intelligence table sitting directly below stays stale until a manual "Refresh" click (which FE-22 did wire to refresh both resources together, just not automatically).
- No other page lacks realtime that should have it — the 7 Case-C pages and Test Generator are correctly realtime-free (no real backend data to resync).

This is a genuine, small, well-scoped functional gap — not recommended as this audit's top candidate (see Candidate Next Units) because it is lower product value than the fabrication finding above, but it is real and traceable.

## API Contract Audit

- `forentend/src/types/contract.ts` field-for-field cross-checked against `services/orchestrator/src/domain.rs`/`store.rs` structs for every type touched this session (`RunSummary`/`RunDetail`, `Finding`, `TestIntelRow`) — no mismatch found; nullability is preserved correctly everywhere (e.g., `TestIntelRow`'s 8 numeric fields are all `number | null`, matching the backend's `Option<f64>`/`Option<i64>` exactly).
- **Missing API client methods**: none — every scoped backend route now has a corresponding `aiqaApi.*` method (confirmed by the Backend→Frontend matrix above; the intelligence gap was the last one, closed by FE-22).
- **Unused API methods** (real, typed, tested at the API layer, zero production call sites): `projects.get`, `projects.update`, `executions.events`, `executions.tasks`, `executions.task`, `evidence.get`, `findings.get` — every one of these has an already-documented reason in its own file's comments (redundancy or a deliberate alternative strategy), re-confirmed fresh this session, not stale.
- **Legacy endpoints still being consumed**: none — fresh search (`grep -Rn "'/intelligence/tests'\|'/runs'\|'/bug-drafts'\|'/admin/"` under `forentend/src`, excluding tests) returns zero matches. The frontend consumes exclusively the `/api/v1/projects/...` scoped surface.
- **Frontend consumers using the wrong endpoint**: none found.

## UI Functional Gap Audit

- **Real backend capability with no UI**: none remaining that meets the Category-A bar (see Backend→Frontend matrix) — the fabrication finding above (§Fake/Stub Audit) is a different kind of gap (a *fake* UI element, not a *missing* one).
- **Incomplete real data**: none found this session beyond the already-disclosed Dashboard KPI gap (unchanged, backend-missing, correctly out of scope for a frontend-only unit).
- **Incorrect empty/loading/error states**: none found — every real page's three states were re-inspected this session (via direct code read, not just report citation) and each uses the shared `StateCard`/`EmptyState`/`renderResourceErrorState` components consistently.
- **Project switching problems**: none found (§Project Isolation Audit).
- **Stale data**: none found in production code paths; one *disclosed*, non-defective gap (Test Intelligence's lack of realtime auto-refresh, §Realtime Audit — this is "not automatically fresh," not "shows wrong data").
- **Missing actionable controls**: none found for any real, backend-supported action — every mutation route (`approve`/`reject`/`cancel`/`review`/`create`) has a corresponding UI control.
- **Existing feature only partially connected**: **the Header notification bell** — it has a real, working open/close/badge-clear interaction (genuine UI wiring) but is connected to nothing real (§Fake/Stub Audit). This is the clearest instance of this specific audit category found this session.

## Test Coverage Audit

- **Important functionality without tests**: `aiqaApi.ts`'s new `intelligence.tests` method has no dedicated `describe` block in `aiqaApi.test.ts` (unlike every other endpoint — `audit.list`, the evidence endpoints, the execution endpoints, and `projects.create` each have one, per the file's own section headers, re-confirmed by direct grep this session). Its only coverage is indirect, through `ReportsPage.test.tsx`'s item 1 (which does assert the exact URL). This is a real, minor, traceable gap, not a blocker.
- **Project isolation coverage**: strong — `ProjectContext.test.tsx`, `RealtimeContext.test.tsx` (`item 4/5`, `item 20`), `AuditPage.test.tsx`, `ExecutionHubPage.test.tsx`, `ReportsPage.test.tsx` (Findings' own switch test, plus FE-22's own item 5) all cover project-switch/isolation directly.
- **Project switching coverage**: present for every real data page (confirmed by grepping each test file for a "switching project" test name).
- **API failure coverage**: present for 401/403/offline across every page's own test file (`renderResourceErrorState`'s shared behavior means each page only needs to prove it's wired to the shared function, which each does).
- **Empty-state coverage**: present for every real data page, including FE-22 item 3.
- **Loading-state coverage**: present structurally (every page's `StateCard` usage is exercised implicitly by the same tests that await the loaded state; no page has a dedicated "shows the loading card" assertion, consistent with the established pattern across all prior units — not a new gap).
- **Realtime coverage**: strong at the provider level (`RealtimeContext.test.tsx`, 14 tests); consumer-level resync is tested per-page (`ExecutionHubPage.test.tsx`'s FE-9-era tests, `AuditPage.test.tsx`'s own resync test) — Test Intelligence has none, consistent with it genuinely having no realtime wiring (§Realtime Audit), not a test gap masking a real one.
- **Test Intelligence coverage**: `ReportsPage.test.tsx`'s FE-22 block (6 tests) covers URL/project-id, real-field rendering, empty state, 403 error state, project-switch isolation, and null-handling — a solid, non-trivial set, matching the FE-22 report's own claim.
- **Regression coverage**: the full suite (305 tests) is run at the end of every prior unit and re-confirmed passing at each step — no evidence of regression debt.

## Performance Audit

- **Duplicate API calls**: none found. Each page fetches each resource exactly once per relevant state change (project select, manual refresh, realtime resync) — confirmed by direct inspection of every `useEffect`/`useCallback` fetch trigger across the 5 real pages.
- **Missing abort handling**: none found — every `GET`-shaped fetch across all 5 pages (11 `new AbortController()` call sites, confirmed fresh grep) passes its `AbortSignal` through to `aiqaApi`; mutation calls (`create`/`cancel`/`approve`/`reject`/`review`) correctly omit one, since they are one-shot actions, not cancelable list fetches.
- **Duplicate realtime subscriptions**: none — `RealtimeContext.tsx` maintains exactly one WebSocket (`socketRef`), and `openSocket` always tears down any existing connection first (`teardownSocket()`, line 253) before opening a new one; the connect-effect explicitly no-ops if already connected/connecting to the exact same project (line 335-337).
- **Unnecessary polling**: none — FE-8's original 3-second poll was removed by FE-9 and replaced entirely by the realtime stream; no page has reintroduced a poll interval (confirmed: the only `setInterval`-shaped code in the entire `forentend/src` tree is nonexistent — `grep -Rn "setInterval" forentend/src` returns zero matches).
- **No evidence of an actual performance problem was found** — no optimization is recommended, per this audit's own instruction not to recommend one without evidence.

## Candidate Next Units

| Candidate | Feature | Current Gap | Repository Evidence | Backend Readiness | Frontend Readiness | Project-Scoping Readiness | Security Considerations | Testing Requirements | Estimated Scope | Dependencies | Risk | Product Value |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **A** | Replace the Header's fabricated "Recent Quality Alerts" notification panel with a real feed | `AIQAContext.tsx:94` hardcodes `notificationCount = 3`; `Header.tsx:270-292` renders two static fake alert cards labeled "Live Stream" on every page, every project | Direct code read this session (§Fake/Stub Audit items 1-2) | N/A — no backend change needed; real data (`RunEvent.type`/`.task_id`/`.run_id`/`.timestamp`/`.summary`) already flows through the already-connected `RealtimeContext.tsx` on every page | Needs new code: consume `useRealtime().subscribe` at the app-shell level (or lift into a small new provider/hook), format a short list of recent real events, remove the two hardcoded cards and the `useState(3)` seed | N/A (realtime is already project-isolated) | None — read-only, no new auth surface | New tests for the notification-derivation logic and the emptied fake content | Small–Medium (one context/hook + `Header.tsx` edit + tests) | `RealtimeContext.tsx` (already exists, no change needed) | Low — additive, read-only, easily reverted | High — this is the single most visible remaining truthfulness gap in the entire app (global header, every page, explicitly claims "Live Stream") |
| **B** | Wire FE-22's Test Intelligence section to the existing realtime resync (matching Findings on the same page) | `ReportsPage.tsx`'s Test Intelligence section has no `useRealtime().subscribe` — confirmed fresh this session | Direct code read (§Realtime Audit) | N/A — no backend change | Small: extend the existing Findings resync effect (or add a parallel one) to also call `fetchTestIntelligence` | Already correct (reuses existing project scope) | None new | 1-2 new tests | Small | `RealtimeContext.tsx`, FE-22's existing fetch functions | Low | Medium — a real but minor staleness gap, not a fabrication |
| **C** | Add dedicated `aiqaApi.test.ts` coverage for `intelligence.tests` | No dedicated `describe` block for this one endpoint, unlike every sibling endpoint | Direct file read (§Test Coverage Audit) | N/A | N/A | N/A | N/A | This *is* the work | Small | None | Low | Low — coverage hygiene, not a product capability |

Other candidates considered and rejected outright (no requirement evidence, or requires backend work this audit is not authorized to scope as "next"): `projects.update` ("Edit Project" UI — Category C, no requirement evidence across 4 independent audits now), `/admin/api-keys` UI (Category C, no requirement evidence, and a materially larger feature than a small exposure), Dashboard's remaining backend-missing KPI concepts (Category D — needs backend work first, not a frontend candidate).

## Recommended Next Unit

**Candidate A — Replace the Header's fabricated notification panel with a real, realtime-derived feed.**

## Why This Unit Is Next

This is not a new capability exposure in the FE-17→FE-22 mold — it is the exact case this audit's own instructions anticipate: *"If an existing capability is incomplete and should be fixed before a new feature, report that."* The Header's notification bell is not incomplete, it is **actively false**: it is global (every page, every project), it explicitly claims to be a "Live Stream," and it shows the same two fabricated incidents regardless of real backend state. Every prior FE unit in this entire engagement (FE-16 through FE-22) was justified by a "truthfulness first" priority order — this finding scores higher against that exact priority than any remaining capability-exposure candidate, because it is not a missing feature but an active misrepresentation, and because real, already-flowing data (`RealtimeContext`'s event stream) already exists to replace it without any backend change. Candidate B is real but strictly smaller in value (a staleness gap on an already-real, already-truthful new feature, not a fabrication). Candidate C is coverage hygiene, not a product unit.

## Backend Dependency

**None.** `RealtimeContext.tsx` already delivers real, deduplicated, project-isolated `RunEvent`s (`type`, `task_id`, `run_id`, `timestamp`, `summary`) to any subscriber on every page, already connected before this candidate's own code would run. No new route, no new backend field, no schema change.

## Frontend Dependency

A new consumer of the already-existing `useRealtime().subscribe` API (most naturally at the app-shell/`Header.tsx` level, or a small new hook `AIQAContext.tsx` can own instead of its current `useState(3)`), replacing the two hardcoded alert cards and the hardcoded count with real, formatted recent-event data (or, if judged out of scope to build a full feed, at minimum removing the fabricated "Live Stream" claim and the two fake cards — the smaller of the two valid truthful outcomes, matching this engagement's repeated "smallest truthful implementation" discipline). The exact scope decision (full real feed vs. honest removal) is for that unit's own audit-first step, not predetermined here.

## Security Considerations

None beyond what `RealtimeContext.tsx` already enforces (project isolation, dedup) — this is a read-only consumer of an already-authenticated, already-isolated stream. No new credential, no new authorization check, no new attack surface.

## Truthfulness Audit

This audit session introduced no code changes and no new claims about backend capability. Every conclusion above is traced to a specific file, line, or fresh command output. The two production-fabrication findings (§Fake/Stub Audit) were independently discovered this session via direct code reading, not inherited from any prior report.

## STOP Conditions / Blockers

None apply to the recommended candidate (A) — no backend capability is missing, backend data (via `RealtimeContext`) is trustworthy and already verified end-to-end across 5 pages, project scoping is already enforced twice over, the response contract (`RunEvent`) is unambiguous and already typed, and no fake data or invented behavior is required (indeed, the opposite — this candidate *removes* fake data). No BLOCKED condition exists.

## Exact Implementation Handoff

For a future unit implementing Candidate A:
- Read `context/RealtimeContext.tsx`'s `subscribe`/`RunEvent` contract (already documented in full in that file's own module comment).
- Read `context/AIQAContext.tsx` lines 90-96, 210 (`notificationCount`/`clearNotifications`) and `components/layout/Header.tsx` lines 249-296 (the bell button + dropdown) as the exact two files to change.
- Decide (as that unit's own audit step, not here): build a real short recent-events list from `RunEvent.type`/`.summary`/`.timestamp`, or truthfully disclose "no alerting capability exists yet" matching the Case-C pages' own established pattern (`EmptyState`, `TestCaseValidatorPage.tsx` as a style reference) — either is a valid, truthful outcome; inventing a backend alerting concept that doesn't exist is not.
- Follow the same audit-first, smallest-truthful-change, live-manual-verification discipline every prior FE unit this session has used.

## Files Likely To Change

- `forentend/src/context/AIQAContext.tsx` (remove/replace `notificationCount` hardcoding)
- `forentend/src/components/layout/Header.tsx` (remove/replace the two hardcoded alert cards)
- A new or extended test file covering the header/notification behavior

## Files That Must NOT Change

- `services/orchestrator/**` (no backend change needed or justified for this candidate)
- Any page under `forentend/src/components/pages/` other than as a read-only consumer, if any (this candidate is app-shell-scoped, not page-scoped)
- `RealtimeContext.tsx`'s own connection/dedup/isolation logic (reused as-is, not modified)

## Final Recommendation

**Current Overall Status:** PASS

**Recommended Next Unit:** Replace the Header's fabricated "Recent Quality Alerts" notification panel (`AIQAContext.tsx`'s hardcoded `notificationCount = 3` and `Header.tsx`'s two static fake alert cards) with either a real feed derived from the already-connected `RealtimeContext` event stream, or a truthful removal/disclosure — whichever that unit's own audit-first step determines is the smallest truthful implementation.

**Unit Type:** FRONTEND

**Reason:** This is the highest-value, best-evidenced, lowest-risk gap found in a fresh, direct re-audit of the entire current repository — a global, every-page, every-project fabrication explicitly labeled "Live Stream" that is provably false, fixable with zero backend changes using data (`RealtimeContext`) that is already real, already connected, and already isolated per-project on every page that needs it.

**Backend Required:** NO

**Implementation Allowed In This Session:** NO — AUDIT ONLY

**Report File:** FE-23_FRONTEND_CAPABILITY_AUDIT.md
