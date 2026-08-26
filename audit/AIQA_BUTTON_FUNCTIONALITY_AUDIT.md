# AIQA Button Functionality Audit

Ugg look at all buttons. Ugg see where button click go.

| Button | File | Handler | API | Backend | Persistence | Result | Status |
|---|---|---|---|---|---|---|---|
| Sidebar collapse | Sidebar | onClick | None | None | None | UI toggle | LOCAL-ONLY |
| Command Palette | Header | onClick | None | None | None | UI toggle | LOCAL-ONLY |
| Sign Out | Header | onClick | None | None | localStorage | Clears auth | LOCAL-ONLY |
| Create Project | ProjectSelector | onSubmit | aiqaApi.projects.create | store.rs | SQLite | New project | REAL |
| Refresh Projects | ProjectSelector | onClick | aiqaApi.projects.list | store.rs | SQLite | List projects | REAL |
| Analyze & Generate | TestCaseGenerator | onGenerate | aiqaApi.testCases.generateAi | ai_generation.rs | None | AI gen | REAL |
| Save Case | GeneratedTestCasesView | onSave | aiqaApi.testCases.create | store.rs | SQLite | Saves case | REAL |
| Compile Preview | GeneratedTestCasesView | onCompile | aiqaApi.testCases.compile | execution_plan.rs | None | Plan preview | REAL |
| Toggle Skills | SkillsAndMcpPanel | onToggle | None | None | React state | UI toggle | LOCAL-ONLY |
| Add Custom Skill | SkillManagerModal | onAdd | None | None | Session state | Skill added | LOCAL-ONLY |
| Start Execution | ExecutionHub | onStart | aiqaApi.executions.create | engine.rs | SQLite | Subprocess | REAL |
| Cancel Run | ExecutionHub | onCancel | aiqaApi.executions.cancel | engine.rs | SQLite | Abort handle | REAL |
| Approve | ExecutionHub | onApprove | aiqaApi.executions.approve | store.rs | SQLite | Status update | REAL |
| Reject | ExecutionHub | onReject | aiqaApi.executions.reject | store.rs | SQLite | Status update | REAL |
| Refresh Runs | ExecutionHub | onRefresh | aiqaApi.executions.list | store.rs | SQLite | List runs | REAL |
| Execute API | APITesting | onExecute | aiqaApi.apiTesting.execute | api_testing.rs | None | HTTP request | REAL |
| Add/Remove assertion rows | APITesting | onClick | None | None | React state | UI update | LOCAL-ONLY |
| Start Test | PerformanceTesting | onStart | aiqaApi.performanceTesting.start | performance_testing.rs | SQLite | Load test | REAL |
| Cancel Test | PerformanceTesting | onCancel | aiqaApi.performanceTesting.cancel | performance_testing.rs | SQLite | Test stop | REAL |
| Change Type | PerformanceTesting | onChange | None | None | React state | UI update | LOCAL-ONLY |
| Select Finding | Reports | onSelect | aiqaApi.findings.get | store.rs | SQLite | Finding details | REAL |
| AI Analysis | Reports | onAnalyze | aiqaApi.findings.analysis | intelligence.rs | SQLite | AI report | REAL |
| Confirm/Dismiss | Reports | onReview | aiqaApi.findings.review | store.rs | SQLite | Status update | REAL |
| Refresh | Reports | onRefresh | aiqaApi.findings.list | store.rs | SQLite | List findings | REAL |
| Export JSON/PDF | Reports | onExport | None | None | None | File download | REAL |
| Save Settings | Settings | onSave | aiqaApi.ai.updateSettings | ai_router.rs | SQLite | Config update | REAL |
| Refresh | Settings | onRefresh | aiqaApi.ai.getSettings | ai_router.rs | SQLite | Get config | REAL |
| Change Tab | Settings | onClick | None | None | React state | UI update | LOCAL-ONLY |
| Connect MCP | SkillsAndMcpPanel | onClick | None | None | None | Modal open | PLACEHOLDER |
| Telemetry Refresh | Telemetry | onRefresh | aiqaApi.metrics.get | metrics.rs | None | Metrics data | REAL |
