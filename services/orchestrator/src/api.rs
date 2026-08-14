use crate::auth::{self, AuthContext, Role};
use crate::domain::*;
use crate::engine::{self, RunRecord};
use crate::policy::{self, PolicyDecision};
use crate::scheduler::validate_plan;
use crate::state::AppState;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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
        .merge(protected)
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

fn to_500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
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

/// POST /runs — FRD §1 Execution Lifecycle: "Run creation -> plan validation
/// -> dependency graph creation -> task scheduling -> parallel execution
/// ...", now with a policy-engine gate in between validation and scheduling
/// (`policy.rs`) and durable persistence of every stage (`store.rs`).
async fn create_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(q): Query<CreateRunQuery>,
    Json(plan): Json<RunPlan>,
) -> Result<(StatusCode, Json<CreateRunResponse>), (StatusCode, String)> {
    auth::require_role(&ctx, Role::Operator)?;
    validate_plan(&plan).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let project_id = ctx.project_id.clone().or_else(|| q.project_id.clone());
    let decision = policy::evaluate(&state.policy, &plan);

    let (initial_state, policy_notes, spawn_now) = match &decision {
        PolicyDecision::Allow => (RunState::Scheduled, None, true),
        PolicyDecision::RequireApproval(reason) => (RunState::PendingApproval, Some(reason.clone()), false),
        PolicyDecision::Deny(reason) => {
            audit(&state, &ctx, "run.create", None, Some(reason), "denied").await;
            return Err((StatusCode::FORBIDDEN, format!("policy denied: {reason}")));
        }
    };

    let run_id = Uuid::new_v4();
    state
        .store
        .insert_run(run_id, &plan, project_id.as_deref(), Some(&ctx.name), initial_state, policy_notes.as_deref())
        .await
        .map_err(to_500)?;

    let record = RunRecord::new(run_id, &plan, project_id.clone(), Some(ctx.name.clone()), initial_state, policy_notes.clone());
    state.runs.write().await.insert(run_id, record.clone());

    audit(&state, &ctx, "run.create", Some(&run_id.to_string()), policy_notes.as_deref(), "ok").await;

    if spawn_now {
        engine::spawn_run(record, state.registry.clone(), plan.concurrency, state.store.clone(), state.evidence.clone());
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateRunResponse { run_id, state: initial_state, policy_notes }),
    ))
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
    Query(q): Query<ListRunsQuery>,
) -> Result<Json<Vec<RunListItem>>, (StatusCode, String)> {
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
    Path(id): Path<Uuid>,
) -> Result<Json<RunDetail>, (StatusCode, String)> {
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
        .ok_or((StatusCode::NOT_FOUND, "run not found".to_string()))?;
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
    Path(id): Path<Uuid>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<Vec<RunEvent>>, (StatusCode, String)> {
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
        .ok_or((StatusCode::NOT_FOUND, "run not found".to_string()))?;
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
    Path(id): Path<Uuid>,
) -> Result<Json<RunSummary>, (StatusCode, String)> {
    auth::require_role(&ctx, Role::Operator)?;
    let record = state
        .runs
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "run not held in this process's memory (see README)".to_string()))?;
    auth::require_project_access(&ctx, record.project_id.as_deref())?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err((StatusCode::CONFLICT, format!("run is in state {cur:?}, not PENDING_APPROVAL")));
        }
    }
    let concurrency = record.plan.concurrency;
    engine::approve_and_spawn(record.clone(), state.registry.clone(), concurrency, state.store.clone(), state.evidence.clone(), &ctx.name).await;
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
    Path(id): Path<Uuid>,
    body: Option<Json<RejectBody>>,
) -> Result<Json<RunSummary>, (StatusCode, String)> {
    auth::require_role(&ctx, Role::Operator)?;
    let record = state
        .runs
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "run not held in this process's memory (see README)".to_string()))?;
    auth::require_project_access(&ctx, record.project_id.as_deref())?;
    {
        let cur = *record.state.read().await;
        if cur != RunState::PendingApproval {
            return Err((StatusCode::CONFLICT, format!("run is in state {cur:?}, not PENDING_APPROVAL")));
        }
    }
    let reason = body.and_then(|b| b.reason.clone()).unwrap_or_else(|| "no reason given".to_string());
    engine::reject_run(record.clone(), &state.store, &ctx.name, &reason).await;
    audit(&state, &ctx, "run.reject", Some(&id.to_string()), Some(&reason), "ok").await;
    Ok(Json(record.summary().await))
}

// ---------------------------------------------------------------- evidence

async fn list_evidence(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::store::EvidenceRow>>, (StatusCode, String)> {
    let summary = state
        .store
        .get_run_summary(id)
        .await
        .map_err(to_500)?
        .ok_or((StatusCode::NOT_FOUND, "run not found".to_string()))?;
    auth::require_project_access(&ctx, summary.project_id.as_deref())?;
    let rows = state.store.list_evidence_for_run(id).await.map_err(to_500)?;
    Ok(Json(rows))
}

async fn get_evidence_content(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> Result<Response, (StatusCode, String)> {
    let (object_key, content_type, run_id) = state
        .store
        .get_evidence_object_key(id)
        .await
        .map_err(to_500)?
        .ok_or((StatusCode::NOT_FOUND, "evidence not found".to_string()))?;
    if let Some(run_id) = run_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()) {
        if let Some(summary) = state.store.get_run_summary(run_id).await.map_err(to_500)? {
            auth::require_project_access(&ctx, summary.project_id.as_deref())?;
        }
    }
    let bytes = state
        .evidence
        .read(&object_key)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("evidence file unavailable: {e}")))?;
    Ok((
        StatusCode::OK,
        [("Content-Type", content_type.unwrap_or_else(|| "text/plain".to_string()))],
        bytes,
    )
        .into_response())
}

// ----------------------------------------------------------- intelligence

async fn list_test_intelligence(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::TestIntelRow>>, (StatusCode, String)> {
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
) -> Result<Json<Vec<crate::store::FailureClusterRow>>, (StatusCode, String)> {
    let rows = state
        .store
        .list_failure_clusters(ctx.project_id.as_deref())
        .await
        .map_err(to_500)?;
    Ok(Json(rows))
}

async fn list_bug_drafts(
    State(state): State<AppState>,
    Extension(_ctx): Extension<AuthContext>,
) -> Result<Json<Vec<crate::store::BugDraftRow>>, (StatusCode, String)> {
    let rows = state.store.list_bug_drafts().await.map_err(to_500)?;
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
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth::require_role(&ctx, Role::Operator)?;
    let new_state = match body.action.as_str() {
        "confirm" => "CONFIRMED",
        "dismiss" => "DISMISSED",
        other => return Err((StatusCode::BAD_REQUEST, format!("unknown action '{other}', expected 'confirm' or 'dismiss'"))),
    };
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
    Json(body): Json<CreateApiKeyBody>,
) -> Result<Json<CreateApiKeyResponse>, (StatusCode, String)> {
    auth::require_role(&ctx, Role::Admin)?;
    let role = match body.role.as_str() {
        "admin" | "operator" | "viewer" => body.role.as_str(),
        other => return Err((StatusCode::BAD_REQUEST, format!("unknown role '{other}'"))),
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
) -> Result<Json<Vec<crate::store::ApiKeyListRow>>, (StatusCode, String)> {
    auth::require_role(&ctx, Role::Admin)?;
    let rows = state.store.list_api_keys().await.map_err(to_500)?;
    Ok(Json(rows))
}

async fn revoke_api_key_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
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
    Query(q): Query<AuditQuery>,
) -> Result<Json<Vec<crate::store::AuditRow>>, (StatusCode, String)> {
    auth::require_role(&ctx, Role::Admin)?;
    let rows = state.store.list_audit(q.limit.unwrap_or(100).clamp(1, 1000)).await.map_err(to_500)?;
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
    let mut rx = record.tx.subscribe();

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

    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Ok(ev) => {
                        if socket.send(Message::Text(serde_json::to_string(&ev).unwrap())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
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
