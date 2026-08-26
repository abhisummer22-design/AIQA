# AIQA — Phase 4 — BE-10 Implementation Report

*(This "BE-10" is a distinct unit from the pre-existing Phase 2 "BE-10" documented in `errors.rs`/`api_tests.rs`, which closed an extractor-rejection error-envelope gap. That earlier unit is untouched by this work; the naming collision is coincidental across two different numbering sequences and is called out explicitly in this session's code comments to avoid confusing future readers.)*

## 1. Status

**PASS.**

## 2. Objective

Expose the real, already-computed Test Intelligence capability (`TestIntelRow`: risk/value/flakiness scoring per test) through a proper project-scoped route — `GET /api/v1/projects/:project_id/intelligence/tests` — so a future frontend unit (FE-22) can consume it truthfully, closing the exact gap FE-21 identified: the legacy `/intelligence/tests` route scopes itself to the caller's key-bound `ctx.project_id` rather than an explicit URL parameter, which is unsafe for this frontend's multi-project-per-key architecture.

## 3. Fresh Audit

Performed before any code change, per §5 of the prompt:

- **Intelligence** (`intelligence.rs`): confirmed `ingest_run` is real, live production code — called from `engine.rs:705` (`intel.ingest_run(record.project_id.as_deref(), &record.plan, &tasks_snapshot)`) immediately after every run reaches a terminal state, using the run's own real `project_id`. `update_test_intelligence` unconditionally computes and persists a risk assessment, a test-value assessment, and a flakiness metric for every terminal task on every run — never leaves these fields null after at least one completed run for that test name.
- **Persistence** (`store.rs::list_test_intelligence`, lines 978-1012): confirmed it already accepts `project_filter: Option<&str>` and, when given `Some(pid)`, runs a real, explicit `WHERE n.project_id = ?` bound to that exact value — no rewrite needed. The `None => "default"` fallback exists only for the legacy route's own (different) calling convention and is never exercised by the new route, which always passes `Some(...)`.
- **Routing** (`api.rs`): confirmed the router's own comment block (lines 74-76) explicitly documents `/intelligence/tests` as one of the "Legacy, flat, unscoped routes" deliberately excluded from the project-scoped overhaul — corroborating FE-21's finding rather than contradicting it. Found the exact template to mirror: `list_findings_scoped` (`require_scoped_project(&state, &ctx, project_id).await?;` then a single store call passing `Some(project_id.to_string().as_str())`).
- **Authorization** (`auth.rs`): confirmed `require_project_access` (lines 154-165) is the real multi-tenancy gate — `None` (admin/global key) always passes, `Some(pid)` only passes when it matches the resource's real project, otherwise 403 `FORBIDDEN`. Confirmed `api.rs::require_scoped_project` (lines 158-167) is the existing composite helper (404 if the project doesn't exist, then the 403 check) already used by every scoped GET route (`get_run_scoped`, `get_project_dashboard`, `get_project_kpis`, `list_findings_scoped`, `get_audit_log_scoped`).
- **Contracts**: confirmed `TestIntelRow` (`store.rs:1388-1399`) already derives `Serialize` and needs no new type — the existing legacy handler already returns `Json<Vec<TestIntelRow>>` directly, so the new handler does the same.

No STOP condition (§18) was triggered — the data model already associates intelligence with real project ids with no ambiguity, authorization already has a project-access mechanism, project isolation is reliably enforceable, no migration is needed, the existing scoped-route architecture is directly reusable, no new calculation or fake data was needed, and FE-21's "real/populated" claim about `TestIntelRow` was independently reconfirmed, not found incorrect.

## 4. Existing Intelligence Capability

- **`TestIntelRow`** (`store.rs:1388-1399`): `id: Uuid`, `name: String`, `risk_score: Option<f64>`, `failure_rate: Option<f64>`, `dependency_count: Option<i64>`, `execution_cost_ms: Option<f64>`, `value_score: Option<f64>`, `flakiness_score: Option<f64>`, `window_runs: Option<i64>`, `window_failures: Option<i64>`.
- **Computation path**: `engine.rs` (run completes) → `Intelligence::ingest_run` (`intelligence.rs:126`) → `update_test_intelligence` per terminal task → `store::upsert_quality_node` / `upsert_risk_assessment` / `upsert_test_value` / `upsert_flakiness`. A deterministic, rule-based (non-LLM) weighted scoring model, documented in `intelligence.rs`'s own module comment as *"most of the PRD's actual differentiators."*
- **Persistence**: `quality_nodes` (one row per test, `project_id` + `node_type='test'` + `name`), joined with `risk_assessments`, `test_value_assessments`, `flakiness_metrics` (each keyed by `node_id`).
- **Existing endpoint**: `GET /intelligence/tests` (legacy, unscoped — untouched by this unit).
- **New endpoint**: `GET /api/v1/projects/:project_id/intelligence/tests`.

## 5. Route Added

```
GET /api/v1/projects/:project_id/intelligence/tests
```
Registered in `api.rs::router` immediately after the existing `/api/v1/projects/:project_id/audit` route, in the same protected-router group (behind `require_auth`), following the exact ordering convention already used for every other scoped resource.

## 6. Authorization

`list_test_intelligence_scoped` calls `require_scoped_project(&state, &ctx, project_id).await?` as its first line — the same helper every other scoped GET handler in this file uses:
1. `state.store.get_project(project_id)` — 404 `RESOURCE_NOT_FOUND` if the project doesn't exist.
2. `auth::require_project_access(ctx, Some(project_id.to_string().as_str()))` — 403 `FORBIDDEN` if the caller's key is bound to a different project; always passes for an unscoped/admin key or a key bound to exactly this project.

No new authorization code was written; no existing authorization code was modified.

## 7. Project Isolation

The query is never given the caller's own bound project or the `"default"` tenant — it is always given the real, URL-supplied `project_id`:
```rust
state.store.list_test_intelligence(Some(project_id.to_string().as_str()))
```
Since `list_test_intelligence`'s `Some(pid)` branch runs a real, parameterized `WHERE n.project_id = ?` against `quality_nodes` (joined to the three assessment tables), and every real project-scoped run tags its intelligence with its own real `project_id` (traced in §3), a request for Project A's intelligence can only ever return rows whose `quality_nodes.project_id` genuinely equals Project A's id. This was verified with a test that goes beyond structural checking (§12, Test 2): two projects are given tasks with opposite outcomes (Project A always passes, Project B always fails) so their real `risk_score` values are genuinely different, and the test asserts each project's response contains only its own task name and never the other project's data.

## 8. Data Contract

Response: `Json<Vec<TestIntelRow>>`, identical shape to the legacy endpoint's response — `id`, `name`, `risk_score`, `failure_rate`, `dependency_count`, `execution_cost_ms`, `value_score`, `flakiness_score`, `window_runs`, `window_failures`. No field was added, renamed, or removed. A test (`phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields`) asserts the response object's exact key set matches this list precisely — no invented/extra fields, no missing ones.

## 9. Legacy Route

`GET /intelligence/tests` was **preserved unmodified** — not deleted, not deprecated, not behaviorally changed. Its handler function (`list_test_intelligence`), its route registration, and its `ctx.project_id`-based scoping semantics are byte-for-byte what they were before this unit (confirmed via the diff in §10/§22 — the legacy handler function body has zero changes, only a new doc comment was added directly above the *new* scoped handler, and a short doc comment was added above the legacy handler itself purely to note it is untouched). A dedicated regression test (`phase4_be10_legacy_unscoped_intelligence_route_still_works`) confirms it still functions end-to-end for an unscoped admin key.

## 10. Files Changed

- `services/orchestrator/src/api.rs` — +41/-0 lines: one new route registration, one new handler function (`list_test_intelligence_scoped`) with its doc comment, one short doc comment added above the pre-existing legacy handler (no behavioral change to that handler).
- `services/orchestrator/src/api_tests.rs` — +186/-0 lines: six new tests plus their helper functions.

## 11. Files Not Changed

**Frontend: confirmed fully untouched.** `git diff --stat -- forentend/` shows only the pre-existing accumulated diff from FE-1 through FE-21 (all predating this session) — zero additional frontend changes from this unit. No file under `forentend/src/**` was opened for editing during this session. No other backend file (`intelligence.rs`, `engine.rs`, `store.rs`, `auth.rs`, `domain.rs`, `errors.rs`, `policy.rs`, `scheduler.rs`, etc.) was modified — `store.rs::list_test_intelligence` was read but not changed, since its existing `Option<&str>` signature was already sufficient.

## 12. Tests Added

1. **`phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields`** (Test 1 + Test 5): an authorized caller requesting their own project gets `200` with a real row whose fields exactly match `TestIntelRow`'s real shape — no invented fields, no missing ones.
2. **`phase4_be10_intelligence_isolated_across_projects`** (Test 2 + Test 13/security): Project A's always-passing task and Project B's always-failing task produce genuinely different real `risk_score` values (`risk_b > risk_a`, asserted, not assumed); each project's own scoped response contains only its own task and never the other's — proving isolation of real data, not just of empty responses.
3. **`phase4_be10_intelligence_unauthorized_and_missing_project_rejected`** (Test 3): a key scoped to Project A requesting Project B's intelligence gets `403 FORBIDDEN`; a nonexistent project gets `404 RESOURCE_NOT_FOUND`; a missing-auth request gets the real structured `401 UNAUTHENTICATED` envelope — all matching the existing, unmodified error-code convention, none invented.
4. **`phase4_be10_intelligence_empty_project_returns_truthful_empty_array`** (Test 4): a real, authorized, empty project returns `200` with a genuinely empty array — never a fake/placeholder row.
5. **`phase4_be10_legacy_unscoped_intelligence_route_still_works`** (regression, §9): the legacy route still works unmodified for an unscoped key.

Test 6 (existing regression) is satisfied by running the full existing suite (§13).

## 13. Test Results

```
cargo test phase4_be10
```
```
running 5 tests
test api_tests::phase4_be10_intelligence_empty_project_returns_truthful_empty_array ... ok
test api_tests::phase4_be10_intelligence_unauthorized_and_missing_project_rejected ... ok
test api_tests::phase4_be10_legacy_unscoped_intelligence_route_still_works ... ok
test api_tests::phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields ... ok
test api_tests::phase4_be10_intelligence_isolated_across_projects ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 96 filtered out
```
Full suite:
```
cargo test
```
```
test result: ok. 101 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.15s
```
(96 pre-existing + 5 new = 101; all pre-existing tests unmodified and passing.)

## 14. Typecheck / Compile

```
cargo build
```
Result: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 1m 23s` — clean, zero errors, zero warnings.
```
cargo build --tests
```
Result: clean, zero errors (one type-inference fix was needed and applied during development — `&String` vs `&str` in a `BTreeSet` comparison in the new test file — resolved before this final run; zero errors in the reported result).

## 15. Lint / Format

```
cargo clippy --all-targets
```
Result: `1 warning` — pre-existing, unrelated to this change (`clippy::too_many_arguments` on `store.rs::insert_audit`, an 8-argument pre-existing function untouched by this unit). **Zero new warnings from BE-10's own code.**

```
cargo fmt --check
```
Result: reports diffs across the majority of the crate's existing files (`main.rs`, `engine.rs`, `engine_tests.rs`, `ws_tests.rs`, `store.rs`, `store_tests.rs`, `intelligence.rs`, `auth.rs`, `domain.rs`, `errors.rs`, `evidence.rs`, `executor.rs`, `lib.rs`, `metrics.rs`, `policy.rs`, `scheduler.rs`, `state.rs`, `webhook.rs`, and both changed files) — **this is pre-existing, repo-wide formatting drift unrelated to this unit**, confirmed by the fact that files this session never opened (e.g. `main.rs`, `ws_tests.rs`, `engine_tests.rs`) show just as many diffs as the two files this unit touched. Running `cargo fmt` repo-wide was deliberately not done, since it would produce a large, unrelated diff across files this unit has no business touching (§15 of the prompt: "no unrelated architecture changes," "no refactor unrelated routes"). The new code in `api.rs`/`api_tests.rs` was written in the same style already used by its immediate neighbors (`list_findings_scoped`, `get_audit_log_scoped`, `be15_audit_log_scoped_to_project_and_isolated`).

## 16. Security Verification

Directly covered by `phase4_be10_intelligence_isolated_across_projects` (§12, item 2): Project A (`risk = A`, low, from an always-passing task) and Project B (`risk = B`, high, from an always-failing task) are real, distinguishable data. Requesting Project A's intelligence with Project A's own key returns only `task-a` and never `task-b`; requesting Project B's with Project B's own key returns only `task-b` and never `task-a`. Requesting Project B's intelligence with a key scoped to Project A is rejected outright with `403` before any query even runs (`phase4_be10_intelligence_unauthorized_and_missing_project_rejected`) — the response body in that case contains no data at all, not even a filtered/empty array, since the authorization check runs first.

## 17. Regression Verification

Full backend suite: **101/101 passed** (up from 96 before this unit — net +5, zero pre-existing tests modified, zero pre-existing tests broken). The legacy `/intelligence/tests` route's own dedicated regression test (`phase4_be10_legacy_unscoped_intelligence_route_still_works`) confirms it is unaffected. `/bug-drafts`, `/admin/audit-log`, and every other pre-existing legacy/scoped route's own tests (`be15_legacy_admin_audit_log_and_bug_drafts_still_work`, `be15_audit_log_scoped_to_project_and_isolated`, and all BE-8/9/12/13/14/15/D1-D5/B1-B6 tests) pass unchanged.

## 18. Truthfulness Audit

- **No fake data**: confirmed — every returned `TestIntelRow` value comes from the real, pre-existing `intelligence.rs` computation, exercised through real completed runs in every test.
- **No mock fallback**: confirmed — the new handler has exactly one code path (`require_scoped_project` then one real store call); there is no fallback branch to fabricate anything.
- **No invented metric**: confirmed — zero new fields, zero new scoring logic; `store.rs::list_test_intelligence` was called, not rewritten.
- **No invented endpoint behavior**: confirmed — the new route follows the identical `require_scoped_project` → store call → `Json(rows)` pattern already used by `list_findings_scoped`/`get_audit_log_scoped`; no new error code, no new response envelope shape.
- **No frontend workaround**: confirmed — zero frontend files touched (§11).
- **No cross-project leakage**: confirmed by direct test (§12 item 2, §16) using genuinely different real data, not just structurally-different requests.

## 19. Remaining Limitations

- The legacy `/intelligence/tests` route still exists and still has its `"default"`-tenant-fallback quirk for unscoped keys with no bound project — unchanged by design (§9); a future unit could consider deprecating it once a frontend unit no longer needs it as a reference, but that is explicitly out of this unit's scope.
- `store.rs::list_test_intelligence`'s `None` branch (used only by the legacy route) was not touched or hardened — this unit's new route never exercises that branch, so it carries no risk from this change, but it remains exactly as limited as it was before.
- No new database index was added for the `quality_nodes.project_id` filter; performance was not a stated concern for this unit and no evidence of a performance problem was found (the existing legacy route already runs the same underlying join).

## 20. FE-22 Handoff

A future frontend unit can now truthfully consume:
- **Endpoint**: `GET /api/v1/projects/{project_id}/intelligence/tests`
- **Authentication**: same `Authorization: Bearer <key>` convention as every other scoped route; any authenticated role may call it (no `require_role` gate beyond authentication + project access, matching `list_findings_scoped`'s/`get_project_dashboard`'s bar, not the higher `Operator` bar `get_audit_log_scoped` uses).
- **Project parameter**: `project_id` is a required path segment (a real project UUID) — the frontend's already-selected project from its existing switcher should be passed here directly, exactly as it already is for `executions`/`findings`/`evidence`/`audit`.
- **Response shape**: `TestIntelRow[]` — `{ id, name, risk_score, failure_rate, dependency_count, execution_cost_ms, value_score, flakiness_score, window_runs, window_failures }`, each numeric field nullable (`Option<f64>`/`Option<i64>` in Rust → `number | null` in a hand-written TypeScript contract).
- **Empty behavior**: `200` with `[]` for a real project with no completed runs yet — not an error, not a fabricated placeholder row.
- **Error behavior**: `404 RESOURCE_NOT_FOUND` for a project that doesn't exist; `403 FORBIDDEN` for a key scoped to a different project; `401 UNAUTHENTICATED` for missing/invalid credentials — identical envelope shape to every other scoped route this frontend already handles.

## 21. Final Recommendation

**FE-22 is now justified.** The exact blocker FE-21 identified — no safe, project-scoped way to reach real Test Intelligence data — is closed. A future FE-22 unit should: add `TestIntelRow`/the new endpoint to `forentend/src/types/contract.ts` and `services/aiqaApi.ts` (mirroring the `findings`/`audit` pattern exactly), then truthfully expose it on whichever page best fits (most likely a small addition near the existing Findings/Reports area, or a new minimal section — that decision is FE-22's own audit to make, not predetermined here), using the same "smallest truthful implementation" discipline as FE-17 through FE-20.
