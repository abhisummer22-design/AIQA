//! Generation from numbered requirements instead of pasted document text.
//!
//! The original generation path pastes every uploaded document into every
//! AI call: each of eight skills re-reads the whole (20,000-character-cut)
//! text, and a case can only point back at a quoted phrase. This path works
//! from the requirements `requirements.rs` read out of the *whole* documents:
//!
//! 1. Each selected skill is shown only the requirements of the kinds it
//!    tests (`scope_of`): a security skill reads the security and interface
//!    requirements, a performance skill the performance ones, and the general
//!    skills everything they can test.
//! 2. Those requirements are cut into batches that fit the provider that will
//!    serve the call (`BatchBudget`, from `AiInfrastructure::
//!    prompt_window_tokens_for`), so no prompt outgrows the model's window and
//!    no requirement is dropped to make room.
//! 3. There is one call per skill per batch, run in the same Base ->
//!    Dependent -> NonFunctional order as before. A call sees only the earlier
//!    cases that belong to *its* requirements, so the list of "already
//!    covered" stays short however large the document is.
//! 4. Every case names the requirement it tests (`requirement_id`). The model
//!    is asked for it, and it is checked: an id that is not in the batch is
//!    replaced by the requirement whose text the case quotes, or dropped.
//!
//! `plan_calls` is the same planning the run itself uses, so what a person is
//! told before running (how many calls, how many requirements each skill
//! gets) cannot differ from what then happens.

use crate::ai_generation::{
    build_crawl_context_block, build_feedback_block, build_prior_wave_block, enforce_traceability,
    finish_waves, normalize_title, parse_and_clean, partition_skills_into_waves,
    resolve_skill_instructions, routing_mode_label, FeedbackContext, GeneratedTestCaseDraft,
    GenerationOutcome, ResolvedSkillInstruction, SkillInput, Wave, WaveSummary,
};
use crate::ai_router::{AiInfrastructure, AiRequest, AiTask, GenerationOutcome2, GenerationResult};
use crate::log_bus::LogBus;
use crate::requirements::RequirementKind;
use serde::Serialize;
use std::collections::HashSet;

// ------------------------------------------------------------------ requirements

/// One requirement as a generation call sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct RequirementItem {
    pub key: String,
    pub section: Option<String>,
    pub kind: RequirementKind,
    pub text: String,
}

impl RequirementItem {
    /// The line a generation prompt shows for this requirement.
    fn rendered(&self) -> String {
        let place = match &self.section {
            Some(section) => format!("{section} · {}", self.kind.as_str()),
            None => self.kind.as_str().to_string(),
        };
        format!("[{}] ({place}) {}", self.key, self.text)
    }
}

/// The kinds of requirement a skill tests. The general skills take nearly
/// everything, because a requirement wrongly kinded as "performance" must not
/// disappear from functional testing; the specialised skills take only what
/// they are about. A skill this does not know (a custom one) is never left
/// without requirements: it gets every kind but raw reference material.
pub fn scope_of(skill_name: &str) -> &'static [RequirementKind] {
    use RequirementKind::*;
    match skill_name.trim().to_lowercase().as_str() {
        "functional testing" | "positive testing" | "business case testing" => {
            &[Functional, Data, Interface, Security, Usability, Context]
        }
        "negative testing" => &[Functional, Data, Interface, Security, Usability],
        "edge case analysis" => &[
            Functional,
            Data,
            Interface,
            Security,
            Usability,
            Performance,
        ],
        "security testing" => &[Security, Interface, Data],
        "rbac testing" => &[Security, Functional, Interface],
        "performance testing" => &[Performance, Interface],
        "ui/ux testing" => &[Usability, Functional, Data],
        "api testing" => &[Interface, Data, Security, Reference],
        _ => &[
            Functional,
            Data,
            Interface,
            Security,
            Usability,
            Context,
            Performance,
        ],
    }
}

// ------------------------------------------------------------------------ budget

/// The most requirement text one call carries, however large the model's
/// window: a model does better with a few thousand tokens of requirements at a
/// time than with a wall of them.
pub const MAX_BATCH_CHARS: usize = 12_000;
/// The least: below this a batch holds one or two requirements and the number
/// of calls stops being sensible; the window is then the real limit.
pub const MIN_BATCH_CHARS: usize = 2_000;
pub const MAX_ITEMS_PER_BATCH: usize = 30;
/// Tokens of one call that are not requirements: the fixed instructions, the
/// skill, the lists of earlier cases and the model's answer.
const RESERVED_TOKENS: u32 = 3_500;
const CHARS_PER_TOKEN: usize = 4;
/// What is assumed when the provider says nothing about its window.
const UNKNOWN_WINDOW_TOKENS: u32 = 8_192;
/// A call sees at most this many earlier cases, the most recent ones that
/// belong to its own requirements.
const MAX_PRIOR_CASES: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BatchBudget {
    pub max_chars: usize,
    pub max_items: usize,
}

impl BatchBudget {
    /// The budget for a provider whose calls can carry `window_tokens` of
    /// input (`None` = unknown). `override_chars` (the
    /// `REQUIREMENT_BATCH_CHARS` setting) replaces the calculation.
    pub fn from_prompt_window(window_tokens: Option<u32>, override_chars: Option<usize>) -> Self {
        let max_chars = match override_chars.filter(|c| *c > 0) {
            Some(chars) => chars,
            None => {
                let room = window_tokens
                    .unwrap_or(UNKNOWN_WINDOW_TOKENS)
                    .saturating_sub(RESERVED_TOKENS) as usize
                    * CHARS_PER_TOKEN;
                room.clamp(MIN_BATCH_CHARS, MAX_BATCH_CHARS)
            }
        };
        BatchBudget {
            max_chars,
            max_items: MAX_ITEMS_PER_BATCH,
        }
    }
}

// -------------------------------------------------------------------------- plan

/// One AI call the run will make.
#[derive(Debug, Clone)]
pub struct PlannedCall {
    /// `None` only for an empty skill selection (one general call).
    pub skill: Option<SkillInput>,
    pub wave: Wave,
    pub batch_number: usize,
    pub batches_of: usize,
    /// Indices into the requirement list.
    pub items: Vec<usize>,
}

/// What one skill will be shown.
#[derive(Debug, Clone, Serialize)]
pub struct SkillPlan {
    pub skill_name: String,
    pub wave: &'static str,
    pub requirements: usize,
    pub batches: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RequirementPlan {
    pub calls: Vec<PlannedCall>,
    pub skills: Vec<SkillPlan>,
    pub total_requirements: usize,
    /// Per requirement: is at least one selected skill going to see it?
    pub offered: Vec<bool>,
    pub budget: BatchBudget,
}

impl RequirementPlan {
    pub fn offered_count(&self) -> usize {
        self.offered.iter().filter(|o| **o).count()
    }
}

/// Splits the requirements at `indices` into batches that fit `budget`, in
/// document order. A batch is closed when the next requirement would not fit,
/// when it holds the most it may, or — once it is more than half full — at the
/// end of a section, so related requirements tend to stay together.
fn split_batches(
    items: &[RequirementItem],
    indices: &[usize],
    budget: BatchBudget,
) -> Vec<Vec<usize>> {
    let mut batches: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0usize;
    for &i in indices {
        let len = items[i].rendered().chars().count() + 1;
        let starts_new_section = current
            .last()
            .is_some_and(|&last| items[last].section != items[i].section);
        let full = !current.is_empty()
            && (used + len > budget.max_chars
                || current.len() >= budget.max_items
                || (starts_new_section && used * 10 >= budget.max_chars * 6));
        if full {
            batches.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(i);
        used += len;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// Plans every call of a run: skills in Base -> Dependent -> NonFunctional
/// order, each shown the requirements of the kinds it tests, in batches.
pub fn plan_calls(
    items: &[RequirementItem],
    skills: &[SkillInput],
    budget: BatchBudget,
) -> RequirementPlan {
    let (base, dependent, non_functional) = partition_skills_into_waves(skills);
    let mut ordered: Vec<(Option<SkillInput>, Wave)> = Vec::new();
    if skills.is_empty() {
        ordered.push((None, Wave::Base));
    }
    ordered.extend(base.into_iter().map(|s| (Some(s), Wave::Base)));
    ordered.extend(dependent.into_iter().map(|s| (Some(s), Wave::Dependent)));
    ordered.extend(
        non_functional
            .into_iter()
            .map(|s| (Some(s), Wave::NonFunctional)),
    );

    let mut plan = RequirementPlan {
        calls: Vec::new(),
        skills: Vec::new(),
        total_requirements: items.len(),
        offered: vec![false; items.len()],
        budget,
    };
    for (skill, wave) in ordered {
        let name = skill.as_ref().map(|s| s.name.clone()).unwrap_or_default();
        // No skill named: the same general functional coverage the original path gives.
        let scope = scope_of(if name.is_empty() {
            "functional testing"
        } else {
            &name
        });
        let indices: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| scope.contains(&item.kind))
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            plan.skills.push(SkillPlan {
                skill_name: name,
                wave: wave.label(),
                requirements: 0,
                batches: 0,
                note: Some(
                    "none of the selected documents has a requirement of a kind this skill tests — skipped"
                        .to_string(),
                ),
            });
            continue;
        }
        for &i in &indices {
            plan.offered[i] = true;
        }
        let batches = split_batches(items, &indices, budget);
        plan.skills.push(SkillPlan {
            skill_name: name,
            wave: wave.label(),
            requirements: indices.len(),
            batches: batches.len(),
            note: None,
        });
        let batches_of = batches.len();
        for (n, batch) in batches.into_iter().enumerate() {
            plan.calls.push(PlannedCall {
                skill: skill.clone(),
                wave,
                batch_number: n + 1,
                batches_of,
                items: batch,
            });
        }
    }
    plan
}

/// A rough, disclosed estimate of a plan's size — not a quote: prompt tokens
/// from what the calls will carry, completion tokens from a typical answer.
pub fn estimate_tokens(plan: &RequirementPlan, items: &[RequirementItem]) -> (u64, u64) {
    const FIXED_PROMPT_TOKENS: u64 = 1_300;
    const PRIOR_CASES_TOKENS: u64 = 300;
    const TYPICAL_ANSWER_TOKENS: u64 = 2_000;
    let mut prompt = 0u64;
    for call in &plan.calls {
        let chars: usize = call
            .items
            .iter()
            .map(|&i| items[i].rendered().chars().count() + 1)
            .sum();
        prompt += FIXED_PROMPT_TOKENS + PRIOR_CASES_TOKENS + (chars / CHARS_PER_TOKEN) as u64;
    }
    (prompt, plan.calls.len() as u64 * TYPICAL_ANSWER_TOKENS)
}

// ------------------------------------------------------------------------ prompt

/// The prompt for one call: this batch's requirements, the skill, the rules.
/// Mirrors `ai_generation::build_prompt`'s evidence rules; what differs is
/// that the requirements are numbered and every case must name its own.
pub fn build_requirement_prompt(
    settings_text: &str,
    batch: &[&RequirementItem],
    batch_number: usize,
    batches_of: usize,
    skills: &[ResolvedSkillInstruction],
    feedback: &FeedbackContext,
    prior_cases: &[GeneratedTestCaseDraft],
    crawl: Option<&crate::crawler::CrawlOutput>,
) -> String {
    let mut skill_blocks = String::new();
    for s in skills {
        skill_blocks.push_str(&format!(
            "- Skill \"{}\" (id: {}): {}\n",
            s.name, s.id, s.instruction
        ));
    }
    if skills.is_empty() {
        skill_blocks
            .push_str("- (no skill selected — generate general functional coverage only)\n");
    }
    let lines: String = batch
        .iter()
        .map(|r| format!("{}\n", r.rendered()))
        .collect();
    let settings = if settings_text.trim().is_empty() {
        "(none)"
    } else {
        settings_text.trim()
    };
    let crawl_block = build_crawl_context_block(crawl);
    let feedback_block = build_feedback_block(feedback);
    let prior_block = build_prior_wave_block(prior_cases);

    format!(
        "You are a QA test case generator. Generate structured test cases strictly from the numbered \
         requirements below.\n\n\
         GENERATION SETTINGS (what the user asked for — not requirements themselves):\n{settings}\n\n\
         REQUIREMENTS — batch {batch_number} of {batches_of} for this skill. Each line starts with the \
         requirement's id in square brackets:\n{lines}\n\
         {crawl_block}\
         SELECTED SKILLS (each test case must map to exactly one of these; do not repeat the same \
         scenario for multiple skills):\n\
         {skill_blocks}\n\
         EVIDENCE RULES (critical, do not violate):\n\
         - Every test case must set \"requirement_id\" to the id of the ONE requirement it tests, exactly \
           as written inside the square brackets above. Never invent an id.\n\
         - If a test case's expected behavior is directly stated in that requirement, set \
           \"evidence\": \"evidenced\" and set \"source_reference\" to the exact short phrase from that \
           requirement's text that supports it.\n\
         - If a test case is a reasonable extension of what the requirement implies but not stated \
           outright, set \"evidence\": \"inferred\" and \"source_reference\": null.\n\
         - If a test case exists only because the selected skill calls for exploring it and the \
           requirement gives no basis at all, set \"evidence\": \"exploratory\" and \
           \"source_reference\": null.\n\
         - Never invent a specific numeric requirement (character counts, time limits, thresholds) that \
           is not literally present in the requirements. If you need a number and there is none, omit \
           the number and mark the case \"inferred\" or \"exploratory\".\n\n\
         COVERAGE: Work through the requirements one by one: every requirement this skill can \
         meaningfully test should get at least one test case, and a requirement with real depth gets \
         as many distinct, non-redundant cases as it supports. Do not stop at a small illustrative \
         sample, and do not invent a case with no real basis just to raise the count.\n\n\
         {feedback_block}\
         {prior_block}\
         Return ONLY a JSON object with this exact shape, no markdown, no commentary:\n\
         {{\"test_cases\": [{{\"title\": string, \"category\": string, \"skill_id\": string, \
         \"requirement_id\": string, \
         \"preconditions\": [string], \"steps\": [{{\"action\": string, \"expected_result\": string}}], \
         \"expected_result\": string, \"priority\": \"CRITICAL\"|\"HIGH\"|\"MEDIUM\"|\"LOW\", \
         \"severity\": \"CRITICAL\"|\"MAJOR\"|\"MINOR\"|\"TRIVIAL\", \
         \"evidence\": \"evidenced\"|\"inferred\"|\"exploratory\", \
         \"source_reference\": string|null}}]}}"
    )
}

// ---------------------------------------------------------------------- linking

/// Makes every case name a requirement of `batch`, or none. The model's own
/// `requirement_id` is kept when it is one of the batch's; otherwise the case
/// is tied to the single requirement whose text contains the phrase it quotes
/// (`source_reference`, already checked against the batch); otherwise it stays
/// unlinked rather than carrying an id nobody wrote.
fn link_to_requirements(cases: &mut [GeneratedTestCaseDraft], batch: &[&RequirementItem]) {
    for case in cases {
        let claimed = case
            .requirement_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let by_id = claimed.and_then(|id| batch.iter().find(|r| r.key.eq_ignore_ascii_case(id)));
        let by_quote = || {
            let quote = case.source_reference.as_deref()?.trim().to_lowercase();
            if quote.is_empty() {
                return None;
            }
            let mut hits = batch
                .iter()
                .filter(|r| r.text.to_lowercase().contains(&quote));
            let first = hits.next()?;
            hits.next().is_none().then_some(first)
        };
        let found = by_id.or_else(by_quote);
        case.requirement_id = found.map(|r| r.key.clone());
    }
}

/// The whole-run de-duplication: two cases are the same coverage only when
/// they test the same requirement and carry the same title. ("Verify login
/// with valid credentials" for two different requirements is two cases.)
fn dedupe_by_requirement(cases: Vec<GeneratedTestCaseDraft>) -> Vec<GeneratedTestCaseDraft> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut out = Vec::new();
    for case in cases {
        let key = (
            case.requirement_id.clone().unwrap_or_default(),
            normalize_title(&case.title),
        );
        if seen.insert(key) {
            out.push(case);
        }
    }
    out
}

// --------------------------------------------------------------------- execution

/// One AI call: the prompt out, cases back, each checked against the batch
/// and tied to its requirement. A response that cannot be used gets one more
/// try, like every other generation path; a refusal by the provider is final.
async fn generate_call(
    infra: &AiInfrastructure,
    prompt: String,
    batch: &[&RequirementItem],
) -> Result<(Vec<GeneratedTestCaseDraft>, GenerationResult), String> {
    let request = AiRequest {
        task: AiTask::TestCaseGeneration,
        prompt,
        preferred_provider: None,
        preferred_model: None,
    };
    let batch_text = batch
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut last_failure: Option<String> = None;
    for _attempt in 0..2 {
        let result = match infra.generate(request.clone()).await {
            GenerationOutcome2::Ok(r) => r,
            GenerationOutcome2::Failed { reason } => return Err(reason),
        };
        match parse_and_clean(&result) {
            Ok(cases) => {
                let mut cases = enforce_traceability(cases, &batch_text);
                link_to_requirements(&mut cases, batch);
                return Ok((cases, result));
            }
            Err(reason) => last_failure = Some(reason),
        }
    }
    Err(last_failure.unwrap_or_else(|| "unknown generation failure".to_string()))
}

/// What a run told about how much of the requirement set it reached.
#[derive(Debug, Clone, Serialize)]
pub struct RequirementCoverage {
    pub total: usize,
    /// Requirements at least one selected skill was shown.
    pub offered: usize,
    /// Requirements with at least one generated case.
    pub covered: usize,
    /// Shown to a skill but left without a case (a failed call, or nothing to
    /// test) — at most 100 ids.
    pub uncovered_keys: Vec<String>,
    /// Of a kind no selected skill tests — at most 100 ids.
    pub not_offered_keys: Vec<String>,
    pub notes: Vec<String>,
    /// AI calls the run planned.
    pub calls: usize,
    pub budget_chars: usize,
}

const MAX_LISTED_KEYS: usize = 100;

fn coverage_of(
    plan: &RequirementPlan,
    items: &[RequirementItem],
    cases: &[GeneratedTestCaseDraft],
) -> RequirementCoverage {
    let with_cases: HashSet<&str> = cases
        .iter()
        .filter_map(|c| c.requirement_id.as_deref())
        .collect();
    let mut covered = 0usize;
    let mut uncovered_keys = Vec::new();
    let mut not_offered_keys = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if with_cases.contains(item.key.as_str()) {
            covered += 1;
        } else if plan.offered[i] {
            if uncovered_keys.len() < MAX_LISTED_KEYS {
                uncovered_keys.push(item.key.clone());
            }
        } else if not_offered_keys.len() < MAX_LISTED_KEYS {
            not_offered_keys.push(item.key.clone());
        }
    }
    let notes = plan
        .skills
        .iter()
        .filter_map(|s| {
            s.note.as_ref().map(|n| {
                if s.skill_name.is_empty() {
                    n.clone()
                } else {
                    format!("{}: {n}", s.skill_name)
                }
            })
        })
        .collect();
    RequirementCoverage {
        total: items.len(),
        offered: plan.offered_count(),
        covered,
        uncovered_keys,
        not_offered_keys,
        notes,
        calls: plan.calls.len(),
        budget_chars: plan.budget.max_chars,
    }
}

pub struct RequirementRun {
    pub outcome: GenerationOutcome,
    pub coverage: RequirementCoverage,
}

/// Runs a requirement-driven generation: plans the calls, makes them one after
/// another, and reports what was reached. `settings_text` is the user's own
/// request text (categories, depth, connected sources), shown in every prompt.
#[allow(clippy::too_many_arguments)]
pub async fn generate_from_requirements(
    infra: &AiInfrastructure,
    settings_text: &str,
    items: &[RequirementItem],
    skills: &[SkillInput],
    feedback: &FeedbackContext,
    crawl: Option<&crate::crawler::CrawlOutput>,
    budget: BatchBudget,
    progress: Option<(&LogBus, &str)>,
) -> RequirementRun {
    let plan = plan_calls(items, skills, budget);
    let mut all_cases: Vec<GeneratedTestCaseDraft> = Vec::new();
    let mut waves: Vec<WaveSummary> = Vec::new();

    for call in &plan.calls {
        let batch: Vec<&RequirementItem> = call.items.iter().map(|&i| &items[i]).collect();
        let keys: HashSet<&str> = batch.iter().map(|r| r.key.as_str()).collect();
        // The most recent earlier cases that belong to this batch's requirements.
        let mut prior: Vec<GeneratedTestCaseDraft> = all_cases
            .iter()
            .rev()
            .filter(|c| {
                c.requirement_id
                    .as_deref()
                    .is_some_and(|id| keys.contains(id))
            })
            .take(MAX_PRIOR_CASES)
            .cloned()
            .collect();
        prior.reverse();

        let skill_list: Vec<SkillInput> = call.skill.iter().cloned().collect();
        let skill_names: Vec<String> = skill_list.iter().map(|s| s.name.clone()).collect();
        let resolved = resolve_skill_instructions(&skill_list);
        let prompt = build_requirement_prompt(
            settings_text,
            &batch,
            call.batch_number,
            call.batches_of,
            &resolved,
            feedback,
            &prior,
            crawl,
        );
        let batch_label = format!("{}/{}", call.batch_number, call.batches_of);

        if let Some((log_bus, project_id)) = progress {
            // Same wording as the original path's message, so the page's live
            // "which skill is running" indicator reads it unchanged.
            log_bus
                .emit(
                    "INFO",
                    "generation",
                    &format!(
                        "Wave '{}' ({}) started",
                        call.wave.label(),
                        skill_names.join(", ")
                    ),
                    None,
                    Some(project_id),
                    None,
                )
                .await;
            log_bus
                .emit(
                    "INFO",
                    "generation",
                    &format!(
                        "Batch {batch_label} of {}: {} requirement(s)",
                        if skill_names.is_empty() {
                            "general coverage".to_string()
                        } else {
                            skill_names.join(", ")
                        },
                        batch.len()
                    ),
                    None,
                    Some(project_id),
                    None,
                )
                .await;
        }

        match generate_call(infra, prompt, &batch).await {
            Ok((cases, result)) => {
                waves.push(WaveSummary {
                    wave: call.wave.label(),
                    skill_names,
                    case_count: cases.len(),
                    provider: Some(result.provider.clone()),
                    model: Some(result.model.clone()),
                    routing_score: Some(result.routing_score),
                    fallback_used: Some(result.fallback_used),
                    routing_mode: Some(routing_mode_label(&result)),
                    prompt_tokens: result.prompt_tokens,
                    completion_tokens: result.completion_tokens,
                    estimated_cost_usd: result.estimated_cost_usd,
                    latency_ms: Some(result.latency_ms),
                    failure_reason: None,
                    skipped: false,
                    batch: Some(batch_label),
                    requirements: Some(batch.len()),
                });
                all_cases.extend(cases);
            }
            Err(reason) => waves.push(WaveSummary {
                wave: call.wave.label(),
                skill_names,
                case_count: 0,
                provider: None,
                model: None,
                routing_score: None,
                fallback_used: None,
                routing_mode: None,
                prompt_tokens: None,
                completion_tokens: None,
                estimated_cost_usd: None,
                latency_ms: None,
                failure_reason: Some(reason),
                skipped: false,
                batch: Some(batch_label),
                requirements: Some(batch.len()),
            }),
        }
    }

    let outcome = if plan.calls.is_empty() {
        GenerationOutcome::Failed {
            reason: "none of the selected documents has a requirement of a kind the selected skills test"
                .to_string(),
        }
    } else {
        finish_waves(waves, all_cases, dedupe_by_requirement)
    };
    let final_cases: &[GeneratedTestCaseDraft] = match &outcome {
        GenerationOutcome::Ai { cases, .. } => cases,
        GenerationOutcome::Failed { .. } => &[],
    };
    let coverage = coverage_of(&plan, items, final_cases);
    RequirementRun { outcome, coverage }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_generation::test_support::skill;
    use crate::ai_router::test_support::{healthy_model, healthy_provider, FakeDiscovery};
    use crate::ai_router::{
        AiExecutor, CapabilityLevel, ExecutionError, ExecutionOutput, TokenUsage,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn item(key: &str, section: &str, kind: RequirementKind, text: &str) -> RequirementItem {
        RequirementItem {
            key: key.to_string(),
            section: Some(section.to_string()),
            kind,
            text: text.to_string(),
        }
    }

    /// `n` functional requirements R1..Rn, each about 100 characters long.
    fn functional(n: usize) -> Vec<RequirementItem> {
        (1..=n)
            .map(|i| {
                item(
                    &format!("R{i}"),
                    "3 Scope",
                    RequirementKind::Functional,
                    &format!("The system shall handle scenario number {i} as the specification says in full detail."),
                )
            })
            .collect()
    }

    fn budget(max_chars: usize) -> BatchBudget {
        BatchBudget {
            max_chars,
            max_items: MAX_ITEMS_PER_BATCH,
        }
    }

    // ------------------------------------------------------------------ scopes

    #[test]
    fn general_skills_take_nearly_everything_and_specialised_ones_only_their_own_kinds() {
        use RequirementKind::*;
        for general in [
            "Functional Testing",
            "Positive Testing",
            "Business Case Testing",
        ] {
            let scope = scope_of(general);
            assert!(
                scope.contains(&Functional) && scope.contains(&Context),
                "{general}"
            );
            assert!(
                !scope.contains(&Performance) && !scope.contains(&Reference),
                "{general}"
            );
        }
        assert!(scope_of("Negative Testing").contains(&Data));
        assert!(scope_of("Edge Case Analysis").contains(&Performance));
        assert_eq!(scope_of("Security Testing"), &[Security, Interface, Data]);
        assert_eq!(scope_of("Performance Testing"), &[Performance, Interface]);
        assert!(scope_of("API Testing").contains(&Reference));
        assert!(scope_of("  ui/ux TESTING ").contains(&Usability));
        // A skill this does not know is never left with nothing, and never sees raw reference material.
        let custom = scope_of("Boundary Testing for Indian PIN Codes");
        assert!(
            custom.contains(&Functional)
                && custom.contains(&Performance)
                && !custom.contains(&Reference)
        );
    }

    // ------------------------------------------------------------------ budget

    #[test]
    fn the_batch_budget_follows_the_window_within_sensible_limits() {
        let of = |w, o| BatchBudget::from_prompt_window(w, o).max_chars;
        assert_eq!(
            of(None, None),
            12_000,
            "an unknown window is assumed to be 8,192 tokens"
        );
        assert_eq!(
            of(Some(4096), None),
            2_384,
            "Ollama's default window leaves room for very little"
        );
        assert_eq!(
            of(Some(1000), None),
            MIN_BATCH_CHARS,
            "never below the floor"
        );
        assert_eq!(
            of(Some(8000), None),
            12_000,
            "Groq's free tier still fills the cap"
        );
        assert_eq!(
            of(Some(1_048_576), None),
            MAX_BATCH_CHARS,
            "however large the window"
        );
        assert_eq!(
            of(Some(4096), Some(5_000)),
            5_000,
            "an explicit setting wins"
        );
        assert_eq!(of(Some(4096), Some(0)), 2_384, "zero is not a setting");
    }

    // -------------------------------------------------------------------- plan

    #[test]
    fn batches_keep_document_order_lose_nothing_and_respect_the_limits() {
        let items = functional(30);
        let plan = plan_calls(
            &items,
            &[skill("s1", "Functional Testing", "")],
            budget(1_000),
        );

        let flat: Vec<usize> = plan.calls.iter().flat_map(|c| c.items.clone()).collect();
        assert_eq!(
            flat,
            (0..30).collect::<Vec<_>>(),
            "every requirement exactly once, in order"
        );
        assert!(plan.calls.len() > 3);
        for call in &plan.calls {
            let chars: usize = call
                .items
                .iter()
                .map(|&i| items[i].rendered().chars().count() + 1)
                .sum();
            assert!(
                chars <= 1_000,
                "a batch of {chars} chars is over the budget"
            );
            assert_eq!(call.batches_of, plan.calls.len());
        }
        assert_eq!(
            plan.calls
                .iter()
                .map(|c| c.batch_number)
                .collect::<Vec<_>>(),
            (1..=plan.calls.len()).collect::<Vec<_>>()
        );
        assert_eq!(plan.skills[0].requirements, 30);
        assert_eq!(plan.skills[0].batches, plan.calls.len());
    }

    #[test]
    fn a_batch_never_holds_more_than_the_most_items_it_may() {
        let items: Vec<RequirementItem> = (0..70)
            .map(|i| item(&format!("R{i}"), "S", RequirementKind::Functional, "Tiny."))
            .collect();
        let plan = plan_calls(
            &items,
            &[skill("s1", "Functional Testing", "")],
            budget(12_000),
        );
        assert_eq!(
            plan.calls.iter().map(|c| c.items.len()).collect::<Vec<_>>(),
            [30, 30, 10]
        );
    }

    #[test]
    fn a_batch_more_than_half_full_closes_at_the_end_of_a_section() {
        let mut items = Vec::new();
        for i in 0..4 {
            items.push(item(
                &format!("A{i}"),
                "A",
                RequirementKind::Functional,
                &"a".repeat(200),
            ));
        }
        for i in 0..4 {
            items.push(item(
                &format!("B{i}"),
                "B",
                RequirementKind::Functional,
                &"b".repeat(200),
            ));
        }
        // Section A fills about 70% of the budget; B would still fit, but starts a new section.
        let plan = plan_calls(
            &items,
            &[skill("s1", "Functional Testing", "")],
            budget(1_300),
        );
        assert_eq!(plan.calls[0].items, [0, 1, 2, 3]);
        assert_eq!(plan.calls[1].items, [4, 5, 6, 7]);
    }

    #[test]
    fn a_batch_less_than_half_full_carries_on_into_the_next_section() {
        let items = vec![
            item("A0", "A", RequirementKind::Functional, &"a".repeat(100)),
            item("B0", "B", RequirementKind::Functional, &"b".repeat(100)),
        ];
        let plan = plan_calls(
            &items,
            &[skill("s1", "Functional Testing", "")],
            budget(5_000),
        );
        assert_eq!(plan.calls.len(), 1);
    }

    fn mixed() -> Vec<RequirementItem> {
        use RequirementKind::*;
        vec![
            item(
                "F1",
                "1",
                Functional,
                "The system shall let a user sign in with a password.",
            ),
            item(
                "S1",
                "1",
                Security,
                "The system shall lock an account after five failed sign-ins.",
            ),
            item(
                "P1",
                "2",
                Performance,
                "The dashboard shall load within two seconds for a user.",
            ),
            item(
                "I1",
                "2",
                Interface,
                "The service shall expose a REST endpoint returning JSON data.",
            ),
            item(
                "X1",
                "3",
                Reference,
                "openapi: 3.0.0 info: title: Sample specification document",
            ),
        ]
    }

    #[test]
    fn each_skill_is_shown_only_the_kinds_it_tests() {
        let skills = [
            skill("a", "Functional Testing", ""),
            skill("b", "Security Testing", ""),
            skill("c", "Performance Testing", ""),
            skill("d", "API Testing", ""),
        ];
        let items = mixed();
        let plan = plan_calls(&items, &skills, budget(12_000));

        let seen = |name: &str| -> Vec<&str> {
            plan.calls
                .iter()
                .filter(|c| c.skill.as_ref().map(|s| s.name.as_str()) == Some(name))
                .flat_map(|c| c.items.iter().map(|&i| items[i].key.as_str()))
                .collect()
        };
        assert_eq!(seen("Functional Testing"), ["F1", "S1", "I1"]);
        assert_eq!(seen("Security Testing"), ["S1", "I1"]);
        assert_eq!(seen("Performance Testing"), ["P1", "I1"]);
        assert_eq!(seen("API Testing"), ["S1", "I1", "X1"]);
        assert_eq!(plan.offered, [true, true, true, true, true]);
    }

    #[test]
    fn a_requirement_no_selected_skill_tests_is_reported_as_not_offered() {
        let items = mixed();
        let plan = plan_calls(
            &items,
            &[skill("a", "Functional Testing", "")],
            budget(12_000),
        );
        // Performance and reference requirements are of no kind a functional skill tests.
        assert_eq!(plan.offered, [true, true, false, true, false]);
        assert_eq!(plan.offered_count(), 3);
        let coverage = coverage_of(&plan, &items, &[]);
        assert_eq!(coverage.not_offered_keys, ["P1", "X1"]);
        assert_eq!(coverage.uncovered_keys, ["F1", "S1", "I1"]);
    }

    #[test]
    fn a_skill_with_nothing_of_its_kind_is_skipped_with_a_note() {
        let items = functional(3);
        let plan = plan_calls(
            &items,
            &[
                skill("a", "Functional Testing", ""),
                skill("c", "Performance Testing", ""),
            ],
            budget(12_000),
        );
        assert_eq!(plan.calls.len(), 1);
        let perf = plan
            .skills
            .iter()
            .find(|s| s.skill_name == "Performance Testing")
            .unwrap();
        assert_eq!((perf.requirements, perf.batches), (0, 0));
        assert!(perf.note.as_deref().unwrap().contains("skipped"));
        let coverage = coverage_of(&plan, &items, &[]);
        assert!(
            coverage.notes[0].starts_with("Performance Testing: "),
            "{:?}",
            coverage.notes
        );
    }

    #[test]
    fn calls_run_base_then_dependent_then_non_functional_whatever_the_selection_order() {
        let items = mixed();
        let plan = plan_calls(
            &items,
            &[
                skill("s", "Security Testing", ""),
                skill("r", "RBAC Testing", ""),
                skill("f", "Functional Testing", ""),
            ],
            budget(12_000),
        );
        let order: Vec<(&str, &str)> = plan
            .calls
            .iter()
            .map(|c| (c.wave.label(), c.skill.as_ref().unwrap().name.as_str()))
            .collect();
        assert_eq!(
            order,
            [
                ("base", "Functional Testing"),
                ("dependent", "RBAC Testing"),
                ("non_functional", "Security Testing")
            ]
        );
    }

    #[test]
    fn an_empty_selection_makes_one_general_call_per_batch() {
        let items = functional(2);
        let plan = plan_calls(&items, &[], budget(12_000));
        assert_eq!(plan.calls.len(), 1);
        assert!(plan.calls[0].skill.is_none());
        assert_eq!(plan.calls[0].wave, Wave::Base);
        assert_eq!(plan.skills[0].skill_name, "");
    }

    #[test]
    fn no_requirements_means_no_calls() {
        let plan = plan_calls(&[], &[skill("a", "Functional Testing", "")], budget(12_000));
        assert!(plan.calls.is_empty());
        assert_eq!(plan.total_requirements, 0);
    }

    #[test]
    fn the_estimate_grows_with_the_calls_and_the_requirement_text() {
        let items = functional(10);
        let small = plan_calls(
            &items,
            &[skill("a", "Functional Testing", "")],
            budget(12_000),
        );
        let large = plan_calls(
            &items,
            &[
                skill("a", "Functional Testing", ""),
                skill("b", "Negative Testing", ""),
            ],
            budget(12_000),
        );
        let (p1, c1) = estimate_tokens(&small, &items);
        let (p2, c2) = estimate_tokens(&large, &items);
        assert!(p2 > p1 && c2 == 2 * c1, "{p1} {c1} {p2} {c2}");
        assert_eq!(
            estimate_tokens(&plan_calls(&[], &[], budget(12_000)), &[]),
            (0, 0)
        );
    }

    // ------------------------------------------------------------------ prompt

    #[test]
    fn the_prompt_holds_this_batchs_requirements_only_and_says_which_batch_it_is() {
        let items = functional(4);
        let batch: Vec<&RequirementItem> = vec![&items[1], &items[2]];
        let resolved = resolve_skill_instructions(&[skill("f", "Functional Testing", "")]);

        let prompt = build_requirement_prompt(
            "Requested generation depth: Standard.",
            &batch,
            2,
            5,
            &resolved,
            &FeedbackContext::default(),
            &[],
            None,
        );

        assert!(prompt.contains("batch 2 of 5"), "{prompt}");
        assert!(prompt
            .contains("[R2] (3 Scope · functional) The system shall handle scenario number 2"));
        assert!(prompt.contains("[R3] "));
        assert!(!prompt.contains("[R1] ") && !prompt.contains("[R4] "));
        assert!(prompt.contains("Requested generation depth: Standard."));
        assert!(prompt.contains("Skill \"Functional Testing\""));
        assert!(prompt.contains("\"requirement_id\": string"));
        assert!(prompt.contains("exactly as written inside the square brackets"));
    }

    #[test]
    fn empty_settings_and_no_skill_are_said_plainly() {
        let items = functional(1);
        let prompt = build_requirement_prompt(
            "  ",
            &[&items[0]],
            1,
            1,
            &[],
            &FeedbackContext::default(),
            &[],
            None,
        );
        assert!(prompt.contains(
            "GENERATION SETTINGS (what the user asked for — not requirements themselves):\n(none)"
        ));
        assert!(prompt.contains("(no skill selected"));
    }

    // ------------------------------------------------------------------ linking

    fn draft(
        title: &str,
        requirement_id: Option<&str>,
        source_reference: Option<&str>,
    ) -> GeneratedTestCaseDraft {
        GeneratedTestCaseDraft {
            title: title.to_string(),
            category: "Functional".to_string(),
            skill_id: "s".to_string(),
            preconditions: vec![],
            steps: vec![crate::ai_generation::DraftStep {
                action: "Do it".to_string(),
                expected_result: "It works".to_string(),
            }],
            expected_result: "It works".to_string(),
            priority: "HIGH".to_string(),
            severity: "MAJOR".to_string(),
            evidence: crate::ai_generation::EvidenceKind::Inferred,
            source_reference: source_reference.map(str::to_string),
            requirement_id: requirement_id.map(str::to_string),
        }
    }

    #[test]
    fn a_case_is_tied_to_the_requirement_it_names_or_else_the_one_it_quotes() {
        let items = vec![
            item(
                "FR-1",
                "S",
                RequirementKind::Functional,
                "Users must sign in with a password.",
            ),
            item(
                "FR-2",
                "S",
                RequirementKind::Functional,
                "Sessions must expire after thirty minutes.",
            ),
            item(
                "FR-3",
                "S",
                RequirementKind::Functional,
                "Passwords must be changed every ninety days.",
            ),
        ];
        let batch: Vec<&RequirementItem> = items.iter().collect();
        let mut cases = vec![
            draft("names an id", Some("FR-2"), None),
            draft("names an id in another case", Some(" fr-1 "), None),
            draft(
                "invents an id, quotes one requirement",
                Some("FR-99"),
                Some("expire after thirty minutes"),
            ),
            draft("invents an id, quotes nothing", Some("FR-99"), None),
            draft(
                "no id, quotes a phrase two requirements share",
                None,
                Some("must"),
            ),
            draft("no id, no quote", None, None),
            draft("blank id", Some("  "), Some("sign in with a password")),
        ];

        link_to_requirements(&mut cases, &batch);

        let linked: Vec<Option<&str>> = cases.iter().map(|c| c.requirement_id.as_deref()).collect();
        assert_eq!(
            linked,
            [
                Some("FR-2"),
                Some("FR-1"),
                Some("FR-2"),
                None,
                None,
                None,
                Some("FR-1")
            ]
        );
    }

    #[test]
    fn the_same_title_for_two_requirements_is_two_cases_and_for_one_requirement_is_one() {
        let cases = vec![
            draft("Verify login", Some("R1"), None),
            draft("Verify login", Some("R2"), None),
            draft("verify  LOGIN!", Some("R1"), None),
            draft("Verify login", None, None),
        ];
        let kept = dedupe_by_requirement(cases);
        let ids: Vec<Option<&str>> = kept.iter().map(|c| c.requirement_id.as_deref()).collect();
        // R1 once, R2 once, unlinked once.
        assert_eq!(ids, [Some("R1"), Some("R2"), None]);
    }

    // --------------------------------------------------------------- execution

    type Reply = Box<dyn Fn(usize, &str) -> Result<String, ExecutionError> + Send + Sync>;

    struct Scripted {
        prompts: Mutex<Vec<String>>,
        reply: Reply,
    }

    #[async_trait::async_trait]
    impl AiExecutor for Scripted {
        async fn execute(
            &self,
            _provider_id: &str,
            _model_id: &str,
            prompt: &str,
        ) -> Result<ExecutionOutput, ExecutionError> {
            let n = {
                let mut prompts = self.prompts.lock().unwrap();
                prompts.push(prompt.to_string());
                prompts.len() - 1
            };
            (self.reply)(n, prompt).map(|text| ExecutionOutput {
                text,
                usage: TokenUsage {
                    prompt_tokens: Some(100),
                    completion_tokens: Some(50),
                },
            })
        }
    }

    fn infra_with(reply: Reply) -> (AiInfrastructure, Arc<Scripted>) {
        let executor = Arc::new(Scripted {
            prompts: Mutex::new(Vec::new()),
            reply,
        });
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("mock", executor.clone());
        let infra = AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        );
        (infra, executor)
    }

    /// The requirement ids a prompt lists, in order.
    fn ids_in(prompt: &str) -> Vec<String> {
        prompt
            .lines()
            .filter_map(|l| {
                let rest = l.strip_prefix('[')?;
                let (id, tail) = rest.split_once("] (")?;
                tail.contains(')').then(|| id.to_string())
            })
            .collect()
    }

    fn case_json(
        title: &str,
        requirement_id: &str,
        evidence: &str,
        source_reference: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "title": title, "category": "Functional", "skill_id": "s", "requirement_id": requirement_id,
            "preconditions": [], "steps": [{"action": "Do it", "expected_result": "It works"}],
            "expected_result": "It works", "priority": "HIGH", "severity": "MAJOR",
            "evidence": evidence, "source_reference": source_reference,
        })
    }

    /// One case per requirement the prompt lists, each naming its own.
    fn echo() -> Reply {
        Box::new(|_, prompt| {
            let cases: Vec<_> = ids_in(prompt)
                .iter()
                .map(|id| case_json(&format!("Check {id}"), id, "inferred", None))
                .collect();
            Ok(serde_json::json!({ "test_cases": cases }).to_string())
        })
    }

    async fn run(
        infra: &AiInfrastructure,
        items: &[RequirementItem],
        skills: &[SkillInput],
        max_chars: usize,
    ) -> RequirementRun {
        generate_from_requirements(
            infra,
            "Requested generation depth: Standard.",
            items,
            skills,
            &FeedbackContext::default(),
            None,
            budget(max_chars),
            None,
        )
        .await
    }

    fn cases_of(run: &RequirementRun) -> Vec<GeneratedTestCaseDraft> {
        match &run.outcome {
            GenerationOutcome::Ai { cases, .. } => cases.clone(),
            GenerationOutcome::Failed { reason } => panic!("expected cases, got: {reason}"),
        }
    }

    fn waves_of(run: &RequirementRun) -> Vec<WaveSummary> {
        match &run.outcome {
            GenerationOutcome::Ai { waves, .. } => waves.clone(),
            GenerationOutcome::Failed { reason } => panic!("expected waves, got: {reason}"),
        }
    }

    #[tokio::test]
    async fn every_requirement_reaches_a_call_in_its_batch_and_every_case_names_its_requirement() {
        let (infra, executor) = infra_with(echo());
        let items = functional(12);

        let result = run(&infra, &items, &[skill("f", "Functional Testing", "")], 600).await;

        let prompts = executor.prompts.lock().unwrap().clone();
        assert!(
            prompts.len() > 2,
            "600 characters cannot hold twelve requirements"
        );
        // No requirement is repeated across calls and none is missing.
        let seen: Vec<String> = prompts.iter().flat_map(|p| ids_in(p)).collect();
        assert_eq!(seen, (1..=12).map(|i| format!("R{i}")).collect::<Vec<_>>());
        // A prompt does not carry the requirements of another batch.
        assert!(!prompts[0].contains("[R12]"));

        let cases = cases_of(&result);
        assert_eq!(cases.len(), 12);
        assert!(cases.iter().all(|c| c.requirement_id.as_deref()
            == Some(&*format!("R{}", c.title.trim_start_matches("Check R")))));

        let waves = waves_of(&result);
        assert_eq!(waves.len(), prompts.len());
        assert_eq!(
            waves[0].batch.as_deref(),
            Some(&*format!("1/{}", prompts.len()))
        );
        assert_eq!(
            waves.iter().map(|w| w.requirements.unwrap()).sum::<usize>(),
            12
        );
        assert!(waves
            .iter()
            .all(|w| w.failure_reason.is_none() && w.skill_names == ["Functional Testing"]));

        let c = &result.coverage;
        assert_eq!((c.total, c.offered, c.covered), (12, 12, 12));
        assert!(c.uncovered_keys.is_empty() && c.not_offered_keys.is_empty());
        assert_eq!(c.calls, prompts.len());
    }

    #[tokio::test]
    async fn a_specialised_skill_is_never_shown_a_kind_it_does_not_test() {
        let (infra, executor) = infra_with(echo());
        let items = mixed();

        let result = run(
            &infra,
            &items,
            &[skill("s", "Security Testing", "")],
            12_000,
        )
        .await;

        let prompts = executor.prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 1);
        assert_eq!(ids_in(&prompts[0]), ["S1", "I1"]);
        assert!(!prompts[0].contains("[F1]") && !prompts[0].contains("[P1]"));
        assert_eq!(result.coverage.covered, 2);
        assert_eq!(result.coverage.not_offered_keys, ["F1", "P1", "X1"]);
    }

    #[tokio::test]
    async fn an_id_the_model_invented_is_replaced_by_the_requirement_it_quotes_or_dropped() {
        let items = vec![
            item(
                "FR-1",
                "S",
                RequirementKind::Functional,
                "Users must sign in with a password.",
            ),
            item(
                "FR-2",
                "S",
                RequirementKind::Functional,
                "Sessions must expire after thirty minutes.",
            ),
        ];
        let reply: Reply = Box::new(|_, _| {
            Ok(serde_json::json!({ "test_cases": [
                case_json("Quotes FR-2", "FR-404", "evidenced", Some("expire after thirty minutes")),
                case_json("Quotes nothing real", "FR-404", "inferred", None),
                case_json("Names FR-1", "FR-1", "inferred", None),
            ]})
            .to_string())
        });
        let (infra, _) = infra_with(reply);

        let result = run(
            &infra,
            &items,
            &[skill("f", "Functional Testing", "")],
            12_000,
        )
        .await;

        let by_title: HashMap<String, Option<String>> = cases_of(&result)
            .into_iter()
            .map(|c| (c.title, c.requirement_id))
            .collect();
        assert_eq!(by_title["Quotes FR-2"].as_deref(), Some("FR-2"));
        assert_eq!(by_title["Quotes nothing real"], None);
        assert_eq!(by_title["Names FR-1"].as_deref(), Some("FR-1"));
        assert_eq!(result.coverage.covered, 2);
    }

    #[tokio::test]
    async fn a_quote_that_is_not_in_the_batch_is_not_trusted_as_evidence() {
        let items = functional(1);
        let reply: Reply = Box::new(|_, _| {
            Ok(serde_json::json!({ "test_cases": [
                case_json("Fake quote", "R1", "evidenced", Some("a sentence nobody wrote")),
                case_json("Real quote", "R1", "evidenced", Some("scenario number 1")),
            ]})
            .to_string())
        });
        let (infra, _) = infra_with(reply);

        let result = run(
            &infra,
            &items,
            &[skill("f", "Functional Testing", "")],
            12_000,
        )
        .await;

        let cases = cases_of(&result);
        let fake = cases.iter().find(|c| c.title == "Fake quote").unwrap();
        assert_eq!(fake.evidence, crate::ai_generation::EvidenceKind::Inferred);
        assert_eq!(fake.source_reference, None);
        let real = cases.iter().find(|c| c.title == "Real quote").unwrap();
        assert_eq!(real.evidence, crate::ai_generation::EvidenceKind::Evidenced);
        assert_eq!(real.source_reference.as_deref(), Some("scenario number 1"));
    }

    #[tokio::test]
    async fn one_failing_call_leaves_its_requirements_uncovered_and_the_others_intact() {
        // The second call is refused by the provider; the rest answer.
        let reply: Reply = Box::new(|n, prompt| {
            if n == 1 {
                return Err(ExecutionError::ProviderRejected {
                    status: 500,
                    message: "the provider is broken".to_string(),
                });
            }
            echo()(n, prompt)
        });
        let (infra, executor) = infra_with(reply);
        let items = functional(9);

        let result = run(&infra, &items, &[skill("f", "Functional Testing", "")], 500).await;

        let prompts = executor.prompts.lock().unwrap().clone();
        let failed_ids = ids_in(&prompts[1]);
        let waves = waves_of(&result);
        assert_eq!(
            waves.iter().filter(|w| w.failure_reason.is_some()).count(),
            1
        );
        assert!(waves[1]
            .failure_reason
            .as_deref()
            .unwrap()
            .contains("the provider is broken"));
        assert_eq!(waves[1].requirements, Some(failed_ids.len()));
        assert_eq!(result.coverage.uncovered_keys, failed_ids);
        assert_eq!(result.coverage.covered, 9 - failed_ids.len());
    }

    #[tokio::test]
    async fn when_every_call_fails_the_run_fails_and_each_reason_is_said_once() {
        let reply: Reply = Box::new(|_, _| {
            Err(ExecutionError::ProviderRejected {
                status: 500,
                message: "the provider is broken".to_string(),
            })
        });
        let (infra, _) = infra_with(reply);
        let items = functional(9);

        let result = run(&infra, &items, &[skill("f", "Functional Testing", "")], 500).await;

        match &result.outcome {
            GenerationOutcome::Failed { reason } => {
                assert_eq!(
                    reason.matches("the provider is broken").count(),
                    1,
                    "{reason}"
                );
            }
            GenerationOutcome::Ai { .. } => panic!("no call answered"),
        }
        assert_eq!(result.coverage.covered, 0);
        assert_eq!(result.coverage.uncovered_keys.len(), 9);
    }

    #[tokio::test]
    async fn nothing_of_a_tested_kind_fails_at_once_without_asking_a_model() {
        let (infra, executor) = infra_with(echo());
        let items = functional(3);

        let result = run(
            &infra,
            &items,
            &[skill("p", "Performance Testing", "")],
            12_000,
        )
        .await;

        assert!(
            executor.prompts.lock().unwrap().is_empty(),
            "no model may be called"
        );
        match &result.outcome {
            GenerationOutcome::Failed { reason } => assert!(
                reason.contains("none of the selected documents"),
                "{reason}"
            ),
            GenerationOutcome::Ai { .. } => panic!("nothing was generated"),
        }
        assert_eq!(result.coverage.not_offered_keys, ["R1", "R2", "R3"]);
        assert_eq!(result.coverage.calls, 0);
    }

    #[tokio::test]
    async fn a_call_is_told_only_about_earlier_cases_of_its_own_requirements() {
        let (infra, executor) = infra_with(echo());
        let items = functional(8);

        // Two skills, several batches each: the second skill's first batch must hear about the first skill's
        // cases for R1.. only, never the ones for requirements it is not shown in that call.
        run(
            &infra,
            &items,
            &[
                skill("f", "Functional Testing", ""),
                skill("n", "Negative Testing", ""),
            ],
            500,
        )
        .await;

        let prompts = executor.prompts.lock().unwrap().clone();
        let per_skill = prompts.len() / 2;
        assert!(per_skill >= 2);
        // The first call of the second skill lists R-ids of batch 1 only.
        let second_skill_first = &prompts[per_skill];
        let ids = ids_in(second_skill_first);
        assert!(
            second_skill_first.contains("ALREADY-GENERATED COVERAGE"),
            "{second_skill_first}"
        );
        for id in &ids {
            assert!(
                second_skill_first.contains(&format!("\"Check {id}\"")),
                "own requirement's earlier case missing: {id}"
            );
        }
        let elsewhere = (1..=8)
            .map(|i| format!("R{i}"))
            .find(|id| !ids.contains(id))
            .unwrap();
        assert!(
            !second_skill_first.contains(&format!("\"Check {elsewhere}\"")),
            "{elsewhere} belongs to another batch and must not be listed"
        );
        // The very first call has nothing before it.
        assert!(!prompts[0].contains("ALREADY-GENERATED COVERAGE"));
    }

    #[tokio::test]
    async fn the_calls_run_in_wave_order_whatever_the_order_the_skills_were_selected_in() {
        let (infra, executor) = infra_with(echo());
        let items = mixed();

        let result = run(
            &infra,
            &items,
            &[
                skill("s", "Security Testing", ""),
                skill("f", "Functional Testing", ""),
                skill("p", "Spike Testing Of Nothing", ""),
            ],
            12_000,
        )
        .await;

        let waves = waves_of(&result);
        let order: Vec<(&str, String)> = waves
            .iter()
            .map(|w| (w.wave, w.skill_names.join(",")))
            .collect();
        assert_eq!(
            order,
            [
                ("base", "Functional Testing".to_string()),
                ("base", "Spike Testing Of Nothing".to_string()),
                ("non_functional", "Security Testing".to_string())
            ]
        );
        assert_eq!(executor.prompts.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_skill_with_no_requirement_of_its_kind_is_named_in_the_notes() {
        let (infra, _) = infra_with(echo());
        let result = run(
            &infra,
            &functional(3),
            &[
                skill("f", "Functional Testing", ""),
                skill("s", "Security Testing", ""),
            ],
            12_000,
        )
        .await;
        assert_eq!(waves_of(&result).len(), 1);
        assert!(
            result
                .coverage
                .notes
                .iter()
                .any(|n| n.starts_with("Security Testing: ")),
            "{:?}",
            result.coverage.notes
        );
    }

    #[tokio::test]
    async fn an_unusable_answer_is_retried_once_like_every_other_generation_path() {
        let reply: Reply = Box::new(|n, prompt| {
            if n == 0 {
                Ok("{}".to_string())
            } else {
                echo()(n, prompt)
            }
        });
        let (infra, executor) = infra_with(reply);

        let result = run(
            &infra,
            &functional(2),
            &[skill("f", "Functional Testing", "")],
            12_000,
        )
        .await;

        assert_eq!(
            executor.prompts.lock().unwrap().len(),
            2,
            "one bad answer, one retry"
        );
        assert_eq!(cases_of(&result).len(), 2);
    }

    #[tokio::test]
    async fn the_users_settings_text_is_in_every_prompt() {
        let (infra, executor) = infra_with(echo());
        run(
            &infra,
            &functional(6),
            &[skill("f", "Functional Testing", "")],
            400,
        )
        .await;
        let prompts = executor.prompts.lock().unwrap().clone();
        assert!(prompts.len() > 1);
        assert!(prompts
            .iter()
            .all(|p| p.contains("Requested generation depth: Standard.")));
    }
}
