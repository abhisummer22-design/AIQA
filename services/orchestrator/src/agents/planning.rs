//! Specialized Agent 1: Test Planning Agent (`TestPlanningAgent`).
//!
//! Evaluates project requirements and API schemas to produce a structured, prioritized
//! QA test plan with risk classifications, category mapping, and coverage-gap analysis
//! before any test generation or execution.
//!
//! Architectural Guarantees:
//! - Multi-turn reasoning leveraging `requirement_lookup` and `schema_inspector`.
//! - Strict multi-tenant project boundary checking (`context.is_authorized_for_project`).
//! - Prioritized test suites (P0..P3) with grounded requirement and endpoint traceability.
//! - Non-executing: strictly generates plans, never invokes real executors.
//! - Zero private chain-of-thought leakage; structured operational telemetry only.

use crate::agent::{Agent, AgentContext, AgentError, TurnDecision};
use crate::agent_tools::{AgentToolRegistry, ToolInvocation};
use crate::ai_router::AiInfrastructure;
use crate::tools::security::sanitize_secrets;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const AGENT_ID: &str = "test_planning_agent";
const AGENT_NAME: &str = "Test Planning Agent";
const AGENT_TYPE: &str = "qa_planning";

/// Input contract requesting a QA test plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningRequest {
    pub project_id: String,
    #[serde(default)]
    pub requirement_query: Option<String>,
    #[serde(default)]
    pub requirement_ids: Vec<String>,
    #[serde(default)]
    pub schema_content: Option<String>,
    #[serde(default)]
    pub schema_file_path: Option<String>,
    #[serde(default)]
    pub schema_url: Option<String>,
    #[serde(default)]
    pub path_filter: Option<String>,
    #[serde(default)]
    pub priority_hints: Vec<String>,
    #[serde(default)]
    pub max_planned_tests: Option<usize>,
    #[serde(default)]
    pub healing_feedback: Vec<crate::governed_healing::HealingFeedback>,
}

/// A specific, testable QA objective within a planned suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestObjective {
    pub id: String,
    pub title: String,
    pub category: String,
    pub priority: String,   // P0, P1, P2, P3
    pub risk_level: String, // CRITICAL, HIGH, MEDIUM, LOW
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_endpoint: Option<String>,
    pub reason: String,
    pub risk: String,
    pub evidence: String,
    pub recommendation: String,
}

/// A cohesive test suite grouping related test objectives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuitePlan {
    pub suite_name: String,
    pub priority: String,
    pub risk_level: String,
    pub description: String,
    pub test_objectives: Vec<TestObjective>,
}

/// A missing requirement or untested API surface identified during planning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGap {
    pub area: String,
    pub gap_description: String,
    pub risk: String,
    pub recommendation: String,
}

/// Comprehensive, structured QA test plan output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestPlanOutput {
    pub project_id: String,
    pub planning_summary: String,
    pub requirements_considered: Vec<String>,
    pub api_areas_considered: Vec<String>,
    pub prioritized_test_suites: Vec<TestSuitePlan>,
    pub coverage_gaps: Vec<CoverageGap>,
    pub assumptions: Vec<String>,
    pub recommended_next_actions: Vec<String>,
    pub total_planned_tests: usize,
    #[serde(default)]
    pub healing_insights: Vec<String>,
}

/// Test Planning Agent implementation.
#[derive(Debug, Clone)]
pub struct TestPlanningAgent {
    pub request: PlanningRequest,
}

impl TestPlanningAgent {
    pub fn new(request: PlanningRequest) -> Self {
        Self { request }
    }
}

#[async_trait]
impl Agent for TestPlanningAgent {
    fn id(&self) -> &str {
        AGENT_ID
    }

    fn name(&self) -> &str {
        AGENT_NAME
    }

    fn agent_type(&self) -> &str {
        AGENT_TYPE
    }

    async fn step(
        &self,
        context: &AgentContext,
        tools: &AgentToolRegistry,
        _ai: &AiInfrastructure,
    ) -> Result<TurnDecision, AgentError> {
        // 1. Cooperative cancellation check
        if context.is_cancelled() {
            return Err(AgentError::Cancelled(
                "planning cancelled by caller".to_string(),
            ));
        }

        // 2. Multi-tenant Authorization & Project Scope Enforcement
        if !context.is_authorized_for_project(&self.request.project_id) {
            return Ok(TurnDecision::Fail {
                reason: format!(
                    "cross-project access violation: agent execution context is unauthorized for project '{}'",
                    self.request.project_id
                ),
            });
        }

        // 3. Inspect context turn history to determine which tools have executed
        let mut req_obs: Option<&Value> = None;
        let mut schema_obs: Option<&Value> = None;

        for record in &context.history {
            for obs in &record.tool_observations {
                if obs.tool_name == "requirement_lookup" {
                    if let Ok(ref val) = obs.output {
                        req_obs = Some(val);
                    }
                } else if obs.tool_name == "schema_inspector" {
                    if let Ok(ref val) = obs.output {
                        schema_obs = Some(val);
                    }
                }
            }
        }

        // Turn Phase A: Need requirement lookup?
        let has_req_query =
            self.request.requirement_query.is_some() || !self.request.requirement_ids.is_empty();

        if req_obs.is_none() && has_req_query && tools.has_tool("requirement_lookup") {
            let (query_type, req_id, keyword) =
                if let Some(first_id) = self.request.requirement_ids.first() {
                    ("id", Some(first_id.clone()), None)
                } else {
                    ("keyword", None, self.request.requirement_query.clone())
                };

            return Ok(TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: format!("req-lookup-{}", context.current_turn),
                    tool_name: "requirement_lookup".to_string(),
                    arguments: json!({
                        "project_id": self.request.project_id,
                        "query_type": query_type,
                        "requirement_id": req_id,
                        "keyword": keyword,
                        "include_test_cases": true,
                        "limit": 20
                    }),
                }],
            });
        }

        // Turn Phase B: Need schema inspection?
        let has_schema = self.request.schema_content.is_some()
            || self.request.schema_file_path.is_some()
            || self.request.schema_url.is_some();

        if schema_obs.is_none() && has_schema && tools.has_tool("schema_inspector") {
            return Ok(TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: format!("schema-inspect-{}", context.current_turn),
                    tool_name: "schema_inspector".to_string(),
                    arguments: json!({
                        "schema_content": self.request.schema_content,
                        "file_path": self.request.schema_file_path,
                        "url": self.request.schema_url,
                        "path_filter": self.request.path_filter,
                        "max_paths": 50
                    }),
                }],
            });
        }

        // Turn Phase C: Synthesize final test plan from gathered diagnostic observations
        let plan = synthesize_test_plan(&self.request, req_obs, schema_obs);

        let mut plan_json = serde_json::to_value(&plan).map_err(|e| {
            AgentError::InvalidOutput(format!("failed to serialize test plan output: {e}"))
        })?;

        // Sanitize secrets before completion
        sanitize_secrets(&mut plan_json);

        Ok(TurnDecision::Complete { output: plan_json })
    }
}

/// Deterministically synthesizes a prioritized test plan from diagnostic tool observations.
fn synthesize_test_plan(
    req: &PlanningRequest,
    req_obs: Option<&Value>,
    schema_obs: Option<&Value>,
) -> TestPlanOutput {
    let mut requirements_considered = Vec::new();
    let mut api_areas_considered = Vec::new();
    let mut suites: Vec<TestSuitePlan> = Vec::new();
    let mut coverage_gaps: Vec<CoverageGap> = Vec::new();
    let max_tests = req.max_planned_tests.unwrap_or(25).clamp(1, 100);

    // 1. Process Requirements Evidence
    let mut req_map: BTreeMap<String, String> = BTreeMap::new();
    for id in &req.requirement_ids {
        if !requirements_considered.contains(id) {
            requirements_considered.push(id.clone());
            req_map.insert(id.clone(), format!("Requirement {id}"));
        }
    }
    if let Some(ref q) = req.requirement_query {
        if requirements_considered.is_empty() {
            requirements_considered.push(q.clone());
            req_map.insert(q.clone(), q.clone());
        }
    }
    if let Some(obs) = req_obs {
        if let Some(req_arr) = obs.get("requirements").and_then(|r| r.as_array()) {
            for item in req_arr {
                let id = item
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("REQ-UNKNOWN");
                let title = item.get("title").and_then(|t| t.as_str()).unwrap_or(id);
                if !requirements_considered.contains(&id.to_string()) {
                    requirements_considered.push(id.to_string());
                }
                req_map.insert(id.to_string(), title.to_string());

                // Check existing test case coverage
                let cases = item.get("related_test_cases").and_then(|c| c.as_array());
                if cases.map_or(true, |c| c.is_empty()) {
                    coverage_gaps.push(CoverageGap {
                        area: id.to_string(),
                        gap_description: format!(
                            "Requirement '{title}' has zero associated test cases in Quality Graph"
                        ),
                        risk: "HIGH".to_string(),
                        recommendation: "Create dedicated functional verification test suite"
                            .to_string(),
                    });
                }
            }
        }
    }

    // 2. Process Schema Evidence & Construct Objectives
    let mut auth_objectives = Vec::new();
    let mut functional_objectives = Vec::new();
    let mut validation_objectives = Vec::new();
    let mut edge_case_objectives = Vec::new();

    let primary_req_ref = requirements_considered.first().cloned();

    if let Some(obs) = schema_obs {
        if let Some(paths) = obs.get("paths").and_then(|p| p.as_array()) {
            for path_item in paths {
                let path = path_item.get("path").and_then(|p| p.as_str()).unwrap_or("");
                api_areas_considered.push(path.to_string());

                if let Some(ops) = path_item.get("operations").and_then(|o| o.as_array()) {
                    for op in ops {
                        let method = op.get("method").and_then(|m| m.as_str()).unwrap_or("GET");
                        let endpoint_label = format!("{method} {path}");
                        let op_id = op.get("operation_id").and_then(|o| o.as_str());
                        let auth_req = op
                            .get("auth_required")
                            .and_then(|a| a.as_bool())
                            .unwrap_or(false);

                        // P0: Authentication & Security Objectives
                        if auth_req {
                            auth_objectives.push(TestObjective {
                                id: format!("OBJ-AUTH-{}", auth_objectives.len() + 1),
                                title: format!("Verify Authentication Guard on {endpoint_label}"),
                                category: "Authentication".to_string(),
                                priority: "P0".to_string(),
                                risk_level: "CRITICAL".to_string(),
                                requirement_reference: primary_req_ref.clone(),
                                api_endpoint: Some(endpoint_label.clone()),
                                reason: format!("Endpoint {endpoint_label} enforces authentication requirements"),
                                risk: "Unauthenticated access may expose protected data or mutate state".to_string(),
                                evidence: "schema_inspector: security schemes specified for operation".to_string(),
                                recommendation: "Verify 401 Unauthorized on missing, expired, and malformed credentials".to_string(),
                            });
                        }

                        // P1: Functional Contract Verification
                        functional_objectives.push(TestObjective {
                            id: format!("OBJ-FUNC-{}", functional_objectives.len() + 1),
                            title: format!("Verify Successful Execution of {endpoint_label}"),
                            category: "Functional".to_string(),
                            priority: "P1".to_string(),
                            risk_level: "HIGH".to_string(),
                            requirement_reference: primary_req_ref.clone(),
                            api_endpoint: Some(endpoint_label.clone()),
                            reason: format!("Core operation {operation} must satisfy API contract", operation = op_id.unwrap_or(&endpoint_label)),
                            risk: "Contract breaking changes will break upstream and consumer clients".to_string(),
                            evidence: "schema_inspector: valid operation definition present".to_string(),
                            recommendation: "Test happy path payload producing expected 2xx response status and schema".to_string(),
                        });

                        // P2: Validation & Parameter Boundaries
                        let params = op.get("parameters").and_then(|p| p.as_array());
                        let has_body = op.get("request_body").is_some();

                        if has_body || params.map_or(false, |p| !p.is_empty()) {
                            validation_objectives.push(TestObjective {
                                id: format!("OBJ-VAL-{}", validation_objectives.len() + 1),
                                title: format!("Verify Input Validation & Bad Request Handling on {endpoint_label}"),
                                category: "Validation".to_string(),
                                priority: "P2".to_string(),
                                risk_level: "MEDIUM".to_string(),
                                requirement_reference: primary_req_ref.clone(),
                                api_endpoint: Some(endpoint_label.clone()),
                                reason: "Enforces input integrity against invalid parameter formats and required fields".to_string(),
                                risk: "Unvalidated input causes 500 internal server errors or corrupted records".to_string(),
                                evidence: "schema_inspector: required parameters and body schema defined".to_string(),
                                recommendation: "Verify 400 Bad Request on missing required fields and boundary overflow values".to_string(),
                            });
                        }

                        // P3: Edge Cases & Error Resilience
                        edge_case_objectives.push(TestObjective {
                            id: format!("OBJ-EDGE-{}", edge_case_objectives.len() + 1),
                            title: format!("Verify Error Resilience & Idempotency on {endpoint_label}"),
                            category: "ErrorHandling".to_string(),
                            priority: "P3".to_string(),
                            risk_level: "LOW".to_string(),
                            requirement_reference: primary_req_ref.clone(),
                            api_endpoint: Some(endpoint_label.clone()),
                            reason: "Resilience check for network timeouts, repeated calls, and edge inputs".to_string(),
                            risk: "Non-idempotent retries cause duplicate state mutations".to_string(),
                            evidence: "schema_inspector: HTTP method semantics".to_string(),
                            recommendation: "Test repeated calls and non-existent resource IDs (404 Not Found)".to_string(),
                        });
                    }
                }
            }
        }
    } else {
        // Requirements-only planning (when no schema was supplied)
        for (id, title) in &req_map {
            functional_objectives.push(TestObjective {
                id: format!("OBJ-FUNC-{}", functional_objectives.len() + 1),
                title: format!("Verify Functional Requirement: {title}"),
                category: "Functional".to_string(),
                priority: "P1".to_string(),
                risk_level: "HIGH".to_string(),
                requirement_reference: Some(id.clone()),
                api_endpoint: None,
                reason: format!(
                    "Primary requirement '{title}' must be validated against business criteria"
                ),
                risk: "Business requirement unmet by application implementation".to_string(),
                evidence: format!(
                    "requirement_lookup: requirement record '{id}' present in Quality Graph"
                ),
                recommendation:
                    "Generate end-to-end user journey tests covering acceptance criteria"
                        .to_string(),
            });

            validation_objectives.push(TestObjective {
                id: format!("OBJ-VAL-{}", validation_objectives.len() + 1),
                title: format!("Verify Negative Boundaries for {id}"),
                category: "Boundary".to_string(),
                priority: "P2".to_string(),
                risk_level: "MEDIUM".to_string(),
                requirement_reference: Some(id.clone()),
                api_endpoint: None,
                reason:
                    "Negative scenarios ensure graceful failure on unauthorized or invalid actions"
                        .to_string(),
                risk: "Application crashes or exposes unhandled exceptions on invalid user input"
                    .to_string(),
                evidence: format!("requirement_lookup: requirement '{id}'"),
                recommendation:
                    "Test invalid formats, empty submissions, and unauthorized role attempts"
                        .to_string(),
            });
        }
    }

    // 3. Assemble and Prioritize Test Suites
    if !auth_objectives.is_empty() {
        suites.push(TestSuitePlan {
            suite_name: "Authentication & Security Governance Suite".to_string(),
            priority: "P0".to_string(),
            risk_level: "CRITICAL".to_string(),
            description:
                "Covers authentication gates, token validation, and unauthenticated attack surface."
                    .to_string(),
            test_objectives: auth_objectives,
        });
    }

    if !functional_objectives.is_empty() {
        suites.push(TestSuitePlan {
            suite_name: "Core Functional & Contract Verification Suite".to_string(),
            priority: "P1".to_string(),
            risk_level: "HIGH".to_string(),
            description: "Validates primary business workflows and API contracts against documented specifications.".to_string(),
            test_objectives: functional_objectives,
        });
    }

    if !validation_objectives.is_empty() {
        suites.push(TestSuitePlan {
            suite_name: "Validation & Boundary Robustness Suite".to_string(),
            priority: "P2".to_string(),
            risk_level: "MEDIUM".to_string(),
            description: "Verifies input boundary handling, required parameter constraints, and rejection of malformed requests.".to_string(),
            test_objectives: validation_objectives,
        });
    }

    if !edge_case_objectives.is_empty() {
        suites.push(TestSuitePlan {
            suite_name: "Error Resilience & Edge Scenarios Suite".to_string(),
            priority: "P3".to_string(),
            risk_level: "LOW".to_string(),
            description: "Tests recovery from non-existent resources, repeated idempotent requests, and edge error paths.".to_string(),
            test_objectives: edge_case_objectives,
        });
    }

    // 4. Enforce max_planned_tests budget across suites
    let mut total_count = 0;
    for suite in &mut suites {
        if total_count >= max_tests {
            suite.test_objectives.clear();
        } else {
            let available = max_tests - total_count;
            suite.test_objectives.truncate(available);
            total_count += suite.test_objectives.len();
        }
    }
    suites.retain(|s| !s.test_objectives.is_empty());

    let summary = format!(
        "Synthesized prioritized QA plan for project '{}': {} test suites, {} test objectives across {} requirements and {} API endpoints.",
        req.project_id,
        suites.len(),
        total_count,
        requirements_considered.len(),
        api_areas_considered.len()
    );

    let mut healing_insights = Vec::new();
    let mut assumptions = vec![
        "Application endpoints are deployed and accessible in the target environment".to_string(),
        "Authentication service issues standard JWT bearer credentials".to_string(),
    ];

    for fb in &req.healing_feedback {
        healing_insights.push(fb.operational_insight.clone());
        assumptions.push(format!(
            "Quality Feedback: Strategy '{}' for category '{}' has {:.0}% calibrated reliability ({} total attempts).",
            fb.healing_strategy, fb.diagnosis_category, fb.confidence * 100.0, fb.historical_success_count + fb.historical_failure_count
        ));
    }

    TestPlanOutput {
        project_id: req.project_id.clone(),
        planning_summary: summary,
        requirements_considered,
        api_areas_considered,
        prioritized_test_suites: suites,
        coverage_gaps,
        assumptions,
        recommended_next_actions: vec![
            "Review P0 and P1 test objectives with engineering lead".to_string(),
            "Proceed to test case compilation for approved suites".to_string(),
        ],
        total_planned_tests: total_count,
        healing_insights,
    }
}
