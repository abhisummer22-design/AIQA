//! Plan Validator — Phase 4: Governed Agent Plan Validation.
//!
//! Enforces strict validation boundary between Agent-generated plans and AIQA
//! execution infrastructure. Validates:
//! - Multi-tenant project boundary & environment
//! - Closed Action AST (execution_plan::Action)
//! - Allowed executor vocabulary (compiled-api, playwright, simulated, pytest, newman, maestro, schemathesis)
//! - Rejection of arbitrary shell/python/code injection
//! - Target URL SSRF protection via `executor::validate_target_url`
//! - DAG dependency acyclicity & reference integrity
//! - Execution budgets (task count, timeout, concurrency)

use crate::agents::planning::TestPlanOutput;
use crate::domain::{RunPlan, TaskDef};
use crate::execution_plan::Action;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// Default environment if omitted.
pub const DEFAULT_ENVIRONMENT: &str = "test";
/// Default concurrency if omitted.
pub const DEFAULT_CONCURRENCY: usize = 4;

/// Known, approved executor kinds. Any executor outside this vocabulary is strictly rejected.
pub const ALLOWED_EXECUTORS: &[&str] = &[
    "compiled-api",
    "playwright",
    "simulated",
    "pytest",
    "newman",
    "maestro",
    "schemathesis",
];

/// Approved environment types.
pub const ALLOWED_ENVIRONMENTS: &[&str] = &[
    "development",
    "dev",
    "staging",
    "stage",
    "test",
    "testing",
    "production",
    "prod",
    "ci",
];

/// Structured validation error describing a single defect in an Agent plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanValidationError {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

impl PlanValidationError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        field: Option<impl Into<String>>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field: field.map(Into::into),
        }
    }
}

impl std::fmt::Display for PlanValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref field) = self.field {
            write!(f, "[{}] ({}): {}", self.code, field, self.message)
        } else {
            write!(f, "[{}]: {}", self.code, self.message)
        }
    }
}

impl std::error::Error for PlanValidationError {}

/// Agent-generated execution plan input representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionPlan {
    pub project_id: String,
    #[serde(default = "default_env")]
    pub environment: String,
    pub plan_name: String,
    #[serde(default = "default_concurrency_val")]
    pub concurrency: usize,
    pub tasks: Vec<AgentTaskPlan>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

fn default_env() -> String {
    DEFAULT_ENVIRONMENT.to_string()
}

fn default_concurrency_val() -> usize {
    DEFAULT_CONCURRENCY
}

/// An individual planned task produced by an Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskPlan {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    pub executor: String,
    #[serde(default)]
    pub target_url: Option<String>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub requirement_reference: Option<String>,
    #[serde(default)]
    pub custom_params: Option<serde_json::Value>,
}

impl AgentExecutionPlan {
    /// Bridges Phase 3 `TestPlanOutput` into a validated, executable `AgentExecutionPlan`.
    pub fn from_test_plan_output(output: &TestPlanOutput, default_target: &str) -> Self {
        let mut tasks = Vec::new();
        let mut seen_ids = HashSet::new();

        for suite in &output.prioritized_test_suites {
            for obj in &suite.test_objectives {
                let task_id = if seen_ids.insert(obj.id.clone()) {
                    obj.id.clone()
                } else {
                    format!("{}-{}", obj.id, uuid::Uuid::new_v4().simple())
                };

                let mut actions = Vec::new();
                let mut target_url = None;

                if let Some(ref endpoint) = obj.api_endpoint {
                    // Parse endpoint like "POST /payments" or "GET http://target/api"
                    let parts: Vec<&str> = endpoint.split_whitespace().collect();
                    let (method, path) = if parts.len() >= 2 {
                        (parts[0].to_uppercase(), parts[1])
                    } else {
                        ("GET".to_string(), endpoint.as_str())
                    };

                    let full_url = if path.starts_with("http://") || path.starts_with("https://") {
                        path.to_string()
                    } else {
                        format!("{}{}", default_target.trim_end_matches('/'), path)
                    };

                    target_url = Some(full_url.clone());
                    actions.push(Action::Request {
                        method,
                        url: full_url,
                        headers: BTreeMap::new(),
                        body: None,
                    });

                    // Expected status based on objective priority/category
                    let expected_status =
                        if obj.priority == "P0" && obj.category == "Authentication" {
                            401
                        } else {
                            200
                        };
                    actions.push(Action::AssertStatus {
                        status: expected_status,
                    });
                }

                tasks.push(AgentTaskPlan {
                    id: task_id,
                    name: obj.title.clone(),
                    prerequisites: Vec::new(),
                    executor: "compiled-api".to_string(),
                    target_url,
                    actions,
                    timeout_secs: Some(30),
                    requirement_reference: obj.requirement_reference.clone(),
                    custom_params: None,
                });
            }
        }

        Self {
            project_id: output.project_id.clone(),
            environment: DEFAULT_ENVIRONMENT.to_string(),
            plan_name: format!("Plan for {}", output.project_id),
            concurrency: DEFAULT_CONCURRENCY,
            tasks,
            metadata: BTreeMap::new(),
        }
    }
}

/// A strictly validated plan ready to be safely materialized into a `RunPlan`.
#[derive(Debug, Clone)]
pub struct ValidatedPlan {
    pub project_id: String,
    pub environment: String,
    pub plan_name: String,
    pub concurrency: usize,
    pub tasks: Vec<AgentTaskPlan>,
    pub metadata: BTreeMap<String, String>,
}

impl ValidatedPlan {
    /// Converts the validated plan into an engine-authoritative `RunPlan`.
    pub fn to_run_plan(&self) -> Result<RunPlan, String> {
        let mut task_defs = Vec::new();

        for task in &self.tasks {
            let task_def = match task.executor.as_str() {
                "compiled-api" => {
                    let exec_plan = crate::execution_plan::ExecutionPlan {
                        test_case_id: task.id.clone(),
                        execution_type: crate::execution_plan::ExecutionType::Api,
                        target: task.target_url.clone(),
                        actions: task.actions.clone(),
                        compiler: crate::execution_plan::CompilerMethod::Manual,
                        diagnostics: vec!["materialized by AgentExecutionDispatcher".to_string()],
                        provider: None,
                        model: None,
                    };

                    crate::execution_plan::materialize_task_def(
                        task.id.clone(),
                        task.name.clone(),
                        &exec_plan,
                    )?
                }
                "playwright" => {
                    let exec_plan = crate::execution_plan::ExecutionPlan {
                        test_case_id: task.id.clone(),
                        execution_type: crate::execution_plan::ExecutionType::Browser,
                        target: task.target_url.clone(),
                        actions: task.actions.clone(),
                        compiler: crate::execution_plan::CompilerMethod::Manual,
                        diagnostics: vec!["materialized by AgentExecutionDispatcher".to_string()],
                        provider: None,
                        model: None,
                    };

                    crate::execution_plan::materialize_task_def(
                        task.id.clone(),
                        task.name.clone(),
                        &exec_plan,
                    )?
                }
                "simulated" => TaskDef {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    prerequisites: task.prerequisites.clone(),
                    simulate: Some(crate::domain::SimSpec {
                        outcome: crate::domain::SimOutcome::Pass,
                        delay_ms: 10,
                    }),
                    executor: Some("simulated".to_string()),
                    // Phase 2: a simulated task is an explicitly declared
                    // outcome, not a real execution of anything, so it
                    // contributes no history and stays unlinked.
                    test_case_id: None,
                    params: task
                        .custom_params
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({})),
                },
                other => TaskDef {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    prerequisites: task.prerequisites.clone(),
                    simulate: None,
                    executor: Some(other.to_string()),
                    // Phase 2: a hand-declared executor carries no case link,
                    // so it stays unlinked rather than being inferred.
                    test_case_id: None,
                    params: task
                        .custom_params
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({})),
                },
            };

            task_defs.push(task_def);
        }

        Ok(RunPlan {
            name: self.plan_name.clone(),
            concurrency: self.concurrency,
            tasks: task_defs,
        })
    }
}

/// Validation engine for Agent plans.
pub struct PlanValidator;

impl PlanValidator {
    /// Validates an `AgentExecutionPlan` against strict safety rules, SSRF guards, and budgets.
    pub async fn validate(
        plan: &AgentExecutionPlan,
        max_tasks: usize,
        max_task_timeout_secs: u64,
        max_concurrency: usize,
    ) -> Result<ValidatedPlan, Vec<PlanValidationError>> {
        let mut errors = Vec::new();

        // 1. Project ID Validation
        let project_id = plan.project_id.trim();
        if project_id.is_empty() {
            errors.push(PlanValidationError::new(
                "EMPTY_PROJECT_ID",
                "project_id must not be empty",
                Some("project_id"),
            ));
        } else if !project_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            errors.push(PlanValidationError::new(
                "INVALID_PROJECT_ID",
                "project_id must only contain alphanumeric characters, hyphens, or underscores",
                Some("project_id"),
            ));
        }

        // 2. Environment Validation
        let env_norm = plan.environment.trim().to_lowercase();
        if !ALLOWED_ENVIRONMENTS.contains(&env_norm.as_str()) {
            errors.push(PlanValidationError::new(
                "UNSUPPORTED_ENVIRONMENT",
                format!(
                    "environment '{}' is not recognized; allowed: {:?}",
                    plan.environment, ALLOWED_ENVIRONMENTS
                ),
                Some("environment"),
            ));
        }

        // 3. Plan Name Validation
        if plan.plan_name.trim().is_empty() {
            errors.push(PlanValidationError::new(
                "EMPTY_PLAN_NAME",
                "plan_name must not be empty",
                Some("plan_name"),
            ));
        }

        // 4. Concurrency Budget
        let concurrency = plan.concurrency.clamp(1, max_concurrency);
        if plan.concurrency > max_concurrency {
            errors.push(PlanValidationError::new(
                "CONCURRENCY_EXCEEDED",
                format!(
                    "requested concurrency {} exceeds allowed budget {}",
                    plan.concurrency, max_concurrency
                ),
                Some("concurrency"),
            ));
        }

        // 5. Tasks Count Validation
        if plan.tasks.is_empty() {
            errors.push(PlanValidationError::new(
                "EMPTY_TASKS",
                "plan contains zero tasks; at least one task is required",
                Some("tasks"),
            ));
        } else if plan.tasks.len() > max_tasks {
            errors.push(PlanValidationError::new(
                "TASK_BUDGET_EXCEEDED",
                format!(
                    "task count {} exceeds maximum allowed budget {}",
                    plan.tasks.len(),
                    max_tasks
                ),
                Some("tasks"),
            ));
        }

        // 6. Individual Task & Action AST Validation
        let mut seen_ids = HashSet::new();
        let mut task_id_set = HashSet::new();
        let mut task_prereqs = HashMap::new();

        for (idx, task) in plan.tasks.iter().enumerate() {
            let task_field = format!("tasks[{idx}]");
            let tid = task.id.trim();

            if tid.is_empty() {
                errors.push(PlanValidationError::new(
                    "EMPTY_TASK_ID",
                    "task id must not be empty",
                    Some(format!("{task_field}.id")),
                ));
            } else if !seen_ids.insert(tid.to_string()) {
                errors.push(PlanValidationError::new(
                    "DUPLICATE_TASK_ID",
                    format!("duplicate task id '{tid}' encountered"),
                    Some(format!("{task_field}.id")),
                ));
            } else {
                task_id_set.insert(tid.to_string());
                task_prereqs.insert(tid.to_string(), task.prerequisites.clone());
            }

            if task.name.trim().is_empty() {
                errors.push(PlanValidationError::new(
                    "EMPTY_TASK_NAME",
                    "task name must not be empty",
                    Some(format!("{task_field}.name")),
                ));
            }

            // Executor validation
            let exec_lower = task.executor.trim().to_lowercase();
            if !ALLOWED_EXECUTORS.contains(&exec_lower.as_str()) {
                errors.push(PlanValidationError::new(
                    "UNSUPPORTED_EXECUTOR",
                    format!(
                        "executor '{}' is not permitted; allowed closed vocabulary: {:?}",
                        task.executor, ALLOWED_EXECUTORS
                    ),
                    Some(format!("{task_field}.executor")),
                ));
            }

            // Reject forbidden arbitrary code executors
            let forbidden = [
                "bash",
                "sh",
                "shell",
                "cmd",
                "powershell",
                "python",
                "py",
                "node",
                "eval",
            ];
            if forbidden.contains(&exec_lower.as_str()) {
                errors.push(PlanValidationError::new(
                    "FORBIDDEN_EXECUTOR",
                    format!(
                        "arbitrary execution engine '{}' is strictly prohibited",
                        task.executor
                    ),
                    Some(format!("{task_field}.executor")),
                ));
            }

            // Timeout validation
            if let Some(t_secs) = task.timeout_secs {
                if t_secs > max_task_timeout_secs {
                    errors.push(PlanValidationError::new(
                        "TIMEOUT_BUDGET_EXCEEDED",
                        format!(
                            "task timeout {}s exceeds maximum budget {}s",
                            t_secs, max_task_timeout_secs
                        ),
                        Some(format!("{task_field}.timeout_secs")),
                    ));
                }
            }

            // Target URL SSRF validation
            if let Some(ref target_url) = task.target_url {
                if let Err(reason) = crate::executor::validate_target_url(target_url).await {
                    errors.push(PlanValidationError::new(
                        "SSRF_TARGET_VIOLATION",
                        format!("target_url '{target_url}' failed SSRF security guard: {reason}"),
                        Some(format!("{task_field}.target_url")),
                    ));
                }
            }

            // Action AST Validation
            for (a_idx, action) in task.actions.iter().enumerate() {
                let action_field = format!("{task_field}.actions[{a_idx}]");
                match action {
                    Action::Navigate { url } => {
                        if let Err(reason) = crate::executor::validate_target_url(url).await {
                            errors.push(PlanValidationError::new(
                                "SSRF_ACTION_VIOLATION",
                                format!(
                                    "NAVIGATE url '{url}' failed SSRF security guard: {reason}"
                                ),
                                Some(action_field),
                            ));
                        }
                    }
                    Action::Request { url, .. } => {
                        if let Err(reason) = crate::executor::validate_target_url(url).await {
                            errors.push(PlanValidationError::new(
                                "SSRF_ACTION_VIOLATION",
                                format!("REQUEST url '{url}' failed SSRF security guard: {reason}"),
                                Some(action_field),
                            ));
                        }
                    }
                    Action::Click { selector }
                    | Action::AssertVisible { selector }
                    | Action::AssertHidden { selector }
                    | Action::AssertEnabled { selector } => {
                        if let Err(err) = validate_selector_string(selector) {
                            errors.push(PlanValidationError::new(
                                "MALFORMED_SELECTOR",
                                err,
                                Some(action_field),
                            ));
                        }
                    }
                    Action::Fill { selector, value } => {
                        if let Err(err) = validate_selector_string(selector) {
                            errors.push(PlanValidationError::new(
                                "MALFORMED_SELECTOR",
                                err,
                                Some(action_field.clone()),
                            ));
                        }
                        if let Err(err) = validate_input_value(value) {
                            errors.push(PlanValidationError::new(
                                "UNSAFE_INPUT_VALUE",
                                err,
                                Some(action_field),
                            ));
                        }
                    }
                    Action::AssertText { selector, .. } => {
                        if let Err(err) = validate_selector_string(selector) {
                            errors.push(PlanValidationError::new(
                                "MALFORMED_SELECTOR",
                                err,
                                Some(action_field),
                            ));
                        }
                    }
                    Action::PressKey { key } => {
                        // The key name is model-supplied and reaches
                        // `page.keyboard.press(...)` verbatim, so it is checked
                        // against the real Playwright key names rather than only
                        // for length. A `key` of "Control+Shift+Delete" or a
                        // shell fragment is not a key.
                        if !key_name_is_real(key) {
                            errors.push(PlanValidationError::new(
                                "UNKNOWN_KEY_NAME",
                                format!(
                                    "PRESS_KEY '{key}' is not one of the {} supported key names ({})",
                                    PRESS_KEY_NAMES.len(),
                                    PRESS_KEY_NAMES.join(", ")
                                ),
                                Some(action_field),
                            ));
                        }
                    }
                    Action::AssertUrl { .. }
                    | Action::AssertStatus { .. }
                    | Action::AssertHeader { .. }
                    | Action::AssertBodyField { .. }
                    | Action::AssertStatusIn { .. }
                    | Action::AssertBodySchema { .. } => {}
                }
            }
        }

        // 7. Dependency Graph Validation (DAG integrity & cycle detection)
        for (task_id, prereqs) in &task_prereqs {
            for prereq in prereqs {
                if prereq == task_id {
                    errors.push(PlanValidationError::new(
                        "SELF_DEPENDENCY",
                        format!("task '{task_id}' cannot depend on itself"),
                        Some(format!("task.{task_id}.prerequisites")),
                    ));
                } else if !task_id_set.contains(prereq) {
                    errors.push(PlanValidationError::new(
                        "UNKNOWN_DEPENDENCY",
                        format!("task '{task_id}' depends on unknown task '{prereq}'"),
                        Some(format!("task.{task_id}.prerequisites")),
                    ));
                }
            }
        }

        // Cycle detection using Kahn's topological sort
        if let Err(cycle_desc) = detect_dag_cycles(&task_id_set, &task_prereqs) {
            errors.push(PlanValidationError::new(
                "CYCLIC_DEPENDENCIES",
                format!("dependency cycle detected: {cycle_desc}"),
                Some("tasks.prerequisites"),
            ));
        }

        if errors.is_empty() {
            Ok(ValidatedPlan {
                project_id: project_id.to_string(),
                environment: env_norm,
                plan_name: plan.plan_name.trim().to_string(),
                concurrency,
                tasks: plan.tasks.clone(),
                metadata: plan.metadata.clone(),
            })
        } else {
            Err(errors)
        }
    }
}

/// The key names `PRESS_KEY` accepts: one unmodified key, by its Playwright
/// name. Deliberately single-key, so a chord like "Shift+Tab" is not accepted
/// -- a model that means two keys should emit two `PRESS_KEY` actions, which
/// keeps each step independently meaningful in the run log.
const PRESS_KEY_NAMES: &[&str] = &[
    "Enter", "Tab", "Escape", "Backspace", "Delete", "Space", "Shift", "Control", "Alt", "Meta",
    "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown",
    "Insert", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

/// Whether `key` names one real key, i.e. something `page.keyboard.press` will
/// accept. The value is model-supplied and reaches Playwright verbatim, so this
/// is an allow-list and not a denylist: an unknown spelling is refused rather
/// than passed through to be discovered as a runtime failure inside the
/// browser.
pub fn key_name_is_real(key: &str) -> bool {
    PRESS_KEY_NAMES.contains(&key)
}

/// Checks CSS selectors to prevent script injection or dangerous directives.
fn validate_selector_string(selector: &str) -> Result<(), String> {
    let lower = selector.to_lowercase();
    let dangerous = [
        "<script",
        "javascript:",
        "eval(",
        "exec(",
        "__proto__",
        "document.cookie",
    ];
    for pat in &dangerous {
        if lower.contains(pat) {
            return Err(format!("selector contains dangerous pattern '{pat}'"));
        }
    }
    if selector.len() > 512 {
        return Err("selector exceeds maximum allowed length of 512 characters".to_string());
    }
    Ok(())
}

/// Checks input fill values to prevent obvious command injection patterns.
fn validate_input_value(value: &str) -> Result<(), String> {
    let lower = value.to_lowercase();
    let dangerous = ["<script", "javascript:", "; rm -rf", "| nc ", "&& cat /etc"];
    for pat in &dangerous {
        if lower.contains(pat) {
            return Err(format!("input value contains dangerous pattern '{pat}'"));
        }
    }
    Ok(())
}

/// Kahn's algorithm for cycle detection in task dependencies.
fn detect_dag_cycles(
    task_ids: &HashSet<String>,
    prereqs: &HashMap<String, Vec<String>>,
) -> Result<(), String> {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    for id in task_ids {
        in_degree.insert(id.clone(), 0);
        adj.insert(id.clone(), Vec::new());
    }

    for (node, deps) in prereqs {
        for dep in deps {
            if task_ids.contains(dep) {
                adj.entry(dep.clone()).or_default().push(node.clone());
                *in_degree.entry(node.clone()).or_insert(0) += 1;
            }
        }
    }

    let mut queue = VecDeque::new();
    for (id, &deg) in &in_degree {
        if deg == 0 {
            queue.push_back(id.clone());
        }
    }

    let mut visited_count = 0;
    while let Some(u) = queue.pop_front() {
        visited_count += 1;
        if let Some(neighbors) = adj.get(&u) {
            for v in neighbors {
                if let Some(deg) = in_degree.get_mut(v) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(v.clone());
                    }
                }
            }
        }
    }

    if visited_count == task_ids.len() {
        Ok(())
    } else {
        Err(format!(
            "graph has cycle: only {visited_count} of {} tasks can be scheduled",
            task_ids.len()
        ))
    }
}
