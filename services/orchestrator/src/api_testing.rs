//! Phase 4 — API Testing page: real, ad-hoc HTTP request execution.
//!
//! Fresh baseline audit before writing this module: the frontend's
//! `APITestingPage.tsx` was already a truthful, non-fabricated redirect to
//! the Execution Hub (a product-owner decision, ACC-012) — real API testing
//! already existed via `NewmanExecutor`/`SchemathesisExecutor` (fixed
//! fixtures only) and, since Phase 2.1, a genuinely dynamic path:
//! `execution_plan::ExecutionPlan` (`Action::Request`/`AssertStatus`/
//! `AssertHeader`/`AssertBodyField`) dispatched to the real, native
//! `executor::CompiledApiExecutor` (direct `reqwest`, real assertion
//! evaluation against the real response) — but only ever reachable by
//! compiling a persisted test case's natural-language steps first.
//!
//! This module adds the one genuinely missing piece: constructing an
//! `ExecutionPlan` **directly** from a user's own explicit request-builder
//! input (method/URL/query/headers/body/assertions) — no natural-language
//! compilation involved, because the user IS the compiler here, typing each
//! field explicitly. Everything downstream is 100% reused, unmodified:
//! `execution_plan::validate_plan` (the same real SSRF/structural guard),
//! `execution_plan::materialize_task_def`, `create_run_core` ->
//! `engine::spawn_run` -> `ExecutorRegistry::resolve` ->
//! `CompiledApiExecutor` -> a real, live HTTP request. No new executor, no
//! new execution pipeline, no new persistence path.
//!
//! ```text
//! AdHocApiRequest (method, url, query_params, headers, body, assertions)
//!         v
//! validate_and_build_plan()  -- real method/URL validation, builds an
//!                                ExecutionPlan{compiler: Manual} directly
//!         v
//! execution_plan::validate_plan()  -- REUSED, unmodified (real SSRF guard)
//!         v
//! execution_plan::materialize_task_def()  -- REUSED, unmodified
//!         v
//! api.rs::create_run_core()  -- REUSED, unmodified (real run/task/policy)
//!         v
//! run_to_completion()  -- polls this same process's own in-memory run
//!                          record (the run was just created in this same
//!                          request; no cross-process fallback needed)
//!         v
//! read_task_evidence() + parse_result_summary()  -- reads the real,
//!         persisted evidence log CompiledApiExecutor itself wrote, and
//!         extracts its own real, structured RESULT_SUMMARY_JSON line
//! ```

use crate::execution_plan::{Action, CompilerMethod, ExecutionPlan, ExecutionType};
use crate::executor::ApiResultSummary;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

/// The one closed, real HTTP method set this page exposes — matches what
/// `CompiledApiExecutor`'s own `reqwest::Method::from_bytes` genuinely
/// accepts for every method a REST API test realistically needs; nothing
/// exotic (CONNECT/TRACE) that this UI has no real use case for.
pub const SUPPORTED_METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdHocApiRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub query_params: Vec<(String, String)>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    /// Reuses `execution_plan::Action`'s own real, closed wire shape
    /// directly — only `AssertStatus`/`AssertHeader`/`AssertBodyField`
    /// variants are valid here (checked in `validate_and_build_plan`); a
    /// `Request`/browser-action variant sent here is a real, honest
    /// `INVALID_REQUEST_CONFIGURATION` rejection, not silently dropped.
    #[serde(default)]
    pub assertions: Vec<Action>,
}

/// `(error_code, message)` — the caller (`api.rs`) prefixes the message
/// with `error_code` in the existing `"{CODE}: {message}"` convention
/// already established by `execution_plan`/`custom_testing`'s own honest
/// failure reporting, rather than inventing a second error-shape convention.
pub type ValidationError = (&'static str, String);

fn build_url(base: &str, query_params: &[(String, String)]) -> Result<String, String> {
    let mut url = reqwest::Url::parse(base.trim())
        .map_err(|e| format!("{base:?} is not a valid URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!(
            "unsupported URL scheme {:?} — only http and https are allowed",
            url.scheme()
        ));
    }
    if !query_params.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in query_params {
            if !k.trim().is_empty() {
                pairs.append_pair(k, v);
            }
        }
    }
    Ok(url.to_string())
}

/// Real request validation (Phase 4 brief §6) — every rejection here is a
/// genuine structural problem with the caller's own input, checked before
/// any network call, before any run is even created. The real SSRF/target
/// check happens one step later, inside the reused `execution_plan::
/// validate_plan` — not duplicated here.
pub fn validate_and_build_plan(
    id: String,
    req: &AdHocApiRequest,
) -> Result<ExecutionPlan, ValidationError> {
    let method = req.method.trim().to_uppercase();
    if !SUPPORTED_METHODS.contains(&method.as_str()) {
        return Err((
            "UNSUPPORTED_HTTP_METHOD",
            format!(
                "HTTP method {:?} is not supported — use one of {SUPPORTED_METHODS:?}",
                req.method
            ),
        ));
    }

    if req.url.trim().is_empty() {
        return Err(("INVALID_URL", "URL must not be empty".to_string()));
    }
    let url_with_query = build_url(&req.url, &req.query_params).map_err(|e| ("INVALID_URL", e))?;

    for a in &req.assertions {
        if !matches!(
            a,
            Action::AssertStatus { .. }
                | Action::AssertHeader { .. }
                | Action::AssertBodyField { .. }
        ) {
            return Err((
                "INVALID_REQUEST_CONFIGURATION",
                format!("{a:?} is not a supported assertion type for the API Testing page"),
            ));
        }
    }

    let mut actions = vec![Action::Request {
        method,
        url: url_with_query.clone(),
        headers: req.headers.clone(),
        body: req.body.clone(),
    }];
    actions.extend(req.assertions.iter().cloned());

    Ok(ExecutionPlan {
        test_case_id: id,
        execution_type: ExecutionType::Api,
        target: Some(url_with_query),
        actions,
        compiler: CompilerMethod::Manual,
        diagnostics: vec!["Built directly from the API Testing page's own request builder fields — not compiled from a test case.".to_string()],
        provider: None,
        model: None,
    })
}

/// Polls this same process's own in-memory run record — always present
/// here because the run was created by this exact request a moment
/// earlier (`create_run_core`, called by the same handler before this),
/// never a cross-process/restart scenario, so the store-fallback branch
/// `api.rs::get_run` needs for that case is genuinely not needed here.
pub async fn run_to_completion(
    state: &AppState,
    run_id: Uuid,
    ceiling: Duration,
) -> Option<crate::domain::TaskState> {
    let deadline = tokio::time::Instant::now() + ceiling;
    loop {
        if let Some(record) = state.runs.read().await.get(&run_id).cloned() {
            let tasks = record.engine.read().await.tasks.clone();
            if let Some(task) = tasks.values().next() {
                if task.status.is_terminal() {
                    return Some(task.clone());
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Reads the real evidence content `CompiledApiExecutor` itself wrote for
/// this task — the exact same store/read path `api.rs::
/// get_evidence_content_scoped` already uses, called in-process instead of
/// via a second HTTP round-trip.
pub async fn read_task_evidence(
    state: &AppState,
    run_id: Uuid,
    task_id: &str,
) -> Option<(Uuid, String)> {
    let rows = state.store.list_evidence_for_run(run_id).await.ok()?;
    let row = rows
        .into_iter()
        .find(|r| r.task_id.as_deref() == Some(task_id))?;
    let bytes = state.evidence.read(&row.object_key).ok()?;
    Some((row.id, String::from_utf8_lossy(&bytes).to_string()))
}

/// Extracts `CompiledApiExecutor`'s own real, structured summary from its
/// evidence log — a real parse of real, executor-produced data, not a
/// second, independently-computed result.
pub fn parse_result_summary(log: &str) -> Option<ApiResultSummary> {
    let line = log
        .lines()
        .find_map(|l| l.strip_prefix("RESULT_SUMMARY_JSON:"))?;
    serde_json::from_str(line).ok()
}

/// Extracts the real response body this executor's own log already
/// captured (`RESPONSE {status}\n{body}\n`) — a real substring of real
/// executor output, never re-fetched or re-derived independently. Ends at
/// the first line that is one of this same log format's own real markers
/// (`ASSERT_*`, `RESULT_SUMMARY_JSON:`, `FAILED ASSERTIONS:`) — a real
/// response body always ends where the next logged action begins.
pub fn parse_response_body_preview(log: &str) -> Option<String> {
    let lines: Vec<&str> = log.lines().collect();
    let response_idx = lines.iter().position(|l| l.starts_with("RESPONSE "))?;
    let end = lines
        .iter()
        .enumerate()
        .skip(response_idx + 1)
        .find(|(_, l)| {
            l.starts_with("ASSERT_STATUS ")
                || l.starts_with("ASSERT_STATUS_IN ")
                || l.starts_with("ASSERT_HEADER ")
                || l.starts_with("ASSERT_BODY_FIELD ")
                || l.starts_with("ASSERT_BODY_SCHEMA ")
                || l.starts_with("RESULT_SUMMARY_JSON:")
                || l.starts_with("FAILED ASSERTIONS:")
        })
        .map(|(i, _)| i)
        .unwrap_or(lines.len());
    Some(
        lines[(response_idx + 1)..end]
            .join("\n")
            .trim_end()
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, url: &str) -> AdHocApiRequest {
        AdHocApiRequest {
            method: method.to_string(),
            url: url.to_string(),
            query_params: vec![],
            headers: BTreeMap::new(),
            body: None,
            assertions: vec![],
        }
    }

    #[test]
    fn a_valid_get_request_builds_a_real_plan_with_manual_compiler_method() {
        let plan = validate_and_build_plan(
            "t1".to_string(),
            &req("GET", "https://example.com/api/health"),
        )
        .expect("must build");
        assert_eq!(plan.compiler, CompilerMethod::Manual);
        assert_eq!(plan.execution_type, ExecutionType::Api);
        assert!(
            matches!(&plan.actions[0], Action::Request { method, url, .. } if method == "GET" && url == "https://example.com/api/health")
        );
    }

    #[test]
    fn an_unsupported_method_is_rejected_before_any_plan_is_built() {
        let err = validate_and_build_plan("t1".to_string(), &req("TRACE", "https://example.com"))
            .unwrap_err();
        assert_eq!(err.0, "UNSUPPORTED_HTTP_METHOD");
    }

    #[test]
    fn an_empty_url_is_rejected() {
        let err = validate_and_build_plan("t1".to_string(), &req("GET", "")).unwrap_err();
        assert_eq!(err.0, "INVALID_URL");
    }

    #[test]
    fn a_malformed_url_is_rejected() {
        let err =
            validate_and_build_plan("t1".to_string(), &req("GET", "not a url at all")).unwrap_err();
        assert_eq!(err.0, "INVALID_URL");
    }

    #[test]
    fn a_non_http_scheme_is_rejected() {
        let err = validate_and_build_plan("t1".to_string(), &req("GET", "ftp://example.com/file"))
            .unwrap_err();
        assert_eq!(err.0, "INVALID_URL");
    }

    #[test]
    fn query_parameters_are_genuinely_merged_into_the_real_request_url() {
        let mut r = req("GET", "https://example.com/search");
        r.query_params = vec![
            ("q".to_string(), "hello world".to_string()),
            ("page".to_string(), "2".to_string()),
        ];
        let plan = validate_and_build_plan("t1".to_string(), &r).expect("must build");
        let Action::Request { url, .. } = &plan.actions[0] else {
            panic!("expected Request action")
        };
        assert!(
            url.contains("q=hello+world") || url.contains("q=hello%20world"),
            "url={url}"
        );
        assert!(url.contains("page=2"), "url={url}");
    }

    #[test]
    fn headers_and_body_genuinely_reach_the_built_plan() {
        let mut r = req("POST", "https://example.com/items");
        r.headers
            .insert("X-Custom".to_string(), "real-value".to_string());
        r.body = Some(serde_json::json!({"name": "widget"}));
        let plan = validate_and_build_plan("t1".to_string(), &r).expect("must build");
        let Action::Request { headers, body, .. } = &plan.actions[0] else {
            panic!("expected Request action")
        };
        assert_eq!(headers.get("X-Custom"), Some(&"real-value".to_string()));
        assert_eq!(body, &Some(serde_json::json!({"name": "widget"})));
    }

    #[test]
    fn assertions_genuinely_reach_the_built_plan_in_order() {
        let mut r = req("GET", "https://example.com/health");
        r.assertions = vec![
            Action::AssertStatus { status: 200 },
            Action::AssertHeader {
                name: "Content-Type".to_string(),
                value: "application/json".to_string(),
            },
        ];
        let plan = validate_and_build_plan("t1".to_string(), &r).expect("must build");
        assert_eq!(plan.actions.len(), 3);
        assert!(matches!(
            &plan.actions[1],
            Action::AssertStatus { status: 200 }
        ));
        assert!(matches!(&plan.actions[2], Action::AssertHeader { .. }));
    }

    #[test]
    fn a_non_assertion_action_sent_as_an_assertion_is_rejected_structurally() {
        let mut r = req("GET", "https://example.com/health");
        r.assertions = vec![Action::Navigate {
            url: "https://example.com".to_string(),
        }];
        let err = validate_and_build_plan("t1".to_string(), &r).unwrap_err();
        assert_eq!(err.0, "INVALID_REQUEST_CONFIGURATION");
    }

    #[test]
    fn result_summary_json_line_is_parsed_from_a_real_executor_log() {
        let log = "REQUEST GET http://x\nRESPONSE 200 OK\nok\nASSERT_STATUS 200 -> actual 200\nRESULT_SUMMARY_JSON:{\"status\":200,\"request_duration_ms\":12,\"response_headers\":[],\"assertions\":[{\"description\":\"Status equals 200\",\"actual\":\"200\",\"passed\":true}]}\n";
        let summary = parse_result_summary(log).expect("must parse");
        assert_eq!(summary.status, 200);
        assert_eq!(summary.request_duration_ms, 12);
        assert_eq!(summary.assertions.len(), 1);
        assert!(summary.assertions[0].passed);
    }

    #[test]
    fn response_body_preview_is_extracted_from_a_real_executor_log_and_stops_before_the_next_logged_action(
    ) {
        let log = "REQUEST GET http://x\nRESPONSE 200 OK\n{\"ok\":true}\nASSERT_STATUS 200 -> actual 200\nRESULT_SUMMARY_JSON:{}\n";
        let body = parse_response_body_preview(log).expect("must parse");
        assert_eq!(
            body, "{\"ok\":true}",
            "the body must not swallow the subsequent ASSERT_STATUS log line"
        );
    }

    #[test]
    fn response_body_preview_handles_a_multi_line_body_with_no_assertions() {
        let log =
            "REQUEST GET http://x\nRESPONSE 200 OK\nline one\nline two\nRESULT_SUMMARY_JSON:{}\n";
        let body = parse_response_body_preview(log).expect("must parse");
        assert_eq!(body, "line one\nline two");
    }
}
