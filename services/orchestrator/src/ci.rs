//! Phase 8D — what a CI job needs from a report endpoint, beside the report: to wait for the run to end in one request, and to read how it
//! came out without parsing anything (`X-AIQA-Verdict`, `X-AIQA-Final`).
//!
//! A job makes a run (`POST ...`), asks for its report with `?wait=` and `?format=junit`, saves the XML for the build page and takes its
//! exit code from the verdict. `scripts/ci/aiqa-ci.sh` (and `.ps1`) do exactly that.

use crate::errors::ApiError;
use axum::http::HeaderValue;
use axum::response::Response;
use serde_json::Value;
use std::future::Future;
use std::time::{Duration, Instant};

/// The longest one request waits for a run to end; a longer run is waited for by asking again.
pub const MAX_WAIT_SECS: u64 = 300;
/// How often a waiting request looks at the run again.
pub const LOOK_EVERY: Duration = Duration::from_secs(2);

/// `?wait=` as a time: none when it is not given, and refused when it is more than a request may wait.
pub fn check_wait(wait: Option<u64>) -> Result<Duration, ApiError> {
    let secs = wait.unwrap_or(0);
    if secs > MAX_WAIT_SECS {
        return Err(ApiError::validation(format!(
            "wait must be at most {MAX_WAIT_SECS} seconds: ask again to wait longer"
        )));
    }
    Ok(Duration::from_secs(secs))
}

/// The report `make` gives, and again after a moment for as long as it is not final and `wait` has not run out. With no wait it is
/// made once. A report that could not be made ends the wait with its error.
pub async fn until_final<F, Fut>(wait: Duration, make: F) -> Result<Value, ApiError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Value, ApiError>>,
{
    look_every(wait, LOOK_EVERY, make).await
}

async fn look_every<F, Fut>(wait: Duration, every: Duration, mut make: F) -> Result<Value, ApiError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Value, ApiError>>,
{
    let started = Instant::now();
    loop {
        let report = make().await?;
        if report["final"] == true || started.elapsed() + every > wait {
            return Ok(report);
        }
        tokio::time::sleep(every).await;
    }
}

/// The response with the report's verdict and whether the run has ended in its headers, for a job that reads no body.
pub fn with_verdict(mut response: Response, report: &Value) -> Response {
    let headers = response.headers_mut();
    if let Ok(verdict) = HeaderValue::from_str(report["verdict"].as_str().unwrap_or("")) {
        headers.insert("x-aiqa-verdict", verdict);
    }
    headers.insert(
        "x-aiqa-final",
        HeaderValue::from_static(if report["final"] == true {
            "true"
        } else {
            "false"
        }),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn a_wait_is_none_when_not_given_and_at_most_five_minutes() {
        assert_eq!(check_wait(None).unwrap(), Duration::ZERO);
        assert_eq!(check_wait(Some(0)).unwrap(), Duration::ZERO);
        assert_eq!(check_wait(Some(60)).unwrap(), Duration::from_secs(60));
        assert_eq!(
            check_wait(Some(MAX_WAIT_SECS)).unwrap(),
            Duration::from_secs(300)
        );
        assert!(check_wait(Some(MAX_WAIT_SECS + 1)).is_err());
    }

    #[tokio::test]
    async fn with_no_wait_the_report_is_made_once_whatever_it_says() {
        let calls = AtomicUsize::new(0);
        let report = until_final(Duration::ZERO, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"final": false}))
        })
        .await
        .unwrap();
        assert_eq!(
            (report["final"].clone(), calls.load(Ordering::SeqCst)),
            (json!(false), 1)
        );
    }

    const QUICK: Duration = Duration::from_millis(10);

    #[tokio::test]
    async fn it_looks_again_until_the_run_has_ended_and_stops_there() {
        let calls = AtomicUsize::new(0);
        let report = look_every(Duration::from_secs(30), QUICK, || async {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(json!({"final": n >= 4, "n": n}))
        })
        .await
        .unwrap();
        assert_eq!(
            (report["n"].clone(), calls.load(Ordering::SeqCst)),
            (json!(4), 4)
        );
    }

    #[tokio::test]
    async fn it_gives_what_it_has_when_the_wait_runs_out() {
        let calls = AtomicUsize::new(0);
        let started = Instant::now();
        let report = look_every(Duration::from_millis(120), QUICK, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"final": false}))
        })
        .await
        .unwrap();
        assert_eq!(report["final"], false);
        assert!(
            calls.load(Ordering::SeqCst) >= 2,
            "it looked more than once"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "and it did not wait beyond the wait"
        );
    }

    #[tokio::test]
    async fn a_report_that_could_not_be_made_ends_the_wait_with_its_error() {
        let error = look_every(Duration::from_secs(30), QUICK, || async {
            Err(ApiError::not_found("gone"))
        })
        .await
        .unwrap_err();
        assert_eq!(error.message(), "gone");
    }

    #[test]
    fn the_verdict_and_whether_the_run_ended_are_in_the_headers() {
        let response = with_verdict(
            "body".into_response(),
            &json!({"verdict": "REGRESSED", "final": true}),
        );
        assert_eq!(response.headers()["x-aiqa-verdict"], "REGRESSED");
        assert_eq!(response.headers()["x-aiqa-final"], "true");
        let going = with_verdict(
            "body".into_response(),
            &json!({"verdict": "RUNNING", "final": false}),
        );
        assert_eq!(going.headers()["x-aiqa-final"], "false");
        let nothing = with_verdict("body".into_response(), &json!({"final": true}));
        assert_eq!(
            nothing.headers()["x-aiqa-verdict"],
            "",
            "no verdict is an empty one, not a missing header"
        );
    }
}
