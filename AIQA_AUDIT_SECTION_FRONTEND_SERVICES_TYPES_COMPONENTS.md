# AIQA Audit Report - Frontend Services, Types, Components, and Pages (DEEP AUDIT)

## 1. Services Audit

### aiqaApi.ts Methods

| Group | Method | HTTP | Path | Backend Match | Callers |
|---|---|---|---|---|---|
| projects | list | GET | /projects | Yes | Checked |
| projects | get | GET | /projects/:id | Yes | Checked |
| projects | create | POST | /projects | Yes | Checked |
| projects | update | PATCH | /projects/:id | Yes | Checked |
| executions | list | GET | /projects/:projectId/executions | Yes | Checked |
| executions | create | POST | /projects/:projectId/executions | Yes | Checked |
| executions | get | GET | /projects/:projectId/executions/:runId | Yes | Checked |
| executions | cancel | POST | /projects/:projectId/executions/:runId/cancel | Yes | Checked |
| executions | approve | POST | /projects/:projectId/executions/:runId/approve | Yes | Checked |
| executions | reject | POST | /projects/:projectId/executions/:runId/reject | Yes | Checked |
| executions | tasks | GET | /projects/:projectId/executions/:runId/tasks | Yes | Checked |
| executions | task | GET | /projects/:projectId/executions/:runId/tasks/:taskId | Yes | Checked |
| executions | events | GET | /projects/:projectId/executions/:runId/events | Yes | Checked |
| executions | evidence | GET | /projects/:projectId/executions/:runId/evidence | Yes | Checked |
| evidence | get | GET | /projects/:projectId/evidence/:evidenceId | Yes | Checked |
| evidence | content | GET | /projects/:projectId/evidence/:evidenceId/content | Yes | Checked |
| findings | list | GET | /projects/:projectId/findings | Yes | Checked |
| findings | get | GET | /projects/:projectId/findings/:findingId | Yes | Checked |
| findings | analysis | POST | /projects/:projectId/findings/:findingId/analysis | Yes | Checked |
| findings | review | POST | /projects/:projectId/findings/:findingId/review | Yes | Checked |
| dashboard | get | GET | /projects/:projectId/dashboard | Yes | Checked |
| dashboard | kpis | GET | /projects/:projectId/kpis | Yes | Checked |
| audit | list | GET | /projects/:projectId/audit | Yes | Checked |
| intelligence | tests | GET | /projects/:projectId/intelligence/tests | Yes | Checked |
| testCases | list | GET | /projects/:projectId/test-cases | Yes | Checked |
| testCases | create | POST | /projects/:projectId/test-cases | Yes | Checked |
| testCases | get | GET | /projects/:projectId/test-cases/:id | Yes | Checked |
| testCases | update | PATCH | /projects/:projectId/test-cases/:id | Yes | Checked |
| testCases | delete | DELETE | /projects/:projectId/test-cases/:id | Yes | Checked |
| testCases | validate | POST | /projects/:projectId/test-cases/:id/validate | Yes | Checked |
| testCases | approve | POST | /projects/:projectId/test-cases/:id/approve | Yes | Checked |
| testCases | reject | POST | /projects/:projectId/test-cases/:id/reject | Yes | Checked |
| testCases | handoff | POST | /projects/:projectId/test-cases/:id/handoff | Yes | Checked |
| testCases | generateAi | POST | /projects/:projectId/test-cases/generate-ai | Yes | Checked |
| testCases | compile | POST | /projects/:projectId/test-cases/:id/compile | Yes | Checked |
| customTesting | generate | POST | /projects/:projectId/custom-testing/generate | Yes | Checked |
| apiTesting | execute | POST | /projects/:projectId/api-testing/execute | Yes | Checked |
| performanceTesting | start | POST | /projects/:projectId/performance-testing/start | Yes | Checked |
| performanceTesting | status | GET | /projects/:projectId/performance-testing/:runId/status | Yes | Checked |
| performanceTesting | cancel | POST | /projects/:projectId/performance-testing/:runId/cancel | Yes | Checked |
| performanceTesting | list | GET | /projects/:projectId/performance-testing | Yes | Checked |
| appSource | verify | POST | /projects/:projectId/verify-application-source | Yes | Checked |
| ai | status | GET | /ai/status | Yes | Checked |
| ai | getSettings | GET | /ai/settings | Yes | Checked |
| ai | updateSettings | PUT | /ai/settings | Yes | Checked |
| ai | refresh | POST | /ai/refresh | Yes | Checked |
| metrics | get | GET | /metrics | Yes | Checked |

### skillRegistry.ts

- skill-functional-testing (CONFIGURATION LABEL)

- skill-edge-case-analysis (CONFIGURATION LABEL)

- skill-security-testing (CONFIGURATION LABEL)

- skill-performance-testing (CONFIGURATION LABEL)

- skill-api-testing (CONFIGURATION LABEL)

- skill-database-testing (CONFIGURATION LABEL)

- skill-rbac-testing (CONFIGURATION LABEL)

- skill-business-case-testing (CONFIGURATION LABEL)


## 2. Type/Contract Audit

### contract.ts

- Project, CreateProjectRequest, UpdateProjectRequest

- TaskDef, RunPlan, CreateExecutionResponse, TaskState, RunSummary, RunDetail, RunListItem, CancelRunResponse

- RunEvent, EvidenceRow, Finding, FailureCluster, FindingAnalysis, ReviewFindingRequest

- TestIntelRow, DashboardResponse, KpisResponse, AuditRow

- TestCaseStatus, TestCaseStep, CreateTestCaseRequest, TestCaseRow, UpdateTestCaseRequest, TestCaseHandoffRequest

- PlanExecutionType, PlanAction, PlanCompilerMethod, ExecutionPlan, CompilationOutcome

- AdHocApiRequest, ApiAssertionResult, ApiResultSummary, ApiTestExecutionResponse

- PerformanceWorkloadSchedule, PerformanceTestConfig, PerformanceSampleOutcome, PerformanceRequestSample

- PerformanceMetrics, PerformanceRunLifecycleStatus, PerformanceVerdict, PerformanceTestStartResponse

- PerformanceRunStatusResponse, PerformanceTestCancelResponse, PerformanceRunRow

- AppSourceVerifyStatus, VerifyApplicationSourceRequest, VerifyApplicationSourceResponse

- GenerateAiSkillInput, GenerateAiTestCasesRequest, GeneratedTestCaseDraftStep, GenerationEvidenceKind

- GeneratedTestCaseDraft, GenerateAiTestCasesResponse, CustomTestingGenerateRequest, CustomTestingAnalysis

All mirror backend explicitly. No mismatches found.


### types.ts

NavPage, AIQAStatus, FailureClassification, Environment, ProjectStatus, ProjectMetrics, ProjectExecutionPoint, ActivityEvent, GeneratorPipelineStep, RequirementDocType, UploadedRequirementDoc, AppSourceType, AppConnectionStatus, AppSourceConnection, TestGenerationCategory, TestCasePriority, TestCaseSeverity, GenerationExecutionState, TraceabilityChain, GeneratedTestCase, GenerationSession, SystemSettings


## 3. Component Audit

| Component | Type | Has Tests | Notes |

|---|---|---|---|

| common/EmptyState.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| common/SectionHeader.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| dashboard/AIQualityCoreOrb.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| dashboard/ProjectExecutionGraph.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| dashboard/ProjectStatusDonut.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| layout/CommandPalette.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| layout/Header.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| layout/Sidebar.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| pages/AIQAOrchestratorPage.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| pages/APITestingPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/AuditPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/CustomTestingPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/DashboardPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/EvidencePage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/ExecutionHubPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/FeatureMapFlowPage.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| pages/MobileTestingPage.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| pages/NotFoundPage.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| pages/PerformanceTestingPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/RBACTestingPage.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| pages/ReportsPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/SettingsPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/TelemetryPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/TestCaseGeneratorPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| pages/TestCaseValidatorPage.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| project/ProjectSelector.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| state/StateViews.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| test-generator/ApplicationSourceSection.tsx | UI/Page | Yes | ACTIVE/PLACEHOLDER |
| test-generator/DocumentUploadSection.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| test-generator/GeneratedTestCasesView.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| test-generator/GenerationConfigSection.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| test-generator/McpConnectModal.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| test-generator/SkillManagerModal.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| test-generator/SkillsAndMcpPanel.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |
| test-generator/TraceabilityGraph.tsx | UI/Page | No | ACTIVE/PLACEHOLDER |

## 4. Page-by-Page Audit (DEEP TRACE)

### DashboardPage.tsx

- **Status:** ACTIVE

- **API Calls:** `aiqaApi.dashboard.get`, `aiqaApi.dashboard.kpis`

### TestCaseGeneratorPage.tsx

- **Status:** ACTIVE

- **API Calls:** `aiqaApi.testCases.generateAi`, `aiqaApi.testCases.create`, `aiqaApi.testCases.validate`, `aiqaApi.testCases.approve`, `aiqaApi.testCases.reject`, `aiqaApi.testCases.handoff`

### AIQAOrchestratorPage.tsx

- **Status:** DEFERRED

- **Renders:** `EmptyState` component with 'DEFERRED - REAL FUTURE SCOPE' badge.

### TestCaseValidatorPage.tsx

- **Status:** DEFERRED (Redirect)

- **Renders:** `EmptyState` explaining validation happens on Test Generator page. No dedicated validator UI here.

### MobileTestingPage.tsx

- **Status:** DEFERRED

- **Renders:** `EmptyState` explaining deferred scope for device farm.

### RBACTestingPage.tsx

- **Status:** DEFERRED

- **Renders:** `EmptyState` explaining RBAC is enforced inline on backend.

### FeatureMapFlowPage.tsx

- **Status:** PLACEHOLDER

- **Renders:** Basic empty state or coming soon.
