//! BE-9/BE-10 — the single structured error representation for the REST API
//! (API Contract §2.5). Every handler in `api.rs`, plus the auth gates in
//! `auth.rs` (`require_auth`, `require_role`, `require_project_access`),
//! converge on this one type so the JSON envelope is built in exactly one
//! place rather than re-serialized ad hoc per call site.
//!
//! BE-10 closes the one gap BE-9 disclosed but did not fix: a malformed
//! request body, an invalid path parameter, or an unparseable query string
//! is rejected by Axum's extractors *before* a handler body ever runs, so
//! BE-9's handler-level conversion never saw them — they still returned
//! Axum's own default plain-text rejection. `ApiJson`/`ApiPath`/`ApiQuery`
//! below are thin wrappers around Axum's own `Json`/`Path`/`Query` that
//! convert exactly that rejection into `ApiError`, preserving Axum's own
//! status code for each rejection kind (Step 5: do not change existing
//! status-code behavior unless the contract requires it — the contract
//! specifies no fine-grained status for "malformed JSON" vs "wrong
//! Content-Type" vs "bad path parameter", so none of those are changed here,
//! only their body shape).
//!
//! Scope note: `api.rs::project_events_ws_handler`'s pre-upgrade
//! rejections are
//! deliberately NOT converted to this envelope. A WebSocket handshake
//! failure is not a JSON API response a browser's `WebSocket` client parses
//! as JSON, and G-WS (WebSocket auth/transport semantics) is an explicitly
//! unresolved, out-of-scope contract decision for this unit — see
//! `AIQA_PHASE2_BE9_IMPLEMENTATION_REPORT.md`.

use axum::{
    extract::{
        FromRequest, FromRequestParts, Json as AxumJson, Path as AxumPath, Query as AxumQuery,
        Request,
    },
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
    /// Contract §2.5's example shows an empty object; nothing in this API's
    /// error paths currently has structured per-field detail worth carrying
    /// (e.g. a list of invalid fields), so this is always `{}` for now
    /// rather than inventing content the contract doesn't ask for.
    details: serde_json::Value,
    request_id: String,
}

/// The one structured error type every REST handler returns. Carries an
/// HTTP status alongside the contract's `{code, message, details,
/// request_id}` envelope.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    /// What went wrong, in words (the `message` of the envelope).
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 401 — missing, malformed, or unrecognized credentials.
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED", message)
    }

    /// 403 — a real, recognized identity that isn't permitted to do this
    /// (wrong role, or a project-scoped key acting outside its own project).
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "FORBIDDEN", message)
    }

    /// 404 — reuses the contract's own §2.5 example code verbatim.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "RESOURCE_NOT_FOUND", message)
    }

    /// 400 — a request the caller could fix by sending different input.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", message)
    }

    /// 409 — the target resource exists but isn't in a state that allows
    /// this action (e.g. approving a run that isn't PENDING_APPROVAL).
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "CONFLICT", message)
    }

    /// 500 — covers both "Persistence" and "Internal" from BE-9's category
    /// list. The contract's error schema doesn't distinguish them (no
    /// separate code is given or implied for a DB failure specifically),
    /// and to the API consumer both are equally "something went wrong on
    /// the server, retry or report it" — the deeper distinction Phase 1's
    /// `note_persistence_failure` mechanism cares about is a durability
    /// concern inside a run's own lifecycle, orthogonal to what HTTP
    /// envelope a synchronous request handler returns.
    ///
    /// Per Contract §2.5 ("Frontend MUST NOT expose secrets, internal stack
    /// traces, credentials, or sensitive infrastructure details"): the real
    /// error (which today can be a raw SQL error such as `database is
    /// locked` or a constraint-violation string naming column/table
    /// internals) is logged server-side via `tracing::error!` and never
    /// placed in the response body. The client sees a fixed, safe message.
    pub fn internal(context: impl std::fmt::Display) -> Self {
        tracing::error!("internal error: {context}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "An internal error occurred.",
        )
    }

    /// BE-10 — a request Axum itself rejected before any handler ran
    /// (malformed JSON, wrong Content-Type, unparseable path/query value).
    /// Always `VALIDATION_ERROR`: every one of these means "the request
    /// itself cannot be processed as sent," the same category as the
    /// handler-level validation failures above — just caught one layer
    /// earlier. The status is whatever Axum's own rejection already
    /// determined (400 for bad JSON/path/query, 415 for a missing/wrong
    /// Content-Type, etc.); only the body changes.
    fn from_rejection(status: StatusCode, message: impl Into<String>) -> Self {
        Self::new(status, "VALIDATION_ERROR", message)
    }
}

/// BE-10 — drop-in replacement for `axum::extract::Json<T>` whose rejection
/// is the `ApiError` envelope instead of Axum's default plain text.
pub struct ApiJson<T>(pub T);

#[async_trait::async_trait]
impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match AxumJson::<T>::from_request(req, state).await {
            Ok(AxumJson(value)) => Ok(ApiJson(value)),
            Err(rejection) => {
                let message = rejection.body_text();
                Err(ApiError::from_rejection(rejection.status(), message))
            }
        }
    }
}

/// BE-10 — drop-in replacement for `axum::extract::Path<T>`.
pub struct ApiPath<T>(pub T);

#[async_trait::async_trait]
impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    T: serde::de::DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match AxumPath::<T>::from_request_parts(parts, state).await {
            Ok(AxumPath(value)) => Ok(ApiPath(value)),
            Err(rejection) => {
                let message = rejection.body_text();
                Err(ApiError::from_rejection(rejection.status(), message))
            }
        }
    }
}

/// BE-10 — drop-in replacement for `axum::extract::Query<T>`.
pub struct ApiQuery<T>(pub T);

#[async_trait::async_trait]
impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    T: serde::de::DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match AxumQuery::<T>::from_request_parts(parts, state).await {
            Ok(AxumQuery(value)) => Ok(ApiQuery(value)),
            Err(rejection) => {
                let message = rejection.body_text();
                Err(ApiError::from_rejection(rejection.status(), message))
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let request_id = format!("req_{}", uuid::Uuid::new_v4().simple());
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code,
                message: self.message,
                details: serde_json::json!({}),
                request_id,
            },
        };
        (self.status, Json(body)).into_response()
    }
}
