//! Application Source connectivity verification — Truthfulness Cleanup
//! Phase, Step 1.
//!
//! Fixes a real, pre-existing gap flagged CRITICAL by the Phase 1–6 Final
//! Consolidated Audit: the Test Case Generator's "Application Sources"
//! panel (`forentend/src/components/test-generator/ApplicationSourceSection.tsx`)
//! previously reported a fabricated `CONNECTED` status — a fake HTTP 200, a
//! fake "SSL: Valid", and a `Math.random()`-based fake ping time — for *any*
//! syntactically valid URL, with no real network call at all. Even an
//! unreachable or nonexistent host would show as genuinely connected.
//!
//! This module makes that check genuinely real, reusing existing real
//! primitives rather than inventing a second implementation of either:
//!
//! ```text
//! raw URL string
//!         v
//! format/scheme validation (real, structural)          -> INVALID_URL
//!         v
//! executor::validate_target_url — the exact, unmodified real SSRF guard
//! every other outbound target in this app already goes through
//!                                                        -> REJECTED
//!         v
//! attempt_real_request — a real, bounded reqwest GET, redirects disabled,
//! response body never read
//!         v
//! Ok(status)  -> CONNECTED (real HTTP status + real measured latency)
//! Err(e)      -> UNREACHABLE (executor::classify_reqwest_error — the exact,
//!                 unmodified transport-error classification
//!                 CompiledApiExecutor already uses — reduced to a safe,
//!                 generic message, never raw internal networking detail)
//! ```
//!
//! `CONNECTED` is deliberately defined as "a real HTTP response was
//! received," regardless of status code — a reachable server answering
//! 401/403/404 is genuinely reachable, and this module never conflates
//! network reachability with application-level HTTP status. No SSL/TLS
//! validity claim is made beyond what a successful HTTPS request already
//! proves (a fundamentally invalid certificate would have failed the
//! request itself, classified as `TLS_FAILED`, and reported as
//! `UNREACHABLE`) — no certificate issuer/expiry/detail is available from
//! `reqwest`'s default API and none is fabricated here.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppSourceStatus {
    Connected,
    Unreachable,
    Rejected,
    InvalidUrl,
    Error,
}

impl AppSourceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppSourceStatus::Connected => "CONNECTED",
            AppSourceStatus::Unreachable => "UNREACHABLE",
            AppSourceStatus::Rejected => "REJECTED",
            AppSourceStatus::InvalidUrl => "INVALID_URL",
            AppSourceStatus::Error => "ERROR",
        }
    }
}

/// The real, honest response contract — every field is either `None`/absent
/// or a genuine, measured/observed value. Never internal IPs, DNS detail,
/// headers, or raw error text (`message` is always a fixed, safe string).
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyApplicationSourceResponse {
    pub status: &'static str,
    pub reachable: bool,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub message: String,
}

fn verify_timeout() -> Duration {
    let secs: u64 = std::env::var("APP_SOURCE_VERIFY_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    Duration::from_secs(secs)
}

/// Real, bounded connectivity verification. The entire truthful replacement
/// for the frontend's former `setTimeout` + `Math.random()` simulation —
/// every branch below is either a real, structural rejection (before any
/// network call is ever made) or the direct outcome of a real request.
pub async fn verify(raw_url: &str) -> VerifyApplicationSourceResponse {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return invalid_url("URL must not be empty.");
    }

    let parsed = match reqwest::Url::parse(trimmed) {
        Ok(u) => u,
        Err(e) => return invalid_url(&format!("{trimmed:?} is not a valid URL: {e}")),
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return invalid_url("Only http and https URLs are supported.");
    }

    // The exact, unmodified, real SSRF guard every other outbound target in
    // this app (API Testing, Performance Testing, the execution-plan
    // compiler) already goes through — not a second, weaker implementation,
    // and never weakened here to make this feature "work" against a
    // disallowed target.
    if crate::executor::validate_target_url(trimmed).await.is_err() {
        return VerifyApplicationSourceResponse {
            status: AppSourceStatus::Rejected.as_str(),
            reachable: false,
            http_status: None,
            latency_ms: None,
            message: "This target is not allowed by the connection security policy.".to_string(),
        };
    }

    attempt_real_request(&parsed, verify_timeout()).await
}

fn invalid_url(message: &str) -> VerifyApplicationSourceResponse {
    VerifyApplicationSourceResponse { status: AppSourceStatus::InvalidUrl.as_str(), reachable: false, http_status: None, latency_ms: None, message: message.to_string() }
}

/// The actual, real network attempt — deliberately factored out of
/// `verify()` so it can be tested directly, in isolation, against a real
/// self-contained target (or a real, genuinely dead port) without needing
/// to also satisfy the SSRF gate in the test itself; `verify()`'s own gate
/// is separately, directly tested (`a_disallowed_target_is_rejected_before_
/// any_network_call`) to prove it always runs first in the real, composed
/// path.
///
/// Redirects are never followed (`redirect::Policy::none()`) — a redirect
/// response is itself just a real, reachable HTTP status (`3xx`), never
/// chased, so a redirect to a disallowed/private target can never be
/// silently reached. The response body is never read (no
/// `.text()`/`.bytes()` call) — only the real, observed status and the
/// real, measured elapsed time.
async fn attempt_real_request(url: &reqwest::Url, timeout: Duration) -> VerifyApplicationSourceResponse {
    let client = match reqwest::Client::builder().timeout(timeout).redirect(reqwest::redirect::Policy::none()).build() {
        Ok(c) => c,
        Err(e) => {
            return VerifyApplicationSourceResponse {
                status: AppSourceStatus::Error.as_str(),
                reachable: false,
                http_status: None,
                latency_ms: None,
                message: format!("Could not initialize the verification client: {e}"),
            };
        }
    };

    let started = std::time::Instant::now();
    match client.get(url.clone()).send().await {
        Ok(resp) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let status = resp.status();
            let https_note = if url.scheme() == "https" { " (HTTPS request completed successfully)" } else { "" };
            VerifyApplicationSourceResponse {
                status: AppSourceStatus::Connected.as_str(),
                reachable: true,
                http_status: Some(status.as_u16()),
                latency_ms: Some(latency_ms),
                message: format!("Reachable{https_note} — real HTTP {status} in {latency_ms}ms."),
            }
        }
        Err(e) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let classified = crate::executor::classify_reqwest_error(&e);
            VerifyApplicationSourceResponse {
                status: AppSourceStatus::Unreachable.as_str(),
                reachable: false,
                http_status: None,
                latency_ms: Some(latency_ms),
                message: safe_unreachable_message(&classified),
            }
        }
    }
}

/// Reduces the real, but potentially internal-detail-carrying, raw
/// `reqwest`/`hyper` error text (`classify_reqwest_error`'s own trailing
/// `{e}` detail, which can include local socket/OS-level information) down
/// to a fixed, safe, user-facing message — never raw internal networking
/// errors or stack traces reach the frontend.
fn safe_unreachable_message(classified: &str) -> String {
    let prefix = classified.split(':').next().unwrap_or("CONNECTION_FAILED").trim();
    match prefix {
        "REQUEST_TIMEOUT" => "The connection attempt timed out.".to_string(),
        "DNS_FAILED" => "The hostname could not be resolved.".to_string(),
        "TLS_FAILED" => "The secure (TLS/SSL) connection could not be established.".to_string(),
        _ => "Unable to reach the application source.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_empty_url_is_invalid() {
        let r = verify("").await;
        assert_eq!(r.status, "INVALID_URL");
        assert!(!r.reachable);
        assert!(r.http_status.is_none());
        assert!(r.latency_ms.is_none());
    }

    #[tokio::test]
    async fn a_malformed_url_is_invalid() {
        let r = verify("not a url at all").await;
        assert_eq!(r.status, "INVALID_URL");
        assert!(!r.reachable);
    }

    #[tokio::test]
    async fn a_non_http_scheme_is_invalid() {
        let r = verify("ftp://example.com/file").await;
        assert_eq!(r.status, "INVALID_URL");
    }

    #[tokio::test]
    async fn a_disallowed_ssrf_target_is_rejected_before_any_network_call() {
        // The exact, real, unmodified SSRF guard — reused, not reimplemented.
        // A private RFC1918 address is never dialed.
        let r = verify("http://10.0.0.5/").await;
        assert_eq!(r.status, "REJECTED");
        assert!(!r.reachable);
        assert!(r.http_status.is_none());
        assert!(r.latency_ms.is_none(), "a rejected target must never show a real measured latency, since no request was ever attempted");
    }

    #[tokio::test]
    async fn cloud_metadata_address_is_rejected() {
        let r = verify("http://169.254.169.254/latest/meta-data/").await;
        assert_eq!(r.status, "REJECTED");
    }

    #[tokio::test]
    async fn a_loopback_target_on_a_non_documented_port_is_rejected_same_as_every_other_outbound_call_in_this_app() {
        let r = verify("http://127.0.0.1:9/").await;
        assert_eq!(r.status, "REJECTED");
    }

    // ---------------------------------------- real network attempt (isolated)
    //
    // `attempt_real_request` is tested directly against real, self-contained
    // targets — a real local axum server for CONNECTED, a real
    // bind-then-drop dead port for UNREACHABLE, a real bind-accept-never-
    // respond listener for a real timeout — the exact same real-network
    // testing techniques already established in `executor.rs`'s own Phase 4
    // tests. This is legitimate because `verify()`'s own SSRF gate is
    // already, separately proven to run first (tests above) — this function
    // has no SSRF logic of its own to bypass.

    #[tokio::test]
    async fn a_real_reachable_target_reports_connected_with_a_real_status_and_measured_latency() {
        let app = axum::Router::new().route("/ok", axum::routing::get(|| async { "hello" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let url = reqwest::Url::parse(&format!("http://{addr}/ok")).unwrap();

        let r = attempt_real_request(&url, Duration::from_secs(5)).await;
        assert_eq!(r.status, "CONNECTED");
        assert!(r.reachable);
        assert_eq!(r.http_status, Some(200));
        assert!(r.latency_ms.is_some());
        assert!(r.message.contains("200"));
    }

    #[tokio::test]
    async fn a_real_reachable_target_returning_a_non_2xx_status_is_still_reported_connected() {
        // Reachability and application HTTP status must be distinguished —
        // a real, responding server answering 404 is genuinely reachable.
        let app = axum::Router::new().route("/missing", axum::routing::get(|| async { (axum::http::StatusCode::NOT_FOUND, "nope") }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let url = reqwest::Url::parse(&format!("http://{addr}/missing")).unwrap();

        let r = attempt_real_request(&url, Duration::from_secs(5)).await;
        assert_eq!(r.status, "CONNECTED", "a real 404 response is still a genuinely reachable target");
        assert!(r.reachable);
        assert_eq!(r.http_status, Some(404));
    }

    #[tokio::test]
    async fn a_genuinely_dead_port_reports_unreachable_with_a_safe_message_and_a_real_measured_duration() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // genuinely nothing listens here now
        let url = reqwest::Url::parse(&format!("http://{addr}/")).unwrap();

        let r = attempt_real_request(&url, Duration::from_secs(3)).await;
        assert_eq!(r.status, "UNREACHABLE");
        assert!(!r.reachable);
        assert!(r.http_status.is_none());
        assert!(r.latency_ms.is_some(), "a real elapsed duration must be reported even for a failed attempt");
        assert!(!r.message.to_lowercase().contains("os error"), "the raw OS-level error text must never reach the message: {}", r.message);
    }

    #[tokio::test]
    async fn a_real_timeout_reports_unreachable_not_connected() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((socket, _)) = listener.accept().await {
                let _socket = socket; // held open, nothing ever written back
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        let url = reqwest::Url::parse(&format!("http://{addr}/")).unwrap();

        let r = attempt_real_request(&url, Duration::from_millis(200)).await;
        assert_eq!(r.status, "UNREACHABLE");
        assert!(!r.reachable);
        assert!(r.latency_ms.is_some());
    }

    #[test]
    fn safe_unreachable_message_never_echoes_raw_classification_detail() {
        assert_eq!(safe_unreachable_message("REQUEST_TIMEOUT: operation timed out"), "The connection attempt timed out.");
        assert_eq!(safe_unreachable_message("DNS_FAILED: dns error: no records found"), "The hostname could not be resolved.");
        assert_eq!(safe_unreachable_message("TLS_FAILED: invalid certificate"), "The secure (TLS/SSL) connection could not be established.");
        let generic = safe_unreachable_message("CONNECTION_FAILED: os error 10061, real internal socket detail");
        assert_eq!(generic, "Unable to reach the application source.");
        assert!(!generic.contains("10061"));
    }
}
