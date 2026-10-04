//! API-key auth, roles, and project scoping — flagged in the original PRD
//! critique ("No auth/roles/multi-tenancy anywhere — but 'governed
//! autonomous execution' and human-approval gates presuppose exactly that")
//! and confirmed still absent in the prior session's build report ("Auth /
//! authz / multi-tenancy: Completely absent").
//!
//! Deliberately simple: opaque random bearer tokens (`aiqa_<32 random
//! bytes, hex>`), only their SHA-256 hash ever persisted (same shape as
//! GitHub PATs / Stripe secret keys), three roles (`admin` > `operator` >
//! `viewer`), and `project_id` scoping (`None` = global/admin, sees every
//! project; `Some(pid)` = restricted to that project's runs). No OAuth, no
//! sessions, no password hashing library — a network-reachable orchestrator
//! API needs *some* gate before "who called POST /runs" is even a
//! meaningful question, and this is the minimum that makes it one.

use crate::errors::ApiError;
use crate::store::Store;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rand::Rng;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Operator,
    Admin,
}

impl Role {
    pub fn parse(s: &str) -> Self {
        match s {
            "admin" => Role::Admin,
            "operator" => Role::Operator,
            _ => Role::Viewer,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Operator => "operator",
            Role::Admin => "admin",
        }
    }
    pub fn at_least(&self, min: Role) -> bool {
        *self >= min
    }
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub key_id: Uuid,
    pub name: String,
    pub role: Role,
    /// `None` = global scope (sees/acts on every project). Only meaningful
    /// for `Admin` keys in practice — `bootstrap_admin_key` issues one with
    /// no project so there's always at least one key that can see
    /// everything.
    pub project_id: Option<String>,
}

tokio::task_local! {
    /// The project a request is about, read from its URL (`/api/v1/projects/:project_id/...`), for the
    /// duration of that request. A global-scope key has `AuthContext::project_id == None`, so without this an
    /// audit row written while it acts inside a project was attributed to no project, and never showed up in
    /// that project's own audit trail.
    static REQUEST_PROJECT: Option<String>;
}

/// The project named in the current request's URL, if there is one.
pub fn request_project() -> Option<String> {
    REQUEST_PROJECT.try_with(|p| p.clone()).ok().flatten()
}

/// Runs `fut` the way a request to `path` runs: with the project its URL names available to `request_project`.
/// `require_auth` does this for every request from outside; the autopilot's in-process calls do not pass through it,
/// so they do it themselves.
pub(crate) async fn with_request_path<F: std::future::Future>(path: &str, fut: F) -> F::Output {
    REQUEST_PROJECT.scope(project_from_path(path), fut).await
}

/// `/api/v1/projects/<uuid>/...` gives `<uuid>` in its canonical form; anything else (`/api/v1/projects`, a
/// path that names no project, a malformed id) gives `None`.
pub(crate) fn project_from_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/projects/")?;
    Uuid::parse_str(rest.split('/').next()?)
        .ok()
        .map(|id| id.to_string())
}

pub fn generate_key() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    format!("aiqa_{}", hex::encode(bytes))
}

pub fn hash_key(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

/// On first startup with zero keys in the DB, mint one admin key and print
/// it once — same pattern as a root/bootstrap token. There is no other way
/// to get a first key in (by design: an unauthenticated "create the first
/// admin" endpoint would just move the hole somewhere else).
pub async fn bootstrap_admin_key_if_needed(store: &Store) -> anyhow::Result<Option<String>> {
    if store.count_active_api_keys().await? > 0 {
        return Ok(None);
    }
    let raw = generate_key();
    store
        .insert_api_key(
            Uuid::new_v4(),
            "bootstrap-admin",
            &hash_key(&raw),
            "admin",
            None,
        )
        .await?;
    Ok(Some(raw))
}

/// Axum middleware: every route it wraps requires `Authorization: Bearer
/// <key>`. `/health` is intentionally routed outside this layer (see
/// `api.rs::router`) so basic liveness checks stay unauthenticated, matching
/// common orchestrator/k8s conventions.
pub async fn require_auth(
    State(state): State<crate::state::AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let store = state.store.clone();
    let header_val = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let path = req.uri().path().to_string();
    let method = req.method().to_string();

    let token = match header_val
        .as_deref()
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        Some(t) if !t.is_empty() => t,
        _ => {
            let _ = store
                .insert_audit(
                    None,
                    None,
                    None,
                    "auth.fail",
                    None,
                    Some("missing or malformed Authorization header"),
                    "denied",
                )
                .await;
            state
                .log_bus
                .emit(
                    "WARN",
                    "auth",
                    &format!("401 {method} {path} — missing or malformed Authorization header"),
                    None,
                    None,
                    None,
                )
                .await;
            return ApiError::unauthenticated(
                "missing or malformed Authorization: Bearer <key> header",
            )
            .into_response();
        }
    };

    let hash = hash_key(token);
    match store.find_api_key_by_hash(&hash).await {
        Ok(Some(row)) => {
            let ctx = AuthContext {
                key_id: row.id,
                name: row.name,
                role: Role::parse(&row.role),
                project_id: row.project_id,
            };
            // Kept for the post-response authorization check below — the
            // request itself takes ownership of `ctx`.
            let actor = ctx.name.clone();
            let actor_role = ctx.role.as_str();
            let actor_project = ctx.project_id.clone();
            req.extensions_mut().insert(ctx);

            let response = with_request_path(&path, next.run(req)).await;

            // Authorization denials (403) are produced by `require_role` /
            // `require_project_access`, which are pure functions with no
            // access to the store or the log bus. Recording them here — the
            // one place every protected request already passes through —
            // captures every denial without duplicating a call into dozens
            // of handlers, and without either helper growing a dependency on
            // application state. Before this, a 403 was returned to the
            // caller and left no trace anywhere: the audit log only ever
            // recorded 401s, so there was no real record of role-boundary
            // enforcement to show.
            if response.status() == StatusCode::FORBIDDEN {
                let detail = format!("{method} {path} denied for role '{actor_role}'");
                let _ = store
                    .insert_audit(
                        Some(&actor),
                        Some(actor_role),
                        actor_project.as_deref(),
                        "authz.deny",
                        None,
                        Some(&detail),
                        "denied",
                    )
                    .await;
                state
                    .log_bus
                    .emit(
                        "WARN",
                        "auth",
                        &format!("403 {method} {path} — role '{actor_role}' not permitted"),
                        None,
                        actor_project.as_deref(),
                        None,
                    )
                    .await;
            }
            response
        }
        Ok(None) => {
            let _ = store
                .insert_audit(
                    None,
                    None,
                    None,
                    "auth.fail",
                    None,
                    Some("unknown or revoked API key"),
                    "denied",
                )
                .await;
            state
                .log_bus
                .emit(
                    "WARN",
                    "auth",
                    &format!("401 {method} {path} — unknown or revoked API key"),
                    None,
                    None,
                    None,
                )
                .await;
            ApiError::unauthenticated("invalid or revoked API key").into_response()
        }
        Err(e) => ApiError::internal(format!("auth lookup failed: {e}")).into_response(),
    }
}

/// Returns `Err(403)` if `ctx.role` doesn't meet `min`.
///
/// Prefer `RequireOperator` / `RequireAdmin` as a handler *argument* over calling
/// this in the handler body. See the extractor definitions below for why; this
/// function stays for the callers that genuinely cannot take an argument (helpers,
/// closures, and the tests that call it directly).
pub fn require_role(ctx: &AuthContext, min: Role) -> Result<(), ApiError> {
    if ctx.role.at_least(min) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "role '{}' cannot perform this action (requires '{}')",
            ctx.role.as_str(),
            min.as_str()
        )))
    }
}

/// Role gates that run **before the request body is read**.
///
/// ## Why this exists
///
/// `require_role` used to be called inside handler bodies, which meant it ran
/// after every argument extractor. `Json<T>` is a `FromRequest` extractor, so a
/// request whose body failed to deserialize was rejected with 422 *without the
/// role ever being checked*. Three consequences, all measured against the running
/// server rather than reasoned about:
///
///  - An authenticated caller with too weak a role could read the request schema
///    of any route they may not call. `POST /api/v1/projects/:id/test-cases` with
///    `{}` answers a viewer `"missing field 'title'"`, and so answers an admin.
///  - `healing/approve` and `healing/execute` look the proposal up before the
///    role check, so they answer 404 rather than 403 -- which tells an unauthorized
///    caller whether a proposal id exists.
///  - The 422 and the 404 were indistinguishable, to a client, from the refusals
///    they were meant to be.
///
/// ## How it is fixed
///
/// These types implement `FromRequestParts`, which axum runs for *all* arguments
/// before it runs the single `FromRequest` (body) argument. So a handler that
/// takes `RequireOperator` refuses an under-privileged caller before axum looks at
/// a single byte of the body, and the 403 always wins over the 422.
///
/// The role is declared at the handler's own signature, next to the code it
/// protects, so there is no separate table that can drift out of agreement with
/// what the code does -- which is the failure mode that killed the earlier
/// attempts at an RBAC matrix in this repo (see `rbac_probe`).
macro_rules! role_gate {
    ($name:ident, $role:expr, $what:literal) => {
        #[doc = concat!("Refuses the request unless the caller is ", $what, " or above,",
                        " before the body is read. See `auth`'s module notes.")]
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        #[async_trait::async_trait]
        impl<S> axum::extract::FromRequestParts<S> for $name
        where
            S: Send + Sync,
        {
            type Rejection = ApiError;

            async fn from_request_parts(
                parts: &mut axum::http::request::Parts,
                _state: &S,
            ) -> Result<Self, Self::Rejection> {
                let ctx = parts
                    .extensions
                    .get::<AuthContext>()
                    .ok_or_else(|| {
                        ApiError::unauthenticated(
                            "no authenticated context on this request -- the auth \
                             middleware did not run",
                        )
                    })?;
                require_role(ctx, $role)?;
                Ok($name)
            }
        }
    };
}

role_gate!(RequireViewer, Role::Viewer, "a viewer");
role_gate!(RequireOperator, Role::Operator, "an operator");
role_gate!(RequireAdmin, Role::Admin, "an admin");

/// Multi-tenancy gate: an `Admin`/global key (`project_id: None`) may act on
/// anything; a scoped key may only act on its own project. `resource_pid`
/// is the project the target run/resource actually belongs to.
pub fn require_project_access(
    ctx: &AuthContext,
    resource_pid: Option<&str>,
) -> Result<(), ApiError> {
    match &ctx.project_id {
        None => Ok(()),
        Some(pid) => {
            if resource_pid == Some(pid.as_str()) {
                Ok(())
            } else {
                Err(ApiError::forbidden(
                    "resource belongs to a different project",
                ))
            }
        }
    }
}
