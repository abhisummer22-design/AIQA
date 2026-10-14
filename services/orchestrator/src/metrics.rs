//! `GET /metrics` — Prometheus text exposition format. Most of it is computed
//! on demand from the DB, but not all of it, and the exception matters:
//!
//! - **Everything counted from the DB is a gauge or counter of *things*.**
//!   Runs by state, tasks by status, audit rows, telemetry events. None of those
//!   say how long anything takes.
//! - **`http_request_duration_seconds` is the one in-process metric.** A latency
//!   histogram cannot be reconstructed from the DB after the fact — the
//!   observations only exist while the request is in flight — so this one is
//!   accumulated in memory as requests are served and read back here. That is a
//!   deliberate departure from the "computed from the DB" rule the rest of this
//!   file follows, made because without it `/metrics` cannot answer the only
//!   latency question anyone asks it ("is this thing slow right now?", "what is
//!   p95?"), and answering that from a DB count is not possible.
//!
//! Still no extra crate: the exposition format is simple enough to emit by hand,
//! and the histogram is a fixed array of atomics.
//!
//! Part of the build report's "Observability, audit log, notifications, run
//! recovery: None implemented" gap.

use sqlx::{Row, SqlitePool};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;

/// Upper bounds, in seconds, of the latency histogram. The standard Prometheus
/// `http_request_duration_seconds` set: dense where a local API spends most of
/// its time, with a long tail so an actual stall is still bounded rather than
/// landing in `+Inf` alone.
pub const DURATION_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Cumulative per-bucket observation counts. Bucket `i` counts every request
/// that took *at most* `DURATION_BUCKETS[i]`, which is the cumulative
/// semantics `histogram_quantile` expects — it does the interpolation itself and
/// would be wrong on per-bucket counts.
static DURATION_BUCKET_COUNTS: LazyLock<[AtomicU64; 11]> =
    LazyLock::new(|| std::array::from_fn(|_| AtomicU64::new(0)));
/// Running total of observed latency, in **microseconds** rather than seconds.
/// A plain integer atomic, so the sum is exact and the type needs no float
/// atomic support; divided out to seconds only at render time.
static DURATION_SUM_MICROS: AtomicU64 = AtomicU64::new(0);
static DURATION_COUNT: AtomicU64 = AtomicU64::new(0);

/// Records one request's observed latency.
///
/// Deliberately **unlabelled**. A `path` label is what most people reach for
/// next, and it is a trap: `run_id`s and `project_id`s are in those paths, so
/// labelling by path creates a new time series per run and per project, and the
/// cardinality grows with usage rather than with the size of the code. A
/// latency metric that takes the scrape target down is worse than no latency
/// metric. Splitting by method (a bounded set) would be safe and is the obvious
/// next step if it is ever wanted.
fn observe_duration(seconds: f64) {
    DURATION_COUNT.fetch_add(1, Ordering::Relaxed);
    let micros = (seconds * 1_000_000.0).round().max(0.0) as u64;
    DURATION_SUM_MICROS.fetch_add(micros, Ordering::Relaxed);
    let counts = &*DURATION_BUCKET_COUNTS;
    for (i, bound) in DURATION_BUCKETS.iter().enumerate() {
        if seconds <= *bound {
            counts[i].fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Axum middleware timing every request it wraps.
///
/// Mounted on the whole router, so it covers the public routes (`/health`,
/// the WebSocket upgrades, `/metrics`) as well as the authenticated ones. The
/// time is measured around `next.run(..)`, i.e. including whatever the handler
/// did, and it is recorded after the response is produced — a slow request is
/// a slow request whether or not it succeeded, so failures are counted too and
/// no status label is attached for the same cardinality reason as above.
pub async fn record_request_duration(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    observe_duration(started.elapsed().as_secs_f64());
    response
}

/// Emits the `http_request_duration_seconds` family into `out`.
///
/// `bucket{le="+Inf"}` is required by the exposition format and equals `_count`;
/// it is written from the counter rather than recomputed so the two can never
/// disagree.
pub(crate) fn push_duration_histogram(out: &mut String) {
    out.push_str(
        "# HELP http_request_duration_seconds Latency of HTTP requests served by this process\n\
         # TYPE http_request_duration_seconds histogram\n",
    );
    let counts = &*DURATION_BUCKET_COUNTS;
    for (i, bound) in DURATION_BUCKETS.iter().enumerate() {
        out.push_str(&format!(
            "http_request_duration_seconds_bucket{{le=\"{bound}\"}} {}\n",
            counts[i].load(Ordering::Relaxed)
        ));
    }
    let count = DURATION_COUNT.load(Ordering::Relaxed);
    out.push_str(&format!(
        "http_request_duration_seconds_bucket{{le=\"+Inf\"}} {count}\n"
    ));
    out.push_str(&format!(
        "http_request_duration_seconds_sum {}\n",
        DURATION_SUM_MICROS.load(Ordering::Relaxed) as f64 / 1_000_000.0
    ));
    out.push_str(&format!("http_request_duration_seconds_count {count}\n"));
}

pub async fn render(pool: &SqlitePool) -> String {
    let mut out = String::new();

    out.push_str("# HELP aiqa_runs_total Number of runs by state\n# TYPE aiqa_runs_total gauge\n");
    if let Ok(rows) = sqlx::query("SELECT state, COUNT(*) as c FROM runs GROUP BY state")
        .fetch_all(pool)
        .await
    {
        for r in rows {
            let state: String = r.get("state");
            let c: i64 = r.get("c");
            out.push_str(&format!("aiqa_runs_total{{state=\"{state}\"}} {c}\n"));
        }
    }

    out.push_str("# HELP aiqa_tasks_total Number of tasks by status (across all runs)\n# TYPE aiqa_tasks_total gauge\n");
    if let Ok(rows) = sqlx::query("SELECT status, COUNT(*) as c FROM tasks GROUP BY status")
        .fetch_all(pool)
        .await
    {
        for r in rows {
            let status: String = r.get("status");
            let c: i64 = r.get("c");
            out.push_str(&format!("aiqa_tasks_total{{status=\"{status}\"}} {c}\n"));
        }
    }

    out.push_str("# HELP aiqa_bug_drafts_total Number of bug drafts by review_state\n# TYPE aiqa_bug_drafts_total gauge\n");
    if let Ok(rows) =
        sqlx::query("SELECT review_state, COUNT(*) as c FROM bug_drafts GROUP BY review_state")
            .fetch_all(pool)
            .await
    {
        for r in rows {
            let s: String = r.get("review_state");
            let c: i64 = r.get("c");
            out.push_str(&format!(
                "aiqa_bug_drafts_total{{review_state=\"{s}\"}} {c}\n"
            ));
        }
    }

    out.push_str("# HELP aiqa_failure_clusters_total Number of distinct failure clusters\n# TYPE aiqa_failure_clusters_total gauge\n");
    if let Ok(row) = sqlx::query("SELECT COUNT(*) as c FROM failure_clusters")
        .fetch_one(pool)
        .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_failure_clusters_total {c}\n"));
    }

    out.push_str("# HELP aiqa_api_keys_active Number of non-revoked API keys\n# TYPE aiqa_api_keys_active gauge\n");
    if let Ok(row) = sqlx::query("SELECT COUNT(*) as c FROM api_keys WHERE revoked_at IS NULL")
        .fetch_one(pool)
        .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_api_keys_active {c}\n"));
    }

    out.push_str("# HELP aiqa_audit_log_total Number of audit log entries\n# TYPE aiqa_audit_log_total counter\n");
    if let Ok(row) = sqlx::query("SELECT COUNT(*) as c FROM audit_log")
        .fetch_one(pool)
        .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_audit_log_total {c}\n"));
    }

    out.push_str("# HELP aiqa_execution_telemetry_total Number of normalized execution telemetry events\n# TYPE aiqa_execution_telemetry_total counter\n");
    if let Ok(row) = sqlx::query("SELECT COUNT(*) as c FROM execution_telemetry")
        .fetch_one(pool)
        .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_execution_telemetry_total {c}\n"));
    }

    out.push_str("# HELP aiqa_flaky_tests_total Number of tracked flakiness metrics\n# TYPE aiqa_flaky_tests_total gauge\n");
    if let Ok(row) =
        sqlx::query("SELECT COUNT(*) as c FROM flakiness_metrics WHERE flakiness_score >= 0.15")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_flaky_tests_total {c}\n"));
    }

    out.push_str("# HELP aiqa_healing_strategy_attempts_total Total strategy attempts\n# TYPE aiqa_healing_strategy_attempts_total counter\n");
    if let Ok(row) =
        sqlx::query("SELECT COALESCE(SUM(attempts_count), 0) as c FROM healing_strategy_metrics")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_healing_strategy_attempts_total {c}\n"));
    }

    out.push_str("# HELP aiqa_healing_strategy_successes_total Total verified strategy successes\n# TYPE aiqa_healing_strategy_successes_total counter\n");
    if let Ok(row) =
        sqlx::query("SELECT COALESCE(SUM(verified_count), 0) as c FROM healing_strategy_metrics")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_healing_strategy_successes_total {c}\n"));
    }

    // Phase 8: Intelligent QA Operations & Cross-Environment Observability
    out.push_str("# HELP aiqa_investigations_total Number of distinct failure investigations\n# TYPE aiqa_investigations_total gauge\n");
    if let Ok(row) = sqlx::query("SELECT COUNT(*) as c FROM failure_clusters")
        .fetch_one(pool)
        .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_investigations_total {c}\n"));
    }

    out.push_str("# HELP aiqa_correlated_failures_total Number of correlated failure clusters with recurring occurrences\n# TYPE aiqa_correlated_failures_total gauge\n");
    if let Ok(row) =
        sqlx::query("SELECT COUNT(*) as c FROM failure_clusters WHERE occurrence_count >= 2")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_correlated_failures_total {c}\n"));
    }

    out.push_str("# HELP aiqa_cross_environment_correlations_total Number of failure clusters spanning multiple execution environments\n# TYPE aiqa_cross_environment_correlations_total gauge\n");
    if let Ok(row) = sqlx::query("SELECT COUNT(*) as c FROM (SELECT failure_fingerprint FROM execution_telemetry WHERE failure_fingerprint IS NOT NULL GROUP BY failure_fingerprint HAVING COUNT(DISTINCT environment) > 1)")
        .fetch_one(pool)
        .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_cross_environment_correlations_total {c}\n"));
    }

    out.push_str("# HELP aiqa_telemetry_pruned_total Number of retention prune operations\n# TYPE aiqa_telemetry_pruned_total counter\n");
    // Counts both prune kinds now that `audit.retention` exists, so "did
    // retention run?" stays answerable after an operator prunes the trail — under
    // the old telemetry-only filter that question silently reported zero.
    if let Ok(row) =
        sqlx::query("SELECT COUNT(*) as c FROM audit_log WHERE action IN ('telemetry.retention','audit.retention')")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_telemetry_pruned_total {c}\n"));
    }

    out.push_str("# HELP aiqa_retention_runs_total Number of executed retention runs\n# TYPE aiqa_retention_runs_total counter\n");
    // Same rows as `aiqa_telemetry_pruned_total` above, under a different name.
    // If a dashboard or alert ever selects both, the two queries return the same
    // number and the result looks like a coincidence instead of a mistake. One
    // count, one name: `aiqa_telemetry_pruned_total` is the one to use.
    if let Ok(row) =
        sqlx::query("SELECT COUNT(*) as c FROM audit_log WHERE action IN ('telemetry.retention','audit.retention')")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_retention_runs_total {c}\n"));
    }

    push_duration_histogram(&mut out);
    push_ai_metrics(&mut out);

    out
}

// ------------------------------------------------------------------- AI metrics

/// Upper bounds, in seconds, of the AI provider execution latency histogram.
/// Bounded intervals up to 180 seconds, covering fast local/hosted completions
/// to long reasoning timeouts.
pub const AI_DURATION_BUCKETS: [f64; 12] = [
    0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 20.0, 30.0, 60.0, 120.0, 180.0,
];

pub struct AiHistogramSeries {
    bucket_counts: [AtomicU64; 12],
    sum_micros: AtomicU64,
    count: AtomicU64,
}

impl AiHistogramSeries {
    fn new() -> Self {
        Self {
            bucket_counts: std::array::from_fn(|_| AtomicU64::new(0)),
            sum_micros: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    fn observe(&self, seconds: f64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        let micros = (seconds * 1_000_000.0).round().max(0.0) as u64;
        self.sum_micros.fetch_add(micros, Ordering::Relaxed);
        for (i, bound) in AI_DURATION_BUCKETS.iter().enumerate() {
            if seconds <= *bound {
                self.bucket_counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

type AiHistKey = (String, String); // (provider, task)
type AiCounterKey = (String, String, String); // (provider, task, outcome)

static AI_HISTOGRAMS: LazyLock<
    std::sync::RwLock<std::collections::BTreeMap<AiHistKey, std::sync::Arc<AiHistogramSeries>>>,
> = LazyLock::new(|| std::sync::RwLock::new(std::collections::BTreeMap::new()));

static AI_COUNTERS: LazyLock<
    std::sync::RwLock<std::collections::BTreeMap<AiCounterKey, std::sync::Arc<AtomicU64>>>,
> = LazyLock::new(|| std::sync::RwLock::new(std::collections::BTreeMap::new()));

fn sanitize_metric_label(s: &str) -> String {
    let clean: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(32)
        .collect();
    if clean.is_empty() {
        "unknown".to_string()
    } else {
        clean.to_ascii_lowercase()
    }
}

fn sanitize_task_label(s: &str) -> String {
    match s {
        "test_case_generation" | "test_case_compilation" | "bug_analysis" | "general_ai" => {
            s.to_string()
        }
        _ => sanitize_metric_label(s),
    }
}

fn sanitize_outcome_label(s: &str) -> String {
    match s {
        "success" | "rejected" | "timeout" | "invalid_response" | "error" => s.to_string(),
        _ => "error".to_string(),
    }
}

/// Records one observed AI provider call execution duration and outcome.
/// Labels are strictly bounded to (provider, task, outcome) to prevent
/// Prometheus metric cardinality explosion. Never labels by prompt, user ID,
/// trace ID, or arbitrary error messages.
pub fn observe_ai_execution(provider: &str, task: &str, outcome: &str, duration_seconds: f64) {
    let provider_label = sanitize_metric_label(provider);
    let task_label = sanitize_task_label(task);
    let outcome_label = sanitize_outcome_label(outcome);

    // 1. Observe histogram
    let hist_key = (provider_label.clone(), task_label.clone());
    let series = {
        let read = AI_HISTOGRAMS.read().unwrap();
        read.get(&hist_key).cloned()
    };
    let series = match series {
        Some(s) => s,
        None => {
            let mut write = AI_HISTOGRAMS.write().unwrap();
            write
                .entry(hist_key)
                .or_insert_with(|| std::sync::Arc::new(AiHistogramSeries::new()))
                .clone()
        }
    };
    series.observe(duration_seconds);

    // 2. Increment outcome counter
    let counter_key = (provider_label, task_label, outcome_label);
    let counter = {
        let read = AI_COUNTERS.read().unwrap();
        read.get(&counter_key).cloned()
    };
    let counter = match counter {
        Some(c) => c,
        None => {
            let mut write = AI_COUNTERS.write().unwrap();
            write
                .entry(counter_key)
                .or_insert_with(|| std::sync::Arc::new(AtomicU64::new(0)))
                .clone()
        }
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Convenience observer accepting an `AiCallEvent`.
pub fn observe_ai_provider_call(event: &crate::ai_usage::AiCallEvent) {
    let outcome = match (&event.answered, &event.error_category) {
        (true, None) => "success",
        (_, Some(crate::ai_usage::AiErrorCategory::ProviderRejected)) => "rejected",
        (_, Some(crate::ai_usage::AiErrorCategory::NonSuccessStatus)) => "rejected",
        (_, Some(crate::ai_usage::AiErrorCategory::Timeout)) => "timeout",
        (_, Some(crate::ai_usage::AiErrorCategory::InvalidResponse)) => "invalid_response",
        (_, Some(crate::ai_usage::AiErrorCategory::ProviderUnavailable)) => "error",
        (_, Some(crate::ai_usage::AiErrorCategory::ModelUnavailable)) => "error",
        (false, None) => "error",
    };
    let duration_secs = event.took_ms as f64 / 1000.0;
    observe_ai_execution(&event.provider, event.task.name(), outcome, duration_secs);
}

pub(crate) fn push_ai_metrics(out: &mut String) {
    let hists = AI_HISTOGRAMS.read().unwrap();
    if !hists.is_empty() {
        out.push_str(
            "# HELP ai_provider_execution_duration_seconds Duration of AI provider execution in seconds, including internal retries and backoff\n\
             # TYPE ai_provider_execution_duration_seconds histogram\n",
        );
        for ((provider, task), series) in hists.iter() {
            for (i, bound) in AI_DURATION_BUCKETS.iter().enumerate() {
                out.push_str(&format!(
                    "ai_provider_execution_duration_seconds_bucket{{provider=\"{provider}\",task=\"{task}\",le=\"{bound}\"}} {}\n",
                    series.bucket_counts[i].load(Ordering::Relaxed)
                ));
            }
            let count = series.count.load(Ordering::Relaxed);
            out.push_str(&format!(
                "ai_provider_execution_duration_seconds_bucket{{provider=\"{provider}\",task=\"{task}\",le=\"+Inf\"}} {count}\n"
            ));
            out.push_str(&format!(
                "ai_provider_execution_duration_seconds_sum{{provider=\"{provider}\",task=\"{task}\"}} {}\n",
                series.sum_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0
            ));
            out.push_str(&format!(
                "ai_provider_execution_duration_seconds_count{{provider=\"{provider}\",task=\"{task}\"}} {count}\n"
            ));
        }
    }

    let counters = AI_COUNTERS.read().unwrap();
    if !counters.is_empty() {
        out.push_str(
            "# HELP ai_provider_calls_total Total number of AI provider calls by provider, task, and outcome\n\
             # TYPE ai_provider_calls_total counter\n",
        );
        for ((provider, task, outcome), counter) in counters.iter() {
            out.push_str(&format!(
                "ai_provider_calls_total{{provider=\"{provider}\",task=\"{task}\",outcome=\"{outcome}\"}} {}\n",
                counter.load(Ordering::Relaxed)
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_duration_histogram_accumulates_and_buckets_are_monotonic() {
        let p = "test_groq_hist";
        let t = "test_case_generation";
        observe_ai_execution(p, t, "success", 0.05);
        observe_ai_execution(p, t, "success", 0.5);
        observe_ai_execution(p, t, "rejected", 3.0);
        observe_ai_execution(p, t, "timeout", 45.0);

        let mut out = String::new();
        push_ai_metrics(&mut out);

        assert!(out.contains("ai_provider_execution_duration_seconds_bucket"));
        assert!(out.contains("ai_provider_calls_total"));

        let mut prev = 0u64;
        for bound in AI_DURATION_BUCKETS {
            let needle = format!(
                "ai_provider_execution_duration_seconds_bucket{{provider=\"{p}\",task=\"{t}\",le=\"{bound}\"}} "
            );
            let line = out.lines().find(|l| l.starts_with(&needle)).expect("bucket found");
            let val: u64 = line.rsplit(' ').next().unwrap().parse().unwrap();
            assert!(val >= prev, "bucket counts must be cumulative and non-decreasing");
            prev = val;
        }

        let inf_needle = format!(
            "ai_provider_execution_duration_seconds_bucket{{provider=\"{p}\",task=\"{t}\",le=\"+Inf\"}} "
        );
        let inf_line = out.lines().find(|l| l.starts_with(&inf_needle)).unwrap();
        let inf_val: u64 = inf_line.rsplit(' ').next().unwrap().parse().unwrap();

        let count_needle = format!(
            "ai_provider_execution_duration_seconds_count{{provider=\"{p}\",task=\"{t}\"}} "
        );
        let count_line = out.lines().find(|l| l.starts_with(&count_needle)).unwrap();
        let count_val: u64 = count_line.rsplit(' ').next().unwrap().parse().unwrap();

        assert_eq!(inf_val, 4);
        assert_eq!(count_val, 4);
        assert_eq!(inf_val, count_val);

        let sum_needle = format!(
            "ai_provider_execution_duration_seconds_sum{{provider=\"{p}\",task=\"{t}\"}} "
        );
        let sum_line = out.lines().find(|l| l.starts_with(&sum_needle)).unwrap();
        let sum_val: f64 = sum_line.rsplit(' ').next().unwrap().parse().unwrap();
        assert!((sum_val - 48.55).abs() < 0.01);
    }

    #[test]
    fn ai_metrics_labels_have_bounded_cardinality_and_no_leakage() {
        let p = "Bad Provider! With Spaces/And/Punctuation $100";
        let t = "unknown_task_injection_attempt_with_prompts";
        let outcome = "arbitrary_error_leak_xyz";
        observe_ai_execution(p, t, outcome, 1.0);

        let mut out = String::new();
        push_ai_metrics(&mut out);

        // Assert sanitization: no spaces, no exclamation marks, no dollar signs
        assert!(!out.contains("Bad Provider!"));
        assert!(!out.contains("$100"));
        assert!(!out.contains("arbitrary_error_leak_xyz"));
        // Mapped to safe error
        assert!(out.contains("outcome=\"error\""));
    }
}
