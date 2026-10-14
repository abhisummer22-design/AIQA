//! AIQA Phase 10 (Batch 1) — QA Copilot: Intent Classification & Query Dispatch Architecture.
//!
//! # Roadmap Reference
//! `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §"PHASE 10 — AI QA COPILOT" (L369–431)
//!
//! ## Core Objective
//! Provide a typed, deterministic conversational query intent classification and dispatch planning
//! layer over existing QA evidence without creating another memory database.
//!
//! ## Purity and Boundary Invariants
//! 1. **Zero Database Access:** No SQL, SQLite connections, transactions, or migrations.
//! 2. **Zero External AI / Model Calls:** Classification is 100% deterministic, rule- and
//!    pattern-driven. No LLM or remote API is invoked.
//! 3. **Stateless Classification:** Pure functions taking string references and returning typed
//!    intent structures without background tasks, locks, or side effects.
//! 4. **Strict Dispatch Planning:** Each classified intent maps directly to the specific
//!    existing evidence readers required by future Phase 10 batches.
//!
//! ## Eight Canonical Questions
//! 1. "Why did this release fail?" (`CopilotIntent::ReleaseFailureReason`)
//! 2. "What should I test next?" (`CopilotIntent::NextTestRecommendation`)
//! 3. "Which changes affect my regression suite?" (`CopilotIntent::RegressionImpact`)
//! 4. "Why is this test high risk?" (`CopilotIntent::TestRiskExplanation`)
//! 5. "Which failures are recurring?" (`CopilotIntent::RecurringFailures`)
//! 6. "Which requirements lack test coverage?" (`CopilotIntent::RequirementCoverageGaps`)
//! 7. "Why was this finding classified as insufficient evidence?" (`CopilotIntent::InsufficientEvidenceReason`)
//! 8. "What is blocking release readiness?" (`CopilotIntent::ReleaseReadinessBlockers`)

use serde::{Deserialize, Serialize};

/// Maximum allowed character length for a user-submitted Copilot question.
pub const MAX_QUERY_LEN: usize = 1000;

/// The eight roadmap-defined QA Copilot query intents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "parameters",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum CopilotIntent {
    /// 1. "Why did this release fail?"
    ReleaseFailureReason { release_id: Option<String> },
    /// 2. "What should I test next?"
    NextTestRecommendation,
    /// 3. "Which changes affect my regression suite?"
    RegressionImpact { change_ref: Option<String> },
    /// 4. "Why is this test high risk?"
    TestRiskExplanation { test_case_id: Option<String> },
    /// 5. "Which failures are recurring?"
    RecurringFailures,
    /// 6. "Which requirements lack test coverage?"
    RequirementCoverageGaps,
    /// 7. "Why was this finding classified as insufficient evidence?"
    InsufficientEvidenceReason {
        finding_id: Option<String>,
        run_id: Option<String>,
    },
    /// 8. "What is blocking release readiness?"
    ReleaseReadinessBlockers { release_id: Option<String> },
}

impl CopilotIntent {
    /// The canonical question phrasing defined by the Phase 10 roadmap.
    pub fn canonical_question(&self) -> &'static str {
        match self {
            Self::ReleaseFailureReason { .. } => "Why did this release fail?",
            Self::NextTestRecommendation => "What should I test next?",
            Self::RegressionImpact { .. } => "Which changes affect my regression suite?",
            Self::TestRiskExplanation { .. } => "Why is this test high risk?",
            Self::RecurringFailures => "Which failures are recurring?",
            Self::RequirementCoverageGaps => "Which requirements lack test coverage?",
            Self::InsufficientEvidenceReason { .. } => {
                "Why was this finding classified as insufficient evidence?"
            }
            Self::ReleaseReadinessBlockers { .. } => "What is blocking release readiness?",
        }
    }

    /// Internal string key identifying the intent kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReleaseFailureReason { .. } => "RELEASE_FAILURE_REASON",
            Self::NextTestRecommendation => "NEXT_TEST_RECOMMENDATION",
            Self::RegressionImpact { .. } => "REGRESSION_IMPACT",
            Self::TestRiskExplanation { .. } => "TEST_RISK_EXPLANATION",
            Self::RecurringFailures => "RECURRING_FAILURES",
            Self::RequirementCoverageGaps => "REQUIREMENT_COVERAGE_GAPS",
            Self::InsufficientEvidenceReason { .. } => "INSUFFICIENT_EVIDENCE_REASON",
            Self::ReleaseReadinessBlockers { .. } => "RELEASE_READINESS_BLOCKERS",
        }
    }

    /// Generates the dispatch plan specifying which existing evidence readers are required.
    pub fn dispatch_plan(&self) -> CopilotDispatchPlan {
        let required_readers = match self {
            Self::ReleaseFailureReason { .. } => vec![EvidenceReaderKind::ReleaseReadiness],
            Self::NextTestRecommendation => vec![
                EvidenceReaderKind::ChangeImpact,
                EvidenceReaderKind::QaMemoryRisk,
            ],
            Self::RegressionImpact { .. } => vec![EvidenceReaderKind::ChangeImpact],
            Self::TestRiskExplanation { .. } => vec![EvidenceReaderKind::QaMemoryRisk],
            Self::RecurringFailures => vec![EvidenceReaderKind::FailureClusters],
            Self::RequirementCoverageGaps => vec![EvidenceReaderKind::RequirementsCoverage],
            Self::InsufficientEvidenceReason { .. } => vec![
                EvidenceReaderKind::IntegrityVerifier,
                EvidenceReaderKind::TestQuality,
            ],
            Self::ReleaseReadinessBlockers { .. } => vec![EvidenceReaderKind::ReleaseReadiness],
        };

        CopilotDispatchPlan {
            intent: self.clone(),
            required_readers,
            project_scoped: true,
        }
    }
}

/// Identifies the existing evidence subsystem needed to answer a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceReaderKind {
    ReleaseReadiness,
    ChangeImpact,
    QaMemoryRisk,
    FailureClusters,
    RequirementsCoverage,
    IntegrityVerifier,
    TestQuality,
}

/// A structured dispatch plan indicating which downstream evidence readers to invoke in Batch 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotDispatchPlan {
    pub intent: CopilotIntent,
    pub required_readers: Vec<EvidenceReaderKind>,
    pub project_scoped: bool,
}

/// The result of classifying a user question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClassificationResult {
    /// The question maps deterministically to one of the 8 canonical QA intents.
    Classified {
        intent: CopilotIntent,
        confidence: f64,
        normalized_query: String,
    },
    /// The question is incomplete or ambiguous and requires user clarification.
    ClarificationRequired {
        reason: String,
        suggested_clarification: String,
        normalized_query: String,
    },
    /// The question is empty, off-topic, adversarial, or outside supported QA capabilities.
    UnsupportedQuery {
        reason: String,
        normalized_query: String,
    },
}

/// Normalizes raw input: trims, lowercases, collapses whitespace, strips trailing punctuation.
pub fn normalize_query(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let mut last_was_space = false;

    for ch in raw.trim().chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(ch.to_ascii_lowercase());
            last_was_space = false;
        }
    }

    // Strip trailing punctuation
    while normalized.ends_with('?')
        || normalized.ends_with('.')
        || normalized.ends_with('!')
        || normalized.ends_with(';')
        || normalized.ends_with(':')
    {
        normalized.pop();
    }

    normalized.trim().to_string()
}

/// Checks for SQL injection, prompt injection, or script injection attempts.
fn detect_disallowed_patterns(normalized: &str) -> Option<&'static str> {
    let lower = normalized;

    // Disallowed SQL syntax patterns
    if lower.contains("drop table")
        || lower.contains("delete from")
        || lower.contains("union select")
        || lower.contains("insert into")
        || lower.contains("update ") && lower.contains(" set ")
        || lower.contains("--;")
        || lower.contains("/*")
    {
        return Some("Query contains disallowed database command syntax.");
    }

    // Disallowed prompt injection / instruction override attempts
    if lower.contains("ignore previous instructions")
        || lower.contains("ignore all instructions")
        || lower.contains("system prompt")
        || lower.contains("you are now")
        || lower.contains("bypass safety")
        || lower.contains("bypass authorization")
        || lower.contains("admin override")
    {
        return Some("Query contains instruction override or safety bypass pattern.");
    }

    // Disallowed script tag attempts
    if lower.contains("<script") || lower.contains("javascript:") {
        return Some("Query contains unsafe script tags.");
    }

    None
}

/// Extracts an optional entity token following a prefix word from the raw query, preserving entity casing.
fn extract_parameter(raw: &str, keywords: &[&str]) -> Option<String> {
    const STOP_WORDS: &[&str] = &[
        "the",
        "this",
        "that",
        "these",
        "those",
        "my",
        "our",
        "a",
        "an",
        "all",
        "each",
        "every",
        "some",
        "any",
        "no",
        "high",
        "failed",
        "fail",
        "for",
        "in",
        "to",
        "of",
        "is",
        "was",
        "readiness",
        "classified",
        "marked",
        "suite",
        "as",
    ];
    let lower_raw = raw.to_ascii_lowercase();
    for kw in keywords {
        if let Some(pos) = lower_raw.find(kw) {
            let after = &raw[pos + kw.len()..];
            let trimmed = after.trim_start();
            let token: String = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
                .collect();
            if !token.is_empty() && !STOP_WORDS.contains(&token.to_ascii_lowercase().as_str()) {
                return Some(token);
            }
        }
    }
    None
}

/// Deterministically classifies a user question into a `ClassificationResult`.
///
/// This function is pure, in-memory, deterministic, and performs zero I/O or network operations.
pub fn classify_question(raw: &str) -> ClassificationResult {
    if raw.trim().is_empty() {
        return ClassificationResult::UnsupportedQuery {
            reason: "Query cannot be empty or whitespace only.".to_string(),
            normalized_query: String::new(),
        };
    }

    if raw.len() > MAX_QUERY_LEN {
        return ClassificationResult::UnsupportedQuery {
            reason: format!(
                "Query length ({} chars) exceeds maximum allowed limit of {} chars.",
                raw.len(),
                MAX_QUERY_LEN
            ),
            normalized_query: normalize_query(&raw[..MAX_QUERY_LEN]),
        };
    }

    let normalized = normalize_query(raw);

    // Safety checks against injection patterns
    if let Some(disallowed_reason) = detect_disallowed_patterns(&normalized) {
        return ClassificationResult::UnsupportedQuery {
            reason: disallowed_reason.to_string(),
            normalized_query: normalized,
        };
    }

    // Ambiguous and underspecified query handling
    if normalized == "why did it fail"
        || normalized == "why did test fail"
        || normalized == "why did the test fail"
        || normalized == "why did run fail"
    {
        return ClassificationResult::ClarificationRequired {
            reason: "Ambiguous failure query: target not specified.".to_string(),
            suggested_clarification: "Specify whether you are asking 'Why did this release fail?' or 'Why was this finding classified as insufficient evidence?'.".to_string(),
            normalized_query: normalized,
        };
    }

    if normalized == "tell me about risk"
        || normalized == "what is the risk"
        || normalized == "show risk"
        || normalized == "risk status"
    {
        return ClassificationResult::ClarificationRequired {
            reason: "Ambiguous risk query: specify whether you are asking about test risk, release readiness, or impact scope.".to_string(),
            suggested_clarification: "Specify 'Why is this test high risk?' or 'What is blocking release readiness?'.".to_string(),
            normalized_query: normalized,
        };
    }

    if normalized == "show status" || normalized == "what is failing" || normalized == "what broke"
    {
        return ClassificationResult::ClarificationRequired {
            reason: "Underspecified query: specify whether you are asking about release failure, recurring failures, or readiness blockers.".to_string(),
            suggested_clarification: "Specify 'Why did this release fail?' or 'Which failures are recurring?'.".to_string(),
            normalized_query: normalized,
        };
    }

    // 1. "Why did this release fail?"
    if (normalized.starts_with("why did")
        || normalized.starts_with("why has")
        || normalized.starts_with("what caused"))
        && normalized.contains("release")
        && (normalized.contains("fail") || normalized.contains("failure"))
    {
        let release_id = extract_parameter(raw, &["release ", "release id ", "release tag "]);
        return ClassificationResult::Classified {
            intent: CopilotIntent::ReleaseFailureReason { release_id },
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // 8. "What is blocking release readiness?"
    if (normalized.contains("blocking")
        || normalized.contains("blocks")
        || normalized.contains("blocker"))
        && normalized.contains("release readiness")
    {
        let release_id = extract_parameter(raw, &["release ", "for release "]);
        return ClassificationResult::Classified {
            intent: CopilotIntent::ReleaseReadinessBlockers { release_id },
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // 2. "What should I test next?"
    if normalized.contains("what should i test next")
        || normalized.contains("what to test next")
        || normalized.contains("what should we test next")
        || normalized.contains("recommend next test")
        || normalized.contains("suggest next test")
        || normalized.contains("which test should i run next")
        || normalized == "what should i test"
        || normalized == "what to test"
    {
        return ClassificationResult::Classified {
            intent: CopilotIntent::NextTestRecommendation,
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // 3. "Which changes affect my regression suite?"
    if (normalized.contains("change") || normalized.contains("changes"))
        && (normalized.contains("affect")
            || normalized.contains("affects")
            || normalized.contains("impact")
            || normalized.contains("impacts"))
        && (normalized.contains("regression") || normalized.contains("suite"))
    {
        let change_ref = extract_parameter(raw, &["in pr ", "in commit ", "in ", "change ", "pr "]);
        return ClassificationResult::Classified {
            intent: CopilotIntent::RegressionImpact { change_ref },
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // 4. "Why is this test high risk?"
    if (normalized.starts_with("why is")
        || normalized.starts_with("why are")
        || normalized.starts_with("explain"))
        && (normalized.contains("test") || normalized.contains("test case"))
        && (normalized.contains("high risk")
            || normalized.contains("risky")
            || normalized.contains("risk score")
            || normalized.contains("risk high"))
    {
        let test_case_id = extract_parameter(raw, &["test case ", "test ", "tc-", "test_case_"]);
        return ClassificationResult::Classified {
            intent: CopilotIntent::TestRiskExplanation { test_case_id },
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // 5. "Which failures are recurring?"
    if (normalized.contains("recurring")
        || normalized.contains("recur")
        || normalized.contains("repeat failure"))
        && (normalized.contains("failure")
            || normalized.contains("failures")
            || normalized.contains("defect"))
    {
        return ClassificationResult::Classified {
            intent: CopilotIntent::RecurringFailures,
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // 6. "Which requirements lack test coverage?"
    if (normalized.contains("requirements") || normalized.contains("requirement"))
        && (normalized.contains("lack")
            || normalized.contains("no test coverage")
            || normalized.contains("uncovered")
            || normalized.contains("missing coverage")
            || normalized.contains("without test coverage"))
        && (normalized.contains("coverage") || normalized.contains("test"))
    {
        return ClassificationResult::Classified {
            intent: CopilotIntent::RequirementCoverageGaps,
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // 7. "Why was this finding classified as insufficient evidence?"
    if (normalized.contains("finding") || normalized.contains("evidence"))
        && (normalized.contains("insufficient evidence")
            || normalized.contains("classified as insufficient")
            || normalized.contains("marked as insufficient"))
    {
        let finding_id = extract_parameter(raw, &["finding id ", "finding "]);
        let run_id = extract_parameter(raw, &["run id ", "run "]);
        return ClassificationResult::Classified {
            intent: CopilotIntent::InsufficientEvidenceReason { finding_id, run_id },
            confidence: 1.0,
            normalized_query: normalized,
        };
    }

    // Default: unsupported query outside QA Copilot scope
    ClassificationResult::UnsupportedQuery {
        reason: "The query does not match any supported QA intelligence intent. Ask one of the eight canonical QA questions (release failure, test recommendations, regression impact, test risk, recurring failures, requirement coverage, insufficient evidence, or readiness blockers).".to_string(),
        normalized_query: normalized,
    }
}
