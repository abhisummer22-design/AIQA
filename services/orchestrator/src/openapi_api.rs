//! The routes of the API tests made from an OpenAPI document (Phase 8B): look at what would be tested, run it, read the report.
//!
//! A person gives the document (pasted, or by address), where the API is, and which kinds of case they want; `openapi_cases.rs` works
//! out the cases by rule, with no AI. `plan` only shows them. `runs` makes them one execution, a task for each case that may be run,
//! through the route every run uses (its policy, its evidence, its audit), and remembers what each task was for (`openapi_runs.rs`), so
//! that the report can say, operation by operation, what was expected and what came back.
//!
//! A request that changes data (POST, PUT, PATCH, DELETE) is shown but is run only when the person asks for unsafe methods. A
//! credential is only ever an `env:NAME` reference: its value is read from the orchestrator's environment when a request is made.

use crate::agent_tools::ToolError;
use crate::api::{
    audit, create_run_core, require_scoped_project, scoped_run_summary, scoped_run_tasks, to_500,
};
use crate::auth::{self, AuthContext, Role};
use crate::domain::{RunPlan, TaskStatus};
use crate::errors::{ApiError, ApiJson, ApiPath, ApiQuery};
use crate::openapi_cases::{self, Case, CaseKind, Derived, Options};
use crate::openapi_runs::{self, OpenApiRunRow, StoredCase, StoredOperation, TaskOutcome};
use crate::openapi_spec::{self, Spec};
use crate::state::AppState;
use axum::{
    extract::{Extension, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// The largest document read, pasted or fetched.
const MAX_SPEC_BYTES: usize = 5 * 1024 * 1024;
const FETCH_TIMEOUT_SECS: u64 = 20;
/// Runs in a project's list.
const LIST_LIMIT: i64 = 50;
const DEFAULT_CONCURRENCY: usize = 4;
const MAX_CONCURRENCY: usize = 8;

#[derive(Debug, Deserialize)]
pub(crate) struct OpenApiBody {
    /// The document itself, JSON or YAML; or `spec_url`, an address to fetch it from. One of the two.
    spec: Option<String>,
    spec_url: Option<String>,
    /// Where the API is. The document's first absolute server when not given.
    base_url: Option<String>,
    #[serde(default)]
    kinds: Vec<CaseKind>,
    #[serde(default)]
    operations: Vec<String>,
    #[serde(default)]
    include_unsafe: bool,
    #[serde(default)]
    auth_headers: BTreeMap<String, String>,
    max_cases_per_operation: Option<usize>,
    concurrency: Option<usize>,
    name: Option<String>,
}

/// What a tool's error says, without the tool's name in front of it.
fn tool_error_text(e: &ToolError) -> String {
    match e {
        ToolError::InvalidInput { details, .. } | ToolError::ExecutionFailed { details, .. } => {
            details.clone()
        }
        ToolError::SafetyViolation { reason, .. } => reason.clone(),
        ToolError::Timeout { timeout_ms, .. } => format!("timed out after {} s", timeout_ms / 1000),
        other => other.to_string(),
    }
}

/// The document as text: what was pasted, or what the address gives (an address that must be one the orchestrator may fetch).
async fn spec_text(body: &OpenApiBody) -> Result<String, ApiError> {
    match (
        body.spec.as_deref().filter(|s| !s.trim().is_empty()),
        body.spec_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    ) {
        (Some(_), Some(_)) => Err(ApiError::validation(
            "give the document either as `spec` or as `spec_url`, not both",
        )),
        (None, None) => Err(ApiError::validation(
            "give the OpenAPI document as `spec` (its text) or as `spec_url` (an address)",
        )),
        (Some(text), None) => {
            if text.len() > MAX_SPEC_BYTES {
                return Err(ApiError::validation(format!(
                    "the document is larger than {} MB",
                    MAX_SPEC_BYTES / 1024 / 1024
                )));
            }
            Ok(text.to_string())
        }
        (None, Some(url)) => crate::tools::security::safe_http_get(
            "openapi",
            url,
            std::time::Duration::from_secs(FETCH_TIMEOUT_SECS),
            MAX_SPEC_BYTES,
        )
        .await
        .map_err(|e| {
            ApiError::validation(format!(
                "could not fetch the document: {}",
                tool_error_text(&e)
            ))
        }),
    }
}

async fn read_spec(body: &OpenApiBody) -> Result<Spec, ApiError> {
    let text = spec_text(body).await?;
    let document = openapi_spec::read_document(&text).map_err(ApiError::validation)?;
    openapi_spec::parse(&document).map_err(ApiError::validation)
}

fn options_of(body: &OpenApiBody, spec: &Spec) -> Result<Options, ApiError> {
    let base_url = match body
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
    {
        Some(base) => base.to_string(),
        None => openapi_spec::base_address(&spec.servers, body.spec_url.as_deref()).ok_or_else(|| {
            ApiError::validation(
                "the document names no server address that can be used as it stands (an absolute one, with every {variable} given a default): give `base_url` (or give the document by its address, `spec_url`, so that a relative one can be worked out from it)",
            )
        })?,
    };
    let options = Options {
        base_url,
        kinds: body.kinds.clone(),
        operations: body.operations.clone(),
        include_unsafe: body.include_unsafe,
        auth_headers: body.auth_headers.clone(),
        max_cases_per_operation: body.max_cases_per_operation,
    };
    if let Some(problem) = openapi_cases::options_problem(&options) {
        return Err(ApiError::validation(problem));
    }
    let unknown: Vec<&String> = options
        .operations
        .iter()
        .filter(|id| !spec.operations.iter().any(|o| &o.id == *id))
        .collect();
    if !unknown.is_empty() {
        return Err(ApiError::validation(format!(
            "the document has no operation {}",
            unknown
                .iter()
                .take(3)
                .map(|u| format!("{u:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(options)
}

/// The variables the credential headers refer to that are not set in the orchestrator's environment.
fn missing_secret_names(options: &Options) -> Vec<String> {
    let names: Vec<String> = options
        .auth_headers
        .values()
        .filter_map(|v| crate::execution_plan::secret_name(v))
        .map(str::to_string)
        .collect();
    crate::execution_plan::missing_secrets(&names)
}

fn operation_view(spec: &Spec) -> Vec<Value> {
    spec.operations
        .iter()
        .map(|o| json!({"id": o.id, "method": o.method, "path": o.path, "summary": o.summary, "tags": o.tags, "deprecated": o.deprecated, "requires_auth": o.requires_auth}))
        .collect()
}

/// POST /api/v1/projects/:project_id/openapi/plan — what would be tested, and nothing is done: the operations of the document and
/// the cases that would be made for them, with the request and the answer each expects (Operator role: an address is fetched).
pub(crate) async fn plan(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<OpenApiBody>,
) -> Result<Json<Value>, ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let spec = read_spec(&body).await?;
    let options = options_of(&body, &spec)?;
    let derived = openapi_cases::derive(&spec, &options);
    let runnable = derived.cases.iter().filter(|c| c.skipped.is_none()).count();
    Ok(Json(json!({
        "spec": {"title": spec.title, "version": spec.version, "servers": spec.servers},
        "base_url": options.base_url,
        "operations": operation_view(&spec),
        "cases": derived.cases,
        "skipped": derived.skipped,
        "notes": derived.notes,
        "counts": {"cases": derived.cases.len(), "runnable": runnable, "not_run": derived.cases.len() - runnable},
        "missing_secrets": missing_secret_names(&options),
    })))
}

/// What became of each operation once the cases were derived, for the report's coverage.
fn stored_operations(spec: &Spec, options: &Options, derived: &Derived) -> Vec<StoredOperation> {
    let picked: BTreeSet<&str> = options.operations.iter().map(String::as_str).collect();
    spec.operations
        .iter()
        .map(|o| StoredOperation {
            id: o.id.clone(),
            summary: o.summary.clone(),
            tags: o.tags.clone(),
            selected: picked.is_empty() || picked.contains(o.id.as_str()),
            skipped: derived
                .skipped
                .iter()
                .find(|s| s.operation == o.id)
                .map(|s| s.reason.clone()),
        })
        .collect()
}

/// POST /api/v1/projects/:project_id/openapi/runs — makes the cases and runs them as one execution (Operator role). Answers at once
/// with the run's id; the report (`GET .../openapi/runs/:id`) fills in as the tasks finish.
pub(crate) async fn create_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<OpenApiBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    auth::require_role(&ctx, Role::Operator)?;
    require_scoped_project(&state, &ctx, project_id).await?;
    let spec = read_spec(&body).await?;
    let options = options_of(&body, &spec)?;
    let missing = missing_secret_names(&options);
    if !missing.is_empty() {
        return Err(ApiError::validation(
            crate::execution_plan::missing_secrets_message(&missing),
        ));
    }
    let concurrency = body.concurrency.unwrap_or(DEFAULT_CONCURRENCY);
    if !(1..=MAX_CONCURRENCY).contains(&concurrency) {
        return Err(ApiError::validation(format!(
            "concurrency must be from 1 to {MAX_CONCURRENCY}"
        )));
    }
    let derived = openapi_cases::derive(&spec, &options);
    let runnable: Vec<&Case> = derived
        .cases
        .iter()
        .filter(|c| c.skipped.is_none())
        .collect();
    if runnable.is_empty() {
        let why = if derived.cases.is_empty() {
            "no case can be made for the operations asked for".to_string()
        } else {
            format!(
                "all {} case(s) change data on the server: allow unsafe methods to run them",
                derived.cases.len()
            )
        };
        return Err(ApiError::validation(format!(
            "there is nothing to run: {why}"
        )));
    }

    // Each case is a task of one run. The plan of each goes through the same structural and target checks as any other plan.
    let mut task_ids: BTreeMap<String, String> = BTreeMap::new();
    let mut tasks = Vec::new();
    let mut refused: BTreeSet<String> = BTreeSet::new();
    for (n, case) in runnable.iter().enumerate() {
        let task_id = format!("api-{:04}", n + 1);
        let plan = openapi_cases::to_plan(case, &task_id);
        if let Err(reasons) = crate::execution_plan::validate_plan(&plan).await {
            refused.extend(reasons);
            continue;
        }
        let task =
            crate::execution_plan::materialize_task_def(task_id.clone(), case.title.clone(), &plan)
                .map_err(ApiError::validation)?;
        task_ids.insert(case.id.clone(), task_id);
        tasks.push(task);
    }
    if !refused.is_empty() {
        return Err(ApiError::validation(format!(
            "TARGET_NOT_ALLOWED: {}",
            refused.into_iter().take(3).collect::<Vec<_>>().join("; ")
        )));
    }
    let run_name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("OpenAPI tests: {} {}", spec.title, spec.version));
    let run_plan = RunPlan {
        name: run_name,
        tasks,
        concurrency,
    };
    let (_, Json(created)) =
        create_run_core(&state, &ctx, Some(project_id.to_string()), run_plan).await?;

    let stored_cases: Vec<StoredCase> = derived
        .cases
        .iter()
        .map(|case| StoredCase {
            case: case.clone(),
            task_id: task_ids.get(&case.id).cloned(),
        })
        .collect();
    let row = OpenApiRunRow {
        id: Uuid::new_v4(),
        project_id: project_id.to_string(),
        run_id: created.run_id,
        title: spec.title.clone(),
        version: spec.version.clone(),
        base_url: options.base_url.clone(),
        options: json!({
            "kinds": options.kinds,
            "operations": options.operations,
            "include_unsafe": options.include_unsafe,
            "auth_headers": options.auth_headers.keys().collect::<Vec<_>>(),
            "max_cases_per_operation": options.max_cases_per_operation,
            "concurrency": concurrency,
        }),
        operations: stored_operations(&spec, &options, &derived),
        cases: stored_cases,
        notes: derived.notes.clone(),
        created_by: ctx.name.clone(),
        created_at: chrono::Utc::now(),
    };
    state.store.insert_openapi_run(&row).await.map_err(to_500)?;
    audit(
        &state,
        &ctx,
        "openapi.run",
        Some(&row.id.to_string()),
        Some(&format!(
            "{} case(s) against {}{}",
            runnable.len(),
            options.base_url,
            if options.include_unsafe {
                ", unsafe methods allowed"
            } else {
                ""
            }
        )),
        "ok",
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row.id,
            "run_id": row.run_id,
            "cases": row.cases.len(),
            "cases_run": runnable.len(),
            "not_run": row.cases.len() - runnable.len(),
            "state": created.state,
            "notes": row.notes,
        })),
    ))
}

async fn load_run(state: &AppState, project_id: Uuid, id: Uuid) -> Result<OpenApiRunRow, ApiError> {
    state
        .store
        .get_openapi_run(id)
        .await
        .map_err(to_500)?
        .filter(|row| row.project_id == project_id.to_string())
        .ok_or_else(|| ApiError::not_found("OpenAPI test run not found in this project"))
}

/// GET /api/v1/projects/:project_id/openapi/runs — the project's runs, newest first (Viewer role).
pub(crate) async fn list_runs(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath(project_id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let rows = state
        .store
        .list_openapi_runs(&project_id.to_string(), LIST_LIMIT)
        .await
        .map_err(to_500)?;
    let mut runs = Vec::new();
    for row in rows {
        let run_state = scoped_run_summary(&state, row.run_id, project_id)
            .await
            .ok()
            .map(|s| s.state);
        let mut value = serde_json::to_value(&row).map_err(to_500)?;
        value["run_state"] = json!(run_state);
        runs.push(value);
    }
    Ok(Json(json!({ "runs": runs })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReportQuery {
    format: Option<String>,
}

/// GET /api/v1/projects/:project_id/openapi/runs/:id — the report (Viewer role; `?format=markdown` gives text to paste): for each
/// operation of the document, the cases made for it with what each expected and what came back, and how much of the document was tested.
/// While the run is going it says what it has so far, and that it has not ended.
pub(crate) async fn get_run(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    ApiPath((project_id, id)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(query): ApiQuery<ReportQuery>,
) -> Result<Response, ApiError> {
    require_scoped_project(&state, &ctx, project_id).await?;
    let format = query.format.as_deref().unwrap_or("json");
    if !matches!(format, "json" | "markdown") {
        return Err(ApiError::validation("format must be json or markdown"));
    }
    let row = load_run(&state, project_id, id).await?;
    let summary = scoped_run_summary(&state, row.run_id, project_id).await?;
    let mut outcomes: BTreeMap<String, TaskOutcome> = BTreeMap::new();
    let mut detailed = 0usize;
    for task in scoped_run_tasks(&state, row.run_id).await? {
        let mut outcome = TaskOutcome {
            status: task.status,
            reason: task.reason.clone(),
            result: None,
        };
        // What a case that did not pass got back is in its evidence: read it for the first few failures, not for every case.
        if matches!(
            task.status,
            TaskStatus::Failed | TaskStatus::Timeout | TaskStatus::InfraError
        ) && detailed < openapi_runs::MAX_DETAILED
        {
            detailed += 1;
            if let Some((_, log)) =
                crate::api_testing::read_task_evidence(&state, row.run_id, &task.id).await
            {
                outcome.result = crate::api_testing::parse_result_summary(&log);
            }
        }
        outcomes.insert(task.id.clone(), outcome);
    }
    let state_name = serde_json::to_value(summary.state)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let report =
        openapi_runs::build_report(&row, &state_name, summary.state.is_terminal(), &outcomes);
    if format == "markdown" {
        return Ok((
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            openapi_runs::to_markdown(&report),
        )
            .into_response());
    }
    Ok(Json(report).into_response())
}
