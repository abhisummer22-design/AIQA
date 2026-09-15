//! Specialized Agent 2: Self-Healing Agent (`SelfHealingAgent`).
//!
//! Analyzes failed test selector and assertion evidence using `dom_prober` to diagnose
//! root causes (renamed IDs, role visibility differentials, altered classes) and proposes
//! healed selector candidates without modifying code or re-executing test suites.
//!
//! Architectural Guarantees:
//! - Multi-turn reasoning leveraging `dom_prober`.
//! - Strict multi-tenant project scoping (`context.is_authorized_for_project`).
//! - Diagnostic and recommendation only: NEVER modifies files, NEVER executes tests.
//! - Confidence-scored candidate recommendations with evidence.
//! - Zero private chain-of-thought leakage.

use crate::agent::{Agent, AgentContext, AgentError, TurnDecision};
use crate::agent_tools::{AgentToolRegistry, ToolInvocation};
use crate::ai_router::AiInfrastructure;
use crate::tools::security::sanitize_secrets;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const AGENT_ID: &str = "self_healing_agent";
const AGENT_NAME: &str = "Self-Healing Agent";
const AGENT_TYPE: &str = "qa_self_healing";

/// Input failure evidence supplied for diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureEvidence {
    pub project_id: String,
    #[serde(default)]
    pub test_case_id: Option<String>,
    #[serde(default)]
    pub test_execution_id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    pub failed_selector: String,
    #[serde(default)]
    pub expected_behavior: Option<String>,
    #[serde(default)]
    pub actual_behavior: Option<String>,
    #[serde(default)]
    pub failure_message: Option<String>,
    #[serde(default)]
    pub crawl_task_id: Option<String>,
    #[serde(default)]
    pub crawl_output: Option<Value>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub surrounding_context: Option<String>,
}

/// A ranked alternative selector candidate discovered during DOM probing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealedCandidate {
    pub candidate_selector: String,
    pub element_kind: String,
    pub element_text: String,
    pub confidence: String, // HIGH, MEDIUM, LOW
    pub confidence_score: f64,
    pub page_url: String,
    pub role: String,
    pub evidence: String,
    pub reason: String,
}

/// Diagnostic self-healing outcome report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingOutput {
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_case_id: Option<String>,
    pub original_selector: String,
    pub failure_classification: String,
    pub element_exists: bool,
    pub candidates: Vec<HealedCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_selector: Option<String>,
    pub confidence: String,
    pub evidence: String,
    pub reason: String,
    pub recommendation: String,
    pub requires_review: bool,
}

/// Self-Healing Agent implementation.
#[derive(Debug, Clone)]
pub struct SelfHealingAgent {
    pub evidence: FailureEvidence,
}

impl SelfHealingAgent {
    pub fn new(evidence: FailureEvidence) -> Self {
        Self { evidence }
    }
}

#[async_trait]
impl Agent for SelfHealingAgent {
    fn id(&self) -> &str {
        AGENT_ID
    }

    fn name(&self) -> &str {
        AGENT_NAME
    }

    fn agent_type(&self) -> &str {
        AGENT_TYPE
    }

    async fn step(
        &self,
        context: &AgentContext,
        tools: &AgentToolRegistry,
        _ai: &AiInfrastructure,
    ) -> Result<TurnDecision, AgentError> {
        // 1. Cooperative cancellation check
        if context.is_cancelled() {
            return Err(AgentError::Cancelled(
                "self-healing cancelled by caller".to_string(),
            ));
        }

        // 2. Multi-tenant Authorization & Project Scope Enforcement
        if !context.is_authorized_for_project(&self.evidence.project_id) {
            return Ok(TurnDecision::Fail {
                reason: format!(
                    "cross-project access violation: agent execution context is unauthorized for project '{}'",
                    self.evidence.project_id
                ),
            });
        }

        let failed_sel = self.evidence.failed_selector.trim();
        if failed_sel.is_empty() {
            return Ok(TurnDecision::Fail {
                reason: "failed_selector must not be empty".to_string(),
            });
        }

        // 3. Inspect context history for prior DOM Prober observations
        let mut direct_probe_obs: Option<&Value> = None;
        let mut candidate_probe_obs: Option<&Value> = None;

        for record in &context.history {
            for obs in &record.tool_observations {
                if obs.tool_name == "dom_prober" {
                    if let Ok(ref val) = obs.output {
                        if direct_probe_obs.is_none() {
                            direct_probe_obs = Some(val);
                        } else {
                            candidate_probe_obs = Some(val);
                        }
                    }
                }
            }
        }

        // Turn Phase A: Probe the original selector directly
        if direct_probe_obs.is_none() && tools.has_tool("dom_prober") {
            return Ok(TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: format!("dom-direct-{}", context.current_turn),
                    tool_name: "dom_prober".to_string(),
                    arguments: json!({
                        "selector": failed_sel,
                        "query_type": "all",
                        "crawl_task_id": self.evidence.crawl_task_id,
                        "crawl_output": self.evidence.crawl_output,
                        "url_filter": self.evidence.url,
                    }),
                }],
            });
        }

        // Turn Phase B: If selector does not exist, probe for candidate elements
        let element_exists = direct_probe_obs
            .and_then(|obs| obs.get("exists"))
            .and_then(|e| e.as_bool())
            .unwrap_or(false);

        if !element_exists && candidate_probe_obs.is_none() && tools.has_tool("dom_prober") {
            // Extract keyword or tag from the failed selector to find candidate replacements
            let search_term = extract_search_keyword(failed_sel);
            if !search_term.is_empty() {
                return Ok(TurnDecision::CallTools {
                    invocations: vec![ToolInvocation {
                        call_id: format!("dom-candidate-{}", context.current_turn),
                        tool_name: "dom_prober".to_string(),
                        arguments: json!({
                            "selector": search_term,
                            "query_type": "context",
                            "crawl_task_id": self.evidence.crawl_task_id,
                            "crawl_output": self.evidence.crawl_output,
                            "url_filter": self.evidence.url,
                            "max_results": 25
                        }),
                    }],
                });
            }
        }

        // Turn Phase C: Diagnose failure and synthesize healing recommendation
        let diagnosis = diagnose_failure(&self.evidence, direct_probe_obs, candidate_probe_obs);

        let mut diag_json = serde_json::to_value(&diagnosis).map_err(|e| {
            AgentError::InvalidOutput(format!("failed to serialize self-healing result: {e}"))
        })?;

        sanitize_secrets(&mut diag_json);

        Ok(TurnDecision::Complete { output: diag_json })
    }
}

/// Diagnoses the failure and ranks alternative candidate selectors.
fn diagnose_failure(
    evidence: &FailureEvidence,
    direct_obs: Option<&Value>,
    candidate_obs: Option<&Value>,
) -> SelfHealingOutput {
    let failed_sel = evidence.failed_selector.trim();

    // Check if original element existed
    let direct_exists = direct_obs
        .and_then(|o| o.get("exists"))
        .and_then(|e| e.as_bool())
        .unwrap_or(false);

    // Scenario 1: Ambiguous matches found (multiple matching elements on the same page/role)
    let ambiguous_count = if let Some(elems) = direct_obs
        .and_then(|o| o.get("matching_elements"))
        .and_then(|e| e.as_array())
    {
        if let Some(ref target_role) = evidence.role {
            elems
                .iter()
                .filter(|e| {
                    e.get("role")
                        .and_then(|r| r.as_str())
                        .map_or(false, |r| r == target_role)
                })
                .count()
        } else {
            let mut page_counts = std::collections::HashMap::new();
            for e in elems {
                let r = e.get("role").and_then(|v| v.as_str()).unwrap_or("");
                let p = e.get("page_url").and_then(|v| v.as_str()).unwrap_or("");
                *page_counts.entry((r, p)).or_insert(0usize) += 1;
            }
            page_counts.values().copied().max().unwrap_or(0)
        }
    } else {
        0
    };

    if ambiguous_count > 1 {
        let reason = format!(
            "Selector '{failed_sel}' is ambiguous: matches {ambiguous_count} distinct elements on the page"
        );
        return SelfHealingOutput {
            project_id: evidence.project_id.clone(),
            test_case_id: evidence.test_case_id.clone(),
            original_selector: failed_sel.to_string(),
            failure_classification: "AmbiguousSelector".to_string(),
            element_exists: true,
            candidates: Vec::new(),
            recommended_selector: None,
            confidence: "LOW".to_string(),
            evidence: format!("dom_prober: ambiguous_count = {ambiguous_count}"),
            reason,
            recommendation:
                "Refine selector with parent container context or unique data attribute".to_string(),
            requires_review: true,
        };
    }

    // Scenario 2: Element exists, but role visibility differential occurred
    if direct_exists {
        if let Some(vis) = direct_obs.and_then(|o| o.get("visibility_details")) {
            let differential = vis
                .get("role_access_differential")
                .and_then(|d| d.as_bool())
                .unwrap_or(false);

            if differential {
                let absent_roles: Vec<String> = vis
                    .get("absent_in_roles")
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(ToString::to_string))
                            .collect()
                    })
                    .unwrap_or_default();

                let visible_roles: Vec<String> = vis
                    .get("visible_in_roles")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(ToString::to_string))
                            .collect()
                    })
                    .unwrap_or_default();

                if let Some(ref role) = evidence.role {
                    if absent_roles.contains(role) {
                        return SelfHealingOutput {
                            project_id: evidence.project_id.clone(),
                            test_case_id: evidence.test_case_id.clone(),
                            original_selector: failed_sel.to_string(),
                            failure_classification: "RoleGatedVisibilityDifferential".to_string(),
                            element_exists: true,
                            candidates: Vec::new(),
                            recommended_selector: Some(failed_sel.to_string()),
                            confidence: "HIGH".to_string(),
                            evidence: format!(
                                "dom_prober: element visible to {:?} but absent for role '{role}'",
                                visible_roles
                            ),
                            reason: format!(
                                "Element exists in DOM but is protected by role-gated visibility; hidden from role '{role}'"
                            ),
                            recommendation: "Verify user permissions or adjust test authentication to an authorized role".to_string(),
                            requires_review: true,
                        };
                    }
                }
            }
        }

        // Element exists and is visible; failure was likely an assertion or timing issue
        return SelfHealingOutput {
            project_id: evidence.project_id.clone(),
            test_case_id: evidence.test_case_id.clone(),
            original_selector: failed_sel.to_string(),
            failure_classification: "TimingOrAssertionMismatch".to_string(),
            element_exists: true,
            candidates: Vec::new(),
            recommended_selector: Some(failed_sel.to_string()),
            confidence: "HIGH".to_string(),
            evidence: "dom_prober: element found and visible in DOM snapshot".to_string(),
            reason:
                "Target element genuinely exists in DOM; failure was not caused by broken selector"
                    .to_string(),
            recommendation:
                "Inspect assertion criteria, page load timing, or dynamic network latency"
                    .to_string(),
            requires_review: true,
        };
    }

    // Scenario 3: Element does NOT exist; search candidate elements
    let mut candidates = Vec::new();

    if let Some(cand_obs) = candidate_obs {
        if let Some(elems) = cand_obs.get("matching_elements").and_then(|e| e.as_array()) {
            for elem in elems {
                let hint = elem
                    .get("selector_hint")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let kind = elem.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                let text = elem.get("text").and_then(|t| t.as_str()).unwrap_or("");
                let page_url = elem.get("page_url").and_then(|u| u.as_str()).unwrap_or("");
                let role = elem.get("role").and_then(|r| r.as_str()).unwrap_or("");

                if hint.is_empty() || hint == failed_sel {
                    continue;
                }

                let score = calculate_similarity(
                    failed_sel,
                    kind,
                    text,
                    hint,
                    evidence.expected_behavior.as_deref(),
                );
                let confidence_label = if score >= 0.80 {
                    "HIGH"
                } else if score >= 0.50 {
                    "MEDIUM"
                } else {
                    "LOW"
                };

                candidates.push(HealedCandidate {
                    candidate_selector: hint.to_string(),
                    element_kind: kind.to_string(),
                    element_text: text.to_string(),
                    confidence: confidence_label.to_string(),
                    confidence_score: score,
                    page_url: page_url.to_string(),
                    role: role.to_string(),
                    evidence: format!(
                        "dom_prober: found {kind} '{text}' with selector hint '{hint}'"
                    ),
                    reason: format!("Element matches action intent and {kind} element role"),
                });
            }
        }
    }

    // Sort candidates by confidence score descending
    candidates.sort_by(|a, b| {
        b.confidence_score
            .partial_cmp(&a.confidence_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.dedup_by(|a, b| a.candidate_selector == b.candidate_selector);

    if let Some(best) = candidates.first() {
        let best_sel = best.candidate_selector.clone();
        let best_kind = best.element_kind.clone();
        let best_conf = best.confidence.clone();
        let best_score = best.confidence_score;
        let best_ev = best.evidence.clone();
        let is_high_conf = best_score >= 0.80;
        let recommendation = format!("Update selector from '{failed_sel}' to '{best_sel}'");
        let reason = format!(
            "Original selector not found; candidate element '{best_sel}' ({best_kind}) matched with {:.0}% confidence",
            best_score * 100.0
        );

        SelfHealingOutput {
            project_id: evidence.project_id.clone(),
            test_case_id: evidence.test_case_id.clone(),
            original_selector: failed_sel.to_string(),
            failure_classification: "SelectorRenamedOrRelocated".to_string(),
            element_exists: false,
            candidates,
            recommended_selector: Some(best_sel),
            confidence: best_conf,
            evidence: best_ev,
            reason,
            recommendation,
            requires_review: !is_high_conf,
        }
    } else {
        SelfHealingOutput {
            project_id: evidence.project_id.clone(),
            test_case_id: evidence.test_case_id.clone(),
            original_selector: failed_sel.to_string(),
            failure_classification: "SelectorNotFound".to_string(),
            element_exists: false,
            candidates: Vec::new(),
            recommended_selector: None,
            confidence: "LOW".to_string(),
            evidence: "dom_prober: no matching elements found in crawler snapshots".to_string(),
            reason: format!("Selector '{failed_sel}' does not exist and no plausible candidate elements were found"),
            recommendation: "Perform fresh crawl or verify element has not been removed from application".to_string(),
            requires_review: true,
        }
    }
}

/// Calculates similarity between failed selector attributes and a candidate element.
fn calculate_similarity(
    failed_sel: &str,
    cand_kind: &str,
    cand_text: &str,
    cand_hint: &str,
    expected_behavior: Option<&str>,
) -> f64 {
    let mut score: f64 = 0.0;
    let failed_lower = failed_sel.to_lowercase();
    let text_lower = cand_text.to_lowercase();
    let hint_lower = cand_hint.to_lowercase();

    // 1. Kind / Tag correlation
    if failed_lower.contains(cand_kind)
        || (cand_kind == "button"
            && (failed_lower.contains("btn") || failed_lower.contains("button")))
        || (cand_kind == "link"
            && (failed_lower.contains("link")
                || failed_lower.contains("anchor")
                || failed_lower.contains("a")))
        || (cand_kind == "input"
            && (failed_lower.contains("input")
                || failed_lower.contains("field")
                || failed_lower.contains("txt")))
    {
        score += 0.25;
    }

    // 2. Text / Label correlation
    let search_term = extract_search_keyword(failed_sel).to_lowercase();
    if !search_term.is_empty()
        && (text_lower.contains(&search_term) || hint_lower.contains(&search_term))
    {
        score += 0.45;
    }

    // 3. Expected behavior hint & word overlap
    if let Some(exp) = expected_behavior {
        let exp_lower = exp.to_lowercase();
        if text_lower.contains(&exp_lower) || exp_lower.contains(&text_lower) {
            score += 0.25;
        } else {
            let exp_words: Vec<&str> = exp_lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 2)
                .collect();
            let mut word_matches = 0;
            for w in &exp_words {
                if text_lower.contains(w) || hint_lower.contains(w) {
                    word_matches += 1;
                }
            }
            if word_matches >= 2 || (word_matches == 1 && exp_words.len() <= 2) {
                score += 0.25;
            }
        }
    }

    // 4. Structural ID / Class pattern similarity
    if failed_lower.starts_with('#') && hint_lower.starts_with('#') {
        score += 0.15;
    } else if failed_lower.starts_with('.') && hint_lower.starts_with('.') {
        score += 0.10;
    }

    score.clamp(0.0, 0.98)
}

/// Extracts a clean search keyword from a CSS selector.
fn extract_search_keyword(selector: &str) -> String {
    let trimmed = selector.trim();
    if let Some(id_part) = trimmed.strip_prefix('#') {
        // e.g. "#delete-btn" -> "delete"
        id_part
            .split(|c| c == '-' || c == '_')
            .next()
            .unwrap_or(id_part)
            .to_string()
    } else if let Some(class_part) = trimmed.strip_prefix('.') {
        class_part
            .split(|c| c == '-' || c == '_')
            .next()
            .unwrap_or(class_part)
            .to_string()
    } else if let Some(text_part) = trimmed.strip_prefix("text=") {
        text_part.to_string()
    } else if trimmed.starts_with("button") {
        "button".to_string()
    } else if trimmed.starts_with("input") {
        "input".to_string()
    } else if trimmed.starts_with('a') {
        "a".to_string()
    } else {
        trimmed.chars().filter(|c| c.is_alphanumeric()).collect()
    }
}
