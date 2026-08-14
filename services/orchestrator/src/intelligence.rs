//! Quality Graph / Risk / Failure / Flakiness intelligence — the prior
//! session's build report listed this as "None built — this is most of the
//! PRD's actual differentiators; pure AI/data-science work, zero started."
//!
//! Deliberately **not** LLM-backed: the task this session was scoped under
//! explicitly leaves Ollama/LLM integration out, and the original PRD
//! critique already flagged Ollama-for-reasoning as unproven with no
//! fallback strategy. So root-cause hypotheses here come from a small
//! rule-based classifier over the failure text, not a model call — every
//! score/hypothesis this module produces is a deterministic function of
//! stored data, with its inputs persisted alongside it (SRD §11
//! explainability requirement, and a direct answer to the critique finding
//! "risk/value/flakiness/confidence scores are described qualitatively
//! only"). Confidence values are capped low (<=0.6) to keep them honestly
//! distinguishable from what an LLM-derived hypothesis would claim.
//!
//! Runs after every run completes (`engine.rs` calls `ingest_run`). Reads
//! and writes through `Store` only — no direct SQL here.

use crate::domain::{RunPlan, TaskState, TaskStatus};
use crate::store::Store;
use anyhow::Result;
use regex::Regex;
use std::sync::OnceLock;

/// See `store.rs::upsert_quality_node` doc comment: SQLite's unique index
/// treats every `NULL` as distinct, so untenanted data uses this sentinel
/// instead of `NULL` to make upserts actually upsert.
pub fn tenant_key(project_id: Option<&str>) -> &str {
    project_id.unwrap_or("default")
}

fn digits_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d+").unwrap())
}

fn hexlike_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[0-9a-fA-F]{8,}\b").unwrap())
}

fn ws_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

/// Collapse a failure `reason` string down to a normalized signature so
/// repeated occurrences of "the same" failure (differing only in
/// timestamps, ports, request ids, line numbers, ...) cluster together
/// instead of each counting as a brand-new failure_cluster row.
pub fn normalize_signature(reason: &str) -> String {
    let lower = reason.to_lowercase();
    let no_hex = hexlike_re().replace_all(&lower, "#");
    let no_digits = digits_re().replace_all(&no_hex, "#");
    let collapsed = ws_re().replace_all(&no_digits, " ");
    let trimmed = collapsed.trim();
    // Cap length: a full stack trace shouldn't produce an unbounded-length
    // "signature" — first line plus a short tail is enough to disambiguate
    // the common cases (assertion text, connection errors, HTTP codes)
    // without the signature drifting on every stray log line.
    trimmed.chars().take(200).collect()
}

/// Rule-based root-cause hypothesis. Returns (hypothesis, confidence in
/// [0,1]). Every branch is a plain string match — no model, no embedding
/// similarity — see module doc comment for why.
pub fn classify_failure(reason: &str) -> (String, f64) {
    let lower = reason.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        return (
            "Operation exceeded its timeout threshold — check for a slow dependency or a deadlock, not necessarily a logic bug".into(),
            0.5,
        );
    }
    if lower.contains("econnrefused") || lower.contains("connection refused") || lower.contains("connect error") {
        return (
            "Target service was unreachable (connection refused) — likely down, still starting, or pointed at the wrong base URL".into(),
            0.55,
        );
    }
    // Checked before the bare status-code checks below: assertion language
    // is a more specific signal than merely containing a 3-digit number
    // (e.g. "expected 200 to equal 404" is a test assertion mismatch, not
    // literally a 404 response — the old ordering let " 404" shadow this).
    if lower.contains("assert") || lower.contains("expect") || lower.contains("to have status") || lower.contains("to eql") {
        return (
            "Assertion mismatch between expected and actual output — behavior likely changed since the assertion was written".into(),
            0.4,
        );
    }
    if lower.contains(" 404") || lower.contains("not found") {
        return (
            "Endpoint returned 404 — route missing, renamed, or moved since the test was written".into(),
            0.45,
        );
    }
    if lower.contains(" 500") || lower.contains("internal server error") {
        return (
            "Endpoint returned a 5xx server error — likely an unhandled exception in the target, not a test bug".into(),
            0.5,
        );
    }
    (
        "Failure signature did not match a known heuristic category — needs human triage (no LLM root-cause analysis in this build)".into(),
        0.2,
    )
}

pub struct Intelligence<'a> {
    pub store: &'a Store,
}

impl<'a> Intelligence<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Called once per completed run (`engine.rs`, on `RunCompleted`).
    /// Updates the quality graph + risk/value/flakiness for every task that
    /// reached a terminal state, and clusters/drafts bugs for product
    /// failures (FAILED/TIMEOUT — deliberately excluding INFRA_ERROR,
    /// BLOCKED, SKIPPED, CANCELLED: those aren't evidence the *thing under
    /// test* is broken, per the FRD §2 product-vs-infrastructure distinction
    /// this codebase already keeps everywhere else).
    pub async fn ingest_run(&self, project_id: Option<&str>, plan: &RunPlan, tasks: &[TaskState]) -> Result<()> {
        let pid = tenant_key(project_id);
        let prereq_counts: std::collections::HashMap<&str, usize> = plan
            .tasks
            .iter()
            .map(|t| (t.id.as_str(), t.prerequisites.len()))
            .collect();

        for t in tasks {
            if !t.status.is_terminal() {
                continue;
            }
            self.update_test_intelligence(pid, t, prereq_counts.get(t.id.as_str()).copied().unwrap_or(0))
                .await?;

            if matches!(t.status, TaskStatus::Failed | TaskStatus::Timeout) {
                self.record_failure(pid, t).await?;
            }
        }
        Ok(())
    }

    async fn update_test_intelligence(&self, project_id: &str, t: &TaskState, dependency_count: usize) -> Result<()> {
        let node_id = self
            .store
            .upsert_quality_node(project_id, "test", &t.name, &t.name)
            .await?;

        let history = self.store.task_history_by_name(&t.name, 20).await?;
        let relevant: Vec<bool> = history
            .iter()
            .filter(|h| matches!(h.status, TaskStatus::Passed | TaskStatus::Failed | TaskStatus::Timeout))
            .map(|h| matches!(h.status, TaskStatus::Passed))
            .collect();

        let window_runs = relevant.len() as i64;
        let window_failures = relevant.iter().filter(|&&pass| !pass).count() as i64;
        let historical_failure_rate = if window_runs > 0 {
            window_failures as f64 / window_runs as f64
        } else {
            0.0
        };
        let flips = relevant.windows(2).filter(|w| w[0] != w[1]).count();
        let flakiness_score = if relevant.len() > 1 {
            flips as f64 / (relevant.len() - 1) as f64
        } else {
            0.0
        };

        let execution_cost_ms = match (t.started_at, t.finished_at) {
            (Some(s), Some(f)) => (f - s).num_milliseconds().max(0) as f64,
            _ => 0.0,
        };

        // Weighted, documented, explainable — see module doc comment. Every
        // weight and input is chosen to be inspectable, not tuned against
        // any labeled dataset (none exists yet — see README "Intelligence
        // layer" limitations).
        let dep_norm = (dependency_count as f64 / 5.0).min(1.0);
        let cost_norm = (execution_cost_ms / 30_000.0).min(1.0);
        let security_sensitivity = 0.0_f64; // no signal source wired up yet
        let business_criticality = 0.0_f64; // no requirement-linkage source wired up yet
        let change_frequency = 0.0_f64; // no change-intelligence source wired up yet (see README)

        let risk_score = (0.40 * historical_failure_rate
            + 0.20 * flakiness_score
            + 0.15 * dep_norm
            + 0.15 * cost_norm
            + 0.10 * security_sensitivity)
            .clamp(0.0, 1.0);

        let inputs = serde_json::json!({
            "historical_failure_rate": historical_failure_rate,
            "flakiness_score": flakiness_score,
            "dependency_count": dependency_count,
            "dependency_count_normalized": dep_norm,
            "execution_cost_ms": execution_cost_ms,
            "execution_cost_normalized": cost_norm,
            "security_sensitivity": security_sensitivity,
            "window_runs": window_runs,
            "weights": {"failure_rate": 0.40, "flakiness": 0.20, "dependency_count": 0.15, "execution_cost": 0.15, "security_sensitivity": 0.10},
            "note": "heuristic, unweighted by any human-override feedback loop yet; business_criticality/change_frequency have no data source wired up in this slice"
        })
        .to_string();

        self.store
            .upsert_risk_assessment(
                node_id,
                risk_score,
                business_criticality,
                change_frequency,
                historical_failure_rate,
                dependency_count as i64,
                security_sensitivity,
                execution_cost_ms,
                &inputs,
            )
            .await?;

        let value_score = (0.40 * historical_failure_rate
            + 0.30 * risk_score
            + 0.20 * (1.0 - flakiness_score)
            + 0.10 * (1.0 - cost_norm))
            .clamp(0.0, 1.0);
        self.store
            .upsert_test_value(
                node_id,
                value_score,
                risk_score,
                historical_failure_rate,
                business_criticality,
                change_frequency,
                execution_cost_ms,
                flakiness_score,
            )
            .await?;

        self.store
            .upsert_flakiness(node_id, flakiness_score, window_runs, window_failures)
            .await?;

        Ok(())
    }

    async fn record_failure(&self, project_id: &str, t: &TaskState) -> Result<()> {
        let reason = t.reason.clone().unwrap_or_default();
        let signature = normalize_signature(&reason);
        if signature.is_empty() {
            return Ok(());
        }
        let (hypothesis, confidence) = classify_failure(&reason);
        let (cluster_id, occurrence_count) = self
            .store
            .upsert_failure_cluster(project_id, &signature, &hypothesis, confidence)
            .await?;

        // Anomaly -> confirmed-defect promotion path (PRD §14, flagged as
        // undefined in the original critique): a single occurrence is
        // still "might be a one-off"; two or more of the *same normalized
        // signature* is recurring enough to draft, but it stays
        // NEEDS_REVIEW until a human calls POST /bug-drafts/:id/review —
        // this module never auto-confirms anything.
        if occurrence_count >= 2 && !self.store.has_bug_draft_for_cluster(cluster_id).await? {
            let severity = if occurrence_count >= 5 { "High" } else { "Medium" };
            let title = format!("Recurring failure in '{}' ({occurrence_count}x): {}", t.name, signature.chars().take(80).collect::<String>());
            let description = format!(
                "Task '{}' has failed {occurrence_count} times with a matching failure signature.\n\n\
                 Heuristic root-cause hypothesis (rule-based, not LLM-derived — see intelligence.rs): {hypothesis}\n\n\
                 Most recent raw reason:\n{reason}\n\n\
                 This draft was generated automatically from recurring FAILED/TIMEOUT executions. \
                 It is NOT a confirmed defect — review and call POST /bug-drafts/{{id}}/review to confirm or dismiss.",
                t.name,
            );
            self.store
                .insert_bug_draft(cluster_id, &title, &description, severity, confidence)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_signature_collapses_numbers_and_hex_ids() {
        let a = "playwright exited with Some(1)\nexpected \"200\" got \"404\" at line 42 (req abcdef1234567890)";
        let b = "playwright exited with Some(1)\nexpected \"200\" got \"404\" at line 99 (req 1122334455667788)";
        assert_eq!(normalize_signature(a), normalize_signature(b));
    }

    #[test]
    fn classify_failure_recognizes_common_categories() {
        assert!(classify_failure("connect ECONNREFUSED 127.0.0.1:8080").0.contains("unreachable"));
        assert!(classify_failure("Timeout 30000ms exceeded").0.contains("timeout"));
        assert!(classify_failure("expected 200 to equal 404").0.contains("Assertion"));
        assert!(classify_failure("some totally novel failure text").1 <= 0.2);
    }
}
