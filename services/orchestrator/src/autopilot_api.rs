//! The routes of the autopilot (`autopilot.rs`): make a run, look at it, start it once its estimate is accepted, cancel it.
//!
//! Making a run costs nothing and does nothing: the run is stored with the estimate a person is shown, and waits. It starts
//! only when they say they accept that estimate (and, when it would call a provider that charges, that cost too). From
//! then on it works through this service's own API as the key that made it (`autopilot_pipeline.rs`), so it can do nothing
//! that key could not.

use crate::api::{
    audit, build_autopilot_estimate, require_scoped_project, scoped_run_summary, to_500,
};
use crate::auth::{self, AuthContext, Role};
use crate::autopilot::{check_acceptance, event, AutopilotConfig, RunState};
use crate::autopilot_store::{
    AutopilotEventRow, AutopilotRunRow, AutopilotStepRow, CancelOutcome, NewAutopilotRun,
};
use crate::errors::{ApiError, ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Runs in a project's list.
const LIST_LIMIT: i64 = 50;
/// Events in a run's detail, and the most one request for events may ask for.
const DETAIL_EVENTS: i64 = 200;
const MAX_EVENTS: i64 = 500;

/// A run, its steps and its timeline.
#[derive(Debug, Serialize)]
pub(crate) struct RunDetail {
    run: AutopilotRunRow,
    steps: Vec<AutopilotStepRow>,
    events: Vec<AutopilotEventRow>,
    /// Whether this service is working on the run at this moment. A run that is `RUNNING` and is not being worked on is
    /// waiting to be picked up again, which the service does when it starts.
    working: bool,
}

async fn load_run(
    state: &AppState,
    project_id: Uuid,
    run_id: Uuid,
) -> Result<AutopilotRunRow, ApiError> {
    let run = state
        .store
        .get_autopilot_run(run_id)
        .await
        .map_err(to_500)?
        .filter(|run| run.project_id == project_id.to_string())
        .ok_or_else(|| ApiError::not_found("autopilot run not found in this project"))?;
    Ok(run)
}

async fn detail_of(state: &AppState, run: AutopilotRunRow) -> Result<RunDetail, ApiError> {
    let steps = state
        .store
        .list_autopilot_steps(run.id)
        .await
        .map_err(to_500)?;
    let events = state
        .store
        .list_autopilot_events(run.id, None, DETAIL_EVENTS)
        .await
        .map_err(to_500)?;
    Ok(RunDetail {
        working: state.autopilot.is_working_on(run.id),
        run,
        steps,
        events,
    })
}

/// POST /api/v1/projects/:project_id/autopilot/runs — makes a run from what a person asks for, with its estimate, and
/// starts nothing. Operator. A project has one run that has not ended at a time.
pub(crate) async fn create_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(mut config): ApiJson<AutopilotConfig>,
) -> Result<(StatusCode, Json<RunDetail>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    config.base_url = config.base_url.trim().to_string();
    config
        .check()
        .map_err(|problems| ApiError::validation(problems.join("; ")))?;

    // The address and the roles' login steps are held to what a crawl would hold them to, and the variables those steps
    // read must be set: a run that could only fail at its crawl is refused now.
    let plan = crate::crawler::CrawlPlan {
        base_url: config.base_url.clone(),
        roles: config.roles.clone(),
    };
    crate::crawler::validate_crawl_plan(&plan)
        .await
        .map_err(|reasons| ApiError::validation(reasons.join("; ")))?;
    let missing =
        crate::execution_plan::missing_secrets(&crate::crawler::crawl_secret_names(&plan));
    if !missing.is_empty() {
        return Err(ApiError::validation(
            crate::execution_plan::missing_secrets_message(&missing),
        ));
    }
    if let Some(crawl) = config.crawl_run_id {
        let summary = scoped_run_summary(&state, crawl, project_id).await?;
        if !summary.name.starts_with("Crawl:") {
            return Err(ApiError::validation(format!("run {crawl} is not a crawl")));
        }
        if summary.state != crate::domain::RunState::Completed {
            return Err(ApiError::validation(format!(
                "the crawl {crawl} has not finished successfully: it is {}",
                summary.state.as_db_str()
            )));
        }
    }

    let estimate = build_autopilot_estimate(&state, project_id, &config.estimate_request()).await?;
    if estimate.requirements == 0 {
        return Err(ApiError::validation(
            if config.requirement_document_ids.is_empty() {
                "there are no requirements to test: upload a requirements document first (Test Case Generator, Source & Generation)"
            } else {
                "the documents you chose have no requirements to test"
            },
        ));
    }

    let id = Uuid::new_v4();
    let name = config.display_name(chrono::Utc::now());
    let config_json = serde_json::to_value(&config).map_err(to_500)?;
    let estimate_json = serde_json::to_value(&estimate).map_err(to_500)?;
    let made = state
        .store
        .insert_autopilot_run(
            &NewAutopilotRun {
                id,
                project_id: &project_id.to_string(),
                name: &name,
                config: &config_json,
                estimate: &estimate_json,
                created_by_key: ctx.key_id,
                created_by_name: &ctx.name,
            },
            &config
                .steps()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        )
        .await
        .map_err(to_500)?;
    if !made {
        let active = state
            .store
            .find_active_autopilot_run(&project_id.to_string())
            .await
            .map_err(to_500)?;
        return Err(ApiError::conflict(match active {
            Some(run) => format!(
                "this project already has an autopilot run that has not ended: \"{}\" ({}, {}). Let it finish or cancel it first",
                run.name, run.id, run.state
            ),
            None => "this project already has an autopilot run that has not ended".to_string(),
        }));
    }
    audit(
        &state,
        &ctx,
        "autopilot.create",
        Some(&id.to_string()),
        Some(&name),
        "ok",
    )
    .await;
    let run = load_run(&state, project_id, id).await?;
    Ok((StatusCode::CREATED, Json(detail_of(&state, run).await?)))
}

/// GET /api/v1/projects/:project_id/autopilot/runs — the project's runs, newest first. Viewer.
pub(crate) async fn list_runs(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let runs = state
        .store
        .list_autopilot_runs(&project_id.to_string(), LIST_LIMIT)
        .await
        .map_err(to_500)?;
    Ok(Json(serde_json::json!({ "runs": runs })))
}

/// GET /api/v1/projects/:project_id/autopilot/runs/:run_id — a run with its steps and the latest of its timeline. Viewer.
pub(crate) async fn get_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<RunDetail>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let run = load_run(&state, project_id, run_id).await?;
    Ok(Json(detail_of(&state, run).await?))
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct StartBody {
    /// Says the person has read the estimate. Required.
    #[serde(default)]
    accept_estimate: bool,
    /// Says the person accepts calling a provider that charges. Required when the estimate says the run would.
    #[serde(default)]
    accept_paid: bool,
}

/// POST /api/v1/projects/:project_id/autopilot/runs/:run_id/start — the person accepts the run's estimate, and the run
/// starts. Operator. Nothing is charged for making a run; this is where the cost is accepted.
pub(crate) async fn start_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
    body: Option<ApiJson<StartBody>>,
) -> Result<Json<RunDetail>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let body = body.map(|ApiJson(b)| b).unwrap_or_default();
    let run = load_run(&state, project_id, run_id).await?;
    if run.state != RunState::AwaitingApproval.as_str() {
        return Err(ApiError::conflict(format!(
            "this run is {}, not awaiting approval",
            run.state
        )));
    }
    let paid = check_acceptance(&run.estimate, body.accept_estimate, body.accept_paid)
        .map_err(ApiError::validation)?;
    if !state
        .store
        .start_autopilot_run(run_id, paid)
        .await
        .map_err(to_500)?
    {
        // Another request started (or cancelled) it between the look above and this.
        return Err(ApiError::conflict(
            "this run is no longer awaiting approval",
        ));
    }
    let _ = state
        .store
        .add_autopilot_event(
            run_id,
            event::STARTED,
            None,
            &format!(
                "Estimate accepted by {}{}; the run started",
                ctx.name,
                if paid { ", including its cost" } else { "" }
            ),
            Some(&serde_json::json!({"accepted_paid": paid})),
        )
        .await;
    audit(
        &state,
        &ctx,
        "autopilot.start",
        Some(&run_id.to_string()),
        Some(if paid { "paid" } else { "free" }),
        "ok",
    )
    .await;
    let run = load_run(&state, project_id, run_id).await?;
    crate::autopilot_runner::spawn_run(&state, &run);
    Ok(Json(detail_of(&state, run).await?))
}

/// What cancelling did, with the run as it is now.
#[derive(Debug, Serialize)]
pub(crate) struct CancelResponse {
    /// `cancelled` (the run was not being worked on and has ended) or `requested` (it is being worked on and stops at
    /// the next point it can).
    outcome: &'static str,
    #[serde(flatten)]
    detail: RunDetail,
}

/// POST /api/v1/projects/:project_id/autopilot/runs/:run_id/cancel — stops a run. Operator.
pub(crate) async fn cancel_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<CancelResponse>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    load_run(&state, project_id, run_id).await?;
    let outcome = match state
        .store
        .cancel_autopilot_run(run_id)
        .await
        .map_err(to_500)?
    {
        CancelOutcome::Cancelled => "cancelled",
        CancelOutcome::Requested => "requested",
        CancelOutcome::AlreadyEnded(state) => {
            return Err(ApiError::conflict(format!(
                "this run has already ended: it is {}",
                state.as_str()
            )))
        }
        CancelOutcome::NotFound => {
            return Err(ApiError::not_found(
                "autopilot run not found in this project",
            ))
        }
    };
    audit(
        &state,
        &ctx,
        "autopilot.cancel",
        Some(&run_id.to_string()),
        Some(outcome),
        "ok",
    )
    .await;
    let run = load_run(&state, project_id, run_id).await?;
    Ok(Json(CancelResponse {
        outcome,
        detail: detail_of(&state, run).await?,
    }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventsQuery {
    /// The id of the last event the reader has; only later ones are returned.
    #[serde(default)]
    after: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

/// GET /api/v1/projects/:project_id/autopilot/runs/:run_id/events — a run's timeline, oldest first: the latest events, or
/// those after `?after=<event id>` (what a page that is watching a run asks for). Viewer.
pub(crate) async fn list_events(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, run_id)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(query): ApiQuery<EventsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    load_run(&state, project_id, run_id).await?;
    let limit = query.limit.unwrap_or(DETAIL_EVENTS).clamp(1, MAX_EVENTS);
    let events = state
        .store
        .list_autopilot_events(run_id, query.after, limit)
        .await
        .map_err(to_500)?;
    Ok(Json(serde_json::json!({ "events": events })))
}
