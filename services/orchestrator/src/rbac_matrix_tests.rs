//! Phase D — the RBAC matrix.
//!
//! This file exists because the coverage was lumpy in a way that mattered. The
//! audit found real, existing denial tests (56 of them) and one genuinely strong
//! cross-project test, so this was never an untested codebase. What it found is
//! that the lumpy parts were precisely the dangerous ones:
//!
//! - `POST /admin/api-keys` had **no request-level test at all**. The one hit for
//!   that path in the suite was a path-parsing assertion, not a request. The
//!   project-scoped-admin-mints-a-global-admin-key escalation lived there
//!   unpunished.
//! - All eight governed-healing HTTP routes had **zero router-level coverage**:
//!   the token `healing` does not appear in `api_tests.rs` at all. That is where
//!   the missing `require_resource_in_project` on three mutations lived.
//! - `GET /metrics` had no denial test and no role gate, so a project-scoped
//!   Viewer could read deployment-wide counters.
//! - `auth.rs` had **no tests of its own** — not one `#[cfg(test)]` anywhere in the
//!   file. Every guard in the API is built from its four primitives, and a
//!   regression there is a regression everywhere.
//!
//! Every signature below was read from source before use rather than recalled. The
//! first draft of this project already deleted one unverified test file, and this
//! one guessed an eight-argument `insert_healing_proposal` that takes a single
//! `&HealingProposalRow`.
//!
//! `Store::create_project` does not exist; the real name is `insert_project`, and
//! the caller generates the id.

use crate::auth::{AuthContext, Role};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::policy::PolicyConfig;
use crate::state::AppState;
use crate::store::Store;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ------------------------------------------------------------------ harness

fn inert_ai_router() -> Arc<crate::ai_router::AiInfrastructure> {
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};

    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors = HashMap::new();
    executors.insert("mock", executor);
    Arc::new(AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider(
            "mock",
            vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
        )))],
        executors,
    ))
}

async fn test_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("rbac.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence");
    (
        AppState::new(
            Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor))),
            store,
            PolicyConfig {
                max_concurrency: 8,
                require_approval_for: HashSet::new(),
                blocked_executors: HashSet::new(),
            },
            evidence,
            inert_ai_router(),
            Arc::new(crate::ai_router::test_support::FakeEmbedder::unconfigured()),
        ),
        dir,
    )
}

async fn mint_key(state: &AppState, role: &str, project_id: Option<&str>) -> String {
    let raw = crate::auth::generate_key();
    state
        .store
        .insert_api_key(
            Uuid::new_v4(),
            "matrix-key",
            &crate::auth::hash_key(&raw),
            role,
            project_id,
        )
        .await
        .unwrap();
    raw
}

/// A real project row. The caller generates the id — that is the real signature.
async fn make_project(state: &AppState, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    state.store.insert_project(id, name, None).await.unwrap();
    id
}

/// Drives the real router, exactly as `api_tests.rs` does — calling handlers
/// directly would bypass the middleware stack the guards sit in.
async fn send(
    state: &AppState,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let app = crate::api::router(state.clone());
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(k) = key {
        builder = builder.header("Authorization", format!("Bearer {k}"));
    }
    let request = match body {
        Some(v) => builder
            .header("Content-Type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

/// A real proposal row owned by `project_id`.
///
/// The three JSON columns are *deserialised on read* by `row_to_proposal`
/// (`original_actions`, `proposed_actions`, `ast_diff`), so placeholder strings
/// like `"before"` turn every read into a 500. They are built from the real types
/// here, and `AstDiff::compute` produces the diff rather than hand-writing it.
async fn make_proposal(state: &AppState, project_id: Uuid, fingerprint: &str) -> Uuid {
    use crate::execution_plan::Action;
    use crate::governed_healing::AstDiff;

    let now = chrono::Utc::now();
    let id = Uuid::new_v4();
    let original: Vec<Action> = vec![Action::Click {
        selector: "#old".into(),
    }];
    let proposed: Vec<Action> = vec![Action::Click {
        selector: "#new".into(),
    }];
    let ast_diff = AstDiff::compute(&original, &proposed);

    state
        .store
        .insert_healing_proposal(&crate::store::HealingProposalRow {
            id,
            project_id: project_id.to_string(),
            run_id: Uuid::new_v4(),
            task_id: "a task".into(),
            failure_fingerprint: fingerprint.into(),
            diagnosis_category: "Selector".into(),
            confidence: 0.9,
            risk_level: "MEDIUM".into(),
            requires_approval: true,
            original_actions: serde_json::to_string(&original).unwrap(),
            proposed_actions: serde_json::to_string(&proposed).unwrap(),
            ast_diff: serde_json::to_string(&ast_diff).unwrap(),
            rationale: "a diagnosis".into(),
            evidence_refs: "[]".into(),
            state: "PENDING_APPROVAL".into(),
            idempotency_key: format!("matrix-{id}"),
            healing_run_id: None,
            verification_result: None,
            approved_by: None,
            approved_at: None,
            rejection_reason: None,
            created_at: now,
            expires_at: now + chrono::Duration::hours(24),
            updated_at: now,
            context: None,
        })
        .await
        .expect("fixture proposal");
    id
}

/// Like `send`, but for a plain-text endpoint. `/metrics` is Prometheus text
/// exposition, not JSON, so parsing the body as JSON yields `Null` and every
/// assertion against it would pass or fail for the wrong reason.
async fn send_raw(state: &AppState, path: &str, key: &str) -> (StatusCode, String) {
    let app = crate::api::router(state.clone());
    let request = Request::builder()
        .method("GET")
        .uri(path)
        .header("Authorization", format!("Bearer {key}"))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

// ------------------------------------------------------- GET /metrics, Admin

/// Deployment-wide counters — runs by state, tasks by status, total audit rows,
/// total telemetry, active key count — across every tenant. A key scoped to one
/// project has no business reading those.
#[tokio::test]
async fn metrics_is_refused_to_a_viewer_and_an_operator() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "tenant-a").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    for (label, key) in [("viewer", &viewer), ("operator", &operator)] {
        let (status, body) = send(&state, "GET", "/metrics", Some(key), None).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a {label} read deployment-wide metrics: {body}"
        );
    }
}

#[tokio::test]
async fn metrics_is_still_readable_by_an_admin() {
    let (state, _dir) = test_state().await;
    let admin = mint_key(&state, "admin", None).await;
    let (status, body) = send_raw(&state, "/metrics", &admin).await;
    // Gating it must not have broken the endpoint it was added to protect.
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("http_request_duration_seconds"),
        "the body must still be the exposition format, not an error envelope"
    );
}

// --------------------------- healing: the URL's project must own the resource

/// The finding: `require_scoped_project` proved the key may act on the project in
/// the URL, but not that the proposal being acted on is *in* it. Both read
/// siblings checked; the three mutations did not.
#[tokio::test]
async fn a_proposal_from_another_project_cannot_be_acted_on_through_this_projects_url() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "project-a").await;
    let project_b = make_project(&state, "project-b").await;
    let proposal = make_proposal(&state, project_b, "fingerprint-a").await;

    let global_admin = mint_key(&state, "admin", None).await;

    for verb in ["approve", "reject", "execute"] {
        let path = format!("/api/v1/projects/{project_a}/healing/proposals/{proposal}/{verb}");
        let body = if verb == "reject" {
            Some(json!({ "reason": "not mine" }))
        } else {
            None
        };
        let (status, response) = send(&state, "POST", &path, Some(&global_admin), body).await;
        assert_ne!(
            status,
            StatusCode::OK,
            "{verb} through project A's URL succeeded on a proposal owned by project B: {response}"
        );
        // 404, not 403: a mismatch must not confirm the proposal exists elsewhere,
        // which is the reasoning `require_resource_in_project` already states.
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{verb} leaked: {response}"
        );
    }

    // The decisive one. A guard placed *after* the mutation looks equivalent and is
    // not: the row is already written by the time the handler can inspect it, so a
    // cross-project approve would change project B's proposal and then answer 404.
    // The status code would be correct and the damage already done. So assert the
    // proposal is untouched, not merely that the call was refused.
    let stored = state
        .store
        .get_healing_proposal(proposal)
        .await
        .expect("proposal readable")
        .expect("proposal still exists");
    assert_eq!(
        stored.state, "PENDING_APPROVAL",
        "a cross-project approve/refact left project B's proposal in state {:?} -- the \\
         guard ran AFTER the mutation, so the refusal was cosmetic",
        stored.state
    );
    assert!(stored.approved_by.is_none(), "and it recorded an approver");
}

/// The read siblings already did this; pinned so the three mutations cannot be
/// "aligned" back into inconsistency later.
#[tokio::test]
async fn a_proposal_from_another_project_cannot_be_read_through_this_projects_url() {
    let (state, _dir) = test_state().await;
    let project_a = make_project(&state, "project-a").await;
    let project_b = make_project(&state, "project-b").await;
    let proposal = make_proposal(&state, project_b, "fingerprint-b").await;

    let global_admin = mint_key(&state, "admin", None).await;
    for suffix in ["", "/diff", "/verification"] {
        let path = format!("/api/v1/projects/{project_a}/healing/proposals/{proposal}{suffix}");
        let (status, resp) = send(&state, "GET", &path, Some(&global_admin), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "GET {suffix} leaked another project: {resp}");
    }
}

/// The proposal's own project must still work — a guard that refuses everything is
/// not a fix.
#[tokio::test]
async fn a_proposal_can_still_be_read_through_its_own_projects_url() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "project-a").await;
    let proposal = make_proposal(&state, project, "fingerprint-own").await;

    let admin = mint_key(&state, "admin", None).await;
    let path = format!("/api/v1/projects/{project}/healing/proposals/{proposal}/diff");
    let (status, body) = send(&state, "GET", &path, Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

// ------------------------------------------------- the admin surface, by role

/// The role half of `POST /admin/api-keys`. The scope half is in
/// `key_scope_tests.rs`; both were missing entirely.
#[tokio::test]
async fn minting_an_api_key_is_refused_to_a_viewer_and_an_operator() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "tenant").await;
    let viewer = mint_key(&state, "viewer", Some(&project.to_string())).await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    for (label, key) in [("viewer", &viewer), ("operator", &operator)] {
        let (status, body) = send(
            &state,
            "POST",
            "/admin/api-keys",
            Some(key),
            Some(json!({ "name": "x", "role": "viewer" })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a {label} minted an API key: {body}"
        );
    }
    let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
        .fetch_one(&state.store.pool)
        .await
        .unwrap();
    assert_eq!(keys, 2, "only the two test keys should exist");
}

#[tokio::test]
async fn the_admin_audit_log_is_refused_to_a_non_admin() {
    let (state, _dir) = test_state().await;
    let project = make_project(&state, "tenant").await;
    let operator = mint_key(&state, "operator", Some(&project.to_string())).await;

    let (status, body) = send(&state, "GET", "/admin/audit-log", Some(&operator), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

// ------------------------------------------------- auth.rs, on its own terms

/// `auth.rs` had no `#[cfg(test)]` at all. These four primitives are what every
/// guard in the API is built from.
#[test]
fn an_unknown_role_string_is_read_as_viewer_not_as_admin() {
    // Fail-closed: anything not exactly "admin" or "operator" becomes Viewer. If an
    // unmapped string ever coerced to something more privileged, a typo in a
    // stored key would become an escalation.
    assert_eq!(Role::parse("supervisor"), Role::Viewer);
    assert_eq!(Role::parse(""), Role::Viewer);
    assert_eq!(Role::parse("admin "), Role::Viewer);
    // Case-sensitive, deliberately: `"ADMIN"` is not a spelling of `admin` and
    // silently accepting it would mean a stored key's role depended on who typed
    // it. The value is written by `create_api_key`, which validates against the
    // same three lowercase literals.
    assert_eq!(Role::parse("ADMIN"), Role::Viewer);
    assert_eq!(Role::parse("admin"), Role::Admin);
    assert_eq!(Role::parse("operator"), Role::Operator);
}

#[test]
fn roles_are_ordered_so_a_minimum_can_be_compared() {
    assert!(Role::Admin.at_least(Role::Viewer));
    assert!(Role::Admin.at_least(Role::Operator));
    assert!(Role::Operator.at_least(Role::Viewer));
    assert!(!Role::Operator.at_least(Role::Admin));
    assert!(!Role::Viewer.at_least(Role::Operator));
}

fn ctx(role: Role, project_id: Option<&str>) -> AuthContext {
    AuthContext {
        key_id: Uuid::new_v4(),
        name: "matrix".into(),
        role,
        project_id: project_id.map(|s| s.to_string()),
    }
}

#[test]
fn a_global_key_passes_every_project_check() {
    let global = ctx(Role::Viewer, None);
    assert!(
        crate::auth::require_project_access(&global, Some("anything")).is_ok(),
        "a global key must reach every project, or nothing works"
    );
    assert!(crate::auth::require_project_access(&global, None).is_ok());
}

#[test]
fn a_scoped_key_reaches_only_its_own_project() {
    let scoped = ctx(Role::Viewer, Some("p1"));
    assert!(crate::auth::require_project_access(&scoped, Some("p1")).is_ok());
    assert!(crate::auth::require_project_access(&scoped, None).is_err());
    assert!(
        crate::auth::require_project_access(&scoped, Some("p2")).is_err(),
        "a p1 key reached p2"
    );
}

#[test]
fn the_role_gate_is_independent_of_project_scope() {
    // The two axes are separate, and conflating them is exactly how the
    // key-minting escalation happened: an Admin role check said nothing at all
    // about scope.
    let viewer_own_project = ctx(Role::Viewer, Some("p1"));
    assert!(crate::auth::require_role(&viewer_own_project, Role::Viewer).is_ok());
    assert!(crate::auth::require_role(&viewer_own_project, Role::Operator).is_err());
    assert!(crate::auth::require_role(&viewer_own_project, Role::Admin).is_err());

    let admin_other_project = ctx(Role::Admin, Some("p1"));
    assert!(crate::auth::require_role(&admin_other_project, Role::Admin).is_ok());
    assert!(crate::auth::require_project_access(&admin_other_project, Some("p2")).is_err());
}

#[test]
fn a_hashed_key_matches_only_its_own_raw_form() {
    let raw = crate::auth::generate_key();
    let hashed = crate::auth::hash_key(&raw);
    assert_ne!(raw, hashed, "the key must not be stored in the clear");
    assert_ne!(hashed, crate::auth::hash_key("something else"));
    assert_eq!(hashed, crate::auth::hash_key(&raw), "hashing must be deterministic");
    assert!(raw.starts_with("aiqa_"), "unexpected key format: {raw}");
}

#[test]
fn generated_keys_are_distinct() {
    let mut seen = HashSet::new();
    for _ in 0..64 {
        assert!(seen.insert(crate::auth::generate_key()));
    }
}

/// The store signatures this matrix relies on, asserted so a rename surfaces here
/// instead of as a confusing compile error further away.
#[tokio::test]
async fn the_helpers_this_matrix_uses_exist_with_the_signatures_assumed() {
    let (state, _dir) = test_state().await;
    let id = Uuid::new_v4();
    state
        .store
        .insert_project(id, "signature-check", None)
        .await
        .expect("insert_project(id, name, description)");
    let listed = state
        .store
        .list_projects(Some(&id.to_string()))
        .await
        .expect("list_projects(Some(filter))");
    assert_eq!(listed.len(), 1);
    assert!(state
        .store
        .get_project(id)
        .await
        .expect("get_project")
        .is_some());
}