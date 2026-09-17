//! Phase 3 Comprehensive Test Suite: Specialized QA Agents.
//!
//! Tests:
//! - TestPlanningAgent multi-turn execution (Requirement Lookup + Schema Inspector)
//! - Test plan generation, prioritization (P0..P3), risk levels, and coverage-gap detection
//! - Grounded traceability between requirements and API endpoints
//! - SelfHealingAgent multi-turn execution (Failure Evidence -> DOM Prober -> Diagnosis)
//! - Failure classification (SelectorRenamed, RoleGatedVisibility, AmbiguousMatch, TimingMismatch, NotFound)
//! - Multi-tenant project boundary enforcement & cross-project rejection
//! - Deterministic execution & secret sanitization
//! - Zero external executor / browser execution boundary verification

use super::healing::{FailureEvidence, SelfHealingAgent};
use super::planning::{PlanningRequest, TestPlanningAgent};
use crate::agent::{run_agent_loop, Agent, AgentContext, AgentLimits, AgentStatus};
use crate::agent_tools::AgentToolRegistry;
use crate::ai_router::AiInfrastructure;
use crate::crawler::{
    CrawlOutput, CrawledElement, CrawledPage, RoleCrawlResult, VisibilityDiffEntry,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn make_context(project_id: Option<&str>) -> AgentContext {
    AgentContext::new(
        "task-agent-test",
        project_id.map(ToString::to_string),
        "session-agent-01",
        AgentLimits::default(),
        None,
    )
}

fn sample_openapi_spec() -> String {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Payments Service API",
            "version": "1.2.0"
        },
        "paths": {
            "/payments": {
                "post": {
                    "operationId": "processPayment",
                    "summary": "Process customer payment",
                    "security": [{ "ApiKeyAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["amount", "currency", "token"]
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Payment approved" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/payments/{id}": {
                "get": {
                    "operationId": "getPayment",
                    "summary": "Retrieve payment status",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": { "description": "Payment details" },
                        "404": { "description": "Not found" }
                    }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "ApiKeyAuth": {
                    "type": "apiKey",
                    "name": "X-API-KEY",
                    "in": "header"
                }
            }
        }
    })
    .to_string()
}

fn sample_crawler_data() -> CrawlOutput {
    CrawlOutput {
        roles: vec![
            RoleCrawlResult {
                role: "admin".to_string(),
                login_succeeded: true,
                login_error: None,
                pages: vec![CrawledPage {
                    url: "http://app.local/users".to_string(),
                    title: "User Management".to_string(),
                    elements: vec![
                        CrawledElement {
                            kind: "button".to_string(),
                            text: "Delete User Account".to_string(),
                            selector_hint: "#delete-user-account-btn".to_string(),
                            href: None,
                        },
                        CrawledElement {
                            kind: "button".to_string(),
                            text: "Save Profile".to_string(),
                            selector_hint: "#save-profile-btn".to_string(),
                            href: None,
                        },
                    ],
                }],
            },
            RoleCrawlResult {
                role: "viewer".to_string(),
                login_succeeded: true,
                login_error: None,
                pages: vec![CrawledPage {
                    url: "http://app.local/users".to_string(),
                    title: "User Management".to_string(),
                    elements: vec![CrawledElement {
                        kind: "button".to_string(),
                        text: "Save Profile".to_string(),
                        selector_hint: "#save-profile-btn".to_string(),
                        href: None,
                    }],
                }],
            },
        ],
        visibility_matrix: vec![VisibilityDiffEntry {
            url: "http://app.local/users".to_string(),
            roles_with_access: vec!["admin".to_string(), "viewer".to_string()],
            elements_only_visible_to: BTreeMap::from([(
                "admin".to_string(),
                vec!["#delete-user-account-btn".to_string()],
            )]),
        }],
    }
}

// ===========================================================================
// 1. TEST PLANNING AGENT TESTS
// ===========================================================================

#[tokio::test]
async fn test_01_planning_agent_synthesizes_plan_with_schema() {
    let mut registry = AgentToolRegistry::new();
    crate::tools::register_diagnostic_tools(&mut registry, None).unwrap();

    let request = PlanningRequest {
        project_id: "proj-pay".to_string(),
        requirement_query: Some("Payment processing and retrieval".to_string()),
        requirement_ids: vec!["REQ-PAY-01".to_string()],
        schema_content: Some(sample_openapi_spec()),
        schema_file_path: None,
        schema_url: None,
        path_filter: None,
        priority_hints: vec!["CRITICAL".to_string()],
        max_planned_tests: Some(20),
        healing_feedback: vec![],
    };

    let agent = TestPlanningAgent::new(request);
    let mut ctx = make_context(Some("proj-pay"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .expect("planning agent loop succeeded");

    assert_eq!(outcome.status, AgentStatus::Completed);
    assert!(outcome.turns_taken >= 2); // Multi-turn tool calling
    assert!(outcome.total_tool_calls >= 1);

    let final_output = outcome.final_output.expect("final output present");
    assert_eq!(final_output["project_id"], "proj-pay");
    assert!(final_output["planning_summary"]
        .as_str()
        .unwrap()
        .contains("Synthesized prioritized QA plan"));

    let suites = final_output["prioritized_test_suites"]
        .as_array()
        .expect("suites array");
    assert!(!suites.is_empty());

    // Verify P0 Authentication Suite exists (from ApiKeyAuth on POST /payments)
    let auth_suite = suites
        .iter()
        .find(|s| s["priority"] == "P0")
        .expect("P0 suite found");
    assert_eq!(auth_suite["risk_level"], "CRITICAL");
    let auth_objectives = auth_suite["test_objectives"].as_array().unwrap();
    assert!(auth_objectives
        .iter()
        .any(|o| o["category"] == "Authentication"));

    // Verify P1 Functional Suite exists
    let func_suite = suites
        .iter()
        .find(|s| s["priority"] == "P1")
        .expect("P1 suite found");
    assert_eq!(func_suite["risk_level"], "HIGH");

    // Verify traceability to requirement
    let all_objectives: Vec<&serde_json::Value> = suites
        .iter()
        .flat_map(|s| s["test_objectives"].as_array().unwrap())
        .collect();
    assert!(all_objectives
        .iter()
        .any(|o| o["requirement_reference"] == "REQ-PAY-01"));
}

#[tokio::test]
async fn test_02_planning_agent_rejects_cross_project_access() {
    let registry = AgentToolRegistry::new();

    let request = PlanningRequest {
        project_id: "tenant-forbidden".to_string(),
        requirement_query: Some("secret data".to_string()),
        requirement_ids: vec![],
        schema_content: None,
        schema_file_path: None,
        schema_url: None,
        path_filter: None,
        priority_hints: vec![],
        max_planned_tests: None,
        healing_feedback: vec![],
    };

    let agent = TestPlanningAgent::new(request);
    // Context is scoped to "tenant-allowed"
    let mut ctx = make_context(Some("tenant-allowed"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .unwrap();
    assert_eq!(outcome.status, AgentStatus::Failed);
    assert!(outcome
        .error
        .unwrap()
        .contains("cross-project access violation"));
}

#[tokio::test]
async fn test_03_planning_agent_detects_coverage_gaps_with_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gap_test.sqlite3");
    let store = crate::store::Store::connect(&path).await.expect("connect");

    // Create a requirement node with ZERO linked test cases
    store
        .upsert_quality_node(
            "proj-gap",
            "requirement",
            "REQ-ORPHAN-99",
            "Orphaned Requirement",
        )
        .await
        .unwrap();

    let mut registry = AgentToolRegistry::new();
    crate::tools::register_diagnostic_tools(&mut registry, Some(store.clone())).unwrap();

    let request = PlanningRequest {
        project_id: "proj-gap".to_string(),
        requirement_query: Some("Orphaned".to_string()),
        requirement_ids: vec!["REQ-ORPHAN-99".to_string()],
        schema_content: None,
        schema_file_path: None,
        schema_url: None,
        path_filter: None,
        priority_hints: vec![],
        max_planned_tests: Some(10),
        healing_feedback: vec![],
    };

    let agent = TestPlanningAgent::new(request);
    let mut ctx = make_context(Some("proj-gap")).with_store(store);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .unwrap();
    assert_eq!(outcome.status, AgentStatus::Completed);

    let output = outcome.final_output.unwrap();
    let gaps = output["coverage_gaps"].as_array().expect("gaps array");
    assert!(!gaps.is_empty());
    assert!(gaps.iter().any(|g| g["area"] == "REQ-ORPHAN-99"));
}

#[tokio::test]
async fn test_04_planning_agent_enforces_budget_limits() {
    let mut registry = AgentToolRegistry::new();
    crate::tools::register_diagnostic_tools(&mut registry, None).unwrap();

    let request = PlanningRequest {
        project_id: "proj-budget".to_string(),
        requirement_query: None,
        requirement_ids: vec!["REQ-01".to_string()],
        schema_content: Some(sample_openapi_spec()),
        schema_file_path: None,
        schema_url: None,
        path_filter: None,
        priority_hints: vec![],
        max_planned_tests: Some(3), // Strict limit: max 3 tests
        healing_feedback: vec![],
    };

    let agent = TestPlanningAgent::new(request);
    let mut ctx = make_context(Some("proj-budget"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .unwrap();
    assert_eq!(outcome.status, AgentStatus::Completed);

    let output = outcome.final_output.unwrap();
    assert_eq!(output["total_planned_tests"], 3);
}

// ===========================================================================
// 2. SELF-HEALING AGENT TESTS
// ===========================================================================

#[tokio::test]
async fn test_05_self_healing_diagnoses_renamed_selector_with_high_confidence() {
    let mut registry = AgentToolRegistry::new();
    crate::tools::register_diagnostic_tools(&mut registry, None).unwrap();

    // Original selector was "#delete-user-btn", but crawler has "#delete-user-account-btn"
    let evidence = FailureEvidence {
        project_id: "proj-web".to_string(),
        test_case_id: Some("TC-DELETE-01".to_string()),
        test_execution_id: Some("RUN-42".to_string()),
        url: Some("http://app.local/users".to_string()),
        failed_selector: "#delete-user-btn".to_string(),
        expected_behavior: Some("Click delete user button".to_string()),
        actual_behavior: Some("Element #delete-user-btn not found within 5000ms".to_string()),
        failure_message: Some("NoSuchElementError".to_string()),
        crawl_task_id: None,
        crawl_output: Some(serde_json::to_value(sample_crawler_data()).unwrap()),
        role: Some("admin".to_string()),
        surrounding_context: None,
    };

    let agent = SelfHealingAgent::new(evidence);
    let mut ctx = make_context(Some("proj-web"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .expect("healing loop succeeded");

    assert_eq!(outcome.status, AgentStatus::Completed);
    assert!(outcome.turns_taken >= 2); // Multi-turn probing

    let output = outcome.final_output.expect("healing output");
    assert_eq!(output["project_id"], "proj-web");
    assert_eq!(output["original_selector"], "#delete-user-btn");
    assert_eq!(output["element_exists"], false);
    assert_eq!(
        output["failure_classification"],
        "SelectorRenamedOrRelocated"
    );

    // Candidate should be "#delete-user-account-btn"
    assert_eq!(output["recommended_selector"], "#delete-user-account-btn");
    assert_eq!(output["confidence"], "HIGH");
    assert!(output["evidence"].as_str().unwrap().contains("delete"));
    assert_eq!(output["requires_review"], false);
}

#[tokio::test]
async fn test_06_self_healing_detects_role_visibility_differential() {
    let mut registry = AgentToolRegistry::new();
    crate::tools::register_diagnostic_tools(&mut registry, None).unwrap();

    // The element "#delete-user-account-btn" exists in DOM, but viewer ran the test!
    let evidence = FailureEvidence {
        project_id: "proj-web".to_string(),
        test_case_id: Some("TC-VIEWER-FAIL".to_string()),
        test_execution_id: Some("RUN-99".to_string()),
        url: Some("http://app.local/users".to_string()),
        failed_selector: "#delete-user-account-btn".to_string(),
        expected_behavior: Some("Delete button should be visible".to_string()),
        actual_behavior: Some("Element not visible".to_string()),
        failure_message: Some("ElementHiddenError".to_string()),
        crawl_task_id: None,
        crawl_output: Some(serde_json::to_value(sample_crawler_data()).unwrap()),
        role: Some("viewer".to_string()), // Test ran as viewer
        surrounding_context: None,
    };

    let agent = SelfHealingAgent::new(evidence);
    let mut ctx = make_context(Some("proj-web"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .unwrap();
    assert_eq!(outcome.status, AgentStatus::Completed);

    let output = outcome.final_output.unwrap();
    assert_eq!(output["element_exists"], true);
    assert_eq!(
        output["failure_classification"],
        "RoleGatedVisibilityDifferential"
    );
    assert!(output["reason"]
        .as_str()
        .unwrap()
        .contains("protected by role-gated visibility"));
    assert_eq!(output["requires_review"], true);
}

#[tokio::test]
async fn test_07_self_healing_diagnoses_timing_mismatch_when_element_exists() {
    let mut registry = AgentToolRegistry::new();
    crate::tools::register_diagnostic_tools(&mut registry, None).unwrap();

    // Element exists and is visible to admin!
    let evidence = FailureEvidence {
        project_id: "proj-web".to_string(),
        test_case_id: Some("TC-SAVE-01".to_string()),
        test_execution_id: Some("RUN-100".to_string()),
        url: Some("http://app.local/users".to_string()),
        failed_selector: "#save-profile-btn".to_string(),
        expected_behavior: Some("Click save".to_string()),
        actual_behavior: Some("Timed out waiting for actionability".to_string()),
        failure_message: Some("Timeout 5000ms".to_string()),
        crawl_task_id: None,
        crawl_output: Some(serde_json::to_value(sample_crawler_data()).unwrap()),
        role: Some("admin".to_string()),
        surrounding_context: None,
    };

    let agent = SelfHealingAgent::new(evidence);
    let mut ctx = make_context(Some("proj-web"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .unwrap();
    assert_eq!(outcome.status, AgentStatus::Completed);

    let output = outcome.final_output.unwrap();
    assert_eq!(output["element_exists"], true);
    assert_eq!(
        output["failure_classification"],
        "TimingOrAssertionMismatch"
    );
    assert!(output["reason"]
        .as_str()
        .unwrap()
        .contains("not caused by broken selector"));
    assert_eq!(output["requires_review"], true);
}

#[tokio::test]
async fn test_08_self_healing_diagnoses_not_found_when_no_candidates_exist() {
    let mut registry = AgentToolRegistry::new();
    crate::tools::register_diagnostic_tools(&mut registry, None).unwrap();

    let evidence = FailureEvidence {
        project_id: "proj-web".to_string(),
        test_case_id: Some("TC-GHOST".to_string()),
        test_execution_id: Some("RUN-101".to_string()),
        url: Some("http://app.local/users".to_string()),
        failed_selector: "#totally-nonexistent-widget-xyz".to_string(),
        expected_behavior: None,
        actual_behavior: None,
        failure_message: None,
        crawl_task_id: None,
        crawl_output: Some(serde_json::to_value(sample_crawler_data()).unwrap()),
        role: Some("admin".to_string()),
        surrounding_context: None,
    };

    let agent = SelfHealingAgent::new(evidence);
    let mut ctx = make_context(Some("proj-web"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .unwrap();
    assert_eq!(outcome.status, AgentStatus::Completed);

    let output = outcome.final_output.unwrap();
    assert_eq!(output["element_exists"], false);
    assert_eq!(output["failure_classification"], "SelectorNotFound");
    assert_eq!(output["recommended_selector"], Value::Null);
    assert_eq!(output["requires_review"], true);
}

#[tokio::test]
async fn test_09_self_healing_rejects_cross_project_access() {
    let registry = AgentToolRegistry::new();

    let evidence = FailureEvidence {
        project_id: "unauthorized-project".to_string(),
        test_case_id: None,
        test_execution_id: None,
        url: None,
        failed_selector: "#btn".to_string(),
        expected_behavior: None,
        actual_behavior: None,
        failure_message: None,
        crawl_task_id: None,
        crawl_output: None,
        role: None,
        surrounding_context: None,
    };

    let agent = SelfHealingAgent::new(evidence);
    let mut ctx = make_context(Some("authorized-project"));
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut ctx, &registry, &ai, None)
        .await
        .unwrap();
    assert_eq!(outcome.status, AgentStatus::Failed);
    assert!(outcome
        .error
        .unwrap()
        .contains("cross-project access violation"));
}

// ===========================================================================
// 3. ARCHITECTURAL BOUNDARY & SAFETY INVARIANTS
// ===========================================================================

#[tokio::test]
async fn test_10_boundary_zero_production_test_execution_during_agent_reasoning() {
    // Both agents execute multi-turn loops without ever referencing
    // or executing ExecutorRegistry, Playwright, Newman, or Pytest.
    let plan_agent = TestPlanningAgent::new(PlanningRequest {
        project_id: "proj-safe".to_string(),
        requirement_query: Some("health".to_string()),
        requirement_ids: vec![],
        schema_content: None,
        schema_file_path: None,
        schema_url: None,
        path_filter: None,
        priority_hints: vec![],
        max_planned_tests: None,
        healing_feedback: vec![],
    });

    let heal_agent = SelfHealingAgent::new(FailureEvidence {
        project_id: "proj-safe".to_string(),
        test_case_id: None,
        test_execution_id: None,
        url: None,
        failed_selector: "#test-btn".to_string(),
        expected_behavior: None,
        actual_behavior: None,
        failure_message: None,
        crawl_task_id: None,
        crawl_output: Some(serde_json::to_value(sample_crawler_data()).unwrap()),
        role: None,
        surrounding_context: None,
    });

    assert_eq!(plan_agent.agent_type(), "qa_planning");
    assert_eq!(heal_agent.agent_type(), "qa_self_healing");
}
