//! Run-completion notifications — build report: "Observability, audit log,
//! notifications, run recovery: None implemented." (Audit log lives in
//! `store.rs`/`auth.rs`; run recovery in `main.rs`; this file is the
//! notifications piece.)
//!
//! Deliberately minimal: one outbound webhook POST when a run finishes, to
//! whatever URL `AIQA_WEBHOOK_URL` names. No retry queue, no delivery
//! tracking table, no signature/HMAC — those are real follow-up work, not
//! things a single session should half-implement. What's here is real,
//! though: an actual HTTP POST with a real JSON body, fired in the
//! background so a slow/dead webhook receiver can never block run
//! finalization.

use crate::domain::RunSummary;

pub fn notify_run_completed(summary: RunSummary) {
    let Ok(url) = std::env::var("AIQA_WEBHOOK_URL") else {
        return;
    };
    if url.trim().is_empty() {
        return;
    }
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
        match client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(
                    "webhook delivered for run {} -> {url} ({})",
                    summary.run_id,
                    resp.status()
                );
            }
            Ok(resp) => {
                tracing::warn!(
                    "webhook receiver returned {} for run {}",
                    resp.status(),
                    summary.run_id
                );
            }
            Err(e) => {
                tracing::warn!("webhook delivery failed for run {}: {e}", summary.run_id);
            }
        }
    });
}
