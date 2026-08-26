//! AI governance / policy engine — flagged in the build report as "Not
//! implemented — no approval gates, no policy checks" and, in the original
//! PRD critique, as presupposed by "governed autonomous execution" but
//! nowhere defined.
//!
//! This is intentionally *mechanism*, not *intelligence*: a small set of
//! configurable, explainable rules evaluated against a `RunPlan` before it's
//! scheduled. There is no AI judgment here (Ollama/LLM integration is out of
//! scope this session) — the governance value is the gate itself and the
//! audit trail around it, which is what the PRD's "governed" actually
//! requires regardless of what (if anything) eventually does the judging.

use crate::domain::RunPlan;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct PolicyConfig {
    pub max_concurrency: usize,
    pub require_approval_for: HashSet<String>,
    pub blocked_executors: HashSet<String>,
}

impl PolicyConfig {
    /// Loaded from env so an operator can tune governance without a
    /// recompile:
    ///   AIQA_MAX_CONCURRENCY        (default 8)
    ///   AIQA_REQUIRE_APPROVAL_FOR   comma-separated executor kinds (default "maestro" —
    ///                               mobile-device automation is the one adapter in this
    ///                               codebase that can touch a real device, so it's the
    ///                               one gated by default; everything else auto-schedules)
    ///   AIQA_BLOCKED_EXECUTORS      comma-separated executor kinds (default empty)
    pub fn from_env() -> Self {
        let max_concurrency = std::env::var("AIQA_MAX_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        let require_approval_for =
            std::env::var("AIQA_REQUIRE_APPROVAL_FOR").unwrap_or_else(|_| "maestro".to_string());
        let blocked_executors = std::env::var("AIQA_BLOCKED_EXECUTORS").unwrap_or_default();
        Self {
            max_concurrency,
            require_approval_for: split_csv(&require_approval_for),
            blocked_executors: split_csv(&blocked_executors),
        }
    }
}

fn split_csv(s: &str) -> HashSet<String> {
    s.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    RequireApproval(String),
    Deny(String),
}

/// Deny wins over RequireApproval: a plan that both exceeds concurrency and
/// uses a gated executor should be rejected outright, not queued for
/// approval on a plan that couldn't run anyway.
pub fn evaluate(cfg: &PolicyConfig, plan: &RunPlan) -> PolicyDecision {
    if plan.concurrency > cfg.max_concurrency {
        return PolicyDecision::Deny(format!(
            "requested concurrency {} exceeds policy max {}",
            plan.concurrency, cfg.max_concurrency
        ));
    }

    let executors_used: Vec<&str> = plan
        .tasks
        .iter()
        .filter_map(|t| t.executor.as_deref())
        .collect();

    let blocked: Vec<&str> = executors_used
        .iter()
        .filter(|e| cfg.blocked_executors.contains(&e.to_lowercase()))
        .copied()
        .collect();
    if !blocked.is_empty() {
        return PolicyDecision::Deny(format!("executor(s) {blocked:?} are blocked by policy"));
    }

    let gated: Vec<&str> = executors_used
        .iter()
        .filter(|e| cfg.require_approval_for.contains(&e.to_lowercase()))
        .copied()
        .collect();
    if !gated.is_empty() {
        return PolicyDecision::RequireApproval(format!(
            "plan uses executor(s) {gated:?} which require human approval before scheduling"
        ));
    }

    PolicyDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TaskDef;

    fn plan_with_executor(exec: &str) -> RunPlan {
        RunPlan {
            name: "p".into(),
            concurrency: 4,
            tasks: vec![TaskDef {
                id: "t1".into(),
                name: "t1".into(),
                executor: Some(exec.into()),
                ..Default::default()
            }],
        }
    }

    #[test]
    fn maestro_requires_approval_by_default() {
        let cfg = PolicyConfig {
            max_concurrency: 8,
            require_approval_for: split_csv("maestro"),
            blocked_executors: HashSet::new(),
        };
        assert_eq!(
            evaluate(&cfg, &plan_with_executor("maestro")),
            PolicyDecision::RequireApproval(
                "plan uses executor(s) [\"maestro\"] which require human approval before scheduling".into()
            )
        );
    }

    #[test]
    fn unrestricted_executor_is_allowed() {
        let cfg = PolicyConfig {
            max_concurrency: 8,
            require_approval_for: split_csv("maestro"),
            blocked_executors: HashSet::new(),
        };
        assert_eq!(
            evaluate(&cfg, &plan_with_executor("playwright")),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn concurrency_over_max_is_denied_even_if_also_gated() {
        let cfg = PolicyConfig {
            max_concurrency: 2,
            require_approval_for: split_csv("maestro"),
            blocked_executors: HashSet::new(),
        };
        let mut plan = plan_with_executor("maestro");
        plan.concurrency = 10;
        assert!(matches!(evaluate(&cfg, &plan), PolicyDecision::Deny(_)));
    }
}
