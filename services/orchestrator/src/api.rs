use crate::auth::{self, AuthContext, Role};
use crate::domain::*;
use crate::engine::{self, RunRecord};
use crate::errors::{ApiError, ApiJson, ApiPath, ApiQuery};
use crate::policy::{self, PolicyDecision};
use crate::scheduler::validate_plan;
use crate::state::AppState;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    // BE-10: `Path`/`Query` are kept imported for `ws_handler` only — its
    // pre-upgrade rejections are deliberately not converted to `ApiError`
    // (see `errors.rs`'s module doc comment; G-WS is out of scope). Every
    // other handler below uses `ApiPath`/`ApiQuery`/`ApiJson` instead.
    extract::{Extension, Path, Query, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use uuid::Uuid;

/// `/health` is intentionally the only route outside the auth layer (see
/// `router` below) — basic liveness checks stay unauthenticated, matching
/// common orchestrator/k8s conventions. `/runs/:id/ws` is also mounted
/// outside the header-based auth layer for a different reason: browsers
/// cannot set an `Authorization` header on a WebSocket handshake, so it
/// authenticates itself via a `?key=` query parameter instead (see
/// `ws_handler`).
pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        // BE-8 — API Contract §5.1-5.4.
        .route("/api/v1/projects", post(create_project).get(list_projects_handler))
        .route("/api/v1/projects/:id", get(get_project_handler).patch(update_project_handler))
        // BE-12 — API Contract §23.1-23.2 Dashboard/KPI JSON endpoints,
        // project-scoped the same way as the executions/evidence routes
        // below (`require_scoped_project`).
        .route("/api/v1/projects/:project_id/dashboard", get(get_project_dashboard))
        .route("/api/v1/projects/:project_id/kpis", get(get_project_kpis))
        // "Actual BE-9" (Integration Plan's own numbering) — project-scoped
        // routing for the execution/evidence resources. See the module doc
        // comment above `require_scoped_project` for exactly which routes
        // moved here and why the rest (bug-drafts/intelligence/audit-log,
        // admin/api-keys, metrics) did not — full reasoning in
        // AIQA_PHASE2_ACTUAL_BE9_PROJECT_SCOPED_ROUTING_REPORT.md §2.
        .route(
            "/api/v1/projects/:project_id/executions",
            post(create_run_scoped).get(list_runs_scoped),
        )
        .route("/api/v1/projects/:project_id/executions/:run_id", get(get_run_scoped))
        .route("/api/v1/projects/:project_id/executions/:run_id/events", get(get_events_scoped))
        .route("/api/v1/projects/:project_id/executions/:run_id/approve", post(approve_run_scoped))
        .route("/api/v1/projects/:project_id/executions/:run_id/reject", post(reject_run_scoped))
        // BE-13 — API Contract §12.3-12.5. Cancel + the standalone task list
        // /detail endpoints (previously only reachable embedded inside
        // `GET .../executions/:run_id`'s `tasks` array).
        .route("/api/v1/projects/:project_id/executions/:run_id/cancel", post(cancel_run_scoped))
        .route("/api/v1/projects/:project_id/executions/:run_id/tasks", get(list_run_tasks_scoped))
        .route("/api/v1/projects/:project_id/executions/:run_id/tasks/:task_id", get(get_run_task_detail_scoped))
        .route("/api/v1/projects/:project_id/executions/:run_id/evidence", get(list_evidence_scoped))
        // BE-15 — API Contract §15.2/§16/§25. `/evidence/:evidence_id` now
        // returns metadata (Detail), not raw bytes — see
        // `get_evidence_detail_scoped`'s doc comment; the bytes moved to
        // the new `/content` sibling, not removed.
        .route("/api/v1/projects/:project_id/evidence/:evidence_id", get(get_evidence_detail_scoped))
        .route("/api/v1/projects/:project_id/evidence/:evidence_id/content", get(get_evidence_content_scoped))
        .route("/api/v1/projects/:project_id/findings", get(list_findings_scoped))
        .route("/api/v1/projects/:project_id/findings/:finding_id", get(get_finding_detail_scoped))
        .route("/api/v1/projects/:project_id/findings/:finding_id/analysis", post(get_finding_analysis_scoped))
        // Build order item 4 — on-demand AI-assisted hypothesis, additive
        // to the real, deterministic `.../analysis` above (never a
        // replacement for it). See `generate_finding_ai_hypothesis_scoped`.
        .route("/api/v1/projects/:project_id/findings/:finding_id/ai-hypothesis", post(generate_finding_ai_hypothesis_scoped))
        .route("/api/v1/projects/:project_id/findings/:finding_id/review", post(review_finding_scoped))
        .route("/api/v1/projects/:project_id/audit", get(get_audit_log_scoped))
        // Phase 4 BE-10 — see `list_test_intelligence_scoped`'s doc comment.
        .route("/api/v1/projects/:project_id/intelligence/tests", get(list_test_intelligence_scoped))
        // AUD-005 — real, persisted Test Case CRUD + lifecycle + execution
        // handoff. See the "test cases" section below for every handler.
        .route("/api/v1/projects/:project_id/test-cases", post(create_test_case_scoped).get(list_test_cases_scoped))
        .route(
            "/api/v1/projects/:project_id/test-cases/:id",
            get(get_test_case_scoped).patch(update_test_case_scoped).delete(delete_test_case_scoped),
        )
        .route("/api/v1/projects/:project_id/test-cases/:id/validate", post(validate_test_case_scoped))
        .route("/api/v1/projects/:project_id/test-cases/:id/approve", post(approve_test_case_scoped))
        .route("/api/v1/projects/:project_id/test-cases/:id/reject", post(reject_test_case_scoped))
        .route("/api/v1/projects/:project_id/test-cases/:id/handoff", post(handoff_test_case_scoped))
        // Phase 2.1 — real, read-only compilation preview (no state mutation).
        .route("/api/v1/projects/:project_id/test-cases/:id/compile", post(compile_test_case_scoped))
        // Phase 2 — real, skill-aware AI test case generation. Produces
        // drafts only (never persists) — the caller then uses the existing
        // `POST .../test-cases` (above) to save whichever drafts it wants,
        // exactly like the pre-existing local-template drafts already do.
        .route("/api/v1/projects/:project_id/test-cases/generate-ai", post(generate_ai_test_cases_scoped))
        // Phase 3 — Custom Testing: real requirement -> AI strategy ->
        // test cases (`custom_testing.rs`). Produces drafts only, exactly
        // like `generate-ai` above — the caller saves whichever drafts it
        // wants via the same existing `POST .../test-cases`, no second
        // persistence path.
        .route("/api/v1/projects/:project_id/custom-testing/generate", post(generate_custom_testing_scoped))
        // Phase 4 — API Testing page: real, ad-hoc HTTP request execution
        // (`api_testing.rs`). Reuses the exact Phase 2.1 execution pipeline
        // (ExecutionPlan -> validate_plan -> materialize_task_def ->
        // create_run_core -> CompiledApiExecutor) — no new executor, no new
        // execution engine.
        .route("/api/v1/projects/:project_id/api-testing/execute", post(execute_api_test_scoped))
        // Truthfulness Cleanup Step 1 — real Application Source connectivity
        // verification (`app_source.rs`). Replaces the Test Case Generator's
        // former fully client-side setTimeout+Math.random() simulation
        // (flagged CRITICAL by the Phase 1-6 Final Consolidated Audit) with
        // a genuine backend check reusing the same, unmodified SSRF guard
        // and transport-error classification every other outbound target in
        // this app already uses.
        .route("/api/v1/projects/:project_id/verify-application-source", post(verify_application_source_scoped))
        // Phase 6 — Performance Testing: real Load/Stress/Spike/Volume
        // workload generation (`performance_testing.rs`). Reuses the exact
        // Phase 4 per-request execution core (`executor::execute_plan_once`,
        // extracted from `CompiledApiExecutor` in this same phase) as the
        // repeated worker underneath a new, minimal concurrency-bounded
        // scheduler — no second HTTP execution engine.
        .route("/api/v1/projects/:project_id/performance-testing/start", post(start_performance_test_scoped))
        .route("/api/v1/projects/:project_id/performance-testing", get(list_performance_runs_scoped))
        .route("/api/v1/projects/:project_id/performance-testing/:run_id/status", get(get_performance_run_status_scoped))
        .route("/api/v1/projects/:project_id/performance-testing/:run_id/cancel", post(cancel_performance_run_scoped))
        // Central AI Provider & Model Intelligence Layer — platform-wide,
        // not project-scoped (the router itself is one shared instance
        // across the whole deployment, exactly like `/admin/api-keys`
        // below is a platform-wide, not project-scoped, resource).
        .route("/api/v1/ai/status", get(get_ai_status))
        .route("/api/v1/ai/settings", get(get_ai_settings_handler).put(put_ai_settings_handler))
        .route("/api/v1/ai/refresh", post(refresh_ai_providers))
        // Legacy, flat, unscoped routes — kept fully functional (Step 6: no
        // document calls for removing or redirecting them; see the report's
        // "Route Compatibility" section for the explicit reasoning).
        .route("/runs", post(create_run).get(list_runs))
        .route("/runs/:id", get(get_run))
        .route("/runs/:id/events", get(get_events))
        .route("/runs/:id/approve", post(approve_run))
        .route("/runs/:id/reject", post(reject_run))
        .route("/runs/:id/evidence", get(list_evidence))
        .route("/evidence/:id/content", get(get_evidence_content))
        .route("/intelligence/tests", get(list_test_intelligence))
        .route("/intelligence/failure-clusters", get(list_failure_clusters))
        .route("/bug-drafts", get(list_bug_drafts))
        .route("/bug-drafts/:id/review", post(review_bug_draft))
        .route("/metrics", get(get_metrics))
        .route("/admin/api-keys", post(create_api_key).get(list_api_keys_handler))
        .route("/admin/api-keys/:id", delete(revoke_api_key_handler))
        .route("/admin/audit-log", get(get_audit_log))
        .layer(axum::middleware::from_fn_with_state(state.store.clone(), auth::require_auth));

    Router::new()
        .route("/health", get(health))
        .route("/runs/:id/ws", get(ws_handler))
        // BE-14 (API Contract §14) — project-scoped realtime stream.
        // Mounted outside the header-based `require_auth` layer for the
        // same reason `/runs/:id/ws` is: a WebSocket upgrade can't carry
        // an `Authorization` header, so this authenticates itself via
        // `?key=` instead (see `project_events_ws_handler`).
        .route("/api/v1/projects/:project_id/events", get(project_events_ws_handler))
        .merge(protected)
        .with_state(state)
}

/// B12 remediation. Was `CorsLayer::new().allow_origin(Any)` in `main.rs` —
/// flagged Medium by the Backend Plan/Deep Audit §15.2: not directly
/// exploitable on its own (every route above is still Bearer-gated by
/// `require_auth`), but a real widening of the browser attack surface
/// (any site could read a response *if* it ever got a caller to hand it a
/// valid key) and explicitly called out as something to fix "before any
/// non-local deployment." A real, explicit allow-list, not a second
/// wildcard dressed up as a list.
///
/// `AIQA_CORS_ALLOWED_ORIGINS` (comma-separated origins) overrides the
/// default entirely when set and non-empty — this is the "ask the user
/// directly rather than guessing" path for any real deployment origin.
/// Unset, it defaults to this project's own documented local dev
/// origin: `forentend`'s Vite dev server (`vite --port=3000`, see
/// `forentend/package.json`'s `dev` script), on both `localhost` and
/// `127.0.0.1` since browsers treat those as distinct origins. `apps/web`
/// (default Vite port, unmaintained since 18 Aug — see
/// AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md §6.1) is deliberately not in
/// the default list; add it via the env var if it's ever needed again.
pub fn cors_layer_from_env() -> CorsLayer {
    let origins = std::env::var("AIQA_CORS_ALLOWED_ORIGINS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.split(',').map(|o| o.trim().to_string()).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                "http://localhost:3000".to_string(),
                "http://127.0.0.1:3000".to_string(),
            ]
        });
    cors_layer_for_origins(&origins)
}

/// The actual allow-list construction, split out from `cors_layer_from_env`
/// so a test can exercise real enforcement against a known, fixed origin
/// list without mutating process-wide environment state (which `cargo
/// test`'s default parallelism makes inherently racy across tests).
/// `pub(crate)` rather than private: `api_tests.rs` is a sibling module
/// (declared in `lib.rs`), not a child of this one, so it needs at least
/// crate-visibility to call this directly.
pub(crate) fn cors_layer_for_origins(origins: &[String]) -> CorsLayer {
    let headers: Vec<HeaderValue> = origins.iter().filter_map(|o| HeaderValue::from_str(o).ok()).collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(headers))
        .allow_methods(Any)
        .allow_headers(Any)
}

async fn health() -> &'static str {
    "ok"
}

fn to_500<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::internal(e)
}

async fn audit(
    state: &AppState,
    ctx: &AuthContext,
    action: &str,
    target: Option<&str>,
    detail: Option<&str>,
    outcome: &str,
) {
    let _ = state
        .store
        .insert_audit(Some(&ctx.name), Some(ctx.role.as_str()), ctx.project_id.as_deref(), action, target, detail, outcome)
        .await;
}

// ---------------------------------------------------- project-scoped routing
//
// "Actual BE-9" (Integration Plan's own numbering, §15: "`/api/v1` + scoped
// routing | All routes contract-shaped") — every handler below this point
// whose name ends in `_scoped` is reached through a URL that names a
// project. Two checks are mandatory for every one of them (Step 3 of the
// brief this was implemented against):
//
//   1. `require_scoped_project` — the project in the URL must actually
//      exist (`store.get_project`), and the caller must be authorized for
//      it (`auth::require_project_access`, the same gate every existing
//      handler already uses — no new authorization primitive was written).
//   2. `require_resource_in_project` — for any handler that also names a
//      specific resource (a run, a piece of evidence) under that project,
//      the resource's *own* stored `project_id` must match the URL's
//      project id. This is what actually prevents Project A's key —
//      including a *global* admin key, which passes check 1 for any
//      project — from reading Project B's run by requesting it under
//      Project A's path. Checking only #1 would authorize the *namespace*
//      without ever confirming the *resource* is really in it.
//
// A mismatch in #2 is reported as 404, never 403: from this URL's point of
// view the resource simply isn't there, and a 403 would leak that it
// exists somewhere else.

/// Requirement #1 above. Existence is checked before authorization,
/// matching this file's own established convention elsewhere (`get_run`,
/// `get_project_handler`): a project that doesn't exist can't be "yours"
/// either way, so 404 is the more informative answer than 403.
async fn require_scoped_project(state: &AppState, ctx: &AuthContext, project_id: Uuid) -> Result<(), ApiError> {
    state
        .store
        .get_project(project_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    auth::require_project_access(ctx, Some(project_id.to_string().as_str()))?;
    Ok(())
}

/// Requirement #2 above.
fn require_resource_in_project(resource_project_id: Option<&str>, url_project_id: Uuid) -> Result<(), ApiError> {
    if resource_project_id == Some(url_project_id.to_string().as_str()) {
        Ok(())
    } else {
        Err(ApiError::not_found("resource not found in this project"))
    }
}

// ------------------------------------------------------------- projects

#[derive(Deserialize)]
struct CreateProjectBody {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

fn validate_project_name(name: &str) -> Result<&str, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::validation("project name must not be empty"));
    }
    Ok(trimmed)
}

/// POST /api/v1/projects — API Contract §5.2. Creating a project is a
/// foundational, cross-cutting action (it defines a new isolation boundary
/// every other resource will scope against), so it follows the same bar as
/// the other action that creates a new access boundary today, minting an
/// API key (`create_api_key`, below): Admin-only, and — matching that
/// handler exactly — not additionally restricted by the creating key's own
/// `project_id` scope, since an admin key (scoped or global) can already
/// mint keys for any project.
async fn create_project(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<CreateProjectBody>,
) -> Result<(StatusCode, Json<crate::store::Project>), ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let name = validate_project_name(&body.name)?;

    let id = Uuid::new_v4();
    state
        .store
        .insert_project(id, name, body.description.as_deref())
        .await
        .map_err(to_500)?;

    audit(&state, &ctx, "project.create", Some(&id.to_string()), Some(name), "ok").await;

    let project = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("project vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// GET /api/v1/projects — API Contract §5.1. Same isolation convention as
/// every other list endpoint in this API (`list_runs`, `list_bug_drafts`,
/// `list_test_intelligence`): a global key (`project_id: None`) sees every
/// project; a project-scoped key sees only its own — which, per BE-8's own
/// scope note, may legitimately be an empty list if that project id was
/// never created via `POST /api/v1/projects`.
async fn list_projects_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::Project>>, ApiError> {
    let rows = state
        .store
        .list_projects(ctx.project_id.as_deref())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

/// GET /api/v1/projects/:id — API Contract §5.3. Fetch-then-authorize, the
/// same order every other scoped-resource handler in this file already uses
/// (`get_run`, `list_evidence`, `get_evidence_content`): existence is
/// checked first (404 if genuinely absent), then `require_project_access`
/// against the project's own id (403 on a cross-project read).
async fn get_project_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<crate::store::Project>, ApiError> {
    let project = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    auth::require_project_access(&ctx, Some(project.id.to_string().as_str()))?;
    Ok(Json(project))
}

#[derive(Deserialize)]
struct UpdateProjectBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// PATCH /api/v1/projects/:id — API Contract §5.4. Mutating a resource
/// within your own scope follows the same bar as every other in-scope
/// mutation (`create_run`, `approve_run`/`reject_run`): Operator, plus
/// `require_project_access` so a project-scoped operator can only edit its
/// own project. A `None` field is left unchanged (partial update); an
/// absent body updates nothing but still refreshes `updated_at`.
async fn update_project_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<UpdateProjectBody>,
) -> Result<Json<crate::store::Project>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let existing = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    auth::require_project_access(&ctx, Some(existing.id.to_string().as_str()))?;

    let name = body.name.as_deref().map(validate_project_name).transpose()?;

    let updated = state
        .store
        .update_project(id, name, body.description.as_deref())
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("project not found"));
    }

    audit(&state, &ctx, "project.update", Some(&id.to_string()), name, "ok").await;

    let project = state
        .store
        .get_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    Ok(Json(project))
}

// ------------------------------------------------------------- test cases
//
// AUD-005 — real, persisted, project-isolated Test Case CRUD + lifecycle +
// execution handoff. See `domain.rs`'s test-case section and `store.rs`'s
// test-case functions for the entity/persistence design; this section is
// just the HTTP surface over them, following the exact same
// `require_scoped_project` + `require_role` + `audit(...)` pattern every
// other mutating scoped resource in this file already uses.

fn validate_test_case_request(req: &CreateTestCaseRequest) -> Result<(), ApiError> {
    if req.title.trim().is_empty() {
        return Err(ApiError::validation("title must not be empty"));
    }
    if req.description.trim().is_empty() {
        return Err(ApiError::validation("description must not be empty"));
    }
    if req.steps.is_empty() {
        return Err(ApiError::validation("at least one step is required"));
    }
    if req.expected_result.as_deref().map(str::trim).unwrap_or("").is_empty() && req.assertions.iter().all(|a| a.trim().is_empty()) {
        return Err(ApiError::validation("expected_result or at least one assertion is required"));
    }
    Ok(())
}

/// POST /api/v1/projects/:project_id/test-cases — persists one generated
/// test case for real. This is deliberately a single-case create, not a
/// batch endpoint: the Test Case Generator produces its cases client-side
/// one at a time already, and a single, uniform create contract is simpler
/// than inventing a second batch shape nothing else in this API uses.
async fn create_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateTestCaseRequest>,
) -> Result<(StatusCode, Json<crate::store::TestCaseRow>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    validate_test_case_request(&body)?;

    let id = state.store.insert_test_case(&project_id.to_string(), &body, Some(&ctx.name)).await.map_err(to_500)?;

    audit(&state, &ctx, "test_case.create", Some(&id.to_string()), Some(&body.title), "ok").await;

    let row = state
        .store
        .get_test_case(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| to_500("test case vanished immediately after insert"))?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct ListTestCasesQuery {
    #[serde(default)]
    status: Option<String>,
}

/// GET /api/v1/projects/:project_id/test-cases — project-scoped, optionally
/// filtered to one lifecycle status (`?status=APPROVED`, etc).
async fn list_test_cases_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListTestCasesQuery>,
) -> Result<Json<Vec<crate::store::TestCaseRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state.store.list_test_cases(&project_id.to_string(), q.status.as_deref()).await.map_err(to_500)?;
    Ok(Json(rows))
}

/// Shared fetch-then-authorize for every `:id`-scoped test-case route —
/// same order every other scoped-resource handler in this file uses
/// (`get_project_handler`, `scoped_run_summary`): existence first (404 if
/// genuinely absent, including when it exists but belongs to a different
/// project than the URL says — never leaks a cross-project id's existence),
/// then `require_project_access` against the row's own real `project_id`
/// (403 if the caller's key is scoped to a different project).
async fn scoped_test_case(state: &AppState, ctx: &AuthContext, project_id: Uuid, id: Uuid) -> Result<crate::store::TestCaseRow, ApiError> {
    let row = state.store.get_test_case(id).await.map_err(to_500)?.ok_or_else(|| ApiError::not_found("test case not found"))?;
    if row.project_id != project_id.to_string() {
        return Err(ApiError::not_found("test case not found"));
    }
    auth::require_project_access(ctx, Some(row.project_id.as_str()))?;
    Ok(row)
}

async fn get_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

#[derive(Deserialize, Default)]
struct UpdateTestCaseBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    preconditions: Option<Vec<String>>,
    #[serde(default)]
    steps: Option<Vec<TestStep>>,
    #[serde(default)]
    expected_result: Option<String>,
}

/// PATCH /api/v1/projects/:project_id/test-cases/:id — partial update of
/// the still-editable descriptive fields (never `status`; that only moves
/// through `/validate`, `/approve`, `/reject`, `/handoff` below).
async fn update_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<UpdateTestCaseBody>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_case(&state, &ctx, project_id, id).await?;

    if let Some(title) = &body.title {
        if title.trim().is_empty() {
            return Err(ApiError::validation("title must not be empty"));
        }
    }

    let updated = state
        .store
        .update_test_case(
            id,
            body.title.as_deref(),
            body.description.as_deref(),
            body.priority.as_deref(),
            body.preconditions.as_ref(),
            body.steps.as_ref(),
            body.expected_result.as_deref(),
        )
        .await
        .map_err(to_500)?;
    if !updated {
        return Err(ApiError::not_found("test case not found"));
    }
    audit(&state, &ctx, "test_case.update", Some(&id.to_string()), None, "ok").await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

async fn delete_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_test_case(&state, &ctx, project_id, id).await?;
    let deleted = state.store.delete_test_case(id).await.map_err(to_500)?;
    if !deleted {
        return Err(ApiError::not_found("test case not found"));
    }
    audit(&state, &ctx, "test_case.delete", Some(&id.to_string()), None, "ok").await;
    Ok(StatusCode::NO_CONTENT)
}

/// POST .../test-cases/:id/validate — FRD-VAL-02 ("generated cases meet
/// configured quality/benchmark rules"). Honest scope: this checks real,
/// verifiable structural completeness (non-empty title/description/steps/
/// expected_result, and every step has both an action and an expected
/// result) — there is no AI/model-scored quality benchmark anywhere in this
/// codebase (no model-gateway/LLM integration exists in `services/
/// orchestrator`), so this deliberately does not claim to evaluate
/// anything beyond structure. A case that fails stays `GENERATED`, with
/// the failure reason returned as a real 409, never silently advanced.
async fn validate_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;

    let mut problems = Vec::new();
    if row.title.trim().is_empty() {
        problems.push("title is empty");
    }
    if row.description.trim().is_empty() {
        problems.push("description is empty");
    }
    if row.steps.is_empty() {
        problems.push("no steps defined");
    }
    if row.steps.iter().any(|s| s.action.trim().is_empty() || s.expected_result.trim().is_empty()) {
        problems.push("a step is missing an action or an expected result");
    }
    if row.expected_result.trim().is_empty() {
        problems.push("expected_result is empty");
    }

    if !problems.is_empty() {
        return Err(ApiError::conflict(format!("structural validation failed: {}", problems.join("; "))));
    }

    let notes = "structural validation passed: title, description, steps and expected result are present";
    let ok = state.store.validate_test_case(id, notes).await.map_err(to_500)?;
    if !ok {
        return Err(ApiError::conflict(format!("test case is not in GENERATED state (current: {})", row.status.as_db_str())));
    }
    audit(&state, &ctx, "test_case.validate", Some(&id.to_string()), None, "ok").await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

/// POST .../test-cases/:id/approve — FRD-EXE-11: only an *approved* case
/// may later convert into a schedulable task. Requires `VALIDATED` first —
/// generation alone is never enough (FRD-AGT-04).
async fn approve_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    let ok = state.store.approve_test_case(id).await.map_err(to_500)?;
    if !ok {
        return Err(ApiError::conflict(format!("test case must be VALIDATED before approval (current: {})", row.status.as_db_str())));
    }
    audit(&state, &ctx, "test_case.approve", Some(&id.to_string()), None, "ok").await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

/// POST .../test-cases/:id/reject — reachable from `GENERATED` or
/// `VALIDATED`, not from `APPROVED`/`EXECUTED` (see `store.rs::
/// reject_test_case`'s own doc comment for why).
async fn reject_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::TestCaseRow>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    let ok = state.store.reject_test_case(id).await.map_err(to_500)?;
    if !ok {
        return Err(ApiError::conflict(format!("test case cannot be rejected from its current state ({})", row.status.as_db_str())));
    }
    audit(&state, &ctx, "test_case.reject", Some(&id.to_string()), None, "ok").await;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    Ok(Json(row))
}

/// POST .../test-cases/:id/compile — Phase 2.1. Real, read-only preview of
/// what `execution_plan::compile` would produce for this exact, real,
/// persisted test case right now — deterministic compilation first, the
/// real central AI router only if that finds nothing. Never mutates
/// anything (no run, no status change) — safe to call before a case is
/// even APPROVED, so the frontend can show READY/NOT_EXECUTABLE/
/// COMPILATION_UNAVAILABLE (and why) before a user commits to approving
/// and handing it off.
async fn compile_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::execution_plan::CompilationOutcome>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;
    let outcome = crate::execution_plan::compile(&state.ai_infra, &row).await;
    Ok(Json(outcome))
}

/// POST .../test-cases/:id/handoff — the real bridge into the existing
/// execution engine (FRD-EXE-11). Requires `APPROVED`. Builds a real,
/// single-task `RunPlan` — the task's id is this test case's own id, so
/// `tasks.id` doubles as the traceability key back to it, no extra join
/// table needed (Test Case -> Run -> Task -> Result/Evidence, all already
/// queryable via the existing `runs`/`tasks`/`evidence`/`run_events`
/// tables) — and dispatches it through the exact same `create_run_core`
/// the real Execution Hub itself calls. No duplicate scheduler, no shadow
/// execution engine.
///
/// Honest disclosure (see `TestCaseHandoffRequest`'s own doc comment): the
/// caller must explicitly supply either a real `executor`, an explicit
/// `simulate` outcome, or `compile_and_execute: true` (Phase 2.1). There is
/// no default here that would silently report a fabricated "passed" result
/// for steps nothing has actually run.
async fn handoff_test_case_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    body: Option<ApiJson<TestCaseHandoffRequest>>,
) -> Result<(StatusCode, Json<CreateExecutionResponse>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_test_case(&state, &ctx, project_id, id).await?;

    if row.status != TestCaseStatus::Approved {
        return Err(ApiError::conflict(format!("test case must be APPROVED before handoff (current: {})", row.status.as_db_str())));
    }

    let ApiJson(body) = body.unwrap_or(ApiJson(TestCaseHandoffRequest { executor: None, params: serde_json::Value::Null, simulate: None, compile_and_execute: false }));

    let task_id = id.to_string();
    let task = if body.compile_and_execute {
        // Phase 2.1 — the real compiler, not the caller, determines the
        // real executor/params; any `executor`/`params`/`simulate` also
        // sent alongside `compile_and_execute: true` is ignored (see
        // `TestCaseHandoffRequest`'s own doc comment) rather than silently
        // mixed with compiled output.
        match crate::execution_plan::compile(&state.ai_infra, &row).await {
            crate::execution_plan::CompilationOutcome::Ready(plan) => {
                crate::execution_plan::materialize_task_def(task_id.clone(), row.title.clone(), &plan).map_err(ApiError::validation)?
            }
            crate::execution_plan::CompilationOutcome::NotExecutable { reasons } => {
                return Err(ApiError::validation(format!("NOT_EXECUTABLE: {}", reasons.join("; "))));
            }
            crate::execution_plan::CompilationOutcome::CompilationFailed { reasons } => {
                return Err(ApiError::validation(format!("COMPILATION_FAILED: {}", reasons.join("; "))));
            }
            crate::execution_plan::CompilationOutcome::CompilationUnavailable { reason } => {
                return Err(ApiError::validation(format!("COMPILATION_UNAVAILABLE: {reason}")));
            }
        }
    } else {
        if body.executor.is_none() && body.simulate.is_none() {
            return Err(ApiError::validation(
                "handoff requires either a real executor, an explicit simulate outcome, or compile_and_execute: true — there is no default that fabricates a result",
            ));
        }
        TaskDef {
            id: task_id.clone(),
            name: row.title.clone(),
            prerequisites: Vec::new(),
            simulate: body.simulate.clone(),
            executor: body.executor.clone(),
            params: body.params.clone(),
        }
    };
    let run_plan = RunPlan { name: format!("Test Case: {}", row.title), tasks: vec![task], concurrency: 1 };

    let (status_code, Json(created)) = create_run_core(&state, &ctx, Some(project_id.to_string()), run_plan).await?;

    let marked = state.store.mark_test_case_executed(id, created.run_id, &task_id).await.map_err(to_500)?;
    if !marked {
        // The run was genuinely created either way (real, not fabricated);
        // this only means this test case's own back-link didn't apply —
        // surfaced, not silently swallowed.
        tracing::warn!("test case {id} handed off as run {} but its own status transition to EXECUTED did not apply", created.run_id);
    }

    audit(&state, &ctx, "test_case.handoff", Some(&id.to_string()), Some(&created.run_id.to_string()), "ok").await;

    Ok((status_code, Json(CreateExecutionResponse { run_id: created.run_id, status: created.state, policy_notes: created.policy_notes })))
}

#[derive(Debug, Deserialize)]
struct SkillInputBody {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
struct GenerateAiTestCasesBody {
    requirement_text: String,
    #[serde(default)]
    skills: Vec<SkillInputBody>,
}

/// `generation_method` is always exactly `"ai"` or `"fallback"` — never
/// anything implying success when `ai_router.generate` actually returned
/// `GenerationOutcome::Failed`. `provider`/`model` are only ever `Some`
/// alongside `generation_method: "ai"`; `fallback_reason` only alongside
/// `"fallback"` — a client cannot receive a shape that claims both or
/// neither.
#[derive(Debug, Serialize)]
struct GenerateAiTestCasesResponse {
    test_cases: Vec<crate::ai_generation::GeneratedTestCaseDraft>,
    generation_method: String,
    provider: Option<String>,
    model: Option<String>,
    fallback_reason: Option<String>,
}

/// POST .../test-cases/generate-ai — Phase 2. Real, skill-aware AI
/// generation (`ai_generation::AiModelRouter`). Produces drafts only, never
/// persists them — the frontend uses the existing
/// `POST .../test-cases` to save whichever drafts it wants, exactly like a
/// pre-existing local-template draft already does (no second, competing
/// persistence path). A `"fallback"` response is a normal 200, not a 5xx:
/// the endpoint did its real job (attempted AI generation and honestly
/// reports the outcome) — the frontend's own existing local generator is
/// what actually produces a result in that case, honestly labeled as such,
/// never silently presented as AI output.
async fn generate_ai_test_cases_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<GenerateAiTestCasesBody>,
) -> Result<Json<GenerateAiTestCasesResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    if body.requirement_text.trim().is_empty() {
        return Err(ApiError::validation("requirement_text must not be empty"));
    }

    let skills: Vec<crate::ai_generation::SkillInput> = body
        .skills
        .into_iter()
        .map(|s| crate::ai_generation::SkillInput { id: s.id, name: s.name, description: s.description })
        .collect();

    let outcome = crate::ai_generation::generate_test_cases(&state.ai_infra, &body.requirement_text, &skills).await;

    let response = match outcome {
        crate::ai_generation::GenerationOutcome::Ai { cases, provider, model } => {
            audit(
                &state,
                &ctx,
                "test_case.generate_ai",
                None,
                Some(&format!("{} case(s) via {provider}/{model}", cases.len())),
                "ok",
            )
            .await;
            GenerateAiTestCasesResponse {
                test_cases: cases,
                generation_method: "ai".to_string(),
                provider: Some(provider),
                model: Some(model),
                fallback_reason: None,
            }
        }
        crate::ai_generation::GenerationOutcome::Failed { reason } => {
            audit(&state, &ctx, "test_case.generate_ai", None, Some(&reason), "fallback").await;
            GenerateAiTestCasesResponse {
                test_cases: Vec::new(),
                generation_method: "fallback".to_string(),
                provider: None,
                model: None,
                fallback_reason: Some(reason),
            }
        }
    };

    Ok(Json(response))
}

// ---------------------------------------------------------- Custom Testing

#[derive(Debug, Deserialize)]
struct CustomTestingGenerateBody {
    requirement_text: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    skills: Vec<SkillInputBody>,
}

/// `generation_source` is always exactly `"REAL_AI"` or `"UNAVAILABLE"` —
/// there is no deterministic local-fallback strategy generator in this
/// codebase, so a `"LOCAL_FALLBACK"` value is never produced (brief §6: do
/// not invent a fallback path that does not genuinely exist). `analysis` is
/// present whenever the real strategy analysis call succeeded, even if the
/// subsequent case-generation call then failed — see
/// `custom_testing::CustomTestingOutcome`'s own doc comment.
#[derive(Debug, Serialize)]
struct CustomTestingGenerateResponse {
    generation_source: String,
    analysis: Option<crate::custom_testing::CustomTestingAnalysis>,
    test_cases: Vec<crate::ai_generation::GeneratedTestCaseDraft>,
    provider: Option<String>,
    model: Option<String>,
    failure_reason: Option<String>,
}

/// POST .../custom-testing/generate — Phase 3. Real requirement -> AI
/// strategy -> test cases, reusing the exact existing central AI router
/// (`AiTask::RequirementAnalysis`, `AiTask::TestCaseGeneration` — both
/// already-existing task variants, the first newly wired up here for the
/// first time, the second reused byte-for-byte from `generate_ai_test_
/// cases_scoped` above via `ai_generation::generate_test_cases`) and the
/// same skill-resolution/quality/traceability/dedup gates. Produces drafts
/// only, never persists — the caller uses the existing `POST .../test-cases`
/// to save whichever drafts it wants, exactly like `generate-ai`'s own
/// drafts, so a saved custom-testing case is an ordinary `TestCaseRow`
/// indistinguishable from any other — it automatically gets the existing
/// validate/approve/handoff/compile pipeline with no new backend code.
async fn generate_custom_testing_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CustomTestingGenerateBody>,
) -> Result<Json<CustomTestingGenerateResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    if body.requirement_text.trim().is_empty() {
        return Err(ApiError::validation("requirement_text must not be empty"));
    }

    let skills: Vec<crate::ai_generation::SkillInput> = body
        .skills
        .into_iter()
        .map(|s| crate::ai_generation::SkillInput { id: s.id, name: s.name, description: s.description })
        .collect();

    let outcome = crate::custom_testing::run(&state.ai_infra, &body.requirement_text, body.scope.as_deref(), &skills).await;

    let response = match outcome {
        crate::custom_testing::CustomTestingOutcome::Ready { analysis, cases, provider, model } => {
            audit(
                &state,
                &ctx,
                "custom_testing.generate",
                None,
                Some(&format!("{} case(s) via {provider}/{model}", cases.len())),
                "ok",
            )
            .await;
            CustomTestingGenerateResponse {
                generation_source: "REAL_AI".to_string(),
                analysis: Some(analysis),
                test_cases: cases,
                provider: Some(provider),
                model: Some(model),
                failure_reason: None,
            }
        }
        crate::custom_testing::CustomTestingOutcome::Failed { reason, analysis } => {
            audit(&state, &ctx, "custom_testing.generate", None, Some(&reason), "unavailable").await;
            CustomTestingGenerateResponse {
                generation_source: "UNAVAILABLE".to_string(),
                analysis,
                test_cases: Vec::new(),
                provider: None,
                model: None,
                failure_reason: Some(reason),
            }
        }
    };

    Ok(Json(response))
}

// -------------------------------------------------------------- API Testing

/// `task_status` is `TaskStatus` directly (real `SCREAMING_SNAKE_CASE`
/// serde, e.g. `"PASSED"`/`"FAILED"`/`"INFRA_ERROR"`) — the exact same real
/// vocabulary every other execution result in this app already uses, not a
/// second status string invented for this one page. `result` is `None`
/// only when the task's own evidence log genuinely couldn't be read/parsed
/// (never a placeholder standing in for a real one).
#[derive(Debug, Serialize)]
struct ApiTestExecutionResponse {
    run_id: Uuid,
    task_status: crate::domain::TaskStatus,
    reason: Option<String>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    result: Option<crate::executor::ApiResultSummary>,
    response_body_preview: Option<String>,
    evidence_id: Option<Uuid>,
}

/// POST .../api-testing/execute — Phase 4. Real, ad-hoc HTTP request
/// execution. Builds an `ExecutionPlan` directly from the caller's own
/// explicit method/URL/headers/query/body/assertions
/// (`api_testing::validate_and_build_plan`), then reuses the exact Phase
/// 2.1 pipeline unmodified: `execution_plan::validate_plan` (real SSRF
/// guard) -> `execution_plan::materialize_task_def` -> `create_run_core`
/// (the same function every other run-creating endpoint calls) ->
/// `engine::spawn_run` -> `CompiledApiExecutor` -> a real, live HTTP
/// request. No new executor, no new execution engine, no new persistence
/// path. Waits (bounded, `API_TEST_WAIT_CEILING_SECS`, default 45s) for the
/// real run this same request just created to reach a terminal state — the
/// endpoint itself is the "poll", server-side, over this process's own
/// in-memory run record; the frontend receives one real, complete result,
/// not a second polling contract to implement.
async fn execute_api_test_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<crate::api_testing::AdHocApiRequest>,
) -> Result<Json<ApiTestExecutionResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    let task_id = Uuid::new_v4().to_string();
    let task_name = format!("API Test: {} {}", body.method.trim().to_uppercase(), body.url);

    let plan = crate::api_testing::validate_and_build_plan(task_id.clone(), &body).map_err(|(code, msg)| ApiError::validation(format!("{code}: {msg}")))?;

    // The real SSRF/structural guard — reused unmodified. A disallowed
    // target (private IP, cloud metadata address, non-http(s) scheme that
    // slipped past the earlier scheme check, etc.) is rejected here, before
    // any run is created and before any real network call happens.
    if let Err(reasons) = crate::execution_plan::validate_plan(&plan).await {
        return Err(ApiError::validation(format!("TARGET_NOT_ALLOWED: {}", reasons.join("; "))));
    }

    let task = crate::execution_plan::materialize_task_def(task_id.clone(), task_name.clone(), &plan).map_err(ApiError::validation)?;
    let run_plan = RunPlan { name: task_name, tasks: vec![task], concurrency: 1 };

    let (_, Json(created)) = create_run_core(&state, &ctx, Some(project_id.to_string()), run_plan).await?;

    let wait_ceiling_secs: u64 = std::env::var("API_TEST_WAIT_CEILING_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(45);
    let terminal = crate::api_testing::run_to_completion(&state, created.run_id, std::time::Duration::from_secs(wait_ceiling_secs)).await;

    let Some(task_state) = terminal else {
        audit(&state, &ctx, "api_testing.execute", None, Some("did not reach a terminal state within the wait ceiling"), "still_running").await;
        return Ok(Json(ApiTestExecutionResponse {
            run_id: created.run_id,
            task_status: crate::domain::TaskStatus::Running,
            reason: Some(format!(
                "Execution is still running past this endpoint's own {wait_ceiling_secs}s wait ceiling — the real run continues; check the Execution Hub (run {}) for its eventual real result.",
                created.run_id
            )),
            started_at: None,
            finished_at: None,
            result: None,
            response_body_preview: None,
            evidence_id: None,
        }));
    };

    let (evidence_id, log) = match crate::api_testing::read_task_evidence(&state, created.run_id, &task_id).await {
        Some((id, content)) => (Some(id), Some(content)),
        None => (None, None),
    };
    let result = log.as_deref().and_then(crate::api_testing::parse_result_summary);
    let response_body_preview = log.as_deref().and_then(crate::api_testing::parse_response_body_preview);

    audit(&state, &ctx, "api_testing.execute", None, Some(&format!("{} {} -> {:?}", body.method.trim().to_uppercase(), body.url, task_state.status)), "ok").await;

    Ok(Json(ApiTestExecutionResponse {
        run_id: created.run_id,
        task_status: task_state.status,
        reason: task_state.reason,
        started_at: task_state.started_at,
        finished_at: task_state.finished_at,
        result,
        response_body_preview,
        evidence_id,
    }))
}

// ------------------------------ Truthfulness Cleanup Step 1 — Application Source

#[derive(Debug, Deserialize)]
struct VerifyApplicationSourceBody {
    url: String,
}

/// POST /api/v1/projects/:project_id/verify-application-source — real,
/// bounded connectivity verification for the Test Case Generator's
/// "Application Sources" panel (`app_source.rs`). `Role::Operator`: this
/// makes a real outbound network request (bounded, SSRF-guarded, redirects
/// disabled), the same authorization bar every other real-outbound-action
/// endpoint in this app (API Testing's `execute`, Performance Testing's
/// `start`) already requires — never left open to a lower role just because
/// nothing is persisted.
async fn verify_application_source_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<VerifyApplicationSourceBody>,
) -> Result<Json<crate::app_source::VerifyApplicationSourceResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    let result = crate::app_source::verify(&body.url).await;
    audit(&state, &ctx, "app_source.verify", None, Some(&format!("{} -> {}", body.url, result.status)), if result.reachable { "ok" } else { "not_reachable" }).await;
    Ok(Json(result))
}

// ---------------------------------------------------- Phase 6 — Performance Testing

#[derive(Debug, Serialize)]
struct PerformanceTestStartResponse {
    run_id: Uuid,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct PerformanceRunStatusResponse {
    run_id: Uuid,
    name: String,
    test_type: String,
    status: String,
    verdict: Option<String>,
    verdict_reasons: Vec<String>,
    /// Set only for a genuine `ERROR` result (§9 of the Phase 6 ERROR-fix
    /// brief) — the same real, safe, sanitized message also present as
    /// `verdict_reasons[0]` for backward-compatible rendering, exposed here
    /// too as its own explicit field rather than requiring a caller to know
    /// that convention. `null` for every other status.
    error_reason: Option<String>,
    elapsed_ms: u64,
    current_concurrency: u32,
    sample_count: u64,
    metrics: Option<crate::performance_testing::PerformanceMetrics>,
    samples: Vec<crate::performance_testing::RequestSample>,
    config: crate::performance_testing::PerformanceTestConfig,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    evidence_id: Option<Uuid>,
    /// `true` when this reflects the live, in-process workload handle
    /// (real-time samples, recomputed-on-read metrics/verdict); `false`
    /// when falling back to the durable DB row because no live handle
    /// exists in *this* process (a different process ran it, or this
    /// process restarted) — `samples` is honestly empty in that case; the
    /// full raw sample set, if it was persisted, is still fetchable via the
    /// existing `evidence_id`/`.../evidence/:id/content` endpoint.
    live: bool,
}

/// POST /api/v1/projects/:project_id/performance-testing/start — real Load/
/// Stress/Spike/Volume workload kickoff. Validates the target (reused,
/// unmodified `execution_plan::validate_plan` — real SSRF guard) and the
/// schedule against server-enforced `SafetyLimits` *before* creating any
/// persisted row or spawning any real request, then returns immediately
/// (`status: RUNNING`) — the real workload runs in a background task;
/// progress is polled via the `.../status` endpoint below, never faked.
async fn start_performance_test_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<crate::performance_testing::PerformanceTestConfig>,
) -> Result<Json<PerformanceTestStartResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;

    let limits = crate::performance_testing::SafetyLimits::from_env();
    crate::performance_testing::validate_schedule(&body.schedule, &limits).map_err(|(code, msg)| ApiError::validation(format!("{code}: {msg}")))?;

    let run_id = Uuid::new_v4();
    let plan = crate::performance_testing::build_request_plan(&run_id.to_string(), &body.request, body.expected_status).map_err(|(code, msg)| ApiError::validation(format!("{code}: {msg}")))?;

    // The real SSRF/structural guard — reused unmodified, run once before
    // the workload starts (every repeated request inside the workload
    // reuses this same, already-validated plan; see `executor::
    // execute_plan_once`'s doc comment for why it is not re-run per sample).
    if let Err(reasons) = crate::execution_plan::validate_plan(&plan).await {
        return Err(ApiError::validation(format!("TARGET_NOT_ALLOWED: {}", reasons.join("; "))));
    }

    let test_type = body.schedule.type_name();
    let name = format!("{test_type} test: {} {}", body.request.method.trim().to_uppercase(), body.request.url);
    let config_json = serde_json::to_string(&body).map_err(to_500)?;

    let handle = crate::performance_testing::PerformanceRunHandle::new(run_id);
    state
        .store
        .insert_performance_run(run_id, &project_id.to_string(), &name, test_type, &config_json, Some(&ctx.name), handle.started_at)
        .await
        .map_err(to_500)?;
    state.performance_tests.write().await.insert(run_id, handle.clone());

    // One real `reqwest::Client` built here and reused for every request
    // this whole workload makes — real HTTP keep-alive connection pooling,
    // not a fresh TCP connection (and fresh local ephemeral port) per
    // single request. Same `API_TEST_TIMEOUT_SECS` convention `CompiledApiExecutor`
    // already uses for its own per-request timeout.
    let timeout = crate::executor::CompiledApiExecutor::from_env().timeout;
    let client = reqwest::Client::builder().timeout(timeout).build().map_err(to_500)?;
    let plan = Arc::new(plan);
    let schedule = body.schedule;
    let config = body.clone();
    let state2 = state.clone();
    let handle2 = handle.clone();
    // Real Tokio task supervision (`performance_testing::supervise_workload_task`)
    // — the fix for a genuine gap this phase's own live verification found: a
    // bare, unobserved `tokio::spawn` here meant a real panic inside the
    // workload task silently vanished, leaving the persisted row stuck at
    // RUNNING until the *next process restart's* recovery sweep eventually
    // noticed. `supervise_workload_task` owns the real `JoinHandle` and
    // reacts immediately: `Ok(())` is the exact same normal-completion path
    // as before; `Err(failure)` is a genuine, unexpected task-level failure,
    // finalized as a real, persisted `ERROR` — not silently orphaned.
    tokio::spawn(async move {
        let handle3 = handle2.clone();
        let outcome = crate::performance_testing::supervise_workload_task(async move {
            crate::performance_testing::run_workload(client, plan, schedule, limits, handle3).await;
        })
        .await;
        match outcome {
            Ok(()) => finalize_performance_run(&state2, run_id, &config, &handle2).await,
            Err(failure) => finalize_performance_run_as_error(&state2, run_id, &handle2, &failure).await,
        }
    });

    audit(&state, &ctx, "performance_testing.start", Some(&run_id.to_string()), Some(&name), "ok").await;

    Ok(Json(PerformanceTestStartResponse { run_id, status: "RUNNING" }))
}

/// Runs exactly once, right after `run_workload` returns — computes real
/// metrics/verdict from the handle's final sample set, persists the raw
/// samples via the *existing* evidence store (same convention
/// `CompiledApiExecutor`'s own evidence already uses), and writes the one
/// terminal `performance_runs` row update.
async fn finalize_performance_run(state: &AppState, run_id: Uuid, config: &crate::performance_testing::PerformanceTestConfig, handle: &crate::performance_testing::PerformanceRunHandle) {
    let samples = handle.samples.read().await.clone();
    // `run_workload` freezes this the instant it actually finishes — use
    // it in preference to a fresh `now - started_at` read, which would
    // include whatever real time has passed since (scheduling delay
    // between `run_workload` returning and this function actually running).
    let actual_duration_ms = match *handle.finished_elapsed_ms.read().await {
        Some(ms) => ms,
        None => (chrono::Utc::now() - handle.started_at).num_milliseconds().max(0) as u64,
    };
    let metrics = crate::performance_testing::compute_metrics(&samples, actual_duration_ms);
    let cancelled = handle.user_cancelled.load(std::sync::atomic::Ordering::SeqCst);
    let (verdict, reasons) = crate::performance_testing::evaluate_verdict(config, &metrics, cancelled);

    let evidence_id = match serde_json::to_string(&samples) {
        Ok(samples_json) => match state.evidence.put(&run_id.to_string(), "performance-test", "raw-samples", &samples_json) {
            Ok((object_key, size)) => state.store.insert_evidence(run_id, "performance-test", "raw-samples", &object_key, "application/json", size).await.ok(),
            Err(e) => {
                tracing::error!("failed to persist performance test raw samples for run {run_id}: {e}");
                None
            }
        },
        Err(e) => {
            tracing::error!("failed to serialize performance test raw samples for run {run_id}: {e}");
            None
        }
    };

    let status_str = if cancelled { "CANCELLED" } else { "COMPLETED" };
    let metrics_json = serde_json::to_string(&metrics).ok();
    let reasons_json = serde_json::to_string(&reasons).ok();
    if let Err(e) = state.store.finalize_performance_run(run_id, status_str, Some(verdict.as_str()), metrics_json.as_deref(), reasons_json.as_deref(), None, evidence_id).await {
        tracing::error!("failed to persist final performance test result for run {run_id}: {e}");
    }
}

/// Runs when `supervise_workload_task` reports that the workload task
/// itself failed unexpectedly (§ the fix this function exists for) — a
/// genuine, immediate ERROR finalization, never a silently orphaned
/// `RUNNING` row left for the next restart's recovery sweep to eventually
/// notice. Preserves whatever real samples the handle had already collected
/// before the failure (never invents the rest — §6 of the fix brief: "Never
/// invent missing samples"), freezes a real elapsed-time snapshot at the
/// moment of detection (the task is *provably* no longer running here —
/// `supervise_workload_task`'s own `join.await` only returns after the
/// spawned task has fully exited, so `now - started_at` at this exact point
/// is the real, true elapsed duration, not an approximation), and persists
/// only `failure`'s already-sanitized `safe_message` — never a raw panic
/// payload.
///
/// `pub(crate)`, not part of the public HTTP surface: this exists so
/// `api_tests.rs` can exercise the real persistence round-trip of a genuine,
/// deterministic supervision failure (obtained via a real panic through the
/// real `supervise_workload_task`) without needing to make the real
/// Load/Stress/Spike/Volume workload logic itself panic, which this fix
/// deliberately leaves untouched — see the fix report's own test-design
/// section for the full reasoning.
pub(crate) async fn finalize_performance_run_as_error(state: &AppState, run_id: Uuid, handle: &crate::performance_testing::PerformanceRunHandle, failure: &crate::performance_testing::SupervisionFailure) {
    let samples = handle.samples.read().await.clone();
    let elapsed_ms = (chrono::Utc::now() - handle.started_at).num_milliseconds().max(0) as u64;
    *handle.finished_elapsed_ms.write().await = Some(elapsed_ms);
    let metrics = crate::performance_testing::compute_metrics(&samples, elapsed_ms);

    *handle.error_reason.write().await = Some(failure.safe_message.clone());
    *handle.state.write().await = crate::performance_testing::PerfRunState::Error;

    let evidence_id = match serde_json::to_string(&samples) {
        Ok(samples_json) => match state.evidence.put(&run_id.to_string(), "performance-test", "raw-samples", &samples_json) {
            Ok((object_key, size)) => state.store.insert_evidence(run_id, "performance-test", "raw-samples", &object_key, "application/json", size).await.ok(),
            Err(e) => {
                tracing::error!("failed to persist partial performance test raw samples for run {run_id} after an internal execution failure: {e}");
                None
            }
        },
        Err(e) => {
            tracing::error!("failed to serialize partial performance test raw samples for run {run_id} after an internal execution failure: {e}");
            None
        }
    };

    let metrics_json = serde_json::to_string(&metrics).ok();
    // Also mirrored into `verdict_reasons_json` (not just the dedicated
    // `error_reason` column) so a reload after the live handle is gone
    // (`get_performance_run_status_scoped`'s durable-row fallback, which
    // only ever reads `verdict_reasons_json` — the same real field every
    // other terminal verdict already uses) still shows the real reason,
    // not an empty list. A real gap this fix's own tests caught: an earlier
    // version left this `None`, and a durable-fallback read of an ERROR run
    // showed no reason at all even though `error_reason` was separately
    // persisted.
    let reasons_json = serde_json::to_string(&[failure.safe_message.clone()]).ok();
    tracing::error!("performance test run {run_id} failed with a genuine internal execution error: {}", failure.safe_message);
    if let Err(e) = state
        .store
        .finalize_performance_run(run_id, "ERROR", Some(crate::performance_testing::PerfVerdict::Error.as_str()), metrics_json.as_deref(), reasons_json.as_deref(), Some(&failure.safe_message), evidence_id)
        .await
    {
        tracing::error!("failed to persist ERROR result for performance test run {run_id}: {e}");
    }
}

/// GET /api/v1/projects/:project_id/performance-testing/:run_id/status —
/// real, live progress while running (real elapsed time, real sample count,
/// real current concurrency, metrics recomputed from the real samples
/// gathered so far — never a fabricated percentage), falling back to the
/// durable row once no live handle exists in this process.
async fn get_performance_run_status_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<PerformanceRunStatusResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = state.store.get_performance_run(run_id).await.map_err(to_500)?.ok_or_else(|| ApiError::not_found("performance test run not found"))?;
    require_resource_in_project(Some(&row.project_id), project_id)?;

    if let Some(handle) = state.performance_tests.read().await.get(&run_id).cloned() {
        let config: crate::performance_testing::PerformanceTestConfig = serde_json::from_str(&row.config_json).map_err(to_500)?;
        let samples = handle.samples.read().await.clone();
        let sample_count = samples.len() as u64;
        // While RUNNING, real, live elapsed time (`now - started_at`) is
        // exactly right for a live progress read. Once terminal, the run's
        // own `finished_elapsed_ms` (frozen by `run_workload` the instant
        // it actually finished) must be used instead — otherwise every
        // later poll of an already-completed run would recompute
        // `metrics`/`throughput_rps` against a denominator that keeps
        // growing with wall-clock time even though nothing is still
        // happening, silently shrinking the reported throughput on repeat
        // reads (a real bug caught during this phase's own live
        // verification — see the report's bug log).
        let frozen_elapsed = *handle.finished_elapsed_ms.read().await;
        let elapsed_ms = match frozen_elapsed {
            Some(ms) => ms,
            None => (chrono::Utc::now() - handle.started_at).num_milliseconds().max(0) as u64,
        };
        let metrics = crate::performance_testing::compute_metrics(&samples, elapsed_ms);
        let run_state = *handle.state.read().await;
        // `PerfRunState::Error` is deliberately never routed through
        // `evaluate_verdict` — that function has no genuine ERROR-producing
        // path of its own (it only ever compares real metrics against
        // configured thresholds) and would misclassify a genuine task-level
        // failure as COMPLETED/PASSED/FAILED/CANCELLED from whatever partial
        // samples happened to exist. The real, safe reason
        // `finalize_performance_run_as_error` recorded is used verbatim.
        let (verdict, reasons, error_reason) = match run_state {
            crate::performance_testing::PerfRunState::Running => (None, Vec::new(), None),
            crate::performance_testing::PerfRunState::Error => {
                let reason = handle.error_reason.read().await.clone().unwrap_or_else(|| "performance execution terminated unexpectedly".to_string());
                (Some(crate::performance_testing::PerfVerdict::Error.as_str().to_string()), vec![reason.clone()], Some(reason))
            }
            _ => {
                let cancelled = handle.user_cancelled.load(std::sync::atomic::Ordering::SeqCst);
                let (v, r) = crate::performance_testing::evaluate_verdict(&config, &metrics, cancelled);
                (Some(v.as_str().to_string()), r, None)
            }
        };
        return Ok(Json(PerformanceRunStatusResponse {
            run_id,
            name: row.name,
            test_type: row.test_type,
            status: run_state.as_str().to_string(),
            verdict,
            verdict_reasons: reasons,
            error_reason,
            elapsed_ms,
            current_concurrency: handle.current_concurrency.load(std::sync::atomic::Ordering::SeqCst),
            sample_count,
            metrics: Some(metrics),
            samples,
            config,
            started_at: handle.started_at,
            finished_at: row.finished_at,
            evidence_id: row.evidence_id,
            live: true,
        }));
    }

    let config: crate::performance_testing::PerformanceTestConfig = serde_json::from_str(&row.config_json).map_err(to_500)?;
    let metrics = row.metrics_json.as_deref().and_then(|s| serde_json::from_str(s).ok());
    let reasons: Vec<String> = row.verdict_reasons_json.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
    let elapsed_ms = row.finished_at.unwrap_or_else(chrono::Utc::now).signed_duration_since(row.started_at).num_milliseconds().max(0) as u64;
    Ok(Json(PerformanceRunStatusResponse {
        run_id,
        name: row.name,
        test_type: row.test_type,
        status: row.status,
        verdict: row.verdict,
        verdict_reasons: reasons,
        error_reason: row.error_reason,
        elapsed_ms,
        current_concurrency: 0,
        sample_count: 0,
        metrics,
        samples: Vec::new(),
        config,
        started_at: row.started_at,
        finished_at: row.finished_at,
        evidence_id: row.evidence_id,
        live: false,
    }))
}

#[derive(Debug, Serialize)]
struct PerformanceTestCancelResponse {
    run_id: Uuid,
    cancelled: bool,
    reason: Option<&'static str>,
}

/// POST /api/v1/projects/:project_id/performance-testing/:run_id/cancel —
/// real cancellation (`PerformanceRunHandle::request_cancel`, races and
/// interrupts in-flight requests, not just "stop starting new ones"). Only
/// reachable when a live handle exists in *this* process — an honest
/// limitation disclosed via `reason`, not silently pretended to succeed,
/// same class of in-memory-only limitation `engine.rs`'s own run
/// cancellation already has.
async fn cancel_performance_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<PerformanceTestCancelResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = state.store.get_performance_run(run_id).await.map_err(to_500)?.ok_or_else(|| ApiError::not_found("performance test run not found"))?;
    require_resource_in_project(Some(&row.project_id), project_id)?;

    let Some(handle) = state.performance_tests.read().await.get(&run_id).cloned() else {
        audit(&state, &ctx, "performance_testing.cancel", Some(&run_id.to_string()), Some("no live handle in this process"), "unavailable").await;
        return Ok(Json(PerformanceTestCancelResponse {
            run_id,
            cancelled: false,
            reason: Some("no live handle for this run exists in this process — it may have already finished, or this process restarted since it started"),
        }));
    };
    let cancelled = handle.request_cancel().await;
    audit(&state, &ctx, "performance_testing.cancel", Some(&run_id.to_string()), None, if cancelled { "ok" } else { "already_terminal" }).await;
    Ok(Json(PerformanceTestCancelResponse { run_id, cancelled, reason: if cancelled { None } else { Some("run was already terminal") } }))
}

/// GET /api/v1/projects/:project_id/performance-testing — real, persisted
/// run history for this project, newest first.
async fn list_performance_runs_scoped(State(state): State<AppState>, Extension(ctx): Extension<AuthContext>, ApiPath(project_id): ApiPath<Uuid>) -> Result<Json<Vec<crate::store::PerformanceRunRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state.store.list_performance_runs(&project_id.to_string()).await.map_err(to_500)?;
    Ok(Json(rows))
}

// ------------------------------------------------------ AI Intelligence Layer

/// GET /api/v1/ai/status — real, current discovery snapshot + active
/// settings. Never triggers a fresh discovery pass itself (that's
/// `POST /api/v1/ai/refresh`, an explicit action) — this returns whatever
/// is already cached, honestly, including an empty provider list if
/// nothing has been discovered yet.
/// GET /api/v1/ai/status — real discovery snapshot + settings, plus a
/// real `routing_preview`: what `AiInfrastructure::route_among` would
/// actually select right now for this app's primary AI task
/// (`TEST_CASE_GENERATION`), computed with the exact same scoring
/// function real generation uses — never a separate, hand-picked "here's
/// what looks good" summary. `null` when no compatible model currently
/// exists, exactly like a real generation attempt would honestly fail.
async fn get_ai_status(State(state): State<AppState>, Extension(_ctx): Extension<AuthContext>) -> Json<serde_json::Value> {
    let providers = state.ai_infra.providers_snapshot().await;
    let settings = state.ai_infra.settings().await;
    let routing_preview = crate::ai_router::AiInfrastructure::route_among(
        &providers,
        crate::ai_router::AiTask::TestCaseGeneration,
        &settings,
        None,
        None,
    )
    .ok();
    Json(serde_json::json!({ "providers": providers, "settings": settings, "routing_preview": routing_preview }))
}

async fn get_ai_settings_handler(State(state): State<AppState>, Extension(_ctx): Extension<AuthContext>) -> Json<crate::ai_router::AiSettings> {
    Json(state.ai_infra.settings().await)
}

/// PUT /api/v1/ai/settings — the only mutating route in this layer.
/// Persists for real (`ai_settings`, a real singleton row) and updates the
/// in-memory router in the same request, so the very next generation call
/// already reflects the new mode/preference — never a setting that "looks"
/// saved but doesn't actually change routing behavior.
async fn put_ai_settings_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<crate::ai_router::AiSettings>,
) -> Result<Json<crate::ai_router::AiSettings>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let mode_str = match body.mode {
        crate::ai_router::RoutingMode::Auto => "AUTO",
        crate::ai_router::RoutingMode::Manual => "MANUAL",
    };
    state
        .store
        .upsert_ai_settings(mode_str, body.preferred_provider.as_deref(), body.preferred_model.as_deref(), body.fallback_enabled)
        .await
        .map_err(to_500)?;
    state.ai_infra.set_settings(body.clone()).await;
    audit(&state, &ctx, "ai.settings.update", None, Some(mode_str), "ok").await;
    Ok(Json(body))
}

/// POST /api/v1/ai/refresh — the explicit, on-demand re-discovery brief
/// §15 calls for ("Manual refresh"), distinct from the TTL-bounded lazy
/// refresh `generate` itself uses.
async fn refresh_ai_providers(State(state): State<AppState>, Extension(ctx): Extension<AuthContext>) -> Result<Json<Vec<crate::ai_router::AiProviderInfo>>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let providers = state.ai_infra.discover_all().await;
    audit(&state, &ctx, "ai.providers.refresh", None, Some(&format!("{} provider(s)", providers.len())), "ok").await;
    Ok(Json(providers))
}

// -------------------------------------------------------------- run create

#[derive(Deserialize)]
struct CreateRunQuery {
    /// Only consulted for a global/admin key (`ctx.project_id.is_none()`) —
    /// a project-scoped key's runs always belong to its own project.
    project_id: Option<String>,
}

#[derive(Serialize)]
struct CreateRunResponse {
    run_id: Uuid,
    state: RunState,
    policy_notes: Option<String>,
}

/// The actual run-creation logic (FRD §1 Execution Lifecycle: plan
/// validation -> dependency graph -> policy gate -> scheduling ->
/// persistence), shared verbatim by the legacy `POST /runs` handler and the
/// project-scoped `POST /api/v1/projects/:project_id/executions` handler —
/// the two differ only in *where their `project_id` comes from* (Step 7:
/// reuse existing logic rather than duplicate it). `project_id` here is
/// already resolved and, for the scoped caller, already verified against a
/// real project by `require_scoped_project`; this function does not decide
/// it, only uses it.
async fn create_run_core(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Option<String>,
    plan: RunPlan,
) -> Result<(StatusCode, Json<CreateRunResponse>), ApiError> {
    auth::require_role(ctx, Role::Operator)?;
    validate_plan(&plan).map_err(ApiError::validation)?;

    let decision = policy::evaluate(&state.policy, &plan);

    let (initial_state, policy_notes, spawn_now) = match &decision {
        PolicyDecision::Allow => (RunState::Scheduled, None, true),
        PolicyDecision::RequireApproval(reason) => (RunState::PendingApproval, Some(reason.clone()), false),
        PolicyDecision::Deny(reason) => {
            audit(state, ctx, "run.create", None, Some(reason), "denied").await;
            return Err(ApiError::forbidden(format!("policy denied: {reason}")));
        }
    };

    let run_id = Uuid::new_v4();
    state
        .store
        .insert_run(run_id, &plan, project_id.as_deref(), Some(&ctx.name), initial_state, policy_notes.as_deref())
        .await
        .map_err(to_500)?;

    // BE-14 — resolve this run's project realtime stream (if any) up front,
    // so `emit()` can fan every event into it without needing `AppState`
    // itself (keeps `engine.rs` decoupled from `state.rs`).
    let project_stream = match &project_id {
        Some(pid) => Some(state.project_stream(pid).await),
        None => None,
    };
    let record = RunRecord::new(run_id, &plan, project_id.clone(), Some(ctx.name.clone()), initial_state, policy_notes.clone(), project_stream);
    state.runs.write().await.insert(run_id, record.clone());

    audit(state, ctx, "run.create", Some(&run_id.to_string()), policy_notes.as_deref(), "ok").await;

    if spawn_now {
        engine::spawn_run(
            record,
            state.registry.clone(),
            plan.concurrency,
            state.store.clone(),
            state.evidence.clone(),
            engine::default_task_timeout(),
        );
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateRunResponse { run_id, state: initial_state, policy_notes }),
    ))
}

/// POST /runs — legacy, flat, unscoped. `project_id` comes from the
/// caller's own key scope, or (global keys only) the `?project_id=` query
/// param — unchanged from before this unit.
async fn create_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiQuery(q): ApiQuery<CreateRunQuery>,
    ApiJson(plan): ApiJson<RunPlan>,
) -> Result<(StatusCode, Json<CreateRunResponse>), ApiError> {
    let project_id = ctx.project_id.clone().or_else(|| q.project_id.clone());
    create_run_core(&state, &ctx, project_id, plan).await
}

// ---------------------------------------------------------------- listing

#[derive(Serialize)]
struct RunListItem {
    run_id: Uuid,
    name: String,
    state: RunState,
    total: usize,
    passed: usize,
    failed: usize,
    project_id: Option<String>,
}

#[derive(Deserialize)]
struct ListRunsQuery {
    limit: Option<i64>,
}

/// GET /runs — history now survives a process restart (SRD/PRD persistence
/// gap): every run id comes from the DB; an in-memory `RunRecord` (if this
/// process is the one driving it) is preferred for freshness, DB summary
/// otherwise. Scoped to the caller's project unless they hold a global key.
async fn list_runs(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiQuery(q): ApiQuery<ListRunsQuery>,
) -> Result<Json<Vec<RunListItem>>, ApiError> {
    let ids = state
        .store
        .list_run_ids(ctx.project_id.as_deref())
        .await
        .map_err(to_500)?;
    let limit = q.limit.unwrap_or(50).max(1) as usize;

    let mut items = Vec::new();
    for id in ids.into_iter().take(limit) {
        let summary = if let Some(record) = state.runs.read().await.get(&id).cloned() {
            record.summary().await
        } else {
            match state.store.get_run_summary(id).await.map_err(to_500)? {
                Some(s) => s,
                None => continue,
            }
        };
        items.push(RunListItem {
            run_id: summary.run_id,
            name: summary.name,
            state: summary.state,
            total: summary.total,
            passed: summary.passed,
            failed: summary.failed,
            project_id: summary.project_id,
        });
    }
    Ok(Json(items))
}

#[derive(Serialize)]
struct RunDetail {
    #[serde(flatten)]
    summary: RunSummary,
    tasks: Vec<TaskState>,
}

/// GET /runs/:id — FRD §7 Live Dashboard UX data source. Falls back to the
/// DB when the run isn't (or is no longer) in this process's memory, e.g.
/// after a restart — see README "Persistence".
async fn get_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<RunDetail>, ApiError> {
    if let Some(record) = state.runs.read().await.get(&id).cloned() {
        let summary = record.summary().await;
        auth::require_project_access(&ctx, summary.project_id.as_deref())?;
        let mut tasks: Vec<TaskState> = record.engine.read().await.tasks.values().cloned().collect();
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        return Ok(Json(RunDetail { summary, tasks }));
    }

    let summary = state
        .store
        .get_run_summary(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("run not found"))?;
    auth::require_project_access(&ctx, summary.project_id.as_deref())?;
    let mut tasks = state.store.list_tasks_for_run(id).await.map_err(to_500)?;
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Json(RunDetail { summary, tasks }))
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<u64>,
}

/// GET /runs/:id/events?since=N — SRD §18 reconnect path: "the client
/// requests authoritative run state plus the last acknowledged event
/// sequence." Now extended to survive a full process restart, not just a
/// dropped WebSocket, by falling back to the DB.
async fn get_events(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<EventsQuery>,
) -> Result<Json<Vec<RunEvent>>, ApiError> {
    let since = q.since.unwrap_or(0);
    if let Some(record) = state.runs.read().await.get(&id).cloned() {
        let summary = record.summary().await;
        auth::require_project_access(&ctx, summary.project_id.as_deref())?;
        let events: Vec<RunEvent> = record.events.read().await.iter().filter(|e| e.sequence >= since).cloned().collect();
        return Ok(Json(events));
    }
    let summary = state
        .store
        .get_run_summary(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("run not found"))?;
    auth::require_project_access(&ctx, summary.project_id.as_deref())?;
    let events = state.store.list_events_since(id, since).await.map_err(to_500)?;
    Ok(Json(events))
}

// ------------------------------------------------------------- governance

/// B11/L4 remediation. Rebuilds a live `Arc<RunRecord>` for a run that
/// exists durably in the store (its full `RunPlan` is persisted verbatim —
/// `store.rs::insert_run`) but is not held in *this* process's memory —
/// exactly the situation a restart leaves a still-`PENDING_APPROVAL` run
/// in, now that `recover_incomplete_runs` no longer destroys it (see that
/// function's doc comment). Inserts the rebuilt record into `state.runs`
/// so every subsequent lookup — this request's own state check, a later
/// `GET`, cancellation, WS subscription once it starts running — sees it
/// exactly as if this process had created it. `Ok(None)` (not an error)
/// when the run simply doesn't exist in the store at all.
async fn rehydrate_run_record(state: &AppState, run_id: Uuid) -> Result<Option<Arc<RunRecord>>, ApiError> {
    let Some((plan, project_id, requested_by, policy_notes, run_state)) =
        state.store.get_run_for_rehydration(run_id).await.map_err(to_500)?
    else {
        return Ok(None);
    };
    let project_stream = match &project_id {
        Some(pid) => Some(state.project_stream(pid).await),
        None => None,
    };
    let record = RunRecord::new(run_id, &plan, project_id, requested_by, run_state, policy_notes, project_stream);
    state.runs.write().await.insert(run_id, record.clone());
    Ok(Some(record))
}

/// Shared by all four approve/reject handlers (flat + scoped): in-memory
/// lookup first (the common, same-process case), falling back to
/// rehydration from the store (B11/L4). A 404 here means the run genuinely
/// does not exist anywhere, not merely that this process hasn't seen it.
async fn find_or_rehydrate_run(state: &AppState, run_id: Uuid) -> Result<Arc<RunRecord>, ApiError> {
    if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        return Ok(record);
    }
    rehydrate_run_record(state, run_id).await?.ok_or_else(|| ApiError::not_found("run not found"))
}

/// POST /runs/:id/approve — releases a run the policy engine held for
/// review (see `policy.rs`). B11/L4: works against a run created by a
/// different (or since-restarted) orchestrator process too, via
/// `find_or_rehydrate_run` above — there is still no cross-process *work
/// queue* (see README "Distributed workers / Redis"), but a durable,
/// still-pending approval is no longer lost to a restart the way it used
/// to be.
async fn approve_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<RunSummary>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let record = find_or_rehydrate_run(&state, id).await?;
    auth::require_project_access(&ctx, record.project_id.as_deref())?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err(ApiError::conflict(format!("run is in state {cur:?}, not PENDING_APPROVAL")));
        }
    }
    let concurrency = record.plan.concurrency;
    engine::approve_and_spawn(
        record.clone(),
        state.registry.clone(),
        concurrency,
        state.store.clone(),
        state.evidence.clone(),
        &ctx.name,
        engine::default_task_timeout(),
    )
    .await
    .map_err(to_500)?;
    audit(&state, &ctx, "run.approve", Some(&id.to_string()), None, "ok").await;
    Ok(Json(record.summary().await))
}

#[derive(Deserialize)]
struct RejectBody {
    #[serde(default)]
    reason: Option<String>,
}

async fn reject_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
    body: Option<ApiJson<RejectBody>>,
) -> Result<Json<RunSummary>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let record = find_or_rehydrate_run(&state, id).await?;
    auth::require_project_access(&ctx, record.project_id.as_deref())?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err(ApiError::conflict(format!("run is in state {cur:?}, not PENDING_APPROVAL")));
        }
    }
    let reason = body.and_then(|b| b.0.reason.clone()).unwrap_or_else(|| "no reason given".to_string());
    engine::reject_run(record.clone(), &state.store, &ctx.name, &reason)
        .await
        .map_err(to_500)?;
    audit(&state, &ctx, "run.reject", Some(&id.to_string()), Some(&reason), "ok").await;
    Ok(Json(record.summary().await))
}

// ---------------------------------------------------------------- evidence

async fn list_evidence(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::EvidenceRow>>, ApiError> {
    let summary = state
        .store
        .get_run_summary(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("run not found"))?;
    auth::require_project_access(&ctx, summary.project_id.as_deref())?;
    let rows = state.store.list_evidence_for_run(id).await.map_err(to_500)?;
    Ok(Json(rows))
}

async fn get_evidence_content(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Response, ApiError> {
    let (object_key, content_type, run_id) = state
        .store
        .get_evidence_object_key(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    if let Some(run_id) = run_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()) {
        if let Some(summary) = state.store.get_run_summary(run_id).await.map_err(to_500)? {
            auth::require_project_access(&ctx, summary.project_id.as_deref())?;
        }
    }
    let bytes = state
        .evidence
        .read(&object_key)
        .map_err(|e| ApiError::not_found(format!("evidence file unavailable: {e}")))?;
    Ok((
        StatusCode::OK,
        [("Content-Type", content_type.unwrap_or_else(|| "text/plain".to_string()))],
        bytes,
    )
        .into_response())
}

// --------------------------------------------- project-scoped executions
//
// API Contract §12 names this resource "executions" (`POST
// /api/v1/projects/{project_id}/executions`); §15.1/§15.2 name evidence
// under the same project scope. BE-13 (§12.1-12.5) aligned the *response*
// field name (`run_id`/`status`, see `CreateExecutionResponse`) and added
// the cancel/tasks endpoints below, but deliberately left the *request*
// schema alone: full alignment to Contract §12's own request concept
// (`test_case_ids`, `test_suite_id`, `execution_mode`) presupposes real
// test-case/suite entities that do not exist in this backend at all (§8
// Test Generation and §10 Suites/Plans are MISSING/PARTIAL-without-
// persistence per the Integration Plan's own gap matrix) — accepting those
// fields without real validation behind them would be exactly the fake-ID
// behavior this unit is told not to build. See
// AIQA_PHASE2_BE13_EXECUTION_ALIGNMENT_IMPLEMENTATION_REPORT.md §3 for the
// full reasoning.
//
// `/approve`, `/reject`, and `/events` are not enumerated in Contract §12
// at all, but are real, necessary, pre-existing functionality (the
// governance gate and the realtime catch-up path) with nowhere else to go
// under the new scheme; they are mounted as sub-resources of the same
// `executions/{run_id}` the contract does define, which Contract §2.1
// permits ("non-breaking additions may remain within the current
// version").

/// BE-13 (§12.1) response alignment: the contract's own field name is
/// `status`, not this backend's internal `state` — renamed here for the
/// contract-defined scoped endpoint only. The *value* is still this run's
/// real initial `RunState` (`SCHEDULED` or `PENDING_APPROVAL`, whichever the
/// policy gate actually produced) — the contract's own illustrative example
/// (`"status": "QUEUED"`) is not reproduced literally, because `QUEUED` is
/// not a value this backend's run-level state machine ever produces (§13's
/// canonical status list is task-level, not run-level; see the BE-13
/// report). Fabricating it would misrepresent the run's actual state. The
/// legacy `POST /runs` response (`CreateRunResponse`, `state` field) is
/// left completely unchanged.
#[derive(Serialize)]
struct CreateExecutionResponse {
    run_id: Uuid,
    status: RunState,
    policy_notes: Option<String>,
}

/// POST /api/v1/projects/:project_id/executions — project_id is canonical
/// from the URL (Step 3: never trust a project id elsewhere when the route
/// already carries one) — the legacy `?project_id=`/key-scope resolution
/// this route's sibling `create_run` uses does not apply here at all.
async fn create_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(plan): ApiJson<RunPlan>,
) -> Result<(StatusCode, Json<CreateExecutionResponse>), ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let (status_code, Json(body)) = create_run_core(&state, &ctx, Some(project_id.to_string()), plan).await?;
    Ok((status_code, Json(CreateExecutionResponse { run_id: body.run_id, status: body.state, policy_notes: body.policy_notes })))
}

/// GET /api/v1/projects/:project_id/executions — filtered to exactly the
/// URL's project, never the caller's own key scope (a global admin
/// listing project X's executions must see only X's, not everything).
async fn list_runs_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListRunsQuery>,
) -> Result<Json<Vec<RunListItem>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid_str = project_id.to_string();
    let ids = state.store.list_run_ids(Some(&pid_str)).await.map_err(to_500)?;
    let limit = q.limit.unwrap_or(50).max(1) as usize;

    let mut items = Vec::new();
    for id in ids.into_iter().take(limit) {
        let summary = if let Some(record) = state.runs.read().await.get(&id).cloned() {
            record.summary().await
        } else {
            match state.store.get_run_summary(id).await.map_err(to_500)? {
                Some(s) => s,
                None => continue,
            }
        };
        items.push(RunListItem {
            run_id: summary.run_id,
            name: summary.name,
            state: summary.state,
            total: summary.total,
            passed: summary.passed,
            failed: summary.failed,
            project_id: summary.project_id,
        });
    }
    Ok(Json(items))
}

/// Fetches a run's durable summary (falling back from the in-memory record
/// to the DB, same as every legacy handler) and confirms it belongs to
/// `project_id` — the shared core every `_scoped` run-detail handler below
/// uses, so that check is written exactly once.
async fn scoped_run_summary(state: &AppState, run_id: Uuid, project_id: Uuid) -> Result<RunSummary, ApiError> {
    let summary = if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        record.summary().await
    } else {
        state
            .store
            .get_run_summary(run_id)
            .await
            .map_err(to_500)?
            .ok_or_else(|| ApiError::not_found("run not found"))?
    };
    require_resource_in_project(summary.project_id.as_deref(), project_id)?;
    Ok(summary)
}

/// GET /api/v1/projects/:project_id/executions/:run_id
/// Fetches a run's tasks (in-memory record first, falling back to the DB —
/// same pattern `scoped_run_summary` uses), sorted by task id. Shared by
/// `get_run_scoped` (embeds them in `RunDetail`) and BE-13's standalone
/// §12.4/§12.5 task endpoints below, so there is exactly one place that
/// knows how to fetch a run's tasks.
async fn scoped_run_tasks(state: &AppState, run_id: Uuid) -> Result<Vec<TaskState>, ApiError> {
    let mut tasks = if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        record.engine.read().await.tasks.values().cloned().collect()
    } else {
        state.store.list_tasks_for_run(run_id).await.map_err(to_500)?
    };
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tasks)
}

async fn get_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<RunDetail>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let summary = scoped_run_summary(&state, run_id, project_id).await?;
    let tasks = scoped_run_tasks(&state, run_id).await?;
    Ok(Json(RunDetail { summary, tasks }))
}

// ---------------------------------------------------- BE-13 — run tasks

/// GET /api/v1/projects/:project_id/executions/:run_id/tasks (§12.4)
async fn list_run_tasks_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<Vec<TaskState>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    Ok(Json(scoped_run_tasks(&state, run_id).await?))
}

/// GET /api/v1/projects/:project_id/executions/:run_id/tasks/:task_id (§12.5)
async fn get_run_task_detail_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id, task_id)): ApiPath<(Uuid, Uuid, String)>,
) -> Result<Json<TaskState>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    let tasks = scoped_run_tasks(&state, run_id).await?;
    tasks
        .into_iter()
        .find(|t| t.id == task_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("task not found in this run"))
}

// -------------------------------------------------------- BE-13 — cancel

#[derive(Serialize)]
struct CancelRunResponse {
    run_id: Uuid,
    /// The run's state at the instant cancellation was accepted — execution
    /// is asynchronous (Contract §12 "All execution is asynchronous"), so
    /// this is not guaranteed to already be `CANCELLED`: an in-flight task's
    /// subprocess needs a moment to actually be killed. Poll `GET
    /// .../executions/:run_id` (or the event stream) to observe the final
    /// state — this response only confirms the request was accepted, it
    /// does not claim execution has already stopped.
    status: RunState,
}

/// POST /api/v1/projects/:project_id/executions/:run_id/cancel (§12.3).
///
/// "Cancellation must be policy-aware and auditable" (contract's entire
/// spec for this endpoint — no request/response schema is given). Resolved
/// here as: policy-aware = the same authorization gate every other mutating
/// execution route already uses (Operator role + project ownership — there
/// is no separate "cancel policy" concept anywhere in `policy.rs`, and
/// inventing one would be exactly the kind of undefined-semantics guess
/// this unit is told not to make); auditable = the existing generic
/// `audit_log` mechanism already used for run.create/approve/reject.
///
/// Only `Created`/`Scheduled`/`Running` runs are cancellable —
/// `PendingApproval` already has its own dedicated stop action (`/reject`),
/// and every other state is already terminal. See `RunState::is_terminal`'s
/// doc comment for the full reasoning.
async fn cancel_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<CancelRunResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    let summary = scoped_run_summary(&state, run_id, project_id).await?;

    if summary.state == RunState::PendingApproval {
        return Err(ApiError::conflict(
            "run is PENDING_APPROVAL; use POST .../reject to stop a run that has not started executing",
        ));
    }
    if summary.state.is_terminal() {
        return Err(ApiError::conflict(format!("run is already in a terminal state ({:?}), cannot be cancelled", summary.state)));
    }

    // A run whose persisted state is non-terminal but that is *not* held in
    // this process's memory would mean recovery (B2) has not yet run for it
    // — recovery forces every non-terminal run/task to a terminal state at
    // startup, so this should not happen. Fail safely rather than silently
    // no-op a cancel request that cannot actually reach anything.
    let record = state
        .runs
        .read()
        .await
        .get(&run_id)
        .cloned()
        .ok_or_else(|| ApiError::internal("run reports a live state but is not tracked in this process; cannot safely cancel"))?;

    record.request_cancel(&ctx.name).await;
    audit(&state, &ctx, "run.cancel", Some(&run_id.to_string()), None, "ok").await;

    let current_state = *record.state.read().await;
    Ok(Json(CancelRunResponse { run_id, status: current_state }))
}

// ------------------------------------------------------- dashboard / kpis

/// BE-12 (API Contract §23.1/§23.2). Only concepts this backend can
/// honestly compute from persisted state are included. §23's "Dashboard may
/// expose"/"Supported KPI concepts include" wording (reinforced by
/// Acceptance Matrix ACC-017's "KPI Coverage (Where implemented)") is a
/// permissive list, not a required schema — several listed concepts (report
/// counts, a computed project-health status, API latency, UI stability,
/// model/worker health, requirement/edge/business-logic/RBAC/security
/// coverage, test-generation accuracy) have no backing data model anywhere
/// in this codebase (no reports table, no test-generation/requirements
/// entities, no latency telemetry, no model gateway). They are
/// intentionally omitted rather than faked; see
/// AIQA_PHASE2_BE12_DASHBOARD_KPI_IMPLEMENTATION_REPORT.md for the full
/// list and reasoning.
#[derive(Serialize)]
struct DashboardResponse {
    project_id: Uuid,
    project_name: String,
    /// Run counts by state (§23.1 "execution counts").
    execution_counts: std::collections::BTreeMap<String, i64>,
    /// Task counts by status, across every run in this project.
    task_counts: std::collections::BTreeMap<String, i64>,
    /// §23.1 "pass/fail ratios" — PASSED vs FAILED task counts.
    pass_fail: PassFailCounts,
    /// §23.1 "test counts" — distinct known tests (`quality_nodes`,
    /// node_type='test') for this project.
    test_count: i64,
    generated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct PassFailCounts {
    passed: i64,
    failed: i64,
}

/// GET /api/v1/projects/:project_id/dashboard
async fn get_project_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<DashboardResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid = project_id.to_string();

    let project = state
        .store
        .get_project(project_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;

    let execution_counts: std::collections::BTreeMap<String, i64> =
        state.store.dashboard_run_counts(&pid).await.map_err(to_500)?.into_iter().collect();
    let task_counts: std::collections::BTreeMap<String, i64> =
        state.store.dashboard_task_counts(&pid).await.map_err(to_500)?.into_iter().collect();
    let passed = *task_counts.get("PASSED").unwrap_or(&0);
    let failed = *task_counts.get("FAILED").unwrap_or(&0);
    let test_count = state.store.dashboard_test_count(&pid).await.map_err(to_500)?;

    Ok(Json(DashboardResponse {
        project_id,
        project_name: project.name,
        execution_counts,
        task_counts,
        pass_fail: PassFailCounts { passed, failed },
        test_count,
        generated_at: chrono::Utc::now(),
    }))
}

/// §23.2 KPI concepts this backend can honestly derive today: `execution
/// reliability` (product pass rate — PASSED / (PASSED+FAILED+TIMEOUT),
/// mirroring `intelligence.rs`'s own product-vs-infrastructure split, which
/// deliberately excludes INFRA_ERROR/BLOCKED/SKIPPED/CANCELLED as not being
/// evidence the thing under test is broken), `execution efficiency`
/// (average task duration), and (BE-12b) average *run* duration. Every
/// other listed KPI concept has no backing data model — see
/// `DashboardResponse`'s doc comment.
#[derive(Serialize)]
struct KpisResponse {
    project_id: Uuid,
    /// PASSED / (PASSED+FAILED+TIMEOUT) among this project's tasks, or
    /// `null` when no task has reached one of those outcomes yet.
    execution_reliability: Option<f64>,
    /// Average task duration in milliseconds, or `null` when no task has
    /// both a recorded start and finish yet.
    execution_efficiency_ms: Option<f64>,
    /// BE-12b — average *run* duration in milliseconds, over every
    /// terminal run (any final `RunState`) that has both `started_at` and
    /// `finished_at`, or `null` when no such run exists yet. See
    /// `store.rs::dashboard_avg_run_duration_ms`'s own doc comment for the
    /// exact inclusion rule.
    execution_avg_run_duration_ms: Option<f64>,
    generated_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/v1/projects/:project_id/kpis
async fn get_project_kpis(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<KpisResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let pid = project_id.to_string();

    let task_counts: std::collections::BTreeMap<String, i64> =
        state.store.dashboard_task_counts(&pid).await.map_err(to_500)?.into_iter().collect();
    let passed = *task_counts.get("PASSED").unwrap_or(&0);
    let failed = *task_counts.get("FAILED").unwrap_or(&0);
    let timeout = *task_counts.get("TIMEOUT").unwrap_or(&0);
    let terminal_product_outcomes = passed + failed + timeout;
    let execution_reliability =
        if terminal_product_outcomes > 0 { Some(passed as f64 / terminal_product_outcomes as f64) } else { None };

    let execution_efficiency_ms = state.store.dashboard_avg_task_duration_ms(&pid).await.map_err(to_500)?;
    let execution_avg_run_duration_ms = state.store.dashboard_avg_run_duration_ms(&pid).await.map_err(to_500)?;

    Ok(Json(KpisResponse {
        project_id,
        execution_reliability,
        execution_efficiency_ms,
        execution_avg_run_duration_ms,
        generated_at: chrono::Utc::now(),
    }))
}

/// GET /api/v1/projects/:project_id/executions/:run_id/events?since=N
async fn get_events_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(q): ApiQuery<EventsQuery>,
) -> Result<Json<Vec<RunEvent>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    let since = q.since.unwrap_or(0);

    if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
        let events: Vec<RunEvent> = record.events.read().await.iter().filter(|e| e.sequence >= since).cloned().collect();
        return Ok(Json(events));
    }
    let events = state.store.list_events_since(run_id, since).await.map_err(to_500)?;
    Ok(Json(events))
}

/// POST /api/v1/projects/:project_id/executions/:run_id/approve
async fn approve_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<RunSummary>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let record = find_or_rehydrate_run(&state, run_id).await?;
    require_resource_in_project(record.project_id.as_deref(), project_id)?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err(ApiError::conflict(format!("run is in state {cur:?}, not PENDING_APPROVAL")));
        }
    }
    let concurrency = record.plan.concurrency;
    engine::approve_and_spawn(
        record.clone(),
        state.registry.clone(),
        concurrency,
        state.store.clone(),
        state.evidence.clone(),
        &ctx.name,
        engine::default_task_timeout(),
    )
    .await
    .map_err(to_500)?;
    audit(&state, &ctx, "run.approve", Some(&run_id.to_string()), None, "ok").await;
    Ok(Json(record.summary().await))
}

/// POST /api/v1/projects/:project_id/executions/:run_id/reject
async fn reject_run_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
    body: Option<ApiJson<RejectBody>>,
) -> Result<Json<RunSummary>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let record = find_or_rehydrate_run(&state, run_id).await?;
    require_resource_in_project(record.project_id.as_deref(), project_id)?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err(ApiError::conflict(format!("run is in state {cur:?}, not PENDING_APPROVAL")));
        }
    }
    let reason = body.and_then(|b| b.0.reason.clone()).unwrap_or_else(|| "no reason given".to_string());
    engine::reject_run(record.clone(), &state.store, &ctx.name, &reason)
        .await
        .map_err(to_500)?;
    audit(&state, &ctx, "run.reject", Some(&run_id.to_string()), Some(&reason), "ok").await;
    Ok(Json(record.summary().await))
}

/// GET /api/v1/projects/:project_id/executions/:run_id/evidence — API
/// Contract §15.1, exact path match.
async fn list_evidence_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<Vec<crate::store::EvidenceRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_run_summary(&state, run_id, project_id).await?;
    let rows = state.store.list_evidence_for_run(run_id).await.map_err(to_500)?;
    Ok(Json(rows))
}

/// GET /api/v1/projects/:project_id/evidence/:evidence_id — API Contract
/// §15.2 "Evidence Detail". BE-15: this used to serve the raw binary
/// content directly at this exact path (the same behavior as the legacy
/// `/evidence/:id/content`) — a genuine path match but not a shape match:
/// the contract names this "Detail" (metadata, like every other "X Detail"
/// endpoint in it, e.g. §8.4) and separately says "Large binary artifacts
/// SHOULD be accessed through secure artifact references rather than
/// embedded in normal API responses." The raw bytes are still fully
/// available — moved to the sibling `/content` path below, not removed —
/// and this path now returns the same `EvidenceRow` metadata shape §15.1's
/// list endpoint already uses per item.
async fn get_evidence_detail_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, evidence_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::EvidenceRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let (row, run_id) = state
        .store
        .get_evidence_by_id(evidence_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    let run_id = run_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    scoped_run_summary(&state, run_id, project_id).await?;
    Ok(Json(row))
}

/// GET /api/v1/projects/:project_id/evidence/:evidence_id/content — the
/// raw binary artifact `get_evidence_detail_scoped` above points at (BE-15;
/// same body this route served before the split). The evidence's *owning
/// run's* project is what must match the URL, since evidence itself
/// carries no `project_id` of its own (same resolve-through-the-run
/// pattern the legacy `get_evidence_content` already uses).
async fn get_evidence_content_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, evidence_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let (object_key, content_type, run_id) = state
        .store
        .get_evidence_object_key(evidence_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    let run_id = run_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ApiError::not_found("evidence not found"))?;
    scoped_run_summary(&state, run_id, project_id).await?;

    let bytes = state
        .evidence
        .read(&object_key)
        .map_err(|e| ApiError::not_found(format!("evidence file unavailable: {e}")))?;
    Ok((
        StatusCode::OK,
        [("Content-Type", content_type.unwrap_or_else(|| "text/plain".to_string()))],
        bytes,
    )
        .into_response())
}

// ------------------------------------------------------------- findings

/// GET /api/v1/projects/:project_id/findings — API Contract §16.1.
/// "Finding" is this backend's real `bug_drafts` entity (title,
/// description, severity, confidence, `review_state` — including
/// `NEEDS_REVIEW`, the contract's own required distinction between an AI
/// hypothesis and a confirmed fact), reused verbatim rather than
/// reinvented under a new name; see the BE-15 report §2 for the full
/// reasoning.
async fn list_findings_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::BugDraftRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state.store.list_bug_drafts(Some(project_id.to_string().as_str())).await.map_err(to_500)?;
    Ok(Json(rows))
}

/// A finding (`bug_drafts` row) carries no `project_id` of its own —
/// ownership resolves through its failure cluster, exactly like
/// `review_bug_draft`'s existing project check. Shared so every findings
/// route below checks it identically.
async fn require_finding_in_project(state: &AppState, finding_id: Uuid, project_id: Uuid) -> Result<(), ApiError> {
    let owner = state
        .store
        .bug_draft_project(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    require_resource_in_project(Some(owner.as_str()), project_id)
}

/// GET /api/v1/projects/:project_id/findings/:finding_id — API Contract
/// §16.2.
async fn get_finding_detail_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<crate::store::BugDraftRow>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    require_finding_in_project(&state, finding_id, project_id).await?;
    let row = state
        .store
        .get_bug_draft_by_id(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    Ok(Json(row))
}

/// POST /api/v1/projects/:project_id/findings/:finding_id/analysis — API
/// Contract §16.3. "Failure analysis may correlate: execution evidence,
/// environment information, historical failures, logs." This backend's
/// real, already-computed correlation for a finding is its underlying
/// failure cluster (root-cause hypothesis, signature, occurrence count,
/// reproducibility — all real, deterministic output of
/// `intelligence::classify_failure`, never an LLM call; see that module's
/// doc comment). No separate "environment"/"log" linkage exists anywhere
/// in this schema to correlate against, so none is fabricated — see the
/// BE-15 report for exactly what was and wasn't included and why. No
/// request body: the contract gives none, and there is nothing to
/// validate.
#[derive(Serialize)]
struct FindingAnalysisResponse {
    finding: crate::store::BugDraftRow,
    failure_cluster: crate::store::FailureClusterRow,
}

async fn get_finding_analysis_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<FindingAnalysisResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    require_finding_in_project(&state, finding_id, project_id).await?;
    let finding = state
        .store
        .get_bug_draft_by_id(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    let failure_cluster = state
        .store
        .get_failure_cluster_by_id(finding.failure_cluster_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::internal("finding references a failure cluster that no longer exists"))?;
    audit(&state, &ctx, "finding.analysis", Some(&finding_id.to_string()), None, "ok").await;
    Ok(Json(FindingAnalysisResponse { finding, failure_cluster }))
}

/// POST /api/v1/projects/:project_id/findings/:finding_id/ai-hypothesis —
/// build order item 4 (22 August 2026). Not a Contract §16 endpoint (same
/// footing as `.../review` above) — real, on-demand AI-assisted root-cause
/// analysis, layered on top of the real, already-computed failure cluster
/// `.../analysis` above already returns. Reuses the exact `generation_method`/
/// `provider`/`model`/`fallback_reason` response shape
/// `generate_ai_test_cases_scoped` established for `AiTask::TestCaseGeneration`
/// — a `"fallback"` outcome is a normal 200 here too, per ACC-018's own
/// Failure Behavior criteria ("controlled fallback, or NEEDS_REVIEW — never
/// fabricated output"): the endpoint did its real job (attempted the AI call
/// and honestly reports what happened), and the failure cluster's real,
/// pre-existing deterministic data is untouched and still returned. Gated on
/// `Role::Operator`, same bar as `.../review` and `generate-ai` — this
/// triggers a real, potentially slow model call, not a cheap read.
#[derive(Serialize)]
struct FindingAiHypothesisResponse {
    finding: crate::store::BugDraftRow,
    failure_cluster: crate::store::FailureClusterRow,
    generation_method: String,
    provider: Option<String>,
    model: Option<String>,
    fallback_reason: Option<String>,
}

async fn generate_finding_ai_hypothesis_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<FindingAiHypothesisResponse>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    require_finding_in_project(&state, finding_id, project_id).await?;

    let finding = state
        .store
        .get_bug_draft_by_id(finding_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("finding not found"))?;
    let failure_cluster = state
        .store
        .get_failure_cluster_by_id(finding.failure_cluster_id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::internal("finding references a failure cluster that no longer exists"))?;

    let rule_based_hypothesis = failure_cluster.root_cause_hypothesis.clone().unwrap_or_default();
    let outcome = crate::intelligence::generate_ai_hypothesis(
        &state.ai_infra,
        &failure_cluster.signature,
        &rule_based_hypothesis,
        failure_cluster.occurrence_count,
    )
    .await;

    let response = match outcome {
        crate::intelligence::AiHypothesisOutcome::Ai { hypothesis, provider, model } => {
            state
                .store
                .set_ai_hypothesis(failure_cluster.id, &hypothesis, &provider, &model)
                .await
                .map_err(to_500)?;
            let refreshed = state
                .store
                .get_failure_cluster_by_id(failure_cluster.id)
                .await
                .map_err(to_500)?
                .ok_or_else(|| ApiError::internal("failure cluster vanished immediately after being updated"))?;
            audit(
                &state,
                &ctx,
                "finding.ai_hypothesis",
                Some(&finding_id.to_string()),
                Some(&format!("via {provider}/{model}")),
                "ok",
            )
            .await;
            FindingAiHypothesisResponse {
                finding,
                failure_cluster: refreshed,
                generation_method: "ai".to_string(),
                provider: Some(provider),
                model: Some(model),
                fallback_reason: None,
            }
        }
        crate::intelligence::AiHypothesisOutcome::Failed { reason } => {
            audit(&state, &ctx, "finding.ai_hypothesis", Some(&finding_id.to_string()), Some(&reason), "fallback").await;
            FindingAiHypothesisResponse {
                finding,
                failure_cluster,
                generation_method: "fallback".to_string(),
                provider: None,
                model: None,
                fallback_reason: Some(reason),
            }
        }
    };

    Ok(Json(response))
}

/// POST /api/v1/projects/:project_id/findings/:finding_id/review — not a
/// Contract §16 endpoint (the contract defines no review/promotion
/// action), but real, necessary, pre-existing functionality
/// (`review_bug_draft`/legacy `POST /bug-drafts/:id/review`) with nowhere
/// else to go under the new scheme; mounted as a sub-resource of the
/// contract-defined `findings/{finding_id}` the same way Actual BE-9
/// mounted `/approve`/`/reject`/`/events` under `executions/{run_id}`
/// (Contract §2.1: "non-breaking additions may remain within the current
/// version"). Reuses `review_bug_draft`'s exact logic, not a second copy.
async fn review_finding_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, finding_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<ReviewBody>,
) -> Result<StatusCode, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    require_finding_in_project(&state, finding_id, project_id).await?;
    let new_state = match body.action.as_str() {
        "confirm" => "CONFIRMED",
        "dismiss" => "DISMISSED",
        other => return Err(ApiError::validation(format!("unknown action '{other}', expected 'confirm' or 'dismiss'"))),
    };
    state.store.review_bug_draft(finding_id, new_state, &ctx.name).await.map_err(to_500)?;
    audit(&state, &ctx, "finding.review", Some(&finding_id.to_string()), Some(new_state), "ok").await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ audit (§25)

/// GET /api/v1/projects/:project_id/audit — API Contract §25.1. Unlike the
/// legacy `/admin/audit-log` (global, `Role::Admin`-only), this is a
/// project's own audit trail — visibility follows the same bar every other
/// scoped resource in this project uses (`Role::Operator` + project
/// access), not the global-admin bar, since seeing your own project's
/// history is not a cross-tenant action. The contract specifies no
/// per-endpoint role, so this is a resolved (documented, not blocking)
/// scope decision — see the BE-15 report.
async fn get_audit_log_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> Result<Json<Vec<crate::store::AuditRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    auth::require_role(&ctx, Role::Operator)?;
    let rows = state
        .store
        .list_audit(Some(project_id.to_string().as_str()), q.limit.unwrap_or(100).clamp(1, 1000))
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

// ----------------------------------------------------------- intelligence

/// GET /api/v1/projects/:project_id/intelligence/tests — Phase 4 BE-10
/// (distinct from the pre-existing Phase 2 "BE-10" in `errors.rs`/
/// `api_tests.rs`, which is about extractor-rejection error envelopes and
/// is unrelated to and untouched by this route). The legacy
/// `/intelligence/tests` below scopes itself to the caller's own
/// key-bound `ctx.project_id`, which has no URL parameter at all — safe
/// only for a single-project key, and silently wrong (or empty, via the
/// `"default"` tenant fallback in `store.rs::list_test_intelligence`) for
/// an unscoped/multi-project key browsing a specific project, exactly the
/// key pattern this frontend's project switcher relies on (see FE-21's
/// report §3/§9). This route closes that gap the same way every other
/// resource under `/api/v1/projects/:project_id/...` already does: the
/// project is explicit in the URL, `require_scoped_project` confirms both
/// that it exists and that the caller's key may access it, and the query
/// is always given that real project id — never the caller's own
/// (possibly different, possibly absent) bound project, never the
/// `"default"` tenant. No new intelligence computation, no new table, no
/// new response shape — `store.rs::list_test_intelligence`/`TestIntelRow`
/// are reused exactly as they already exist.
async fn list_test_intelligence_scoped(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Vec<crate::store::TestIntelRow>>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_test_intelligence(Some(project_id.to_string().as_str()))
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

/// GET /intelligence/tests — legacy, unscoped. Kept exactly as-is (Phase 4
/// BE-10 does not touch this handler or its behavior); nothing in this backend
/// or its tests depends on it changing, and the project-scoped route above
/// coexists with it the same way every other legacy/scoped pair in this
/// router already does (see the router's own "Legacy, flat, unscoped
/// routes" comment).
async fn list_test_intelligence(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::TestIntelRow>>, ApiError> {
    let rows = state
        .store
        .list_test_intelligence(ctx.project_id.as_deref())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

async fn list_failure_clusters(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::FailureClusterRow>>, ApiError> {
    let rows = state
        .store
        .list_failure_clusters(ctx.project_id.as_deref())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

/// GET /bug-drafts — B14: scoped to the caller's project.
///
/// This previously bound `Extension(_ctx)`, discarding the auth context
/// entirely, and called an unfiltered query — so a project-scoped *viewer*
/// key received every tenant's drafts. A global/admin key (`project_id:
/// None`) still sees everything, matching `list_runs`.
async fn list_bug_drafts(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::BugDraftRow>>, ApiError> {
    let rows = state
        .store
        .list_bug_drafts(ctx.project_id.as_deref())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct ReviewBody {
    action: String, // "confirm" | "dismiss"
}

/// POST /bug-drafts/:id/review — the explicit, audited, human-gated
/// anomaly -> confirmed-defect promotion step the original PRD critique
/// flagged as undefined (PRD §14). Never happens automatically.
async fn review_bug_draft(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<ReviewBody>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let new_state = match body.action.as_str() {
        "confirm" => "CONFIRMED",
        "dismiss" => "DISMISSED",
        other => {
            return Err(ApiError::validation(format!(
                "unknown action '{other}', expected 'confirm' or 'dismiss'"
            )))
        }
    };

    // B14 — this mutation previously performed no project check at all, so a
    // project-scoped operator could confirm or dismiss another tenant's
    // draft. Resolve the owning project first and reuse the same gate every
    // other scoped resource uses.
    let owner = state
        .store
        .bug_draft_project(id)
        .await
        .map_err(to_500)?
        .ok_or_else(|| ApiError::not_found("bug draft not found"))?;
    auth::require_project_access(&ctx, Some(owner.as_str()))?;

    state.store.review_bug_draft(id, new_state, &ctx.name).await.map_err(to_500)?;
    audit(&state, &ctx, "bug_draft.review", Some(&id.to_string()), Some(new_state), "ok").await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------- governance admin

#[derive(Deserialize)]
struct CreateApiKeyBody {
    name: String,
    role: String, // "admin" | "operator" | "viewer"
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Serialize)]
struct CreateApiKeyResponse {
    id: Uuid,
    key: String,
}

/// POST /admin/api-keys — Admin-only. The plaintext key is returned exactly
/// once, at creation time (same convention as GitHub/Stripe tokens); only
/// its hash is ever persisted (`auth::hash_key`).
async fn create_api_key(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiJson(body): ApiJson<CreateApiKeyBody>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let role = match body.role.as_str() {
        "admin" | "operator" | "viewer" => body.role.as_str(),
        other => return Err(ApiError::validation(format!("unknown role '{other}'"))),
    };
    let id = Uuid::new_v4();
    let raw = auth::generate_key();
    state
        .store
        .insert_api_key(id, &body.name, &auth::hash_key(&raw), role, body.project_id.as_deref())
        .await
        .map_err(to_500)?;
    audit(&state, &ctx, "apikey.create", Some(&id.to_string()), Some(&format!("name={} role={role}", body.name)), "ok").await;
    Ok(Json(CreateApiKeyResponse { id, key: raw }))
}

async fn list_api_keys_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::ApiKeyListRow>>, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let rows = state.store.list_api_keys().await.map_err(to_500)?;
    Ok(Json(rows))
}

async fn revoke_api_key_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    state.store.revoke_api_key(id).await.map_err(to_500)?;
    audit(&state, &ctx, "apikey.revoke", Some(&id.to_string()), None, "ok").await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
}

async fn get_audit_log(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> Result<Json<Vec<crate::store::AuditRow>>, ApiError> {
    auth::require_role(&ctx, Role::Admin)?;
    let rows = state.store.list_audit(None, q.limit.unwrap_or(100).clamp(1, 1000)).await.map_err(to_500)?;
    Ok(Json(rows))
}

// ----------------------------------------------------------- observability

async fn get_metrics(State(state): State<AppState>, Extension(_ctx): Extension<AuthContext>) -> Response {
    let body = crate::metrics::render(&state.store.pool).await;
    (StatusCode::OK, [("Content-Type", "text/plain; version=0.0.4")], body).into_response()
}

// --------------------------------------------------------------------- ws

#[derive(Deserialize)]
struct WsAuthQuery {
    key: Option<String>,
}

/// GET /runs/:id/ws — SRD §6 Real-Time Data Flow. Authenticated via a `?key=`
/// query parameter instead of the `Authorization` header the rest of the
/// API uses, because browsers cannot set custom headers on a WebSocket
/// handshake (`new WebSocket(url)` takes no headers argument) — this is the
/// standard workaround, same as e.g. many streaming APIs' `?access_token=`.
/// On connect, replays the durable event log first (so a fresh client sees
/// everything that already happened), then forwards new events as they're
/// broadcast.
async fn ws_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<WsAuthQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let ctx = match q.key.as_deref() {
        Some(key) => match state.store.find_api_key_by_hash(&auth::hash_key(key)).await {
            Ok(Some(row)) => AuthContext {
                key_id: row.id,
                name: row.name,
                role: Role::parse(&row.role),
                project_id: row.project_id,
            },
            _ => return (StatusCode::UNAUTHORIZED, "invalid or revoked API key").into_response(),
        },
        None => return (StatusCode::UNAUTHORIZED, "missing ?key= query parameter").into_response(),
    };

    let record = { state.runs.read().await.get(&id).cloned() };
    match record {
        Some(record) => {
            let summary = record.summary().await;
            if auth::require_project_access(&ctx, summary.project_id.as_deref()).is_err() {
                return (StatusCode::FORBIDDEN, "resource belongs to a different project").into_response();
            }
            ws.on_upgrade(move |socket| handle_socket(socket, record))
        }
        None => (StatusCode::NOT_FOUND, "run not found (or not held in this process's memory)").into_response(),
    }
}

async fn handle_socket(mut socket: WebSocket, record: Arc<RunRecord>) {
    let rx = record.tx.subscribe();

    let backlog = record.events.read().await.clone();
    for ev in backlog {
        if socket
            .send(Message::Text(serde_json::to_string(&ev).unwrap()))
            .await
            .is_err()
        {
            return;
        }
    }

    drive_socket(socket, rx).await;
}

/// BE-14 — API Contract §14. Project-scoped counterpart to `/runs/:id/ws`:
/// multiplexes every run in this project's real events onto one stream
/// (Integration Plan §11.2 item 1), keeping the per-run socket above as
/// internal plumbing rather than replacing it. Auth reuses the exact same
/// `?key=` technique as the per-run socket (browsers cannot set headers on
/// a WS upgrade; see that handler's doc comment, and G-WS in the
/// Integration Plan for the broader, deliberately-not-resolved-here
/// question of whether that technique should eventually change) and the
/// same project-access model every REST scoped route already uses
/// (`auth::require_project_access`) — the project id in the URL does not
/// itself grant access.
async fn project_events_ws_handler(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(q): Query<WsAuthQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let ctx = match q.key.as_deref() {
        Some(key) => match state.store.find_api_key_by_hash(&auth::hash_key(key)).await {
            Ok(Some(row)) => AuthContext {
                key_id: row.id,
                name: row.name,
                role: Role::parse(&row.role),
                project_id: row.project_id,
            },
            _ => return (StatusCode::UNAUTHORIZED, "invalid or revoked API key").into_response(),
        },
        None => return (StatusCode::UNAUTHORIZED, "missing ?key= query parameter").into_response(),
    };

    match state.store.get_project(project_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "project not found").into_response(),
        Err(e) => {
            tracing::error!("project_events_ws_handler: get_project failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    }
    if auth::require_project_access(&ctx, Some(project_id.to_string().as_str())).is_err() {
        return (StatusCode::FORBIDDEN, "resource belongs to a different project").into_response();
    }

    // No backlog replay here: unlike a single run's small, bounded event
    // list, "every event from every run this project has ever had" is not
    // something to hold in memory or dump on connect. Contract §14/ACC-008
    // catch-up is `GET .../executions/:run_id/events?since=`, per-run, and
    // continues to work unchanged (Step 7's honest ordering/delivery
    // semantics: this stream delivers events occurring *after* a
    // successful subscription, nothing before it).
    let stream = state.project_stream(&project_id.to_string()).await;
    let rx = stream.tx.subscribe();
    ws.on_upgrade(move |socket| drive_socket(socket, rx))
}

/// Shared consumption loop for both the per-run and project-scoped
/// sockets: forward broadcast events to the client, watch for the client
/// closing, and stop cleanly either way so no subscriber/task/channel is
/// leaked (dropping `rx` on return unsubscribes it from the `broadcast`
/// channel automatically).
async fn drive_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<RunEvent>) {
    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Ok(ev) => {
                        if socket.send(Message::Text(serde_json::to_string(&ev).unwrap())).await.is_err() {
                            break;
                        }
                    }
                    // BE-14 (Integration Plan §11.2 item 5 / audit M5) — a
                    // slow consumer previously `continue`d silently here,
                    // meaning missed events left no trace at all: a client
                    // trusting a gap-free stream would be silently wrong.
                    // Closing with a distinct code makes the gap visible
                    // and tells the client to reconnect and re-catch-up via
                    // the REST endpoint, rather than inventing an in-band
                    // "resync marker" event that never came from real
                    // execution state.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        let _ = socket
                            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: 4001,
                                reason: format!("lagged behind by {skipped} events; reconnect and catch up via REST").into(),
                            })))
                            .await;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}
