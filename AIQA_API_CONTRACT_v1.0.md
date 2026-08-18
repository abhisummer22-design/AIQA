# AIQA — API Contract
## Version 1.0

**Document Type:** Backend ↔ Frontend Integration Contract  
**Status:** Implementation Baseline  
**Authority:** Derived from AIQA Business Logic & Architecture v2.0 and AIQA Consolidated PRD + FRD + SRD v6.0

---

## 1. Purpose

This document is the single integration contract between the AIQA backend and frontend.

The backend and frontend MUST implement the behavior defined here. Neither side may invent endpoint names, request fields, response fields, status values, business rules, or realtime event semantics that conflict with this contract.

This contract does not replace the Business Logic, PRD, FRD, or SRD documents. It converts their requirements into an implementation-facing interface between the frontend and backend.

### Source-of-truth hierarchy

1. Business Logic & Architecture v2.0 — business behavior and architectural rules
2. PRD + FRD + SRD v6.0 — product, functional, and system requirements
3. This API Contract v1.0 — concrete frontend/backend interface
4. Acceptance Matrix v1.0 — completion and verification status

If a conflict is discovered, implementation MUST NOT silently choose a behavior. The conflict must be reported and resolved in the source documents/contract.

---

# 2. Global API Rules

## 2.1 API Versioning

All public application APIs MUST use an explicit version prefix:

`/api/v1/...`

Breaking changes require a new API version.

Non-breaking additions may remain within the current version.

## 2.2 Content Type

JSON APIs use:

`application/json`

Binary evidence/files MUST NOT be embedded directly inside normal JSON responses. APIs SHOULD return metadata plus a reference to the stored artifact.

## 2.3 Project Scoping

AIQA is project-centric.

Any project-owned resource MUST be associated with a `project_id`, either explicitly in the request path/body or through an authenticated project context.

The backend MUST enforce project isolation server-side.

The frontend MUST never assume that project isolation is only a UI concern.

## 2.4 Authentication

Protected APIs require an authenticated identity.

The backend is authoritative for authentication and authorization.

The frontend may hide or disable unavailable actions, but server-side authorization MUST always be enforced.

## 2.5 Error Contract

Errors MUST be structured and machine-readable.

Recommended baseline:

```json
{
  "error": {
    "code": "RESOURCE_NOT_FOUND",
    "message": "Project was not found.",
    "details": {},
    "request_id": "req_123"
  }
}
```

Frontend MUST display a user-safe message and MUST NOT expose secrets, internal stack traces, credentials, or sensitive infrastructure details.

---

# 3. Common Data Conventions

## 3.1 Identifiers

Resources use stable opaque identifiers.

Examples:

- `project_id`
- `test_case_id`
- `test_suite_id`
- `run_id`
- `task_id`
- `finding_id`
- `evidence_id`
- `report_id`
- `agent_task_id`

The frontend MUST treat IDs as opaque strings.

## 3.2 Timestamps

Timestamps MUST be serialized consistently and include timezone information.

Recommended representation:

`2026-08-17T09:30:00Z`

## 3.3 Pagination

Collection APIs SHOULD support:

- `page`
- `page_size`

or an equivalent cursor mechanism.

Responses SHOULD expose enough metadata for the frontend to render pagination reliably.

## 3.4 Sorting and Filtering

Where applicable, collection APIs SHOULD support:

- filter
- search
- sort
- status/category filtering
- project/module filtering

The exact supported fields MUST be documented per endpoint before frontend implementation.

---

# 4. Core Resource Model

The frontend and backend share the following conceptual resources:

```text
Project
 ├── Context / Documents
 ├── Features / Flows
 ├── Requirements
 ├── Test Cases
 ├── Test Suites
 ├── Test Plans
 ├── Runs
 │    ├── Tasks
 │    ├── Events
 │    └── Evidence
 ├── Findings
 ├── Reports
 ├── RBAC Configuration / Observations
 ├── API Test Data
 ├── Mobile Device / Capability Data
 └── Audit Records
```

Relational/project-owned relationships are authoritative. Vector/semantic retrieval is supplemental.

Traceability MUST support:

`requirement → feature → module → API/DB/UI → test → execution → evidence → finding/report`

---

# 5. Project APIs

## 5.1 List Projects

`GET /api/v1/projects`

Purpose:
Return projects available to the authenticated user.

Frontend uses this for project selection.

## 5.2 Create Project

`POST /api/v1/projects`

Request concept:

```json
{
  "name": "Project Name",
  "description": "Project description"
}
```

Response MUST return the created project's stable identifier and authoritative metadata.

## 5.3 Get Project

`GET /api/v1/projects/{project_id}`

## 5.4 Update Project

`PATCH /api/v1/projects/{project_id}`

## 5.5 Project Context

`GET /api/v1/projects/{project_id}/context`

Returns available project context references such as:

- PRD
- FRD
- SRD
- Figma/design references
- API specifications
- DB schema
- repository metadata
- existing tests
- historical defects

Large raw artifacts SHOULD NOT be sent wholesale to an LLM. Targeted context references are preferred.

---

# 6. Context / Ingestion APIs

## 6.1 Ingest Context

`POST /api/v1/projects/{project_id}/context`

Supported context types may include:

- document
- API specification
- database schema
- repository metadata
- design/Figma reference
- existing test
- historical defect

Response MUST identify the created context reference.

## 6.2 Context Processing Status

`GET /api/v1/projects/{project_id}/context/{context_id}`

Frontend MUST be able to distinguish at least:

- processing
- ready
- failed
- needs_review

---

# 7. Quality Graph / Requirement APIs

The quality model maintains project relationships between requirements, features, modules, tests, executions, findings and reports.

## 7.1 Get Quality Graph

`GET /api/v1/projects/{project_id}/quality-graph`

The response SHOULD provide graph nodes/relationships required by the frontend.

## 7.2 List Requirements

`GET /api/v1/projects/{project_id}/requirements`

## 7.3 List Features

`GET /api/v1/projects/{project_id}/features`

## 7.4 Feature Flow

`GET /api/v1/projects/{project_id}/features/{feature_id}/flow`

---

# 8. Test Case Generator APIs

The generator supports the unified categories defined by the source requirements:

- Functional
- Non-Functional
- UI/UX
- Edge Case
- Scenario-Based
- Business Logic
- Security
- Penetration
- RBAC
- Regression
- Database
- API
- Performance
- Mobile where applicable

## 8.1 Generate Tests

`POST /api/v1/projects/{project_id}/test-generation`

Request concept:

```json
{
  "scope": {
    "module_ids": [],
    "feature_ids": [],
    "requirement_ids": []
  },
  "categories": [],
  "include_positive": true,
  "include_negative": true,
  "constraints": {}
}
```

Response MUST return a generation job/task identifier.

Generation is asynchronous.

## 8.2 Get Generation Job

`GET /api/v1/projects/{project_id}/test-generation/{generation_id}`

## 8.3 Generated Test Cases

`GET /api/v1/projects/{project_id}/test-cases`

Filtering SHOULD support:

- category
- priority
- status
- requirement
- feature
- validation status

## 8.4 Test Case Detail

`GET /api/v1/projects/{project_id}/test-cases/{test_case_id}`

Each test case MUST preserve:

- category
- priority
- preconditions
- steps
- expected result
- traceability
- validation status

---

# 9. Test Case Validator APIs

Generated tests MUST pass configured quality/benchmark rules before unrestricted automatic execution.

## 9.1 Validate Tests

`POST /api/v1/projects/{project_id}/test-validation`

Request:

```json
{
  "test_case_ids": []
}
```

## 9.2 Validation Result

`GET /api/v1/projects/{project_id}/test-validation/{validation_id}`

Validation SHOULD expose:

- traceability status
- quality status
- duplicate status
- coverage status
- risk status
- approval/review state
- rejection reasons

Possible high-level result:

- PASSED
- FAILED
- NEEDS_REVIEW

The backend MUST NOT mark an invalid AI-generated test as automatically approved.

---

# 10. Test Suite / Plan APIs

## 10.1 Create Test Suite

`POST /api/v1/projects/{project_id}/test-suites`

## 10.2 Get Test Suite

`GET /api/v1/projects/{project_id}/test-suites/{test_suite_id}`

## 10.3 Create Execution Plan

`POST /api/v1/projects/{project_id}/test-plans`

The plan may include:

- selected tests
- scope
- environment
- priority
- dependencies
- execution policy
- parallelism policy

---

# 11. AIQA Orchestrator APIs

The central AIQA Orchestrator coordinates specialized testing intelligence.

## 11.1 Create Orchestration Plan

`POST /api/v1/projects/{project_id}/orchestrator/plans`

Request concept:

```json
{
  "scope": {},
  "objective": "Focused quality assessment",
  "constraints": {}
}
```

## 11.2 Get Plan

`GET /api/v1/projects/{project_id}/orchestrator/plans/{plan_id}`

The plan SHOULD expose:

- selected capabilities
- task priorities
- dependencies
- routing decisions
- risk/change/test-value metadata
- validation state

Agent failures SHOULD be isolated from unrelated work where possible.

Unsupported AI conclusions MUST NOT be exposed as confirmed defects.

---

# 12. Execution APIs

All execution is asynchronous.

Long-running tests MUST NOT block normal API request threads.

## 12.1 Start Execution

`POST /api/v1/projects/{project_id}/executions`

Request concept:

```json
{
  "test_case_ids": [],
  "test_suite_id": null,
  "environment_id": "env_123",
  "execution_mode": "controlled"
}
```

Response:

```json
{
  "run_id": "run_123",
  "status": "QUEUED"
}
```

## 12.2 Get Run

`GET /api/v1/projects/{project_id}/executions/{run_id}`

## 12.3 Cancel Run

`POST /api/v1/projects/{project_id}/executions/{run_id}/cancel`

Cancellation must be policy-aware and auditable.

## 12.4 Run Tasks

`GET /api/v1/projects/{project_id}/executions/{run_id}/tasks`

## 12.5 Task Detail

`GET /api/v1/projects/{project_id}/executions/{run_id}/tasks/{task_id}`

---

# 13. Canonical Execution Status

The following statuses are canonical:

- `QUEUED`
- `RUNNING`
- `PASSED`
- `FAILED`
- `BLOCKED`
- `SKIPPED`
- `CANCELLED`
- `NEEDS_REVIEW`
- `INFRASTRUCTURE_ERROR`

Frontend MUST NOT create alternate spellings for these statuses.

## Dependency behavior

- Independent tests continue after unrelated failures.
- Dependent tests become `BLOCKED` when prerequisites fail.
- Product failure and infrastructure/environment failure MUST remain distinguishable.

---

# 14. Realtime Event Contract

AIQA requires incremental UI updates and authoritative state reconciliation after reconnect.

WebSocket or equivalent event transport SHOULD expose:

`/api/v1/projects/{project_id}/events`

Events MUST contain enough information to correlate them with:

- project
- run
- task
- test
- evidence
- finding

Recommended event envelope:

```json
{
  "event_id": "evt_123",
  "event_type": "TASK_STATUS_CHANGED",
  "project_id": "project_123",
  "run_id": "run_123",
  "task_id": "task_123",
  "timestamp": "2026-08-17T09:30:00Z",
  "sequence": 42,
  "data": {}
}
```

## Core event types

At minimum, the system should support normalized events for:

- `RUN_CREATED`
- `RUN_STATUS_CHANGED`
- `TASK_CREATED`
- `TASK_STATUS_CHANGED`
- `TASK_PROGRESS`
- `EVIDENCE_CREATED`
- `FINDING_CREATED`
- `MODEL_REQUEST`
- `MODEL_RESPONSE`
- `MODEL_FALLBACK`
- `WORKER_STATUS_CHANGED`
- `API_TELEMETRY`
- `RBAC_ACCESS_PATH`
- `REPORT_READY`
- `ERROR`

The backend is authoritative for event ordering/state.

Frontend event handling MUST be idempotent.

After reconnect, frontend MUST reconcile with authoritative REST state rather than assuming that no missed events occurred.

---

# 15. Evidence APIs

Evidence is first-class and must remain linked to execution.

## 15.1 List Evidence

`GET /api/v1/projects/{project_id}/executions/{run_id}/evidence`

## 15.2 Evidence Detail

`GET /api/v1/projects/{project_id}/evidence/{evidence_id}`

Evidence may reference:

- screenshots
- browser traces
- logs
- request/response
- timing data
- mobile evidence
- worker output

Large binary artifacts SHOULD be accessed through secure artifact references rather than embedded in normal API responses.

---

# 16. Findings / Failure Intelligence APIs

## 16.1 List Findings

`GET /api/v1/projects/{project_id}/findings`

## 16.2 Finding Detail

`GET /api/v1/projects/{project_id}/findings/{finding_id}`

## 16.3 Failure Analysis

`POST /api/v1/projects/{project_id}/findings/{finding_id}/analysis`

Failure intelligence may correlate:

- execution evidence
- environment data
- historical failures
- logs

AI hypotheses MUST remain clearly distinguishable from confirmed facts.

Uncertain findings use:

`NEEDS_REVIEW`

rather than being automatically classified as confirmed defects.

---

# 17. RBAC APIs

RBAC is first-class business logic and authorization remains server-side.

## 17.1 Roles

`GET /api/v1/projects/{project_id}/rbac/roles`

## 17.2 Users

`GET /api/v1/projects/{project_id}/rbac/users`

## 17.3 Permissions

`GET /api/v1/projects/{project_id}/rbac/permissions`

## 17.4 Access Matrix

`GET /api/v1/projects/{project_id}/rbac/matrix`

The matrix SHOULD represent:

`role × permission × action`

## 17.5 RBAC Test Generation

`POST /api/v1/projects/{project_id}/rbac/test-generation`

Generated coverage MUST include allowed and unauthorized actions where applicable.

## 17.6 RBAC Execution View

Realtime events SHOULD expose the active role/access path during execution.

---

# 18. API Testing APIs

## 18.1 API Specifications

`GET /api/v1/projects/{project_id}/api/specifications`

## 18.2 API Test Generation

`POST /api/v1/projects/{project_id}/api/test-generation`

## 18.3 API Test Execution

`POST /api/v1/projects/{project_id}/api/executions`

API testing must support, where applicable:

- request/response bodies
- headers
- parameters
- schemas
- status codes
- errors
- timeouts
- rate limits
- latency

## 18.4 API Telemetry

`GET /api/v1/projects/{project_id}/api/telemetry`

Realtime API telemetry SHOULD expose:

- latency
- response time
- p95
- p99
- error rate
- timeout information

---

# 19. Mobile APIs

Mobile testing supports compatible Android/Samsung and iOS/Apple devices.

## 19.1 Device Inventory

`GET /api/v1/projects/{project_id}/mobile/devices`

## 19.2 Device Health/Capabilities

`GET /api/v1/projects/{project_id}/mobile/devices/{device_id}`

## 19.3 Lock Device

`POST /api/v1/projects/{project_id}/mobile/devices/{device_id}/lock`

## 19.4 Unlock Device

`POST /api/v1/projects/{project_id}/mobile/devices/{device_id}/unlock`

## 19.5 Mobile Execution

`POST /api/v1/projects/{project_id}/mobile/executions`

Mobile execution MUST honor capability checks and resource/device locks.

The existing mobile requirement excludes UI/UX cases from this execution path where specified by the source requirements.

---

# 20. Custom Testing APIs

## 20.1 Create Custom Objective

`POST /api/v1/projects/{project_id}/custom-tests`

Request concept:

```json
{
  "module_id": "module_123",
  "feature_id": "feature_123",
  "objective": "Validate recovery behavior after failed payment"
}
```

The backend MUST scope generated tests to the selected module/feature/objective.

---

# 21. Reports APIs

Supported report concepts:

- Bug Report
- Test Report
- Final Audit Report

## 21.1 List Reports

`GET /api/v1/projects/{project_id}/reports`

## 21.2 Generate Report

`POST /api/v1/projects/{project_id}/reports`

Request concept:

```json
{
  "report_type": "TEST_REPORT",
  "scope": "PROJECT",
  "module_ids": []
}
```

## 21.3 Report Status

`GET /api/v1/projects/{project_id}/reports/{report_id}`

## 21.4 Export Report

`GET /api/v1/projects/{project_id}/reports/{report_id}/export`

Supported export formats from the source requirements:

- PDF
- Excel

Reports MUST reconcile with durable execution state.

---

# 22. Model Gateway APIs

The frontend should not directly call individual model providers for AIQA application workflows.

Agents call the Model Gateway.

## 22.1 Model Health

`GET /api/v1/ai/models/health`

## 22.2 Available Models

`GET /api/v1/ai/models`

## 22.3 Runtime Telemetry

`GET /api/v1/ai/models/telemetry`

Telemetry may include:

- model/provider
- task type
- latency
- fallback
- availability
- usage
- configured cost information

## 22.4 Routing Behavior

Routing may consider:

- task type
- context size
- latency
- availability
- configured cost

Local inference is preferred where configured; API fallback may be used for stronger reasoning.

Model/provider failure MUST produce controlled fallback or `NEEDS_REVIEW`.

The system MUST NEVER fabricate an AI result to hide model failure.

---

# 23. Dashboard APIs

Dashboard data must be derived from authoritative backend state.

## 23.1 Project Dashboard

`GET /api/v1/projects/{project_id}/dashboard`

Dashboard may expose:

- project health
- execution counts
- pass/fail ratios
- test counts
- report counts
- quality KPIs
- API latency
- execution reliability
- model health
- worker health

## 23.2 KPI Data

`GET /api/v1/projects/{project_id}/kpis`

Supported KPI concepts include:

- test generation accuracy
- requirement coverage
- edge coverage
- business logic coverage
- RBAC coverage
- security coverage
- API latency
- UI stability
- execution reliability
- model health
- execution efficiency

---

# 24. Settings / Configuration APIs

`GET /api/v1/projects/{project_id}/settings`

`PATCH /api/v1/projects/{project_id}/settings`

Configuration may cover:

- execution
- reporting
- integrations
- AI/model routing
- concurrency
- policy

Security-sensitive configuration changes MUST be audited.

---

# 25. Audit APIs

## 25.1 Audit Log

`GET /api/v1/projects/{project_id}/audit`

Audit records SHOULD cover:

- authentication/security events
- RBAC changes
- policy decisions
- approvals
- agent routing/decisions
- execution actions
- destructive/side-effect actions
- configuration changes
- report generation

---

# 26. Security and Authorization Rules

The following are mandatory:

1. Server-side RBAC.
2. Project isolation.
3. Secrets MUST NOT be embedded in tests or logs.
4. Security executors must be isolated.
5. Destructive DB, production load, production security, and external side effects require explicit authorization/policy.
6. Agent decisions and side effects must be auditable.
7. Unsupported AI conclusions must not become confirmed defects.
8. Frontend authorization checks are UX controls only; backend authorization is authoritative.

---

# 27. Backend ↔ Frontend Responsibility Boundary

## Backend owns

- Business rules
- Authentication
- Authorization
- Project isolation
- Persistence
- API contracts
- Orchestration
- Model Gateway
- Validation
- Execution
- Workers
- DAG/dependencies
- Evidence
- Findings
- Realtime authoritative state
- Reports
- Audit
- Security policy

## Frontend owns

- Navigation
- Screens/views
- User interactions
- Visualizations
- Forms
- Client-side validation
- Loading states
- Empty states
- Error presentation
- Realtime visualization
- Execution visualization
- Accessibility
- Responsive behavior

## Frontend MUST NOT own

- Authoritative business rules
- Authorization decisions
- Test execution state
- Project isolation
- Final pass/fail determination
- AI truth/defect confirmation
- Database state

---

# 28. Frontend Integration Rules

Frontend implementation MUST:

1. Use only documented API contracts.
2. Never hard-code fake production data.
3. Never invent missing backend fields.
4. Handle loading, success, empty, error and `NEEDS_REVIEW` states.
5. Handle expired/invalid authentication.
6. Handle permission denied responses.
7. Handle realtime disconnect/reconnect.
8. Reconcile state after reconnect.
9. Treat backend state as authoritative.
10. Keep API calls behind a typed/service layer rather than scattering raw calls throughout UI components.

---

# 29. Backend Implementation Rules

Backend implementation MUST:

1. Implement the contract exactly.
2. Keep API behavior deterministic where business rules are deterministic.
3. Return structured errors.
4. Validate all client input server-side.
5. Enforce RBAC server-side.
6. Enforce project isolation.
7. Persist durable run/task state.
8. Make event processing idempotent.
9. Never fabricate model results.
10. Keep long-running work asynchronous.
11. Preserve evidence and traceability.
12. Provide tests for contract behavior.

---

# 30. Contract Testing

Before frontend/backend integration is considered complete:

## Backend must verify

- endpoint availability
- request validation
- response schema
- error schema
- authentication
- authorization
- project isolation
- status transitions
- realtime event schema
- event idempotency
- persistence/recovery
- API versioning

## Frontend must verify

- every required API has a consumer
- every response state has UI handling
- every canonical status renders correctly
- error responses render safely
- permission-denied states work
- realtime updates work
- reconnect reconciliation works
- no fake/stub production path remains

## Integration must verify

- login → project selection
- project → context
- context → generation
- generation → validation
- validation → execution
- execution → realtime events
- execution → evidence
- failure → finding/RCA
- run → report
- RBAC live execution
- API telemetry
- mobile device selection/locking
- model gateway telemetry/fallback

---

# 31. Contract Change Policy

Any change to:

- endpoint
- request field
- response field
- enum/status
- event name
- event payload
- authentication behavior
- authorization behavior
- business rule

MUST be treated as an API contract change.

The developer making the change MUST:

1. Update this document.
2. Update backend tests.
3. Update frontend integration.
4. Run contract/integration tests.
5. Record the change in the changelog.

No undocumented breaking change is allowed.

---

# 32. Current Implementation Status

This document defines the target contract.

It does NOT claim that every endpoint above already exists.

Implementation status MUST be tracked separately through:

`AIQA_ACCEPTANCE_MATRIX_v1.0.md`

Each endpoint/module should ultimately be marked:

- NOT_STARTED
- IN_PROGRESS
- IMPLEMENTED
- TESTED
- VERIFIED
- BLOCKED

---

# 33. Final Integration Principle

AIQA follows this rule:

**Business Logic defines behavior.**

**PRD/FRD/SRD defines requirements and architecture.**

**API Contract defines communication.**

**Backend implements authoritative behavior.**

**Frontend consumes the contract.**

**Acceptance Matrix proves completion.**

Neither frontend nor backend should silently invent behavior outside these sources.

The goal is not merely:

`Frontend works`

or:

`Backend works`

The goal is:

`Frontend + API Contract + Backend + Realtime + Execution + Evidence + Reports`

work together as one coherent AIQA system.

---

## Document Control

**Document:** AIQA API Contract  
**Version:** 1.0  
**Status:** Implementation Baseline  
**Depends On:** Business Logic & Architecture v2.0; Consolidated PRD + FRD + SRD v6.0  
**Next Companion Document:** AIQA Acceptance Matrix v1.0
