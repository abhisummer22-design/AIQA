# AIQA Skills Audit

Ugg audit skills. Skills not do magic. Skills just word for magic box (AI).

## Built-in Skills (8)

| ID | Name | Category | Prompt Instruction | Backend Function | Consumers | Classification |
|---|---|---|---|---|---|---|
| func-1 | Functional Testing | Functional | Happy/alternate paths, business rules | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |
| edge-1 | Edge Case Analysis | Functional | Boundary values, null/empty, concurrency | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |
| sec-1 | Security Testing | Security | Auth, injection, session, data exposure | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |
| perf-1 | Performance Testing | Non-Functional | Response-time, load, stress, spike | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |
| api-1 | API Testing | Technical | Request/response contract, status codes | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |
| db-1 | Database Testing | Technical | CRUD, constraints, relationships | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |
| rbac-1 | RBAC Testing | Security | Role/permission boundaries | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |
| biz-1 | Business Case Testing | Business | End-to-end business rules, workflows | resolve_skill_instructions() | TestCaseGeneratorPage | CONFIGURATION LABEL |

## MCP Audit
- Registry: UNBUILT
- Server Discovery: UNBUILT
- Connection: UNBUILT
- Authentication: UNBUILT
- Tool Discovery: UNBUILT
- Tool Invocation: UNBUILT
- Classification: ENTIRELY UNBUILT. Placeholder only.

## Custom Skills
Custom skills only live in session state. Local only. No database save. Not real.
