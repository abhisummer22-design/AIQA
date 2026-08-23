# AIQA Frontend Phase 2 Integration Readiness Report

> **⚠ SUPERSEDED — 22 August 2026.** `apps/web/` is confirmed **not** the shipping frontend. Every unit
> from FE-1 through FE-26, the central AI Model Auto-Discovery/Smart-Routing work, and the 22 August
> Security Remediation work all landed in `forentend/` instead — `apps/web/` has had no real commit
> since 18 August (`556c5cc`) and was again explicitly excluded from that same week's CORS default
> allow-list for being unmaintained. This report's "Phase 2 prepares `apps/web/`..." framing and its
> endpoint-mapping table below describe `apps/web/` as it stood on 17 August and are **historical**, not
> current guidance. See `AIQA_DOCUMENT_INDEX.md` for the full corpus status and
> `AIQA_PRODUCT_DECISIONS_RECORD.md` for the decision trail. `apps/web/` itself has **not** been archived
> or deleted — no product decision to do so has been made.

**Date:** 17 August 2026  
**Agent:** AIQA Frontend Integration Readiness Agent  
**Contract Version Verified:** `AIQA_API_CONTRACT_v1.0.md` (v1.0)  
**Acceptance Matrix Verified:** `AIQA_ACCEPTANCE_MATRIX_v1.0.md` (v1.0)  
**Status:** **FRONTEND PHASE 2: PASS**

---

## 1. Executive Summary

Phase 2 Integration Readiness prepares `apps/web/` to interface with the upcoming backend capabilities without altering existing backend routes or fabricating mock data. The frontend data contracts across all 18 functional workspaces are now strictly structured around `AIQA_API_CONTRACT_v1.0.md`.

---

## 2. Endpoint Mapping & Current Integration Status

Every endpoint across the 25 contract sections has been audited and classified:

| Contract Section | API Endpoint Path | HTTP Method | Frontend Service Layer | Frontend Consuming View | Integration Status | Notes / Backend Action |
|---|---|---|---|---|---|---|
| **§4 Auth** | `/api/v1/auth/login` | `POST` | `contract.ts` / `apiClient.ts` | `SettingsView.tsx` | **PARTIAL** | Active backend uses `/admin/api-keys` (API key based Bearer tokens). Session user endpoints pending. |
| **§5 Projects** | `/api/v1/projects` | `GET`, `POST` | `contract.ts` (Staged) | `ProjectsView.tsx` | **MISSING_BACKEND** | Requires backend project CRUD endpoints. |
| **§5 Projects** | `/api/v1/projects/{id}` | `GET`, `PATCH`, `DELETE` | `contract.ts` (Staged) | `ProjectsView.tsx` | **MISSING_BACKEND** | Requires project lookup and isolation middleware. |
| **§6 Context** | `/api/v1/projects/{id}/context` | `GET`, `POST` | `contract.ts` (Staged) | `TestGeneratorView.tsx` | **MISSING_BACKEND** | Requires document upload & chunking pipeline. |
| **§7 Quality Graph** | `/api/v1/projects/{id}/quality-graph` | `GET` | `contract.ts` (Staged) | `QualityGraphView.tsx` | **MISSING_BACKEND** | DB tables exist in Migration 0002; HTTP route missing. |
| **§7 Requirements** | `/api/v1/projects/{id}/requirements` | `GET`, `POST` | `contract.ts` (Staged) | `QualityGraphView.tsx` | **MISSING_BACKEND** | Relational CRUD missing. |
| **§8 Generator** | `/api/v1/projects/{id}/test-generation` | `POST` | `contract.ts` (Staged) | `TestGeneratorView.tsx` | **MISSING_BACKEND** | Requires async LLM test generator worker. |
| **§9 Validator** | `/api/v1/projects/{id}/test-validation` | `POST` | `contract.ts` (Staged) | `TestValidatorView.tsx` | **MISSING_BACKEND** | Requires semantic validation gate rules engine. |
| **§10 Suites** | `/api/v1/projects/{id}/test-suites` | `GET`, `POST` | `contract.ts` (Staged) | `TestSuitesView.tsx` | **MISSING_BACKEND** | Requires test suite persistence catalogue. |
| **§10 Plans** | `/api/v1/projects/{id}/test-plans` | `POST` | `contract.ts` (Staged) | `TestSuitesView.tsx` | **MISSING_BACKEND** | Requires execution plan definition store. |
| **§11 Orchestrator** | `/api/v1/projects/{id}/orchestrator/plans` | `POST` | `contract.ts` (Staged) | `OrchestratorPlanView.tsx` | **MISSING_BACKEND** | Requires DAG capability router & agent planner. |
| **§12 Executions** | `/runs` | `GET`, `POST` | `orchestrator.ts::listRuns`, `createRun` | `RunsView.tsx`, `DashboardView.tsx` | **CONNECTED** *(MISMATCH)* | Flat route `/runs` active. Target is `/api/v1/projects/{id}/executions`. |
| **§12 Executions** | `/runs/{id}` | `GET` | `orchestrator.ts::getRun` | `RunDetailView.tsx` | **CONNECTED** | Detail view with real task states and durations. |
| **§12 Governance** | `/runs/{id}/approve` | `POST` | `orchestrator.ts::approveRun` | `RunsView.tsx`, `RunDetailView.tsx` | **CONNECTED** | Policy engine approval gate wired. |
| **§12 Governance** | `/runs/{id}/reject` | `POST` | `orchestrator.ts::rejectRun` | `RunsView.tsx` | **CONNECTED** | Rejection with reason payload wired. |
| **§13 Realtime** | `/runs/{id}/ws?key=` | `WS` | `orchestrator.ts::openRunEventSocket` | `RunDetailView.tsx` | **CONNECTED** *(MISMATCH)* | WebSocket active on flat route. Replays durable event log. |
| **§13 Replay** | `/runs/{id}/events?since=` | `GET` | `orchestrator.ts::getRunEvents` | `RunDetailView.tsx` | **CONNECTED** | Reconnection replay with monotonic sequence numbers. |
| **§14 Evidence** | `/runs/{id}/evidence` | `GET` | `orchestrator.ts::listRunEvidence` | `RunDetailView.tsx` | **CONNECTED** | Returns artifact metadata list. |
| **§14 Evidence** | `/evidence/{id}/content` | `GET` | `orchestrator.ts::getEvidenceContent` | `EvidenceViewer.tsx` | **CONNECTED** | Streams raw bytes (logs/traces/blobs). |
| **§15 Intelligence** | `/intelligence/failure-clusters` | `GET` | `orchestrator.ts::listFailureClusters` | `IntelligenceView.tsx` | **CONNECTED** | Returns cluster signatures & root-cause hypotheses. |
| **§15 Intelligence** | `/intelligence/tests` | `GET` | `orchestrator.ts::listTestIntelligence` | `IntelligenceView.tsx` | **CONNECTED** | Returns test risk, value, and flakiness scores. |
| **§16 Bug Drafts** | `/bug-drafts` | `GET` | `orchestrator.ts::listBugDrafts` | `BugDraftsView.tsx` | **CONNECTED** | Lists AI-generated defect candidates. |
| **§16 Review Gate** | `/bug-drafts/{id}/review` | `POST` | `orchestrator.ts::reviewBugDraft` | `BugDraftsView.tsx` | **CONNECTED** | Human confirmation/dismissal review gate. |
| **§17 RBAC** | `/api/v1/projects/{id}/rbac/matrix` | `GET`, `POST` | `contract.ts` (Staged) | `RBACTestingView.tsx` | **MISSING_BACKEND** | Requires target app role/permission evaluator. |
| **§18 API Testing** | `/api/v1/projects/{id}/api/specifications` | `GET` | `contract.ts` (Staged) | `APITestingView.tsx` | **MISSING_BACKEND** | Requires OpenAPI catalogue & telemetry collector. |
| **§19 Mobile** | `/api/v1/projects/{id}/mobile/devices` | `GET`, `POST` | `contract.ts` (Staged) | `MobileTestingView.tsx` | **MISSING_BACKEND** | Requires device farm inventory & lock service. |
| **§20 Custom** | `/api/v1/projects/{id}/custom-tests` | `POST` | `contract.ts` (Staged) | `CustomTestingView.tsx` | **MISSING_BACKEND** | Requires ad-hoc objective synthesizer. |
| **§21 Reports** | `/api/v1/projects/{id}/reports` | `GET`, `POST` | `contract.ts` (Staged) | `ReportsView.tsx` | **MISSING_BACKEND** | Requires report compiler & PDF/Excel exporter. |
| **§22 Model Gateway**| `/api/v1/ai/models/health` | `GET` | `contract.ts` (Staged) | `ModelGatewayView.tsx` | **MISSING_BACKEND** | Requires centralized backend LLM router. |
| **§23 Dashboard** | `/metrics` | `GET` | `orchestrator.ts::getMetrics` | `DashboardView.tsx` | **CONNECTED** *(PARTIAL)* | Real Prometheus text parser connected. Project KPIs pending. |
| **§24 Settings** | `/admin/api-keys` | `GET`, `POST`, `DELETE` | `orchestrator.ts::listApiKeys`... | `SettingsView.tsx` | **CONNECTED** | Key administration & single-view plaintext token. |
| **§25 Audit** | `/admin/audit-log` | `GET` | `orchestrator.ts::getAuditLog` | `AuditLogView.tsx` | **CONNECTED** | Immutable audit trail viewer wired. |

---

## 3. Contract Alignment & Mismatch Analysis

### Route Path Mismatch
- **Contract Expectation:** All project-scoped capabilities reside under `/api/v1/projects/{project_id}/...` (e.g. `/api/v1/projects/{id}/executions`).
- **Current Backend Reality:** The Rust orchestrator serves flat routes (`/runs`, `/bug-drafts`, `/intelligence/tests`, `/admin/audit-log`).
- **Contract Rule §31:** No contract change was made. The frontend maintains backward compatibility with flat routes while embedding project identifiers (`project_id`) in payloads and queries.
- **Backend Change Required:** In Backend Phase 2, the Rust orchestrator must introduce the `/api/v1/projects/...` routing hierarchy while providing aliases for flat routes during migration.

---

## 4. Frontend-Only Enhancements Made

1. **Contract Types (`apps/web/src/types/contract.ts`):** Created full TypeScript interfaces for all 12 staged areas directly derived from `AIQA_API_CONTRACT_v1.0.md` schemas.
2. **Error Taxonomy Standardization (`apps/web/src/services/apiClient.ts`):**
   - Added support for HTTP 422 (`validation_error`), HTTP 429 (`rate_limited`), HTTP 503 (`service_unavailable`), and gateway timeouts.
   - Updated `isRetryable` logic to cleanly guide UI retry actions.
3. **Acceptance Matrix Creation (`AIQA_FRONTEND_CONTRACT_READINESS_MATRIX.md`):** Complete mapping of every acceptance item (`ACC-001` through `ACC-022`) to routes, services, endpoints, and verification mechanisms.

---

## 5. Architectural Boundaries Verified

- **Real Data Boundary (PRD §27):** UI views remain purely presentational. All I/O flows through `services/apiClient.ts` and `services/orchestrator.ts`.
- **Project/Tenant Boundary (PRD §26):** Project scoping is explicit in types and requests; client-side filtering is never treated as authoritative security.
- **Realtime Stream & Replay (PRD §13, SRD §18):** Sequence-based frame merging and replay catchup (`?since=<last_sequence>`) are preserved without mock event generation.
- **Zero Mock Policy (Audit Rule):** Zero occurrences of `Math.random()`, `AIQAContext`, or fabricated status flags exist in `apps/web/`.

---

## 6. Build & Test Verification Results

```
> tsc --noEmit
Exit code: 0 (0 TypeScript errors)

> npm run lint
Exit code: 0 (0 Lint errors)

> vite build
✓ 2298 modules transformed.
dist/index.html                   1.02 kB │ gzip:   0.57 kB
dist/assets/index-DiVf2yry.css   56.28 kB │ gzip:   9.69 kB
dist/assets/index-B_vvklE8.js   742.52 kB │ gzip: 211.57 kB
Exit code: 0 (Build succeeded in 5.30s)
```

---

## 7. Remaining Backend Dependencies

The frontend in `apps/web/` is ready for immediate integration as soon as the backend exposes:
1. `GET/POST /api/v1/projects` (Project CRUD & workspace isolation)
2. `POST /api/v1/projects/{id}/context` (PRD/OpenAPI ingestion)
3. `GET /api/v1/projects/{id}/quality-graph` (Traceability topology API)
4. `POST /api/v1/projects/{id}/test-generation` (Asynchronous LLM test generator)
5. `POST /api/v1/projects/{id}/test-validation` (Semantic validation gate)
6. `GET/POST /api/v1/projects/{id}/test-suites` (Test suite catalogue)
7. `GET /api/v1/ai/models/health` (Central Model Gateway telemetry)

---

## Final Status

**FRONTEND PHASE 2: PASS**
