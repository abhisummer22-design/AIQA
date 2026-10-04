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
use crate::grounding::{Grounding, GroundingReport};
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

/// What is wrong with a request header value that starts `env:`, or `None` when it is an ordinary value or a good reference
/// (`env:NAME`, read from the orchestrator's environment when the request is made). Never repeats the value.
pub fn header_reference_problem(name: &str, value: &str) -> Option<String> {
    let variable = crate::ai_router::env_key_name(value)?;
    if is_valid_secret_name(variable) {
        None
    } else {
        Some(format!(
            "the value of the header {name} starts env: so it must be a secret reference, env:NAME, where NAME starts with {} (letters, digits and _)",
            SECRET_NAME_PREFIXES.join(" or ")
        ))
    }
}

/// The variables `actions` refer to (in a `FILL` value or a request header), each once, in name order. Names only — never a value.
pub fn secret_names_of(actions: &[Action]) -> Vec<String> {
    let mut names: Vec<String> = actions
        .iter()
        .flat_map(|action| match action {
            Action::Fill { value, .. } => secret_name(value)
                .map(str::to_string)
                .into_iter()
                .collect::<Vec<_>>(),
            Action::Request { headers, .. } => headers
                .values()
                .filter_map(|v| secret_name(v))
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
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
    /// Phase 8B — the status is one of these. (`ASSERT_STATUS` is one exact status; the cases derived from an OpenAPI document accept
    /// every success status the document lists, or every client error it documents.)
    AssertStatusIn {
        statuses: Vec<u16>,
    },
    /// Phase 8B — the JSON body fits this schema (`json_schema.rs`): its types, required properties, items, enums and bounds. What
    /// `ASSERT_BODY_FIELD` (one field equals one value) cannot say: that the response has the shape the specification promises.
    AssertBodySchema {
        schema: serde_json::Value,
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
        /// The prompt asks for `"executable": true`. `untagged` ignores unknown
        /// fields, so this accepted a well-formed executable plan either way --
        /// but it also meant the *distinguishing* key was thrown away, and a
        /// response with neither `execution_type` nor `reasons` failed with an
        /// untagged-enum error that said nothing about which shape was missing.
        ///
        /// Captured so serde does not warn about a field nothing reads. The
        /// variants are already told apart by their required fields, which is
        /// why accepting either spelling works.
        #[serde(default, rename = "executable")]
        #[allow(dead_code)]
        executable: Option<bool>,
        execution_type: ExecutionType,
        #[serde(default)]
        target: Option<String>,
        actions: Vec<Action>,
    },
    NotExecutable {
        #[serde(default, rename = "executable")]
        #[allow(dead_code)]
        executable: Option<bool>,
        reasons: Vec<String>,
    },
}

/// The prompt for one case. With a `grounding` it also carries the real pages and locators of a crawl, who signs in
/// first, and the rule that a selector must be copied from that list (`grounding::prompt_block`); without one it is
/// exactly what it always was.
/// A short, safe excerpt of a model's reply for a parse-failure message: the
/// whole thing can be many kilobytes of prompt echo, and this string ends up in
/// an API response and the audit log.
fn summarize_for_error(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= 400 {
        flat
    } else {
        let head: String = flat.chars().take(400).collect();
        format!("{head}... ({} chars total)", flat.chars().count())
    }
}

/// The prompt for one case. With a `grounding` it also carries the real pages and locators of a crawl, who signs in
/// first, and the rule that a selector must be copied from that list (`grounding::prompt_block`); without one it is
/// exactly what it always was.
fn build_compiler_prompt(case: &TestCaseRow, grounding: Option<&Grounding>) -> String {
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
    let grounding_block = grounding
        .map(crate::grounding::prompt_block)
        .unwrap_or_default();
    format!(
        "You are a QA test case compiler. Given a real test case's title, category, steps, and expected \
         result, produce a structured, SAFE execution plan — or truthfully state one cannot be produced.\n\n\
         TEST CASE:\nTitle: {title}\nCategory: {category}\nSteps:\n{steps_text}Case-level expected result: \
         \"{expected_result}\"\nAssertions: {assertions:?}\n\n\
         {grounding_block}\
         Every entry of \"actions\" MUST be a JSON OBJECT with a \"type\" field plus exactly the \
         fields listed for that type. Write them out in full. A shorthand string like \
         \"NAVIGATE{{...}}\" is NOT accepted — write the object:\n\n\
           {{\"type\":\"NAVIGATE\",\"url\":\"https://example.test/page\"}}\n\
           {{\"type\":\"CLICK\",\"selector\":\"#save\"}}\n\
           {{\"type\":\"FILL\",\"selector\":\"#email\",\"value\":\"user@example.test\"}}\n\
           {{\"type\":\"ASSERT_VISIBLE\",\"selector\":\"#toast\"}}\n\
           {{\"type\":\"ASSERT_TEXT\",\"selector\":\"h1\",\"text\":\"Dashboard\"}}\n\
           {{\"type\":\"ASSERT_URL\",\"url_contains\":\"/dashboard\"}}\n\
           {{\"type\":\"REQUEST\",\"method\":\"GET\",\"url\":\"/health\",\"headers\":{{}},\"body\":null}}\n\
           {{\"type\":\"ASSERT_STATUS\",\"status\":200}}\n\
           {{\"type\":\"ASSERT_HEADER\",\"name\":\"content-type\",\"value\":\"application/json\"}}\n\
           {{\"type\":\"ASSERT_BODY_FIELD\",\"path\":\"data.id\",\"equals\":\"42\"}}\n\n\
         That closed vocabulary is all of it — nothing else exists, do not invent a new type:\n\
         Browser: NAVIGATE, CLICK, FILL, ASSERT_VISIBLE, ASSERT_TEXT, ASSERT_URL\n\
         API: REQUEST, ASSERT_STATUS, ASSERT_HEADER, ASSERT_BODY_FIELD\n\n\
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

async fn compile_with_ai(
    infra: &AiInfrastructure,
    case: &TestCaseRow,
    grounding: Option<&Grounding>,
) -> CompilationOutcome {
    let request = AiRequest {
        task: AiTask::TestCaseCompilation,
        prompt: build_compiler_prompt(case, grounding),
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

    // Through `extract_json_payload`, not `from_str` on the raw text. The same
    // models that write `ai_generation`'s JSON routinely wrap it in ```json
    // fences or prefix it with a `<think>` block, and that text is not valid
    // JSON -- so parsing it raw failed on output that was perfectly good apart
    // from its packaging. That function already existed, in this same crate,
    // for exactly this reason; the compiler prompt simply never used it.
    let payload = crate::ai_generation::extract_json_payload(&result.text);
    let parsed: AiPlanResponse = match payload.as_deref().map(serde_json::from_str) {
        Some(Ok(v)) => v,
        _ => {
            return CompilationOutcome::CompilationFailed {
                reasons: vec![format!(
                    "AI response did not match the required structured schema (expected \
                     {{\"executable\": true, \"execution_type\": \"API\"|\"BROWSER\", \
                     \"target\": string|null, \"actions\": [...]}} or \
                     {{\"executable\": false, \"reasons\": [...]}}); got: {}",
                    summarize_for_error(&result.text)
                )],
            }
        }
    };

    match parsed {
        AiPlanResponse::NotExecutable { executable: _, reasons } => {
            CompilationOutcome::NotExecutable { reasons }
        }
        AiPlanResponse::Executable {
            executable: _,
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
            let mut actions = actions;
            let mut diagnostics = vec![format!("compiled by {}/{}", result.provider, result.model)];
            // A grounded browser plan may use only selectors the crawl found. One that uses a selector it did not is
            // not compiled: it is NOT_EXECUTABLE, whatever the AI said. The login (never the AI's business) is put
            // in front of what the AI wrote.
            if let (Some(grounding), ExecutionType::Browser) = (grounding, execution_type) {
                let mut ungrounded = crate::grounding::ungrounded(&actions, grounding);
                if !ungrounded.is_empty() {
                    ungrounded.push(
                        "only selectors the crawl found can be compiled: crawl the page that has the element \
                         (in the state that shows it), then compile again"
                            .to_string(),
                    );
                    return CompilationOutcome::NotExecutable {
                        reasons: ungrounded,
                    };
                }
                diagnostics.extend(crate::grounding::literal_password_notes(
                    &actions, grounding,
                ));
                diagnostics.push(grounded_note(grounding));
                actions.splice(0..0, grounding.login.iter().cloned());
            }
            let plan = ExecutionPlan {
                test_case_id: case.id.to_string(),
                execution_type,
                target,
                actions,
                compiler: CompilerMethod::Ai,
                diagnostics,
                provider: Some(result.provider),
                model: Some(result.model),
            };
            validated(plan).await
        }
    }
}

/// A plan that has passed `validate_plan` is `Ready`; one that has not is a real `CompilationFailed`.
async fn validated(plan: ExecutionPlan) -> CompilationOutcome {
    match validate_plan(&plan).await {
        Ok(()) => CompilationOutcome::Ready(plan),
        Err(reasons) => CompilationOutcome::CompilationFailed { reasons },
    }
}

/// What a grounded plan says about its grounding, for the plan's own diagnostics.
fn grounded_note(grounding: &Grounding) -> String {
    let report = grounding.report();
    let mut note = format!(
        "grounded in a crawl: {} of its {} pages ({} elements) were shown to the compiler, and every selector was found by the crawl",
        report.pages_offered, report.pages_in_crawl, report.elements_offered
    );
    if let Some(role) = &report.role {
        note.push_str(&format!(
            "; signs in as role {role:?} first, reading {} from the environment",
            report.secrets.join(", ")
        ));
    }
    note
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
        if let Action::Request { headers, .. } = action {
            for (name, value) in headers {
                if let Some(problem) = header_reference_problem(name, value) {
                    reasons.push(problem);
                }
            }
        }
        if let Action::AssertStatusIn { statuses } = action {
            if statuses.is_empty() {
                reasons.push("ASSERT_STATUS_IN needs at least one status".to_string());
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
    compile_grounded(infra, case, None).await.outcome
}

/// What `compile_grounded` returns: the outcome, and — when a crawl grounded the compile — what the compiler was given.
#[derive(Debug, Clone)]
pub struct GroundedCompilation {
    pub outcome: CompilationOutcome,
    pub report: Option<GroundingReport>,
}

/// `compile`, optionally grounded in a crawl of the real application (`grounding.rs`).
///
/// With a grounding, a browser case is compiled from the crawl's real pages and locators rather than from prose alone:
/// the AI is shown them and told to copy selectors from them, the plan is refused as NOT_EXECUTABLE if it uses one the
/// crawl never found, and the role's login steps (credentials as `env:NAME` references, read when the test runs) are
/// put in front of the plan. An API case needs no page and no login, and is compiled exactly as before, as is any case
/// when there is no grounding. A case whose only step opens a page has nothing to ground: it still compiles
/// deterministically, and signs in first.
///
/// The AI call is the ordinary `TestCaseCompilation` task, so a task chain set in Settings (a local model, Groq, ...)
/// decides who answers it, and nothing outside that chain is asked.
pub async fn compile_grounded(
    infra: &AiInfrastructure,
    case: &TestCaseRow,
    grounding: Option<&Grounding>,
) -> GroundedCompilation {
    let Some(grounding) = grounding else {
        // No crawl: exactly what `compile` always did.
        let outcome = match compile_deterministic(case) {
            Some(plan) => validated(plan).await,
            None => compile_with_ai(infra, case, None).await,
        };
        return GroundedCompilation {
            outcome,
            report: None,
        };
    };

    // An API case needs no page and no login: grounding is not used for it.
    if let Some(plan) = compile_deterministic_api(case) {
        return GroundedCompilation {
            outcome: validated(plan).await,
            report: None,
        };
    }
    if case.steps.len() <= 1 {
        if let Some(mut plan) = compile_deterministic_browser(case) {
            plan.actions.splice(0..0, grounding.login.iter().cloned());
            plan.diagnostics.push(grounded_note(grounding));
            return GroundedCompilation {
                outcome: validated(plan).await,
                report: Some(grounding.report()),
            };
        }
    }
    GroundedCompilation {
        outcome: compile_with_ai(infra, case, Some(grounding)).await,
        report: Some(grounding.report()),
    }
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
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor, RecordingExecutor,
    };
    use crate::ai_router::{
        AiExecutor, AiSettings, CapabilityLevel, RoutingMode, TaskProviderPreference, TaskTarget,
    };
    use crate::grounding::CrawlGrounding;
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

    fn api_plan(headers: &[(&str, &str)], expect: Vec<Action>) -> ExecutionPlan {
        let mut actions = vec![Action::Request {
            method: "GET".into(),
            url: "http://93.184.216.34/pets".into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: None,
        }];
        actions.extend(expect);
        ExecutionPlan {
            test_case_id: "t".into(),
            execution_type: ExecutionType::Api,
            target: Some("http://93.184.216.34/pets".into()),
            actions,
            compiler: CompilerMethod::Deterministic,
            diagnostics: vec![],
            provider: None,
            model: None,
        }
    }

    #[test]
    fn a_plans_secret_names_include_the_ones_its_request_headers_refer_to() {
        let plan = api_plan(
            &[
                ("Authorization", "env:AIQA_SECRET_API_TOKEN"),
                ("X-Plain", "plain"),
                ("X-Not-Allowed", "env:GROQ_API_KEY"),
            ],
            vec![],
        );
        let mut actions = plan.actions;
        actions.push(Action::Fill {
            selector: "#p".into(),
            value: "env:AIQA_LOGIN_ADMIN_PASSWORD".into(),
        });
        assert_eq!(
            secret_names_of(&actions),
            vec![
                "AIQA_LOGIN_ADMIN_PASSWORD".to_string(),
                "AIQA_SECRET_API_TOKEN".to_string()
            ],
            "a name a plan may not use is not listed"
        );
    }

    #[test]
    fn only_a_header_value_that_starts_env_and_is_not_a_good_reference_is_a_problem() {
        assert_eq!(
            header_reference_problem("Authorization", "Bearer abc"),
            None
        );
        assert_eq!(
            header_reference_problem("Authorization", "env:AIQA_SECRET_API_TOKEN"),
            None
        );
        let problem =
            header_reference_problem("Authorization", "env:GROQ_API_KEY").expect("a problem");
        assert!(
            problem.contains("the header Authorization"),
            "problem={problem}"
        );
        assert!(
            problem.contains("AIQA_LOGIN_ or AIQA_SECRET_"),
            "problem={problem}"
        );
        assert!(
            !problem.contains("GROQ_API_KEY"),
            "the variable asked for is not repeated: {problem}"
        );
    }

    #[tokio::test]
    async fn validate_plan_rejects_a_bad_header_reference_and_an_empty_status_list_and_accepts_the_new_assertions(
    ) {
        assert!(validate_plan(&api_plan(
            &[("Authorization", "env:AIQA_SECRET_API_TOKEN")],
            vec![]
        ))
        .await
        .is_ok());
        assert!(validate_plan(&api_plan(
            &[("X", "plain")],
            vec![
                Action::AssertStatusIn {
                    statuses: vec![200, 201]
                },
                Action::AssertBodySchema {
                    schema: serde_json::json!({"type": "object"})
                }
            ]
        ))
        .await
        .is_ok());
        let reasons = validate_plan(&api_plan(
            &[("Authorization", "env:GEMINI_API_KEY")],
            vec![],
        ))
        .await
        .unwrap_err();
        assert_eq!(reasons.len(), 1);
        assert!(
            reasons[0].contains("the header Authorization"),
            "{reasons:?}"
        );
        assert!(!reasons[0].contains("GEMINI_API_KEY"), "{reasons:?}");
        let reasons = validate_plan(&api_plan(
            &[],
            vec![Action::AssertStatusIn { statuses: vec![] }],
        ))
        .await
        .unwrap_err();
        assert_eq!(
            reasons,
            vec!["ASSERT_STATUS_IN needs at least one status".to_string()]
        );
    }

    #[test]
    fn the_new_assertions_are_written_and_read_by_their_wire_names() {
        let in_list = serde_json::to_value(Action::AssertStatusIn {
            statuses: vec![200, 404],
        })
        .unwrap();
        assert_eq!(
            in_list,
            serde_json::json!({"type": "ASSERT_STATUS_IN", "statuses": [200, 404]})
        );
        let schema = serde_json::to_value(Action::AssertBodySchema {
            schema: serde_json::json!({"type": "object"}),
        })
        .unwrap();
        assert_eq!(
            schema,
            serde_json::json!({"type": "ASSERT_BODY_SCHEMA", "schema": {"type": "object"}})
        );
        let back: Action = serde_json::from_value(in_list).unwrap();
        assert_eq!(
            back,
            Action::AssertStatusIn {
                statuses: vec![200, 404]
            }
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

    /// The defect: the compiler parsed `result.text` raw, so the same model
    /// reply that `ai_generation` handles every day — the JSON wrapped in a
    /// ```json fence, as instructed by half the world's chat models — was read
    /// as a schema failure here. Nothing about the plan was wrong; only its
    /// packaging was, and the case was reported as COMPILATION_FAILED.
    #[tokio::test]
    async fn a_fenced_json_plan_compiles_instead_of_being_reported_as_a_schema_failure() {
        let c = case(
            "Real click",
            "UI",
            vec![("Click the save button", "Saved")],
            "Saved",
            vec![],
        );
        let fenced = format!(
            "```json\n{{\"executable\":true,\"execution_type\":\"BROWSER\",\"target\":null,\"actions\":[{{\"type\":\"NAVIGATE\",\"url\":\"{DEFAULT_TARGET}/settings\"}},{{\"type\":\"CLICK\",\"selector\":\"#save\"}}]}}\n```"
        );
        let infra = infra_with_ai(Ok(fenced.as_str()));
        match compile(&infra, &c).await {
            CompilationOutcome::Ready(plan) => {
                assert_eq!(plan.actions.len(), 2, "the fenced plan must compile");
            }
            other => panic!("a fenced but otherwise valid plan must compile, got {other:?}"),
        }
    }

    /// Same, for the reasoning models that prefix the JSON with a `<think>`
    /// block -- qwen emits these, and one is configured here.
    #[tokio::test]
    async fn a_think_prefixed_plan_compiles_too() {
        let c = case(
            "Real click",
            "UI",
            vec![("Click the save button", "Saved")],
            "Saved",
            vec![],
        );
        let raw = format!(
            "<think>\nThe user wants to click save. I will navigate then click.\n</think>\n{{\"executable\":true,\"execution_type\":\"BROWSER\",\"target\":null,\"actions\":[{{\"type\":\"NAVIGATE\",\"url\":\"{DEFAULT_TARGET}/settings\"}},{{\"type\":\"CLICK\",\"selector\":\"#save\"}}]}}"
        );
        let infra = infra_with_ai(Ok(raw.as_str()));
        match compile(&infra, &c).await {
            CompilationOutcome::Ready(plan) => assert_eq!(plan.actions.len(), 2),
            other => panic!("a think-prefixed plan must compile, got {other:?}"),
        }
    }

    /// The defect, as it actually reached production: the prompt listed the
    /// closed vocabulary as shorthand (`NAVIGATE{url}`, `CLICK{selector}`) and
    /// never showed one complete action object. The model read the shorthand
    /// literally and answered
    /// `"actions":["NAVIGATE{http://...}","ASSERT_VISIBLE{#launch-run-header-button}"]`,
    /// which is not a plan — every entry is a string. That is a prompt defect,
    /// not a model failure, and it is pinned here so the vocabulary can never
    /// again be shown without its object form.
    #[test]
    fn the_prompt_shows_a_complete_action_object_so_the_model_cannot_answer_with_shorthand_strings() {
        let c = case(
            "Click save",
            "UI",
            vec![("Click the save button", "Saved")],
            "Saved",
            vec![],
        );
        let prompt = build_compiler_prompt(&c, None);
        // Every action type must appear in its real object form.
        for needle in [
            r##"{"type":"NAVIGATE","url":"https://example.test/page"}"##,
            r##"{"type":"CLICK","selector":"#save"}"##,
            r##"{"type":"FILL","selector":"#email","value":"user@example.test"}"##,
            r##"{"type":"ASSERT_VISIBLE","selector":"#toast"}"##,
            r##"{"type":"ASSERT_TEXT","selector":"h1","text":"Dashboard"}"##,
            r##"{"type":"ASSERT_URL","url_contains":"/dashboard"}"##,
            r##"{"type":"ASSERT_STATUS","status":200}"##,
        ] {
            assert!(
                prompt.contains(needle),
                "the prompt must show {needle} in full, or the model answers with a \
                 shorthand string that is not a plan"
            );
        }
        assert!(
            prompt.contains("MUST be a JSON OBJECT"),
            "the prompt must state the requirement, not just imply it"
        );
        // And the shorthand form must be called out as wrong, since it reads
        // naturally enough that a model will reach for it.
        assert!(
            prompt.contains("NOT accepted"),
            "the prompt must say the shorthand string is rejected"
        );
    }

    /// The shorthand a real model produced, verbatim. It must still be refused --
    /// the prompt fix stops it being produced, it does not make it acceptable.
    #[tokio::test]
    async fn shorthand_action_strings_are_still_refused() {
        let c = case(
            "Click save",
            "UI",
            vec![("Click the save button", "Saved")],
            "Saved",
            vec![],
        );
        let shorthand = r##"{"executable":true,"execution_type":"BROWSER","target":"http://x.test/","actions":["NAVIGATE{http://x.test/}","ASSERT_VISIBLE{#save}"]}"##;
        let infra = infra_with_ai(Ok(shorthand));
        match compile(&infra, &c).await {
            CompilationOutcome::CompilationFailed { reasons } => assert!(
                reasons.iter().any(|r| r.contains("did not match the required structured schema")),
                "a string where an action object belongs is not a plan, got {reasons:?}"
            ),
            other => panic!("shorthand strings must not compile, got {other:?}"),
        }
    }

    /// A plan that is genuinely not JSON must still fail — the fence handling is
    /// not a licence to accept anything.
    #[tokio::test]
    async fn prose_with_no_json_in_it_is_still_a_real_failure() {
        let c = case(
            "Vague",
            "UI",
            vec![("Do the thing", "Done")],
            "Done",
            vec![],
        );
        let infra = infra_with_ai(Ok("I am not able to produce a plan for this."));
        match compile(&infra, &c).await {
            CompilationOutcome::CompilationFailed { reasons } => {
                assert!(
                    reasons.iter().any(|r| r.contains("did not match the required structured schema")),
                    "reasons={reasons:?}"
                );
                // And the message must show what actually came back, so this is
                // diagnosable rather than just "it didn't match".
                assert!(
                    reasons.iter().any(|r| r.contains("I am not able to produce")),
                    "the error must quote the reply, got {reasons:?}"
                );
            }
            other => panic!("prose must still fail, got {other:?}"),
        }
    }

    /// The failure message used to echo the entire reply, which can be many
    /// kilobytes and lands in an API response and the audit log.
    #[test]
    fn a_long_reply_is_summarized_rather_than_echoed_whole() {
        let long = "x".repeat(5_000);
        let s = summarize_for_error(&long);
        assert!(
            s.len() < 600,
            "a 5000-char reply must not be quoted in full, got {} chars",
            s.len()
        );
        assert!(s.contains("5000 chars total"), "it must say how much there was: {s}");
        assert_eq!(summarize_for_error("short reply"), "short reply");
    }

    // ---------------------------------------------------------------- grounding

    /// A crawl with two roles: `admin` (signs in with two secrets, reaches a dashboard and Settings) and `anonymous`
    /// (the login page and its fields).
    const CRAWL_JSON: &str = r##"{"roles":[
      {"role":"admin","loginSucceeded":true,"loginUsesLiterals":false,
       "loginActions":[
         {"type":"NAVIGATE","url":"http://127.0.0.1:8080/login"},
         {"type":"FILL","selector":"#username","value":"env:AIQA_LOGIN_ADMIN_USERNAME"},
         {"type":"FILL","selector":"#password","value":"env:AIQA_LOGIN_ADMIN_PASSWORD"},
         {"type":"CLICK","selector":"#submit-login"}],
       "pages":[
         {"url":"http://127.0.0.1:8080/dashboard","title":"Dashboard","elements":[
           {"kind":"link","text":"Settings","locator":"a[href=\"/settings\"]"},
           {"kind":"link","text":"Manage Users","locator":"a[href=\"/users\"]"}]},
         {"url":"http://127.0.0.1:8080/settings","title":"Settings","elements":[
           {"kind":"button","text":"Delete Account","locator":"#danger-zone"}]}]},
      {"role":"anonymous","loginSucceeded":true,"loginUsesLiterals":false,"loginActions":[],
       "pages":[{"url":"http://127.0.0.1:8080/login","title":"Log in","elements":[
           {"kind":"input","text":"Username","locator":"#username","inputType":"text"},
           {"kind":"input","text":"Password","locator":"#password","inputType":"password"},
           {"kind":"button","text":"Log in","locator":"#submit-login"}]}]}]}"##;

    fn grounding_as(role: Option<&str>, c: &TestCaseRow) -> Grounding {
        let crawl: CrawlGrounding = serde_json::from_str(CRAWL_JSON).expect("the crawl parses");
        Grounding::for_case(&crawl, role, c).expect("grounded")
    }

    /// A case that opens Settings from the dashboard — several steps, so nothing deterministic can stand in for the AI.
    fn open_settings() -> TestCaseRow {
        case(
            "Admin opens Settings",
            "Functional",
            vec![
                ("Open the dashboard", "The dashboard is shown"),
                ("Click the Settings link", "The Settings page is shown"),
            ],
            "Settings page is shown with its danger zone",
            vec![],
        )
    }

    fn infra_recording(answer: &str) -> (AiInfrastructure, Arc<RecordingExecutor>) {
        let executor = Arc::new(RecordingExecutor::ok_sequence(vec![answer]));
        let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> =
            std::collections::HashMap::new();
        executors.insert("mock", executor.clone() as Arc<dyn AiExecutor>);
        let infra = AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        );
        (infra, executor)
    }

    const SETTINGS_PLAN: &str = r##"{"executable":true,"execution_type":"BROWSER","target":"http://127.0.0.1:8080/dashboard","actions":[
        {"type":"NAVIGATE","url":"http://127.0.0.1:8080/dashboard"},
        {"type":"CLICK","selector":"a[href=\"/settings\"]"},
        {"type":"ASSERT_URL","url_contains":"/settings"},
        {"type":"ASSERT_VISIBLE","selector":"#danger-zone"}]}"##;

    #[tokio::test]
    async fn a_grounded_browser_case_is_compiled_with_the_login_first_and_only_crawled_selectors() {
        let c = open_settings();
        let (infra, _) = infra_recording(SETTINGS_PLAN);
        let g = grounding_as(Some("admin"), &c);
        let compiled = compile_grounded(&infra, &c, Some(&g)).await;

        let CompilationOutcome::Ready(plan) = compiled.outcome else {
            panic!("expected Ready, got {:?}", compiled.outcome)
        };
        assert_eq!(plan.execution_type, ExecutionType::Browser);
        // The role's login first (its credentials as references), then the AI's own steps.
        assert_eq!(plan.actions.len(), 4 + 4);
        assert!(matches!(&plan.actions[0], Action::Navigate { url } if url.ends_with("/login")));
        assert!(
            matches!(&plan.actions[3], Action::Click { selector } if selector == "#submit-login")
        );
        assert!(
            matches!(&plan.actions[4], Action::Navigate { url } if url.ends_with("/dashboard"))
        );
        assert!(
            matches!(&plan.actions[5], Action::Click { selector } if selector == "a[href=\"/settings\"]")
        );
        assert_eq!(
            secret_names_of(&plan.actions),
            vec![
                "AIQA_LOGIN_ADMIN_PASSWORD".to_string(),
                "AIQA_LOGIN_ADMIN_USERNAME".to_string()
            ]
        );
        assert!(
            plan.diagnostics
                .iter()
                .any(|d| d.contains("grounded in a crawl")
                    && d.contains("signs in as role \"admin\"")),
            "diagnostics={:?}",
            plan.diagnostics
        );
        // What the response says was given to the compiler: names, never values.
        let report = compiled
            .report
            .expect("a grounded compile reports its grounding");
        assert_eq!(report.role.as_deref(), Some("admin"));
        assert_eq!(report.login_steps, 4);
        assert_eq!(
            report.secrets,
            vec![
                "AIQA_LOGIN_ADMIN_PASSWORD".to_string(),
                "AIQA_LOGIN_ADMIN_USERNAME".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn the_compiler_is_shown_the_crawls_real_locators_and_told_who_signs_in_and_never_a_secret(
    ) {
        let c = open_settings();
        let (infra, executor) = infra_recording(SETTINGS_PLAN);
        let g = grounding_as(Some("admin"), &c);
        compile_grounded(&infra, &c, Some(&g)).await;

        let prompts = executor.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        let prompt = &prompts[0];
        assert!(prompt.contains("GROUNDED APPLICATION"), "{prompt}");
        assert!(
            prompt.contains("- link \"Settings\" -> a[href=\"/settings\"]"),
            "{prompt}"
        );
        assert!(
            prompt.contains("- button \"Delete Account\" -> #danger-zone"),
            "{prompt}"
        );
        assert!(
            prompt.contains("signs in as the role \"admin\""),
            "{prompt}"
        );
        assert!(prompt.contains("ONLY a locator copied exactly"), "{prompt}");
        // The case itself is still in it.
        assert!(prompt.contains("Admin opens Settings"), "{prompt}");
        assert!(
            !prompt.contains("AIQA_LOGIN_"),
            "no secret name reaches a model: {prompt}"
        );
    }

    #[tokio::test]
    async fn a_selector_the_crawl_never_found_makes_the_case_not_executable_however_the_ai_answered(
    ) {
        let c = open_settings();
        let invented = r##"{"executable":true,"execution_type":"BROWSER","target":"http://127.0.0.1:8080/dashboard","actions":[
            {"type":"NAVIGATE","url":"http://127.0.0.1:8080/dashboard"},
            {"type":"CLICK","selector":"#settings-link"},
            {"type":"ASSERT_VISIBLE","selector":"#danger-zone"},
            {"type":"ASSERT_TEXT","selector":"h1","text":"Settings"}]}"##;
        let (infra, _) = infra_recording(invented);
        let g = grounding_as(Some("admin"), &c);
        let compiled = compile_grounded(&infra, &c, Some(&g)).await;

        match compiled.outcome {
            CompilationOutcome::NotExecutable { reasons } => {
                assert!(
                    reasons
                        .iter()
                        .any(|r| r.contains("step 2 (CLICK)") && r.contains("#settings-link")),
                    "reasons={reasons:?}"
                );
                assert!(
                    reasons.iter().any(|r| r.contains("step 4 (ASSERT_TEXT)")),
                    "reasons={reasons:?}"
                );
                assert!(
                    !reasons.iter().any(|r| r.contains("step 3")),
                    "a selector the crawl found is not blamed: {reasons:?}"
                );
                assert!(
                    reasons
                        .iter()
                        .any(|r| r.contains("only selectors the crawl found can be compiled")),
                    "reasons={reasons:?}"
                );
            }
            other => panic!("expected NotExecutable, got {other:?}"),
        }
        assert!(
            compiled.report.is_some(),
            "the response still says what the compiler was given"
        );
    }

    #[tokio::test]
    async fn without_a_crawl_an_invented_selector_compiles_as_it_always_did() {
        // The old behaviour, unchanged: no crawl, no grounding check, no login, and the prompt as it was.
        let c = open_settings();
        let invented = r##"{"executable":true,"execution_type":"BROWSER","target":"http://127.0.0.1:8080/x","actions":[
            {"type":"NAVIGATE","url":"http://127.0.0.1:8080/x"},{"type":"CLICK","selector":"#settings-link"}]}"##;
        let (infra, executor) = infra_recording(invented);
        let compiled = compile_grounded(&infra, &c, None).await;
        assert!(
            matches!(compiled.outcome, CompilationOutcome::Ready(_)),
            "outcome={:?}",
            compiled.outcome
        );
        assert!(compiled.report.is_none());
        let prompts = executor.prompts.lock().unwrap();
        assert!(
            !prompts[0].contains("GROUNDED APPLICATION") && !prompts[0].contains("LOGIN:"),
            "{}",
            prompts[0]
        );
        assert!(
            prompts[0].contains("Never invent a URL, selector, or value"),
            "the old rules are still there"
        );
    }

    #[tokio::test]
    async fn an_ai_that_declines_a_grounded_case_gives_its_own_reasons() {
        let c = open_settings();
        let (infra, _) = infra_recording(
            r#"{"executable":false,"reasons":["there is no Billing link in the crawl"]}"#,
        );
        let g = grounding_as(Some("admin"), &c);
        match compile_grounded(&infra, &c, Some(&g)).await.outcome {
            CompilationOutcome::NotExecutable { reasons } => {
                assert_eq!(
                    reasons,
                    vec!["there is no Billing link in the crawl".to_string()]
                )
            }
            other => panic!("expected NotExecutable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_case_that_only_opens_a_page_compiles_deterministically_and_signs_in_first_without_the_ai(
    ) {
        let c = case(
            "Dashboard loads",
            "Functional",
            vec![(
                "Navigate to http://127.0.0.1:8080/dashboard",
                "The dashboard is shown",
            )],
            "Dashboard is shown",
            vec![],
        );
        let (infra, executor) = infra_recording("not asked");
        let g = grounding_as(Some("admin"), &c);
        let compiled = compile_grounded(&infra, &c, Some(&g)).await;
        let CompilationOutcome::Ready(plan) = compiled.outcome else {
            panic!("expected Ready")
        };
        assert_eq!(plan.compiler, CompilerMethod::Deterministic);
        assert_eq!(
            plan.actions.len(),
            4 + 2,
            "the login, then the navigation and its URL check"
        );
        assert!(matches!(&plan.actions[0], Action::Navigate { url } if url.ends_with("/login")));
        assert!(
            executor.prompts.lock().unwrap().is_empty(),
            "the AI was not asked"
        );
    }

    #[tokio::test]
    async fn a_case_with_more_steps_than_opening_a_page_is_never_cut_down_to_the_page_it_opens() {
        // Compiled deterministically this would keep only "Navigate to ..." and quietly drop the rest.
        let c = case(
            "Open Settings",
            "Functional",
            vec![
                (
                    "Navigate to http://127.0.0.1:8080/dashboard",
                    "The dashboard is shown",
                ),
                ("Click the Settings link", "The Settings page is shown"),
            ],
            "Settings page is shown",
            vec![],
        );
        let (infra, executor) = infra_recording(SETTINGS_PLAN);
        let g = grounding_as(Some("admin"), &c);
        let compiled = compile_grounded(&infra, &c, Some(&g)).await;
        let CompilationOutcome::Ready(plan) = compiled.outcome else {
            panic!("expected Ready")
        };
        assert_eq!(plan.compiler, CompilerMethod::Ai);
        assert_eq!(
            executor.prompts.lock().unwrap().len(),
            1,
            "the AI compiled it"
        );
        assert!(
            plan.actions
                .iter()
                .any(|a| matches!(a, Action::Click { .. })),
            "its click is in the plan"
        );
    }

    #[tokio::test]
    async fn an_api_case_ignores_the_grounding_and_gets_no_login() {
        let c = case(
            "Health check",
            "API",
            vec![("GET /health", "HTTP 200 with body ok")],
            "HTTP 200 ok",
            vec![],
        );
        let (infra, executor) = infra_recording("not asked");
        let g = grounding_as(Some("admin"), &c);
        let compiled = compile_grounded(&infra, &c, Some(&g)).await;
        let CompilationOutcome::Ready(plan) = compiled.outcome else {
            panic!("expected Ready")
        };
        assert_eq!(plan.execution_type, ExecutionType::Api);
        assert!(!plan
            .actions
            .iter()
            .any(|a| matches!(a, Action::Fill { .. } | Action::Click { .. })));
        assert!(compiled.report.is_none(), "grounding was not used");
        assert!(executor.prompts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_case_that_runs_without_signing_in_is_grounded_on_the_role_that_never_logged_in() {
        let c = case(
            "Login form",
            "Functional",
            vec![
                ("Type a wrong password", "It is refused"),
                ("Click Log in", "An error is shown"),
            ],
            "Login is refused",
            vec![],
        );
        let plan_json = r##"{"executable":true,"execution_type":"BROWSER","target":"http://127.0.0.1:8080/login","actions":[
            {"type":"NAVIGATE","url":"http://127.0.0.1:8080/login"},
            {"type":"FILL","selector":"#username","value":"bob"},
            {"type":"FILL","selector":"#password","value":"wrong-password"},
            {"type":"CLICK","selector":"#submit-login"}]}"##;
        let (infra, executor) = infra_recording(plan_json);
        let g = grounding_as(None, &c);
        let compiled = compile_grounded(&infra, &c, Some(&g)).await;
        let CompilationOutcome::Ready(plan) = compiled.outcome else {
            panic!("expected Ready")
        };
        assert_eq!(
            plan.actions.len(),
            4,
            "no login steps in front: the case does not sign in"
        );
        assert!(executor.prompts.lock().unwrap()[0].contains("runs without signing in"));
        // A literal typed into a password field is noted, never hidden (and the note does not repeat it).
        let note = plan
            .diagnostics
            .iter()
            .find(|d| d.contains("password field"))
            .expect("a note");
        assert!(
            note.contains("step 3") && !note.contains("wrong-password"),
            "note={note}"
        );
    }

    #[tokio::test]
    async fn the_compile_task_chain_decides_who_answers_a_grounded_compile() {
        // "paid" would win automatic routing (it is Strong); the task's chain names only "local".
        let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> =
            std::collections::HashMap::new();
        executors.insert("local", Arc::new(FakeExecutor::ok(SETTINGS_PLAN)));
        let paid = Arc::new(RecordingExecutor::ok_sequence(vec![SETTINGS_PLAN]));
        executors.insert("paid", paid.clone() as Arc<dyn AiExecutor>);
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider(
                    "local",
                    vec![healthy_model(
                        "local-model",
                        "local",
                        CapabilityLevel::Basic,
                    )],
                ))),
                Arc::new(FakeDiscovery(healthy_provider(
                    "paid",
                    vec![healthy_model("paid-model", "paid", CapabilityLevel::Strong)],
                ))),
            ],
            executors,
        );
        let mut task_overrides = std::collections::HashMap::new();
        task_overrides.insert(
            AiTask::TestCaseCompilation,
            TaskProviderPreference {
                chain: vec![TaskTarget {
                    provider: "local".to_string(),
                    model: "local-model".to_string(),
                }],
                ..Default::default()
            },
        );
        infra
            .set_settings(AiSettings {
                mode: RoutingMode::Auto,
                preferred_provider: None,
                preferred_model: None,
                fallback_enabled: true,
                task_overrides,
            })
            .await;

        let c = open_settings();
        let g = grounding_as(Some("admin"), &c);
        let CompilationOutcome::Ready(plan) = compile_grounded(&infra, &c, Some(&g)).await.outcome
        else {
            panic!("expected Ready")
        };
        assert_eq!(plan.provider.as_deref(), Some("local"));
        assert!(
            paid.prompts.lock().unwrap().is_empty(),
            "a provider outside the chain is never asked"
        );
    }

    #[tokio::test]
    async fn however_large_the_crawl_the_compile_prompt_stays_small_enough_for_a_local_window() {
        let elements: Vec<serde_json::Value> = (0..500)
            .map(|i| {
                serde_json::json!({
                    "kind": "link",
                    "text": format!("Report number {i} with a long descriptive label"),
                    "locator": format!("a[href=\"/r/{i}\"]"),
                })
            })
            .collect();
        let pages: Vec<serde_json::Value> = (0..40)
            .map(|i| {
                serde_json::json!({
                    "url": format!("http://127.0.0.1:8080/section/{i}"),
                    "title": format!("Section {i}"),
                    "elements": elements,
                })
            })
            .collect();
        let crawl: CrawlGrounding = serde_json::from_value(serde_json::json!({"roles": [
            {"role": "anonymous", "loginSucceeded": true, "loginUsesLiterals": false, "loginActions": [], "pages": pages}
        ]}))
        .unwrap();
        let c = open_settings();
        let g = Grounding::for_case(&crawl, None, &c).expect("grounded");
        let (infra, executor) = infra_recording(r#"{"executable":false,"reasons":["x"]}"#);
        compile_grounded(&infra, &c, Some(&g)).await;
        let prompt = executor.prompts.lock().unwrap()[0].clone();
        assert!(
            prompt.chars().count() < 8_000,
            "{} characters is too many for a 4,096-token window with the case and the answer",
            prompt.chars().count()
        );
    }
}
