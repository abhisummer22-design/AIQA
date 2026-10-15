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
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

// ------------------------------------------------------------- Rate limit gauges

pub const KNOWN_GROQ_MODELS: &[&str] = &[
    "llama-3.3-70b-versatile",
    "llama-3.1-70b-versatile",
    "llama-3.1-8b-instant",
    "llama3-70b-8192",
    "llama3-8b-8192",
    "mixtral-8x7b-32768",
    "gemma2-9b-it",
    "qwen/qwen3.8-27b",
    "qwen-2.5-32b",
    "deepseek-r1-distill-llama-70b",
];

pub fn canonical_groq_model_label(model: &str) -> &'static str {
    let clean = model.trim();
    for &known in KNOWN_GROQ_MODELS {
        if clean.eq_ignore_ascii_case(known) {
            return known;
        }
    }
    "other"
}

pub const MAX_TOKEN_OBSERVATION_AGE_MICROS: u64 = 120 * 1_000_000;
pub const MAX_REQUEST_OBSERVATION_AGE_MICROS: u64 = 86_400 * 1_000_000;

#[derive(Default)]
pub struct AiRateLimitGauges {
    pub remaining_requests: AtomicI64, // -1 means unobserved
    pub remaining_tokens: AtomicI64,   // -1 means unobserved
    pub limit_requests: AtomicI64,     // -1 means unobserved
    pub limit_tokens: AtomicI64,       // -1 means unobserved
    pub reset_requests_timestamp_micros: AtomicU64, // 0 means unobserved
    pub reset_tokens_timestamp_micros: AtomicU64,   // 0 means unobserved
    pub last_observed_requests_micros: AtomicU64,   // 0 means unobserved
    pub last_observed_tokens_micros: AtomicU64,     // 0 means unobserved
    pub low_requests_warned: AtomicBool,
    pub low_tokens_warned: AtomicBool,
}

impl AiRateLimitGauges {
    pub fn new() -> Self {
        Self {
            remaining_requests: AtomicI64::new(-1),
            remaining_tokens: AtomicI64::new(-1),
            limit_requests: AtomicI64::new(-1),
            limit_tokens: AtomicI64::new(-1),
            reset_requests_timestamp_micros: AtomicU64::new(0),
            reset_tokens_timestamp_micros: AtomicU64::new(0),
            last_observed_requests_micros: AtomicU64::new(0),
            last_observed_tokens_micros: AtomicU64::new(0),
            low_requests_warned: AtomicBool::new(false),
            low_tokens_warned: AtomicBool::new(false),
        }
    }
}

type AiRateLimitKey = (String, String); // (provider, model)

static AI_RATELIMITS: LazyLock<
    std::sync::RwLock<std::collections::BTreeMap<AiRateLimitKey, std::sync::Arc<AiRateLimitGauges>>>,
> = LazyLock::new(|| std::sync::RwLock::new(std::collections::BTreeMap::new()));

/// Clears rate limit gauges for isolated unit tests.
#[cfg(test)]
pub fn reset_ai_ratelimits_for_test() {
    let mut write = AI_RATELIMITS.write().unwrap();
    write.clear();
}

/// Records observed AI provider rate-limit capacities and resets.
/// Strictly bounded cardinality: Groq models are verified against `KNOWN_GROQ_MODELS`
/// and mapped to `"other"` if unknown. No arbitrary strings, trace IDs, prompts, or
/// secrets are ever used in labels.
pub fn observe_ai_ratelimit(
    provider: &str,
    model: &str,
    remaining_requests: Option<u64>,
    remaining_tokens: Option<u64>,
    limit_requests: Option<u64>,
    limit_tokens: Option<u64>,
    reset_requests: Option<Duration>,
    reset_tokens: Option<Duration>,
) {
    let now_micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    observe_ai_ratelimit_with_time(
        provider,
        model,
        remaining_requests,
        remaining_tokens,
        limit_requests,
        limit_tokens,
        reset_requests,
        reset_tokens,
        now_micros,
    );
}

pub fn observe_ai_ratelimit_with_time(
    provider: &str,
    model: &str,
    remaining_requests: Option<u64>,
    remaining_tokens: Option<u64>,
    limit_requests: Option<u64>,
    limit_tokens: Option<u64>,
    reset_requests: Option<Duration>,
    reset_tokens: Option<Duration>,
    now_micros: u64,
) {
    if remaining_requests.is_none()
        && remaining_tokens.is_none()
        && limit_requests.is_none()
        && limit_tokens.is_none()
        && reset_requests.is_none()
        && reset_tokens.is_none()
    {
        return;
    }

    let provider_label = sanitize_metric_label(provider);
    let model_label = if provider_label == "groq" {
        canonical_groq_model_label(model).to_string()
    } else {
        sanitize_metric_label(model)
    };

    let key = (provider_label, model_label);
    let gauges = {
        let read = AI_RATELIMITS.read().unwrap();
        read.get(&key).cloned()
    };
    let gauges = match gauges {
        Some(g) => g,
        None => {
            let mut write = AI_RATELIMITS.write().unwrap();
            write
                .entry(key.clone())
                .or_insert_with(|| std::sync::Arc::new(AiRateLimitGauges::new()))
                .clone()
        }
    };

    if let Some(r) = remaining_requests {
        let r_val = (r as i64).max(0);
        gauges.remaining_requests.store(r_val, Ordering::Relaxed);
        gauges
            .last_observed_requests_micros
            .store(now_micros, Ordering::Relaxed);

        if r < 10 {
            if !gauges.low_requests_warned.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    provider = %key.0,
                    model = %key.1,
                    remaining_requests = r,
                    "Groq remaining requests quota is low (< 10)"
                );
            }
        } else {
            gauges.low_requests_warned.store(false, Ordering::Relaxed);
        }
    }

    if let Some(lim) = limit_requests {
        gauges.limit_requests.store((lim as i64).max(0), Ordering::Relaxed);
    }

    if let Some(lim) = limit_tokens {
        gauges.limit_tokens.store((lim as i64).max(0), Ordering::Relaxed);
    }

    if let Some(t) = remaining_tokens {
        let t_val = (t as i64).max(0);
        gauges.remaining_tokens.store(t_val, Ordering::Relaxed);
        gauges
            .last_observed_tokens_micros
            .store(now_micros, Ordering::Relaxed);

        let lim_tok = limit_tokens.or_else(|| {
            let v = gauges.limit_tokens.load(Ordering::Relaxed);
            if v > 0 {
                Some(v as u64)
            } else {
                None
            }
        });
        if let Some(lim) = lim_tok {
            if lim > 0 {
                if t * 10 < lim {
                    if !gauges.low_tokens_warned.swap(true, Ordering::Relaxed) {
                        tracing::warn!(
                            provider = %key.0,
                            model = %key.1,
                            remaining_tokens = t,
                            limit_tokens = lim,
                            "Groq remaining tokens quota is below 10%"
                        );
                    }
                } else {
                    gauges.low_tokens_warned.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    if let Some(d) = reset_requests {
        let rst_micros = now_micros.saturating_add(d.as_micros() as u64);
        gauges
            .reset_requests_timestamp_micros
            .store(rst_micros, Ordering::Relaxed);
    } else if remaining_requests.is_some() {
        let prev_rst = gauges.reset_requests_timestamp_micros.load(Ordering::Relaxed);
        if prev_rst > 0 && prev_rst <= now_micros {
            gauges.reset_requests_timestamp_micros.store(0, Ordering::Relaxed);
        }
    }

    if let Some(d) = reset_tokens {
        let rst_micros = now_micros.saturating_add(d.as_micros() as u64);
        gauges
            .reset_tokens_timestamp_micros
            .store(rst_micros, Ordering::Relaxed);
    } else if remaining_tokens.is_some() {
        let prev_rst = gauges.reset_tokens_timestamp_micros.load(Ordering::Relaxed);
        if prev_rst > 0 && prev_rst <= now_micros {
            gauges.reset_tokens_timestamp_micros.store(0, Ordering::Relaxed);
        }
    }
}

pub fn observe_groq_ratelimit(
    model: &str,
    remaining_requests: Option<u64>,
    remaining_tokens: Option<u64>,
    limit_requests: Option<u64>,
    limit_tokens: Option<u64>,
    reset_requests: Option<Duration>,
    reset_tokens: Option<Duration>,
) {
    observe_ai_ratelimit(
        "groq",
        model,
        remaining_requests,
        remaining_tokens,
        limit_requests,
        limit_tokens,
        reset_requests,
        reset_tokens,
    );
}

#[cfg(test)]
pub fn observe_groq_ratelimit_with_time(
    model: &str,
    remaining_requests: Option<u64>,
    remaining_tokens: Option<u64>,
    limit_requests: Option<u64>,
    limit_tokens: Option<u64>,
    reset_requests: Option<Duration>,
    reset_tokens: Option<Duration>,
    now_micros: u64,
) {
    observe_ai_ratelimit_with_time(
        "groq",
        model,
        remaining_requests,
        remaining_tokens,
        limit_requests,
        limit_tokens,
        reset_requests,
        reset_tokens,
        now_micros,
    );
}

pub(crate) fn push_ai_metrics(out: &mut String) {
    let now_micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    push_ai_metrics_with_time(out, now_micros);
}

pub(crate) fn push_ai_metrics_with_time(out: &mut String, now_micros: u64) {
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

    let ratelimits = AI_RATELIMITS.read().unwrap();
    if !ratelimits.is_empty() {
        let mut wrote_rem_req = false;
        for ((provider, model), g) in ratelimits.iter() {
            let val = g.remaining_requests.load(Ordering::Relaxed);
            if val >= 0 {
                let rst = g.reset_requests_timestamp_micros.load(Ordering::Relaxed);
                let last = g.last_observed_requests_micros.load(Ordering::Relaxed);
                let is_stale = if rst > 0 {
                    now_micros >= rst
                } else {
                    now_micros.saturating_sub(last) > MAX_REQUEST_OBSERVATION_AGE_MICROS
                };
                if !is_stale {
                    if !wrote_rem_req {
                        out.push_str(
                            "# HELP ai_ratelimit_remaining_requests Remaining Groq requests in current time window (RPD)\n\
                             # TYPE ai_ratelimit_remaining_requests gauge\n",
                        );
                        wrote_rem_req = true;
                    }
                    out.push_str(&format!(
                        "ai_ratelimit_remaining_requests{{provider=\"{provider}\",model=\"{model}\"}} {val}\n"
                    ));
                }
            }
        }

        let mut wrote_rem_tok = false;
        for ((provider, model), g) in ratelimits.iter() {
            let val = g.remaining_tokens.load(Ordering::Relaxed);
            if val >= 0 {
                let rst = g.reset_tokens_timestamp_micros.load(Ordering::Relaxed);
                let last = g.last_observed_tokens_micros.load(Ordering::Relaxed);
                let is_stale = if rst > 0 {
                    now_micros >= rst
                } else {
                    now_micros.saturating_sub(last) > MAX_TOKEN_OBSERVATION_AGE_MICROS
                };
                if !is_stale {
                    if !wrote_rem_tok {
                        out.push_str(
                            "# HELP ai_ratelimit_remaining_tokens Remaining Groq tokens in current time window (TPM)\n\
                             # TYPE ai_ratelimit_remaining_tokens gauge\n",
                        );
                        wrote_rem_tok = true;
                    }
                    out.push_str(&format!(
                        "ai_ratelimit_remaining_tokens{{provider=\"{provider}\",model=\"{model}\"}} {val}\n"
                    ));
                }
            }
        }

        let mut wrote_lim_req = false;
        for ((provider, model), g) in ratelimits.iter() {
            let val = g.limit_requests.load(Ordering::Relaxed);
            if val >= 0 {
                if !wrote_lim_req {
                    out.push_str(
                        "# HELP ai_ratelimit_limit_requests Total Groq request limit in time window (RPD)\n\
                         # TYPE ai_ratelimit_limit_requests gauge\n",
                    );
                    wrote_lim_req = true;
                }
                out.push_str(&format!(
                    "ai_ratelimit_limit_requests{{provider=\"{provider}\",model=\"{model}\"}} {val}\n"
                ));
            }
        }

        let mut wrote_lim_tok = false;
        for ((provider, model), g) in ratelimits.iter() {
            let val = g.limit_tokens.load(Ordering::Relaxed);
            if val >= 0 {
                if !wrote_lim_tok {
                    out.push_str(
                        "# HELP ai_ratelimit_limit_tokens Total Groq token limit in time window (TPM)\n\
                         # TYPE ai_ratelimit_limit_tokens gauge\n",
                    );
                    wrote_lim_tok = true;
                }
                out.push_str(&format!(
                    "ai_ratelimit_limit_tokens{{provider=\"{provider}\",model=\"{model}\"}} {val}\n"
                ));
            }
        }

        let mut wrote_rst_req = false;
        for ((provider, model), g) in ratelimits.iter() {
            let val = g.reset_requests_timestamp_micros.load(Ordering::Relaxed);
            if val > 0 && now_micros < val {
                if !wrote_rst_req {
                    out.push_str(
                        "# HELP ai_ratelimit_reset_requests_timestamp_seconds Expected unix timestamp when request limit counter resets\n\
                         # TYPE ai_ratelimit_reset_requests_timestamp_seconds gauge\n",
                    );
                    wrote_rst_req = true;
                }
                let secs = val as f64 / 1_000_000.0;
                out.push_str(&format!(
                    "ai_ratelimit_reset_requests_timestamp_seconds{{provider=\"{provider}\",model=\"{model}\"}} {secs:.3}\n"
                ));
            }
        }

        let mut wrote_rst_tok = false;
        for ((provider, model), g) in ratelimits.iter() {
            let val = g.reset_tokens_timestamp_micros.load(Ordering::Relaxed);
            if val > 0 && now_micros < val {
                if !wrote_rst_tok {
                    out.push_str(
                        "# HELP ai_ratelimit_reset_tokens_timestamp_seconds Expected unix timestamp when token limit counter resets\n\
                         # TYPE ai_ratelimit_reset_tokens_timestamp_seconds gauge\n",
                    );
                    wrote_rst_tok = true;
                }
                let secs = val as f64 / 1_000_000.0;
                out.push_str(&format!(
                    "ai_ratelimit_reset_tokens_timestamp_seconds{{provider=\"{provider}\",model=\"{model}\"}} {secs:.3}\n"
                ));
            }
        }

        let mut wrote_last_obs = false;
        for ((provider, model), g) in ratelimits.iter() {
            let last_req = g.last_observed_requests_micros.load(Ordering::Relaxed);
            if last_req > 0 {
                if !wrote_last_obs {
                    out.push_str(
                        "# HELP ai_ratelimit_last_observed_timestamp_seconds Unix timestamp when rate limit capacity was last observed\n\
                         # TYPE ai_ratelimit_last_observed_timestamp_seconds gauge\n",
                    );
                    wrote_last_obs = true;
                }
                let secs = last_req as f64 / 1_000_000.0;
                out.push_str(&format!(
                    "ai_ratelimit_last_observed_timestamp_seconds{{provider=\"{provider}\",model=\"{model}\",dimension=\"requests\"}} {secs:.3}\n"
                ));
            }
            let last_tok = g.last_observed_tokens_micros.load(Ordering::Relaxed);
            if last_tok > 0 {
                if !wrote_last_obs {
                    out.push_str(
                        "# HELP ai_ratelimit_last_observed_timestamp_seconds Unix timestamp when rate limit capacity was last observed\n\
                         # TYPE ai_ratelimit_last_observed_timestamp_seconds gauge\n",
                    );
                    wrote_last_obs = true;
                }
                let secs = last_tok as f64 / 1_000_000.0;
                out.push_str(&format!(
                    "ai_ratelimit_last_observed_timestamp_seconds{{provider=\"{provider}\",model=\"{model}\",dimension=\"tokens\"}} {secs:.3}\n"
                ));
            }
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

    #[test]
    fn groq_ratelimit_gauges_render_valid_exposition_and_bounded_cardinality() {
        // 1. Valid known model
        observe_groq_ratelimit(
            "llama-3.3-70b-versatile",
            Some(14399),
            Some(29841),
            Some(14400),
            Some(30000),
            Some(Duration::from_secs(300)),
            Some(Duration::from_secs(600)),
        );

        let mut out = String::new();
        push_ai_metrics(&mut out);

        assert!(out.contains("ai_ratelimit_remaining_requests{provider=\"groq\",model=\"llama-3.3-70b-versatile\"} 14399"));
        assert!(out.contains("ai_ratelimit_remaining_tokens{provider=\"groq\",model=\"llama-3.3-70b-versatile\"} 29841"));
        assert!(out.contains("ai_ratelimit_limit_requests{provider=\"groq\",model=\"llama-3.3-70b-versatile\"} 14400"));
        assert!(out.contains("ai_ratelimit_limit_tokens{provider=\"groq\",model=\"llama-3.3-70b-versatile\"} 30000"));
        assert!(out.contains("ai_ratelimit_reset_requests_timestamp_seconds{provider=\"groq\",model=\"llama-3.3-70b-versatile\"}"));
        assert!(out.contains("ai_ratelimit_reset_tokens_timestamp_seconds{provider=\"groq\",model=\"llama-3.3-70b-versatile\"}"));
        assert!(out.contains("ai_ratelimit_last_observed_timestamp_seconds{provider=\"groq\",model=\"llama-3.3-70b-versatile\",dimension=\"requests\"}"));
        assert!(out.contains("ai_ratelimit_last_observed_timestamp_seconds{provider=\"groq\",model=\"llama-3.3-70b-versatile\",dimension=\"tokens\"}"));

        // 2. Unknown model must map to "other" to enforce strictly bounded cardinality
        observe_groq_ratelimit(
            "arbitrary_unknown_model_attempting_cardinality_explosion_12345",
            Some(500),
            Some(2000),
            Some(1000),
            Some(5000),
            None,
            None,
        );

        let mut out2 = String::new();
        push_ai_metrics(&mut out2);

        assert!(!out2.contains("arbitrary_unknown_model_attempting_cardinality_explosion_12345"));
        assert!(out2.contains("model=\"other\""));
        assert!(out2.contains("ai_ratelimit_remaining_requests{provider=\"groq\",model=\"other\"} 500"));
    }

    #[test]
    fn groq_ratelimit_missing_headers_and_zero_values_handled_safely() {
        let model = "deepseek-r1-distill-llama-70b";

        // 429 response with 0 remaining requests must record 0, not missing or unobserved
        observe_groq_ratelimit(
            model,
            Some(0),
            Some(120),
            None, // Missing limits
            None,
            None,
            None,
        );

        let mut out = String::new();
        push_ai_metrics(&mut out);

        let rem_req_needle = format!("ai_ratelimit_remaining_requests{{provider=\"groq\",model=\"{model}\"}} 0");
        let rem_tok_needle = format!("ai_ratelimit_remaining_tokens{{provider=\"groq\",model=\"{model}\"}} 120");
        let lim_req_needle = format!("ai_ratelimit_limit_requests{{provider=\"groq\",model=\"{model}\"}}");

        assert!(out.contains(&rem_req_needle));
        assert!(out.contains(&rem_tok_needle));
        // Limits were None, so no limit metric emitted for this model
        assert!(!out.contains(&lim_req_needle));

        // Follow-up observation with only limits should preserve existing remaining counts
        observe_groq_ratelimit(
            model,
            None,
            None,
            Some(1000),
            Some(8000),
            None,
            None,
        );

        let mut out2 = String::new();
        push_ai_metrics(&mut out2);
        assert!(out2.contains(&rem_req_needle));
        assert!(out2.contains(&format!("ai_ratelimit_limit_requests{{provider=\"groq\",model=\"{model}\"}} 1000")));
        assert!(out2.contains(&format!("ai_ratelimit_limit_tokens{{provider=\"groq\",model=\"{model}\"}} 8000")));
    }

    #[test]
    fn groq_ratelimit_resets_produce_absolute_timestamps_not_frozen_durations() {
        let model = "llama-3.1-8b-instant";
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        observe_groq_ratelimit(
            model,
            None,
            None,
            None,
            None,
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(10)),
        );

        let mut out = String::new();
        push_ai_metrics(&mut out);

        let req_needle = format!("ai_ratelimit_reset_requests_timestamp_seconds{{provider=\"groq\",model=\"{model}\"}} ");
        let req_line = out
            .lines()
            .find(|l| l.starts_with(&req_needle))
            .expect("request reset timestamp exists");
        let req_ts: f64 = req_line.rsplit(' ').next().unwrap().parse().unwrap();

        let tok_needle = format!("ai_ratelimit_reset_tokens_timestamp_seconds{{provider=\"groq\",model=\"{model}\"}} ");
        let tok_line = out
            .lines()
            .find(|l| l.starts_with(&tok_needle))
            .expect("token reset timestamp exists");
        let tok_ts: f64 = tok_line.rsplit(' ').next().unwrap().parse().unwrap();

        // Absolute timestamps should be roughly before + 5s and before + 10s
        assert!(req_ts >= before + 4.9 && req_ts <= before + 6.5);
        assert!(tok_ts >= before + 9.9 && tok_ts <= before + 11.5);
        assert!(tok_ts > req_ts);
    }

    #[test]
    fn groq_ratelimit_low_capacity_warnings_deduplicate_correctly() {
        let model = "gemma2-9b-it";
        let key = ("groq".to_string(), model.to_string());

        // Transition: remaining drops below 10 -> low_requests_warned becomes true
        observe_groq_ratelimit(model, Some(5), Some(5000), Some(100), Some(10000), None, None);

        {
            let read = AI_RATELIMITS.read().unwrap();
            let g = read.get(&key).expect("model gauges must exist");
            assert!(g.low_requests_warned.load(Ordering::Relaxed));
            assert!(!g.low_tokens_warned.load(Ordering::Relaxed)); // 5000 / 10000 is 50%, not < 10%
        }

        // Subsequent observation below 10 should keep warning flag true (avoiding repeated spam)
        observe_groq_ratelimit(model, Some(4), Some(500), Some(100), Some(10000), None, None);
        {
            let read = AI_RATELIMITS.read().unwrap();
            let g = read.get(&key).expect("model gauges must exist");
            assert!(g.low_requests_warned.load(Ordering::Relaxed));
            assert!(g.low_tokens_warned.load(Ordering::Relaxed)); // 500 / 10000 is 5%, < 10%
        }

        // Recovery: remaining returns above 10 and 10% -> flags reset
        observe_groq_ratelimit(model, Some(50), Some(9000), Some(100), Some(10000), None, None);
        {
            let read = AI_RATELIMITS.read().unwrap();
            let g = read.get(&key).expect("model gauges must exist");
            assert!(!g.low_requests_warned.load(Ordering::Relaxed));
            assert!(!g.low_tokens_warned.load(Ordering::Relaxed));
        }
    }

    #[test]
    fn groq_ratelimit_concurrent_observations_are_thread_safe() {
        let model = "llama-3.1-70b-versatile";
        use std::thread;

        let handles: Vec<_> = (0..16)
            .map(|i| {
                thread::spawn(move || {
                    observe_groq_ratelimit(
                        model,
                        Some(100 + i),
                        Some(5000 + i * 10),
                        Some(1000),
                        Some(10000),
                        Some(Duration::from_secs(i + 1)),
                        Some(Duration::from_secs(i + 2)),
                    );
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let mut out = String::new();
        push_ai_metrics(&mut out);
        assert!(out.contains(&format!("ai_ratelimit_remaining_requests{{provider=\"groq\",model=\"{model}\"}}")));
        assert!(out.contains(&format!("ai_ratelimit_remaining_tokens{{provider=\"groq\",model=\"{model}\"}}")));
    }

    #[test]
    fn groq_ratelimit_token_freshness_suppression_and_resurgence() {
        let model = "qwen/qwen3.8-27b";
        let base_micros: u64 = 1_700_000_000_000_000;

        // 1. Observation with reset duration of 10 seconds
        observe_groq_ratelimit_with_time(
            model,
            None,
            Some(5000),
            None,
            Some(10000),
            None,
            Some(Duration::from_secs(10)),
            base_micros,
        );

        // Render at base + 5s (fresh)
        let mut out_fresh = String::new();
        push_ai_metrics_with_time(&mut out_fresh, base_micros + 5_000_000);
        assert!(out_fresh.contains(&format!("ai_ratelimit_remaining_tokens{{provider=\"groq\",model=\"{model}\"}} 5000")));
        assert!(out_fresh.contains(&format!("ai_ratelimit_reset_tokens_timestamp_seconds{{provider=\"groq\",model=\"{model}\"}}")));
        assert!(out_fresh.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"tokens\"}}")));

        // Render at base + 10s (now >= reset timestamp -> STALE suppression)
        let mut out_stale = String::new();
        push_ai_metrics_with_time(&mut out_stale, base_micros + 10_000_000);
        assert!(!out_stale.contains(&format!("ai_ratelimit_remaining_tokens{{provider=\"groq\",model=\"{model}\"}}")));
        assert!(!out_stale.contains(&format!("ai_ratelimit_reset_tokens_timestamp_seconds{{provider=\"groq\",model=\"{model}\"}}")));
        // Last observed timestamp should still be present
        assert!(out_stale.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"tokens\"}}")));

        // 2. Resurgence: fresh observation arrives at base + 15s
        observe_groq_ratelimit_with_time(
            model,
            None,
            Some(9500),
            None,
            Some(10000),
            None,
            Some(Duration::from_secs(30)),
            base_micros + 15_000_000,
        );

        let mut out_resurge = String::new();
        push_ai_metrics_with_time(&mut out_resurge, base_micros + 16_000_000);
        assert!(out_resurge.contains(&format!("ai_ratelimit_remaining_tokens{{provider=\"groq\",model=\"{model}\"}} 9500")));

        // 3. Fallback when reset header is missing: expires after 120s
        let fallback_base = base_micros + 100_000_000;
        observe_groq_ratelimit_with_time(
            model,
            None,
            Some(7500),
            None,
            None,
            None,
            None,
            fallback_base,
        );

        // At +60s: not stale yet (<= 120s)
        let mut out_mid = String::new();
        push_ai_metrics_with_time(&mut out_mid, fallback_base + 60_000_000);
        assert!(out_mid.contains(&format!("ai_ratelimit_remaining_tokens{{provider=\"groq\",model=\"{model}\"}} 7500")));

        // At +121s: stale (> 120s) -> suppressed
        let mut out_exp = String::new();
        push_ai_metrics_with_time(&mut out_exp, fallback_base + 121_000_000);
        assert!(!out_exp.contains(&format!("ai_ratelimit_remaining_tokens{{provider=\"groq\",model=\"{model}\"}}")));
    }

    #[test]
    fn groq_ratelimit_request_freshness_suppression_and_resurgence() {
        let model = "qwen-2.5-32b";
        let base_micros: u64 = 1_700_000_000_000_000;

        // 1. Observation with reset duration of 60 seconds
        observe_groq_ratelimit_with_time(
            model,
            Some(14000),
            None,
            Some(14400),
            None,
            Some(Duration::from_secs(60)),
            None,
            base_micros,
        );

        // Render at base + 30s (fresh)
        let mut out_fresh = String::new();
        push_ai_metrics_with_time(&mut out_fresh, base_micros + 30_000_000);
        assert!(out_fresh.contains(&format!("ai_ratelimit_remaining_requests{{provider=\"groq\",model=\"{model}\"}} 14000")));
        assert!(out_fresh.contains(&format!("ai_ratelimit_reset_requests_timestamp_seconds{{provider=\"groq\",model=\"{model}\"}}")));
        assert!(out_fresh.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"requests\"}}")));

        // Render at base + 60s (now >= reset timestamp -> STALE suppression)
        let mut out_stale = String::new();
        push_ai_metrics_with_time(&mut out_stale, base_micros + 60_000_000);
        assert!(!out_stale.contains(&format!("ai_ratelimit_remaining_requests{{provider=\"groq\",model=\"{model}\"}}")));
        assert!(!out_stale.contains(&format!("ai_ratelimit_reset_requests_timestamp_seconds{{provider=\"groq\",model=\"{model}\"}}")));
        assert!(out_stale.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"requests\"}}")));

        // 2. Resurgence: fresh observation arrives at base + 70s
        observe_groq_ratelimit_with_time(
            model,
            Some(14399),
            None,
            Some(14400),
            None,
            Some(Duration::from_secs(10)),
            None,
            base_micros + 70_000_000,
        );

        let mut out_resurge = String::new();
        push_ai_metrics_with_time(&mut out_resurge, base_micros + 71_000_000);
        assert!(out_resurge.contains(&format!("ai_ratelimit_remaining_requests{{provider=\"groq\",model=\"{model}\"}} 14399")));

        // 3. Fallback when reset header is missing: expires after 86,400s (24h)
        let fallback_base = base_micros + 100_000_000;
        observe_groq_ratelimit_with_time(
            model,
            Some(12000),
            None,
            None,
            None,
            None,
            None,
            fallback_base,
        );

        // At +43,200s (12h): not stale yet (<= 86400s)
        let mut out_mid = String::new();
        push_ai_metrics_with_time(&mut out_mid, fallback_base + 43_200 * 1_000_000);
        assert!(out_mid.contains(&format!("ai_ratelimit_remaining_requests{{provider=\"groq\",model=\"{model}\"}} 12000")));

        // At +86,401s: stale (> 86400s) -> suppressed
        let mut out_exp = String::new();
        push_ai_metrics_with_time(&mut out_exp, fallback_base + 86_401 * 1_000_000);
        assert!(!out_exp.contains(&format!("ai_ratelimit_remaining_requests{{provider=\"groq\",model=\"{model}\"}}")));
    }

    #[test]
    fn groq_ratelimit_asymmetric_dimension_observation_updates_only_observed_freshness() {
        let model = "llama3-70b-8192";
        let t1: u64 = 1_700_000_000_000_000;
        let t2: u64 = t1 + 50_000_000;

        // Observation 1: only tokens
        observe_groq_ratelimit_with_time(
            model,
            None,
            Some(4000),
            None,
            None,
            None,
            Some(Duration::from_secs(300)),
            t1,
        );

        let mut out1 = String::new();
        push_ai_metrics_with_time(&mut out1, t1 + 1_000_000);
        assert!(out1.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"tokens\"}}")));
        assert!(!out1.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"requests\"}}")));

        // Observation 2: only requests at t2
        observe_groq_ratelimit_with_time(
            model,
            Some(100),
            None,
            None,
            None,
            Some(Duration::from_secs(300)),
            None,
            t2,
        );

        let mut out2 = String::new();
        push_ai_metrics_with_time(&mut out2, t2 + 1_000_000);

        let tok_ts_str = format!("{:.3}", t1 as f64 / 1_000_000.0);
        let req_ts_str = format!("{:.3}", t2 as f64 / 1_000_000.0);

        assert!(out2.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"tokens\"}} {tok_ts_str}")));
        assert!(out2.contains(&format!("ai_ratelimit_last_observed_timestamp_seconds{{provider=\"groq\",model=\"{model}\",dimension=\"requests\"}} {req_ts_str}")));
    }
}
