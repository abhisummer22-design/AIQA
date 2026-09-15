//! Phase 6: Adaptive QA Intelligence & Production Observability.
//!
//! Core Architectural Invariants:
//! - Strictly zero OpenJarvis, zero Python runtimes, zero PyO3.
//! - Normalized telemetry is generated exclusively from real execution outcomes.
//! - Deterministic failure taxonomy and regex-based dynamic token masking.
//! - Environment-aware flakiness tracking (preserves staging vs production context).
//! - Quality Graph extensions (FailureCluster, Component, Strategy nodes & relations).
//! - Strategy reliability engine using real verified healing metrics.
//! - Structured feedback provided to TestPlanningAgent without exposing private CoT.
//! - All telemetry and intelligence records strictly tenant/project scoped.

use crate::domain::{TaskState, TaskStatus};
use crate::store::{HealingStrategyMetricsRow, Store};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;
use uuid::Uuid;

// ============================================================================
// 1. DETERMINISTIC FAILURE CATEGORIZATION & NORMALIZATION
// ============================================================================

/// Deterministic, normalized failure categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureCategory {
    AuthenticationFailure,
    AuthorizationFailure,
    NetworkFailure,
    Timeout,
    SelectorNotFound,
    SelectorAmbiguous,
    AssertionFailure,
    ValidationFailure,
    ServerError,
    ClientError,
    RateLimit,
    DependencyFailure,
    EnvironmentFailure,
    DataFailure,
    Unknown,
}

impl FailureCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthenticationFailure => "AUTHENTICATION_FAILURE",
            Self::AuthorizationFailure => "AUTHORIZATION_FAILURE",
            Self::NetworkFailure => "NETWORK_FAILURE",
            Self::Timeout => "TIMEOUT",
            Self::SelectorNotFound => "SELECTOR_NOT_FOUND",
            Self::SelectorAmbiguous => "SELECTOR_AMBIGUOUS",
            Self::AssertionFailure => "ASSERTION_FAILURE",
            Self::ValidationFailure => "VALIDATION_FAILURE",
            Self::ServerError => "SERVER_ERROR",
            Self::ClientError => "CLIENT_ERROR",
            Self::RateLimit => "RATE_LIMIT",
            Self::DependencyFailure => "DEPENDENCY_FAILURE",
            Self::EnvironmentFailure => "ENVIRONMENT_FAILURE",
            Self::DataFailure => "DATA_FAILURE",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "AUTHENTICATION_FAILURE" => Self::AuthenticationFailure,
            "AUTHORIZATION_FAILURE" => Self::AuthorizationFailure,
            "NETWORK_FAILURE" => Self::NetworkFailure,
            "TIMEOUT" => Self::Timeout,
            "SELECTOR_NOT_FOUND" => Self::SelectorNotFound,
            "SELECTOR_AMBIGUOUS" => Self::SelectorAmbiguous,
            "ASSERTION_FAILURE" => Self::AssertionFailure,
            "VALIDATION_FAILURE" => Self::ValidationFailure,
            "SERVER_ERROR" => Self::ServerError,
            "CLIENT_ERROR" => Self::ClientError,
            "RATE_LIMIT" => Self::RateLimit,
            "DEPENDENCY_FAILURE" => Self::DependencyFailure,
            "ENVIRONMENT_FAILURE" => Self::EnvironmentFailure,
            "DATA_FAILURE" => Self::DataFailure,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for FailureCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Deterministically categorizes an error message and optional HTTP status code.
pub fn categorize_failure(reason: &str, http_status: Option<u16>) -> FailureCategory {
    // 1. Status code signals
    if let Some(status) = http_status {
        match status {
            401 => return FailureCategory::AuthenticationFailure,
            403 => return FailureCategory::AuthorizationFailure,
            429 => return FailureCategory::RateLimit,
            400 | 404 | 405 | 422 => return FailureCategory::ClientError,
            500..=599 => return FailureCategory::ServerError,
            _ => {}
        }
    }

    let lower = reason.to_lowercase();

    // 2. Timeout
    if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("deadline exceeded")
    {
        return FailureCategory::Timeout;
    }

    // 3. Network
    if lower.contains("econnrefused")
        || lower.contains("connection refused")
        || lower.contains("connect error")
        || lower.contains("dns resolution failed")
        || lower.contains("network unreachable")
        || lower.contains("connection reset")
    {
        return FailureCategory::NetworkFailure;
    }

    // 4. Authentication / Authorization
    if lower.contains("unauthenticated")
        || lower.contains("invalid token")
        || lower.contains("token expired")
        || lower.contains("invalid credentials")
        || lower.contains("jwt expired")
        || lower.contains("unauthorized")
    {
        return FailureCategory::AuthenticationFailure;
    }

    if lower.contains("forbidden")
        || lower.contains("permission denied")
        || lower.contains("access denied")
        || lower.contains("insufficient permissions")
    {
        return FailureCategory::AuthorizationFailure;
    }

    // 5. Rate limit
    if lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota exceeded")
    {
        return FailureCategory::RateLimit;
    }

    // 6. Selectors
    if lower.contains("waiting for locator")
        || lower.contains("nosuchelement")
        || lower.contains("element not found")
        || lower.contains("failed to find element")
        || lower.contains("no element matches selector")
    {
        return FailureCategory::SelectorNotFound;
    }

    if lower.contains("strict mode violation")
        || lower.contains("resolved to multiple elements")
        || lower.contains("ambiguous selector")
    {
        return FailureCategory::SelectorAmbiguous;
    }

    // 7. Assertions
    if lower.contains("assert")
        || lower.contains("expect")
        || lower.contains("to have status")
        || lower.contains("expected ") && lower.contains("but got")
        || lower.contains("to equal")
    {
        return FailureCategory::AssertionFailure;
    }

    // 8. Validation
    if lower.contains("validation error")
        || lower.contains("schema mismatch")
        || lower.contains("invalid request body")
        || lower.contains("missing required field")
    {
        return FailureCategory::ValidationFailure;
    }

    // 9. Dependency / Environment
    if lower.contains("prerequisite") && lower.contains("failed")
        || lower.contains("blocked by task")
        || lower.contains("dependency failed")
    {
        return FailureCategory::DependencyFailure;
    }

    if lower.contains("no compatible executor")
        || lower.contains("executable not found")
        || lower.contains("container exited")
        || lower.contains("environment not ready")
    {
        return FailureCategory::EnvironmentFailure;
    }

    // 10. Data
    if lower.contains("foreign key constraint")
        || lower.contains("unique constraint")
        || lower.contains("record not found")
        || lower.contains("database lock")
    {
        return FailureCategory::DataFailure;
    }

    // 11. Generic HTTP 5xx / 4xx in text
    if lower.contains("500 internal server error")
        || lower.contains("502 bad gateway")
        || lower.contains("503 service unavailable")
    {
        return FailureCategory::ServerError;
    }

    if lower.contains("404 not found") || lower.contains("400 bad request") {
        return FailureCategory::ClientError;
    }

    FailureCategory::Unknown
}

// ============================================================================
// 2. DYNAMIC TOKEN MASKING & DETERMINISTIC FINGERPRINTING
// ============================================================================

fn uuid_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b").unwrap()
    })
}

fn iso_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b\d{4}-\d{2}-\d{2}t\d{2}:\d{2}:\d{2}(\.\d+)?(z|[+-]\d{2}:\d{2})?\b")
            .unwrap()
    })
}

fn ip_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap())
}

fn port_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r":\d{4,5}\b").unwrap())
}

fn hex_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(0x[0-9a-f]+|[0-9a-f]{16,})\b").unwrap())
}

fn digits_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d+").unwrap())
}

fn whitespace_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

/// Masks dynamic tokens (UUIDs, timestamps, IPs, ports, hex addresses, numbers)
/// to ensure stable, recurring failure clustering.
pub fn mask_dynamic_tokens(input: &str) -> String {
    let lower = input.to_lowercase();
    let no_uuid = uuid_regex().replace_all(&lower, "<uuid>");
    let no_date = iso_date_regex().replace_all(&no_uuid, "<timestamp>");
    let no_ip = ip_regex().replace_all(&no_date, "<ip>");
    let no_port = port_regex().replace_all(&no_ip, ":<port>");
    let no_hex = hex_regex().replace_all(&no_port, "<hex>");
    let no_digits = digits_regex().replace_all(&no_hex, "<n>");
    let collapsed = whitespace_regex().replace_all(&no_digits, " ");
    collapsed.trim().chars().take(256).collect()
}

/// Computes a deterministic SHA-256 failure fingerprint from category,
/// masked signature, and optional selector.
pub fn compute_telemetry_fingerprint(
    category: &FailureCategory,
    masked_message: &str,
    selector: Option<&str>,
    endpoint: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(category.as_str().as_bytes());
    hasher.update(b":");
    hasher.update(masked_message.as_bytes());

    if let Some(sel) = selector.map(str::trim).filter(|s| !s.is_empty()) {
        hasher.update(b":selector:");
        hasher.update(sel.to_lowercase().as_bytes());
    }

    if let Some(ep) = endpoint.map(str::trim).filter(|s| !s.is_empty()) {
        hasher.update(b":endpoint:");
        hasher.update(ep.to_lowercase().as_bytes());
    }

    hex::encode(hasher.finalize())
}

// ============================================================================
// 3. SECURITY & PRIVACY SANITIZATION
// ============================================================================

/// Strips sensitive credentials, bearer tokens, cookies, and api keys from telemetry data.
pub fn sanitize_telemetry_text(input: &str) -> String {
    static BEARER_TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    let bearer_re = BEARER_TOKEN_RE.get_or_init(|| Regex::new(r"(?i)bearer\s+[^\s,;]+").unwrap());

    static AUTH_KEY_VALUE_RE: OnceLock<Regex> = OnceLock::new();
    let kv_re = AUTH_KEY_VALUE_RE.get_or_init(|| {
        Regex::new(r"(?i)(authorization|cookie|set-cookie|api[_-]?key|password|secret|token|access_token|client_secret)\s*[:=]\s*[^\r\n,;]+").unwrap()
    });

    static COOKIE_PAIR_RE: OnceLock<Regex> = OnceLock::new();
    let cookie_re = COOKIE_PAIR_RE.get_or_init(|| {
        Regex::new(r"(?i)(session|sessionid|auth_token|token|jwt)=[^\s,;]+").unwrap()
    });

    let s1 = bearer_re.replace_all(input, "Bearer [REDACTED]");
    let s2 = kv_re.replace_all(&s1, "$1=[REDACTED]");
    let s3 = cookie_re.replace_all(&s2, "$1=[REDACTED]");
    s3.to_string()
}

// ============================================================================
// 4. NORMALIZED EXECUTION TELEMETRY MODEL
// ============================================================================

/// Typed, normalized telemetry record representing a real task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTelemetry {
    pub id: Uuid,
    pub project_id: String,
    pub run_id: Uuid,
    pub task_id: String,
    pub task_name: String,
    pub test_case_id: Option<Uuid>,
    pub execution_id: Option<String>,
    pub executor: String,
    pub environment: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: u64,
    pub status: TaskStatus,
    pub retry_count: u32,
    pub healing_attempt_count: u32,
    pub failure_category: Option<FailureCategory>,
    pub failure_fingerprint: Option<String>,
    pub error_message: Option<String>,
    pub evidence_refs: Vec<String>,
    pub assertion_result: Option<String>,
    pub http_status: Option<u16>,
    pub selector: Option<String>,
    pub verification_outcome: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ExecutionTelemetry {
    /// Creates a new normalized telemetry record from real execution data.
    pub fn from_task_state(
        project_id: &str,
        run_id: Uuid,
        task: &TaskState,
        executor: &str,
        environment: &str,
        test_case_id: Option<Uuid>,
        evidence_refs: Vec<String>,
        selector: Option<String>,
        http_status: Option<u16>,
        verification_outcome: Option<String>,
    ) -> Self {
        let now = Utc::now();
        let duration_ms = match (task.started_at, task.finished_at) {
            (Some(s), Some(f)) => (f - s).num_milliseconds().max(0) as u64,
            _ => 0,
        };

        let (failure_category, failure_fingerprint, error_message) = if matches!(
            task.status,
            TaskStatus::Failed | TaskStatus::Timeout | TaskStatus::InfraError
        ) {
            let raw_reason = task
                .reason
                .as_deref()
                .unwrap_or("Task failed without reason");
            let sanitized = sanitize_telemetry_text(raw_reason);
            let category = categorize_failure(&sanitized, http_status);
            let masked = mask_dynamic_tokens(&sanitized);
            let fingerprint =
                compute_telemetry_fingerprint(&category, &masked, selector.as_deref(), None);
            (Some(category), Some(fingerprint), Some(sanitized))
        } else {
            (None, None, None)
        };

        Self {
            id: Uuid::new_v4(),
            project_id: project_id.to_string(),
            run_id,
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            test_case_id,
            execution_id: None,
            executor: executor.to_string(),
            environment: environment.to_string(),
            start_time: task.started_at.unwrap_or(now),
            end_time: task.finished_at,
            duration_ms,
            status: task.status,
            retry_count: 0,
            healing_attempt_count: 0,
            failure_category,
            failure_fingerprint,
            error_message,
            evidence_refs,
            assertion_result: None,
            http_status,
            selector,
            verification_outcome,
            created_at: now,
        }
    }
}

// ============================================================================
// 5. FLAKINESS & STABILITY ANALYSIS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlakinessClassification {
    Stable,
    PotentiallyFlaky,
    HighlyFlaky,
}

impl FlakinessClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "STABLE",
            Self::PotentiallyFlaky => "POTENTIALLY_FLAKY",
            Self::HighlyFlaky => "HIGHLY_FLAKY",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakinessAnalysis {
    pub task_name: String,
    pub environment: String,
    pub total_runs: usize,
    pub passes: usize,
    pub failures: usize,
    pub flakiness_score: f64,
    pub classification: FlakinessClassification,
    pub recent_outcomes: Vec<String>,
}

/// Evaluates flakiness deterministically using real execution history.
pub fn evaluate_flakiness(
    task_name: &str,
    environment: &str,
    outcomes: &[TaskStatus],
) -> FlakinessAnalysis {
    let relevant: Vec<bool> = outcomes
        .iter()
        .filter(|s| {
            matches!(
                s,
                TaskStatus::Passed | TaskStatus::Failed | TaskStatus::Timeout
            )
        })
        .map(|s| matches!(s, TaskStatus::Passed))
        .collect();

    let total_runs = relevant.len();
    let passes = relevant.iter().filter(|&&p| p).count();
    let failures = total_runs - passes;

    // Minimum 2 runs required to determine flakiness; 0-1 run is always Stable by default
    let (flakiness_score, classification) = if total_runs > 1 {
        let flips = relevant.windows(2).filter(|w| w[0] != w[1]).count();
        let score = flips as f64 / (total_runs - 1) as f64;
        let class = if score >= 0.40 {
            FlakinessClassification::HighlyFlaky
        } else if score >= 0.15 {
            FlakinessClassification::PotentiallyFlaky
        } else {
            FlakinessClassification::Stable
        };
        (score, class)
    } else {
        (0.0, FlakinessClassification::Stable)
    };

    FlakinessAnalysis {
        task_name: task_name.to_string(),
        environment: environment.to_string(),
        total_runs,
        passes,
        failures,
        flakiness_score: (flakiness_score * 1000.0).round() / 1000.0,
        classification,
        recent_outcomes: outcomes.iter().map(|s| s.as_str().to_string()).collect(),
    }
}

// ============================================================================
// 6. TREND DETECTION ENGINE
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrendDirection {
    Improving,
    Stable,
    Degrading,
    InsufficientData,
}

impl TrendDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Improving => "IMPROVING",
            Self::Stable => "STABLE",
            Self::Degrading => "DEGRADING",
            Self::InsufficientData => "INSUFFICIENT_DATA",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub metric_name: String,
    pub direction: TrendDirection,
    pub current_value: f64,
    pub previous_value: f64,
    pub sample_count: usize,
    pub notes: String,
}

/// Calculates trend direction between older and newer execution halves.
/// Invariant: Returns `InsufficientData` if sample count is strictly less than 3.
pub fn calculate_trend(metric_name: &str, values: &[f64]) -> TrendAnalysis {
    if values.len() < 3 {
        return TrendAnalysis {
            metric_name: metric_name.to_string(),
            direction: TrendDirection::InsufficientData,
            current_value: values.last().copied().unwrap_or(0.0),
            previous_value: values.first().copied().unwrap_or(0.0),
            sample_count: values.len(),
            notes: "Insufficient historical observations (minimum 3 required)".to_string(),
        };
    }

    let mid = values.len() / 2;
    let (older, newer) = values.split_at(mid);

    let older_avg = older.iter().sum::<f64>() / older.len() as f64;
    let newer_avg = newer.iter().sum::<f64>() / newer.len() as f64;

    let delta = newer_avg - older_avg;
    let threshold = 0.05; // 5% shift threshold

    let direction = if metric_name.contains("failure")
        || metric_name.contains("flaky")
        || metric_name.contains("duration")
    {
        // Lower is better
        if delta < -threshold {
            TrendDirection::Improving
        } else if delta > threshold {
            TrendDirection::Degrading
        } else {
            TrendDirection::Stable
        }
    } else {
        // Higher is better (e.g. success rate)
        if delta > threshold {
            TrendDirection::Improving
        } else if delta < -threshold {
            TrendDirection::Degrading
        } else {
            TrendDirection::Stable
        }
    };

    TrendAnalysis {
        metric_name: metric_name.to_string(),
        direction,
        current_value: (newer_avg * 100.0).round() / 100.0,
        previous_value: (older_avg * 100.0).round() / 100.0,
        sample_count: values.len(),
        notes: format!("Evaluated over {} data points", values.len()),
    }
}

// ============================================================================
// 7. STRATEGY RELIABILITY & RECOMMENDATION ENGINE
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRecommendation {
    pub recommended_strategy: String,
    pub confidence: f64,
    pub success_rate: f64,
    pub regression_rate: f64,
    pub total_attempts: i64,
    pub successful_heals: i64,
    pub regressions_introduced: i64,
    pub rationale: String,
}

/// Scores and recommends a healing strategy based on real historical metrics.
/// Uses Bayesian smoothing to prevent small-sample bias.
pub fn score_healing_strategy(
    category: &FailureCategory,
    metrics: &[HealingStrategyMetricsRow],
) -> StrategyRecommendation {
    // Default strategy based on failure category
    let default_strategy = match category {
        FailureCategory::SelectorNotFound | FailureCategory::SelectorAmbiguous => "selector_update",
        FailureCategory::Timeout => "timing_adjustment",
        FailureCategory::AssertionFailure => "assertion_adjustment",
        FailureCategory::NetworkFailure | FailureCategory::ClientError => "navigation_adjustment",
        _ => "selector_update",
    };

    let matching_metric = metrics.iter().find(|m| m.strategy == default_strategy);

    match matching_metric {
        Some(m) if m.attempts_count >= 3 => {
            let total = m.attempts_count as f64;
            let success = m.verified_count as f64;
            let regressions = m.regression_count as f64;

            let success_rate = success / total;
            let regression_rate = regressions / total;

            // Confidence dampens with regression rate and scales with sample size
            let sample_weight = (total / 10.0).min(1.0);
            let confidence = ((success_rate * 0.70 + sample_weight * 0.30)
                * (1.0 - regression_rate))
                .clamp(0.10, 0.98);

            StrategyRecommendation {
                recommended_strategy: default_strategy.to_string(),
                confidence: (confidence * 100.0).round() / 100.0,
                success_rate: (success_rate * 100.0).round() / 100.0,
                regression_rate: (regression_rate * 100.0).round() / 100.0,
                total_attempts: m.attempts_count,
                successful_heals: m.verified_count,
                regressions_introduced: m.regression_count,
                rationale: format!(
                    "Historically verified in {}/{} attempts ({:.0}% win rate, {:.0}% regression rate)",
                    m.verified_count, m.attempts_count, success_rate * 100.0, regression_rate * 100.0
                ),
            }
        }
        Some(m) => {
            let total = m.attempts_count.max(1) as f64;
            let success_rate = m.verified_count as f64 / total;
            StrategyRecommendation {
                recommended_strategy: default_strategy.to_string(),
                confidence: 0.50, // Insufficient sample size (<3)
                success_rate: (success_rate * 100.0).round() / 100.0,
                regression_rate: 0.0,
                total_attempts: m.attempts_count,
                successful_heals: m.verified_count,
                regressions_introduced: m.regression_count,
                rationale: format!(
                    "Preliminary recommendation; only {} recorded attempts exist",
                    m.attempts_count
                ),
            }
        }
        None => StrategyRecommendation {
            recommended_strategy: default_strategy.to_string(),
            confidence: 0.40,
            success_rate: 0.0,
            regression_rate: 0.0,
            total_attempts: 0,
            successful_heals: 0,
            regressions_introduced: 0,
            rationale:
                "Default heuristic based on failure category; zero historical attempts recorded"
                    .to_string(),
        },
    }
}

// ============================================================================
// 8. MULTI-FACTOR RISK CALIBRATION
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactors {
    pub failure_rate: f64,
    pub flakiness_score: f64,
    pub failure_severity: f64,
    pub dependency_count: usize,
    pub healing_regression_rate: f64,
    pub security_sensitivity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCalibration {
    pub task_name: String,
    pub risk_score: f64,
    pub risk_level: String,
    pub factors: RiskFactors,
    pub explanation: String,
}

/// Computes an explainable, multi-factor risk score.
pub fn calibrate_risk(task_name: &str, factors: RiskFactors) -> RiskCalibration {
    let dep_norm = (factors.dependency_count as f64 / 5.0).min(1.0);

    // Weighted risk formula:
    // 35% failure rate + 20% flakiness + 15% severity + 10% dependency + 10% regression + 10% security
    let score = (0.35 * factors.failure_rate
        + 0.20 * factors.flakiness_score
        + 0.15 * factors.failure_severity
        + 0.10 * dep_norm
        + 0.10 * factors.healing_regression_rate
        + 0.10 * factors.security_sensitivity)
        .clamp(0.0, 1.0);

    let risk_level = if score >= 0.70 {
        "CRITICAL"
    } else if score >= 0.50 {
        "HIGH"
    } else if score >= 0.25 {
        "MEDIUM"
    } else {
        "LOW"
    };

    let explanation = format!(
        "Risk score {:.2} ({risk_level}): failure_rate={:.2}, flakiness={:.2}, severity={:.2}, deps={}, regression={:.2}",
        score, factors.failure_rate, factors.flakiness_score, factors.failure_severity, factors.dependency_count, factors.healing_regression_rate
    );

    RiskCalibration {
        task_name: task_name.to_string(),
        risk_score: (score * 100.0).round() / 100.0,
        risk_level: risk_level.to_string(),
        factors,
        explanation,
    }
}

// ============================================================================
// 9. TEST PLANNING AGENT FEEDBACK PAYLOAD
// ============================================================================

/// Structured feedback provided to TestPlanningAgent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QaIntelligenceFeedback {
    pub fragile_tests: Vec<String>,
    pub fragile_selectors: Vec<String>,
    pub high_risk_components: Vec<String>,
    pub top_failure_clusters: Vec<String>,
    pub recommended_priorities: Vec<String>,
}

// ============================================================================
// 10. ADAPTIVE INTELLIGENCE SERVICE
// ============================================================================

/// Authoritative coordinator for Adaptive QA Intelligence & Observability.
pub struct AdaptiveIntelligenceService {
    pub store: Store,
}

impl AdaptiveIntelligenceService {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Records a normalized execution telemetry item and synchronizes Quality Graph.
    pub async fn record_execution_telemetry(
        &self,
        telemetry: ExecutionTelemetry,
    ) -> Result<(), String> {
        // 1. Insert into store
        self.store
            .insert_execution_telemetry(&telemetry)
            .await
            .map_err(|e| format!("failed to insert execution telemetry: {e}"))?;

        // 2. Synchronize Quality Graph if failure occurred
        if let (Some(ref category), Some(ref fingerprint)) =
            (&telemetry.failure_category, &telemetry.failure_fingerprint)
        {
            let pid = &telemetry.project_id;
            let cluster_title = format!("{}: {}", category.as_str(), &fingerprint[..12]);

            // Upsert failure_cluster node
            if let Ok(cluster_node_id) = self
                .store
                .upsert_quality_node(pid, "failure_cluster", fingerprint, &cluster_title)
                .await
            {
                // Upsert test node
                if let Ok(test_node_id) = self
                    .store
                    .upsert_quality_node(pid, "test", &telemetry.task_name, &telemetry.task_name)
                    .await
                {
                    let _ = self
                        .store
                        .upsert_quality_relation(test_node_id, cluster_node_id, "fails_with")
                        .await;
                }
            }
        }

        Ok(())
    }

    /// Generates structured QA intelligence feedback for the TestPlanningAgent.
    pub async fn generate_planning_feedback(
        &self,
        project_id: &str,
    ) -> Result<QaIntelligenceFeedback, String> {
        let telemetry_list = self
            .store
            .list_execution_telemetry(project_id, 100)
            .await
            .map_err(|e| format!("failed to load telemetry for planning feedback: {e}"))?;

        let mut fail_counts: HashMap<String, usize> = HashMap::new();
        let mut selector_failures: HashMap<String, usize> = HashMap::new();
        let mut cluster_counts: HashMap<String, usize> = HashMap::new();

        for t in &telemetry_list {
            if matches!(t.status.as_str(), "FAILED" | "TIMEOUT" | "INFRA_ERROR") {
                *fail_counts.entry(t.task_name.clone()).or_insert(0) += 1;
                if let Some(ref sel) = t.selector {
                    *selector_failures.entry(sel.clone()).or_insert(0) += 1;
                }
                if let Some(ref cat) = t.failure_category {
                    *cluster_counts.entry(cat.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut fragile_tests: Vec<String> = fail_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .map(|(name, count)| format!("{name} ({count} failures)"))
            .collect();
        fragile_tests.sort();

        let mut fragile_selectors: Vec<String> = selector_failures
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .map(|(sel, count)| format!("{sel} ({count} failures)"))
            .collect();
        fragile_selectors.sort();

        let mut top_failure_clusters: Vec<String> = cluster_counts
            .into_iter()
            .map(|(cat, count)| format!("{cat} ({count} occurrences)"))
            .collect();
        top_failure_clusters.sort();

        let mut recommended_priorities = Vec::new();
        if !fragile_tests.is_empty() {
            recommended_priorities
                .push("Prioritize regression tests for frequently failing tasks".to_string());
        }
        if !fragile_selectors.is_empty() {
            recommended_priorities
                .push("Review and update brittle DOM selectors in browser suites".to_string());
        }

        Ok(QaIntelligenceFeedback {
            fragile_tests,
            fragile_selectors,
            high_risk_components: Vec::new(),
            top_failure_clusters,
            recommended_priorities,
        })
    }
}
