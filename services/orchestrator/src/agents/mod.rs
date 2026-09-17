//! Phase 3 — Specialized QA Agents.
//!
//! Provides the first concrete multi-turn QA reasoning agents for AIQA:
//! - `TestPlanningAgent`: Uses `requirement_lookup` and `schema_inspector` to synthesize
//!   prioritized, grounded QA test plans with risk assessments and coverage-gap analysis.
//! - `SelfHealingAgent`: Uses `dom_prober` to diagnose broken selector failures and recommend
//!   healed selector candidates without re-executing suites or modifying code.
//!
//! Architectural Invariants:
//! - Implements the native Rust `Agent` trait.
//! - Multi-turn reasoning loops driving Phase 2 diagnostic tools through `AgentToolRegistry`.
//! - Strict multi-tenant boundary checks via `AgentContext::is_authorized_for_project`.
//! - Strictly diagnostic, reasoning, and planning: zero external executor invocation.

pub mod healing;
pub mod planning;

#[cfg(test)]
mod tests;

pub use healing::{FailureEvidence, HealedCandidate, SelfHealingAgent, SelfHealingOutput};
pub use planning::{
    CoverageGap, PlanningRequest, TestObjective, TestPlanOutput, TestPlanningAgent, TestSuitePlan,
};
