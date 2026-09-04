//! BE-9 regression tests — the structured error envelope (API Contract
//! §2.5). Unlike every other test file in this crate, these drive real
//! HTTP requests through the real `axum::Router` in-process
//! (`tower::ServiceExt::oneshot`) rather than calling handler/store
//! functions directly, because BE-9's own requirement is specifically about
//! the *HTTP response body* — status code alone proves nothing about
//! whether the JSON envelope shape is actually correct.

use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
use crate::api::router;
use crate::domain::RunState;
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::policy::PolicyConfig;
use crate::state::AppState;
use crate::store::Store;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Row;
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

/// Builds an `AiInfrastructure` with exactly one fake provider/model whose
/// executor returns `response_or_none` (`Ok`) or the given error (`Err`) —
/// no real network call, ever, in any test using this. `response = "{}"`
/// (the default via `inert_ai_router`) deliberately fails structured-output
/// parsing if the AI path is ever accidentally exercised, so an unrelated
/// test gets a real, honest `Failed` outcome rather than a panic.
fn ai_infra_with(result: Result<&str, crate::ai_router::ExecutionError>) -> Arc<AiInfrastructure> {
    let executor: Arc<dyn AiExecutor> = match result {
        Ok(text) => Arc::new(FakeExecutor::ok(text)),
        Err(e) => Arc::new(FakeExecutor::err(e)),
    };
    let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> =
        std::collections::HashMap::new();
    executors.insert("mock", executor);
    Arc::new(AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider(
            "mock",
            vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
        )))],
        executors,
    ))
}

fn inert_ai_router() -> Arc<AiInfrastructure> {
    ai_infra_with(Ok("{}"))
}

/// Phase 3 — two independently-scripted real providers, deterministically
/// routed by the real capability-scoring mechanism
/// (`ai_router.rs::score_model`): a model with `long_context: None` is
/// disqualified from `AiTask::RequirementAnalysis` (which requires
/// `LongContext`) but fully eligible for `AiTask::TestCaseGeneration`
/// (which never requires it) — the same lever `custom_testing.rs`'s own
/// unit tests use, applied here at the HTTP layer so a genuine end-to-end
/// `REAL_AI` response (real analysis AND real cases, not the same fixed
/// text satisfying both) can be observed on the wire.
fn custom_testing_ai_infra_two_calls(
    analysis_response: &str,
    cases_response: &str,
) -> Arc<AiInfrastructure> {
    let analysis_model = healthy_model("analysis-model", "analysis-only", CapabilityLevel::Basic);
    let mut cases_model = healthy_model("cases-model", "cases-only", CapabilityLevel::Strong);
    cases_model.capabilities.long_context = Some(CapabilityLevel::None);

    let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> =
        std::collections::HashMap::new();
    executors.insert(
        "analysis-only",
        Arc::new(FakeExecutor::ok(analysis_response)),
    );
    executors.insert("cases-only", Arc::new(FakeExecutor::ok(cases_response)));

    Arc::new(AiInfrastructure::new(
        vec![
            Arc::new(FakeDiscovery(healthy_provider(
                "analysis-only",
                vec![analysis_model],
            ))),
            Arc::new(FakeDiscovery(healthy_provider(
                "cases-only",
                vec![cases_model],
            ))),
        ],
        executors,
    ))
}

async fn test_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register("maestro", Arc::new(SimulatedExecutor));
    let policy = PolicyConfig {
        max_concurrency: 8,
        // Matches the production default (`PolicyConfig::from_env`'s
        // documented default is "maestro") so a plan naming that executor
        // genuinely lands in PENDING_APPROVAL, exactly like the real
        // deployed policy gate — needed by the Execution-category test.
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    };
    (
        AppState::new(
            Arc::new(registry),
            store,
            policy,
            evidence,
            inert_ai_router(),
        ),
        dir,
    )
}

/// Phase 4 — same setup as `test_state()`, but with the real, unmodified
/// `CompiledApiExecutor` registered under `"compiled-api"`, exactly like
/// `main.rs`'s own real production registration — so an API Testing page
/// integration test genuinely dispatches to the same executor a real
/// deployment uses, not a double.
async fn test_state_with_compiled_api() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register("maestro", Arc::new(SimulatedExecutor))
        .register(
            "compiled-api",
            Arc::new(crate::executor::CompiledApiExecutor::from_env()),
        );
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    };
    (
        AppState::new(
            Arc::new(registry),
            store,
            policy,
            evidence,
            inert_ai_router(),
        ),
        dir,
    )
}

/// Phase 2 — same setup as `test_state()`, but with a caller-supplied AI
/// infrastructure (typically via `ai_infra_with(...)`), so an AI-generation
/// test can inject a controllable provider/executor and assert on the
/// real HTTP response the endpoint produces for that outcome — without
/// ever making a real network call.
async fn test_state_with_ai_router(
    ai_router: Arc<AiInfrastructure>,
) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register("maestro", Arc::new(SimulatedExecutor));
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    };
    (
        AppState::new(Arc::new(registry), store, policy, evidence, ai_router),
        dir,
    )
}

/// Mints a real, durably persisted API key and returns its plaintext.
async fn mint_key(state: &AppState, role: &str, project_id: Option<&str>) -> String {
    let raw = crate::auth::generate_key();
    state
        .store
        .insert_api_key(
            Uuid::new_v4(),
            "test-key",
            &crate::auth::hash_key(&raw),
            role,
            project_id,
        )
        .await
        .expect("insert_api_key");
    raw
}

async fn send(state: &AppState, req: Request<Body>) -> (StatusCode, Value) {
    let app = router(state.clone());
    let resp = app.oneshot(req).await.expect("router call");
    let status = resp.status();
    let content_type = resp.headers().get("content-type").cloned();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "response body was not valid JSON (content-type={content_type:?}): {e}\nbody={}",
                String::from_utf8_lossy(&bytes)
            )
        })
    };
    (status, body)
}

fn req(method: &str, path: &str, key: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(k) = key {
        builder = builder.header("Authorization", format!("Bearer {k}"));
    }
    let body = match body {
        Some(v) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

/// Like `req`, but sends `raw_body` verbatim (not JSON-serialized) — needed
/// to construct genuinely malformed bodies BE-10's tests inject.
fn req_raw(
    method: &str,
    path: &str,
    key: Option<&str>,
    content_type: Option<&str>,
    raw_body: &str,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(k) = key {
        builder = builder.header("Authorization", format!("Bearer {k}"));
    }
    if let Some(ct) = content_type {
        builder = builder.header("Content-Type", ct);
    }
    builder.body(Body::from(raw_body.to_string())).unwrap()
}

/// Asserts the response is a Contract §2.5 envelope with the expected
/// status/code, and that `message`/`details`/`request_id` are present with
/// the right shape — not just that *a* JSON object came back.
fn assert_envelope(
    status: StatusCode,
    body: &Value,
    expected_status: StatusCode,
    expected_code: &str,
) {
    assert_eq!(status, expected_status, "HTTP status; body={body}");
    let error = &body["error"];
    assert!(
        error.is_object(),
        "response must have a top-level \"error\" object; body={body}"
    );
    assert_eq!(error["code"], expected_code, "error.code; body={body}");
    assert!(
        error["message"].is_string(),
        "error.message must be a string; body={body}"
    );
    assert!(
        !error["message"].as_str().unwrap().is_empty(),
        "error.message must not be empty"
    );
    assert!(
        error["details"].is_object(),
        "error.details must be an object (contract's own example shows {{}})"
    );
    let request_id = error["request_id"]
        .as_str()
        .expect("error.request_id must be a string");
    assert!(
        request_id.starts_with("req_"),
        "request_id should look like the contract's own example (\"req_123\"): {request_id}"
    );
}

// ---------------------------------------------------------- Authentication

/// Category: Authentication — no credentials at all.
#[tokio::test]
async fn be9_authentication_missing_credentials_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let (status, body) = send(&state, req("GET", "/api/v1/projects", None, None)).await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

/// Category: Authentication — a syntactically present but unknown key.
#[tokio::test]
async fn be9_authentication_invalid_credentials_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let (status, body) = send(
        &state,
        req("GET", "/api/v1/projects", Some("aiqa_not_a_real_key"), None),
    )
    .await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

// ----------------------------------------------------------- Authorization

/// Category: Authorization — a real, valid identity with an insufficient role.
#[tokio::test]
async fn be9_authorization_insufficient_role_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let viewer = mint_key(&state, "viewer", None).await;
    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&viewer),
            Some(serde_json::json!({"name": "should be forbidden"})),
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::FORBIDDEN, "FORBIDDEN");
}

/// Category: Authorization — a real, valid identity, correct role, but
/// scoped to a *different* project than the one it's trying to reach.
#[tokio::test]
async fn be9_authorization_cross_project_access_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (_, created) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(serde_json::json!({"name": "Project A"})),
        ),
    )
    .await;
    let project_a_id = created["id"].as_str().expect("created project id");

    let scoped_to_other = mint_key(&state, "operator", Some("some-other-project")).await;
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a_id}"),
            Some(&scoped_to_other),
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::FORBIDDEN, "FORBIDDEN");
}

// -------------------------------------------------------------- Validation

/// Category: Validation — a syntactically valid request the server rejects
/// on content (blank project name).
#[tokio::test]
async fn be9_validation_blank_name_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(serde_json::json!({"name": "   "})),
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
    assert!(
        body["error"]["message"].as_str().unwrap().contains("empty"),
        "message should explain what was wrong, not just that something was: {body}"
    );
}

// -------------------------------------------------------------- Not Found

/// Category: Not Found — a syntactically valid, nonexistent resource id.
#[tokio::test]
async fn be9_not_found_returns_structured_envelope_with_contract_example_code() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let missing_id = Uuid::new_v4();
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{missing_id}"),
            Some(&admin),
            None,
        ),
    )
    .await;
    // "RESOURCE_NOT_FOUND" is not a code this implementation invented — it
    // is the exact code the contract's own §2.5 example uses.
    assert_envelope(status, &body, StatusCode::NOT_FOUND, "RESOURCE_NOT_FOUND");
}

// ----------------------------------------------------- Persistence/Internal

/// Category: Persistence (surfaces as Internal — see `errors.rs` doc
/// comment on `ApiError::internal` for why the contract gives no basis to
/// split these into separate codes). A REAL SQLite write failure — a
/// trigger that aborts every INSERT into `projects` — not a mocked error.
/// The critical assertion is not just the envelope shape but that the raw
/// database error text (which names the trigger, the table, "RAISE",
/// "ABORT") never reaches the response body, per Contract §2.5's explicit
/// prohibition on leaking "internal stack traces ... sensitive
/// infrastructure details".
#[tokio::test]
async fn be9_persistence_failure_returns_sanitized_internal_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;

    sqlx::query(
        "CREATE TRIGGER be9_fail_project_insert BEFORE INSERT ON projects \
         BEGIN SELECT RAISE(ABORT, 'injected disk write failure for BE-9 test'); END;",
    )
    .execute(&state.store.pool)
    .await
    .expect("create trigger");

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(serde_json::json!({"name": "will fail to persist"})),
        ),
    )
    .await;

    assert_envelope(
        status,
        &body,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    let message = body["error"]["message"].as_str().unwrap();
    assert_eq!(
        message, "An internal error occurred.",
        "the real SQL error must not reach the client verbatim"
    );
    for leaked_detail in [
        "RAISE",
        "ABORT",
        "be9_fail_project_insert",
        "injected disk write failure",
        "SQLite",
        "sqlite",
    ] {
        assert!(
            !message.contains(leaked_detail) && !body.to_string().contains(leaked_detail),
            "response must not leak internal detail {leaked_detail:?}; full body={body}"
        );
    }
}

/// Category: Execution — a persistence failure surfaced through a
/// *different* handler (`approve_run_scoped`, which calls
/// `engine::approve_and_spawn`) than the Persistence test above, proving
/// the same centralized `ApiError` path covers more than one call site
/// uniformly rather than each handler re-implementing its own sanitization.
/// (Phase B cleanup: originally exercised the legacy flat routes; now
/// exercised through the scoped executions surface.)
#[tokio::test]
async fn be9_execution_persistence_failure_returns_sanitized_internal_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let project = make_project(&state, "BE-9 Sanitize Exec").await;

    // A run stuck in PENDING_APPROVAL, exactly like the policy-gate path
    // already tested end-to-end elsewhere in this codebase.
    let plan = serde_json::json!({
        "name": "be9-execution-test",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 0}}],
    });
    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&admin),
            Some(plan),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "run creation should succeed; body={created}"
    );
    assert_eq!(
        created["status"],
        serde_json::to_value(RunState::PendingApproval).unwrap()
    );
    let run_id = created["run_id"].as_str().expect("run_id");

    sqlx::query(
        "CREATE TRIGGER be9_fail_approve BEFORE UPDATE ON runs WHEN NEW.state = 'SCHEDULED' \
         BEGIN SELECT RAISE(ABORT, 'injected disk write failure approving BE-9 test run'); END;",
    )
    .execute(&state.store.pool)
    .await
    .expect("create trigger");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/approve"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_envelope(
        status,
        &body,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    assert_eq!(body["error"]["message"], "An internal error occurred.");
    assert!(
        !body.to_string().contains("RAISE"),
        "approve_run's failure must also be sanitized, not just create_project's"
    );
}

// -------------------------------------------------------- success path guard

/// BE-9's own change must not turn a *successful* response into an
/// envelope — Step 5's explicit "do not change success responses".
#[tokio::test]
async fn be9_successful_response_is_not_wrapped_in_an_error_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(serde_json::json!({"name": "Real Project"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(
        body.get("error").is_none(),
        "a successful response must not contain an \"error\" key: {body}"
    );
    assert_eq!(
        body["name"], "Real Project",
        "successful response schema must be unchanged by BE-9"
    );
}

// -------------------------------------------------------- BE-8 regression

/// BE-8 regression (Step 7): the full CRUD lifecycle through the *real*
/// router, now that every handler's error type changed underneath it —
/// list/get/update success paths specifically, which the category tests
/// above don't otherwise exercise (they focus on create + failure paths).
#[tokio::test]
async fn be9_be8_regression_full_project_crud_lifecycle_through_router() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;

    let (status, created) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(serde_json::json!({"name": "Lifecycle Project", "description": "d1"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().expect("id").to_string();

    // A key genuinely scoped to this project (not just an admin) for the
    // list/get/update legs, so this also re-confirms isolation still holds.
    let scoped = mint_key(&state, "operator", Some(&id)).await;

    let (status, list_body) =
        send(&state, req("GET", "/api/v1/projects", Some(&scoped), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        list_body.as_array().unwrap().len(),
        1,
        "a scoped key must list only its own project"
    );
    assert_eq!(list_body[0]["id"], id);

    let (status, got) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{id}"),
            Some(&scoped),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "Lifecycle Project");
    assert_eq!(got["description"], "d1");

    let (status, updated) = send(
        &state,
        req(
            "PATCH",
            &format!("/api/v1/projects/{id}"),
            Some(&scoped),
            Some(serde_json::json!({"description": "d2"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        updated["name"], "Lifecycle Project",
        "an omitted field must survive the update, unchanged by BE-9"
    );
    assert_eq!(updated["description"], "d2");
}

// ============================================================ BE-10

/// BE-10: the one gap BE-9's own report disclosed — a malformed JSON body
/// is rejected by Axum's `Json<T>` extractor *before* any handler body
/// runs, so BE-9's handler-level `ApiError` conversion never saw it. Status
/// is Axum's own (400, unchanged — Step 5) but the body must now be the
/// envelope, not Axum's default plain text.
#[tokio::test]
async fn be10_malformed_json_body_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send(
        &state,
        req_raw(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some("application/json"),
            "{not valid json",
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
}

/// BE-10: an invalid path parameter (not a UUID) is rejected by Axum's
/// `Path<Uuid>` extractor before the handler runs.
#[tokio::test]
async fn be10_invalid_path_parameter_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send(
        &state,
        req("GET", "/api/v1/projects/not-a-uuid", Some(&admin), None),
    )
    .await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
}

/// BE-10: a JSON body sent with no (or the wrong) `Content-Type` is
/// rejected by Axum's `Json<T>` extractor with 415, before the handler runs.
#[tokio::test]
async fn be10_missing_content_type_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send(
        &state,
        req_raw(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            None,
            r#"{"name":"x"}"#,
        ),
    )
    .await;
    assert_envelope(
        status,
        &body,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "VALIDATION_ERROR",
    );
}

/// BE-10: a query parameter that doesn't deserialize to its declared type
/// (`limit` must be an integer) is rejected by Axum's `Query<T>` extractor.
/// Phase B cleanup: originally exercised the legacy flat `GET /runs`;
/// now exercised against the surviving Admin-only audit-log route (the
/// extractor rejection happens before any role check either way).
#[tokio::test]
async fn be10_invalid_query_parameter_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send(
        &state,
        req(
            "GET",
            "/admin/audit-log?limit=notanumber",
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
}

/// BE-10 regression guard: a *valid* request through each converted
/// extractor (`ApiJson`, `ApiPath`, `ApiQuery`) must behave exactly as
/// before — this is what proves the wrapper types are truly transparent
/// pass-throughs, not just error-path shims.
#[tokio::test]
async fn be10_valid_requests_through_converted_extractors_are_unaffected() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;

    // ApiJson: a well-formed create body.
    let (status, created) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(serde_json::json!({"name": "Extractor Check"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().expect("id");

    // ApiPath: a well-formed UUID path segment.
    let (status, got) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{id}"), Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "Extractor Check");

    // ApiQuery: a well-formed query string (against the surviving
    // Admin-only audit-log route — Phase B cleanup removed the legacy
    // flat `/runs` this case used to exercise).
    let (status, list) = send(
        &state,
        req("GET", "/admin/audit-log?limit=5", Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.is_array());
}

// ==================================== Actual BE-9 — project-scoped routing

/// Creates a real project directly against the store (bypassing the router,
/// since these tests need a project id in hand before minting the key
/// that's scoped to it) and returns its id.
async fn make_project(state: &AppState, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    state
        .store
        .insert_project(id, name, None)
        .await
        .expect("insert_project");
    id
}

/// A minimal, always-Allow (never PENDING_APPROVAL) plan, so `create_run`
/// tests don't need to also thread the policy gate — the gate itself is
/// already covered by `be9_execution_persistence_failure_returns_sanitized_internal_envelope`.
fn simple_plan(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "pass", "delay_ms": 0}}],
    })
}

/// The full positive path through every new scoped route: create, list,
/// get, events, evidence, approve/reject are exercised by dedicated tests
/// below (approve/reject need a gated plan) — this one covers the
/// straightforward, ungated lifecycle.
#[tokio::test]
async fn actual_be9_scoped_execution_lifecycle_through_router() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let project = make_project(&state, "Lifecycle").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(simple_plan("scoped-run")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let run_id = created["run_id"].as_str().expect("run_id").to_string();

    // Give the (SimulatedExecutor-backed) run a moment to finish.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let (status, list) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["run_id"], run_id);

    let (status, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["name"], "scoped-run");
    assert!(detail["tasks"].as_array().unwrap().len() == 1);

    let (status, events) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}/events"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !events.as_array().unwrap().is_empty(),
        "a completed run must have real events"
    );

    let (status, evidence) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}/evidence"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(evidence.is_array());

    // Regression: a global admin key can also reach it, listing/filtering
    // correctly to *this* project only.
    let (status, admin_list) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin_list.as_array().unwrap().len(), 1);
}

/// Approve/reject specifically, using a plan whose executor the default
/// test policy gates (mirrors BE-9(structured-errors)'s own
/// `require_approval_for: ["maestro"]` test setup).
#[tokio::test]
async fn actual_be9_scoped_approve_and_reject_through_router() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Governance").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let gated_plan = serde_json::json!({
        "name": "gated-run",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 100}}],
    });
    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(gated_plan),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    // BE-13 (§12.1 response alignment): the scoped create endpoint's field
    // is `status`, not the legacy `state` (see `CreateExecutionResponse`).
    assert_eq!(created["status"], "PENDING_APPROVAL");
    let run_id = created["run_id"].as_str().unwrap().to_string();

    let (status, approved) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={approved}");
    assert_eq!(approved["state"], "RUNNING");

    // A second gated run, to exercise reject.
    let gated_plan2 = serde_json::json!({
        "name": "gated-run-2",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 0}}],
    });
    let (_, created2) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(gated_plan2),
        ),
    )
    .await;
    let run_id2 = created2["run_id"].as_str().unwrap().to_string();
    let (status, rejected) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id2}/reject"),
            Some(&owner),
            Some(serde_json::json!({"reason": "not needed"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={rejected}");
    assert_eq!(rejected["state"], "REJECTED");
}

/// Step 4 — cross-project isolation. Project A's key must not be able to
/// read, mutate, or otherwise reach Project B's run through *any* of the
/// new scoped routes, whether requested under A's own path (403 — not
/// authorized for A's own path to serve someone else's resource... no,
/// actually requested under B's path with A's key — 403, wrong namespace)
/// or under A's path naming B's run id (404 — A's namespace genuinely does
/// not contain that run).
#[tokio::test]
async fn actual_be9_cross_project_isolation_is_enforced_on_every_scoped_route() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Project A").await;
    let project_b = make_project(&state, "Project B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    // A real run genuinely owned by Project B.
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions"),
            Some(&owner_b),
            Some(simple_plan("b-run")),
        ),
    )
    .await;
    let b_run = created["run_id"].as_str().unwrap().to_string();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // A cannot even name Project B's path at all.
    for (method, path, body) in [
        (
            "GET",
            format!("/api/v1/projects/{project_b}/executions/{b_run}"),
            None,
        ),
        (
            "GET",
            format!("/api/v1/projects/{project_b}/executions/{b_run}/events"),
            None,
        ),
        (
            "GET",
            format!("/api/v1/projects/{project_b}/executions/{b_run}/evidence"),
            None,
        ),
        (
            "GET",
            format!("/api/v1/projects/{project_b}/executions"),
            None,
        ),
    ] {
        let (status, body_json) = send(&state, req(method, &path, Some(&owner_a), body)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{method} {path} must 403 for a key scoped to a different project; body={body_json}"
        );
        assert_eq!(body_json["error"]["code"], "FORBIDDEN");
    }

    // A cannot reach B's run by guessing/using its id under A's OWN path
    // either — the resource genuinely doesn't belong to A's namespace.
    for (method, path) in [
        (
            "GET",
            format!("/api/v1/projects/{project_a}/executions/{b_run}"),
        ),
        (
            "GET",
            format!("/api/v1/projects/{project_a}/executions/{b_run}/events"),
        ),
        (
            "GET",
            format!("/api/v1/projects/{project_a}/executions/{b_run}/evidence"),
        ),
    ] {
        let (status, body_json) = send(&state, req(method, &path, Some(&owner_a), None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path} must 404 (not 403) — A's namespace genuinely lacks this run; body={body_json}");
        assert_eq!(body_json["error"]["code"], "RESOURCE_NOT_FOUND");
    }

    // Mutation: A cannot approve/reject B's run either way.
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions/{b_run}/approve"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "A must not be able to mutate B's run under A's own path"
    );

    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions/{b_run}/approve"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "A must not be able to mutate B's run under B's path either"
    );
}

/// The same isolation, but for evidence specifically addressed by its own
/// id (`GET /api/v1/projects/:project_id/evidence/:evidence_id` for
/// metadata, `.../content` for the raw bytes — BE-15 split this path into
/// the two per API Contract §15.2; see `get_evidence_detail_scoped`'s doc
/// comment), which resolves ownership through the evidence's owning run
/// rather than a direct `project_id` column — the exact path BE-8/legacy
/// `get_evidence_content` already used, now re-verified under the new
/// scoped route.
#[tokio::test]
async fn actual_be9_cross_project_isolation_on_evidence_by_id() {
    let (state, dir) = test_state().await;
    let project_a = make_project(&state, "Evidence A").await;
    let project_b = make_project(&state, "Evidence B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    // A run in project B with real evidence attached (SimulatedExecutor
    // never produces evidence, so insert it directly against the store —
    // exactly mirroring how the legacy evidence tests in this codebase seed
    // real evidence without needing a real failing executor).
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions"),
            Some(&owner_b),
            Some(simple_plan("b-evidence-run")),
        ),
    )
    .await;
    let b_run: Uuid = created["run_id"].as_str().unwrap().parse().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let evidence_store =
        crate::evidence::LocalEvidenceStore::new(dir.path().join("evidence")).unwrap();
    let (object_key, size) = evidence_store
        .put(&b_run.to_string(), "t1", "log", "real captured output")
        .unwrap();
    let evidence_id = state
        .store
        .insert_evidence(b_run, "t1", "log", &object_key, "text/plain", size)
        .await
        .expect("insert_evidence");

    // B can read its own evidence detail (JSON metadata — BE-15) via the
    // scoped path.
    let (status, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/evidence/{evidence_id}"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={detail}");
    assert_eq!(detail["id"], evidence_id.to_string());
    assert_eq!(detail["kind"], "log");

    // ...and the actual bytes via the sibling `/content` path. Success here
    // is raw evidence bytes, not JSON (matching the legacy
    // `get_evidence_content` exactly), so this one assertion bypasses
    // `send`'s JSON parsing.
    let app = router(state.clone());
    let resp = app
        .oneshot(req(
            "GET",
            &format!("/api/v1/projects/{project_b}/evidence/{evidence_id}/content"),
            Some(&owner_b),
            None,
        ))
        .await
        .expect("router call");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    assert_eq!(&bytes[..], b"real captured output");

    // A cannot, under either project's path, on either the detail or the
    // content route.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/evidence/{evidence_id}"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/evidence/{evidence_id}"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/evidence/{evidence_id}/content"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

/// Step 5 — tenant isolation must still hold exactly as Phase 1 (B3/B14)
/// left it: this is a *routing* unit, the tenant-isolation mechanism itself
/// (`require_project_access`) is reused unmodified, not reimplemented. This
/// test exists to prove that reuse is real, not just claimed — two
/// distinct "tenants" (projects), cross-access rejected through the new
/// routes, same as §isolation tests above but framed as the explicit
/// tenant-A/tenant-B language Step 5 asks for.
#[tokio::test]
async fn actual_be9_tenant_isolation_preserved_through_scoped_routes() {
    let (state, _dir) = test_state().await;
    let tenant_a_project = make_project(&state, "Tenant A").await;
    let tenant_b_project = make_project(&state, "Tenant B").await;
    let tenant_a_key = mint_key(&state, "viewer", Some(&tenant_a_project.to_string())).await;
    let tenant_b_key = mint_key(&state, "operator", Some(&tenant_b_project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{tenant_b_project}/executions"),
            Some(&tenant_b_key),
            Some(simple_plan("tenant-b-run")),
        ),
    )
    .await;
    let tenant_b_run = created["run_id"].as_str().unwrap().to_string();

    // Tenant A (even just a viewer) must not see Tenant B's run under B's own path.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{tenant_b_project}/executions/{tenant_b_run}"),
            Some(&tenant_a_key),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    // And Tenant B's own key genuinely can.
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{tenant_b_project}/executions/{tenant_b_run}"),
            Some(&tenant_b_key),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// A URL naming a project id that was never created must 404, not silently
/// treat it as an empty-but-valid namespace — Step 3's explicit "project
/// exists" requirement.
#[tokio::test]
async fn actual_be9_nonexistent_project_returns_structured_not_found() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let phantom_project = Uuid::new_v4();

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{phantom_project}/executions"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{phantom_project}/executions"),
            Some(&admin),
            Some(simple_plan("x")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

// ============================================ BE-12 — Dashboard / KPI JSON

/// A plan with one passing and one failing task, so pass/fail and
/// reliability arithmetic has real, non-trivial numbers to check.
fn mixed_outcome_plan(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 2,
        "tasks": [
            {"id": "t1", "name": "t1", "simulate": {"outcome": "pass", "delay_ms": 0}},
            {"id": "t2", "name": "t2", "simulate": {"outcome": "fail", "delay_ms": 0}},
        ],
    })
}

/// Category: valid response + real aggregation + status/count accuracy.
/// One project, one run with a real pass and a real fail, checked against
/// both `/dashboard` and `/kpis`.
#[tokio::test]
async fn be12_dashboard_and_kpis_reflect_real_run_and_task_data() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Dashboard Real Data").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(mixed_outcome_plan("mixed-run")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (status, dash) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/dashboard"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={dash}");
    assert_eq!(dash["project_id"], project.to_string());
    assert_eq!(dash["project_name"], "Dashboard Real Data");
    // One run, terminal (COMPLETED or PARTIAL depending on scheduler
    // semantics for a mixed-outcome plan) — assert the count sums to 1
    // rather than pinning an exact state, since that state machine is
    // owned by engine.rs, not this unit.
    let exec_counts = dash["execution_counts"]
        .as_object()
        .expect("execution_counts object");
    let total_runs: i64 = exec_counts.values().map(|v| v.as_i64().unwrap()).sum();
    assert_eq!(total_runs, 1, "execution_counts={exec_counts:?}");
    assert_eq!(dash["pass_fail"]["passed"], 1);
    assert_eq!(dash["pass_fail"]["failed"], 1);
    assert_eq!(dash["task_counts"]["PASSED"], 1);
    assert_eq!(dash["task_counts"]["FAILED"], 1);
    assert_eq!(
        dash["test_count"], 2,
        "two distinct task names ingested into the quality graph"
    );
    assert!(dash["generated_at"].is_string());

    let (status, kpis) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/kpis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={kpis}");
    assert_eq!(kpis["project_id"], project.to_string());
    assert_eq!(
        kpis["execution_reliability"], 0.5,
        "1 passed / (1 passed + 1 failed)"
    );
    assert!(kpis["execution_efficiency_ms"].is_number(), "kpis={kpis}");
}

/// Category: empty/new project. No runs exist yet — counts must be real
/// zeros/empty objects and the KPI ratios must be `null` (no fabricated 0s
/// or 1s for a ratio with no data), per the BE-12 data-integrity rule.
#[tokio::test]
async fn be12_dashboard_and_kpis_on_empty_project_show_honest_absence_not_fake_zero() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Empty Project").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, dash) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/dashboard"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={dash}");
    assert_eq!(dash["execution_counts"], serde_json::json!({}));
    assert_eq!(dash["task_counts"], serde_json::json!({}));
    assert_eq!(dash["pass_fail"]["passed"], 0);
    assert_eq!(dash["pass_fail"]["failed"], 0);
    assert_eq!(dash["test_count"], 0);

    let (status, kpis) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/kpis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={kpis}");
    assert!(
        kpis["execution_reliability"].is_null(),
        "no terminal product outcome yet -> null, not 0 or 1; kpis={kpis}"
    );
    assert!(
        kpis["execution_efficiency_ms"].is_null(),
        "no finished task yet -> null, not a fake 0; kpis={kpis}"
    );
    // BE-12b Test 2 — no runs at all -> null, never a fabricated 0.
    assert!(
        kpis["execution_avg_run_duration_ms"].is_null(),
        "no completed run yet -> null, not a fake 0; kpis={kpis}"
    );
}

// ==================================================== BE-12b — average run duration

/// Sets a run's real `started_at`/`finished_at` columns to exact,
/// deterministic RFC3339 values via direct SQL — the only way to make a
/// duration-based average assertable exactly, since the real engine's own
/// wall-clock timing is not deterministic to the millisecond in a test
/// environment. This mutates only the two columns `dashboard_avg_run_
/// duration_ms` itself reads; it does not touch `state`, which is set by
/// the real engine/store calls already exercised above it in each test.
///
/// Fresh-audit finding (this unit): every caller polls `poll_run_state`
/// for a terminal state first, then calls this. But `poll_run_state`
/// reads `GET .../executions/:id`, which — while the run is still held in
/// the in-memory `RunRecord` registry — serves `record.summary()`
/// (`engine.rs`), whose `state`/`finished_at` fields are set in-memory
/// *before* the corresponding `store.set_run_finished(...)` DB write is
/// even issued (`engine.rs`'s completion path sets `record.state`/
/// `record.finished_at`, then `.await`s the DB `UPDATE`). So a terminal
/// state is real and observable via the API strictly before the real
/// `finished_at` column is guaranteed to be durably written — a distinct
/// two-step-write visibility gap, this time inside the run-completion
/// path rather than intelligence ingestion. Overwriting `finished_at`
/// here before that real `UPDATE` lands is a genuine race: the engine's
/// own (real, tiny-duration) write can land *after* this deliberate
/// overwrite and silently clobber it back to a near-zero duration,
/// breaking the exact-average assertions above. So this first waits for
/// the real write to land, then performs its own overwrite as the
/// guaranteed-last write.
async fn set_run_timestamps(state: &AppState, run_id: Uuid, started_at: &str, finished_at: &str) {
    wait_for_run_row_finished(state, run_id).await;
    sqlx::query("UPDATE runs SET started_at = ?, finished_at = ? WHERE id = ?")
        .bind(started_at)
        .bind(finished_at)
        .bind(run_id.to_string())
        .execute(&state.store.pool)
        .await
        .expect("set_run_timestamps");
}

/// Waits for the real `runs.finished_at` DB column (written by `engine.rs`'s
/// `store.set_run_finished`) to actually land, rather than trusting the
/// API-visible terminal state alone — see `set_run_timestamps`'s doc
/// comment for why those two signals are not the same moment.
async fn wait_for_run_row_finished(state: &AppState, run_id: Uuid) {
    for _ in 0..100 {
        let row = sqlx::query("SELECT finished_at FROM runs WHERE id = ?")
            .bind(run_id.to_string())
            .fetch_one(&state.store.pool)
            .await
            .expect("fetch run row");
        let finished_at: Option<String> = row.get("finished_at");
        if finished_at.is_some() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    panic!("run {run_id} never got a durable finished_at in time");
}

fn infra_error_plan(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "infra_error", "delay_ms": 0}}],
    })
}

fn fail_plan(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "fail", "delay_ms": 0}}],
    })
}

/// Test 1 — a real, deterministic average over 2 real terminal runs.
#[tokio::test]
async fn be12b_avg_run_duration_is_exact_over_two_real_runs() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "BE-12b Exact Average").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, run_a) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(simple_plan("run-a")),
        ),
    )
    .await;
    let (_, run_b) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(simple_plan("run-b")),
        ),
    )
    .await;
    let run_a_id: Uuid = run_a["run_id"].as_str().unwrap().parse().unwrap();
    let run_b_id: Uuid = run_b["run_id"].as_str().unwrap().parse().unwrap();
    poll_run_state(&state, project, &run_a_id.to_string(), &owner, "COMPLETED").await;
    poll_run_state(&state, project, &run_b_id.to_string(), &owner, "COMPLETED").await;

    // Real rows, now overwritten with exact, known durations: 10s and 30s.
    // The real (fast, sub-second) durations the engine actually produced
    // are deliberately replaced so the average is assertable exactly,
    // rather than "is a number" — the real engine/store code path that
    // ends a run is exercised unmodified above; only the timing values
    // themselves are made deterministic for this assertion.
    set_run_timestamps(
        &state,
        run_a_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:10.000Z",
    )
    .await;
    set_run_timestamps(
        &state,
        run_b_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:30.000Z",
    )
    .await;

    let (status, kpis) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/kpis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={kpis}");
    let avg = kpis["execution_avg_run_duration_ms"]
        .as_f64()
        .expect("execution_avg_run_duration_ms must be a real number");
    assert!(
        (avg - 20_000.0).abs() < 1.0,
        "expected exactly (10s+30s)/2 = 20000ms, got {avg}; kpis={kpis}"
    );
}

/// Test 3 — a run that has started but not finished contributes nothing.
#[tokio::test]
async fn be12b_running_run_excluded_from_avg_run_duration() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "BE-12b Running Excluded").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(delayed_plan("slow-run", 5_000)),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    // Real: wait for the task to actually start (started_at now real and
    // non-null) but well before its 5s delay finishes.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    poll_run_state(&state, project, &run_id, &owner, "RUNNING").await;

    let (status, kpis) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/kpis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={kpis}");
    assert!(
        kpis["execution_avg_run_duration_ms"].is_null(),
        "a run with started_at but no finished_at must contribute nothing -> null, not a fabricated value; kpis={kpis}"
    );

    // Real cleanup — cancel the still-running task so the test process
    // doesn't leave a 5s task outliving it.
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/cancel"),
            Some(&owner),
            None,
        ),
    )
    .await;
}

/// Test 4 — mixed real terminal states (COMPLETED/FAILED/PARTIAL/CANCELLED)
/// all count; a REJECTED run is a genuine, evidence-based negative case —
/// fresh audit of `store.rs::reject_run` (line 232-244) confirms a rejected
/// run's `started_at` is never set (`reject_run` only ever fires from
/// PENDING_APPROVAL, before the scheduler loop starts a run) — so it is
/// correctly excluded by the existing `started_at IS NOT NULL` predicate
/// alone, with no state-based special-casing required or added.
#[tokio::test]
async fn be12b_mixed_terminal_states_all_counted_rejected_naturally_excluded() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "BE-12b Mixed States").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // COMPLETED
    let (_, completed) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(simple_plan("completed-run")),
        ),
    )
    .await;
    let completed_id: Uuid = completed["run_id"].as_str().unwrap().parse().unwrap();
    poll_run_state(
        &state,
        project,
        &completed_id.to_string(),
        &owner,
        "COMPLETED",
    )
    .await;

    // FAILED
    let (_, failed) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(fail_plan("failed-run")),
        ),
    )
    .await;
    let failed_id: Uuid = failed["run_id"].as_str().unwrap().parse().unwrap();
    poll_run_state(&state, project, &failed_id.to_string(), &owner, "FAILED").await;

    // PARTIAL (a real INFRA_ERROR task, not a product failure)
    let (_, partial) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(infra_error_plan("partial-run")),
        ),
    )
    .await;
    let partial_id: Uuid = partial["run_id"].as_str().unwrap().parse().unwrap();
    poll_run_state(&state, project, &partial_id.to_string(), &owner, "PARTIAL").await;

    // CANCELLED
    let (_, cancelled) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(delayed_plan("cancelled-run", 5_000)),
        ),
    )
    .await;
    let cancelled_id: Uuid = cancelled["run_id"].as_str().unwrap().parse().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{cancelled_id}/cancel"),
            Some(&owner),
            None,
        ),
    )
    .await;
    poll_run_state(
        &state,
        project,
        &cancelled_id.to_string(),
        &owner,
        "CANCELLED",
    )
    .await;

    // REJECTED (never actually started — held at PENDING_APPROVAL then rejected)
    let (_, rejected) = send(&state, req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(serde_json::json!({
        "name": "rejected-run",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 0}}],
    })))).await;
    let rejected_id: Uuid = rejected["run_id"].as_str().unwrap().parse().unwrap();
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{rejected_id}/reject"),
            Some(&owner),
            Some(serde_json::json!({"reason": "be12b test"})),
        ),
    )
    .await;

    // Give every real terminal run an exact, known duration so their
    // combined average is assertable precisely (10s each, except the
    // rejected run which is deliberately left untouched — it must never
    // even be eligible, since its real `started_at` stays NULL).
    set_run_timestamps(
        &state,
        completed_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:10.000Z",
    )
    .await;
    set_run_timestamps(
        &state,
        failed_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:10.000Z",
    )
    .await;
    set_run_timestamps(
        &state,
        partial_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:10.000Z",
    )
    .await;
    set_run_timestamps(
        &state,
        cancelled_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:10.000Z",
    )
    .await;

    // Confirm the fresh-audit finding directly: the real REJECTED row
    // genuinely has no `started_at`.
    let row = sqlx::query("SELECT started_at, finished_at FROM runs WHERE id = ?")
        .bind(rejected_id.to_string())
        .fetch_one(&state.store.pool)
        .await
        .expect("fetch rejected run row");
    let rejected_started_at: Option<String> = row.get("started_at");
    let rejected_finished_at: Option<String> = row.get("finished_at");
    assert!(
        rejected_started_at.is_none(),
        "a real rejected run must never have a started_at (it never entered the scheduler)"
    );
    assert!(rejected_finished_at.is_some(), "a real rejected run does get a real finished_at — it just never qualifies for the duration average");

    let (status, kpis) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/kpis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={kpis}");
    let avg = kpis["execution_avg_run_duration_ms"]
        .as_f64()
        .expect("execution_avg_run_duration_ms must be a real number");
    // All 4 eligible runs (COMPLETED/FAILED/PARTIAL/CANCELLED) were set to
    // exactly 10s -> the average must be exactly 10000ms, proving the
    // REJECTED run's absence of `started_at` correctly kept it out.
    assert!(
        (avg - 10_000.0).abs() < 1.0,
        "expected exactly 10000ms (rejected run correctly excluded), got {avg}; kpis={kpis}"
    );
}

/// Test 5 — project isolation: Project A's real run durations never
/// influence Project B's average, and vice versa.
#[tokio::test]
async fn be12b_avg_run_duration_is_project_isolated() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "BE-12b Isolation A").await;
    let project_b = make_project(&state, "BE-12b Isolation B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (_, run_a) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&owner_a),
            Some(simple_plan("a-run")),
        ),
    )
    .await;
    let (_, run_b) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions"),
            Some(&owner_b),
            Some(simple_plan("b-run")),
        ),
    )
    .await;
    let run_a_id: Uuid = run_a["run_id"].as_str().unwrap().parse().unwrap();
    let run_b_id: Uuid = run_b["run_id"].as_str().unwrap().parse().unwrap();
    poll_run_state(
        &state,
        project_a,
        &run_a_id.to_string(),
        &owner_a,
        "COMPLETED",
    )
    .await;
    poll_run_state(
        &state,
        project_b,
        &run_b_id.to_string(),
        &owner_b,
        "COMPLETED",
    )
    .await;

    // Deliberately different, known durations per project.
    set_run_timestamps(
        &state,
        run_a_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:05.000Z",
    )
    .await; // 5s
    set_run_timestamps(
        &state,
        run_b_id,
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:01:00.000Z",
    )
    .await; // 60s

    let (status, kpis_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/kpis"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={kpis_a}");
    let avg_a = kpis_a["execution_avg_run_duration_ms"]
        .as_f64()
        .expect("project A must have a real average");
    assert!(
        (avg_a - 5_000.0).abs() < 1.0,
        "project A must reflect only its own 5s run, got {avg_a}; kpis_a={kpis_a}"
    );

    let (status, kpis_b) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/kpis"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={kpis_b}");
    let avg_b = kpis_b["execution_avg_run_duration_ms"]
        .as_f64()
        .expect("project B must have a real average");
    assert!((avg_b - 60_000.0).abs() < 1.0, "project B must reflect only its own 60s run, never project A's 5s run, got {avg_b}; kpis_b={kpis_b}");

    // Cross-project access is still real-403'd — same existing gate every
    // other scoped route uses, unmodified by this unit.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/kpis"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

/// Category: no cross-project leakage. Two projects each get their own
/// distinct run outcome; each project's dashboard/kpis must reflect only
/// its own data.
#[tokio::test]
async fn be12_dashboard_and_kpis_do_not_leak_across_projects() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Tenant A").await;
    let project_b = make_project(&state, "Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    // Project A: one passing run. Project B: one failing task.
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&owner_a),
            Some(simple_plan("a-run")),
        ),
    )
    .await;
    let fail_plan = serde_json::json!({
        "name": "b-run",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "fail", "delay_ms": 0}}],
    });
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions"),
            Some(&owner_b),
            Some(fail_plan),
        ),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (_, dash_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/dashboard"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(dash_a["pass_fail"]["passed"], 1);
    assert_eq!(dash_a["pass_fail"]["failed"], 0);

    let (_, dash_b) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/dashboard"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(dash_b["pass_fail"]["passed"], 0);
    assert_eq!(dash_b["pass_fail"]["failed"], 1);

    let (_, kpis_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/kpis"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(kpis_a["execution_reliability"], 1.0);

    let (_, kpis_b) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/kpis"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(kpis_b["execution_reliability"], 0.0);
}

/// Category: project isolation / unauthorized access. A key scoped to
/// project A must not be able to read project B's dashboard or kpis, even
/// though project B genuinely exists.
#[tokio::test]
async fn be12_dashboard_and_kpis_reject_unauthorized_cross_project_access() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Tenant A").await;
    let project_b = make_project(&state, "Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/dashboard"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/kpis"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // A global admin key, by contrast, may read either project.
    let admin = mint_key(&state, "admin", None).await;
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/dashboard"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// Category: nonexistent project -> structured 404, same as every other
/// project-scoped route (Actual BE-9's own convention).
#[tokio::test]
async fn be12_dashboard_and_kpis_nonexistent_project_returns_structured_not_found() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let phantom = Uuid::new_v4();

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{phantom}/dashboard"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{phantom}/kpis"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
}

/// Category: malformed input. A non-UUID path segment must hit the same
/// enveloped extractor-rejection path BE-10 already covers, not a panic or
/// a bare Axum 400.
#[tokio::test]
async fn be12_dashboard_malformed_project_id_returns_structured_validation_error() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;

    let (status, body) = send(
        &state,
        req(
            "GET",
            "/api/v1/projects/not-a-uuid/dashboard",
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
}

/// Category: unauthenticated request -> structured 401, matching every
/// other route behind the `require_auth` middleware layer.
#[tokio::test]
async fn be12_dashboard_and_kpis_require_authentication() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Auth Check").await;

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/dashboard"),
            None,
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/kpis"),
            None,
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

// ================================== BE-13 — Execution alignment + cancel

fn delayed_plan(name: &str, delay_ms: u64) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "pass", "delay_ms": delay_ms}}],
    })
}

/// Polls `GET .../executions/:run_id` until its `state` matches `want`, or
/// panics after a bounded number of attempts. Used instead of a fixed sleep
/// so these tests aren't flaky under load while still failing fast.
async fn poll_run_state(
    state: &AppState,
    project: Uuid,
    run_id: &str,
    key: &str,
    want: &str,
) -> Value {
    for _ in 0..100 {
        let (status, detail) = send(
            state,
            req(
                "GET",
                &format!("/api/v1/projects/{project}/executions/{run_id}"),
                Some(key),
                None,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body={detail}");
        if detail["state"] == want {
            return detail;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    panic!("run {run_id} did not reach state {want} in time");
}

/// Category: successful cancel + response schema + real lifecycle + no
/// false success + required event/audit behavior. A task with a real 1.5s
/// delay is cancelled almost immediately; the response must not claim the
/// run is already CANCELLED (it's asynchronous — Contract §12), but the run
/// must actually reach CANCELLED shortly after, with the task marked
/// CANCELLED, a completion event that says so, and a durable audit entry.
#[tokio::test]
async fn be13_cancel_running_execution_succeeds_and_is_auditable() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Cancel Success").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(delayed_plan("slow-run", 1_500)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let run_id = created["run_id"].as_str().expect("run_id").to_string();
    assert_eq!(created["status"], "SCHEDULED", "created={created}");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await; // let the task actually start

    let start = std::time::Instant::now();
    let (status, cancel_body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/cancel"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={cancel_body}");
    assert_eq!(cancel_body["run_id"], run_id);
    assert_ne!(
        cancel_body["status"], "COMPLETED",
        "cancel response must never claim the run completed normally"
    );

    let detail = poll_run_state(&state, project, &run_id, &owner, "CANCELLED").await;
    assert!(
        start.elapsed() < std::time::Duration::from_millis(1_000),
        "cancellation must actually interrupt the 1500ms task, not wait it out (took {:?})",
        start.elapsed()
    );
    let tasks = detail["tasks"].as_array().expect("tasks array");
    assert_eq!(tasks[0]["status"], "CANCELLED", "detail={detail}");

    // Required event behavior: the run's completion event must say it was
    // cancelled, not the generic "run finished".
    let (status, events) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}/events"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let events = events.as_array().unwrap();
    assert!(
        events.iter().any(|e| e["summary"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("cancel")),
        "events must include a cancellation-related summary; events={events:?}"
    );

    // Required audit behavior (Contract §12.3 "auditable").
    let audit = state.store.list_audit(None, 50).await.expect("list_audit");
    assert!(
        audit.iter().any(|a| a.action == "run.cancel"
            && a.target.as_deref() == Some(run_id.as_str())
            && a.outcome == "ok"),
        "audit_log must contain a run.cancel entry for this run"
    );
}

/// Category: terminal/non-cancellable state behavior + no false cancel
/// success. A run that already finished cannot be cancelled.
#[tokio::test]
async fn be13_cancel_rejects_already_terminal_run() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Cancel Terminal").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(simple_plan("fast-run")),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    poll_run_state(&state, project, &run_id, &owner, "COMPLETED").await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/cancel"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::CONFLICT, "CONFLICT");
}

/// Category: undefined-vs-resolved cancel semantics. A PENDING_APPROVAL run
/// has never entered the execution pipeline — it has its own dedicated stop
/// action (`/reject`) — so `/cancel` rejects it with a message pointing
/// there, rather than silently no-op-ing or inventing new semantics for it.
#[tokio::test]
async fn be13_cancel_rejects_pending_approval_run_pointing_to_reject() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Cancel Pending").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let gated = serde_json::json!({
        "name": "gated",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 0}}],
    });
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(gated),
        ),
    )
    .await;
    assert_eq!(created["status"], "PENDING_APPROVAL", "created={created}");
    let run_id = created["run_id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/cancel"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::CONFLICT, "CONFLICT");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("reject"),
        "body={body}"
    );
}

/// Category: cross-project cancel blocked + unauthorized cancel blocked.
#[tokio::test]
async fn be13_cancel_rejects_cross_project_and_insufficient_role() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Cancel Tenant A").await;
    let project_b = make_project(&state, "Cancel Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let viewer_a = mint_key(&state, "viewer", Some(&project_a.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&owner_a),
            Some(delayed_plan("r", 1_500)),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();

    // Cross-project: project B's own key, valid role, wrong project.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/cancel"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // Insufficient role: project A's own key, right project, wrong role.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/cancel"),
            Some(&viewer_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // Confirm neither rejected attempt actually cancelled it.
    let (_, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(
        detail["state"], "RUNNING",
        "an unauthorized cancel attempt must not have changed the run's state"
    );
}

/// Category: nonexistent project/run.
#[tokio::test]
async fn be13_cancel_nonexistent_run_and_project_return_structured_not_found() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Cancel Phantom").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let phantom_run = Uuid::new_v4();
    let phantom_project = Uuid::new_v4();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{phantom_run}/cancel"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{phantom_project}/executions/{phantom_run}/cancel"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

/// Category: §12.4/12.5 real data + response schema + cross-project
/// isolation + nonexistent task.
#[tokio::test]
async fn be13_run_tasks_list_and_detail_endpoints() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Tasks Tenant A").await;
    let project_b = make_project(&state, "Tasks Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&owner_a),
            Some(simple_plan("task-run")),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    poll_run_state(&state, project_a, &run_id, &owner_a, "COMPLETED").await;

    let (status, tasks) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={tasks}");
    let tasks_arr = tasks.as_array().unwrap();
    assert_eq!(tasks_arr.len(), 1);
    assert_eq!(tasks_arr[0]["id"], "t1");
    assert_eq!(tasks_arr[0]["status"], "PASSED");

    let (status, task_detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks/t1"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={task_detail}");
    assert_eq!(task_detail["id"], "t1");
    assert_eq!(task_detail["status"], "PASSED");

    // Nonexistent task id within a real run.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks/does-not-exist"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    // Cross-project: project B's key must not read project A's tasks.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks/t1"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

// Phase B cleanup: `be13_legacy_create_response_shape_is_unchanged` was
// removed together with the legacy flat `POST /runs` route it pinned —
// the scoped create response (`status` field, per BE-13) is covered by the
// tests above.

// =============================== BE-15 — Evidence/Findings/Audit contract paths

fn failing_plan(name: &str, task_name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": task_name, "simulate": {"outcome": "fail", "delay_ms": 0}}],
    })
}

/// A finding (`bug_drafts` row) is only drafted once the same normalized
/// failure signature recurs (`occurrence_count >= 2` — see
/// `intelligence.rs::record_failure`), so this runs the same failing task
/// name twice across two separate real runs, then polls until the finding
/// is actually persisted (ingestion happens after `RunCompleted`, in the
/// background). Returns the finding id.
///
/// Note: `SimulatedExecutor`'s failure reason is the fixed string
/// "simulated failure" regardless of task name, so the *signature*
/// clustering is keyed on is identical across every simulated-fail task —
/// clustering is per `(project_id, signature)`, not per task name. Two
/// calls to this helper *within the same project* would collapse onto the
/// same cluster and only the first ever drafts a finding; call it against
/// a fresh project per distinct finding a test needs.
async fn make_real_finding(state: &AppState, project: Uuid, owner: &str, task_name: &str) -> Uuid {
    for i in 0..2 {
        let (status, _created) = send(
            state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/executions"),
                Some(owner),
                Some(failing_plan(&format!("fail-run-{i}"), task_name)),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    for _ in 0..50 {
        let drafts = state
            .store
            .list_bug_drafts(Some(&project.to_string()))
            .await
            .expect("list_bug_drafts");
        if let Some(d) = drafts.into_iter().find(|d| d.title.contains(task_name)) {
            return d.id;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("finding for task '{task_name}' was not drafted in time");
}

/// Category: valid response + real data (not fabricated) for list/detail.
#[tokio::test]
async fn be15_findings_list_and_detail_reflect_real_recurring_failures() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Findings Real Data").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let finding_id = make_real_finding(&state, project, &owner, "flaky-login-check").await;

    let (status, list) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/findings"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={list}");
    let arr = list.as_array().unwrap();
    assert!(
        arr.iter().any(|f| f["id"] == finding_id.to_string()),
        "list={arr:?}"
    );

    let (status, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/findings/{finding_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={detail}");
    assert_eq!(detail["id"], finding_id.to_string());
    assert!(
        detail["title"]
            .as_str()
            .unwrap()
            .contains("flaky-login-check"),
        "detail={detail}"
    );
    // Contract §16.3: uncertain findings use NEEDS_REVIEW, never an
    // automatically-confirmed defect.
    assert_eq!(detail["review_state"], "NEEDS_REVIEW", "detail={detail}");
}

/// Category: "Failure analysis may correlate ... historical failures" —
/// the response must contain the finding's *real, linked* failure-cluster
/// data (root cause hypothesis, occurrence count), and the action must be
/// audited (Contract §25).
#[tokio::test]
async fn be15_finding_analysis_correlates_real_failure_cluster_and_is_audited() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Findings Analysis").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let finding_id = make_real_finding(&state, project, &owner, "flaky-payment-check").await;

    let (status, analysis) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/findings/{finding_id}/analysis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={analysis}");
    assert_eq!(
        analysis["finding"]["id"],
        finding_id.to_string(),
        "analysis={analysis}"
    );
    assert!(
        analysis["failure_cluster"]["occurrence_count"]
            .as_i64()
            .unwrap()
            >= 2,
        "analysis={analysis}"
    );
    assert!(
        analysis["failure_cluster"]["root_cause_hypothesis"].is_string(),
        "analysis={analysis}"
    );

    let audit = state.store.list_audit(None, 50).await.expect("list_audit");
    assert!(
        audit.iter().any(|a| a.action == "finding.analysis"
            && a.target.as_deref() == Some(finding_id.to_string().as_str())),
        "audit_log must record the analysis request"
    );
}

/// Category: review/promotion transitions + auditability, scoped path.
/// Two separate projects, not two task names in one — see
/// `make_real_finding`'s doc comment on why clustering can't distinguish
/// same-project findings by task name alone with `SimulatedExecutor`.
#[tokio::test]
async fn be15_finding_review_scoped_confirms_and_dismisses() {
    let (state, _dir) = test_state().await;
    let project_confirm = make_project(&state, "Findings Review Confirm").await;
    let project_dismiss = make_project(&state, "Findings Review Dismiss").await;
    let owner_confirm = mint_key(&state, "operator", Some(&project_confirm.to_string())).await;
    let owner_dismiss = mint_key(&state, "operator", Some(&project_dismiss.to_string())).await;

    let confirm_id =
        make_real_finding(&state, project_confirm, &owner_confirm, "confirm-me-check").await;
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_confirm}/findings/{confirm_id}/review"),
            Some(&owner_confirm),
            Some(serde_json::json!({"action": "confirm"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_confirm}/findings/{confirm_id}"),
            Some(&owner_confirm),
            None,
        ),
    )
    .await;
    assert_eq!(detail["review_state"], "CONFIRMED", "detail={detail}");

    let dismiss_id =
        make_real_finding(&state, project_dismiss, &owner_dismiss, "dismiss-me-check").await;
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_dismiss}/findings/{dismiss_id}/review"),
            Some(&owner_dismiss),
            Some(serde_json::json!({"action": "dismiss"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_dismiss}/findings/{dismiss_id}"),
            Some(&owner_dismiss),
            None,
        ),
    )
    .await;
    assert_eq!(detail["review_state"], "DISMISSED", "detail={detail}");

    let audit = state.store.list_audit(None, 50).await.expect("list_audit");
    assert!(audit
        .iter()
        .any(|a| a.action == "finding.review" && a.detail.as_deref() == Some("CONFIRMED")));
    assert!(audit
        .iter()
        .any(|a| a.action == "finding.review" && a.detail.as_deref() == Some("DISMISSED")));

    // Invalid action is rejected as validation, not silently accepted.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_confirm}/findings/{confirm_id}/review"),
            Some(&owner_confirm),
            Some(serde_json::json!({"action": "nonsense"})),
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
}

/// Category: cross-project isolation on every findings route + unauthorized
/// role on review.
#[tokio::test]
async fn be15_findings_cross_project_isolation_and_role_check() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Findings Tenant A").await;
    let project_b = make_project(&state, "Findings Tenant B").await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let viewer_b = mint_key(&state, "viewer", Some(&project_b.to_string())).await;

    let finding_id = make_real_finding(&state, project_b, &owner_b, "isolation-check").await;

    // A's list must never contain B's finding.
    let (_, list_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/findings"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert!(
        list_a
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["id"] != finding_id.to_string()),
        "list_a={list_a}"
    );

    // A cannot read/analyze/review B's finding under B's own project path.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/findings/{finding_id}"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/findings/{finding_id}/analysis"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/findings/{finding_id}/review"),
            Some(&owner_a),
            Some(serde_json::json!({"action": "confirm"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // B's own viewer key (right project, insufficient role) cannot review.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/findings/{finding_id}/review"),
            Some(&viewer_b),
            Some(serde_json::json!({"action": "confirm"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // Finding remains untouched by every rejected attempt.
    let (_, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/findings/{finding_id}"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(detail["review_state"], "NEEDS_REVIEW", "detail={detail}");
}

/// Category: missing auth + nonexistent project/finding.
#[tokio::test]
async fn be15_findings_require_authentication_and_reject_nonexistent_resources() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Findings Missing").await;
    let admin = mint_key(&state, "admin", None).await;
    let phantom_project = Uuid::new_v4();
    let phantom_finding = Uuid::new_v4();

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/findings"),
            None,
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{phantom_project}/findings"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/findings/{phantom_finding}"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

/// Category: §15.2 response-shape split. Detail is metadata (no raw
/// bytes embedded); the sibling `/content` path is where the actual
/// artifact lives.
#[tokio::test]
async fn be15_evidence_detail_is_metadata_not_raw_bytes() {
    let (state, dir) = test_state().await;
    let project = make_project(&state, "Evidence Shape").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(simple_plan("ev-run")),
        ),
    )
    .await;
    let run_id: Uuid = created["run_id"].as_str().unwrap().parse().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let evidence_store =
        crate::evidence::LocalEvidenceStore::new(dir.path().join("evidence")).unwrap();
    let (object_key, size) = evidence_store
        .put(&run_id.to_string(), "t1", "log", "shape-check bytes")
        .unwrap();
    let evidence_id = state
        .store
        .insert_evidence(run_id, "t1", "log", &object_key, "text/plain", size)
        .await
        .unwrap();

    let (status, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/evidence/{evidence_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["size_bytes"], size, "detail={detail}");
    assert_eq!(detail["content_type"], "text/plain", "detail={detail}");
    assert!(
        detail.get("object_key").is_some(),
        "detail must be the real EvidenceRow metadata, not a raw byte string; detail={detail}"
    );
}

/// Category: project-scoped audit list, isolation, role bar (Operator, not
/// Admin-only like the legacy global route), and legacy-route regression.
#[tokio::test]
async fn be15_audit_log_scoped_to_project_and_isolated() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Audit Tenant A").await;
    let project_b = make_project(&state, "Audit Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    // Real audited actions in each project.
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&owner_a),
            Some(simple_plan("audit-a")),
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions"),
            Some(&owner_b),
            Some(simple_plan("audit-b")),
        ),
    )
    .await;

    let (status, log_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/audit"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={log_a}");
    let entries_a = log_a.as_array().unwrap();
    assert!(
        !entries_a.is_empty(),
        "project A must see its own audit entries"
    );
    assert!(
        entries_a
            .iter()
            .all(|e| e["project_id"] == project_a.to_string()),
        "project A's audit view must never include another project's rows; entries={entries_a:?}"
    );

    // Cross-project blocked.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/audit"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // Nonexistent project.
    let phantom = Uuid::new_v4();
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{phantom}/audit"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    // Missing auth.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/audit"),
            None,
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

/// Category: admin-route regression — the platform-wide, Admin-only
/// `/admin/audit-log` route keeps working (Phase B cleanup removed the
/// `/bug-drafts` flat routes this test used to also cover; the scoped
/// findings/audit surface is covered by the tests above).
#[tokio::test]
async fn be15_legacy_admin_audit_log_still_works() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let operator = mint_key(&state, "operator", None).await;

    // Seed a real audit entry through a real scoped run creation (a global
    // key may target any existing project).
    let project = make_project(&state, "Admin Audit Seed").await;
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&admin),
            Some(simple_plan("audit-seed")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, log) = send(&state, req("GET", "/admin/audit-log", Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK, "body={log}");
    assert!(!log.as_array().unwrap().is_empty());

    // Still admin-only, unchanged.
    let (status, body) = send(
        &state,
        req("GET", "/admin/audit-log", Some(&operator), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

// ============================================ Phase 4 BE-10 — project-scoped
// test intelligence (distinct from the pre-existing Phase 2 "BE-10" section
// above, which is about extractor-rejection error envelopes and is
// unrelated). FE-21 found `store.rs::list_test_intelligence`/`TestIntelRow`
// real and populated but reachable only via the legacy, key-bound-scoped
// `/intelligence/tests` — unsafe for this frontend's multi-project-per-key
// model. These tests cover the new `GET
// /api/v1/projects/:project_id/intelligence/tests` route's project isolation,
// authorization, empty state, and real-field shape.

/// A plan whose one task deterministically passes or fails, so the resulting
/// `risk_score` (weighted 40% on historical failure rate) is verifiably
/// different between a passing and a failing task — used to prove isolation
/// is real (distinct per-project *data*, not just a coincidentally-empty
/// cross-project response).
fn scored_plan(run_name: &str, task_name: &str, outcome: &str) -> Value {
    serde_json::json!({
        "name": run_name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": task_name, "simulate": {"outcome": outcome, "delay_ms": 0}}],
    })
}

/// Polls the scoped intelligence route until it contains a *fully computed*
/// row for `task_name`, or panics after a bounded number of attempts.
/// `intel.ingest_run` runs *after* the run's HTTP-visible `state` already
/// went terminal (a real await gap in `engine.rs`), so polling run state
/// alone would be flaky here — this polls the actual thing under test.
///
/// Fresh-audit finding (this unit): `ingest_run` itself writes a task's
/// intelligence in three separate sequential upserts — `quality_nodes`
/// (`upsert_quality_node`) first, then `risk_assessments`/
/// `test_value_assessments`/`flakiness_metrics` after — and
/// `list_test_intelligence`'s `LEFT JOIN` means the row becomes
/// name-matchable the instant the *first* write lands, with the rest of
/// its fields still `NULL`. Waiting only for "a row named X exists" (the
/// old condition) can therefore return a genuinely mid-write row — this is
/// not "not enough total wait time", it's waiting on the wrong condition,
/// reproducible even in fast, unloaded, isolated runs. Every real caller
/// needs the fully-scored row, so that's what this helper now waits for.
async fn poll_intelligence_row(
    state: &AppState,
    project: Uuid,
    key: &str,
    task_name: &str,
) -> Value {
    for _ in 0..100 {
        let (status, rows) = send(
            state,
            req(
                "GET",
                &format!("/api/v1/projects/{project}/intelligence/tests"),
                Some(key),
                None,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body={rows}");
        if let Some(row) = rows.as_array().unwrap().iter().find(|r| {
            r["name"] == task_name
                && r["risk_score"].is_number()
                && r["value_score"].is_number()
                && r["flakiness_score"].is_number()
        }) {
            return row.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    panic!(
        "no fully-scored intelligence row named {task_name} appeared for project {project} in time"
    );
}

/// Test 1 (authorized project) + Test 5 (existing intelligence fields): a
/// real completed run produces a real, fully-populated `TestIntelRow` for
/// its own project, with the actual computed/persisted fields — not
/// invented ones.
#[tokio::test]
async fn phase4_be10_intelligence_scoped_to_authorized_project_with_real_fields() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Intel Tenant A").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(scored_plan("intel-run", "risk-task-a", "pass")),
        ),
    )
    .await;

    let row = poll_intelligence_row(&state, project, &owner, "risk-task-a").await;
    assert!(row["id"].as_str().is_some(), "row={row}");
    assert_eq!(row["name"], "risk-task-a", "row={row}");
    // Real persisted/computed fields (`store.rs::TestIntelRow`) — never
    // invented ones (no `id`/`name` other than these, no extra field).
    assert!(
        row["risk_score"].is_number(),
        "risk_score must be a real computed value; row={row}"
    );
    assert!(
        row["value_score"].is_number(),
        "value_score must be a real computed value; row={row}"
    );
    assert!(
        row["flakiness_score"].is_number(),
        "flakiness_score must be a real computed value; row={row}"
    );
    assert!(
        row["failure_rate"].is_number(),
        "failure_rate must be a real computed value; row={row}"
    );
    assert!(
        row["window_runs"].is_number(),
        "window_runs must be a real computed value; row={row}"
    );
    assert!(
        row["window_failures"].is_number(),
        "window_failures must be a real computed value; row={row}"
    );
    assert_eq!(
        row.as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "id",
            "name",
            "risk_score",
            "failure_rate",
            "dependency_count",
            "execution_cost_ms",
            "value_score",
            "flakiness_score",
            "window_runs",
            "window_failures"
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>(),
        "response must be exactly the real TestIntelRow shape, no invented/extra fields; row={row}"
    );
}

/// Test 2 (multi-project isolation) + Test 13/security (distinct real risk
/// values never cross projects). Project A's task always passes (low
/// historical failure rate → low risk); Project B's task always fails (high
/// historical failure rate → high risk) — genuinely different real data, not
/// just different names, so this proves the *values* never leak, not merely
/// that an empty list happens to look isolated.
#[tokio::test]
async fn phase4_be10_intelligence_isolated_across_projects() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Intel Tenant A").await;
    let project_b = make_project(&state, "Intel Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let admin = mint_key(&state, "admin", None).await; // unscoped — can legitimately see either.

    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&owner_a),
            Some(scored_plan("run-a", "task-a", "pass")),
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions"),
            Some(&owner_b),
            Some(scored_plan("run-b", "task-b", "fail")),
        ),
    )
    .await;

    let row_a = poll_intelligence_row(&state, project_a, &admin, "task-a").await;
    let row_b = poll_intelligence_row(&state, project_b, &admin, "task-b").await;
    let risk_a = row_a["risk_score"].as_f64().unwrap();
    let risk_b = row_b["risk_score"].as_f64().unwrap();
    assert!(risk_b > risk_a, "project B's always-failing task must score a real, distinctly higher risk than A's always-passing task; risk_a={risk_a} risk_b={risk_b}");

    // Project A's own view: task-a present, task-b (and B's higher risk
    // value) never appears.
    let (status, rows_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/intelligence/tests"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={rows_a}");
    let arr_a = rows_a.as_array().unwrap();
    assert!(
        arr_a.iter().any(|r| r["name"] == "task-a"),
        "project A must see its own intelligence; rows={arr_a:?}"
    );
    assert!(
        arr_a.iter().all(|r| r["name"] != "task-b"),
        "project A must never see project B's task; rows={arr_a:?}"
    );
    assert!(
        arr_a.iter().all(|r| r["risk_score"].as_f64() != Some(risk_b) || r["name"] != "task-b"),
        "project A's response must never carry project B's real risk value under B's task; rows={arr_a:?}"
    );

    // Project B's own view: symmetric.
    let (status, rows_b) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/intelligence/tests"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={rows_b}");
    let arr_b = rows_b.as_array().unwrap();
    assert!(
        arr_b.iter().any(|r| r["name"] == "task-b"),
        "project B must see its own intelligence; rows={arr_b:?}"
    );
    assert!(
        arr_b.iter().all(|r| r["name"] != "task-a"),
        "project B must never see project A's task; rows={arr_b:?}"
    );
}

/// Test 3 (unauthorized project) + existing-project-check regression: a key
/// scoped to Project A hitting Project B's route 403s (matches every other
/// scoped route's convention, `be15_audit_log_scoped_to_project_and_isolated`
/// above); a nonexistent project 404s; missing auth 401s with the real
/// envelope.
#[tokio::test]
async fn phase4_be10_intelligence_unauthorized_and_missing_project_rejected() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Intel Tenant A").await;
    let project_b = make_project(&state, "Intel Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;

    // A's key cannot name B's intelligence at all.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/intelligence/tests"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    // Nonexistent project.
    let phantom = Uuid::new_v4();
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{phantom}/intelligence/tests"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    // Missing auth.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/intelligence/tests"),
            None,
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

/// Test 4 (empty project): a real, authorized, existing project with zero
/// completed runs must return a truthful empty array — 200, not an error,
/// and no fabricated placeholder row.
#[tokio::test]
async fn phase4_be10_intelligence_empty_project_returns_truthful_empty_array() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Intel Tenant Empty").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, rows) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/intelligence/tests"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={rows}");
    assert_eq!(rows.as_array().unwrap().len(), 0, "a project with no completed runs must return a truthful empty array, not fabricated rows; body={rows}");
}

// Phase B cleanup: `phase4_be10_legacy_unscoped_intelligence_route_still_works`
// was removed together with the legacy key-bound `/intelligence/tests`
// route it pinned — the scoped route's real-data coverage above replaces it.

// ============================================================ AUD-005 — test cases

fn sample_test_case_body(title: &str) -> Value {
    serde_json::json!({
        "title": title,
        "description": "Verify the login form rejects an invalid password with a clear error.",
        "category": "Functional",
        "priority": "HIGH",
        "severity": "MAJOR",
        "preconditions": ["A registered user account exists"],
        "steps": [
            {"step_number": 1, "action": "Navigate to /login", "expected_result": "Login form is rendered"},
            {"step_number": 2, "action": "Submit an invalid password", "expected_result": "A visible validation error is shown"}
        ],
        "expected_result": "The user is not logged in and sees a clear error message",
        "assertions": ["No session cookie is set", "Error text is visible"],
        "requirement_reference": "FRD-VAL-05",
        "tags": ["auth", "negative"]
    })
}

/// Create -> Get -> List -> persistence-across-requests, all in one real
/// round trip: the row returned by `POST` is re-fetched independently by
/// both `GET :id` and `GET` (list), proving this is real database
/// persistence, not an in-memory echo of the request body.
#[tokio::test]
async fn aud005_create_get_list_test_case_real_persistence() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Persistence").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Reject invalid password")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["status"], "GENERATED", "created={created}");
    assert_eq!(created["title"], "Reject invalid password");
    assert_eq!(created["steps"].as_array().unwrap().len(), 2);

    // Independent GET (a fresh request, no shared in-memory state with the
    // POST above) must return the exact same real row.
    let (status, fetched) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={fetched}");
    assert_eq!(fetched["id"], created["id"]);
    assert_eq!(fetched["title"], "Reject invalid password");
    assert_eq!(fetched["description"], created["description"]);

    // List must include it.
    let (status, listed) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={listed}");
    let arr = listed.as_array().unwrap();
    assert!(
        arr.iter().any(|r| r["id"] == id),
        "list must contain the created case; rows={arr:?}"
    );
}

// ================================================== Phase 4 — requirement graph

fn test_case_body_with_reference(title: &str, reference: &str) -> Value {
    serde_json::json!({
        "title": title,
        "description": "description",
        "category": "Functional",
        "priority": "HIGH",
        "severity": "MAJOR",
        "steps": [
            {"step_number": 1, "action": "Do X", "expected_result": "Y happens"}
        ],
        "expected_result": "Y happens",
        "requirement_reference": reference
    })
}

/// Real end-to-end round trip through the HTTP API: two real test cases
/// created citing the same real `requirement_reference` both come back
/// from `GET .../requirement-impact?key=...`; a third citing a different
/// reference does not; a never-cited key returns a real, honest empty
/// list rather than a 404 or an error. The identity/edge-writing logic
/// itself is already fully covered by `intelligence.rs`'s own tests — this
/// proves the real HTTP wiring (`create_test_case_scoped` ->
/// `get_requirement_impact_scoped`) end to end.
#[tokio::test]
async fn phase4_requirement_impact_reflects_real_test_cases_created_against_it() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase4 Requirement Graph").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    for (title, reference) in [
        ("Login succeeds", "REQ-IMPACT-TEST"),
        ("Login rejects bad password", "REQ-IMPACT-TEST"),
        ("Unrelated case", "REQ-OTHER"),
    ] {
        let (status, body) = send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/test-cases"),
                Some(&owner),
                Some(test_case_body_with_reference(title, reference)),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "body={body}");
    }

    let (status, impact) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/requirement-impact?key=REQ-IMPACT-TEST"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={impact}");
    let rows = impact.as_array().expect("array");
    assert_eq!(rows.len(), 2, "rows={rows:?}");
    let titles: Vec<&str> = rows.iter().map(|r| r["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Login succeeds"), "titles={titles:?}");
    assert!(
        titles.contains(&"Login rejects bad password"),
        "titles={titles:?}"
    );
    assert!(
        !titles.contains(&"Unrelated case"),
        "a case citing a different requirement must not appear: titles={titles:?}"
    );

    let (status, empty) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/requirement-impact?key=REQ-NEVER-CITED"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={empty}");
    assert_eq!(
        empty.as_array().expect("array").len(),
        0,
        "a never-cited key must be a real, honest empty list, not an error: empty={empty}"
    );
}

/// A test case created with neither `requirement_reference` nor
/// `source_requirement` must still succeed normally (the graph-linking
/// step is genuinely optional, supplementary bookkeeping — see
/// `api.rs::create_test_case_scoped`'s own comment on why a linking
/// failure must never surface as a 500 for what is otherwise a fully
/// successful create).
#[tokio::test]
async fn phase4_creating_a_test_case_with_no_requirement_data_still_succeeds() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase4 No Requirement Data").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(serde_json::json!({
                "title": "No requirement data at all",
                "description": "description",
                "category": "Functional",
                "priority": "HIGH",
                "severity": "MAJOR",
                "steps": [{"step_number": 1, "action": "Do X", "expected_result": "Y happens"}],
                "expected_result": "Y happens"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
}

// ================================================ Phase 5 — plan/suite hierarchy

fn minimal_test_case_body(title: &str) -> Value {
    serde_json::json!({
        "title": title,
        "description": "description",
        "category": "Functional",
        "priority": "HIGH",
        "severity": "MAJOR",
        "steps": [{"step_number": 1, "action": "Do X", "expected_result": "Y happens"}],
        "expected_result": "Y happens"
    })
}

/// Full real round trip: create a plan, get it back independently, list it,
/// patch its name AND status in one call, and confirm both landed (proving
/// `update_test_plan`'s real `COALESCE` semantics reach the wire), then
/// delete it and confirm it is genuinely gone.
#[tokio::test]
async fn phase5_test_plan_full_crud_round_trip() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase5 Test Plan CRUD").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-plans"),
            Some(&owner),
            Some(serde_json::json!({"name": "Release 1.0", "entry_criteria": "All P0 cases automated"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["status"], "DRAFT", "created={created}");
    assert_eq!(created["suite_count"], 0);

    let (status, fetched) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-plans/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={fetched}");
    assert_eq!(fetched["name"], "Release 1.0");

    let (status, listed) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-plans"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={listed}");
    assert!(listed.as_array().unwrap().iter().any(|r| r["id"] == id));

    let (status, updated) = send(
        &state,
        req(
            "PATCH",
            &format!("/api/v1/projects/{project}/test-plans/{id}"),
            Some(&owner),
            Some(serde_json::json!({"name": "Release 1.0 GA", "status": "ACTIVE"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={updated}");
    assert_eq!(updated["name"], "Release 1.0 GA");
    assert_eq!(updated["status"], "ACTIVE");
    assert_eq!(
        updated["entry_criteria"], "All P0 cases automated",
        "an untouched field must survive the partial update"
    );

    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/projects/{project}/test-plans/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-plans/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A suite created inside a real plan, reassigned to a different real plan
/// via the dedicated `PUT .../test-suites/:id/plan`, then explicitly
/// detached (`plan_id: null`) — all through real HTTP, and `?plan_id=`
/// filtering on the list endpoint reflects each state correctly.
#[tokio::test]
async fn phase5_test_suite_plan_assignment_reassignment_and_clearing() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase5 Test Suite Plan").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, plan_a) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-plans"),
            Some(&owner),
            Some(serde_json::json!({"name": "Plan A"})),
        ),
    )
    .await;
    let plan_a_id = plan_a["id"].as_str().unwrap().to_string();
    let (_, plan_b) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-plans"),
            Some(&owner),
            Some(serde_json::json!({"name": "Plan B"})),
        ),
    )
    .await;
    let plan_b_id = plan_b["id"].as_str().unwrap().to_string();

    let (status, suite) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-suites"),
            Some(&owner),
            Some(serde_json::json!({"name": "Auth Suite", "plan_id": plan_a_id})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={suite}");
    let suite_id = suite["id"].as_str().unwrap().to_string();
    assert_eq!(suite["plan_id"], plan_a_id);

    let (status, reassigned) = send(
        &state,
        req(
            "PUT",
            &format!("/api/v1/projects/{project}/test-suites/{suite_id}/plan"),
            Some(&owner),
            Some(serde_json::json!({"plan_id": plan_b_id})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={reassigned}");
    assert_eq!(reassigned["plan_id"], plan_b_id);

    let (status, in_plan_b) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-suites?plan_id={plan_b_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={in_plan_b}");
    assert_eq!(in_plan_b.as_array().unwrap().len(), 1);

    let (status, cleared) = send(
        &state,
        req(
            "PUT",
            &format!("/api/v1/projects/{project}/test-suites/{suite_id}/plan"),
            Some(&owner),
            Some(serde_json::json!({"plan_id": null})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={cleared}");
    assert!(cleared["plan_id"].is_null(), "cleared={cleared}");
}

/// A case's suite membership set via the dedicated
/// `PUT .../test-cases/:id/suite`, reflected in `?suite_id=` filtering on
/// the test-cases list, and a cross-project `suite_id` genuinely rejected
/// rather than silently creating a dangling reference.
#[tokio::test]
async fn phase5_test_case_suite_assignment_and_cross_project_rejection() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Phase5 Case Suite A").await;
    let project_b = make_project(&state, "Phase5 Case Suite B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let admin = mint_key(&state, "admin", None).await;

    let (_, suite_a) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/test-suites"),
            Some(&owner_a),
            Some(serde_json::json!({"name": "Suite A"})),
        ),
    )
    .await;
    let suite_a_id = suite_a["id"].as_str().unwrap().to_string();
    let (_, suite_b) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/test-suites"),
            Some(&admin),
            Some(serde_json::json!({"name": "Suite B"})),
        ),
    )
    .await;
    let suite_b_id = suite_b["id"].as_str().unwrap().to_string();

    let (_, case) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/test-cases"),
            Some(&owner_a),
            Some(minimal_test_case_body("Movable case")),
        ),
    )
    .await;
    let case_id = case["id"].as_str().unwrap().to_string();

    let (status, assigned) = send(
        &state,
        req(
            "PUT",
            &format!("/api/v1/projects/{project_a}/test-cases/{case_id}/suite"),
            Some(&owner_a),
            Some(serde_json::json!({"suite_id": suite_a_id})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={assigned}");
    assert_eq!(assigned["suite_id"], suite_a_id);

    let (status, in_suite_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/test-cases?suite_id={suite_a_id}"),
            Some(&owner_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={in_suite_a}");
    assert_eq!(in_suite_a.as_array().unwrap().len(), 1);

    let (status, rejected) = send(
        &state,
        req(
            "PUT",
            &format!("/api/v1/projects/{project_a}/test-cases/{case_id}/suite"),
            Some(&owner_a),
            Some(serde_json::json!({"suite_id": suite_b_id})),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a suite from a different project must be rejected, not silently linked: body={rejected}"
    );
}

#[tokio::test]
async fn phase5_test_plan_rbac_viewer_forbidden_operator_allowed() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase5 Plan RBAC").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-plans"),
            Some(&viewer),
            Some(serde_json::json!({"name": "Viewer Attempt"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-plans"),
            Some(&operator),
            Some(serde_json::json!({"name": "Operator Attempt"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
}

// ===================================================== Phase 6 — crawler
//
// "playwright" is not a registered executor in `test_state()` (matching
// every other real-tool-executor test in this file — see
// `test_state_with_compiled_api()`'s own doc comment for the one exception
// this codebase makes, for the API executor specifically), so a run
// created here genuinely cannot produce a real crawl result; these tests
// verify validation/auth/wiring/ownership, the same split
// `executors/playwright/tests/crawler.spec.ts` and this file's own
// convention already draw elsewhere — the real, end-to-end browser
// mechanism is proven there, against a real fixture app, not here.

fn valid_crawl_body() -> Value {
    serde_json::json!({
        "base_url": "http://127.0.0.1:8080/app",
        "roles": [
            {"name": "admin", "login_actions": [
                {"type": "NAVIGATE", "url": "http://127.0.0.1:8080/login"},
                {"type": "FILL", "selector": "#username", "value": "admin"},
                {"type": "CLICK", "selector": "#submit-login"}
            ]},
            {"name": "anonymous", "login_actions": []}
        ]
    })
}

#[tokio::test]
async fn phase6_crawler_start_rejects_an_ssrf_target_via_the_existing_real_guard() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase6 Crawler SSRF").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(serde_json::json!({
                "base_url": "http://169.254.169.254/",
                "roles": [{"name": "anonymous", "login_actions": []}]
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
}

#[tokio::test]
async fn phase6_crawler_start_rejects_an_empty_role_list() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase6 Crawler Empty Roles").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(serde_json::json!({"base_url": "http://127.0.0.1:8080/app", "roles": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
}

/// A well-formed crawl request must genuinely create a real run through the
/// exact same pipeline every other execution uses — proven by fetching
/// that run back independently via the existing, unrelated
/// `GET .../executions/:run_id`, not just trusting the crawl endpoint's
/// own response.
#[tokio::test]
async fn phase6_crawler_start_creates_a_real_run_through_the_existing_pipeline() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase6 Crawler Real Run").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, started) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={started}");
    let run_id = started["run_id"].as_str().expect("run_id").to_string();

    let (status, run) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={run}");
    assert_eq!(run["tasks"].as_array().unwrap().len(), 1, "run={run}");
}

/// "playwright" has no registered executor in this test harness, so the
/// crawl's one task can never actually run and no result file is ever
/// written — the result endpoint must report a real, honest 404, never a
/// fabricated empty result standing in for one that was never produced.
#[tokio::test]
async fn phase6_crawler_result_is_a_real_404_when_no_result_was_ever_written() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase6 Crawler No Result").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, started) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    let run_id = started["run_id"].as_str().expect("run_id").to_string();

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/crawl/{run_id}/result"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

#[tokio::test]
async fn phase6_crawler_result_cross_project_access_is_rejected() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Phase6 Crawler Cross Project A").await;
    let project_b = make_project(&state, "Phase6 Crawler Cross Project B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (_, started) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/crawl"),
            Some(&owner_a),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    let run_id = started["run_id"].as_str().expect("run_id").to_string();

    // project_b's own key must not be able to reach project_a's crawl run
    // via project_b's own URL, even to learn "not found" vs "forbidden".
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/crawl/{run_id}/result"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

#[tokio::test]
async fn phase6_crawler_rbac_viewer_forbidden_operator_allowed() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase6 Crawler RBAC").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&viewer),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&operator),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
}

#[tokio::test]
async fn aud005_update_test_case_persists_and_ignores_status() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Update").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Original title")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, updated) = send(
        &state,
        req(
            "PATCH",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            Some(serde_json::json!({"title": "Updated title", "priority": "LOW"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={updated}");
    assert_eq!(updated["title"], "Updated title");
    assert_eq!(updated["priority"], "LOW");
    // Untouched fields survive a partial update.
    assert_eq!(updated["description"], created["description"]);
    // A PATCH body has no `status` field at all in its contract — status
    // never moves through this route.
    assert_eq!(updated["status"], "GENERATED");
}

#[tokio::test]
async fn aud005_delete_test_case_removes_it_for_real() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Delete").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("To be deleted")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "deleted case must genuinely be gone; body={body}"
    );
}

/// The real lifecycle end to end: GENERATED -> VALIDATED -> APPROVED, each
/// transition checked against the real, current row after each step.
#[tokio::test]
async fn aud005_lifecycle_validate_then_approve_succeeds_in_order() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Lifecycle").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Lifecycle case")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, validated) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={validated}");
    assert_eq!(validated["status"], "VALIDATED", "validated={validated}");
    assert!(
        validated["validation_notes"].as_str().is_some(),
        "a real validation note must be recorded"
    );

    let (status, approved) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={approved}");
    assert_eq!(approved["status"], "APPROVED", "approved={approved}");
}

/// FRD-EXE-11 enforcement: approval is not reachable by skipping validation
/// — a GENERATED case (never validated) must be rejected with a real 409,
/// and must remain GENERATED afterward (no silent partial advance).
#[tokio::test]
async fn aud005_approve_without_validate_first_is_rejected_with_409() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Skip Validate").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Not yet validated")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");

    let (_, row) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(
        row["status"], "GENERATED",
        "a rejected approve attempt must not silently move the case forward; row={row}"
    );
}

/// A structurally incomplete case (an empty step) fails real validation
/// with a specific, honest reason and stays GENERATED.
#[tokio::test]
async fn aud005_validate_rejects_structurally_incomplete_case() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Bad Case").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let mut body = sample_test_case_body("Missing step content");
    body["steps"] = serde_json::json!([{"step_number": 1, "action": "", "expected_result": ""}]);
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(body),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");

    let (_, row) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(
        row["status"], "GENERATED",
        "a failed validation must not advance the case; row={row}"
    );
}

#[tokio::test]
async fn aud005_reject_from_generated_and_validated_both_work() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Reject").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, a) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Reject from generated")),
        ),
    )
    .await;
    let id_a = a["id"].as_str().unwrap().to_string();
    let (status, rejected) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id_a}/reject"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={rejected}");
    assert_eq!(rejected["status"], "REJECTED");

    let (_, b) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Reject from validated")),
        ),
    )
    .await;
    let id_b = b["id"].as_str().unwrap().to_string();
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id_b}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    let (status, rejected) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id_b}/reject"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={rejected}");
    assert_eq!(rejected["status"], "REJECTED");

    // A rejected case cannot then be approved.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id_a}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");
}

/// Project isolation, enforced server-side: Project A's key can never see,
/// read, or mutate Project B's test case, and vice versa. Mirrors the
/// established convention every other scoped resource's own isolation test
/// already uses in this file.
#[tokio::test]
async fn aud005_project_isolation_enforced_both_directions() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "AUD-005 Isolation A").await;
    let project_b = make_project(&state, "AUD-005 Isolation B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/test-cases"),
            Some(&owner_a),
            Some(sample_test_case_body("A-only case")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    // B's key hitting A's case through B's own project URL: not found (the
    // case genuinely does not belong to B's project).
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/test-cases/{id}"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    // B's key hitting A's project URL directly (even naming A's own real
    // project id): forbidden — the standard cross-project 403.
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/test-cases/{id}"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    // B's list of its own project never contains A's case.
    let (status, listed) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/test-cases"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={listed}");
    assert!(
        listed.as_array().unwrap().iter().all(|r| r["id"] != id),
        "project B must never see project A's test case; rows={listed:?}"
    );

    // B cannot mutate A's case either.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/test-cases/{id}/reject"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

/// RBAC: a Viewer key can read but not create/mutate. Not just a "did the
/// route return non-2xx" check — asserts the real 403 envelope, and that
/// the read path stays open (RBAC is about mutation, not visibility).
#[tokio::test]
async fn aud005_rbac_viewer_cannot_mutate_but_can_read() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 RBAC").await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&operator),
            Some(sample_test_case_body("RBAC case")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&viewer),
            Some(sample_test_case_body("Viewer attempt")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&viewer),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // But a Viewer can still read.
    let (status, row) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&viewer),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={row}");
}

#[tokio::test]
async fn aud005_missing_project_and_missing_test_case_return_404() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 404s").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let phantom_project = Uuid::new_v4();
    let phantom_case = Uuid::new_v4();

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{phantom_project}/test-cases"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{phantom_case}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
}

#[tokio::test]
async fn aud005_invalid_input_rejected_with_validation_error() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Invalid").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let mut empty_title = sample_test_case_body("");
    empty_title["title"] = serde_json::json!("   ");
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(empty_title),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    let mut no_steps = sample_test_case_body("No steps");
    no_steps["steps"] = serde_json::json!([]);
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(no_steps),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
}

/// Handoff must refuse to fabricate an outcome: no `executor` and no
/// `simulate` in the body is a real 400, not a silent default pass.
#[tokio::test]
async fn aud005_handoff_without_executor_or_simulate_is_rejected() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 No Fake Outcome").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Needs real outcome")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
}

/// Handoff must refuse a case that is not yet APPROVED.
#[tokio::test]
async fn aud005_handoff_before_approval_is_rejected_with_409() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Not Approved Yet").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Still generated")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({"simulate": {"outcome": "pass"}})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");
}

/// THE full real path, end to end, using the actual execution engine:
/// Generate (persist) -> Validate -> Approve -> Handoff -> real DAG
/// scheduling -> real task execution -> real result -> traceability back
/// on the test case row. No mock, no simulated persistence layer, no
/// shadow engine — the same `router`/`engine`/`scheduler` every other
/// execution test in this file exercises.
#[tokio::test]
async fn aud005_full_generate_persist_validate_approve_handoff_execute_traceability() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 E2E").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // 1. Generate -> persist.
    let (status, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("E2E login rejection")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let id = created["id"].as_str().unwrap().to_string();

    // 2. Retrieve after "refresh" (a completely independent GET).
    let (status, refetched) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={refetched}");
    assert_eq!(refetched["status"], "GENERATED");

    // 3. Validate.
    let (status, validated) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={validated}");
    assert_eq!(validated["status"], "VALIDATED");

    // 4. Approve.
    let (status, approved) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={approved}");
    assert_eq!(approved["status"], "APPROVED");

    // 5. Handoff — explicit, disclosed simulated outcome (no real script
    // exists for a generated case; see `handoff_test_case_scoped`'s doc
    // comment). This dispatches through the real engine/scheduler.
    let (status, handoff) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({"simulate": {"outcome": "pass", "delay_ms": 0}})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={handoff}");
    let run_id = handoff["run_id"].as_str().expect("run_id").to_string();

    // 6. Real execution engine drives the real run to completion.
    let final_run = poll_run_state(&state, project, &run_id, &owner, "COMPLETED").await;
    assert_eq!(
        final_run["passed"], 1,
        "the single real task must have really passed; final_run={final_run}"
    );

    // 7. Traceability: the test case row now points at the real run/task,
    // and the real run/task both genuinely exist and reflect the outcome —
    // Test Case -> Run -> Task -> Result, all independently re-fetchable.
    let (status, final_case) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={final_case}");
    assert_eq!(final_case["status"], "EXECUTED", "final_case={final_case}");
    assert_eq!(
        final_case["executed_run_id"], run_id,
        "final_case={final_case}"
    );
    assert_eq!(
        final_case["executed_task_id"], id,
        "final_case={final_case}"
    );

    let task_id = final_case["executed_task_id"].as_str().unwrap();
    let task = final_run["tasks"] // not present on RunSummary directly; re-fetch full detail
        .as_array();
    let _ = task; // RunSummary (poll_run_state's return) has no `tasks`; fetch RunDetail explicitly below.

    let (status, run_detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={run_detail}");
    let tasks = run_detail["tasks"].as_array().expect("tasks");
    let matching_task = tasks
        .iter()
        .find(|t| t["id"] == task_id)
        .expect("the real task the test case was dispatched as must exist in the real run");
    assert_eq!(
        matching_task["status"], "PASSED",
        "matching_task={matching_task}"
    );
    assert_eq!(matching_task["name"], "E2E login rejection", "the real task's name must be the real test case's own title; matching_task={matching_task}");
}

/// Handoff creates a real, project-scoped run — visible through the
/// existing, already-real Execution Hub list/detail routes, not a shadow
/// path only the test-case feature can see.
#[tokio::test]
async fn aud005_handoff_run_is_a_real_execution_hub_run() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "AUD-005 Real Run Visibility").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Visible in Execution Hub")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;

    let (_, handoff) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({"simulate": {"outcome": "fail", "delay_ms": 0}})),
        ),
    )
    .await;
    let run_id = handoff["run_id"].as_str().unwrap().to_string();

    let (status, listed_runs) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={listed_runs}");
    assert!(
        listed_runs.as_array().unwrap().iter().any(|r| r["run_id"] == run_id),
        "the handoff-created run must appear in the real Execution Hub's own list; rows={listed_runs:?}"
    );
}

// ============================================================ Execution Handoff — real executor routing
//
// Note on naming: this codebase's own pre-existing section marker below
// ("Phase 2 — AI generation") uses "Phase 2" for an earlier, unrelated unit
// of work already in this file before this session. The tests in this
// section are from a *different* "Phase 2" (this session's own execution-
// handoff task) — deliberately not labeled "Phase 2" here too, to avoid
// implying they're the same phase as the section immediately below.

/// Test-only marker executor (legitimate per this repo's own fake/stub
/// audit rules — a controlled test fixture, never reachable from production
/// code). Its entire purpose is to return a fixed, distinctive outcome that
/// could not be confused with `SimulatedExecutor`'s own default (`Passed`,
/// no reason) — proof that a specific value of `TestCaseHandoffRequest
/// .executor`, sent as real JSON over the real HTTP handoff endpoint,
/// genuinely determines which registered `Executor` impl actually runs,
/// not merely that *a* run gets created. Mirrors the exact pattern already
/// used for this purpose in `engine_tests.rs` (`EvidenceProducingExecutor`,
/// `HangingExecutor`), applied here to the HTTP handoff endpoint
/// specifically, which no existing test exercised with a real (non-
/// "simulate") executor field.
struct MarkerExecutor;

#[async_trait::async_trait]
impl crate::executor::Executor for MarkerExecutor {
    async fn execute(&self, _task: &crate::domain::TaskDef) -> crate::executor::ExecutionOutcome {
        crate::executor::ExecutionOutcome {
            status: crate::domain::TaskStatus::Failed,
            reason: Some("phase2-marker-executor-actually-ran".to_string()),
            evidence: Some("marker executor evidence — proves this specific executor, not SimulatedExecutor, produced this outcome".to_string()),
        }
    }
}

async fn test_state_with_marker_executor() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register("marker", Arc::new(MarkerExecutor));
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    };
    (
        AppState::new(
            Arc::new(registry),
            store,
            policy,
            evidence,
            inert_ai_router(),
        ),
        dir,
    )
}

/// The core Phase 2 backend proof: `POST .../handoff` with a real
/// `executor` field (no `simulate`) dispatches to *that specific* real,
/// registered `Executor` — not always `SimulatedExecutor`'s default `Pass`.
/// If this test used `test_state()` (every other test in this file, and
/// every test in `engine_tests.rs`) it would prove nothing here: that
/// harness registers no executor other than `SimulatedExecutor`, so any
/// `executor` name — real or not — silently falls back to it (see the next
/// test, which documents that exact fallback deliberately). This is why a
/// dedicated `MarkerExecutor` registered under a real key is used instead.
#[tokio::test]
async fn phase2_handoff_with_a_real_executor_field_dispatches_to_that_specific_registered_executor()
{
    let (state, _dir) = test_state_with_marker_executor().await;
    let project = make_project(&state, "Phase 2 Real Executor Routing").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Routed to marker executor")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;

    // Real JSON body, exactly what `GeneratedTestCasesView.tsx`'s new
    // "Executor" dropdown sends for a real choice — `executor` only, no
    // `simulate`, no `params` (see `HandoffChoice`'s own doc comment for
    // why no params: no per-case params UI exists anywhere in this app).
    let (status, handoff) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({"executor": "marker"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={handoff}");
    let run_id = handoff["run_id"].as_str().expect("run_id").to_string();

    // A single always-Failed task resolves the run itself to FAILED (see
    // `be12b_mixed_terminal_states_all_counted_rejected_naturally_excluded`
    // for the same real state-machine fact with a different executor).
    let final_run = poll_run_state(&state, project, &run_id, &owner, "FAILED").await;
    // SimulatedExecutor would report 0 failed (its own default is Pass).
    // `MarkerExecutor` always reports Failed — this specific, distinctive
    // outcome is only reachable if the real `executor: "marker"` field
    // really was threaded from the HTTP body through `TaskDef.executor`
    // through `ExecutorRegistry::resolve` to this exact executor.
    assert_eq!(
        final_run["failed"], 1,
        "the marker executor's distinctive FAILED outcome must appear; final_run={final_run}"
    );

    let (status, run_detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={run_detail}");
    let tasks = run_detail["tasks"].as_array().expect("tasks");
    let task = tasks
        .iter()
        .find(|t| t["id"] == id)
        .expect("the handed-off task must exist in the real run");
    assert_eq!(task["status"], "FAILED", "task={task}");
    assert_eq!(
        task["reason"], "phase2-marker-executor-actually-ran",
        "task={task}"
    );
}

/// Phase 2.1 (brief §13) fix, superseding the Phase 2-era finding this test
/// used to document (`ExecutorRegistry::resolve` used to have no allowlist
/// and silently substituted `SimulatedExecutor` for any unrecognized
/// `executor` name — see `executor.rs::ExecutorRegistry::resolve`'s own doc
/// comment for the full before/after). The endpoint itself still accepts
/// any string (validating executor names against the registry at the HTTP
/// layer would require this handler to know the registry's live contents,
/// which it deliberately does not — the real rejection now happens one
/// layer down, in the engine, at real dispatch time), but the run this
/// produces must now genuinely fail with a real, honest reason — never a
/// silently-simulated `PASSED`.
#[tokio::test]
async fn phase2_1_handoff_with_an_unregistered_executor_name_is_a_real_honest_infra_error_not_a_silent_simulate(
) {
    let (state, _dir) = test_state().await; // only SimulatedExecutor is registered here
    let project = make_project(&state, "Phase 2.1 Unregistered Executor Safety").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Unregistered executor name")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/approve"),
            Some(&owner),
            None,
        ),
    )
    .await;

    let (status, handoff) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({"executor": "totally-unregistered-tool"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={handoff}");
    let run_id = handoff["run_id"].as_str().expect("run_id").to_string();

    // A single always-InfraError task resolves the run itself to PARTIAL
    // (the existing, real, pre-existing state-machine rule for a real
    // infrastructure failure — see `be12b_mixed_terminal_states_...`).
    let final_run = poll_run_state(&state, project, &run_id, &owner, "PARTIAL").await;
    assert_eq!(final_run["infra_error"], 1, "an unrecognized executor name must be a real, honest infra error, never a silently-simulated PASSED; final_run={final_run}");
    assert_eq!(final_run["passed"], 0, "final_run={final_run}");

    let (status, run_detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={run_detail}");
    let task = run_detail["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .expect("task must exist");
    assert_eq!(task["status"], "INFRA_ERROR", "task={task}");
    assert!(
        task["reason"]
            .as_str()
            .unwrap()
            .contains("NO_COMPATIBLE_EXECUTOR"),
        "task={task}"
    );
}

// ============================================================ Phase 2 — AI generation

fn valid_ai_response() -> &'static str {
    r#"{"test_cases":[{"title":"Reject short password","category":"Security","skill_id":"skill-security-testing",
        "preconditions":[],"steps":[{"action":"Submit a 7-character password","expected_result":"Validation error shown"}],
        "expected_result":"Validation error shown","priority":"HIGH","severity":"MAJOR",
        "evidence":"evidenced","source_reference":"must be at least 8 characters"}]}"#
}

#[tokio::test]
async fn phase2_generate_ai_returns_real_structured_cases_on_success() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase2 AI Success").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({
                "requirement_text": "Password must be at least 8 characters.",
                "skills": [{"id": "skill-security-testing", "name": "Security Testing", "description": ""}]
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["generation_method"], "ai", "body={body}");
    assert_eq!(body["provider"], "mock", "body={body}");
    assert_eq!(body["model"], "mock-model", "body={body}");
    assert!(body["fallback_reason"].is_null(), "body={body}");
    let cases = body["test_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1, "body={body}");
    assert_eq!(cases[0]["title"], "Reject short password");
    assert_eq!(cases[0]["evidence"], "evidenced");
    assert_eq!(
        cases[0]["source_reference"],
        "must be at least 8 characters"
    );
    // No `crawl_run_id` was supplied — the real, honest "zero real pages
    // used" value, not omitted from the response.
    assert_eq!(body["crawl_pages_used"], 0, "body={body}");
}

/// Phase 1 self-learning wiring — the endpoint now loads real feedback
/// context (`intelligence::Intelligence::load_feedback_context`) before
/// generating. This proves the real end-to-end wiring: seeded real risk
/// data + a real CONFIRMED bug draft for this exact project, via the same
/// `Store` methods `intelligence.rs`'s own tests use, then a genuine HTTP
/// call through the real router — must still return the same honest,
/// structurally-unchanged 200 response (feedback context only ever shifts
/// prompt content, never the response contract or the traceability/dedup
/// gates downstream of it).
#[tokio::test]
async fn phase2_generate_ai_succeeds_with_real_seeded_risk_and_confirmed_defect_data() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase2 AI With Feedback Context").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let risky_node = state
        .store
        .upsert_quality_node(&project.to_string(), "test", "Checkout Flow", "Checkout Flow")
        .await
        .expect("upsert risky node");
    state
        .store
        .upsert_risk_assessment(risky_node, 0.8, 0.0, 0.0, 0.9, 2, 0.0, 500.0, "{}")
        .await
        .expect("upsert risk assessment");
    let (cluster_id, _) = state
        .store
        .upsert_failure_cluster(&project.to_string(), "sig-confirmed", "hypothesis", 0.5)
        .await
        .expect("upsert failure cluster");
    let draft_id = state
        .store
        .insert_bug_draft(cluster_id, "Recurring checkout crash", "desc", "High", 0.5)
        .await
        .expect("insert bug draft");
    state
        .store
        .review_bug_draft(draft_id, "CONFIRMED", "tester")
        .await
        .expect("review bug draft");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({
                "requirement_text": "Password must be at least 8 characters.",
                "skills": [{"id": "skill-security-testing", "name": "Security Testing", "description": ""}]
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["generation_method"], "ai", "body={body}");
    let cases = body["test_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1, "body={body}");
}

// ---------------------------------------------------- crawl-grounded generation
// Real-crawl-content-reaches-the-prompt is proven at the `ai_generation`
// unit level (`integration_threads_real_crawl_context_all_the_way_into_the_
// actual_prompt_sent_to_the_model`); these two prove what is only real at
// this HTTP layer — the same ownership/completeness checks
// `GET .../crawl/:run_id/result` itself already holds, now also enforced at
// the point generation consumes a `crawl_run_id`.

/// A `crawl_run_id` from another project must not be usable to pull that
/// project's real crawl data into this project's generation — the exact
/// same real cross-project check `phase6_crawler_result_cross_project_
/// access_is_rejected` already proves for the crawl-result endpoint itself.
#[tokio::test]
async fn phase8_generate_ai_rejects_a_crawl_run_id_belonging_to_another_project() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project_a = make_project(&state, "Phase8 Crawl Owner").await;
    let project_b = make_project(&state, "Phase8 Crawl Other").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (_, started) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/crawl"),
            Some(&owner_a),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    let run_id = started["run_id"].as_str().expect("run_id").to_string();

    // project_b tries to ground its own generation in project_a's crawl run.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/test-cases/generate-ai"),
            Some(&owner_b),
            Some(serde_json::json!({
                "requirement_text": "Users can log in.",
                "skills": [],
                "crawl_run_id": run_id
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

/// A `crawl_run_id` for a crawl that never actually produced a result (the
/// same real state `phase6_crawler_result_is_a_real_404_when_no_result_was_
/// ever_written` already covers for the result endpoint itself) must be a
/// real, honest error here too — never silently generating without it as if
/// the caller had never asked for crawl grounding at all.
#[tokio::test]
async fn phase8_generate_ai_reports_a_real_error_when_the_crawl_has_no_result_yet() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase8 Crawl Incomplete").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, started) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    let run_id = started["run_id"].as_str().expect("run_id").to_string();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({
                "requirement_text": "Users can log in.",
                "skills": [],
                "crawl_run_id": run_id
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

/// The core truthfulness requirement: a provider failure must be reported
/// as `generation_method: "fallback"` with a real reason — never as a
/// fabricated `"ai"` success, and never as an opaque 500 that hides why.
#[tokio::test]
async fn phase2_generate_ai_reports_truthful_fallback_on_provider_failure() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Err(
        crate::ai_router::ExecutionError::Timeout,
    )))
    .await;
    let project = make_project(&state, "Phase2 AI Failure").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can log in.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a real, honest fallback report is not an HTTP error; body={body}"
    );
    assert_eq!(body["generation_method"], "fallback", "body={body}");
    assert!(body["provider"].is_null(), "body={body}");
    assert!(body["model"].is_null(), "body={body}");
    assert!(
        body["test_cases"].as_array().unwrap().is_empty(),
        "body={body}"
    );
    let reason = body["fallback_reason"]
        .as_str()
        .expect("a real reason must be present");
    assert!(
        reason.contains("TIMEOUT"),
        "reason must be the real, specific failure, not generic; reason={reason}"
    );
}

#[tokio::test]
async fn phase2_generate_ai_reports_truthful_fallback_on_invalid_structured_output() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok("not json at all"))).await;
    let project = make_project(&state, "Phase2 AI Garbage").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can log in.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["generation_method"], "fallback", "body={body}");
    assert!(
        body["fallback_reason"]
            .as_str()
            .unwrap()
            .contains("invalid structured output"),
        "body={body}"
    );
}

#[tokio::test]
async fn phase2_generate_ai_rejects_empty_requirement_text() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let project = make_project(&state, "Phase2 AI Empty Input").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "   ", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn phase2_generate_ai_rbac_viewer_forbidden_operator_allowed() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase2 AI RBAC").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let request_body = serde_json::json!({"requirement_text": "Users can log in.", "skills": []});

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&viewer),
            Some(request_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&operator),
            Some(request_body),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn phase2_generate_ai_project_isolation_enforced() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let project_a = make_project(&state, "Phase2 AI Isolation A").await;
    let project_b = make_project(&state, "Phase2 AI Isolation B").await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/test-cases/generate-ai"),
            Some(&owner_b),
            Some(serde_json::json!({"requirement_text": "Anything.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

/// End-to-end proof that a generated draft can be saved for real via the
/// pre-existing AUD-005 persistence path — the AI endpoint's own contract
/// (title/category/preconditions/steps/expected_result/priority/severity)
/// is a strict subset of `CreateTestCaseRequest`'s fields, so this is a
/// real compatibility fact, not an assumption.
#[tokio::test]
async fn phase2_ai_generated_draft_can_be_saved_via_existing_test_case_persistence() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase2 AI Save").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, gen_body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Password must be at least 8 characters.", "skills": []})),
        ),
    )
    .await;
    let draft = &gen_body["test_cases"][0];

    let (status, saved) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(serde_json::json!({
                "title": draft["title"],
                "description": draft["expected_result"],
                "category": draft["category"],
                "priority": draft["priority"],
                "severity": draft["severity"],
                "preconditions": draft["preconditions"],
                "steps": [{"step_number": 1, "action": draft["steps"][0]["action"], "expected_result": draft["steps"][0]["expected_result"]}],
                "expected_result": draft["expected_result"],
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "an AI-generated draft must be persistable via the existing real test-case store; body={saved}");
    assert_eq!(saved["status"], "GENERATED");
}

// ============================================================ Phase 3 — Custom Testing

fn valid_custom_testing_analysis_response() -> &'static str {
    r#"{"objective":"Verify login lockout after repeated invalid attempts","normalized_scope":"Authentication",
        "recommended_skills":["Security Testing","Edge Case Analysis"],"recommended_testing_types":["Negative","Security"],
        "risk_areas":["Account takeover via brute force"],"assumptions":["Lockout threshold is a fixed number of attempts"],
        "prerequisites":["A valid test account"],"limitations":["Exact lockout threshold not stated in the requirement"],
        "generation_approach":"Deterministic negative-path scenarios around repeated invalid attempts","diagnostics":[]}"#
}

fn valid_custom_testing_cases_response() -> &'static str {
    r#"{"test_cases":[{"title":"Lockout after 5 invalid attempts","category":"Security","skill_id":"skill-security-testing",
        "preconditions":[],"steps":[{"action":"Submit 5 invalid passwords in a row","expected_result":"Account is locked"}],
        "expected_result":"Account is locked","priority":"HIGH","severity":"MAJOR",
        "evidence":"evidenced","source_reference":"repeated invalid password attempts"}]}"#
}

#[tokio::test]
async fn phase3_custom_testing_returns_real_analysis_and_real_cases_on_success() {
    let (state, _dir) = test_state_with_ai_router(custom_testing_ai_infra_two_calls(
        valid_custom_testing_analysis_response(),
        valid_custom_testing_cases_response(),
    ))
    .await;
    let project = make_project(&state, "Phase3 Custom Testing Success").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/custom-testing/generate"),
            Some(&owner),
            Some(serde_json::json!({
                "requirement_text": "Test login lockout after repeated invalid password attempts.",
                "scope": "Authentication",
                "skills": [{"id": "skill-security-testing", "name": "Security Testing", "description": ""}]
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["generation_source"], "REAL_AI", "body={body}");
    assert_eq!(body["provider"], "cases-only", "the reported provider must be whichever real call produced the persisted cases; body={body}");
    assert_eq!(body["model"], "cases-model", "body={body}");
    assert!(body["failure_reason"].is_null(), "body={body}");

    let analysis = &body["analysis"];
    assert_eq!(
        analysis["objective"], "Verify login lockout after repeated invalid attempts",
        "body={body}"
    );
    assert_eq!(
        analysis["normalized_scope"], "Authentication",
        "body={body}"
    );
    assert!(
        analysis["recommended_skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "Security Testing"),
        "body={body}"
    );

    let cases = body["test_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1, "body={body}");
    assert_eq!(cases[0]["title"], "Lockout after 5 invalid attempts");
}

/// The core truthfulness requirement, mirrored from Phase 1/2's own
/// `generate-ai` tests: a real provider failure must be reported as
/// `generation_source: "UNAVAILABLE"` with a real reason — never fabricated
/// `REAL_AI` output, and never an opaque 500.
#[tokio::test]
async fn phase3_custom_testing_reports_truthful_unavailable_on_provider_failure() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Err(
        crate::ai_router::ExecutionError::Timeout,
    )))
    .await;
    let project = make_project(&state, "Phase3 Custom Testing Failure").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/custom-testing/generate"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Test login lockout.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a real, honest unavailable report is not an HTTP error; body={body}"
    );
    assert_eq!(body["generation_source"], "UNAVAILABLE", "body={body}");
    assert!(body["analysis"].is_null(), "no real provider was reachable at all, so no real analysis could have been produced; body={body}");
    assert!(
        body["test_cases"].as_array().unwrap().is_empty(),
        "body={body}"
    );
    assert!(body["provider"].is_null(), "body={body}");
    let reason = body["failure_reason"]
        .as_str()
        .expect("a real reason must be present");
    assert!(
        reason.contains("TIMEOUT"),
        "reason must be the real, specific failure, not generic; reason={reason}"
    );
}

/// A genuinely-produced analysis must survive even when the *subsequent*
/// case-generation call fails — proven at the HTTP layer with a single
/// provider whose fixed response satisfies the analysis schema but not the
/// (different) test-case schema.
#[tokio::test]
async fn phase3_custom_testing_preserves_a_real_analysis_when_case_generation_subsequently_fails() {
    let (state, _dir) =
        test_state_with_ai_router(ai_infra_with(Ok(valid_custom_testing_analysis_response())))
            .await;
    let project = make_project(&state, "Phase3 Custom Testing Partial").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/custom-testing/generate"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Test login lockout.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["generation_source"], "UNAVAILABLE", "body={body}");
    assert!(
        body["test_cases"].as_array().unwrap().is_empty(),
        "body={body}"
    );
    assert_eq!(
        body["analysis"]["objective"], "Verify login lockout after repeated invalid attempts",
        "a real, successfully-produced analysis must not be discarded; body={body}"
    );
    assert!(
        body["failure_reason"]
            .as_str()
            .unwrap()
            .contains("invalid structured output"),
        "body={body}"
    );
}

#[tokio::test]
async fn phase3_custom_testing_rejects_empty_requirement_text() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let project = make_project(&state, "Phase3 Custom Testing Empty Input").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/custom-testing/generate"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "   ", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn phase3_custom_testing_rbac_viewer_forbidden_operator_allowed() {
    let (state, _dir) =
        test_state_with_ai_router(ai_infra_with(Ok(valid_custom_testing_analysis_response())))
            .await;
    let project = make_project(&state, "Phase3 Custom Testing RBAC").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let request_body = serde_json::json!({"requirement_text": "Test login lockout.", "skills": []});

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/custom-testing/generate"),
            Some(&viewer),
            Some(request_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/custom-testing/generate"),
            Some(&operator),
            Some(request_body),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn phase3_custom_testing_project_isolation_enforced() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let project_a = make_project(&state, "Phase3 Custom Testing Isolation A").await;
    let project_b = make_project(&state, "Phase3 Custom Testing Isolation B").await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/custom-testing/generate"),
            Some(&owner_b),
            Some(serde_json::json!({"requirement_text": "Anything.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

/// End-to-end proof that a Custom-Testing-generated draft integrates with
/// the existing AUD-005 persistence path with zero new backend code: the
/// same `POST .../test-cases` used by Phase 1's drafts, with this
/// requirement's own real, honest traceability (`requirement_reference`,
/// `traceability`) — both pre-existing, generic fields, not new columns
/// invented for this phase.
#[tokio::test]
async fn phase3_custom_testing_generated_draft_can_be_saved_via_existing_test_case_persistence_with_real_traceability(
) {
    let (state, _dir) = test_state_with_ai_router(custom_testing_ai_infra_two_calls(
        valid_custom_testing_analysis_response(),
        valid_custom_testing_cases_response(),
    ))
    .await;
    let project = make_project(&state, "Phase3 Custom Testing Save").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, gen_body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/custom-testing/generate"),
            Some(&owner),
            Some(serde_json::json!({
                "requirement_text": "Test login lockout after repeated invalid password attempts.",
                "scope": "Authentication",
                "skills": []
            })),
        ),
    )
    .await;
    let draft = &gen_body["test_cases"][0];

    let (status, saved) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(serde_json::json!({
                "title": draft["title"],
                "description": draft["expected_result"],
                "category": draft["category"],
                "priority": draft["priority"],
                "severity": draft["severity"],
                "preconditions": draft["preconditions"],
                "steps": [{"step_number": 1, "action": draft["steps"][0]["action"], "expected_result": draft["steps"][0]["expected_result"]}],
                "expected_result": draft["expected_result"],
                "requirement_reference": "Custom Testing: Test login lockout after repeated invalid password attempts.",
                "traceability": {
                    "source": "custom_testing",
                    "scope": "Authentication",
                    "custom_requirement": "Test login lockout after repeated invalid password attempts.",
                    "analysis_objective": gen_body["analysis"]["objective"],
                },
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "a Custom-Testing-generated draft must be persistable via the existing real test-case store; body={saved}");
    assert_eq!(saved["status"], "GENERATED");
    assert_eq!(
        saved["requirement_reference"],
        "Custom Testing: Test login lockout after repeated invalid password attempts.",
        "body={saved}"
    );
    assert_eq!(
        saved["traceability"]["source"], "custom_testing",
        "body={saved}"
    );
    assert_eq!(
        saved["traceability"]["scope"], "Authentication",
        "body={saved}"
    );

    // Once persisted, this is an ordinary TestCaseRow — the existing
    // validate/approve/handoff/compile pipeline applies with zero new code,
    // proven by exercising the very next real lifecycle step for real.
    let id = saved["id"].as_str().unwrap();
    let (status, validated) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/validate"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={validated}");
    assert_eq!(validated["status"], "VALIDATED", "body={validated}");
}

// ============================================================ Phase 4 — API Testing

/// The orchestrator's own real, always-reachable `/health` endpoint — the
/// exact same "no separate product-under-test in this dev environment"
/// convention `PytestExecutor`/`SchemathesisExecutor`/`execution_plan::
/// DEFAULT_TARGET` already established, reused here rather than reaching
/// out to any third-party service from a test.
///
/// Deliberately still the literal, fixed `:8080` — not a "replacement"
/// hardcoded port, but the *one* real, production-significant loopback
/// address the unmodified SSRF guard (`executor::is_blocked_ip`'s
/// `ORCHESTRATOR_PORT`) already allows; every other loopback port is
/// genuinely rejected by that same, untouched guard
/// (`validate_target_url_rejects_loopback_on_a_non_orchestrator_port`), so
/// an arbitrary ephemeral port cannot be substituted here without
/// weakening real production security logic, which this fix must not do
/// (see `AIQA_BACKEND_TEST_SELF_CONTAINED_FIX_REPORT.md` §9/§11 for the
/// full reasoning). What this fix actually changes: this file no longer
/// requires a developer to have manually started a real orchestrator
/// there first — `ensure_local_health_target` below makes `cargo test`
/// start (and own) a real, minimal, self-contained target itself.
const HEALTH_URL: &str = "http://127.0.0.1:8080/health";

/// Real, minimal, self-contained HTTP target this test binary starts
/// itself — the fix for a genuine reproducibility gap: exactly 3 tests
/// (`phase4_api_test_executes_a_real_request_and_passes_a_real_status_assertion`,
/// `phase4_api_test_assertion_mismatch_genuinely_fails_not_a_fake_pass`,
/// `phase4_api_test_query_parameters_and_headers_genuinely_reach_the_real_request`)
/// assert on a *genuinely successful* real HTTP response from `HEALTH_URL`,
/// not merely that the request was attempted — every other test in this
/// file that references `HEALTH_URL` only needs the target to be
/// SSRF-valid (a real, allowed URL), not actually reachable, since it
/// asserts on validation/RBAC/project-isolation outcomes that are decided
/// before or independent of the real response.
///
/// Bound once, lazily, per test-binary process (`tokio::sync::OnceCell`,
/// safe for concurrent callers under `cargo test`'s default parallel
/// execution — only the first caller actually binds; every other caller
/// awaits that same real bind, so there is no rebind and no port
/// collision), not per-test — this never has to fight itself for the one
/// real, SSRF-allowed loopback port. If a real orchestrator dev instance
/// already happens to be listening there (e.g. a developer testing
/// manually alongside `cargo test`), the bind attempt honestly detects
/// `AddrInUse` and simply reuses whatever is already there instead of
/// erroring — the real orchestrator's own `/health` route (`api.rs::health`)
/// returns the exact same literal `"ok"` body this fixture does, so either
/// way the tests observe the same real, correct response. Never left
/// running beyond this test binary's own process lifetime — no separate
/// OS process is spawned; the port is reclaimed by the OS the moment
/// `cargo test` itself exits.
///
/// This is deliberately NOT `tokio::spawn`'d onto whichever `#[tokio::test]`
/// happens to call this first: each `#[tokio::test]` function gets its own,
/// independent Tokio runtime that is torn down (aborting anything spawned
/// on it) the instant that one test function returns. Under `cargo test`'s
/// default parallelism, that made this fixture's real lifetime depend on
/// how long the first-calling test's own runtime happened to stay alive
/// relative to every other test needing it — reliable by accident while the
/// test mix stayed the same, and silently broken (`CONNECTION_FAILED`) the
/// moment the mix (and therefore the scheduling) changed, as happened when
/// this suite gained new tests. The fix spawns the real axum server on a
/// dedicated OS thread with its own long-lived runtime that nothing ever
/// tears down, so it genuinely outlives every individual test.
static HEALTH_TARGET_STARTED: std::sync::Once = std::sync::Once::new();

async fn ensure_local_health_target() {
    HEALTH_TARGET_STARTED.call_once(|| {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect(
                "build a dedicated runtime for the self-contained :8080 health test target",
            );
            rt.block_on(async move {
                let app =
                    axum::Router::new().route("/health", axum::routing::get(|| async { "ok" }));
                match tokio::net::TcpListener::bind("127.0.0.1:8080").await {
                    Ok(listener) => {
                        let _ = ready_tx.send(());
                        let _ = axum::serve(listener, app).await;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                        // Something else (most likely a real, manually-started
                        // dev orchestrator) already owns :8080 -- its own real
                        // `/health` route serves the identical real response, so
                        // the tests that depend on this still get a real,
                        // correct target either way.
                        let _ = ready_tx.send(());
                    }
                    Err(e) => {
                        panic!("failed to bind the self-contained :8080 health test target: {e}")
                    }
                }
            });
        });
        // Block briefly (this closure runs synchronously, exactly once)
        // until the dedicated thread confirms it actually bound the real
        // listener (or found a real `AddrInUse` fallback) -- never an
        // arbitrary sleep standing in for a real readiness signal.
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the self-contained :8080 health test target did not become ready in time");
    });
}

#[tokio::test]
async fn phase4_api_test_executes_a_real_request_and_passes_a_real_status_assertion() {
    ensure_local_health_target().await;
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase4 API Testing Success").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&owner),
            Some(serde_json::json!({
                "method": "GET",
                "url": HEALTH_URL,
                "assertions": [{"type": "ASSERT_STATUS", "status": 200}]
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["task_status"], "PASSED",
        "a real, matching status assertion must genuinely pass; body={body}"
    );
    assert!(body["run_id"].is_string());
    assert!(body["finished_at"].is_string(), "body={body}");
    let result = &body["result"];
    assert_eq!(result["status"], 200, "body={body}");
    assert!(
        result["request_duration_ms"].as_u64().is_some(),
        "a real, measured latency must be present; body={body}"
    );
    let assertions = result["assertions"].as_array().unwrap();
    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0]["passed"], true, "body={body}");
    assert_eq!(
        body["response_body_preview"], "ok",
        "the real /health response body must be captured verbatim; body={body}"
    );
}

/// The core truthfulness requirement: an assertion that genuinely does not
/// match the real response must report FAILED — never a fabricated PASS.
#[tokio::test]
async fn phase4_api_test_assertion_mismatch_genuinely_fails_not_a_fake_pass() {
    ensure_local_health_target().await;
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase4 API Testing Assertion Fail").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&owner),
            Some(serde_json::json!({
                "method": "GET",
                "url": HEALTH_URL,
                "assertions": [{"type": "ASSERT_STATUS", "status": 404}]
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a real, honest FAILED result is not itself an HTTP error; body={body}"
    );
    assert_eq!(body["task_status"], "FAILED", "body={body}");
    let assertions = body["result"]["assertions"].as_array().unwrap();
    assert_eq!(assertions[0]["passed"], false, "body={body}");
    assert_eq!(assertions[0]["actual"], "200 OK", "the real observed status (reqwest's own Display, incl. reason phrase) must be reported, not the expected one; body={body}");
}

#[tokio::test]
async fn phase4_api_test_rejects_an_invalid_url_before_any_execution() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase4 API Testing Invalid URL").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&owner),
            Some(serde_json::json!({"method": "GET", "url": "not a url"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("INVALID_URL"),
        "body={body}"
    );
}

#[tokio::test]
async fn phase4_api_test_rejects_an_unsupported_http_method() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase4 API Testing Bad Method").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&owner),
            Some(serde_json::json!({"method": "TRACE", "url": HEALTH_URL})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("UNSUPPORTED_HTTP_METHOD"),
        "body={body}"
    );
}

/// Reuses the exact, pre-existing, real SSRF guard (`execution_plan::
/// validate_plan` -> `executor::validate_target_url`) — the same one
/// Phase 2.1 already proved rejects the real cloud-metadata address — not
/// a second, weaker check invented for this page.
#[tokio::test]
async fn phase4_api_test_rejects_a_disallowed_ssrf_target_via_the_existing_real_guard() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase4 API Testing SSRF").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&owner),
            Some(serde_json::json!({"method": "GET", "url": "http://169.254.169.254/latest/meta-data/"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("TARGET_NOT_ALLOWED"),
        "body={body}"
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("disallowed"),
        "body={body}"
    );
}

#[tokio::test]
async fn phase4_api_test_query_parameters_and_headers_genuinely_reach_the_real_request() {
    ensure_local_health_target().await;
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase4 API Testing Query/Headers").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // /health ignores query params/headers, but a real request that
    // includes them must still genuinely succeed (proving they reached a
    // real reqwest call without breaking it) — the plan-construction
    // level already proves the values themselves propagate correctly
    // (api_testing::tests), so this proves the *live* request tolerates
    // and sends them for real.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&owner),
            Some(serde_json::json!({
                "method": "GET",
                "url": HEALTH_URL,
                "query_params": [["debug", "true"]],
                "headers": {"X-Test-Header": "real-value"},
                "assertions": [{"type": "ASSERT_STATUS", "status": 200}]
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["task_status"], "PASSED", "body={body}");
}

#[tokio::test]
async fn phase4_api_test_rbac_viewer_forbidden_operator_allowed() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase4 API Testing RBAC").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;
    let request_body = serde_json::json!({"method": "GET", "url": HEALTH_URL});

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&viewer),
            Some(request_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/api-testing/execute"),
            Some(&operator),
            Some(request_body),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn phase4_api_test_project_isolation_enforced() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project_a = make_project(&state, "Phase4 API Testing Isolation A").await;
    let project_b = make_project(&state, "Phase4 API Testing Isolation B").await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/api-testing/execute"),
            Some(&owner_b),
            Some(serde_json::json!({"method": "GET", "url": HEALTH_URL})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

// Note: a dedicated "real connection failure -> INFRA_ERROR" integration
// test was deliberately not added here — the pre-existing, unmodified SSRF
// guard (`validate_target_url_rejects_loopback_on_a_non_orchestrator_port`,
// `execution_plan.rs`) rejects every loopback target except the
// orchestrator's own :8080, so this harness has no real, allowed-but-
// genuinely-unreachable target to construct one against honestly. Real
// connection-failure classification (`classify_reqwest_error`) is unit-
// tested directly in `executor.rs` and live-verified in the Phase 4 report
// §38 against a real, external, closed port.

// ==================== Truthfulness Cleanup Step 1 — Application Source verify
//
// `app_source::verify`'s own unit tests (in `app_source.rs`) already prove
// the real request/SSRF/classification logic in isolation. These tests
// prove the *real HTTP endpoint* (auth, RBAC, project scoping, the real
// JSON contract) is genuinely wired to that same logic — reusing the
// existing `ensure_local_health_target`/`HEALTH_URL` self-contained fixture
// (§ above) rather than a second, duplicate one, for exactly the same
// reason those Phase 4 tests needed it: the real, unmodified SSRF guard
// only ever allows a loopback target on `:8080`.

#[tokio::test]
async fn app_source_verify_a_real_reachable_target_reports_connected() {
    ensure_local_health_target().await;
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "App Source Verify - Connected").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/verify-application-source"),
            Some(&owner),
            Some(serde_json::json!({"url": HEALTH_URL})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["status"], "CONNECTED", "body={body}");
    assert_eq!(body["reachable"], true, "body={body}");
    assert_eq!(body["http_status"], 200, "body={body}");
    assert!(
        body["latency_ms"].as_u64().is_some(),
        "a real measured latency must be present; body={body}"
    );
}

#[tokio::test]
async fn app_source_verify_a_disallowed_target_reports_rejected_through_the_real_endpoint() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "App Source Verify - Rejected").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/verify-application-source"),
            Some(&owner),
            Some(serde_json::json!({"url": "http://169.254.169.254/latest/meta-data/"})),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a real, honest REJECTED result is not itself an HTTP error; body={body}"
    );
    assert_eq!(body["status"], "REJECTED", "body={body}");
    assert_eq!(body["reachable"], false, "body={body}");
    assert!(body["http_status"].is_null());
    assert!(body["latency_ms"].is_null(), "a rejected target must show no real latency, since no request was ever attempted; body={body}");
}

#[tokio::test]
async fn app_source_verify_a_malformed_url_reports_invalid_url() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "App Source Verify - Invalid").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/verify-application-source"),
            Some(&owner),
            Some(serde_json::json!({"url": "not a url"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["status"], "INVALID_URL", "body={body}");
}

#[tokio::test]
async fn app_source_verify_rbac_viewer_forbidden_operator_allowed() {
    ensure_local_health_target().await;
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "App Source Verify - RBAC").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;
    let request_body = serde_json::json!({"url": HEALTH_URL});

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/verify-application-source"),
            Some(&viewer),
            Some(request_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/verify-application-source"),
            Some(&operator),
            Some(request_body),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn app_source_verify_project_isolation_enforced() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project_a = make_project(&state, "App Source Verify - Isolation A").await;
    let project_b = make_project(&state, "App Source Verify - Isolation B").await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/verify-application-source"),
            Some(&owner_b),
            Some(serde_json::json!({"url": HEALTH_URL})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

// ================================== Phase 6 ERROR-fix — run-level supervision
//
// These deliberately do NOT go through the real `POST .../performance-testing/
// start` endpoint: that endpoint spawns the real, unmodified Load/Stress/
// Spike/Volume workload, which this fix explicitly must not be made to
// panic (a real panic can only be produced by a genuine bug, and forcing one
// into working workload logic just to test the supervisor would violate the
// fix brief's own "do not rewrite working Load/Stress/Spike/Volume logic").
// Instead, each test drives the exact real, unmodified pieces the real
// endpoint itself uses — `Store::insert_performance_run` (the same real DB
// write `/start` performs), a real `PerformanceRunHandle`, real
// `RequestSample`s standing in for "already collected before the failure",
// the real `supervise_workload_task` (a genuine Tokio panic, not simulated),
// and the real, `pub(crate)` `finalize_performance_run_as_error` — then
// verifies the real, persisted result through the real HTTP `GET .../status`
// endpoint, exactly as a client would. This is the "controlled internal test
// implementation" seam the fix brief itself names as an acceptable option,
// used only because the alternative (a real HTTP-triggered panic) would
// require touching code this fix must not touch.

fn perf_load_body(url: &str) -> Value {
    serde_json::json!({
        "request": {"method": "GET", "url": url},
        "expected_status": 200,
        "schedule": {"test_type": "LOAD", "concurrency": 2, "duration_secs": 5, "max_requests": null}
    })
}

async fn make_running_performance_run(
    state: &AppState,
    project: Uuid,
) -> (Uuid, Arc<crate::performance_testing::PerformanceRunHandle>) {
    let run_id = Uuid::new_v4();
    let handle = crate::performance_testing::PerformanceRunHandle::new(run_id);
    state
        .store
        .insert_performance_run(
            run_id,
            &project.to_string(),
            "LOAD test: GET http://127.0.0.1:8080/health",
            "LOAD",
            &serde_json::to_string(&perf_load_body(HEALTH_URL)).unwrap(),
            None,
            handle.started_at,
        )
        .await
        .expect("insert_performance_run");
    (run_id, handle)
}

fn real_sample(
    seq: u64,
    outcome: crate::performance_testing::SampleOutcome,
) -> crate::performance_testing::RequestSample {
    crate::performance_testing::RequestSample {
        sequence: seq,
        offset_ms: seq * 10,
        duration_ms: 5,
        outcome,
        http_status: Some(200),
        error_reason: None,
        concurrency_at_start: 2,
    }
}

#[tokio::test]
async fn phase6_error_a_genuine_internal_failure_is_detected_persisted_and_visible_live() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6 ERROR fix - live").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (run_id, handle) = make_running_performance_run(&state, project).await;

    // 40 real samples "already completed" before the unexpected failure —
    // real `RequestSample`s, not invented after the fact.
    for i in 0..40u64 {
        handle.samples.write().await.push(real_sample(
            i,
            crate::performance_testing::SampleOutcome::Success,
        ));
    }

    // The live handle is registered exactly as the real `/start` handler
    // registers it, so the live status-read path is exercised too.
    state
        .performance_tests
        .write()
        .await
        .insert(run_id, handle.clone());

    // A real, genuine, deterministic Tokio panic — caught by the real
    // supervisor, not a hand-built `SupervisionFailure`.
    let outcome = crate::performance_testing::supervise_workload_task(async {
        panic!("deterministic test failure: simulated internal orchestration fault");
    })
    .await;
    let failure = outcome.expect_err("must be a real supervision failure");

    crate::api::finalize_performance_run_as_error(&state, run_id, &handle, &failure).await;

    // No orphan RUNNING state: the live handle itself must show ERROR.
    assert_eq!(
        *handle.state.read().await,
        crate::performance_testing::PerfRunState::Error
    );

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/performance-testing/{run_id}/status"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["status"], "ERROR", "body={body}");
    assert_eq!(body["verdict"], "ERROR", "body={body}");
    assert!(body["live"].as_bool().unwrap(), "body={body}");
    let reason = body["verdict_reasons"][0]
        .as_str()
        .expect("a real reason must be present");
    assert!(
        reason.contains("deterministic test failure: simulated internal orchestration fault"),
        "reason={reason}"
    );
    assert!(
        body["finished_at"].is_string(),
        "finished_at must be persisted, body={body}"
    );

    // Partial metrics: exactly the 40 real samples that actually completed,
    // never padded to a planned total, never discarded.
    assert_eq!(body["metrics"]["total_requests"], 40, "body={body}");
    assert_eq!(body["metrics"]["successful"], 40, "body={body}");
    assert_eq!(body["sample_count"], 40, "body={body}");
}

#[tokio::test]
async fn phase6_error_persists_and_reloads_correctly_once_the_live_handle_is_gone() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6 ERROR fix - durable reload").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (run_id, handle) = make_running_performance_run(&state, project).await;
    handle.samples.write().await.push(real_sample(
        0,
        crate::performance_testing::SampleOutcome::Success,
    ));
    handle.samples.write().await.push(real_sample(
        1,
        crate::performance_testing::SampleOutcome::Error,
    ));
    // Deliberately NOT inserted into `state.performance_tests` — simulating
    // "no live handle exists in this process" (a restart, or another
    // process), forcing the status endpoint's durable-row fallback path.

    let outcome = crate::performance_testing::supervise_workload_task(async {
        panic!("deterministic test failure: durable reload check")
    })
    .await;
    let failure = outcome.expect_err("must be a real supervision failure");
    crate::api::finalize_performance_run_as_error(&state, run_id, &handle, &failure).await;

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/performance-testing/{run_id}/status"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["status"], "ERROR", "body={body}");
    assert_eq!(body["verdict"], "ERROR", "body={body}");
    assert!(
        !body["live"].as_bool().unwrap(),
        "must reflect the durable fallback path, body={body}"
    );
    assert!(body["verdict_reasons"][0]
        .as_str()
        .unwrap()
        .contains("deterministic test failure: durable reload check"));
    assert!(body["finished_at"].is_string());
    assert_eq!(body["metrics"]["total_requests"], 2, "the durable row's own persisted metrics must reflect the real partial sample count, body={body}");

    // Re-read a second time: nothing about a terminal ERROR row changes on
    // repeat reads (no wall-clock drift, no re-computation from a growing
    // "now").
    let (status2, body2) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/performance-testing/{run_id}/status"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(
        body2["metrics"], body["metrics"],
        "a terminal ERROR row's metrics must not change between reads"
    );
    assert_eq!(body2["finished_at"], body["finished_at"]);
}

#[tokio::test]
async fn phase6_error_cannot_later_be_overwritten_back_to_completed_terminal_idempotency() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6 ERROR fix - idempotency").await;
    let (run_id, handle) = make_running_performance_run(&state, project).await;

    let outcome = crate::performance_testing::supervise_workload_task(async {
        panic!("deterministic test failure: idempotency check")
    })
    .await;
    let failure = outcome.expect_err("must be a real supervision failure");
    crate::api::finalize_performance_run_as_error(&state, run_id, &handle, &failure).await;

    let after_error = state
        .store
        .get_performance_run(run_id)
        .await
        .expect("get")
        .expect("row exists");
    assert_eq!(after_error.status, "ERROR");

    // A second, independent finalize attempt for the SAME run (defensive:
    // this should never happen from real production code, since the two
    // finalize call sites are mutually exclusive branches of one
    // `JoinHandle` await — but the DB-level guard, not just "it shouldn't
    // happen," is what this test proves) must be a real, honest no-op.
    let config: crate::performance_testing::PerformanceTestConfig =
        serde_json::from_str(&after_error.config_json).unwrap();
    let rows_changed = state
        .store
        .finalize_performance_run(
            run_id,
            "COMPLETED",
            Some(crate::performance_testing::PerfVerdict::Completed.as_str()),
            Some("{}"),
            None,
            None,
            None,
        )
        .await
        .expect("finalize_performance_run should not error, just no-op");
    assert!(
        !rows_changed,
        "a second finalize attempt on an already-terminal run must not report a row as changed"
    );
    let _ = config;

    let still = state
        .store
        .get_performance_run(run_id)
        .await
        .expect("get")
        .expect("row exists");
    assert_eq!(
        still.status, "ERROR",
        "an already-terminal ERROR row must never be overwritten back to COMPLETED"
    );
    assert_eq!(
        still.error_reason.as_deref(),
        after_error.error_reason.as_deref()
    );
}

#[tokio::test]
async fn phase6_error_is_distinct_from_threshold_fail_and_from_cancelled() {
    // Pure, deterministic proof that the three real terminal "something is
    // wrong" states this system can report are genuinely different strings,
    // never collapsed into one generic failure — reused directly against
    // the same real `PerfVerdict`/`PerfRunState` the API actually returns.
    assert_ne!(
        crate::performance_testing::PerfVerdict::Error.as_str(),
        crate::performance_testing::PerfVerdict::Failed.as_str()
    );
    assert_ne!(
        crate::performance_testing::PerfVerdict::Error.as_str(),
        crate::performance_testing::PerfVerdict::Cancelled.as_str()
    );
    assert_ne!(
        crate::performance_testing::PerfRunState::Error.as_str(),
        "INTERRUPTED"
    );
}

#[tokio::test]
async fn phase6_error_normal_completion_regression_still_reaches_completed_not_error() {
    // Regression (fix brief §12.A): an ordinary, successful run supervised
    // by the new wrapper must still finalize as COMPLETED, never ERROR.
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6 ERROR fix - normal regression").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/performance-testing/start"),
            Some(&owner),
            Some(perf_load_body(HEALTH_URL)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let run_id = body["run_id"].as_str().unwrap();

    // Poll briefly for real completion (a real, short 5s Load test against
    // the orchestrator's own always-listening health endpoint).
    let mut final_status = Value::Null;
    for _ in 0..80 {
        let (s, b) = send(
            &state,
            req(
                "GET",
                &format!("/api/v1/projects/{project}/performance-testing/{run_id}/status"),
                Some(&owner),
                None,
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        if b["status"] != "RUNNING" {
            final_status = b;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(final_status["status"], "COMPLETED", "body={final_status}");
    assert_ne!(final_status["status"], "ERROR");
}

// ============================= Build order item 4 — AI-assisted BugAnalysis

fn ai_hypothesis_response() -> &'static str {
    r#"{"hypothesis":"The target dependency's readiness probe likely had not yet succeeded when this run started, independent of any code change in the system under test."}"#
}

/// Real routing to `AiTask::BugAnalysis`, a real successful AI outcome, and
/// — the explicit requirement — proof the AI hypothesis is genuinely
/// additive: the pre-existing, real, deterministic `root_cause_hypothesis`/
/// `confidence`/`occurrence_count` (`intelligence.rs::classify_failure`,
/// unrelated to this call) are read *before* the AI call and asserted
/// byte-for-byte unchanged *after* it, alongside the new, separately-named
/// `ai_hypothesis`/`ai_hypothesis_provider`/`ai_hypothesis_model` fields.
#[tokio::test]
async fn be18_finding_ai_hypothesis_returns_real_outcome_and_never_overwrites_deterministic_cluster_data(
) {
    let (state, _dir) =
        test_state_with_ai_router(ai_infra_with(Ok(ai_hypothesis_response()))).await;
    let project = make_project(&state, "AI Hypothesis Success").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let finding_id = make_real_finding(&state, project, &owner, "flaky-refund-check").await;

    let (_, before) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/findings/{finding_id}/analysis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    let real_deterministic_hypothesis = before["failure_cluster"]["root_cause_hypothesis"]
        .as_str()
        .unwrap()
        .to_string();
    let real_confidence = before["failure_cluster"]["confidence"].clone();
    let real_occurrence_count = before["failure_cluster"]["occurrence_count"].clone();
    assert!(
        before["failure_cluster"]["ai_hypothesis"].is_null(),
        "no AI hypothesis has been requested yet; before={before}"
    );

    let (status, after) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/findings/{finding_id}/ai-hypothesis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={after}");
    assert_eq!(after["generation_method"], "ai", "body={after}");
    assert_eq!(after["provider"], "mock", "body={after}");
    assert_eq!(after["model"], "mock-model", "body={after}");
    assert!(after["fallback_reason"].is_null(), "body={after}");
    assert_eq!(
        after["failure_cluster"]["ai_hypothesis"].as_str().unwrap(),
        "The target dependency's readiness probe likely had not yet succeeded when this run started, independent of any code change in the system under test.",
        "body={after}"
    );
    assert_eq!(
        after["failure_cluster"]["ai_hypothesis_provider"], "mock",
        "body={after}"
    );
    assert_eq!(
        after["failure_cluster"]["ai_hypothesis_model"], "mock-model",
        "body={after}"
    );
    assert!(
        after["failure_cluster"]["ai_hypothesis_generated_at"].is_string(),
        "body={after}"
    );

    // The real, pre-existing deterministic fields must be exactly what
    // they were before the AI call — additive, never overwritten.
    assert_eq!(
        after["failure_cluster"]["root_cause_hypothesis"], real_deterministic_hypothesis,
        "body={after}"
    );
    assert_eq!(
        after["failure_cluster"]["confidence"], real_confidence,
        "body={after}"
    );
    assert_eq!(
        after["failure_cluster"]["occurrence_count"], real_occurrence_count,
        "body={after}"
    );

    let audit = state.store.list_audit(None, 50).await.expect("list_audit");
    assert!(
        audit.iter().any(|a| a.action == "finding.ai_hypothesis"
            && a.target.as_deref() == Some(finding_id.to_string().as_str())
            && a.outcome == "ok"),
        "audit_log must record the AI hypothesis request"
    );
}

/// ACC-018 Failure Behavior: "Model/provider failure must result in:
/// controlled fallback, or NEEDS_REVIEW. Never fabricated output." A real
/// timeout must report `generation_method: "fallback"` (a normal 200, same
/// established convention as `generate-ai`) with a real reason, and must
/// leave the failure cluster exactly as it already was — `ai_hypothesis`
/// stays null, `review_state` stays the real, pre-existing `NEEDS_REVIEW`,
/// never a fabricated hypothesis or a falsely-advanced review state.
#[tokio::test]
async fn be18_finding_ai_hypothesis_reports_truthful_fallback_on_provider_timeout_never_fabricated()
{
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Err(
        crate::ai_router::ExecutionError::Timeout,
    )))
    .await;
    let project = make_project(&state, "AI Hypothesis Timeout").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let finding_id = make_real_finding(&state, project, &owner, "flaky-shipping-check").await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/findings/{finding_id}/ai-hypothesis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a real, honest fallback report is not an HTTP error; body={body}"
    );
    assert_eq!(body["generation_method"], "fallback", "body={body}");
    assert!(body["provider"].is_null(), "body={body}");
    assert!(body["model"].is_null(), "body={body}");
    let reason = body["fallback_reason"]
        .as_str()
        .expect("a real reason must be present");
    assert!(
        reason.contains("TIMEOUT"),
        "reason must be the real, specific failure; reason={reason}"
    );

    // Never fabricated output: no AI hypothesis was written, and the real
    // deterministic data (including review_state) is untouched.
    assert!(
        body["failure_cluster"]["ai_hypothesis"].is_null(),
        "body={body}"
    );
    assert!(
        body["failure_cluster"]["ai_hypothesis_provider"].is_null(),
        "body={body}"
    );
    assert_eq!(
        body["failure_cluster"]["review_state"], "NEEDS_REVIEW",
        "body={body}"
    );
    assert!(
        body["failure_cluster"]["root_cause_hypothesis"].is_string(),
        "the real deterministic hypothesis must still be present; body={body}"
    );

    let audit = state.store.list_audit(None, 50).await.expect("list_audit");
    assert!(
        audit
            .iter()
            .any(|a| a.action == "finding.ai_hypothesis" && a.outcome == "fallback"),
        "a fallback outcome must still be audited, honestly labeled"
    );
}

#[tokio::test]
async fn be18_finding_ai_hypothesis_rbac_viewer_forbidden_operator_allowed() {
    let (state, _dir) =
        test_state_with_ai_router(ai_infra_with(Ok(ai_hypothesis_response()))).await;
    let project = make_project(&state, "AI Hypothesis RBAC").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let finding_id = make_real_finding(&state, project, &owner, "flaky-invoice-check").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/findings/{finding_id}/ai-hypothesis"),
            Some(&viewer),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/findings/{finding_id}/ai-hypothesis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn be18_finding_ai_hypothesis_project_isolation_enforced() {
    let (state, _dir) =
        test_state_with_ai_router(ai_infra_with(Ok(ai_hypothesis_response()))).await;
    let project_a = make_project(&state, "AI Hypothesis Isolation A").await;
    let project_b = make_project(&state, "AI Hypothesis Isolation B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let finding_id = make_real_finding(&state, project_a, &owner_a, "flaky-tax-check").await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/findings/{finding_id}/ai-hypothesis"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

#[tokio::test]
async fn be18_finding_ai_hypothesis_missing_finding_returns_structured_not_found() {
    let (state, _dir) =
        test_state_with_ai_router(ai_infra_with(Ok(ai_hypothesis_response()))).await;
    let project = make_project(&state, "AI Hypothesis Not Found").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let phantom_finding = Uuid::new_v4();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/findings/{phantom_finding}/ai-hypothesis"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
}

// ================================================ AI Intelligence Layer (settings/status)

#[tokio::test]
async fn ai_status_returns_real_provider_snapshot_and_settings() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    state.ai_infra.discover_all().await;
    let admin = mint_key(&state, "admin", None).await;

    let (status, body) = send(&state, req("GET", "/api/v1/ai/status", Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let providers = body["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 1, "body={body}");
    assert_eq!(providers[0]["id"], "mock");
    assert_eq!(body["settings"]["mode"], "AUTO", "body={body}");
    assert_eq!(body["settings"]["fallback_enabled"], true, "body={body}");
}

#[tokio::test]
async fn ai_settings_put_persists_for_real_and_changes_routing_behavior() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    state.ai_infra.discover_all().await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, body) = send(
        &state,
        req(
            "PUT",
            "/api/v1/ai/settings",
            Some(&operator),
            Some(serde_json::json!({"mode": "MANUAL", "preferred_provider": "mock", "preferred_model": "mock-model", "fallback_enabled": false})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["mode"], "MANUAL", "body={body}");

    // A fresh, independent GET (not just an echo of the PUT body) must show the real, persisted setting.
    let (status, refetched) = send(
        &state,
        req("GET", "/api/v1/ai/settings", Some(&operator), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={refetched}");
    assert_eq!(refetched["mode"], "MANUAL");
    assert_eq!(refetched["preferred_model"], "mock-model");
    assert_eq!(refetched["fallback_enabled"], false);

    // The in-memory router used for real generation must reflect it too —
    // not just the settings row.
    let live_settings = state.ai_infra.settings().await;
    assert_eq!(live_settings.mode, crate::ai_router::RoutingMode::Manual);
}

#[tokio::test]
async fn ai_settings_survive_a_fresh_store_reload_real_durable_persistence() {
    let (state, dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let operator = mint_key(&state, "operator", None).await;
    send(
        &state,
        req(
            "PUT",
            "/api/v1/ai/settings",
            Some(&operator),
            Some(serde_json::json!({"mode": "MANUAL", "preferred_provider": "mock", "preferred_model": "mock-model", "fallback_enabled": true})),
        ),
    )
    .await;

    // A completely independent `Store` connection against the same real
    // sqlite file — proves this is durable, on-disk persistence, not an
    // in-memory echo of the request that set it.
    let reloaded = crate::store::Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("reconnect");
    let (mode, preferred_provider, preferred_model, fallback_enabled, task_overrides) = reloaded
        .get_ai_settings()
        .await
        .expect("query")
        .expect("a row must exist");
    assert_eq!(mode, "MANUAL");
    assert_eq!(preferred_provider.as_deref(), Some("mock"));
    assert_eq!(preferred_model.as_deref(), Some("mock-model"));
    assert!(fallback_enabled);
    assert_eq!(
        task_overrides, None,
        "a settings row saved with no task_overrides must persist NULL, not a fabricated empty object"
    );
}

#[tokio::test]
async fn ai_settings_task_overrides_survive_a_fresh_store_reload_real_durable_persistence() {
    let (state, dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let operator = mint_key(&state, "operator", None).await;
    send(
        &state,
        req(
            "PUT",
            "/api/v1/ai/settings",
            Some(&operator),
            Some(serde_json::json!({
                "mode": "AUTO",
                "preferred_provider": null,
                "preferred_model": null,
                "fallback_enabled": true,
                "task_overrides": {
                    "TEST_CASE_GENERATION": {"provider": "gemini", "model": "gemini-2.5-pro"}
                }
            })),
        ),
    )
    .await;

    // A completely independent `Store` connection against the same real
    // sqlite file — proves the override is durable, on-disk persistence,
    // not an in-memory echo of the request that set it.
    let reloaded = crate::store::Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("reconnect");
    let (_, _, _, _, task_overrides) = reloaded
        .get_ai_settings()
        .await
        .expect("query")
        .expect("a row must exist");
    let raw =
        task_overrides.expect("a real task override must persist as real JSON text, not NULL");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    assert_eq!(parsed["TEST_CASE_GENERATION"]["provider"], "gemini");
    assert_eq!(parsed["TEST_CASE_GENERATION"]["model"], "gemini-2.5-pro");

    // The in-memory router (restored the same way `main.rs` restores it on
    // a real process restart — the actual mechanism this test is proving,
    // not just the raw DB row) must resolve the very same override.
    let live_settings = state.ai_infra.settings().await;
    let entry = live_settings
        .task_overrides
        .get(&crate::ai_router::AiTask::TestCaseGeneration)
        .expect("the saved override must be readable back through the real AiSettings type");
    assert_eq!(entry.provider.as_deref(), Some("gemini"));
    assert_eq!(entry.model.as_deref(), Some("gemini-2.5-pro"));
}

#[tokio::test]
async fn ai_settings_rbac_viewer_cannot_mutate_but_can_read() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let viewer = mint_key(&state, "viewer", None).await;

    let (status, body) = send(
        &state,
        req(
            "PUT",
            "/api/v1/ai/settings",
            Some(&viewer),
            Some(serde_json::json!({"mode": "AUTO", "fallback_enabled": true})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, _) = send(
        &state,
        req("GET", "/api/v1/ai/settings", Some(&viewer), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn ai_refresh_requires_operator_and_returns_real_providers() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let viewer = mint_key(&state, "viewer", None).await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, _) = send(
        &state,
        req("POST", "/api/v1/ai/refresh", Some(&viewer), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = send(
        &state,
        req("POST", "/api/v1/ai/refresh", Some(&operator), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let providers = body.as_array().unwrap();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["id"], "mock");
}

/// Security (brief §22/§25): the real HTTP response for AI status/settings
/// must never carry a credential-shaped value anywhere in its JSON — even
/// when a real, non-empty credential is genuinely held and genuinely
/// discovered (a real `MjGatewayDiscovery`, not a fake).
#[tokio::test]
async fn ai_status_response_never_exposes_a_credential_shaped_value() {
    let discoveries: Vec<Arc<dyn crate::ai_router::ProviderDiscovery>> =
        vec![Arc::new(crate::ai_router::MjGatewayDiscovery {
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: Some("mj_test_should_never_leak_1234567890".to_string()),
            timeout: std::time::Duration::from_millis(100),
        })];
    let ai_infra = Arc::new(AiInfrastructure::new(
        discoveries,
        std::collections::HashMap::new(),
    ));
    let (state, _dir) = test_state_with_ai_router(ai_infra).await;
    state.ai_infra.discover_all().await;
    let admin = mint_key(&state, "admin", None).await;

    let (status, body) = send(&state, req("GET", "/api/v1/ai/status", Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["providers"][0]["configured"], true,
        "the real key's presence must genuinely be detected; body={body}"
    );
    let raw = body.to_string();
    assert!(
        !raw.contains("should_never_leak"),
        "the real credential value must never appear in this response; body={body}"
    );
}

// ======================================================== B12 — CORS allow-list

/// B12 remediation. The old `allow_origin(Any)` would echo back *any*
/// `Origin` header; a real allow-list must not. Drives a real request
/// through the real router wrapped in the real `CorsLayer` construction
/// (`cors_layer_for_origins` — the same function `main.rs` calls, via
/// `cors_layer_from_env`, minus the env-var indirection so this test is
/// deterministic under `cargo test`'s parallelism) and inspects the actual
/// `Access-Control-Allow-Origin` response header — CORS is enforced by the
/// browser reading that header, not by the server returning a non-2xx
/// status, so the header is the only real thing to assert on here.
#[tokio::test]
async fn cors_rejects_a_disallowed_origin() {
    let (state, _dir) = test_state().await;
    let app = router(state).layer(crate::api::cors_layer_for_origins(&[
        "http://localhost:3000".to_string(),
    ]));

    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .header("Origin", "http://evil.example")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(request).await.expect("router call");

    assert_eq!(resp.status(), StatusCode::OK, "the request itself still succeeds — CORS is a browser-enforced header, not a server-side block");
    let cors_header = resp.headers().get("access-control-allow-origin");
    assert!(
        cors_header.is_none() || cors_header.unwrap() != "http://evil.example",
        "a disallowed origin must never be echoed back in Access-Control-Allow-Origin, got: {cors_header:?}"
    );
}

#[tokio::test]
async fn cors_allows_the_configured_origin() {
    let (state, _dir) = test_state().await;
    let app = router(state).layer(crate::api::cors_layer_for_origins(&[
        "http://localhost:3000".to_string(),
    ]));

    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .header("Origin", "http://localhost:3000")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(request).await.expect("router call");

    assert_eq!(resp.status(), StatusCode::OK);
    let cors_header = resp
        .headers()
        .get("access-control-allow-origin")
        .expect("an allowed origin must receive the CORS header");
    assert_eq!(cors_header, "http://localhost:3000");
}

// ================================================ B11/L4 — approval durability

fn restart_test_policy() -> PolicyConfig {
    PolicyConfig {
        max_concurrency: 8,
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    }
}

async fn restart_test_state(
    db_path: &std::path::Path,
    evidence_dir: std::path::PathBuf,
) -> AppState {
    let store = Store::connect(db_path).await.expect("connect");
    let evidence = LocalEvidenceStore::new(evidence_dir).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register("maestro", Arc::new(SimulatedExecutor));
    AppState::new(
        Arc::new(registry),
        store,
        restart_test_policy(),
        evidence,
        inert_ai_router(),
    )
}

fn gated_plan(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 100}}],
    })
}

/// B11/L4 remediation — the exact scenario the finding named: request
/// approval, restart the process, confirm the approval is still pending
/// and can still be acted on. "Restart" here means what it means for every
/// other durability test in this file (`ai_settings_survive_a_fresh_store_reload_real_durable_persistence`):
/// a completely independent `AppState`/`Store`/registry reconnected to the
/// same real sqlite file, with a fresh, empty `state.runs` — exactly what
/// a real restarted process has, without needing to actually exec a second
/// OS process to prove it. Also exercises the real startup recovery pass
/// in between, to prove the more severe bug found alongside the documented
/// one (`recover_incomplete_runs` used to force every `PENDING_APPROVAL`
/// run to `FAILED` on *every* restart, clean or not — see that function's
/// doc comment) is actually fixed, not just the in-memory-lookup half.
#[tokio::test]
async fn approval_survives_a_process_restart_and_can_still_be_approved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("api.sqlite3");
    let run_id;
    let project;

    {
        let state = restart_test_state(&db_path, dir.path().join("evidence")).await;
        project = make_project(&state, "Restart Durability").await;
        let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
        let (status, created) = send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/executions"),
                Some(&owner),
                Some(gated_plan("gated-run")),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "body={created}");
        assert_eq!(created["status"], "PENDING_APPROVAL");
        run_id = created["run_id"].as_str().unwrap().to_string();
        // `state` (and its in-memory `state.runs`) is dropped here — the
        // run now exists only in the sqlite file, same as after a real
        // process exit.
    }

    {
        // The real startup recovery pass, against the same file, run the
        // same way `main.rs` runs it — must leave a genuinely pending
        // approval untouched, not force it to a terminal state.
        let store = Store::connect(&db_path).await.expect("connect");
        crate::engine::recover_incomplete_runs(&store)
            .await
            .expect("recovery");
        let summary = store
            .get_run_summary(Uuid::parse_str(&run_id).unwrap())
            .await
            .expect("query")
            .expect("run must still exist");
        assert_eq!(
            summary.state,
            RunState::PendingApproval,
            "startup recovery must not touch a run that is genuinely still awaiting approval"
        );
    }

    {
        let state = restart_test_state(&db_path, dir.path().join("evidence")).await;
        let owner2 = mint_key(&state, "operator", Some(&project.to_string())).await;
        assert!(
            state
                .runs
                .read()
                .await
                .get(&Uuid::parse_str(&run_id).unwrap())
                .is_none(),
            "sanity check: the new process must not already have this run in memory"
        );

        let (status, approved) = send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/executions/{run_id}/approve"),
                Some(&owner2),
                None,
            ),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a pending approval must survive a restart and remain approvable; body={approved}"
        );
        assert_eq!(approved["state"], "RUNNING");
    }
}

/// The other half of the same finding: reject, not approve, must also
/// still work against a rehydrated run.
#[tokio::test]
async fn rejection_survives_a_process_restart_and_can_still_be_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("api.sqlite3");
    let run_id;
    let project;

    {
        let state = restart_test_state(&db_path, dir.path().join("evidence")).await;
        project = make_project(&state, "Restart Durability Reject").await;
        let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
        let (status, created) = send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/executions"),
                Some(&owner),
                Some(gated_plan("gated-run-reject")),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "body={created}");
        run_id = created["run_id"].as_str().unwrap().to_string();
    }

    {
        let state = restart_test_state(&db_path, dir.path().join("evidence")).await;
        let owner2 = mint_key(&state, "operator", Some(&project.to_string())).await;
        let (status, rejected) = send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/executions/{run_id}/reject"),
                Some(&owner2),
                Some(serde_json::json!({"reason": "no longer needed"})),
            ),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a pending approval must survive a restart and remain rejectable; body={rejected}"
        );
        assert_eq!(rejected["state"], "REJECTED");
    }
}

#[tokio::test]
async fn delete_project_admin_success_with_cascading_cleanup() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let project = make_project(&state, "To Delete").await;

    // Verify project exists
    let (status, got) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}"), Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "To Delete");

    // DELETE project
    let (status, _) = send(
        &state,
        req("DELETE", &format!("/api/v1/projects/{project}"), Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify GET now returns 404
    let (status, _) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}"), Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Verify audit log has project.delete entry
    let audit_rows = state
        .store
        .list_audit(None, 100)
        .await
        .expect("audit log");
    let delete_entry = audit_rows
        .iter()
        .find(|a| a.action == "project.delete" && a.target.as_deref() == Some(&project.to_string()));
    assert!(delete_entry.is_some(), "audit log must record project.delete");
}

#[tokio::test]
async fn delete_project_forbidden_for_non_admin_roles() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Role Check").await;
    let operator = mint_key(&state, "operator", None).await;
    let viewer = mint_key(&state, "viewer", None).await;

    let (status, _) = send(
        &state,
        req("DELETE", &format!("/api/v1/projects/{project}"), Some(&operator), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &state,
        req("DELETE", &format!("/api/v1/projects/{project}"), Some(&viewer), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_project_returns_404_for_unknown_project() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let random_id = Uuid::new_v4();

    let (status, _) = send(
        &state,
        req("DELETE", &format!("/api/v1/projects/{random_id}"), Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_project_scoped_admin_cannot_delete_different_project() {
    let (state, _dir) = test_state().await;
    let proj_a = make_project(&state, "Project A").await;
    let proj_b = make_project(&state, "Project B").await;

    let scoped_admin_a = mint_key(&state, "admin", Some(&proj_a.to_string())).await;

    let (status, _) = send(
        &state,
        req("DELETE", &format!("/api/v1/projects/{proj_b}"), Some(&scoped_admin_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_logs_returns_recent_history_and_respects_project_scoping() {
    let (state, _dir) = test_state().await;
    let proj_a = make_project(&state, "Proj Logs A").await;
    let proj_b = make_project(&state, "Proj Logs B").await;

    let admin = mint_key(&state, "admin", None).await;
    let operator_a = mint_key(&state, "operator", Some(&proj_a.to_string())).await;

    // Emit logs
    state.log_bus.emit("INFO", "ai_router", "Global discovery started", None, None, None).await;
    state.log_bus.emit("INFO", "generation", "Generating for A", None, Some(&proj_a.to_string()), None).await;
    state.log_bus.emit("INFO", "execution", "Running B", None, Some(&proj_b.to_string()), None).await;

    // Admin sees all logs
    let (status, logs) = send(
        &state,
        req("GET", "/api/v1/logs", Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = logs.as_array().expect("array");
    assert_eq!(arr.len(), 3);

    // Operator A sees global + Proj A logs, but not Proj B
    let (status, logs_a) = send(
        &state,
        req("GET", "/api/v1/logs", Some(&operator_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr_a = logs_a.as_array().expect("array");
    assert_eq!(arr_a.len(), 2);
    let msgs: Vec<&str> = arr_a.iter().map(|l| l["message"].as_str().unwrap()).collect();
    assert!(msgs.contains(&"Global discovery started"));
    assert!(msgs.contains(&"Generating for A"));
    assert!(!msgs.contains(&"Running B"));

    // Scoped route GET /api/v1/projects/:id/logs
    let (status, proj_logs) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{proj_a}/logs"), Some(&operator_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(proj_logs.as_array().unwrap().len(), 2);
}
