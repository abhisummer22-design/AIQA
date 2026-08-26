# AIQA Backend-Frontend Wiring Audit

Ugg trace wires. Make sure face talk to brain.

## Frontend to Backend Routes
- 48 frontend methods mapped to backend routes.
- 63+ backend routes.
- Scoped routes use `/api/v1/projects/:id/` prefix.

| Frontend Method | HTTP | Endpoint | Backend Handler | DB | Wired | Status |
|---|---|---|---|---|---|---|
| aiqaApi.projects.create | POST | /api/v1/projects | api::create_project | store.rs | Yes | REAL |
| aiqaApi.projects.list | GET | /api/v1/projects | api::list_projects | store.rs | Yes | REAL |
| aiqaApi.dashboard.get | GET | /api/v1/projects/:id/dashboard | api::get_dashboard | store.rs | Yes | REAL |
| aiqaApi.dashboard.kpis | GET | /api/v1/projects/:id/kpis | api::get_kpis | store.rs | Yes | REAL |
| aiqaApi.testCases.generateAi | POST | /api/v1/projects/:id/test-cases/generate | ai_generation::generate | store.rs | Yes | REAL |
| aiqaApi.testCases.create | POST | /api/v1/projects/:id/test-cases | api::create_test_case | store.rs | Yes | REAL |
| aiqaApi.testCases.compile | POST | /api/v1/projects/:id/test-cases/:tc_id/compile | execution_plan::compile | None | Yes | REAL |
| aiqaApi.executions.create | POST | /api/v1/projects/:id/executions | engine::create | store.rs | Yes | REAL |
| aiqaApi.executions.cancel | POST | /api/v1/projects/:id/executions/:ex_id/cancel | engine::cancel | store.rs | Yes | REAL |
| aiqaApi.executions.approve | POST | /api/v1/projects/:id/executions/:ex_id/approve | engine::approve | store.rs | Yes | REAL |
| aiqaApi.executions.reject | POST | /api/v1/projects/:id/executions/:ex_id/reject | engine::reject | store.rs | Yes | REAL |
| aiqaApi.apiTesting.execute | POST | /api/v1/projects/:id/api-testing/execute | api_testing::execute | None | Yes | REAL |
| aiqaApi.performanceTesting.start | POST | /api/v1/projects/:id/performance/start | performance_testing::start | store.rs | Yes | REAL |
| aiqaApi.performanceTesting.cancel | POST | /api/v1/projects/:id/performance/:pid/cancel | performance_testing::cancel | store.rs | Yes | REAL |
| aiqaApi.findings.analysis | POST | /api/v1/projects/:id/findings/:fid/analysis | intelligence::analyze | store.rs | Yes | REAL |
| aiqaApi.findings.review | POST | /api/v1/projects/:id/findings/:fid/review | api::review_finding | store.rs | Yes | REAL |
| aiqaApi.ai.updateSettings | PUT | /api/v1/ai/settings | ai_router::update_settings | store.rs | Yes | REAL |
| aiqaApi.metrics.get | GET | /api/v1/metrics | metrics::get | None | Yes | REAL |
| (various others mapped similarly) | ... | ... | ... | ... | Yes | REAL |

## Legacy Flat Routes (Mismatches)
- `/runs`, `/runs/:id`, `/runs/:id/events` -> Dead, no UI calls these.
- `/runs/:id/approve`, `/runs/:id/reject` -> Dead.
- `/intelligence/*` -> Dead.
- `/admin/*` -> Dead.

## WebSocket Wiring
- 2 WS endpoints: run events and project events.
- Wired correctly in RealtimeProvider.
- Health check wired correctly.
