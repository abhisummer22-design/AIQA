//! `GET /metrics` — Prometheus text exposition format, computed on demand
//! from the DB (no in-process counters to keep in sync, no extra crate: the
//! format is simple enough to emit by hand). Part of the build report's
//! "Observability, audit log, notifications, run recovery: None
//! implemented" gap.

use sqlx::{Row, SqlitePool};

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

    out.push_str("# HELP aiqa_telemetry_pruned_total Number of telemetry retention prune operations\n# TYPE aiqa_telemetry_pruned_total counter\n");
    if let Ok(row) =
        sqlx::query("SELECT COUNT(*) as c FROM audit_log WHERE action = 'telemetry.retention'")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_telemetry_pruned_total {c}\n"));
    }

    out.push_str("# HELP aiqa_retention_runs_total Number of executed retention runs\n# TYPE aiqa_retention_runs_total counter\n");
    if let Ok(row) =
        sqlx::query("SELECT COUNT(*) as c FROM audit_log WHERE action = 'telemetry.retention'")
            .fetch_one(pool)
            .await
    {
        let c: i64 = row.get("c");
        out.push_str(&format!("aiqa_retention_runs_total {c}\n"));
    }

    out
}
