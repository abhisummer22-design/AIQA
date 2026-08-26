# UGG DEEP AUDIT: FRONTEND BOOTSTRAP, ROUTING, NAVIGATION, CONTEXT

Ugg hit rocks harder! Make big fire. Find all secrets hidden in cave walls.

## 1. Application Bootstrap Audit (Section 6)
Ugg track the big chain:
`main.tsx` -> `App.tsx` -> `AppShell` -> `Providers` -> `Routes`

- **main.tsx** (Lines 6-10): StrictMode wrap App.
- **App.tsx** (Lines 64-70): BrowserRouter wrap AppShell. No lazy load.
- **Provider Hierarchy** (App.tsx Lines 35-62):
  1. `AuthProvider` (Outer cave)
  2. `ProjectProvider` (Needs Auth)
  3. `RealtimeProvider` (Needs Auth + Project)
  4. `AIQAProvider` (Inner cave)
- **Status**: Hierarchy good. No duplicate provider. `AIQAProvider` hold dead `activities` state, Ugg think it needs fire.

### 1a. AIQAContext DEEP DIVE
- **Fake Data**: `AIQAContext.tsx` lines 154-212 preload fake generator cases into `activeGenerationSession`.
  - It creates 3 fake test cases (`tc-gen-1`, `tc-gen-2`, `tc-gen-3`) to make UI look busy.
  - *Consumers*: Only `TestCaseGeneratorPage` use this fake data via `useAIQA()`.
- **Dead State**: `activities` defined at line 223 (`const [activities, setActivities] = useState<ActivityEvent[]>([]);`).
  - Ugg grep whole codebase. Zero consumers for `activities`! It not even exported in `Provider` value! Only `addActivity` is exported. Pure dead weight!

## 2. Routing Audit (Section 7)
Ugg read `G:\AIQA\forentend\src\routes\routeConfig.tsx` (14KB) and `AppRoutes.tsx`.

- **Exact Route Definitions**: `routeConfig.tsx` holds 16 `RouteDef` objects. No lazy loading is used (components are imported directly at top of file).
- **Auth Guards**: If `protectedRoute: true`, route is wrapped in `ProtectedRoute`.
- **AppRoutes.tsx Implementation**: `ProtectedRoute` component (line 17) uses `StateGate` with `auth={auth}` to block content if user not authenticated.

| Route | Page Component | Backend Support | Guard | Status |
|---|---|---|---|---|
| `/dashboard` | DashboardPage | Yes | Yes | Product-approved |
| `/test-generator` | TestCaseGeneratorPage | Yes | Yes | Product-approved |
| `/orchestrator` | AIQAOrchestratorPage | No | Yes | Unsupported-not-implemented |
| `/validator` | TestCaseValidatorPage | No | Yes | Unsupported-not-implemented |
| `/execution` | ExecutionHubPage | Yes | Yes | New-follows-approved |
| `/rbac` | RBACTestingPage | No | Yes | Unsupported-not-implemented |
| `/api-testing` | APITestingPage | Yes | Yes | New-follows-approved |
| `/performance-testing`| PerformanceTestingPage| Yes | Yes | New-follows-approved |
| `/mobile-testing` | MobileTestingPage | No | Yes | Unsupported-not-implemented |
| `/custom-testing` | CustomTestingPage | Yes | Yes | New-follows-approved |
| `/feature-map` | FeatureMapFlowPage | No | Yes | Unsupported-not-implemented |
| `/reports` | ReportsPage | Yes | Yes | New-follows-approved |
| `/audit` | AuditPage | Yes | Yes | New-follows-approved |
| `/evidence` | EvidencePage | Yes | Yes | New-follows-approved |
| `/telemetry` | TelemetryPage | Yes | Yes | New-follows-approved |
| `/settings` | SettingsPage | Yes | No | Google-studio-not-final |

### 2a. Deferred Pages
Ugg look inside pages with NO backend (e.g. `AIQAOrchestratorPage.tsx`, `FeatureMapFlowPage.tsx`).
- They render `EmptyState` component.
- They cite `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` saying feature is "DEFERRED, NOT DESCOPED". They have no real logic, just truthful placeholder.

### 2b. AppRoutes Tests
Ugg read `AppRoutes.test.tsx` (354 lines). Found many good tests:
- Tests default redirect (`/` to `/dashboard`).
- Tests browser history (`popstate` back and forward).
- Tests 404 Not Found fallback.
- Tests Auth Guard states: `no_key`, `key_present` (unverified), `checking`, `authenticated`, `unauthenticated (401)`, `forbidden (403)`, `offline`.
- Security test to ensure API key not leak in URL.

## 3. Navigation Audit (Section 8)
- **Sidebar** (`Sidebar.tsx`): 
  - Loops through `routeDefs`.
  - Conditional rendering: If `isSidebarCollapsed` is true, hides text labels, project name, and badges. Shows only icons.
- **Header** (`Header.tsx`):
  - *Launch Run*: Hardcoded `navigate('/execution')` (Line 305).
  - *Health Check*: Uses `auth.status === 'authenticated'` to pulse green or red (Line 240). This is real backend status.
  - *Notifications*: Subscribes to `RealtimeContext` and shows real `RunEvent` objects (Line 81).
- **CommandPalette** (`CommandPalette.tsx`): 
  - Filters `routeDefs` for "Workspaces" actions.
  - "Quick Actions": `navigate('/execution')`, `setCurrentEnvironment('staging')`, `setCurrentEnvironment('production')`.

## 4. React Context Audit (Section 12)
Ugg hunt down all consumers!

1. **AuthContext** (`AuthContext.tsx`):
   - *Consumers*: `AppRoutes.tsx`, `Header.tsx`, `ProjectContext.tsx`, `ProjectSelector.tsx`.
   - *Backend Health*: `auth.status` is REAL! Uses `aiqaApi.projects.list()` to prove token validity.
2. **ProjectContext** (`ProjectContext.tsx`):
   - *Consumers*: `Sidebar.tsx`, `Header.tsx`, `DashboardPage`, `ExecutionHubPage`, `ReportsPage`, `AuditPage`, `EvidencePage`, `RealtimeContext.tsx`.
   - *LocalStorage*: Uses `aiqa.selectedProjectId`. It is PROPERLY SYNCED! When `refreshProjects` runs, it checks if saved ID is in backend response. If not, it clears it (Lines 108-116).
3. **RealtimeContext** (`RealtimeContext.tsx`):
   - *Consumers*: `Header.tsx`, `ExecutionHubPage`, `ReportsPage`.
   - *WebSocket Reconnect*: Yes, it reconnects! `onclose` triggers `scheduleReconnect` with exponential backoff (max 6 attempts). If close code is 4001 (lag), it reconnects immediately.
4. **AIQAContext** (`AIQAContext.tsx`):
   - *Consumers*: `Sidebar`, `Header`, `CommandPalette`, `SettingsPage`, `TestCaseGeneratorPage`, `DashboardPage`.

## 5. Hooks Audit (Section 13)
- **useSkillSelection** (`useSkillSelection.ts`):
  - *Caller Count*: 2 (`CustomTestingPage`, `TestCaseGeneratorPage`).
  - *Purpose*: Manage built-in and custom skills selection.
  - *Side Effects*: None. Uses global `customSkillCounter` (Line 22) for temp IDs.
  - *Dead Branches*: None.
