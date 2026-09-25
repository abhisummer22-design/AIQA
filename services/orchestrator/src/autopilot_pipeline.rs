//! How an autopilot run calls this service's own API.
//!
//! A run does nothing a person could not do through the routes the application already has (start a crawl, check the
//! gaps, generate cases, compile and run them), so its steps call exactly those routes. `PipelineApi` is the one place
//! they do it from; `InternalApi` is the real one, which sends each request through the router in process, as the
//! person who started the run, so every check the routes make (role, project, policy, audit) applies to the run as it
//! would to that person. The steps' own tests use a scripted one instead.

use crate::auth::{self, AuthContext, Role};
use crate::state::AppState;
use crate::store::Store;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

/// The largest answer read back. A run asks for summaries; a crawl's whole result is the biggest thing it might meet.
const MAX_REPLY_BYTES: usize = 32 * 1024 * 1024;

/// What the API answered: the status, and the body as JSON (`null` when there was none; text that is not JSON is a JSON
/// string).
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

impl Reply {
    pub fn new(status: u16, body: Value) -> Self {
        Reply { status, body }
    }

    /// An answer in the shape the API gives its errors (`{"error": {"code", "message"}}`).
    pub fn error(status: u16, code: &str, message: impl Into<String>) -> Self {
        Reply {
            status,
            body: json!({"error": {"code": code, "message": message.into()}}),
        }
    }

    pub fn is_ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Why a request failed, in words: the message of the API's error, or else what came back.
    pub fn problem(&self) -> String {
        if let Some(message) = self.body.pointer("/error/message").and_then(Value::as_str) {
            return message.to_string();
        }
        let text = match &self.body {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        let text: String = text.chars().take(300).collect();
        if text.is_empty() {
            format!("the service answered {}", self.status)
        } else {
            format!("the service answered {}: {text}", self.status)
        }
    }
}

/// A way to call the API.
#[async_trait]
pub trait PipelineApi: Send + Sync {
    /// `path` starts at `/api/...`; `body`, when there is one, is JSON.
    async fn send(&self, method: &str, path: &str, body: Option<Value>) -> Reply;
}

impl dyn PipelineApi {
    pub async fn get(&self, path: &str) -> Reply {
        self.send("GET", path, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Reply {
        self.send("POST", path, Some(body)).await
    }
}

/// The real thing: the service's own routes, called in process with the identity of the key that made the run.
pub struct InternalApi {
    router: Router,
    store: Store,
    key: Uuid,
}

impl InternalApi {
    pub fn new(state: &AppState, key: Uuid) -> Self {
        InternalApi {
            router: crate::api::internal_router(state.clone()),
            store: state.store.clone(),
            key,
        }
    }
}

#[async_trait]
impl PipelineApi for InternalApi {
    async fn send(&self, method: &str, path: &str, body: Option<Value>) -> Reply {
        // Looked up for every call, so a key that is revoked stops the runs it made at their next call.
        let who = match self.store.find_active_api_key(self.key).await {
            Ok(Some(key)) => AuthContext {
                key_id: key.id,
                name: key.name,
                role: Role::parse(&key.role),
                project_id: key.project_id,
            },
            Ok(None) => {
                return Reply::error(
                    401,
                    "unauthenticated",
                    "the API key that started this run has been revoked",
                )
            }
            Err(e) => {
                return Reply::error(
                    500,
                    "internal",
                    format!("the API key that started this run could not be checked: {e}"),
                )
            }
        };
        let builder = Request::builder().method(method).uri(path);
        let request = match body {
            Some(body) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => builder.body(Body::empty()),
        };
        let mut request = match request {
            Ok(request) => request,
            Err(e) => return Reply::error(400, "invalid_request", format!("not a request: {e}")),
        };
        request.extensions_mut().insert(who);

        let response = auth::with_request_path(path, self.router.clone().oneshot(request))
            .await
            .unwrap_or_else(|never| match never {});
        let status = response.status().as_u16();
        let bytes = match axum::body::to_bytes(response.into_body(), MAX_REPLY_BYTES).await {
            Ok(bytes) => bytes,
            Err(e) => {
                return Reply::error(
                    502,
                    "unreadable",
                    format!("the answer could not be read: {e}"),
                )
            }
        };
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        Reply { status, body }
    }
}

#[cfg(test)]
pub(crate) mod scripted {
    //! A `PipelineApi` for the steps' tests: answers are given per method and path, requests are recorded.
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct ScriptedApi {
        /// `"GET /path"` → the answers to give, in order; the last one repeats.
        answers: Mutex<HashMap<String, VecDeque<Reply>>>,
        pub requests: Mutex<Vec<(String, String, Option<Value>)>>,
    }

    impl ScriptedApi {
        pub fn new() -> Self {
            Self::default()
        }

        /// Answers `method path` with `replies` in turn (the last one for every request after them).
        pub fn on(self, method: &str, path: &str, replies: Vec<Reply>) -> Self {
            self.answers
                .lock()
                .unwrap()
                .insert(format!("{method} {path}"), replies.into());
            self
        }

        pub fn seen(&self) -> Vec<String> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .map(|(m, p, _)| format!("{m} {p}"))
                .collect()
        }

        /// Every body sent to `method path`, in the order they were sent.
        pub fn bodies_of(&self, method: &str, path: &str) -> Vec<Value> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|(m, p, _)| m == method && p == path)
                .filter_map(|(_, _, body)| body.clone())
                .collect()
        }

        pub fn body_of(&self, method: &str, path: &str) -> Option<Value> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .find(|(m, p, _)| m == method && p == path)
                .and_then(|(_, _, body)| body.clone())
        }
    }

    #[async_trait]
    impl PipelineApi for ScriptedApi {
        async fn send(&self, method: &str, path: &str, body: Option<Value>) -> Reply {
            self.requests
                .lock()
                .unwrap()
                .push((method.to_string(), path.to_string(), body));
            let mut answers = self.answers.lock().unwrap();
            match answers.get_mut(&format!("{method} {path}")) {
                Some(queue) if queue.len() > 1 => queue.pop_front().unwrap(),
                Some(queue) => queue.front().cloned().unwrap(),
                None => Reply::error(
                    404,
                    "not_found",
                    format!("nothing scripted for {method} {path}"),
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scripted::ScriptedApi;
    use super::*;

    #[test]
    fn an_answer_is_ok_from_200_to_299() {
        assert!(!Reply::new(199, Value::Null).is_ok());
        assert!(Reply::new(200, Value::Null).is_ok());
        assert!(Reply::new(202, Value::Null).is_ok());
        assert!(Reply::new(299, Value::Null).is_ok());
        assert!(!Reply::new(300, Value::Null).is_ok());
        assert!(!Reply::new(404, Value::Null).is_ok());
    }

    #[test]
    fn a_problem_is_the_apis_own_message_or_else_what_came_back() {
        assert_eq!(
            Reply::error(422, "validation", "roles must be between 1 and 20").problem(),
            "roles must be between 1 and 20"
        );
        assert_eq!(
            Reply::new(500, Value::String("boom".into())).problem(),
            "the service answered 500: boom"
        );
        assert_eq!(
            Reply::new(502, Value::Null).problem(),
            "the service answered 502"
        );
        assert_eq!(
            Reply::new(400, json!({"detail": "x"})).problem(),
            r#"the service answered 400: {"detail":"x"}"#
        );
        let long = Reply::new(500, Value::String("y".repeat(1000))).problem();
        assert_eq!(long.chars().filter(|c| *c == 'y').count(), 300, "cut");
    }

    #[tokio::test]
    async fn a_scripted_api_answers_in_order_repeats_the_last_and_records_what_it_was_asked() {
        let api = ScriptedApi::new().on(
            "GET",
            "/a",
            vec![Reply::new(200, json!(1)), Reply::new(200, json!(2))],
        );
        let api: &dyn PipelineApi = &api;
        assert_eq!(api.get("/a").await.body, json!(1));
        assert_eq!(api.get("/a").await.body, json!(2));
        assert_eq!(
            api.get("/a").await.body,
            json!(2),
            "the last answer repeats"
        );
        let missing = api.post("/b", json!({"x": 1})).await;
        assert_eq!(missing.status, 404);
        assert!(missing.problem().contains("nothing scripted for POST /b"));
    }
}
