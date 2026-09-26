//! The routes of scheduled regression (Phase 8C): make and change schedules, run one now, read what each run found and how it compares
//! with the run before it. `regression_runner.rs` starts the runs, `regression.rs` decides and reports.
//!
//! A schedule can be made disabled: it never runs by itself and is run when asked (`.../run`), which is how a CI job uses it.

use crate::api::{
    audit, require_scoped_project, scoped_run_summary, scoped_run_tasks, scoped_test_suite, to_500,
};
use crate::auth::{self, AuthContext, Role};
use crate::ci;
use crate::domain::TaskStatus;
use crate::errors::{ApiError, ApiJson, ApiPath, ApiQuery};
use crate::openapi_runs::{outcome_of, TaskOutcome};
use crate::regression::{
    build_report, check_concurrency, check_every, check_name, next_slot_after, schedule_view,
    to_markdown, Before, BeforeFrom, RegressionRunRow, ScheduleRow, DEFAULT_CONCURRENCY,
    LIST_LIMIT, MAX_SCHEDULES_PER_PROJECT,
};
use crate::regression_runner::{self, Blocked};
use crate::state::AppState;
use axum::{
    extract::{Extension, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

/// A field that may be given as `null` to mean "none", told apart from a field that is not given at all.
fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// The body of making a schedule (`name` and `every_minutes` are needed) and of changing one (only what is given changes).
#[derive(Debug, Deserialize)]
pub(crate) struct ScheduleBody {
    name: Option<String>,
    /// A suite of the project; `null` for every approved case of the project.
    #[serde(default, deserialize_with = "nullable")]
    suite_id: Option<Option<Uuid>>,
    every_minutes: Option<i64>,
    /// The first time it is due, and the grid the later ones fall on. Made schedules: one interval from now when not given.
    start_at: Option<DateTime<Utc>>,
    concurrency: Option<i64>,
    enabled: Option<bool>,
}

async fn scoped_schedule(
    state: &AppState,
    project_id: Uuid,
    id: Uuid,
) -> Result<ScheduleRow, ApiError> {
    state
        .store
        .get_regression_schedule(id)
        .await
        .map_err(to_500)?
        .filter(|s| s.project_id == project_id.to_string())
        .ok_or_else(|| ApiError::not_found("regression schedule not found in this project"))
}

async fn checked_suite(
    state: &AppState,
    ctx: &AuthContext,
    project_id: Uuid,
    suite: Option<Uuid>,
) -> Result<Option<String>, ApiError> {
    match suite {
        Some(id) => {
            scoped_test_suite(state, ctx, project_id, id).await?;
            Ok(Some(id.to_string()))
        }
        None => Ok(None),
    }
}

/// POST /api/v1/projects/:project_id/regression/schedules (Operator role).
pub(crate) async fn create_schedule(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<ScheduleBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let name = check_name(
        body.name
            .as_deref()
            .ok_or_else(|| ApiError::validation("name is required"))?,
    )
    .map_err(ApiError::validation)?;
    let every = body
        .every_minutes
        .ok_or_else(|| ApiError::validation("every_minutes is required"))?;
    check_every(every).map_err(ApiError::validation)?;
    let concurrency = body.concurrency.unwrap_or(DEFAULT_CONCURRENCY);
    check_concurrency(concurrency).map_err(ApiError::validation)?;
    let suite_id = checked_suite(&state, &ctx, project_id, body.suite_id.flatten()).await?;
    if state
        .store
        .count_regression_schedules(&project_id.to_string())
        .await
        .map_err(to_500)?
        >= MAX_SCHEDULES_PER_PROJECT
    {
        return Err(ApiError::conflict(format!(
            "a project may have at most {MAX_SCHEDULES_PER_PROJECT} schedules"
        )));
    }
    let now = Utc::now();
    let anchor = body
        .start_at
        .unwrap_or_else(|| now + chrono::Duration::minutes(every));
    let row = ScheduleRow {
        id: Uuid::new_v4(),
        project_id: project_id.to_string(),
        name,
        suite_id,
        every_minutes: every,
        anchor_at: anchor,
        next_run_at: next_slot_after(anchor, every, now),
        concurrency,
        enabled: body.enabled.unwrap_or(true),
        last_started_at: None,
        last_note: None,
        created_by_key: ctx.key_id,
        created_by: ctx.name.clone(),
        created_at: now,
    };
    state
        .store
        .insert_regression_schedule(&row)
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "regression.schedule.create",
        Some(&row.id.to_string()),
        Some(&format!(
            "{}, every {} minute(s)",
            row.name, row.every_minutes
        )),
        "ok",
    )
    .await;
    Ok((StatusCode::CREATED, Json(schedule_view(&row, None))))
}

/// PATCH /api/v1/projects/:project_id/regression/schedules/:id (Operator role): only what is given changes. Turning a schedule on, or
/// changing its interval or start, puts its next run on the next slot of its grid.
pub(crate) async fn update_schedule(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(body): ApiJson<ScheduleBody>,
) -> Result<Json<Value>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let mut row = scoped_schedule(&state, project_id, id).await?;
    let now = Utc::now();
    let mut regrid = false;
    if let Some(name) = &body.name {
        row.name = check_name(name).map_err(ApiError::validation)?;
    }
    if let Some(every) = body.every_minutes {
        check_every(every).map_err(ApiError::validation)?;
        if every != row.every_minutes {
            row.every_minutes = every;
            regrid = true;
        }
    }
    if let Some(start) = body.start_at {
        row.anchor_at = start;
        regrid = true;
    }
    if let Some(n) = body.concurrency {
        check_concurrency(n).map_err(ApiError::validation)?;
        row.concurrency = n;
    }
    if let Some(suite) = body.suite_id {
        row.suite_id = checked_suite(&state, &ctx, project_id, suite).await?;
    }
    if let Some(enabled) = body.enabled {
        if enabled && !row.enabled {
            regrid = true;
            row.last_note = None;
        }
        row.enabled = enabled;
    }
    if regrid {
        row.next_run_at = next_slot_after(row.anchor_at, row.every_minutes, now);
    }
    state
        .store
        .update_regression_schedule(&row)
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "regression.schedule.update",
        Some(&row.id.to_string()),
        Some(&row.name),
        "ok",
    )
    .await;
    Ok(Json(schedule_view(&row, None)))
}

/// DELETE /api/v1/projects/:project_id/regression/schedules/:id (Operator role): the schedule and its history; the executions stay.
pub(crate) async fn delete_schedule(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let row = scoped_schedule(&state, project_id, id).await?;
    state
        .store
        .delete_regression_schedule(id)
        .await
        .map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "regression.schedule.delete",
        Some(&id.to_string()),
        Some(&row.name),
        "ok",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/projects/:project_id/regression/schedules/:id/run (Operator role): a run now, as the person asking. A schedule keeps
/// its slots. Answers at once; the report fills in as the tasks finish.
pub(crate) async fn run_schedule_now(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let schedule = scoped_schedule(&state, project_id, id).await?;
    match regression_runner::run_now(&state, &schedule, &ctx, Utc::now()).await {
        Ok(run) => Ok((
            StatusCode::CREATED,
            Json(json!({
                "id": run.id,
                "run_id": run.run_id,
                "cases": run.cases.len(),
                "skipped": run.skipped.len(),
            })),
        )),
        Err(Blocked::StillRunning(_)) => Err(ApiError::conflict(
            "the run before this one has not ended yet",
        )),
        Err(other) => Err(ApiError::validation(other.note())),
    }
}

// ------------------------------------------------------------------------------------------------------------------------ reading

/// What each case of a run came to the last time: in the run of the same schedule before it when that repeated the case and got an answer,
/// else in the run the case's task was copied from.
async fn before_of(
    state: &AppState,
    row: &RegressionRunRow,
) -> Result<BTreeMap<String, Before>, ApiError> {
    let previous = state
        .store
        .previous_regression_run(row)
        .await
        .map_err(to_500)?;
    let previous = match previous {
        Some(run) => {
            let found = outcomes(state, run.run_id).await?;
            Some((run, found))
        }
        None => None,
    };
    let mut sources: BTreeMap<String, BTreeMap<String, TaskOutcome>> = BTreeMap::new();
    let mut before = BTreeMap::new();
    for case in &row.cases {
        let answered = |status: TaskStatus| {
            let outcome = outcome_of(status);
            matches!(outcome, "PASSED" | "FAILED" | "ERROR").then(|| outcome.to_string())
        };
        if let Some((run, found)) = &previous {
            let earlier = run
                .cases
                .iter()
                .find(|c| c.case_id == case.case_id)
                .and_then(|c| found.get(&c.task_id))
                .and_then(|t| answered(t.status));
            if let Some(outcome) = earlier {
                before.insert(
                    case.case_id.clone(),
                    Before {
                        outcome,
                        from: BeforeFrom::PreviousRun,
                    },
                );
                continue;
            }
        }
        if let Ok(source) = Uuid::parse_str(&case.source_run_id) {
            if !sources.contains_key(&case.source_run_id) {
                let found = outcomes(state, source).await?;
                sources.insert(case.source_run_id.clone(), found);
            }
            if let Some(outcome) = sources[&case.source_run_id]
                .get(&case.task_id)
                .and_then(|t| answered(t.status))
            {
                before.insert(
                    case.case_id.clone(),
                    Before {
                        outcome,
                        from: BeforeFrom::SourceRun,
                    },
                );
            }
        }
    }
    Ok(before)
}

async fn outcomes(
    state: &AppState,
    run_id: Uuid,
) -> Result<BTreeMap<String, TaskOutcome>, ApiError> {
    Ok(scoped_run_tasks(state, run_id)
        .await?
        .into_iter()
        .map(|task| {
            (
                task.id.clone(),
                TaskOutcome {
                    status: task.status,
                    reason: task.reason.clone(),
                    result: None,
                },
            )
        })
        .collect())
}

/// The report of a regression run, made now from the execution's tasks.
pub(crate) async fn report_of(
    state: &AppState,
    project_id: Uuid,
    row: &RegressionRunRow,
) -> Result<Value, ApiError> {
    let schedule_name = state
        .store
        .get_regression_schedule(row.schedule_id)
        .await
        .map_err(to_500)?
        .map(|s| s.name)
        .unwrap_or_default();
    // An execution that is no longer there is a run that ended with nothing to show.
    let (run_state, final_run) = match scoped_run_summary(state, row.run_id, project_id).await {
        Ok(summary) => (
            serde_json::to_value(summary.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            summary.state.is_terminal(),
        ),
        Err(_) => ("GONE".to_string(), true),
    };
    let tasks = outcomes(state, row.run_id).await?;
    let before = before_of(state, row).await?;
    Ok(build_report(
        row,
        &schedule_name,
        &run_state,
        final_run,
        &tasks,
        &before,
    ))
}

/// A run in a list: when, how it was started and how it came out, not its cases.
async fn account_of(
    state: &AppState,
    project_id: Uuid,
    row: &RegressionRunRow,
) -> Result<Value, ApiError> {
    let report = report_of(state, project_id, row).await?;
    Ok(json!({
        "id": row.id,
        "run_id": row.run_id,
        "created_at": row.created_at,
        "trigger": row.trigger.as_str(),
        "run_state": report["run_state"],
        "final": report["final"],
        "verdict": report["verdict"],
        "summary": report["summary"],
        "skipped": row.skipped.len(),
    }))
}

/// GET /api/v1/projects/:project_id/regression/schedules (Viewer role): each with an account of its latest run.
pub(crate) async fn list_schedules(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let mut out = Vec::new();
    for row in state
        .store
        .list_regression_schedules(&project_id.to_string())
        .await
        .map_err(to_500)?
    {
        let latest = state
            .store
            .list_regression_runs(row.id, 1)
            .await
            .map_err(to_500)?
            .into_iter()
            .next();
        let last_run = match latest {
            Some(run) => Some(account_of(&state, project_id, &run).await?),
            None => None,
        };
        out.push(schedule_view(&row, last_run));
    }
    Ok(Json(json!({ "schedules": out })))
}

/// GET /api/v1/projects/:project_id/regression/schedules/:id/runs (Viewer role): the schedule's latest runs, newest first.
pub(crate) async fn list_runs(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    scoped_schedule(&state, project_id, id).await?;
    let mut runs = Vec::new();
    for run in state
        .store
        .list_regression_runs(id, LIST_LIMIT)
        .await
        .map_err(to_500)?
    {
        runs.push(account_of(&state, project_id, &run).await?);
    }
    Ok(Json(json!({ "runs": runs })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReportQuery {
    format: Option<String>,
    /// Seconds to wait for the run to end (at most 300): the report comes when it has, or when the wait is over.
    wait: Option<u64>,
}

/// GET /api/v1/projects/:project_id/regression/runs/:id (Viewer role; `?format=markdown` gives text to paste, `?format=junit` gives
/// JUnit XML for a CI system, `?wait=` waits for the run to end): what each case came to, beside what it came to the last time,
/// regressions first. While the run goes it says what it has so far, and that it has not ended. `X-AIQA-Verdict` and `X-AIQA-Final`
/// say how it came out without reading the body.
pub(crate) async fn get_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(query): ApiQuery<ReportQuery>,
) -> Result<Response, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let format = query.format.as_deref().unwrap_or("json");
    if !matches!(format, "json" | "markdown" | "junit") {
        return Err(ApiError::validation(
            "format must be json, markdown or junit",
        ));
    }
    let wait = ci::check_wait(query.wait)?;
    let row = state
        .store
        .get_regression_run(id)
        .await
        .map_err(to_500)?
        .filter(|r| r.project_id == project_id.to_string())
        .ok_or_else(|| ApiError::not_found("regression run not found in this project"))?;
    let report = ci::until_final(wait, || report_of(&state, project_id, &row)).await?;
    let response = match format {
        "markdown" => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            to_markdown(&report),
        )
            .into_response(),
        "junit" => (
            [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
            crate::junit::regression_junit(&report),
        )
            .into_response(),
        _ => Json(&report).into_response(),
    };
    Ok(ci::with_verdict(response, &report))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ScopeQuery {
    suite_id: Option<Uuid>,
}

/// GET /api/v1/projects/:project_id/regression/scope?suite_id= (Viewer role): what a schedule over that suite (or the whole project)
/// would repeat now, and what it could not and why. Starts nothing.
pub(crate) async fn get_scope(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<ScopeQuery>,
) -> Result<Json<Value>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let suite = checked_suite(&state, &ctx, project_id, query.suite_id).await?;
    let found = regression_runner::scope(&state, &project_id.to_string(), suite.as_deref())
        .await
        .map_err(to_500)?;
    Ok(Json(json!({
        "repeatable": found.tasks.iter().map(|(case, _)| json!({
            "case_id": case.case_id,
            "title": case.title,
            "source_run_id": case.source_run_id,
        })).collect::<Vec<_>>(),
        "skipped": found.skipped,
    })))
}
