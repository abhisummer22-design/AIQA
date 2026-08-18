# AIQA Frontend Pre-Implementation Audit & Source of Truth

**Mode:** READ-ONLY inspection. No source code modified.

## 1. Repository Frontend Inventory

An inspection of the repository root (`D:\AIQA`) reveals two distinct frontend implementations:

1. **`forentend/`** (~11,444 LOC): A React 19 + Vite + Tailwind implementation. Contains 12 pages visually mapping to the PRD but **zero** backend integration. Uses simulated/fabricated data extensively via a monolith Context state (`AIQAContext.tsx`).
2. **`apps/web/`** (~3,314 LOC): A React 19 + Vite + Tailwind implementation. Contains 7 pages and is **fully wired** to the actual Rust orchestrator (`apiClient.ts`, `orchestrator.ts`) using the backend's existing endpoints, including `Authorization: Bearer` and a WebSocket with sequence replay.

### Frameworks & Build Tools
Both implementations share an identical stack: React 19.0.1, TypeScript ~5.8.2, Vite 6.2.3, and Tailwind CSS v4.

### Entry Points
- `forentend/src/main.tsx` -> `<App />` -> switch-based navigation with no URLs.
- `apps/web/src/main.tsx` -> `<App />` -> real React structure with views.

### Build/Development Commands
- `forentend/`: `npm run dev` (Vite on port 3000, 0.0.0.0), `npm run build`, `npm run lint` (`tsc --noEmit`).
- `apps/web/`: `npm run dev` (Vite proxying `/api` to port 8080), `npm run build`, `npm run lint` (`tsc --noEmit`).

---

## 2. Frontend Comparison

| Dimension | `forentend/` | `apps/web/` |
| --- | --- | --- |
| **Completeness (Visual)** | 12 mock pages | 7 functional pages |
| **UI Quality** | High (acts as a UI spec) | Functional & unverified for deep design |
| **Architecture Quality** | Monolithic State, No Routing | Typed service layer, Proper Routing/Hooks |
| **Backend Integration Readiness** | 0% (Zero network calls) | 100% (wired to available backend routes) |
| **Realtime Capability** | Fake `LIVE` badge | Real `openRunEventSocket` WebSocket |
| **Authentication** | Missing | Implemented (`Bearer` token in `localStorage`) |
| **RBAC** | Simulated matrix UI | Real `admin/api-keys` usage |
| **Project Isolation** | Cosmetic UI drop-down | Passed as `project_id` in API where supported |
| **Build Health** | PASS (`tsc --noEmit` & build) | PASS (`tsc --noEmit` & build) |
| **Testing** | None | None |

### AUDIT DOCUMENT SAYS vs CURRENT REPOSITORY ACTUALLY SHOWS
- **Audit Document Says:** "The frontend contains zero network calls." (Referring strictly to `forentend/`).
- **Current Repository Actually Shows:** `apps/web/` exists alongside it and has a fully fleshed out `src/services/apiClient.ts` that makes robust network calls with structured typed error handling matching the actual Rust orchestrator. 

---

## 3. API Contract Compatibility

Against `AIQA_API_CONTRACT_v1.0.md` (~61 project-scoped endpoints under `/api/v1/...`):

- **`forentend/`**: 0% compatible. Its mocked API (`aiqaApi.ts`) is hardcoded to `https://api.aiqa.internal/v1` and makes no fetch requests.
- **`apps/web/`**: Connects via `apiClient.ts` to `/api` proxying the Rust orchestrator. The Rust orchestrator itself (and thus `apps/web/`) currently exposes 16 flat, unscoped routes (`/runs`, `/admin/api-keys`, `/metrics`, etc.) rather than the contract's 61 project-scoped endpoints. 

**Identified Contract Gaps in the Backend (which `apps/web/` correctly reflects):**
- **Missing Endpoints:** Project CRUD, Context Ingestion, Test Generation, Validation, Suites/Plans, API Testing, Mobile Testing, Custom Tests, Reports, Model Gateway.
- **Incorrect Paths:** Uses flat routes (`/runs`) instead of contract's `/api/v1/projects/{id}/executions`.
- **Project-Scoping Gaps:** Backend endpoints are missing project isolation (`/runs/:id` instead of scoping).
- **Authentication:** Works as per backend implementation, missing specific Contract session-management/401 handling details.

---

## 4. UI Feature Coverage (vs PRD Intended Structure)

| Feature | `forentend/` Status | `apps/web/` Status |
| --- | --- | --- |
| Dashboard | MOCK | PARTIAL (Prometheus `metrics` parser) |
| Projects | MOCK | MISSING |
| Requirements | MOCK | MISSING |
| Test Generation | MOCK | MISSING |
| Test Validation | MOCK | MISSING |
| Test Suites/Plans | MOCK | MISSING |
| Execution Hub | MOCK | EXISTS (`RunsView`, `RunDetailView`) |
| RBAC | MOCK | EXISTS (`SettingsView`, API keys) |
| API Testing | MOCK | MISSING |
| Security | MISSING | MISSING |
| Performance | MISSING | MISSING |
| Edge/Business Logic | MISSING | MISSING |
| Evidence | MOCK | EXISTS (via `/runs/:id/evidence`) |
| Findings / RCA | MOCK | EXISTS (`BugDraftsView`, `IntelligenceView`) |
| Reports | MOCK | MISSING |
| Model Gateway/Settings | PARTIAL (Local) | MISSING |
| Audit / Governance | MISSING | EXISTS (`AuditLogView`) |

---

## 5. Duplicate Code Findings

Two entirely separate frontends exist implementing the same core stack (React 19, Tailwind 4, Vite 6). `forentend/` has built highly advanced UX views but backed them entirely with `Math.random()` and literal arrays. `apps/web/` has built the necessary foundation (authentication, typed API client, WebSockets) and wired the exact screens the backend supports today.

---

## 6. Risks

1. **Blindly migrating `forentend/` code:** Bringing over UI from `forentend/` into `apps/web/` risks accidentally carrying over fake mock-data literal injection and client-side authoritative state management (e.g. `policyApprovalState = 'APPROVED'`).
2. **Backend/Contract Drift:** `apps/web/` is built against the *actual* backend, which currently violates the API Contract (flat routes vs project-scoped routes). The UI must follow the backend reality or wait for backend refactoring.

---

## 7. What Should NOT Be Changed

- Do **not** modify the Rust orchestrator or backend services. They are the source of truth for available endpoints.
- Do **not** modify `apps/web/src/services/orchestrator.ts`. It maps exactly to the live backend.
- Do **not** attempt to wire `forentend/` from scratch. The architectural foundation (state management, routing) is too weak.

---

## 8. Recommended UI Implementation Phases

1. **Establish Source of Truth:** Formally declare `apps/web/` as the single frontend repository.
2. **UI Porting:** Systematically copy the visual `EmptyState`, layout shells, and CSS components from `forentend/` into `apps/web/` without copying its `AIQAContext.tsx` fake data layer.
3. **Backend Alignment:** As the Rust backend implements the remaining 40+ API Contract endpoints (Projects, Test Generation, Mobile), extend `apps/web/src/services/orchestrator.ts` and port the corresponding UI views from `forentend/`, binding them strictly to real fetch responses.
4. **Delete `forentend/`:** Once the UI assets have been absorbed by `apps/web/`, safely delete the mock repository.

---

## FRONTEND SOURCE OF TRUTH RECOMMENDATION

**RECOMMENDED FRONTEND DIRECTORY:** `apps/web/`

**CONFIDENCE:** HIGH

**REASON:** `apps/web/` is an actual application. It has a real service layer (`apiClient.ts`), implements the requisite `Authorization: Bearer` security, natively consumes the complex event-sourced WebSocket stream for realtime data, and handles network errors and routing. `forentend/` is essentially a high-fidelity Figma prototype built in React—it lacks routing, networking, authentication, and error handling. The correct path forward is to use `apps/web/` as the host and lift the beautiful visual components from `forentend/` into it as the backend APIs become available.
