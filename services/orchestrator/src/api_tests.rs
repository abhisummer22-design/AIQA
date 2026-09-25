//! BE-9 regression tests — the structured error envelope (API Contract
//! §2.5). Unlike every other test file in this crate, these drive real
//! HTTP requests through the real `axum::Router` in-process
//! (`tower::ServiceExt::oneshot`) rather than calling handler/store
//! functions directly, because BE-9's own requirement is specifically about
//! the *HTTP response body* — status code alone proves nothing about
//! whether the JSON envelope shape is actually correct.

use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor, RecordingEmbedder,
    RecordingExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel, Embedder};
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

/// Like `custom_testing_ai_infra_two_calls` above, but the caller supplies
/// the "cases" side's own real executor directly — used with a
/// `RecordingExecutor` so a test can inspect the real prompt text
/// `custom_testing::run` actually sent, e.g. to prove Vector Store/RAG
/// grounding genuinely reached this endpoint's own prompt.
fn custom_testing_ai_infra_with_cases_executor(
    analysis_response: &str,
    cases_executor: Arc<dyn AiExecutor>,
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
    executors.insert("cases-only", cases_executor);

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
            unconfigured_embedder(),
        ),
        dir,
    )
}

/// Vector Store + RAG Retrieval v1 — the honest default every existing test
/// (predating this feature) gets: no real `GEMINI_API_KEY`, same as a real
/// deployment with it unset. `state.embedder.is_configured() == false`, so
/// `find_similar_approved_test_cases`/`embed_approved_test_case` both no-op
/// without ever attempting a real network call — an existing test's
/// behavior is completely unchanged by this feature's addition.
fn unconfigured_embedder() -> Arc<dyn Embedder> {
    Arc::new(FakeEmbedder::unconfigured())
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
            unconfigured_embedder(),
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
    test_state_with_ai_router_and_embedder(ai_router, unconfigured_embedder()).await
}

/// Vector Store + RAG Retrieval v1 — like `test_state_with_ai_router`, but
/// also with a caller-supplied `Embedder`, so a test can script a real,
/// deterministic embedding vector (`FakeEmbedder::configured`) instead of
/// ever making a real network call to Gemini.
async fn test_state_with_ai_router_and_embedder(
    ai_router: Arc<AiInfrastructure>,
    embedder: Arc<dyn Embedder>,
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
        AppState::new(
            Arc::new(registry),
            store,
            policy,
            evidence,
            ai_router,
            embedder,
        ),
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

/// Category: real re-planning v1 (`retry_failed_tasks_scoped`) — only real
/// INFRA_ERROR/TIMEOUT tasks are ever retried, never a real FAILED (product)
/// failure, and a retried task's real dependency edge to a task outside the
/// retry set is dropped (that task's own real prior result already stands).
#[tokio::test]
async fn be_retry_failed_only_retries_real_infra_classified_tasks_with_correctly_filtered_dependencies(
) {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Retry Failed").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let plan = serde_json::json!({
        "name": "mixed-outcome-run",
        "concurrency": 4,
        "tasks": [
            {"id": "t-pass", "name": "t-pass", "simulate": {"outcome": "pass", "delay_ms": 0}},
            {"id": "t-fail", "name": "t-fail", "simulate": {"outcome": "fail", "delay_ms": 0}},
            {"id": "t-infra-1", "name": "t-infra-1", "simulate": {"outcome": "infra_error", "delay_ms": 0}},
            {
                "id": "t-infra-2",
                "name": "t-infra-2",
                "prerequisites": ["t-pass"],
                "simulate": {"outcome": "infra_error", "delay_ms": 0}
            }
        ],
    });
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(plan),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    poll_run_state(&state, project, &run_id, &owner, "PARTIAL").await;

    let (status, retry) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/retry-failed"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={retry}");
    let mut retried_ids: Vec<String> = retry["retried_task_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    retried_ids.sort();
    assert_eq!(
        retried_ids,
        vec!["t-infra-1".to_string(), "t-infra-2".to_string()],
        "only the two real infra-classified tasks — never t-pass or the real t-fail failure; retry={retry:?}"
    );

    let new_run_id: Uuid = retry["run_id"].as_str().unwrap().parse().unwrap();
    let new_plan = state
        .store
        .get_run_plan(new_run_id)
        .await
        .expect("get_run_plan")
        .expect("new run must have a real persisted plan");
    assert_eq!(new_plan.tasks.len(), 2, "new_plan={new_plan:?}");
    let t_infra_2 = new_plan
        .tasks
        .iter()
        .find(|t| t.id == "t-infra-2")
        .expect("t-infra-2 must be in the retry plan");
    assert!(
        t_infra_2.prerequisites.is_empty(),
        "t-infra-2's real prerequisite (t-pass) is not itself being retried, so it real dependency edge must be dropped, not point at a task that does not exist in this new run: t_infra_2={t_infra_2:?}"
    );

    // The new run genuinely executes for real through the exact same real
    // pipeline — it is not a fabricated/no-op record. (PARTIAL, not FAILED
    // — engine.rs's own real state computation checks InfraError/Timeout
    // before Failed, and both retried tasks are real infra_error outcomes
    // again in this deterministic simulation.)
    poll_run_state(&state, project, &new_run_id.to_string(), &owner, "PARTIAL").await;
}

#[tokio::test]
async fn be_retry_failed_rejects_a_run_with_no_real_infra_classified_failures() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Retry Failed None").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // A real product failure only — must never be silently retried.
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(fail_plan("only-a-real-failure")),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    poll_run_state(&state, project, &run_id, &owner, "FAILED").await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/retry-failed"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
}

#[tokio::test]
async fn be_retry_failed_rejects_a_run_that_has_not_finished_yet() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Retry Failed Not Terminal").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let long_running_plan = serde_json::json!({
        "name": "still-running",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "pass", "delay_ms": 30000}}],
    });
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions"),
            Some(&owner),
            Some(long_running_plan),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/executions/{run_id}/retry-failed"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_envelope(status, &body, StatusCode::CONFLICT, "CONFLICT");
}

#[tokio::test]
async fn be_retry_failed_is_project_scoped_and_role_gated() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Retry Failed Tenant A").await;
    let project_b = make_project(&state, "Retry Failed Tenant B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let viewer_a = mint_key(&state, "viewer", Some(&project_a.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&owner_a),
            Some(infra_error_plan("cross-tenant-retry")),
        ),
    )
    .await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    // PARTIAL, not FAILED — engine.rs's own real state computation checks
    // InfraError/Timeout before Failed.
    poll_run_state(&state, project_a, &run_id, &owner_a, "PARTIAL").await;

    // A different project's real operator key must never reach it.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/retry-failed"),
            Some(&owner_b),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // A real viewer-role key must never be permitted to trigger a retry.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions/{run_id}/retry-failed"),
            Some(&viewer_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
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
    let audit = state
        .store
        .list_audit(None, 50, 0)
        .await
        .expect("list_audit");
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

    let audit = state
        .store
        .list_audit(None, 50, 0)
        .await
        .expect("list_audit");
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

    let audit = state
        .store
        .list_audit(None, 50, 0)
        .await
        .expect("list_audit");
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
    let entries_a = log_a["rows"].as_array().unwrap();
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
    // The real total (`store.rs::count_audit`) must match this project's
    // own real row count, not the legacy admin route's global total and
    // not the batch's own length under a limit smaller than the real count.
    assert_eq!(
        log_a["total"].as_i64().unwrap(),
        entries_a.len() as i64,
        "body={log_a}"
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

/// Category: a global-scope key (the bootstrap admin key has no project) acting inside a project. Its audit
/// rows used to carry the key's own scope, that is no project, so the project's audit trail stayed empty
/// however much the key did there.
#[tokio::test]
async fn be15_audit_trail_lists_a_global_keys_actions_under_the_project_they_happened_in() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "Audit Global A").await;
    let project_b = make_project(&state, "Audit Global B").await;
    let admin = mint_key(&state, "admin", None).await;

    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/executions"),
            Some(&admin),
            Some(simple_plan("global-a")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/test-cases"),
            Some(&admin),
            Some(sample_test_case_body("Global key case")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/executions"),
            Some(&admin),
            Some(simple_plan("global-b")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, log_a) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_a}/audit"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={log_a}");
    let rows_a = log_a["rows"].as_array().unwrap();
    let actions_a: Vec<&str> = rows_a
        .iter()
        .map(|r| r["action"].as_str().unwrap())
        .collect();
    assert!(actions_a.contains(&"run.create"), "actions={actions_a:?}");
    assert!(
        actions_a.contains(&"test_case.create"),
        "actions={actions_a:?}"
    );
    assert!(
        rows_a
            .iter()
            .all(|r| r["project_id"] == project_a.to_string()),
        "rows={rows_a:?}"
    );
    assert_eq!(log_a["total"].as_i64().unwrap(), rows_a.len() as i64);

    // Project B's trail holds B's own single action, and none of A's.
    let (status, log_b) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project_b}/audit"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={log_b}");
    let rows_b = log_b["rows"].as_array().unwrap();
    assert_eq!(rows_b.len(), 1, "rows={rows_b:?}");
    assert_eq!(rows_b[0]["action"], "run.create");
    assert_eq!(rows_b[0]["project_id"], project_b.to_string());
}

/// Category: `project.create` has no project in its URL (the project did not exist until the request), so it
/// is attributed to the project it created, which is what a new project's audit trail should start with.
#[tokio::test]
async fn be15_project_create_is_audited_under_the_new_project() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, created) = send(
        &state,
        req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(serde_json::json!({ "name": "Audited From The Start" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let id = created["id"].as_str().unwrap().to_string();

    let (status, log) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{id}/audit"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={log}");
    let rows = log["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "rows={rows:?}");
    assert_eq!(rows[0]["action"], "project.create");
    assert_eq!(rows[0]["project_id"], id);
}

/// Category: which project a request URL names, which is what the attribution above is read from.
#[test]
fn project_from_path_reads_only_a_real_project_id_from_a_project_url() {
    let id = "6f95ee30-3fc8-4bc6-9d6d-f8cb4c8ef988";
    assert_eq!(
        crate::auth::project_from_path(&format!("/api/v1/projects/{id}/test-cases")).as_deref(),
        Some(id)
    );
    assert_eq!(
        crate::auth::project_from_path(&format!("/api/v1/projects/{id}")).as_deref(),
        Some(id)
    );
    // an upper-case id is canonicalised to the form projects are stored under
    assert_eq!(
        crate::auth::project_from_path(&format!("/api/v1/projects/{}/audit", id.to_uppercase()))
            .as_deref(),
        Some(id)
    );
    for not_a_project in [
        "/api/v1/projects",
        "/api/v1/projects/",
        "/api/v1/projects/not-a-uuid/audit",
        "/api/v1/logs",
        "/health",
        "/admin/api-keys",
    ] {
        assert_eq!(
            crate::auth::project_from_path(not_a_project),
            None,
            "path={not_a_project}"
        );
    }
}

/// A call made in process (the autopilot's) has no `require_auth` to say which project its URL names, so `with_request_path` does
/// it: inside, `request_project` gives the project, and outside it gives none.
#[tokio::test]
async fn with_request_path_makes_the_urls_project_known_only_inside_the_call() {
    let id = "6f95ee30-3fc8-4bc6-9d6d-f8cb4c8ef988";
    assert_eq!(crate::auth::request_project(), None);
    let inside = crate::auth::with_request_path(&format!("/api/v1/projects/{id}/gap-check"), async {
        crate::auth::request_project()
    })
    .await;
    assert_eq!(inside.as_deref(), Some(id));
    let elsewhere = crate::auth::with_request_path("/api/v1/logs", async { crate::auth::request_project() }).await;
    assert_eq!(elsewhere, None, "a URL that names no project names none");
    assert_eq!(crate::auth::request_project(), None, "and nothing leaks out of the call");
}

/// Category: real pagination — `offset` genuinely moves the window rather
/// than being silently ignored, `total` is the real full count (not this
/// response's own batch length), and paging all the way through with a
/// small `limit` reconstructs exactly the same real rows, in the same real
/// order, as one unpaged fetch.
#[tokio::test]
async fn be15_audit_log_offset_pages_through_real_rows_and_total_is_the_real_full_count() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Audit Pagination").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // 5 real, separately-audited actions.
    for i in 0..5 {
        send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/executions"),
                Some(&owner),
                Some(simple_plan(&format!("audit-page-{i}"))),
            ),
        )
        .await;
    }

    async fn fetch_audit_page(
        state: &AppState,
        project: Uuid,
        owner: &str,
        limit: i64,
        offset: i64,
    ) -> Value {
        let (status, body) = send(
            state,
            req(
                "GET",
                &format!("/api/v1/projects/{project}/audit?limit={limit}&offset={offset}"),
                Some(owner),
                None,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body={body}");
        body
    }

    let unpaged = fetch_audit_page(&state, project, &owner, 100, 0).await;
    let all_rows = unpaged["rows"].as_array().unwrap().clone();
    assert_eq!(unpaged["total"].as_i64().unwrap(), all_rows.len() as i64);
    assert!(all_rows.len() >= 5, "rows={all_rows:?}");

    let page1 = fetch_audit_page(&state, project, &owner, 2, 0).await;
    let page2 = fetch_audit_page(&state, project, &owner, 2, 2).await;
    let page3 = fetch_audit_page(&state, project, &owner, 2, 4).await;

    let real_total = unpaged["total"].as_i64().unwrap();
    for page in [&page1, &page2, &page3] {
        assert_eq!(
            page["total"].as_i64().unwrap(),
            real_total,
            "the real total must be the same regardless of which page is being viewed; page={page:?}"
        );
    }
    assert_eq!(page1["rows"].as_array().unwrap().len(), 2);
    assert_eq!(page2["rows"].as_array().unwrap().len(), 2);
    // The real remainder — never a short/long guess from batch length.
    assert_eq!(
        page3["rows"].as_array().unwrap().len() as i64,
        real_total - 4
    );

    // Concatenating every page reconstructs exactly the same real rows, in
    // the same real order, as one unpaged fetch — offset genuinely moves
    // the window rather than being silently ignored or double-fetching.
    let mut paged_rows = page1["rows"].as_array().unwrap().clone();
    paged_rows.extend(page2["rows"].as_array().unwrap().clone());
    paged_rows.extend(page3["rows"].as_array().unwrap().clone());
    assert_eq!(
        paged_rows, all_rows,
        "paged reconstruction must match the real unpaged order exactly"
    );
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

fn crawl_body_logging_in_with(username: &str, password: &str) -> Value {
    serde_json::json!({
        "base_url": "http://127.0.0.1:8080/app",
        "roles": [{"name": "admin", "login_actions": [
            {"type": "NAVIGATE", "url": "http://127.0.0.1:8080/login"},
            {"type": "FILL", "selector": "#username", "value": username},
            {"type": "FILL", "selector": "#password", "value": password},
            {"type": "CLICK", "selector": "#submit-login"}
        ]}]
    })
}

/// A login written as `env:NAME` references is stored as those names: the run's persisted plan (which is what a
/// restart rebuilds it from) holds the variable names in `params.secrets` and the references in nothing but the
/// generated spec — never the values themselves.
#[tokio::test]
async fn phase5a_crawl_start_stores_only_the_names_of_login_secrets_never_their_values() {
    std::env::set_var("AIQA_LOGIN_T5A_API_USER", "api-user-value-1");
    std::env::set_var("AIQA_LOGIN_T5A_API_PW", "api-pw-value-2");
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase5A Crawl Secrets").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, started) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(crawl_body_logging_in_with(
                "env:AIQA_LOGIN_T5A_API_USER",
                "env:AIQA_LOGIN_T5A_API_PW",
            )),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={started}");
    assert!(
        !started.to_string().contains("api-pw-value-2"),
        "started={started}"
    );
    let run_id: Uuid = started["run_id"].as_str().expect("run_id").parse().unwrap();

    let (plan, ..) = state
        .store
        .get_run_for_rehydration(run_id)
        .await
        .expect("read the run")
        .expect("the run is persisted");
    let stored = serde_json::to_string(&plan).unwrap();
    assert!(
        !stored.contains("api-pw-value-2") && !stored.contains("api-user-value-1"),
        "the persisted plan must hold no secret value: {stored}"
    );
    assert_eq!(
        plan.tasks[0].params["secrets"],
        serde_json::json!(["AIQA_LOGIN_T5A_API_PW", "AIQA_LOGIN_T5A_API_USER"]),
        "stored={stored}"
    );
}

#[tokio::test]
async fn phase5a_crawl_start_refuses_a_login_whose_secret_is_not_set_and_names_only_the_variable() {
    std::env::remove_var("AIQA_LOGIN_T5A_API_MISSING");
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase5A Crawl Missing Secret").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(crawl_body_logging_in_with(
                "admin",
                "env:AIQA_LOGIN_T5A_API_MISSING",
            )),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(
        body.to_string()
            .contains("AIQA_LOGIN_T5A_API_MISSING is not set"),
        "body={body}"
    );
}

#[tokio::test]
async fn phase5a_crawl_start_refuses_a_reference_to_a_variable_outside_the_reserved_names() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Phase5A Crawl Reserved Names").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(&owner),
            Some(crawl_body_logging_in_with("admin", "env:GROQ_API_KEY")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(body.to_string().contains("secret reference"), "body={body}");
    assert!(
        !body.to_string().contains("GROQ_API_KEY"),
        "the message must not repeat what was written: {body}"
    );
}

// ------------------------------------------------ Phase 5B — grounded compile
//
// A crawl cannot really run in this harness (see above), so each test writes the grounding file a finished crawl
// would have written for its one task (`helpers/crawler.ts` writes it in real life; that side is proven by
// `executors/playwright/tests/crawler.spec.ts` against a real browser and a real fixture app).

/// The crawl of a small app: `admin` signs in with two secrets (`AIQA_LOGIN_ADMIN_*`, renamed per test so tests that
/// set and unset variables never meet) and reaches a dashboard and Settings.
const GROUNDING_TEMPLATE: &str = r##"{"roles":[
  {"role":"admin","loginSucceeded":true,"loginUsesLiterals":false,
   "loginActions":[
     {"type":"NAVIGATE","url":"http://127.0.0.1:8080/login"},
     {"type":"FILL","selector":"#username","value":"env:AIQA_LOGIN_ADMIN_USERNAME"},
     {"type":"FILL","selector":"#password","value":"env:AIQA_LOGIN_ADMIN_PASSWORD"},
     {"type":"CLICK","selector":"#submit-login"}],
   "pages":[
     {"url":"http://127.0.0.1:8080/dashboard","title":"Dashboard","elements":[
       {"kind":"link","text":"Settings","locator":"a[href=\"/settings\"]"}]},
     {"url":"http://127.0.0.1:8080/settings","title":"Settings","elements":[
       {"kind":"button","text":"Delete Account","locator":"#danger-zone"}]}]},
  {"role":"anonymous","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[{"url":"http://127.0.0.1:8080/login","title":"Log in","elements":[
       {"kind":"button","text":"Log in","locator":"#submit-login"}]}]}]}"##;

fn grounding_json(secret_prefix: &str) -> String {
    GROUNDING_TEMPLATE.replace("AIQA_LOGIN_ADMIN", secret_prefix)
}

const SETTINGS_PLAN_JSON: &str = r##"{"executable":true,"execution_type":"BROWSER","target":"http://127.0.0.1:8080/dashboard","actions":[
  {"type":"NAVIGATE","url":"http://127.0.0.1:8080/dashboard"},
  {"type":"CLICK","selector":"a[href=\"/settings\"]"},
  {"type":"ASSERT_URL","url_contains":"/settings"},
  {"type":"ASSERT_VISIBLE","selector":"#danger-zone"}]}"##;

const INVENTED_PLAN_JSON: &str = r##"{"executable":true,"execution_type":"BROWSER","target":"http://127.0.0.1:8080/dashboard","actions":[
  {"type":"NAVIGATE","url":"http://127.0.0.1:8080/dashboard"},
  {"type":"CLICK","selector":"#settings-link"},
  {"type":"ASSERT_VISIBLE","selector":"#danger-zone"}]}"##;

/// A case with several steps, so the AI (not the deterministic compiler) compiles it.
fn open_settings_case_body(title: &str) -> Value {
    serde_json::json!({
        "title": title,
        "description": "The admin opens Settings from the dashboard.",
        "category": "Functional",
        "priority": "HIGH",
        "severity": "MAJOR",
        "preconditions": ["An admin account exists"],
        "steps": [
            {"step_number": 1, "action": "Open the dashboard", "expected_result": "The dashboard is shown"},
            {"step_number": 2, "action": "Click the Settings link", "expected_result": "The Settings page is shown"}
        ],
        "expected_result": "Settings page is shown with its danger zone",
        "assertions": [],
        "requirement_reference": "FRD-SET-01",
        "tags": []
    })
}

/// Starts a crawl (which cannot really run here) and, when `grounding` is given, writes what a finished crawl would have
/// recorded for its one task. Returns the crawl's run id and the file to remove afterwards.
async fn crawl_with_grounding(
    state: &AppState,
    project: Uuid,
    key: &str,
    grounding: Option<&str>,
) -> (String, Option<std::path::PathBuf>) {
    let (status, started) = send(
        state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(key),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={started}");
    let run_id = started["run_id"].as_str().expect("run_id").to_string();
    let (_, run) = send(
        state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}"),
            Some(key),
            None,
        ),
    )
    .await;
    let task_id = run["tasks"][0]["id"].as_str().expect("task id").to_string();
    let file = grounding.map(|json| {
        let dir = crate::crawler::generated_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(crate::crawler::grounding_file_name(&task_id));
        std::fs::write(&path, json).unwrap();
        path
    });
    (run_id, file)
}

/// Creates a case, validates and approves it, and returns its id.
async fn approved_case(state: &AppState, project: Uuid, key: &str, body: Value) -> String {
    let (status, created) = send(
        state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(key),
            Some(body),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let id = created["id"].as_str().unwrap().to_string();
    for step in ["validate", "approve"] {
        let (status, body) = send(
            state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/test-cases/{id}/{step}"),
                Some(key),
                None,
            ),
        )
        .await;
        assert!(status.is_success(), "{step}: {status} {body}");
    }
    id
}

#[tokio::test]
async fn phase5b_a_compile_grounded_in_a_crawl_has_the_login_first_and_reports_only_names() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(SETTINGS_PLAN_JSON))).await;
    let project = make_project(&state, "Phase5B Grounded Compile").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (run_id, file) =
        crawl_with_grounding(&state, project, &owner, Some(&grounding_json("AIQA_LOGIN_T5B_ONE"))).await;
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(open_settings_case_body("Admin opens Settings")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/compile"),
            Some(&owner),
            Some(serde_json::json!({"crawl_run_id": run_id, "role": "admin"})),
        ),
    )
    .await;
    if let Some(file) = file {
        std::fs::remove_file(file).ok();
    }
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["status"], "READY", "body={body}");
    assert_eq!(body["actions"].as_array().unwrap().len(), 8, "the login (4) and the plan (4): {body}");
    assert_eq!(body["actions"][0]["type"], "NAVIGATE");
    assert_eq!(body["actions"][2]["value"], "env:AIQA_LOGIN_T5B_ONE_PASSWORD", "the login holds a reference: {body}");
    // What the compiler was given: the role, the size of what it saw, and the names — never values — of the secrets.
    assert_eq!(body["grounding"]["role"], "admin", "body={body}");
    assert_eq!(body["grounding"]["login_steps"], 4);
    assert_eq!(
        body["grounding"]["secrets"],
        serde_json::json!(["AIQA_LOGIN_T5B_ONE_PASSWORD", "AIQA_LOGIN_T5B_ONE_USERNAME"])
    );
}

#[tokio::test]
async fn phase5b_a_selector_the_crawl_never_found_is_not_executable_through_the_api() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(INVENTED_PLAN_JSON))).await;
    let project = make_project(&state, "Phase5B Ungrounded").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (run_id, file) =
        crawl_with_grounding(&state, project, &owner, Some(&grounding_json("AIQA_LOGIN_T5B_TWO"))).await;
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(open_settings_case_body("Admin opens Settings")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/compile"),
            Some(&owner),
            Some(serde_json::json!({"crawl_run_id": run_id, "role": "admin"})),
        ),
    )
    .await;
    if let Some(file) = file {
        std::fs::remove_file(file).ok();
    }
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["status"], "NOT_EXECUTABLE", "body={body}");
    let reasons = body["reasons"].to_string();
    assert!(reasons.contains("#settings-link") && reasons.contains("step 2 (CLICK)"), "reasons={reasons}");
    assert_eq!(body["grounding"]["role"], "admin");
}

#[tokio::test]
async fn phase5b_a_compile_with_no_crawl_is_exactly_what_it_was() {
    // The same invented selector compiles when no crawl is named — the old behaviour, unchanged — and the response has
    // no grounding key. Both an empty body and no body at all mean "no crawl".
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(INVENTED_PLAN_JSON))).await;
    let project = make_project(&state, "Phase5B No Crawl").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(open_settings_case_body("Admin opens Settings")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    for body in [None, Some(serde_json::json!({}))] {
        let (status, response) = send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/test-cases/{id}/compile"),
                Some(&owner),
                body,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "response={response}");
        assert_eq!(response["status"], "READY", "response={response}");
        assert!(response.get("grounding").is_none(), "response={response}");
        assert_eq!(response["actions"].as_array().unwrap().len(), 3, "no login was put in front: {response}");
    }
}

#[tokio::test]
async fn phase5b_the_compile_says_what_is_wrong_with_the_crawl_or_the_role() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(SETTINGS_PLAN_JSON))).await;
    let project = make_project(&state, "Phase5B Bad Crawl").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (run_with, file) =
        crawl_with_grounding(&state, project, &owner, Some(&grounding_json("AIQA_LOGIN_T5B_THREE"))).await;
    let (run_without, _) = crawl_with_grounding(&state, project, &owner, None).await;
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(open_settings_case_body("Admin opens Settings")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    let compile = |body: Value| {
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/compile"),
            Some(&owner),
            Some(body),
        )
    };

    // A crawl that recorded no grounding (still running, or made before grounding existed).
    let (status, body) = send(&state, compile(serde_json::json!({"crawl_run_id": run_without}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert!(body.to_string().contains("recorded no grounding"), "body={body}");
    // A role the crawl does not have: the ones it has are named.
    let (status, body) = send(
        &state,
        compile(serde_json::json!({"crawl_run_id": run_with, "role": "auditor"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(body.to_string().contains("admin, anonymous"), "body={body}");
    // A role without a crawl to take it from.
    let (status, body) = send(&state, compile(serde_json::json!({"role": "admin"}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    // A crawl run that does not exist.
    let (status, _) = send(
        &state,
        compile(serde_json::json!({"crawl_run_id": Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    if let Some(file) = file {
        std::fs::remove_file(file).ok();
    }
}

#[tokio::test]
async fn phase5b_another_projects_crawl_cannot_ground_a_compile() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(SETTINGS_PLAN_JSON))).await;
    let project_a = make_project(&state, "Phase5B Crawl Owner").await;
    let project_b = make_project(&state, "Phase5B Crawl Borrower").await;
    let key_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let key_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let (run_id, file) =
        crawl_with_grounding(&state, project_a, &key_a, Some(&grounding_json("AIQA_LOGIN_T5B_FOUR"))).await;
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/test-cases"),
            Some(&key_b),
            Some(open_settings_case_body("Admin opens Settings")),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_b}/test-cases/{id}/compile"),
            Some(&key_b),
            Some(serde_json::json!({"crawl_run_id": run_id, "role": "admin"})),
        ),
    )
    .await;
    if let Some(file) = file {
        std::fs::remove_file(file).ok();
    }
    assert_eq!(status, StatusCode::NOT_FOUND, "another project's crawl is not reachable: {body}");
}

#[tokio::test]
async fn phase5b_a_handoff_whose_login_secrets_are_not_set_is_refused_before_a_run_exists() {
    std::env::remove_var("AIQA_LOGIN_T5B_FIVE_USERNAME");
    std::env::remove_var("AIQA_LOGIN_T5B_FIVE_PASSWORD");
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(SETTINGS_PLAN_JSON))).await;
    let project = make_project(&state, "Phase5B Handoff Missing Secret").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (run_id, file) =
        crawl_with_grounding(&state, project, &owner, Some(&grounding_json("AIQA_LOGIN_T5B_FIVE"))).await;
    let id = approved_case(&state, project, &owner, open_settings_case_body("Admin opens Settings")).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({"compile_and_execute": true, "crawl_run_id": run_id, "role": "admin"})),
        ),
    )
    .await;
    if let Some(file) = file {
        std::fs::remove_file(file).ok();
    }
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(
        body.to_string().contains("AIQA_LOGIN_T5B_FIVE_PASSWORD") && body.to_string().contains("not set"),
        "body={body}"
    );
}

/// The core Phase 5 guarantee, at the API: a grounded handoff creates a real run whose stored task holds the *names* of
/// the login's secrets and a spec that reads them when it runs — and no value is anywhere it can be read back from.
#[tokio::test]
async fn phase5b_a_grounded_handoff_stores_a_spec_and_a_task_that_hold_no_credential() {
    std::env::set_var("AIQA_LOGIN_T5B_SIX_USERNAME", "six-user-value-1");
    std::env::set_var("AIQA_LOGIN_T5B_SIX_PASSWORD", "six-pass-value-2");
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(SETTINGS_PLAN_JSON))).await;
    let project = make_project(&state, "Phase5B Handoff Stores No Credential").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (crawl_run, file) =
        crawl_with_grounding(&state, project, &owner, Some(&grounding_json("AIQA_LOGIN_T5B_SIX"))).await;
    let id = approved_case(&state, project, &owner, open_settings_case_body("Admin opens Settings")).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{id}/handoff"),
            Some(&owner),
            Some(serde_json::json!({"compile_and_execute": true, "crawl_run_id": crawl_run, "role": "admin"})),
        ),
    )
    .await;
    if let Some(file) = file {
        std::fs::remove_file(file).ok();
    }
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    assert!(!body.to_string().contains("six-pass-value-2"), "body={body}");
    let run_id: Uuid = body["run_id"].as_str().expect("run_id").parse().unwrap();

    let (plan, ..) = state
        .store
        .get_run_for_rehydration(run_id)
        .await
        .expect("read the run")
        .expect("the run is persisted");
    let stored = serde_json::to_string(&plan).unwrap();
    assert!(
        !stored.contains("six-pass-value-2") && !stored.contains("six-user-value-1"),
        "the persisted run must hold no secret value: {stored}"
    );
    let task = &plan.tasks[0];
    assert_eq!(task.executor.as_deref(), Some("playwright"));
    assert_eq!(
        task.params["secrets"],
        serde_json::json!(["AIQA_LOGIN_T5B_SIX_PASSWORD", "AIQA_LOGIN_T5B_SIX_USERNAME"])
    );

    // The spec the run will execute: the login as it was recorded, secrets read at run time, no value.
    let spec = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../executors/playwright")
            .join(task.params["spec"].as_str().expect("a spec path")),
    )
    .expect("read the generated spec");
    assert!(spec.contains("secret(\"AIQA_LOGIN_T5B_SIX_PASSWORD\")"), "{spec}");
    assert!(spec.contains("import { secret } from \"../../helpers/secrets\";"), "{spec}");
    assert!(!spec.contains("six-pass-value-2") && !spec.contains("six-user-value-1"), "{spec}");
    assert!(spec.contains("AIQA_STEP"), "each step is logged as the run goes: {spec}");
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

// ============================================ Vector Store + RAG Retrieval v1

/// `RecordingEmbedder` both controls what `embed` returns (a fixed,
/// known vector) and proves the real call actually happened with this
/// case's own real content, not a fabricated/simulated one.
#[tokio::test]
async fn approving_a_test_case_computes_and_stores_a_real_embedding_when_configured() {
    let embedder = Arc::new(RecordingEmbedder::new(vec![0.1, 0.2, 0.3]));
    let (state, _dir) =
        test_state_with_ai_router_and_embedder(inert_ai_router(), embedder.clone()).await;
    let project = make_project(&state, "Vector Store Approve").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Embed me")),
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

    assert_eq!(
        embedder.call_count(),
        1,
        "approval must trigger exactly one real embed call"
    );
    let embedded_text = embedder.texts.lock().unwrap()[0].clone();
    assert!(
        embedded_text.contains("Embed me"),
        "the real embedded text must include this case's own real title: {embedded_text}"
    );
    assert_eq!(
        state
            .store
            .count_test_case_embeddings(&project.to_string())
            .await
            .unwrap(),
        1,
        "a real embedding row must now exist for this project"
    );
}

/// Same "fails open" contract `find_similar_approved_test_cases`'s own doc
/// comment states — approval must never fail just because no embedder is
/// configured (the honest default `test_state()` already gives every
/// pre-existing test in this file).
#[tokio::test]
async fn approving_a_test_case_succeeds_even_when_no_embedder_is_configured() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "No Embedder Configured").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("No embed")),
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
    assert_eq!(
        state
            .store
            .count_test_case_embeddings(&project.to_string())
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn editing_an_approved_test_case_recomputes_its_embedding() {
    let embedder = Arc::new(RecordingEmbedder::new(vec![0.4, 0.5, 0.6]));
    let (state, _dir) =
        test_state_with_ai_router_and_embedder(inert_ai_router(), embedder.clone()).await;
    let project = make_project(&state, "Vector Store Edit Approved").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Approved then edited")),
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
    assert_eq!(
        embedder.call_count(),
        1,
        "approval itself must have embedded once"
    );

    let (status, updated) = send(
        &state,
        req(
            "PATCH",
            &format!("/api/v1/projects/{project}/test-cases/{id}"),
            Some(&owner),
            Some(serde_json::json!({"title": "Edited while approved"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={updated}");
    assert_eq!(
        updated["status"], "APPROVED",
        "editing must not itself change status"
    );

    assert_eq!(
        embedder.call_count(),
        2,
        "editing an already-APPROVED case must recompute its embedding"
    );
    let second_text = embedder.texts.lock().unwrap()[1].clone();
    assert!(
        second_text.contains("Edited while approved"),
        "the recomputed embedding must reflect the real edited content: {second_text}"
    );
    assert_eq!(
        state
            .store
            .count_test_case_embeddings(&project.to_string())
            .await
            .unwrap(),
        1,
        "must still be exactly one row, not a second stale one"
    );
}

#[tokio::test]
async fn editing_a_not_yet_approved_test_case_never_attempts_to_embed() {
    let embedder = Arc::new(RecordingEmbedder::new(vec![0.7, 0.8, 0.9]));
    let (state, _dir) =
        test_state_with_ai_router_and_embedder(inert_ai_router(), embedder.clone()).await;
    let project = make_project(&state, "Vector Store Edit Unapproved").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Never approved")),
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
            Some(serde_json::json!({"title": "Still just generated"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={updated}");
    assert_eq!(updated["status"], "GENERATED");

    assert_eq!(
        embedder.call_count(),
        0,
        "a case that was never approved must never trigger a real embed call"
    );
    assert_eq!(
        state
            .store
            .count_test_case_embeddings(&project.to_string())
            .await
            .unwrap(),
        0
    );
}

/// The other half of the real end-to-end loop (the approve-time half is
/// proven in isolation by `approving_a_test_case_computes_and_stores_a_
/// real_embedding_when_configured` above): a LATER real generation request
/// genuinely retrieves a project's own previously-approved case and
/// reports it honestly via `similar_cases_used` — a real, live-computed
/// number, never a static placeholder.
#[tokio::test]
async fn generate_ai_retrieves_and_honestly_reports_a_real_similar_approved_case() {
    // A fixed vector for BOTH the approved case's own embedding AND this
    // generation request's own query embedding — guarantees a real,
    // deterministic cosine similarity of 1.0 between them without this
    // test needing to reproduce real semantic embedding behavior.
    let embedder = Arc::new(FakeEmbedder::configured(vec![1.0, 0.0, 0.0]));
    let (state, _dir) =
        test_state_with_ai_router_and_embedder(ai_infra_with(Ok(valid_ai_response())), embedder)
            .await;
    let project = make_project(&state, "RAG Retrieval").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // No case approved yet — the real, honest zero.
    let (status, first) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can log in.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={first}");
    assert_eq!(first["similar_cases_used"], 0, "body={first}");

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Reject invalid password")),
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

    let (status, second) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can log in.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={second}");
    assert_eq!(
        second["similar_cases_used"], 1,
        "the same project's own real, newly-approved case must now be retrieved: body={second}"
    );
}

/// Phase 2 gap-closing — document-level RAG. The other half of the real
/// end-to-end loop, mirroring `generate_ai_retrieves_and_honestly_reports_a_
/// real_similar_approved_case` above exactly, but for a document uploaded
/// through `POST .../documents/extract-text` instead of an approved test
/// case: a LATER, unrelated generation request must still be able to
/// retrieve that document's real content and report it honestly via
/// `similar_documents_used` — proving a document's content now survives
/// beyond the single request that uploaded it.
#[tokio::test]
async fn generate_ai_retrieves_and_honestly_reports_a_real_similar_document() {
    let embedder = Arc::new(FakeEmbedder::configured(vec![1.0, 0.0, 0.0]));
    let (state, _dir) =
        test_state_with_ai_router_and_embedder(ai_infra_with(Ok(valid_ai_response())), embedder)
            .await;
    let project = make_project(&state, "Document RAG Retrieval").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // No document uploaded yet — the real, honest zero.
    let (status, first) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can request a refund.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={first}");
    assert_eq!(first["similar_documents_used"], 0, "body={first}");

    let (status, extracted) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&owner),
            Some(serde_json::json!({
                "file_name": "Refunds-BRD.txt",
                "content_base64": b64(b"Refunds must be processed within 5 business days.")
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={extracted}");

    let (status, second) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can request a refund.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={second}");
    assert_eq!(
        second["similar_documents_used"], 1,
        "the same project's own real, newly-uploaded document must now be retrieved: body={second}"
    );
}

/// Re-uploading a document whose extracted text is byte-for-byte identical
/// (a real, common case — the same file re-selected in the UI, or the same
/// requirement text saved under a different file name) must not create a
/// second, duplicate row that retrieval then double-counts —
/// `store::upsert_document`'s own `ON CONFLICT(project_id, content_hash)`
/// is what this proves, end-to-end through the real HTTP endpoints rather
/// than at the SQL layer directly.
#[tokio::test]
async fn document_re_upload_with_identical_content_deduplicates_not_a_second_retrieved_document() {
    let embedder = Arc::new(FakeEmbedder::configured(vec![1.0, 0.0, 0.0]));
    let (state, _dir) =
        test_state_with_ai_router_and_embedder(ai_infra_with(Ok(valid_ai_response())), embedder)
            .await;
    let project = make_project(&state, "Document RAG Dedup").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let content = b"Refunds must be processed within 5 business days.";

    for file_name in ["Refunds-BRD.txt", "Refunds-BRD-copy.txt"] {
        let (status, body) = send(
            &state,
            req(
                "POST",
                &format!("/api/v1/projects/{project}/documents/extract-text"),
                Some(&owner),
                Some(serde_json::json!({"file_name": file_name, "content_base64": b64(content)})),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body={body}");
    }

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can request a refund.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["similar_documents_used"], 1,
        "identical content re-uploaded under a different file name must dedupe to one real document, not two: body={body}"
    );
}

#[tokio::test]
async fn pipeline_reports_replanning_vector_and_rag_as_genuinely_available_with_real_backing() {
    let embedder = Arc::new(FakeEmbedder::configured(vec![1.0, 0.0, 0.0]));
    let (state, _dir) = test_state_with_ai_router_and_embedder(inert_ai_router(), embedder).await;
    let project = make_project(&state, "Pipeline Stages").await;
    let owner = mint_key(&state, "viewer", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/pipeline"),
            Some(&owner),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let stages = body["stages"].as_array().unwrap();
    for stage_id in ["replanning", "vector", "rag"] {
        let stage = stages
            .iter()
            .find(|s| s["id"] == stage_id)
            .unwrap_or_else(|| panic!("stage {stage_id} must exist: {body}"));
        assert_eq!(
            stage["available"], true,
            "stage {stage_id} must now report available: {stage}"
        );
        assert!(
            !stage["backing"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("not implemented"),
            "stage {stage_id}'s backing text must no longer claim NOT IMPLEMENTED: {stage}"
        );
    }
    // Real, honest zero — nothing approved/embedded in this fresh project.
    let vector_stage = stages.iter().find(|s| s["id"] == "vector").unwrap();
    assert_eq!(vector_stage["metric"], 0, "body={body}");

    // Phase 2 gap-closing — both stages' backing text must honestly
    // disclose that document-level grounding exists now too, not just
    // test-case grounding, otherwise this text would immediately go stale
    // the moment document-level RAG shipped (the same kind of gap Phase 0
    // fixed in `AIEnginePanel.tsx`).
    let rag_stage = stages.iter().find(|s| s["id"] == "rag").unwrap();
    for stage in [vector_stage, rag_stage] {
        assert!(
            stage["backing"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("document"),
            "stage {stage}'s backing text must disclose document-level grounding too"
        );
    }
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
            unconfigured_embedder(),
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
        .upsert_quality_node(
            &project.to_string(),
            "test",
            "Checkout Flow",
            "Checkout Flow",
        )
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

/// Phase 1 gap-closing — Custom Testing's own generation endpoint now gets
/// the same real Vector Store + RAG Retrieval grounding the main
/// `generate-ai` endpoint already has. Proven the same way `ai_generation
/// .rs`'s own prompt-content tests prove grounding: capture the real prompt
/// text `custom_testing::run` actually sent to the "cases" AI call (via
/// `RecordingExecutor`) and assert it contains the real approved case's
/// own title — not just a response-shape assertion, real prompt content.
#[tokio::test]
async fn phase1_custom_testing_generation_is_grounded_by_a_real_similar_approved_case() {
    // Fixed vector for both the approved case's own embedding and this
    // generation's query embedding — real, deterministic cosine similarity
    // of 1.0, same technique as the main generator's own RAG test.
    let embedder = Arc::new(FakeEmbedder::configured(vec![1.0, 0.0, 0.0]));
    let cases_executor = Arc::new(RecordingExecutor::ok_sequence(vec![
        valid_custom_testing_cases_response(),
    ]));
    let ai_infra = custom_testing_ai_infra_with_cases_executor(
        valid_custom_testing_analysis_response(),
        cases_executor.clone(),
    );
    let (state, _dir) = test_state_with_ai_router_and_embedder(ai_infra, embedder).await;
    let project = make_project(&state, "Custom Testing RAG").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // Approve one real case first — this project now has exactly one real
    // embedded case (same approve flow already proven end-to-end for the
    // main generator's own RAG test).
    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&owner),
            Some(sample_test_case_body("Reject invalid password")),
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

    let prompts = cases_executor.prompts.lock().unwrap();
    assert_eq!(
        prompts.len(),
        1,
        "the cases call must have run exactly once"
    );
    assert!(
        prompts[0].contains("Reject invalid password"),
        "the real prompt sent to the cases call must include this project's own real approved \
         case as a similar-case grounding example: {}",
        prompts[0]
    );
    assert!(
        prompts[0].to_lowercase().contains("already-approved"),
        "the real similar-cases prompt section must be present: {}",
        prompts[0]
    );
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

// ================================== Truthfulness Cleanup — real Product
// Documentation content extraction (Test Case Generator)

/// Builds a real, valid minimal .docx (a genuine zip archive with a real
/// `word/document.xml` body) — mirrors `doc_extract.rs`'s own unit-test
/// helper, exercised here through the real HTTP endpoint end-to-end.
fn build_test_docx_bytes(paragraphs: &[&str]) -> Vec<u8> {
    use std::io::Write;
    let body: String = paragraphs
        .iter()
        .map(|p| format!("<w:p><w:r><w:t>{p}</w:t></w:r></w:p>"))
        .collect();
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>{body}</w:body>
</w:document>"#
    );
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[tokio::test]
async fn extract_document_text_reads_the_real_content_of_a_real_docx() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Doc Extract - Real Docx").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let docx = build_test_docx_bytes(&[
        "Agents must self-register.",
        "Admin approval is required before login.",
    ]);

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&owner),
            Some(serde_json::json!({"file_name": "BRD.docx", "content_base64": b64(&docx)})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["text"], "Agents must self-register.\nAdmin approval is required before login.",
        "body={body}"
    );
    assert_eq!(body["truncated"], false, "body={body}");
    assert!(body["char_count"].as_u64().unwrap() > 0, "body={body}");
}

#[tokio::test]
async fn extract_document_text_reads_plain_text_formats_verbatim() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Doc Extract - Plain Text").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&owner),
            Some(serde_json::json!({"file_name": "notes.md", "content_base64": b64(b"# Real Spec\n\nDo the real thing.")})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["text"], "# Real Spec\n\nDo the real thing.",
        "body={body}"
    );
}

#[tokio::test]
async fn extract_document_text_reports_a_real_specific_error_for_legacy_doc_not_a_fake_success() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Doc Extract - Legacy Doc").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&owner),
            Some(serde_json::json!({"file_name": "RTM.doc", "content_base64": b64(b"anything")})),
        ),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a legacy .doc must never report a fake success; body={body}"
    );
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(".doc"),
        "expected the real error to name the .doc limitation; body={body}"
    );
}

#[tokio::test]
async fn extract_document_text_rejects_invalid_base64_with_a_real_validation_error() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Doc Extract - Invalid Base64").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&owner),
            Some(serde_json::json!({"file_name": "notes.txt", "content_base64": "not-valid-base64!!!"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
}

#[tokio::test]
async fn extract_document_text_rbac_viewer_forbidden_operator_allowed() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Doc Extract - RBAC").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;
    let payload = serde_json::json!({"file_name": "notes.txt", "content_base64": b64(b"hello")});

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&viewer),
            Some(payload.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&operator),
            Some(payload),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn extract_document_text_project_isolation_enforced() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project_a = make_project(&state, "Doc Extract - Isolation A").await;
    let project_b = make_project(&state, "Doc Extract - Isolation B").await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project_a}/documents/extract-text"),
            Some(&owner_b),
            Some(serde_json::json!({"file_name": "notes.txt", "content_base64": b64(b"hello")})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

// ============================================ Phase 3 — requirements read out of documents

/// A minimal but genuine one-page text PDF, same construction as
/// `doc_extract.rs`'s own test helper.
fn build_test_pdf_bytes(lines: &[&str]) -> Vec<u8> {
    let mut content = String::from("BT\n/F1 12 Tf\n14 TL\n72 720 Td\n");
    for l in lines {
        content.push_str(&format!("({l}) Tj T*\n"));
    }
    content.push_str("ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{obj}\nendobj\n", i + 1).as_bytes());
    }
    let xref_pos = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// A .docx whose paragraphs may carry a Word style (`Heading1`, ...).
fn build_styled_docx_bytes(paragraphs: &[(Option<&str>, &str)]) -> Vec<u8> {
    use std::io::Write;
    let body: String = paragraphs
        .iter()
        .map(|(style, text)| match style {
            Some(st) => format!(
                r#"<w:p><w:pPr><w:pStyle w:val="{st}"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
            ),
            None => format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"),
        })
        .collect();
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buf
}

async fn upload_document(
    state: &AppState,
    key: &str,
    project: Uuid,
    file_name: &str,
    bytes: &[u8],
    document_type: Option<&str>,
) -> (StatusCode, Value) {
    let mut body = serde_json::json!({"file_name": file_name, "content_base64": b64(bytes)});
    if let Some(t) = document_type {
        body["document_type"] = Value::String(t.to_string());
    }
    send(
        state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(key),
            Some(body),
        ),
    )
    .await
}

const SAMPLE_FRD: &str = "# Login\n\n## 3.2 Login\n\nFR-012: The system shall lock an account after 5 failed login attempts.\n\nFR-013: A locked account shall be unlocked automatically after 15 minutes.\n\n## 3.3 Reports\n\nThe system shall export any report as a CSV file.\n";

#[tokio::test]
async fn an_uploaded_document_is_also_read_for_its_requirements_and_the_old_response_is_unchanged() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Upload").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = upload_document(&state, &owner, project, "FRD.md", SAMPLE_FRD.as_bytes(), Some("FRD")).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    // What this endpoint has always answered.
    assert_eq!(body["text"], SAMPLE_FRD.trim());
    assert_eq!(body["truncated"], false);
    assert_eq!(body["char_count"], SAMPLE_FRD.trim().chars().count());
    // What it now also says.
    let summary = &body["requirements_document"];
    assert_eq!(summary["requirements_count"], 3, "body={body}");
    assert_eq!(summary["sections_count"], 3, "body={body}");
    assert_eq!(summary["full_truncated"], false);
    assert_eq!(summary["full_char_count"], SAMPLE_FRD.trim().chars().count());
    assert!(summary["document_id"].as_str().is_some(), "body={body}");
    assert_eq!(summary["by_kind"]["security"], 2, "body={body}");

    let (status, listed) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/requirements"), Some(&owner), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={listed}");
    assert_eq!(listed["documents"].as_array().unwrap().len(), 1);
    assert_eq!(listed["documents"][0]["file_name"], "FRD.md");
    assert_eq!(listed["documents"][0]["doc_type"], "FRD");
    assert_eq!(listed["documents"][0]["requirements_count"], 3);
    let reqs = listed["requirements"].as_array().unwrap();
    let keys: Vec<&str> = reqs.iter().map(|r| r["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["FR-012", "FR-013", "FRD-3.3-01"]);
    assert_eq!(reqs[0]["section"], "3.2 Login");
    assert_eq!(reqs[0]["kind"], "security");
    assert_eq!(reqs[2]["text"], "The system shall export any report as a CSV file.");
    assert_eq!(reqs[0]["document_id"], summary["document_id"]);
}

#[tokio::test]
async fn nothing_past_the_old_20000_character_mark_is_lost() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Long").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // ~60,000 characters of filler paragraphs, then a requirement at the very end.
    let mut doc = String::from("## 1 Filler\n\n");
    for i in 0..600 {
        doc.push_str(&format!(
            "The system shall support filler scenario number {i} in the portal today.\n\n"
        ));
    }
    doc.push_str("## 2 The end\n\nFR-9999: The system shall still read this last requirement.\n");
    assert!(doc.chars().count() > 40_000);

    let (status, body) = upload_document(&state, &owner, project, "long.md", doc.as_bytes(), Some("FRD")).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    // The bounded text the single-prompt path uses is still cut, and says so.
    assert_eq!(body["truncated"], true);
    assert_eq!(body["char_count"], 20_000);
    // The whole document was read all the same.
    assert_eq!(body["requirements_document"]["full_char_count"], doc.trim().chars().count());
    assert_eq!(body["requirements_document"]["requirements_count"], 601, "body={body}");
    let (_, listed) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/requirements"), Some(&owner), None),
    )
    .await;
    let last = listed["requirements"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["key"], "FR-9999");
    assert_eq!(last["text"], "The system shall still read this last requirement.");
}

#[tokio::test]
async fn a_docx_keeps_its_headings_so_requirements_know_their_section() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Docx").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let docx = build_styled_docx_bytes(&[
        (Some("Heading1"), "3. Authentication"),
        (Some("Heading2"), "3.2 Login"),
        (None, "Users shall sign in with an email address and a password."),
        (None, "A session shall expire after 30 minutes of inactivity."),
    ]);

    let (status, body) = upload_document(&state, &owner, project, "BRD.docx", &docx, Some("BRD")).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["requirements_document"]["sections_count"], 2, "body={body}");
    let (_, listed) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/requirements"), Some(&owner), None),
    )
    .await;
    let reqs = listed["requirements"].as_array().unwrap();
    assert_eq!(reqs.len(), 2, "{listed}");
    assert_eq!(reqs[0]["section"], "3.2 Login");
    assert_eq!(reqs[0]["key"], "BRD-3.2-01");
    assert_eq!(reqs[1]["key"], "BRD-3.2-02");
    // The old, bounded reading of a docx is exactly what it was: bare lines.
    assert_eq!(
        body["text"],
        "3. Authentication\n3.2 Login\nUsers shall sign in with an email address and a password.\nA session shall expire after 30 minutes of inactivity."
    );
}

#[tokio::test]
async fn a_text_pdf_is_read_and_split_into_requirements() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Pdf").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let pdf = build_test_pdf_bytes(&[
        "FR-1: The system shall lock an account after 5 failed attempts.",
        "FR-2: The system shall email a reset link.",
    ]);

    let (status, body) = upload_document(&state, &owner, project, "spec.pdf", &pdf, Some("PRD")).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["text"].as_str().unwrap().contains("lock an account"), "body={body}");
    assert_eq!(body["requirements_document"]["requirements_count"], 2, "body={body}");
}

#[tokio::test]
async fn the_document_type_prefixes_generated_ids_and_the_file_name_does_when_it_is_absent() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Prefix").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let text = b"The system shall show a dashboard.\n";

    upload_document(&state, &owner, project, "a.md", text, Some("SRD")).await;
    upload_document(&state, &owner, project, "checkout-flow.md", b"The system shall take a payment.\n", None).await;

    let (_, listed) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/requirements"), Some(&owner), None),
    )
    .await;
    let keys: Vec<&str> = listed["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["SRD-S0-01", "CHECKO-S0-01"]);
}

#[tokio::test]
async fn uploading_the_same_content_again_reuses_the_document_and_a_different_version_is_a_new_one() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Reupload").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, first) = upload_document(&state, &owner, project, "FRD.md", SAMPLE_FRD.as_bytes(), Some("FRD")).await;
    let (_, again) = upload_document(&state, &owner, project, "FRD-final.md", SAMPLE_FRD.as_bytes(), Some("FRD")).await;
    assert_eq!(
        first["requirements_document"]["document_id"], again["requirements_document"]["document_id"],
        "same content, same document"
    );

    let changed = format!("{SAMPLE_FRD}\n## 3.4 Audit\n\nThe system shall record every login.\n");
    let (_, third) = upload_document(&state, &owner, project, "FRD-v2.md", changed.as_bytes(), Some("FRD")).await;
    assert_ne!(first["requirements_document"]["document_id"], third["requirements_document"]["document_id"]);
    assert_eq!(third["requirements_document"]["requirements_count"], 4);

    let (_, listed) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/requirements"), Some(&owner), None),
    )
    .await;
    let docs = listed["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0]["file_name"], "FRD-final.md", "the name is the latest upload's");
    assert_eq!(docs[0]["requirements_count"], 3, "not doubled by the second upload");
    assert_eq!(listed["requirements"].as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn two_versions_that_differ_only_after_the_first_20000_characters_are_two_documents() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Versions").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let filler = "The system shall support filler scenario in the portal today.\n\n".repeat(400);
    let v1 = format!("{filler}FR-1: The old last requirement.\n");
    let v2 = format!("{filler}FR-1: The new last requirement, edited later.\n");

    let (_, a) = upload_document(&state, &owner, project, "big.md", v1.as_bytes(), None).await;
    let (_, b) = upload_document(&state, &owner, project, "big.md", v2.as_bytes(), None).await;

    assert_ne!(a["requirements_document"]["document_id"], b["requirements_document"]["document_id"]);
    let (_, listed) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/requirements?document_ids={}", b["requirements_document"]["document_id"].as_str().unwrap()),
            Some(&owner),
            None,
        ),
    )
    .await;
    let last = listed["requirements"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["text"], "The new last requirement, edited later.");
    assert_eq!(listed["documents"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_file_that_cannot_be_read_stores_no_requirements() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Unreadable").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, _) = upload_document(&state, &owner, project, "RTM.doc", b"anything", Some("FRD")).await;
    assert_ne!(status, StatusCode::OK);
    let (status, _) = upload_document(&state, &owner, project, "broken.pdf", b"%PDF-1.4 not really", Some("FRD")).await;
    assert_ne!(status, StatusCode::OK);

    let (_, listed) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/requirements"), Some(&owner), None),
    )
    .await;
    assert!(listed["documents"].as_array().unwrap().is_empty());
    assert!(listed["requirements"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_failure_to_store_the_requirements_never_fails_the_upload() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Fail Open").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    sqlx::query("DROP TABLE requirements").execute(&state.store.pool).await.unwrap();

    let (status, body) = upload_document(&state, &owner, project, "FRD.md", SAMPLE_FRD.as_bytes(), Some("FRD")).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["text"], SAMPLE_FRD.trim());
    assert!(body.get("requirements_document").is_none(), "nothing was stored, so nothing is claimed: {body}");
}

#[tokio::test]
async fn listing_requirements_is_project_scoped_readable_by_a_viewer_and_validates_its_filter() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project_a = make_project(&state, "Requirements - A").await;
    let project_b = make_project(&state, "Requirements - B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let viewer_a = mint_key(&state, "viewer", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    upload_document(&state, &owner_a, project_a, "FRD.md", SAMPLE_FRD.as_bytes(), Some("FRD")).await;
    upload_document(&state, &owner_b, project_b, "Other.md", b"The system shall do the other project's thing.\n", Some("FRD")).await;

    // A viewer may read.
    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/requirements"), Some(&viewer_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["requirements"].as_array().unwrap().len(), 3);

    // Another project's key may not.
    let (status, _) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/requirements"), Some(&owner_b), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Another project's document id contributes nothing, and a malformed id is refused.
    let (_, b_list) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_b}/requirements"), Some(&owner_b), None),
    )
    .await;
    let b_doc = b_list["documents"][0]["id"].as_str().unwrap().to_string();
    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/requirements?document_ids={b_doc}"), Some(&owner_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["requirements"].as_array().unwrap().is_empty(), "body={body}");
    assert!(body["documents"].as_array().unwrap().is_empty(), "body={body}");

    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/requirements?document_ids=not-a-uuid"), Some(&owner_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
}

#[tokio::test]
async fn deleting_a_project_removes_its_documents_and_requirements() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Requirements - Delete").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SAMPLE_FRD.as_bytes(), Some("FRD")).await;

    assert!(state.store.delete_project(project).await.unwrap());

    let documents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM requirement_documents")
        .fetch_one(&state.store.pool)
        .await
        .unwrap();
    let requirements: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM requirements")
        .fetch_one(&state.store.pool)
        .await
        .unwrap();
    assert_eq!(documents, 0);
    assert_eq!(requirements, 0);
}

// ============================= Phase 3 — generation from the requirements of documents

/// The generate/plan endpoints read `REQUIREMENT_BATCH_CHARS` and
/// `REQUIREMENT_MAX_CALLS` from the environment, so the tests that go through
/// them (and the two that set them) take turns.
static REQUIREMENT_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn ai_infra_with_executor(executor: Arc<dyn AiExecutor>) -> Arc<AiInfrastructure> {
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

const PASSWORD_FRD: &str = "## 2 Passwords\n\nA password must be at least 8 characters long.\n\nUsers shall sign in with an email address.\n";

fn generate_ai_body(document_ids: &[&str], skills: &[&str]) -> Value {
    serde_json::json!({
        "requirement_text": "Requested generation depth: Standard.",
        "skills": skills.iter().map(|n| serde_json::json!({"id": format!("skill-{n}"), "name": n})).collect::<Vec<_>>(),
        "requirement_document_ids": document_ids,
    })
}

async fn post_generate(state: &AppState, key: &str, project: Uuid, body: Value) -> (StatusCode, Value) {
    send(
        state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(key),
            Some(body),
        ),
    )
    .await
}

#[tokio::test]
async fn generating_from_a_documents_requirements_shows_the_model_them_and_ties_the_cases_to_them() {
    let _env = REQUIREMENT_ENV.lock().await;
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![valid_ai_response()]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let project = make_project(&state, "ReqGen - Basic").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, uploaded) = upload_document(&state, &owner, project, "FRD.md", PASSWORD_FRD.as_bytes(), Some("FRD")).await;
    let doc_id = uploaded["requirements_document"]["document_id"].as_str().unwrap().to_string();

    let (status, body) = post_generate(&state, &owner, project, generate_ai_body(&[&doc_id], &["Functional Testing"])).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["generation_method"], "ai", "body={body}");
    // The case quotes the password rule, so it is tied to the requirement that says it.
    assert_eq!(body["test_cases"][0]["requirement_id"], "FRD-2-01", "body={body}");
    assert_eq!(body["waves"][0]["batch"], "1/1");
    assert_eq!(body["waves"][0]["requirements"], 2);
    let coverage = &body["requirements"];
    assert_eq!(coverage["total"], 2, "body={body}");
    assert_eq!(coverage["offered"], 2);
    assert_eq!(coverage["covered"], 1);
    assert_eq!(coverage["uncovered_keys"], serde_json::json!(["FRD-2-02"]));
    assert_eq!(coverage["calls"], 1);

    // What the model was actually asked: numbered requirements and the settings, not the document text.
    let prompts = executor.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("[FRD-2-01] (2 Passwords · security) A password must be at least 8 characters long."), "{}", prompts[0]);
    assert!(prompts[0].contains("[FRD-2-02] "));
    assert!(prompts[0].contains("Requested generation depth: Standard."));
    assert!(!prompts[0].contains("## 2 Passwords"), "the raw document must not be pasted in");
}

#[tokio::test]
async fn a_request_that_names_no_documents_is_generated_exactly_as_before() {
    let _env = REQUIREMENT_ENV.lock().await;
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "ReqGen - Old Path").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = post_generate(
        &state,
        &owner,
        project,
        serde_json::json!({"requirement_text": "Password must be at least 8 characters.", "skills": []}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body.get("requirements").is_none(), "no coverage claim for a request that named no documents: {body}");
    assert!(body["test_cases"][0].get("requirement_id").is_none(), "body={body}");
    assert!(body["waves"][0].get("batch").is_none(), "body={body}");
}

#[tokio::test]
async fn documents_with_no_requirements_or_of_another_project_are_refused() {
    let _env = REQUIREMENT_ENV.lock().await;
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project_a = make_project(&state, "ReqGen - Refuse A").await;
    let project_b = make_project(&state, "ReqGen - Refuse B").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let (_, b_upload) = upload_document(&state, &owner_b, project_b, "FRD.md", PASSWORD_FRD.as_bytes(), Some("FRD")).await;
    let b_doc = b_upload["requirements_document"]["document_id"].as_str().unwrap().to_string();

    for doc in [Uuid::new_v4().to_string(), b_doc] {
        let (status, body) = post_generate(&state, &owner_a, project_a, generate_ai_body(&[&doc], &["Functional Testing"])).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "doc={doc} body={body}");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        assert!(body["error"]["message"].as_str().unwrap().contains("none of the selected documents"), "body={body}");
    }
}

#[tokio::test]
async fn when_the_model_cannot_answer_the_response_says_so_and_still_reports_what_was_not_reached() {
    let _env = REQUIREMENT_ENV.lock().await;
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Err(crate::ai_router::ExecutionError::Timeout))).await;
    let project = make_project(&state, "ReqGen - Failure").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, uploaded) = upload_document(&state, &owner, project, "FRD.md", PASSWORD_FRD.as_bytes(), Some("FRD")).await;
    let doc_id = uploaded["requirements_document"]["document_id"].as_str().unwrap().to_string();

    let (status, body) = post_generate(&state, &owner, project, generate_ai_body(&[&doc_id], &["Functional Testing"])).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["generation_method"], "fallback");
    assert!(body["fallback_reason"].as_str().unwrap().contains("TIMEOUT"), "body={body}");
    assert_eq!(body["requirements"]["covered"], 0);
    assert_eq!(body["requirements"]["uncovered_keys"], serde_json::json!(["FRD-2-01", "FRD-2-02"]));
}

#[tokio::test]
async fn a_smaller_batch_budget_makes_more_calls_each_carrying_its_own_requirements() {
    let _env = REQUIREMENT_ENV.lock().await;
    let mut doc = String::from("## 1 Scope\n\n");
    for i in 1..=6 {
        doc.push_str(&format!("The system shall handle scenario number {i} exactly as the specification describes it.\n\n"));
    }
    // Every call gets a usable answer; a batch may legitimately produce nothing.
    let empty = r#"{"test_cases":[]}"#;
    let mut answers = vec![valid_ai_response()];
    answers.extend(std::iter::repeat(valid_ai_response()).take(5));
    answers.extend([empty, empty]);
    let executor = Arc::new(RecordingExecutor::ok_sequence(answers));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let project = make_project(&state, "ReqGen - Budget").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, uploaded) = upload_document(&state, &owner, project, "FRD.md", doc.as_bytes(), Some("FRD")).await;
    let doc_id = uploaded["requirements_document"]["document_id"].as_str().unwrap().to_string();

    std::env::set_var("REQUIREMENT_BATCH_CHARS", "300");
    let (status, body) = post_generate(&state, &owner, project, generate_ai_body(&[&doc_id], &["Functional Testing"])).await;
    std::env::remove_var("REQUIREMENT_BATCH_CHARS");

    assert_eq!(status, StatusCode::OK, "body={body}");
    let calls = body["requirements"]["calls"].as_u64().unwrap();
    assert!(calls >= 3, "300 characters cannot hold six requirements: body={body}");
    assert_eq!(body["requirements"]["budget_chars"], 300);
    let prompts = executor.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len() as u64, calls);
    // Each requirement appears in exactly one prompt.
    for i in 1..=6 {
        let id = format!("[FRD-1-0{i}]");
        assert_eq!(prompts.iter().filter(|p| p.contains(&id)).count(), 1, "{id}");
    }
}

#[tokio::test]
async fn a_request_that_would_make_too_many_calls_is_refused_before_any_is_made() {
    let _env = REQUIREMENT_ENV.lock().await;
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let project = make_project(&state, "ReqGen - Too Many").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, uploaded) = upload_document(&state, &owner, project, "FRD.md", PASSWORD_FRD.as_bytes(), Some("FRD")).await;
    let doc_id = uploaded["requirements_document"]["document_id"].as_str().unwrap().to_string();

    std::env::set_var("REQUIREMENT_MAX_CALLS", "1");
    let (status, body) = post_generate(
        &state,
        &owner,
        project,
        generate_ai_body(&[&doc_id], &["Functional Testing", "Negative Testing"]),
    )
    .await;
    std::env::remove_var("REQUIREMENT_MAX_CALLS");

    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(body["error"]["message"].as_str().unwrap().contains("2 AI calls"), "body={body}");
    assert!(executor.prompts.lock().unwrap().is_empty(), "no model may be called");
}

#[tokio::test]
async fn the_plan_says_what_a_generation_would_do_without_calling_a_model() {
    let _env = REQUIREMENT_ENV.lock().await;
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let project = make_project(&state, "ReqGen - Plan").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let frd = "## 1 Rules\n\nThe system shall let a user sign in with a password.\n\nThe dashboard shall load within two seconds for a user.\n";
    let (_, uploaded) = upload_document(&state, &owner, project, "FRD.md", frd.as_bytes(), Some("FRD")).await;
    let doc_id = uploaded["requirements_document"]["document_id"].as_str().unwrap().to_string();

    let (status, plan) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai/plan"),
            Some(&owner),
            Some(generate_ai_body(&[&doc_id], &["Functional Testing", "Performance Testing", "Security Testing"])),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "plan={plan}");
    assert_eq!(plan["requirements_total"], 2);
    assert_eq!(plan["by_kind"]["performance"], 1, "plan={plan}");
    assert_eq!(plan["by_kind"]["security"], 1, "plan={plan}");
    assert_eq!(plan["offered"], 2);
    assert_eq!(plan["calls"], 3, "functional, performance and security each see something");
    assert_eq!(plan["within_limit"], true);
    assert_eq!(plan["max_calls"], 150);
    let skills = plan["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 3);
    assert_eq!(skills[0]["skill_name"], "Functional Testing");
    assert_eq!(skills[0]["requirements"], 1);
    assert_eq!(plan["provider_name"], "mock");
    assert_eq!(plan["model"], "mock-model");
    assert_eq!(plan["prompt_window_tokens"], 16_384, "no window reported: the modest default");
    assert_eq!(plan["budget_chars"], 12_000);
    assert!(plan["estimated_prompt_tokens"].as_u64().unwrap() > 0);
    assert_eq!(plan["estimated_completion_tokens"], 6_000);
    assert!(executor.prompts.lock().unwrap().is_empty(), "planning must never call a model");
}

#[tokio::test]
async fn the_plan_says_when_a_request_is_over_the_call_limit() {
    let _env = REQUIREMENT_ENV.lock().await;
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "ReqGen - Plan Limit").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, uploaded) = upload_document(&state, &owner, project, "FRD.md", PASSWORD_FRD.as_bytes(), Some("FRD")).await;
    let doc_id = uploaded["requirements_document"]["document_id"].as_str().unwrap().to_string();
    let path = format!("/api/v1/projects/{project}/test-cases/generate-ai/plan");
    let body = generate_ai_body(&[&doc_id], &["Functional Testing", "Negative Testing"]);

    let (_, within) = send(&state, req("POST", &path, Some(&owner), Some(body.clone()))).await;
    std::env::set_var("REQUIREMENT_MAX_CALLS", "1");
    let (_, over) = send(&state, req("POST", &path, Some(&owner), Some(body))).await;
    std::env::remove_var("REQUIREMENT_MAX_CALLS");

    assert_eq!((within["calls"].as_u64(), within["within_limit"].as_bool()), (Some(2), Some(true)));
    assert_eq!((over["calls"].as_u64(), over["max_calls"].as_u64(), over["within_limit"].as_bool()), (Some(2), Some(1), Some(false)));
}

#[test]
fn the_retrieval_query_is_the_request_text_plus_the_start_of_the_requirements() {
    let item = |text: &str| crate::requirement_generation::RequirementItem {
        key: "R".to_string(),
        section: None,
        kind: crate::requirements::RequirementKind::Functional,
        text: text.to_string(),
    };
    let long = "x".repeat(1_500);
    let items = vec![item("first requirement"), item(&long), item(&long)];

    let query = crate::api::requirement_retrieval_query("Depth: Standard.", &items);

    assert!(query.starts_with("Depth: Standard.\nfirst requirement\n"), "{query}");
    let requirement_part = query.trim_start_matches("Depth: Standard.\n").chars().count();
    assert!(requirement_part <= 2_000 + 3, "at most about 2,000 characters of requirements: {requirement_part}");
    assert_eq!(
        query.matches('x').count(),
        2_000 - "first requirement".len(),
        "the second requirement fits, the third is cut at the room that is left"
    );
    assert_eq!(crate::api::requirement_retrieval_query("Only text.", &[]), "Only text.");
}

#[tokio::test]
async fn the_plan_is_operator_only_project_scoped_and_needs_documents() {
    let _env = REQUIREMENT_ENV.lock().await;
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "ReqGen - Plan Auth").await;
    let other = make_project(&state, "ReqGen - Plan Other").await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let other_owner = mint_key(&state, "operator", Some(&other.to_string())).await;
    let path = format!("/api/v1/projects/{project}/test-cases/generate-ai/plan");

    let (status, _) = send(&state, req("POST", &path, Some(&viewer), Some(generate_ai_body(&[], &[])))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&state, req("POST", &path, Some(&other_owner), Some(generate_ai_body(&[], &[])))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(&state, req("POST", &path, Some(&operator), Some(generate_ai_body(&[], &["Functional Testing"])))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(body["error"]["message"].as_str().unwrap().contains("requirement_document_ids"));
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

    let audit = state
        .store
        .list_audit(None, 50, 0)
        .await
        .expect("list_audit");
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

    let audit = state
        .store
        .list_audit(None, 50, 0)
        .await
        .expect("list_audit");
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
async fn ai_status_routing_preview_follows_the_test_generation_chain() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    state.ai_infra.discover_all().await;
    let operator = mint_key(&state, "operator", None).await;
    let preview = |chain: serde_json::Value| {
        req(
            "PUT",
            "/api/v1/ai/settings",
            Some(&operator),
            Some(serde_json::json!({
                "mode": "AUTO",
                "fallback_enabled": true,
                "task_overrides": {"TEST_CASE_GENERATION": {"provider": null, "model": null, "chain": chain}}
            })),
        )
    };

    let (_, body) = send(&state, req("GET", "/api/v1/ai/status", Some(&operator), None)).await;
    assert_eq!(body["routing_preview"]["model_id"], "mock-model", "body={body}");
    assert!(!body["routing_preview"]["reason"].as_str().unwrap().contains("Task chain"));

    let (status, _) = send(&state, preview(serde_json::json!([{"provider": "mock", "model": "mock-model"}]))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = send(&state, req("GET", "/api/v1/ai/status", Some(&operator), None)).await;
    assert_eq!(body["routing_preview"]["model_id"], "mock-model", "body={body}");
    assert!(body["routing_preview"]["reason"].as_str().unwrap().contains("Task chain"), "body={body}");

    // A chain whose only entry is unavailable must not silently preview some other model.
    let (status, _) = send(&state, preview(serde_json::json!([{"provider": "gone", "model": "nope"}]))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = send(&state, req("GET", "/api/v1/ai/status", Some(&operator), None)).await;
    assert!(body["routing_preview"].is_null(), "body={body}");
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
async fn ai_settings_task_chain_is_persisted_read_back_and_live_in_the_router() {
    let (state, dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let operator = mint_key(&state, "operator", None).await;
    let (status, body) = send(
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
                    "TEST_CASE_GENERATION": {"provider": null, "model": null, "chain": [
                        {"provider": "ollama", "model": "qwen3:14b"},
                        {"provider": "gemini", "model": "gemini-2.5-flash"}
                    ]}
                }
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");

    let (_, refetched) = send(&state, req("GET", "/api/v1/ai/settings", Some(&operator), None)).await;
    let chain = &refetched["task_overrides"]["TEST_CASE_GENERATION"]["chain"];
    assert_eq!(chain[0]["provider"], "ollama", "body={refetched}");
    assert_eq!(chain[1]["model"], "gemini-2.5-flash", "body={refetched}");

    // Durable, on-disk persistence (same independent-connection proof the plain overrides use).
    let reloaded = crate::store::Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("reconnect");
    let (_, _, _, _, task_overrides) = reloaded.get_ai_settings().await.expect("query").expect("a row must exist");
    let parsed: serde_json::Value = serde_json::from_str(&task_overrides.expect("persisted as JSON text")).unwrap();
    assert_eq!(parsed["TEST_CASE_GENERATION"]["chain"][1]["provider"], "gemini");

    let live = state.ai_infra.settings().await;
    let entry = live
        .task_overrides
        .get(&crate::ai_router::AiTask::TestCaseGeneration)
        .expect("the chain must be live in the in-memory router");
    assert_eq!(entry.chain.len(), 2);
}

#[tokio::test]
async fn ai_settings_put_rejects_a_task_chain_that_is_malformed_or_too_long() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;
    let put = |chain: serde_json::Value| {
        req(
            "PUT",
            "/api/v1/ai/settings",
            Some(&operator),
            Some(serde_json::json!({
                "mode": "AUTO",
                "fallback_enabled": true,
                "task_overrides": {"TEST_CASE_GENERATION": {"provider": null, "model": null, "chain": chain}}
            })),
        )
    };

    let (status, body) = send(&state, put(serde_json::json!([{"provider": "ollama", "model": "  "}]))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    let nine: Vec<_> = (0..9).map(|i| serde_json::json!({"provider": "p", "model": format!("m{i}")})).collect();
    let (status, body) = send(&state, put(serde_json::Value::Array(nine))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");

    // Nothing was saved by either rejected request.
    assert!(state.ai_infra.settings().await.task_overrides.is_empty());
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

// -------------------------------------------- custom AI providers (Settings UI)

#[tokio::test]
async fn custom_ai_provider_create_is_live_on_the_very_next_discovery_no_restart_needed() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "My Custom Provider",
                "base_url": "http://127.0.0.1:1",
                "api_key": "sk-real-secret-value"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    assert_eq!(body["name"], "My Custom Provider");
    assert!(
        body.get("api_key").is_none(),
        "the real secret must never be echoed back in the create response: body={body}"
    );
    let id = body["id"].as_str().expect("real id").to_string();

    // The real, in-memory AiInfrastructure — the actual thing `generate()`
    // reads from — must already know about it, with no process restart.
    let providers = state.ai_infra.discover_all().await;
    assert!(
        providers.iter().any(|p| p.id == id),
        "the new custom provider must be discoverable immediately: providers={providers:?}"
    );
}

#[tokio::test]
async fn custom_ai_provider_list_never_includes_the_raw_api_key() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;
    send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "My Custom Provider",
                "base_url": "http://127.0.0.1:1",
                "api_key": "sk-real-secret-value"
            })),
        ),
    )
    .await;

    let (status, body) = send(
        &state,
        req("GET", "/api/v1/ai/providers/custom", Some(&operator), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let list = body.as_array().expect("array");
    assert_eq!(list.len(), 1);
    assert!(
        list[0].get("api_key").is_none(),
        "the real secret must never appear in the list response: body={body}"
    );
    let raw = body.to_string();
    assert!(
        !raw.contains("sk-real-secret-value"),
        "the raw key must not appear anywhere in the response body at all: body={raw}"
    );
}

#[tokio::test]
async fn custom_ai_providers_survive_a_fresh_store_reload_real_durable_persistence() {
    let (state, dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;
    send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "My Custom Provider",
                "base_url": "http://127.0.0.1:1",
                "api_key": "sk-real-secret-value"
            })),
        ),
    )
    .await;

    // A completely independent `Store` connection against the same real
    // sqlite file — proves this is durable, on-disk persistence, not an
    // in-memory echo of the request that created it.
    let reloaded = crate::store::Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("reconnect");
    let rows = reloaded.list_custom_ai_providers().await.expect("query");
    assert_eq!(rows.len(), 1, "rows={rows:?}");
    assert_eq!(rows[0].name, "My Custom Provider");
    assert_eq!(rows[0].base_url, "http://127.0.0.1:1");
    assert_eq!(
        rows[0].api_key, "sk-real-secret-value",
        "the real key must round-trip exactly — it has to be usable for real outbound auth"
    );
}

#[tokio::test]
async fn custom_ai_provider_delete_removes_it_and_it_stops_being_discovered() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;
    let (_, created) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "My Custom Provider",
                "base_url": "http://127.0.0.1:1",
                "api_key": "sk-real-secret-value"
            })),
        ),
    )
    .await;
    let id = created["id"].as_str().expect("real id").to_string();

    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/ai/providers/custom/{id}"),
            Some(&operator),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let providers = state.ai_infra.discover_all().await;
    assert!(
        !providers.iter().any(|p| p.id == id),
        "a deleted custom provider must genuinely stop being discovered: providers={providers:?}"
    );

    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/ai/providers/custom/{id}"),
            Some(&operator),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "deleting an already-deleted (or never-existed) id must be a real 404, never a false ok"
    );
}

#[tokio::test]
async fn custom_ai_provider_rbac_viewer_can_list_but_cannot_create_or_delete() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;
    let viewer = mint_key(&state, "viewer", None).await;

    let (status, _) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&viewer),
            Some(serde_json::json!({
                "name": "Should Not Be Created",
                "base_url": "http://127.0.0.1:1",
                "api_key": "sk-whatever"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &state,
        req("GET", "/api/v1/ai/providers/custom", Some(&viewer), None),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "reading configured providers is not a privileged action"
    );

    let (_, created) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "Real Provider",
                "base_url": "http://127.0.0.1:1",
                "api_key": "sk-whatever"
            })),
        ),
    )
    .await;
    let id = created["id"].as_str().expect("real id").to_string();
    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/ai/providers/custom/{id}"),
            Some(&viewer),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

fn custom_provider_body(extra: serde_json::Value) -> serde_json::Value {
    let mut body = serde_json::json!({
        "name": "My Custom Provider",
        "base_url": "http://127.0.0.1:1",
        "api_key": "sk-real-secret-value"
    });
    for (k, v) in extra.as_object().expect("an object").clone() {
        body[k] = v;
    }
    body
}

#[tokio::test]
async fn custom_ai_provider_price_is_saved_shown_and_makes_the_provider_metered() {
    let (state, dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(custom_provider_body(
                serde_json::json!({"input_usd_per_1k": 0.5, "output_usd_per_1k": 1.5}),
            )),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    assert_eq!(body["input_usd_per_1k"], 0.5, "body={body}");
    assert_eq!(body["output_usd_per_1k"], 1.5, "body={body}");
    let id = body["id"].as_str().expect("real id").to_string();

    let (_, listed) = send(
        &state,
        req("GET", "/api/v1/ai/providers/custom", Some(&operator), None),
    )
    .await;
    assert_eq!(listed[0]["input_usd_per_1k"], 0.5, "body={listed}");

    let reloaded = crate::store::Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("reconnect");
    let rows = reloaded.list_custom_ai_providers().await.expect("query");
    assert_eq!(rows[0].input_usd_per_1k, Some(0.5));
    assert_eq!(rows[0].output_usd_per_1k, Some(1.5));

    // Live in the router the very next discovery, not only on disk.
    let providers = state.ai_infra.discover_all().await;
    let provider = providers.iter().find(|p| p.id == id).expect("discovered");
    assert_eq!(
        provider.pricing,
        crate::ai_router::ProviderPricing::Metered {
            input_usd_per_1k: 0.5,
            output_usd_per_1k: 1.5
        }
    );
}

#[tokio::test]
async fn custom_ai_provider_without_a_price_shows_none_and_stays_unmetered() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(custom_provider_body(serde_json::json!({}))),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    assert!(body["input_usd_per_1k"].is_null(), "body={body}");
    assert!(body["output_usd_per_1k"].is_null(), "body={body}");
    let id = body["id"].as_str().expect("real id").to_string();

    let providers = state.ai_infra.discover_all().await;
    let provider = providers.iter().find(|p| p.id == id).expect("discovered");
    assert_eq!(provider.pricing, crate::ai_router::ProviderPricing::Unmetered);
}

#[tokio::test]
async fn custom_ai_provider_rejects_a_half_or_negative_price() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;

    for extra in [
        serde_json::json!({"input_usd_per_1k": 0.5}),
        serde_json::json!({"output_usd_per_1k": 1.5}),
        serde_json::json!({"input_usd_per_1k": -0.1, "output_usd_per_1k": 1.5}),
        serde_json::json!({"input_usd_per_1k": 0.5, "output_usd_per_1k": -1.0}),
    ] {
        let (status, body) = send(
            &state,
            req(
                "POST",
                "/api/v1/ai/providers/custom",
                Some(&operator),
                Some(custom_provider_body(extra.clone())),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "extra={extra} body={body}");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    }
    let (_, listed) = send(
        &state,
        req("GET", "/api/v1/ai/providers/custom", Some(&operator), None),
    )
    .await;
    assert!(listed.as_array().unwrap().is_empty(), "nothing was saved");

    // Zero is a real price (a provider that charges nothing), not a missing one.
    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(custom_provider_body(
                serde_json::json!({"input_usd_per_1k": 0.0, "output_usd_per_1k": 0.0}),
            )),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
}

#[tokio::test]
async fn custom_ai_provider_key_can_be_an_env_reference_and_the_secret_is_never_stored_or_shown() {
    const VAR: &str = "AIQA_TEST_API_ENV_KEY_VAR";
    const SECRET: &str = "the-actual-secret-value-in-the-environment";
    std::env::remove_var(VAR);
    let (state, dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(custom_provider_body(
                serde_json::json!({"api_key": format!("env:{VAR}")}),
            )),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    assert_eq!(body["api_key_env"], VAR, "body={body}");
    assert_eq!(
        body["api_key_env_set"], false,
        "the variable is not set on the server yet: body={body}"
    );
    let id = body["id"].as_str().expect("real id").to_string();

    // Not set: the router reports it as not configured instead of trying to call out with no key.
    let providers = state.ai_infra.discover_all().await;
    let provider = providers.iter().find(|p| p.id == id).expect("discovered");
    assert_eq!(
        provider.status,
        crate::ai_router::ProviderStatus::NotConfigured
    );

    std::env::set_var(VAR, SECRET);
    let (_, listed) = send(
        &state,
        req("GET", "/api/v1/ai/providers/custom", Some(&operator), None),
    )
    .await;
    std::env::remove_var(VAR);
    assert_eq!(listed[0]["api_key_env_set"], true, "body={listed}");
    assert!(
        !listed.to_string().contains(SECRET),
        "the value of the variable must never reach a response: body={listed}"
    );

    let reloaded = crate::store::Store::connect(&dir.path().join("api.sqlite3"))
        .await
        .expect("reconnect");
    let rows = reloaded.list_custom_ai_providers().await.expect("query");
    assert_eq!(
        rows[0].api_key,
        format!("env:{VAR}"),
        "only the variable's name is stored, never its value"
    );
}

#[tokio::test]
async fn a_key_stored_as_itself_has_no_env_fields() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;
    let (_, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(custom_provider_body(serde_json::json!({}))),
        ),
    )
    .await;
    assert!(body["api_key_env"].is_null(), "body={body}");
    assert!(body["api_key_env_set"].is_null(), "body={body}");
}

#[tokio::test]
async fn custom_ai_provider_rejects_a_malformed_env_reference() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;

    for bad in ["env:", "env:  ", "env:1ABC", "env:A B", "env:A-B", "env:A=B"] {
        let (status, body) = send(
            &state,
            req(
                "POST",
                "/api/v1/ai/providers/custom",
                Some(&operator),
                Some(custom_provider_body(serde_json::json!({"api_key": bad}))),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "key={bad} body={body}");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    }
    for good in ["env:OPENAI_API_KEY", "env:_x9", "env: PADDED "] {
        let (status, body) = send(
            &state,
            req(
                "POST",
                "/api/v1/ai/providers/custom",
                Some(&operator),
                Some(custom_provider_body(serde_json::json!({"api_key": good}))),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "key={good} body={body}");
    }
}

#[tokio::test]
async fn custom_ai_provider_rejects_a_real_validation_failure_never_silently_accepted() {
    let (state, _dir) = test_state_with_ai_router(inert_ai_router()).await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "",
                "base_url": "http://127.0.0.1:1",
                "api_key": "sk-whatever"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "Bad URL",
                "base_url": "not-a-real-url",
                "api_key": "sk-whatever"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");

    let (status, body) = send(
        &state,
        req(
            "POST",
            "/api/v1/ai/providers/custom",
            Some(&operator),
            Some(serde_json::json!({
                "name": "No Key",
                "base_url": "http://127.0.0.1:1",
                "api_key": ""
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
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
        unconfigured_embedder(),
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
        req(
            "GET",
            &format!("/api/v1/projects/{project}"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "To Delete");

    // DELETE project
    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/projects/{project}"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify GET now returns 404
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Verify audit log has project.delete entry
    let audit_rows = state
        .store
        .list_audit(None, 100, 0)
        .await
        .expect("audit log");
    let delete_entry = audit_rows.iter().find(|a| {
        a.action == "project.delete" && a.target.as_deref() == Some(&project.to_string())
    });
    assert!(
        delete_entry.is_some(),
        "audit log must record project.delete"
    );
}

/// Phase 2 gap-closing. `delete_project_admin_success_with_cascading_
/// cleanup` above never actually seeds a `test_case_embeddings`/`documents`
/// row, so it could not have caught either table being left behind — this
/// test seeds a real one of each (a real approved+embedded test case, a
/// real extracted+embedded document) and proves both are genuinely gone
/// after delete. `test_case_embeddings` cleanup was a real, pre-existing
/// gap (no FK, and `delete_project` never touched it before this change) —
/// this closes that alongside the new `documents` table's own cleanup.
#[tokio::test]
async fn delete_project_also_cleans_up_test_case_embeddings_and_documents() {
    let embedder = Arc::new(FakeEmbedder::configured(vec![1.0, 0.0, 0.0]));
    let (state, _dir) = test_state_with_ai_router_and_embedder(inert_ai_router(), embedder).await;
    let admin = mint_key(&state, "admin", None).await;
    let project = make_project(&state, "To Delete With Embeddings").await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (_, created) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases"),
            Some(&operator),
            Some(sample_test_case_body("Reject invalid password")),
        ),
    )
    .await;
    let case_id = created["id"].as_str().unwrap().to_string();
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{case_id}/validate"),
            Some(&operator),
            None,
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/{case_id}/approve"),
            Some(&operator),
            None,
        ),
    )
    .await;
    send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/documents/extract-text"),
            Some(&operator),
            Some(serde_json::json!({
                "file_name": "notes.txt",
                "content_base64": b64(b"Refunds must be processed within 5 business days.")
            })),
        ),
    )
    .await;

    assert_eq!(
        state
            .store
            .count_test_case_embeddings(&project.to_string())
            .await
            .unwrap(),
        1,
        "setup: a real embedded case must exist before delete"
    );
    assert_eq!(
        state
            .store
            .count_document_embeddings(&project.to_string())
            .await
            .unwrap(),
        1,
        "setup: a real embedded document must exist before delete"
    );

    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/projects/{project}"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    assert_eq!(
        state
            .store
            .count_test_case_embeddings(&project.to_string())
            .await
            .unwrap(),
        0,
        "deleting a project must not leave an orphaned test_case_embeddings row behind"
    );
    assert_eq!(
        state
            .store
            .count_document_embeddings(&project.to_string())
            .await
            .unwrap(),
        0,
        "deleting a project must not leave an orphaned documents row behind"
    );
}

#[tokio::test]
async fn delete_project_forbidden_for_non_admin_roles() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Role Check").await;
    let operator = mint_key(&state, "operator", None).await;
    let viewer = mint_key(&state, "viewer", None).await;

    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/projects/{project}"),
            Some(&operator),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &state,
        req(
            "DELETE",
            &format!("/api/v1/projects/{project}"),
            Some(&viewer),
            None,
        ),
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
        req(
            "DELETE",
            &format!("/api/v1/projects/{random_id}"),
            Some(&admin),
            None,
        ),
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
        req(
            "DELETE",
            &format!("/api/v1/projects/{proj_b}"),
            Some(&scoped_admin_a),
            None,
        ),
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
    state
        .log_bus
        .emit(
            "INFO",
            "ai_router",
            "Global discovery started",
            None,
            None,
            None,
        )
        .await;
    state
        .log_bus
        .emit(
            "INFO",
            "generation",
            "Generating for A",
            None,
            Some(&proj_a.to_string()),
            None,
        )
        .await;
    state
        .log_bus
        .emit(
            "INFO",
            "execution",
            "Running B",
            None,
            Some(&proj_b.to_string()),
            None,
        )
        .await;

    // Admin sees all logs
    let (status, logs) = send(&state, req("GET", "/api/v1/logs", Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK);
    let arr = logs.as_array().expect("array");
    assert_eq!(arr.len(), 3);

    // Operator A sees global + Proj A logs, but not Proj B
    let (status, logs_a) = send(&state, req("GET", "/api/v1/logs", Some(&operator_a), None)).await;
    assert_eq!(status, StatusCode::OK);
    let arr_a = logs_a.as_array().expect("array");
    assert_eq!(arr_a.len(), 2);
    let msgs: Vec<&str> = arr_a
        .iter()
        .map(|l| l["message"].as_str().unwrap())
        .collect();
    assert!(msgs.contains(&"Global discovery started"));
    assert!(msgs.contains(&"Generating for A"));
    assert!(!msgs.contains(&"Running B"));

    // Scoped route GET /api/v1/projects/:id/logs
    let (status, proj_logs) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{proj_a}/logs"),
            Some(&operator_a),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(proj_logs.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_project_risk_intelligence_endpoint() {
    let (state, _dir) = test_state().await;
    let proj_id = Uuid::new_v4();
    state
        .store
        .insert_project(proj_id, "Risk Test Proj", None)
        .await
        .unwrap();

    let admin = mint_key(&state, "admin", None).await;

    // 1. Initial state: insufficient data
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{proj_id}/intelligence/risk"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["risk_level"], "INSUFFICIENT_DATA");
    assert_eq!(body["risk_score"], 0.0);
    assert_eq!(body["factors"]["failure_rate"], 0.0);

    // 2. Insert real execution telemetry using TaskState
    let mut task_state =
        crate::domain::TaskState::new("task-1".to_string(), "Checkout Task".to_string());
    task_state.status = crate::domain::TaskStatus::Failed;
    task_state.started_at = Some(chrono::Utc::now());
    task_state.finished_at = Some(chrono::Utc::now() + chrono::Duration::milliseconds(1500));
    task_state.reason = Some("Selector timed out".to_string());

    let tel = crate::adaptive_intelligence::ExecutionTelemetry::from_task_state(
        &proj_id.to_string(),
        Uuid::new_v4(),
        &task_state,
        "playwright",
        "staging",
        None,
        vec![],
        Some("#btn".to_string()),
        None,
        None,
    );
    state.store.insert_execution_telemetry(&tel).await.unwrap();

    // 3. Query risk again: now should compute real score from authoritative 35% failure rate model
    let (status2, body2) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{proj_id}/intelligence/risk"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    assert_ne!(body2["risk_level"], "INSUFFICIENT_DATA");
    assert!(body2["risk_score"].as_f64().unwrap() > 0.0);
    assert_eq!(body2["factors"]["failure_rate"], 1.0);
}

#[tokio::test]
async fn test_phase8_investigations_and_drilldown_endpoints() {
    let (state, _dir) = test_state().await;
    let proj_id = Uuid::new_v4();
    state
        .store
        .insert_project(proj_id, "Operations Project", None)
        .await
        .unwrap();

    let admin = mint_key(&state, "admin", None).await;

    // Register failure cluster
    let (cluster_id, _) = state
        .store
        .upsert_failure_cluster(
            &proj_id.to_string(),
            "sig-ops-test",
            "API Gateway Timeout",
            0.9,
        )
        .await
        .unwrap();

    // Add telemetry
    let mut task = crate::domain::TaskState::new("task-ops-1", "GatewayTest");
    task.status = crate::domain::TaskStatus::Failed;
    task.reason = Some("sig-ops-test: connection timed out after 30000ms".to_string());
    task.started_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
    task.finished_at = Some(chrono::Utc::now() - chrono::Duration::minutes(59));

    let tel = crate::adaptive_intelligence::ExecutionTelemetry::from_task_state(
        &proj_id.to_string(),
        Uuid::new_v4(),
        &task,
        "api_executor",
        "staging",
        None,
        vec!["evidence/api-log.txt".to_string()],
        None,
        None,
        None,
    );
    state.store.insert_execution_telemetry(&tel).await.unwrap();

    // 1. GET investigations list
    let (status, list) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{proj_id}/intelligence/investigations"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["signature"], "sig-ops-test");

    // 2. GET full investigation
    let (status_inv, inv) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{proj_id}/intelligence/failures/{cluster_id}/investigation"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status_inv, StatusCode::OK);
    assert_eq!(inv["signature"], "sig-ops-test");
    assert_eq!(inv["failure_category"], "TIMEOUT");
    assert!(!inv["recommendations"].as_array().unwrap().is_empty());

    // 3. GET environment matrix
    let (status_env, env_mat) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{proj_id}/intelligence/failures/{cluster_id}/environments"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status_env, StatusCode::OK);
    assert_eq!(env_mat["signature"], "sig-ops-test");
    assert_eq!(env_mat["environments"].as_array().unwrap().len(), 1);
    assert_eq!(env_mat["environments"][0]["environment"], "staging");

    // 4. GET timeline
    let (status_time, timeline) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{proj_id}/intelligence/failures/{cluster_id}/timeline"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status_time, StatusCode::OK);
    assert!(!timeline.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_phase8_retention_endpoints_and_rbac() {
    let (state, _dir) = test_state().await;
    let proj_id = Uuid::new_v4();
    state
        .store
        .insert_project(proj_id, "Retention Project", None)
        .await
        .unwrap();

    let admin = mint_key(&state, "admin", None).await;
    let viewer = mint_key(&state, "viewer", Some(&proj_id.to_string())).await;

    // 1. Viewer attempt to call retention dry-run -> 403 Forbidden
    let (status_forbidden, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{proj_id}/intelligence/retention/dry-run"),
            Some(&viewer),
            Some(serde_json::json!({ "retention_days": 30 })),
        ),
    )
    .await;
    assert_eq!(status_forbidden, StatusCode::FORBIDDEN);

    // 2. Admin calls dry-run -> 200 OK
    let (status_ok, dry_report) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{proj_id}/intelligence/retention/dry-run"),
            Some(&admin),
            Some(serde_json::json!({ "retention_days": 30 })),
        ),
    )
    .await;
    assert_eq!(status_ok, StatusCode::OK);
    assert_eq!(dry_report["dry_run"], true);
    assert_eq!(dry_report["retention_days"], 30);

    // 3. Admin calls prune -> 200 OK
    let (status_prune, prune_report) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{proj_id}/intelligence/retention/prune"),
            Some(&admin),
            Some(serde_json::json!({ "retention_days": 30 })),
        ),
    )
    .await;
    assert_eq!(status_prune, StatusCode::OK);
    assert_eq!(prune_report["dry_run"], false);
    assert_eq!(prune_report["retention_days"], 30);
}

// ========================= Planning endpoint — schema_file_path containment

/// A minimal but genuinely valid OpenAPI document (`schema_inspector`
/// refuses anything that isn't one), so a request whose schema *does* get
/// read produces an observable plan: the distinctive `/widgets` area.
fn planning_openapi_json() -> String {
    serde_json::json!({
        "openapi": "3.0.3",
        "info": { "title": "Widget API", "version": "1.0.0" },
        "paths": {
            "/widgets": {
                "get": {
                    "operationId": "listWidgets",
                    "responses": { "200": { "description": "ok" } }
                }
            }
        }
    })
    .to_string()
}

/// Security finding: `schema_file_path` let any Operator key make the
/// server read and parse an arbitrary file on its own disk. A real, valid
/// schema that exists on the host but is not under `AIQA_SCHEMA_ROOT` must
/// be refused at the HTTP boundary, and the refusal must not echo the path.
///
/// Deliberately env-independent: whether `AIQA_SCHEMA_ROOT` is unset (file
/// paths disabled) or points somewhere else entirely, this file is refused
/// with the same 400, so the test never mutates process-wide state.
#[tokio::test]
async fn planning_test_plan_refuses_a_schema_file_path_outside_the_schema_root() {
    let (state, dir) = test_state().await;
    let project = make_project(&state, "Planning containment").await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let host_file = dir.path().join("host-only-spec.json");
    std::fs::write(&host_file, planning_openapi_json()).expect("write host file");

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/planning/test-plan"),
            Some(&operator),
            Some(serde_json::json!({
                "project_id": project.to_string(),
                "schema_file_path": host_file.to_string_lossy(),
            })),
        ),
    )
    .await;

    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("schema_content") && message.contains("AIQA_SCHEMA_ROOT"),
        "the caller must be told how to proceed: {message}"
    );
    assert!(
        !message.contains("host-only-spec") && !message.contains(&*dir.path().to_string_lossy()),
        "the error must not echo the requested or resolved path: {message}"
    );
}

/// The inline path (`schema_content`) is untouched by the containment: no
/// `AIQA_SCHEMA_ROOT` is needed, and the plan is built from the inline
/// document exactly as before.
#[tokio::test]
async fn planning_test_plan_still_accepts_an_inline_schema() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Planning inline").await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, plan) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/planning/test-plan"),
            Some(&operator),
            Some(serde_json::json!({
                "project_id": project.to_string(),
                "schema_content": planning_openapi_json(),
            })),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body={plan}");
    assert!(
        plan["api_areas_considered"]
            .as_array()
            .expect("api_areas_considered")
            .iter()
            .any(|a| a == "/widgets"),
        "the plan must be built from the inline schema: {plan}"
    );
}

/// A `PlanningRequest` as the HTTP layer deserializes it (every optional
/// field defaulted), with only the schema fields under test set.
fn planning_request(
    schema_file_path: Option<&str>,
    schema_content: Option<&str>,
) -> crate::agents::planning::PlanningRequest {
    serde_json::from_value(serde_json::json!({
        "project_id": "proj-confine",
        "schema_file_path": schema_file_path,
        "schema_content": schema_content,
    }))
    .expect("valid PlanningRequest")
}

/// Renders an `ApiError` the way the router would, so a test can read its
/// real status and envelope (its fields are private to `errors.rs`).
async fn api_error_parts(err: crate::errors::ApiError) -> (StatusCode, Value) {
    let resp = axum::response::IntoResponse::into_response(err);
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).expect("error envelope"),
    )
}

/// `AIQA_SCHEMA_ROOT` unset means server-side schema files are off: any
/// real path is refused with guidance, while the inline path (even with a
/// blank `schema_file_path` beside it) passes through untouched.
#[tokio::test]
async fn planning_confine_disables_file_paths_when_no_schema_root_is_configured() {
    let mut req = planning_request(Some("api.yaml"), None);
    let err = crate::api::confine_schema_file_path(&mut req, None).unwrap_err();
    let (status, body) = api_error_parts(err).await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("schema_content") && message.contains("AIQA_SCHEMA_ROOT"),
        "the caller must be told how to proceed: {message}"
    );

    let inline = planning_openapi_json();
    let mut req = planning_request(Some("   "), Some(&inline));
    crate::api::confine_schema_file_path(&mut req, None).expect("inline needs no root");
    assert_eq!(req.schema_file_path.as_deref(), Some("   "));
    assert_eq!(req.schema_content.as_deref(), Some(inline.as_str()));
}

/// The three ways a path can be wrong (outside the root, missing, a
/// directory) get one identical response with no path in it: telling them
/// apart would make the endpoint a file-existence oracle for the whole host.
#[tokio::test]
async fn planning_confine_refuses_every_bad_path_with_one_uniform_message() {
    let (_state, dir) = test_state().await;
    let root = dir.path().join("schemas");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    let outside = dir.path().join("host-only-spec.json");
    std::fs::write(&outside, planning_openapi_json()).unwrap();

    let outside_abs = outside.to_string_lossy().to_string();
    let mut messages = Vec::new();
    for raw in [
        outside_abs.as_str(),
        "../host-only-spec.json",
        "missing.json",
        "sub",
    ] {
        let mut req = planning_request(Some(raw), None);
        let err = crate::api::confine_schema_file_path(&mut req, Some(&root)).unwrap_err();
        let (status, body) = api_error_parts(err).await;
        assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
        let message = body["error"]["message"].as_str().unwrap().to_string();
        assert!(
            !message.contains("host-only-spec")
                && !message.contains("schemas")
                && !message.contains(&*dir.path().to_string_lossy()),
            "{raw}: the error must not echo the requested or resolved path: {message}"
        );
        assert_eq!(req.schema_file_path.as_deref(), Some(raw), "left untouched");
        messages.push(message);
    }
    assert!(
        messages.windows(2).all(|w| w[0] == w[1]),
        "every refusal must read the same: {messages:?}"
    );
}

/// The accepted path is rewritten to the canonical file that was checked,
/// and that is what the real agent and tool then read. A root-relative name
/// would otherwise be re-resolved by the schema inspector against the repo
/// root, so the file checked and the file read would differ.
#[tokio::test]
async fn planning_confine_hands_the_tool_the_canonical_file_it_checked() {
    let (state, dir) = test_state().await;
    let root = dir.path().join("schemas");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("widgets.json"), planning_openapi_json()).unwrap();

    let mut req = planning_request(Some("widgets.json"), None);
    crate::api::confine_schema_file_path(&mut req, Some(&root)).expect("inside the root");
    let canonical = std::fs::canonicalize(root.join("widgets.json")).unwrap();
    assert_eq!(
        req.schema_file_path.as_deref().map(std::path::Path::new),
        Some(canonical.as_path())
    );

    let plan =
        crate::agents::planning::generate_test_plan(state.store.clone(), "proj-confine", req)
            .await
            .expect("the tool reads the canonical path");
    assert!(
        plan.api_areas_considered.iter().any(|a| a == "/widgets"),
        "the plan must come from the file inside the root: {plan:?}"
    );
}

// ============================= Phase 4 — the coverage engine on top of requirement generation

const BASKET_FRD: &str = "## 1 Basket\n\nThe system shall let a user add an item to the basket.\n\nThe system shall let a user remove an item from the basket.\n";

/// A model answer: `(requirement id, technique, title)` per case.
fn coverage_answer(cases: &[(&str, &str, &str)]) -> String {
    serde_json::json!({
        "test_cases": cases.iter().map(|(id, technique, title)| serde_json::json!({
            "title": title, "category": "Functional", "skill_id": "s", "requirement_id": id, "technique": technique,
            "preconditions": [], "steps": [{"action": "Do it", "expected_result": "It works"}],
            "expected_result": "It works", "priority": "HIGH", "severity": "MAJOR",
            "evidence": "inferred", "source_reference": null,
        })).collect::<Vec<_>>()
    })
    .to_string()
}

async fn basket_project(state: &AppState, name: &str) -> (Uuid, String, String) {
    let project = make_project(state, name).await;
    let owner = mint_key(state, "operator", Some(&project.to_string())).await;
    let (_, uploaded) = upload_document(state, &owner, project, "FRD.md", BASKET_FRD.as_bytes(), Some("FRD")).await;
    let doc_id = uploaded["requirements_document"]["document_id"].as_str().unwrap().to_string();
    (project, owner, doc_id)
}

#[tokio::test]
async fn naming_a_depth_adds_the_coverage_matrix_and_a_request_without_one_has_none() {
    let _env = REQUIREMENT_ENV.lock().await;
    let functional = coverage_answer(&[("FRD-1-01", "positive", "Add an item"), ("FRD-1-02", "positive", "Remove an item")]);
    let negative = coverage_answer(&[("FRD-1-01", "negative", "Add nothing"), ("FRD-1-02", "negative", "Remove what is not there")]);
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![&functional, &negative, &functional, &negative]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, doc_id) = basket_project(&state, "Coverage - Matrix").await;
    let skills = ["Functional Testing", "Negative Testing"];

    let mut with_depth = generate_ai_body(&[&doc_id], &skills);
    with_depth["depth"] = serde_json::json!("Standard");
    let (status, body) = post_generate(&state, &owner, project, with_depth).await;
    let (_, without) = post_generate(&state, &owner, project, generate_ai_body(&[&doc_id], &skills)).await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    let matrix = &body["requirements"]["matrix"];
    assert_eq!(matrix["depth"], "STANDARD", "body={body}");
    let technique = |name: &str| matrix["techniques"].as_array().unwrap().iter().find(|t| t["technique"] == name).cloned();
    assert_eq!((technique("positive").unwrap()["met"].as_u64(), technique("negative").unwrap()["met"].as_u64()), (Some(2), Some(2)));
    let edge = technique("edge").unwrap();
    assert_eq!((edge["applicable"].as_u64(), edge["short"].as_u64()), (Some(2), Some(2)));
    assert!(edge["provided_by"].is_null(), "no selected skill provides edge cases: {edge}");
    assert_eq!(matrix["gap_total"], 0, "a gap nobody could fill is not listed: {matrix}");
    assert!(matrix.get("top_up").is_none(), "no top-up was asked for: {matrix}");
    // Each case says which technique it counts for.
    let techniques: Vec<&str> = body["test_cases"].as_array().unwrap().iter().filter_map(|c| c["technique"].as_str()).collect();
    assert!(techniques.contains(&"positive") && techniques.contains(&"negative"), "body={body}");
    assert!(without["requirements"].get("matrix").is_none(), "no depth, no matrix: {without}");
}

#[tokio::test]
async fn an_unknown_depth_or_a_top_up_with_no_depth_is_refused_before_any_call() {
    let _env = REQUIREMENT_ENV.lock().await;
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, doc_id) = basket_project(&state, "Coverage - Refused").await;

    let mut deep = generate_ai_body(&[&doc_id], &["Functional Testing"]);
    deep["depth"] = serde_json::json!("deep");
    let mut no_depth = generate_ai_body(&[&doc_id], &["Functional Testing"]);
    no_depth["coverage_top_up"] = serde_json::json!(true);

    let (deep_status, deep_body) = post_generate(&state, &owner, project, deep).await;
    let (top_up_status, top_up_body) = post_generate(&state, &owner, project, no_depth).await;

    assert_eq!(deep_status, StatusCode::BAD_REQUEST, "body={deep_body}");
    assert!(deep_body["error"]["message"].as_str().unwrap().contains("depth must be SMOKE, STANDARD, EXHAUSTIVE or CHAOS_EXPLORATORY"), "{deep_body}");
    assert_eq!(top_up_status, StatusCode::BAD_REQUEST, "body={top_up_body}");
    assert!(top_up_body["error"]["message"].as_str().unwrap().contains("coverage_top_up needs a depth"), "{top_up_body}");
    assert!(executor.prompts.lock().unwrap().is_empty(), "no model may be called");
}

#[tokio::test]
async fn a_depth_on_a_request_that_names_no_documents_changes_nothing() {
    let _env = REQUIREMENT_ENV.lock().await;
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Coverage - Old Path").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = post_generate(
        &state,
        &owner,
        project,
        serde_json::json!({"requirement_text": "Password must be at least 8 characters.", "skills": [], "depth": "STANDARD", "coverage_top_up": true}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body.get("requirements").is_none(), "{body}");
    assert!(body["test_cases"][0].get("technique").is_none(), "the original path labels nothing: {body}");
}

#[tokio::test]
async fn a_top_up_makes_the_extra_call_and_the_response_says_what_it_did() {
    let _env = REQUIREMENT_ENV.lock().await;
    let functional = coverage_answer(&[("FRD-1-01", "positive", "Add an item"), ("FRD-1-02", "positive", "Remove an item")]);
    let negative = coverage_answer(&[("FRD-1-01", "negative", "Add nothing"), ("FRD-1-02", "negative", "Remove what is not there")]);
    // The edge skill only found something for the first requirement.
    let edge = coverage_answer(&[("FRD-1-01", "edge", "Add the largest quantity")]);
    let top_up = coverage_answer(&[("FRD-1-02", "edge", "Remove the last item of the basket")]);
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![&functional, &negative, &edge, &top_up]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, doc_id) = basket_project(&state, "Coverage - Top Up").await;

    let mut body = generate_ai_body(&[&doc_id], &["Functional Testing", "Negative Testing", "Edge Case Analysis"]);
    body["depth"] = serde_json::json!("STANDARD");
    body["coverage_top_up"] = serde_json::json!(true);
    let (status, response) = post_generate(&state, &owner, project, body).await;

    assert_eq!(status, StatusCode::OK, "body={response}");
    let prompts = executor.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 4, "three first-pass calls and one top-up call");
    let last = &prompts[3];
    assert!(last.contains("COVERAGE TOP-UP") && last.contains("Generate ONLY edge test cases"), "{last}");
    assert!(last.contains("[FRD-1-02] ") && !last.contains("[FRD-1-01] "), "only the requirement that lacks it is asked about: {last}");
    let matrix = &response["requirements"]["matrix"];
    assert_eq!(matrix["complete"], 2, "body={response}");
    assert_eq!(matrix["gap_total"], 0);
    assert_eq!(
        matrix["top_up"],
        serde_json::json!({"rounds": 1, "calls": 1, "cases_added": 1, "stopped": "every target is met"}),
        "body={response}"
    );
    assert_eq!(response["requirements"]["calls"], 3, "the calls planned up front; the top-up is reported apart");
    let waves = response["waves"].as_array().unwrap();
    assert_eq!(waves.len(), 4);
    assert_eq!((waves[3]["wave"].as_str(), waves[3]["technique"].as_str(), waves[3]["batch"].as_str()), (Some("top_up"), Some("edge"), Some("1/1")));
    assert_eq!(response["test_cases"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn the_plan_says_which_techniques_a_depth_covers_and_what_a_top_up_may_add() {
    let _env = REQUIREMENT_ENV.lock().await;
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, doc_id) = basket_project(&state, "Coverage - Plan").await;
    let path = format!("/api/v1/projects/{project}/test-cases/generate-ai/plan");

    let mut with_depth = generate_ai_body(&[&doc_id], &["Functional Testing", "Negative Testing"]);
    with_depth["depth"] = serde_json::json!("EXHAUSTIVE");
    with_depth["coverage_top_up"] = serde_json::json!(true);
    let (status, plan) = send(&state, req("POST", &path, Some(&owner), Some(with_depth))).await;
    let (_, without) = send(&state, req("POST", &path, Some(&owner), Some(generate_ai_body(&[&doc_id], &["Functional Testing"])))).await;
    let mut bad = generate_ai_body(&[&doc_id], &["Functional Testing"]);
    bad["depth"] = serde_json::json!("deep");
    let (bad_status, _) = send(&state, req("POST", &path, Some(&owner), Some(bad))).await;

    assert_eq!(status, StatusCode::OK, "plan={plan}");
    let coverage = &plan["coverage"];
    assert_eq!(coverage["depth"], "EXHAUSTIVE");
    assert_eq!(coverage["top_up"], true);
    assert_eq!(coverage["top_up_rounds"], 2);
    assert_eq!(coverage["top_up_call_limit"], 148, "150 calls, two of them the first pass");
    let positive = coverage["techniques"].as_array().unwrap().iter().find(|t| t["technique"] == "positive").cloned().unwrap();
    assert_eq!((positive["requirements"].as_u64(), positive["per_requirement"].as_u64()), (Some(2), Some(2)));
    assert_eq!(positive["provided_by"], "Functional Testing");
    let edge = coverage["techniques"].as_array().unwrap().iter().find(|t| t["technique"] == "edge").cloned().unwrap();
    assert!(edge["provided_by"].is_null());
    assert!(without.get("coverage").is_none(), "no depth, no coverage plan: {without}");
    assert_eq!(bad_status, StatusCode::BAD_REQUEST);
    assert!(executor.prompts.lock().unwrap().is_empty(), "planning must never call a model");
}

#[tokio::test]
async fn the_rounds_of_a_top_up_are_two_unless_set_and_never_fewer_than_one_or_more_than_five() {
    let _env = REQUIREMENT_ENV.lock().await;
    assert_eq!(crate::api::coverage_top_up_rounds(), 2);
    for (value, expected) in [("1", 1), ("3", 3), (" 4 ", 4), ("9", 5), ("0", 1), ("many", 2), ("", 2)] {
        std::env::set_var("COVERAGE_TOP_UP_ROUNDS", value);
        assert_eq!(crate::api::coverage_top_up_rounds(), expected, "{value:?}");
    }
    std::env::remove_var("COVERAGE_TOP_UP_ROUNDS");
    assert_eq!(crate::api::coverage_top_up_rounds(), 2);
}

// ------------------------------------------------------- Phase 6A — the gap check
//
// A crawl cannot really run in this harness (see Phase 5B above), so each test writes what a finished crawl would have
// written for its one task: the grounding file (`helpers/crawler.ts` writes it in real life), or the older result.

const SHOP_FRD: &str = "## 3 Shop\n\nFR-001: The user shall be able to log in with a username and password.\n\nFR-002: A user can add a product to the shopping cart.\n\nFR-003: The user can remove an item from the cart.\n\nFR-004: Users can add products to a wishlist.\n\nFR-005: The system shall export any report as a CSV file.\n\nFR-006: Pages shall load within 2 seconds.\n";

const SHOP_GROUNDING: &str = r##"{"roles":[
  {"role":"standard","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[
     {"url":"https://shop.example.test/","title":"Shop","elements":[
        {"kind":"input","text":"Username","locator":"[data-test=\"username\"]","inputType":"text"},
        {"kind":"input","text":"Password","locator":"[data-test=\"password\"]","inputType":"password"},
        {"kind":"button","text":"Login","locator":"[data-test=\"login-button\"]"}]},
     {"url":"https://shop.example.test/inventory.html","title":"Shop","elements":[
        {"kind":"button","text":"Add to cart","locator":"[data-test=\"add-to-cart-backpack\"]"}]},
     {"url":"https://shop.example.test/cart.html","title":"Shop","elements":[
        {"kind":"button","text":"Remove","locator":"[data-test=\"remove-backpack\"]"},
        {"kind":"button","text":"Checkout","locator":"[data-test=\"checkout\"]"}]},
     {"url":"https://shop.example.test/legal/terms.html","title":"Terms of Service","headings":["Terms"],"elements":[
        {"kind":"link","text":"Privacy Policy","locator":"a"}]}]},
  {"role":"admin","loginSucceeded":false,"loginError":"selector never resolved","loginUsesLiterals":false,"loginActions":[],
   "pages":[{"url":"https://shop.example.test/","title":"Shop","elements":[]}]}]}"##;

/// The older result a crawl made before grounding was recorded leaves: links and buttons only.
const SHOP_RESULT: &str = r##"{"roles":[{"role":"anonymous","loginSucceeded":true,"pages":[
  {"url":"https://shop.example.test/","title":"Shop","elements":[
    {"kind":"button","text":"Login","selectorHint":"[data-test=\"login-button\"]"},
    {"kind":"link","text":"Cart","selectorHint":"#cart","href":"https://shop.example.test/cart.html"}]}]}],
  "visibilityMatrix":[]}"##;

/// Starts a crawl (which cannot really run here) and returns its run id and its task's id.
async fn crawl_run_and_task(state: &AppState, project: Uuid, key: &str) -> (String, String) {
    let (status, started) = send(
        state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/crawl"),
            Some(key),
            Some(valid_crawl_body()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={started}");
    let run_id = started["run_id"].as_str().expect("run_id").to_string();
    let (_, run) = send(
        state,
        req(
            "GET",
            &format!("/api/v1/projects/{project}/executions/{run_id}"),
            Some(key),
            None,
        ),
    )
    .await;
    let task_id = run["tasks"][0]["id"].as_str().expect("task id").to_string();
    (run_id, task_id)
}

fn write_crawl_file(file_name: &str, json: &str) -> std::path::PathBuf {
    let dir = crate::crawler::generated_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(file_name);
    std::fs::write(&path, json).unwrap();
    path
}

async fn gap_check(state: &AppState, project: Uuid, key: &str, body: Option<Value>) -> (StatusCode, Value) {
    send(
        state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/gap-check"),
            Some(key),
            body,
        ),
    )
    .await
}

fn finding_of<'a>(body: &'a Value, key: &str) -> &'a Value {
    body["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == key)
        .unwrap_or_else(|| panic!("no requirement {key} in {body}"))
}

fn case_body_for(requirement: &str, title: &str, tags: Value) -> Value {
    let mut body = open_settings_case_body(title);
    body["requirement_reference"] = Value::String(requirement.to_string());
    body["tags"] = tags;
    body
}

#[tokio::test]
async fn phase6a_the_gap_check_lines_the_requirements_up_with_the_crawl_and_the_cases() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6A Gap Check").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (status, uploaded) =
        upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    assert_eq!(status, StatusCode::OK, "body={uploaded}");
    let (run_id, task_id) = crawl_run_and_task(&state, project, &owner).await;
    let file = write_crawl_file(&crate::crawler::grounding_file_name(&task_id), SHOP_GROUNDING);
    // One case for a requirement the shop shows, one for a requirement it does not.
    approved_case(
        &state,
        project,
        &owner,
        case_body_for("FR-002", "Add a product", serde_json::json!(["technique:positive"])),
    )
    .await;
    approved_case(
        &state,
        project,
        &owner,
        case_body_for("FR-005", "Export a report", serde_json::json!([])),
    )
    .await;

    let (status, body) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    std::fs::remove_file(file).ok();
    assert_eq!(status, StatusCode::OK, "body={body}");

    // Which crawl this is, and what it recorded.
    assert_eq!(body["crawl"]["run_id"], run_id);
    assert_eq!(body["crawl"]["source"], "grounding");
    assert_eq!(body["crawl"]["roles"].as_array().unwrap().len(), 2);
    assert!(
        body["notes"][0].as_str().unwrap_or_default().contains("\"admin\" could not log in"),
        "notes={}",
        body["notes"]
    );

    // Each requirement, against the pages.
    assert_eq!(finding_of(&body, "FR-001")["status"], "FOUND", "body={body}");
    assert_eq!(finding_of(&body, "FR-001")["matches"][0]["url"], "https://shop.example.test/");
    assert_eq!(finding_of(&body, "FR-002")["status"], "FOUND");
    assert_eq!(finding_of(&body, "FR-002")["matches"][0]["url"], "https://shop.example.test/inventory.html");
    assert_eq!(finding_of(&body, "FR-003")["status"], "FOUND");
    assert_eq!(finding_of(&body, "FR-003")["matches"][0]["url"], "https://shop.example.test/cart.html");
    assert_eq!(finding_of(&body, "FR-004")["status"], "WEAK");
    assert_eq!(finding_of(&body, "FR-005")["status"], "MISSING");
    assert_eq!(finding_of(&body, "FR-006")["status"], "NOT_CHECKABLE");
    assert_eq!(finding_of(&body, "FR-006")["kind"], "performance");

    // Cases: the two it was given, counted for their requirement, and the techniques still missing.
    let added = &finding_of(&body, "FR-002")["cases"];
    assert_eq!((added["total"].clone(), added["approved"].clone()), (1.into(), 1.into()), "body={body}");
    assert_eq!(added["techniques"]["positive"], 1);
    assert_eq!(added["missing_techniques"], serde_json::json!(["negative", "edge"]));
    assert_eq!(finding_of(&body, "FR-005")["cases"]["total"], 1);
    assert_eq!(finding_of(&body, "FR-003")["cases"]["total"], 0);

    // The page nobody wrote a requirement for, and the counts.
    assert_eq!(body["undocumented_pages"].as_array().unwrap().len(), 1, "body={body}");
    assert_eq!(body["undocumented_pages"][0]["url"], "https://shop.example.test/legal/terms.html");
    let summary = &body["summary"];
    assert_eq!(summary["requirements"], 6);
    assert_eq!(summary["found"], 3);
    assert_eq!(summary["weak"], 1);
    assert_eq!(summary["missing"], 1);
    assert_eq!(summary["not_checkable"], 1);
    assert_eq!(summary["tested_without_evidence"], 1, "FR-005 has a case and no sign in the crawl: {body}");
    assert_eq!(summary["pages"], 4);
}

#[tokio::test]
async fn phase6a_a_role_can_be_chosen_and_an_unknown_one_lists_the_roles() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6A Gap Role").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    let (run_id, task_id) = crawl_run_and_task(&state, project, &owner).await;
    let file = write_crawl_file(&crate::crawler::grounding_file_name(&task_id), SHOP_GROUNDING);

    let (status, only_admin) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": run_id, "role": "ADMIN"})),
    )
    .await;
    let (bad_status, unknown) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": run_id, "role": "auditor"})),
    )
    .await;
    // With no crawl named, the newest one is used, and a role it lacks is the same 400 that lists the roles: not a
    // "no crawl" 404, and not a reason to look for an older crawl.
    let (default_status, default_admin) =
        gap_check(&state, project, &owner, Some(serde_json::json!({"role": "ADMIN"}))).await;
    let (default_bad_status, default_unknown) =
        gap_check(&state, project, &owner, Some(serde_json::json!({"role": "auditor"}))).await;
    std::fs::remove_file(file).ok();

    // The admin's crawl reached one page (the login page) and found nothing else.
    assert_eq!(status, StatusCode::OK, "body={only_admin}");
    assert_eq!(only_admin["crawl"]["role"], "ADMIN");
    assert_eq!(only_admin["summary"]["pages"], 1);
    assert_eq!(finding_of(&only_admin, "FR-002")["status"], "MISSING", "body={only_admin}");
    assert_eq!(bad_status, StatusCode::BAD_REQUEST, "body={unknown}");
    assert!(unknown.to_string().contains("standard, admin"), "body={unknown}");
    assert_eq!(default_status, StatusCode::OK, "body={default_admin}");
    assert_eq!(default_admin["crawl"]["run_id"], run_id);
    assert_eq!(default_admin["crawl"]["role"], "ADMIN");
    assert_eq!(default_bad_status, StatusCode::BAD_REQUEST, "body={default_unknown}");
    assert!(default_unknown.to_string().contains("standard, admin"), "body={default_unknown}");
}

#[tokio::test]
async fn phase6a_an_older_crawl_with_only_a_result_is_read_and_says_so() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6A Gap Older Crawl").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    let (run_id, task_id) = crawl_run_and_task(&state, project, &owner).await;
    let file = write_crawl_file(&format!("crawl-result-{task_id}.json"), SHOP_RESULT);

    let (status, body) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    std::fs::remove_file(file).ok();

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["crawl"]["source"], "result");
    assert!(body["notes"].to_string().contains("form fields"), "notes={}", body["notes"]);
    // The login button is there; the cart page was never crawled, only linked to.
    assert_eq!(finding_of(&body, "FR-001")["status"], "WEAK", "body={body}");
}

#[tokio::test]
async fn phase6a_with_no_crawl_named_the_latest_one_is_used_and_no_crawl_is_a_404() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6A Gap Latest").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;

    // No crawl at all yet.
    let (status, body) = gap_check(&state, project, &owner, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert!(body.to_string().contains("no finished crawl"), "body={body}");

    let (run_id, task_id) = crawl_run_and_task(&state, project, &owner).await;
    // A crawl that left nothing to read is not used either.
    let (status, body) = gap_check(&state, project, &owner, Some(serde_json::json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    // Once it has left its grounding, it is the crawl the check is made against — and the newest of two is the one used.
    let file = write_crawl_file(&crate::crawler::grounding_file_name(&task_id), SHOP_GROUNDING);
    let (newer_run, newer_task) = crawl_run_and_task(&state, project, &owner).await;
    let newer_file = write_crawl_file(&crate::crawler::grounding_file_name(&newer_task), SHOP_GROUNDING);
    let (status, body) = gap_check(&state, project, &owner, None).await;
    // Naming the older one uses the older one.
    let (named_status, named) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    std::fs::remove_file(file).ok();
    std::fs::remove_file(newer_file).ok();
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["crawl"]["run_id"], newer_run);
    assert_ne!(newer_run, run_id);
    assert_eq!(finding_of(&body, "FR-001")["status"], "FOUND");
    assert_eq!(named_status, StatusCode::OK, "body={named}");
    assert_eq!(named["crawl"]["run_id"], run_id);
}

#[tokio::test]
async fn phase6a_the_gap_check_says_what_is_missing_to_run_it() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6A Gap Refusals").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // No requirements yet.
    let (status, body) = gap_check(&state, project, &owner, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(body.to_string().contains("no requirements"), "body={body}");

    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    // Documents that are not this project's contribute no requirements.
    let (status, body) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"document_ids": [Uuid::new_v4()]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    // A crawl run that does not exist.
    let (status, _) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // A run that left no crawl behind (a plain run, or a crawl still going).
    let (run_id, _) = crawl_run_and_task(&state, project, &owner).await;
    let (status, body) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert!(body.to_string().contains("recorded no crawl"), "body={body}");
}

#[tokio::test]
async fn phase6a_a_crawl_that_reached_no_page_is_refused_rather_than_reported_as_all_missing() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6A Gap Empty Crawl").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    let (run_id, task_id) = crawl_run_and_task(&state, project, &owner).await;
    let file = write_crawl_file(
        &crate::crawler::grounding_file_name(&task_id),
        r#"{"roles":[{"role":"admin","loginSucceeded":false,"loginActions":[],"pages":[]}]}"#,
    );

    let (status, body) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    std::fs::remove_file(file).ok();
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(body.to_string().contains("reached no page"), "body={body}");
}

#[tokio::test]
async fn phase6a_the_check_is_project_scoped_and_a_viewer_may_read_it() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project_a = make_project(&state, "Phase6A Gap Owner").await;
    let project_b = make_project(&state, "Phase6A Gap Other").await;
    let owner_a = mint_key(&state, "operator", Some(&project_a.to_string())).await;
    let owner_b = mint_key(&state, "operator", Some(&project_b.to_string())).await;
    let viewer_a = mint_key(&state, "viewer", Some(&project_a.to_string())).await;
    upload_document(&state, &owner_a, project_a, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    upload_document(&state, &owner_b, project_b, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    let (run_id, task_id) = crawl_run_and_task(&state, project_a, &owner_a).await;
    let file = write_crawl_file(&crate::crawler::grounding_file_name(&task_id), SHOP_GROUNDING);

    let (viewer_status, viewer_body) = gap_check(
        &state,
        project_a,
        &viewer_a,
        Some(serde_json::json!({"crawl_run_id": run_id})),
    )
    .await;
    // Another project's crawl cannot be checked against, and another project's key cannot check this one.
    let (borrowed, _) = gap_check(
        &state,
        project_b,
        &owner_b,
        Some(serde_json::json!({"crawl_run_id": run_id})),
    )
    .await;
    let (foreign, _) = gap_check(
        &state,
        project_a,
        &owner_b,
        Some(serde_json::json!({"crawl_run_id": run_id})),
    )
    .await;
    std::fs::remove_file(file).ok();

    assert_eq!(viewer_status, StatusCode::OK, "body={viewer_body}");
    assert_eq!(borrowed, StatusCode::NOT_FOUND);
    assert_eq!(foreign, StatusCode::FORBIDDEN);
}

// ----------------------------------------- Phase 6B — the embedder and the AI, on request
//
// What the optional steps do is proved in `gap_verify.rs`; these prove that the request reaches them, who may ask, and
// what the response says.

/// An embedder whose vectors count how many words of each concept a text has, so what is close in meaning is whatever the
/// test says it is: no network. A text with no concept word is background, with a direction of its own for pages (their
/// text starts with an address) and for requirements, so background never resembles anything.
struct ConceptEmbedder {
    concepts: Vec<Vec<&'static str>>,
}

#[async_trait::async_trait]
impl Embedder for ConceptEmbedder {
    fn is_configured(&self) -> bool {
        true
    }
    fn model_name(&self) -> String {
        "concept-embedder".to_string()
    }
    fn dimensions(&self) -> u32 {
        self.concepts.len() as u32 + 2
    }
    async fn embed(&self, text: &str) -> Result<Vec<f32>, crate::ai_router::ExecutionError> {
        let lower = text.to_lowercase();
        let mut vector: Vec<f32> = self
            .concepts
            .iter()
            .map(|words| words.iter().filter(|w| lower.contains(*w)).count() as f32)
            .collect();
        let hits: f32 = vector.iter().sum();
        vector.push(if hits == 0.0 && text.starts_with('/') { 1.0 } else { 0.0 });
        vector.push(if hits == 0.0 && !text.starts_with('/') { 1.0 } else { 0.0 });
        Ok(vector)
    }
}

/// The shop's crawl with two more pages nobody wrote a requirement for: six pages, enough for a comparison of meaning.
const SIX_PAGE_GROUNDING: &str = r##"{"roles":[
  {"role":"standard","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[
     {"url":"https://shop.example.test/","title":"Shop","elements":[
        {"kind":"input","text":"Username","locator":"[data-test=\"username\"]","inputType":"text"},
        {"kind":"button","text":"Login","locator":"[data-test=\"login-button\"]"}]},
     {"url":"https://shop.example.test/inventory.html","title":"Shop","elements":[
        {"kind":"button","text":"Add to cart","locator":"[data-test=\"add-to-cart-backpack\"]"}]},
     {"url":"https://shop.example.test/cart.html","title":"Shop","elements":[
        {"kind":"button","text":"Checkout","locator":"[data-test=\"checkout\"]"}]},
     {"url":"https://shop.example.test/legal/terms.html","title":"Terms of Service","elements":[
        {"kind":"link","text":"Privacy Policy","locator":"a"}]},
     {"url":"https://shop.example.test/about","title":"About us","elements":[
        {"kind":"link","text":"Contact","locator":"a"}]},
     {"url":"https://shop.example.test/help","title":"Help","elements":[
        {"kind":"link","text":"FAQ","locator":"a"}]}]}]}"##;


fn verdict_answer(items: &[(&str, &str, &[&str], &str)]) -> String {
    let verdicts: Vec<Value> = items
        .iter()
        .map(|(id, verdict, pages, reason)| serde_json::json!({"id": id, "verdict": verdict, "pages": pages, "reason": reason}))
        .collect();
    serde_json::json!({ "verdicts": verdicts }).to_string()
}

/// A project with the shop's requirements, and a crawl of the shop (which cannot really run here) that has left its
/// grounding. Returns the project, an operator's key, the crawl's run id and the file to remove afterwards.
async fn shop_project(state: &AppState, name: &str) -> (Uuid, String, String, std::path::PathBuf) {
    let project = make_project(state, name).await;
    let owner = mint_key(state, "operator", Some(&project.to_string())).await;
    let (status, uploaded) = upload_document(state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    assert_eq!(status, StatusCode::OK, "body={uploaded}");
    let (run_id, task_id) = crawl_run_and_task(state, project, &owner).await;
    let file = write_crawl_file(&crate::crawler::grounding_file_name(&task_id), SHOP_GROUNDING);
    (project, owner, run_id, file)
}

#[tokio::test]
async fn phase6b_with_no_option_no_step_runs_and_the_response_says_so() {
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, run_id, file) = shop_project(&state, "Phase6B No Options").await;

    let (status, body) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    std::fs::remove_file(file).ok();

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["method"]["embeddings"]["requested"], false);
    assert_eq!(body["method"]["verification"]["requested"], false);
    assert_eq!(body["method"]["verification"]["calls"], 0);
    assert!(executor.prompts.lock().unwrap().is_empty(), "no provider is called for the words check");
    assert!(finding_of(&body, "FR-004").get("verification").is_none());
    assert_eq!(finding_of(&body, "FR-004")["status"], "WEAK");
}

#[tokio::test]
async fn phase6b_asking_the_ai_to_check_confirms_or_rejects_and_says_who_said_so() {
    // FR-004 (a wishlist) is weak and FR-005 (a CSV export) missing: the AI is asked about them in that order, and shown
    // the pages with the one the words came nearest to first.
    let answer = verdict_answer(&[
        ("R1", "CONFIRMED", &["P1"], "The product list has a heart button that adds a product to the wishlist."),
        ("R2", "NOT_FOUND", &[], "Nothing exports anything."),
    ]);
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![&answer]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, run_id, file) = shop_project(&state, "Phase6B Verify").await;

    let (status, body) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": run_id, "verify": true})),
    )
    .await;
    std::fs::remove_file(file).ok();

    assert_eq!(status, StatusCode::OK, "body={body}");
    let prompts = executor.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 1, "one call carries both requirements");
    assert!(prompts[0].contains("(FR-004, 3 Shop, functional) Users can add products to a wishlist."), "{}", prompts[0]);
    assert!(prompts[0].contains("(FR-005, 3 Shop, data) The system shall export any report as a CSV file."), "{}", prompts[0]);
    assert!(prompts[0].contains("P1 — /inventory.html"), "the page the words came nearest to is listed first: {}", prompts[0]);
    assert!(!prompts[0].contains("FR-001"), "a requirement the words found is not asked about");

    let wishlist = finding_of(&body, "FR-004");
    assert_eq!(wishlist["status"], "FOUND", "body={body}");
    assert_eq!(wishlist["basis"], "ai");
    assert_eq!(wishlist["matches"][0]["url"], "https://shop.example.test/inventory.html");
    assert_eq!(wishlist["matches"][0]["basis"], "ai");
    assert_eq!(wishlist["verification"]["verdict"], "CONFIRMED");
    assert_eq!(wishlist["verification"]["by"], "mock/mock-model");
    assert_eq!(wishlist["verification"]["reason"], "The product list has a heart button that adds a product to the wishlist.");
    assert_eq!(wishlist["verification"]["pages"], serde_json::json!(["https://shop.example.test/inventory.html"]));
    let export = finding_of(&body, "FR-005");
    assert_eq!(export["status"], "MISSING");
    assert_eq!(export["basis"], "ai");
    assert_eq!(export["verification"]["verdict"], "NOT_FOUND");

    let v = &body["method"]["verification"];
    assert_eq!((v["requested"].clone(), v["calls"].clone(), v["checked"].clone()), (true.into(), 1.into(), 2.into()));
    assert_eq!((v["confirmed"].clone(), v["not_found"].clone()), (1.into(), 1.into()));
    assert_eq!(v["by"], serde_json::json!(["mock/mock-model"]));
    // The counts follow what the AI decided: the wishlist is now found, and nothing is left weak.
    assert_eq!(body["summary"]["found"], 4);
    assert_eq!(body["summary"]["weak"], 0);
    assert_eq!(body["summary"]["missing"], 1);
}

#[tokio::test]
async fn phase6b_a_limit_on_the_questions_is_kept_and_a_provider_that_refuses_fails_the_step_not_the_request() {
    let executor = Arc::new(RecordingExecutor::sequence(vec![Err(crate::ai_router::ExecutionError::NonSuccessStatus(503))]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, run_id, file) = shop_project(&state, "Phase6B Refused").await;

    let (status, body) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": run_id, "verify": true, "max_verifications": 1})),
    )
    .await;
    std::fs::remove_file(file).ok();

    assert_eq!(status, StatusCode::OK, "the check itself is not lost when the provider is: {body}");
    let prompts = executor.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].lines().filter(|l| l.starts_with("[R")).count(), 1, "only one requirement was asked about");
    assert_eq!(body["method"]["verification"]["failed"], 1);
    assert_eq!(body["method"]["verification"]["checked"], 0);
    // What was asked keeps its status, and says the check failed.
    assert_eq!(finding_of(&body, "FR-004")["status"], "WEAK");
    assert_eq!(finding_of(&body, "FR-004")["verification"]["verdict"], "FAILED");
    assert!(finding_of(&body, "FR-005").get("verification").is_none(), "FR-005 was not asked");
}

#[tokio::test]
async fn phase6b_the_options_that_call_a_provider_need_the_operator_role_and_the_free_check_does_not() {
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, run_id, file) = shop_project(&state, "Phase6B Roles").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let ask = |extra: Value| {
        let mut body = serde_json::json!({"crawl_run_id": run_id});
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        Some(body)
    };

    let (verify, _) = gap_check(&state, project, &viewer, ask(serde_json::json!({"verify": true}))).await;
    let (embed, _) = gap_check(&state, project, &viewer, ask(serde_json::json!({"use_embeddings": true}))).await;
    let (free, _) = gap_check(&state, project, &viewer, ask(serde_json::json!({}))).await;
    // An operator may.
    let (operator, _) = gap_check(&state, project, &owner, ask(serde_json::json!({"use_embeddings": true}))).await;
    std::fs::remove_file(file).ok();

    assert_eq!(verify, StatusCode::FORBIDDEN);
    assert_eq!(embed, StatusCode::FORBIDDEN);
    assert_eq!(free, StatusCode::OK);
    assert_eq!(operator, StatusCode::OK);
    assert!(executor.prompts.lock().unwrap().is_empty(), "a refused request calls no provider");
}

#[tokio::test]
async fn phase6b_an_embedder_that_is_not_set_up_is_reported_and_changes_nothing() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(Arc::new(RecordingExecutor::ok_sequence(vec![])))).await;
    let (project, owner, run_id, file) = shop_project(&state, "Phase6B No Embedder").await;

    let (_, free) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    let (status, body) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": run_id, "use_embeddings": true})),
    )
    .await;
    std::fs::remove_file(file).ok();

    assert_eq!(status, StatusCode::OK, "body={body}");
    let e = &body["method"]["embeddings"];
    assert_eq!((e["requested"].clone(), e["used"].clone()), (true.into(), false.into()));
    assert!(e["note"].as_str().unwrap().contains("GEMINI_API_KEY"), "body={body}");
    assert_eq!(body["requirements"], free["requirements"], "nothing changed");
}

#[tokio::test]
async fn phase6b_an_embedder_names_the_page_closest_in_meaning_for_a_requirement_the_words_missed() {
    let embedder: Arc<dyn Embedder> = Arc::new(ConceptEmbedder {
        // The legal page's meaning: what "the small print" is.
        concepts: vec![vec!["terms", "privacy", "small print"], vec!["cart", "checkout"], vec!["username", "password"], vec!["add to cart"]],
    });
    let (state, _dir) =
        test_state_with_ai_router_and_embedder(ai_infra_with_executor(Arc::new(RecordingExecutor::ok_sequence(vec![]))), embedder).await;
    let project = make_project(&state, "Phase6B Embedder").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", b"## 4 Legal\n\nFR-301: Users can read the small print.\n", Some("FRD")).await;
    let (run_id, task_id) = crawl_run_and_task(&state, project, &owner).await;
    let file = write_crawl_file(&crate::crawler::grounding_file_name(&task_id), SIX_PAGE_GROUNDING);

    let (_, words) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    let (status, body) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": run_id, "use_embeddings": true})),
    )
    .await;
    std::fs::remove_file(file).ok();

    assert_eq!(finding_of(&words, "FR-301")["status"], "MISSING", "the words see nothing: {words}");
    assert_eq!(status, StatusCode::OK, "body={body}");
    let small_print = finding_of(&body, "FR-301");
    // Only ever weak: how close is close enough is not something the words can confirm.
    assert_eq!(small_print["status"], "WEAK", "body={body}");
    assert_eq!(small_print["basis"], "embedding");
    assert_eq!(small_print["matches"][0]["url"], "https://shop.example.test/legal/terms.html");
    assert_eq!(small_print["matches"][0]["basis"], "embedding");
    assert_eq!(small_print["matches"][0]["similarity"], 1.0);
    let e = &body["method"]["embeddings"];
    assert_eq!((e["requested"].clone(), e["used"].clone()), (true.into(), true.into()));
    assert_eq!(e["model"], "concept-embedder");
    // Six pages and the one requirement.
    assert_eq!(e["texts"], 7);
    assert_eq!(e["failed"], 0);
    assert_eq!(body["summary"]["weak"], 1);
}

#[tokio::test]
async fn phase6b_asking_the_ai_to_check_is_audited_and_the_free_check_is_not() {
    let answer = verdict_answer(&[("R1", "UNCLEAR", &[], "x"), ("R2", "UNCLEAR", &[], "x")]);
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![&answer]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor)).await;
    let (project, owner, run_id, file) = shop_project(&state, "Phase6B Audit").await;

    let (free, _) = gap_check(&state, project, &owner, Some(serde_json::json!({"crawl_run_id": run_id}))).await;
    let after_free = state.store.list_audit(None, 100, 0).await.expect("list_audit");
    let (asked, _) = gap_check(
        &state,
        project,
        &owner,
        Some(serde_json::json!({"crawl_run_id": run_id, "verify": true})),
    )
    .await;
    let after_verify = state.store.list_audit(None, 100, 0).await.expect("list_audit");
    std::fs::remove_file(file).ok();

    assert_eq!((free, asked), (StatusCode::OK, StatusCode::OK));
    assert!(!after_free.iter().any(|a| a.action == "gap_check.verify"), "the words check calls no provider");
    let project_id = project.to_string();
    assert!(
        after_verify.iter().any(|a| a.action == "gap_check.verify"
            && a.target.as_deref() == Some(run_id.as_str())
            && a.project_id.as_deref() == Some(project_id.as_str())
            && a.outcome == "ok"),
        "asking the AI is an audited action: {after_verify:?}"
    );
}

// ------------------------------------------------- Phase 6C — look for the pages the requirements mention first

async fn crawl_body_with(state: &AppState, project: Uuid, key: &str, extra: Value) -> (StatusCode, Value) {
    let mut body = valid_crawl_body();
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    send(
        state,
        req("POST", &format!("/api/v1/projects/{project}/crawl"), Some(key), Some(body)),
    )
    .await
}

/// The spec a crawl run's one task was given (which the run itself cannot execute here).
async fn crawl_spec_of(state: &AppState, project: Uuid, key: &str, run_id: &str) -> (String, std::path::PathBuf) {
    let (_, run) = send(
        state,
        req("GET", &format!("/api/v1/projects/{project}/executions/{run_id}"), Some(key), None),
    )
    .await;
    let task_id = run["tasks"][0]["id"].as_str().unwrap().to_string();
    let path = crate::crawler::generated_dir().join(format!("crawl-{task_id}.spec.ts"));
    (std::fs::read_to_string(&path).expect("the crawl's generated spec"), path)
}

#[tokio::test]
async fn phase6c_a_crawl_can_ask_to_visit_the_pages_the_requirements_mention_first() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6C Focus").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;

    let (status, started) = crawl_body_with(&state, project, &owner, serde_json::json!({"focus_requirements": true})).await;
    assert_eq!(status, StatusCode::CREATED, "body={started}");
    let (spec, path) = crawl_spec_of(&state, project, &owner, started["run_id"].as_str().unwrap()).await;
    std::fs::remove_file(path).ok();

    let start = spec.find("const PLAN: CrawlPlan = ").unwrap() + "const PLAN: CrawlPlan = ".len();
    let end = spec[start..].find(";\n").unwrap() + start;
    let plan: Value = serde_json::from_str(&spec[start..end]).unwrap();
    let terms: Vec<&str> = plan["focusTerms"].as_array().expect("the plan carries the words").iter().map(|t| t.as_str().unwrap()).collect();
    // What the shop's requirements are about: its cart, its wishlist, its report export.
    for word in ["cart", "wishlist", "report", "product"] {
        assert!(terms.contains(&word), "{word} in {terms:?}");
    }
    // The plan is otherwise the plan that was sent.
    assert_eq!(plan["baseUrl"], "http://127.0.0.1:8080/app");
    assert_eq!(plan["roles"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn phase6c_without_the_option_a_crawl_is_exactly_what_it_was() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6C No Focus").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;

    let (status, started) = crawl_body_with(&state, project, &owner, serde_json::json!({})).await;
    let (off_status, off) = crawl_body_with(&state, project, &owner, serde_json::json!({"focus_requirements": false})).await;
    assert_eq!((status, off_status), (StatusCode::CREATED, StatusCode::CREATED));
    let (a, path_a) = crawl_spec_of(&state, project, &owner, started["run_id"].as_str().unwrap()).await;
    let (b, path_b) = crawl_spec_of(&state, project, &owner, off["run_id"].as_str().unwrap()).await;
    std::fs::remove_file(path_a).ok();
    std::fs::remove_file(path_b).ok();
    assert!(!a.contains("focusTerms") && !b.contains("focusTerms"), "{a}");
}

#[tokio::test]
async fn phase6c_asking_for_the_focus_of_a_project_with_no_requirements_says_what_to_do() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase6C Nothing To Focus On").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = crawl_body_with(&state, project, &owner, serde_json::json!({"focus_requirements": true})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert!(body.to_string().contains("upload a requirements document first"), "body={body}");
    // Nothing was started: the project has no run.
    let (_, runs) = send(&state, req("GET", &format!("/api/v1/projects/{project}/executions"), Some(&owner), None)).await;
    assert_eq!(runs.as_array().unwrap().len(), 0, "{runs}");
}

// ------------------------------------------------- Phase 7A — a role's login steps, suggested from a crawl

/// A crawl of a shop: the login page as `anonymous`, the shop as `standard`. (Its role's name in a test is chosen so the
/// variables the suggestion names are ones no test environment has set.)
const LOGIN_GROUNDING: &str = r##"{"roles":[
  {"role":"anonymous","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[{"url":"https://shop.example.test/","title":"Swag Labs","elements":[
      {"kind":"input","text":"Username","locator":"[data-test=\"username\"]","inputType":"text"},
      {"kind":"input","text":"Password","locator":"[data-test=\"password\"]","inputType":"password"},
      {"kind":"button","text":"Login","locator":"[data-test=\"login-button\"]"}]}]},
  {"role":"standard","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[{"url":"https://shop.example.test/inventory.html","title":"Products","elements":[
      {"kind":"button","text":"Add to cart","locator":"[data-test=\"add\"]"}]}]}]}"##;

/// A login page in another language with two buttons that say nothing: what the rules are unsure of.
const ODD_LOGIN_GROUNDING: &str = r##"{"roles":[
  {"role":"anonymous","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[{"url":"https://odd.example.test/entrar","title":"Acceso","elements":[
      {"kind":"input","text":"Nom","locator":"#nom","inputType":"text"},
      {"kind":"input","text":"Clave","locator":"#clave","inputType":"password"},
      {"kind":"button","text":"Alfa","locator":"#alfa"},
      {"kind":"button","text":"Beta","locator":"#beta"}]}]}]}"##;

/// A crawl that reached pages with no password field on any.
const NO_LOGIN_GROUNDING: &str = r##"{"roles":[
  {"role":"anonymous","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[{"url":"https://shop.example.test/about","title":"About","elements":[
      {"kind":"link","text":"Home","locator":"a.home","href":"https://shop.example.test/"}]}]}]}"##;

/// Two pages with no password field on either.
const NO_LOGIN_TWO_PAGES: &str = r##"{"roles":[
  {"role":"anonymous","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
   "pages":[{"url":"https://shop.example.test/about","title":"About","elements":[]},
            {"url":"https://shop.example.test/help","title":"Help","elements":[]}]}]}"##;

async fn login_suggestion(state: &AppState, project: Uuid, key: &str, body: Value) -> (StatusCode, Value) {
    send(
        state,
        req("POST", &format!("/api/v1/projects/{project}/login-suggestion"), Some(key), Some(body)),
    )
    .await
}

/// A project with a crawl whose grounding is `grounding`: its operator key, the crawl's run id and the file to remove.
async fn crawled_project(state: &AppState, name: &str, grounding: &str) -> (Uuid, String, String, std::path::PathBuf) {
    let project = make_project(state, name).await;
    let owner = mint_key(state, "operator", Some(&project.to_string())).await;
    let (run_id, task_id) = crawl_run_and_task(state, project, &owner).await;
    let file = write_crawl_file(&crate::crawler::grounding_file_name(&task_id), grounding);
    (project, owner, run_id, file)
}

#[tokio::test]
async fn phase7a_a_crawl_with_a_login_page_gives_the_steps_and_the_variables_still_to_set() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let (project, owner, run_id, file) = crawled_project(&state, "Phase7A Login Steps", LOGIN_GROUNDING).await;

    let (status, body) =
        login_suggestion(&state, project, &owner, serde_json::json!({"role": "phase7a-zz", "crawl_run_id": run_id})).await;
    std::fs::remove_file(file).ok();

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["crawl_run_id"], run_id);
    let s = &body["suggestion"];
    assert_eq!(s["confidence"], "high", "body={body}");
    assert_eq!(s["by"], "rules");
    assert_eq!(s["page_url"], "https://shop.example.test/");
    assert_eq!(
        s["actions"],
        serde_json::json!([
            {"type": "NAVIGATE", "url": "https://shop.example.test/"},
            {"type": "FILL", "selector": "[data-test=\"username\"]", "value": "env:AIQA_LOGIN_PHASE7A_ZZ_USERNAME"},
            {"type": "FILL", "selector": "[data-test=\"password\"]", "value": "env:AIQA_LOGIN_PHASE7A_ZZ_PASSWORD"},
            {"type": "CLICK", "selector": "[data-test=\"login-button\"]"}
        ]),
        "body={body}"
    );
    assert_eq!(s["secrets"], serde_json::json!(["AIQA_LOGIN_PHASE7A_ZZ_PASSWORD", "AIQA_LOGIN_PHASE7A_ZZ_USERNAME"]));
    // Not set in this environment, so the person is told to set them (names only).
    assert_eq!(body["missing_secrets"], s["secrets"], "body={body}");
    assert_eq!(body["alternatives"], serde_json::json!([]));
    assert_eq!(body["ai"], serde_json::json!({"requested": false, "used": false, "note": null}));
}

#[tokio::test]
async fn phase7a_with_no_crawl_named_the_newest_crawl_that_has_a_login_page_is_used() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase7A Newest Login Crawl").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    // No crawl at all.
    let (status, body) = login_suggestion(&state, project, &owner, serde_json::json!({"role": "admin"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert!(body.to_string().contains("no crawl with recorded fields"), "body={body}");

    // The older crawl has the login page; the newer one only saw an about page.
    let (older_run, older_task) = crawl_run_and_task(&state, project, &owner).await;
    let older_file = write_crawl_file(&crate::crawler::grounding_file_name(&older_task), LOGIN_GROUNDING);
    let (newer_run, newer_task) = crawl_run_and_task(&state, project, &owner).await;
    let newer_file = write_crawl_file(&crate::crawler::grounding_file_name(&newer_task), NO_LOGIN_GROUNDING);
    let (status, body) = login_suggestion(&state, project, &owner, serde_json::json!({"role": "admin"})).await;
    // Naming the newer crawl says what is wrong with it.
    let (named_status, named) =
        login_suggestion(&state, project, &owner, serde_json::json!({"role": "admin", "crawl_run_id": newer_run})).await;
    std::fs::remove_file(older_file).ok();
    std::fs::remove_file(newer_file).ok();

    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["crawl_run_id"], older_run);
    assert_eq!(body["suggestion"]["confidence"], "high");
    assert_eq!(named_status, StatusCode::BAD_REQUEST, "body={named}");
    assert!(named.to_string().contains("no page of the crawl has a password field"), "body={named}");
}

#[tokio::test]
async fn phase7a_crawls_that_never_reached_a_login_page_say_so() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let (project, owner, _run_id, file) = crawled_project(&state, "Phase7A No Login Page", NO_LOGIN_GROUNDING).await;

    let (status, body) = login_suggestion(&state, project, &owner, serde_json::json!({"role": "admin"})).await;
    std::fs::remove_file(file).ok();

    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    let text = body.to_string();
    assert!(text.contains("no page of the crawl has a password field") && text.contains("1 page seen"), "body={body}");
    assert!(text.contains("crawl the application again as `anonymous`"), "body={body}");
}

#[tokio::test]
async fn phase7a_when_no_crawl_has_a_login_page_the_newest_crawls_problem_is_the_one_reported() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let project = make_project(&state, "Phase7A Newest Problem").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, older_task) = crawl_run_and_task(&state, project, &owner).await;
    let older_file = write_crawl_file(&crate::crawler::grounding_file_name(&older_task), NO_LOGIN_TWO_PAGES);
    let (_, newer_task) = crawl_run_and_task(&state, project, &owner).await;
    let newer_file = write_crawl_file(&crate::crawler::grounding_file_name(&newer_task), NO_LOGIN_GROUNDING);

    let (status, body) = login_suggestion(&state, project, &owner, serde_json::json!({"role": "admin"})).await;
    std::fs::remove_file(older_file).ok();
    std::fs::remove_file(newer_file).ok();

    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    // The newest crawl saw one page; the older one two. The report is about the newest.
    let text = body.to_string();
    assert!(text.contains("(1 page seen") && !text.contains("2 pages seen"), "body={body}");
}

#[tokio::test]
async fn phase7a_the_role_the_crawl_and_the_project_are_checked() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let (project, owner, run_id, file) = crawled_project(&state, "Phase7A Checks", LOGIN_GROUNDING).await;
    let other = make_project(&state, "Phase7A Checks Other").await;
    let other_owner = mint_key(&state, "operator", Some(&other.to_string())).await;
    // A crawl that has left no grounding (still running, or made before grounding was recorded).
    let (bare_run, _) = crawl_run_and_task(&state, project, &owner).await;

    let (empty_status, empty) = login_suggestion(&state, project, &owner, serde_json::json!({"role": "  "})).await;
    let (bad_status, bad) = login_suggestion(&state, project, &owner, serde_json::json!({"role": "!!!"})).await;
    let (foreign_status, foreign) =
        login_suggestion(&state, other, &other_owner, serde_json::json!({"role": "admin", "crawl_run_id": run_id})).await;
    let (bare_status, bare) =
        login_suggestion(&state, project, &owner, serde_json::json!({"role": "admin", "crawl_run_id": bare_run})).await;
    let (scope_status, _) = login_suggestion(&state, project, &other_owner, serde_json::json!({"role": "admin"})).await;
    // A role that cannot name variables is refused before any crawl is looked for.
    let uncrawled = make_project(&state, "Phase7A Checks No Crawl").await;
    let uncrawled_owner = mint_key(&state, "operator", Some(&uncrawled.to_string())).await;
    let (early_status, early) =
        login_suggestion(&state, uncrawled, &uncrawled_owner, serde_json::json!({"role": "!!!"})).await;
    let (missing_status, _) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{project}/login-suggestion"), Some(&owner), Some(serde_json::json!({}))),
    )
    .await;
    std::fs::remove_file(file).ok();

    assert_eq!(empty_status, StatusCode::BAD_REQUEST, "body={empty}");
    assert!(empty.to_string().contains("role must not be empty"), "body={empty}");
    assert_eq!(bad_status, StatusCode::BAD_REQUEST, "body={bad}");
    assert!(bad.to_string().contains("role needs a name"), "body={bad}");
    assert_eq!(early_status, StatusCode::BAD_REQUEST, "the role is checked first: body={early}");
    assert!(early.to_string().contains("role needs a name"), "body={early}");
    assert_eq!(foreign_status, StatusCode::NOT_FOUND, "another project's crawl: body={foreign}");
    assert_eq!(bare_status, StatusCode::NOT_FOUND, "body={bare}");
    assert!(bare.to_string().contains("recorded no grounding"), "body={bare}");
    assert_eq!(scope_status, StatusCode::FORBIDDEN, "a key of another project");
    assert!(missing_status.is_client_error(), "the role is required");
}

#[tokio::test]
async fn phase7a_a_viewer_may_ask_for_the_rules_but_not_for_the_ai() {
    let (state, _dir) = test_state_with_compiled_api().await;
    let (project, owner, run_id, file) = crawled_project(&state, "Phase7A Viewer", LOGIN_GROUNDING).await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;

    let (rules_status, rules) =
        login_suggestion(&state, project, &viewer, serde_json::json!({"role": "admin", "crawl_run_id": run_id})).await;
    let (ai_status, ai) = login_suggestion(
        &state,
        project,
        &viewer,
        serde_json::json!({"role": "admin", "crawl_run_id": run_id, "use_ai": true}),
    )
    .await;
    let (operator_status, _) = login_suggestion(
        &state,
        project,
        &owner,
        serde_json::json!({"role": "admin", "crawl_run_id": run_id, "use_ai": true}),
    )
    .await;
    std::fs::remove_file(file).ok();

    assert_eq!(rules_status, StatusCode::OK, "body={rules}");
    assert_eq!(ai_status, StatusCode::FORBIDDEN, "body={ai}");
    // The default test router has no provider; the operator's request is a normal 200 and the rules were sure anyway.
    assert_eq!(operator_status, StatusCode::OK);
}

#[tokio::test]
async fn phase7a_the_ai_is_asked_only_when_the_rules_are_unsure_and_its_answer_is_checked_and_audited() {
    // The AI is scripted: one answer for the odd page, and none for the plain one (a call there would panic).
    let answer = r#"{"username":"E1","password":"E2","submit":"E4","reason":"Beta is the second, so the submit"}"#;
    let agreeing = r#"{"username":"E1","password":"E2","submit":"E3"}"#;
    let executor = Arc::new(RecordingExecutor::ok_sequence(vec![answer, r#"{"password":"E9"}"#, agreeing]));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor.clone())).await;
    let (project, owner, odd_run, odd_file) = crawled_project(&state, "Phase7A AI", ODD_LOGIN_GROUNDING).await;
    let (plain_run, plain_task) = crawl_run_and_task(&state, project, &owner).await;
    let plain_file = write_crawl_file(&crate::crawler::grounding_file_name(&plain_task), LOGIN_GROUNDING);

    let ask = |run: &str| serde_json::json!({"role": "phase7a-ai", "crawl_run_id": run, "use_ai": true});
    // Sure rules: the AI is not asked.
    let (plain_status, plain) = login_suggestion(&state, project, &owner, ask(&plain_run)).await;
    assert_eq!(executor.prompts.lock().unwrap().len(), 0);
    // Rules with a doubt worth `low` (two buttons that say nothing): the AI is asked, and its answer takes the first place.
    let (odd_status, odd) = login_suggestion(&state, project, &owner, ask(&odd_run)).await;
    // An answer that names an element that is not on the page is not used: the rules stand, and the note says why.
    let (bad_status, bad) = login_suggestion(&state, project, &owner, ask(&odd_run)).await;
    // An answer that is the rules' own adds nothing to list.
    let (same_status, same) = login_suggestion(&state, project, &owner, ask(&odd_run)).await;
    let audit_rows = state.store.list_audit(None, 100, 0).await.expect("list_audit");
    std::fs::remove_file(odd_file).ok();
    std::fs::remove_file(plain_file).ok();

    assert_eq!(plain_status, StatusCode::OK, "body={plain}");
    assert_eq!(plain["ai"]["used"], false);
    assert!(plain["ai"]["note"].as_str().unwrap().contains("rules were sure"), "body={plain}");

    assert_eq!(odd_status, StatusCode::OK, "body={odd}");
    assert_eq!(odd["ai"]["requested"], true);
    assert_eq!(odd["ai"]["used"], true);
    assert_eq!(odd["suggestion"]["by"], "mock/mock-model", "body={odd}");
    assert_eq!(odd["suggestion"]["confidence"], "medium");
    assert_eq!(odd["suggestion"]["actions"][3], serde_json::json!({"type": "CLICK", "selector": "#beta"}));
    assert_eq!(odd["alternatives"].as_array().unwrap().len(), 1, "body={odd}");
    assert_eq!(odd["alternatives"][0]["by"], "rules");
    assert_eq!(odd["alternatives"][0]["actions"][3], serde_json::json!({"type": "CLICK", "selector": "#alfa"}));

    assert_eq!(bad_status, StatusCode::OK, "body={bad}");
    assert_eq!(bad["ai"]["used"], false);
    assert!(bad["ai"]["note"].as_str().unwrap().contains("not one of the elements listed"), "body={bad}");
    assert_eq!(bad["suggestion"]["by"], "rules");
    assert_eq!(bad["alternatives"], serde_json::json!([]));

    assert_eq!(same_status, StatusCode::OK, "body={same}");
    assert_eq!(same["ai"]["used"], true, "the AI was asked and answered");
    assert_eq!(same["suggestion"]["by"], "rules", "body={same}");
    assert_eq!(same["alternatives"], serde_json::json!([]), "the same steps are listed once: body={same}");

    // Every answer that was used is an audited action, against the crawl run; one that was refused is not.
    let asked: Vec<_> = audit_rows.iter().filter(|a| a.action == "login_suggestion.ai").collect();
    assert_eq!(asked.len(), 2, "{audit_rows:?}");
    assert!(asked.iter().all(|a| a.target.as_deref() == Some(odd_run.as_str())));
    assert!(asked.iter().all(|a| a.detail.as_deref() == Some("mock/mock-model")));
    assert_eq!(executor.prompts.lock().unwrap().len(), 3);
}

// ------------------------------------------------- Phase 7B — what an autopilot run would cost

async fn autopilot_estimate(state: &AppState, project: Uuid, key: &str, body: Value) -> (StatusCode, Value) {
    send(
        state,
        req("POST", &format!("/api/v1/projects/{project}/autopilot/estimate"), Some(key), Some(body)),
    )
    .await
}

/// A router whose one provider charges: hosted, $0.002 per 1,000 tokens in and $0.01 out.
fn priced_ai_infra() -> Arc<AiInfrastructure> {
    let mut provider = healthy_provider("paid", vec![healthy_model("big-1", "paid", CapabilityLevel::Strong)]);
    provider.provider_type = crate::ai_router::ProviderType::Api;
    provider.pricing = crate::ai_router::ProviderPricing::Metered {
        input_usd_per_1k: 0.002,
        output_usd_per_1k: 0.01,
    };
    let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> = std::collections::HashMap::new();
    executors.insert("paid", Arc::new(FakeExecutor::ok("{}")));
    Arc::new(AiInfrastructure::new(vec![Arc::new(FakeDiscovery(provider))], executors))
}

fn task_of<'a>(estimate: &'a Value, name: &str) -> &'a Value {
    estimate["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["task"] == name)
        .unwrap_or_else(|| panic!("no {name} in {estimate}"))
}

#[tokio::test]
async fn phase7b_the_estimate_says_calls_tokens_time_and_what_they_rest_on() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase7B Estimate").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;

    let (status, e) = autopilot_estimate(
        &state,
        project,
        &owner,
        serde_json::json!({"depth": "STANDARD", "coverage_top_up": true, "roles": 2, "focus_requirements": true}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body={e}");
    assert_eq!(e["requirements"], 6);
    // Six short requirements fit one batch: one call in a first pass, a quarter more allowed for answers that cannot be read
    // and two top-up rounds of at most as many calls as the first pass.
    let generation = task_of(&e, "TEST_CASE_GENERATION");
    assert_eq!(generation["calls"], serde_json::json!({"min": 1, "max": 4}), "body={e}");
    assert_eq!(generation["provider"]["id"], "mock");
    assert_eq!(generation["provider"]["model"], "mock-model");
    assert_eq!(generation["provider"]["kind"], "LOCAL");
    assert_eq!(generation["provider"]["pricing"]["kind"], "UNMETERED");
    assert_eq!(generation["cost_usd"], serde_json::json!({"min": 0.0, "max": 0.0}));
    assert_eq!(generation["latency"]["basis"], "ASSUMED", "nothing measured yet");
    assert!(generation["seconds"]["max"].as_u64().unwrap() > generation["seconds"]["min"].as_u64().unwrap());
    // What the depth asks for is more cases than requirements; at least one each.
    assert_eq!(e["cases"]["min"], 6);
    assert!(e["cases"]["max"].as_u64().unwrap() > 6, "body={e}");
    // Every step, in the order the run takes them; the wait for an approval is not counted.
    let steps: Vec<&str> = e["steps"].as_array().unwrap().iter().map(|s| s["step"].as_str().unwrap()).collect();
    assert_eq!(steps, ["REQUIREMENTS", "CRAWL", "GAP_CHECK", "GENERATE", "VALIDATE", "APPROVE", "EXECUTE", "REPORT"]);
    let crawl = e["steps"].as_array().unwrap().iter().find(|s| s["step"] == "CRAWL").unwrap();
    assert_eq!(crawl["note"], "2 role(s), up to 30 pages each", "a focused crawl may take ten more pages");
    assert_eq!(e["totals"]["paid"], false);
    assert!(e["assumptions"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("mock/mock-model is assumed to write 20 tokens a second")), "body={e}");
    // Compiling is counted (rules first: none to one AI call a case); the gap check's AI step is not asked for.
    assert_eq!(task_of(&e, "TEST_CASE_COMPILATION")["calls"]["min"], 0);
    assert!(e["tasks"].as_array().unwrap().iter().all(|t| t["task"] != "TRACEABILITY"));
}

#[tokio::test]
async fn phase7b_a_generation_is_remembered_and_the_estimate_after_enough_of_them_uses_what_they_took() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase7B Measured").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;

    // A real generation through the endpoint records its call: one wave, one call.
    let (status, generated) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can log in.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={generated}");
    let stats = state.store.ai_call_stats("TEST_CASE_GENERATION").await.unwrap();
    assert_eq!(stats.len(), 1, "{stats:?}");
    assert_eq!((stats[0].provider.as_str(), stats[0].model.as_str(), stats[0].calls), ("mock", "mock-model", 1));
    // Only generation is recorded here.
    assert!(state.store.ai_call_stats("TEST_CASE_COMPILATION").await.unwrap().is_empty());

    // One call is not an average.
    let (_, first) = autopilot_estimate(&state, project, &owner, serde_json::json!({})).await;
    assert_eq!(task_of(&first, "TEST_CASE_GENERATION")["latency"]["basis"], "ASSUMED", "body={first}");

    // Two more, slow ones: three calls make an average, and the estimate says it is measured.
    state.store.record_ai_call("mock", "mock-model", "TEST_CASE_GENERATION", 80_000, Some(4_000), Some(1_000)).await.unwrap();
    state.store.record_ai_call("mock", "mock-model", "TEST_CASE_GENERATION", 80_000, Some(4_000), Some(1_000)).await.unwrap();
    let (_, second) = autopilot_estimate(&state, project, &owner, serde_json::json!({})).await;
    let latency = &task_of(&second, "TEST_CASE_GENERATION")["latency"];
    assert_eq!(latency["basis"], "MEASURED", "body={second}");
    assert_eq!(latency["calls"], 3);
    let avg = latency["avg_seconds"].as_f64().unwrap();
    assert!((53.0..54.0).contains(&avg), "two 80 s calls and a quick one average about 53 s: {avg}");
    // The seconds are that average, spread.
    let seconds = &task_of(&second, "TEST_CASE_GENERATION")["seconds"];
    assert!(seconds["min"].as_u64().unwrap() > 30 && seconds["max"].as_u64().unwrap() > 90, "body={second}");
}

#[tokio::test]
async fn phase7b_a_provider_that_charges_makes_the_estimate_a_paid_one_and_says_how_much() {
    let (state, _dir) = test_state_with_ai_router(priced_ai_infra()).await;
    let project = make_project(&state, "Phase7B Paid").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;

    let (status, e) = autopilot_estimate(&state, project, &owner, serde_json::json!({"execute": false})).await;

    assert_eq!(status, StatusCode::OK, "body={e}");
    let generation = task_of(&e, "TEST_CASE_GENERATION");
    assert_eq!(generation["provider"]["kind"], "API");
    assert_eq!(
        generation["provider"]["pricing"],
        serde_json::json!({"kind": "METERED", "input_usd_per_1k": 0.002, "output_usd_per_1k": 0.01})
    );
    assert!(generation["cost_usd"]["max"].as_f64().unwrap() > 0.0, "body={e}");
    assert_eq!(e["totals"]["paid"], true);
    assert!(e["totals"]["cost_usd"]["max"].as_f64().unwrap() > 0.0);
    assert!(e["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("may cost up to $") && w.as_str().unwrap().contains("on paid")), "body={e}");
    // Not asked to run the cases: nothing is compiled.
    assert!(e["tasks"].as_array().unwrap().iter().all(|t| t["task"] != "TEST_CASE_COMPILATION"), "body={e}");
}

#[tokio::test]
async fn phase7b_with_no_provider_the_estimate_gives_no_cost_and_a_warning_not_a_zero() {
    let (state, _dir) = test_state_with_ai_router(Arc::new(AiInfrastructure::new(vec![], std::collections::HashMap::new()))).await;
    let project = make_project(&state, "Phase7B No Provider").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;

    let (status, e) = autopilot_estimate(&state, project, &owner, serde_json::json!({})).await;

    assert_eq!(status, StatusCode::OK, "body={e}");
    let generation = task_of(&e, "TEST_CASE_GENERATION");
    assert!(generation["provider"].is_null());
    assert!(generation["cost_usd"].is_null(), "no provider: no cost figure");
    assert_eq!(generation["latency"]["basis"], "UNKNOWN");
    assert!(e["totals"]["cost_usd"].is_null(), "body={e}");
    assert!(e["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("No provider can serve generating the cases")), "body={e}");
}

#[tokio::test]
async fn phase7b_the_documents_chosen_and_the_skills_selected_decide_what_is_counted() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase7B Choices").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (_, shop) = upload_document(&state, &owner, project, "Shop.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    let (_, sample) = upload_document(&state, &owner, project, "Sample.md", SAMPLE_FRD.as_bytes(), Some("PRD")).await;
    let shop_id = shop["requirements_document"]["document_id"].as_str().unwrap().to_string();
    let sample_id = sample["requirements_document"]["document_id"].as_str().unwrap().to_string();

    let (_, all) = autopilot_estimate(&state, project, &owner, serde_json::json!({})).await;
    let (_, only_sample) = autopilot_estimate(&state, project, &owner, serde_json::json!({"requirement_document_ids": [sample_id]})).await;
    let (_, only_shop) = autopilot_estimate(&state, project, &owner, serde_json::json!({"requirement_document_ids": [shop_id]})).await;
    assert_eq!(all["requirements"], 9);
    assert_eq!(only_shop["requirements"], 6);
    assert_eq!(only_sample["requirements"], 3);

    // A skill that tests only performance requirements is offered one of the shop's six: the other five would get no case,
    // and the estimate says so.
    let (_, one_offered) = autopilot_estimate(
        &state,
        project,
        &owner,
        serde_json::json!({"requirement_document_ids": [shop_id], "skills": [{"id": "skill-performance", "name": "Performance Testing"}]}),
    )
    .await;
    assert_eq!(task_of(&one_offered, "TEST_CASE_GENERATION")["calls"]["min"], 1, "body={one_offered}");
    assert!(
        one_offered["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().starts_with("5 requirement(s) are of a kind no selected skill tests")),
        "body={one_offered}"
    );

    // The gap check's AI step is counted when asked for: at most 12 requirements, eight to a call.
    let (_, verify) = autopilot_estimate(&state, project, &owner, serde_json::json!({"verify_gaps": 12})).await;
    assert_eq!(task_of(&verify, "TRACEABILITY")["calls"], serde_json::json!({"min": 0, "max": 2}), "body={verify}");
}

#[tokio::test]
async fn phase7b_a_project_with_no_requirements_gets_an_estimate_that_says_so() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase7B Nothing").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, e) = autopilot_estimate(&state, project, &owner, serde_json::json!({"depth": "STANDARD"})).await;

    assert_eq!(status, StatusCode::OK, "body={e}");
    assert_eq!(e["requirements"], 0);
    assert!(e["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("no requirements")), "body={e}");
    assert!(e["tasks"].as_array().unwrap().iter().all(|t| t["task"] != "TEST_CASE_GENERATION"));
}

#[tokio::test]
async fn phase7b_what_is_left_out_of_the_request_takes_its_defaults_and_the_figures_follow_what_was_asked_for() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase7B Defaults").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    let step_of = |e: &Value, name: &str| e["steps"].as_array().unwrap().iter().find(|s| s["step"] == name).unwrap().clone();
    let has = |e: &Value, text: &str| e["assumptions"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains(text));

    // Nothing said: one role, two runs at a time.
    let (_, plain) = autopilot_estimate(&state, project, &owner, serde_json::json!({})).await;
    assert_eq!(step_of(&plain, "CRAWL")["note"], "1 role(s), up to 20 pages each");
    assert!(has(&plain, "Running: 10-45 s a case, 2 at a time;"), "body={plain}");
    // One requirement no skill is chosen for tests: named as such, and it is one, not "more than one".
    assert!(
        plain["warnings"].as_array().unwrap().iter().any(|w| w == "1 requirement(s) are of a kind no selected skill tests, so no case would be generated for them."),
        "body={plain}"
    );

    // Asked for: the number of runs at a time reaches the figures.
    let (_, four) = autopilot_estimate(&state, project, &owner, serde_json::json!({"depth": "STANDARD", "run_concurrency": 4})).await;
    assert!(has(&four, "Running: 10-45 s a case, 4 at a time;"), "body={four}");
    let (_, one) = autopilot_estimate(&state, project, &owner, serde_json::json!({"depth": "STANDARD", "run_concurrency": 1})).await;
    let seconds = |e: &Value| step_of(e, "EXECUTE")["seconds"]["max"].as_u64().unwrap();
    assert!(seconds(&one) > seconds(&four), "one at a time takes longer: {} against {}", seconds(&one), seconds(&four));

    // A depth with no top-up makes no gap-filling calls; with one, two rounds of them.
    let (_, without) = autopilot_estimate(&state, project, &owner, serde_json::json!({"depth": "STANDARD", "coverage_top_up": false})).await;
    assert_eq!(task_of(&without, "TEST_CASE_GENERATION")["calls"], serde_json::json!({"min": 1, "max": 2}), "body={without}");
    assert!(!has(&without, "gap-filling"), "body={without}");
    let (_, with) = autopilot_estimate(&state, project, &owner, serde_json::json!({"depth": "STANDARD", "coverage_top_up": true})).await;
    assert_eq!(task_of(&with, "TEST_CASE_GENERATION")["calls"], serde_json::json!({"min": 1, "max": 4}), "body={with}");
    assert!(has(&with, "up to 2 round(s) of gap-filling calls"), "body={with}");

    // The cases a depth asks for are, for each technique, its requirements times what it wants of each.
    assert_eq!(without["cases"]["min"], 6);
    assert_eq!(without["cases"]["max"], 15, "body={without}");
    // A deeper run wants more of each: the same techniques, times what the depth asks of each.
    let (_, thorough) = autopilot_estimate(&state, project, &owner, serde_json::json!({"depth": "EXHAUSTIVE"})).await;
    assert_eq!(thorough["cases"]["max"], 30, "twice what a standard run asks of each: body={thorough}");
}

/// Two providers, `alpha` (local, unmetered) and `beta` (charges, of the kind given), to tell which task goes where.
fn two_provider_infra(beta_kind: crate::ai_router::ProviderType) -> Arc<AiInfrastructure> {
    let alpha = healthy_provider("alpha", vec![healthy_model("alpha-model", "alpha", CapabilityLevel::Strong)]);
    let mut beta = healthy_provider("beta", vec![healthy_model("beta-model", "beta", CapabilityLevel::Strong)]);
    beta.provider_type = beta_kind;
    beta.pricing = crate::ai_router::ProviderPricing::Metered { input_usd_per_1k: 0.002, output_usd_per_1k: 0.01 };
    let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> = std::collections::HashMap::new();
    executors.insert("alpha", Arc::new(FakeExecutor::ok("{}")));
    executors.insert("beta", Arc::new(FakeExecutor::ok("{}")));
    Arc::new(AiInfrastructure::new(vec![Arc::new(FakeDiscovery(alpha)), Arc::new(FakeDiscovery(beta))], executors))
}

/// Sends generation to `alpha` and compiling to `beta`.
async fn route_generation_to_alpha_and_compiling_to_beta(state: &AppState) {
    let operator = mint_key(state, "operator", None).await;
    let (status, body) = send(
        state,
        req(
            "PUT",
            "/api/v1/ai/settings",
            Some(&operator),
            Some(serde_json::json!({
                "mode": "AUTO",
                "fallback_enabled": true,
                "task_overrides": {
                    "TEST_CASE_GENERATION": {"provider": null, "model": null, "chain": [{"provider": "alpha", "model": "alpha-model"}]},
                    "TEST_CASE_COMPILATION": {"provider": null, "model": null, "chain": [{"provider": "beta", "model": "beta-model"}]}
                }
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn phase7b_each_task_is_estimated_for_the_provider_it_goes_to_with_the_measurements_of_that_task_provider_and_model() {
    let (state, _dir) = test_state_with_ai_router(two_provider_infra(crate::ai_router::ProviderType::Api)).await;
    let project = make_project(&state, "Phase7B Two Providers").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    route_generation_to_alpha_and_compiling_to_beta(&state).await;
    let record = |provider: &'static str, model: &'static str, task: &'static str, ms: u64, tokens: Option<(u32, u32)>| {
        let store = state.store.clone();
        async move {
            store.record_ai_call(provider, model, task, ms, tokens.map(|t| t.0), tokens.map(|t| t.1)).await.unwrap();
        }
    };
    // Generation on alpha took a minute a call (two of the three said how many tokens); compiling on beta ten seconds (none did).
    record("alpha", "alpha-model", "TEST_CASE_GENERATION", 60_000, Some((1_000, 400))).await;
    record("alpha", "alpha-model", "TEST_CASE_GENERATION", 60_000, Some((2_000, 600))).await;
    record("alpha", "alpha-model", "TEST_CASE_GENERATION", 60_000, None).await;
    for _ in 0..3 {
        record("beta", "beta-model", "TEST_CASE_COMPILATION", 10_000, None).await;
    }
    // What is measured of another model of the same provider, or of another provider's model of the same name, is not this one's.
    for _ in 0..3 {
        // (Named to sort before the right ones, so a lookup that ignored the difference would find them first.)
        record("alpha", "aaa-model", "TEST_CASE_GENERATION", 5_000, Some((9, 9))).await;
        record("aaa-provider", "alpha-model", "TEST_CASE_GENERATION", 7_000, Some((9, 9))).await;
        // Nor is what is measured of a task another task's.
        record("beta", "beta-model", "TEST_CASE_GENERATION", 1_000, Some((9, 9))).await;
    }

    let (status, e) = autopilot_estimate(&state, project, &owner, serde_json::json!({"depth": "STANDARD"})).await;
    assert_eq!(status, StatusCode::OK, "body={e}");
    let generation = task_of(&e, "TEST_CASE_GENERATION");
    let compile = task_of(&e, "TEST_CASE_COMPILATION");
    assert_eq!((generation["provider"]["id"].as_str(), compile["provider"]["id"].as_str()), (Some("alpha"), Some("beta")), "each task to its own provider: body={e}");
    assert_eq!(generation["latency"], serde_json::json!({"basis": "MEASURED", "calls": 3, "avg_seconds": 60.0}), "body={e}");
    assert_eq!(compile["latency"], serde_json::json!({"basis": "MEASURED", "calls": 3, "avg_seconds": 10.0}), "body={e}");
    // The plan's one to two calls, each carrying what the measured calls carried: 3,000 in and 1,000 out over two calls.
    assert_eq!(generation["calls"], serde_json::json!({"min": 1, "max": 2}));
    assert_eq!(generation["prompt_tokens"], serde_json::json!({"min": 1500, "max": 3000}), "body={e}");
    assert_eq!(generation["completion_tokens"], serde_json::json!({"min": 500, "max": 1000}), "body={e}");
    // Compiling has no token figures measured: the plan's own are kept, not a division of nothing by nothing.
    let compile_calls_max = compile["calls"]["max"].as_u64().unwrap();
    assert_eq!(compile["prompt_tokens"]["max"].as_u64().unwrap(), compile_calls_max * crate::autopilot_estimate::COMPILE_TOKENS.0, "body={e}");
    assert_eq!(compile["completion_tokens"]["max"].as_u64().unwrap(), compile_calls_max * crate::autopilot_estimate::COMPILE_TOKENS.1);
    // Only compiling is on the provider that charges.
    assert_eq!(generation["cost_usd"], serde_json::json!({"min": 0.0, "max": 0.0}));
    assert!(compile["cost_usd"]["max"].as_f64().unwrap() > 0.0);
    assert_eq!(e["totals"]["paid"], true);
}

#[tokio::test]
async fn phase7b_another_endpoint_is_assumed_slower_than_a_hosted_service_and_faster_than_a_home_machine() {
    let (state, _dir) = test_state_with_ai_router(two_provider_infra(crate::ai_router::ProviderType::OpenAiCompatible)).await;
    let project = make_project(&state, "Phase7B Other Endpoint").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    upload_document(&state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    route_generation_to_alpha_and_compiling_to_beta(&state).await;
    let (_, e) = autopilot_estimate(&state, project, &owner, serde_json::json!({})).await;
    assert_eq!(task_of(&e, "TEST_CASE_GENERATION")["latency"], serde_json::json!({"basis": "ASSUMED", "tokens_per_second": 20.0}), "a local one: body={e}");
    assert_eq!(task_of(&e, "TEST_CASE_COMPILATION")["latency"], serde_json::json!({"basis": "ASSUMED", "tokens_per_second": 40.0}), "another endpoint: body={e}");
    assert_eq!(task_of(&e, "TEST_CASE_COMPILATION")["provider"]["kind"], "OTHER");
}

#[tokio::test]
async fn phase7b_the_tokens_a_call_reports_are_remembered_with_its_time() {
    let executor = Arc::new(FakeExecutor::ok_with_usage(valid_ai_response(), 120, 30));
    let (state, _dir) = test_state_with_ai_router(ai_infra_with_executor(executor)).await;
    let project = make_project(&state, "Phase7B Tokens Remembered").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let (status, generated) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{project}/test-cases/generate-ai"),
            Some(&owner),
            Some(serde_json::json!({"requirement_text": "Users can log in.", "skills": []})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={generated}");
    let stats = state.store.ai_call_stats("TEST_CASE_GENERATION").await.unwrap();
    assert_eq!(stats.len(), 1, "{stats:?}");
    assert_eq!((stats[0].calls, stats[0].token_calls), (1, 1));
    assert_eq!((stats[0].prompt_tokens, stats[0].completion_tokens), (120, 30), "{stats:?}");
}

#[tokio::test]
async fn phase7b_the_request_is_checked_and_needs_an_operator_of_the_project() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let project = make_project(&state, "Phase7B Checks").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let stranger = mint_key(&state, "operator", Some(&make_project(&state, "Phase7B Other").await.to_string())).await;

    for (what, body) in [
        ("no roles", serde_json::json!({"roles": 0})),
        ("too many roles", serde_json::json!({"roles": 21})),
        ("no concurrency", serde_json::json!({"run_concurrency": 0})),
        ("too much concurrency", serde_json::json!({"run_concurrency": 9})),
        ("no verifications", serde_json::json!({"verify_gaps": 0})),
        ("too many verifications", serde_json::json!({"verify_gaps": 61})),
        ("an unknown depth", serde_json::json!({"depth": "EXTREME"})),
        ("a top-up with no depth", serde_json::json!({"coverage_top_up": true})),
    ] {
        let (status, body) = autopilot_estimate(&state, project, &owner, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: body={body}");
    }
    for ok in [serde_json::json!({"roles": 20}), serde_json::json!({"run_concurrency": 8}), serde_json::json!({"verify_gaps": 60})] {
        let (status, body) = autopilot_estimate(&state, project, &owner, ok).await;
        assert_eq!(status, StatusCode::OK, "body={body}");
    }
    let (viewer_status, _) = autopilot_estimate(&state, project, &viewer, serde_json::json!({})).await;
    let (stranger_status, _) = autopilot_estimate(&state, project, &stranger, serde_json::json!({})).await;
    assert_eq!(viewer_status, StatusCode::FORBIDDEN);
    assert_eq!(stranger_status, StatusCode::FORBIDDEN);
}

// ------------------------------------------------- Phase 7C — an autopilot run: made, accepted, started, watched

fn fast_autopilot(state: &AppState) {
    state.autopilot.set_options(crate::autopilot_runner::RunnerOptions {
        poll_every: std::time::Duration::from_millis(10),
        crawl_timeout: std::time::Duration::from_secs(20),
    });
}

async fn autopilot(state: &AppState, method: &str, path: &str, key: &str, body: Option<Value>) -> (StatusCode, Value) {
    send(state, req(method, path, Some(key), body)).await
}

fn autopilot_body(extra: Value) -> Value {
    let mut body = serde_json::json!({
        "base_url": "http://127.0.0.1:8080/app",
        "roles": [{"name": "anonymous"}],
    });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        body[key] = value;
    }
    body
}

/// A project with the shop's requirements document and its operator key.
async fn autopilot_project(state: &AppState, name: &str) -> (Uuid, String) {
    let project = make_project(state, name).await;
    let owner = mint_key(state, "operator", Some(&project.to_string())).await;
    let (status, uploaded) = upload_document(state, &owner, project, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
    assert_eq!(status, StatusCode::OK, "body={uploaded}");
    (project, owner)
}

/// A crawl run that has finished and left the grounding a crawl leaves: what a run can be told to use instead of crawling.
async fn finished_crawl(state: &AppState, project: Uuid, key: &str, grounding: Option<&str>) -> (String, Option<std::path::PathBuf>) {
    let (run_id, task_id) = crawl_run_and_task(state, project, key).await;
    let file = grounding.map(|g| write_crawl_file(&crate::crawler::grounding_file_name(&task_id), g));
    let id = Uuid::parse_str(&run_id).unwrap();
    // The crawl cannot really run here: wait until the database says the run has ended however it ended (the engine writes
    // that last, so a change made before it would be overwritten), then make it what a crawl that worked is.
    for _ in 0..400 {
        let stored = state.store.get_run_summary(id).await.unwrap();
        if stored.is_some_and(|s| s.state.is_terminal()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    state.store.set_run_finished(id, crate::domain::RunState::Completed, None).await.unwrap();
    state.runs.write().await.remove(&id);
    (run_id, file)
}

async fn wait_for_autopilot(state: &AppState, project: Uuid, key: &str, run: &str, wanted: &[&str]) -> Value {
    let mut last = Value::Null;
    for _ in 0..600 {
        let (status, detail) = autopilot(state, "GET", &format!("/api/v1/projects/{project}/autopilot/runs/{run}"), key, None).await;
        assert_eq!(status, StatusCode::OK, "body={detail}");
        if wanted.contains(&detail["run"]["state"].as_str().unwrap_or("")) {
            return detail;
        }
        last = detail;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the run never reached one of {wanted:?}: {last}");
}

fn steps_of(detail: &Value) -> Vec<(String, String)> {
    detail["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["name"].as_str().unwrap().to_string(), s["state"].as_str().unwrap().to_string()))
        .collect()
}

#[tokio::test]
async fn phase7c_a_run_is_made_with_its_estimate_and_nothing_starts() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let (project, owner) = autopilot_project(&state, "Phase7C Make").await;
    let (status, made) = autopilot(
        &state,
        "POST",
        &format!("/api/v1/projects/{project}/autopilot/runs"),
        &owner,
        Some(autopilot_body(serde_json::json!({"name": "  Release 4  ", "depth": "STANDARD"}))),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "body={made}");
    let run = &made["run"];
    assert_eq!(run["state"], "AWAITING_APPROVAL");
    assert_eq!(run["name"], "Release 4");
    assert_eq!(run["project_id"], project.to_string());
    assert_eq!(run["created_by_name"], "test-key");
    assert!(run.get("created_by_key").is_none(), "the key's id is not shown: {run}");
    assert_eq!(run["config"]["base_url"], "http://127.0.0.1:8080/app");
    assert_eq!(run["config"]["depth"], "STANDARD");
    assert_eq!(run["estimate"]["requirements"], 6, "the estimate is stored with the run: {run}");
    assert!(run["estimate"]["totals"]["ai_calls"].is_object());
    assert!(run["approved_at"].is_null() && run["started_at"].is_null());
    assert_eq!(steps_of(&made), [("REQUIREMENTS", "PENDING"), ("CRAWL", "PENDING"), ("GAP_CHECK", "PENDING")].map(|(a, b)| (a.to_string(), b.to_string())));
    assert_eq!(made["events"][0]["kind"], "run.created");
    assert_eq!(made["working"], false);

    // Nothing has been started: no crawl run exists, and the run is still awaiting approval a moment later.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let (_, runs) = autopilot(&state, "GET", &format!("/api/v1/projects/{project}/executions"), &owner, None).await;
    assert_eq!(runs.as_array().unwrap().len(), 0, "{runs}");
    let (_, again) = autopilot(&state, "GET", &format!("/api/v1/projects/{project}/autopilot/runs/{}", run["id"].as_str().unwrap()), &owner, None).await;
    assert_eq!(again["run"]["state"], "AWAITING_APPROVAL");

    // The run appears in the project's list, newest first, and its creation is an audited action.
    let (_, list) = autopilot(&state, "GET", &format!("/api/v1/projects/{project}/autopilot/runs"), &owner, None).await;
    assert_eq!(list["runs"].as_array().unwrap().len(), 1);
    assert_eq!(list["runs"][0]["id"], run["id"]);
    let audit_rows = state.store.list_audit(None, 100, 0).await.unwrap();
    let created: Vec<_> = audit_rows.iter().filter(|a| a.action == "autopilot.create").collect();
    assert_eq!(created.len(), 1, "{audit_rows:?}");
    assert_eq!(created[0].target.as_deref(), run["id"].as_str());
    assert_eq!(created[0].detail.as_deref(), Some("Release 4"));
}

#[tokio::test]
async fn phase7c_the_estimate_a_run_is_made_with_is_the_one_the_estimate_route_gives() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let (project, owner) = autopilot_project(&state, "Phase7C Same Estimate").await;
    let (_, from_route) = autopilot_estimate(
        &state,
        project,
        &owner,
        serde_json::json!({"depth": "STANDARD", "coverage_top_up": true, "roles": 2, "focus_requirements": true, "verify_gaps": 5, "run_concurrency": 4}),
    )
    .await;
    let (status, made) = autopilot(
        &state,
        "POST",
        &format!("/api/v1/projects/{project}/autopilot/runs"),
        &owner,
        Some(autopilot_body(serde_json::json!({
            "depth": "STANDARD", "coverage_top_up": true, "focus_requirements": true, "verify_gaps": 5, "run_concurrency": 4,
            "roles": [{"name": "anonymous"}, {"name": "second"}],
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={made}");
    assert_eq!(made["run"]["estimate"], from_route, "the same estimate, whichever way it was asked for");
}

#[tokio::test]
async fn phase7c_a_run_that_could_only_fail_is_refused_when_it_is_made() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let (project, owner) = autopilot_project(&state, "Phase7C Refused").await;
    let empty = make_project(&state, "Phase7C Refused Empty").await;
    let empty_owner = mint_key(&state, "operator", Some(&empty.to_string())).await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let make = |body: Value| autopilot(&state, "POST", &path, &owner, Some(body));

    let (a, no_address) = make(autopilot_body(serde_json::json!({"base_url": "  "}))).await;
    let (b, private_address) = make(autopilot_body(serde_json::json!({"base_url": "http://169.254.169.254/"}))).await;
    let (c, no_roles) = make(autopilot_body(serde_json::json!({"roles": []}))).await;
    let (d, secrets) = make(autopilot_body(serde_json::json!({"roles": [
        {"name": "admin", "login_actions": [{"type": "FILL", "selector": "#u", "value": "env:AIQA_LOGIN_P7C_NEVER_SET_USERNAME"}]}
    ]}))).await;
    let (e, too_many) = make(autopilot_body(serde_json::json!({"run_concurrency": 9}))).await;
    let (f, bad_depth) = make(autopilot_body(serde_json::json!({"depth": "EXTREME"}))).await;
    let (g, no_requirements) = autopilot(
        &state,
        "POST",
        &format!("/api/v1/projects/{empty}/autopilot/runs"),
        &empty_owner,
        Some(autopilot_body(serde_json::json!({}))),
    )
    .await;
    let (h, chosen_none) = make(autopilot_body(serde_json::json!({"requirement_document_ids": [Uuid::new_v4()]}))).await;
    let (i, not_a_crawl) = {
        // A run of the project that is not a crawl: any other run will do.
        let (_, plain) = send(&state, req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(simple_plan("not a crawl")))).await;
        let plain_id = plain["run_id"].as_str().unwrap().to_string();
        make(autopilot_body(serde_json::json!({"crawl_run_id": plain_id}))).await
    };
    let (j, unknown_crawl) = make(autopilot_body(serde_json::json!({"crawl_run_id": Uuid::new_v4()}))).await;

    assert_eq!(a, StatusCode::BAD_REQUEST, "body={no_address}");
    assert!(no_address.to_string().contains("base_url"), "body={no_address}");
    assert_eq!(b, StatusCode::BAD_REQUEST, "an address the guard refuses: body={private_address}");
    assert_eq!(c, StatusCode::BAD_REQUEST, "body={no_roles}");
    assert!(no_roles.to_string().contains("needs at least one role"), "body={no_roles}");
    assert_eq!(d, StatusCode::BAD_REQUEST, "body={secrets}");
    assert!(secrets.to_string().contains("AIQA_LOGIN_P7C_NEVER_SET_USERNAME"), "the variable is named: body={secrets}");
    assert_eq!(e, StatusCode::BAD_REQUEST, "body={too_many}");
    assert!(too_many.to_string().contains("run_concurrency must be between 1 and 8"), "body={too_many}");
    assert_eq!(f, StatusCode::BAD_REQUEST, "body={bad_depth}");
    assert_eq!(g, StatusCode::BAD_REQUEST, "body={no_requirements}");
    assert!(no_requirements.to_string().contains("upload a requirements document"), "body={no_requirements}");
    assert_eq!(h, StatusCode::BAD_REQUEST, "body={chosen_none}");
    assert!(chosen_none.to_string().contains("documents you chose have no requirements"), "body={chosen_none}");
    assert_eq!(i, StatusCode::BAD_REQUEST, "body={not_a_crawl}");
    assert!(not_a_crawl.to_string().contains("is not a crawl"), "body={not_a_crawl}");
    assert_eq!(j, StatusCode::NOT_FOUND, "body={unknown_crawl}");

    // Not one of them made a run.
    let (_, list) = autopilot(&state, "GET", &path, &owner, None).await;
    assert_eq!(list["runs"], serde_json::json!([]));
}

#[tokio::test]
async fn phase7c_a_project_has_one_run_that_has_not_ended() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let (project, owner) = autopilot_project(&state, "Phase7C One At A Time").await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (first_status, first) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"name": "First"})))).await;
    let (second_status, second) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"name": "Second"})))).await;
    assert_eq!(first_status, StatusCode::CREATED, "body={first}");
    assert_eq!(second_status, StatusCode::CONFLICT, "body={second}");
    let message = second.to_string();
    assert!(message.contains("First") && message.contains(first["run"]["id"].as_str().unwrap()), "the run in the way is named: {message}");

    // Once it has ended, the project may have another.
    let cancel = format!("{path}/{}/cancel", first["run"]["id"].as_str().unwrap());
    let (cancel_status, cancelled) = autopilot(&state, "POST", &cancel, &owner, None).await;
    assert_eq!(cancel_status, StatusCode::OK, "body={cancelled}");
    let (third_status, third) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"name": "Third"})))).await;
    assert_eq!(third_status, StatusCode::CREATED, "body={third}");
    // Another project is not held back by this one.
    let (other, other_owner) = autopilot_project(&state, "Phase7C One At A Time Other").await;
    let (other_status, _) = autopilot(&state, "POST", &format!("/api/v1/projects/{other}/autopilot/runs"), &other_owner, Some(autopilot_body(serde_json::json!({})))).await;
    assert_eq!(other_status, StatusCode::CREATED);
}

#[tokio::test]
async fn phase7c_starting_needs_the_estimate_accepted_and_a_paid_one_the_cost_accepted_too() {
    // A router whose one provider charges: the estimate says the run is paid.
    let (state, _dir) = test_state_with_ai_router(priced_ai_infra()).await;
    fast_autopilot(&state);
    let (project, owner) = autopilot_project(&state, "Phase7C Accept").await;
    let (crawl, file) = finished_crawl(&state, project, &owner, Some(SHOP_GROUNDING)).await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(
        &state,
        "POST",
        &path,
        &owner,
        Some(autopilot_body(serde_json::json!({"crawl_run_id": crawl, "depth": "STANDARD"}))),
    )
    .await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(made["run"]["estimate"]["totals"]["paid"], true, "body={made}");
    let start = format!("{path}/{run_id}/start");

    let (none_status, none) = autopilot(&state, "POST", &start, &owner, None).await;
    let (empty_status, _) = autopilot(&state, "POST", &start, &owner, Some(serde_json::json!({}))).await;
    let (cost_only_status, cost_only) = autopilot(&state, "POST", &start, &owner, Some(serde_json::json!({"accept_paid": true}))).await;
    let (free_only_status, free_only) = autopilot(&state, "POST", &start, &owner, Some(serde_json::json!({"accept_estimate": true}))).await;
    // None of these started it.
    let (_, still) = autopilot(&state, "GET", &format!("{path}/{run_id}"), &owner, None).await;
    assert_eq!(still["run"]["state"], "AWAITING_APPROVAL");

    let (ok_status, started) =
        autopilot(&state, "POST", &start, &owner, Some(serde_json::json!({"accept_estimate": true, "accept_paid": true}))).await;
    let (again_status, again) = autopilot(&state, "POST", &start, &owner, Some(serde_json::json!({"accept_estimate": true, "accept_paid": true}))).await;
    let finished = wait_for_autopilot(&state, project, &owner, &run_id, &["COMPLETED", "FAILED"]).await;
    file.map(std::fs::remove_file);

    assert_eq!(none_status, StatusCode::BAD_REQUEST, "body={none}");
    assert!(none.to_string().contains("accept_estimate: true"), "body={none}");
    assert_eq!(empty_status, StatusCode::BAD_REQUEST);
    assert_eq!(cost_only_status, StatusCode::BAD_REQUEST, "accepting the cost is not accepting the estimate: body={cost_only}");
    assert_eq!(free_only_status, StatusCode::BAD_REQUEST, "body={free_only}");
    assert!(free_only.to_string().contains("accept_paid: true"), "body={free_only}");
    assert_eq!(ok_status, StatusCode::OK, "body={started}");
    assert_eq!(started["run"]["state"], "RUNNING", "body={started}");
    assert_eq!(started["run"]["approved_paid"], true);
    assert!(started["run"]["approved_at"].is_string() && started["run"]["started_at"].is_string());
    assert_eq!(again_status, StatusCode::CONFLICT, "a run starts once: body={again}");
    assert_eq!(finished["run"]["state"], "COMPLETED", "body={finished}");
    let kinds: Vec<&str> = finished["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"run.started"), "{kinds:?}");
    let started_event = finished["events"].as_array().unwrap().iter().find(|e| e["kind"] == "run.started").unwrap();
    assert!(started_event["message"].as_str().unwrap().contains("including its cost"), "{started_event}");

    let audit_rows = state.store.list_audit(None, 100, 0).await.unwrap();
    let starts: Vec<_> = audit_rows.iter().filter(|a| a.action == "autopilot.start").collect();
    assert_eq!(starts.len(), 1, "only the start that worked: {audit_rows:?}");
    assert_eq!(starts[0].detail.as_deref(), Some("paid"));
}

#[tokio::test]
async fn phase7c_a_free_run_needs_only_the_estimate_accepted() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    fast_autopilot(&state);
    let (project, owner) = autopilot_project(&state, "Phase7C Free").await;
    let (crawl, file) = finished_crawl(&state, project, &owner, Some(SHOP_GROUNDING)).await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"crawl_run_id": crawl})))).await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(made["run"]["estimate"]["totals"]["paid"], false, "an unmetered provider costs nothing: body={made}");
    let (status, started) = autopilot(&state, "POST", &format!("{path}/{run_id}/start"), &owner, Some(serde_json::json!({"accept_estimate": true}))).await;
    assert_eq!(status, StatusCode::OK, "body={started}");
    assert_eq!(started["run"]["approved_paid"], false);
    wait_for_autopilot(&state, project, &owner, &run_id, &["COMPLETED", "FAILED"]).await;
    file.map(std::fs::remove_file);
}

#[tokio::test]
async fn phase7c_a_started_run_reads_the_requirements_uses_the_crawl_and_checks_the_gaps() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    fast_autopilot(&state);
    let (project, owner) = autopilot_project(&state, "Phase7C Flow").await;
    let (crawl, file) = finished_crawl(&state, project, &owner, Some(SHOP_GROUNDING)).await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"crawl_run_id": crawl})))).await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();
    autopilot(&state, "POST", &format!("{path}/{run_id}/start"), &owner, Some(serde_json::json!({"accept_estimate": true}))).await;
    let done = wait_for_autopilot(&state, project, &owner, &run_id, &["COMPLETED", "FAILED"]).await;
    // The gap check the run made, made by hand, says the same.
    let (_, by_hand) = autopilot(&state, "POST", &format!("/api/v1/projects/{project}/gap-check"), &owner, Some(serde_json::json!({"crawl_run_id": crawl}))).await;
    file.map(std::fs::remove_file);

    assert_eq!(done["run"]["state"], "COMPLETED", "body={done}");
    assert!(done["run"]["error"].is_null() && done["run"]["finished_at"].is_string());
    assert_eq!(steps_of(&done), [("REQUIREMENTS", "DONE"), ("CRAWL", "SKIPPED"), ("GAP_CHECK", "DONE")].map(|(a, b)| (a.to_string(), b.to_string())));
    let steps = done["steps"].as_array().unwrap();
    assert_eq!(steps[0]["detail"]["requirements"], 6);
    assert_eq!(steps[0]["detail"]["documents"], 1);
    assert_eq!(steps[0]["detail"]["by_kind"], serde_json::json!({"data": 1, "functional": 3, "performance": 1, "security": 1}));
    assert_eq!(steps[1]["detail"], serde_json::json!({"crawl_run_id": crawl, "reused": true}));
    assert_eq!(steps[2]["detail"]["crawl_run_id"], crawl);
    assert_eq!(steps[2]["detail"]["summary"], by_hand["summary"]);
    assert!(steps.iter().all(|s| s["attempts"] == 1));
    let kinds: Vec<&str> = done["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.first(), Some(&"run.created"));
    assert_eq!(kinds.last(), Some(&"run.completed"));
    assert_eq!(kinds.iter().filter(|k| **k == "step.started").count(), 3);
    assert_eq!(done["working"], false, "nothing works on a run that has ended");
}

#[tokio::test]
async fn phase7c_a_run_that_fails_says_which_step_and_why_and_the_project_may_start_another() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    fast_autopilot(&state);
    let (project, owner) = autopilot_project(&state, "Phase7C Fails").await;
    // A finished crawl that left nothing to read: the gap check cannot run.
    let (crawl, _) = finished_crawl(&state, project, &owner, None).await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"crawl_run_id": crawl})))).await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();
    autopilot(&state, "POST", &format!("{path}/{run_id}/start"), &owner, Some(serde_json::json!({"accept_estimate": true}))).await;
    let failed = wait_for_autopilot(&state, project, &owner, &run_id, &["COMPLETED", "FAILED"]).await;

    assert_eq!(failed["run"]["state"], "FAILED", "body={failed}");
    let error = failed["run"]["error"].as_str().unwrap();
    assert!(error.starts_with("Checking the requirements against the crawl: "), "{error}");
    assert!(error.contains("recorded no crawl"), "the reason is the service's own: {error}");
    assert_eq!(failed["run"]["current_step"], "GAP_CHECK");
    assert_eq!(steps_of(&failed), [("REQUIREMENTS", "DONE"), ("CRAWL", "SKIPPED"), ("GAP_CHECK", "FAILED")].map(|(a, b)| (a.to_string(), b.to_string())));
    assert!(failed["steps"][2]["error"].as_str().unwrap().contains("recorded no crawl"));
    let kinds: Vec<&str> = failed["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(&kinds[kinds.len() - 2..], ["step.failed", "run.failed"]);

    let (status, another) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({})))).await;
    assert_eq!(status, StatusCode::CREATED, "a failed run does not hold the project: body={another}");
}

#[tokio::test]
async fn phase7c_cancelling_a_run_that_awaits_approval_ends_it_and_an_ended_run_cannot_be_cancelled() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let (project, owner) = autopilot_project(&state, "Phase7C Cancel").await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({})))).await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();

    let (status, cancelled) = autopilot(&state, "POST", &format!("{path}/{run_id}/cancel"), &owner, None).await;
    assert_eq!(status, StatusCode::OK, "body={cancelled}");
    assert_eq!(cancelled["outcome"], "cancelled");
    assert_eq!(cancelled["run"]["state"], "CANCELLED");
    assert!(cancelled["run"]["finished_at"].is_string());
    assert!(cancelled["steps"].as_array().unwrap().iter().all(|s| s["state"] == "SKIPPED"));
    let (again_status, again) = autopilot(&state, "POST", &format!("{path}/{run_id}/cancel"), &owner, None).await;
    assert_eq!(again_status, StatusCode::CONFLICT, "body={again}");
    assert!(again.to_string().contains("CANCELLED"), "body={again}");
    // A cancelled run cannot be started.
    let (start_status, start) = autopilot(&state, "POST", &format!("{path}/{run_id}/start"), &owner, Some(serde_json::json!({"accept_estimate": true}))).await;
    assert_eq!(start_status, StatusCode::CONFLICT, "body={start}");
    assert!(start.to_string().contains("CANCELLED"), "body={start}");
    let audit_rows = state.store.list_audit(None, 100, 0).await.unwrap();
    let cancels: Vec<_> = audit_rows.iter().filter(|a| a.action == "autopilot.cancel").collect();
    assert_eq!(cancels.len(), 1, "{audit_rows:?}");
    assert_eq!(cancels[0].detail.as_deref(), Some("cancelled"));
}

#[tokio::test]
async fn phase7c_only_an_operator_of_the_project_may_make_start_or_cancel_a_run_and_a_run_stays_in_its_project() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let (project, owner) = autopilot_project(&state, "Phase7C Roles").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let (other, other_owner) = autopilot_project(&state, "Phase7C Roles Other").await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({})))).await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();
    let body = Some(serde_json::json!({"accept_estimate": true}));

    // A viewer may look, and may not change anything.
    let (list, _) = autopilot(&state, "GET", &path, &viewer, None).await;
    let (get, _) = autopilot(&state, "GET", &format!("{path}/{run_id}"), &viewer, None).await;
    let (events, _) = autopilot(&state, "GET", &format!("{path}/{run_id}/events"), &viewer, None).await;
    let (create, _) = autopilot(&state, "POST", &path, &viewer, Some(autopilot_body(serde_json::json!({})))).await;
    let (start, _) = autopilot(&state, "POST", &format!("{path}/{run_id}/start"), &viewer, body.clone()).await;
    let (cancel, _) = autopilot(&state, "POST", &format!("{path}/{run_id}/cancel"), &viewer, None).await;
    assert_eq!((list, get, events), (StatusCode::OK, StatusCode::OK, StatusCode::OK));
    assert_eq!((create, start, cancel), (StatusCode::FORBIDDEN, StatusCode::FORBIDDEN, StatusCode::FORBIDDEN));

    // A key of another project may do nothing here.
    let mut denied = Vec::new();
    for (method, tail, body) in [
        ("GET", String::new(), None),
        ("POST", String::new(), Some(autopilot_body(serde_json::json!({})))),
        ("GET", format!("/{run_id}"), None),
        ("POST", format!("/{run_id}/start"), body.clone()),
        ("POST", format!("/{run_id}/cancel"), None),
        ("GET", format!("/{run_id}/events"), None),
    ] {
        denied.push(autopilot(&state, method, &format!("{path}{tail}"), &other_owner, body).await.0);
    }
    assert!(denied.iter().all(|s| *s == StatusCode::FORBIDDEN), "{denied:?}");

    // A run is found only under its own project, even by a key that may see both.
    let admin = mint_key(&state, "admin", None).await;
    let (wrong_project, _) = autopilot(&state, "GET", &format!("/api/v1/projects/{other}/autopilot/runs/{run_id}"), &admin, None).await;
    let (wrong_start, _) = autopilot(&state, "POST", &format!("/api/v1/projects/{other}/autopilot/runs/{run_id}/start"), &admin, body).await;
    let (wrong_cancel, _) = autopilot(&state, "POST", &format!("/api/v1/projects/{other}/autopilot/runs/{run_id}/cancel"), &admin, None).await;
    let (wrong_events, _) = autopilot(&state, "GET", &format!("/api/v1/projects/{other}/autopilot/runs/{run_id}/events"), &admin, None).await;
    let (unknown, _) = autopilot(&state, "GET", &format!("{path}/{}", Uuid::new_v4()), &owner, None).await;
    assert_eq!((wrong_project, wrong_start, wrong_cancel, wrong_events, unknown), (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND, StatusCode::NOT_FOUND, StatusCode::NOT_FOUND, StatusCode::NOT_FOUND));
    // Without a key at all.
    let (anonymous, _) = send(&state, req("GET", &path, None, None)).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
    // Nothing above changed the run.
    let (_, still) = autopilot(&state, "GET", &format!("{path}/{run_id}"), &owner, None).await;
    assert_eq!(still["run"]["state"], "AWAITING_APPROVAL");
}

#[tokio::test]
async fn phase7c_the_timeline_can_be_read_from_where_a_watcher_was() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    fast_autopilot(&state);
    let (project, owner) = autopilot_project(&state, "Phase7C Events").await;
    let (crawl, file) = finished_crawl(&state, project, &owner, Some(SHOP_GROUNDING)).await;
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"crawl_run_id": crawl})))).await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();
    autopilot(&state, "POST", &format!("{path}/{run_id}/start"), &owner, Some(serde_json::json!({"accept_estimate": true}))).await;
    wait_for_autopilot(&state, project, &owner, &run_id, &["COMPLETED", "FAILED"]).await;
    file.map(std::fs::remove_file);

    let (_, all) = autopilot(&state, "GET", &format!("{path}/{run_id}/events"), &owner, None).await;
    let events = all["events"].as_array().unwrap();
    assert!(events.len() >= 8, "{all}");
    let ids: Vec<i64> = events.iter().map(|e| e["id"].as_i64().unwrap()).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "oldest first: {ids:?}");
    let (_, after) = autopilot(&state, "GET", &format!("{path}/{run_id}/events?after={}", ids[2]), &owner, None).await;
    let later: Vec<i64> = after["events"].as_array().unwrap().iter().map(|e| e["id"].as_i64().unwrap()).collect();
    assert_eq!(later, ids[3..].to_vec(), "only what the watcher has not seen");
    let (_, limited) = autopilot(&state, "GET", &format!("{path}/{run_id}/events?limit=2"), &owner, None).await;
    let last_two: Vec<i64> = limited["events"].as_array().unwrap().iter().map(|e| e["id"].as_i64().unwrap()).collect();
    assert_eq!(last_two, ids[ids.len() - 2..].to_vec(), "the latest two, still oldest first");
    let (_, none) = autopilot(&state, "GET", &format!("{path}/{run_id}/events?after={}", ids[ids.len() - 1]), &owner, None).await;
    assert_eq!(none["events"], serde_json::json!([]));
}

#[tokio::test]
async fn phase7c_a_key_that_has_been_revoked_stops_the_runs_it_made() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    fast_autopilot(&state);
    let (project, starter) = autopilot_project(&state, "Phase7C Revoked").await;
    // The key that makes the run, and another operator who accepts the estimate.
    let maker_id = Uuid::new_v4();
    let maker = crate::auth::generate_key();
    state
        .store
        .insert_api_key(maker_id, "maker", &crate::auth::hash_key(&maker), "operator", Some(&project.to_string()))
        .await
        .unwrap();
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (_, made) = autopilot(&state, "POST", &path, &maker, Some(autopilot_body(serde_json::json!({})))).await;
    let run_id = made["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(made["run"]["created_by_name"], "maker");

    state.store.revoke_api_key(maker_id).await.unwrap();
    let (status, started) = autopilot(&state, "POST", &format!("{path}/{run_id}/start"), &starter, Some(serde_json::json!({"accept_estimate": true}))).await;
    assert_eq!(status, StatusCode::OK, "the operator who accepts is not the one whose identity the run uses: body={started}");
    let failed = wait_for_autopilot(&state, project, &starter, &run_id, &["COMPLETED", "FAILED"]).await;

    assert_eq!(failed["run"]["state"], "FAILED", "body={failed}");
    let error = failed["run"]["error"].as_str().unwrap();
    assert!(error.contains("the API key that started this run has been revoked"), "{error}");
    assert_eq!(steps_of(&failed)[0], ("REQUIREMENTS".to_string(), "FAILED".to_string()));
}

#[tokio::test]
async fn phase7c_a_run_that_was_going_when_the_service_stopped_carries_on_after_it_starts_again() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("api.sqlite3");
    let (project, run_id, waiting_id, awaiting_id, crawl, file);
    {
        let state = restart_test_state(&db_path, dir.path().join("evidence")).await;
        fast_autopilot(&state);
        let made = autopilot_project(&state, "Phase7C Restart").await;
        project = made.0;
        let owner = made.1;
        let finished = finished_crawl(&state, project, &owner, Some(SHOP_GROUNDING)).await;
        crawl = finished.0;
        file = finished.1;
        let path = format!("/api/v1/projects/{project}/autopilot/runs");
        let (_, run) = autopilot(&state, "POST", &path, &owner, Some(autopilot_body(serde_json::json!({"crawl_run_id": crawl})))).await;
        run_id = Uuid::parse_str(run["run"]["id"].as_str().unwrap()).unwrap();
        // The service stops after the run was accepted and its first step finished, in the middle of the second: what the
        // database holds then, without a task working on it.
        assert!(state.store.start_autopilot_run(run_id, false).await.unwrap());
        state.store.begin_autopilot_step(run_id, "REQUIREMENTS").await.unwrap();
        state
            .store
            .settle_autopilot_step(run_id, "REQUIREMENTS", crate::autopilot::StepState::Done, Some(&serde_json::json!({"requirements": 6})), None)
            .await
            .unwrap();
        state.store.begin_autopilot_step(run_id, "CRAWL").await.unwrap();
        // Runs of other projects, that wait for a person, are not picked up.
        let other = make_project(&state, "Phase7C Restart Waiting").await;
        let other_owner = mint_key(&state, "operator", Some(&other.to_string())).await;
        upload_document(&state, &other_owner, other, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
        let (_, waiting) = autopilot(&state, "POST", &format!("/api/v1/projects/{other}/autopilot/runs"), &other_owner, Some(autopilot_body(serde_json::json!({})))).await;
        waiting_id = Uuid::parse_str(waiting["run"]["id"].as_str().unwrap()).unwrap();
        state.store.start_autopilot_run(waiting_id, false).await.unwrap();
        state.store.wait_autopilot_run(waiting_id).await.unwrap();
        let third = make_project(&state, "Phase7C Restart Awaiting").await;
        let third_owner = mint_key(&state, "operator", Some(&third.to_string())).await;
        upload_document(&state, &third_owner, third, "FRD.md", SHOP_FRD.as_bytes(), Some("FRD")).await;
        let (_, awaiting) = autopilot(&state, "POST", &format!("/api/v1/projects/{third}/autopilot/runs"), &third_owner, Some(autopilot_body(serde_json::json!({})))).await;
        awaiting_id = Uuid::parse_str(awaiting["run"]["id"].as_str().unwrap()).unwrap();
    }

    // A new process: the same database, an empty registry.
    let state = restart_test_state(&db_path, dir.path().join("evidence")).await;
    fast_autopilot(&state);
    let resumed = crate::autopilot_runner::resume_runs(&state).await.unwrap();
    assert_eq!(resumed, 1, "only the run that was going");
    let owner = mint_key(&state, "admin", None).await;
    let done = wait_for_autopilot(&state, project, &owner, &run_id.to_string(), &["COMPLETED", "FAILED"]).await;
    file.map(std::fs::remove_file);

    assert_eq!(done["run"]["state"], "COMPLETED", "body={done}");
    let steps = done["steps"].as_array().unwrap();
    assert_eq!(steps[0]["attempts"], 1, "the step that was done is not done again");
    assert_eq!(steps[0]["detail"], serde_json::json!({"requirements": 6}));
    assert_eq!(steps[1]["attempts"], 2, "the step that was cut short was started again");
    assert_eq!(steps[1]["state"], "SKIPPED");
    assert_eq!(steps[2]["state"], "DONE");
    let kinds: Vec<&str> = done["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"run.resumed"), "{kinds:?}");
    // The others are as they were.
    assert_eq!(state.store.get_autopilot_run(waiting_id).await.unwrap().unwrap().state, "WAITING");
    assert_eq!(state.store.get_autopilot_run(awaiting_id).await.unwrap().unwrap().state, "AWAITING_APPROVAL");
    // Once done, a second look picks nothing up.
    assert_eq!(crate::autopilot_runner::resume_runs(&state).await.unwrap(), 0);
}

#[tokio::test]
async fn phase7c_the_internal_routes_are_not_reachable_without_a_key_and_the_public_ones_still_are_guarded() {
    let (state, _dir) = test_state_with_ai_router(ai_infra_with(Ok(valid_ai_response()))).await;
    let (project, _owner) = autopilot_project(&state, "Phase7C Guard").await;
    // The public router still refuses a request with no key, or with a key that does not exist.
    let path = format!("/api/v1/projects/{project}/autopilot/runs");
    let (none, _) = send(&state, req("GET", &path, None, None)).await;
    let (wrong, _) = send(&state, req("GET", &path, Some("aiqa_not_a_key"), None)).await;
    assert_eq!((none, wrong), (StatusCode::UNAUTHORIZED, StatusCode::UNAUTHORIZED));
    // The internal one has no such check (its callers attach an identity themselves): a request with none is a server
    // error, not a way in, and one that names a key that is not active is stopped before it is sent.
    let internal = crate::api::internal_router(state.clone());
    let response = internal.oneshot(req("GET", &path, None, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR, "no identity, no route");
    let gone = crate::autopilot_pipeline::InternalApi::new(&state, Uuid::new_v4());
    let reply = crate::autopilot_pipeline::PipelineApi::send(&gone, "GET", &path, None).await;
    assert_eq!(reply.status, 401);
    assert!(reply.problem().contains("has been revoked"), "{}", reply.problem());
}
