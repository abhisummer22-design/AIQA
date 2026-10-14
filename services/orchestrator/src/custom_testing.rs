//! Phase 3 — Custom Testing: real requirement -> AI strategy -> test cases.
//!
//! Fresh baseline audit before writing this module: `CustomTestingPage.tsx`
//! (frontend) was a truthful "not implemented" placeholder — no backend
//! route, no custom-test-suite entity, confirmed by an earlier sprint's own
//! investigation (`CaseCPages.test.tsx`'s `customTestingCase` doc comment:
//! ACC-015 checked and found not satisfied). `ai_router.rs::AiTask` already
//! declared a `RequirementAnalysis` variant with real, sensible required
//! capabilities (`TextGeneration, Reasoning, LongContext`) — declared at
//! the router's original design time but never wired to anything. This
//! module is what wires it up for real, rather than adding a new task
//! variant the router didn't already anticipate.
//!
//! ```text
//! Custom Requirement + Scope + Selected Skills
//!         v
//! build_analysis_prompt()
//!         v
//! analyze_requirement()  -- AiTask::RequirementAnalysis, via the EXISTING
//!                            central ai_router::AiInfrastructure (no new
//!                            router, no new provider registry)
//!         v
//! CustomTestingAnalysis (strict schema; malformed/unavailable is a real,
//!                         honest failure, never a fabricated strategy)
//!         v
//! ai_generation::generate_test_cases()  -- reused VERBATIM, unmodified,
//!                                           same function Phase 1's Test
//!                                           Case Generator calls, same
//!                                           AiTask::TestCaseGeneration,
//!                                           same skill resolver, same
//!                                           quality/traceability/dedup
//!                                           gates. Not reimplemented here.
//!         v
//! CustomTestingOutcome::Ready { analysis, cases, provider, model }
//!                      | Failed { reason, analysis }
//! ```
//!
//! Deliberately NOT a single combined "analysis+cases in one AI call"
//! design: two genuinely distinct AI-router calls (RequirementAnalysis,
//! then the unmodified TestCaseGeneration call) means the test-case
//! generation half has zero duplicated prompt/schema/quality-gate logic —
//! it is the exact same, already-tested Phase 1 code path, just invoked a
//! second time with a requirement text that has been materially widened by
//! this custom request's own scope. `analysis` is carried through even on a
//! later `Failed` (case-generation-only failure) so a genuinely-produced
//! strategy is never discarded just because the second call failed.

use crate::ai_generation::{self, GeneratedTestCaseDraft, GenerationOutcome, SkillInput};
use crate::ai_router::{AiInfrastructure, AiRequest, AiTask, GenerationOutcome2};
use serde::{Deserialize, Serialize};

/// Mirrors brief §5's conceptual `CustomTestingAnalysis` schema field-for-
/// field (renamed to snake_case for the Rust/wire convention every other
/// struct in this crate already uses). Every field is real model output,
/// structurally validated by `serde_json` deserialization (a missing
/// required field is a real parse failure, not a silently-defaulted one).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomTestingAnalysis {
    pub objective: String,
    pub normalized_scope: String,
    #[serde(default)]
    pub recommended_skills: Vec<String>,
    #[serde(default)]
    pub recommended_testing_types: Vec<String>,
    #[serde(default)]
    pub risk_areas: Vec<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub limitations: Vec<String>,
    pub generation_approach: String,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

/// Every truthful outcome this module can produce. `analysis` on `Failed`
/// is `Some` only when the analysis step itself genuinely succeeded and the
/// *subsequent* test-case-generation call is what failed — never a
/// fabricated strategy standing in for an unavailable one.
#[derive(Debug, Clone)]
pub enum CustomTestingOutcome {
    Ready {
        analysis: CustomTestingAnalysis,
        cases: Vec<GeneratedTestCaseDraft>,
        provider: String,
        model: String,
    },
    Failed {
        reason: String,
        analysis: Option<CustomTestingAnalysis>,
    },
}

/// Folds the real, user-provided scope into the requirement text sent for
/// generation — the same "no dedicated wire field, so fold into the
/// prompt's own text" pattern `TestCaseGeneratorPage.tsx::buildRequirementText`
/// already established for category/depth/documents (Phase 1), not a new
/// convention invented here.
fn augmented_requirement_text(requirement_text: &str, scope: Option<&str>) -> String {
    match scope.map(str::trim).filter(|s| !s.is_empty()) {
        Some(scope) => {
            format!("{requirement_text}\n\nTesting scope/module (user-provided): {scope}")
        }
        None => requirement_text.to_string(),
    }
}

fn build_analysis_prompt(
    requirement_text: &str,
    scope: Option<&str>,
    skills: &[SkillInput],
) -> String {
    let scope_line = match scope.map(str::trim).filter(|s| !s.is_empty()) {
        Some(scope) => format!("Testing scope/module (user-provided): {scope}"),
        None => "Testing scope/module: not specified — infer scope from the requirement text only."
            .to_string(),
    };
    let skills_line = if skills.is_empty() {
        "Selected skills: (none selected)".to_string()
    } else {
        format!(
            "Selected skills: {}",
            skills
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    format!(
        "You are a Principal QA Test Strategy Analyst. Given a real, user-authored custom testing \
         requirement, produce a structured test strategy analysis ONLY — do not write test cases here.\n\n\
         REQUIREMENT:\n\"\"\"\n{requirement_text}\n\"\"\"\n\n\
         {scope_line}\n{skills_line}\n\n\
         RULES (critical, do not violate):\n\
         - Every field must be genuinely derived from the requirement/scope/skills text actually given. \
           Never invent a specific system, feature, number, or constraint that is not stated or directly \
           implied by the text.\n\
         - \"recommended_skills\" must be real, well-known QA skill/testing-focus names (e.g. \"Security \
           Testing\", \"Edge Case Analysis\", \"RBAC Testing\") genuinely relevant to this specific \
           requirement — not a generic list reused regardless of content.\n\
         - \"limitations\" must honestly state what this analysis cannot determine from the given text alone \
           (e.g. missing target system, missing concrete thresholds).\n\
         - If the requirement is too vague to produce a real, specific strategy, say so honestly in \
           \"limitations\" and \"assumptions\" rather than inventing specifics to fill the schema.\n\n\
         Return ONLY JSON, no markdown, no commentary, matching exactly:\n\
         {{\"objective\": string, \"normalized_scope\": string, \"recommended_skills\": [string], \
         \"recommended_testing_types\": [string], \"risk_areas\": [string], \"assumptions\": [string], \
         \"prerequisites\": [string], \"limitations\": [string], \"generation_approach\": string, \
         \"diagnostics\": [string]}}"
    )
}

async fn analyze_requirement(
    infra: &AiInfrastructure,
    requirement_text: &str,
    scope: Option<&str>,
    skills: &[SkillInput],
) -> Result<(CustomTestingAnalysis, String, String), String> {
    let request = AiRequest {
        task: AiTask::RequirementAnalysis,
        prompt: build_analysis_prompt(requirement_text, scope, skills),
        preferred_provider: None,
        preferred_model: None,
        trace_id: None,
    };
    let result = match infra.generate(request).await {
        GenerationOutcome2::Ok(r) => r,
        GenerationOutcome2::Failed { reason } => return Err(reason),
    };
    let analysis: CustomTestingAnalysis = serde_json::from_str(&result.text).map_err(|e| {
        format!("AI analysis response did not match the required structured schema: {e}")
    })?;
    Ok((analysis, result.provider, result.model))
}

/// The real, single entry point. Reuses `ai_generation::generate_test_cases`
/// unmodified for the case-generation half — see this module's own doc
/// comment for why that is a deliberate design choice, not an oversight.
/// `feedback` (the same real `FeedbackContext` `generate_ai_test_cases_scoped`
/// loads via `intelligence::Intelligence::load_feedback_context`) is passed
/// straight through, unmodified, to that shared function — no second
/// feedback-loading path invented for this module.
pub async fn run(
    infra: &AiInfrastructure,
    requirement_text: &str,
    scope: Option<&str>,
    skills: &[SkillInput],
    feedback: &ai_generation::FeedbackContext,
) -> CustomTestingOutcome {
    let (analysis, analysis_provider, analysis_model) =
        match analyze_requirement(infra, requirement_text, scope, skills).await {
            Ok(v) => v,
            Err(reason) => {
                return CustomTestingOutcome::Failed {
                    reason,
                    analysis: None,
                }
            }
        };

    let augmented = augmented_requirement_text(requirement_text, scope);
    // Custom testing has no crawl-run concept of its own (no project-scoped
    // request to resolve a `crawl_run_id` from) — always `None`, honestly
    // reducing to the exact behaviour this call already had.
    // No live per-wave signal to report either — this path has no
    // project-scoped log stream caller to receive it, honestly reducing to
    // the exact behaviour this call already had.
    match ai_generation::generate_test_cases(infra, &augmented, skills, feedback, None, None).await
    {
        GenerationOutcome::Ai {
            cases,
            provider,
            model,
            ..
        } => {
            // Real router health can differ per task/call; report whichever
            // real provider/model actually produced the *cases* (what the
            // caller ultimately persists), not the analysis call's own —
            // both are real, but they are not guaranteed to be the same
            // model, and reporting the wrong one would misattribute output.
            let _ = (&analysis_provider, &analysis_model);
            CustomTestingOutcome::Ready {
                analysis,
                cases,
                provider,
                model,
            }
        }
        GenerationOutcome::Failed { reason } => CustomTestingOutcome::Failed {
            reason,
            analysis: Some(analysis),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_generation::test_support::skill;
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, CapabilityLevel, ExecutionError, ModelDefinition};
    use std::collections::HashMap;
    use std::sync::Arc;

    const VALID_ANALYSIS: &str = r#"{"objective":"Verify login lockout after repeated invalid attempts",
        "normalized_scope":"Authentication","recommended_skills":["Security Testing","Edge Case Analysis"],
        "recommended_testing_types":["Negative","Security"],"risk_areas":["Account takeover via brute force"],
        "assumptions":["Lockout threshold is a fixed number of attempts"],"prerequisites":["A valid test account"],
        "limitations":["Exact lockout threshold not stated in the requirement"],
        "generation_approach":"Deterministic negative-path scenarios around repeated invalid attempts",
        "diagnostics":[]}"#;

    const VALID_CASES: &str = r#"{"test_cases":[{"title":"Lockout after 5 invalid attempts","category":"Security",
        "skill_id":"skill-security-testing","preconditions":[],
        "steps":[{"action":"Submit 5 invalid passwords in a row","expected_result":"Account is locked"}],
        "expected_result":"Account is locked","priority":"HIGH","severity":"MAJOR",
        "evidence":"evidenced","source_reference":"repeated invalid password attempts"}]}"#;

    fn no_ai_infra() -> AiInfrastructure {
        AiInfrastructure::new(vec![], HashMap::new())
    }

    /// A model with `long_context: None` is structurally disqualified from
    /// `AiTask::RequirementAnalysis` (`score_model` returns `None`,
    /// `required_capabilities` includes `LongContext`) but fully eligible
    /// for `AiTask::TestCaseGeneration` (which never requires
    /// `LongContext`, so the disqualifying value there is simply not
    /// consulted). This is the real, existing capability-scoring mechanism
    /// (`ai_router.rs::score_model`) — not a test-only shortcut — used here
    /// to deterministically route the two real calls `run()` makes to two
    /// independently-scripted fake providers, proving each call is a
    /// genuinely separate, real router decision.
    fn cases_only_model(id: &str, provider_id: &str) -> ModelDefinition {
        let mut m = healthy_model(id, provider_id, CapabilityLevel::Strong);
        m.capabilities.long_context = Some(CapabilityLevel::None);
        m
    }

    /// Two real providers: `analysis-only` (weaker `structured_output`,
    /// normal `long_context`) can answer `RequirementAnalysis` — the only
    /// task `cases-only` is disqualified from — and loses on score to
    /// `cases-only` for `TestCaseGeneration` (stronger `structured_output`),
    /// so each of `run()`'s two real calls deterministically reaches its
    /// own independently-scripted executor.
    fn two_call_infra(analysis_response: &str, cases_response: &str) -> AiInfrastructure {
        let analysis_provider = healthy_provider(
            "analysis-only",
            vec![healthy_model(
                "analysis-model",
                "analysis-only",
                CapabilityLevel::Basic,
            )],
        );
        let cases_provider = healthy_provider(
            "cases-only",
            vec![cases_only_model("cases-model", "cases-only")],
        );

        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert(
            "analysis-only",
            Arc::new(FakeExecutor::ok(analysis_response)),
        );
        executors.insert("cases-only", Arc::new(FakeExecutor::ok(cases_response)));

        AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(analysis_provider)),
                Arc::new(FakeDiscovery(cases_provider)),
            ],
            executors,
        )
    }

    #[tokio::test]
    async fn a_full_real_round_trip_produces_ready_with_both_a_real_analysis_and_real_cases() {
        let infra = two_call_infra(VALID_ANALYSIS, VALID_CASES);
        let outcome = run(
            &infra,
            "Test login lockout after repeated invalid password attempts.",
            Some("Authentication"),
            &[skill("s", "Security Testing", "")],
            &ai_generation::FeedbackContext::default(),
        )
        .await;
        match outcome {
            CustomTestingOutcome::Ready {
                analysis,
                cases,
                provider,
                model,
            } => {
                assert_eq!(
                    analysis.objective,
                    "Verify login lockout after repeated invalid attempts"
                );
                assert_eq!(analysis.normalized_scope, "Authentication");
                assert!(analysis
                    .recommended_skills
                    .contains(&"Security Testing".to_string()));
                assert_eq!(cases.len(), 1);
                assert_eq!(cases[0].title, "Lockout after 5 invalid attempts");
                // The reported provider/model are whichever real router
                // decision produced the *cases* (what actually gets
                // persisted) — proven here to be the cases-only provider,
                // not accidentally the analysis provider's identity.
                assert_eq!(provider, "cases-only");
                assert_eq!(model, "cases-model");
            }
            CustomTestingOutcome::Failed { reason, .. } => {
                panic!("expected a full real Ready outcome, got Failed: {reason}")
            }
        }
    }

    #[tokio::test]
    async fn a_real_analysis_is_preserved_even_when_the_subsequent_case_generation_call_fails() {
        // The cases-only provider is scripted with a malformed response —
        // analysis succeeds for real, case generation then genuinely fails.
        let infra = two_call_infra(VALID_ANALYSIS, "not json at all");
        let outcome = run(
            &infra,
            "Test login lockout.",
            None,
            &[],
            &ai_generation::FeedbackContext::default(),
        )
        .await;
        match outcome {
            CustomTestingOutcome::Failed { reason, analysis } => {
                assert!(analysis.is_some(), "a successfully-produced analysis must not be discarded just because case generation failed afterward");
                assert_eq!(
                    analysis.unwrap().objective,
                    "Verify login lockout after repeated invalid attempts"
                );
                assert!(
                    reason.contains("invalid structured output"),
                    "reason={reason}"
                );
            }
            CustomTestingOutcome::Ready { .. } => {
                panic!("case generation was scripted to fail and must not be reported as Ready")
            }
        }
    }

    #[tokio::test]
    async fn ai_unavailable_is_reported_truthfully_with_no_analysis_and_no_fabricated_cases() {
        let infra = no_ai_infra();
        let outcome = run(
            &infra,
            "Test login lockout after repeated invalid password attempts.",
            Some("Authentication"),
            &[],
            &ai_generation::FeedbackContext::default(),
        )
        .await;
        match outcome {
            CustomTestingOutcome::Failed { reason, analysis } => {
                assert!(
                    analysis.is_none(),
                    "no AI provider reachable at all means no real analysis was ever produced"
                );
                assert!(!reason.is_empty());
            }
            CustomTestingOutcome::Ready { .. } => {
                panic!("must never fabricate output with zero AI providers configured")
            }
        }
    }

    #[tokio::test]
    async fn malformed_analysis_json_is_a_real_failure_not_a_crash_or_a_guess() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("mock", Arc::new(FakeExecutor::ok("not json at all")));
        let infra = AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        );
        let outcome = run(
            &infra,
            "Anything.",
            None,
            &[],
            &ai_generation::FeedbackContext::default(),
        )
        .await;
        match outcome {
            CustomTestingOutcome::Failed { reason, analysis } => {
                assert!(analysis.is_none());
                assert!(reason.contains("required structured schema"));
            }
            CustomTestingOutcome::Ready { .. } => {
                panic!("malformed AI output must never be treated as a real analysis")
            }
        }
    }

    #[tokio::test]
    async fn a_real_provider_timeout_on_the_analysis_call_is_reported_truthfully() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("mock", Arc::new(FakeExecutor::err(ExecutionError::Timeout)));
        let infra = AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        );
        let outcome = run(
            &infra,
            "Anything.",
            None,
            &[],
            &ai_generation::FeedbackContext::default(),
        )
        .await;
        match outcome {
            CustomTestingOutcome::Failed { reason, analysis } => {
                assert!(analysis.is_none());
                assert!(reason.contains("TIMEOUT"), "reason={reason}");
            }
            CustomTestingOutcome::Ready { .. } => {
                panic!("a real provider timeout must never be reported as success")
            }
        }
    }

    #[test]
    fn scope_is_genuinely_folded_into_the_augmented_requirement_text() {
        let with_scope = augmented_requirement_text("Users can log in.", Some("Authentication"));
        let without_scope = augmented_requirement_text("Users can log in.", None);
        assert!(with_scope.contains("Authentication"));
        assert_ne!(with_scope, without_scope);
        assert!(without_scope.contains("Users can log in."));
    }

    #[test]
    fn analysis_prompt_carries_the_real_requirement_scope_and_skills() {
        let prompt = build_analysis_prompt(
            "Users can log in.",
            Some("Authentication"),
            &[skill("s", "Security Testing", "")],
        );
        assert!(prompt.contains("Users can log in."));
        assert!(prompt.contains("Authentication"));
        assert!(prompt.contains("Security Testing"));
    }

    #[test]
    fn analysis_prompt_honestly_states_when_no_scope_or_skills_are_given() {
        let prompt = build_analysis_prompt("Users can log in.", None, &[]);
        assert!(prompt.contains("not specified"));
        assert!(prompt.contains("(none selected)"));
    }
}
