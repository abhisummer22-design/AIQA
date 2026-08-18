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
    http::header,
    middleware::Next,
    response::{IntoResponse, Response},
};
use rand::Rng;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
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
        .insert_api_key(Uuid::new_v4(), "bootstrap-admin", &hash_key(&raw), "admin", None)
        .await?;
    Ok(Some(raw))
}

/// Axum middleware: every route it wraps requires `Authorization: Bearer
/// <key>`. `/health` is intentionally routed outside this layer (see
/// `api.rs::router`) so basic liveness checks stay unauthenticated, matching
/// common orchestrator/k8s conventions.
pub async fn require_auth(State(store): State<Store>, mut req: Request, next: Next) -> Response {
    let header_val = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let token = match header_val.as_deref().and_then(|h| h.strip_prefix("Bearer ")) {
        Some(t) if !t.is_empty() => t,
        _ => {
            let _ = store
                .insert_audit(None, None, None, "auth.fail", None, Some("missing or malformed Authorization header"), "denied")
                .await;
            return ApiError::unauthenticated("missing or malformed Authorization: Bearer <key> header").into_response();
        }
    };

    let hash = hash_key(token);
    match store.find_api_key_by_hash(&hash).await {
        Ok(Some(row)) => {
            req.extensions_mut().insert(AuthContext {
                key_id: row.id,
                name: row.name,
                role: Role::parse(&row.role),
                project_id: row.project_id,
            });
            next.run(req).await
        }
        Ok(None) => {
            let _ = store
                .insert_audit(None, None, None, "auth.fail", None, Some("unknown or revoked API key"), "denied")
                .await;
            ApiError::unauthenticated("invalid or revoked API key").into_response()
        }
        Err(e) => ApiError::internal(format!("auth lookup failed: {e}")).into_response(),
    }
}

/// Returns `Err(403)` if `ctx.role` doesn't meet `min` — call at the top of
/// any handler that needs more than viewer-level access.
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

/// Multi-tenancy gate: an `Admin`/global key (`project_id: None`) may act on
/// anything; a scoped key may only act on its own project. `resource_pid`
/// is the project the target run/resource actually belongs to.
pub fn require_project_access(ctx: &AuthContext, resource_pid: Option<&str>) -> Result<(), ApiError> {
    match &ctx.project_id {
        None => Ok(()),
        Some(pid) => {
            if resource_pid == Some(pid.as_str()) {
                Ok(())
            } else {
                Err(ApiError::forbidden("resource belongs to a different project"))
            }
        }
    }
}
