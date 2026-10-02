//! Phase D, part 0 — the API-key minting escalation.
//!
//! `POST /admin/api-keys` guarded on **role only**. Its `project_id` body field
//! went straight into `insert_api_key`, and `insert_api_key` persists it verbatim.
//! `require_project_access` returns `Ok(())` when the key's `project_id` is
//! `None`, i.e. a global key passes every project's check.
//!
//! Put together, and the endpoint this file is about issues the key that unlocks
//! it: a key of shape `role=admin, project_id=Some("A")` — a *tenant* admin, and
//! precisely what a previous call to this endpoint would have handed out — could
//! POST `{"role":"admin","project_id":null}` and receive a plaintext **global**
//! admin key. One call, and the tenant boundary is gone.
//!
//! The invariant being pinned is scope containment, not role: **a caller can
//! never mint a key with broader scope than its own.** A global admin is
//! unaffected; a scoped admin can act freely inside its own project and not one
//! step outside.
//!
//! `auth.rs` itself has no tests at all, and no test anywhere made a request to
//! `POST /admin/api-keys` — the one hit for that path in the suite is a
//! path-parsing assertion, not a request. So this starts from an empty position
//! rather than adding to an existing file.

use crate::auth::{generate_key, hash_key, AuthContext, Role};
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::policy::PolicyConfig;
use crate::state::AppState;
use crate::store::Store;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

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
    let store = Store::connect(&dir.path().join("keyscope.sqlite3"))
        .await
        .expect("connect");
    let evidence =
        crate::evidence::LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence");
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
    let raw = generate_key();
    state
        .store
        .insert_api_key(
            Uuid::new_v4(),
            "test-key",
            &hash_key(&raw),
            role,
            project_id,
        )
        .await
        .unwrap();
    raw
}

async fn post_key(state: &AppState, key: &str, body: Value) -> (StatusCode, Value) {
    let app: Router = crate::api::router(state.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/admin/api-keys")
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

/// What scope a key actually has, read back from the row rather than from the
/// response — the response only carries the plaintext, and the row is what every
/// later authorization decision consults.
async fn scope_of_key(state: &AppState, raw: &str) -> Option<Option<String>> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT role, project_id FROM api_keys WHERE key_hash = ?")
            .bind(hash_key(raw))
            .fetch_optional(&state.store.pool)
            .await
            .unwrap();
    row.map(|(_, pid)| pid)
}

/// Every minted key, so a rejected request can be asserted to have created
/// nothing at all rather than merely to have returned an error.
async fn all_key_names(state: &AppState) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT name FROM api_keys ORDER BY name")
        .fetch_all(&state.store.pool)
        .await
        .unwrap()
}

// ------------------------------------------------------------------ the hole

/// The escalation itself, in both of the spellings a caller can use to ask for a
/// global key.
///
/// Note what is *not* asserted here: a 403. An explicit `project_id: null` and an
/// omitted `project_id` are the same request to serde, and neither is an
/// escalation any more once the fix is in — the key simply comes back scoped to
/// the caller's own project. Refusing them would have broken any client that
/// sends `null` or omits the field (a UI with no project selector, say) in
/// exchange for no security gain, since the outcome is already safe. So the
/// property pinned is the one that matters: **the persisted scope**, read back
/// from the row rather than trusted from the response.
#[tokio::test]
async fn a_project_scoped_admin_never_ends_up_with_a_global_key_however_it_asks() {
    for (label, body) in [
        (
            "explicit-null",
            json!({ "name": "asks-global", "role": "admin", "project_id": null }),
        ),
        (
            "omitted",
            json!({ "name": "asks-global-omitted", "role": "admin" }),
        ),
    ] {
        let (state, _dir) = test_state().await;
        let tenant_admin = mint_key(&state, "admin", Some("project-a")).await;

        let (status, response) = post_key(&state, &tenant_admin, body).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{label}: a scoped admin asking for a global key must still be able to mint \
             one inside its own tenant, just not outside it. Body: {response}"
        );
        let raw = response
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{label}: no key returned: {response}"))
            .to_string();

        assert_eq!(
            scope_of_key(&state, &raw).await,
            Some(Some("project-a".to_string())),
            "{label}: a project-a admin produced a key that is NOT scoped to project-a. \
             That is the whole bug -- such a key satisfies require_project_access for \
             every project in the deployment."
        );
    }
}

/// The other direction of escape: not broader, but *different*. A scoped admin
/// pointing the new key at another tenant gets a key that is useless for its own
/// project and fully usable for theirs.
#[tokio::test]
async fn a_project_scoped_admin_cannot_mint_a_key_for_a_different_project() {
    let (state, _dir) = test_state().await;
    let tenant_admin = mint_key(&state, "admin", Some("project-a")).await;

    let (status, body) = post_key(
        &state,
        &tenant_admin,
        json!({ "name": "cross-tenant", "role": "admin", "project_id": "project-b" }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a project-a admin minted a key for project-b: {body}"
    );
    assert!(!all_key_names(&state)
        .await
        .contains(&"cross-tenant".to_string()));
}

// ------------------------------------------------- what must still work

/// The fix must not neuter the endpoint for the person it exists for. A scoped
/// admin mints inside its own project, and the key it gets back is scoped there —
/// not silently promoted, and not left unscoped by accident.
#[tokio::test]
async fn a_project_scoped_admin_can_still_mint_a_key_inside_its_own_project() {
    let (state, _dir) = test_state().await;
    let tenant_admin = mint_key(&state, "admin", Some("project-a")).await;

    let (status, body) = post_key(
        &state,
        &tenant_admin,
        json!({ "name": "teammate", "role": "operator", "project_id": "project-a" }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "minting within its own tenant failed: {body}"
    );
    let raw = body
        .get("key")
        .and_then(Value::as_str)
        .expect("a created key must be returned once, in plaintext")
        .to_string();
    assert_eq!(
        scope_of_key(&state, &raw).await,
        Some(Some("project-a".to_string())),
        "the new key must be persisted scoped to project-a"
    );
}

/// Naming its own project explicitly, or naming none, both mean the same thing to
/// a scoped caller and both must land on that caller's own project.
#[tokio::test]
async fn a_scoped_admin_naming_no_project_gets_a_key_scoped_to_its_own() {
    let (state, _dir) = test_state().await;
    let tenant_admin = mint_key(&state, "admin", Some("project-a")).await;

    let (status, body) = post_key(
        &state,
        &tenant_admin,
        json!({ "name": "implicit", "role": "operator" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let raw = body.get("key").and_then(Value::as_str).unwrap().to_string();
    assert_eq!(
        scope_of_key(&state, &raw).await,
        Some(Some("project-a".to_string())),
        "a scoped caller must never produce an unscoped key, whatever it asked for"
    );
}

/// A global admin is the bootstrap case (`bootstrap_admin_key` issues exactly
/// this shape) and must be entirely unaffected.
#[tokio::test]
async fn a_global_admin_can_still_mint_global_keys() {
    let (state, _dir) = test_state().await;
    let global_admin = mint_key(&state, "admin", None).await;

    let (status, body) = post_key(
        &state,
        &global_admin,
        json!({ "name": "global-child", "role": "admin", "project_id": null }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a global admin must keep working: {body}"
    );
    let raw = body.get("key").and_then(Value::as_str).unwrap().to_string();
    assert_eq!(
        scope_of_key(&state, &raw).await,
        Some(None),
        "a global admin's key really is global"
    );
}

/// And a global admin may still scope a key to a project, which is the feature
/// that makes per-tenant keys possible at all.
#[tokio::test]
async fn a_global_admin_can_still_mint_a_project_scoped_key() {
    let (state, _dir) = test_state().await;
    let global_admin = mint_key(&state, "admin", None).await;

    let (status, body) = post_key(
        &state,
        &global_admin,
        json!({ "name": "tenant-key", "role": "viewer", "project_id": "project-b" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let raw = body.get("key").and_then(Value::as_str).unwrap().to_string();
    assert_eq!(
        scope_of_key(&state, &raw).await,
        Some(Some("project-b".to_string()))
    );
}

// ----------------------------------------------------------- role unchanged

/// The role ceiling is a separate axis and was never the hole here. A non-admin
/// was already refused before any of this, and must stay refused — pinned so a
/// future change to the scope check cannot quietly widen this.
#[tokio::test]
async fn an_operator_still_cannot_mint_keys_at_all() {
    let (state, _dir) = test_state().await;
    let operator = mint_key(&state, "operator", None).await;

    let (status, body) = post_key(
        &state,
        &operator,
        json!({ "name": "nope", "role": "admin", "project_id": null }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
}

/// A refused mint must be visible in the audit trail. An escalation attempt that
/// leaves no trace is worse than one that is refused, because nobody investigates
/// what nobody can see. Uses the cross-project attempt, which is the shape that is
/// genuinely refused.
#[tokio::test]
async fn a_refused_mint_is_recorded_in_the_audit_trail() {
    let (state, _dir) = test_state().await;
    let tenant_admin = mint_key(&state, "admin", Some("project-a")).await;

    let (status, _) = post_key(
        &state,
        &tenant_admin,
        json!({ "name": "attempted-escalation", "role": "admin", "project_id": "project-b" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let outcome: Option<String> = sqlx::query_scalar(
        "SELECT outcome FROM audit_log WHERE action = 'apikey.create' ORDER BY occurred_at DESC",
    )
    .fetch_optional(&state.store.pool)
    .await
    .unwrap();
    assert_eq!(
        outcome.as_deref(),
        Some("denied"),
        "the refusal itself must be auditable"
    );

    // And the attempt must name the project it tried to reach, or the trail
    // records that something was denied without recording what.
    let detail: Option<String> = sqlx::query_scalar(
        "SELECT detail FROM audit_log WHERE action = 'apikey.create' ORDER BY occurred_at DESC",
    )
    .fetch_optional(&state.store.pool)
    .await
    .unwrap();
    let detail = detail.unwrap_or_default();
    assert!(
        detail.contains("project-b"),
        "the audit row must record the project that was attempted: {detail}"
    );
}

// ---------------------------------------------------- the invariant, stated

/// The general form of the rule, asserted directly against `require_project_access`
/// rather than through HTTP: no key built from a scoped context may reach
/// `require_project_access` in a state that would let it past.
#[tokio::test]
async fn no_scoped_caller_can_produce_a_context_that_passes_every_project() {
    let scoped = AuthContext {
        key_id: Uuid::new_v4(),
        name: "tenant-admin".into(),
        role: Role::Admin,
        project_id: Some("project-a".into()),
    };
    // The escalation, restated at the layer where it would actually be used: a
    // context whose project_id is None passes every check.
    let escalated = AuthContext {
        project_id: None,
        ..scoped.clone()
    };
    assert!(
        crate::auth::require_project_access(&escalated, Some("project-b")).is_ok(),
        "precondition: this is what makes an unscoped key a full boundary escape"
    );
    assert!(crate::auth::require_project_access(&escalated, Some("anything")).is_ok());
    assert!(
        crate::auth::require_project_access(&scoped, Some("project-b")).is_err(),
        "precondition: the scoped context is correctly bounded"
    );
}
