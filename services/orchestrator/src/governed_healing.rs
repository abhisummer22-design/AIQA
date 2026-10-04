//! Governed Self-Healing & Quality Feedback — Phase 5.
//!
//! Authoritative, closed-AST, governed self-healing engine and calibration feedback loop:
//! - Multi-tenant project boundary & RBAC enforcement
//! - Closed Action AST only (execution_plan::Action); zero source-file or shell mutation
//! - Deterministic AST Diff answering the 7 audit questions
//! - Strict PlanValidator & SSRF validation (executor::validate_target_url)
//! - Risk classification (Low, Medium, High, Critical)
//! - Human approval gate (mandatory for production, high-risk, and low-confidence)
//! - Typed healing lifecycle state machine
//! - Bounded healing attempt (MAX = 1 attempt per failed task/run)
//! - Fingerprint idempotency
//! - Bounded verification (Verified, NotVerified, RegressionIntroduced)
//! - Quality Graph calibration and structured TestPlanningAgent feedback
//! - Traceability to LocalEvidenceStore and audit logs
//! - Zero private chain-of-thought leakage (concise operational rationale only)

use crate::agent::{run_agent_loop, AgentContext, AgentLimits, AgentStatus};
use crate::agent_execution::{
    AgentExecutionBudget, AgentExecutionDispatcher, AgentExecutionRequest,
};
use crate::agent_tools::AgentToolRegistry;
pub use crate::agents::healing::{FailureEvidence, SelfHealingAgent, SelfHealingOutput};
use crate::ai_router::AiInfrastructure;
use crate::auth::{AuthContext, Role};
use crate::domain::TaskStatus;
use crate::evidence::LocalEvidenceStore;
use crate::execution_plan::Action;
use crate::plan_validator::{
    AgentExecutionPlan, AgentTaskPlan, PlanValidationError, PlanValidator,
};
use crate::policy::PolicyConfig;
use crate::store::{HealingProposalRow, Store};
use crate::tools::register_diagnostic_tools;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

/// Default proposal TTL: 24 hours.
pub const DEFAULT_PROPOSAL_TTL_SECS: i64 = 86400;

// ============================================================================
// 1. RISK & LIFECYCLE ENUMS
// ============================================================================

/// Strongly typed healing risk level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealingRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl HealingRiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "CRITICAL" => Self::Critical,
            "HIGH" => Self::High,
            "MEDIUM" => Self::Medium,
            _ => Self::Low,
        }
    }
}

/// Typed lifecycle state machine for healing proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealingState {
    Diagnosed,
    ProposalCreated,
    ValidationFailed,
    PendingApproval,
    Approved,
    Executing,
    Verified,
    NotVerified,
    RegressionIntroduced,
    Rejected,
    Expired,
    Cancelled,
    Failed,
}

impl HealingState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Diagnosed => "DIAGNOSED",
            Self::ProposalCreated => "PROPOSAL_CREATED",
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::PendingApproval => "PENDING_APPROVAL",
            Self::Approved => "APPROVED",
            Self::Executing => "EXECUTING",
            Self::Verified => "VERIFIED",
            Self::NotVerified => "NOT_VERIFIED",
            Self::RegressionIntroduced => "REGRESSION_INTRODUCED",
            Self::Rejected => "REJECTED",
            Self::Expired => "EXPIRED",
            Self::Cancelled => "CANCELLED",
            Self::Failed => "FAILED",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "DIAGNOSED" => Self::Diagnosed,
            "PROPOSAL_CREATED" => Self::ProposalCreated,
            "VALIDATION_FAILED" => Self::ValidationFailed,
            "PENDING_APPROVAL" => Self::PendingApproval,
            "APPROVED" => Self::Approved,
            "EXECUTING" => Self::Executing,
            "VERIFIED" => Self::Verified,
            "NOT_VERIFIED" => Self::NotVerified,
            "REGRESSION_INTRODUCED" => Self::RegressionIntroduced,
            "REJECTED" => Self::Rejected,
            "EXPIRED" => Self::Expired,
            "CANCELLED" => Self::Cancelled,
            _ => Self::Failed,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Verified
                | Self::NotVerified
                | Self::RegressionIntroduced
                | Self::Rejected
                | Self::Expired
                | Self::Cancelled
                | Self::Failed
        )
    }
}

/// Verification outcome after approved healing executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationOutcome {
    Verified,
    NotVerified,
    RegressionIntroduced,
}

impl VerificationOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::NotVerified => "NOT_VERIFIED",
            Self::RegressionIntroduced => "REGRESSION_INTRODUCED",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "VERIFIED" => Self::Verified,
            "REGRESSION_INTRODUCED" => Self::RegressionIntroduced,
            _ => Self::NotVerified,
        }
    }
}

// ============================================================================
// 2. CLOSED ACTION AST DIFF
// ============================================================================

/// Typed representation of an action-level change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionChangeKind {
    SelectorChanged {
        old_selector: String,
        new_selector: String,
    },
    AssertionChanged {
        assertion_type: String,
        old_expected: String,
        new_expected: String,
    },
    TimeoutChanged {
        old_timeout_secs: u64,
        new_timeout_secs: u64,
    },
    NavigationPathChanged {
        old_url: String,
        new_url: String,
    },
    ParameterChanged {
        parameter_name: String,
        old_val: String,
        new_val: String,
    },
    ActionReplaced {
        old_action_type: String,
        new_action_type: String,
    },
    ActionAdded {
        action_type: String,
    },
    ActionRemoved {
        action_type: String,
    },
    Unchanged,
}

/// A diff entry for an individual action step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDiff {
    pub step_index: usize,
    pub original_action: Option<Action>,
    pub proposed_action: Option<Action>,
    pub change_kind: ActionChangeKind,
    pub description: String,
}

/// Deterministic diff between Original Action AST and Proposed Action AST.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AstDiff {
    pub action_diffs: Vec<ActionDiff>,
    pub summary: String,
    pub has_changes: bool,
    pub risk_factors: Vec<String>,
}

impl AstDiff {
    /// Computes the typed diff between original and proposed Action AST sequences.
    pub fn compute(original: &[Action], proposed: &[Action]) -> Self {
        let mut diffs = Vec::new();
        let mut risk_factors = Vec::new();
        let max_len = usize::max(original.len(), proposed.len());

        for i in 0..max_len {
            let orig = original.get(i);
            let prop = proposed.get(i);

            match (orig, prop) {
                (Some(o), Some(p)) => {
                    let (change, desc) = diff_action_pair(o, p, &mut risk_factors);
                    diffs.push(ActionDiff {
                        step_index: i,
                        original_action: Some(o.clone()),
                        proposed_action: Some(p.clone()),
                        change_kind: change,
                        description: desc,
                    });
                }
                (Some(o), None) => {
                    risk_factors.push(format!("Step {i}: action was removed"));
                    diffs.push(ActionDiff {
                        step_index: i,
                        original_action: Some(o.clone()),
                        proposed_action: None,
                        change_kind: ActionChangeKind::ActionRemoved {
                            action_type: action_name(o).to_string(),
                        },
                        description: format!("Action {} removed at step {i}", action_name(o)),
                    });
                }
                (None, Some(p)) => {
                    risk_factors.push(format!("Step {i}: action was added"));
                    diffs.push(ActionDiff {
                        step_index: i,
                        original_action: None,
                        proposed_action: Some(p.clone()),
                        change_kind: ActionChangeKind::ActionAdded {
                            action_type: action_name(p).to_string(),
                        },
                        description: format!("Action {} added at step {i}", action_name(p)),
                    });
                }
                (None, None) => unreachable!(),
            }
        }

        let has_changes = diffs
            .iter()
            .any(|d| !matches!(d.change_kind, ActionChangeKind::Unchanged));

        let changed_count = diffs
            .iter()
            .filter(|d| !matches!(d.change_kind, ActionChangeKind::Unchanged))
            .count();

        let summary = if !has_changes {
            "No changes between original and proposed action AST".to_string()
        } else {
            format!("{changed_count} action(s) modified in closed AST")
        };

        Self {
            action_diffs: diffs,
            summary,
            has_changes,
            risk_factors,
        }
    }

    /// Evaluates the risk level based on AST diff semantics, environment, and confidence score.
    pub fn calculate_risk_level(&self, environment: &str, confidence: f64) -> HealingRiskLevel {
        let env_norm = environment.trim().to_lowercase();
        let is_prod = env_norm == "production" || env_norm == "prod";

        // Production environment is always high risk at minimum
        if is_prod {
            return HealingRiskLevel::High;
        }

        // Check if any critical risk factor is present (e.g. host changed or security sensitive)
        let has_critical = self
            .risk_factors
            .iter()
            .any(|rf| rf.contains("target host changed"));
        if has_critical {
            return HealingRiskLevel::Critical;
        }

        let has_navigation_change = self.action_diffs.iter().any(|d| {
            matches!(
                d.change_kind,
                ActionChangeKind::NavigationPathChanged { .. }
            )
        });
        let has_action_replaced_or_removed = self.action_diffs.iter().any(|d| {
            matches!(
                d.change_kind,
                ActionChangeKind::ActionReplaced { .. }
                    | ActionChangeKind::ActionRemoved { .. }
                    | ActionChangeKind::ActionAdded { .. }
            )
        });

        if has_navigation_change || has_action_replaced_or_removed || confidence < 0.60 {
            return HealingRiskLevel::High;
        }

        let has_assertion_change = self
            .action_diffs
            .iter()
            .any(|d| matches!(d.change_kind, ActionChangeKind::AssertionChanged { .. }));

        if has_assertion_change || confidence < 0.80 {
            return HealingRiskLevel::Medium;
        }

        // Pure selector replacement with high confidence (>= 0.80) in same page
        HealingRiskLevel::Low
    }
}

fn action_name(action: &Action) -> &'static str {
    match action {
        Action::Navigate { .. } => "Navigate",
        Action::Click { .. } => "Click",
        Action::Fill { .. } => "Fill",
        Action::AssertVisible { .. } => "AssertVisible",
        Action::AssertText { .. } => "AssertText",
        Action::AssertUrl { .. } => "AssertUrl",
        Action::AssertHidden { .. } => "AssertHidden",
        Action::AssertEnabled { .. } => "AssertEnabled",
        Action::PressKey { .. } => "PressKey",
        Action::Request { .. } => "Request",
        Action::AssertStatus { .. } => "AssertStatus",
        Action::AssertHeader { .. } => "AssertHeader",
        Action::AssertBodyField { .. } => "AssertBodyField",
        Action::AssertStatusIn { .. } => "AssertStatusIn",
        Action::AssertBodySchema { .. } => "AssertBodySchema",
    }
}

fn diff_action_pair(
    orig: &Action,
    prop: &Action,
    risk_factors: &mut Vec<String>,
) -> (ActionChangeKind, String) {
    match (orig, prop) {
        (Action::Click { selector: s1 }, Action::Click { selector: s2 }) => {
            if s1 == s2 {
                (
                    ActionChangeKind::Unchanged,
                    "Click selector unchanged".to_string(),
                )
            } else {
                (
                    ActionChangeKind::SelectorChanged {
                        old_selector: s1.clone(),
                        new_selector: s2.clone(),
                    },
                    format!("Selector changed from '{s1}' to '{s2}'"),
                )
            }
        }
        (Action::AssertVisible { selector: s1 }, Action::AssertVisible { selector: s2 }) => {
            if s1 == s2 {
                (
                    ActionChangeKind::Unchanged,
                    "AssertVisible selector unchanged".to_string(),
                )
            } else {
                (
                    ActionChangeKind::SelectorChanged {
                        old_selector: s1.clone(),
                        new_selector: s2.clone(),
                    },
                    format!("AssertVisible selector changed from '{s1}' to '{s2}'"),
                )
            }
        }
        (
            Action::Fill {
                selector: s1,
                value: v1,
            },
            Action::Fill {
                selector: s2,
                value: v2,
            },
        ) => {
            if s1 != s2 {
                (
                    ActionChangeKind::SelectorChanged {
                        old_selector: s1.clone(),
                        new_selector: s2.clone(),
                    },
                    format!("Fill selector changed from '{s1}' to '{s2}'"),
                )
            } else if v1 != v2 {
                (
                    ActionChangeKind::ParameterChanged {
                        parameter_name: "value".to_string(),
                        old_val: v1.clone(),
                        new_val: v2.clone(),
                    },
                    format!("Fill value changed from '{v1}' to '{v2}'"),
                )
            } else {
                (
                    ActionChangeKind::Unchanged,
                    "Fill action unchanged".to_string(),
                )
            }
        }
        (Action::Navigate { url: u1 }, Action::Navigate { url: u2 }) => {
            if u1 == u2 {
                (
                    ActionChangeKind::Unchanged,
                    "Navigate URL unchanged".to_string(),
                )
            } else {
                let h1 = reqwest::Url::parse(u1)
                    .ok()
                    .and_then(|u| u.host_str().map(ToString::to_string));
                let h2 = reqwest::Url::parse(u2)
                    .ok()
                    .and_then(|u| u.host_str().map(ToString::to_string));
                if h1 != h2 {
                    risk_factors.push(format!("target host changed from {h1:?} to {h2:?}"));
                }
                (
                    ActionChangeKind::NavigationPathChanged {
                        old_url: u1.clone(),
                        new_url: u2.clone(),
                    },
                    format!("Navigation URL changed from '{u1}' to '{u2}'"),
                )
            }
        }
        (Action::AssertUrl { url_contains: c1 }, Action::AssertUrl { url_contains: c2 }) => {
            if c1 == c2 {
                (
                    ActionChangeKind::Unchanged,
                    "AssertUrl pattern unchanged".to_string(),
                )
            } else {
                (
                    ActionChangeKind::AssertionChanged {
                        assertion_type: "url_contains".to_string(),
                        old_expected: c1.clone(),
                        new_expected: c2.clone(),
                    },
                    format!("AssertUrl expected pattern changed from '{c1}' to '{c2}'"),
                )
            }
        }
        (
            Action::AssertText {
                selector: s1,
                text: t1,
            },
            Action::AssertText {
                selector: s2,
                text: t2,
            },
        ) => {
            if s1 != s2 {
                (
                    ActionChangeKind::SelectorChanged {
                        old_selector: s1.clone(),
                        new_selector: s2.clone(),
                    },
                    format!("AssertText selector changed from '{s1}' to '{s2}'"),
                )
            } else if t1 != t2 {
                (
                    ActionChangeKind::AssertionChanged {
                        assertion_type: "text".to_string(),
                        old_expected: t1.clone(),
                        new_expected: t2.clone(),
                    },
                    format!("AssertText expected text changed from '{t1}' to '{t2}'"),
                )
            } else {
                (
                    ActionChangeKind::Unchanged,
                    "AssertText unchanged".to_string(),
                )
            }
        }
        (Action::AssertStatus { status: st1 }, Action::AssertStatus { status: st2 }) => {
            if st1 == st2 {
                (
                    ActionChangeKind::Unchanged,
                    "AssertStatus unchanged".to_string(),
                )
            } else {
                (
                    ActionChangeKind::AssertionChanged {
                        assertion_type: "status".to_string(),
                        old_expected: st1.to_string(),
                        new_expected: st2.to_string(),
                    },
                    format!("AssertStatus expected status changed from {st1} to {st2}"),
                )
            }
        }
        (
            Action::AssertHeader {
                name: n1,
                value: v1,
            },
            Action::AssertHeader {
                name: n2,
                value: v2,
            },
        ) => {
            if n1 == n2 && v1 == v2 {
                (
                    ActionChangeKind::Unchanged,
                    "AssertHeader unchanged".to_string(),
                )
            } else {
                (
                    ActionChangeKind::AssertionChanged {
                        assertion_type: format!("header:{n1}"),
                        old_expected: v1.clone(),
                        new_expected: v2.clone(),
                    },
                    format!("AssertHeader {n1} expected value changed from '{v1}' to '{v2}'"),
                )
            }
        }
        (
            Action::AssertBodyField {
                path: p1,
                equals: e1,
            },
            Action::AssertBodyField {
                path: p2,
                equals: e2,
            },
        ) => {
            if p1 == p2 && e1 == e2 {
                (
                    ActionChangeKind::Unchanged,
                    "AssertBodyField unchanged".to_string(),
                )
            } else {
                (
                    ActionChangeKind::AssertionChanged {
                        assertion_type: format!("body:{p1}"),
                        old_expected: e1.to_string(),
                        new_expected: e2.to_string(),
                    },
                    format!("AssertBodyField at path '{p1}' expected changed"),
                )
            }
        }
        (
            Action::Request {
                method: m1,
                url: u1,
                ..
            },
            Action::Request {
                method: m2,
                url: u2,
                ..
            },
        ) => {
            if u1 != u2 {
                let h1 = reqwest::Url::parse(u1)
                    .ok()
                    .and_then(|u| u.host_str().map(ToString::to_string));
                let h2 = reqwest::Url::parse(u2)
                    .ok()
                    .and_then(|u| u.host_str().map(ToString::to_string));
                if h1 != h2 {
                    risk_factors.push(format!("target host changed from {h1:?} to {h2:?}"));
                }
                (
                    ActionChangeKind::NavigationPathChanged {
                        old_url: u1.clone(),
                        new_url: u2.clone(),
                    },
                    format!("Request URL changed from '{u1}' to '{u2}'"),
                )
            } else if m1 != m2 {
                (
                    ActionChangeKind::ParameterChanged {
                        parameter_name: "method".to_string(),
                        old_val: m1.clone(),
                        new_val: m2.clone(),
                    },
                    format!("Request method changed from '{m1}' to '{m2}'"),
                )
            } else {
                (ActionChangeKind::Unchanged, "Request unchanged".to_string())
            }
        }
        (o, p) => {
            risk_factors.push(format!(
                "Action variant replaced from {} to {}",
                action_name(o),
                action_name(p)
            ));
            (
                ActionChangeKind::ActionReplaced {
                    old_action_type: action_name(o).to_string(),
                    new_action_type: action_name(p).to_string(),
                },
                format!(
                    "Action replaced from {} to {}",
                    action_name(o),
                    action_name(p)
                ),
            )
        }
    }
}

// ============================================================================
// 3. HEALING PROPOSAL & VERIFICATION STRUCTS
// ============================================================================

/// Strongly typed Healing Proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingProposal {
    pub id: Uuid,
    pub project_id: String,
    pub run_id: Uuid,
    pub task_id: String,
    pub task_name: String,
    pub executor: String,
    pub environment: String,
    pub target_url: Option<String>,
    pub failure_fingerprint: String,
    pub diagnosis_category: String,
    pub confidence: f64,
    pub risk_level: HealingRiskLevel,
    pub requires_approval: bool,
    pub original_actions: Vec<Action>,
    pub proposed_actions: Vec<Action>,
    pub ast_diff: AstDiff,
    pub rationale: String,
    pub evidence_refs: Vec<String>,
    pub state: HealingState,
    pub idempotency_key: String,
    pub healing_run_id: Option<Uuid>,
    pub verification_result: Option<HealingVerification>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub timeout_secs: Option<u64>,
    pub requirement_reference: Option<String>,
    pub custom_params: Option<serde_json::Value>,
}

/// Verification result captured after approved healing execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealingVerification {
    pub proposal_id: Uuid,
    pub original_run_id: Uuid,
    pub original_task_id: String,
    pub healing_run_id: Uuid,
    pub healing_task_id: String,
    pub outcome: VerificationOutcome,
    pub original_failure_resolved: bool,
    pub revised_task_passed: bool,
    pub new_failures_introduced: usize,
    pub regression_detected: bool,
    pub evidence_refs: Vec<String>,
    pub message: String,
    pub verified_at: DateTime<Utc>,
}

/// Structured feedback provided to `TestPlanningAgent`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealingFeedback {
    pub project_id: String,
    pub failure_cluster: Option<String>,
    pub diagnosis_category: String,
    pub healing_strategy: String,
    pub confidence: f64,
    pub verification_result: String,
    pub regression_detected: bool,
    pub historical_success_count: i64,
    pub historical_failure_count: i64,
    pub operational_insight: String,
}

/// Request to create a new governed healing proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingProposalRequest {
    pub project_id: String,
    pub run_id: Uuid,
    pub task_id: String,
    pub task_name: String,
    pub executor: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub target_url: Option<String>,
    #[serde(default)]
    pub failed_selector: Option<String>,
    pub failure_reason: String,
    pub original_actions: Vec<Action>,
    /// The healed action plan. May be omitted, along with
    /// `diagnosis_category`/`confidence`/`rationale`, to have the real
    /// `SelfHealingAgent` (Phase 3) diagnose the failure itself via the
    /// Phase 2 diagnostic tools instead of requiring the caller to already
    /// know the fix — see `crawl_task_id`/`crawl_output` below.
    #[serde(default)]
    pub proposed_actions: Option<Vec<Action>>,
    #[serde(default)]
    pub diagnosis_category: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub requirement_reference: Option<String>,
    #[serde(default)]
    pub custom_params: Option<serde_json::Value>,
    #[serde(default)]
    pub auto_approve_safe: bool,
    /// Task ID of a prior crawler run, read from orchestrator disk exactly
    /// as `dom_prober` reads it — used only for auto-diagnosis (i.e. when
    /// `proposed_actions`/`diagnosis_category`/`confidence`/`rationale` are
    /// omitted), so the agent has real DOM evidence to reason over.
    #[serde(default)]
    pub crawl_task_id: Option<String>,
    /// Inline `CrawlOutput` JSON, as an alternative to `crawl_task_id` for
    /// auto-diagnosis.
    #[serde(default)]
    pub crawl_output: Option<serde_json::Value>,
}

// ============================================================================
// 4. ERRORS & SECURITY BOUNDARY CHECKS
// ============================================================================

/// Structured errors encountered during governed self-healing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GovernedHealingError {
    ValidationError(Vec<PlanValidationError>),
    SecurityViolation(String),
    Unauthorized(String),
    CrossProjectForbidden(String),
    MaxHealingAttemptsExceeded(String),
    InvalidStateTransition(String),
    ProposalExpired(String),
    NotFound(String),
    Conflict(String),
    ExecutionError(String),
    StorageError(String),
    Internal(String),
}

impl std::fmt::Display for GovernedHealingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationError(errs) => write!(f, "plan validation failed: {errs:?}"),
            Self::SecurityViolation(msg) => write!(f, "security boundary violation: {msg}"),
            Self::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
            Self::CrossProjectForbidden(msg) => write!(f, "cross-project forbidden: {msg}"),
            Self::MaxHealingAttemptsExceeded(msg) => {
                write!(f, "max healing attempts exceeded: {msg}")
            }
            Self::InvalidStateTransition(msg) => write!(f, "invalid state transition: {msg}"),
            Self::ProposalExpired(msg) => write!(f, "proposal expired: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::ExecutionError(msg) => write!(f, "execution error: {msg}"),
            Self::StorageError(msg) => write!(f, "storage error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for GovernedHealingError {}

/// Computes a deterministic SHA-256 fingerprint of a failure.
pub fn compute_failure_fingerprint(
    run_id: &Uuid,
    task_id: &str,
    failed_selector: Option<&str>,
    reason: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(run_id.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(task_id.as_bytes());
    hasher.update(b":");
    if let Some(sel) = failed_selector {
        hasher.update(sel.as_bytes());
    }
    hasher.update(b":");
    hasher.update(reason.as_bytes());
    hex::encode(hasher.finalize())
}

/// Verifies immutable security boundaries on a proposed healing.
pub async fn verify_immutable_security_boundaries(
    req: &HealingProposalRequest,
    proposed_actions: &[Action],
    original_env: &str,
    original_executor: &str,
) -> Result<(), GovernedHealingError> {
    // 1. Immutable Environment Check
    let req_env = req.environment.as_deref().unwrap_or(original_env);
    if req_env != original_env {
        return Err(GovernedHealingError::SecurityViolation(format!(
            "environment mutation forbidden: original '{original_env}', requested '{req_env}'"
        )));
    }

    // 2. Immutable Executor Check
    if req.executor.trim().to_lowercase() != original_executor.trim().to_lowercase() {
        return Err(GovernedHealingError::SecurityViolation(format!(
            "executor mutation forbidden: original '{original_executor}', requested '{}'",
            req.executor
        )));
    }

    // 3. Closed Action AST & SSRF Verification
    for action in proposed_actions {
        match action {
            Action::Navigate { url } => {
                crate::executor::validate_target_url(url)
                    .await
                    .map_err(|e| {
                        GovernedHealingError::SecurityViolation(format!(
                            "SSRF target rejected: {e}"
                        ))
                    })?;
            }
            Action::Request { url, .. } => {
                crate::executor::validate_target_url(url)
                    .await
                    .map_err(|e| {
                        GovernedHealingError::SecurityViolation(format!(
                            "SSRF target rejected: {e}"
                        ))
                    })?;
            }
            Action::Click { selector } | Action::AssertVisible { selector } => {
                validate_safe_selector(selector)?;
            }
            Action::Fill { selector, value } => {
                validate_safe_selector(selector)?;
                validate_safe_input_value(value)?;
            }
            Action::AssertText { selector, .. } => {
                validate_safe_selector(selector)?;
            }
            _ => {}
        }
    }

    Ok(())
}

pub fn validate_safe_selector(selector: &str) -> Result<(), GovernedHealingError> {
    let lower = selector.to_lowercase();
    let dangerous = [
        "<script",
        "javascript:",
        "data:text/html",
        "onload=",
        "onerror=",
        "onclick=",
        "eval(",
    ];
    for d in dangerous {
        if lower.contains(d) {
            return Err(GovernedHealingError::SecurityViolation(format!(
                "dangerous payload detected in selector: '{d}' is strictly forbidden"
            )));
        }
    }
    Ok(())
}

pub fn validate_safe_input_value(value: &str) -> Result<(), GovernedHealingError> {
    let lower = value.to_lowercase();
    let dangerous = [
        "<script",
        "javascript:",
        "data:text/html",
        "; drop table",
        "' or 1=1",
        "; rm",
        "rm -rf",
        "cat /etc",
        "/etc/passwd",
        "| nc ",
        "&& cat",
        "powershell",
        "/bin/sh",
        "/bin/bash",
    ];
    for d in dangerous {
        if lower.contains(d) {
            return Err(GovernedHealingError::SecurityViolation(format!(
                "dangerous payload detected in input value: '{d}' is strictly forbidden"
            )));
        }
    }
    Ok(())
}

/// A proposal whose proposed actions equal the original ones changes nothing; the autonomous
/// Phase 10 path raises exactly these, because it can see *that* a task keeps failing but not
/// what should replace the step. Approving one authorizes no fix, and executing one would re-run
/// the failing step and then have `verify_healing` record the result as if a healing strategy
/// had been tried, feeding the calibration metrics and the quality graph. Recomputed from the
/// actions rather than read from the stored diff, so a hand-written row cannot slip past.
fn ensure_changes_something(proposal: &HealingProposal) -> Result<(), GovernedHealingError> {
    if AstDiff::compute(&proposal.original_actions, &proposal.proposed_actions).has_changes {
        return Ok(());
    }
    Err(GovernedHealingError::Conflict(
        "proposal changes nothing: its proposed actions are identical to the original, so there \
         is nothing to approve or execute (reject it, or investigate the failure manually)"
            .to_string(),
    ))
}

// ============================================================================
// 4B. PHASE 1-3 WIRING — REAL AGENT-DRIVEN AUTO-DIAGNOSIS
// ============================================================================

/// Resolved diagnosis fields, either supplied directly by the caller or
/// computed by actually running the real `SelfHealingAgent` (Phase 3)
/// through `run_agent_loop` (Phase 1) against the diagnostic tools (Phase
/// 2) — the concrete wiring point that makes those three phases reachable
/// from a live request instead of only from their own tests.
struct ResolvedDiagnosis {
    diagnosis_category: String,
    confidence: f64,
    rationale: String,
    proposed_actions: Vec<Action>,
}

/// Maps a completed `SelfHealingOutput` onto the fields a `HealingProposal`
/// needs, replacing the failed selector with the agent's recommended
/// replacement inside a copy of the original action plan. When the agent
/// found no confident replacement, the original actions are proposed
/// unchanged — the existing low-confidence approval gate (`confidence <
/// 0.80` in `propose_healing`) is what keeps this from ever auto-executing
/// silently; this function's job is only to produce an honest record.
fn healed_output_to_diagnosis(
    output: &SelfHealingOutput,
    original_actions: &[Action],
) -> ResolvedDiagnosis {
    let confidence = output
        .candidates
        .first()
        .map(|c| c.confidence_score)
        .unwrap_or(match output.confidence.as_str() {
            "HIGH" => 0.9,
            "MEDIUM" => 0.6,
            _ => 0.3,
        });

    let rationale = format!("{} {}", output.reason, output.recommendation)
        .trim()
        .to_string();

    let proposed_actions = match &output.recommended_selector {
        Some(new_sel) if new_sel != &output.original_selector => original_actions
            .iter()
            .cloned()
            .map(|action| replace_action_selector(action, &output.original_selector, new_sel))
            .collect(),
        _ => original_actions.to_vec(),
    };

    ResolvedDiagnosis {
        diagnosis_category: output.failure_classification.clone(),
        confidence,
        rationale,
        proposed_actions,
    }
}

/// Replaces `old_sel` with `new_sel` inside the selector field of any
/// action variant that carries one; every other variant (and any occurrence
/// of a different selector) passes through unchanged, so healing can only
/// ever touch the exact selector it diagnosed.
fn replace_action_selector(action: Action, old_sel: &str, new_sel: &str) -> Action {
    match action {
        Action::Click { selector } if selector == old_sel => Action::Click {
            selector: new_sel.to_string(),
        },
        Action::AssertVisible { selector } if selector == old_sel => Action::AssertVisible {
            selector: new_sel.to_string(),
        },
        Action::Fill { selector, value } if selector == old_sel => Action::Fill {
            selector: new_sel.to_string(),
            value,
        },
        Action::AssertText { selector, text } if selector == old_sel => Action::AssertText {
            selector: new_sel.to_string(),
            text,
        },
        other => other,
    }
}

// ============================================================================
// 5. GOVERNED HEALING SERVICE
// ============================================================================

/// Authoritative coordinator for Governed Self-Healing & Quality Feedback.
#[derive(Clone)]
pub struct GovernedHealingService {
    pub store: Store,
    pub dispatcher: AgentExecutionDispatcher,
    pub evidence: Arc<LocalEvidenceStore>,
    pub policy: Arc<PolicyConfig>,
    pub budget: AgentExecutionBudget,
}

impl GovernedHealingService {
    pub fn new(
        store: Store,
        dispatcher: AgentExecutionDispatcher,
        evidence: Arc<LocalEvidenceStore>,
        policy: Arc<PolicyConfig>,
        budget: Option<AgentExecutionBudget>,
    ) -> Self {
        Self {
            store,
            dispatcher,
            evidence,
            policy,
            budget: budget.unwrap_or_default(),
        }
    }

    /// Runs the real `SelfHealingAgent` (Phase 3) through `run_agent_loop`
    /// (Phase 1) with the Phase 2 diagnostic tools registered, so a caller
    /// that does not already know the fix can still get a governed healing
    /// proposal — diagnosed from real DOM evidence rather than supplied by
    /// hand. Requires `failed_selector` plus one of `crawl_task_id` /
    /// `crawl_output`; without crawl evidence `dom_prober` cannot answer
    /// anything, so this fails fast with a clear error instead of letting
    /// the agent burn its whole turn budget on a tool call that can only
    /// ever fail.
    async fn auto_diagnose(
        &self,
        project_id: &str,
        req: &HealingProposalRequest,
    ) -> Result<ResolvedDiagnosis, GovernedHealingError> {
        let failed_selector = req.failed_selector.as_deref().unwrap_or("").trim();
        if failed_selector.is_empty() {
            return Err(GovernedHealingError::ValidationError(vec![
                PlanValidationError::new(
                    "MISSING_FAILED_SELECTOR",
                    "failed_selector is required to auto-diagnose a healing proposal when proposed_actions/diagnosis_category/confidence/rationale are not supplied directly",
                    Some("failed_selector"),
                ),
            ]));
        }
        if req.crawl_task_id.is_none() && req.crawl_output.is_none() {
            return Err(GovernedHealingError::ValidationError(vec![
                PlanValidationError::new(
                    "MISSING_CRAWL_EVIDENCE",
                    "crawl_task_id or crawl_output is required to auto-diagnose a healing proposal; supply one, or supply proposed_actions/diagnosis_category/confidence/rationale directly instead",
                    Some("crawl_task_id"),
                ),
            ]));
        }

        let evidence = FailureEvidence {
            project_id: project_id.to_string(),
            test_case_id: None,
            test_execution_id: Some(req.run_id.to_string()),
            url: req.target_url.clone(),
            failed_selector: failed_selector.to_string(),
            expected_behavior: None,
            actual_behavior: None,
            failure_message: Some(req.failure_reason.clone()),
            crawl_task_id: req.crawl_task_id.clone(),
            crawl_output: req.crawl_output.clone(),
            role: None,
            surrounding_context: None,
        };

        let mut registry = AgentToolRegistry::new();
        register_diagnostic_tools(&mut registry, Some(self.store.clone())).map_err(|e| {
            GovernedHealingError::Internal(format!(
                "failed to register diagnostic tools for self-healing: {e}"
            ))
        })?;

        let mut context = AgentContext::new(
            format!("heal-diagnose-{}", req.task_id),
            Some(project_id.to_string()),
            req.run_id.to_string(),
            AgentLimits::default(),
            None,
        );

        // SelfHealingAgent is a deterministic, tool-driven diagnostician
        // (see agents/healing.rs — its `step` takes `_ai` and never reads
        // it); `run_agent_loop` still requires an `AiInfrastructure` by
        // signature, so an empty, provider-less one is genuinely
        // equivalent here, not a stand-in for a real call that got skipped.
        let ai = AiInfrastructure::new(vec![], std::collections::HashMap::new());
        let agent = SelfHealingAgent::new(evidence);

        let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
            .await
            .map_err(|e| {
                GovernedHealingError::Internal(format!("self-healing diagnosis failed: {e}"))
            })?;

        if outcome.status != AgentStatus::Completed {
            return Err(GovernedHealingError::Internal(format!(
                "self-healing agent could not reach a diagnosis (status: {:?}{})",
                outcome.status,
                outcome.error.map(|e| format!(", {e}")).unwrap_or_default()
            )));
        }
        let output_val = outcome.final_output.ok_or_else(|| {
            GovernedHealingError::Internal(
                "self-healing agent completed without producing a diagnosis".to_string(),
            )
        })?;
        let output: SelfHealingOutput = serde_json::from_value(output_val).map_err(|e| {
            GovernedHealingError::Internal(format!(
                "failed to parse self-healing diagnosis output: {e}"
            ))
        })?;

        Ok(healed_output_to_diagnosis(&output, &req.original_actions))
    }

    /// Creates a governed healing proposal from a diagnosed failure.
    pub async fn propose_healing(
        &self,
        ctx: &AuthContext,
        req: HealingProposalRequest,
    ) -> Result<HealingProposal, GovernedHealingError> {
        let project_id = req.project_id.trim();

        // 1. Multi-Tenant Authorization & RBAC Enforcement
        if !ctx.role.at_least(Role::Operator) {
            return Err(GovernedHealingError::Unauthorized(format!(
                "role '{}' is not authorized to create healing proposals; requires operator or admin",
                ctx.role.as_str()
            )));
        }

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != project_id {
                let _ = self
                    .store
                    .insert_audit(
                        Some(&ctx.name),
                        Some(ctx.role.as_str()),
                        Some(project_id),
                        "healing.cross_project_deny",
                        None,
                        Some("cross-project access violation blocked on healing proposal"),
                        "denied",
                    )
                    .await;
                return Err(GovernedHealingError::CrossProjectForbidden(format!(
                    "cross-project access violation: key scoped to '{auth_pid}' cannot create proposal for '{project_id}'"
                )));
            }
        }

        // 2. Failure Fingerprint & Idempotency Key
        let fingerprint = compute_failure_fingerprint(
            &req.run_id,
            &req.task_id,
            req.failed_selector.as_deref(),
            &req.failure_reason,
        );
        let idempotency_key = format!("heal:{}:{}:{}", req.run_id, req.task_id, &fingerprint[..12]);

        // Idempotency check: if an identical proposal already exists, return it!
        if let Ok(Some(existing_row)) = self
            .store
            .get_healing_proposal_by_idempotency(project_id, &idempotency_key)
            .await
        {
            return row_to_proposal(existing_row);
        }

        // 3. Bounded Healing Attempt Check (MAX 1 attempt per failed task/run)
        let attempt_count = self
            .store
            .count_healing_attempts(project_id, &req.run_id.to_string(), &req.task_id)
            .await
            .unwrap_or(0);

        if attempt_count >= 1 {
            return Err(GovernedHealingError::MaxHealingAttemptsExceeded(format!(
                "failed task '{}:{}' has already reached maximum allowed healing attempts (1)",
                req.run_id, req.task_id
            )));
        }

        // 3B. Resolve the diagnosis: use the caller-supplied fields verbatim
        // when all four are present (a human or external system already
        // knows the fix), otherwise run the real SelfHealingAgent (Phase 3)
        // against the Phase 2 diagnostic tools to diagnose it here — this
        // is the actual Phase 1-3 wiring point.
        let diagnosis = match (
            &req.proposed_actions,
            &req.diagnosis_category,
            req.confidence,
            &req.rationale,
        ) {
            (Some(actions), Some(category), Some(confidence), Some(rationale)) => {
                ResolvedDiagnosis {
                    diagnosis_category: category.clone(),
                    confidence,
                    rationale: rationale.clone(),
                    proposed_actions: actions.clone(),
                }
            }
            _ => self.auto_diagnose(project_id, &req).await?,
        };

        if diagnosis.proposed_actions.is_empty() {
            return Err(GovernedHealingError::ValidationError(vec![
                PlanValidationError::new(
                    "EMPTY_ACTIONS",
                    "healing proposal must specify at least one proposed action",
                    Some("proposed_actions"),
                ),
            ]));
        }

        // 4. Immutable Security Boundaries Verification
        let original_env = req.environment.as_deref().unwrap_or("test");
        verify_immutable_security_boundaries(
            &req,
            &diagnosis.proposed_actions,
            original_env,
            &req.executor,
        )
        .await?;

        // 5. Closed Action AST Diff Computation
        let ast_diff = AstDiff::compute(&req.original_actions, &diagnosis.proposed_actions);

        // 6. Risk Level & Human Approval Gate Calculation
        let risk_level = ast_diff.calculate_risk_level(original_env, diagnosis.confidence);
        let is_prod = original_env == "production" || original_env == "prod";
        let is_gated_executor = self
            .policy
            .require_approval_for
            .contains(&req.executor.to_lowercase());

        let requires_approval = is_prod
            || is_gated_executor
            || risk_level >= HealingRiskLevel::High
            || diagnosis.confidence < 0.80
            || !req.auto_approve_safe;

        // 7. PlanValidator Structural & Budget Validation
        let healing_task_id = format!("heal-{}", req.task_id);
        let agent_task = AgentTaskPlan {
            id: healing_task_id,
            name: format!("Healed: {}", req.task_name),
            prerequisites: Vec::new(),
            executor: req.executor.clone(),
            target_url: req.target_url.clone(),
            actions: diagnosis.proposed_actions.clone(),
            timeout_secs: req.timeout_secs.or(Some(30)),
            requirement_reference: req.requirement_reference.clone(),
            custom_params: req.custom_params.clone(),
        };

        let agent_plan = AgentExecutionPlan {
            project_id: project_id.to_string(),
            environment: original_env.to_string(),
            plan_name: format!("Self-Healing: {}", req.task_name),
            concurrency: 1,
            tasks: vec![agent_task],
            metadata: BTreeMap::new(),
        };

        let validation_res = PlanValidator::validate(
            &agent_plan,
            self.budget.max_tasks_per_plan,
            self.budget.max_task_duration_secs,
            self.budget.max_concurrency,
        )
        .await;

        let proposal_id = Uuid::new_v4();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(DEFAULT_PROPOSAL_TTL_SECS);

        let initial_state = match validation_res {
            Ok(_) => {
                if requires_approval {
                    HealingState::PendingApproval
                } else {
                    HealingState::Approved
                }
            }
            Err(errs) => {
                // Record validation failure in audit log and return error
                let _ = self
                    .store
                    .insert_audit(
                        Some(&ctx.name),
                        Some(ctx.role.as_str()),
                        Some(project_id),
                        "healing.validation_failed",
                        Some(&proposal_id.to_string()),
                        Some(&format!("plan validation failed: {errs:?}")),
                        "error",
                    )
                    .await;
                return Err(GovernedHealingError::ValidationError(errs));
            }
        };

        // 8. Construct Strongly-Typed Proposal
        // Non-leaking invariant: store only operational rationale, never private CoT
        let clean_rationale = diagnosis.rationale.trim().to_string();

        let proposal = HealingProposal {
            id: proposal_id,
            project_id: project_id.to_string(),
            run_id: req.run_id,
            task_id: req.task_id.clone(),
            task_name: req.task_name.clone(),
            executor: req.executor.clone(),
            environment: original_env.to_string(),
            target_url: req.target_url.clone(),
            failure_fingerprint: fingerprint.clone(),
            diagnosis_category: diagnosis.diagnosis_category.clone(),
            confidence: diagnosis.confidence,
            risk_level,
            requires_approval,
            original_actions: req.original_actions.clone(),
            proposed_actions: diagnosis.proposed_actions.clone(),
            ast_diff: ast_diff.clone(),
            rationale: clean_rationale,
            evidence_refs: req.evidence_refs.clone(),
            state: initial_state,
            idempotency_key: idempotency_key.clone(),
            healing_run_id: None,
            verification_result: None,
            approved_by: None,
            approved_at: None,
            rejection_reason: None,
            created_at: now,
            expires_at,
            updated_at: now,
            timeout_secs: req.timeout_secs,
            requirement_reference: req.requirement_reference.clone(),
            custom_params: req.custom_params.clone(),
        };

        // 9. Persist to Database
        let row = proposal_to_row(&proposal)?;
        self.store
            .insert_healing_proposal(&row)
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        // 10. Audit Logging
        let audit_action = if requires_approval {
            "healing.proposal_held_for_approval"
        } else {
            "healing.proposal_auto_approved"
        };
        let _ = self
            .store
            .insert_audit(
                Some(&ctx.name),
                Some(ctx.role.as_str()),
                Some(project_id),
                audit_action,
                Some(&proposal_id.to_string()),
                Some(&format!(
                    "Risk: {}, Category: {}, Confidence: {:.2}",
                    risk_level.as_str(),
                    diagnosis.diagnosis_category,
                    diagnosis.confidence
                )),
                "ok",
            )
            .await;

        Ok(proposal)
    }

    /// Approves a held healing proposal.
    pub async fn approve_proposal(
        &self,
        ctx: &AuthContext,
        proposal_id: Uuid,
    ) -> Result<HealingProposal, GovernedHealingError> {
        // 1. RBAC & Project Check
        if !ctx.role.at_least(Role::Operator) {
            return Err(GovernedHealingError::Unauthorized(
                "only operators and admins can approve healing proposals; viewer access denied"
                    .to_string(),
            ));
        }

        let mut proposal = self.get_proposal(proposal_id).await?;

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != &proposal.project_id {
                return Err(GovernedHealingError::CrossProjectForbidden(
                    "cannot approve proposal belonging to a different project".to_string(),
                ));
            }
        }

        // 2. Lifecycle Checks
        if proposal.state != HealingState::PendingApproval {
            return Err(GovernedHealingError::Conflict(format!(
                "proposal is in state {:?}, not PENDING_APPROVAL",
                proposal.state
            )));
        }

        let now = Utc::now();
        if now > proposal.expires_at {
            self.store
                .update_healing_proposal_state(
                    proposal_id,
                    HealingState::Expired.as_str(),
                    None,
                    None,
                    Some("proposal TTL expired"),
                    None,
                    None,
                )
                .await
                .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;
            return Err(GovernedHealingError::ProposalExpired(
                "healing proposal has expired and cannot be approved".to_string(),
            ));
        }

        ensure_changes_something(&proposal)?;

        // 3. Transition to Approved
        proposal.state = HealingState::Approved;
        proposal.approved_by = Some(ctx.name.clone());
        proposal.approved_at = Some(now);
        proposal.updated_at = now;

        self.store
            .update_healing_proposal_state(
                proposal_id,
                HealingState::Approved.as_str(),
                Some(&ctx.name),
                Some(now),
                None,
                None,
                None,
            )
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        // 4. Audit Log
        let _ = self
            .store
            .insert_audit(
                Some(&ctx.name),
                Some(ctx.role.as_str()),
                Some(&proposal.project_id),
                "healing.proposal_approved",
                Some(&proposal_id.to_string()),
                Some("human operator authorized healing execution"),
                "ok",
            )
            .await;

        Ok(proposal)
    }

    /// Rejects a held healing proposal.
    pub async fn reject_proposal(
        &self,
        ctx: &AuthContext,
        proposal_id: Uuid,
        reason: &str,
    ) -> Result<HealingProposal, GovernedHealingError> {
        if !ctx.role.at_least(Role::Operator) {
            return Err(GovernedHealingError::Unauthorized(
                "only operators and admins can reject healing proposals".to_string(),
            ));
        }

        let mut proposal = self.get_proposal(proposal_id).await?;

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != &proposal.project_id {
                return Err(GovernedHealingError::CrossProjectForbidden(
                    "cannot reject proposal belonging to a different project".to_string(),
                ));
            }
        }

        if proposal.state != HealingState::PendingApproval {
            return Err(GovernedHealingError::Conflict(format!(
                "proposal is in state {:?}, not PENDING_APPROVAL",
                proposal.state
            )));
        }

        let now = Utc::now();
        proposal.state = HealingState::Rejected;
        proposal.rejection_reason = Some(reason.to_string());
        proposal.updated_at = now;

        self.store
            .update_healing_proposal_state(
                proposal_id,
                HealingState::Rejected.as_str(),
                None,
                None,
                Some(reason),
                None,
                None,
            )
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        let _ = self
            .store
            .insert_audit(
                Some(&ctx.name),
                Some(ctx.role.as_str()),
                Some(&proposal.project_id),
                "healing.proposal_rejected",
                Some(&proposal_id.to_string()),
                Some(reason),
                "ok",
            )
            .await;

        Ok(proposal)
    }

    /// Executes an approved healing proposal via existing AgentExecutionDispatcher.
    pub async fn execute_approved(
        &self,
        ctx: &AuthContext,
        proposal_id: Uuid,
    ) -> Result<HealingProposal, GovernedHealingError> {
        if !ctx.role.at_least(Role::Operator) {
            return Err(GovernedHealingError::Unauthorized(
                "only operators and admins can execute healing proposals".to_string(),
            ));
        }

        let mut proposal = self.get_proposal(proposal_id).await?;

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != &proposal.project_id {
                return Err(GovernedHealingError::CrossProjectForbidden(
                    "cannot execute proposal belonging to a different project".to_string(),
                ));
            }
        }

        // Must be in APPROVED state
        if proposal.state != HealingState::Approved {
            return Err(GovernedHealingError::Conflict(format!(
                "proposal cannot be executed: must be in APPROVED state, current state is {:?}",
                proposal.state
            )));
        }

        // Also guarded at approval, but a proposal can reach APPROVED without a person
        // (`auto_approve_safe`), so this is the check every route into execution passes through.
        ensure_changes_something(&proposal)?;

        // Convert proposal into an AgentExecutionPlan
        let healing_task_id = format!("heal-{}", proposal.task_id);
        let task = AgentTaskPlan {
            id: healing_task_id.clone(),
            name: format!("Healed: {}", proposal.task_name),
            prerequisites: Vec::new(),
            executor: proposal.executor.clone(),
            target_url: proposal.target_url.clone(),
            actions: proposal.proposed_actions.clone(),
            timeout_secs: proposal.timeout_secs.or(Some(30)),
            requirement_reference: proposal.requirement_reference.clone(),
            custom_params: proposal.custom_params.clone(),
        };

        let agent_plan = AgentExecutionPlan {
            project_id: proposal.project_id.clone(),
            environment: proposal.environment.clone(),
            plan_name: format!("Self-Healing: {}", proposal.task_name),
            concurrency: 1,
            tasks: vec![task],
            metadata: BTreeMap::new(),
        };

        let exec_request = AgentExecutionRequest {
            plan: agent_plan,
            idempotency_key: Some(format!("exec-{}", proposal.idempotency_key)),
            auto_approve_safe: true,
        };

        // Dispatch through existing AgentExecutionDispatcher
        let dispatch_resp = self
            .dispatcher
            .dispatch(ctx, exec_request)
            .await
            .map_err(|e| GovernedHealingError::ExecutionError(e.to_string()))?;

        let now = Utc::now();
        proposal.state = HealingState::Executing;
        proposal.healing_run_id = Some(dispatch_resp.run_id);
        proposal.updated_at = now;

        self.store
            .update_healing_proposal_state(
                proposal_id,
                HealingState::Executing.as_str(),
                None,
                None,
                None,
                Some(dispatch_resp.run_id),
                None,
            )
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        let _ = self
            .store
            .insert_audit(
                Some(&ctx.name),
                Some(ctx.role.as_str()),
                Some(&proposal.project_id),
                "healing.dispatched_to_engine",
                Some(&proposal_id.to_string()),
                Some(&format!("Healing run id: {}", dispatch_resp.run_id)),
                "ok",
            )
            .await;

        Ok(proposal)
    }

    /// Verifies execution outcome of a healing run and calibrates the Quality Graph.
    pub async fn verify_healing(
        &self,
        ctx: &AuthContext,
        proposal_id: Uuid,
    ) -> Result<HealingVerification, GovernedHealingError> {
        let proposal = self.get_proposal(proposal_id).await?;

        if let Some(ref auth_pid) = ctx.project_id {
            if auth_pid != &proposal.project_id {
                return Err(GovernedHealingError::CrossProjectForbidden(
                    "cannot verify proposal belonging to a different project".to_string(),
                ));
            }
        }

        let healing_run_id = proposal.healing_run_id.ok_or_else(|| {
            GovernedHealingError::Conflict(
                "cannot verify healing: execution has not been dispatched yet".to_string(),
            )
        })?;

        // Query run summary and tasks from existing store
        let _run_summary = self
            .store
            .get_run_summary(healing_run_id)
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?
            .ok_or_else(|| {
                GovernedHealingError::NotFound(format!("healing run '{healing_run_id}' not found"))
            })?;

        let tasks = self
            .store
            .list_tasks_for_run(healing_run_id)
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        let healing_task_id = format!("heal-{}", proposal.task_id);
        let healed_task = tasks.iter().find(|t| t.id == healing_task_id);

        let revised_task_passed = healed_task
            .map(|t| matches!(t.status, TaskStatus::Passed))
            .unwrap_or(false);

        let original_failure_resolved = revised_task_passed;

        // Regression check: did any other task fail?
        let other_failures = tasks
            .iter()
            .filter(|t| t.id != healing_task_id)
            .filter(|t| matches!(t.status, TaskStatus::Failed | TaskStatus::Timeout))
            .count();

        let regression_detected = other_failures > 0;

        let outcome = if revised_task_passed && !regression_detected {
            VerificationOutcome::Verified
        } else if regression_detected {
            VerificationOutcome::RegressionIntroduced
        } else {
            VerificationOutcome::NotVerified
        };

        let evidence_rows = self
            .store
            .list_evidence_for_run(healing_run_id)
            .await
            .unwrap_or_default();
        let evidence_refs: Vec<String> = evidence_rows.into_iter().map(|e| e.object_key).collect();

        let message = match outcome {
            VerificationOutcome::Verified => format!(
                "Healing verified: failure in '{}' resolved successfully without regressions",
                proposal.task_id
            ),
            VerificationOutcome::NotVerified => format!(
                "Healing not verified: revised task '{}' failed to reach PASS status",
                healing_task_id
            ),
            VerificationOutcome::RegressionIntroduced => format!(
                "Regression introduced: healing resolved target task but introduced {other_failures} failures in suite"
            ),
        };

        let now = Utc::now();
        let verification = HealingVerification {
            proposal_id,
            original_run_id: proposal.run_id,
            original_task_id: proposal.task_id.clone(),
            healing_run_id,
            healing_task_id,
            outcome,
            original_failure_resolved,
            revised_task_passed,
            new_failures_introduced: other_failures,
            regression_detected,
            evidence_refs,
            message: message.clone(),
            verified_at: now,
        };

        let final_state = match outcome {
            VerificationOutcome::Verified => HealingState::Verified,
            VerificationOutcome::NotVerified => HealingState::NotVerified,
            VerificationOutcome::RegressionIntroduced => HealingState::RegressionIntroduced,
        };

        let ver_json = serde_json::to_string(&verification).unwrap_or_default();

        self.store
            .update_healing_proposal_state(
                proposal_id,
                final_state.as_str(),
                None,
                None,
                None,
                None,
                Some(&ver_json),
            )
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        // 5. Quality Graph Calibration
        let strategy = match &proposal.diagnosis_category.as_str() {
            &"SelectorRenamedOrRelocated" => "selector_candidate_replacement",
            &"TimingOrAssertionMismatch" => "timing_and_assertion_recalibration",
            &"RoleGatedVisibilityDifferential" => "role_access_reconciliation",
            other => other,
        };

        let _ = self
            .store
            .record_healing_verification_metric(
                &proposal.project_id,
                strategy,
                &proposal.diagnosis_category,
                outcome.as_str(),
            )
            .await;

        // Upsert Quality Graph Node & Relation
        if let Ok(strategy_node_id) = self
            .store
            .upsert_quality_node(
                &proposal.project_id,
                "healing_strategy",
                strategy,
                &format!("{strategy} ({})", proposal.diagnosis_category),
            )
            .await
        {
            if let Ok(task_node_id) = self
                .store
                .upsert_quality_node(
                    &proposal.project_id,
                    "test",
                    &proposal.task_name,
                    &proposal.task_name,
                )
                .await
            {
                let relation = match outcome {
                    VerificationOutcome::Verified => "healed_by",
                    VerificationOutcome::RegressionIntroduced => "regressed_by",
                    VerificationOutcome::NotVerified => "failed_heal_attempt",
                };
                let _ = self
                    .store
                    .upsert_quality_relation(task_node_id, strategy_node_id, relation)
                    .await;
            }
        }

        // 6. Audit Log
        let _ = self
            .store
            .insert_audit(
                Some(&ctx.name),
                Some(ctx.role.as_str()),
                Some(&proposal.project_id),
                "healing.verification_completed",
                Some(&proposal_id.to_string()),
                Some(&format!(
                    "Outcome: {}, Resolved: {}, Regressions: {}",
                    outcome.as_str(),
                    original_failure_resolved,
                    other_failures
                )),
                "ok",
            )
            .await;

        Ok(verification)
    }

    /// Retrieves structured feedback from historical healing outcomes for `TestPlanningAgent`.
    pub async fn get_feedback_for_planning(
        &self,
        project_id: &str,
    ) -> Result<Vec<HealingFeedback>, GovernedHealingError> {
        let metrics = self
            .store
            .list_healing_strategy_metrics(project_id)
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        let mut feedbacks = Vec::new();
        for m in metrics {
            let total = m.attempts_count;
            let success_rate = if total > 0 {
                m.verified_count as f64 / total as f64
            } else {
                0.0
            };

            let insight = format!(
                "Strategy '{}' for category '{}' succeeded {}/{} times ({:.0}%). Calibrated confidence: {:.2}",
                m.strategy, m.failure_category, m.verified_count, total, success_rate * 100.0, m.calibrated_confidence
            );

            feedbacks.push(HealingFeedback {
                project_id: project_id.to_string(),
                failure_cluster: None,
                diagnosis_category: m.failure_category,
                healing_strategy: m.strategy,
                confidence: m.calibrated_confidence,
                verification_result: format!(
                    "{} verified, {} unverified, {} regressions",
                    m.verified_count, m.not_verified_count, m.regression_count
                ),
                regression_detected: m.regression_count > 0,
                historical_success_count: m.verified_count,
                historical_failure_count: m.not_verified_count + m.regression_count,
                operational_insight: insight,
            });
        }

        Ok(feedbacks)
    }

    /// Fetches a healing proposal by ID.
    pub async fn get_proposal(&self, id: Uuid) -> Result<HealingProposal, GovernedHealingError> {
        let row = self
            .store
            .get_healing_proposal(id)
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?
            .ok_or_else(|| GovernedHealingError::NotFound(format!("proposal '{id}' not found")))?;

        row_to_proposal(row)
    }

    /// Lists healing proposals for a project, optionally filtered by state.
    pub async fn list_proposals(
        &self,
        project_id: &str,
        state: Option<&str>,
    ) -> Result<Vec<HealingProposal>, GovernedHealingError> {
        let rows = self
            .store
            .list_healing_proposals(project_id, state)
            .await
            .map_err(|e| GovernedHealingError::StorageError(e.to_string()))?;

        rows.into_iter().map(row_to_proposal).collect()
    }
}

// ============================================================================
// 6. SERIALIZATION & ROW CONVERSIONS
// ============================================================================

/// What a `HealingProposal` needs in order to be executed later but `HealingProposalRow` has no
/// column of its own for, kept as JSON in its `context` column. Without it a reloaded proposal
/// came back as a `simulated` task in the `test` environment, so `execute_approved` ran the
/// healing plan on the wrong executor (always passing) and skipped the production run gate.
#[derive(Serialize, Deserialize)]
struct StoredProposalContext {
    task_name: String,
    executor: String,
    environment: String,
    target_url: Option<String>,
    timeout_secs: Option<u64>,
    requirement_reference: Option<String>,
    custom_params: Option<serde_json::Value>,
}

fn proposal_to_row(p: &HealingProposal) -> Result<HealingProposalRow, GovernedHealingError> {
    let context = StoredProposalContext {
        task_name: p.task_name.clone(),
        executor: p.executor.clone(),
        environment: p.environment.clone(),
        target_url: p.target_url.clone(),
        timeout_secs: p.timeout_secs,
        requirement_reference: p.requirement_reference.clone(),
        custom_params: p.custom_params.clone(),
    };
    Ok(HealingProposalRow {
        id: p.id,
        project_id: p.project_id.clone(),
        run_id: p.run_id,
        task_id: p.task_id.clone(),
        failure_fingerprint: p.failure_fingerprint.clone(),
        diagnosis_category: p.diagnosis_category.clone(),
        confidence: p.confidence,
        risk_level: p.risk_level.as_str().to_string(),
        requires_approval: p.requires_approval,
        original_actions: serde_json::to_string(&p.original_actions)
            .map_err(|e| GovernedHealingError::Internal(e.to_string()))?,
        proposed_actions: serde_json::to_string(&p.proposed_actions)
            .map_err(|e| GovernedHealingError::Internal(e.to_string()))?,
        ast_diff: serde_json::to_string(&p.ast_diff)
            .map_err(|e| GovernedHealingError::Internal(e.to_string()))?,
        rationale: p.rationale.clone(),
        evidence_refs: serde_json::to_string(&p.evidence_refs)
            .map_err(|e| GovernedHealingError::Internal(e.to_string()))?,
        state: p.state.as_str().to_string(),
        idempotency_key: p.idempotency_key.clone(),
        healing_run_id: p.healing_run_id,
        verification_result: p
            .verification_result
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| GovernedHealingError::Internal(e.to_string()))?,
        approved_by: p.approved_by.clone(),
        approved_at: p.approved_at,
        rejection_reason: p.rejection_reason.clone(),
        created_at: p.created_at,
        expires_at: p.expires_at,
        updated_at: p.updated_at,
        context: Some(
            serde_json::to_string(&context)
                .map_err(|e| GovernedHealingError::Internal(e.to_string()))?,
        ),
    })
}

fn row_to_proposal(r: HealingProposalRow) -> Result<HealingProposal, GovernedHealingError> {
    let original_actions: Vec<Action> = serde_json::from_str(&r.original_actions).map_err(|e| {
        GovernedHealingError::Internal(format!("failed to deserialize original_actions: {e}"))
    })?;
    let proposed_actions: Vec<Action> = serde_json::from_str(&r.proposed_actions).map_err(|e| {
        GovernedHealingError::Internal(format!("failed to deserialize proposed_actions: {e}"))
    })?;
    let ast_diff: AstDiff = serde_json::from_str(&r.ast_diff).map_err(|e| {
        GovernedHealingError::Internal(format!("failed to deserialize ast_diff: {e}"))
    })?;
    let evidence_refs: Vec<String> = serde_json::from_str(&r.evidence_refs).map_err(|e| {
        GovernedHealingError::Internal(format!("failed to deserialize evidence_refs: {e}"))
    })?;
    let verification_result: Option<HealingVerification> = r
        .verification_result
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            GovernedHealingError::Internal(format!(
                "failed to deserialize verification_result: {e}"
            ))
        })?;

    // A row written before the `context` column existed has none. Its real values were never
    // recorded, so it is rebuilt with the fixed defaults every row used to get; a context that is
    // present but unreadable is an error, not a silent fall back to a different executor.
    let context: StoredProposalContext = match r.context.as_deref() {
        Some(json) => serde_json::from_str(json).map_err(|e| {
            GovernedHealingError::Internal(format!("failed to deserialize context: {e}"))
        })?,
        None => StoredProposalContext {
            task_name: format!("Task {}", r.task_id),
            executor: "simulated".to_string(),
            environment: "test".to_string(),
            target_url: None,
            timeout_secs: Some(30),
            requirement_reference: None,
            custom_params: None,
        },
    };

    Ok(HealingProposal {
        id: r.id,
        project_id: r.project_id,
        run_id: r.run_id,
        task_id: r.task_id.clone(),
        task_name: context.task_name,
        executor: context.executor,
        environment: context.environment,
        target_url: context.target_url,
        failure_fingerprint: r.failure_fingerprint,
        diagnosis_category: r.diagnosis_category,
        confidence: r.confidence,
        risk_level: HealingRiskLevel::from_str_lossy(&r.risk_level),
        requires_approval: r.requires_approval,
        original_actions,
        proposed_actions,
        ast_diff,
        rationale: r.rationale,
        evidence_refs,
        state: HealingState::from_str_lossy(&r.state),
        idempotency_key: r.idempotency_key,
        healing_run_id: r.healing_run_id,
        verification_result,
        approved_by: r.approved_by,
        approved_at: r.approved_at,
        rejection_reason: r.rejection_reason,
        created_at: r.created_at,
        expires_at: r.expires_at,
        updated_at: r.updated_at,
        timeout_secs: context.timeout_secs,
        requirement_reference: context.requirement_reference,
        custom_params: context.custom_params,
    })
}
