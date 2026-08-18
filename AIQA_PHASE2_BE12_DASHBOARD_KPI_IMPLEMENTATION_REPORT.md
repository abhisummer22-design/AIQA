# AIQA — Phase 2 — BE-12 Implementation Report

**Unit:** BE-12 — Dashboard / KPI JSON API (Integration Plan milestone table: `BE-12 | Dashboard/KPI JSON | §23.1–23.2 from authoritative state`)
**Mode:** Implementation sprint (no large independent verification pass, per instruction)
**Status: COMPLETE**

## 1. Requirements implemented

- API Contract §23.1 — `GET /api/v1/projects/{project_id}/dashboard`
- API Contract §23.2 — `GET /api/v1/projects/{project_id}/kpis`
- Both project-scoped per the "Actual BE-9" routing pattern (project existence + caller authorization), consistent with every other `/api/v1/projects/:project_id/...` route added this phase.

## 2. Source sections used

- `AIQA_API_CONTRACT_v1.0.md` §23.1/§23.2 — the only two contract-defined endpoints; both described with **permissive** "may expose" / "Supported KPI concepts include" language, not a required field list.
- `AIQA_ACCEPTANCE_MATRIX_v1.0.md` §22/ACC-017 — reinforces the same concept lists, explicitly qualified as "KPI Coverage **(Where implemented)**", and states "Dashboard values MUST come from authoritative backend state."
- `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md` — gap matrix marks Dashboard/KPI `PARTIAL` (`/metrics` only, Prometheus text, no JSON/KPI concepts); Phase 2 item 14 describes the work as JSON endpoints that "replace `/metrics`'s role as UI source" (not a removal of `/metrics` itself); Backend Milestones table confirms scope as "§23.1–23.2 from authoritative state."
- `services/orchestrator/src/store.rs`'s own `Project` DTO doc comment (written during BE-8): "no nested run counts — that's Dashboard's job, §23, not this resource's" — direct in-repo confirmation this aggregation was always meant to live here.

## 3. Scope decision (resolved, not blocked)

§23's concept lists are explicitly permissive ("may expose", "Supported… include", "Where implemented"), so this implementation returns **only the concepts genuinely derivable from real, persisted data** and omits the rest rather than fabricating them. This is not treated as an ambiguity requiring a stop, because the documents themselves already resolve it via that permissive wording — it's a scope decision, made and disclosed here.

**Implemented (real, derivable data):**
- Execution counts by run state (`runs.state`, scoped by `project_id`)
- Task counts by status, pass/fail counts (`tasks` joined to `runs`)
- Test count (`quality_nodes` rows of `node_type='test'` — this backend's only real "test" identity; established by BE-3/intelligence work, not invented here)
- Execution reliability KPI: `PASSED / (PASSED + FAILED + TIMEOUT)`, mirroring `intelligence.rs`'s own product-vs-infrastructure split (INFRA_ERROR/BLOCKED/SKIPPED/CANCELLED excluded — not evidence the thing under test is broken)
- Execution efficiency KPI: average task duration in milliseconds (`tasks.started_at`/`finished_at`)

**Intentionally omitted (no backing data model anywhere in this codebase — fabricating them would violate the no-fake-data rule):**
- `report_counts` — no reports table/entity exists (§21 Reports API is not built)
- `project_health` — no document anywhere defines a health-scoring algorithm; inventing one would be an invented business rule
- `quality_kpis` (as a distinct field) — not a named metric anywhere, would duplicate the `/kpis` endpoint's own real fields
- `api_latency` — confirmed via `metrics.rs`: no latency data is computed or persisted anywhere (only state/status counts)
- `model_health`, `worker_health` — no model gateway or worker-health tracking exists
- `test_generation_accuracy`, `requirement_coverage`, `edge_coverage`, `business_logic_coverage`, `rbac_coverage`, `security_coverage`, `ui_stability` — no requirements/test-generation/RBAC-testing/security-testing/UI entities exist in this backend's data model at all

This matches FE-10's own frontend convention (an unavailable KPI renders as `—`, never `0`) — the backend's honest behavior is to omit the field or return `null`, never a fabricated zero.

## 4. Endpoints and response schema

### `GET /api/v1/projects/{project_id}/dashboard`
```json
{
  "project_id": "uuid",
  "project_name": "string",
  "execution_counts": { "COMPLETED": 3, "PARTIAL": 1 },
  "task_counts": { "PASSED": 5, "FAILED": 2 },
  "pass_fail": { "passed": 5, "failed": 2 },
  "test_count": 4,
  "generated_at": "2026-08-18T00:00:00Z"
}
```
Empty project: `execution_counts`/`task_counts` are `{}`, `pass_fail` is `{passed:0, failed:0}`, `test_count` is `0` — these are real zeros (an empty GROUP BY / COUNT), not fabricated placeholders.

### `GET /api/v1/projects/{project_id}/kpis`
```json
{
  "project_id": "uuid",
  "execution_reliability": 0.71,
  "execution_efficiency_ms": 842.5,
  "generated_at": "2026-08-18T00:00:00Z"
}
```
Both KPI fields are `null` (not `0`) when there is no qualifying data yet — verified by test.

## 5. Data sources / queries

New `Store` methods (`services/orchestrator/src/store.rs`), all project-scoped by binding `project_id`:
- `dashboard_run_counts` — `SELECT state, COUNT(*) FROM runs WHERE project_id = ? GROUP BY state`
- `dashboard_task_counts` — `SELECT t.status, COUNT(*) FROM tasks t JOIN runs r ON r.id = t.run_id WHERE r.project_id = ? GROUP BY t.status`
- `dashboard_test_count` — `SELECT COUNT(*) FROM quality_nodes WHERE project_id = ? AND node_type = 'test'`
- `dashboard_avg_task_duration_ms` — `AVG(julianday delta * 86400000)` over tasks with both `started_at`/`finished_at` set, joined to their owning run's `project_id`

No migration was added — every field is computed from existing tables (`runs`, `tasks`, `quality_nodes`), confirming the gap really was "missing endpoint," not "missing schema."

## 6. Authorization / isolation behavior

Both handlers call the same `require_scoped_project(state, ctx, project_id)` helper Actual BE-9 already established: project must exist (else `404 RESOURCE_NOT_FOUND`), then `auth::require_project_access` (else `403 FORBIDDEN`) — a key scoped to project A gets `403` reading project B's dashboard/kpis even though project B genuinely exists; a global (unscoped) admin key can read any project. All aggregation queries additionally filter by `project_id` at the SQL level, so even if authorization were bypassed the data itself cannot cross projects — verified directly with two real projects carrying different pass/fail outcomes (§8 below).

## 7. Files changed

- `services/orchestrator/src/store.rs` — 4 new read-only aggregation methods, no changes to existing methods.
- `services/orchestrator/src/api.rs` — 2 new handlers (`get_project_dashboard`, `get_project_kpis`) + 2 new response DTOs (`DashboardResponse`, `KpisResponse`) + 2 new routes mounted inside the existing `protected` router, alongside (not replacing) `/metrics`, `/api/v1/projects/:id`, and every other existing route.
- `services/orchestrator/src/api_tests.rs` — 7 new `be12_*` in-process HTTP tests appended.

`/metrics` (`services/orchestrator/src/metrics.rs`) was not touched — read only, to confirm it has no latency data to reuse.

## 8. Tests added (7)

1. `be12_dashboard_and_kpis_reflect_real_run_and_task_data` — real aggregation + status/count accuracy: one project, one run with one PASSED + one FAILED task; asserts execution/task/pass-fail counts, `test_count == 2`, `execution_reliability == 0.5`.
2. `be12_dashboard_and_kpis_on_empty_project_show_honest_absence_not_fake_zero` — empty/new project: counts are real `0`/`{}`, KPI ratios are `null`, not fabricated.
3. `be12_dashboard_and_kpis_do_not_leak_across_projects` — two projects, distinct real outcomes (one pass, one fail); each project's own dashboard/kpis reflect only its own data (`reliability` 1.0 vs 0.0).
4. `be12_dashboard_and_kpis_reject_unauthorized_cross_project_access` — a project-A-scoped key gets `403` on project B's dashboard/kpis; a global admin key succeeds.
5. `be12_dashboard_and_kpis_nonexistent_project_returns_structured_not_found` — phantom project id → `404 RESOURCE_NOT_FOUND` envelope, both endpoints.
6. `be12_dashboard_malformed_project_id_returns_structured_validation_error` — non-UUID path segment → `400 VALIDATION_ERROR` envelope (BE-10's extractor-rejection path).
7. `be12_dashboard_and_kpis_require_authentication` — no credentials → `401 UNAUTHENTICATED` envelope, both endpoints.

## 9. Targeted test result

```
running 7 tests
test api_tests::be12_dashboard_and_kpis_require_authentication ... ok
test api_tests::be12_dashboard_and_kpis_nonexistent_project_returns_structured_not_found ... ok
test api_tests::be12_dashboard_and_kpis_on_empty_project_show_honest_absence_not_fake_zero ... ok
test api_tests::be12_dashboard_and_kpis_reject_unauthorized_cross_project_access ... ok
test api_tests::be12_dashboard_malformed_project_id_returns_structured_validation_error ... ok
test api_tests::be12_dashboard_and_kpis_reflect_real_run_and_task_data ... ok
test api_tests::be12_dashboard_and_kpis_do_not_leak_across_projects ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 62 filtered out
```

## 10. `cargo check --all-targets`

Clean — no warnings, no errors.

## 11. Full regression (`cargo test`)

```
running 69 tests
...
test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.75s
```
62 pre-existing tests (D1–D5, B-series, BE-8, BE-9 errors, BE-10, Actual BE-9) + 7 new BE-12 tests, all passing. No existing test was modified.

## 12. Ambiguity / blockers encountered

None that stopped implementation. The only judgment call — which §23 "may expose" concepts to implement vs. omit — is documented in full in §3 above and resolved via the contract's own permissive wording ("may expose", "Where implemented"), not invented.

## 13. Explicit out-of-scope (not touched)

BE-13, BE-14, BE-15, G-AUTHN, G-WS, any frontend code (`forentend/`, `apps/web/`). `/metrics` unchanged. No new migration. No changes to authorization logic, error envelope, or any existing route's behavior.

## 14. Final status

**BE-12 — COMPLETE.** Both §23.1 and §23.2 endpoints are implemented, project-scoped, backed entirely by real persisted state, with 7 targeted tests and a clean 69/69 full regression. Stopping here per instruction — no automatic start of the next milestone.
