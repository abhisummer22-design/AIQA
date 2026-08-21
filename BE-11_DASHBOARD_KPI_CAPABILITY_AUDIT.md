# BE-11 — Dashboard KPI Capability Audit

## Status

**AUDIT COMPLETE — Mixed GO/STOP.** No files were modified. One candidate is genuinely implementable with today's schema (Average Run Duration — a real, evidence-backed, low-risk extension of an already-shipped pattern). Every other examined missing-KPI concept is a **STOP**: either the required source data does not exist at all (test coverage, requirement traceability, health scoring, latency/model-gateway telemetry), or it would require a genuine, non-trivial design decision beyond what a fresh source-code audit can respons­ibly settle in place of a product owner (time-bucketed trend series, project-level flakiness/risk aggregation, run-frequency windowing).

## Audit Objective

Determine, from the current repository only, which Dashboard KPI concepts are real, which are missing, whether any missing concept can be truthfully computed from data that already exists, and whether any implementation is justified — without inventing a feature merely to produce one.

## Fresh Repository Verification

Every conclusion below is from a direct re-read performed this session, not from FE-10/FE-19/FE-21/FE-25's own prior reports (though their conclusions are cross-referenced where they still hold):

- `services/orchestrator/src/api.rs` lines 902-1019 (`DashboardResponse`, `KpisResponse`, `get_project_dashboard`, `get_project_kpis`) — read in full.
- `services/orchestrator/src/store.rs` lines 446-497 (`dashboard_run_counts`, `dashboard_task_counts`, `dashboard_test_count`, `dashboard_avg_task_duration_ms`) — read in full, exact SQL inspected.
- `services/orchestrator/migrations/0001_core_execution.sql` through `0004_projects.sql` — read in full (all 4 files; this is the complete schema, no other migration files exist).
- `forentend/src/components/pages/DashboardPage.tsx` — read in full (734 lines).
- `forentend/src/components/dashboard/ProjectExecutionGraph.tsx` — read in full; the module doc comment's claim ("always renders its own real, honest empty state") was independently verified against the actual code, not trusted: `DashboardPage.tsx` always calls it with `timelineData={[]}`, so `hasData` is always `false` and the empty-state branch is the only one ever reachable today. **Confirmed true, not merely asserted.**
- `forentend/src/types/contract.ts` lines 294-311 (`DashboardResponse`, `KpisResponse`) — field-for-field matches the Rust structs exactly.
- `forentend/src/components/pages/DashboardPage.test.tsx` — 32 existing test items enumerated (see §Testing Requirements) — extensive, already covers empty/error/isolation/realtime/regression states.
- `domain.rs::EventType` (re-confirmed via `RealtimeContext.tsx`'s own already-verified doc comment, itself sourced from every `record.emit(...)` call site in `engine.rs`) — no dashboard-specific or KPI-specific event type exists; only `RUN_SCHEDULED`, 9× `TASK_*`, `RUN_COMPLETED`.

## Current Dashboard KPI Inventory

| KPI | Frontend Field | Backend Source | Real Calculation | Project Scoped | Realtime | Status |
|---|---|---|---|---|---|---|
| Total Test Cases | `dashboard.test_count` | `store.rs::dashboard_test_count` | `SELECT COUNT(*) FROM quality_nodes WHERE project_id=? AND node_type='test'` | Yes | Yes (debounced any-event resync) | **REAL** — genuinely 0 today for most projects since Test Generator doesn't persist cases; that's the honest value, not a bug |
| Active Runs | `dashboard.execution_counts['RUNNING']` | `store.rs::dashboard_run_counts` | `SELECT state, COUNT(*) FROM runs WHERE project_id=? GROUP BY state` | Yes | Yes | **REAL** |
| Passed / Failed / Blocked / Needs Review / Skipped / Running / Queued (task counts) | `dashboard.task_counts[...]` | `store.rs::dashboard_task_counts` | `SELECT t.status, COUNT(*) FROM tasks t JOIN runs r ON r.id=t.run_id WHERE r.project_id=? GROUP BY t.status` | Yes | Yes | **REAL** |
| Execution Reliability | `kpis.execution_reliability` | `get_project_kpis` (inline) | `passed / (passed+failed+timeout)`, `null` if denominator is 0 | Yes | Yes | **DERIVED FROM REAL DATA** |
| Execution Efficiency (avg task duration) | `kpis.execution_efficiency_ms` | `store.rs::dashboard_avg_task_duration_ms` | `AVG((julianday(finished_at)-julianday(started_at))*86400000)` over tasks with both timestamps | Yes | Yes | **DERIVED FROM REAL DATA** |
| Data as of (`generated_at`) | `dashboard.generated_at` | `get_project_dashboard` (inline, `chrono::Utc::now()`) | Real server timestamp of the response itself | N/A | N/A | **REAL** |
| Project Status badge (`Running`/`Healthy`) | client-derived | derived from `runs.find(r => r.state === 'RUNNING')` | Real boolean over already-fetched `RunListItem[]` | Yes | Yes | **DERIVED FROM REAL DATA** — deliberately excludes any invented Warning/Degraded/Critical/Offline tier (no health-scoring model exists; the file's own comment discloses this) |
| Current Execution Run panel (started/elapsed/executed/progress) | client-derived from `RunDetail` | `aiqaApi.executions.get` (FE-8, unmodified) | Real fields (`started_at`, per-status counts, `total`), `elapsed` computed client-side from real `started_at` | Yes | Yes | **DERIVED FROM REAL DATA** — no fabricated ETA |
| Execution Activity timeline chart | `ProjectExecutionGraph` | none | Always called with `timelineData=[]}` | N/A | N/A | **MISSING**, honestly disclosed as its own real empty state, never populated by invented data — verified directly, not just claimed |

**No FABRICATED KPI was found anywhere in current production code** (see §Fake/Stub/Fabrication Audit for the full search).

## Backend → Frontend KPI Data Flow

Every real/derived KPI above follows the identical, already-verified chain:
```
SQLite (runs/tasks/quality_nodes tables, real project_id columns)
  → store.rs (parameterized, project-scoped SQL, one function per concept)
  → api.rs (require_scoped_project gate, then one of two handlers)
  → DashboardResponse / KpisResponse (Serialize structs, field-for-field typed)
  → aiqaApi.ts (dashboard.get / dashboard.kpis, unmodified since FE-1)
  → types/contract.ts (DashboardResponse / KpisResponse, exact field match)
  → DashboardPage.tsx (renders exactly these fields, no transformation beyond null-safe formatting)
```
No link in this chain was found broken, mismatched, or silently estimating.

## Missing KPI Inventory

Candidates evaluated against the illustrative list in the prompt, classified by actual repository evidence:

| Candidate | Evidence for real product need | Source data exists? | Verdict |
|---|---|---|---|
| **Average Run Duration** | `RunSummary.started_at`/`finished_at` are real, already-typed, already-populated fields on every run (`domain.rs::RunSummary`, `store.rs::set_run_finished`); `dashboard_avg_task_duration_ms` already establishes the exact identical pattern one level down (task, not run) | **YES — fully** (`runs.started_at`/`runs.finished_at` columns, migration `0001`) | **Category A candidate — see §GO/STOP** |
| Execution Success Rate (run-level, not task-level) | `dashboard.execution_counts` already returns real counts by `RunState` (`COMPLETED`/`PARTIAL`/`FAILED`/`CANCELLED`/`REJECTED`/etc.) in the response **already being fetched today** | YES, already returned | **Not a backend gap at all** — this is a pure frontend-derivation opportunity from data already in hand; out of scope for a *backend* capability audit. Documented here so it is not later mistaken for a missing backend concept. |
| Defect Rate (Findings confirmed / total Findings) | `bug_drafts`/`failure_clusters` already real and project-scoped (`failure_clusters.project_id`); `GET .../findings` already returns this data — **already fetched by `ReportsPage.tsx`, just not by Dashboard** | YES, already returned by an existing endpoint | **Not a backend gap** — would require Dashboard to additionally call the existing `findings.list` endpoint (a frontend integration decision), not new backend work. Out of BE-11's backend-only scope. |
| Pass/Fail Trend, Execution Trend, Test-Case Growth, Failure Trend (any time-bucketed series) | `ProjectExecutionGraph.tsx` component already exists and is UI-ready; `DashboardPage.tsx`'s own doc comment explicitly discloses "the real backend has no time-bucketed history endpoint at all (only aggregate counts)" — verified true by direct code read | **PARTIAL** — raw timestamps (`runs.created_at`/`started_at`/`finished_at`, `tasks.started_at`/`finished_at`) exist, but no query buckets them by time window today | **STOP** — building this requires a genuine, non-trivial design decision (bucket size: hourly/daily? timezone handling? gap-filling for empty buckets — the module doc comment explicitly names "gap-filling" as something this backend does not define) that a source-code audit cannot responsibly invent on a product owner's behalf. Real backend work, out of BE-11's audit-only scope to specify further than "genuinely possible, not yet designed." |
| Test Stability / Flakiness (project-level) | `flakiness_metrics` table is real and populated (per-test, `intelligence.rs::update_test_intelligence`, already exposed via BE-10/FE-22's `TestIntelRow.flakiness_score`) | **PARTIAL** — per-test data exists; no project-level aggregate query exists | **STOP for Dashboard specifically** — aggregating N per-test flakiness scores into one "project flakiness" number requires a disclosed methodology choice (simple average? weighted by execution count? worst-N?) that does not yet have product/repository evidence for one specific formula. Inventing one would be exactly the "casual formula" the audit forbids. (Already real and already exposed per-test on `ReportsPage.tsx`'s Test Intelligence section — no gap exists there, only at a *project-aggregate* level for Dashboard.) |
| Risk Score (project-level) | Same as flakiness — `risk_assessments` is real, per-test, already exposed via `TestIntelRow.risk_score` | **PARTIAL**, same reasoning | **STOP**, same reasoning as flakiness |
| Run Frequency (runs per day/week) | `runs.created_at` is real | **YES**, raw data exists | **STOP for now** — technically computable, but "frequency" requires a defined window (daily? weekly? rolling 7-day?) with no repository or product evidence pointing to one specific answer; a smaller, more clearly-scoped case than the trend-series candidates but still a genuine design decision, not a mechanical extension of an existing pattern the way Average Run Duration is. |
| Test Coverage | No requirement-traceability entity exists anywhere in the schema (re-confirmed: no `requirements`/`coverage` table in any of the 4 migrations) | **NO** | **STOP** — no backing data model, matches the already-disclosed `TestCaseValidatorPage.tsx`/`api.rs` doc-comment finding, unchanged |
| Project Health Score | No scoring model, no thresholds table, explicitly disclosed as "not backend-computable" in `DashboardPage.tsx`'s own already-verified comment | **NO** | **STOP** — already correctly avoided by the current `'Running'`/`'Healthy'` binary status, which is real (derived from an actual RUNNING run) rather than an invented tiered score |
| API Latency / Model/Worker Health / UI Stability / Report Counts | Explicitly named in `api.rs`'s own `DashboardResponse` doc comment as having "no backing data model anywhere in this codebase (no reports table, no test-generation/requirements entities, no latency telemetry, no model gateway)" | **NO** | **STOP**, unchanged from BE-12's original documented decision |

## Evidence For Each Missing KPI

Consolidated above per-candidate; the single strongest, cleanest piece of evidence in the entire audit is **Average Run Duration**: `RunSummary` (the same struct BE-10/FE-20 already fully exposed via the 11-field count grid) already carries real `started_at: DateTime<Utc>` and `finished_at: Option<DateTime<Utc>>` on every run, and `store.rs::dashboard_avg_task_duration_ms` is a byte-for-byte-adaptable template one join simpler (it operates directly on `runs`, no join to `tasks` needed at all).

## Database / Data Availability

For **Average Run Duration** specifically (the one candidate carried into §GO/STOP):

- A. Database data already exists? **YES** — `runs.started_at`, `runs.finished_at` (migration `0001_core_execution.sql`, both real `TEXT` (RFC3339) columns, already populated by `engine.rs`/`store.rs::set_run_finished`).
- B. Store already exposes it? **NO** (no `dashboard_avg_run_duration_ms`-equivalent function exists yet — but the pattern to add one is already proven, line-for-line, by `dashboard_avg_task_duration_ms`).
- C. Domain model already contains it? **YES** — `RunSummary.started_at`/`.finished_at` already real, already serialized.
- D. Existing API can calculate it? **NO**, not exposed on `KpisResponse` yet — would need one new field.
- E. New database migration required? **NO**.
- F. New backend calculation required? **YES**, but trivial — one new `store.rs` function, structurally identical to the existing one.
- G. New API route required? **NO**.
- H. Existing API can be extended? **YES** — `KpisResponse` (already `Option<f64>`-typed, already nullable-by-convention) is the correct, logical home; no backward-compatibility break (additive field only).

## Calculation Logic

**Average Run Duration** (the only candidate reaching this section — every other candidate is a STOP with no formula to responsibly finalize):

```
avg_run_duration_ms =
  AVG((julianday(r.finished_at) - julianday(r.started_at)) * 86400000.0)
  FROM runs r
  WHERE r.project_id = ?
    AND r.started_at IS NOT NULL
    AND r.finished_at IS NOT NULL
```
- **Source fields**: `runs.started_at`, `runs.finished_at` — both already real, both already required non-null for a terminal run.
- **Numerator**: sum of per-run elapsed milliseconds, computed identically to the existing task-duration query (same `julianday` arithmetic, already proven correct in production).
- **Denominator**: count of runs with both timestamps present (implicit in `AVG`).
- **Zero handling**: if no run has both timestamps, `AVG` returns SQL `NULL` → `Option<f64>` → `None` → frontend already has the exact `formatMs(null) → '—'` convention in place, matching `execution_efficiency_ms`'s existing null handling precisely — no new frontend logic needed either.
- **Null handling**: a run that is still in-flight (`finished_at IS NULL`) is correctly excluded, not treated as 0 duration — matches the existing task-duration query's own exclusion rule exactly.
- **Cancelled runs**: included if they reached `finished_at` (cancellation is a real terminal transition that sets `finished_at`, confirmed via `engine.rs`'s cancellation path) — a cancelled run's elapsed wall-clock time is real data, not misleading to include; this mirrors the task-duration query's own treatment (no `state`/`status` filter there either, just presence of both timestamps).
- **Failed runs**: included on the same basis — same reasoning as cancelled.
- **Incomplete/non-terminal runs**: excluded via the `finished_at IS NOT NULL` predicate, same as the existing pattern.
- **Historical window**: unbounded (all-time), matching every other existing Dashboard/KPI query — no windowing exists anywhere in the current KPI surface, so introducing one here alone would be an inconsistent, undocumented deviation.
- **Project scope**: `WHERE r.project_id = ?`, identical binding pattern to every other `dashboard_*` function.

This is the **only** formula in this audit presented as ready-to-implement; it is a direct, one-for-one structural copy of an already-shipped, already-tested pattern — not a new calculation being invented.

## Project Isolation Audit

Every existing `dashboard_*` store function was re-inspected: `dashboard_run_counts`, `dashboard_task_counts` (via `JOIN runs r ON r.id = t.run_id WHERE r.project_id = ?`, since `tasks` carries no `project_id` of its own — same join `task_history_by_name` already uses), `dashboard_test_count`, `dashboard_avg_task_duration_ms` — **all four** bind `project_id` as a real SQL parameter in their `WHERE` clause; none aggregates across projects, none accepts an unscoped/global query path. Both handlers (`get_project_dashboard`, `get_project_kpis`) call `require_scoped_project(&state, &ctx, project_id)` as their first line — a scoped key mismatched against the URL's `project_id` gets a real `403` before any query runs; a nonexistent project gets a real `404`. **Project A data cannot contribute to Project B's KPI values** — verified structurally (every query has an explicit, bound `project_id` filter) and consistent with `DashboardPage.test.tsx`'s own existing item 4 ("Project A metrics never appear under Project B") already passing. The proposed `dashboard_avg_run_duration_ms` would use the identical `WHERE r.project_id = ?` binding — no new isolation risk.

## Realtime Audit

- **Total Test Cases / Active Runs / task counts / reliability / efficiency**: currently `Refresh after any real event` (debounced, `EVENT_RESYNC_DEBOUNCE_MS = 400` in `DashboardPage.tsx`) — a deliberate, already-justified choice (the file's own comment: "any real event for this project could change these aggregate counts... dashboard/kpis are aggregate SQL queries, not derivable from one event's payload"). Not `RUN_COMPLETED`-only like FE-25's Test Intelligence resync, because task-level counts (`RUNNING`, `QUEUED`, etc.) genuinely do change on non-terminal events too — this is a correct, evidence-based difference from FE-25's tighter filter, not an inconsistency.
- **Proposed Average Run Duration**: would only ever change at a run's `finished_at` transition — i.e., exactly `RUN_COMPLETED` (or a `RunState` becoming terminal) — but since it would live on the same `KpisResponse` object already refetched by the existing any-event debounced resync, **no new realtime wiring is needed at all**; it rides along for free on the existing subscription.
- **Execution Activity timeline** (if ever built): would be `Refresh after RUN_COMPLETED` at minimum, likely `Periodic aggregation` for real-time bucket boundaries — but this entire concept is a STOP this audit, so no realtime design is finalized for it.

## API Contract Audit

Comparing current vs. a hypothetical extension for Average Run Duration:
- **Current `KpisResponse`**: `{ project_id, execution_reliability: Option<f64>, execution_efficiency_ms: Option<f64>, generated_at }`.
- **Desired**: add one field, e.g. `execution_avg_run_duration_ms: Option<f64>`.
- **Backward compatibility**: additive-only, `#[derive(Serialize)]` struct — no existing consumer breaks; `KpisResponse` has no `#[serde(deny_unknown_fields)]` equivalent on the response side (it's server→client only) so this is a pure superset change.
- **Nullability**: matches `execution_efficiency_ms`'s existing `Option<f64>` convention exactly — no precedent-breaking decision needed.
- **Naming**: `execution_avg_run_duration_ms` (or similar) would need to be picked to avoid ambiguity with the existing `execution_efficiency_ms` (task-level) — a real naming decision, but a small, low-risk one with an obvious existing-convention answer (`_ms` suffix, `execution_` prefix already established).
- **Project scope**: unchanged, same handler, same `require_scoped_project` gate.
- **Frontend contract**: `types/contract.ts::KpisResponse` would need the same one-field addition — but this audit does not modify frontend code and takes no position on when/whether a future frontend unit consumes it.

**Preferred design, if implemented**: extend `KpisResponse` (not `DashboardResponse`, not a new endpoint) — it is the logically correct home (a computed KPI, not a raw count), and no new route is justified when an existing one already serves the identical shape of concept.

## Performance Audit

For the one implementable candidate: one additional `AVG(...)` query over `runs` (already indexed implicitly by `PRIMARY KEY (id)`; filtering is by `project_id`, an unindexed `TEXT` column — same as every other existing `dashboard_*` query, none of which have a dedicated index either). Query count: +1 per `/kpis` call (currently 1 query; would become 2). No join, no subquery, no N+1 risk. Should run in Rust-triggered SQL (as every sibling query already does), not client-side — consistent with the existing architecture. **No caching need is evidenced** — the existing KPI queries are already simple aggregates over a per-project row count that, per this repository's own manual-verification history across FE-17 through FE-25, tops out in the tens to low hundreds of runs per project; no evidence of a performance problem exists anywhere in this repository (test suites, manual verification logs) to justify recommending an index or cache that isn't already proven necessary for the sibling queries it would exactly mirror.

## Security Audit

Unchanged from the existing, already-verified model: `require_scoped_project` enforces authentication + project authorization before any query executes; every `dashboard_*` query is parameterized (`sqlx::query(...).bind(project_id)`), no string concatenation, no SQL-injection surface; a scoped Operator/Viewer key cannot reach another project's aggregate (403), an Admin/global key can reach any real project explicitly named in the URL (by design, same as every other scoped endpoint) but never an aggregate spanning multiple projects at once (no such aggregation code path exists anywhere in `dashboard_*`). No new security surface would be introduced by the one implementable candidate — it reuses the identical authorization/scoping code path unchanged.

## Fake/Stub/Fabrication Audit

```
grep -RniE "fake|mock|demo|sample|fabricated|hardcoded|Math\.random" components/pages/DashboardPage.tsx components/dashboard/
```
Every match is a doc comment documenting a **prior** fabrication that was **already removed** (fabricated health score, fabricated ETA, fabricated relative-time "Last Activity", fabricated `AIQAContext` project list, fabricated `RunTriggerModal`) — zero live fabrication in current production code. Independently verified (not merely grepped): `ProjectExecutionGraph.tsx`'s empty-state branch is the only reachable one today (§Fresh Repository Verification). No hardcoded numbers, no seeded statistics, no fake percentages, no placeholder KPI values, no invented thresholds, no fake trend data exist anywhere in the current Dashboard implementation. Empty/loading/error states all use the same shared `StateCard`/`EmptyState`/`renderResourceErrorState` components every other real page uses — no bespoke fallback-number logic exists to audit separately.

## Testing Requirements

If Average Run Duration were implemented, the following would be required (not implemented in this audit-only session):
- `store.rs` unit test: a project with 2+ runs having both timestamps → correct average; a project with only in-flight runs (`finished_at IS NULL`) → `None`, never `0`.
- `api_tests.rs`: `GET .../kpis` returns the new field with a real value for a project with completed runs; `null` for an empty project; project-isolation test (Project A's runs never influence Project B's average) — mirroring the exact pattern `phase4_be10_intelligence_isolated_across_projects` already established for BE-10.
- Mixed execution-state test: a project with a mix of COMPLETED/PARTIAL/FAILED/CANCELLED/REJECTED runs — confirm all terminal runs with both timestamps are included, exactly per §Calculation Logic.
- Zero-run / empty-project test: confirm `null`, not `0`.
- Regression: full `cargo test` suite (currently 101 passing) must remain green.
- Frontend (a separate future unit, not this audit): a new KPI-card test mirroring `DashboardPage.test.tsx`'s existing item 12/13 pattern, plus a null-handling test mirroring item 16.

## Candidate Implementation Scope

Only **Average Run Duration** reaches this section.

## GO / STOP Decision

**GO — Average Run Duration only.**
- KPI requirement is supported by direct evidence (`RunSummary.started_at`/`.finished_at` already real; `dashboard_avg_task_duration_ms` already proves the identical pattern one level down).
- Source data exists today, no migration needed.
- Calculation is unambiguous (§Calculation Logic, a direct structural copy of an existing, already-shipped, already-tested formula).
- Project isolation is already guaranteed by the same binding pattern every sibling query uses.
- API design is clear (extend `KpisResponse`, additive field, no new route).
- Implementation scope is controlled (one new `store.rs` function + one new struct field + one existing-handler call — no new tables, no new routes, no new auth path).

**STOP — every other examined candidate.**
- **Pass/Fail/Execution/Failure trend series, Test-Case Growth**: real raw timestamp data exists, but the required time-bucketing/gap-filling semantics have no product or repository evidence pointing to one specific design — implementing any one choice would mean inventing business logic, explicitly forbidden by this audit's own rules.
- **Test Stability/Flakiness and Risk Score at the project (aggregate) level**: real per-test data exists and is already exposed (BE-10/FE-22), but a project-level aggregation formula (simple average vs. weighted vs. worst-N) is not evidenced anywhere in the repository — inventing one would misrepresent a specific methodology as the definitive one without product sign-off.
- **Run Frequency**: real raw data exists, but the required time window is undefined — same class of problem as the trend series, smaller in scope but still a genuine, unresolved design decision.
- **Test Coverage, Project Health Score, API Latency, Model/Worker Health, UI Stability, Report Counts**: no backing data model exists anywhere in the current schema (re-confirmed against all 4 migration files) — these remain correctly, explicitly disclaimed by the backend's own `DashboardResponse` doc comment, unchanged since BE-12.
- **Execution Success Rate (run-level) and Defect Rate**: not backend gaps at all — both are computable today from data the frontend *already fetches* (`dashboard.execution_counts`, and the existing `findings.list` endpoint respectively); any implementation here would be a frontend integration decision, out of a backend-capability audit's scope, and not something BE-11 should recommend as backend work.

## Files Likely To Change (if Average Run Duration is implemented in a future unit)

- `services/orchestrator/src/store.rs` — new `dashboard_avg_run_duration_ms` function.
- `services/orchestrator/src/api.rs` — `KpisResponse` struct (+1 field), `get_project_kpis` handler (+1 store call).
- `services/orchestrator/src/api_tests.rs` — new tests per §Testing Requirements.
- (A separate future frontend unit, not part of this backend audit's scope, would then touch `forentend/src/types/contract.ts::KpisResponse` and `DashboardPage.tsx`.)

## Files That Must NOT Change

`services/orchestrator/migrations/**` (no migration needed for the one GO candidate), `domain.rs` (no domain type change needed — `KpisResponse` lives in `api.rs`, not `domain.rs`), `engine.rs`, `intelligence.rs`, `auth.rs`, any route registration in `api.rs::router` (no new route), any frontend file (this is a backend-only audit; §22 forbids modification either way this session).

## Dependencies

None — the one GO candidate depends on nothing not already shipped (`runs.started_at`/`.finished_at` have existed since migration `0001`, the very first migration in this repository).

## Risks

Low for the one GO candidate: additive-only API change, structurally identical to an already-proven query, no new authorization path, no new route. No risk identified for any STOP candidate since none would be implemented.

## Final Recommendation

Implement **only** Average Run Duration, as a small, future backend unit extending `KpisResponse` — the smallest, most evidence-backed, lowest-risk closure of a genuine (if minor) Dashboard KPI gap. Do not implement any trend/time-series KPI, any project-level flakiness/risk aggregate, or run-frequency metric without first obtaining an explicit product/design decision on the open questions identified above (bucket size, aggregation methodology, time window) — implementing any of them today would mean this audit inventing business logic on the product owner's behalf, which is exactly what it was instructed not to do.

## Recommended Next Unit

**BE-12b (naming placeholder) — Average Run Duration KPI**, backend-only, scoped exactly as described in §Candidate Implementation Scope. This is the only next unit this audit can responsibly recommend without inventing scope; if the product owner instead wants to prioritize a trend-series/aggregate-flakiness Dashboard feature, that requires a product-design decision first (bucket size, aggregation formula, time window) — a prerequisite this audit cannot supply on its own, consistent with its instruction to prefer **STOP** over inventing a KPI just to create the next unit.
