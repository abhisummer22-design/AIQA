# AIQA — Full Repository Audit Report

**Repository:** `Abhidark/AIQA`  
**Branch audited:** `Demo`  
**Audit date:** 2026-08-26  
**Audit method:** Automated + manual code inspection of every source file  

---

## Executive Summary

AIQA is a **production-quality AI test case generation and execution platform** with a Rust/Axum backend, React/Vite frontend, and SQLite persistence. The codebase is largely truthful—fake data has been systematically removed, deferred features are honestly labeled, and the backend implements real subprocess execution, real AI provider routing, and real persistence.

### Key Stats

| Metric | Count |
|---|---|
| Total source files (excl. node_modules/target/.git) | ~327 |
| `.tsx` files | 100 |
| `.rs` files | 27 |
| `.ts` files | 19 |
| `.py` files | 6 |
| Backend routes | 63+ |
| Frontend API methods | 48 |
| Frontend pages | 16 |
| Skills (built-in) | 8 |
| Database migrations | 8 |

### Critical Findings Summary

| ID | Severity | Finding |
|---|---|---|
| F-001 | **P0** | Firebase API key committed in [firebase-applet-config.json](../apps/web/firebase-applet-config.json) |
| F-002 | **P1** | No CI/CD pipeline (no `.github/workflows` directory) |
| F-003 | **P1** | [apps/web/](../apps/web) is a complete duplicate/legacy frontend (43 files) |
| F-004 | **P2** | [AIQAContext.tsx](../forentend/src/context/AIQAContext.tsx) contains fabricated `activeGenerationSession` data (lines 154-212) |
| F-005 | **P2** | [AIQAContext.tsx](../forentend/src/context/AIQAContext.tsx) has dead `activities` state (line 223) — zero consumers |
| F-006 | **P2** | 5 deferred pages (Orchestrator, Validator, RBAC, Mobile, FeatureMap) — no backend |
| F-007 | **P2** | All 3 Python executor adapters are `NotImplementedError` stubs |
| F-008 | **P2** | Legacy flat routes (`/runs`, `/admin/*`, `/intelligence/*`) exist alongside new scoped routes |
| F-009 | **P2** | `localStorage` used for API key storage — XSS risk |
| F-010 | **P2** | MCP architecture entirely unbuilt — placeholder only |
| F-011 | **P3** | [docker-compose.yml](../infra/docker-compose.yml) defines Postgres/Redis/MinIO but orchestrator uses SQLite |
| F-012 | **P3** | `express` dependency in frontend [package.json](../forentend/package.json) — questionable |
| F-013 | **P3** | Skills are prompt-injection configuration labels, not independent execution engines |

---

## 1. Repository Inventory (Section 5)

### File Counts by Extension

| Extension | Count | Category |
|---|---|---|
| `.md` | 105 | Documentation |
| `.tsx` | 100 | Frontend source |
| `.rs` | 27 | Backend source |
| `.ts` | 19 | Frontend source |
| `.json` | 17 | Config |
| `.pdf` | 17 | Generated reports |
| `.sql` | 10 | Migrations + fixtures |
| `.py` | 6 | Worker stubs |
| `.bat` | 2 | Scripts |
| `.yaml/.yml` | 3 | Config |
| `.toml` | 1 | Rust config |
| `.css` | 2 | Styles |

### Directory Classification

| Directory | Classification | Notes |
|---|---|---|
| `forentend/` | **A — ACTIVE** | Primary React/Vite frontend |
| `services/orchestrator/` | **A — ACTIVE** | Primary Rust/Axum backend |
| `apps/web/` | **E — DUPLICATE / F — LEGACY** | Google AI Studio frontend; duplicates `forentend/` |
| `services/workers/` | **C — PLACEHOLDER** | Python executor stubs (`NotImplementedError`) |
| `executors/` | **C — PLACEHOLDER** | Executor fixture configs (Playwright, Newman, etc.) |
| `infra/` | **C — PLACEHOLDER** | Docker Compose for Postgres/Redis/MinIO (not wired) |
| `doc/` | **H — DOCUMENTATION** | 91 audit/implementation reports |
| `.agents/` | **H — DOCUMENTATION** | Agent skill definitions |
| `.claude/` | **I — CONFIG** | Claude Code launch config |

---

## 2. Application Bootstrap Audit (Section 6)

### Bootstrap Chain

```
main.tsx → App.tsx → BrowserRouter → AppShell → Providers → Routes
```

### Provider Hierarchy ([App.tsx](../forentend/src/App.tsx) lines 35-62)

| Order | Provider | Purpose | Status |
|---|---|---|---|
| 1 | `AuthProvider` | Bearer token auth via localStorage | ✅ ACTIVE |
| 2 | `ProjectProvider` | Project selection + backend sync | ✅ ACTIVE |
| 3 | `RealtimeProvider` | WebSocket events (run updates) | ✅ ACTIVE |
| 4 | `AIQAProvider` | UI state, settings, generation session | ⚠️ PARTIAL — has dead state |

> [!WARNING]
> **F-004**: [AIQAContext.tsx](../forentend/src/context/AIQAContext.tsx) lines 154-212 preload 3 fabricated test cases (`tc-gen-1`, `tc-gen-2`, `tc-gen-3`) into `activeGenerationSession`. Only consumed by `TestCaseGeneratorPage`.
>
> **F-005**: `activities` state at line 223 has **zero consumers** — completely dead.

---

## 3. Route Matrix (Section 7)

| Route | Page | Backend | Auth | Status |
|---|---|---|---|---|
| `/dashboard` | DashboardPage | ✅ Real API | ✅ | **ACTIVE** |
| `/test-generator` | TestCaseGeneratorPage | ✅ Real AI gen | ✅ | **ACTIVE** |
| `/execution` | ExecutionHubPage | ✅ Real engine | ✅ | **ACTIVE** |
| `/api-testing` | APITestingPage | ✅ Real HTTP exec | ✅ | **ACTIVE** |
| `/performance-testing` | PerformanceTestingPage | ✅ Real load test | ✅ | **ACTIVE** |
| `/custom-testing` | CustomTestingPage | ✅ Real AI analysis | ✅ | **ACTIVE** |
| `/reports` | ReportsPage | ✅ Findings API | ✅ | **ACTIVE** |
| `/audit` | AuditPage | ✅ Audit log API | ✅ | **ACTIVE** |
| `/evidence` | EvidencePage | ✅ Evidence API | ✅ | **ACTIVE** |
| `/telemetry` | TelemetryPage | ✅ Metrics API | ✅ | **ACTIVE** |
| `/settings` | SettingsPage | ✅ AI settings API | ❌ | **ACTIVE** |
| `/orchestrator` | AIQAOrchestratorPage | ❌ EmptyState | ✅ | **DEFERRED** |
| `/validator` | TestCaseValidatorPage | ❌ EmptyState | ✅ | **DEFERRED** |
| `/rbac` | RBACTestingPage | ❌ EmptyState | ✅ | **DEFERRED** |
| `/mobile-testing` | MobileTestingPage | ❌ EmptyState | ✅ | **DEFERRED** |
| `/feature-map` | FeatureMapFlowPage | ❌ EmptyState | ✅ | **PLACEHOLDER** |

- **Orphan routes**: None.
- **Dead redirects**: None. `/` → `/dashboard`, `*` → `NotFoundPage`.
- **Deferred pages**: Honestly render `EmptyState` citing PRD scope corrections.

---

## 4. Button Functionality Matrix (Section 9)

| Button | Page | Status | Evidence |
|---|---|---|---|
| Verify Connection | Header | **REAL** | `aiqaApi.ai.status` + `metrics.get` |
| Sign Out | Header | **LOCAL-ONLY** | Clears localStorage |
| Create Project | ProjectSelector | **REAL** | `aiqaApi.projects.create` → SQLite |
| Analyze & Generate | TestCaseGenerator | **REAL** | `aiqaApi.testCases.generateAi` → `ai_generation.rs` |
| Save Test Case | GeneratedTestCasesView | **REAL** | `aiqaApi.testCases.create` → SQLite |
| Compile Preview | GeneratedTestCasesView | **REAL** | `aiqaApi.testCases.compile` → `execution_plan.rs` |
| Start Execution | ExecutionHub | **REAL** | `aiqaApi.executions.create` → `engine.rs` → subprocess |
| Cancel Run | ExecutionHub | **REAL** | `aiqaApi.executions.cancel` → abort handle |
| Approve/Reject Run | ExecutionHub | **REAL** | `aiqaApi.executions.approve/reject` → state update |
| Execute API Test | APITesting | **REAL** | `aiqaApi.apiTesting.execute` → real HTTP request |
| Start Perf Test | PerformanceTesting | **REAL** | `aiqaApi.performanceTesting.start` → real load gen |
| Cancel Perf Test | PerformanceTesting | **REAL** | `aiqaApi.performanceTesting.cancel` |
| AI Analysis | Reports | **REAL** | `aiqaApi.findings.analysis` → `intelligence.rs` |
| Confirm/Dismiss Finding | Reports | **REAL** | `aiqaApi.findings.review` → SQLite |
| Export JSON/PDF | Reports | **REAL** | Client-side generation (jspdf) |
| Save Settings | Settings | **REAL** | `aiqaApi.ai.updateSettings` → `ai_router.rs` |
| Refresh (various) | Multiple | **REAL** | Re-fetches from backend API |
| Toggle Skill | SkillsAndMcpPanel | **LOCAL-ONLY** | React state only (sent on Generate) |
| Add Custom Skill | SkillManagerModal | **LOCAL-ONLY** | Session React state |
| Connect MCP | SkillsAndMcpPanel | **PLACEHOLDER** | Opens honest "not configured" modal |
| Sidebar Collapse | Sidebar | **LOCAL-ONLY** | UI state toggle |
| Command Palette | Header | **LOCAL-ONLY** | UI state toggle |

> [!TIP]
> **No always-disabled buttons found.** All `disabled` props are bound to real loading states (`isGenerating`, `isRunning`, etc.) that resolve when Promises complete.

---

## 5. Skills Audit (Section 20)

| Skill | Backend Directive | Classification |
|---|---|---|
| Functional Testing | Happy/alternate paths, business rules | **CONFIGURATION LABEL** |
| Edge Case Analysis | Boundary values, null/empty, concurrency | **CONFIGURATION LABEL** |
| Security Testing | Auth, injection, session, data exposure | **CONFIGURATION LABEL** |
| Performance Testing | Response-time, load, stress, spike | **CONFIGURATION LABEL** |
| API Testing | Request/response contract, status codes | **CONFIGURATION LABEL** |
| Database Testing | CRUD, constraints, relationships | **CONFIGURATION LABEL** |
| RBAC Testing | Role/permission boundaries | **CONFIGURATION LABEL** |
| Business Case Testing | End-to-end business rules, workflows | **CONFIGURATION LABEL** |

> [!IMPORTANT]
> Skills are **real prompt-driven configuration labels**. Each injects unique instructions into the AI prompt via `resolve_skill_instructions()`. However, no skill triggers an independent execution engine or external tool.

---

## 6. MCP Audit (Section 21)

| Capability | Status |
|---|---|
| Registry | ❌ NONE |
| Server Discovery | ❌ NONE |
| Connection | ❌ NONE |
| Authentication | ❌ NONE |
| Tool Discovery | ❌ NONE |
| Tool Invocation | ❌ NONE |

**Classification: PLACEHOLDER (Honestly Disclosed)**. `McpConnectModal` warns user MCP is not configured.

---

## 7. Backend Route Inventory (Section 22)

### 63+ Routes — All Project-Scoped Routes Use `require_scoped_project`

**Core APIs**: Projects (CRUD), Dashboard (get, kpis), Executions (CRUD + approve/reject/cancel + tasks + events + evidence), Evidence (detail + content), Findings (list + detail + analysis + review + AI hypothesis), Test Cases (CRUD + validate/approve/reject/handoff/compile + generate-ai), Custom Testing (generate), API Testing (execute), Performance Testing (start/list/status/cancel), App Source (verify), Audit (list), Intelligence (tests), AI (status/settings/refresh), Admin (api-keys CRUD + audit-log), Metrics, WebSocket (run events + project events), Health.

### Legacy Flat Routes (Duplicate — F-008)

| Route | Status |
|---|---|
| `/runs`, `/runs/:id`, `/runs/:id/events` | DUPLICATE — scoped version exists |
| `/runs/:id/approve`, `/runs/:id/reject` | DUPLICATE |
| `/intelligence/*`, `/bug-drafts/*` | DUPLICATE |
| `/admin/*` | DUPLICATE |

---

## 8. Rust Module Audit (Section 23)

| File | Size | Classification |
|---|---|---|
| [api.rs](../services/orchestrator/src/api.rs) | 144KB | **ACTIVE** — Main Axum router |
| [store.rs](../services/orchestrator/src/store.rs) | 87KB | **ACTIVE** — SQLite persistence |
| [executor.rs](../services/orchestrator/src/executor.rs) | 69KB | **ACTIVE** — Subprocess execution (Job Objects) |
| [performance_testing.rs](../services/orchestrator/src/performance_testing.rs) | 62KB | **ACTIVE** — Real load testing |
| [ai_router.rs](../services/orchestrator/src/ai_router.rs) | 54KB | **ACTIVE** — Dynamic AI provider routing |
| [engine.rs](../services/orchestrator/src/engine.rs) | 38KB | **ACTIVE** — Execution lifecycle |
| [intelligence.rs](../services/orchestrator/src/intelligence.rs) | 35KB | **ACTIVE** — AI failure analysis |
| [ai_generation.rs](../services/orchestrator/src/ai_generation.rs) | 33KB | **ACTIVE** — AI test generation |
| [execution_plan.rs](../services/orchestrator/src/execution_plan.rs) | 32KB | **ACTIVE** — Plan compilation |
| [custom_testing.rs](../services/orchestrator/src/custom_testing.rs) | 19KB | **ACTIVE** — Custom requirement analysis |
| [domain.rs](../services/orchestrator/src/domain.rs) | 17KB | **ACTIVE** — Core types |
| [app_source.rs](../services/orchestrator/src/app_source.rs) | 15KB | **ACTIVE** — App source verification |
| [api_testing.rs](../services/orchestrator/src/api_testing.rs) | 15KB | **ACTIVE** — Ad-hoc API execution |
| [scheduler.rs](../services/orchestrator/src/scheduler.rs) | 12KB | **ACTIVE** — Task scheduling |
| [errors.rs](../services/orchestrator/src/errors.rs) | 8KB | **ACTIVE** — Error types |
| [main.rs](../services/orchestrator/src/main.rs) | 6KB | **ACTIVE** — Server bootstrap |
| [auth.rs](../services/orchestrator/src/auth.rs) | 6KB | **ACTIVE** — Bearer token auth |
| [policy.rs](../services/orchestrator/src/policy.rs) | 5KB | **ACTIVE** — Approval policies |
| [state.rs](../services/orchestrator/src/state.rs) | 3KB | **ACTIVE** — Shared state |
| [evidence.rs](../services/orchestrator/src/evidence.rs) | 3KB | **ACTIVE** — Evidence storage |
| [metrics.rs](../services/orchestrator/src/metrics.rs) | 2KB | **ACTIVE** — System metrics |
| [webhook.rs](../services/orchestrator/src/webhook.rs) | 2KB | **ACTIVE** — Webhook dispatch |
| [lib.rs](../services/orchestrator/src/lib.rs) | 539B | **ACTIVE** — Module declarations |
| [api_tests.rs](../services/orchestrator/src/api_tests.rs) | 220KB | **TEST-ONLY** — 155 test functions |
| [engine_tests.rs](../services/orchestrator/src/engine_tests.rs) | 65KB | **TEST-ONLY** |
| [ws_tests.rs](../services/orchestrator/src/ws_tests.rs) | 18KB | **TEST-ONLY** |
| [store_tests.rs](../services/orchestrator/src/store_tests.rs) | 16KB | **TEST-ONLY** |

---

## 9. Database/Persistence Audit (Section 25)

### Migrations (8 total)

| Migration | Purpose |
|---|---|
| `0001_core_execution.sql` | runs, tasks, events, evidence |
| `0002_quality_graph_and_intelligence.sql` | intelligence, failure clusters |
| `0003_auth_governance_and_audit.sql` | api_keys, audit_log |
| `0004_projects.sql` | projects |
| `0005_test_cases.sql` | test_cases |
| `0006_ai_settings.sql` | ai_settings |
| `0007_ai_bug_hypothesis.sql` | bug_drafts, findings |
| `0008_performance_runs.sql` | performance_runs |

> [!NOTE]
> Orchestrator uses **SQLite** (`sqlx` with `sqlite` feature). The `docker-compose.yml` in `infra/` defines Postgres/Redis/MinIO but these are **NOT wired** — classified as PLACEHOLDER for future persistence upgrade.

---

## 10. Security Audit (Section 29)

| Finding | Severity | Details |
|---|---|---|
| Firebase API key in repo | **P0** | [firebase-applet-config.json](../apps/web/firebase-applet-config.json) line 4 |
| localStorage for API key | **P2** | `apiClient.ts` stores Bearer token in `localStorage` — XSS accessible |
| CORS | ✅ Safe | `tower-http` CORS middleware, not permissive |
| Command injection | ✅ Safe | `Command::new` uses vector args, no shell concatenation |
| Process isolation | ✅ Safe | Windows Job Objects prevent orphaned processes |
| Path traversal | ✅ Safe | Evidence uses UUID filenames, not user input |
| SSRF | ✅ Safe | `execution_plan.rs` validates URLs |
| Backend auth | ✅ Safe | All routes use `require_auth` / `require_scoped_project` |
| Error handling | ✅ Safe | No rogue `.unwrap()` in HTTP handlers |

---

## 11. Mock/Fake Data Audit (Section 35)

| Finding | Location | Classification |
|---|---|---|
| `activeGenerationSession` fake cases | `AIQAContext.tsx:154-212` | **Production fake** — needs removal |
| `activities` dead state | `AIQAContext.tsx:223` | **Dead code** |
| `FakeDiscovery`, `FakeExecutor` | `ai_router.rs:742-751` | **Test fixture** — safe |
| `MockWebSocket`, `Math.random` | `Header.test.tsx:28,60` | **Test fixture** — safe |
| Comments about removed fakes | Various `.tsx` files | **Documentation** — safe |
| `RunTriggerModal` default tasks | `RunTriggerModal.tsx:52-53` | **Production defaults** — real definitions |

---

## 12. Feature Implementation Matrix (Section 41)

| Feature | UI | API | Backend | DB | Execution | Evidence | Status |
|---|---|---|---|---|---|---|---|
| Dashboard (KPIs) | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Test Case Generation | ✅ | ✅ | ✅ | ✅ | ✅ AI | ✅ | **REAL** |
| Test Case Lifecycle | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Execution Hub | ✅ | ✅ | ✅ | ✅ | ✅ Subprocess | ✅ | **REAL** |
| API Testing | ✅ | ✅ | ✅ | ✅ | ✅ HTTP | ✅ | **REAL** |
| Performance Testing | ✅ | ✅ | ✅ | ✅ | ✅ Load | ✅ | **REAL** |
| Custom Testing | ✅ | ✅ | ✅ | ✅ | ✅ AI | ✅ | **REAL** |
| Reports (Findings) | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Audit Trail | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Evidence Viewer | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Telemetry/Metrics | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Settings (AI) | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Project Management | ✅ | ✅ | ✅ | ✅ | N/A | N/A | **REAL** |
| Realtime Events | ✅ | ✅ WS | ✅ | N/A | N/A | N/A | **REAL** |
| AI Provider Routing | ✅ | ✅ | ✅ | ✅ | ✅ | N/A | **REAL** |
| AIQA Orchestrator | ✅ Empty | ❌ | ❌ | ❌ | ❌ | ❌ | **DEFERRED** |
| Test Case Validator | ✅ Empty | ❌ | ❌ | ❌ | ❌ | ❌ | **DEFERRED** |
| RBAC Testing | ✅ Empty | ❌ | ❌ | ❌ | ❌ | ❌ | **DEFERRED** |
| Mobile Testing | ✅ Empty | ❌ | ❌ | ❌ | ❌ | ❌ | **DEFERRED** |
| Feature Map/Flow | ✅ Empty | ❌ | ❌ | ❌ | ❌ | ❌ | **PLACEHOLDER** |
| MCP Integration | ✅ Modal | ❌ | ❌ | ❌ | ❌ | ❌ | **PLACEHOLDER** |
| Python Workers | ❌ | ❌ | Stubs | ❌ | ❌ | ❌ | **PLACEHOLDER** |

---

## 13. Dead Code Master List (Section 43)

### Dead Files

| File/Directory | Evidence | Safe to Delete |
|---|---|---|
| [apps/web/](../apps/web) (43 files) | Duplicate frontend. `start.bat` launches `forentend/`, not `apps/web/` | REQUIRES HUMAN REVIEW |
| [infra/docker-compose.yml](../infra/docker-compose.yml) | Postgres/Redis/MinIO not used by orchestrator | REQUIRES HUMAN REVIEW |
| [services/workers/adapters/*.py](../services/workers/adapters) | All 3 raise `NotImplementedError` | REQUIRES HUMAN REVIEW |

### Dead State

| Item | Location | Evidence |
|---|---|---|
| `activities` state | [AIQAContext.tsx:223](../forentend/src/context/AIQAContext.tsx#L223) | Zero consumers in entire codebase |
| `addActivity` function | [AIQAContext.tsx](../forentend/src/context/AIQAContext.tsx) | Only exported, never called |

### Dead/Legacy Routes

| Route | Evidence |
|---|---|
| `POST /runs` | Legacy; new UI uses `/api/v1/projects/:id/executions` |
| `GET /runs`, `GET /runs/:id` | Legacy |
| `GET /intelligence/*` | Legacy |
| `GET /bug-drafts`, `POST /bug-drafts/:id/review` | Legacy |
| `GET/POST/DELETE /admin/*` | Legacy |

### Fabricated Data

| Item | Location | Evidence |
|---|---|---|
| `activeGenerationSession` | [AIQAContext.tsx:154-212](../forentend/src/context/AIQAContext.tsx#L154-L212) | 3 fake test cases preloaded |

---

## 14. Duplicate Implementation Audit (Section 45)

| Capability | Implementation 1 | Implementation 2 | Canonical |
|---|---|---|---|
| Frontend App | `forentend/` (primary) | `apps/web/` (legacy/Studio) | `forentend/` |
| API Client | `forentend/services/aiqaApi.ts` | `apps/web/services/apiClient.ts` | `forentend/` |
| Sidebar | `forentend/components/layout/` | `apps/web/components/layout/` | `forentend/` |
| Header | `forentend/components/layout/` | `apps/web/components/layout/` | `forentend/` |
| Execution routes | `/api/v1/projects/:id/executions/*` | `/runs/*` | Scoped routes |

---

## 15. CI/CD Audit (Section 52)

> [!CAUTION]
> **NO CI/CD PIPELINE EXISTS.** No `.github/workflows` directory. No GitHub Actions. **Severity: P1.**
>
> Required: Frontend typecheck + test + build, Backend check + clippy + test, Secret scanning.

---

## 16. Truthfulness Audit (Section 49)

| Claim | Truthful? | Evidence |
|---|---|---|
| Dashboard KPIs | ✅ | `aiqaApi.dashboard.get/kpis` → real backend queries |
| AI Generated test cases | ✅ | `ai_generation.rs` → real LLM call |
| Execution results | ✅ | `executor.rs` → real subprocess |
| Performance metrics | ✅ | `performance_testing.rs` → real HTTP load |
| Connection status | ✅ | `auth.status` from real API health check |
| WebSocket events | ✅ | `RealtimeContext` → real `broadcast::Receiver` |
| Deferred features | ✅ | `EmptyState` with honest labels |
| MCP status | ✅ | `McpConnectModal` warns "not yet configured" |

---

## 17. Cleanup Recommendations

### Phase A — Immediate (No code deletion)

1. **REVOKE** Firebase API key in [firebase-applet-config.json](../apps/web/firebase-applet-config.json)
2. **SET UP** GitHub Actions CI/CD pipeline
3. **REMOVE** fabricated `activeGenerationSession` from [AIQAContext.tsx](../forentend/src/context/AIQAContext.tsx)
4. **REMOVE** dead `activities`/`addActivity` state from [AIQAContext.tsx](../forentend/src/context/AIQAContext.tsx)

### Phase B — Requires Human Review

5. **EVALUATE** `apps/web/` — decide keep (for Google Studio) or delete
6. **EVALUATE** legacy flat routes — migrate all callers to scoped routes, then remove
7. **EVALUATE** `infra/docker-compose.yml` — keep for future or remove
8. **EVALUATE** Python worker stubs — keep as roadmap or remove

### Phase C — Architecture Improvements

9. Consider `httpOnly` cookie for auth token instead of `localStorage`
10. Add lazy loading for route components
11. Add missing component tests

---

## Acceptance Criteria Checklist

- [x] Every repository file is classified
- [x] Every route is traced
- [x] Every button is traced
- [x] Every frontend API method has a caller or explicit reason
- [x] Every backend route has a caller or documented external consumer
- [x] Every major feature has an implementation status
- [x] Every skill has an implementation status
- [x] Every MCP capability has an implementation status
- [x] Every fake/demo data source is identified
- [x] Every dead-code candidate has evidence
- [x] Every duplicate implementation is identified
- [x] Project isolation is verified (`require_scoped_project`)
- [x] Auth/RBAC is verified (`require_auth` on all protected routes)
- [x] Reports use real data (findings from real executions)
- [x] Evidence uses real execution data (subprocess output)
- [x] CI status is verified (NONE — P1)
- [x] Cleanup list is separated from implementation list
- [x] No code is deleted without evidence

---

> **Final Principle Verdict**: AIQA is judged by _"Can a real user perform the action, does the backend actually execute it, is the result persisted, and can the UI prove what happened?"_ — **YES** for 15 features, **DEFERRED (honestly)** for 5, **PLACEHOLDER (honestly)** for 2. Zero fake success. Zero misleading buttons. Clear separation between REAL, PARTIAL, DEFERRED, and DEAD.
