//! Phase 2.1 — Test Case Compiler / Execution Planner.
//!
//! Fresh audit before writing this module: Phase 2 (`api.rs::
//! handoff_test_case_scoped`) already let a caller name a real `executor`
//! at handoff time, but sent no per-case data at all — every real executor
//! ran its own fixed default reference fixture (`executors/playwright/
//! tests/sample.spec.ts`, `executors/pytest/test_health.py`, ...), never
//! anything derived from the actual generated test case's own `steps`/
//! `expected_result`. This module is what makes the case's own real content
//! materially determine what actually executes.
//!
//! ```text
//! TestCaseRow (real, persisted)
//!         v
//! compile()  -- LEVEL 1: deterministic pattern compiler (compile_deterministic)
//!            -- LEVEL 3: AI-assisted compiler, only if Level 1 found nothing
//!               (compile_with_ai — reuses the EXISTING central
//!               ai_router::AiInfrastructure, no new router)
//!         v
//! CompilationOutcome::Ready { plan, .. } | NotExecutable | CompilationFailed | CompilationUnavailable
//!         v
//! validate_plan()  -- structural safety + SSRF guard (reuses
//!                      executor::validate_target_url, does not reinvent it)
//!         v
//! materialize_task_def()  -- turns a validated plan into a real, dispatchable
//!                             TaskDef for the EXISTING engine/scheduler/
//!                             executor pipeline (api.rs::handoff_test_case_scoped,
//!                             unchanged beyond wiring this in)
//! ```
//!
//! Security posture (brief §12), enforced structurally, not by convention:
//! `Action` is a closed, internally-tagged enum — there is no "run this
//! shell command" variant, so neither the deterministic compiler nor the AI
//! compiler can ever produce one; an AI response naming an action type
//! outside this vocabulary fails `serde_json` deserialization and becomes a
//! real `CompilationFailed`, never a best-effort guess. Every target URL
//! (deterministic or AI-produced) passes through the exact same
//! `executor::validate_target_url` SSRF guard real executors already use —
//! no second, weaker check was invented.

use crate::ai_router::{AiInfrastructure, AiRequest, AiTask, GenerationOutcome2};
use crate::domain::TaskDef;
use crate::store::TestCaseRow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// This dev environment has no separate product-under-test (confirmed
/// repeatedly across this repository's own prior sessions/reports) — the
/// orchestrator's own address is the one real, always-reachable default,
/// already used identically by `PytestExecutor`/`SchemathesisExecutor`
/// (`executor.rs`). A relative API path in a compiled step resolves against
/// this same, pre-existing convention rather than a newly-invented one.
pub const DEFAULT_TARGET: &str = "http://127.0.0.1:8080";

// ----------------------------------------------------------------- secrets

/// A `FILL` value written `env:NAME` is not text to type but a reference: the text is read from the environment
/// variable `NAME` of the orchestrator when the test runs. A plan, the spec generated from it, the parameters a run
/// stores and every log carry only the name; the value exists in the orchestrator's environment and in the browser
/// that types it. (`crate::ai_router::env_key_name` is the same `env:NAME` convention, for provider keys.)
///
/// Only names starting with one of these may be referenced. Without that, a plan — which an AI writes — could ask for
/// `env:GROQ_API_KEY` to be typed into whatever page it is testing.
pub const SECRET_NAME_PREFIXES: [&str; 2] = ["AIQA_LOGIN_", "AIQA_SECRET_"];

const MAX_SECRET_NAME_LEN: usize = 64;

/// Whether a plan may reference the variable `name`: letters, digits and `_` (not starting with a digit), at most 64
/// characters, and starting with one of `SECRET_NAME_PREFIXES`.
pub fn is_valid_secret_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= MAX_SECRET_NAME_LEN
        && SECRET_NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// The variable a `FILL` value refers to, when the value is a well-formed `env:NAME` reference.
pub fn secret_name(value: &str) -> Option<&str> {
    crate::ai_router::env_key_name(value).filter(|name| is_valid_secret_name(name))
}

/// What is wrong with a `FILL` value that starts `env:`, or `None` when it is an ordinary value or a good
/// reference. The message never repeats the value: it may have been a password typed by mistake.
pub fn secret_reference_problem(value: &str) -> Option<String> {
    let name = crate::ai_router::env_key_name(value)?;
    if is_valid_secret_name(name) {
        None
    } else {
        Some(format!(
            "a FILL value starting env: must be a secret reference, env:NAME, where NAME starts with {} (letters, digits and _)",
            SECRET_NAME_PREFIXES.join(" or ")
        ))
    }
}

/// The variables `actions` refer to, each once, in name order. Names only — never a value.
pub fn secret_names_of(actions: &[Action]) -> Vec<String> {
    let mut names: Vec<String> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Fill { value, .. } => secret_name(value).map(str::to_string),
            _ => None,
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The variables among `names` that are not set (or hold only blanks) in the orchestrator's environment.
pub fn missing_secrets(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| !crate::ai_router::env_key_is_set(name))
        .cloned()
        .collect()
}

/// The message for a run that cannot start because `missing` are not set. Names only.
pub fn missing_secrets_message(missing: &[String]) -> String {
    format!(
        "the secret{} {} {} not set in the orchestrator's environment — add {} to its .env (or environment) and restart it",
        if missing.len() == 1 { "" } else { "s" },
        missing.join(", "),
        if missing.len() == 1 { "is" } else { "are" },
        if missing.len() == 1 { "it" } else { "them" },
    )
}

// ------------------------------------------------------------------ schema

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionType {
    Api,
    Browser,
    /// Declared for schema completeness (brief §5's own conceptual list)
    /// but never produced as `Ready` by this module — see
    /// `validate_plan`. No dynamic-Python-file materialization exists in
    /// this phase; see the Phase 2.1 report §18/§35 for why.
    Python,
}

/// Closed action vocabulary (brief §5) — deliberately has no generic
/// "run this shell command" or "eval this code" variant. Internally
/// tagged (`type`), so an unrecognized action name is a real
/// deserialization error, not silently ignored or executed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Action {
    // ---- Browser
    Navigate {
        url: String,
    },
    Click {
        selector: String,
    },
    Fill {
        selector: String,
        value: String,
    },
    AssertVisible {
        selector: String,
    },
    AssertText {
        selector: String,
        text: String,
    },
    AssertUrl {
        url_contains: String,
    },
    // ---- API
    Request {
        method: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        body: Option<serde_json::Value>,
    },
    AssertStatus {
        status: u16,
    },
    AssertHeader {
        name: String,
        value: String,
    },
    AssertBodyField {
        path: String,
        equals: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompilerMethod {
    Deterministic,
    Ai,
    /// Phase 4 (API Testing page) — no compilation happened at all: the
    /// user explicitly authored every action (method/URL/headers/body/
    /// assertions) via the request builder's own real fields. Distinct
    /// from `Deterministic` (which implies parsing/inferring a plan from
    /// natural-language test-case text) so this plan's own truthfulness
    /// disclosure (`ExecutionPlan.compiler`) never overstates what
    /// actually produced it.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub test_case_id: String,
    pub execution_type: ExecutionType,
    pub target: Option<String>,
    pub actions: Vec<Action>,
    pub compiler: CompilerMethod,
    /// Human-readable notes about what was and wasn't compiled — never
    /// empty for an AI-produced plan (brief §7's own "diagnostics" field);
    /// may be empty for an unambiguous deterministic plan.
    #[serde(default)]
    pub diagnostics: Vec<String>,
    /// Real provider/model that produced this plan, only when
    /// `compiler == Ai` — mirrors the same truthfulness rule Phase 1's
    /// `GenerateAiTestCasesResponse` already established (never fabricated,
    /// `None` for a deterministic plan).
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// Every truthful outcome `compile()` can produce — mirrors brief §7's own
/// vocabulary exactly (`READY` maps to `Ready`, everything else keeps its
/// own distinct, non-collapsed reason).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompilationOutcome {
    Ready(ExecutionPlan),
    NotExecutable { reasons: Vec<String> },
    CompilationFailed { reasons: Vec<String> },
    CompilationUnavailable { reason: String },
}

// ------------------------------------------------------- LEVEL 1: deterministic

/// Finds the first step (or case-level `assertions`/`expected_result`)
/// whose *action* text unambiguously names an HTTP method and a path/URL —
/// the one pattern this compiler will resolve without AI, because it is
/// genuinely unambiguous (brief §6 LEVEL 1's own example). Everything else
/// (a bare "Submit an authenticated request...", a vague UI description)
/// deliberately falls through to LEVEL 3 rather than being guessed here.
fn compile_deterministic_api(case: &TestCaseRow) -> Option<ExecutionPlan> {
    let method_re =
        regex::Regex::new(r"(?i)\b(GET|POST|PUT|DELETE|PATCH)\s+(/\S+|https?://\S+)").ok()?;

    let mut chosen: Option<(String, String)> = None;
    for step in &case.steps {
        if let Some(caps) = method_re.captures(&step.action) {
            chosen = Some((caps[1].to_uppercase(), caps[2].to_string()));
            break;
        }
    }
    let (method, raw_target) = chosen?;

    let url = if raw_target.starts_with("http://") || raw_target.starts_with("https://") {
        raw_target.clone()
    } else {
        format!("{DEFAULT_TARGET}{raw_target}")
    };

    // Status assertion: scan every real text field this case actually has —
    // never invented if none is present.
    let status_re =
        regex::Regex::new(r"(?i)HTTP\s+(\d{3})|status(?:\s+code)?\s*(?:is|=|of)?\s*(\d{3})")
            .ok()?;
    let mut statuses: Vec<u16> = Vec::new();
    let mut scan = |text: &str| {
        for caps in status_re.captures_iter(text) {
            let code = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
            if let Ok(n) = code.parse::<u16>() {
                if !statuses.contains(&n) {
                    statuses.push(n);
                }
            }
        }
    };
    for step in &case.steps {
        scan(&step.expected_result);
    }
    scan(&case.expected_result);
    for a in &case.assertions {
        scan(a);
    }

    if statuses.is_empty() {
        // A request with no real, stated assertion is not a complete plan —
        // do not guess "200" just because that's the common case.
        return None;
    }

    let mut actions = vec![Action::Request {
        method,
        url: url.clone(),
        headers: BTreeMap::new(),
        body: None,
    }];
    for s in statuses {
        actions.push(Action::AssertStatus { status: s });
    }

    Some(ExecutionPlan {
        test_case_id: case.id.to_string(),
        execution_type: ExecutionType::Api,
        target: Some(url),
        actions,
        compiler: CompilerMethod::Deterministic,
        diagnostics: vec![],
        provider: None,
        model: None,
    })
}

/// The other unambiguous LEVEL 1 pattern: "Navigate to <absolute URL>" (or
/// "Open"/"Go to"). The one assertion this produces (`AssertUrl`) is
/// directly derived from the same URL just navigated to — not invented
/// content, just confirming navigation actually landed there.
fn compile_deterministic_browser(case: &TestCaseRow) -> Option<ExecutionPlan> {
    let nav_re = regex::Regex::new(r"(?i)\b(?:navigate to|open|go to)\s+(https?://\S+)").ok()?;

    let mut url: Option<String> = None;
    for step in &case.steps {
        if let Some(caps) = nav_re.captures(&step.action) {
            url = Some(caps[1].trim_end_matches(['.', ',']).to_string());
            break;
        }
    }
    let url = url?;

    let host = reqwest::Url::parse(&url).ok()?.host_str()?.to_string();

    Some(ExecutionPlan {
        test_case_id: case.id.to_string(),
        execution_type: ExecutionType::Browser,
        target: Some(url.clone()),
        actions: vec![
            Action::Navigate { url },
            Action::AssertUrl { url_contains: host },
        ],
        compiler: CompilerMethod::Deterministic,
        diagnostics: vec![],
        provider: None,
        model: None,
    })
}

fn compile_deterministic(case: &TestCaseRow) -> Option<ExecutionPlan> {
    compile_deterministic_api(case).or_else(|| compile_deterministic_browser(case))
}

// ------------------------------------------------------- LEVEL 3: AI-assisted

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AiPlanResponse {
    Executable {
        execution_type: ExecutionType,
        #[serde(default)]
        target: Option<String>,
        actions: Vec<Action>,
    },
    NotExecutable {
        reasons: Vec<String>,
    },
}

fn build_compiler_prompt(case: &TestCaseRow) -> String {
    let steps_text: String = case
        .steps
        .iter()
        .map(|s| {
            format!(
                "- action: \"{}\" | expected_result: \"{}\"\n",
                s.action, s.expected_result
            )
        })
        .collect();
    format!(
        "You are a QA test case compiler. Given a real test case's title, category, steps, and expected \
         result, produce a structured, SAFE execution plan — or truthfully state one cannot be produced.\n\n\
         TEST CASE:\nTitle: {title}\nCategory: {category}\nSteps:\n{steps_text}Case-level expected result: \
         \"{expected_result}\"\nAssertions: {assertions:?}\n\n\
         Only use actions from this exact closed vocabulary — nothing else exists, do not invent a new one:\n\
         Browser: NAVIGATE{{url}}, CLICK{{selector}}, FILL{{selector,value}}, ASSERT_VISIBLE{{selector}}, \
         ASSERT_TEXT{{selector,text}}, ASSERT_URL{{url_contains}}\n\
         API: REQUEST{{method,url,headers?,body?}}, ASSERT_STATUS{{status}}, ASSERT_HEADER{{name,value}}, \
         ASSERT_BODY_FIELD{{path,equals}}\n\n\
         RULES (critical, do not violate):\n\
         - Only compile what the text actually, unambiguously states. Never invent a URL, selector, or value \
           that is not stated or directly implied by the text.\n\
         - A relative API path (e.g. \"/health\") is allowed as \"url\" — it will be resolved against a safe, \
           pre-configured default target; never invent an absolute external hostname that is not in the text.\n\
         - Never propose a non-http/https scheme, and never propose a private/internal/link-local address.\n\
         - If the case does not contain enough real information for a complete, unambiguous plan, do not guess: \
           return {{\"executable\": false, \"reasons\": [\"...\"]}} instead.\n\n\
         Return ONLY JSON, no markdown, no commentary, matching exactly one of:\n\
         {{\"executable\": true, \"execution_type\": \"API\"|\"BROWSER\", \"target\": string|null, \"actions\": [...]}}\n\
         {{\"executable\": false, \"reasons\": [string, ...]}}",
        title = case.title,
        category = case.category,
        expected_result = case.expected_result,
        assertions = case.assertions,
    )
}

/// Resolves a relative path against the same safe, pre-configured default
/// target `compile_deterministic_api` already uses. `build_compiler_prompt`
/// explicitly tells the AI a relative API path (e.g. `/health`) is allowed
/// and "will be resolved against a safe, pre-configured default target" —
/// this is that resolution, applied identically to an AI-produced plan.
fn resolve_relative_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{DEFAULT_TARGET}{url}")
    }
}

async fn compile_with_ai(infra: &AiInfrastructure, case: &TestCaseRow) -> CompilationOutcome {
    let request = AiRequest {
        task: AiTask::TestCaseCompilation,
        prompt: build_compiler_prompt(case),
        preferred_provider: None,
        preferred_model: None,
    };
    let result = match infra.generate(request).await {
        GenerationOutcome2::Ok(r) => r,
        // A real, honest routing/provider failure (e.g. NoCompatibleModel)
        // is reported as truthfully "unavailable" here — never as a
        // fabricated plan.
        GenerationOutcome2::Failed { reason } => {
            return CompilationOutcome::CompilationUnavailable { reason }
        }
    };

    let parsed: AiPlanResponse = match serde_json::from_str(&result.text) {
        Ok(v) => v,
        Err(e) => {
            return CompilationOutcome::CompilationFailed {
                reasons: vec![format!(
                    "AI response did not match the required structured schema: {e}"
                )],
            }
        }
    };

    match parsed {
        AiPlanResponse::NotExecutable { reasons } => CompilationOutcome::NotExecutable { reasons },
        AiPlanResponse::Executable {
            execution_type,
            target,
            actions,
        } => {
            let actions: Vec<Action> = actions
                .into_iter()
                .map(|a| match a {
                    Action::Navigate { url } => Action::Navigate {
                        url: resolve_relative_url(&url),
                    },
                    Action::Request {
                        method,
                        url,
                        headers,
                        body,
                    } => Action::Request {
                        method,
                        url: resolve_relative_url(&url),
                        headers,
                        body,
                    },
                    other => other,
                })
                .collect();
            let target = target.map(|t| resolve_relative_url(&t));
            // A secret reference is something only a login plan may carry. An AI-written plan naming one — from
            // a prompt injection in the case text, say — would have the orchestrator type a variable's value into
            // the page under test.
            if !secret_names_of(&actions).is_empty() {
                return CompilationOutcome::CompilationFailed {
                    reasons: vec![
                        "the AI plan referred to a secret (env:NAME); only a login plan may"
                            .to_string(),
                    ],
                };
            }
            let plan = ExecutionPlan {
                test_case_id: case.id.to_string(),
                execution_type,
                target,
                actions,
                compiler: CompilerMethod::Ai,
                diagnostics: vec![format!("compiled by {}/{}", result.provider, result.model)],
                provider: Some(result.provider),
                model: Some(result.model),
            };
            match validate_plan(&plan).await {
                Ok(()) => CompilationOutcome::Ready(plan),
                Err(reasons) => CompilationOutcome::CompilationFailed { reasons },
            }
        }
    }
}

// --------------------------------------------------------------- validation

/// Structural + SSRF safety validation (brief §8/§12). Reuses
/// `executor::validate_target_url` — the exact same guard real executors
/// already apply to `pytest`'s `base_url`/`schemathesis`'s `url` — rather
/// than inventing a second, possibly-weaker target check for this new path.
pub async fn validate_plan(plan: &ExecutionPlan) -> Result<(), Vec<String>> {
    let mut reasons = Vec::new();

    if plan.actions.is_empty() {
        reasons.push("plan has no actions".to_string());
    }
    if matches!(plan.execution_type, ExecutionType::Python) {
        reasons.push(
            "PYTHON execution type is not implemented in this phase — see the Phase 2.1 report"
                .to_string(),
        );
    }

    let has_primary_action = plan
        .actions
        .iter()
        .any(|a| matches!(a, Action::Navigate { .. } | Action::Request { .. }));
    if !has_primary_action && !matches!(plan.execution_type, ExecutionType::Python) {
        reasons.push(
            "plan has no NAVIGATE or REQUEST action to actually interact with a target".to_string(),
        );
    }

    for action in &plan.actions {
        if let Action::Fill { value, .. } = action {
            if let Some(problem) = secret_reference_problem(value) {
                reasons.push(problem);
            }
        }
        let url = match action {
            Action::Navigate { url } => Some(url.as_str()),
            Action::Request { url, .. } => Some(url.as_str()),
            _ => None,
        };
        if let Some(url) = url {
            if let Err(reason) = crate::executor::validate_target_url(url).await {
                reasons.push(reason);
            }
        }
    }

    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons)
    }
}

// -------------------------------------------------------------- orchestration

/// The real, single entry point (brief §6): try LEVEL 1 (deterministic)
/// first; only if it finds nothing, fall through to LEVEL 3 (AI-assisted,
/// via the existing central router). LEVEL 2 ("existing structured data")
/// is not a separate code path — the deterministic compiler already
/// prefers whatever explicit method/path/URL text the case already states
/// over any inference, which is the same principle.
pub async fn compile(infra: &AiInfrastructure, case: &TestCaseRow) -> CompilationOutcome {
    if let Some(plan) = compile_deterministic(case) {
        return match validate_plan(&plan).await {
            Ok(()) => CompilationOutcome::Ready(plan),
            Err(reasons) => CompilationOutcome::CompilationFailed { reasons },
        };
    }

    compile_with_ai(infra, case).await
}

// --------------------------------------------------------- materialization

/// Turns a validated, `Ready` plan into a real, dispatchable `TaskDef` for
/// the existing, unmodified engine/scheduler/executor pipeline. This is the
/// one place a compiled plan actually becomes something the real backend
/// runs — everything downstream of the returned `TaskDef` is pre-existing,
/// unmodified code (`create_run_core` -> `engine::spawn_run` ->
/// `ExecutorRegistry::resolve` -> a real adapter).
pub fn materialize_task_def(
    task_id: String,
    task_name: String,
    plan: &ExecutionPlan,
) -> Result<TaskDef, String> {
    match plan.execution_type {
        ExecutionType::Api => Ok(TaskDef {
            id: task_id,
            name: task_name,
            prerequisites: Vec::new(),
            simulate: None,
            executor: Some("compiled-api".to_string()),
            params: serde_json::to_value(plan)
                .map_err(|e| format!("failed to serialize execution plan: {e}"))?,
        }),
        ExecutionType::Browser => {
            let spec_path = crate::executor::materialize_playwright_spec(&task_id, plan)?;
            let mut params = serde_json::json!({ "spec": spec_path });
            // The names of the secrets the spec reads — never their values — so the executor can refuse a run
            // whose secrets are not set and scrub them from the evidence (`PlaywrightExecutor`).
            let secrets = secret_names_of(&plan.actions);
            if !secrets.is_empty() {
                params["secrets"] = serde_json::json!(secrets);
            }
            Ok(TaskDef {
                id: task_id,
                name: task_name,
                prerequisites: Vec::new(),
                simulate: None,
                executor: Some("playwright".to_string()),
                params,
            })
        }
        ExecutionType::Python => {
            Err("PYTHON execution type is not implemented in this phase".to_string())
        }
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use crate::domain::{TestCaseStatus, TestStep};
    use uuid::Uuid;

    pub fn case(
        title: &str,
        category: &str,
        steps: Vec<(&str, &str)>,
        expected_result: &str,
        assertions: Vec<&str>,
    ) -> TestCaseRow {
        TestCaseRow {
            id: Uuid::new_v4(),
            project_id: "test-project".to_string(),
            title: title.to_string(),
            description: "test description".to_string(),
            category: category.to_string(),
            priority: "MEDIUM".to_string(),
            severity: "MINOR".to_string(),
            preconditions: vec![],
            steps: steps
                .into_iter()
                .enumerate()
                .map(|(i, (action, expected_result))| TestStep {
                    step_number: (i + 1) as i64,
                    action: action.to_string(),
                    expected_result: expected_result.to_string(),
                })
                .collect(),
            expected_result: expected_result.to_string(),
            assertions: assertions.into_iter().map(String::from).collect(),
            test_data: None,
            source_document: None,
            requirement_reference: None,
            source_requirement: None,
            traceability: None,
            risk_factor: None,
            tags: vec![],
            source: "generator".to_string(),
            status: TestCaseStatus::Approved,
            validation_notes: None,
            executed_run_id: None,
            executed_task_id: None,
            created_by: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            suite_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, CapabilityLevel};
    use std::sync::Arc;

    fn infra_with_ai(result: Result<&str, crate::ai_router::ExecutionError>) -> AiInfrastructure {
        let executor: Arc<dyn AiExecutor> = match result {
            Ok(text) => Arc::new(FakeExecutor::ok(text)),
            Err(e) => Arc::new(FakeExecutor::err(e)),
        };
        let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> =
            std::collections::HashMap::new();
        executors.insert("mock", executor);
        AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        )
    }

    fn no_ai_infra() -> AiInfrastructure {
        AiInfrastructure::new(vec![], std::collections::HashMap::new())
    }

    // --------------------------------------------------- LEVEL 1: deterministic

    #[test]
    fn deterministic_compiles_a_clear_api_step_with_a_stated_status_assertion() {
        let c = case(
            "Health check",
            "API",
            vec![("GET /health", "HTTP 200 with body ok")],
            "HTTP 200 ok",
            vec![],
        );
        let plan = compile_deterministic(&c).expect("must compile deterministically");
        assert_eq!(plan.execution_type, ExecutionType::Api);
        assert_eq!(plan.compiler, CompilerMethod::Deterministic);
        assert!(
            matches!(&plan.actions[0], Action::Request { method, url, .. } if method == "GET" && url == "http://127.0.0.1:8080/health")
        );
        assert!(plan
            .actions
            .iter()
            .any(|a| matches!(a, Action::AssertStatus { status: 200 })));
    }

    #[test]
    fn deterministic_compiles_a_clear_navigate_step() {
        let c = case(
            "Homepage loads",
            "Functional",
            vec![(
                "Navigate to https://example.com/login",
                "Login form is shown",
            )],
            "Login form visible",
            vec![],
        );
        let plan = compile_deterministic(&c).expect("must compile deterministically");
        assert_eq!(plan.execution_type, ExecutionType::Browser);
        assert!(
            matches!(&plan.actions[0], Action::Navigate { url } if url == "https://example.com/login")
        );
        assert!(plan.actions.iter().any(
            |a| matches!(a, Action::AssertUrl { url_contains } if url_contains == "example.com")
        ));
    }

    #[test]
    fn ambiguous_prose_is_not_guessed_by_the_deterministic_compiler() {
        let c = case(
            "Vague case",
            "API",
            vec![(
                "Submit an authenticated request using the expired token",
                "Request is rejected",
            )],
            "Request rejected",
            vec![],
        );
        assert!(
            compile_deterministic(&c).is_none(),
            "must not invent a method/URL/status from vague prose"
        );
    }

    #[test]
    fn an_api_step_with_no_stated_status_assertion_anywhere_is_not_guessed() {
        let c = case(
            "No assertion stated",
            "API",
            vec![("GET /health", "It works")],
            "It works",
            vec![],
        );
        assert!(
            compile_deterministic(&c).is_none(),
            "must not invent a 200 just because that's the common case"
        );
    }

    // ------------------------------------------------------------- validation

    #[tokio::test]
    async fn validate_plan_rejects_an_ssrf_target_reusing_the_real_existing_guard() {
        let plan = ExecutionPlan {
            test_case_id: "t".into(),
            execution_type: ExecutionType::Api,
            target: Some("http://169.254.169.254/".into()),
            actions: vec![
                Action::Request {
                    method: "GET".into(),
                    url: "http://169.254.169.254/latest/meta-data/".into(),
                    headers: BTreeMap::new(),
                    body: None,
                },
                Action::AssertStatus { status: 200 },
            ],
            compiler: CompilerMethod::Deterministic,
            diagnostics: vec![],
            provider: None,
            model: None,
        };
        let err = validate_plan(&plan)
            .await
            .expect_err("cloud metadata address must be rejected");
        assert!(
            err.iter().any(|r| r.contains("disallowed")),
            "reasons={err:?}"
        );
    }

    #[tokio::test]
    async fn validate_plan_rejects_python_execution_type() {
        let plan = ExecutionPlan {
            test_case_id: "t".into(),
            execution_type: ExecutionType::Python,
            target: None,
            actions: vec![Action::AssertStatus { status: 200 }],
            compiler: CompilerMethod::Ai,
            diagnostics: vec![],
            provider: None,
            model: None,
        };
        let err = validate_plan(&plan)
            .await
            .expect_err("PYTHON must be rejected — not implemented this phase");
        assert!(err.iter().any(|r| r.contains("PYTHON")), "reasons={err:?}");
    }

    #[tokio::test]
    async fn validate_plan_accepts_a_real_well_formed_api_plan_targeting_the_documented_default() {
        let plan = ExecutionPlan {
            test_case_id: "t".into(),
            execution_type: ExecutionType::Api,
            target: Some(format!("{DEFAULT_TARGET}/health")),
            actions: vec![
                Action::Request {
                    method: "GET".into(),
                    url: format!("{DEFAULT_TARGET}/health"),
                    headers: BTreeMap::new(),
                    body: None,
                },
                Action::AssertStatus { status: 200 },
            ],
            compiler: CompilerMethod::Deterministic,
            diagnostics: vec![],
            provider: None,
            model: None,
        };
        assert!(validate_plan(&plan).await.is_ok());
    }

    // ------------------------------------------------------- LEVEL 3: AI-assisted

    #[tokio::test]
    async fn ai_compilation_is_used_only_when_deterministic_compilation_finds_nothing() {
        let c = case(
            "Vague case",
            "API",
            vec![(
                "Submit an authenticated request using the expired token",
                "Request is rejected",
            )],
            "Request rejected",
            vec![],
        );
        let ai_json = r#"{"executable":true,"execution_type":"API","target":null,"actions":[{"type":"REQUEST","method":"GET","url":"/health"},{"type":"ASSERT_STATUS","status":401}]}"#;
        let infra = infra_with_ai(Ok(ai_json));
        let outcome = compile(&infra, &c).await;
        match outcome {
            CompilationOutcome::Ready(plan) => {
                assert_eq!(plan.compiler, CompilerMethod::Ai);
                assert_eq!(plan.provider.as_deref(), Some("mock"));
            }
            other => panic!("expected Ready via AI compilation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn deterministic_compilation_is_preferred_and_never_calls_the_ai_router_when_it_already_succeeded(
    ) {
        let c = case(
            "Health check",
            "API",
            vec![("GET /health", "HTTP 200 with body ok")],
            "HTTP 200 ok",
            vec![],
        );
        // An AI infra with zero providers would fail loudly if actually
        // invoked (NoCompatibleModel) — using it here and still getting a
        // successful Ready result proves the AI path was never reached.
        let infra = no_ai_infra();
        let outcome = compile(&infra, &c).await;
        assert!(
            matches!(outcome, CompilationOutcome::Ready(ref plan) if plan.compiler == CompilerMethod::Deterministic),
            "outcome={outcome:?}"
        );
    }

    #[tokio::test]
    async fn ai_compilation_unavailable_is_reported_truthfully_not_faked() {
        let c = case(
            "Vague case",
            "UI",
            vec![("Do the thing on the page", "It works")],
            "It works",
            vec![],
        );
        let infra = no_ai_infra();
        let outcome = compile(&infra, &c).await;
        assert!(
            matches!(outcome, CompilationOutcome::CompilationUnavailable { .. }),
            "outcome={outcome:?}"
        );
    }

    #[tokio::test]
    async fn ai_response_declining_to_compile_is_reported_as_not_executable() {
        let c = case(
            "Vague case",
            "UI",
            vec![("Do the thing", "It works")],
            "It works",
            vec![],
        );
        let ai_json = r#"{"executable":false,"reasons":["no target URL or selector is stated anywhere in this case"]}"#;
        let infra = infra_with_ai(Ok(ai_json));
        let outcome = compile(&infra, &c).await;
        match outcome {
            CompilationOutcome::NotExecutable { reasons } => {
                assert!(reasons[0].contains("no target URL"))
            }
            other => panic!("expected NotExecutable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_ai_json_is_a_real_compilation_failure_not_a_crash_or_a_guess() {
        let c = case(
            "Vague case",
            "UI",
            vec![("Do the thing", "It works")],
            "It works",
            vec![],
        );
        let infra = infra_with_ai(Ok("not json at all"));
        let outcome = compile(&infra, &c).await;
        assert!(
            matches!(outcome, CompilationOutcome::CompilationFailed { .. }),
            "outcome={outcome:?}"
        );
    }

    #[tokio::test]
    async fn an_ai_response_naming_an_action_type_outside_the_closed_vocabulary_is_rejected_structurally(
    ) {
        let c = case(
            "Vague case",
            "UI",
            vec![("Do the thing", "It works")],
            "It works",
            vec![],
        );
        // "RUN_SHELL" is not, and will never be, a real Action variant —
        // this must fail to deserialize, proving the AI cannot smuggle an
        // arbitrary-code action through even if it tried.
        let ai_json = r#"{"executable":true,"execution_type":"API","target":null,"actions":[{"type":"RUN_SHELL","command":"rm -rf /"}]}"#;
        let infra = infra_with_ai(Ok(ai_json));
        let outcome = compile(&infra, &c).await;
        assert!(
            matches!(outcome, CompilationOutcome::CompilationFailed { .. }),
            "outcome={outcome:?}"
        );
    }

    #[tokio::test]
    async fn an_ai_response_proposing_an_ssrf_target_is_rejected_by_the_same_real_guard() {
        let c = case(
            "Vague case",
            "API",
            vec![("Call the internal service", "It works")],
            "It works",
            vec![],
        );
        let ai_json = r#"{"executable":true,"execution_type":"API","target":"http://169.254.169.254/","actions":[{"type":"REQUEST","method":"GET","url":"http://169.254.169.254/latest/meta-data/"},{"type":"ASSERT_STATUS","status":200}]}"#;
        let infra = infra_with_ai(Ok(ai_json));
        let outcome = compile(&infra, &c).await;
        match outcome {
            CompilationOutcome::CompilationFailed { reasons } => {
                assert!(reasons.iter().any(|r| r.contains("disallowed")))
            }
            other => panic!("expected CompilationFailed (SSRF), got {other:?}"),
        }
    }

    // ----------------------------------------------------------------- secrets

    #[test]
    fn a_secret_reference_names_a_prefixed_variable_and_nothing_else_is_one() {
        assert_eq!(
            secret_name("env:AIQA_LOGIN_ADMIN_PASSWORD"),
            Some("AIQA_LOGIN_ADMIN_PASSWORD")
        );
        assert_eq!(
            secret_name("env:AIQA_SECRET_TOKEN"),
            Some("AIQA_SECRET_TOKEN")
        );
        assert_eq!(
            secret_name("env: AIQA_LOGIN_X "),
            Some("AIQA_LOGIN_X"),
            "surrounding blanks are ignored, as for provider keys"
        );
        // Not a reference: a plain value.
        assert_eq!(secret_name("hunter2"), None);
        assert_eq!(
            secret_name("ENV:AIQA_LOGIN_X"),
            None,
            "the prefix is lower case, as for provider keys"
        );
        // A variable outside the reserved prefixes can never be typed into a page.
        assert_eq!(secret_name("env:GROQ_API_KEY"), None);
        assert_eq!(secret_name("env:PATH"), None);
        // Malformed names.
        assert_eq!(secret_name("env:"), None);
        assert_eq!(secret_name("env:AIQA_LOGIN_A B"), None);
        assert_eq!(secret_name("env:AIQA_LOGIN_A-B"), None);
        assert_eq!(
            secret_name(&format!("env:AIQA_LOGIN_{}", "X".repeat(60))),
            None,
            "longer than 64 characters"
        );
    }

    #[test]
    fn only_a_value_that_starts_env_and_is_not_a_good_reference_is_a_problem() {
        assert_eq!(secret_reference_problem("admin"), None);
        assert_eq!(
            secret_reference_problem("env:AIQA_LOGIN_ADMIN_PASSWORD"),
            None
        );
        let problem = secret_reference_problem("env:GROQ_API_KEY").expect("a problem");
        assert!(
            problem.contains("AIQA_LOGIN_ or AIQA_SECRET_"),
            "problem={problem}"
        );
        // The message never repeats what was written: it may have been a password.
        let problem = secret_reference_problem("env:hunter2-is-my-password").expect("a problem");
        assert!(!problem.contains("hunter2"), "problem={problem}");
    }

    #[test]
    fn a_plans_secret_names_are_listed_once_in_order_and_only_from_fill_values() {
        let actions = vec![
            Action::Fill {
                selector: "#u".into(),
                value: "env:AIQA_LOGIN_ADMIN_USERNAME".into(),
            },
            Action::Fill {
                selector: "#p".into(),
                value: "env:AIQA_LOGIN_ADMIN_PASSWORD".into(),
            },
            Action::Fill {
                selector: "#again".into(),
                value: "env:AIQA_LOGIN_ADMIN_PASSWORD".into(),
            },
            Action::Fill {
                selector: "#plain".into(),
                value: "just text".into(),
            },
            Action::AssertText {
                selector: "#t".into(),
                text: "env:AIQA_LOGIN_NOT_A_FILL".into(),
            },
        ];
        assert_eq!(
            secret_names_of(&actions),
            vec![
                "AIQA_LOGIN_ADMIN_PASSWORD".to_string(),
                "AIQA_LOGIN_ADMIN_USERNAME".to_string()
            ]
        );
    }

    #[test]
    fn a_secret_is_missing_when_its_variable_is_unset_or_blank_and_the_message_names_only_variables(
    ) {
        std::env::set_var("AIQA_LOGIN_T5A_SET", "value-1");
        std::env::set_var("AIQA_LOGIN_T5A_BLANK", "   ");
        std::env::remove_var("AIQA_LOGIN_T5A_UNSET");
        let names = vec![
            "AIQA_LOGIN_T5A_SET".to_string(),
            "AIQA_LOGIN_T5A_BLANK".to_string(),
            "AIQA_LOGIN_T5A_UNSET".to_string(),
        ];
        let missing = missing_secrets(&names);
        assert_eq!(
            missing,
            vec![
                "AIQA_LOGIN_T5A_BLANK".to_string(),
                "AIQA_LOGIN_T5A_UNSET".to_string()
            ]
        );
        let message = missing_secrets_message(&missing);
        assert!(
            message.contains("AIQA_LOGIN_T5A_BLANK, AIQA_LOGIN_T5A_UNSET are not set"),
            "message={message}"
        );
        assert!(!message.contains("value-1"));
        assert!(missing_secrets_message(&["AIQA_LOGIN_ONE".to_string()])
            .contains("secret AIQA_LOGIN_ONE is not set"));
    }

    #[tokio::test]
    async fn validate_plan_rejects_a_malformed_secret_reference_and_accepts_a_good_one() {
        let plan = |value: &str| ExecutionPlan {
            test_case_id: "t".into(),
            execution_type: ExecutionType::Browser,
            target: Some(DEFAULT_TARGET.into()),
            actions: vec![
                Action::Navigate {
                    url: format!("{DEFAULT_TARGET}/login"),
                },
                Action::Fill {
                    selector: "#password".into(),
                    value: value.into(),
                },
            ],
            compiler: CompilerMethod::Deterministic,
            diagnostics: vec![],
            provider: None,
            model: None,
        };
        assert!(validate_plan(&plan("env:AIQA_LOGIN_ADMIN_PASSWORD"))
            .await
            .is_ok());
        assert!(validate_plan(&plan("an ordinary value")).await.is_ok());
        let reasons = validate_plan(&plan("env:GEMINI_API_KEY"))
            .await
            .expect_err("outside the reserved prefixes");
        assert!(
            reasons.iter().any(|r| r.contains("secret reference")),
            "reasons={reasons:?}"
        );
    }

    #[test]
    fn a_browser_task_carries_the_names_of_its_secrets_and_never_a_value() {
        std::env::set_var("AIQA_LOGIN_T5A_TASK_PW", "task-pw-value");
        let plan = |value: &str| ExecutionPlan {
            test_case_id: "t".into(),
            execution_type: ExecutionType::Browser,
            target: None,
            actions: vec![
                Action::Navigate {
                    url: "https://example.com/login".into(),
                },
                Action::Fill {
                    selector: "#password".into(),
                    value: value.into(),
                },
            ],
            compiler: CompilerMethod::Deterministic,
            diagnostics: vec![],
            provider: None,
            model: None,
        };
        let task = materialize_task_def(
            "t5a-task-def".into(),
            "Login".into(),
            &plan("env:AIQA_LOGIN_T5A_TASK_PW"),
        )
        .expect("a task");
        assert_eq!(
            task.params["secrets"],
            serde_json::json!(["AIQA_LOGIN_T5A_TASK_PW"])
        );
        assert!(!task.params.to_string().contains("task-pw-value"));
        // A plan that refers to nothing has no `secrets` key at all.
        let plain =
            materialize_task_def("t5a-task-def-plain".into(), "Plain".into(), &plan("text"))
                .expect("a task");
        assert!(plain.params.get("secrets").is_none(), "{}", plain.params);
    }

    #[tokio::test]
    async fn an_ai_plan_that_names_a_secret_is_refused_because_only_a_login_plan_may() {
        let c = case(
            "Vague",
            "UI",
            vec![("Log in somehow", "Logged in")],
            "Logged in",
            vec![],
        );
        let ai_json = format!(
            r##"{{"executable":true,"execution_type":"BROWSER","target":"{DEFAULT_TARGET}/login","actions":[{{"type":"NAVIGATE","url":"{DEFAULT_TARGET}/login"}},{{"type":"FILL","selector":"#password","value":"env:AIQA_LOGIN_ADMIN_PASSWORD"}}]}}"##
        );
        let infra = infra_with_ai(Ok(ai_json.as_str()));
        match compile(&infra, &c).await {
            CompilationOutcome::CompilationFailed { reasons } => assert!(
                reasons.iter().any(|r| r.contains("only a login plan may")),
                "reasons={reasons:?}"
            ),
            other => panic!("expected CompilationFailed, got {other:?}"),
        }
    }
}
