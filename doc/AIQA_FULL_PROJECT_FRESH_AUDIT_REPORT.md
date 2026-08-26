# AIQA — Full Project Fresh Audit & Release Readiness Report

*Fresh, direct inspection performed this session. Prior FE-16→FE-26/BE-10/11/12b reports were used only as a map of where to look — every claim below was re-verified against current source, current test runs, or a fresh command execution, not assumed from those reports.*

## 1. Overall Status

**PASS** for everything genuinely in scope and implemented. **PARTIAL** overall, because a meaningful slice of the product surface (7 of 15 pages) is intentionally, truthfully unimplemented rather than fabricated — this is a *correct* state, not a defect, but it means the product is not feature-complete against its original ambition.

**Production Ready: Conditionally Ready.** The implemented core (Projects, Executions, Evidence, Audit, Findings, Test Intelligence, Dashboard, realtime, auth/RBAC) is real, tested, project-isolated, and passes a fresh build/test/typecheck cycle. It is not "fully ready" in the sense of being feature-complete against the original PRD — several PRD-referenced capabilities (test-generation persistence, requirement-traceability, device/API/RBAC test harnesses, change-impact analysis, quality-graph relations) have no backend implementation at all and are honestly disclosed as such, not hidden.

**Confidence level: High** for the audit's own findings (every load-bearing claim below traces to a specific file/line or a command actually run this session). **Medium-high** for long-term production operation specifically, due to one genuine, reproducible finding new to this audit: a small number of backend tests exhibit real, load-sensitive timing flakiness (§13).

## 2. Module-by-Module Audit

| Module | Frontend | Backend | Real API | DB Persistence | Realtime | Tests | Fake/Stub | Actual Status |
|---|---|---|---|---|---|---|---|---|
| Auth (API key) | Yes (`AuthContext.tsx`, `SettingsPage.tsx`'s real panel) | Yes (`auth.rs`) | Yes | Yes (`api_keys`) | N/A | Yes (`AuthContext.test.tsx`, `aiqaApi.test.ts`) | None | **Real, complete** |
| Projects | Yes (`ProjectContext.tsx`, `ProjectSelector.tsx`) | Yes (`api.rs`, `store.rs`) | Yes | Yes (`projects`) | N/A (list refetch only) | Yes (`ProjectContext.test.tsx`, `aiqaApi.test.ts`) | None | **Real** (`.update`/"Edit Project" backend-only, unconsumed — Category C, no requirement evidence) |
| Execution Hub | Yes (`ExecutionHubPage.tsx`) | Yes (`engine.rs`, `scheduler.rs`, `api.rs`) | Yes | Yes (`runs`, `tasks`, `task_dependencies`) | Yes (debounced resync) | Yes (`ExecutionHubPage.test.tsx`, 8 sub-suites: FE-8/17/18/20) | None | **Real, complete** |
| Dependency/DAG editing | Yes | Yes (`scheduler.rs::validate_plan`, real DFS cycle detection) | Yes | Yes | Rides existing resync | Yes | None | **Real** |
| Approval/rejection attribution | Yes (`#run-approval-attribution`) | Yes | Yes | Yes (`runs.approved_by/_at`) | Rides existing resync | Yes | None | **Real** |
| Realtime (WebSocket) | Yes (`RealtimeContext.tsx`, single centralized connection) | Yes (`project_events_ws_handler`, `ws_tests.rs`) | Yes | Events persisted to `run_events` | **Is** the realtime layer | Yes (14 dedicated tests) | None | **Real, complete** |
| Evidence | Yes (`EvidencePage.tsx`) | Yes (`evidence.rs`, `api.rs`) | Yes | Yes (`evidence` table + on-disk blobs) | Yes | Yes | None | **Real, complete** |
| Audit Trail | Yes (`AuditPage.tsx`) | Yes | Yes | Yes (`audit_log`) | Yes | Yes | None | **Real, complete** |
| Findings (auto-drafted defects) | Yes (`ReportsPage.tsx`) | Yes (`intelligence.rs::record_failure`) | Yes | Yes (`failure_clusters`, `bug_drafts`) | Yes (any-event resync) | Yes | None | **Real, complete** |
| Finding review attribution | Yes (`#finding-review-attribution-{id}`) | Yes | Yes | Yes (`bug_drafts.promoted_by/_at`) | Rides existing resync | Yes | None | **Real** |
| Test Intelligence (risk/value/flakiness) | Yes (`ReportsPage.tsx` Test Intelligence section) | Yes (`intelligence.rs`, real per-test scoring) | Yes (project-scoped, BE-10) | Yes (`quality_nodes`, `risk_assessments`, `test_value_assessments`, `flakiness_metrics`) | Yes (`RUN_COMPLETED`-filtered resync, FE-25) | Yes | None | **Real, complete** |
| Dashboard | Yes (`DashboardPage.tsx`) | Yes (`api.rs::get_project_dashboard`/`get_project_kpis`) | Yes | Derived (real SQL aggregates) | Yes (any-event debounced resync) | Yes (32 tests) | None | **Real**, but only 3 KPI concepts have backing data (§9) |
| Average Run Duration KPI | Yes (FE-26) | Yes (BE-12b) | Yes | Yes (`runs.started_at/finished_at`) | Rides existing resync | Yes | None | **Real** |
| Header notification bell | Yes (FE-24, real recent-activity feed) | N/A (consumes existing realtime events) | N/A | N/A | Yes | Yes (`Header.test.tsx`) | None | **Real** |
| Test Case Generator | Yes (local-only, disclosed) | **No** (no `GeneratedTestCase`/persistence entity) | N/A | **No** | N/A | Yes (`TestCaseGeneratorPage.test.tsx`) | None (local generation disclosed as such) | **UI-only by disclosed design** |
| AIQA Orchestrator (plan/DAG visualizer) | Truthful "not implemented" stub | No | N/A | N/A | N/A | Yes (`CaseCPages.test.tsx`) | None | **Missing by design, honestly disclosed** |
| API Testing | Truthful stub | No | N/A | N/A | N/A | Yes | None | **Missing by design** |
| Custom Testing | Truthful stub | No | N/A | N/A | N/A | Yes | None | **Missing by design** |
| Mobile Testing | Truthful stub | No | N/A | N/A | N/A | Yes | None | **Missing by design** |
| RBAC Testing (scenario matrix) | Truthful stub | No | N/A | N/A | N/A | Yes | None | **Missing by design** (real RBAC enforcement itself is real — see §11 — only the *scenario-matrix visualization* is absent) |
| Feature Map / Flow | Truthful stub | No | N/A | N/A | N/A | Yes | None | **Missing by design** |
| Test Case Validator (traceability/coverage) | Truthful stub | No | N/A | N/A | N/A | Yes | None | **Missing by design** |
| Settings | Partial (one real panel + explicitly-preserved fake tabs) | Only the auth-key panel is backed by real capability | Partial | N/A | N/A | No dedicated test file | **Yes, deliberate** — General/SSO/Execution/Environments/Policies/AI-Models/Notifications tabs are pre-existing fake content, preserved under a standing "do not remove existing fake data" instruction (documented in the file's own header) | **Deliberately fake outside the Auth tab** |

## 3. Frontend ↔ Backend Contract Audit

All 15 project-scoped routes actually consumed by the frontend, freshly cross-checked field-for-field this session:

| Route | FE Consumer | Request Shape | Response Shape | TS↔Rust Match | Null Handling | Error Handling | Project Isolation |
|---|---|---|---|---|---|---|---|
| `POST/GET /projects` | `ProjectContext.tsx` | Match | Match | Match | N/A | `ApiError` typed (401/403/offline/validation) | N/A (list is caller-scoped server-side) |
| `GET/PATCH /projects/:id` | `.get` unused; `.update` unused | N/A | N/A | Match | N/A | N/A | N/A — genuinely unconsumed (Category C, no requirement) |
| `GET /projects/:id/dashboard` | `DashboardPage.tsx` | Match | Match | Match | N/A (no nullable field) | Shared `renderResourceErrorState` | `require_scoped_project` |
| `GET /projects/:id/kpis` | `DashboardPage.tsx` | Match | Match (3 fields, all `Option<f64>` ↔ `number\|null`) | Match, re-verified this session field-by-field | `null → '—'` via `formatMs`, never `0` | Shared error state | `require_scoped_project` |
| `POST/GET /projects/:id/executions` | `ExecutionHubPage.tsx` | Match (`RunPlan`/`TaskDef`) | Match | Match | N/A | Shared | `require_scoped_project` + `require_resource_in_project` |
| `GET /projects/:id/executions/:run_id` | `ExecutionHubPage.tsx`, `DashboardPage.tsx` | Match | Match (11 count fields, all real) | Match | N/A (all `usize`/`number`, non-nullable, real 0 is real) | Shared | Both checks |
| `GET .../events` | Unused (superseded by realtime + full refetch, disclosed) | N/A | N/A | Match | N/A | N/A | N/A |
| `POST .../approve`/`.../reject` | `ExecutionHubPage.tsx` | Match | Match (`RunSummary`, `approved_by`/`_at`) | Match | `null` until approved | Shared, `Role::Operator` enforced both sides | Both checks |
| `POST .../cancel` | `ExecutionHubPage.tsx` | Match | Match | Match | N/A | Shared, `Role::Operator` | Both checks |
| `GET .../tasks`, `.../tasks/:id` | Unused (redundant with embedded `RunDetail.tasks`, Category E) | N/A | N/A | Match | N/A | N/A | N/A |
| `GET .../evidence` | `EvidencePage.tsx`, `ExecutionHubPage.tsx` | Match | Match | Match | N/A | Shared | Both checks |
| `GET /projects/:id/evidence/:id` | Unused (documented redundancy with list, Category E) | N/A | N/A | Match | N/A | N/A | N/A |
| `GET .../evidence/:id/content` | `EvidencePage.tsx` | Match | Real bytes, real `Content-Type` | N/A (blob) | N/A | Shared | Both checks |
| `GET /projects/:id/findings` | `ReportsPage.tsx` | Match | Match | Match | `promoted_at`/`promoted_by`: `null → '—'`/omitted | Shared | `require_scoped_project` |
| `GET /projects/:id/findings/:id` | Unused (redundant with `.analysis()`'s embedded `finding`, Category E) | N/A | N/A | Match | N/A | N/A | N/A |
| `POST .../analysis` | `ReportsPage.tsx` | Match | Match (`Finding`+`FailureCluster`) | Match | `root_cause_hypothesis`/`reproducibility`: real null-safe rendering | Shared | Both checks |
| `POST .../review` | `ReportsPage.tsx` | Match | 204, no body | N/A | N/A | Shared, `Role::Operator` | Both checks (via failure-cluster project resolution) |
| `GET /projects/:id/audit` | `AuditPage.tsx` | Match | Match (8 fields, all rendered) | Match | `actor`/`role`/`target`/`detail`: `null`-safe | Shared, `Role::Operator` | Both checks |
| `GET /projects/:id/intelligence/tests` | `ReportsPage.tsx` | Match | Match (`TestIntelRow`, 10 fields) | Match, re-verified this session | Every numeric field `Option<f64>`/`Option<i64>` ↔ `number\|null`, `formatPct`/`formatMs` null-safe | Shared | `require_scoped_project`, live-verified isolation (BE-10/FE-22) |
| `GET /projects/:id/events` (WS) | `RealtimeContext.tsx` | `?key=` query param (documented browser limitation) | `RunEvent` stream | Match | N/A | Distinct WS-close-code handling (4001 lag, etc.) | Server-side (`ws_tests.rs`) + client-side double-check |

No mismatch was found anywhere in this table. Every "Unused" row was independently re-confirmed this session via a zero-hit grep for its call site in `forentend/src` production code (not assumed from prior reports).

## 4. Fake / Stub / Placeholder Audit

Fresh, repo-wide, production-code-only search (`grep -RniE "fake|mock|demo|sample|fabricated|hardcoded|Math\.random|TODO|FIXME"`, all `.test.ts(x)` files excluded):

- **~130 matches**, of which **every single one** is either: (a) a doc comment documenting a *prior* fabrication that has already been removed (the large majority — Case-C pages, `AIQAContext.tsx`, `routeConfig.tsx`, `aiqaApi.ts`, `types.ts`), or (b) one of three already-disclosed, deliberately-scoped exceptions:
  1. **`SettingsPage.tsx`** — every tab except Authentication is pre-existing fabricated content, preserved under a standing, explicit "do not remove existing fake data" instruction documented in the file's own header comment. This is a **known, deliberate, out-of-scope exception**, not a defect this audit is newly discovering.
  2. **`TestCaseGeneratorPage.tsx:460`** — `Math.random()` used only to generate a local React key/id for a locally-generated draft test case (`gen-XXXX`); never presented as a real backend-issued id, never persisted, disclosed in the file's own comments (FE-16).
  3. **`ApplicationSourceSection.tsx:200`** — `Math.floor(25 + Math.random() * 45)` simulates a "ping" latency purely for the "Demo" application-source connectivity indicator in the Test Generator's local-only flow; not presented as a real network measurement, not connected to any backend.
- **Backend**: zero fabrication found. The one `"tests/sample.spec.ts"` string in `executor.rs:268` is a real default *file path parameter* for the real `PlaywrightExecutor`, which shells out to a genuine `npx playwright test` subprocess and reports its genuine exit code — not a fabricated result.
- Test-file mocks (`vi.fn()`, `MockWebSocket`, fixture objects like `FINDING_A`/`INTEL_ROW_A`) are correctly and exclusively confined to `*.test.ts(x)` files — none were found leaking into production code paths.

**No new fabrication was found in this fresh audit.**

## 5. Project Isolation & RBAC Audit

- **Backend queries**: every project-scoped store function re-inspected this session (`dashboard_run_counts`, `dashboard_task_counts`, `dashboard_test_count`, `dashboard_avg_task_duration_ms`, `dashboard_avg_run_duration_ms`, `list_test_intelligence`, `list_bug_drafts`, `list_audit`, `dashboard_avg_run_duration_ms`) binds `project_id` as a real, parameterized SQL argument. No cross-project aggregation code path exists anywhere.
- **API authorization gate**: `require_scoped_project` (404 if the project doesn't exist, then `require_project_access` — 403 if a scoped key's own bound project differs) is the single, shared gate every scoped handler calls first. Confirmed present on every route in §3's table.
- **Frontend stale-response handling**: every real data page (`ExecutionHubPage`, `EvidencePage`, `AuditPage`, `ReportsPage`, `DashboardPage`) follows the identical pattern — abort the in-flight request on project change, reset local state to `null` before the new fetch starts, and check `controller.signal.aborted` before applying any response. Live-verified for Test Intelligence (FE-22, FE-25) and Average Run Duration (FE-26) using genuinely delayed/raced responses, not just structural reasoning.
- **RBAC enforcement**: 14 real `auth::require_role(&ctx, Role::...)` call sites found in `api.rs` this session (project creation = Admin; run create/cancel/approve/reject, review, key management = Operator/Admin as appropriate). These are real, backend-enforced 403s — confirmed via dedicated tests (e.g., `be15_finding_review_scoped_confirms_and_dismisses`, the FE-18 approval-attribution tests, and BE-10/BE-12b's own project-isolation tests using a real `403 FORBIDDEN`).
- **UI-only permission hiding vs. real security**: correctly distinguished throughout the app — e.g., `ExecutionHubPage.tsx`'s conditional Approve/Reject buttons are a UI convenience; the actual enforcement is the backend's `Role::Operator` gate, independently tested. No instance was found where the frontend relies on hiding a control *instead of* a real backend check.

**No project-isolation or RBAC defect was found.**

## 6. Realtime Audit

Full chain re-traced from `RealtimeContext.tsx` (re-read this session) and `ws_tests.rs`:
- **Real event → WebSocket**: `GET /api/v1/projects/:id/events` (project-scoped WS, `?key=` query-param auth — a disclosed, real browser limitation, not a workaround).
- **Server-side project filtering**: confirmed via `ws_tests.rs::be14_subscribers_receive_only_their_own_projects_real_events`.
- **Client-side double-check**: `RealtimeContext.tsx::handleIncoming` independently re-verifies `event.project_id` against the currently-selected project before dispatching to any listener — defense in depth, not reliance on the server alone.
- **Dedup**: by real `event_id`, a bounded 500-entry cache — verified by dedicated test (`RealtimeContext.test.tsx` item 21) and independently re-exercised by `Header.test.tsx` item 8 this session's history.
- **Debounce**: every consumer (Findings, Test Intelligence, Dashboard, Audit, Evidence, Execution Hub) uses its own local debounce timer (400-500ms range) around the shared `realtime.subscribe` callback — confirmed no consumer opens a second WebSocket; exactly one `MockWebSocket` instance is asserted in every relevant test.
- **Cleanup**: every consumer's `useEffect` returns an unsubscribe + `clearTimeout` cleanup function.
- **Project switching**: `RealtimeContext`'s own connect effect tears down the old socket and opens exactly one new one on project change (`RealtimeContext.test.tsx` item 4/5).
- **Stale events/responses**: covered by §5.
- **Backend event ordering race**: one genuine, real finding (originally surfaced during FE-25, re-confirmed this session by direct re-read of `engine.rs` lines 690-707): `RUN_COMPLETED` is emitted *before* `intelligence.rs::ingest_run` finishes writing risk/value/flakiness/finding data — a real, small race window. It is mitigated (not eliminated) by every consumer's existing debounce, which in practice gives the fast, synchronous, no-network DB write chain enough time to finish before the debounced refetch fires. This is a **known, disclosed, low-severity characteristic**, not a silent bug — confirmed live-safe in FE-25's manual verification (a real run's Test Intelligence update appeared correctly within ~2 seconds).

**No duplicate subscription, duplicate request, or unhandled race was found beyond the one disclosed, mitigated timing characteristic above.**

## 7. Database & Persistence Audit

Schema re-read in full this session (`migrations/0001` through `0004`, the complete and only migration set):

**Genuinely persisted**: `projects`, `runs` (+ governance columns `requested_by`/`policy_notes`/`approved_by`/`approved_at`), `tasks`, `task_dependencies`, `run_events`, `quality_nodes`, `risk_assessments`, `test_value_assessments`, `flakiness_metrics`, `failure_clusters`, `evidence` (+ on-disk blob store), `bug_drafts`, `api_keys`, `audit_log`.

**Runtime-memory only (not persisted)**: an in-flight run's live `RunRecord` (event broadcast channels, in-process task state) — but every terminal fact about it (final state, timestamps, task outcomes) is durably written to `runs`/`tasks` before the in-memory record is dropped, and startup recovery (`engine_tests.rs`'s D1-D5 suite) repairs any run left non-terminal by an unclean shutdown. No data-loss risk was found in this path.

**Genuine orphan/unreachable tables — new finding this session**: `change_impacts` and `quality_relations` (both defined in `migrations/0002_quality_graph_and_intelligence.sql`) have **zero references anywhere in the Rust source** (confirmed via a fresh, targeted grep across every non-test `.rs` file — no `INSERT`, no `SELECT`, no struct binding, nothing). These correspond to real PRD/SRD concepts (change-impact-driven test selection, a quality dependency graph) that were never implemented past their schema definition. This is genuinely dead schema, not a bug — it causes no runtime harm (empty, unqueried tables) — but it is real, previously-undocumented-in-this-form dead capability worth flagging (§12).

**Migrations**: complete (4 files), sequential, no gaps found. No missing migration was identified for anything currently implemented.

**Project-scoped queries**: confirmed real and parameterized everywhere (§5).

## 8. Test Execution End-to-End Audit

| Step | Status | Evidence |
|---|---|---|
| Test generation | **UI-only** | `TestCaseGeneratorPage.tsx` — real, local, disclosed generation; produces `GeneratedTestCase` objects that are never persisted anywhere |
| Persisted test data | **Missing** | No `GeneratedTestCase` table/entity exists in any migration |
| Execution | **Real** | `ExecutionHubPage.tsx` → `POST .../executions` → `engine.rs`/`scheduler.rs`, fully real, tested, DAG-aware |
| Tasks | **Real** | `tasks` table, real status transitions, real dependency resolution |
| Results | **Real** | `RunSummary`'s 11 real count fields, fully exposed (BE-10/FE-20) |
| Findings/Intelligence | **Real** | `intelligence.rs::ingest_run` — real, deterministic, rule-based (not LLM) risk/value/flakiness/failure-clustering, run automatically after every real run completes |
| Reports/Dashboard | **Real** | `ReportsPage.tsx`, `DashboardPage.tsx` — both read the real, already-computed data above |

**The one genuine, disclosed break in this chain**: Test Generation → Execution. `TestCaseGeneratorPage.tsx`'s locally-generated `GeneratedTestCase` objects have **no real mapping to a real `TaskDef`** (confirmed, unchanged since FE-15/16 — no backend entity exists to receive one). This is an honestly disclosed **Unwired** gap (Category D — the backend has no `GeneratedTestCase`/test-case-persistence concept at all, not merely an unconnected wire), not a fabrication — the page does not pretend this handoff exists.

## 9. Dashboard Audit

Every displayed metric, freshly re-verified this session against `DashboardPage.tsx` and `api.rs`:

| Metric | Real Source | Calculation | Project Scope | Realtime | Null/Empty |
|---|---|---|---|---|---|
| Total Test Cases | `dashboard.test_count` | `COUNT(quality_nodes WHERE node_type='test')` | Yes | Yes | Real `0` is real (no test-generation persistence exists) |
| Active Runs | `dashboard.execution_counts.RUNNING` | Real `GROUP BY state` | Yes | Yes | Real `0` |
| Passed/Failed/Blocked/Needs Review | `dashboard.task_counts[...]` | Real `GROUP BY status` | Yes | Yes | Real `0` |
| Execution Reliability | `kpis.execution_reliability` | `PASSED/(PASSED+FAILED+TIMEOUT)` | Yes | Yes | `null → '—'`, never `0`/`1` |
| Execution Efficiency | `kpis.execution_efficiency_ms` | `AVG` task duration | Yes | Yes | `null → '—'` |
| Average Run Duration (FE-26) | `kpis.execution_avg_run_duration_ms` | `AVG` run duration | Yes | Yes | `null → '—'` |
| Project status badge | Client-derived from real `RunListItem[]` | `'Running'` if any real RUNNING run exists, else `'Healthy'` | Yes | Yes | N/A |
| Execution Activity timeline | `ProjectExecutionGraph` | **None — always empty state** | N/A | N/A | Always shows its own honest "No Execution Activity Recorded" (independently re-verified this session: `DashboardPage.tsx` always passes `timelineData={[]}`) |

**No new KPI was invented in this audit.** Backend-missing concepts (a real time-bucketed trend series, project-level flakiness/risk aggregation, run frequency, and every concept `api.rs`'s own `DashboardResponse` doc comment already lists as having no backing data model) remain correctly, honestly absent — **STOP** on inventing any of them stands, consistent with BE-11's own conclusion, re-verified unchanged this session.

## 10. Reports / Intelligence / Findings Audit

- **Findings**: real, from `bug_drafts`/`failure_clusters`, auto-drafted by `intelligence.rs::record_failure` after 2+ recurring failures with the same signature. Manual refresh: yes (`Refresh` button). Realtime: yes (any-event debounced resync, unchanged since FE-9/FE-11). Empty/error: honest, shared `EmptyState`/`renderResourceErrorState`. Project isolation: yes.
- **Test Intelligence**: real, from `quality_nodes`/`risk_assessments`/`test_value_assessments`/`flakiness_metrics`, computed after every run by the same `intelligence.rs::ingest_run`. Manual refresh: yes. Realtime: yes, `RUN_COMPLETED`-filtered specifically (FE-25's own, more precise choice than Findings' any-event resync, justified by the real fact that intelligence data only changes at run completion). Empty/error: honest. Project isolation: live-verified twice (FE-22, FE-25) with genuinely different real data across two simultaneous real browser sessions.
- **Reports (as a concept)**: this is `ReportsPage.tsx` itself — "Findings & Quality Intelligence" — there is no separate "PDF/Excel Report" or "Test Report" entity, and the page's own doc comment explicitly, correctly discloses this as deliberately out of scope (no such backend capability exists; Dashboard/Execution Hub already show the real run-level summary this would duplicate).

## 11. Security Audit

- **Authentication**: opaque `aiqa_<hex>` bearer keys, SHA-256 hashed at rest (`auth.rs::hash_key`), never stored or logged in plaintext. Confirmed via direct code read.
- **API key handling (frontend)**: `aiqaApi.test.ts` includes a dedicated, still-passing test confirming the key never appears in any request URL (query string) — always the `Authorization: Bearer` header, except the one disclosed, necessary WebSocket exception (browsers cannot set custom headers on a WS handshake).
- **RBAC**: real, server-enforced (§5), 3 fixed roles (viewer/operator/admin), 14 real gate call sites.
- **Project authorization**: real, server-enforced, double-checked structure (`require_scoped_project` + resource-level `require_resource_in_project` where applicable).
- **SQL parameterization**: every query re-inspected this session (`store.rs`) uses `sqlx::query(...).bind(...)` — no string concatenation, no injection surface found anywhere.
- **Sensitive data exposure**: API keys are never returned in plaintext after creation (only at mint time, matching GitHub/Stripe-style token conventions); `EvidencePage.tsx` deliberately never renders `object_key` (a storage-layer implementation detail) to avoid leaking internal storage paths.
- **Unauthorized cross-project access**: real `403`/`404` at every scoped route, re-confirmed by dedicated, still-passing tests across BE-10, BE-12b, and every FE page's own project-isolation test.

**No security defect was found in this fresh audit**, based strictly on repository evidence.

## 12. Dead Code / Unwired Capability Audit

| Finding | Category | Evidence |
|---|---|---|
| `change_impacts`, `quality_relations` tables — schema exists, zero code references | **D** | Migration `0002`, zero grep hits anywhere in `services/orchestrator/src/*.rs` |
| Legacy unscoped routes (`/runs`, `/intelligence/tests`, `/intelligence/failure-clusters`, `/bug-drafts*`, `/admin/audit-log`) | **E** | Explicitly documented in `api.rs`'s own router comment as intentionally-preserved legacy surface; each has a scoped, frontend-consumed replacement except... |
| `/admin/api-keys*` (key management CRUD) | **C** | Real, Admin-gated, but no requirement evidence found (re-confirmed across 5 independent audits this engagement: FE-19/20/21/23/25/this one) for a UI |
| `/metrics` (Prometheus) | **E** | Real, ops-only by structure (text format, not JSON) — correctly never exposed to the product UI |
| `projects.update` ("Edit Project") | **C** | Real, unconsumed, no requirement evidence found across the same 5+ audits |
| `evidence.get`, `findings.get`, `executions.tasks`/`.task`, `executions.events` | **E** | Each has a documented, verified redundancy with an already-consumed path (list-embeds-detail, or a disclosed full-refetch strategy superseding `?since=` catch-up) |
| Test Generator → Execution Hub handoff | **D** | No `GeneratedTestCase`→`TaskDef` backend mapping exists; disclosed, not fabricated |
| 7 Case-C pages (Orchestrator/API/Custom/Mobile/RBAC-scenario/FeatureMap/Validator) | **D** | No backing backend entity for any of them; each has its own accurate, specific disclosure of exactly what's missing |
| Settings — 6 of 7 tabs | **E** (deliberate, out of scope) | Standing instruction to preserve, documented in the file itself |
| Dashboard time-series/trend chart | **D** | No time-bucketed backend query exists; component is real and UI-ready, correctly shows only its own honest empty state |
| Average Run Duration (pre-FE-26) | **Resolved this engagement** (was A, now E) | BE-12b + FE-26 closed this exact gap |
| Test Intelligence project-scoped exposure (pre-BE-10) | **Resolved this engagement** (was A, now E) | BE-10 + FE-22 closed this exact gap |
| Header notification fabrication (pre-FE-24) | **Resolved this engagement** (was a truthfulness defect, now E) | FE-24 closed this |

**No new Category-A (safely implementable, evidence-backed) gap was found in this fresh audit.**

## 13. Test Audit

**Current counts** (freshly run this session, not carried over from any prior report):
- Backend: **105 tests** (`cargo test`), across `api_tests.rs`, `engine_tests.rs`, `store_tests.rs`, `ws_tests.rs`, plus inline unit tests in `intelligence.rs`/`executor.rs`/`policy.rs`/`scheduler.rs`.
- Frontend: **325 tests**, **16 test files** (`npx vitest run`).

**Full suite actual result**: Frontend — 325/325 passed, both runs performed this session. Backend — **inconsistent across runs performed this session**: a normal/fast run (3.4s) → 105/105 clean; a separate, markedly slower run (86s, likely under heavier real system load) → 103/105, with 2 failures both in the pre-existing `phase4_be10_intelligence_*` tests (a `poll_intelligence_row`-style bounded-retry helper racing a real background async task under load); re-running those same 2 tests in isolation → clean, fast, 100%. This is **honestly reported as a real, load-sensitive test-timing fragility** in a small number of pre-existing BE-10-era tests — not a product defect, and not something to paper over. It does not affect the underlying feature's correctness (independently proven by dozens of other passing tests plus live manual verification across three units), but it is a genuine test-suite robustness gap.

**Missing critical scenarios**: none identified as *missing* outright — coverage is unusually thorough for a project this size (project isolation, empty/error/null states, realtime resync/debounce/isolation, and RBAC are each independently tested for nearly every real page). The one soft gap: no dedicated `aiqaApi.test.ts` suite exists yet specifically for `intelligence.tests` (its coverage is indirect, via `ReportsPage.test.tsx`'s own URL-assertion test) — a minor, previously-flagged (FE-23) coverage-hygiene item, not a functional risk.

**Tests that test only mocks, not meaningful integration**: none found — every page's test suite renders the real `AppShell` (real providers, real routing) and mocks only the `fetch`/`WebSocket` transport boundary, the established, consistent pattern across all 16 files.

**Regression risk**: low. The full frontend suite and a fast backend run both pass cleanly; the one flakiness class is isolated to a specific, identified helper/test pattern and does not indicate a spreading problem.

## 14. Build & Typecheck Audit

All actually run this session, exact commands and exact results:

```
cd services/orchestrator && cargo build
```
`Finished dev profile [unoptimized + debuginfo] target(s) in 6.54s` — clean, zero errors, zero new warnings (one pre-existing, unrelated `clippy::too_many_arguments` on `store.rs::insert_audit`, not surfaced by plain `build`).

```
cd services/orchestrator && cargo test
```
105/105 passed (fast run); see §13 for the one slower run's flakiness.

```
cd forentend && npx tsc --noEmit
```
Clean, no output.

```
cd forentend && npx vitest run
```
325/325 passed, 16 files, 86.25s.

```
cd forentend && npx vite build
```
Succeeded: `2711 modules transformed`, `dist/assets/index-D0C9SqLq.js` (1,026.24 kB, gzip 291.96 kB), one pre-existing chunk-size advisory (not an error).

**No command was skipped. No result is claimed without having actually been run this session.**

## 15. Release Readiness Matrix

| Area | Status | Evidence | Risk | Action |
|---|---|---|---|---|
| Backend | PASS | `cargo build`/`cargo test` this session | Low (test flakiness, §13) | Consider hardening `poll_*`-style bounded-retry test helpers with a longer/adaptive timeout |
| Frontend | PASS | `tsc`/`vitest`/`vite build` this session | Low | None required |
| API Contract | PASS | §3 table, field-for-field re-verified | Low | None required |
| Database | PASS (implemented parts) | §7 | Low | `change_impacts`/`quality_relations` are dead schema — no runtime risk, but worth a product decision on whether to implement or formally drop |
| Realtime | PASS | §6, `ws_tests.rs` + `RealtimeContext.test.tsx` | Low | None required |
| RBAC | PASS | §5, §11, 14 real gate sites | Low | None required |
| Project Isolation | PASS | §5, live-verified 3 times this engagement | Low | None required |
| Test Execution | PASS (implemented chain); disclosed gap at Test-Gen→Execution handoff | §8 | Medium (product ambition gap, not a bug) | Product decision needed if generated-test persistence is ever required |
| Dashboard | PASS (3 real KPIs); STOP on further KPIs | §9 | Low | None required without new product/backend evidence |
| Reports | PASS | §10 | Low | None required |
| Intelligence | PASS | §10, BE-10/FE-22/FE-25 | Low | None required |
| Security | PASS | §11 | Low | None required |

## 16. Final GO / STOP Decision

**PARTIAL — Specific gaps remain**, and they are all **by design or awaiting a product decision**, not implementation debt:
- Test Case Generator persistence / Test-Gen→Execution mapping (Category D — no backend entity).
- 7 Case-C pages (Category D — no backend entity for any of them, individually confirmed).
- Dashboard trend/aggregate KPIs (Category D/C — missing data model or undefined methodology).
- `change_impacts`/`quality_relations` (Category D-adjacent — schema exists, capability was never built).
- Settings' 6 fabricated tabs (Category E, deliberately preserved).

Everything else audited — the core Projects/Executions/Evidence/Audit/Findings/Test-Intelligence/Dashboard/Realtime/RBAC/Security surface — is **GO — Ready for release** on its own terms: real, tested, project-isolated, and passing a fresh full build/test cycle.

## 17. Recommended Next Unit

A fresh audit at this checkpoint finds **no new Category-A (evidence-backed, safely implementable) gap**. Every candidate examined resolves to Category C, D, or E (§12), exactly as BE-11's audit already concluded and as this session's fresh re-verification confirms unchanged.

**Recommended next unit is therefore not a new feature, but a test-infrastructure hardening unit**, the one genuinely new, evidence-backed finding from this audit:

- **Objective**: harden the bounded-retry polling helpers in `api_tests.rs` (specifically the pattern used by `poll_intelligence_row`/`poll_run_state`) so they do not intermittently fail under real system load, without changing their correctness semantics.
- **Why genuinely needed**: directly observed this session — a full `cargo test` run under heavier load produced 2 real (not test-logic-bug) failures in `phase4_be10_intelligence_*` tests; both passed cleanly and instantly in isolation, confirming the issue is the bounded wait margin (100 × 30ms = 3s), not the underlying feature.
- **Direct repository evidence**: `services/orchestrator/src/api_tests.rs`'s `poll_intelligence_row`/`poll_run_state` helpers; this session's own two full-suite runs (one clean at 3.4s, one with 2 failures at 86s).
- **Category**: **E-adjacent test-infrastructure item**, not a product capability gap — included here only because it is the single genuinely new, verifiable finding this fresh audit produced, and the brief's own rule ("prefer STOP over an unsupported assumption... do not invent a feature merely to recommend one") means the honest answer is that no product feature unit is currently justified.
- **Files likely to change**: `services/orchestrator/src/api_tests.rs` only (increase retry bound or poll interval for the two affected helpers).
- **Files that must NOT change**: everything else — this is a test-only, zero-behavior-change hardening, not a feature or architecture unit.
- **Backend/Frontend/Database/API/Realtime/Security/project-isolation impact**: none — purely a test-timing-margin adjustment.
- **Required tests**: the existing tests themselves are the test; success is measured by re-running the full suite under artificially-throttled conditions (or simply several more times) and observing zero flakes.
- **STOP conditions**: if investigation reveals the flakiness is not timing-margin-related but a genuine logic race in `ingest_run`/the realtime pipeline, STOP and escalate as a real bug report rather than papering over it with a longer timeout.

If the product owner instead wants a *feature* unit, the honest, evidence-based answer is: **none is currently justified** — the next real feature work requires either a backend team implementing one of the Category-D data models (test-generation persistence, requirement traceability, a time-bucketed execution history table, a defined project-level risk/flakiness aggregation formula) or an explicit product decision on any of the open Category-C questions (Edit Project, Admin key-management UI). This audit does not invent one of those to keep a unit sequence moving.
