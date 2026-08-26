# AIQA Audit Report: Buttons, Forms, Modals, Skills, and MCP

## 1. Button Audit — CRITICAL (Section 9)

An exhaustive search of all `.tsx` files in `G:\AIQA\forentend\src\` was conducted. Every action was traced.

### Comprehensive Button Inventory

| Button / Interaction | File | Handler | API / Service / Action | Backend / Persistence | Status |
|---|---|---|---|---|---|
| **Layout & Common** |
| `Verify Connection` | `Header.tsx` / `StateViews` | `onVerify` | `aiqaApi.ai.status` / `metrics.get` | `api.rs` / `metrics.rs` | **REAL** |
| `Sign Out` | `Header.tsx` / `StateViews` | `onSignOut` | Local Storage | Clears `API_KEY_STORAGE` | **LOCAL-ONLY** |
| `Configure API Key` | `StateViews.tsx` | `onConfigure` | Modifies UI state | None | **LOCAL-ONLY** |
| `Retry` | `StateViews.tsx` | `onRetry` | Re-triggers active API call | Target API | **REAL** |
| `Sidebar Collapse` | `Sidebar.tsx` | `toggleSidebar` | UI State | None | **LOCAL-ONLY** |
| `Command Palette` | `Sidebar.tsx` | `setCommandPaletteOpen` | UI State | None | **LOCAL-ONLY** |
| **Dashboard & Telemetry** |
| `Refresh` | `TelemetryPage.tsx` | `handleRefresh` | `aiqaApi.metrics.get` | `metrics.rs` | **REAL** |
| **Projects & Settings** |
| `Open/Close Create` | `ProjectSelector.tsx` | `setIsOpen` / `closeCreateForm` | UI State | None | **LOCAL-ONLY** |
| `Create Project` | `ProjectSelector.tsx` | `handleCreateSubmit` | `aiqaApi.projects.create` | `api.rs` | **REAL** |
| `Refresh Projects` | `ProjectSelector.tsx` | `refreshProjects` | `aiqaApi.projects.list` | `api.rs` | **REAL** |
| `Save Settings` | `SettingsPage.tsx` | `handleSave` / `handleSaveSettings` | `aiqaApi.ai.updateSettings` | `ai_router.rs` | **REAL** |
| `Refresh Settings` | `SettingsPage.tsx` | `handleRefresh` | `aiqaApi.ai.refresh` | `ai_router.rs` | **REAL** |
| `Change Tab` | `SettingsPage.tsx` | `setActiveTab` | UI State | None | **LOCAL-ONLY** |
| **Test Generation & Skills** |
| `Toggle Category/Skill` | `GenerationConfigSection` | `onToggleCategory` / `onToggleSkill` | Context/UI State | None | **LOCAL-ONLY** |
| `Analyze & Generate` | `TestCaseGeneratorPage` | `handleAnalyzeAndGenerate` | `aiqaApi.testCases.generateAi` | `ai_generation.rs` | **REAL** (Draft) |
| `Switch Tabs` | `TestCaseGeneratorPage` | `setActiveBranch` | UI State | None | **LOCAL-ONLY** |
| `Save Case` | `GeneratedTestCasesView` | `handleSaveAsTestCase` | `aiqaApi.testCases.create` | SQLite DB | **REAL** |
| `Compile Preview` | `GeneratedTestCasesView` | `onCompilePreview` | `aiqaApi.testCases.compile` | `api.rs` | **REAL** |
| `Add Custom Skill` | `SkillManagerModal.tsx` | `onAddCustomSkill` | React State | None | **LOCAL-ONLY** |
| **Execution Hub & API Testing** |
| `Start Execution` | `ExecutionHubPage.tsx` | `handleCreate` | `aiqaApi.executions.create` | `engine.rs` | **REAL** |
| `Cancel Run` | `ExecutionHubPage.tsx` | `handleCancel` | `aiqaApi.executions.cancel` | `api.rs` | **REAL** |
| `Approve / Reject` | `ExecutionHubPage.tsx` | `handleApprove` / `handleReject` | `aiqaApi.executions.approve/reject`| `api.rs` | **REAL** |
| `Refresh Runs` | `ExecutionHubPage.tsx` | `handleRefreshRuns` | `aiqaApi.executions.list` | `api.rs` | **REAL** |
| `Add/Remove Rows` | `APITestingPage.tsx` | `setRows` / `setAssertions` | UI Form State | Sent on exec | **LOCAL-ONLY** |
| `Execute API` | `APITestingPage.tsx` | `handleExecute` | `aiqaApi.apiTesting.execute` | `executor.rs` | **REAL** |
| **Performance Testing** |
| `Change Type` | `PerformanceTestingPage`| `setTestType` | UI State | None | **LOCAL-ONLY** |
| `Start Perf Test` | `PerformanceTestingPage`| `handleStart` | `aiqaApi.performanceTesting.start` | `performance_testing.rs`| **REAL** |
| `Cancel Perf Test` | `PerformanceTestingPage`| `handleCancel` | `aiqaApi.performanceTesting.cancel`| `performance_testing.rs`| **REAL** |
| **Reports, Audit & Evidence** |
| `Select Finding` | `ReportsPage.tsx` | `handleSelectFinding` | UI State | None | **LOCAL-ONLY** |
| `AI Analysis` | `ReportsPage.tsx` | `fetchAnalysis` | `aiqaApi.findings.analysis` | `intelligence.rs` | **REAL** |
| `Confirm/Dismiss` | `ReportsPage.tsx` | `handleReview` | `aiqaApi.findings.review` | `api.rs` | **REAL** |
| `Refresh Reports` | `ReportsPage.tsx` | `handleRefresh` | `aiqaApi.findings.list` | `api.rs` | **REAL** |
| `Export JSON/PDF` | `ReportsPage.tsx` | `handleExportJSON` / `PDF` | Client-side export generator | Local download | **REAL** (Client) |

### Disabled States Analysis
We checked for buttons that are "always disabled" (`disabled={true}`). **No hardcoded dead buttons exist.** Every `disabled` property is dynamically bound to a real state variable:
- `isGeneratingNow` / `isGenerating` / `compilePreviewLoading`
- `createSubmitting` / `saving` / `refreshing` / `exporting !== null`
- `isRunning` / `starting` / `cancelling` (Performance Testing)
- `disabled` (Props passed from parent when a test is already running or requires project selection)

All "loading" or disabled states resolve natively when the Promises complete. There are no infinite spinners tied to unbacked functions.

## 2. Form Audit (Section 10)

Forms within the application strictly tie into context state or API persistence without fabricated "dummy" submissions.

1. **Settings / AI Model Form (`SettingsPage.tsx`)**: Calls `aiqaApi.ai.updateSettings` and saves orchestrator API key locally (`setApiKey`).
2. **Project Creation Form (`ProjectSelector.tsx`)**: Calls `aiqaApi.projects.create`. Persists directly via backend SQLite store.
3. **Assertion Builders (API Testing)**: Values are packed into the compilation payload and sent to the orchestrator. No fake data.

## 3. Modal/Dialog Audit (Section 11)

1. **MCP Connect Modal (`McpConnectModal.tsx`)**: Purely informational warning that MCP is not yet configured. **PLACEHOLDER (Honest Disclosure)**.
2. **Skill Manager Modal (`SkillManagerModal.tsx`)**: Allows creation of Session-based Custom Skills mapped to Prompt logic. **LOCAL-ONLY**.

## 4. Skills Audit (Section 20)

**Path Tracing**: Skill Selected → `useSkillSelection` → API Request → Backend `ai_generation.rs::resolve_skill_instructions` → `build_prompt` → Execution via `AiInfrastructure`.

### Complete Inventory of Built-In Skills (from `skillRegistry.ts` & `ai_generation.rs`)

| Skill ID | Name | UI Description | Backend Prompt Instruction |
|---|---|---|---|
| `skill-functional-testing` | Functional Testing | Core requirement and acceptance-criteria behavior coverage. | Identify: happy paths, alternate paths, valid inputs, invalid inputs, business rules, expected system behavior, and dependencies actually described or implied by the requirement text. |
| `skill-edge-case-analysis` | Edge Case Analysis | Boundary values, unusual sequences, and rare-but-real conditions. | Identify: boundary values, null/empty values, malformed values, maximum/minimum values, unexpected state transitions, unusual input combinations, and concurrency-sensitive edge conditions where the requirement text gives a real basis for them. |
| `skill-security-testing` | Security Testing | Authentication, authorization, and input-handling risk coverage. | Identify applicable authentication, authorization, privilege-escalation, input-validation, injection, session-handling, sensitive-data-exposure, and API-abuse scenarios... Do not invent security scenarios the text gives no basis for. |
| `skill-performance-testing`| Performance Testing | Latency, throughput, and resource-behavior scenarios. | Identify applicable response-time, load, stress, spike, volume, concurrency, or throughput scenarios... Do not invent a specific number that is not in the text; if none is given, describe qualitatively. |
| `skill-api-testing` | API Testing | Request/response contract and status-code coverage. | Identify request validation, response validation, status codes, authentication, authorization, malformed-request handling, rate limiting, and contract behavior... |
| `skill-database-testing` | Database Testing | Data integrity, constraint, and query-behavior coverage. | Identify CRUD behavior, constraints, relationships, transaction behavior, data consistency, nullability, and duplicate handling... |
| `skill-rbac-testing` | RBAC Testing | Role and permission-boundary scenario coverage. | Identify role- and permission-boundary scenarios (which roles can/cannot perform which actions)... |
| `skill-business-case-testing`| Business Case Testing| End-to-end business-rule and workflow scenario coverage. | Identify end-to-end business-rule and workflow scenarios, decision paths, rule conflicts, exceptions, and boundary business scenarios actually described by the requirement text. |

**Classification:** **REAL SKILL (Prompt-Driven)**. 
**However, there is NO external tool or service invoked.** The skills operate strictly as configuration labels that inject the specific directives listed above into the core LLM prompt.

## 5. MCP Audit (Section 21)

**Audit Trace:**
- Registry: **NONE** (`skillRegistry.ts` line 4 confirms: "no plugin registry, MCP connector... exists").
- Server Discovery: **NONE**.
- Authentication / Credentials: **NONE**.
- Tool Discovery: **NONE**.

**Verdict**: The MCP architecture is completely unbuilt in this phase. The UI correctly acknowledges this via the `McpConnectModal`. **PLACEHOLDER**.
