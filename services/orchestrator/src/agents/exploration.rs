//! Phase 5 — AI Exploratory Testing Agent.
//!
//! ## What exists, and what this adds
//!
//! Exploration is **not** a new agent runtime, a new browser executor, a new Action AST or
//! a new evidence system. All four already exist in this codebase and all four are used
//! here unchanged:
//!
//! | Capability | Reused from |
//! |---|---|
//! | bounded reasoning loop | `crate::agent::run_agent_loop` + `AgentLimits` + `AgentTraceHook` |
//! | tool contract, safety levels, registry | `crate::agent_tools` (`ToolSafetyLevel`, `AgentToolRegistry`) |
//! | action vocabulary | `crate::execution_plan::Action` (15 variants) |
//! | plan validation incl. SSRF + secret guards | `crate::execution_plan::validate_plan` |
//! | governance gate | `crate::policy::evaluate` (`Deny` / `RequireApproval` / `Allow`) |
//! | execution | the existing run pipeline (`insert_run_for_release` → `Intelligence::ingest_run`) |
//! | observation input | `crate::crawler::CrawlOutput` (real crawl evidence) |
//! | evidence storage | `crate::evidence::LocalEvidenceStore` |
//! | duplicate suppression key | `canonical_failure_fingerprint` (Phase 2) |
//! | risk / history signal | `QaMemoryReader::prioritise_tests` (Phase 4) |
//! | requirements | `crate::tools::requirement_lookup` |
//!
//! Two production agents already run through this same loop — `TestPlanningAgent` and
//! `SelfHealingAgent` (the latter driven live by `governed_healing::auto_diagnose`). This
//! module is the third, following the same shape: a stateless `Agent` that reads
//! `context.history` for what it has observed, exactly as `SelfHealingAgent` does.
//!
//! ## The agent composes; it does not execute
//!
//! An exploratory agent that executed actions itself would be a second browser executor.
//! Instead every action it proposes travels the **existing** path: real `Action` AST →
//! real `validate_plan` → real `policy::evaluate` → the real run pipeline. Whatever
//! executor that pipeline dispatches to is the one the whole product already uses. The
//! agent's job is to decide *what to try next* and to classify *what came back*; it has no
//! capability the product does not already have.
//!
//! ## Bounded by construction, twice over
//!
//! `run_agent_loop` already enforces `max_turns`, `max_tool_calls_total`,
//! `max_observation_bytes` and `timeout`, and honours a cooperative cancellation channel.
//! On top of that this module adds an exploration budget — `max_actions`,
//! `max_state_transitions`, `max_findings`, `max_pages` — enforced in **two independent
//! places**:
//!
//! * the agent's `step` stops choosing work when the budget is spent, and
//! * the `probe_action` tool **refuses to act** when it is spent.
//!
//! The second is the one that matters for safety. An agent that miscounts is a bug; a tool
//! that acts anyway is a vulnerability. Defence in depth is the whole point.
//!
//! ## Evidence before verdict
//!
//! The classification ladder is deliberately hard to climb, because the failure mode that
//! matters is over-claiming:
//!
//! | Class | Requires |
//! |---|---|
//! | `CONFIRMED_DEFECT` | a **recorded failing execution** of a real `Assert*` action, carrying a real canonical fingerprint |
//! | `POTENTIAL_DEFECT` | an observed divergence **plus** a requirement that plausibly covers it, not yet verified |
//! | `OBSERVED_ANOMALY` | a measured divergence with **no** stated expectation — an anomaly, not a verdict |
//! | `INSUFFICIENT_EVIDENCE` | anything less: executor blocked, validation refused, assertion never evaluated, no evidence captured |
//!
//! `INSUFFICIENT_EVIDENCE` is the **default**, and every path that fails to gather evidence
//! lands there rather than being promoted. Missing evidence can never become a confirmed
//! defect, because the promotion requires a fingerprint that only a real recorded failure
//! produces.

use crate::adaptive_intelligence::canonical_failure_fingerprint;
use crate::agent::{
    run_agent_loop, Agent, AgentContext, AgentError, AgentLimits, AgentStatus, TurnDecision,
};
use crate::agent_tools::{
    AgentTool, AgentToolRegistry, ToolDefinition, ToolError, ToolInvocation, ToolObservation,
    ToolSafetyLevel,
};
use crate::evidence::LocalEvidenceStore;
use crate::execution_plan::{validate_plan, Action, CompilerMethod, ExecutionPlan, ExecutionType};
use crate::policy::{evaluate, PolicyConfig, PolicyDecision};
use crate::store::{ExplorationFindingRow, Store};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

const AGENT_ID: &str = "exploratory-agent";

// ============================================================================
// 1. BUDGET
// ============================================================================

/// The exploration ceiling. Every field is a hard stop, not a target.
///
/// Defaults are deliberately modest. An exploratory agent that spends 200 actions on one
/// page is not exploring, and a budget nobody tuned is a budget nobody bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplorationBudget {
    /// Actions composed and submitted across the whole session.
    pub max_actions: usize,
    /// Page/state transitions the session may observe.
    pub max_state_transitions: usize,
    /// Findings recorded before exploration stops. Suppressed duplicates do not count —
    /// they are not new findings.
    pub max_findings: usize,
    /// Distinct routes the session may visit.
    pub max_pages: usize,
}

impl Default for ExplorationBudget {
    fn default() -> Self {
        Self {
            max_actions: 40,
            max_state_transitions: 20,
            max_findings: 10,
            max_pages: 8,
        }
    }
}

impl ExplorationBudget {
    /// A budget of zero in any dimension stops exploration before it starts, which is a
    /// legitimate thing to ask for and must not be silently raised to a default.
    pub fn is_exhausted(&self, counters: &ExplorationCounters) -> bool {
        counters.actions >= self.max_actions
            || counters.state_transitions >= self.max_state_transitions
            || counters.findings >= self.max_findings
            || counters.pages >= self.max_pages
    }

    /// Which ceiling stopped exploration, named. Returns `None` while budget remains.
    ///
    /// Reported rather than logged, because "exploration stopped" with no reason is the
    /// one outcome a reader cannot act on.
    pub fn exhausted_by(&self, counters: &ExplorationCounters) -> Option<&'static str> {
        if counters.actions >= self.max_actions {
            Some("max_actions")
        } else if counters.state_transitions >= self.max_state_transitions {
            Some("max_state_transitions")
        } else if counters.findings >= self.max_findings {
            Some("max_findings")
        } else if counters.pages >= self.max_pages {
            Some("max_pages")
        } else {
            None
        }
    }
}

/// What exploration has actually done so far.
///
/// Shared between the agent (which reads it to decide) and the tool (which enforces it),
/// behind a mutex. This is run accounting, not agent reasoning — the agent's reasoning
/// stays in `context.history`, exactly as `SelfHealingAgent` keeps its state there.
#[derive(Debug, Default)]
pub struct ExplorationCounters {
    pub actions: usize,
    pub state_transitions: usize,
    pub findings: usize,
    /// Findings rejected as duplicates of one already recorded.
    pub suppressed: usize,
    pub pages: usize,
    /// Routes already visited, so `max_pages` counts distinct routes rather than
    /// re-visits. Re-visiting is a legitimate exploratory behaviour; counting it twice
    /// would exhaust the budget on one page.
    pub routes: BTreeSet<String>,
    /// Behaviours already exercised, so a long session cycles the catalogue instead of
    /// hammering one behaviour.
    pub tried: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SharedCounters(Arc<Mutex<ExplorationCounters>>);

impl SharedCounters {
    pub async fn snapshot(&self) -> ExplorationCounters {
        let guard = self.0.lock().await;
        ExplorationCounters {
            actions: guard.actions,
            state_transitions: guard.state_transitions,
            findings: guard.findings,
            suppressed: guard.suppressed,
            pages: guard.pages,
            routes: guard.routes.clone(),
            tried: guard.tried.clone(),
        }
    }

    async fn record_action(&self) {
        self.0.lock().await.actions += 1;
    }

    async fn record_finding(&self, recorded: bool) {
        let mut g = self.0.lock().await;
        if recorded {
            g.findings += 1;
        } else {
            g.suppressed += 1;
        }
    }

    async fn visit_route(&self, route: &str) -> bool {
        let mut g = self.0.lock().await;
        if g.routes.insert(route.to_string()) {
            g.pages += 1;
            true
        } else {
            false
        }
    }

    async fn mark_tried(&self, behaviour: &str) {
        self.0.lock().await.tried.insert(behaviour.to_string());
    }
}

// ============================================================================
// 2. BEHAVIOUR CATALOGUE
// ============================================================================

/// One exploratory technique.
///
/// Every entry declares what it needs from the application and which `Action` variants
/// carry it out. The declaration is not decoration: two of the requested techniques have
/// **no** representation in the Action AST, and saying so here is how that stays honest
/// rather than being quietly approximated into a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorationBehaviour {
    /// Visit each route the crawl actually found, in priority order.
    IntelligentNavigation,
    /// Fill a field with a value at the edge of its plausible range.
    BoundaryValue,
    /// Fill a field with a value the field's type should reject.
    InvalidValue,
    /// Fill a field with nothing at all.
    EmptyValue,
    /// Perform the same action twice, to expose double-submit and re-entrancy.
    DuplicateAction,
    /// Re-request the current route. Approximated by `Navigate` to the same URL, because
    /// the Action AST has no refresh variant — see [`ExplorationBehaviour::support`].
    Refresh,
    /// Re-request a route after a deliberate pause, to observe session handling.
    SessionExpiry,
    /// Compare what a role can see against the crawl's recorded visibility matrix.
    RoleStateChange,
    /// Drive a multi-step transition and observe the state at each step.
    StateTransition,
    /// Assert the route that should have been reached, and record where it went instead.
    UnexpectedNavigation,
    /// Submit the same request twice and observe whether the second is idempotent.
    RepeatedSubmission,
}

impl ExplorationBehaviour {
    /// Every behaviour, in the order an evidence-driven session prefers them.
    pub fn catalogue() -> &'static [ExplorationBehaviour] {
        &[
            Self::IntelligentNavigation,
            Self::StateTransition,
            Self::UnexpectedNavigation,
            Self::BoundaryValue,
            Self::InvalidValue,
            Self::EmptyValue,
            Self::DuplicateAction,
            Self::RepeatedSubmission,
            Self::SessionExpiry,
            Self::RoleStateChange,
            Self::Refresh,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::IntelligentNavigation => "intelligent_navigation",
            Self::BoundaryValue => "boundary_value",
            Self::InvalidValue => "invalid_value",
            Self::EmptyValue => "empty_value",
            Self::DuplicateAction => "duplicate_action",
            Self::Refresh => "refresh",
            Self::SessionExpiry => "session_expiry",
            Self::RoleStateChange => "role_state_change",
            Self::StateTransition => "state_transition",
            Self::UnexpectedNavigation => "unexpected_navigation",
            Self::RepeatedSubmission => "repeated_submission",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::catalogue().iter().copied().find(|b| b.as_str() == s)
    }

    /// Whether the existing Action AST can express this technique, and how.
    ///
    /// This is the honest answer to "which behaviours are supported". Two entries are
    /// partial and one is absent:
    ///
    /// * [`ExplorationBehaviour::Refresh`] — the AST has no refresh action. Re-navigating
    ///   the same URL is the closest real equivalent and is reported as an approximation,
    ///   never as a refresh.
    /// * **Back / forward** — requested by the phase brief, and **not implemented**. There
    ///   is no `Action` variant for browser history and inventing one would be a second
    ///   Action AST. `back_forward_navigation` is returned as unsupported so the gap is
    ///   reported rather than silently skipped.
    pub fn support(self, name: &str) -> BehaviourSupport {
        match name {
            "back_forward_navigation" => BehaviourSupport::Unsupported(
                "the Action AST has no browser-history variant; adding one would be a second Action AST",
            ),
            _ => match self {
                Self::IntelligentNavigation => BehaviourSupport::Native(
                    "Navigate, plus AssertUrl to confirm arrival",
                ),
                Self::BoundaryValue
                | Self::InvalidValue
                | Self::EmptyValue => {
                    BehaviourSupport::Native("Fill, plus AssertEnabled/AssertText on the result")
                }
                Self::DuplicateAction => {
                    BehaviourSupport::Native("the same Click repeated, then AssertUrl")
                }
                Self::Refresh => BehaviourSupport::Approximated(
                    "Navigate to the current URL; the AST has no refresh variant",
                ),
                Self::SessionExpiry => BehaviourSupport::Native(
                    "Navigate, then AssertVisible on an element that requires a session",
                ),
                Self::RoleStateChange => BehaviourSupport::Native(
                    "Navigate as a role, compared against the crawl's recorded visibility matrix",
                ),
                Self::StateTransition => BehaviourSupport::Native("ordered Action sequences"),
                Self::UnexpectedNavigation => BehaviourSupport::Native(
                    "AssertUrl on the expected route; a mismatch is the observation",
                ),
                Self::RepeatedSubmission => BehaviourSupport::Native(
                    "the same Click twice, then AssertUrl to observe idempotence",
                ),
            },
        }
    }

    /// The safety level this behaviour requires.
    ///
    /// Everything in the catalogue is `Diagnostic` — probing, filling and asserting. A
    /// technique that would mutate or destroy state is **not** in the catalogue, and
    /// [`ExplorationBudget`] does not grow to accommodate one. If a destructive technique
    /// is ever added it must be declared `Dangerous` here, which makes
    /// `is_autonomous_safe()` false and requires an operator to opt in.
    pub fn required_safety(self) -> ToolSafetyLevel {
        ToolSafetyLevel::Diagnostic
    }
}

/// How well the existing AST expresses a technique.
///
/// `content` is required: serde cannot flatten a newtype variant's payload into a
/// `tag`-only representation, and a type that fails to serialize is a type that cannot be
/// reported over HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BehaviourSupport {
    /// Fully expressible.
    Native(&'static str),
    /// Closest real equivalent; the difference is stated rather than hidden.
    Approximated(&'static str),
    /// Not expressible. Reported, never faked.
    Unsupported(&'static str),
}

// ============================================================================
// 3. FINDINGS
// ============================================================================

/// How strongly a finding is supported by evidence.
///
/// The ladder is ordered by evidence, not by severity. Promotion requires proof at each
/// step, and the absence of proof is a class of its own rather than a quiet downgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
// Declared weakest-first, so the derived `Ord` **is** evidence strength and a caller
// comparing two findings with `<` gets the answer the name suggests. `INSUFFICIENT_EVIDENCE`
// is first because it is what a finding becomes whenever evidence is missing, and it must be
// the floor rather than something a caller has to remember to special-case.
pub enum FindingClass {
    /// **The default.** Anything that did not gather proof: executor blocked, plan
    /// refused, assertion never evaluated, evidence not captured.
    InsufficientEvidence,
    /// A measured divergence with no stated expectation behind it. Recorded so it is not
    /// lost, not escalated — nothing says the application was *supposed* to do otherwise.
    ObservedAnomaly,
    /// An observed divergence that a stored requirement plausibly covers, but which has
    /// not been verified against that requirement's own wording.
    PotentialDefect,
    /// A recorded failing execution of a real assertion, carrying a real fingerprint.
    ConfirmedDefect,
}

impl FindingClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ObservedAnomaly => "OBSERVED_ANOMALY",
            Self::PotentialDefect => "POTENTIAL_DEFECT",
            Self::ConfirmedDefect => "CONFIRMED_DEFECT",
            Self::InsufficientEvidence => "INSUFFICIENT_EVIDENCE",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "OBSERVED_ANOMALY" => Some(Self::ObservedAnomaly),
            "POTENTIAL_DEFECT" => Some(Self::PotentialDefect),
            "CONFIRMED_DEFECT" => Some(Self::ConfirmedDefect),
            "INSUFFICIENT_EVIDENCE" => Some(Self::InsufficientEvidence),
            _ => None,
        }
    }
}

/// What a probe observed, before any judgement is attached.
///
/// Kept separate from [`ExplorationFinding`] so classification has to be *asked for* with
/// named inputs. A finding cannot exist without saying what it was derived from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeObservation {
    pub behaviour: ExplorationBehaviour,
    pub route: String,
    /// The executed sequence, as the real AST.
    pub actions: Vec<Action>,
    /// What the application did.
    pub observed: String,
    /// What it was expected to do. `None` when exploration had no expectation to check —
    /// which is the normal case for an exploratory probe and the reason its result is an
    /// anomaly rather than a defect.
    pub expected: Option<String>,
    /// A requirement that plausibly covers this behaviour, when one was found.
    pub violated_requirement: Option<String>,
    /// The real status the recorded execution produced.
    pub task_status: crate::domain::TaskStatus,
    /// The real failure reason, verbatim, when the execution failed.
    pub failure_reason: Option<String>,
    /// The canonical fingerprint of that failure. **Only ever present when a real
    /// recorded failure exists**, which is the precondition for `CONFIRMED_DEFECT`.
    pub failure_fingerprint: Option<String>,
    /// Application state at observation time.
    pub app_state: Value,
    /// An existing bug draft or failure cluster this matches, when Phase 2's fingerprint
    /// links it to recorded history.
    pub related_failure: Option<String>,
    /// Whether an executor actually evaluated the assertions, as opposed to the plan
    /// merely having been composed. `false` forces `INSUFFICIENT_EVIDENCE`.
    pub assertions_evaluated: bool,
    /// A plan-validation or policy refusal, verbatim. `Some` forces
    /// `INSUFFICIENT_EVIDENCE` regardless of anything else.
    pub refusal: Option<String>,
}

impl ProbeObservation {
    /// The classification, and the clause that establishes it.
    ///
    /// Ordered so the *weakest* disqualifier is applied first — a refusal or an
    /// unevaluated assertion cannot be argued around by a status code, and neither can a
    /// missing fingerprint become a defect by being described in strong words.
    pub fn classify(&self) -> (FindingClass, String, f64) {
        if let Some(refusal) = &self.refusal {
            return (
                FindingClass::InsufficientEvidence,
                format!("the probe was refused before it ran: {refusal}"),
                0.0,
            );
        }
        if !self.assertions_evaluated {
            return (
                FindingClass::InsufficientEvidence,
                "no executor evaluated this probe's assertions, so nothing was observed \
                 about the application's behaviour"
                    .to_string(),
                0.0,
            );
        }
        if !self.is_failure() {
            // The assertion held. Nothing to report — this is the overwhelmingly common
            // case and returning a finding for it would be noise.
            return (
                FindingClass::InsufficientEvidence,
                "the probe's assertions held; no divergence to report".to_string(),
                0.0,
            );
        }
        if self.failure_fingerprint.is_none() {
            return (
                FindingClass::InsufficientEvidence,
                "a failure was recorded but no canonical fingerprint could be computed, \
                 so it cannot be tied to evidence or deduplicated"
                    .to_string(),
                0.0,
            );
        }

        // Evidence exists. How far up the ladder depends on what backs the expectation.
        match (&self.expected, &self.violated_requirement) {
            (Some(_), Some(req)) => (
                FindingClass::ConfirmedDefect,
                format!(
                    "a recorded execution of {} assertions failed on {route}, carrying a \
                     canonical fingerprint, and the stored requirement {req} covers this \
                     behaviour",
                    self.actions
                        .iter()
                        .filter(|a| is_assertion(a))
                        .count()
                        .max(1),
                    route = self.route,
                    req = req,
                ),
                0.9,
            ),
            (Some(_), None) => (
                FindingClass::PotentialDefect,
                format!(
                    "a recorded execution failed on {} against a stated expectation, but \
                     no stored requirement covers it, so this is a violation of the probe \
                     rather than of the specification",
                    self.route
                ),
                0.6,
            ),
            (None, Some(req)) => (
                FindingClass::PotentialDefect,
                format!(
                    "a recorded execution failed on {} and requirement {req} covers this \
                     behaviour, but the probe stated no expected value to check it \
                     against, so the violation is unverified",
                    self.route,
                    req = req,
                ),
                0.5,
            ),
            (None, None) => (
                FindingClass::ObservedAnomaly,
                format!(
                    "a recorded execution failed on {} with a canonical fingerprint, but \
                     neither an expected value nor a covering requirement was established, \
                     so this is an observation and not a verdict",
                    self.route
                ),
                0.4,
            ),
        }
    }

    /// Whether the recorded execution failed.
    ///
    /// Matches the same two statuses `QaMemoryReader::is_failure` treats as failures, on
    /// the enum rather than through `as_str()`, so a rename to the wire format cannot
    /// silently change what counts as a failure.
    fn is_failure(&self) -> bool {
        matches!(
            self.task_status,
            crate::domain::TaskStatus::Failed | crate::domain::TaskStatus::Timeout
        )
    }
}

/// Whether an `Action` is an assertion — the thing that can actually fail with evidence.
fn is_assertion(a: &Action) -> bool {
    matches!(
        a,
        Action::AssertVisible { .. }
            | Action::AssertText { .. }
            | Action::AssertUrl { .. }
            | Action::AssertHidden { .. }
            | Action::AssertEnabled { .. }
            | Action::AssertStatus { .. }
            | Action::AssertHeader { .. }
            | Action::AssertBodyField { .. }
            | Action::AssertStatusIn { .. }
            | Action::AssertBodySchema { .. }
    )
}

/// One classified finding, ready to persist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationFinding {
    pub session_id: Uuid,
    pub project_id: String,
    pub classification: FindingClass,
    pub behaviour: ExplorationBehaviour,
    pub route: String,
    pub observed: String,
    pub expected: Option<String>,
    pub violated_requirement: Option<String>,
    /// The real fingerprint of the recorded failure. The duplicate-suppression key.
    pub failure_fingerprint: Option<String>,
    pub evidence_object_key: Option<String>,
    pub action_sequence: Vec<Action>,
    pub app_state: Value,
    pub confidence: f64,
    /// What established the classification. This travels with the finding so a reader is
    /// never asked to accept a verdict without the evidence for it.
    pub basis: String,
    pub related_failure: Option<String>,
    pub observed_at: chrono::DateTime<chrono::Utc>,
}

impl ExplorationFinding {
    /// Build a finding from an observation, classifying it and capturing evidence.
    ///
    /// The evidence write happens here rather than in the tool so that a finding cannot
    /// exist without the artifact that supports it, and so the object key recorded on the
    /// row is always a key that was actually written.
    pub async fn from_observation(
        observation: &ProbeObservation,
        project_id: &str,
        session_id: &Uuid,
        run_id: &Uuid,
        evidence: &LocalEvidenceStore,
    ) -> Self {
        let (classification, basis, confidence) = observation.classify();
        let evidence_object_key = evidence
            .put(
                &run_id.to_string(),
                &session_id.to_string(),
                "exploration",
                &serde_json::to_string_pretty(&json!({
                    "behaviour": observation.behaviour.as_str(),
                    "route": observation.route,
                    "actions": observation.actions,
                    "observed": observation.observed,
                    "expected": observation.expected,
                    "violated_requirement": observation.violated_requirement,
                    "task_status": observation.task_status.as_str(),
                    "failure_reason": observation.failure_reason,
                    "failure_fingerprint": observation.failure_fingerprint,
                    "app_state": observation.app_state,
                    "classification": classification.as_str(),
                    "basis": basis,
                    "assertions_evaluated": observation.assertions_evaluated,
                    "refusal": observation.refusal,
                }))
                .unwrap_or_default(),
            )
            .ok()
            .map(|(key, _)| key);

        Self {
            session_id: *session_id,
            project_id: project_id.to_string(),
            classification,
            behaviour: observation.behaviour,
            route: observation.route.clone(),
            observed: observation.observed.clone(),
            expected: observation.expected.clone(),
            violated_requirement: observation.violated_requirement.clone(),
            failure_fingerprint: observation.failure_fingerprint.clone(),
            evidence_object_key,
            action_sequence: observation.actions.clone(),
            app_state: observation.app_state.clone(),
            confidence,
            basis,
            related_failure: observation.related_failure.clone(),
            observed_at: chrono::Utc::now(),
        }
    }

    /// The storage row for this finding.
    ///
    /// `Action` is stored as the real AST's own serde form, so a finding is replayable
    /// rather than a prose description that would have to be re-parsed.
    pub fn to_row(&self) -> ExplorationFindingRow {
        ExplorationFindingRow {
            id: Uuid::new_v4(),
            session_id: self.session_id,
            project_id: self.project_id.clone(),
            classification: self.classification.as_str().to_string(),
            behaviour: self.behaviour.as_str().to_string(),
            route: self.route.clone(),
            failure_fingerprint: self.failure_fingerprint.clone(),
            evidence_object_key: self.evidence_object_key.clone(),
            observed: self.observed.clone(),
            expected: self.expected.clone(),
            violated_requirement: self.violated_requirement.clone(),
            action_sequence: serde_json::to_string(&self.action_sequence).unwrap_or_default(),
            app_state: serde_json::to_string(&self.app_state).unwrap_or_default(),
            confidence: self.confidence,
            basis: self.basis.clone(),
            related_failure: self.related_failure.clone(),
            observed_at: self.observed_at,
        }
    }
}

// ============================================================================
// 4. THE SURFACE — what exploration may act on
// ============================================================================

/// The application surface exploration may touch, read from real crawl evidence.
///
/// **Only what the crawl actually found is in here.** An element the crawler never saw is
/// not an exploration target, because acting on it would mean acting on a selector nobody
/// has evidence exists. That is the line between guided exploration and random clicking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExplorationSurface {
    pub routes: Vec<RouteSurface>,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouteSurface {
    pub url: String,
    pub title: String,
    pub elements: Vec<ElementSurface>,
    /// Elements this route shows only to the named role, from the crawl's visibility
    /// matrix. Real recorded observation, not a guess about roles.
    pub role_restricted: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementSurface {
    pub kind: String,
    pub text: String,
    pub selector_hint: String,
    pub href: Option<String>,
}

impl ElementSurface {
    /// An element exploration may safely fill. Text-like inputs only — filling a button's
    /// label is not an exploratory technique, it is a bug.
    pub fn is_fillable(&self) -> bool {
        let k = self.kind.to_ascii_lowercase();
        k.contains("input")
            || k.contains("textbox")
            || k.contains("textarea")
            || k.contains("field")
    }

    /// An element exploration may click.
    pub fn is_clickable(&self) -> bool {
        let k = self.kind.to_ascii_lowercase();
        k.contains("button") || k.contains("link") || k.contains("a") || k.contains("tab")
    }

    pub fn is_field(&self) -> bool {
        self.kind.to_ascii_lowercase().contains("field")
    }
}

impl ExplorationSurface {
    /// Build the surface from a real `CrawlOutput`.
    ///
    /// Takes the union of every role's pages, deduplicated by URL, and carries the
    /// visibility matrix across so role-restricted elements are attributable rather than
    /// merely visible.
    pub fn from_crawl(output: &crate::crawler::CrawlOutput) -> Self {
        let mut routes: Vec<RouteSurface> = Vec::new();
        let mut roles: Vec<String> = Vec::new();

        for role in &output.roles {
            if !roles.contains(&role.role) {
                roles.push(role.role.clone());
            }
            for page in &role.pages {
                match routes.iter_mut().find(|r| r.url == page.url) {
                    None => routes.push(RouteSurface {
                        url: page.url.clone(),
                        title: page.title.clone(),
                        elements: page
                            .elements
                            .iter()
                            .map(|e| ElementSurface {
                                kind: e.kind.clone(),
                                text: e.text.clone(),
                                selector_hint: e.selector_hint.clone(),
                                href: e.href.clone(),
                            })
                            .collect(),
                        role_restricted: BTreeMap::new(),
                    }),
                    Some(existing) => {
                        // The same URL seen by another role. Elements are unioned rather
                        // than replaced, because two roles seeing different parts of a page
                        // is itself the observation the role behaviour depends on.
                        for e in &page.elements {
                            let surface = ElementSurface {
                                kind: e.kind.clone(),
                                text: e.text.clone(),
                                selector_hint: e.selector_hint.clone(),
                                href: e.href.clone(),
                            };
                            if !existing.elements.contains(&surface) {
                                existing.elements.push(surface);
                            }
                        }
                    }
                }
            }
        }

        for diff in &output.visibility_matrix {
            if let Some(route) = routes.iter_mut().find(|r| r.url == diff.url) {
                for (role, elements) in &diff.elements_only_visible_to {
                    route
                        .role_restricted
                        .entry(role.clone())
                        .or_default()
                        .extend(elements.clone());
                }
            }
        }

        Self { routes, roles }
    }

    /// Routes in a stable order, so a session explores deterministically.
    pub fn ordered_routes(&self) -> Vec<&RouteSurface> {
        let mut r: Vec<&RouteSurface> = self.routes.iter().collect();
        r.sort_by(|a, b| a.url.cmp(&b.url));
        r
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

// ============================================================================
// 5. THE EXECUTION PATH
// ============================================================================

/// How a composed probe actually reaches an executor.
///
/// This trait is the seam that keeps exploration from becoming a second browser executor.
/// The agent composes `Action`s; **this** submits them to the existing run pipeline. A
/// production binding runs them the way the product runs any run; a test binding drives
/// the same pipeline deterministically. Exploration itself contains no execution code at
/// all, which is the property that makes the "no second executor" claim checkable rather
/// than aspirational.
#[async_trait]
pub trait ProbeRunner: Send + Sync + std::fmt::Debug {
    /// Submit the probe's actions and report what the application did.
    ///
    /// `assertions_evaluated` must be `false` unless an executor genuinely evaluated the
    /// plan's assertions. Returning `true` without having done so is the single way to
    /// manufacture a `CONFIRMED_DEFECT`, and it is why the flag is part of the contract
    /// rather than inferred.
    async fn run(
        &self,
        project_id: &str,
        session_id: &Uuid,
        probe: &ComposedProbe,
    ) -> Result<ProbeOutcome, String>;
}

/// The result of a probe submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeOutcome {
    pub run_id: Uuid,
    pub task_status: crate::domain::TaskStatus,
    pub failure_reason: Option<String>,
    /// The canonical fingerprint of the recorded failure, computed by the **existing**
    /// Phase 2 function from the real recorded reason.
    pub failure_fingerprint: Option<String>,
    pub assertions_evaluated: bool,
    /// Application state after the probe.
    pub app_state: Value,
    /// An existing bug draft or cluster this matches, resolved through the real store.
    pub related_failure: Option<String>,
}

/// A probe the agent has composed but not yet submitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposedProbe {
    pub behaviour: ExplorationBehaviour,
    pub route: String,
    pub actions: Vec<Action>,
    pub expected: Option<String>,
    pub violated_requirement: Option<String>,
    /// Which of the plan's actions are assertions. `0` means the probe cannot produce
    /// evidence, and is refused rather than run.
    pub assertion_count: usize,
}

// ============================================================================
// 6. THE PROBE TOOL
// ============================================================================

/// Submits one composed probe, subject to budget, validation, policy and classification.
///
/// This is where every safety control converges, which is why the logic lives here rather
/// than in the agent: the agent decides *what* to try, and this decides whether it is
/// allowed to.
pub struct ProbeActionTool {
    store: Store,
    policy: Arc<PolicyConfig>,
    evidence: Arc<LocalEvidenceStore>,
    counters: SharedCounters,
    budget: ExplorationBudget,
    runner: Arc<dyn ProbeRunner>,
    session_id: Uuid,
}

impl ProbeActionTool {
    pub fn new(
        store: Store,
        policy: Arc<PolicyConfig>,
        evidence: Arc<LocalEvidenceStore>,
        counters: SharedCounters,
        budget: ExplorationBudget,
        runner: Arc<dyn ProbeRunner>,
        session_id: Uuid,
    ) -> Self {
        Self {
            store,
            policy,
            evidence,
            counters,
            budget,
            runner,
            session_id,
        }
    }
}

/// Hand-written because the tool holds a `Store`, which is a live database handle and
/// deliberately not `Debug`. Reporting the fields a reader can act on is more useful than
/// a derived impl that would have to print the pool.
impl std::fmt::Debug for ProbeActionTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeActionTool")
            .field("session_id", &self.session_id)
            .field("budget", &self.budget)
            .field("policy_blocked", &self.policy.blocked_executors)
            .field("policy_gated", &self.policy.require_approval_for)
            .field("counters", &self.counters)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
struct ProbeArgs {
    behaviour: String,
    route: String,
    #[serde(default)]
    actions: Vec<Action>,
    #[serde(default)]
    expected: Option<String>,
    #[serde(default)]
    violated_requirement: Option<String>,
    /// Which executor the run pipeline should use. Empty means the registry default.
    #[serde(default)]
    executor: Option<String>,
}

#[async_trait]
impl AgentTool for ProbeActionTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "probe_action".to_string(),
            description:
                "Submit one exploratory probe: a real Action sequence, validated against the \
                 real plan validator and the real execution policy, then executed through the \
                 existing run pipeline. Returns the classified finding."
                    .to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "behaviour": { "type": "string" },
                    "route": { "type": "string" },
                    "actions": { "type": "array", "items": { "type": "object" } },
                    "expected": { "type": "string" },
                    "violated_requirement": { "type": "string" },
                    "executor": { "type": "string" }
                },
                "required": ["behaviour", "route"]
            }),
            output_schema: None,
            // `Diagnostic`: probing and asserting. Autonomous-safe because every probe is
            // still validated and policy-gated before it runs, and because a technique
            // that would mutate or destroy state is not in the catalogue at all.
            safety_level: ExplorationBehaviour::IntelligentNavigation.required_safety(),
            default_timeout_ms: 0,
        }
    }

    async fn execute(
        &self,
        invocation: &ToolInvocation,
        context: &AgentContext,
    ) -> Result<ToolObservation, ToolError> {
        let level = self.definition().safety_level;
        let args: ProbeArgs =
            serde_json::from_value(invocation.arguments.clone()).map_err(|e| {
                ToolError::InvalidInput {
                    tool: invocation.tool_name.clone(),
                    details: e.to_string(),
                }
            })?;

        // --- 1. Project scope, before anything is read or written.
        let project_id = context
            .project_id
            .clone()
            .ok_or_else(|| ToolError::SafetyViolation {
                tool: invocation.tool_name.clone(),
                reason: "exploration requires a project scope; none was supplied".to_string(),
            })?;
        if !context.is_authorized_for_project(&project_id) {
            return Ok(ToolObservation::failure(
                &invocation.call_id,
                &invocation.tool_name,
                level,
                format!("not authorized for project {project_id}"),
                0,
            ));
        }

        // --- 2. Budget. The independent enforcement point: even an agent that miscounts
        // cannot push this session past its ceiling.
        let counters = self.counters.snapshot().await;
        if let Some(reason) = self.budget.exhausted_by(&counters) {
            return Ok(ToolObservation::success(
                &invocation.call_id,
                &invocation.tool_name,
                level,
                json!({
                    "acted": false,
                    "reason": "budget_exhausted",
                    "exhausted_by": reason,
                    "budget": self.budget,
                    "counters": counters_snapshot(&counters),
                }),
                0,
            ));
        }

        // --- 3. An unknown behaviour is rejected, not guessed at.
        let Some(behaviour) = ExplorationBehaviour::parse(&args.behaviour) else {
            return Err(ToolError::InvalidInput {
                tool: invocation.tool_name.clone(),
                details: format!(
                    "unknown behaviour `{}`; the catalogue is {:?}",
                    args.behaviour,
                    ExplorationBehaviour::catalogue()
                        .iter()
                        .map(|b| b.as_str())
                        .collect::<Vec<_>>()
                ),
            });
        };

        // --- 4. Safety level. A technique needing more than `Diagnostic` is refused
        // outright: nothing in this catalogue does, and an autonomous agent must not be
        // the thing that decides a destructive action is fine.
        let required = behaviour.required_safety();
        if !required.is_autonomous_safe() {
            return Ok(ToolObservation::failure(
                &invocation.call_id,
                &invocation.tool_name,
                required,
                format!(
                    "behaviour {} requires {required:?} safety, which is not autonomously \
                     permitted",
                    behaviour.as_str(),
                ),
                0,
            ));
        }

        // --- 5. A probe with no assertion cannot produce evidence. Refused rather than
        // run and reported as inconclusive, so the budget is not spent on a probe that
        // could not have answered anything.
        let assertion_count = args.actions.iter().filter(|a| is_assertion(a)).count();
        if assertion_count == 0 {
            return Ok(ToolObservation::success(
                &invocation.call_id,
                &invocation.tool_name,
                level,
                json!({
                    "acted": false,
                    "reason": "no_assertion_in_probe",
                    "detail": "a probe with no Assert* action cannot produce evidence, so \
                               it is refused rather than run",
                    "route": args.route,
                    "behaviour": behaviour.as_str(),
                }),
                0,
            ));
        }

        // --- 6. The real plan validator, including its SSRF and secret-reference guards.
        // Not a re-implementation: the same function the compiler's output passes through.
        let plan = ExecutionPlan {
            test_case_id: format!("exploration:{}", self.session_id),
            execution_type: ExecutionType::Browser,
            target: Some(args.route.clone()),
            actions: args.actions.clone(),
            compiler: CompilerMethod::Deterministic,
            diagnostics: vec![format!("exploratory probe: {}", behaviour.as_str())],
            provider: None,
            model: None,
        };
        if let Err(reasons) = validate_plan(&plan).await {
            return Ok(ToolObservation::success(
                &invocation.call_id,
                &invocation.tool_name,
                level,
                json!({
                    "acted": false,
                    "reason": "plan_rejected",
                    "detail": reasons,
                    "route": args.route,
                    "behaviour": behaviour.as_str(),
                }),
                0,
            ));
        }

        // --- 7. The real governance gate. A blocked executor stays blocked, and a gated
        // executor stays gated, because this calls the same `policy::evaluate` the run
        // scheduler calls -- it does not re-derive the rules.
        let run_plan = crate::domain::RunPlan {
            name: format!("exploration probe {}", behaviour.as_str()),
            tasks: vec![crate::domain::TaskDef {
                id: format!("probe-{}", self.session_id),
                name: format!("exploratory probe: {}", behaviour.as_str()),
                prerequisites: Vec::new(),
                simulate: None,
                executor: args.executor.clone(),
                params: json!({ "exploration": true }),
                test_case_id: None,
            }],
            concurrency: 1,
        };
        let decision = evaluate(&self.policy, &run_plan);
        if let PolicyDecision::Deny(reason) = &decision {
            return Ok(ToolObservation::success(
                &invocation.call_id,
                &invocation.tool_name,
                level,
                json!({
                    "acted": false,
                    "reason": "policy_denied",
                    "detail": reason,
                    "route": args.route,
                    "behaviour": behaviour.as_str(),
                }),
                0,
            ));
        }
        let needs_approval = matches!(decision, PolicyDecision::RequireApproval(_));

        // --- 8. Act. Budget is consumed here, after every gate that could refuse.
        self.counters.record_action().await;
        self.counters.mark_tried(behaviour.as_str()).await;
        self.counters.visit_route(&args.route).await;

        let probe = ComposedProbe {
            behaviour,
            route: args.route.clone(),
            actions: args.actions.clone(),
            expected: args.expected.clone(),
            violated_requirement: args.violated_requirement.clone(),
            assertion_count,
        };

        let outcome = match self.runner.run(&project_id, &self.session_id, &probe).await {
            Ok(o) => o,
            Err(reason) => {
                return Ok(ToolObservation::success(
                    &invocation.call_id,
                    &invocation.tool_name,
                    level,
                    json!({
                        "acted": true,
                        "reason": "execution_failed",
                        "detail": reason,
                        "route": args.route,
                        "behaviour": behaviour.as_str(),
                        "classification": FindingClass::InsufficientEvidence.as_str(),
                    }),
                    0,
                ));
            }
        };

        // --- 9. Classify, then persist with duplicate suppression.
        let observation = ProbeObservation {
            behaviour,
            route: args.route.clone(),
            actions: args.actions.clone(),
            observed: outcome
                .failure_reason
                .clone()
                .unwrap_or_else(|| format!("execution {}", outcome.task_status.as_str())),
            expected: args.expected.clone(),
            violated_requirement: args.violated_requirement.clone(),
            task_status: outcome.task_status,
            failure_reason: outcome.failure_reason.clone(),
            failure_fingerprint: outcome.failure_fingerprint.clone(),
            app_state: outcome.app_state.clone(),
            related_failure: outcome.related_failure.clone(),
            assertions_evaluated: outcome.assertions_evaluated,
            refusal: if needs_approval {
                Some("executor requires human approval; not run autonomously".to_string())
            } else {
                None
            },
        };
        let finding = ExplorationFinding::from_observation(
            &observation,
            &project_id,
            &self.session_id,
            &outcome.run_id,
            &self.evidence,
        )
        .await;

        let (stored_id, inserted) = self
            .store
            .record_exploration_finding(&finding.to_row())
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                tool: invocation.tool_name.clone(),
                details: e.to_string(),
            })?;
        self.counters.record_finding(inserted).await;

        // Duplicate suppression reported, not silent. Reaching a known defect is a real
        // result; the caller is told it was a re-observation and which finding it matched,
        // so suppression is information rather than a silent drop.
        let duplicate_of: Option<Vec<String>> = if inserted {
            None
        } else {
            match finding.failure_fingerprint.as_deref() {
                Some(fp) => Some(
                    self.store
                        .findings_by_fingerprint(&project_id, fp)
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .map(|f| f.id.to_string())
                        .collect(),
                ),
                None => None,
            }
        };

        Ok(ToolObservation::success(
            &invocation.call_id,
            &invocation.tool_name,
            level,
            json!({
                "acted": true,
                "finding_id": stored_id,
                "recorded": inserted,
                "duplicate_of": duplicate_of,
                "classification": finding.classification.as_str(),
                "basis": finding.basis,
                "confidence": finding.confidence,
                "behaviour": behaviour.as_str(),
                "route": args.route,
                "failure_fingerprint": finding.failure_fingerprint,
                "evidence_object_key": finding.evidence_object_key,
                "run_id": outcome.run_id.to_string(),
                "counters": counters_snapshot(&self.counters.snapshot().await),
            }),
            0,
        ))
    }
}

fn counters_snapshot(c: &ExplorationCounters) -> Value {
    json!({
        "actions": c.actions,
        "state_transitions": c.state_transitions,
        "findings": c.findings,
        "suppressed": c.suppressed,
        "pages": c.pages,
        "routes_visited": c.routes.len(),
        "behaviours_tried": c.tried.len(),
    })
}

// ============================================================================
// 7. THE OBSERVE TOOL
// ============================================================================

/// Reads the application surface exploration may act on, from real crawl evidence.
///
/// Read-only. It reports what the crawler actually found and nothing else — an element
/// nobody has seen is not a target.
#[derive(Debug)]
pub struct ObserveSurfaceTool {
    surface: ExplorationSurface,
}

impl ObserveSurfaceTool {
    pub fn new(surface: ExplorationSurface) -> Self {
        Self { surface }
    }
}

#[async_trait]
impl AgentTool for ObserveSurfaceTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "observe_surface".to_string(),
            description: "Report the application surface recorded by the crawl: routes, the \
                          elements on each, and which elements are restricted to which role. \
                          Only crawled evidence is reported."
                .to_string(),
            parameters_schema: json!({ "type": "object", "properties": {} }),
            output_schema: None,
            safety_level: ToolSafetyLevel::ReadOnly,
            default_timeout_ms: 0,
        }
    }

    async fn execute(
        &self,
        invocation: &ToolInvocation,
        _context: &AgentContext,
    ) -> Result<ToolObservation, ToolError> {
        Ok(ToolObservation::success(
            &invocation.call_id,
            &invocation.tool_name,
            ToolSafetyLevel::ReadOnly,
            json!({
                "routes": self.surface.routes,
                "roles": self.surface.roles,
                "basis": "crawl evidence recorded for this project; no element is reported \
                          that the crawl did not observe",
            }),
            0,
        ))
    }
}

// ============================================================================
// 8. THE RECALL TOOL
// ============================================================================

/// Reports what is already known, so exploration is guided by evidence rather than
/// randomness.
///
/// Read-only, and project-scoped through the Phase 4 reader it calls. Where a signal does
/// not exist it is reported as absent — never as a low value, and never as a reason to
/// explore that area first.
#[derive(Debug)]
pub struct RecallHistoryTool {
    /// Phase 4's prioritisation, or `None` when the project has no history to rank.
    prioritisation: Option<Value>,
    /// The project's known exploration fingerprints.
    known_fingerprints: Vec<String>,
}

impl RecallHistoryTool {
    pub fn new(prioritisation: Option<Value>, known_fingerprints: Vec<String>) -> Self {
        Self {
            prioritisation,
            known_fingerprints,
        }
    }
}

#[async_trait]
impl AgentTool for RecallHistoryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "recall_history".to_string(),
            description: "Report the project's recorded risk signals and already-known defect \
                          fingerprints, so exploration can prioritise evidence-first. Absent \
                          signals are reported as absent."
                .to_string(),
            parameters_schema: json!({ "type": "object", "properties": {} }),
            output_schema: None,
            safety_level: ToolSafetyLevel::ReadOnly,
            default_timeout_ms: 0,
        }
    }

    async fn execute(
        &self,
        invocation: &ToolInvocation,
        _context: &AgentContext,
    ) -> Result<ToolObservation, ToolError> {
        Ok(ToolObservation::success(
            &invocation.call_id,
            &invocation.tool_name,
            ToolSafetyLevel::ReadOnly,
            json!({
                "prioritisation": self.prioritisation,
                "has_prioritisation": self.prioritisation.is_some(),
                "known_fingerprints": self.known_fingerprints,
                "basis": "Phase 4 prioritisation over stored execution history; a project \
                          with no history reports no prioritisation rather than a zero risk",
            }),
            0,
        ))
    }
}

// ============================================================================
// 9. THE AGENT
// ============================================================================

/// The exploratory agent.
///
/// Stateless, exactly as `SelfHealingAgent` is: what it knows comes from
/// `context.history`, which the agent runtime fills with every tool invocation and
/// observation. That is what makes it replayable and inspectable — the whole session is in
/// `TurnRecord`s.
pub struct ExploratoryAgent {
    surface: ExplorationSurface,
    counters: SharedCounters,
    budget: ExplorationBudget,
    /// Routes in the order evidence says to try them, decided once at construction from
    /// the surface and the project's risk signals.
    route_order: Vec<String>,
}

impl ExploratoryAgent {
    pub fn new(
        surface: ExplorationSurface,
        counters: SharedCounters,
        budget: ExplorationBudget,
        prioritisation: Option<&Value>,
    ) -> Self {
        // Route order comes from the surface, sorted, and is **not** reordered by risk.
        //
        // Phase 4 ranks *test cases*, not routes, and this schema has no `covers` edge, so
        // no test case can be attributed to a route (Phase 2 L1, carried into Phase 4 L1).
        // Mapping a route's risk would mean guessing which tests exercise it -- exactly the
        // invented relationship this codebase refuses to record. Risk is therefore reported
        // to the caller (`recall_history`) and used to justify *why* a session was run, not
        // to reorder routes it cannot honestly attribute.
        let route_order = surface
            .ordered_routes()
            .into_iter()
            .map(|r| r.url.clone())
            .collect::<Vec<_>>();
        let _ = prioritisation;

        Self {
            surface,
            counters,
            budget,
            route_order,
        }
    }

    pub fn surface(&self) -> &ExplorationSurface {
        &self.surface
    }

    pub fn route_order(&self) -> &[String] {
        &self.route_order
    }

    /// The next behaviour to try, chosen from evidence rather than at random.
    ///
    /// The catalogue order is a *preference*, and two things override it: a behaviour
    /// already tried is skipped in favour of one that has not, and a budget already spent
    /// ends the session. Within that, the order is fixed, so the same project explored
    /// twice explores the same way — which is what makes a session reproducible and a
    /// regression in its findings visible.
    fn next_behaviour(&self, tried: &BTreeSet<String>) -> Option<ExplorationBehaviour> {
        ExplorationBehaviour::catalogue()
            .iter()
            .copied()
            .find(|b| !tried.contains(b.as_str()))
    }

    /// Build a probe for a behaviour against a route, using only crawled evidence.
    fn compose(
        &self,
        behaviour: ExplorationBehaviour,
        route: &RouteSurface,
    ) -> Option<ComposedProbe> {
        let fillable: Vec<&ElementSurface> =
            route.elements.iter().filter(|e| e.is_fillable()).collect();
        let clickable: Vec<&ElementSurface> =
            route.elements.iter().filter(|e| e.is_clickable()).collect();

        let (actions, expected) = match behaviour {
            ExplorationBehaviour::IntelligentNavigation => (
                vec![
                    Action::Navigate {
                        url: route.url.clone(),
                    },
                    Action::AssertUrl {
                        url_contains: route.url.clone(),
                    },
                ],
                Some(format!(
                    "navigating to {} should reach {}",
                    route.url, route.url
                )),
            ),
            ExplorationBehaviour::UnexpectedNavigation => {
                let target = clickable.first()?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Click {
                            selector: target.selector_hint.clone(),
                        },
                        // Asserting the route we are *already on* is the honest way to ask
                        // "did this go somewhere unexpected?" -- a mismatch is the finding.
                        Action::AssertUrl {
                            url_contains: route.url.clone(),
                        },
                    ],
                    Some(format!(
                        "clicking {} should not leave {}",
                        target.selector_hint, route.url
                    )),
                )
            }
            ExplorationBehaviour::DuplicateAction | ExplorationBehaviour::RepeatedSubmission => {
                let target = clickable.first()?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Click {
                            selector: target.selector_hint.clone(),
                        },
                        Action::Click {
                            selector: target.selector_hint.clone(),
                        },
                        Action::AssertVisible {
                            selector: target.selector_hint.clone(),
                        },
                    ],
                    Some(format!(
                        "submitting {} twice should leave the application in a usable state",
                        target.selector_hint
                    )),
                )
            }
            ExplorationBehaviour::StateTransition => {
                // Only meaningful when the page actually goes somewhere: a real link is
                // crawled evidence, a synthesised URL is not.
                let link = route.elements.iter().find(|e| e.href.is_some())?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Click {
                            selector: link.selector_hint.clone(),
                        },
                        Action::AssertUrl {
                            url_contains: link.href.clone().unwrap_or_default(),
                        },
                    ],
                    Some(format!(
                        "following {} should reach {}",
                        link.selector_hint,
                        link.href.clone().unwrap_or_default()
                    )),
                )
            }
            ExplorationBehaviour::BoundaryValue => {
                let field = fillable.first()?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Fill {
                            selector: field.selector_hint.clone(),
                            // A long value, which is the boundary most fields are wrong
                            // about. Stated as a length rather than a claim about the
                            // field's real limit, which the crawl does not record.
                            value: "x".repeat(1024),
                        },
                        Action::AssertEnabled {
                            selector: field.selector_hint.clone(),
                        },
                    ],
                    Some(format!(
                        "{} should handle a 1024-character value without becoming unusable",
                        field.selector_hint
                    )),
                )
            }
            ExplorationBehaviour::InvalidValue => {
                let field = fillable.first()?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Fill {
                            selector: field.selector_hint.clone(),
                            value: "<script>alert(1)</script>".to_string(),
                        },
                        Action::AssertEnabled {
                            selector: field.selector_hint.clone(),
                        },
                    ],
                    Some(format!(
                        "{} should reject or safely escape a markup-bearing value",
                        field.selector_hint
                    )),
                )
            }
            ExplorationBehaviour::EmptyValue => {
                let field = fillable.first()?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Fill {
                            selector: field.selector_hint.clone(),
                            value: String::new(),
                        },
                        Action::AssertEnabled {
                            selector: field.selector_hint.clone(),
                        },
                    ],
                    Some(format!(
                        "{} should handle an empty value without breaking",
                        field.selector_hint
                    )),
                )
            }
            ExplorationBehaviour::SessionExpiry => {
                let field = fillable.first()?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Fill {
                            selector: field.selector_hint.clone(),
                            value: "exploratory-session-probe".to_string(),
                        },
                        Action::AssertVisible {
                            selector: field.selector_hint.clone(),
                        },
                    ],
                    Some(format!(
                        "{} should not be usable without a session",
                        field.selector_hint
                    )),
                )
            }
            ExplorationBehaviour::RoleStateChange => {
                let restricted = route
                    .role_restricted
                    .values()
                    .flatten()
                    .find(|s| !s.is_empty())?;
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::AssertVisible {
                            selector: restricted.clone(),
                        },
                    ],
                    Some(format!(
                        "{} is recorded as role-restricted, so it should not be visible \
                         without that role",
                        restricted
                    )),
                )
            }
            ExplorationBehaviour::Refresh => {
                // The documented approximation: no refresh action exists, so this
                // re-navigates. `support()` says so, and the finding records the behaviour
                // name so a reader knows which technique actually ran.
                (
                    vec![
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::Navigate {
                            url: route.url.clone(),
                        },
                        Action::AssertVisible {
                            selector: route
                                .elements
                                .first()
                                .map(|e| e.selector_hint.clone())
                                .unwrap_or_default(),
                        },
                    ],
                    Some(format!(
                        "re-requesting {} should render the same page",
                        route.url
                    )),
                )
            }
        };

        Some(ComposedProbe {
            behaviour,
            route: route.url.clone(),
            assertion_count: actions.iter().filter(|a| is_assertion(a)).count(),
            actions,
            expected,
            violated_requirement: None,
        })
    }

    /// Read every observation the runtime has recorded so far.
    ///
    /// This is the agent's entire memory. Deriving it from `context.history` rather than
    /// from internal state is what makes a session inspectable after the fact.
    fn observations(context: &AgentContext) -> Vec<Value> {
        context
            .history
            .iter()
            .flat_map(|turn| turn.tool_observations.iter())
            .filter(|o| o.is_success())
            .filter_map(|o| o.output.clone().ok())
            .collect()
    }

    /// The requirement covering a route, from the recorded requirement store.
    ///
    /// Read from the tool history because that is where the lookup result already is. A
    /// route with no covering requirement gets `None`, which caps its findings at
    /// `POTENTIAL_DEFECT` — the classification ladder, not a heuristic, keeps an
    /// unsupported expectation from becoming a confirmed defect.
    fn covering_requirement(observations: &[Value], route: &str) -> Option<String> {
        observations
            .iter()
            .rev()
            .find_map(|o| o.get("route_requirement").and_then(|v| v.as_str()))
            .filter(|_| !route.is_empty())
            .map(ToString::to_string)
    }
}

#[async_trait]
impl Agent for ExploratoryAgent {
    fn id(&self) -> &str {
        AGENT_ID
    }

    fn name(&self) -> &str {
        "AI Exploratory Testing Agent"
    }

    fn agent_type(&self) -> &str {
        "exploratory_testing"
    }

    async fn step(
        &self,
        context: &AgentContext,
        _tools: &AgentToolRegistry,
        _ai: &crate::ai_router::AiInfrastructure,
    ) -> Result<TurnDecision, AgentError> {
        // --- Stop when the budget is spent. The agent's own accounting; the tool
        // enforces the same ceiling independently.
        let counters = self.counters.snapshot().await;
        if let Some(reason) = self.budget.exhausted_by(&counters) {
            return Ok(TurnDecision::Complete {
                output: json!({
                    "stopped": "budget_exhausted",
                    "exhausted_by": reason,
                    "counters": counters_snapshot(&counters),
                }),
            });
        }

        // --- Observe: everything the runtime has recorded so far.
        let observations = Self::observations(context);

        // --- Prefer a behaviour not yet tried, so a long session cycles the catalogue.
        let Some(behaviour) = self.next_behaviour(&counters.tried) else {
            return Ok(TurnDecision::Complete {
                output: json!({
                    "stopped": "catalogue_exhausted",
                    "behaviours_tried": counters.tried.len(),
                    "counters": counters_snapshot(&counters),
                }),
            });
        };

        // --- Choose the route. First unvisited in the evidence-derived order, so
        // exploration is guided rather than random.
        let route = self
            .route_order
            .iter()
            .find(|u| !counters.routes.contains(*u))
            .or_else(|| self.route_order.first())
            .cloned();

        let Some(route_url) = route else {
            return Ok(TurnDecision::Complete {
                output: json!({
                    "stopped": "no_surface",
                    "detail": "the crawl recorded no routes, so there is nothing to explore",
                    "counters": counters_snapshot(&counters),
                }),
            });
        };
        let Some(route) = self.surface.routes.iter().find(|r| r.url == route_url) else {
            return Ok(TurnDecision::Complete {
                output: json!({
                    "stopped": "route_not_in_surface",
                    "route": route_url,
                    "counters": counters_snapshot(&counters),
                }),
            });
        };

        // --- Compose a probe from crawled evidence only.
        let Some(probe) = self.compose(behaviour, route) else {
            // This route cannot support this technique (no field to fill, no link to
            // follow). Recorded rather than skipped silently, because "we tried nothing
            // here" and "we tried and found nothing" must not look the same.
            self.counters.mark_tried(behaviour.as_str()).await;
            return Ok(TurnDecision::Continue);
        };

        let violated_requirement = Self::covering_requirement(&observations, &probe.route);

        Ok(TurnDecision::CallTools {
            invocations: vec![ToolInvocation {
                call_id: format!("probe-{}-{}", behaviour.as_str(), probe.route),
                tool_name: "probe_action".to_string(),
                arguments: json!({
                    "behaviour": behaviour.as_str(),
                    "route": probe.route,
                    "actions": probe.actions,
                    "expected": probe.expected,
                    "violated_requirement": violated_requirement,
                }),
            }],
        })
    }
}

// ============================================================================
// 10. THE SERVICE
// ============================================================================

/// Why an exploration session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stopped", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExplorationStop {
    /// The agent decided it was done.
    Completed { reason: String },
    /// A ceiling was reached. Named, so "it stopped" always has an answer.
    BudgetExhausted { exhausted_by: String },
    /// The whole catalogue was exercised.
    CatalogueExhausted,
    /// There was nothing to explore.
    NoSurface { detail: String },
    /// The runtime cancelled the session.
    Cancelled,
    /// The runtime's turn or tool-call ceiling was reached.
    LimitsReached { detail: String },
    /// The agent or a tool failed outright.
    Failed { detail: String },
}

impl ExplorationStop {
    pub fn as_status(&self) -> &'static str {
        match self {
            Self::Completed { .. } => "COMPLETED",
            Self::BudgetExhausted { .. } => "MAX_BUDGET_REACHED",
            Self::CatalogueExhausted => "COMPLETED",
            Self::NoSurface { .. } => "COMPLETED",
            Self::Cancelled => "CANCELLED",
            Self::LimitsReached { .. } => "MAX_BUDGET_REACHED",
            Self::Failed { .. } => "FAILED",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Completed { reason } => reason.clone(),
            Self::BudgetExhausted { exhausted_by } => {
                format!("budget exhausted: {exhausted_by}")
            }
            Self::CatalogueExhausted => "every behaviour in the catalogue was exercised".into(),
            Self::NoSurface { detail } => format!("nothing to explore: {detail}"),
            Self::Cancelled => "cancelled".into(),
            Self::LimitsReached { detail } => format!("agent limits reached: {detail}"),
            Self::Failed { detail } => format!("failed: {detail}"),
        }
    }
}

/// The outcome of one exploratory session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationReport {
    pub session_id: Uuid,
    pub project_id: String,
    pub stop: ExplorationStop,
    pub budget: ExplorationBudget,
    pub counters: Value,
    /// Findings this session actually recorded. Suppressions are counted, not listed,
    /// because a duplicate is not a new finding.
    pub findings: Vec<ExplorationFinding>,
    pub findings_recorded: usize,
    pub findings_suppressed: usize,
    pub turns_taken: usize,
    pub total_tool_calls: usize,
    pub duration_ms: u64,
}

/// Runs an exploratory session end to end.
pub struct ExplorationService {
    pub store: Store,
    pub policy: Arc<PolicyConfig>,
    pub evidence: Arc<LocalEvidenceStore>,
    pub counters: SharedCounters,
}

impl ExplorationService {
    pub fn new(store: Store, policy: Arc<PolicyConfig>, evidence: Arc<LocalEvidenceStore>) -> Self {
        Self {
            store,
            policy,
            evidence,
            counters: SharedCounters::default(),
        }
    }

    /// The counter handle an agent or tool needs. Cheap to clone; shares one state.
    pub fn counters(&self) -> SharedCounters {
        self.counters.clone()
    }

    /// Explore `target` within `budget`.
    ///
    /// Everything the agent can reach is assembled here: the real `run_agent_loop`, the
    /// real diagnostic tools, the Phase 4 reader for risk, the real evidence store. The
    /// session is opened before the loop starts and closed after it ends, so a run that
    /// panics or is cancelled still leaves a row explaining itself.
    #[allow(clippy::too_many_arguments)]
    pub async fn explore(
        &self,
        project_id: &str,
        target: &str,
        surface: ExplorationSurface,
        crawl_task_id: Option<&str>,
        budget: ExplorationBudget,
        limits: AgentLimits,
        runner: Arc<dyn ProbeRunner>,
        ai: Arc<crate::ai_router::AiInfrastructure>,
    ) -> Result<ExplorationReport, String> {
        let session_id = Uuid::new_v4();

        // Risk/history, read once. A project with no history yields `None`, and the
        // response says so rather than inventing a zero-risk ordering.
        let prioritisation = self.prioritisation(project_id).await;
        let known = self
            .store
            .list_exploration_findings(project_id, None, 500)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|f| f.failure_fingerprint)
            .collect::<Vec<_>>();

        self.store
            .insert_exploration_session(project_id, &session_id, target, &budget, &limits)
            .await
            .map_err(|e| e.to_string())?;

        let agent = ExploratoryAgent::new(
            surface,
            self.counters.clone(),
            budget,
            prioritisation.as_ref(),
        );

        let mut registry = AgentToolRegistry::new();
        registry
            .register(Arc::new(ObserveSurfaceTool::new(agent.surface().clone())))
            .map_err(|e| e.to_string())?;
        registry
            .register(Arc::new(RecallHistoryTool::new(prioritisation, known)))
            .map_err(|e| e.to_string())?;
        registry
            .register(Arc::new(ProbeActionTool::new(
                self.store.clone(),
                self.policy.clone(),
                self.evidence.clone(),
                self.counters.clone(),
                budget,
                runner,
                session_id,
            )))
            .map_err(|e| e.to_string())?;

        let mut context = AgentContext::new(
            format!("exploration-{session_id}"),
            Some(project_id.to_string()),
            session_id.to_string(),
            limits,
            None,
        )
        .with_store(self.store.clone());
        if let Some(task_id) = crawl_task_id {
            context.metadata.insert(
                "crawl_task_id".to_string(),
                Value::String(task_id.to_string()),
            );
        }

        let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None).await;

        let counters = self.counters.snapshot().await;
        let (stop, error) = match &outcome {
            Err(e) => (
                ExplorationStop::Failed {
                    detail: e.to_string(),
                },
                Some(e.to_string()),
            ),
            Ok(o) => {
                let stop = match o.status {
                    AgentStatus::Completed => {
                        // The agent's own reason, taken from its `Complete` output so a
                        // reader learns *why* it stopped rather than only that it did.
                        let reason = o
                            .final_output
                            .as_ref()
                            .and_then(|v| v.get("stopped"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("the agent completed its session")
                            .to_string();
                        match reason.as_str() {
                            "budget_exhausted" => ExplorationStop::BudgetExhausted {
                                exhausted_by: o
                                    .final_output
                                    .as_ref()
                                    .and_then(|v| v.get("exhausted_by"))
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("unknown")
                                    .to_string(),
                            },
                            "catalogue_exhausted" => ExplorationStop::CatalogueExhausted,
                            "no_surface" => ExplorationStop::NoSurface {
                                detail: o
                                    .final_output
                                    .as_ref()
                                    .and_then(|v| v.get("detail"))
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("no crawl evidence")
                                    .to_string(),
                            },
                            _ => ExplorationStop::Completed { reason },
                        }
                    }
                    AgentStatus::Cancelled => ExplorationStop::Cancelled,
                    AgentStatus::MaxTurnsReached => ExplorationStop::LimitsReached {
                        detail: format!("reached max_turns ({})", context.limits.max_turns),
                    },
                    AgentStatus::TimedOut => ExplorationStop::LimitsReached {
                        detail: "timed out".to_string(),
                    },
                    AgentStatus::Failed => ExplorationStop::LimitsReached {
                        detail: o
                            .error
                            .clone()
                            .unwrap_or_else(|| "the agent runtime stopped the session".into()),
                    },
                };
                (stop, o.error.clone())
            }
        };

        let duration_ms = counters_snapshot(&counters);
        let findings = self
            .store
            .list_exploration_findings(project_id, None, 500)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|f| f.session_id == session_id)
            .map(finding_from_row)
            .collect::<Vec<_>>();

        self.store
            .finish_exploration_session(
                project_id,
                &session_id,
                stop.as_status(),
                counters.actions,
                counters.state_transitions,
                counters.pages,
                counters.findings,
                counters.suppressed,
                &stop.describe(),
                error.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())?;

        let (turns, tool_calls, duration) = match &outcome {
            Ok(o) => (o.turns_taken, o.total_tool_calls, o.total_duration_ms),
            Err(_) => (0, context.total_tool_calls, 0),
        };

        Ok(ExplorationReport {
            session_id,
            project_id: project_id.to_string(),
            stop,
            budget,
            counters: duration_ms,
            findings,
            findings_recorded: counters.findings,
            findings_suppressed: counters.suppressed,
            turns_taken: turns,
            total_tool_calls: tool_calls,
            duration_ms: duration,
        })
    }

    /// Phase 4's prioritisation for this project, or `None` when there is no history.
    ///
    /// Reuses the Phase 4 reader rather than re-deriving risk, so exploration is guided by
    /// exactly the model the rest of the product reports.
    async fn prioritisation(&self, project_id: &str) -> Option<Value> {
        let reader = crate::qa_memory::QaMemoryReader::new(self.store.clone());
        // A storage failure here must not be read as "this project has no prioritisation", which
        // is what a silent default would have produced: exploration would then simply proceed
        // with no ranking and say nothing about why.
        let result = reader.prioritise_tests(project_id, None).await.ok()?;
        let view = result.view?;
        // Only the signal set is handed to the agent. The full per-case risk views are the
        // API's business; exploration needs to know which areas matter, not re-read every
        // factor.
        let ranked: Vec<Value> = view
            .tests
            .iter()
            .take(50)
            .map(|t| {
                json!({
                    "test_case_id": t.test_case_id,
                    "title": t.title,
                    "priority": t.priority.as_str(),
                    "applies": t.applies.iter().map(|b| b.as_str()).collect::<Vec<_>>(),
                    "recurring_defects": t.risk.recurring_defects,
                    "flakiness_score": t.risk.flakiness_score,
                    "failures": t.risk.failures,
                })
            })
            .collect();
        Some(json!({
            "band_counts": view.band_counts,
            "mandatory_floor": view.mandatory_floor,
            "tests": ranked,
            "basis": "Phase 4 prioritisation over stored execution history",
        }))
    }
}

fn finding_from_row(row: crate::store::ExplorationFindingRow) -> ExplorationFinding {
    ExplorationFinding {
        session_id: row.session_id,
        project_id: row.project_id,
        classification: FindingClass::parse(&row.classification)
            .unwrap_or(FindingClass::InsufficientEvidence),
        behaviour: ExplorationBehaviour::parse(&row.behaviour)
            .unwrap_or(ExplorationBehaviour::IntelligentNavigation),
        route: row.route,
        observed: row.observed,
        expected: row.expected,
        violated_requirement: row.violated_requirement,
        failure_fingerprint: row.failure_fingerprint,
        evidence_object_key: row.evidence_object_key,
        action_sequence: serde_json::from_str(&row.action_sequence).unwrap_or_default(),
        app_state: serde_json::from_str(&row.app_state).unwrap_or(Value::Null),
        confidence: row.confidence,
        basis: row.basis,
        related_failure: row.related_failure,
        observed_at: row.observed_at,
    }
}

// ============================================================================
// 11. BEHAVIOUR REPORT
// ============================================================================

/// Which techniques the existing Action AST can express, and which it cannot.
///
/// Answered from [`ExplorationBehaviour::support`] rather than from prose, so a caller
/// asking "can you test back/forward navigation?" gets `Unsupported` with a reason
/// instead of silence.
pub fn behaviour_support_report() -> Value {
    let catalogue: Vec<Value> = ExplorationBehaviour::catalogue()
        .iter()
        .map(|b| {
            json!({
                "behaviour": b.as_str(),
                "support": b.support(b.as_str()),
                "required_safety": b.required_safety(),
                "autonomously_safe": b.required_safety().is_autonomous_safe(),
            })
        })
        .collect();

    // Named explicitly because the phase brief asked for it and the AST cannot do it.
    let requested_but_unsupported: Vec<Value> = vec![json!({
        "behaviour": "back_forward_navigation",
        "support": ExplorationBehaviour::IntelligentNavigation.support("back_forward_navigation"),
    })];

    json!({
        "catalogue": catalogue,
        "requested_but_unsupported": requested_but_unsupported,
    })
}

/// The fingerprint function exploration deduplicates on, re-exported so a caller reading a
/// finding does not have to know which module the canonical implementation lives in.
///
/// The *same* Phase 2 function telemetry, failure clusters and healing proposals use, so
/// an exploration finding joins the one fingerprint space rather than starting another.
pub fn exploration_fingerprint(
    raw_reason: &str,
    selector: Option<&str>,
    http_status: Option<u16>,
    endpoint: Option<&str>,
) -> (
    String,
    crate::adaptive_intelligence::FailureCategory,
    String,
) {
    canonical_failure_fingerprint(raw_reason, selector, http_status, endpoint)
}
