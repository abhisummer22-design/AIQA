//! Quality Graph / Risk / Failure / Flakiness intelligence — the prior
//! session's build report listed this as "None built — this is most of the
//! PRD's actual differentiators; pure AI/data-science work, zero started."
//!
//! **The automatic clustering/promotion pipeline below remains deliberately
//! not LLM-backed**, unchanged by build order item 4: the original PRD
//! critique flagged Ollama-for-reasoning as unproven with no fallback
//! strategy, so every score/hypothesis `ingest_run`/`record_failure`
//! produce automatically, at run-completion time, still comes from a small
//! rule-based classifier over the failure text (`classify_failure`), not a
//! model call — a deterministic function of stored data, with its inputs
//! persisted alongside it (SRD §11 explainability requirement). Confidence
//! values here are still capped low (<=0.6) to keep them honestly
//! distinguishable from what an LLM-derived hypothesis would claim. This
//! choice was never about LLM calls being slow (a real one can take on the
//! order of a minute — see `ai_generation.rs`'s own live test) as much as
//! about not gating run-completion processing, which must stay fast, on a
//! model call at all.
//!
//! **Build order item 4 (22 August 2026) adds a second, separate, on-demand
//! layer on top of that unchanged pipeline** — see `generate_ai_hypothesis`
//! below. It is never triggered automatically and never touches
//! `record_failure`'s own deterministic output; a caller (currently
//! `POST .../findings/:id/ai-hypothesis`, `api.rs`) explicitly asks, for
//! one already-clustered failure at a time, for an additional,
//! AI-generated hypothesis — reusing the exact same central
//! `ai_router::AiInfrastructure` (discovery/routing/scoring/fallback)
//! `ai_generation.rs` already established for `AiTask::TestCaseGeneration`,
//! now for the pre-existing `AiTask::BugAnalysis` task type (defined in
//! `ai_router.rs` since an earlier phase but never wired to a real caller
//! until this unit — confirmed by grep before writing a line of this
//! layer). The result is stored in `FailureClusterRow`'s own separate
//! `ai_hypothesis`/`ai_hypothesis_provider`/`ai_hypothesis_model` columns,
//! never merged into `root_cause_hypothesis` — the concrete mechanism
//! behind ACC-010's "AI hypotheses must remain distinguishable from
//! confirmed facts" criteria for this specific case, and ACC-018's Failure
//! Behavior criteria ("controlled fallback, or NEEDS_REVIEW — never
//! fabricated output"): a failed/unavailable AI attempt writes nothing at
//! all, so the cluster's real, already-real deterministic data is the only
//! thing a caller ever sees on failure, not a fabricated substitute.
//!
//! Runs after every run completes (`engine.rs` calls `ingest_run`). Reads
//! and writes through `Store` only — no direct SQL here.
//!
//! **Requirement-graph edges (closes a third gap this module's own doc
//! comment lineage left open): `quality_relations` has been real schema
//! since Phase 1 but had zero write paths anywhere in this codebase — this
//! module's own `upsert_quality_node` only ever created `"test"` nodes.
//! `link_test_case_to_requirement` is the first real writer: called once,
//! by `api.rs::create_test_case_scoped`, whenever a persisted test case
//! carries a real `requirement_reference` and/or `source_requirement` (both
//! already real, existing fields on `CreateTestCaseRequest` — no schema
//! change). A case with neither writes nothing at all — the same
//! "grounding over generation" rule `ai_generation.rs`'s traceability gate
//! already enforces, applied here to the graph itself: no real requirement
//! text or reference, no requirement node.**
//!
//! This is deliberately the narrow, honest slice of the architecture
//! brief's "Requirement Graph" layer that is actually buildable today —
//! real edges between requirement text and the test cases actually
//! generated from it, real impact analysis over them
//! (`Store::list_test_cases_for_requirement`). The brief's larger vision
//! (parsed sub-requirement/field/rule/role nodes, a doc-vs-reality gap
//! check) needs real document ingestion and a real crawler, neither of
//! which exist in this codebase yet — building that structure now, over
//! data this thin, would be exactly the "planning over fake data" this
//! project's own roadmap discussion already rejected. What's here is real
//! and load-bearing; nothing further is implied.

use crate::domain::{RunPlan, TaskState, TaskStatus};
use crate::store::Store;
use anyhow::Result;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use uuid::Uuid;

/// See `store.rs::upsert_quality_node` doc comment: SQLite's unique index
/// treats every `NULL` as distinct, so untenanted data uses this sentinel
/// instead of `NULL` to make upserts actually upsert.
pub fn tenant_key(project_id: Option<&str>) -> &str {
    project_id.unwrap_or("default")
}

/// The real, stable identity a `"requirement"` quality-node is keyed on —
/// pure and DB-free so every rule below is unit-testable without a
/// database, the same "real logic lives in a pure function" pattern
/// `ai_generation.rs::build_feedback_context` already established. Returns
/// `(external_ref, display_name)`, or `None` when there is genuinely
/// nothing real to key a node on (grounding over generation: no
/// requirement text or reference, no requirement node — never a
/// fabricated placeholder identity).
///
/// A real, human-assigned `requirement_reference` (e.g. "REQ-042") is
/// preferred when present — it is the more stable identity: two test cases
/// citing the same reference should link to the same node even if their
/// `source_requirement` excerpts differ slightly. Only when no reference
/// was given does this fall back to a stable SHA-256 of the real
/// `source_requirement` text itself (prefixed `text:` so it can never
/// collide with a real human-assigned reference string) — the same real
/// text `ai_generation.rs`'s traceability gate already trusts as the
/// source of truth for this test case, not a second, independent guess.
pub fn requirement_identity(
    requirement_reference: Option<&str>,
    source_requirement: Option<&str>,
) -> Option<(String, String)> {
    if let Some(reference) = requirement_reference
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some((reference.to_string(), reference.to_string()));
    }
    if let Some(text) = source_requirement.map(str::trim).filter(|s| !s.is_empty()) {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let external_ref = format!("text:{}", hex::encode(hasher.finalize()));
        return Some((external_ref, text.to_string()));
    }
    None
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
    if lower.contains("econnrefused")
        || lower.contains("connection refused")
        || lower.contains("connect error")
    {
        return (
            "Target service was unreachable (connection refused) — likely down, still starting, or pointed at the wrong base URL".into(),
            0.55,
        );
    }
    // Checked before the bare status-code checks below: assertion language
    // is a more specific signal than merely containing a 3-digit number
    // (e.g. "expected 200 to equal 404" is a test assertion mismatch, not
    // literally a 404 response — the old ordering let " 404" shadow this).
    if lower.contains("assert")
        || lower.contains("expect")
        || lower.contains("to have status")
        || lower.contains("to eql")
    {
        return (
            "Assertion mismatch between expected and actual output — behavior likely changed since the assertion was written".into(),
            0.4,
        );
    }
    if lower.contains(" 404") || lower.contains("not found") {
        return (
            "Endpoint returned 404 — route missing, renamed, or moved since the test was written"
                .into(),
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
    pub async fn ingest_run(
        &self,
        project_id: Option<&str>,
        plan: &RunPlan,
        tasks: &[TaskState],
    ) -> Result<()> {
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
            self.update_test_intelligence(
                pid,
                t,
                prereq_counts.get(t.id.as_str()).copied().unwrap_or(0),
            )
            .await?;

            if matches!(t.status, TaskStatus::Failed | TaskStatus::Timeout) {
                self.record_failure(pid, t).await?;
            }
        }
        Ok(())
    }

    async fn update_test_intelligence(
        &self,
        project_id: &str,
        t: &TaskState,
        dependency_count: usize,
    ) -> Result<()> {
        let node_id = self
            .store
            .upsert_quality_node(project_id, "test", &t.name, &t.name)
            .await?;

        // B3 — scored history must come from this tenant only. `project_id`
        // here is already the resolved tenant key (see `tenant_key`), so an
        // untenanted run scores against other untenanted runs and never
        // against a real project's history.
        let history = self
            .store
            .task_history_by_name(&t.name, Some(project_id), 20)
            .await?;
        let relevant: Vec<bool> = history
            .iter()
            .filter(|h| {
                matches!(
                    h.status,
                    TaskStatus::Passed | TaskStatus::Failed | TaskStatus::Timeout
                )
            })
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
            let severity = if occurrence_count >= 5 {
                "High"
            } else {
                "Medium"
            };
            let title = format!(
                "Recurring failure in '{}' ({occurrence_count}x): {}",
                t.name,
                signature.chars().take(80).collect::<String>()
            );
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

    /// The "pattern learner" half of the feedback loop this module's own doc
    /// comment describes but, until now, nothing actually read back:
    /// `ingest_run` above (and `record_failure`'s human-reviewed bug drafts)
    /// have always computed real risk/flakiness scores and clustered/
    /// confirmed real defect patterns — this is the read path that lets
    /// `ai_generation::generate_test_cases` see them. Two existing,
    /// already-tested `Store` reads only (`list_test_intelligence`,
    /// `list_bug_drafts`) reshaped by the pure, already-unit-tested
    /// `ai_generation::build_feedback_context` — no new SQL, no new table.
    /// Called once per generation request (`api.rs`), never automatically,
    /// so a slow/failing read can never block a run the way `ingest_run`'s
    /// synchronous run-completion path must not be blocked either (see this
    /// module's own doc comment on why that path stays fast).
    pub async fn load_feedback_context(
        &self,
        project_id: &str,
    ) -> Result<crate::ai_generation::FeedbackContext> {
        let intel = self.store.list_test_intelligence(Some(project_id)).await?;
        let bug_drafts = self.store.list_bug_drafts(Some(project_id)).await?;
        Ok(crate::ai_generation::build_feedback_context(
            &intel,
            &bug_drafts,
        ))
    }

    /// The first real write to `quality_relations` (see this module's own
    /// doc comment above). Called once per real test-case persistence
    /// (`api.rs::create_test_case_scoped`) — never at draft/generation
    /// time, since a draft may be discarded and never becomes a real,
    /// durable fact worth graphing. A case with no real requirement
    /// reference or text (`requirement_identity` returns `None`) writes
    /// nothing at all and returns `Ok(())` — not an error, just genuinely
    /// nothing to link.
    ///
    /// Upserts a `"requirement"` node (real identity — see
    /// `requirement_identity`) and a `"test_case"` node (identity: the
    /// real, already-persisted test case's own id — distinct from the
    /// `"test"` node type `update_test_intelligence` above creates from
    /// *execution* history; a case can exist, and be graphed here, long
    /// before it is ever run), then a real `"validates"` edge between them.
    pub async fn link_test_case_to_requirement(
        &self,
        project_id: &str,
        test_case_id: Uuid,
        test_case_title: &str,
        requirement_reference: Option<&str>,
        source_requirement: Option<&str>,
    ) -> Result<()> {
        let Some((req_ref, req_name)) =
            requirement_identity(requirement_reference, source_requirement)
        else {
            return Ok(());
        };
        let pid = tenant_key(Some(project_id));

        let req_node_id = self
            .store
            .upsert_quality_node(pid, "requirement", &req_ref, &req_name)
            .await?;
        let case_node_id = self
            .store
            .upsert_quality_node(pid, "test_case", &test_case_id.to_string(), test_case_title)
            .await?;
        self.store
            .upsert_quality_relation(case_node_id, req_node_id, "validates")
            .await?;
        Ok(())
    }
}

// ------------------------------------------------- AI-assisted root cause (BugAnalysis)
//
// Build order item 4. Follows the exact shape `ai_generation.rs` already
// established for `AiTask::TestCaseGeneration` (prompt builder ->
// structured-JSON schema -> central `AiInfrastructure::generate` ->
// validate -> `Ai { .. } | Failed { reason }`), reusing that same central
// infrastructure as-is — no parallel provider/router/executor path. See
// this module's own doc comment above for why this lives here (colocated
// with its real consumer, `record_failure`'s already-clustered data)
// rather than inside `ai_generation.rs` itself, and for why it is on-demand
// rather than wired into `ingest_run`'s synchronous run-completion path.

use crate::ai_router::{AiInfrastructure, AiRequest, AiTask, GenerationOutcome2};

/// The one structured field this task asks the model for — deliberately
/// minimal (unlike `GeneratedTestCaseDraft`'s multi-field schema, a
/// root-cause hypothesis is inherently one piece of prose, not a record
/// with independently-meaningful sub-fields). No fabricated "AI confidence"
/// score is requested or invented: unlike `classify_failure`'s
/// heuristic-derived confidence (a real, disclosed function of which
/// branch matched), a raw model call here has no comparably real basis for
/// a numeric confidence, and `ai_generation.rs`'s own schema does not ask
/// its model for one either — no new precedent invented for this task.
#[derive(Debug, Clone, serde::Deserialize)]
struct AiHypothesisResponse {
    hypothesis: String,
}

#[derive(Debug, Clone)]
pub enum AiHypothesisOutcome {
    Ai {
        hypothesis: String,
        provider: String,
        model: String,
    },
    Failed {
        reason: String,
    },
}

/// Builds the real prompt sent to the model. Takes only what is actually,
/// really persisted on a `FailureClusterRow` — the *normalized* signature
/// (numbers/hex ids already collapsed to `#` by `normalize_signature`, see
/// module doc comment above), the existing deterministic hypothesis, and
/// the real occurrence count. There is no raw, per-occurrence failure
/// reason text stored anywhere once `record_failure` has run (`store.rs`'s
/// `failure_clusters` table has no such column) — so unlike
/// `ai_generation.rs::build_prompt` (which has the real, full requirement
/// text to work from), this prompt is honest about operating on the
/// normalized signature only, and explicitly tells the model that numbers/
/// ids were collapsed, so it does not mistake `#` for a literal character
/// in the real failure text or invent a specific number to fill the gap.
pub fn build_bug_analysis_prompt(
    signature: &str,
    rule_based_hypothesis: &str,
    occurrence_count: i64,
) -> String {
    format!(
        "You are assisting with root-cause analysis of a recurring software test failure. A deterministic, \
         rule-based classifier has already produced a first-pass hypothesis (shown below) by matching the \
         failure text against known categories (timeout, connection error, assertion mismatch, HTTP status, \
         etc.). Your job is to add a genuinely deeper, more specific hypothesis using your own reasoning over \
         the actual failure signature below — do not just restate or rephrase the rule-based hypothesis.\n\n\
         NORMALIZED FAILURE SIGNATURE (numbers and hex-like ids have already been collapsed to \"#\" so \
         repeats of \"the same\" failure cluster together — \"#\" is a placeholder, not a literal character \
         from the real failure text):\n\
         \"\"\"\n{signature}\n\"\"\"\n\n\
         THIS SIGNATURE HAS RECURRED {occurrence_count} TIME(S).\n\n\
         EXISTING RULE-BASED HYPOTHESIS (for context only — do not just repeat this):\n\"{rule_based_hypothesis}\"\n\n\
         Rules:\n\
         - Base your hypothesis only on the signature text actually given above — never invent a specific \
           file name, line number, function name, request id, or other concrete detail the collapsed \
           signature does not actually evidence.\n\
         - Do not invent a specific number to fill in for a collapsed \"#\" — reason about the category of \
           value it represents (a count, an id, a status code, ...) instead.\n\
         - If the signature genuinely gives no additional basis beyond the rule-based hypothesis, say so \
           plainly rather than inventing detail.\n\n\
         Return ONLY a JSON object with this exact shape, no markdown, no commentary:\n\
         {{\"hypothesis\": string}}"
    )
}

/// The `BugAnalysis` consumer of the central `AiInfrastructure` — the same
/// crossing-the-boundary role `ai_generation.rs::generate_test_cases` plays
/// for `TestCaseGeneration`. Never returns a fabricated success: a routing
/// failure, execution failure, or invalid structured response all become a
/// real `Failed { reason }`, and the caller (`api.rs`) is the one place
/// that decides what a failure means for the HTTP response — this function
/// itself does not touch `Store` at all (see `set_ai_hypothesis`, called
/// only by the caller, only on `Ai { .. }`).
pub async fn generate_ai_hypothesis(
    infra: &AiInfrastructure,
    signature: &str,
    rule_based_hypothesis: &str,
    occurrence_count: i64,
) -> AiHypothesisOutcome {
    let prompt = build_bug_analysis_prompt(signature, rule_based_hypothesis, occurrence_count);
    let request = AiRequest {
        task: AiTask::BugAnalysis,
        prompt,
        preferred_provider: None,
        preferred_model: None,
    };
    let result = match infra.generate(request).await {
        GenerationOutcome2::Ok(r) => r,
        GenerationOutcome2::Failed { reason } => return AiHypothesisOutcome::Failed { reason },
    };

    let parsed: AiHypothesisResponse = match serde_json::from_str(&result.text) {
        Ok(v) => v,
        Err(e) => {
            return AiHypothesisOutcome::Failed {
                reason: format!("invalid structured output: {e}"),
            }
        }
    };
    if parsed.hypothesis.trim().is_empty() {
        return AiHypothesisOutcome::Failed {
            reason: "AI response contained an empty hypothesis".to_string(),
        };
    }

    AiHypothesisOutcome::Ai {
        hypothesis: parsed.hypothesis,
        provider: result.provider,
        model: result.model,
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
        assert!(classify_failure("connect ECONNREFUSED 127.0.0.1:8080")
            .0
            .contains("unreachable"));
        assert!(classify_failure("Timeout 30000ms exceeded")
            .0
            .contains("timeout"));
        assert!(classify_failure("expected 200 to equal 404")
            .0
            .contains("Assertion"));
        assert!(classify_failure("some totally novel failure text").1 <= 0.2);
    }

    // ------------------------------------------------------ requirement graph

    #[test]
    fn requirement_identity_prefers_a_real_reference_over_the_source_text() {
        let (key, name) = requirement_identity(Some("REQ-042"), Some("Users can log in.")).unwrap();
        assert_eq!(key, "REQ-042");
        assert_eq!(name, "REQ-042");
    }

    #[test]
    fn requirement_identity_falls_back_to_a_stable_hash_of_the_real_text() {
        let (key1, name1) = requirement_identity(None, Some("Users can log in.")).unwrap();
        let (key2, _) = requirement_identity(None, Some("Users can log in.")).unwrap();
        assert!(key1.starts_with("text:"), "key={key1}");
        assert_eq!(
            key1, key2,
            "the same real text must always hash to the same identity"
        );
        assert_eq!(name1, "Users can log in.");
    }

    #[test]
    fn requirement_identity_of_different_real_text_never_collides() {
        let (key1, _) = requirement_identity(None, Some("Users can log in.")).unwrap();
        let (key2, _) =
            requirement_identity(None, Some("Users can reset their password.")).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn requirement_identity_trims_whitespace_before_hashing_and_before_comparing_references() {
        let (key1, _) = requirement_identity(None, Some("Users can log in.")).unwrap();
        let (key2, _) = requirement_identity(None, Some("  Users can log in.  \n")).unwrap();
        assert_eq!(key1, key2);
        let (ref1, _) = requirement_identity(Some("REQ-042"), None).unwrap();
        let (ref2, _) = requirement_identity(Some("  REQ-042  "), None).unwrap();
        assert_eq!(ref1, ref2);
    }

    #[test]
    fn requirement_identity_is_none_when_neither_field_carries_anything_real() {
        assert!(requirement_identity(None, None).is_none());
        assert!(
            requirement_identity(Some("   "), Some("")).is_none(),
            "whitespace-only/empty strings are not real content either"
        );
    }

    fn minimal_create_request(
        title: &str,
        reference: Option<&str>,
    ) -> crate::domain::CreateTestCaseRequest {
        crate::domain::CreateTestCaseRequest {
            title: title.to_string(),
            description: "description".to_string(),
            category: "Functional".to_string(),
            priority: "MEDIUM".to_string(),
            severity: "MINOR".to_string(),
            preconditions: vec![],
            steps: vec![crate::domain::TestStep {
                step_number: 1,
                action: "Do X".to_string(),
                expected_result: "Y happens".to_string(),
            }],
            expected_result: Some("Y happens".to_string()),
            assertions: vec![],
            test_data: None,
            source_document: None,
            requirement_reference: reference.map(String::from),
            source_requirement: None,
            traceability: None,
            risk_factor: None,
            tags: vec![],
            suite_id: None,
        }
    }

    /// Real `Store`, real sqlite file, and — unlike a test that only calls
    /// `link_test_case_to_requirement` in isolation — real `test_cases` rows
    /// inserted first via `Store::insert_test_case`, the same real sequence
    /// `api.rs::create_test_case_scoped` actually runs. This matters
    /// because `list_test_cases_for_requirement` genuinely INNER JOINs
    /// against `test_cases` (a graphed case that was since deleted must not
    /// come back as a phantom result) — a test that skipped the real insert
    /// would pass for the wrong reason. Proves: two test cases citing the
    /// same real `requirement_reference` link to the SAME requirement node
    /// (not two), a third citing a different reference does not, a fourth
    /// with neither field writes nothing at all (no crash, no phantom
    /// node), and project isolation holds. The identity-derivation rules
    /// themselves are already fully covered, DB-free, by
    /// `requirement_identity`'s own unit tests above.
    #[tokio::test]
    async fn link_test_case_to_requirement_reflects_real_edges_and_respects_project_isolation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = crate::store::Store::connect(&dir.path().join("intel.sqlite3"))
            .await
            .expect("connect");
        let intel = Intelligence::new(&store);

        let case_a = store
            .insert_test_case(
                "proj-1",
                &minimal_create_request("Login succeeds", Some("REQ-042")),
                None,
            )
            .await
            .expect("insert case_a");
        let case_b = store
            .insert_test_case(
                "proj-1",
                &minimal_create_request("Login rejects bad password", Some("REQ-042")),
                None,
            )
            .await
            .expect("insert case_b");
        let case_c = store
            .insert_test_case(
                "proj-1",
                &minimal_create_request("Password reset works", Some("REQ-099")),
                None,
            )
            .await
            .expect("insert case_c");
        let case_d = store
            .insert_test_case(
                "proj-1",
                &minimal_create_request("Unrelated case", None),
                None,
            )
            .await
            .expect("insert case_d");
        let case_other_project = store
            .insert_test_case(
                "proj-2",
                &minimal_create_request("Other project's login case", Some("REQ-042")),
                None,
            )
            .await
            .expect("insert case_other_project");

        for (project, id, title, reference) in [
            ("proj-1", case_a, "Login succeeds", Some("REQ-042")),
            (
                "proj-1",
                case_b,
                "Login rejects bad password",
                Some("REQ-042"),
            ),
            ("proj-1", case_c, "Password reset works", Some("REQ-099")),
            ("proj-1", case_d, "Unrelated case", None),
            (
                "proj-2",
                case_other_project,
                "Other project's login case",
                Some("REQ-042"),
            ),
        ] {
            intel
                .link_test_case_to_requirement(project, id, title, reference, None)
                .await
                .unwrap_or_else(|e| panic!("link {id}: {e}"));
        }

        let affected = store
            .list_test_cases_for_requirement("proj-1", "REQ-042")
            .await
            .expect("list_test_cases_for_requirement");
        let affected_ids: Vec<Uuid> = affected.iter().map(|c| c.id).collect();
        assert_eq!(
            affected_ids.len(),
            2,
            "must contain exactly case_a and case_b, not case_c/case_d/the other project's case: {affected:?}"
        );
        assert!(affected_ids.contains(&case_a));
        assert!(affected_ids.contains(&case_b));
        assert!(!affected_ids.contains(&case_c));
        assert!(!affected_ids.contains(&case_other_project));

        let unrelated = store
            .list_test_cases_for_requirement("proj-1", "REQ-does-not-exist")
            .await
            .expect("a never-linked key must return a real, empty answer, not an error");
        assert!(unrelated.is_empty());
    }

    // --------------------------------------------------- feedback context loader

    /// Real `Store`, real sqlite file (tempdir-backed, same pattern
    /// `api_tests.rs::test_state()` already uses) — not a mock. Proves the
    /// actual wiring end-to-end: a genuinely risky node and a genuinely
    /// CONFIRMED bug draft come back in the loaded `FeedbackContext`; an
    /// ordinary node, a NEEDS_REVIEW draft, a DISMISSED draft, and another
    /// project's same-named risky node do not. The filtering/threshold/cap
    /// logic itself is already fully covered, DB-free, by
    /// `ai_generation.rs`'s own `build_feedback_context` unit tests — this
    /// test's job is only to prove the two real `Store` reads feeding it are
    /// wired to the right project and the right columns.
    #[tokio::test]
    async fn load_feedback_context_reflects_real_seeded_risk_and_confirmed_defect_data_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = crate::store::Store::connect(&dir.path().join("intel.sqlite3"))
            .await
            .expect("connect");

        // A genuinely risky, named node in "proj-1".
        let risky_node = store
            .upsert_quality_node("proj-1", "test", "Checkout Flow", "Checkout Flow")
            .await
            .expect("upsert risky node");
        store
            .upsert_risk_assessment(risky_node, 0.8, 0.0, 0.0, 0.9, 2, 0.0, 500.0, "{}")
            .await
            .expect("upsert risk assessment");
        store
            .upsert_flakiness(risky_node, 0.1, 10, 1)
            .await
            .expect("upsert flakiness");

        // An ordinary, low-risk node in the same project — must not appear.
        let safe_node = store
            .upsert_quality_node("proj-1", "test", "About Page", "About Page")
            .await
            .expect("upsert safe node");
        store
            .upsert_risk_assessment(safe_node, 0.1, 0.0, 0.0, 0.05, 0, 0.0, 100.0, "{}")
            .await
            .expect("upsert risk assessment");

        // The exact same risky node name, but under a DIFFERENT project —
        // tenant isolation must hold, same as `list_test_intelligence`'s own
        // B3 guarantee this loader relies on rather than reimplementing.
        let other_project_node = store
            .upsert_quality_node("proj-2", "test", "Checkout Flow", "Checkout Flow")
            .await
            .expect("upsert other-project node");
        store
            .upsert_risk_assessment(
                other_project_node,
                0.95,
                0.0,
                0.0,
                0.99,
                3,
                0.0,
                900.0,
                "{}",
            )
            .await
            .expect("upsert risk assessment");

        // A CONFIRMED bug draft — must appear.
        let (confirmed_cluster, _) = store
            .upsert_failure_cluster("proj-1", "sig-confirmed", "hypothesis", 0.5)
            .await
            .expect("upsert failure cluster");
        let confirmed_draft = store
            .insert_bug_draft(
                confirmed_cluster,
                "Recurring checkout crash",
                "description",
                "High",
                0.5,
            )
            .await
            .expect("insert bug draft");
        store
            .review_bug_draft(confirmed_draft, "CONFIRMED", "tester")
            .await
            .expect("review bug draft");

        // A DISMISSED bug draft — a human already judged this NOT a real
        // defect pattern; must not appear.
        let (dismissed_cluster, _) = store
            .upsert_failure_cluster("proj-1", "sig-dismissed", "hypothesis", 0.5)
            .await
            .expect("upsert failure cluster");
        let dismissed_draft = store
            .insert_bug_draft(
                dismissed_cluster,
                "Dismissed as environment noise",
                "description",
                "Medium",
                0.4,
            )
            .await
            .expect("insert bug draft");
        store
            .review_bug_draft(dismissed_draft, "DISMISSED", "tester")
            .await
            .expect("review bug draft");

        // A still-unreviewed (NEEDS_REVIEW) bug draft — unverified, must not
        // appear either.
        let (needs_review_cluster, _) = store
            .upsert_failure_cluster("proj-1", "sig-needs-review", "hypothesis", 0.5)
            .await
            .expect("upsert failure cluster");
        store
            .insert_bug_draft(
                needs_review_cluster,
                "Unreviewed recurring timeout",
                "description",
                "Medium",
                0.3,
            )
            .await
            .expect("insert bug draft");

        let intel = Intelligence::new(&store);
        let ctx = intel
            .load_feedback_context("proj-1")
            .await
            .expect("load_feedback_context");

        let risky_names: Vec<&str> = ctx.risky_areas.iter().map(|r| r.name.as_str()).collect();
        assert!(
            risky_names.contains(&"Checkout Flow"),
            "the real risky node must appear: {risky_names:?}"
        );
        assert!(
            !risky_names.contains(&"About Page"),
            "an ordinary-risk node must not appear: {risky_names:?}"
        );
        assert_eq!(
            ctx.risky_areas.iter().filter(|r| r.name == "Checkout Flow").count(),
            1,
            "must reflect only this project's own 'Checkout Flow', not proj-2's same-named node too"
        );

        assert_eq!(
            ctx.confirmed_defect_patterns,
            vec!["Recurring checkout crash".to_string()],
            "only the CONFIRMED draft may appear, never NEEDS_REVIEW or DISMISSED"
        );
    }

    // ------------------------------------------------- AI-assisted root cause

    #[test]
    fn bug_analysis_prompt_carries_the_real_signature_hypothesis_and_occurrence_count() {
        let prompt = build_bug_analysis_prompt(
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            4,
        );
        assert!(prompt.contains("connect econnrefused #.#.#.#:#"));
        assert!(prompt.contains("Target service was unreachable"));
        assert!(prompt.contains('4'));
        assert!(prompt
            .to_lowercase()
            .contains("never invent a specific file name"));
    }

    #[test]
    fn bug_analysis_prompt_differs_by_real_signature_not_just_by_occurrence_count() {
        let timeout_prompt = build_bug_analysis_prompt(
            "operation exceeded its timeout threshold",
            "Operation exceeded its timeout threshold",
            3,
        );
        let refused_prompt = build_bug_analysis_prompt(
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            3,
        );
        assert_ne!(timeout_prompt, refused_prompt);
    }

    #[test]
    fn ai_hypothesis_response_schema_rejects_missing_hypothesis_field() {
        let parsed: Result<AiHypothesisResponse, _> =
            serde_json::from_str(r#"{"not_hypothesis":"x"}"#);
        assert!(
            parsed.is_err(),
            "a response missing the required 'hypothesis' field must fail schema validation"
        );
    }

    // --------------------------------------------- central-router integration

    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
    };
    use crate::ai_router::{CapabilityLevel, ExecutionError};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn infra_with_executor_response(response: &str) -> AiInfrastructure {
        let mut executors: HashMap<&'static str, Arc<dyn crate::ai_router::AiExecutor>> =
            HashMap::new();
        executors.insert("mock", Arc::new(FakeExecutor::ok(response)));
        AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        )
    }

    fn infra_with_executor_error(err: ExecutionError) -> AiInfrastructure {
        let mut executors: HashMap<&'static str, Arc<dyn crate::ai_router::AiExecutor>> =
            HashMap::new();
        executors.insert("mock", Arc::new(FakeExecutor::err(err)));
        AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        )
    }

    /// ACC-018 Verification item 1 — "Local model success."
    #[tokio::test]
    async fn integration_real_routing_to_bug_analysis_returns_ai_outcome_on_success() {
        let infra = infra_with_executor_response(
            r#"{"hypothesis":"The target dependency's health check was still warming up when this run started."}"#,
        );
        let outcome = generate_ai_hypothesis(
            &infra,
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            3,
        )
        .await;
        match outcome {
            AiHypothesisOutcome::Ai {
                hypothesis,
                provider,
                model,
            } => {
                assert_eq!(hypothesis, "The target dependency's health check was still warming up when this run started.");
                assert_eq!(provider, "mock");
                assert_eq!(model, "mock-model");
            }
            AiHypothesisOutcome::Failed { reason } => {
                panic!("expected Ai outcome, got Failed: {reason}")
            }
        }
    }

    /// ACC-018 Verification item 3 — "Model timeout." Never a fabricated
    /// hypothesis on a real timeout, and the real, specific reason ("TIMEOUT")
    /// is preserved, not swallowed into a generic message.
    #[tokio::test]
    async fn integration_real_timeout_reports_truthful_failure_never_fabricated_hypothesis() {
        let infra = infra_with_executor_error(ExecutionError::Timeout);
        let outcome = generate_ai_hypothesis(
            &infra,
            "operation exceeded its timeout threshold",
            "Operation exceeded its timeout threshold",
            2,
        )
        .await;
        match outcome {
            AiHypothesisOutcome::Failed { reason } => {
                assert!(reason.contains("TIMEOUT"), "reason={reason}")
            }
            AiHypothesisOutcome::Ai { .. } => {
                panic!("a real timeout must never produce a fabricated Ai outcome")
            }
        }
    }

    /// ACC-018 Verification item 4 — "Model unavailable." No provider at
    /// all is registered, so routing itself fails before any execution is
    /// even attempted.
    #[tokio::test]
    async fn integration_no_provider_available_reports_no_compatible_model_never_fabricated_hypothesis(
    ) {
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        let outcome = generate_ai_hypothesis(
            &infra,
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            2,
        )
        .await;
        match outcome {
            AiHypothesisOutcome::Failed { reason } => {
                assert!(reason.contains("NoCompatibleModel"), "reason={reason}")
            }
            AiHypothesisOutcome::Ai { .. } => {
                panic!("no provider exists at all — this must never fabricate a success")
            }
        }
    }

    #[tokio::test]
    async fn integration_invalid_structured_output_reports_truthful_failure() {
        let infra = infra_with_executor_response("not json at all");
        let outcome = generate_ai_hypothesis(
            &infra,
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            2,
        )
        .await;
        assert!(matches!(outcome, AiHypothesisOutcome::Failed { .. }));
    }

    #[tokio::test]
    async fn integration_empty_hypothesis_string_is_treated_as_a_real_failure_not_a_blank_success()
    {
        let infra = infra_with_executor_response(r#"{"hypothesis":"   "}"#);
        let outcome = generate_ai_hypothesis(
            &infra,
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            2,
        )
        .await;
        assert!(
            matches!(outcome, AiHypothesisOutcome::Failed { .. }),
            "a blank hypothesis must never be accepted as a real Ai outcome"
        );
    }

    /// ACC-018 Verification item 2 — "API fallback." A genuine second
    /// provider, real AUTO-mode fallback (reused from `AiInfrastructure`
    /// as-is, not a parallel fallback mechanism written for this task).
    #[tokio::test]
    async fn integration_falls_back_to_the_next_best_model_when_the_primary_fails() {
        let mut executors: HashMap<&'static str, Arc<dyn crate::ai_router::AiExecutor>> =
            HashMap::new();
        executors.insert(
            "primary",
            Arc::new(FakeExecutor::err(ExecutionError::Timeout)),
        );
        executors.insert(
            "backup",
            Arc::new(FakeExecutor::ok(
                r#"{"hypothesis":"A genuinely different, backup-sourced hypothesis."}"#,
            )),
        );
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider(
                    "primary",
                    vec![healthy_model(
                        "primary-model",
                        "primary",
                        CapabilityLevel::Strong,
                    )],
                ))),
                Arc::new(FakeDiscovery(healthy_provider(
                    "backup",
                    vec![healthy_model(
                        "backup-model",
                        "backup",
                        CapabilityLevel::Basic,
                    )],
                ))),
            ],
            executors,
        );
        infra
            .set_settings(crate::ai_router::AiSettings {
                mode: crate::ai_router::RoutingMode::Auto,
                preferred_provider: Some("primary".to_string()),
                preferred_model: None,
                fallback_enabled: true,
                task_overrides: std::collections::HashMap::new(),
            })
            .await;

        let outcome = generate_ai_hypothesis(
            &infra,
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            2,
        )
        .await;
        match outcome {
            AiHypothesisOutcome::Ai {
                hypothesis,
                provider,
                ..
            } => {
                assert_eq!(
                    provider, "backup",
                    "must genuinely fall back to the real, different, working provider"
                );
                assert_eq!(
                    hypothesis,
                    "A genuinely different, backup-sourced hypothesis."
                );
            }
            AiHypothesisOutcome::Failed { reason } => {
                panic!("expected a real fallback to succeed, got Failed: {reason}")
            }
        }
    }

    /// ACC-018 Verification item 5 — "Fallback failure." Both the primary
    /// and the only fallback candidate fail — a real, honest `Failed`, never
    /// a fabricated success synthesized from neither.
    #[tokio::test]
    async fn integration_reports_truthful_failure_when_every_candidate_including_fallback_fails() {
        let mut executors: HashMap<&'static str, Arc<dyn crate::ai_router::AiExecutor>> =
            HashMap::new();
        executors.insert(
            "primary",
            Arc::new(FakeExecutor::err(ExecutionError::Timeout)),
        );
        executors.insert(
            "backup",
            Arc::new(FakeExecutor::err(ExecutionError::ProviderUnavailable(
                "backup down too".to_string(),
            ))),
        );
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider(
                    "primary",
                    vec![healthy_model(
                        "primary-model",
                        "primary",
                        CapabilityLevel::Strong,
                    )],
                ))),
                Arc::new(FakeDiscovery(healthy_provider(
                    "backup",
                    vec![healthy_model(
                        "backup-model",
                        "backup",
                        CapabilityLevel::Basic,
                    )],
                ))),
            ],
            executors,
        );
        infra
            .set_settings(crate::ai_router::AiSettings {
                mode: crate::ai_router::RoutingMode::Auto,
                preferred_provider: Some("primary".to_string()),
                preferred_model: None,
                fallback_enabled: true,
                task_overrides: std::collections::HashMap::new(),
            })
            .await;

        let outcome = generate_ai_hypothesis(
            &infra,
            "connect econnrefused #.#.#.#:#",
            "Target service was unreachable",
            2,
        )
        .await;
        assert!(
            matches!(outcome, AiHypothesisOutcome::Failed { .. }),
            "both candidates genuinely failed — this must never fabricate a success from neither"
        );
    }

    #[test]
    fn required_capabilities_for_bug_analysis_are_real_and_unchanged_by_this_unit() {
        // Confirms the pre-existing `AiTask::BugAnalysis` capability
        // requirement (defined in an earlier phase, per `ai_router.rs`) is
        // exactly what this unit's routing tests above already rely on —
        // not silently redefined here.
        let required = crate::ai_router::required_capabilities(AiTask::BugAnalysis);
        assert_eq!(
            required,
            vec![
                crate::ai_router::RequiredCapability::TextGeneration,
                crate::ai_router::RequiredCapability::Reasoning
            ]
        );
    }

    // ---------------------------------------------------------------- live

    /// Genuinely live proof against a real, running local Ollama server —
    /// not part of the default `cargo test` run (`#[ignore]`, same as
    /// `ai_generation.rs::live_ollama_different_skills_produce_different_real_coverage`,
    /// which this test mirrors exactly). Run manually with
    /// `cargo test --lib live_ollama -- --ignored --nocapture` against a
    /// real instance. Proves two genuinely different real failure clusters
    /// (their signatures/hypotheses produced by the real, unchanged
    /// `normalize_signature`/`classify_failure` functions above, over two
    /// different real example failure texts — not hand-picked strings) get
    /// genuinely different AI-generated hypothesis text back, end-to-end
    /// through the real `AiInfrastructure`/discovery/router — not a fixed
    /// template with a field substituted in.
    #[tokio::test]
    #[ignore = "requires a real, running local Ollama server — not run automatically this session, see the report's Evidence section"]
    async fn live_ollama_different_failure_clusters_produce_different_real_hypotheses() {
        let connection_refused_reason = "connect ECONNREFUSED 127.0.0.1:8080";
        let timeout_reason = "Timeout 30000ms exceeded";
        let (connection_signature, connection_hypothesis) = {
            let sig = normalize_signature(connection_refused_reason);
            let (h, _) = classify_failure(connection_refused_reason);
            (sig, h)
        };
        let (timeout_signature, timeout_hypothesis) = {
            let sig = normalize_signature(timeout_reason);
            let (h, _) = classify_failure(timeout_reason);
            (sig, h)
        };
        assert_ne!(
            connection_signature, timeout_signature,
            "the two real example failures must genuinely normalize to different signatures"
        );

        let infra = AiInfrastructure::from_env();
        infra.discover_all().await;

        let connection_outcome =
            generate_ai_hypothesis(&infra, &connection_signature, &connection_hypothesis, 3).await;
        let timeout_outcome =
            generate_ai_hypothesis(&infra, &timeout_signature, &timeout_hypothesis, 3).await;

        let connection_text = match connection_outcome {
            AiHypothesisOutcome::Ai {
                hypothesis,
                provider,
                model,
            } => {
                println!("CONNECTION-REFUSED via {provider}/{model}: {hypothesis}");
                hypothesis
            }
            AiHypothesisOutcome::Failed { reason } => {
                panic!("live Ollama call failed (connection-refused cluster): {reason}")
            }
        };
        let timeout_text = match timeout_outcome {
            AiHypothesisOutcome::Ai {
                hypothesis,
                provider,
                model,
            } => {
                println!("TIMEOUT via {provider}/{model}: {hypothesis}");
                hypothesis
            }
            AiHypothesisOutcome::Failed { reason } => {
                panic!("live Ollama call failed (timeout cluster): {reason}")
            }
        };

        assert_ne!(
            connection_text.to_lowercase(),
            timeout_text.to_lowercase(),
            "two genuinely different real failure clusters must produce genuinely different AI hypothesis text, not the same text (or a fixed template) twice"
        );
    }
}
