//! Run-completion notifications — build report: "Observability, audit log,
//! notifications, run recovery: None implemented." (Audit log lives in
//! `store.rs`/`auth.rs`; run recovery in `main.rs`; this file is the
//! notifications piece.)
//!
//! Phase 4 gap-closing: the original v1 fired exactly one POST with no
//! retry and no signature — this file's own original comment named both as
//! "real follow-up work, not things a single session should half-implement."
//! This closes both, and only both: a transient failure (a receiver
//! that's briefly restarting, a network blip) now gets [`MAX_ATTEMPTS`] real
//! attempts with real exponential backoff before being given up on, and
//! every real delivery carries a real HMAC-SHA256 signature header
//! (`X-AIQA-Signature-256: sha256=<hex>`, the same convention GitHub/Stripe
//! webhooks already use) whenever `AIQA_WEBHOOK_SECRET` is configured, so a
//! receiver can verify a payload genuinely came from this server and was
//! not forged or tampered with in transit. Still no retry-queue/delivery-
//! tracking table — this stays fire-and-forget and log-only, matching the
//! original comment's own "not a full implementation" disclosure; only the
//! two specific gaps it named are closed.
//!
//! A real CI deploy step (the other half of the original Phase 4 note) is
//! deliberately NOT part of this change: it needs a real deploy target
//! (server/cloud/container host), and none exists in this repo yet — no
//! infra is configured anywhere (`.github/workflows/ci.yml` has no deploy
//! job), and there is no container runtime in this dev environment either
//! (see `store.rs`'s own module doc comment). That part stays deferred
//! until real infra exists to deploy to.

use crate::domain::RunSummary;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Real, bounded retry attempts for a transient delivery failure — enough to
/// ride out a brief receiver restart or network blip without retrying
/// forever against a genuinely broken/misconfigured `AIQA_WEBHOOK_URL`. Not
/// tuned against any real receiver's own uptime data — a reasonable,
/// disclosed default for a v1.
const MAX_ATTEMPTS: u32 = 3;
/// Real exponential backoff between attempts — 1s, then 2s for
/// [`MAX_ATTEMPTS`] = 3 (no sleep after the final attempt, since nothing
/// follows it). Long enough to matter for a brief blip, short enough that
/// this background task never lingers long after a run has already
/// finished.
const BASE_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);

/// Real HMAC-SHA256 of `body`, keyed by `secret`, hex-encoded and prefixed
/// `sha256=` — the same header-value convention established webhook
/// providers (GitHub, Stripe) already use, so a receiver can reuse an
/// existing signature-verification library rather than inventing a new
/// scheme just for this one sender. Pure and network-free, so it is
/// directly unit-testable.
fn sign(secret: &str, body: &[u8]) -> String {
    // `new_from_slice` only fails for a key length an algorithm rejects,
    // which does not exist for HMAC — RFC 2104 hashes an over-long key down
    // first, and a short/empty one is padded — so this is safe to unwrap
    // for every real `secret` value this module ever passes it.
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC-SHA256 accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// A 4xx means the request itself is wrong (the receiver rejecting this
/// exact payload/URL/auth) and resending identical bytes will never change
/// that outcome, so only a real server-side (5xx) problem — genuinely
/// possible to be transient — is ever retried, matching the transport-level
/// `Err` case's own retry treatment below.
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
}

/// The real, testable delivery core `notify_run_completed` below fires into
/// a background task. Kept separate (and returning a real `bool`) so a test
/// can `.await` it directly against a real local server and observe genuine
/// retry/give-up/signature behavior, rather than only being able to assert
/// on log output from a detached `tokio::spawn`.
async fn deliver_with_retry(
    client: &reqwest::Client,
    url: &str,
    run_id: uuid::Uuid,
    body: &[u8],
    secret: Option<&str>,
) -> bool {
    let signature = secret.map(|s| sign(s, body));

    for attempt in 1..=MAX_ATTEMPTS {
        let mut request = client
            .post(url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .body(body.to_vec());
        if let Some(sig) = &signature {
            request = request.header("X-AIQA-Signature-256", sig.clone());
        }

        match request.send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(
                    "webhook delivered for run {run_id} -> {url} ({}) on attempt {attempt}/{MAX_ATTEMPTS}",
                    resp.status()
                );
                return true;
            }
            Ok(resp) => {
                let retry = attempt < MAX_ATTEMPTS && is_retryable_status(resp.status());
                tracing::warn!(
                    "webhook receiver returned {} for run {run_id} (attempt {attempt}/{MAX_ATTEMPTS}){}",
                    resp.status(),
                    if retry { ", retrying" } else { ", giving up" }
                );
                if !retry {
                    return false;
                }
            }
            Err(e) => {
                let retry = attempt < MAX_ATTEMPTS;
                tracing::warn!(
                    "webhook delivery failed for run {run_id} (attempt {attempt}/{MAX_ATTEMPTS}): {e}{}",
                    if retry { ", retrying" } else { ", giving up" }
                );
                if !retry {
                    return false;
                }
            }
        }

        tokio::time::sleep(BASE_BACKOFF * 2u32.pow(attempt - 1)).await;
    }
    false
}

pub fn notify_run_completed(summary: RunSummary) {
    let Ok(url) = std::env::var("AIQA_WEBHOOK_URL") else {
        return;
    };
    if url.trim().is_empty() {
        return;
    }
    // Optional — see this module's own doc comment. Unset means exactly
    // today's behavior: a real, unsigned delivery, never a failure.
    let secret = std::env::var("AIQA_WEBHOOK_SECRET")
        .ok()
        .filter(|s| !s.trim().is_empty());

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "event": "run.completed",
            "run_id": summary.run_id,
            "name": summary.name,
            "state": summary.state,
            "total": summary.total,
            "passed": summary.passed,
            "failed": summary.failed,
            "blocked": summary.blocked,
            "infra_error": summary.infra_error,
            "finished_at": summary.finished_at,
        });
        // Serialized once, up front — every retry attempt sends and signs
        // the exact same bytes, never a re-serialization that could (even
        // if only in principle) drift between attempts.
        let body_bytes = serde_json::to_vec(&body).expect("a JSON value always serializes");
        deliver_with_retry(
            &client,
            &url,
            summary.run_id,
            &body_bytes,
            secret.as_deref(),
        )
        .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn sign_is_deterministic_for_the_same_secret_and_body() {
        assert_eq!(sign("shh", b"hello"), sign("shh", b"hello"));
    }

    #[test]
    fn sign_differs_for_a_different_secret() {
        assert_ne!(sign("shh", b"hello"), sign("other-secret", b"hello"));
    }

    #[test]
    fn sign_differs_for_a_different_body() {
        assert_ne!(sign("shh", b"hello"), sign("shh", b"goodbye"));
    }

    #[test]
    fn sign_is_hex_encoded_and_prefixed_for_downstream_verification_libraries() {
        let sig = sign("shh", b"hello");
        assert!(sig.starts_with("sha256="), "sig={sig}");
        let hex_part = sig.strip_prefix("sha256=").unwrap();
        assert_eq!(
            hex_part.len(),
            64,
            "a real SHA-256 digest is 32 bytes = 64 hex chars: {sig}"
        );
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()), "sig={sig}");
    }

    #[test]
    fn only_a_real_server_error_is_treated_as_retryable() {
        assert!(is_retryable_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(is_retryable_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(
            !is_retryable_status(reqwest::StatusCode::BAD_REQUEST),
            "a 4xx means resending identical bytes will never succeed"
        );
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(reqwest::StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(reqwest::StatusCode::OK));
    }

    /// (real received body, real `X-AIQA-Signature-256` header if present)
    /// per request, in arrival order.
    type ReceivedRequests = Arc<Mutex<Vec<(String, Option<String>)>>>;

    #[derive(Clone, Default)]
    struct Captured {
        request_count: Arc<AtomicU32>,
        received: ReceivedRequests,
    }

    /// Real local HTTP server (same `TcpListener` + `axum::serve` pattern
    /// `ws_tests.rs` already uses for its own real-network tests) that
    /// returns a real 500 for the first `fail_first_n` requests it receives,
    /// then a real 200 — so a test can drive `deliver_with_retry` against
    /// genuine HTTP responses instead of a hand-rolled fake.
    async fn spawn_flaky_server(fail_first_n: u32) -> (String, Captured) {
        let captured = Captured::default();

        // Returns a real 500 for each of the first `threshold` requests it
        // receives, then a real 200 — `threshold = 0` means "always
        // healthy," covering both the flaky and never-flaky test cases with
        // one handler rather than two.
        async fn flaky_handler(
            State((state, threshold)): State<(Captured, u32)>,
            headers: HeaderMap,
            body: axum::body::Bytes,
        ) -> axum::http::StatusCode {
            let n = state.request_count.fetch_add(1, Ordering::SeqCst);
            let sig = headers
                .get("X-AIQA-Signature-256")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            state
                .received
                .lock()
                .unwrap()
                .push((String::from_utf8_lossy(&body).to_string(), sig));
            if n < threshold {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            } else {
                axum::http::StatusCode::OK
            }
        }

        let app = axum::Router::new()
            .route("/webhook", post(flaky_handler))
            .with_state((captured.clone(), fail_first_n));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}/webhook"), captured)
    }

    #[tokio::test]
    async fn delivers_on_the_first_attempt_when_the_receiver_is_healthy() {
        let (url, captured) = spawn_flaky_server(0).await;
        let client = reqwest::Client::new();
        let ok = deliver_with_retry(&client, &url, uuid::Uuid::new_v4(), b"{}", None).await;
        assert!(ok);
        assert_eq!(captured.request_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_a_real_transient_server_error_and_eventually_succeeds() {
        // Fails the first 2 real requests (500), succeeds on the 3rd — the
        // exact real `MAX_ATTEMPTS` boundary, proving retry genuinely
        // happens rather than giving up after one failure.
        let (url, captured) = spawn_flaky_server(2).await;
        let client = reqwest::Client::new();
        let ok = deliver_with_retry(&client, &url, uuid::Uuid::new_v4(), b"{}", None).await;
        assert!(
            ok,
            "must succeed once the receiver recovers within MAX_ATTEMPTS"
        );
        assert_eq!(captured.request_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts_against_a_receiver_that_never_recovers() {
        let (url, captured) = spawn_flaky_server(MAX_ATTEMPTS + 5).await;
        let client = reqwest::Client::new();
        let ok = deliver_with_retry(&client, &url, uuid::Uuid::new_v4(), b"{}", None).await;
        assert!(!ok, "must honestly report failure, never a false success");
        assert_eq!(
            captured.request_count.load(Ordering::SeqCst),
            MAX_ATTEMPTS,
            "must attempt exactly MAX_ATTEMPTS times, never fewer and never unboundedly more"
        );
    }

    #[tokio::test]
    async fn a_real_signature_is_sent_and_verifies_against_the_real_body_when_a_secret_is_configured(
    ) {
        let (url, captured) = spawn_flaky_server(0).await;
        let client = reqwest::Client::new();
        let body = br#"{"event":"run.completed"}"#;
        deliver_with_retry(&client, &url, uuid::Uuid::new_v4(), body, Some("shh")).await;

        let received = captured.received.lock().unwrap();
        assert_eq!(received.len(), 1);
        let (received_body, received_sig) = &received[0];
        assert_eq!(received_body.as_bytes(), body);
        assert_eq!(
            received_sig.as_deref(),
            Some(sign("shh", body).as_str()),
            "the receiver must be able to recompute the exact same signature from the real secret and the real body it received"
        );
    }

    #[tokio::test]
    async fn no_signature_header_is_sent_when_no_secret_is_configured() {
        let (url, captured) = spawn_flaky_server(0).await;
        let client = reqwest::Client::new();
        deliver_with_retry(&client, &url, uuid::Uuid::new_v4(), b"{}", None).await;

        let received = captured.received.lock().unwrap();
        assert_eq!(
            received[0].1, None,
            "no AIQA_WEBHOOK_SECRET means no signature header, matching today's behavior"
        );
    }
}
