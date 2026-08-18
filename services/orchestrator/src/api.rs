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
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
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
        .route("/api/v1/projects/:project_id/findings/:finding_id/review", post(review_finding_scoped))
        .route("/api/v1/projects/:project_id/audit", get(get_audit_log_scoped))
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

/// POST /runs/:id/approve — releases a run the policy engine held for
/// review (see `policy.rs`). Only works against the process that currently
/// holds the run's `RunRecord` in memory: approving a run created by a
/// different (or since-restarted) orchestrator process isn't supported in
/// this slice (there's no cross-process work queue — see README
/// "Distributed workers / Redis").
async fn approve_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<RunSummary>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    let record = state
        .runs
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("run not held in this process's memory (see README)"))?;
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
    let record = state
        .runs
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("run not held in this process's memory (see README)"))?;
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
/// evidence the thing under test is broken) and `execution efficiency`
/// (average task duration). Every other listed KPI concept has no backing
/// data model — see `DashboardResponse`'s doc comment.
#[derive(Serialize)]
struct KpisResponse {
    project_id: Uuid,
    /// PASSED / (PASSED+FAILED+TIMEOUT) among this project's tasks, or
    /// `null` when no task has reached one of those outcomes yet.
    execution_reliability: Option<f64>,
    /// Average task duration in milliseconds, or `null` when no task has
    /// both a recorded start and finish yet.
    execution_efficiency_ms: Option<f64>,
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

    Ok(Json(KpisResponse {
        project_id,
        execution_reliability,
        execution_efficiency_ms,
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
    let record = state
        .runs
        .read()
        .await
        .get(&run_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("run not held in this process's memory (see README)"))?;
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
    let record = state
        .runs
        .read()
        .await
        .get(&run_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("run not held in this process's memory (see README)"))?;
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
