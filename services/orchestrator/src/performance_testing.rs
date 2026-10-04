//! Phase 6 — Performance Testing: real Load/Stress/Spike/Volume workload
//! generation against a target HTTP endpoint.
//!
//! Fresh baseline audit before writing this module (see the Phase 6
//! report): this codebase had no performance/load executor, no
//! JMeter/k6/Locust-style tool integrated, and no existing component that
//! safely schedules *many repeated, concurrent, identical* requests (the
//! `engine.rs` scheduler bounds concurrency across *different* DAG tasks in
//! a run, not repeated hits against one target). A dedicated orchestration
//! layer is therefore genuinely justified — but it reuses the real,
//! existing per-request execution core unmodified:
//!
//! ```text
//! PerformanceTestConfig (request template + WorkloadSchedule + thresholds)
//!         v
//! validate_and_build_plan()   -- REUSED, unmodified (api_testing.rs)
//! validate_plan()             -- REUSED, unmodified (real SSRF guard, run ONCE)
//!         v
//! run_workload()  -- NEW, minimal scheduler (this module)
//!         v
//! execute_plan_once()  -- REUSED, unmodified real reqwest core, extracted
//!                          from executor::CompiledApiExecutor in this same
//!                          phase so both the single-shot API Testing page
//!                          and this repeated workload call the identical
//!                          real request/assertion-evaluation logic
//!         v
//! RequestSample (real per-request latency/outcome)
//!         v
//! compute_metrics() -- real aggregation + documented percentile algorithm
//!         v
//! evaluate_verdict() -- COMPLETED (no thresholds) vs PASSED/FAILED
//!                        (thresholds configured) vs CANCELLED
//! ```
//!
//! Every sample comes from a real HTTP round-trip via `execute_plan_once`
//! (the same core `CompiledApiExecutor` uses) — no `sleep`-simulated
//! latency, no fabricated throughput. Concurrency is real Tokio tasks
//! racing a real `tokio::sync::watch` stop signal (the exact same
//! cancellation primitive `engine.rs`'s BE-13 run-cancellation already
//! uses), not a fake progress counter.

use crate::api_testing::{AdHocApiRequest, ValidationError};
use crate::domain::TaskStatus;
use crate::execution_plan::{Action, ExecutionPlan};
use crate::executor::execute_plan_once;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use uuid::Uuid;

// --------------------------------------------------------- configuration

/// Real, behaviorally-distinct workload schedules (Phase 6 brief §5) —
/// internally tagged like `execution_plan::Action`, so an unrecognized
/// `test_type` is a real deserialization error, never silently defaulted.
///
/// * `Load` — a steady worker pool of `concurrency` real concurrent
///   workers, each firing requests back-to-back, until `duration_secs`
///   elapses or (if set) `max_requests` real requests have been attempted,
///   whichever happens first.
/// * `Stress` — starts at `start_concurrency` real workers; every
///   `step_interval_secs` a real, additional batch of `step_size` workers
///   is spawned (via `tokio::spawn`, not simulated), up to
///   `max_concurrency`; the real, recorded concurrency-over-time schedule
///   is what distinguishes this from Load, not a relabeled Load run.
/// * `Spike` — `baseline_concurrency` workers run for `baseline_secs`, then
///   the *entire* remaining `(spike_concurrency - baseline_concurrency)`
///   worker batch is spawned in one single burst (not incrementally, unlike
///   Stress) and held for `spike_secs` — the real, immediate jump is the
///   behavioral distinction from Stress's gradual ramp.
/// * `Volume` — a fixed, moderate `concurrency` drives exactly
///   `total_requests` real requests to completion; the metric of interest
///   is real total count/throughput at a stable concurrency, not a
///   response-time curve under increasing load.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(tag = "test_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkloadSchedule {
    Load {
        concurrency: u32,
        duration_secs: u64,
        max_requests: Option<u64>,
    },
    Stress {
        start_concurrency: u32,
        max_concurrency: u32,
        step_size: u32,
        step_interval_secs: u64,
        max_duration_secs: u64,
    },
    Spike {
        baseline_concurrency: u32,
        spike_concurrency: u32,
        baseline_secs: u64,
        spike_secs: u64,
    },
    Volume {
        concurrency: u32,
        total_requests: u64,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PerformanceTestConfig {
    /// Reuses `api_testing::AdHocApiRequest`'s exact real request shape
    /// (method/url/query/headers/body). `assertions` must be empty here —
    /// a performance test's real per-request correctness check is the
    /// single, simple `expected_status` field below, deliberately distinct
    /// from the full API Testing assertions list (which would be redundant
    /// per-request overhead and ambiguous under load).
    pub request: AdHocApiRequest,
    #[serde(default)]
    pub expected_status: Option<u16>,
    pub schedule: WorkloadSchedule,
    #[serde(default)]
    pub max_p95_ms: Option<u64>,
    #[serde(default)]
    pub max_error_rate_percent: Option<f64>,
    #[serde(default)]
    pub min_throughput_rps: Option<f64>,
}

/// Server-side-enforced safety ceilings (Phase 6 brief §7) — the frontend
/// may mirror these for fast UX feedback, but `validate_schedule` is the
/// authoritative check; nothing here is trusted from the frontend alone.
/// Env-var-overridable, same convention as `API_TEST_TIMEOUT_SECS`.
#[derive(Debug, Clone, Copy)]
pub struct SafetyLimits {
    pub max_concurrency: u32,
    pub max_duration_secs: u64,
    pub max_total_requests: u64,
}

impl SafetyLimits {
    pub fn from_env() -> Self {
        Self {
            max_concurrency: std::env::var("PERF_TEST_MAX_CONCURRENCY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            max_duration_secs: std::env::var("PERF_TEST_MAX_DURATION_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            max_total_requests: std::env::var("PERF_TEST_MAX_TOTAL_REQUESTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000),
        }
    }
}

/// Builds the one real per-request `ExecutionPlan` every worker replays —
/// reuses `api_testing::validate_and_build_plan` unmodified. `expected_status`
/// becomes a real `Action::AssertStatus`, so every sample's Success/Failed
/// classification comes from `execute_plan_once`'s own real assertion
/// evaluation, not a second, hand-rolled comparison.
pub fn build_request_plan(
    id: &str,
    req: &AdHocApiRequest,
    expected_status: Option<u16>,
) -> Result<ExecutionPlan, ValidationError> {
    if !req.assertions.is_empty() {
        return Err((
            "INVALID_REQUEST_CONFIGURATION",
            "performance test requests use `expected_status`, not a full assertions list"
                .to_string(),
        ));
    }
    let mut built = req.clone();
    if let Some(status) = expected_status {
        built.assertions = vec![Action::AssertStatus { status }];
    }
    crate::api_testing::validate_and_build_plan(id.to_string(), &built)
}

pub fn validate_schedule(
    schedule: &WorkloadSchedule,
    limits: &SafetyLimits,
) -> Result<(), ValidationError> {
    match *schedule {
        WorkloadSchedule::Load {
            concurrency,
            duration_secs,
            max_requests,
        } => {
            if concurrency == 0 {
                return Err((
                    "INVALID_SCHEDULE",
                    "concurrency must be at least 1".to_string(),
                ));
            }
            if concurrency > limits.max_concurrency {
                return Err((
                    "CONCURRENCY_LIMIT_EXCEEDED",
                    format!(
                        "concurrency {concurrency} exceeds the server-enforced maximum of {}",
                        limits.max_concurrency
                    ),
                ));
            }
            if duration_secs == 0 || duration_secs > limits.max_duration_secs {
                return Err((
                    "DURATION_LIMIT_EXCEEDED",
                    format!(
                        "duration_secs must be between 1 and {}",
                        limits.max_duration_secs
                    ),
                ));
            }
            if let Some(m) = max_requests {
                if m == 0 || m > limits.max_total_requests {
                    return Err((
                        "TOTAL_REQUESTS_LIMIT_EXCEEDED",
                        format!(
                            "max_requests must be between 1 and {}",
                            limits.max_total_requests
                        ),
                    ));
                }
            }
        }
        WorkloadSchedule::Stress {
            start_concurrency,
            max_concurrency,
            step_size,
            step_interval_secs,
            max_duration_secs,
        } => {
            if start_concurrency == 0 || step_size == 0 || step_interval_secs == 0 {
                return Err((
                    "INVALID_SCHEDULE",
                    "start_concurrency, step_size and step_interval_secs must all be at least 1"
                        .to_string(),
                ));
            }
            if max_concurrency < start_concurrency {
                return Err((
                    "INVALID_SCHEDULE",
                    "max_concurrency must be >= start_concurrency".to_string(),
                ));
            }
            if max_concurrency > limits.max_concurrency {
                return Err(("CONCURRENCY_LIMIT_EXCEEDED", format!("max_concurrency {max_concurrency} exceeds the server-enforced maximum of {}", limits.max_concurrency)));
            }
            if max_duration_secs == 0 || max_duration_secs > limits.max_duration_secs {
                return Err((
                    "DURATION_LIMIT_EXCEEDED",
                    format!(
                        "max_duration_secs must be between 1 and {}",
                        limits.max_duration_secs
                    ),
                ));
            }
        }
        WorkloadSchedule::Spike {
            baseline_concurrency,
            spike_concurrency,
            baseline_secs,
            spike_secs,
        } => {
            if spike_concurrency <= baseline_concurrency {
                return Err((
                    "INVALID_SCHEDULE",
                    "spike_concurrency must be greater than baseline_concurrency".to_string(),
                ));
            }
            if spike_concurrency > limits.max_concurrency {
                return Err(("CONCURRENCY_LIMIT_EXCEEDED", format!("spike_concurrency {spike_concurrency} exceeds the server-enforced maximum of {}", limits.max_concurrency)));
            }
            let total = baseline_secs.saturating_add(spike_secs);
            if total == 0 || total > limits.max_duration_secs {
                return Err((
                    "DURATION_LIMIT_EXCEEDED",
                    format!(
                        "baseline_secs + spike_secs must be between 1 and {}",
                        limits.max_duration_secs
                    ),
                ));
            }
        }
        WorkloadSchedule::Volume {
            concurrency,
            total_requests,
        } => {
            if concurrency == 0 {
                return Err((
                    "INVALID_SCHEDULE",
                    "concurrency must be at least 1".to_string(),
                ));
            }
            if concurrency > limits.max_concurrency {
                return Err((
                    "CONCURRENCY_LIMIT_EXCEEDED",
                    format!(
                        "concurrency {concurrency} exceeds the server-enforced maximum of {}",
                        limits.max_concurrency
                    ),
                ));
            }
            if total_requests == 0 || total_requests > limits.max_total_requests {
                return Err((
                    "TOTAL_REQUESTS_LIMIT_EXCEEDED",
                    format!(
                        "total_requests must be between 1 and {}",
                        limits.max_total_requests
                    ),
                ));
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------- samples/metrics

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SampleOutcome {
    /// The request completed and (if `expected_status` was configured) the
    /// real response status matched it.
    Success,
    /// The request completed but the real response status did not match
    /// `expected_status` — a real assertion mismatch, not a transport error.
    Failed,
    /// The request itself never completed — real
    /// timeout/DNS/connection/TLS failure, per
    /// `executor::classify_reqwest_error`'s own vocabulary (carried in
    /// `error_reason`).
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestSample {
    pub sequence: u64,
    /// Milliseconds since the workload started when this request began —
    /// a real relative timeline, letting the frontend chart latency and
    /// concurrency together without needing full per-sample timestamps.
    pub offset_ms: u64,
    pub duration_ms: u64,
    pub outcome: SampleOutcome,
    pub http_status: Option<u16>,
    pub error_reason: Option<String>,
    /// The real, live worker-count in effect when this request began —
    /// lets Stress/Spike charts truthfully overlay "concurrency at this
    /// moment" against latency, read from the same atomic the scheduler
    /// itself updates as it ramps.
    pub concurrency_at_start: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceMetrics {
    pub total_requests: u64,
    pub successful: u64,
    pub failed: u64,
    pub errored: u64,
    pub error_rate_percent: f64,
    pub success_rate_percent: f64,
    pub min_ms: u64,
    pub avg_ms: f64,
    pub max_ms: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
    pub throughput_rps: f64,
    pub actual_duration_ms: u64,
}

/// Real percentile via the nearest-rank method: sort ascending, take the
/// value at rank `ceil(p/100 * N)` (1-indexed), clamped into range.
/// Deliberately chosen over linear interpolation — it always returns an
/// *actual observed sample*, never a synthetic number between two real
/// requests, so "P95 is 842ms" always means "a real request really took
/// 842ms," which is easier to reason about and matches how most real load
/// testing tools report a single fixed dataset's percentiles.
///
/// Worked example, `sorted_ms = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]`
/// (N=10): P50 -> rank `ceil(0.50*10)=5` -> index 4 -> `50`. P95 -> rank
/// `ceil(0.95*10)=10` -> index 9 -> `100`. See the unit test with this
/// exact dataset below.
pub fn percentile(sorted_ms: &[u64], p: f64) -> u64 {
    if sorted_ms.is_empty() {
        return 0;
    }
    let rank = (p / 100.0 * sorted_ms.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted_ms.len() - 1);
    sorted_ms[idx]
}

/// Real aggregation from real samples only — every field here is computed
/// directly from the `samples` slice, never hardcoded or estimated.
/// `actual_duration_ms` is the real wall-clock span of the workload (passed
/// in by the caller, who alone knows when it started/stopped), used only
/// for `throughput_rps` — never conflated with any individual request's own
/// `duration_ms`.
pub fn compute_metrics(samples: &[RequestSample], actual_duration_ms: u64) -> PerformanceMetrics {
    let total = samples.len() as u64;
    if total == 0 {
        return PerformanceMetrics {
            actual_duration_ms,
            ..Default::default()
        };
    }
    let successful = samples
        .iter()
        .filter(|s| s.outcome == SampleOutcome::Success)
        .count() as u64;
    let failed = samples
        .iter()
        .filter(|s| s.outcome == SampleOutcome::Failed)
        .count() as u64;
    let errored = samples
        .iter()
        .filter(|s| s.outcome == SampleOutcome::Error)
        .count() as u64;

    let mut durations: Vec<u64> = samples.iter().map(|s| s.duration_ms).collect();
    durations.sort_unstable();
    let sum: u64 = durations.iter().sum();
    let min_ms = *durations.first().unwrap();
    let max_ms = *durations.last().unwrap();
    let avg_ms = sum as f64 / total as f64;
    let throughput_rps = if actual_duration_ms > 0 {
        total as f64 / (actual_duration_ms as f64 / 1000.0)
    } else {
        0.0
    };

    PerformanceMetrics {
        total_requests: total,
        successful,
        failed,
        errored,
        error_rate_percent: ((failed + errored) as f64 / total as f64) * 100.0,
        success_rate_percent: (successful as f64 / total as f64) * 100.0,
        min_ms,
        avg_ms,
        max_ms,
        p50_ms: percentile(&durations, 50.0),
        p95_ms: percentile(&durations, 95.0),
        p99_ms: percentile(&durations, 99.0),
        throughput_rps,
        actual_duration_ms,
    }
}

// ------------------------------------------------------------- verdict

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PerfVerdict {
    /// No thresholds were configured — real metrics are reported, but no
    /// pass/fail judgement is made (Phase 6 brief §17: never inflate a
    /// threshold-less run into a false PASS).
    Completed,
    Passed,
    Failed,
    /// The workload could not run to completion for an infrastructure
    /// reason (distinct from a per-request `SampleOutcome::Error`, which is
    /// real performance data, not a test-execution fault). Not reachable
    /// from `evaluate_verdict` today — target validation happens before any
    /// run is created, so nothing left to fail this way once a run starts;
    /// reserved for a genuine future infra-fault path. An abandoned run
    /// from a crashed process is instead marked `INTERRUPTED` (a distinct,
    /// run-`status` value, not a verdict) by the startup recovery sweep.
    Error,
    Cancelled,
}

impl PerfVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            PerfVerdict::Completed => "COMPLETED",
            PerfVerdict::Passed => "PASSED",
            PerfVerdict::Failed => "FAILED",
            PerfVerdict::Error => "ERROR",
            PerfVerdict::Cancelled => "CANCELLED",
        }
    }
}

/// Real, documented threshold evaluation — only evaluated when at least one
/// threshold field is `Some`; each violation is reported with the actual
/// measured value alongside the configured limit, never a bare true/false.
///
/// A verdict of `Passed` additionally requires that at least one request
/// actually succeeded: a run whose every sample failed or errored cannot pass
/// on latency grounds alone.
pub fn evaluate_verdict(
    config: &PerformanceTestConfig,
    metrics: &PerformanceMetrics,
    cancelled: bool,
) -> (PerfVerdict, Vec<String>) {
    if cancelled {
        return (
            PerfVerdict::Cancelled,
            vec!["cancelled by user request before the configured schedule completed".to_string()],
        );
    }
    let has_thresholds = config.max_p95_ms.is_some()
        || config.max_error_rate_percent.is_some()
        || config.min_throughput_rps.is_some();
    if !has_thresholds {
        return (PerfVerdict::Completed, vec!["no thresholds were configured — reporting real measured metrics only, no pass/fail judgement".to_string()]);
    }
    let mut violations = Vec::new();
    // Request correctness is judged before any latency/throughput threshold.
    // `expected_status` decides each sample's outcome (`SampleOutcome` comes
    // straight from the request's `TaskStatus`), so a run where not one sample
    // succeeded is a run in which nothing worked. Without this check such a run
    // could still be reported `Passed` on the strength of a latency threshold
    // alone — 200 requests, 0 successes, 100% error rate, verdict PASSED.
    //
    // Gated on `successful == 0` rather than on any failure at all, because
    // there is no configured success tolerance to judge a partial failure
    // against, and inventing one would invent a policy. "Not one request
    // worked" needs no policy.
    if metrics.total_requests > 0 && metrics.successful == 0 {
        violations.push(format!(
            "no request succeeded: all {} of {} samples failed or errored ({} failed, {} errored){}",
            metrics.total_requests,
            metrics.total_requests,
            metrics.failed,
            metrics.errored,
            match config.expected_status {
                Some(code) => format!(", none returned the expected status {code}"),
                None => String::new(),
            }
        ));
    }
    if let Some(max_p95) = config.max_p95_ms {
        if metrics.p95_ms > max_p95 {
            violations.push(format!(
                "P95 latency {}ms exceeded the configured threshold of {}ms",
                metrics.p95_ms, max_p95
            ));
        }
    }
    if let Some(max_err) = config.max_error_rate_percent {
        if metrics.error_rate_percent > max_err {
            violations.push(format!(
                "error rate {:.2}% exceeded the configured threshold of {:.2}%",
                metrics.error_rate_percent, max_err
            ));
        }
    }
    if let Some(min_tp) = config.min_throughput_rps {
        if metrics.throughput_rps < min_tp {
            violations.push(format!(
                "throughput {:.2} req/s was below the configured threshold of {:.2} req/s",
                metrics.throughput_rps, min_tp
            ));
        }
    }
    if violations.is_empty() {
        (
            PerfVerdict::Passed,
            vec!["all configured thresholds were met".to_string()],
        )
    } else {
        (PerfVerdict::Failed, violations)
    }
}

// -------------------------------------------------------------- engine

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PerfRunState {
    Running,
    Completed,
    Cancelled,
    Error,
}

impl PerfRunState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PerfRunState::Running => "RUNNING",
            PerfRunState::Completed => "COMPLETED",
            PerfRunState::Cancelled => "CANCELLED",
            PerfRunState::Error => "ERROR",
        }
    }
}

impl WorkloadSchedule {
    pub fn type_name(&self) -> &'static str {
        match self {
            WorkloadSchedule::Load { .. } => "LOAD",
            WorkloadSchedule::Stress { .. } => "STRESS",
            WorkloadSchedule::Spike { .. } => "SPIKE",
            WorkloadSchedule::Volume { .. } => "VOLUME",
        }
    }
}

/// Live, in-process handle for one running (or just-finished) performance
/// test — session-only (lost on restart, same honest limitation as
/// `engine.rs`'s own `runs` map; see `AppState::performance_tests`'s doc
/// comment).
///
/// Two distinct real Tokio `watch` channels, deliberately not one:
/// * `stop_new_tx` — "stop starting new request iterations." Fired by
///   natural completion (a deadline supervisor, or a worker whose reserved
///   ticket reached the configured total) *and* by an explicit user
///   cancel. A request already in flight when this fires is allowed to
///   finish and its real sample is still recorded — natural completion
///   must never silently discard a valid, already-reserved sample (the
///   root cause of a genuine early bug: see the Phase 6 report's bug
///   log — a Volume test undercounted because an in-flight *valid* request
///   was being raced against, and lost to, a *different* worker's
///   out-of-range ticket signalling stop at the same moment).
/// * `abort_tx` — "abort whatever is in flight right now, too." Fired
///   *only* by an explicit user cancel — this is what makes cancellation
///   genuinely interrupt a running request rather than waiting it out.
pub struct PerformanceRunHandle {
    pub id: Uuid,
    pub stop_new_tx: watch::Sender<bool>,
    pub abort_tx: watch::Sender<bool>,
    pub user_cancelled: Arc<AtomicBool>,
    pub state: Arc<RwLock<PerfRunState>>,
    pub samples: Arc<RwLock<Vec<RequestSample>>>,
    pub current_concurrency: Arc<AtomicU32>,
    pub active_workers: Arc<AtomicU32>,
    pub started_at: DateTime<Utc>,
    pub error_reason: Arc<RwLock<Option<String>>>,
    /// Set exactly once, by `run_workload` itself, the moment the real
    /// workload actually finishes — the frozen, true elapsed duration of
    /// the run. `None` while still `Running`. Reading elapsed time as
    /// `now - started_at` remains correct *while running*, but is wrong
    /// once terminal: `now` keeps advancing on every later poll even
    /// though nothing about the run is still happening, which would
    /// silently shrink `throughput_rps` (recomputed against a growing,
    /// unrelated denominator) on every repeat status read after
    /// completion — a real bug caught during this phase's own live
    /// verification (see the report's bug log). Status reads must use this
    /// frozen value once it is `Some`, never a fresh `now`-based one.
    pub finished_elapsed_ms: Arc<RwLock<Option<u64>>>,
}

impl PerformanceRunHandle {
    pub fn new(id: Uuid) -> Arc<Self> {
        let (stop_new_tx, _) = watch::channel(false);
        let (abort_tx, _) = watch::channel(false);
        Arc::new(Self {
            id,
            stop_new_tx,
            abort_tx,
            user_cancelled: Arc::new(AtomicBool::new(false)),
            state: Arc::new(RwLock::new(PerfRunState::Running)),
            samples: Arc::new(RwLock::new(Vec::new())),
            current_concurrency: Arc::new(AtomicU32::new(0)),
            active_workers: Arc::new(AtomicU32::new(0)),
            started_at: Utc::now(),
            error_reason: Arc::new(RwLock::new(None)),
            finished_elapsed_ms: Arc::new(RwLock::new(None)),
        })
    }

    /// Real cancellation — signals both channels, so no new request starts
    /// *and* whatever is currently in flight is genuinely interrupted.
    /// Returns `true` if this handle was actually still running (a second
    /// call, or a call after natural completion, is a real no-op, not an
    /// error — mirrors `engine.rs`'s own idempotent cancel semantics).
    pub async fn request_cancel(&self) -> bool {
        if *self.state.read().await != PerfRunState::Running {
            return false;
        }
        self.user_cancelled.store(true, Ordering::SeqCst);
        let _ = self.stop_new_tx.send(true);
        let _ = self.abort_tx.send(true);
        true
    }
}

async fn spawn_worker(
    client: reqwest::Client,
    plan: Arc<ExecutionPlan>,
    handle: Arc<PerformanceRunHandle>,
    sequence: Arc<AtomicU64>,
    start: tokio::time::Instant,
    count_target: Option<u64>,
) {
    handle.active_workers.fetch_add(1, Ordering::SeqCst);
    let stop_new_rx = handle.stop_new_tx.subscribe();
    let mut abort_rx = handle.abort_tx.subscribe();
    loop {
        if *stop_new_rx.borrow() {
            break;
        }
        let seq = sequence.fetch_add(1, Ordering::SeqCst);
        if let Some(target) = count_target {
            if seq >= target {
                // This real, shared ticket counter reached its target —
                // tell every worker to stop starting new iterations
                // (idempotent: `send` on an already-true channel is a
                // harmless no-op). Does NOT abort anyone else's in-flight
                // request — their own reservation was valid and their real
                // sample must still be recorded.
                let _ = handle.stop_new_tx.send(true);
                break;
            }
        }
        if *abort_rx.borrow() {
            break; // a cancel raced in between this reservation and firing it
        }

        let req_start = tokio::time::Instant::now();
        let result = tokio::select! {
            r = execute_plan_once(&client, &plan) => r,
            _ = abort_rx.changed() => break, // genuine user cancel interrupted this in-flight request; no sample recorded for the aborted attempt
        };
        let duration_ms = req_start.elapsed().as_millis() as u64;
        let offset_ms = req_start.duration_since(start).as_millis() as u64;
        let outcome = match result.status {
            TaskStatus::Passed => SampleOutcome::Success,
            TaskStatus::Failed => SampleOutcome::Failed,
            _ => SampleOutcome::Error,
        };
        let sample = RequestSample {
            sequence: seq,
            offset_ms,
            duration_ms,
            outcome,
            http_status: result.http_status,
            error_reason: result.reason.clone(),
            concurrency_at_start: handle.current_concurrency.load(Ordering::SeqCst),
        };
        handle.samples.write().await.push(sample);
    }
    handle.active_workers.fetch_sub(1, Ordering::SeqCst);
}

fn spawn_worker_task(
    client: reqwest::Client,
    plan: Arc<ExecutionPlan>,
    handle: Arc<PerformanceRunHandle>,
    sequence: Arc<AtomicU64>,
    start: tokio::time::Instant,
    count_target: Option<u64>,
) {
    tokio::spawn(spawn_worker(
        client,
        plan,
        handle,
        sequence,
        start,
        count_target,
    ));
}

#[allow(clippy::too_many_arguments)]
async fn run_load(
    client: reqwest::Client,
    plan: Arc<ExecutionPlan>,
    handle: Arc<PerformanceRunHandle>,
    sequence: Arc<AtomicU64>,
    start: tokio::time::Instant,
    concurrency: u32,
    duration_secs: u64,
    max_requests: Option<u64>,
) {
    handle
        .current_concurrency
        .store(concurrency, Ordering::SeqCst);
    for _ in 0..concurrency {
        spawn_worker_task(
            client.clone(),
            plan.clone(),
            handle.clone(),
            sequence.clone(),
            start,
            max_requests,
        );
    }
    let mut stop_new_rx = handle.stop_new_tx.subscribe();
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(duration_secs)) => { let _ = handle.stop_new_tx.send(true); }
        _ = stop_new_rx.changed() => {}
    }
}

async fn run_volume(
    client: reqwest::Client,
    plan: Arc<ExecutionPlan>,
    handle: Arc<PerformanceRunHandle>,
    sequence: Arc<AtomicU64>,
    start: tokio::time::Instant,
    concurrency: u32,
    total_requests: u64,
) {
    handle
        .current_concurrency
        .store(concurrency, Ordering::SeqCst);
    for _ in 0..concurrency {
        spawn_worker_task(
            client.clone(),
            plan.clone(),
            handle.clone(),
            sequence.clone(),
            start,
            Some(total_requests),
        );
    }
    // Bounded intrinsically by the shared ticket counter reaching
    // `total_requests` (every worker stops itself and signals
    // `stop_new_tx`) — no separate deadline supervisor needed here; the
    // global safety-net deadline in `run_workload` still applies as the
    // outer hard stop.
    let mut stop_new_rx = handle.stop_new_tx.subscribe();
    let _ = stop_new_rx.changed().await;
}

#[allow(clippy::too_many_arguments)]
async fn run_stress(
    client: reqwest::Client,
    plan: Arc<ExecutionPlan>,
    handle: Arc<PerformanceRunHandle>,
    sequence: Arc<AtomicU64>,
    start: tokio::time::Instant,
    start_concurrency: u32,
    max_concurrency: u32,
    step_size: u32,
    step_interval_secs: u64,
    max_duration_secs: u64,
) {
    for _ in 0..start_concurrency {
        spawn_worker_task(
            client.clone(),
            plan.clone(),
            handle.clone(),
            sequence.clone(),
            start,
            None,
        );
    }
    handle
        .current_concurrency
        .store(start_concurrency, Ordering::SeqCst);

    let deadline = start + Duration::from_secs(max_duration_secs);
    let mut current = start_concurrency;
    let mut stop_new_rx = handle.stop_new_tx.subscribe();
    while current < max_concurrency {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(step_interval_secs)) => {}
            _ = stop_new_rx.changed() => break,
        }
        if *stop_new_rx.borrow() || tokio::time::Instant::now() >= deadline {
            break;
        }
        let add = step_size.min(max_concurrency - current);
        for _ in 0..add {
            spawn_worker_task(
                client.clone(),
                plan.clone(),
                handle.clone(),
                sequence.clone(),
                start,
                None,
            );
        }
        current += add;
        handle.current_concurrency.store(current, Ordering::SeqCst);
    }

    tokio::select! {
        _ = tokio::time::sleep_until(deadline) => { let _ = handle.stop_new_tx.send(true); }
        _ = stop_new_rx.changed() => {}
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_spike(
    client: reqwest::Client,
    plan: Arc<ExecutionPlan>,
    handle: Arc<PerformanceRunHandle>,
    sequence: Arc<AtomicU64>,
    start: tokio::time::Instant,
    baseline_concurrency: u32,
    spike_concurrency: u32,
    baseline_secs: u64,
    spike_secs: u64,
) {
    for _ in 0..baseline_concurrency {
        spawn_worker_task(
            client.clone(),
            plan.clone(),
            handle.clone(),
            sequence.clone(),
            start,
            None,
        );
    }
    handle
        .current_concurrency
        .store(baseline_concurrency, Ordering::SeqCst);

    let mut stop_new_rx = handle.stop_new_tx.subscribe();
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(baseline_secs)) => {}
        _ = stop_new_rx.changed() => { return; }
    }
    if *stop_new_rx.borrow() {
        return;
    }

    // The real, immediate jump — every extra worker for the spike is
    // spawned in this single burst, not incrementally like Stress's steps.
    let jump = spike_concurrency - baseline_concurrency;
    for _ in 0..jump {
        spawn_worker_task(
            client.clone(),
            plan.clone(),
            handle.clone(),
            sequence.clone(),
            start,
            None,
        );
    }
    handle
        .current_concurrency
        .store(spike_concurrency, Ordering::SeqCst);

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(spike_secs)) => { let _ = handle.stop_new_tx.send(true); }
        _ = stop_new_rx.changed() => {}
    }
}

/// Drives one real workload from start to finish. Always installs a global
/// safety-net hard stop at `limits.max_duration_secs` first, regardless of
/// schedule type or its own configured durations — protects against a
/// count-bounded schedule (Volume) against an extremely slow/hung target
/// running far longer than intended. Returns once every worker has actually
/// observed the stop signal and exited (polls `active_workers`, not a fixed
/// sleep), so the final sample set is complete and stable before the caller
/// reads it.
pub async fn run_workload(
    client: reqwest::Client,
    plan: Arc<ExecutionPlan>,
    schedule: WorkloadSchedule,
    limits: SafetyLimits,
    handle: Arc<PerformanceRunHandle>,
) {
    let start = tokio::time::Instant::now();
    let sequence = Arc::new(AtomicU64::new(0));

    {
        let handle2 = handle.clone();
        let mut stop_new_rx = handle.stop_new_tx.subscribe();
        let hard_deadline = Duration::from_secs(limits.max_duration_secs);
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(hard_deadline) => { let _ = handle2.stop_new_tx.send(true); }
                _ = stop_new_rx.changed() => {}
            }
        });
    }

    match schedule {
        WorkloadSchedule::Load {
            concurrency,
            duration_secs,
            max_requests,
        } => {
            run_load(
                client,
                plan,
                handle.clone(),
                sequence,
                start,
                concurrency,
                duration_secs,
                max_requests,
            )
            .await
        }
        WorkloadSchedule::Volume {
            concurrency,
            total_requests,
        } => {
            run_volume(
                client,
                plan,
                handle.clone(),
                sequence,
                start,
                concurrency,
                total_requests,
            )
            .await
        }
        WorkloadSchedule::Stress {
            start_concurrency,
            max_concurrency,
            step_size,
            step_interval_secs,
            max_duration_secs,
        } => {
            run_stress(
                client,
                plan,
                handle.clone(),
                sequence,
                start,
                start_concurrency,
                max_concurrency,
                step_size,
                step_interval_secs,
                max_duration_secs.min(limits.max_duration_secs),
            )
            .await
        }
        WorkloadSchedule::Spike {
            baseline_concurrency,
            spike_concurrency,
            baseline_secs,
            spike_secs,
        } => {
            run_spike(
                client,
                plan,
                handle.clone(),
                sequence,
                start,
                baseline_concurrency,
                spike_concurrency,
                baseline_secs,
                spike_secs,
            )
            .await
        }
    }

    while handle.active_workers.load(Ordering::SeqCst) > 0 {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let final_state = if handle.user_cancelled.load(Ordering::SeqCst) {
        PerfRunState::Cancelled
    } else {
        PerfRunState::Completed
    };
    *handle.finished_elapsed_ms.write().await = Some(start.elapsed().as_millis() as u64);
    *handle.state.write().await = final_state;
}

/// A genuine, unexpected failure of the *task itself* driving one
/// performance run — not a per-request `SampleOutcome::Error` (that's real
/// performance data about the target, handled entirely within
/// `run_workload`), but the workload task never returning normally at all
/// (a real Tokio panic, or, in principle, the task being aborted). Carries
/// only an already-sanitized, safe message — never the raw panic payload,
/// which could in principle be an arbitrary value, not necessarily a
/// string, and must never reach a log/API/UI verbatim.
#[derive(Debug, Clone)]
pub struct SupervisionFailure {
    pub safe_message: String,
}

impl SupervisionFailure {
    fn from_join_error(e: tokio::task::JoinError) -> Self {
        if e.is_panic() {
            let payload = e.into_panic();
            let detail = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned());
            let safe_message = match detail {
                Some(s) => format!("performance execution terminated unexpectedly: {s}"),
                None => {
                    "performance execution terminated unexpectedly (internal panic)".to_string()
                }
            };
            Self { safe_message }
        } else {
            // The only other real `JoinError` cause — the task was aborted
            // (`JoinHandle::abort`) before it could finish. Nothing in this
            // module's own real code path calls `.abort()`, but every
            // `JoinError` is honestly handled, not assumed-panic.
            Self {
                safe_message: "performance execution task was terminated before it could finish"
                    .to_string(),
            }
        }
    }
}

/// Real Tokio task supervision for one spawned performance-test workload —
/// the fix for a genuine gap found in this phase's own live verification:
/// the workload task was previously spawned via a bare `tokio::spawn(...)`
/// whose `JoinHandle` was discarded unobserved. If that task panicked, the
/// spawned future's own `.await` chain (which included the finalization
/// call, sequenced *after* `run_workload`) never ran at all — the task
/// simply vanished, leaving the persisted row stuck at `RUNNING` until the
/// *next process restart's* recovery sweep eventually noticed and marked it
/// `INTERRUPTED` (a real, but delayed and indirect, detection path).
///
/// This function is the real fix: it owns the `tokio::spawn` and its real
/// `JoinHandle`, and distinguishes the two genuinely different outcomes:
/// `Ok(())` — the workload returned normally, exactly as before (its own
/// internal state already reflects `Completed`/`Cancelled`); `Err(failure)`
/// — the task itself failed, detected immediately (not on the next
/// restart), with a real, safe, actionable reason.
///
/// Generic over the workload future so this same, real supervision
/// mechanism can be tested directly against a deliberately, deterministically
/// panicking future — proving the supervisor's own real behavior — without
/// ever needing to make the real `run_workload` (Load/Stress/Spike/Volume)
/// logic panic, which this fix deliberately leaves untouched.
pub async fn supervise_workload_task<F>(workload: F) -> Result<(), SupervisionFailure>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    match tokio::spawn(workload).await {
        Ok(()) => Ok(()),
        Err(join_err) => Err(SupervisionFailure::from_join_error(join_err)),
    }
}

/// Startup recovery (mirrors `engine::recover_incomplete_runs`) — any
/// `performance_runs` row still `RUNNING` when this process starts was
/// genuinely orphaned by a prior unclean shutdown: the real, in-memory
/// `PerformanceRunHandle` driving it lived only in that prior process's
/// `AppState::performance_tests` map and is gone. Marked `INTERRUPTED`
/// honestly — never guessed at as `COMPLETED`, never left `RUNNING` forever.
pub async fn recover_incomplete_performance_runs(
    store: &crate::store::Store,
) -> anyhow::Result<usize> {
    let ids = store.list_incomplete_performance_runs().await?;
    let reason = "orchestrator process restarted while this performance test was running; its live in-memory progress was not durable, so it is marked INTERRUPTED rather than left RUNNING forever";
    for id in &ids {
        store
            .finalize_performance_run(*id, "INTERRUPTED", None, None, None, Some(reason), None)
            .await?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn ad_hoc(method: &str, url: &str) -> AdHocApiRequest {
        AdHocApiRequest {
            method: method.to_string(),
            url: url.to_string(),
            query_params: vec![],
            headers: BTreeMap::new(),
            body: None,
            assertions: vec![],
        }
    }

    fn limits() -> SafetyLimits {
        SafetyLimits {
            max_concurrency: 50,
            max_duration_secs: 120,
            max_total_requests: 5000,
        }
    }

    // ------------------------------------------------------- percentile

    #[test]
    fn percentile_matches_the_documented_worked_example() {
        let data: Vec<u64> = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&data, 50.0), 50);
        assert_eq!(percentile(&data, 95.0), 100);
        assert_eq!(percentile(&data, 99.0), 100);
    }

    #[test]
    fn percentile_of_a_single_sample_is_that_sample() {
        assert_eq!(percentile(&[42], 50.0), 42);
        assert_eq!(percentile(&[42], 99.0), 42);
    }

    #[test]
    fn percentile_of_empty_data_is_zero_not_a_panic() {
        assert_eq!(percentile(&[], 95.0), 0);
    }

    // --------------------------------------------------------- metrics

    fn sample(ms: u64, outcome: SampleOutcome) -> RequestSample {
        RequestSample {
            sequence: 0,
            offset_ms: 0,
            duration_ms: ms,
            outcome,
            http_status: Some(200),
            error_reason: None,
            concurrency_at_start: 1,
        }
    }

    #[test]
    fn metrics_are_computed_purely_from_real_samples() {
        let samples = vec![
            sample(10, SampleOutcome::Success),
            sample(20, SampleOutcome::Success),
            sample(30, SampleOutcome::Failed),
            sample(40, SampleOutcome::Error),
        ];
        let m = compute_metrics(&samples, 2000);
        assert_eq!(m.total_requests, 4);
        assert_eq!(m.successful, 2);
        assert_eq!(m.failed, 1);
        assert_eq!(m.errored, 1);
        assert_eq!(m.min_ms, 10);
        assert_eq!(m.max_ms, 40);
        assert!((m.avg_ms - 25.0).abs() < 0.001);
        assert!((m.error_rate_percent - 50.0).abs() < 0.001);
        assert!((m.success_rate_percent - 50.0).abs() < 0.001);
        assert!(
            (m.throughput_rps - 2.0).abs() < 0.001,
            "4 requests / 2s = 2 req/s, got {}",
            m.throughput_rps
        );
    }

    #[test]
    fn metrics_of_zero_samples_are_all_zero_not_nan_or_a_panic() {
        let m = compute_metrics(&[], 0);
        assert_eq!(m.total_requests, 0);
        assert_eq!(m.error_rate_percent, 0.0);
        assert_eq!(m.min_ms, 0);
    }

    // --------------------------------------------------------- verdict

    fn base_config(schedule: WorkloadSchedule) -> PerformanceTestConfig {
        PerformanceTestConfig {
            request: ad_hoc("GET", "http://example.com"),
            expected_status: Some(200),
            schedule,
            max_p95_ms: None,
            max_error_rate_percent: None,
            min_throughput_rps: None,
        }
    }

    #[test]
    fn no_thresholds_configured_yields_completed_never_a_false_pass() {
        let cfg = base_config(WorkloadSchedule::Load {
            concurrency: 1,
            duration_secs: 1,
            max_requests: None,
        });
        let m = compute_metrics(&[sample(10, SampleOutcome::Success)], 1000);
        let (verdict, _) = evaluate_verdict(&cfg, &m, false);
        assert_eq!(verdict, PerfVerdict::Completed);
    }

    #[test]
    fn thresholds_met_yields_passed() {
        let mut cfg = base_config(WorkloadSchedule::Load {
            concurrency: 1,
            duration_secs: 1,
            max_requests: None,
        });
        cfg.max_p95_ms = Some(1000);
        cfg.max_error_rate_percent = Some(5.0);
        let m = compute_metrics(
            &[
                sample(10, SampleOutcome::Success),
                sample(20, SampleOutcome::Success),
            ],
            1000,
        );
        let (verdict, reasons) = evaluate_verdict(&cfg, &m, false);
        assert_eq!(verdict, PerfVerdict::Passed);
        assert!(!reasons.is_empty());
    }

    #[test]
    fn a_violated_threshold_yields_failed_with_the_real_measured_value_in_the_reason() {
        let mut cfg = base_config(WorkloadSchedule::Load {
            concurrency: 1,
            duration_secs: 1,
            max_requests: None,
        });
        cfg.max_p95_ms = Some(5);
        let m = compute_metrics(&[sample(100, SampleOutcome::Success)], 1000);
        let (verdict, reasons) = evaluate_verdict(&cfg, &m, false);
        assert_eq!(verdict, PerfVerdict::Failed);
        assert!(reasons[0].contains("100ms"), "reasons={reasons:?}");
    }

    /// The regression this guards: a latency threshold alone used to be able
    /// to carry a run in which *every* request failed. 200 samples, 0
    /// successes, and the verdict still came back `Passed` with the reason
    /// "all configured thresholds were met".
    #[test]
    fn a_run_where_every_request_failed_is_never_passed_on_latency_grounds_alone() {
        let mut cfg = base_config(WorkloadSchedule::Load {
            concurrency: 4,
            duration_secs: 6,
            max_requests: Some(200),
        });
        cfg.expected_status = Some(200);
        cfg.max_p95_ms = Some(2000); // the measured p95 was 13ms, so this passed
        let m = compute_metrics(
            &[
                sample(13, SampleOutcome::Failed),
                sample(13, SampleOutcome::Failed),
                sample(13, SampleOutcome::Failed),
            ],
            502,
        );
        assert_eq!(m.successful, 0);
        assert_eq!(m.total_requests, 3);
        let (verdict, reasons) = evaluate_verdict(&cfg, &m, false);
        assert_eq!(verdict, PerfVerdict::Failed);
        assert!(
            reasons.iter().any(|r| r.contains("no request succeeded")),
            "reasons={reasons:?}"
        );
        // The reason must carry the real measured numbers, not a bare flag.
        assert!(reasons[0].contains("3 of 3"), "reasons={reasons:?}");
        assert!(reasons[0].contains("200"), "reasons={reasons:?}");
    }

    /// A run that did work stays `Passed` — the new check must not turn every
    /// threshold-meeting run into a failure.
    #[test]
    fn a_run_where_requests_succeeded_is_still_passed() {
        let mut cfg = base_config(WorkloadSchedule::Load {
            concurrency: 1,
            duration_secs: 1,
            max_requests: None,
        });
        cfg.expected_status = Some(200);
        cfg.max_p95_ms = Some(1000);
        let m = compute_metrics(
            &[
                sample(10, SampleOutcome::Success),
                sample(20, SampleOutcome::Success),
                sample(30, SampleOutcome::Failed),
            ],
            1000,
        );
        let (verdict, reasons) = evaluate_verdict(&cfg, &m, false);
        assert_eq!(verdict, PerfVerdict::Passed);
        assert!(!reasons.iter().any(|r| r.contains("no request succeeded")));
    }

    /// No requests at all is not "every request failed" — nothing ran, so
    /// there is no failure to report and the thresholds stand alone.
    #[test]
    fn a_run_that_never_made_a_request_is_not_reported_as_a_total_failure() {
        let mut cfg = base_config(WorkloadSchedule::Load {
            concurrency: 1,
            duration_secs: 1,
            max_requests: None,
        });
        cfg.max_p95_ms = Some(1000);
        let m = compute_metrics(&[], 1000);
        assert_eq!(m.total_requests, 0);
        let (verdict, reasons) = evaluate_verdict(&cfg, &m, false);
        assert_eq!(verdict, PerfVerdict::Passed);
        assert!(!reasons.iter().any(|r| r.contains("no request succeeded")));
    }

    #[test]
    fn cancellation_always_wins_over_threshold_evaluation() {
        let mut cfg = base_config(WorkloadSchedule::Load {
            concurrency: 1,
            duration_secs: 1,
            max_requests: None,
        });
        cfg.max_p95_ms = Some(1_000_000); // would trivially pass if evaluated
        let m = compute_metrics(&[sample(1, SampleOutcome::Success)], 1000);
        let (verdict, _) = evaluate_verdict(&cfg, &m, true);
        assert_eq!(verdict, PerfVerdict::Cancelled);
    }

    // --------------------------------------------------- schedule limits

    #[test]
    fn a_concurrency_above_the_server_limit_is_rejected_regardless_of_frontend_input() {
        let err = validate_schedule(
            &WorkloadSchedule::Load {
                concurrency: 999,
                duration_secs: 10,
                max_requests: None,
            },
            &limits(),
        )
        .unwrap_err();
        assert_eq!(err.0, "CONCURRENCY_LIMIT_EXCEEDED");
    }

    #[test]
    fn a_duration_above_the_server_limit_is_rejected() {
        let err = validate_schedule(
            &WorkloadSchedule::Load {
                concurrency: 1,
                duration_secs: 99999,
                max_requests: None,
            },
            &limits(),
        )
        .unwrap_err();
        assert_eq!(err.0, "DURATION_LIMIT_EXCEEDED");
    }

    #[test]
    fn a_total_requests_above_the_server_limit_is_rejected_for_volume() {
        let err = validate_schedule(
            &WorkloadSchedule::Volume {
                concurrency: 1,
                total_requests: 999_999,
            },
            &limits(),
        )
        .unwrap_err();
        assert_eq!(err.0, "TOTAL_REQUESTS_LIMIT_EXCEEDED");
    }

    #[test]
    fn stress_max_concurrency_below_start_concurrency_is_rejected() {
        let err = validate_schedule(
            &WorkloadSchedule::Stress {
                start_concurrency: 10,
                max_concurrency: 5,
                step_size: 1,
                step_interval_secs: 1,
                max_duration_secs: 10,
            },
            &limits(),
        )
        .unwrap_err();
        assert_eq!(err.0, "INVALID_SCHEDULE");
    }

    #[test]
    fn spike_concurrency_not_greater_than_baseline_is_rejected() {
        let err = validate_schedule(
            &WorkloadSchedule::Spike {
                baseline_concurrency: 10,
                spike_concurrency: 10,
                baseline_secs: 1,
                spike_secs: 1,
            },
            &limits(),
        )
        .unwrap_err();
        assert_eq!(err.0, "INVALID_SCHEDULE");
    }

    #[test]
    fn a_valid_schedule_of_each_type_is_accepted() {
        assert!(validate_schedule(
            &WorkloadSchedule::Load {
                concurrency: 5,
                duration_secs: 5,
                max_requests: None
            },
            &limits()
        )
        .is_ok());
        assert!(validate_schedule(
            &WorkloadSchedule::Stress {
                start_concurrency: 2,
                max_concurrency: 10,
                step_size: 2,
                step_interval_secs: 2,
                max_duration_secs: 20
            },
            &limits()
        )
        .is_ok());
        assert!(validate_schedule(
            &WorkloadSchedule::Spike {
                baseline_concurrency: 2,
                spike_concurrency: 10,
                baseline_secs: 2,
                spike_secs: 2
            },
            &limits()
        )
        .is_ok());
        assert!(validate_schedule(
            &WorkloadSchedule::Volume {
                concurrency: 5,
                total_requests: 50
            },
            &limits()
        )
        .is_ok());
    }

    // -------------------------------------------------- request plan build

    #[test]
    fn expected_status_becomes_a_real_assert_status_action() {
        let plan = build_request_plan("t1", &ad_hoc("GET", "http://example.com/health"), Some(200))
            .expect("must build");
        assert_eq!(plan.actions.len(), 2);
        assert!(matches!(
            &plan.actions[1],
            Action::AssertStatus { status: 200 }
        ));
    }

    #[test]
    fn a_request_carrying_its_own_assertions_is_rejected() {
        let mut req = ad_hoc("GET", "http://example.com");
        req.assertions = vec![Action::AssertStatus { status: 200 }];
        let err = build_request_plan("t1", &req, None).unwrap_err();
        assert_eq!(err.0, "INVALID_REQUEST_CONFIGURATION");
    }

    // ------------------------------------------------ real workload engine
    //
    // A real, self-contained local HTTP target this test itself spins up
    // (mirrors the pattern already used in `executor.rs`'s Phase 4 tests) —
    // genuine concurrent request handling under real Tokio tasks, not
    // mocked and not dependent on any other process being up.

    async fn spawn_test_target() -> String {
        let app = axum::Router::new().route("/ok", axum::routing::get(|| async { "ok" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}/ok")
    }

    fn test_client(timeout: Duration) -> reqwest::Client {
        reqwest::Client::builder().timeout(timeout).build().unwrap()
    }

    #[tokio::test]
    async fn a_load_test_produces_real_samples_at_the_configured_concurrency() {
        let url = spawn_test_target().await;
        let plan = Arc::new(
            build_request_plan("t1", &ad_hoc("GET", &url), Some(200)).expect("must build"),
        );
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Load {
            concurrency: 4,
            duration_secs: 1,
            max_requests: None,
        };
        run_workload(
            test_client(Duration::from_secs(5)),
            plan,
            schedule,
            limits(),
            handle.clone(),
        )
        .await;

        let samples = handle.samples.read().await;
        assert!(
            !samples.is_empty(),
            "a 1s load test at concurrency 4 against a real local server must produce real samples"
        );
        assert!(
            samples.iter().all(|s| s.outcome == SampleOutcome::Success),
            "every request to /ok expecting 200 must succeed: {samples:?}"
        );
        assert_eq!(*handle.state.read().await, PerfRunState::Completed);
    }

    #[tokio::test]
    async fn finished_elapsed_ms_is_frozen_the_instant_the_workload_completes_not_left_growing_with_wall_clock_time(
    ) {
        // Regression test for a real bug caught during this phase's own
        // live verification: a status re-read of an already-COMPLETED run
        // was recomputing metrics against `now - started_at`, which keeps
        // growing on every later poll even though nothing is still
        // happening — silently shrinking the reported `throughput_rps` on
        // each repeat read. `run_workload` must freeze a real elapsed value
        // the moment it actually finishes, and that value must never change
        // afterward no matter how much real wall-clock time passes.
        let url = spawn_test_target().await;
        let plan = Arc::new(
            build_request_plan("t1", &ad_hoc("GET", &url), Some(200)).expect("must build"),
        );
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Load {
            concurrency: 2,
            duration_secs: 1,
            max_requests: None,
        };
        run_workload(
            test_client(Duration::from_secs(5)),
            plan,
            schedule,
            limits(),
            handle.clone(),
        )
        .await;

        assert_eq!(*handle.state.read().await, PerfRunState::Completed);
        let frozen_first_read = handle
            .finished_elapsed_ms
            .read()
            .await
            .expect("finished_elapsed_ms must be set once the workload has actually completed");

        tokio::time::sleep(Duration::from_millis(300)).await;

        let frozen_second_read = handle
            .finished_elapsed_ms
            .read()
            .await
            .expect("must still be set");
        assert_eq!(frozen_first_read, frozen_second_read, "elapsed time must stay frozen after completion, not keep growing with real wall-clock time");
    }

    #[tokio::test]
    async fn a_volume_test_stops_at_exactly_the_configured_total_and_never_overshoots() {
        let url = spawn_test_target().await;
        let plan =
            Arc::new(build_request_plan("t1", &ad_hoc("GET", &url), None).expect("must build"));
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Volume {
            concurrency: 5,
            total_requests: 37,
        };
        run_workload(
            test_client(Duration::from_secs(5)),
            plan,
            schedule,
            limits(),
            handle.clone(),
        )
        .await;

        let samples = handle.samples.read().await;
        assert_eq!(
            samples.len(),
            37,
            "Volume must fire exactly the configured total, no more, no fewer"
        );
    }

    #[tokio::test]
    async fn a_stress_test_genuinely_ramps_concurrency_over_real_time() {
        let url = spawn_test_target().await;
        let plan =
            Arc::new(build_request_plan("t1", &ad_hoc("GET", &url), None).expect("must build"));
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Stress {
            start_concurrency: 2,
            max_concurrency: 6,
            step_size: 2,
            step_interval_secs: 1,
            max_duration_secs: 3,
        };
        run_workload(
            test_client(Duration::from_secs(5)),
            plan,
            schedule,
            limits(),
            handle.clone(),
        )
        .await;

        let samples = handle.samples.read().await;
        let max_observed_concurrency = samples
            .iter()
            .map(|s| s.concurrency_at_start)
            .max()
            .unwrap_or(0);
        assert!(max_observed_concurrency > 2, "a real ramp must reach beyond the starting concurrency of 2, observed max was {max_observed_concurrency}");
        let min_observed_concurrency = samples
            .iter()
            .map(|s| s.concurrency_at_start)
            .min()
            .unwrap_or(0);
        assert_eq!(
            min_observed_concurrency, 2,
            "the earliest real samples must reflect the real starting concurrency"
        );
    }

    #[tokio::test]
    async fn a_spike_test_jumps_concurrency_immediately_rather_than_ramping_gradually() {
        let url = spawn_test_target().await;
        let plan =
            Arc::new(build_request_plan("t1", &ad_hoc("GET", &url), None).expect("must build"));
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Spike {
            baseline_concurrency: 2,
            spike_concurrency: 8,
            baseline_secs: 1,
            spike_secs: 1,
        };
        run_workload(
            test_client(Duration::from_secs(5)),
            plan,
            schedule,
            limits(),
            handle.clone(),
        )
        .await;

        let samples = handle.samples.read().await;
        let observed: std::collections::BTreeSet<u32> =
            samples.iter().map(|s| s.concurrency_at_start).collect();
        // A real jump only ever exposes the baseline value and the full
        // spike value to samples — never an intermediate value a gradual
        // Stress-style ramp would produce (e.g. 4 or 6).
        assert!(
            observed.contains(&2),
            "baseline concurrency must appear: {observed:?}"
        );
        assert!(
            observed.contains(&8),
            "the full spike concurrency must appear: {observed:?}"
        );
        assert!(
            !observed.contains(&4) && !observed.contains(&6),
            "a spike must jump directly, not ramp through intermediate values: {observed:?}"
        );
    }

    #[tokio::test]
    async fn cancellation_actually_interrupts_a_running_workload_rather_than_waiting_it_out() {
        let url = spawn_test_target().await;
        let plan =
            Arc::new(build_request_plan("t1", &ad_hoc("GET", &url), None).expect("must build"));
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        // A schedule that would otherwise run for a full 30s if cancellation
        // did not really interrupt it.
        let schedule = WorkloadSchedule::Load {
            concurrency: 2,
            duration_secs: 30,
            max_requests: None,
        };

        let run_handle = handle.clone();
        let plan2 = plan.clone();
        let workload = tokio::spawn(async move {
            run_workload(
                test_client(Duration::from_secs(5)),
                plan2,
                schedule,
                limits(),
                run_handle,
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;
        let cancelled = handle.request_cancel().await;
        assert!(
            cancelled,
            "cancel must succeed on a genuinely running workload"
        );

        let outcome = tokio::time::timeout(Duration::from_secs(3), workload).await;
        assert!(
            outcome.is_ok(),
            "a cancelled 30s workload must actually stop in well under 3s, not run to completion"
        );
        assert_eq!(*handle.state.read().await, PerfRunState::Cancelled);
    }

    #[tokio::test]
    async fn a_second_cancel_after_completion_is_a_harmless_no_op_not_an_error() {
        let url = spawn_test_target().await;
        let plan =
            Arc::new(build_request_plan("t1", &ad_hoc("GET", &url), None).expect("must build"));
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Volume {
            concurrency: 1,
            total_requests: 2,
        };
        run_workload(
            test_client(Duration::from_secs(5)),
            plan,
            schedule,
            limits(),
            handle.clone(),
        )
        .await;

        assert_eq!(*handle.state.read().await, PerfRunState::Completed);
        let cancelled = handle.request_cancel().await;
        assert!(
            !cancelled,
            "cancelling an already-completed run must be a real, honest no-op"
        );
        assert_eq!(
            *handle.state.read().await,
            PerfRunState::Completed,
            "state must not be corrupted into CANCELLED by a stale cancel call"
        );
    }

    #[tokio::test]
    async fn a_genuine_transport_error_is_recorded_as_error_not_success_or_failed() {
        // A real, ephemeral port nothing listens on — every request is a
        // genuine connection failure, not a mocked one.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let url = format!("http://127.0.0.1:{port}/nothing-here");
        let plan =
            Arc::new(build_request_plan("t1", &ad_hoc("GET", &url), None).expect("must build"));
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Volume {
            concurrency: 2,
            total_requests: 3,
        };
        run_workload(
            test_client(Duration::from_secs(2)),
            plan,
            schedule,
            limits(),
            handle.clone(),
        )
        .await;

        let samples = handle.samples.read().await;
        assert_eq!(samples.len(), 3);
        assert!(
            samples.iter().all(|s| s.outcome == SampleOutcome::Error),
            "every sample against a dead port must be a real Error outcome: {samples:?}"
        );
        // The exact classification (CONNECTION_FAILED vs REQUEST_TIMEOUT) for
        // a just-closed loopback port under concurrent connection attempts is
        // a genuine OS-level timing detail (observed to differ between a
        // single sequential attempt and several concurrent ones on Windows);
        // both are real, honest transport-failure classifications from
        // `classify_reqwest_error` — what this test actually verifies is that
        // a genuine transport failure is recorded as `Error`, never
        // `Success`/`Failed`.
        assert!(
            samples
                .iter()
                .all(|s| s.error_reason.as_deref().is_some_and(|r| r
                    .starts_with("CONNECTION_FAILED")
                    || r.starts_with("REQUEST_TIMEOUT"))),
            "{samples:?}"
        );
    }

    // ---------------------------------------------- run-level ERROR supervision

    #[tokio::test]
    async fn a_normal_workload_task_supervised_returns_ok() {
        let outcome = supervise_workload_task(
            async { /* returns normally, like a real completed/cancelled run_workload */ },
        )
        .await;
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn a_genuine_deterministic_panic_in_the_supervised_task_is_caught_not_left_to_crash_the_process(
    ) {
        // A real Tokio panic inside a real spawned task — not a mock, not a
        // simulated failure flag. `supervise_workload_task` must catch this
        // via the real `JoinHandle`, not let it silently vanish.
        let outcome = supervise_workload_task(async {
            panic!("deterministic test failure: internal workload orchestration fault");
        })
        .await;
        let failure = outcome.expect_err("a genuine panic must be reported as a real supervision failure, not silently swallowed as Ok");
        assert!(
            failure.safe_message.contains("deterministic test failure: internal workload orchestration fault"),
            "the real panic message must be safely surfaced, not replaced with a generic string when it is available: {}",
            failure.safe_message
        );
        assert!(failure
            .safe_message
            .starts_with("performance execution terminated unexpectedly"));
    }

    #[tokio::test]
    async fn a_real_normal_workload_wrapped_in_supervision_still_completes_normally() {
        // Regression: wrapping the real, unmodified `run_workload` in
        // `supervise_workload_task` must not change its normal-completion
        // behavior at all — real requests, real samples, real `Completed`
        // state, `Ok(())` from the supervisor.
        let url = spawn_test_target().await;
        let plan = Arc::new(
            build_request_plan("t1", &ad_hoc("GET", &url), Some(200)).expect("must build"),
        );
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Volume {
            concurrency: 2,
            total_requests: 5,
        };
        let client = test_client(Duration::from_secs(5));
        let handle2 = handle.clone();
        let outcome = supervise_workload_task(async move {
            run_workload(client, plan, schedule, limits(), handle2).await;
        })
        .await;
        assert!(
            outcome.is_ok(),
            "a real, normal workload must supervise to Ok, not a failure"
        );
        assert_eq!(*handle.state.read().await, PerfRunState::Completed);
        assert_eq!(handle.samples.read().await.len(), 5);
    }

    #[tokio::test]
    async fn a_real_cancelled_workload_wrapped_in_supervision_still_reports_cancelled_not_error() {
        // Regression + terminal-state precedence: a real cancellation must
        // still resolve through the supervisor's `Ok(())` branch (the
        // workload returns normally, having internally recorded
        // `Cancelled`) — cancellation is never mistaken for a task-level
        // supervision failure.
        let url = spawn_test_target().await;
        let plan =
            Arc::new(build_request_plan("t1", &ad_hoc("GET", &url), None).expect("must build"));
        let handle = PerformanceRunHandle::new(Uuid::new_v4());
        let schedule = WorkloadSchedule::Load {
            concurrency: 2,
            duration_secs: 30,
            max_requests: None,
        };
        let client = test_client(Duration::from_secs(5));
        let handle2 = handle.clone();

        let supervised = tokio::spawn(async move {
            supervise_workload_task(async move {
                run_workload(client, plan, schedule, limits(), handle2).await
            })
            .await
        });

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(handle.request_cancel().await);

        let outcome = tokio::time::timeout(Duration::from_secs(3), supervised)
            .await
            .expect("must finish well under 3s")
            .expect("join");
        assert!(
            outcome.is_ok(),
            "a real cancellation must resolve as a normal supervised completion, not a failure"
        );
        assert_eq!(*handle.state.read().await, PerfRunState::Cancelled);
    }

    #[tokio::test]
    async fn a_panic_with_a_non_string_payload_still_produces_a_safe_generic_message() {
        // `std::panic::panic_any` can carry an arbitrary, non-string value —
        // this must never crash the supervisor itself trying to format it,
        // and must never leak an unformattable/arbitrary payload into the
        // safe message.
        let outcome = supervise_workload_task(async {
            std::panic::panic_any(42u32);
        })
        .await;
        let failure = outcome.expect_err("must be caught");
        assert_eq!(
            failure.safe_message,
            "performance execution terminated unexpectedly (internal panic)"
        );
    }
}
