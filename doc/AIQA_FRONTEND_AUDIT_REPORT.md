# AIQA — Frontend Audit Report

**Mode:** READ-ONLY. No source file was created, modified, deleted, renamed or refactored.
**Date:** 17 August 2026
**Audited path:** `D:\AIQA\forentend` (see §0 — the brief specifies `D:\AIQA\frontend`, which does not exist)
**Sources of truth used:**
- `AIQA_API_CONTRACT_v1.0.md` (found in `C:\Users\Orange\Downloads`, not in the repo)
- `AIQA_ACCEPTANCE_MATRIX_v1.0.md` (same location)
- `AIQA_BACKEND_AUDIT_REPORT.md`, `AIQA_DEEP_AUDIT_REPORT.md` (in repo)
- Live read of `services/orchestrator` source for backend capability mapping

**Not found anywhere on this machine:** *AIQA Business Logic & Architecture v2.0*. Statements that would depend on it are marked as such rather than inferred.

**Artifacts produced by permitted commands:** `forentend/node_modules/` (dependency install) and `forentend/dist/` (build output). No tracked file was altered; `forentend/` is untracked in git in its entirety.

---

## 0. Preliminary discrepancies

Three facts must be established before the audit is readable.

| Stated | Actual |
|---|---|
| Frontend at `D:\AIQA\frontend` | Directory is **`D:\AIQA\forentend`** (misspelled). No `frontend/` exists. |
| One frontend | **Two exist.** `forentend/` (this audit, 11,444 LOC) and `apps/web/` (3,314 LOC), which is already wired to the Rust orchestrator. |
| Spec documents in repo | All four governing documents are **outside** the repository, in `Downloads`. Only audit reports are committed. |

The coexistence of two frontends is itself a finding — see §16, C4.

---

## 1. Executive Summary

**The frontend contains zero network calls.**

A repository-wide scan of all 11,444 lines returns **0 occurrences** of `fetch(`, `axios`, `XMLHttpRequest`, `new WebSocket`, `EventSource`, and `navigator.sendBeacon`. There is no HTTP client, no socket, no polling. Every value the user sees originates from TypeScript literals inside `src/context/AIQAContext.tsx` (1,201 lines) or from inline object construction in event handlers.

The file named `src/services/aiqaApi.ts` presents as a service layer — a singleton class, typed method signatures, an endpoint field — but **no method in it performs I/O**. `checkHealth()` returns a hardcoded `connected` object inside a `try` block that cannot throw. `triggerRun()` returns a fabricated run id. `executeEndpointTest()` returns a hardcoded `PASSED / 200 / 45ms`. Its configured endpoint is `https://api.aiqa.internal/v1`, a domain that does not resolve.

This is not a frontend with mock data in some places. It is a **self-contained simulation of the AIQA product**, with the visual and structural completeness of a finished application and no integration surface whatsoever.

The quality of what *is* there is genuinely high: 12 coherent pages covering nearly every functional area the PRD describes, a consistent design system, real empty-state components, correct canonical status vocabulary, TypeScript that compiles clean, and a production build that succeeds. As a UI specification it is valuable. As an integration candidate it is not ready, because there is nothing to integrate — the wiring does not exist to be corrected.

**FRONTEND READY FOR INTEGRATION: NO** (§21).

---

## 2. Frontend Technology Stack

| Concern | Implementation | Notes |
|---|---|---|
| Framework | React 19.0.1 | function components + hooks throughout |
| Language | TypeScript ~5.8.2 | `tsc --noEmit` passes with 0 errors |
| Build system | Vite 6.2.3 (resolved 6.4.3) | `@vitejs/plugin-react` |
| Package manager | **Ambiguous** — `bun.lock` present, no `package-lock.json`/`yarn.lock` | audited via `npm install --no-package-lock` |
| Styling | Tailwind CSS 4.1.14 via `@tailwindcss/vite` | utility classes inline |
| Icons | `lucide-react` 0.546.0 | |
| Charts | `recharts` 3.10.1 | used by dashboard visualisations |
| Animation | `motion` 12.23.24 | |
| Routing | **None** — no router dependency | `switch (activePage)` in `App.tsx` |
| State | React Context, single provider | `AIQAContext.tsx` |
| Data fetching | **None** | no client library, no calls |
| Testing | **None** | no framework, no `test` script, 0 test files |
| Dev server port | **3000** (`vite --port=3000 --host=0.0.0.0`) | |

### Declared but unused dependencies

`express`, `@types/express`, `tsx`, `dotenv`, `esbuild` are declared, but there is **no server file** in the project — the `clean` script references `server.js`, which does not exist. `@google/genai` is declared and **never imported** (0 occurrences).

The `@google/genai` presence is worth noting against API Contract §22, which states the frontend must not call model providers directly. It currently does not — but the dependency is staged for exactly that.

`--host=0.0.0.0` binds the dev server to all interfaces.

---

## 3. Complete Folder / Code Structure

```
forentend/                              11,444 LOC (TS/TSX/CSS/JSON)
├── index.html
├── package.json                        scripts: dev/build/preview/clean/lint
├── vite.config.ts          22          no proxy, no API config
├── tsconfig.json           26
├── bun.lock                            (no npm/yarn lockfile)
├── metadata.json           6           AI Studio applet metadata
├── .env.example                        GEMINI_API_KEY, APP_URL only
├── assets/.aistudio/                   AI Studio scaffold marker
└── src/
    ├── main.tsx            10          createRoot → <App/>
    ├── App.tsx             87          switch-based page dispatch
    ├── index.css           33
    ├── types.ts            541         domain types
    ├── context/
    │   └── AIQAContext.tsx 1,201       ALL application state, all literal
    ├── services/
    │   └── aiqaApi.ts      148         stub client, zero I/O
    └── components/
        ├── common/         (4)         EmptyState, MetricCard, SectionHeader, StatusBadge
        ├── layout/         (4)         Header, Sidebar, CommandPalette, RunTriggerModal
        ├── dashboard/      (3)         AIQualityCoreOrb, ProjectExecutionGraph, ProjectStatusDonut
        ├── test-generator/ (5)         ApplicationSource, DocumentUpload, GeneratedTestCasesView,
        │                               GenerationConfig, TraceabilityGraph
        └── pages/          (12)        one per navigation entry
```

**Distribution of concern:** 1,201 lines of state vs 148 lines of "service" vs ~9,400 lines of presentation. The inversion — more state literal than integration code by 8× — is the structural signature of the finding in §1.

---

## 4. Route / Page Inventory

**There is no router and there are no URLs.** Navigation is a `switch` on a context string; the address bar never changes, so no page is deep-linkable, bookmarkable, or reload-safe.

| # | Page id | Component | LOC | Sidebar label |
|---|---|---|---:|---|
| 1 | `dashboard` | `DashboardPage` | 477 | Dashboard |
| 2 | `generator` | `TestCaseGeneratorPage` | 669 | Test Case Generator `AI` |
| 3 | `orchestrator` | `AIQAOrchestratorPage` | 465 | AIQA Orchestrator `PLAN` |
| 4 | `validator` | `TestCaseValidatorPage` | 413 | Test Case Validator `GATE` |
| 5 | `execution` | `TestExecutionPage` | 402 | Test Execution Hub `LIVE` |
| 6 | `rbac` | `RBACTestingPage` | 369 | RBAC Testing |
| 7 | `api-testing` | `APITestingPage` | 428 | API Testing |
| 8 | `mobile-testing` | `MobileTestingPage` | 501 | Mobile Testing |
| 9 | `custom` | `CustomTestingPage` | 300 | Custom Testing |
| 10 | `feature-map` | `FeatureMapFlowPage` | 383 | Feature Map / Flow |
| 11 | `reports` | `ReportsPage` | 319 | Reports |
| 12 | `settings` | `SettingsPage` | 480 | Settings / AI Model |

Coverage against the contract's functional areas is broad — 12 pages map onto most of ACC-001…ACC-020. The gap is depth, not breadth.

---

## 5. Component Inventory

**28 components.** Every page consumes `useAIQA()`; **no page holds local literal datasets** (`localConst = 0` across all 12) — all fabricated data is centralised in the context, which is architecturally cleaner than scattering it.

| Group | Components | Role |
|---|---|---|
| common (4) | `EmptyState`, `MetricCard`, `SectionHeader`, `StatusBadge` | reusable primitives; `StatusBadge` (185 LOC) encodes the canonical status vocabulary |
| layout (4) | `Header`, `Sidebar`, `CommandPalette`, `RunTriggerModal` | shell; ⌘K palette and ⌘B sidebar toggle are real and functional |
| dashboard (3) | `AIQualityCoreOrb`, `ProjectExecutionGraph`, `ProjectStatusDonut` | recharts + SVG visualisations |
| test-generator (5) | `ApplicationSourceSection`, `DocumentUploadSection`, `GeneratedTestCasesView` (871), `GenerationConfigSection`, `TraceabilityGraph` | the deepest feature area |
| pages (12) | see §4 | |

`GeneratedTestCasesView` at 871 lines is the largest component and the most developed single feature surface in the app.

---

## 6. Feature-by-Feature Audit

Legend — **REAL**: works end to end against a backend · **PARTIAL**: real UI, no backend path · **MOCK/FAKE**: presents fabricated data as genuine · **PLACEHOLDER**: visible but inert · **MISSING**: absent.

| Feature | UI | Data source | Backend call | Status |
|---|---|---|---|---|
| Dashboard / KPIs | rich, charts, project selector | context literals | none | **MOCK/FAKE** |
| Project management | selector on Dashboard only | 1 literal project (`prj-aiqa`) | none | **MOCK/FAKE** |
| Test Case Generator | multi-step wizard, upload, config | literal `generated[]` at ctx:900 | none | **MOCK/FAKE** |
| Test Case Validator | validation states, review gate | context literals | none | **MOCK/FAKE** |
| AIQA Orchestrator | plan/routing view | context + `Math.random()` | none | **MOCK/FAKE** |
| Test Execution Hub | run list, task table, counters | context literals | none | **MOCK/FAKE** |
| Execution progress | live-looking counters | fabricated in `triggerNewRun` | none | **MOCK/FAKE** |
| Realtime events | `LIVE` badge in sidebar | — | **no socket exists** | **MISSING** |
| Evidence | referenced in types/UI | literals | none | **MOCK/FAKE** |
| Findings / RCA | present in validator/reports | literals | none | **MOCK/FAKE** |
| RBAC testing | matrix, scenarios, evaluate button | literal `rbacScenarios` | none | **MOCK/FAKE** |
| API testing | endpoint list, execute, response view | literal `apiTests` | none | **MOCK/FAKE** |
| API telemetry | latency/p95 surfaces | `Math.random()` | none | **MOCK/FAKE** |
| Mobile testing | device inventory, lock/unlock | literals + 4× `Math.random()` | none | **MOCK/FAKE** |
| Custom testing | objective builder | literal `customSuites` | none | **MOCK/FAKE** |
| Feature map / flow | flow graph | literals | none | **MOCK/FAKE** |
| Reports | list, generate, export UI | literals | none | **MOCK/FAKE** |
| Settings / AI model | full settings form | `defaultSettings` literal | none | **PARTIAL** (local state persists in-session only) |
| Audit / governance | **no dedicated page** | — | — | **MISSING** |
| Authentication | **no login, no token, no storage** | — | — | **MISSING** |
| Context ingestion | `DocumentUploadSection` UI exists | no upload target | none | **PLACEHOLDER** |

---

## 7. Real vs Mock vs Placeholder Analysis

### The evidence

**7.1 — `services/aiqaApi.ts`: a client that never calls anything.**

```ts
private endpoint: string = 'https://api.aiqa.internal/v1';   // non-resolving domain

public async checkHealth(): Promise<BackendConnectionState> {
  try {
    // Clean fallback contract when backend is in local standalone mode
    return { status: 'connected', daemonVersion: 'v2.4.0-controlplane', latencyMs: 14, ... };
  } catch { return { status: 'disconnected', ... }; }
}
```

The `try/catch` cannot throw — there is no operation inside it. Health is unconditionally `connected` with an invented daemon version and a fixed 14 ms latency. Remaining methods: `getActiveRuns() → []`, `getRunById() → null`, `triggerRun() → fabricated id`, `cancelRun() → {success:true}`, `executeEndpointTest() → PASSED/200/45ms`, `getSettings() → null`, `updateSettings() → true`.

**7.2 — `context/AIQAContext.tsx:164`: connection status is a literal.**

```ts
const [backendState, setBackendState] = useState<BackendConnectionState>({
  status: 'connected',
  endpoint: 'https://controlplane.aiqa.internal',
  daemonVersion: 'v2.4.2-enterprise',
  latencyMs: 18,
});
```

`Header.tsx:180` renders this as **"Daemon Online"** with the latency. The application therefore permanently asserts a healthy backend connection that has never been attempted. Note the two files disagree on both endpoint and version (`api.aiqa.internal` / `v2.4.0` vs `controlplane.aiqa.internal` / `v2.4.2`) — a tell that neither is real.

**7.3 — `triggerNewRun()`: fabricates a complete execution.**

```ts
const newRunId = `run-${Math.floor(1000 + Math.random() * 9000)}`;
const newRun: ExecutionRun = {
  status: 'RUNNING',
  policyApprovalState: 'APPROVED',        // frontend deciding policy — Contract §27 violation
  totalTests: 10,
  counts: { passed: 1, failed: 0, running: 3, queued: 6, ... },
  tests: [ { name: 'Smoke: Healthcheck ping to ingress router', status: 'PASSED', durationMs: 120 }, ... ]
};
```

Pressing "Run" invents a run id, invents ten tests, invents their names and durations, invents the counter split, and marks the run policy-**APPROVED** — a decision API Contract §27 assigns exclusively to the backend.

**7.4 — `executeApiTest()`: guaranteed success.**

```ts
setApiTests(... status: 'RUNNING' ...);
await new Promise((res) => setTimeout(res, 600));   // "Realistic API execution delay"
setApiTests(... status: 'PASSED', statusCode: 200,
            responseTimeMs: Math.floor(35 + Math.random() * 40) ...);
```

Every API test passes, always, after a cosmetic 600 ms delay, with a randomised response time. No request is issued.

### Repository-wide counts

| Signal | Count | Assessment |
|---|---:|---|
| `fetch` / `axios` / `XHR` / `WebSocket` / `EventSource` | **0** | no integration surface |
| `Math.random()` | 10 (6 files) | fabricated metrics/ids/telemetry |
| `setTimeout` | 17 | cosmetic delays simulating work |
| `localStorage` / `sessionStorage` | **0** | no session persistence |
| `TODO` / `FIXME` | 0 | nothing marked as incomplete |
| `EmptyState` usages | 5 pages | genuine empty-state handling |
| `catch` | 1 | effectively no error handling |

The zero `TODO` count matters: the code does not signal that it is unfinished. Everything reads as complete.

### Classification totals

| Class | Areas |
|---|---:|
| REAL | **0** |
| PARTIAL | 1 (Settings, session-local) |
| MOCK/FAKE | 17 |
| PLACEHOLDER | 1 (Context ingestion) |
| MISSING | 3 (Auth, Realtime, Audit) |

---

## 8. API Contract Mapping

Against `AIQA_API_CONTRACT_v1.0.md`.

### 8.1 Frontend calls that match the contract

**None.** The frontend issues no HTTP requests, so no endpoint is correctly integrated.

### 8.2 Frontend calls using incorrect endpoints

Not applicable in the usual sense — but the two configured base URLs are both invalid and neither matches the contract's `/api/v1/...` convention:

| Location | Configured base | Contract requires |
|---|---|---|
| `aiqaApi.ts:25` | `https://api.aiqa.internal/v1` | `/api/v1/...` (§2.1) |
| `AIQAContext.tsx:167` | `https://controlplane.aiqa.internal` | — |

### 8.3 Contract endpoints with no frontend consumer

Every endpoint in the contract is unconsumed. Summarised by section:

| Contract § | Area | Endpoints | Consumers |
|---|---|---:|---:|
| 5 | Projects | 5 | 0 |
| 6 | Context ingestion | 2 | 0 |
| 7 | Quality graph / requirements | 4 | 0 |
| 8 | Test generation | 4 | 0 |
| 9 | Validation | 2 | 0 |
| 10 | Suites / plans | 3 | 0 |
| 11 | Orchestrator plans | 2 | 0 |
| 12 | Execution | 5 | 0 |
| 14 | Realtime events | 1 (WS) | 0 |
| 15 | Evidence | 2 | 0 |
| 16 | Findings / RCA | 3 | 0 |
| 17 | RBAC | 6 | 0 |
| 18 | API testing | 4 | 0 |
| 19 | Mobile | 5 | 0 |
| 20 | Custom tests | 1 | 0 |
| 21 | Reports | 4 | 0 |
| 22 | Model gateway | 3 | 0 |
| 23 | Dashboard / KPIs | 2 | 0 |
| 24 | Settings | 2 | 0 |
| 25 | Audit | 1 | 0 |
| | **Total** | **~61** | **0** |

### 8.4 Frontend features requiring endpoints absent from the contract

None identified. The UI's feature set is a subset of the contract's scope — a genuine positive: the AI Studio build did not invent product surface beyond specification.

### 8.5 Contract conformance on non-endpoint rules

| Contract rule | Status |
|---|---|
| §13 canonical statuses | **PASS (mostly)** — `QUEUED/RUNNING/PASSED/FAILED/BLOCKED/SKIPPED/CANCELLED/NEEDS_REVIEW` all used correctly. **One deviation:** the contract specifies `INFRASTRUCTURE_ERROR`; the frontend uses `infraErrors`/`infraFailuresCount` field naming and the backend emits `INFRA_ERROR`. Three spellings, none aligned. |
| §2.5 structured errors | **FAIL** — no error contract handling; 1 `catch` in the codebase |
| §2.3 project scoping | **FAIL** — selector exists but no data is scoped by it (§12) |
| §28.2 "never hard-code fake production data" | **FAIL** — the entire data layer |
| §28.10 "API calls behind a typed service layer" | **STRUCTURALLY PASS** — the seam exists and is typed; it is simply empty. This is the single most valuable property of the codebase for remediation. |
| §27 frontend MUST NOT own execution state / approval | **FAIL** — `policyApprovalState: 'APPROVED'` set client-side |

---

## 9. Backend Compatibility Mapping

Against the actual Rust orchestrator (`services/orchestrator/src/api.rs`, verified in the backend audit).

The backend exposes **16 HTTP routes + 1 WebSocket**, flat and **not project-scoped**:

```
/health · /runs · /runs/:id · /runs/:id/events · /runs/:id/approve · /runs/:id/reject
/runs/:id/evidence · /evidence/:id/content · /intelligence/tests
/intelligence/failure-clusters · /bug-drafts · /bug-drafts/:id/review · /metrics
/admin/api-keys ×3 · /admin/audit-log · WS /runs/:id/ws
```

| Frontend feature | Contract endpoint | Backend today | Verdict |
|---|---|---|---|
| Execution hub | `POST/GET .../executions` | `POST/GET /runs` | **shape mismatch** — no project scope, different path |
| Run tasks | `.../executions/{run_id}/tasks` | included in `GET /runs/:id` | partial |
| Realtime | `WS .../projects/{id}/events` | `WS /runs/:id/ws?key=` | **different granularity** (per-run vs per-project) |
| Evidence | `.../executions/{run_id}/evidence` | `/runs/:id/evidence` ✓ | exists, unscoped |
| Findings / RCA | `.../findings` | `/intelligence/failure-clusters`, `/bug-drafts` | conceptually present, different names |
| Audit | `.../audit` | `/admin/audit-log` ✓ | exists, admin-only, unscoped |
| Dashboard KPIs | `.../dashboard`, `.../kpis` | `/metrics` (Prometheus **text**) | format mismatch |
| Projects | 5 endpoints | **none** | missing |
| Context ingestion | 2 | **none** | missing |
| Quality graph | 4 | tables only, no route | missing |
| Test generation | 4 | **none** | missing |
| Validation | 2 | **none** | missing |
| Suites / plans | 3 | **none** | missing |
| Orchestrator plans | 2 | **none** | missing |
| RBAC | 6 | API keys + 3 fixed roles, no user model | missing |
| API testing | 4 | **none** | missing |
| Mobile | 5 | Maestro executor, no device API | missing |
| Custom tests | 1 | **none** | missing |
| Reports | 4 | **none** | missing |
| Model gateway | 3 | **none** | missing |
| Settings | 2 | **none** | missing |

**Roughly 7 of ~61 contract endpoints have a backend analogue, none at the contract's path or scope.** Even a fully-wired frontend could satisfy only a small fraction of its screens today.

---

## 10. Realtime / WebSocket Audit

**No realtime implementation exists.**

- `new WebSocket` — 0 occurrences
- `EventSource` — 0 occurrences
- No polling loop (`setInterval` appears once, and it is not a data fetch)
- No event handler, reducer, or sequence-tracking logic anywhere

The sidebar renders a `LIVE` badge on Test Execution Hub (`Sidebar.tsx:41`, `badgeType: 'live'`) with no live data behind it.

Against Contract §14 and ACC-008, all of the following are absent: event envelope handling, `sequence` ordering, idempotent processing, reconnect, and post-reconnect reconciliation against authoritative REST state. ACC-008's verification procedure (start → disconnect → reconnect → reconcile) cannot begin.

The backend does provide a working authenticated socket with sequence numbers and durable replay (`WS /runs/:id/ws?key=`, verified live in the backend audit). It is per-run rather than the contract's per-project, and has no consumer here.

---

## 11. Authentication / RBAC Audit

### Authentication — MISSING

| Check | Result |
|---|---|
| Login/sign-in UI | none |
| Auth provider / hook | none |
| Token acquisition | none |
| Token storage (`localStorage`/`sessionStorage`/cookie) | **0 occurrences** |
| `Authorization` header construction | none |
| 401 / expired-session handling | none |
| Logout | none |

All 57 `auth`/48 `Auth`/29 `token`/1 `Bearer` textual matches are **display strings inside fabricated test-case data** — e.g. `'POST /api/v2/auth/oauth-token (Bearer exchange)'` (`FeatureMapFlowPage.tsx:100`), `'Validate OAuth2 Authorization Code Exchange with PKCE'`. None is an implementation.

Contract §28.5 (handle expired/invalid authentication) and §28.6 (handle permission denied) are unimplementable in the current structure.

### RBAC — feature-simulated, not enforced

`RBACTestingPage` is a **product feature** (testing someone else's RBAC), not access control over AIQA itself. It is driven by the literal `rbacScenarios` array (`AIQAContext.tsx:537`). `evaluateRBACMatrix()` mutates local state.

There is no client-side permission gating of AIQA's own UI, which is *correct* per Contract §26.8 (frontend checks are UX only) — but there is also no backend authorization to defer to, because no request is ever made. ACC-011 cannot be exercised.

**Positive:** because the frontend performs no authorization, it does not contain the common anti-pattern of client-side permission logic masquerading as security.

---

## 12. State Management Audit

**Single React Context**, `AIQAProvider`, exposing ~45 values. No Redux/Zustand/TanStack Query. No persistence — a page refresh resets everything.

### Composition of `AIQAContext.tsx` (1,201 lines)

| Lines | Content |
|---|---|
| 108–152 | `defaultSettings` literal |
| 176–309 | `projects[]` — literal |
| 314–460 | `runs[]` — literal (~147 lines of fabricated runs) |
| 466–536 | `activeGenerationSession` — literal |
| 537–602 | `rbacScenarios[]` — literal |
| 606–675 | `apiTests[]` — literal |
| 679–706 | `customSuites[]` — literal |
| 707–752 | `activities[]` — literal |
| 753–~899 | `qualityIntelligence` — literal |
| ~900 | `generated: GeneratedTestCase[]` — literal generation output |
| 1121–1134 | `useEffect` — keyboard shortcuts only (**not** a simulation timer) |

Pre-selected ids are hardcoded: `selectedProjectId = 'prj-aiqa'`, `activeRunId = 'run-8902'`, `activeRBACScenarioId = 'rbac-sc-1'`, `activeApiTestId = 'api-t-1'`.

### Project scoping is cosmetic

`DashboardPage` renders a project `<select>` bound to `setSelectedProjectId`, and `selectedProject` is read for **display only** (name, status, environment, metrics). A repository-wide search for data filtered by project id (`.projectId`, `projectId ===`) returns **0 matches**. Switching project changes labels, not data — Contract §2.3 and ACC-001 unmet.

### Assessment

The context is well-organised and correctly typed; its failing is exclusively that it is a fixture rather than a cache. Notably, **no page holds its own literal dataset** — centralisation means the fake data has exactly one replacement point, which materially lowers remediation cost.

---

## 13. Error / Loading / Empty State Audit

Against Contract §28.4 (loading, success, empty, error, `NEEDS_REVIEW`).

| State | Implementation | Verdict |
|---|---|---|
| Loading | 3 textual matches; no `isLoading` flag anywhere (0 occurrences); `setTimeout` used for cosmetic delay | **INADEQUATE** — no request means no genuine loading state |
| Empty | `EmptyState` component, used in 5 pages (APITesting, CustomTesting, RBAC, Reports, TestExecution) | **REAL** — the strongest state handling present |
| Error | 19 `error` / 28 `Error` matches, but **1 `catch` in 11,444 lines**; no error boundary; no API error surface | **MISSING** |
| Unauthorized / 401 | none | **MISSING** |
| Permission denied / 403 | none | **MISSING** |
| Offline / disconnected | `backendState.status` supports `'disconnected'` but is never set to it | **UNREACHABLE** |
| `NEEDS_REVIEW` | 4 matches; rendered by `StatusBadge` | **REAL** (vocabulary), untested against backend |

`EmptyState` and `NEEDS_REVIEW` support show the author understood the requirement. The gap is that error/loading/offline states cannot exist without a request to fail.

---

## 14. Acceptance Matrix Mapping

Per ACC status definitions. **Frontend** column only; Backend/API/Integration reflect current reality.

| ID | Area | Backend | API | Frontend | Integration | Test | Final |
|---|---|---|---|---|---|---|---|
| ACC-001 | Project Management | NOT_STARTED | NOT_STARTED | **REJECTED** (UI-only isolation, §32 breach) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-002 | Context Ingestion | NOT_STARTED | NOT_STARTED | **IN_PROGRESS** (upload UI, no target) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-003 | Quality Graph | IN_PROGRESS (tables only) | NOT_STARTED | **IN_PROGRESS** (`TraceabilityGraph`) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-004 | Test Case Generator | NOT_STARTED | NOT_STARTED | **REJECTED** (fabricated output) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-005 | Test Case Validator | NOT_STARTED | NOT_STARTED | **REJECTED** (fabricated verdicts) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-006 | AIQA Orchestrator | IN_PROGRESS (DAG scheduler) | NOT_STARTED | **REJECTED** (fabricated routing) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-007 | Test Execution Hub | **IMPLEMENTED** (real executors, DAG, continue-on-failure) | IN_PROGRESS (unscoped paths) | **REJECTED** (fabricated runs) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-008 | Realtime Events | **IMPLEMENTED** (WS + sequence + replay) | IN_PROGRESS | **NOT_STARTED** | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-009 | Evidence | **IMPLEMENTED** (capture + retrieval verified) | IN_PROGRESS | **REJECTED** (literal evidence) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-010 | Failure / RCA | IMPLEMENTED (clusters, drafts, confidence) | IN_PROGRESS | **REJECTED** | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-011 | RBAC Testing | NOT_STARTED | NOT_STARTED | **REJECTED** (simulated evaluation) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-012 | API Testing | NOT_STARTED | NOT_STARTED | **REJECTED** (always PASSED) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-013 | API Latency / Telemetry | NOT_STARTED | NOT_STARTED | **REJECTED** (`Math.random()`) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-014 | Mobile Testing | IN_PROGRESS (Maestro executor) | NOT_STARTED | **REJECTED** (fabricated devices) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-015 | Custom Testing | NOT_STARTED | NOT_STARTED | **REJECTED** | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-016 | Reports | NOT_STARTED | NOT_STARTED | **REJECTED** (static reports) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-017 | Dashboard / KPIs | IN_PROGRESS (`/metrics`) | IN_PROGRESS | **REJECTED** (hardcoded metrics) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-018 | AI Model Gateway | NOT_STARTED | NOT_STARTED | **REJECTED** (fake model status) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-019 | Settings / Config | NOT_STARTED | NOT_STARTED | **IN_PROGRESS** (form real, session-local) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-020 | Audit / Governance | **IMPLEMENTED** (audit log verified) | IN_PROGRESS | **NOT_STARTED** (no page) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-021 | Security / Isolation | IMPLEMENTED (auth, roles, project scope) | IN_PROGRESS | **REJECTED** (no auth at all) | NOT_STARTED | NOT_STARTED | NOT_STARTED |
| ACC-022 | Recovery / Fault Tolerance | **REJECTED** (see backend audit C1/C2 — silent data corruption) | IN_PROGRESS | NOT_STARTED | NOT_STARTED | NOT_STARTED | NOT_STARTED |

**`REJECTED` is used deliberately** per §3: implementations that present fabricated data as real do not meet acceptance criteria, and Acceptance Matrix §32 lists every one of these patterns as unacceptable in production paths.

### E2E scenarios

| Scenario | Status |
|---|---|
| E2E-001 Project → Report | **BLOCKED** — no step is wired |
| E2E-002 RBAC flow | **BLOCKED** |
| E2E-003 API flow | **BLOCKED** |
| E2E-004 Security flow | **BLOCKED** |
| E2E-005 Realtime recovery | **BLOCKED** — no socket to disconnect |
| E2E-006 Model failure | **BLOCKED** — no gateway |
| E2E-007 Mobile flow | **BLOCKED** |

### Frontend Completion Gate (§29)

| Requirement | Met |
|---|---|
| all required screens exist | **YES** |
| all required states exist | NO |
| real API integration exists | **NO** |
| fake production data removed | **NO** |
| mock success paths removed | **NO** |
| API errors handled | NO |
| permission errors handled | NO |
| loading/empty/error states exist | PARTIAL (empty only) |
| realtime events consumed | **NO** |
| reconnect reconciliation | **NO** |
| canonical statuses supported | **YES** (one spelling deviation) |
| no authoritative business rules in frontend | **NO** (`policyApprovalState`) |
| accessibility/UX verified | not assessed — no tests |

**2 of 13 gate items met.**

---

## 15. Build / Test Results

All commands run without modifying source.

| Command | Result | Detail |
|---|---|---|
| `npm install --no-package-lock` | **PASS** | `node_modules/` was absent beforehand |
| `tsc --noEmit` (= `npm run lint`) | **PASS** | 0 errors |
| `vite build` | **PASS** | 2,690 modules → `dist/` |
| Frontend tests | **N/A — none exist** | no framework, no `test` script, 0 test files |

Build output:

```
dist/index.html                 0.77 kB │ gzip:   0.37 kB
dist/assets/index-*.css        93.80 kB │ gzip:  13.41 kB
dist/assets/index-*.js      1,028.33 kB │ gzip: 285.04 kB
✓ built in 12.82s
(!) Some chunks are larger than 500 kB after minification
```

**Two notes.**

1. A first typecheck attempt via `npx tsc` installed the unrelated `tsc@2.0.4` squatter package and produced a misleading result. Re-run through `./node_modules/.bin/tsc` after a real install; the PASS above is the corrected run.
2. A **1.03 MB single bundle with no code-splitting** is a genuine performance issue independent of integration (§18).

**The build passing is not evidence of readiness.** It compiles precisely because it has no external dependencies to fail against.

---

## 16. Critical Issues

**C1 — No integration surface exists.**
Zero `fetch`/`axios`/`WebSocket`/`EventSource` across 11,444 lines. The frontend cannot communicate with any backend. This is not a bug to fix but a layer to build. Breaches Contract §28.1 and Acceptance Matrix §32.

**C2 — Fabricated data presented as authoritative.**
`backendState` hardcoded to `connected` with an invented daemon version renders as "Daemon Online". `triggerNewRun()` invents runs, tests, durations and counter splits. `executeApiTest()` always returns PASSED/200. Acceptance Matrix §32 names each of these as unacceptable.

**C3 — Frontend owns decisions the contract assigns to the backend.**
`triggerNewRun()` sets `policyApprovalState: 'APPROVED'` client-side. Contract §27 places approval, execution state and pass/fail determination exclusively with the backend.

**C4 — Two competing frontends in one repository.**
`forentend/` (11,444 LOC, zero integration) and `apps/web/` (3,314 LOC, fully wired to the orchestrator with Bearer auth, typed error taxonomy, real WebSocket consumption and six honest UI states). Both are React+Vite+Tailwind; both claim to be the AIQA UI. No decision record exists. Contract §33 warns against silently divergent implementations.

**C5 — No authentication of any kind.**
No login, no token, no storage, no `Authorization` header, no 401 handling. The backend requires `Authorization: Bearer <key>` on 18 of 19 routes, so **every request this frontend might make would fail 401** the moment one is added. ACC-021 unmeetable.

---

## 17. High Priority Issues

**H1 — No routing.** `switch(activePage)` with no URL. Nothing is deep-linkable, bookmarkable or reload-safe; browser back/forward do not work. Adding a router later is invasive.

**H2 — Project scoping is cosmetic.** Selector exists; **0** places filter by project id. ACC-001 requires switching projects to change downstream data, and requires the frontend not to rely on UI-only isolation.

**H3 — No realtime consumption.** A `LIVE` badge with no socket. ACC-008's disconnect/reconnect/reconcile procedure cannot be started.

**H4 — Error handling effectively absent.** One `catch` in 11,444 lines; no error boundary; no 401/403/5xx surface; no structured error parsing per Contract §2.5.

**H5 — Zero tests, no framework.** Contract §30 requires the frontend to verify every API has a consumer, every status renders, error and permission states work, and reconnect reconciles. None is verifiable.

**H6 — No audit/governance UI.** ACC-020 requires it; the backend already implements and exposes `/admin/audit-log` (verified working). This is one of the few screens buildable against the backend *today*.

**H7 — Status vocabulary drift.** Contract says `INFRASTRUCTURE_ERROR`; backend emits `INFRA_ERROR`; frontend uses `infraErrors`/`infraFailuresCount`. Three spellings for one canonical status; Contract §13 forbids alternates.

---

## 18. Medium / Low Priority Issues

**Medium**

- **M1** — 1.03 MB unsplit bundle; Vite emits a size warning. No lazy loading despite 12 route-like pages.
- **M2** — Dead dependencies: `express`, `@types/express`, `tsx`, `dotenv`, `esbuild` with no server file; `clean` script references a non-existent `server.js`.
- **M3** — `@google/genai` declared, never imported. If used later for direct model calls it would breach Contract §22.
- **M4** — Ambiguous package manager: `bun.lock` only. CI reproducibility is undefined.
- **M5** — Settings persist only in session; no backend, no `localStorage`. Contract §24 expects a settings API.
- **M6** — `--host=0.0.0.0` in the dev script exposes the dev server on all interfaces.
- **M7** — Two mutually inconsistent fake endpoints (`api.aiqa.internal` vs `controlplane.aiqa.internal`) and versions (`v2.4.0` vs `v2.4.2`).

**Low**

- **L1** — Directory misspelled `forentend`; the brief and any tooling expecting `frontend` will miss it.
- **L2** — `package.json` name is `react-example`, the AI Studio scaffold default.
- **L3** — AI Studio scaffold residue: `assets/.aistudio/`, `metadata.json`, HMR comments about agent edits.
- **L4** — `.env.example` documents only `GEMINI_API_KEY` and `APP_URL`; no backend URL variable exists.
- **L5** — Hardcoded pre-selected ids (`prj-aiqa`, `run-8902`, `rbac-sc-1`, `api-t-1`) will 404 against a real backend.
- **L6** — `notificationCount` initialised to `3` with nothing behind it.

---

## 19. Missing Backend Dependencies

Endpoints the frontend's existing screens require, which **do not exist** in the orchestrator today. Ordered by number of screens unblocked.

| # | Capability | Contract § | Unblocks | Backend today |
|---|---|---|---|---|
| 1 | Projects CRUD + scoping | 5 | **all 12 pages** | none |
| 2 | Dashboard / KPIs (JSON) | 23 | Dashboard | `/metrics` as Prometheus **text** |
| 3 | Test case CRUD | 8.3–8.4 | Generator, Validator, Execution | none |
| 4 | Test generation (async job) | 8.1–8.2 | Generator | none — no LLM integration at all |
| 5 | Test validation | 9 | Validator | none |
| 6 | Project-scoped event stream | 14 | Execution, Orchestrator, RBAC, Mobile | per-**run** WS only |
| 7 | Findings / RCA | 16 | Reports, Validator | `/intelligence/*`, `/bug-drafts` (different shape) |
| 8 | RBAC roles/users/matrix | 17 | RBAC page | API keys + 3 fixed roles, no user model |
| 9 | API testing + telemetry | 18 | API Testing | none |
| 10 | Mobile devices + locking | 19 | Mobile Testing | Maestro executor, no device API |
| 11 | Reports + PDF/Excel export | 21 | Reports | none |
| 12 | Model gateway health/telemetry | 22 | Settings, Orchestrator | none |
| 13 | Settings persistence | 24 | Settings | none |
| 14 | Context ingestion | 6 | Generator upload | none |
| 15 | Quality graph query | 7 | Feature Map | tables in migration 0002, no route |
| 16 | Suites / plans | 10 | Execution | none |
| 17 | Custom test objectives | 20 | Custom Testing | none |

**Immediately buildable against today's backend:** Audit log (`/admin/audit-log`), Evidence (`/runs/:id/evidence`, `/evidence/:id/content`), run listing/detail, per-run realtime, findings in the backend's native shape.

Also note the backend audit's **C1/C2**: under load the orchestrator silently corrupts persisted run state (passing runs stored as FAILED, tasks stuck RUNNING). **Integrating against it before those are fixed would surface corrupted data in the UI** and make frontend correctness untestable.

---

## 20. Recommended Implementation Order

Sequenced so each step is verifiable before the next.

**Phase 0 — Decisions (blocking; no code)**
1. Choose one frontend: `forentend/` or `apps/web/`. Retire the other. (C4)
2. Reconcile the status spelling across contract, backend and frontend. (H7)
3. Confirm whether the backend adopts contract paths (`/api/v1/projects/{id}/...`) or the contract is amended to the backend's flat shape. Everything downstream depends on this.

**Phase 1 — Backend prerequisites**
4. Fix backend C1/C2 (WAL + busy timeout, stop swallowing persistence errors, correct recovery state derivation) — otherwise the UI displays corrupted data.
5. Implement Projects API + project scoping. Unblocks all 12 pages.
6. Add a JSON dashboard/KPI endpoint.

**Phase 2 — Frontend foundation**
7. Build a real HTTP client: base URL config, `Authorization: Bearer`, structured error parsing, typed errors. `aiqaApi.ts` is the correct seam and can be filled in place.
8. Add authentication: key/token entry, storage, 401/403 handling. (C5)
9. Add routing so pages are addressable. (H1)
10. Add loading/error/unauthorized/offline states alongside the existing `EmptyState`. (H4)

**Phase 3 — Replace fabrication, screen by screen**
Order chosen by backend readiness:
11. Audit log — backend ready today.
12. Evidence viewer — backend ready today.
13. Execution hub against `/runs` — closest existing analogue.
14. Realtime, once the project-scoped stream exists; implement idempotent sequence merging and reconnect reconciliation. (H3, ACC-008)
15. Dashboard against the new KPI endpoint.
16. Remaining screens as their backends land.

**Phase 4 — Verification**
17. Add a test framework and cover the client, error taxonomy and event merging. (H5)
18. Run Contract §30's frontend checklist and the E2E scenarios.
19. Code-split the bundle. (M1)

**Principle:** do not remove fabricated data before its replacement endpoint exists. Deleting it first yields a blank product with no diagnostic value; replacing screen by screen keeps the app demonstrable throughout.

---

## 21. Final Frontend Readiness Score

| Dimension | Weight | Score | Basis |
|---|---:|---:|---|
| UI completeness / design | 15% | **9/10** | 12 coherent pages, consistent system, real components |
| Code quality / structure | 10% | **7/10** | clean typing, centralised state, compiles clean; no tests |
| Contract conformance (structure) | 10% | **5/10** | typed service seam and canonical statuses present; scoping and errors absent |
| API integration | 25% | **0/10** | zero network calls |
| Realtime | 10% | **0/10** | no socket |
| Authentication / RBAC | 10% | **0/10** | none |
| State correctness | 10% | **2/10** | organised but entirely fixture data |
| Error / loading / empty states | 5% | **3/10** | empty only |
| Testing | 5% | **0/10** | none |

**Weighted score: 2.9 / 10**

### FRONTEND READY FOR INTEGRATION: **NO**

**Why.**

Readiness would mean the frontend can be pointed at a backend and exchange data. This frontend has no mechanism to do so. There is no HTTP client, no socket, no authentication, and no request anywhere in 11,444 lines. Integration is not a matter of correcting endpoints or reshaping payloads — the entire client layer has yet to be written. `services/aiqaApi.ts` looks like that layer, which makes the gap easy to miss: it has the class, the singleton, the typed signatures and the endpoint field, and none of the behaviour.

Every screen currently satisfies itself from literals. Pressing "Run" invents an execution; pressing "Execute" on an API test always returns 200; the header permanently reports a healthy daemon that was never contacted. Under Acceptance Matrix §32 each of these is explicitly disqualifying, which is why §14 marks 15 areas `REJECTED` rather than `IN_PROGRESS`.

Two structural blockers sit above the code. Two complete frontends exist in this repository, and the other one — `apps/web/` — is already integrated with the orchestrator, authenticated, and consuming the real WebSocket. Choosing between them is a prerequisite to any work here. And the backend itself supplies roughly 7 of ~61 contract endpoints, none at the contract's path or project scope, so even a perfectly wired frontend could not populate most of these screens today.

**What is genuinely valuable here.** The information architecture is strong and maps cleanly onto the PRD's functional areas. Canonical status vocabulary is used correctly. `EmptyState` and `NEEDS_REVIEW` show the author read the requirements. Fabricated data is centralised in one file rather than scattered across 28 components, giving it a single replacement point. And critically, the service seam exists in the right place — Contract §28.10 is satisfied structurally, so the client can be filled in without restructuring the UI.

Treated as an **executable UI specification**, this is a good artifact and a sound basis for the real frontend. Treated as a deliverable frontend, it is a simulation, and the honest status is **NOT READY**.

**PARTIAL would be the wrong verdict.** Partial implies some integration works. None does.

---

*Read-only audit. No source file was created, modified, deleted or refactored. Permitted commands produced `forentend/node_modules/` and `forentend/dist/`; no tracked file changed. Every count and command output quoted here was executed against this repository. One superseded measurement is disclosed in §15.*
