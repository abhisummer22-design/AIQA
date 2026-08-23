# AIQA Frontend Contract Readiness Matrix

> **⚠ SUPERSEDED — 22 August 2026.** `apps/web/` is confirmed **not** the shipping frontend. Every unit
> from FE-1 through FE-26, the central AI Model Auto-Discovery/Smart-Routing work, and the 22 August
> Security Remediation work all landed in `forentend/` instead — `apps/web/` has had no real commit
> since 18 August (`556c5cc`) and was again explicitly excluded from that same week's CORS default
> allow-list for being unmaintained. The "Frontend Host: `apps/web/`" line below, and every CONNECTED/
> PARTIAL/MISSING_BACKEND classification that follows, describes `apps/web/` as it stood on 17 August and
> is **historical**, not current readiness guidance — none of it has been re-derived against `forentend/`.
> See `AIQA_DOCUMENT_INDEX.md` for the full corpus status and `AIQA_PRODUCT_DECISIONS_RECORD.md` for the
> decision trail. `apps/web/` itself has **not** been archived or deleted — no product decision to do so
> has been made.

**Document Version:** 1.0  
**Date:** 17 August 2026  
**Reference API Contract:** `AIQA_API_CONTRACT_v1.0.md`  
**Reference Acceptance Matrix:** `AIQA_ACCEPTANCE_MATRIX_v1.0.md`  
**Frontend Host:** `apps/web/`

---

## 1. Status Definitions

- **CONNECTED:** Feature in `apps/web/` is wired to an active, real backend endpoint that handles real I/O.
- **PARTIAL:** Feature has a working backend path (e.g. flat route or admin API key) but contract-defined project-scoped or session endpoints are not yet fully implemented on the backend.
- **MISSING_BACKEND:** UI structure, types, and error states exist in `apps/web/`, but the Rust orchestrator provides no HTTP endpoint.
- **MISMATCH:** Active backend route functions, but its URL structure or parameters deviate from `AIQA_API_CONTRACT_v1.0.md` (e.g., flat route `/runs` vs projected `/api/v1/projects/{id}/executions`).
- **NOT_REQUIRED_YET:** Staged capability deferred to post-MVP iteration.

---

## 2. Master Feature & Acceptance Mapping

| Acceptance ID | Feature Area | Frontend Route / View | Frontend Service Layer | API Contract Endpoint | Current Status | Backend Dependency | Verification Method |
|---|---|---|---|---|---|---|---|
| **ACC-001** | **Project Management** | `ProjectsView.tsx` (`projects`) | `contract.ts` (Staged Interface) | `GET /api/v1/projects`<br>`POST /api/v1/projects`<br>`GET /api/v1/projects/{id}` | **MISSING_BACKEND** | Projects table, CRUD handlers, project context middleware in Axum router | UI renders `BACKEND API REQUIRED` notification detailing Contract §5 |
| **ACC-002** | **Context Ingestion** | `TestGeneratorView.tsx` (`test-generator`) | `contract.ts` (Staged Interface) | `POST /api/v1/projects/{id}/context`<br>`GET /api/v1/projects/{id}/context` | **MISSING_BACKEND** | Multi-part upload handler, document chunking & embedding worker | UI displays drag-and-drop ingestion placeholder with schema details |
| **ACC-003** | **Quality Graph** | `QualityGraphView.tsx` (`quality-graph`) | `contract.ts` (Staged Interface) | `GET /api/v1/projects/{id}/quality-graph`<br>`POST .../requirements` | **MISSING_BACKEND** | HTTP query router for Migration 0002 relational tables & adjacency matrix | UI displays interactive topological map container with Contract §7 schemas |
| **ACC-004** | **Test Case Generator** | `TestGeneratorView.tsx` (`test-generator`) | `contract.ts` (Staged Interface) | `POST /api/v1/projects/{id}/test-generation`<br>`GET .../jobs/{id}` | **MISSING_BACKEND** | LLM model gateway integration & async job worker | UI provides multi-category configuration wizard with Contract §8 payload |
| **ACC-005** | **Test Case Validator** | `TestValidatorView.tsx` (`test-validator`) | `contract.ts` (Staged Interface) | `POST /api/v1/projects/{id}/test-validation` | **MISSING_BACKEND** | Semantic validation rules engine (traceability, duplicate risk check) | UI displays validation gate review cards and benchmark criteria |
| **ACC-006** | **AIQA Orchestrator Planner** | `OrchestratorPlanView.tsx` (`orchestrator-plan`) | `contract.ts` (Staged Interface) | `POST /api/v1/projects/{id}/orchestrator/plans` | **MISSING_BACKEND** | DAG objective compiler & agent task routing service | UI displays DAG routing topology and capability assignment cards |
| **ACC-007** | **Test Execution Hub** | `RunsView.tsx`<br>`RunDetailView.tsx`<br>`RunTriggerModal.tsx` | `orchestrator.ts::listRuns`<br>`createRun`<br>`getRun`<br>`approveRun`<br>`rejectRun` | `GET /runs`<br>`POST /runs`<br>`POST /runs/{id}/approve`<br>`POST /runs/{id}/reject` | **CONNECTED** *(MISMATCH on path)* | Rust orchestrator running on port 8080 (currently flat routes) | `createRun` dispatches valid `RunPlan`; approvals and task states update live |
| **ACC-008** | **Realtime Events** | `RunDetailView.tsx` | `orchestrator.ts::openRunEventSocket`<br>`getRunEvents` | `WS /runs/{id}/ws?key=`<br>`GET /runs/{id}/events?since=` | **CONNECTED** *(MISMATCH on path)* | WebSocket broadcast stream in `api.rs` + SQLite event journal | Live WebSocket streaming, sequence-based deduplication & reconnect replay |
| **ACC-009** | **Evidence Capture** | `RunDetailView.tsx`<br>`EvidenceViewer.tsx` | `orchestrator.ts::listRunEvidence`<br>`getEvidenceContent` | `GET /runs/{id}/evidence`<br>`GET /evidence/{id}/content` | **CONNECTED** | Executor artifact storage and `store::insert_evidence` | Real logs/traces downloaded as Blob/Text and rendered in modal viewer |
| **ACC-010** | **Failure / RCA Intelligence** | `IntelligenceView.tsx` | `orchestrator.ts::listTestIntelligence`<br>`listFailureClusters` | `GET /intelligence/tests`<br>`GET /intelligence/failure-clusters` | **CONNECTED** | Intelligence analysis pipeline in `intelligence.rs` | Unscored metrics render as "—", failure signatures & hypotheses display |
| **ACC-011** | **RBAC Testing** | `RBACTestingView.tsx` (`rbac`) | `contract.ts` (Staged Interface) | `GET /api/v1/projects/{id}/rbac/matrix`<br>`POST .../rbac/test-generation` | **MISSING_BACKEND** | Target application role extraction & permission evaluation worker | UI renders matrix grid structure with Contract §17 specification |
| **ACC-012** | **API Testing** | `APITestingView.tsx` (`api-testing`) | `contract.ts` (Staged Interface) | `GET /api/v1/projects/{id}/api/specifications`<br>`POST .../api/executions` | **MISSING_BACKEND** | OpenAPI spec catalogue & Schemathesis/Newman management routes | UI displays API test harness and contract schema details |
| **ACC-013** | **API Latency / Telemetry** | `APITestingView.tsx` | `contract.ts` (Staged Interface) | `GET /api/v1/projects/{id}/api/telemetry` | **MISSING_BACKEND** | Latency collector & schema drift detector | UI renders p50/p95/p99 telemetry tiles with Contract §18 schemas |
| **ACC-014** | **Mobile Testing** | `MobileTestingView.tsx` (`mobile-testing`) | `contract.ts` (Staged Interface) | `GET /api/v1/projects/{id}/mobile/devices`<br>`POST .../devices/{id}/lock` | **MISSING_BACKEND** | Device farm inventory & mutex locking service for Maestro | UI displays connected device farm and lock state indicators |
| **ACC-015** | **Custom Testing** | `CustomTestingView.tsx` (`custom-testing`) | `contract.ts` (Staged Interface) | `POST /api/v1/projects/{id}/custom-tests` | **MISSING_BACKEND** | Ad-hoc objective parser & module execution binder | UI displays custom objective builder with Contract §20 schemas |
| **ACC-016** | **Reports & Exports** | `ReportsView.tsx` (`reports`) | `contract.ts` (Staged Interface) | `GET /api/v1/projects/{id}/reports`<br>`POST .../reports`<br>`GET .../export` | **MISSING_BACKEND** | Report compilation engine & PDF/Excel artifact generator | UI displays report archive and export format selectors |
| **ACC-017** | **Dashboard / KPIs** | `DashboardView.tsx` (`dashboard`) | `orchestrator.ts::getMetrics`<br>`listRuns` | `GET /metrics`<br>`GET /runs` | **CONNECTED** *(PARTIAL on project KPIs)* | Prometheus `/metrics` exporter in `metrics.rs` | Live Prometheus counter parser; zero invented numbers |
| **ACC-018** | **AI Model Gateway** | `ModelGatewayView.tsx` (`model-gateway`) | `contract.ts` (Staged Interface) | `GET /api/v1/ai/models/health`<br>`GET .../telemetry` | **MISSING_BACKEND** | Centralized backend LLM router & provider fallback engine | UI displays model routing monitor and Contract §22 notice |
| **ACC-019** | **Settings & Security** | `SettingsView.tsx` (`settings`) | `orchestrator.ts::listApiKeys`<br>`createApiKey`<br>`revokeApiKey` | `GET /admin/api-keys`<br>`POST /admin/api-keys`<br>`DELETE /admin/api-keys/{id}` | **CONNECTED** | API key SHA-256 store in `store.rs` and `auth.rs` | Keys minted, copy-once modal shown, revocation tested |
| **ACC-020** | **Audit & Governance** | `AuditLogView.tsx` (`audit-log`) | `orchestrator.ts::getAuditLog` | `GET /admin/audit-log` | **CONNECTED** | Immutable audit log in `store.rs` (`insert_audit`) | Server-recorded audit trail rendered with limit and text search |
| **ACC-021** | **Security / Isolation** | `apiClient.ts`<br>`SettingsView.tsx` | `apiClient.ts::rawRequest` | `Authorization: Bearer <key>` | **CONNECTED** | Server-side role check (`require_role`) in `auth.rs` | 401/403 states rendered faithfully; Bearer header attached |
| **ACC-022** | **Fault Tolerance & Reconnect** | `RunDetailView.tsx` | `orchestrator.ts::openRunEventSocket`<br>`getRunEvents` | `WS /runs/{id}/ws`<br>`GET /runs/{id}/events?since=` | **CONNECTED** | Monotonic sequence assignment in `engine.rs` | Disconnect simulation verifies catchup replay from last sequence |

---

## 3. Route & Path Reconciliation Summary

### Active Flat Endpoints (Current Backend Reality)
The following 16 HTTP routes and 1 WebSocket route are actively connected:
1. `GET /health` → `checkHealth()`
2. `GET /runs?limit=` → `listRuns()`
3. `POST /runs?project_id=` → `createRun()`
4. `GET /runs/:id` → `getRun()`
5. `GET /runs/:id/events?since=` → `getRunEvents()`
6. `POST /runs/:id/approve` → `approveRun()`
7. `POST /runs/:id/reject` → `rejectRun()`
8. `GET /runs/:id/evidence` → `listRunEvidence()`
9. `GET /evidence/:id/content` → `getEvidenceContent()`
10. `GET /intelligence/tests` → `listTestIntelligence()`
11. `GET /intelligence/failure-clusters` → `listFailureClusters()`
12. `GET /bug-drafts` → `listBugDrafts()`
13. `POST /bug-drafts/:id/review` → `reviewBugDraft()`
14. `GET /metrics` → `getMetrics()`
15. `GET /admin/api-keys` → `listApiKeys()`
16. `POST /admin/api-keys` → `createApiKey()`
17. `DELETE /admin/api-keys/:id` → `revokeApiKey()`
18. `GET /admin/audit-log?limit=` → `getAuditLog()`
19. `WS /runs/:id/ws?key=` → `openRunEventSocket()`

### Target Contract Endpoints (Required in Backend Phase 2)
The backend must implement the following project-scoped routes defined in `AIQA_API_CONTRACT_v1.0.md`:
- `GET /api/v1/projects`, `POST /api/v1/projects` (§5)
- `POST /api/v1/projects/{id}/context` (§6)
- `GET /api/v1/projects/{id}/quality-graph` (§7)
- `POST /api/v1/projects/{id}/test-generation` (§8)
- `POST /api/v1/projects/{id}/test-validation` (§9)
- `GET /api/v1/projects/{id}/test-suites`, `POST .../test-plans` (§10)
- `POST /api/v1/projects/{id}/orchestrator/plans` (§11)
- `GET/POST /api/v1/projects/{id}/executions` (§12)
- `WS /ws/v1/projects/{id}/executions/{id}` (§13)
- `GET /api/v1/projects/{id}/rbac/matrix` (§17)
- `GET /api/v1/projects/{id}/api/specifications`, `.../telemetry` (§18)
- `GET /api/v1/projects/{id}/mobile/devices` (§19)
- `POST /api/v1/projects/{id}/custom-tests` (§20)
- `GET/POST /api/v1/projects/{id}/reports` (§21)
- `GET /api/v1/ai/models/health`, `.../telemetry` (§22)
- `GET /api/v1/projects/{id}/dashboard`, `.../kpis` (§23)
- `GET /api/v1/projects/{id}/audit` (§25)
