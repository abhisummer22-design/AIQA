//! BE-9 regression tests — the structured error envelope (API Contract
//! §2.5). Unlike every other test file in this crate, these drive real
//! HTTP requests through the real `axum::Router` in-process
//! (`tower::ServiceExt::oneshot`) rather than calling handler/store
//! functions directly, because BE-9's own requirement is specifically about
//! the *HTTP response body* — status code alone proves nothing about
//! whether the JSON envelope shape is actually correct.

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
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

async fn test_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("api.sqlite3")).await.expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor));
    let policy = PolicyConfig {
        max_concurrency: 8,
        // Matches the production default (`PolicyConfig::from_env`'s
        // documented default is "maestro") so a plan naming that executor
        // genuinely lands in PENDING_APPROVAL, exactly like the real
        // deployed policy gate — needed by the Execution-category test.
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    };
    (AppState::new(Arc::new(registry), store, policy, evidence), dir)
}

/// Mints a real, durably persisted API key and returns its plaintext.
async fn mint_key(state: &AppState, role: &str, project_id: Option<&str>) -> String {
    let raw = crate::auth::generate_key();
    state
        .store
        .insert_api_key(Uuid::new_v4(), "test-key", &crate::auth::hash_key(&raw), role, project_id)
        .await
        .expect("insert_api_key");
    raw
}

async fn send(state: &AppState, req: Request<Body>) -> (StatusCode, Value) {
    let app = router(state.clone());
    let resp = app.oneshot(req).await.expect("router call");
    let status = resp.status();
    let content_type = resp.headers().get("content-type").cloned();
    let bytes = resp.into_body().collect().await.expect("collect body").to_bytes();
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
fn req_raw(method: &str, path: &str, key: Option<&str>, content_type: Option<&str>, raw_body: &str) -> Request<Body> {
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
fn assert_envelope(status: StatusCode, body: &Value, expected_status: StatusCode, expected_code: &str) {
    assert_eq!(status, expected_status, "HTTP status; body={body}");
    let error = &body["error"];
    assert!(error.is_object(), "response must have a top-level \"error\" object; body={body}");
    assert_eq!(error["code"], expected_code, "error.code; body={body}");
    assert!(error["message"].is_string(), "error.message must be a string; body={body}");
    assert!(!error["message"].as_str().unwrap().is_empty(), "error.message must not be empty");
    assert!(error["details"].is_object(), "error.details must be an object (contract's own example shows {{}})");
    let request_id = error["request_id"].as_str().expect("error.request_id must be a string");
    assert!(request_id.starts_with("req_"), "request_id should look like the contract's own example (\"req_123\"): {request_id}");
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
    let (status, body) = send(&state, req("GET", "/api/v1/projects", Some("aiqa_not_a_real_key"), None)).await;
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
        req("POST", "/api/v1/projects", Some(&viewer), Some(serde_json::json!({"name": "should be forbidden"}))),
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
        req("POST", "/api/v1/projects", Some(&admin), Some(serde_json::json!({"name": "Project A"}))),
    )
    .await;
    let project_a_id = created["id"].as_str().expect("created project id");

    let scoped_to_other = mint_key(&state, "operator", Some("some-other-project")).await;
    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a_id}"), Some(&scoped_to_other), None),
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
        req("POST", "/api/v1/projects", Some(&admin), Some(serde_json::json!({"name": "   "}))),
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
        req("GET", &format!("/api/v1/projects/{missing_id}"), Some(&admin), None),
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
        req("POST", "/api/v1/projects", Some(&admin), Some(serde_json::json!({"name": "will fail to persist"}))),
    )
    .await;

    assert_envelope(status, &body, StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR");
    let message = body["error"]["message"].as_str().unwrap();
    assert_eq!(message, "An internal error occurred.", "the real SQL error must not reach the client verbatim");
    for leaked_detail in ["RAISE", "ABORT", "be9_fail_project_insert", "injected disk write failure", "SQLite", "sqlite"] {
        assert!(
            !message.contains(leaked_detail) && !body.to_string().contains(leaked_detail),
            "response must not leak internal detail {leaked_detail:?}; full body={body}"
        );
    }
}

/// Category: Execution — a persistence failure surfaced through a
/// *different* handler (`approve_run`, which calls `engine::approve_and_spawn`)
/// than the Persistence test above, proving the same centralized `ApiError`
/// path covers more than one call site uniformly rather than each handler
/// re-implementing its own sanitization.
#[tokio::test]
async fn be9_execution_persistence_failure_returns_sanitized_internal_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;

    // A run stuck in PENDING_APPROVAL, exactly like the policy-gate path
    // already tested end-to-end elsewhere in this codebase.
    let plan = serde_json::json!({
        "name": "be9-execution-test",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 0}}],
    });
    let (status, created) = send(&state, req("POST", "/runs", Some(&admin), Some(plan))).await;
    assert_eq!(status, StatusCode::CREATED, "run creation should succeed; body={created}");
    assert_eq!(created["state"], serde_json::to_value(RunState::PendingApproval).unwrap());
    let run_id = created["run_id"].as_str().expect("run_id");

    sqlx::query(
        "CREATE TRIGGER be9_fail_approve BEFORE UPDATE ON runs WHEN NEW.state = 'SCHEDULED' \
         BEGIN SELECT RAISE(ABORT, 'injected disk write failure approving BE-9 test run'); END;",
    )
    .execute(&state.store.pool)
    .await
    .expect("create trigger");

    let (status, body) = send(&state, req("POST", &format!("/runs/{run_id}/approve"), Some(&admin), None)).await;
    assert_envelope(status, &body, StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR");
    assert_eq!(body["error"]["message"], "An internal error occurred.");
    assert!(!body.to_string().contains("RAISE"), "approve_run's failure must also be sanitized, not just create_project's");
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
        req("POST", "/api/v1/projects", Some(&admin), Some(serde_json::json!({"name": "Real Project"}))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body.get("error").is_none(), "a successful response must not contain an \"error\" key: {body}");
    assert_eq!(body["name"], "Real Project", "successful response schema must be unchanged by BE-9");
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
        req("POST", "/api/v1/projects", Some(&admin), Some(serde_json::json!({"name": "Lifecycle Project", "description": "d1"}))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().expect("id").to_string();

    // A key genuinely scoped to this project (not just an admin) for the
    // list/get/update legs, so this also re-confirms isolation still holds.
    let scoped = mint_key(&state, "operator", Some(&id)).await;

    let (status, list_body) = send(&state, req("GET", "/api/v1/projects", Some(&scoped), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_body.as_array().unwrap().len(), 1, "a scoped key must list only its own project");
    assert_eq!(list_body[0]["id"], id);

    let (status, got) = send(&state, req("GET", &format!("/api/v1/projects/{id}"), Some(&scoped), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "Lifecycle Project");
    assert_eq!(got["description"], "d1");

    let (status, updated) = send(
        &state,
        req("PATCH", &format!("/api/v1/projects/{id}"), Some(&scoped), Some(serde_json::json!({"description": "d2"}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Lifecycle Project", "an omitted field must survive the update, unchanged by BE-9");
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
        req_raw("POST", "/api/v1/projects", Some(&admin), Some("application/json"), "{not valid json"),
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
    let (status, body) = send(&state, req("GET", "/api/v1/projects/not-a-uuid", Some(&admin), None)).await;
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
        req_raw("POST", "/api/v1/projects", Some(&admin), None, r#"{"name":"x"}"#),
    )
    .await;
    assert_envelope(status, &body, StatusCode::UNSUPPORTED_MEDIA_TYPE, "VALIDATION_ERROR");
}

/// BE-10: a query parameter that doesn't deserialize to its declared type
/// (`limit` must be an integer) is rejected by Axum's `Query<T>` extractor.
#[tokio::test]
async fn be10_invalid_query_parameter_returns_structured_envelope() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send(&state, req("GET", "/runs?limit=notanumber", Some(&admin), None)).await;
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
        req("POST", "/api/v1/projects", Some(&admin), Some(serde_json::json!({"name": "Extractor Check"}))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().expect("id");

    // ApiPath: a well-formed UUID path segment.
    let (status, got) = send(&state, req("GET", &format!("/api/v1/projects/{id}"), Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "Extractor Check");

    // ApiQuery: a well-formed query string.
    let (status, list) = send(&state, req("GET", "/runs?limit=5", Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.is_array());
}

// ==================================== Actual BE-9 — project-scoped routing

/// Creates a real project directly against the store (bypassing the router,
/// since these tests need a project id in hand before minting the key
/// that's scoped to it) and returns its id.
async fn make_project(state: &AppState, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    state.store.insert_project(id, name, None).await.expect("insert_project");
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
        req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(simple_plan("scoped-run"))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let run_id = created["run_id"].as_str().expect("run_id").to_string();

    // Give the (SimulatedExecutor-backed) run a moment to finish.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let (status, list) = send(&state, req("GET", &format!("/api/v1/projects/{project}/executions"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["run_id"], run_id);

    let (status, detail) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/executions/{run_id}"), Some(&owner), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["name"], "scoped-run");
    assert!(detail["tasks"].as_array().unwrap().len() == 1);

    let (status, events) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/executions/{run_id}/events"), Some(&owner), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!events.as_array().unwrap().is_empty(), "a completed run must have real events");

    let (status, evidence) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project}/executions/{run_id}/evidence"), Some(&owner), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(evidence.is_array());

    // Regression: a global admin key can also reach it, listing/filtering
    // correctly to *this* project only.
    let (status, admin_list) = send(&state, req("GET", &format!("/api/v1/projects/{project}/executions"), Some(&admin), None)).await;
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
        "tasks": [{"id": "t1", "name": "t1", "executor": "maestro", "simulate": {"outcome": "pass", "delay_ms": 0}}],
    });
    let (status, created) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(gated_plan)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    // BE-13 (§12.1 response alignment): the scoped create endpoint's field
    // is `status`, not the legacy `state` (see `CreateExecutionResponse`).
    assert_eq!(created["status"], "PENDING_APPROVAL");
    let run_id = created["run_id"].as_str().unwrap().to_string();

    let (status, approved) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{project}/executions/{run_id}/approve"), Some(&owner), None),
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
        req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(gated_plan2)),
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
        req("POST", &format!("/api/v1/projects/{project_b}/executions"), Some(&owner_b), Some(simple_plan("b-run"))),
    )
    .await;
    let b_run = created["run_id"].as_str().unwrap().to_string();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // A cannot even name Project B's path at all.
    for (method, path, body) in [
        ("GET", format!("/api/v1/projects/{project_b}/executions/{b_run}"), None),
        ("GET", format!("/api/v1/projects/{project_b}/executions/{b_run}/events"), None),
        ("GET", format!("/api/v1/projects/{project_b}/executions/{b_run}/evidence"), None),
        ("GET", format!("/api/v1/projects/{project_b}/executions"), None),
    ] {
        let (status, body_json) = send(&state, req(method, &path, Some(&owner_a), body)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path} must 403 for a key scoped to a different project; body={body_json}");
        assert_eq!(body_json["error"]["code"], "FORBIDDEN");
    }

    // A cannot reach B's run by guessing/using its id under A's OWN path
    // either — the resource genuinely doesn't belong to A's namespace.
    for (method, path) in [
        ("GET", format!("/api/v1/projects/{project_a}/executions/{b_run}")),
        ("GET", format!("/api/v1/projects/{project_a}/executions/{b_run}/events")),
        ("GET", format!("/api/v1/projects/{project_a}/executions/{b_run}/evidence")),
    ] {
        let (status, body_json) = send(&state, req(method, &path, Some(&owner_a), None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path} must 404 (not 403) — A's namespace genuinely lacks this run; body={body_json}");
        assert_eq!(body_json["error"]["code"], "RESOURCE_NOT_FOUND");
    }

    // Mutation: A cannot approve/reject B's run either way.
    let (status, _) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{project_a}/executions/{b_run}/approve"), Some(&owner_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "A must not be able to mutate B's run under A's own path");

    let (status, _) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{project_b}/executions/{b_run}/approve"), Some(&owner_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "A must not be able to mutate B's run under B's path either");
}

/// The same isolation, but for evidence specifically addressed by its own
/// id (`GET /api/v1/projects/:project_id/evidence/:evidence_id`), which
/// resolves ownership through the evidence's owning run rather than a
/// direct `project_id` column — the exact path BE-8/legacy
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
        req("POST", &format!("/api/v1/projects/{project_b}/executions"), Some(&owner_b), Some(simple_plan("b-evidence-run"))),
    )
    .await;
    let b_run: Uuid = created["run_id"].as_str().unwrap().parse().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let evidence_store = crate::evidence::LocalEvidenceStore::new(dir.path().join("evidence")).unwrap();
    let (object_key, size) = evidence_store.put(&b_run.to_string(), "t1", "log", "real captured output").unwrap();
    let evidence_id = state
        .store
        .insert_evidence(b_run, "t1", "log", &object_key, "text/plain", size)
        .await
        .expect("insert_evidence");

    // B can read its own evidence via the scoped path. Success here is raw
    // evidence bytes, not JSON (matching the legacy `get_evidence_content`
    // exactly), so this one assertion bypasses `send`'s JSON parsing.
    let app = router(state.clone());
    let resp = app
        .oneshot(req("GET", &format!("/api/v1/projects/{project_b}/evidence/{evidence_id}"), Some(&owner_b), None))
        .await
        .expect("router call");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.expect("collect body").to_bytes();
    assert_eq!(&bytes[..], b"real captured output");

    // A cannot, under either project's path.
    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/evidence/{evidence_id}"), Some(&owner_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_b}/evidence/{evidence_id}"), Some(&owner_a), None),
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
        req("POST", &format!("/api/v1/projects/{tenant_b_project}/executions"), Some(&tenant_b_key), Some(simple_plan("tenant-b-run"))),
    )
    .await;
    let tenant_b_run = created["run_id"].as_str().unwrap().to_string();

    // Tenant A (even just a viewer) must not see Tenant B's run under B's own path.
    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{tenant_b_project}/executions/{tenant_b_run}"), Some(&tenant_a_key), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    // And Tenant B's own key genuinely can.
    let (status, _) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{tenant_b_project}/executions/{tenant_b_run}"), Some(&tenant_b_key), None),
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

    let (status, body) = send(&state, req("GET", &format!("/api/v1/projects/{phantom_project}/executions"), Some(&admin), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    let (status, body) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{phantom_project}/executions"), Some(&admin), Some(simple_plan("x"))),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

/// Legacy route compatibility (Step 6): the flat, unscoped routes must
/// remain fully functional and byte-for-byte unchanged in behavior — this
/// is what makes "keep both, migrate nothing away" a safe, low-risk choice
/// rather than an assumption.
#[tokio::test]
async fn actual_be9_legacy_flat_routes_remain_fully_functional() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;

    let (status, created) = send(&state, req("POST", "/runs", Some(&admin), Some(simple_plan("legacy-run")))).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = created["run_id"].as_str().unwrap();

    let (status, _) = send(&state, req("GET", &format!("/runs/{run_id}"), Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, list) = send(&state, req("GET", "/runs", Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!list.as_array().unwrap().is_empty());
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
        req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(mixed_outcome_plan("mixed-run"))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (status, dash) = send(&state, req("GET", &format!("/api/v1/projects/{project}/dashboard"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::OK, "body={dash}");
    assert_eq!(dash["project_id"], project.to_string());
    assert_eq!(dash["project_name"], "Dashboard Real Data");
    // One run, terminal (COMPLETED or PARTIAL depending on scheduler
    // semantics for a mixed-outcome plan) — assert the count sums to 1
    // rather than pinning an exact state, since that state machine is
    // owned by engine.rs, not this unit.
    let exec_counts = dash["execution_counts"].as_object().expect("execution_counts object");
    let total_runs: i64 = exec_counts.values().map(|v| v.as_i64().unwrap()).sum();
    assert_eq!(total_runs, 1, "execution_counts={exec_counts:?}");
    assert_eq!(dash["pass_fail"]["passed"], 1);
    assert_eq!(dash["pass_fail"]["failed"], 1);
    assert_eq!(dash["task_counts"]["PASSED"], 1);
    assert_eq!(dash["task_counts"]["FAILED"], 1);
    assert_eq!(dash["test_count"], 2, "two distinct task names ingested into the quality graph");
    assert!(dash["generated_at"].is_string());

    let (status, kpis) = send(&state, req("GET", &format!("/api/v1/projects/{project}/kpis"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::OK, "body={kpis}");
    assert_eq!(kpis["project_id"], project.to_string());
    assert_eq!(kpis["execution_reliability"], 0.5, "1 passed / (1 passed + 1 failed)");
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

    let (status, dash) = send(&state, req("GET", &format!("/api/v1/projects/{project}/dashboard"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::OK, "body={dash}");
    assert_eq!(dash["execution_counts"], serde_json::json!({}));
    assert_eq!(dash["task_counts"], serde_json::json!({}));
    assert_eq!(dash["pass_fail"]["passed"], 0);
    assert_eq!(dash["pass_fail"]["failed"], 0);
    assert_eq!(dash["test_count"], 0);

    let (status, kpis) = send(&state, req("GET", &format!("/api/v1/projects/{project}/kpis"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::OK, "body={kpis}");
    assert!(kpis["execution_reliability"].is_null(), "no terminal product outcome yet -> null, not 0 or 1; kpis={kpis}");
    assert!(kpis["execution_efficiency_ms"].is_null(), "no finished task yet -> null, not a fake 0; kpis={kpis}");
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
    send(&state, req("POST", &format!("/api/v1/projects/{project_a}/executions"), Some(&owner_a), Some(simple_plan("a-run")))).await;
    let fail_plan = serde_json::json!({
        "name": "b-run",
        "concurrency": 1,
        "tasks": [{"id": "t1", "name": "t1", "simulate": {"outcome": "fail", "delay_ms": 0}}],
    });
    send(&state, req("POST", &format!("/api/v1/projects/{project_b}/executions"), Some(&owner_b), Some(fail_plan))).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (_, dash_a) = send(&state, req("GET", &format!("/api/v1/projects/{project_a}/dashboard"), Some(&owner_a), None)).await;
    assert_eq!(dash_a["pass_fail"]["passed"], 1);
    assert_eq!(dash_a["pass_fail"]["failed"], 0);

    let (_, dash_b) = send(&state, req("GET", &format!("/api/v1/projects/{project_b}/dashboard"), Some(&owner_b), None)).await;
    assert_eq!(dash_b["pass_fail"]["passed"], 0);
    assert_eq!(dash_b["pass_fail"]["failed"], 1);

    let (_, kpis_a) = send(&state, req("GET", &format!("/api/v1/projects/{project_a}/kpis"), Some(&owner_a), None)).await;
    assert_eq!(kpis_a["execution_reliability"], 1.0);

    let (_, kpis_b) = send(&state, req("GET", &format!("/api/v1/projects/{project_b}/kpis"), Some(&owner_b), None)).await;
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

    let (status, body) = send(&state, req("GET", &format!("/api/v1/projects/{project_b}/dashboard"), Some(&owner_a), None)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    let (status, body) = send(&state, req("GET", &format!("/api/v1/projects/{project_b}/kpis"), Some(&owner_a), None)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // A global admin key, by contrast, may read either project.
    let admin = mint_key(&state, "admin", None).await;
    let (status, _) = send(&state, req("GET", &format!("/api/v1/projects/{project_b}/dashboard"), Some(&admin), None)).await;
    assert_eq!(status, StatusCode::OK);
}

/// Category: nonexistent project -> structured 404, same as every other
/// project-scoped route (Actual BE-9's own convention).
#[tokio::test]
async fn be12_dashboard_and_kpis_nonexistent_project_returns_structured_not_found() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let phantom = Uuid::new_v4();

    let (status, body) = send(&state, req("GET", &format!("/api/v1/projects/{phantom}/dashboard"), Some(&admin), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    let (status, body) = send(&state, req("GET", &format!("/api/v1/projects/{phantom}/kpis"), Some(&admin), None)).await;
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

    let (status, body) = send(&state, req("GET", "/api/v1/projects/not-a-uuid/dashboard", Some(&admin), None)).await;
    assert_envelope(status, &body, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
}

/// Category: unauthenticated request -> structured 401, matching every
/// other route behind the `require_auth` middleware layer.
#[tokio::test]
async fn be12_dashboard_and_kpis_require_authentication() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Auth Check").await;

    let (status, body) = send(&state, req("GET", &format!("/api/v1/projects/{project}/dashboard"), None, None)).await;
    assert_envelope(status, &body, StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");

    let (status, body) = send(&state, req("GET", &format!("/api/v1/projects/{project}/kpis"), None, None)).await;
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
async fn poll_run_state(state: &AppState, project: Uuid, run_id: &str, key: &str, want: &str) -> Value {
    for _ in 0..100 {
        let (status, detail) =
            send(state, req("GET", &format!("/api/v1/projects/{project}/executions/{run_id}"), Some(key), None)).await;
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
        req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(delayed_plan("slow-run", 1_500))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    let run_id = created["run_id"].as_str().expect("run_id").to_string();
    assert_eq!(created["status"], "SCHEDULED", "created={created}");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await; // let the task actually start

    let start = std::time::Instant::now();
    let (status, cancel_body) =
        send(&state, req("POST", &format!("/api/v1/projects/{project}/executions/{run_id}/cancel"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::OK, "body={cancel_body}");
    assert_eq!(cancel_body["run_id"], run_id);
    assert_ne!(cancel_body["status"], "COMPLETED", "cancel response must never claim the run completed normally");

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
    let (status, events) =
        send(&state, req("GET", &format!("/api/v1/projects/{project}/executions/{run_id}/events"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::OK);
    let events = events.as_array().unwrap();
    assert!(
        events.iter().any(|e| e["summary"].as_str().unwrap_or("").to_lowercase().contains("cancel")),
        "events must include a cancellation-related summary; events={events:?}"
    );

    // Required audit behavior (Contract §12.3 "auditable").
    let audit = state.store.list_audit(50).await.expect("list_audit");
    assert!(
        audit.iter().any(|a| a.action == "run.cancel" && a.target.as_deref() == Some(run_id.as_str()) && a.outcome == "ok"),
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

    let (_, created) =
        send(&state, req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(simple_plan("fast-run")))).await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    poll_run_state(&state, project, &run_id, &owner, "COMPLETED").await;

    let (status, body) =
        send(&state, req("POST", &format!("/api/v1/projects/{project}/executions/{run_id}/cancel"), Some(&owner), None)).await;
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
    let (_, created) =
        send(&state, req("POST", &format!("/api/v1/projects/{project}/executions"), Some(&owner), Some(gated))).await;
    assert_eq!(created["status"], "PENDING_APPROVAL", "created={created}");
    let run_id = created["run_id"].as_str().unwrap().to_string();

    let (status, body) =
        send(&state, req("POST", &format!("/api/v1/projects/{project}/executions/{run_id}/cancel"), Some(&owner), None)).await;
    assert_envelope(status, &body, StatusCode::CONFLICT, "CONFLICT");
    assert!(body["error"]["message"].as_str().unwrap().to_lowercase().contains("reject"), "body={body}");
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

    let (_, created) =
        send(&state, req("POST", &format!("/api/v1/projects/{project_a}/executions"), Some(&owner_a), Some(delayed_plan("r", 1_500)))).await;
    let run_id = created["run_id"].as_str().unwrap().to_string();

    // Cross-project: project B's own key, valid role, wrong project.
    let (status, body) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{project_a}/executions/{run_id}/cancel"), Some(&owner_b), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // Insufficient role: project A's own key, right project, wrong role.
    let (status, body) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{project_a}/executions/{run_id}/cancel"), Some(&viewer_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");

    // Confirm neither rejected attempt actually cancelled it.
    let (_, detail) =
        send(&state, req("GET", &format!("/api/v1/projects/{project_a}/executions/{run_id}"), Some(&owner_a), None)).await;
    assert_eq!(detail["state"], "RUNNING", "an unauthorized cancel attempt must not have changed the run's state");
}

/// Category: nonexistent project/run.
#[tokio::test]
async fn be13_cancel_nonexistent_run_and_project_return_structured_not_found() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "Cancel Phantom").await;
    let owner = mint_key(&state, "operator", Some(&project.to_string())).await;
    let phantom_run = Uuid::new_v4();
    let phantom_project = Uuid::new_v4();

    let (status, body) =
        send(&state, req("POST", &format!("/api/v1/projects/{project}/executions/{phantom_run}/cancel"), Some(&owner), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    let (status, body) = send(
        &state,
        req("POST", &format!("/api/v1/projects/{phantom_project}/executions/{phantom_run}/cancel"), Some(&owner), None),
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

    let (_, created) =
        send(&state, req("POST", &format!("/api/v1/projects/{project_a}/executions"), Some(&owner_a), Some(simple_plan("task-run")))).await;
    let run_id = created["run_id"].as_str().unwrap().to_string();
    poll_run_state(&state, project_a, &run_id, &owner_a, "COMPLETED").await;

    let (status, tasks) =
        send(&state, req("GET", &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks"), Some(&owner_a), None)).await;
    assert_eq!(status, StatusCode::OK, "body={tasks}");
    let tasks_arr = tasks.as_array().unwrap();
    assert_eq!(tasks_arr.len(), 1);
    assert_eq!(tasks_arr[0]["id"], "t1");
    assert_eq!(tasks_arr[0]["status"], "PASSED");

    let (status, task_detail) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks/t1"), Some(&owner_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={task_detail}");
    assert_eq!(task_detail["id"], "t1");
    assert_eq!(task_detail["status"], "PASSED");

    // Nonexistent task id within a real run.
    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks/does-not-exist"), Some(&owner_a), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");

    // Cross-project: project B's key must not read project A's tasks.
    let (status, body) =
        send(&state, req("GET", &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks"), Some(&owner_b), None)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    let (status, body) = send(
        &state,
        req("GET", &format!("/api/v1/projects/{project_a}/executions/{run_id}/tasks/t1"), Some(&owner_b), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
}

/// Category: legacy execution route regression. BE-13 intentionally
/// realigned the *scoped* create response's field name (`state` ->
/// `status`, see `CreateExecutionResponse`); the legacy, unscoped `POST
/// /runs` response must be completely unaffected — still `state`, not
/// `status` — since `create_run_core`'s shared `CreateRunResponse` was left
/// untouched and only the scoped wrapper changed.
#[tokio::test]
async fn be13_legacy_create_response_shape_is_unchanged() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;

    let (status, created) = send(&state, req("POST", "/runs", Some(&admin), Some(simple_plan("legacy-unchanged")))).await;
    assert_eq!(status, StatusCode::CREATED, "body={created}");
    assert!(created["state"].is_string(), "legacy response must keep its original `state` field; body={created}");
    assert!(created["status"].is_null(), "legacy response must NOT gain the new `status` field; body={created}");
}
