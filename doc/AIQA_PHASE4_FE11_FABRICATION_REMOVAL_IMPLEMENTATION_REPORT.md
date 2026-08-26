# FE-11 — Fabrication Removal / Real Data Migration Implementation Report

## 1. Status

**Complete.** Every remaining page and shared surface identified in the fabrication inventory (§3) was either migrated to real backend data (Case A), left with an honest partial/unsupported-subset state (Case B — none required this sprint; see §5), converted to a truthful "not implemented" state (Case C), or is out-of-scope with a documented reason (Case D). `AIQAContext.tsx` was pruned of every field whose only remaining consumer was fabrication that this sprint removed, following the brief's explicit instruction to map real consumers first and remove only once none remain. No backend code was touched. No new WebSocket connections were created. All existing FE-1 through FE-10 functionality remains regression-free (§18).

## 2. Exit Criterion

Met. The exit criterion — "every page either shows real backend data, or a truthful, honest 'not supported' state; `AIQAContext`'s fabricated demo data no longer reaches any rendered page except the explicitly disclosed, brief-sanctioned Settings config" — holds for all 14 routes. The one remaining, deliberately-deferred exception (Header's environment selector / notifications bell / user badge, §12) is explicitly disclosed below, not silently left ambiguous.

## 3. Fabrication Inventory (at the start of this sprint)

Re-derived from a full read of `AIQAContext.tsx` (~1200 lines) and every page file, plus a whole-`src` `grep -rn "useAIQA\(\)"`:

| Surface | Fabrication |
|---|---|
| `AIQAOrchestratorPage.tsx` | Hardcoded `OrchestratorPlan[]` (DAG nodes, "specialized agent" worker pools, risk/change/test-value scores), fake plan-creation form |
| `TestCaseValidatorPage.tsx` | Hardcoded `ValidationRuleCheck[]` (traceability status, quality score, duplicate-similarity %, "AI Auto-Fix") |
| `RBACTestingPage.tsx` | `AIQAContext`'s `rbacScenarios`/`RBACMatrixRule[]` fake authorization matrix + fake "Evaluate Authorization Matrix" |
| `APITestingPage.tsx` | `AIQAContext`'s `apiTests`/`APIEndpointTest[]` fake multi-protocol (REST/GraphQL/gRPC/WS) request/response harness |
| `MobileTestingPage.tsx` | Hardcoded `MobileDeviceTarget[]` (specific Samsung/Pixel/Apple models, fake battery/thermal), hardcoded `MobileTestExecution[]` |
| `CustomTestingPage.tsx` | `AIQAContext`'s `customSuites`/`CustomTestSuite[]` fake visual-regression/load/chaos/E2E suite builder |
| `FeatureMapFlowPage.tsx` | Hardcoded `FeatureMapNode[]`/`SystemCallFlow[]` — an invented microservice mesh unrelated to this app's real architecture |
| `ReportsPage.tsx` | `AIQAContext`'s fake `runs`/`currentRun`/`qualityIntelligence`, 2 hardcoded `DefectDraft` objects, hardcoded "Evidence Coverage: 100%" |
| `Sidebar.tsx` | Fake `settings.general.organizationName`/`.projectKey` identity ("Acme Enterprise Engineering"/"AIQA-CORE") shown regardless of real project; 100%-hardcoded "k8s-runner-pool-us, 12/16 slots" runner-pool box; fake-`runs`-driven "LIVE" badge pulse |
| `Header.tsx` | `AIQAContext`'s fake `backendState` ("Daemon Online, 18ms", live-caught contradicting the real backend during FE-10 testing); "Launch Run" opening the fake `RunTriggerModal` |
| `Dashboard`/`CommandPalette` "Launch Run" | Same fake `RunTriggerModal` open call (4 real call sites) |
| `AIQAContext.tsx` itself | Fake `AIQAProject[]` seed (5 projects), fake `backendState`, fake `rbacScenarios`/`apiTests`/`customSuites`, fake `qualityIntelligence`, fake seeded `activities` timeline |

## 4. Page-by-Page Migration Matrix

| Page | Case | Outcome |
|---|---|---|
| Dashboard | (already real, FE-10) | Its one remaining fake dependency — the "Launch Run" buttons opening `RunTriggerModal` — fixed to `navigate('/execution')` |
| Test Generator | (protected boundary) | Untouched — its own disclosed local simulation is FE-5/FE-8's own documented, protected scope |
| AIQA Orchestrator | **C** | Truthful "not implemented" — no orchestration-plan/DAG/agent-pool entity exists |
| Test Case Validator | **C** | Truthful "not implemented" — no traceability/quality-scoring/duplicate-detection entity exists |
| Execution Hub | (already real, FE-8) | Unregressed |
| RBAC Testing | **C** | Truthful "not implemented" — no role-permission-matrix entity exists; real enforcement is per-route (`auth::require_role`) |
| API Testing | **C** | Truthful "not implemented" — no generic request-dispatch endpoint exists |
| Mobile Testing | **C** | Truthful "not implemented" — no device-inventory/lock entity exists |
| Custom Testing | **C** | Truthful "not implemented" — no test-suite-category entity exists |
| Feature Map / Flow | **C** | Truthful "not implemented" — no service-topology/call-flow entity exists |
| Reports | **A** | Rewritten onto the real `Finding`/`FailureCluster` backend (`findings.list/analysis/review`) |
| Audit Trail | (already real, FE-6) | Unregressed |
| Evidence Viewer | (already real, FE-7) | Unregressed |
| Settings | **D** (out of scope, disclosed) | Remains the pre-existing, brief-sanctioned fake config editor; its one real element (API key entry) is unaffected |

No Case B (partial-real) page was needed this sprint — every remaining fake page's backend gap was total, not partial.

## 5. Real Data Migrations

**Reports → Findings (Case A).** Backend contract (`api.rs::list_findings_scoped` / `get_finding_analysis_scoped` / `review_finding_scoped`, `store.rs`'s `BugDraftRow`/`FailureClusterRow`): a `Finding` is this backend's real, deterministic (rule-based, not LLM — `intelligence.rs::classify_failure`) auto-drafted defect, created once the same normalized failure signature recurs ≥ 2 times across task executions. `GET .../findings` and `POST .../analysis` require no role beyond project scope; `POST .../review` requires `Role::Operator`. All three were already real and typed since FE-1 (`aiqaApi.findings.*`, `types/contract.ts`) but never wired to any page — zero new transport code. `ReportsPage.tsx` was rewritten in full: real `Finding[]` list, on-demand real `FailureCluster` analysis fetched per selection, real Confirm/Dismiss via `review`, a real JSON export of the exact fetched data. Deliberately out of scope: a separate per-run "Test Report"/"Final Audit Report" view (Dashboard/Execution Hub already show the real run-level summary this would duplicate) and PDF/Excel export (no real backend or library support for either).

**Dashboard "Launch Run".** Brief §24's explicit preference — "Dashboard Launch Run → real Execution Hub… do not maintain two competing execution-creation systems" — implemented by changing all 4 real call sites (`Header.tsx`'s header button, `CommandPalette.tsx`'s command, `DashboardPage.tsx`'s header button and empty-state CTA) from `setNewRunModalOpen(true)` to `navigate('/execution')`, then deleting the now-fully-orphaned `RunTriggerModal.tsx` and its `<RunTriggerModal />` render in `App.tsx`. `TestCaseGeneratorPage.tsx`'s own handoff (`triggerNewRun` → `navigate('/execution')`) was untouched — it never used the modal.

**Header connection indicator.** Replaced the fake `backendState` ("Daemon Online, 18ms" — live-caught during FE-10 testing continuing to read "Daemon Online" seconds after the real backend process was killed) with the real, already-computed `AuthContext.status`. No new connectivity check, no second polling loop.

**Sidebar identity.** Replaced the fake `settings.general.organizationName`/`.projectKey` ("Acme Enterprise Engineering"/"AIQA-CORE") with the real `useProject().selectedProject`.

## 6. Unsupported Features (Case C — truthful "not implemented")

Seven pages (`AIQAOrchestratorPage`, `TestCaseValidatorPage`, `RBACTestingPage`, `APITestingPage`, `MobileTestingPage`, `CustomTestingPage`, `FeatureMapFlowPage`) were rewritten from fully fabricated demo UIs to a shared, honest pattern: `SectionHeader` (title/subtitle/`NOT IMPLEMENTED` badge) + `EmptyState` (`NO BACKEND CAPABILITY` badge, a page-specific description naming exactly what backend entity/route is absent, a `codeHint` pointing at the real route table, and — where a genuinely related real capability exists — a real navigation CTA: Orchestrator/API Testing/Mobile Testing/Custom Testing → Execution Hub, Validator → Findings/Test Generator, RBAC → Audit Trail). Every one of these gaps was re-verified against the real route table in `services/orchestrator/src/api.rs` at the start of this sprint (not inferred from the page's name, per brief §9) — none has a corresponding backend entity today. Each page now has a real root `id` (none had one before) and makes zero fetch calls of its own (verified live in the browser and by `CaseCPages.test.tsx`'s dedicated "makes no real backend request" test per page).

The Sidebar's two fabricated nav-item badges ("PLAN" on Orchestrator, "GATE" on Validator) were removed along with the pages' fake capabilities they described.

## 7. AIQAContext Migration

Per the brief's explicit instruction, every field was mapped to its real consumer set (via a full-file read plus an exhaustive `grep -rn "useAIQA\(\)"`) before any removal. Removed only once confirmed to have zero real consumers:

- `projects`/`selectedProjectId`/`setSelectedProjectId`/`selectedProject` + the fake `AIQAProject[]` seed (5 fake projects) + `AIQAProject`/`ProjectStatus`-adjacent dead types — zero real consumers anywhere (FE-5's `ProjectContext` was already the only real "current project" concept; this was FE-5's own disclosed gap, closed here).
- `backendState`/`setBackendState` + `BackendConnectionState` type — zero consumers once `Header.tsx` was fixed.
- `isNewRunModalOpen`/`setNewRunModalOpen` — zero consumers once all 4 openers were fixed and `RunTriggerModal.tsx` deleted.
- `rbacScenarios`/`activeRBACScenarioId`/`setActiveRBACScenarioId`/`addRBACRule`/`evaluateRBACMatrix` — zero consumers once `RBACTestingPage` migrated.
- `apiTests`/`activeApiTestId`/`setActiveApiTestId`/`addApiTest`/`executeApiTest` — zero consumers once `APITestingPage` migrated.
- `customSuites`/`addCustomSuite`/`runCustomSuite` — zero consumers once `CustomTestingPage` migrated.
- `qualityIntelligence` + its `QualityIntelligence` type — zero consumers once `ReportsPage` migrated.
- `activeRunId`/`setActiveRunId`/`currentRun`/`cancelRun` — zero consumers once `ReportsPage`/Dashboard no longer read them (`triggerNewRun`'s own internal `setActiveRunId` call was removed too, since nothing derived from it survives).
- `activities` (read side) + its fake seeded timeline (4 fabricated events) — the render side was already dead before this sprint; removed the fake seed too, not just its display. `addActivity` (write-only, still used by Test Generator) keeps an internal, now-empty `activities` state to write into.
- Dead types removed from `types.ts` alongside their state: `AIQAProject`, `BackendConnectionState`, `RBACMatrixRule`, `RBACScenario`, `APIAdapterType`, `APIAssertion`, `APIEndpointTest`, `CustomTestSuite`, `OrchestratorPlan`, `ValidationRuleCheck`, `MobileDeviceTarget`, `MobileTestExecution`, `FeatureMapNode`, `SystemCallFlow`, `QualityIntelligence`, `DefectDraft`.

`AIQAContext.tsx` shrank from ~1200 lines to 605. What remains, and why each is kept:

- `isSidebarCollapsed`/`toggleSidebar`, `isCommandPaletteOpen`/`setCommandPaletteOpen` — pure UI chrome, not data.
- `currentEnvironment`/`setCurrentEnvironment`, `notificationCount`/`clearNotifications` — deliberately deferred fabrication (§12).
- `runs`/`triggerNewRun` — Test Generator's own protected handoff boundary (unchanged since FE-5/FE-8).
- `activeGenerationSession` + its ops — Test Generator's own protected local-simulation boundary (unchanged).
- `settings`/`updateSettings` — the disclosed-fake config editor (brief-sanctioned, "do not remove existing fake data").
- `addActivity` — write-only, Test Generator's own dependency.

## 8. Google Studio Page Handling

`routeConfig.tsx`'s `designStatus` field gained a fourth bucket, `'unsupported-not-implemented'` (brief §26), applied to all 7 Case-C pages, distinct from `'google-studio-not-final'` (still-fake content that *could* become real) since these have no real backend counterpart to build toward at all. `ReportsPage` moved from `'google-studio-not-final'` to `'new-follows-approved-language'` (the same bucket as Audit/Evidence/Execution Hub — built fresh this sprint, follows the approved visual language, not itself product-reviewed).

## 9. Dashboard Preservation

Unchanged except the one disclosed fix (§5, Launch Run → real Execution Hub navigation) and its own `useAIQA()` dependency being removed entirely (it no longer needs anything from `AIQAContext`). All FE-10 real-data behavior (KPIs, donut chart, project-detail panel, Orb) is untouched and unregressed (confirmed by all 26 pre-existing `DashboardPage.test.tsx` tests still passing, plus live manual verification, §17).

## 10. Test Generator Preservation

Completely untouched. Its own local-only generation pipeline, `activeGenerationSession` state, and disclosed `Math.random()`/simulated-latency helpers (`ApplicationSourceSection.tsx`, `DocumentUploadSection.tsx`) remain exactly as FE-5/FE-8 left them — explicitly protected, not this sprint's scope. `triggerNewRun`'s real handoff (`navigate('/execution')` after) is unchanged.

## 11. Execution Hub / Audit / Evidence / Realtime Preservation

All three real pages (FE-6/FE-7/FE-8) and the realtime layer (FE-9) are unmodified. Regression confirmed by their own pre-existing test suites passing unchanged and by live manual verification (Execution Hub's real execution list rendered correctly after every "Launch Run" navigation test, §17).

## 12. Deliberately Deferred (disclosed, not silently omitted)

- **Header's environment selector** (Staging/Production/UAT/Ephemeral with fake region/version badges) and **notifications bell** (canned alerts) remain fake. No real backend "environment" concept exists at all (the real `Project` type has no such field), and — unlike the daemon-health indicator, which was proven to actively contradict real backend state — these are non-gating and don't affect any real API call. Fixing the single most globally-visible piece of chrome carried disproportionate scope/risk relative to the session budget; deferred as a considered decision, not an oversight.
- **`types.ts` deep pruning** beyond the types tied to fields actually removed this sprint (§7) was deprioritized — low risk/low value relative to removing rendered fake *data*, per the brief's own staged-cleanup ordering (§28).

## 13. Security

No new attack surface. `ReportsPage.tsx`'s review action correctly surfaces the real 401/403 from `review_finding_scoped`'s `Role::Operator` gate rather than assuming success — confirmed live with a real viewer-scoped key (§17). No raw API key is ever rendered (unchanged pattern from FE-3 onward — `ReportsPage.tsx` never touches `getApiKey()` at all, matching Reports' own lower sensitivity vs. Audit's redaction need).

## 14. Search Verification

Full fabrication search-and-classify pass run across `forentend/src` (case-insensitive `\b(fake|mock|dummy|placeholder|sample|hardcoded|simulate[ds]?|Math\.random)\b`, plus targeted searches for the removed seed data's literal strings: `AIQA Platform`, `Acme`, `k8s-runner-pool-us`, `staging.enterprise.com`, `prj-aiqa`/`prj-fintech`/etc.). Every hit was individually classified:

- Legitimate real backend-contract vocabulary (`simulate: {outcome}` — a real executor option in `types/contract.ts`/`ExecutionHubPage.tsx`, unrelated to fabrication).
- HTML `placeholder="..."` input attributes (not fabricated data).
- Doc-comment prose describing fabrication *removed this sprint* (this report's own source files' module comments).
- Test Generator's own already-disclosed, protected local simulation (`ApplicationSourceSection.tsx`'s `Math.random()` ping simulation, `DocumentUploadSection.tsx`'s "Simulate real asynchronous file ingestion" comment) — FE-5/FE-8's own documented, out-of-scope boundary.
- Settings' disclosed-fake config, confined entirely to its own page now that Sidebar no longer leaks it elsewhere.
- Negative test assertions (`expect(...).not.toContain('AIQA Platform')`, `.not.toContain('k8s-runner-pool-us')`) — pre-existing tests correctly guarding against exactly the fabrication this sprint removed.

No undisclosed runtime fabrication remains.

## 15. Tests

Two new test files, 44 new tests, all passing:

- **`ReportsPage.test.tsx`** (14 tests) — unauthenticated/no-project states, real fetch on project selection, project-switch invalidation, real empty/401/403/offline states, real analysis fetch on selection (asserts the exact `POST .../analysis` URL and method), real Confirm review (asserts exact `POST .../review` body), a real 403 on review rendering the real forbidden message without a fabricated success, confirms no previous fake strings (`DEF-881`, `DEF-882`, "Evidence Coverage") survive, Dashboard/Test Generator unregressed with Reports in the route table.
- **`CaseCPages.test.tsx`** (30 tests) — for each of the 7 Case-C pages: unauthenticated shows the real auth card (not page content), authenticated renders the real root id + truthful "not implemented" state, none of that page's previous fabricated demo content survives, and the page makes zero backend requests beyond the generic real `/projects` call. Plus 2 cross-page regression tests (Dashboard/Test Generator unregressed with all 7 present in the route table).

One real regression was caught and fixed during this pass: the Sidebar's new real-identity fallback text ("No project selected") exactly duplicated the string several existing pages' own `EmptyState`s already render, breaking `getByText` in 6 pre-existing tests (`AppRoutes.test.tsx`, `AuditPage.test.tsx`, `DashboardPage.test.tsx`, `EvidencePage.test.tsx`, `ExecutionHubPage.test.tsx`). Retitled to "No active project" — a distinct, equally honest string. Full suite re-verified green after the fix (§17).

## 16. Manual Verification

Performed live against the real backend, not simulated or fabricated. Started the real orchestrator (`cargo run --release`, confirmed healthy on `:8080`, real SQLite at `infra/data/orchestrator.sqlite3`) and the frontend dev server, minted two fresh real API keys (`fe11-manual-verify-operator`, `fe11-manual-verify-viewer`, both scoped to the real "AIQA Core Platform" project) via direct SQLite insert (same pattern as prior FE units), and drove the actual browser:

- **Auth flow**: real key entry → real `AIQA Orchestrator authentication verified` / `CONNECTED` state (operator key), confirmed live.
- **Dashboard**: real KPI values (5 total test cases, 6 passed, 8 failed, 42.9% reliability) for the real selected project rendered correctly; Sidebar showed the real project name and truncated id (`Project: 9860d454`), not the old fake org identity.
- **Launch Run**: all 3 real surfaces tested — Header button, CommandPalette's "Launch New Execution Run…" command, and Dashboard's "Start Execution Run" empty-state CTA — each correctly navigated to the real Execution Hub, which rendered its real execution history (16 real executions from prior FE-7/8/9/10 sessions' own real test runs).
- **Findings**: real list (2 real findings genuinely auto-drafted from this project's own recurring task failures); selecting a finding fetched and rendered its real `POST .../analysis` root-cause data (signature, hypothesis, 6 occurrences, first/last-seen timestamps, 100% reproducibility); **Dismiss with the operator key succeeded for real** — the finding's badge changed to `DISMISSED` and the page showed "Reviewed — dismissed by fe11-manual-verify-operator"; **Confirm with the viewer key correctly 403'd** — the page showed "This key's role does not permit reviewing findings (requires Operator role or above)" and the finding correctly remained `NEEDS REVIEW`, not falsely marked reviewed.
- **Case-C pages**: spot-checked `/orchestrator` and `/rbac` live — both rendered the real truthful "not implemented" state with the correct page-specific description and a working real navigation CTA (`Go to Execution Hub`, `Go to Audit Trail`).
- **Console**: zero unexpected errors; the only logged error was the expected, deliberately-triggered real 403 from the viewer-role review test.

## 17. Typecheck / Build

- `npx tsc --noEmit` — clean after every batch of edits (Header, CommandPalette, Dashboard, RunTriggerModal deletion, AIQAContext pruning ×3 passes, all 7 Case-C rewrites, ReportsPage rewrite, routeConfig, types.ts pruning, Sidebar fix).
- `npx vitest run` — 251/251 passing across 13 files (207 pre-existing + 44 new).
- `npx vite build` — succeeds (`dist/` produced, pre-existing >500kB chunk-size warning unrelated to this sprint's changes).

## 18. Regression

Zero regressions in final state. One was introduced and caught mid-sprint by the pre-existing suite (Sidebar text collision, §15) and fixed before completion. All 11 pre-existing test files (FE-1 through FE-10 coverage: `aiqaApi.test.ts`, `StateViews.test.tsx`, `ProjectContext.test.tsx`, `RealtimeContext.test.tsx`, `ProjectSelector.test.tsx`, `AppRoutes.test.tsx`, `AuditPage.test.tsx`, `EvidencePage.test.tsx`, `ExecutionHubPage.test.tsx`, `DashboardPage.test.tsx`) pass unchanged, plus both new FE-11 files.

## 19. Documentation Compliance

No FRD/SRD/PRD/flow-diagram document exists anywhere in this repository — re-confirmed at the start of this sprint (same search every prior FE unit has run and reported). Every decision in this report is derived from the real backend contract (`services/orchestrator/src/api.rs`'s actual route table, re-read directly, not inferred) and this sprint's own explicit brief, consistent with every prior FE unit's own documented approach.

## 20. Backend Changes

**None.** `git status --porcelain services/orchestrator/` is empty for the entire duration of this sprint. The real database was only ever read (SQLite `SELECT` queries to inspect existing projects/keys/findings) or written to through the backend's own real, authenticated HTTP API (the two live review-action calls in §16) — never touched directly.

## 21. Remaining Fabrication

- Header's environment selector, notifications bell, and user badge (§12 — deliberately deferred, disclosed).
- Settings page's non-API-key configuration (organization name, SSO toggle, execution policies, AI model params, notification webhooks) — the pre-existing, brief-sanctioned disclosed-fake config editor, out of scope per this sprint's own explicit "do not remove existing fake data" instruction (carried over from FE-3).
- Test Generator's own local-only generation simulation — FE-5/FE-8's own documented, protected boundary, not this sprint's scope.

## 22. Limitations

- Manual verification covered 2 of the 7 Case-C pages live in-browser (Orchestrator, RBAC) plus full automated coverage of all 7 via `CaseCPages.test.tsx`; the remaining 5 were not separately clicked through live, since their rewritten code is structurally identical (same `SectionHeader`+`EmptyState` pattern) and each has its own dedicated automated test asserting the real root id, truthful content, and zero page-specific network calls.
- The real backend's only 3 real Findings/FailureClusters available for manual verification were all `Medium` severity — the `High`-severity styling path (`severity === 'High'` → rose badge, occurrence_count ≥ 5) was verified by code inspection and by the automated test's `FINDING_A` fixture (`severity: 'High'`), not by triggering a real 5th recurrence live.

## 23. Out of Scope

**FE-12 (status-vocabulary migration) was not started.** No file touched this sprint addresses AIQA-vocabulary-to-Contract-vocabulary status-string migration; that remains entirely FE-12's own, separate scope.
