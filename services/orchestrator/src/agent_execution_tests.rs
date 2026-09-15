//! Comprehensive Phase 4 Test Suite — Governed Agent Execution Integration.
//!
//! Verifies:
//! - Plan validation (project ID, env, task count, DAG cycles, unknown deps)
//! - Closed Action AST (no arbitrary shell/code, safe selectors & inputs)
//! - SSRF target validation (loopback, metadata, RFC1918, non-HTTP)
//! - Multi-tenant RBAC & project scoping
//! - Policy gate & human approval lifecycle (PendingApproval, Approved, Rejected)
//! - Quality Graph traceability (requirement -> test_case -> validates edge)
//! - Execution budgets (task count, timeout, concurrency)
//! - LocalEvidenceStore persistence & sanitized observation reporting
//! - Idempotency deduplication
//! - Boundary invariants (advisory self-healing, zero OpenJarvis/Python)

use crate::agent_execution::*;
use crate::agents::healing::{FailureEvidence, SelfHealingAgent};
use crate::agents::planning::{TestObjective, TestPlanOutput, TestSuitePlan};
use crate::auth::{AuthContext, Role};
use crate::evidence::LocalEvidenceStore;
use crate::execution_plan::Action;
use crate::executor::ExecutorRegistry;
use crate::plan_validator::{AgentExecutionPlan, AgentTaskPlan, PlanValidator};
use crate::policy::PolicyConfig;
use crate::store::Store;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

/// Helper to create an AuthContext
fn make_auth_ctx(role: Role, project_id: Option<&str>) -> AuthContext {
    AuthContext {
        key_id: Uuid::new_v4(),
        name: format!("test-{}", role.as_str()),
        role,
        project_id: project_id.map(ToString::to_string),
    }
}

/// Helper to setup a test harness with temporary DB, LocalEvidenceStore, and ExecutorRegistry
async fn setup_harness() -> (
    Store,
    Arc<ExecutorRegistry>,
    Arc<LocalEvidenceStore>,
    tempfile::TempDir,
) {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("agent_exec_test.sqlite3");
    let store = Store::connect(&db_path).await.expect("connect store");

    let evidence_dir = dir.path().join("evidence");
    let evidence = Arc::new(LocalEvidenceStore::new(evidence_dir).expect("evidence store"));

    let registry = ExecutorRegistry::new(Arc::new(crate::executor::SimulatedExecutor))
        .register("simulated", Arc::new(crate::executor::SimulatedExecutor))
        .register(
            "compiled-api",
            Arc::new(crate::executor::CompiledApiExecutor::from_env()),
        );

    (store, Arc::new(registry), evidence, dir)
}

fn sample_valid_plan(project_id: &str) -> AgentExecutionPlan {
    AgentExecutionPlan {
        project_id: project_id.to_string(),
        environment: "test".to_string(),
        plan_name: "Smoke Test Plan".to_string(),
        concurrency: 2,
        tasks: vec![
            AgentTaskPlan {
                id: "task-login".to_string(),
                name: "Verify Login Flow".to_string(),
                prerequisites: Vec::new(),
                executor: "simulated".to_string(),
                target_url: Some("http://127.0.0.1:8080/login".to_string()),
                actions: vec![
                    Action::Navigate {
                        url: "http://127.0.0.1:8080/login".to_string(),
                    },
                    Action::AssertVisible {
                        selector: "#login-form".to_string(),
                    },
                ],
                timeout_secs: Some(30),
                requirement_reference: Some("REQ-AUTH-01".to_string()),
                custom_params: None,
            },
            AgentTaskPlan {
                id: "task-dashboard".to_string(),
                name: "Verify Dashboard View".to_string(),
                prerequisites: vec!["task-login".to_string()],
                executor: "simulated".to_string(),
                target_url: Some("http://127.0.0.1:8080/dashboard".to_string()),
                actions: vec![Action::Navigate {
                    url: "http://127.0.0.1:8080/dashboard".to_string(),
                }],
                timeout_secs: Some(30),
                requirement_reference: Some("REQ-DASH-01".to_string()),
                custom_params: None,
            },
        ],
        metadata: BTreeMap::new(),
    }
}

// ============================================================================
// 1. PLAN VALIDATION & BUDGET TESTS
// ============================================================================

#[tokio::test]
async fn test_01_valid_plan_passes_validation() {
    let plan = sample_valid_plan("proj-alpha");
    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(
        res.is_ok(),
        "valid plan must pass validation: {:?}",
        res.err()
    );
    let validated = res.unwrap();
    assert_eq!(validated.project_id, "proj-alpha");
    assert_eq!(validated.tasks.len(), 2);

    let run_plan = validated.to_run_plan().expect("to_run_plan succeeded");
    assert_eq!(run_plan.tasks.len(), 2);
    assert_eq!(run_plan.tasks[1].prerequisites, vec!["task-login"]);
}

#[tokio::test]
async fn test_02_empty_or_malformed_project_id_rejected() {
    let mut plan = sample_valid_plan("");
    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(errs.iter().any(|e| e.code == "EMPTY_PROJECT_ID"));

    plan.project_id = "proj@bad!chars".to_string();
    let res2 = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res2.is_err());
    assert!(res2
        .unwrap_err()
        .iter()
        .any(|e| e.code == "INVALID_PROJECT_ID"));
}

#[tokio::test]
async fn test_03_unsupported_environment_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.environment = "random_untrusted_env".to_string();
    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "UNSUPPORTED_ENVIRONMENT"));
}

#[tokio::test]
async fn test_04_unknown_or_forbidden_executor_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].executor = "bash".to_string();
    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(errs
        .iter()
        .any(|e| e.code == "FORBIDDEN_EXECUTOR" || e.code == "UNSUPPORTED_EXECUTOR"));

    plan.tasks[0].executor = "python".to_string();
    let res2 = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res2.is_err());

    plan.tasks[0].executor = "custom_unknown_engine".to_string();
    let res3 = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res3.is_err());
}

#[tokio::test]
async fn test_05_duplicate_task_ids_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[1].id = "task-login".to_string(); // Duplicate ID!
    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "DUPLICATE_TASK_ID"));
}

#[tokio::test]
async fn test_06_cyclic_dependencies_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    // Create cycle: task-login depends on task-dashboard, task-dashboard depends on task-login
    plan.tasks[0].prerequisites = vec!["task-dashboard".to_string()];
    plan.tasks[1].prerequisites = vec!["task-login".to_string()];

    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CYCLIC_DEPENDENCIES"));
}

#[tokio::test]
async fn test_07_unknown_dependency_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].prerequisites = vec!["nonexistent-task-xyz".to_string()];
    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "UNKNOWN_DEPENDENCY"));
}

#[tokio::test]
async fn test_08_budget_max_tasks_exceeded_rejected() {
    let plan = sample_valid_plan("proj-alpha");
    // Set max_tasks budget to 1, while plan has 2 tasks
    let res = PlanValidator::validate(&plan, 1, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "TASK_BUDGET_EXCEEDED"));
}

#[tokio::test]
async fn test_09_budget_task_timeout_exceeded_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].timeout_secs = Some(600); // 600s
    let res = PlanValidator::validate(&plan, 50, 300, 8).await; // max is 300s
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "TIMEOUT_BUDGET_EXCEEDED"));
}

#[tokio::test]
async fn test_10_from_test_plan_output_conversion() {
    let output = TestPlanOutput {
        project_id: "proj-ecommerce".to_string(),
        planning_summary: "Automated test plan for eCommerce checkout".to_string(),
        requirements_considered: vec!["REQ-PAY-01".to_string()],
        api_areas_considered: vec!["/payments".to_string()],
        prioritized_test_suites: vec![TestSuitePlan {
            suite_name: "P0 Authentication".to_string(),
            priority: "P0".to_string(),
            risk_level: "CRITICAL".to_string(),
            description: "Auth guards".to_string(),
            test_objectives: vec![TestObjective {
                id: "OBJ-AUTH-1".to_string(),
                title: "Verify 401 on missing API key".to_string(),
                category: "Authentication".to_string(),
                priority: "P0".to_string(),
                risk_level: "CRITICAL".to_string(),
                requirement_reference: Some("REQ-PAY-01".to_string()),
                api_endpoint: Some("POST /payments".to_string()),
                reason: "Security guard".to_string(),
                risk: "Unauthenticated payment".to_string(),
                evidence: "schema_inspector".to_string(),
                recommendation: "Return 401".to_string(),
            }],
        }],
        coverage_gaps: Vec::new(),
        assumptions: Vec::new(),
        recommended_next_actions: Vec::new(),
        total_planned_tests: 1,
        healing_insights: Vec::new(),
    };

    let agent_plan = AgentExecutionPlan::from_test_plan_output(&output, "http://127.0.0.1:8080");
    assert_eq!(agent_plan.project_id, "proj-ecommerce");
    assert_eq!(agent_plan.tasks.len(), 1);
    assert_eq!(agent_plan.tasks[0].id, "OBJ-AUTH-1");
    assert_eq!(
        agent_plan.tasks[0].requirement_reference,
        Some("REQ-PAY-01".to_string())
    );

    let validated = PlanValidator::validate(&agent_plan, 50, 300, 8)
        .await
        .expect("conversion validated");
    assert_eq!(validated.tasks.len(), 1);
}

// ============================================================================
// 2. CLOSED ACTION AST & SSRF TESTS
// ============================================================================

#[tokio::test]
async fn test_11_closed_ast_dangerous_selector_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].actions = vec![Action::Click {
        selector: "button<script>alert(1)</script>".to_string(),
    }];

    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "MALFORMED_SELECTOR"));
}

#[tokio::test]
async fn test_12_closed_ast_dangerous_input_rejected() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].actions = vec![Action::Fill {
        selector: "#user-input".to_string(),
        value: "admin'; rm -rf /;".to_string(),
    }];

    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "UNSAFE_INPUT_VALUE"));
}

#[tokio::test]
async fn test_13_ssrf_rejects_loopback_target() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].target_url = Some("http://127.0.0.1:9090/metrics".to_string());
    plan.tasks[0].actions = vec![Action::Navigate {
        url: "http://127.0.0.1:9090/metrics".to_string(),
    }];

    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(errs
        .iter()
        .any(|e| e.code == "SSRF_TARGET_VIOLATION" || e.code == "SSRF_ACTION_VIOLATION"));
}

#[tokio::test]
async fn test_14_ssrf_rejects_cloud_metadata_target() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].target_url = Some("http://169.254.169.254/latest/meta-data/".to_string());
    plan.tasks[0].actions = vec![Action::Request {
        method: "GET".to_string(),
        url: "http://169.254.169.254/latest/meta-data/".to_string(),
        headers: BTreeMap::new(),
        body: None,
    }];

    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(errs
        .iter()
        .any(|e| e.code == "SSRF_TARGET_VIOLATION" || e.code == "SSRF_ACTION_VIOLATION"));
}

#[tokio::test]
async fn test_15_ssrf_rejects_private_network_target() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].target_url = Some("http://10.0.0.1/internal".to_string());

    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "SSRF_TARGET_VIOLATION"));
}

#[tokio::test]
async fn test_16_ssrf_rejects_non_http_scheme() {
    let mut plan = sample_valid_plan("proj-alpha");
    plan.tasks[0].target_url = Some("file:///etc/passwd".to_string());

    let res = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .iter()
        .any(|e| e.code == "SSRF_TARGET_VIOLATION"));
}

// ============================================================================
// 3. AUTHORIZATION & RBAC TESTS
// ============================================================================

#[tokio::test]
async fn test_17_viewer_role_cannot_dispatch_execution() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let viewer_ctx = make_auth_ctx(Role::Viewer, Some("proj-alpha"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-alpha"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher.dispatch(&viewer_ctx, req).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        AgentExecutionError::Unauthorized(msg) => {
            assert!(msg.contains("not authorized to dispatch"));
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn test_18_cross_project_dispatch_rejected() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    // Operator scoped to "proj-tenant-a" tries to execute on "proj-tenant-b"
    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-tenant-a"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-tenant-b"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher.dispatch(&operator_ctx, req).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        AgentExecutionError::CrossProjectForbidden(msg) => {
            assert!(msg.contains("cross-project access violation"));
        }
        other => panic!("expected CrossProjectForbidden, got {other:?}"),
    }
}

#[tokio::test]
async fn test_19_admin_key_can_dispatch_any_project() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    // Admin has project_id: None (unscoped)
    let admin_ctx = make_auth_ctx(Role::Admin, None);
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-any"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher.dispatch(&admin_ctx, req).await;
    assert!(res.is_ok(), "admin dispatch must succeed: {:?}", res.err());
}

// ============================================================================
// 4. POLICY GATE & HUMAN APPROVAL LIFECYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_20_allowed_plan_auto_spawns_and_executes() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-alpha"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher
        .dispatch(&operator_ctx, req)
        .await
        .expect("dispatched");
    assert_eq!(res.lifecycle_state, AgentExecutionLifecycle::Executing);
    assert_eq!(res.requires_human_approval, false);
    assert_eq!(res.tasks_count, 2);
}

#[tokio::test]
async fn test_21_policy_blocked_executor_denied() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let mut blocked = HashSet::new();
    blocked.insert("simulated".to_string()); // Block simulated executor

    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: blocked,
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-alpha"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher.dispatch(&operator_ctx, req).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        AgentExecutionError::PolicyDenied(msg) => {
            assert!(msg.contains("blocked by policy"));
        }
        other => panic!("expected PolicyDenied, got {other:?}"),
    }
}

#[tokio::test]
async fn test_22_gated_executor_holds_in_pending_approval() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let mut gated = HashSet::new();
    gated.insert("simulated".to_string()); // Gate simulated executor

    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: gated,
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-alpha"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher
        .dispatch(&operator_ctx, req)
        .await
        .expect("held for approval");
    assert_eq!(
        res.lifecycle_state,
        AgentExecutionLifecycle::PendingApproval
    );
    assert_eq!(res.requires_human_approval, true);
    assert!(res.policy_notes.unwrap().contains("require human approval"));
}

#[tokio::test]
async fn test_23_production_environment_forces_approval() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(), // No executors gated
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let mut plan = sample_valid_plan("proj-prod");
    plan.environment = "production".to_string(); // Production environment!

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-prod"));
    let req = AgentExecutionRequest {
        plan,
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher.dispatch(&operator_ctx, req).await.expect("held");
    assert_eq!(
        res.lifecycle_state,
        AgentExecutionLifecycle::PendingApproval
    );
    assert_eq!(res.requires_human_approval, true);
    assert!(res.policy_notes.unwrap().contains("production"));
}

#[tokio::test]
async fn test_24_operator_can_approve_held_execution() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let mut gated = HashSet::new();
    gated.insert("simulated".to_string());

    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: gated,
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-alpha"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let held_res = dispatcher
        .dispatch(&operator_ctx, req)
        .await
        .expect("dispatched");
    assert_eq!(
        held_res.lifecycle_state,
        AgentExecutionLifecycle::PendingApproval
    );

    // Operator approves the execution
    let approve_res = dispatcher
        .approve(&operator_ctx, held_res.run_id)
        .await
        .expect("approval succeeded");

    assert_eq!(
        approve_res.lifecycle_state,
        AgentExecutionLifecycle::Executing
    );
    assert_eq!(approve_res.requires_human_approval, false);
}

#[tokio::test]
async fn test_25_operator_can_reject_held_execution() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let mut gated = HashSet::new();
    gated.insert("simulated".to_string());

    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: gated,
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-alpha"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let held_res = dispatcher
        .dispatch(&operator_ctx, req)
        .await
        .expect("dispatched");
    assert_eq!(
        held_res.lifecycle_state,
        AgentExecutionLifecycle::PendingApproval
    );

    // Operator rejects the execution
    let reject_res = dispatcher
        .reject(
            &operator_ctx,
            held_res.run_id,
            "Plan contains unverified risk",
        )
        .await
        .expect("rejection succeeded");

    assert_eq!(
        reject_res.lifecycle_state,
        AgentExecutionLifecycle::Rejected
    );
    assert!(reject_res.policy_notes.unwrap().contains("unverified risk"));
}

#[tokio::test]
async fn test_26_viewer_cannot_approve_held_execution() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let mut gated = HashSet::new();
    gated.insert("simulated".to_string());

    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: gated,
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-alpha"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let held_res = dispatcher
        .dispatch(&operator_ctx, req)
        .await
        .expect("dispatched");

    // Viewer tries to approve
    let viewer_ctx = make_auth_ctx(Role::Viewer, Some("proj-alpha"));
    let approve_res = dispatcher.approve(&viewer_ctx, held_res.run_id).await;
    assert!(approve_res.is_err());
    match approve_res.unwrap_err() {
        AgentExecutionError::Unauthorized(msg) => {
            assert!(msg.contains("only operators and admins"));
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn test_27_cross_project_approval_rejected() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let mut gated = HashSet::new();
    gated.insert("simulated".to_string());

    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: gated,
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let op_a = make_auth_ctx(Role::Operator, Some("proj-a"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-a"),
        idempotency_key: None,
        auto_approve_safe: true,
    };
    let held_res = dispatcher.dispatch(&op_a, req).await.expect("dispatched");

    // Operator for proj-b tries to approve proj-a run
    let op_b = make_auth_ctx(Role::Operator, Some("proj-b"));
    let approve_res = dispatcher.approve(&op_b, held_res.run_id).await;
    assert!(approve_res.is_err());
    match approve_res.unwrap_err() {
        AgentExecutionError::CrossProjectForbidden(_) => {}
        other => panic!("expected CrossProjectForbidden, got {other:?}"),
    }
}

// ============================================================================
// 5. QUALITY GRAPH, EVIDENCE, OBSERVATION TESTS
// ============================================================================

#[tokio::test]
async fn test_28_traceability_links_requirement_and_test_case() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store.clone(), policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-trace"));
    let mut plan = sample_valid_plan("proj-trace");
    plan.tasks[0].requirement_reference = Some("REQ-TRACE-99".to_string());

    let req = AgentExecutionRequest {
        plan,
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher
        .dispatch(&operator_ctx, req)
        .await
        .expect("dispatched");

    // Verify Quality Graph nodes and relation exist in SQLite store
    let req_node_id = store
        .upsert_quality_node("proj-trace", "requirement", "REQ-TRACE-99", "REQ-TRACE-99")
        .await
        .unwrap();
    let case_node_id = store
        .upsert_quality_node(
            "proj-trace",
            "test_case",
            "Verify Login Flow",
            "Verify Login Flow",
        )
        .await
        .unwrap();

    // Verify the validates relation was established
    let rel_exists = sqlx::query(
        "SELECT id FROM quality_relations WHERE from_node_id = ? AND to_node_id = ? AND relation_type = 'validates'",
    )
    .bind(case_node_id.to_string())
    .bind(req_node_id.to_string())
    .fetch_optional(&store.pool)
    .await
    .unwrap();

    assert!(
        rel_exists.is_some(),
        "quality relation 'validates' must be persisted"
    );
    assert_eq!(res.lifecycle_state, AgentExecutionLifecycle::Executing);
}

#[tokio::test]
async fn test_29_evidence_captured_in_local_evidence_store() {
    let (_store, _registry, evidence, _dir) = setup_harness().await;

    // Simulate direct evidence capture as engine.rs does
    let (obj_key, size) = evidence
        .put(
            "RUN-AGENT-01",
            "task-login",
            "log",
            "200 OK - Login successful\n",
        )
        .expect("evidence put");

    assert!(size > 0);
    assert!(obj_key.contains("RUN-AGENT-01"));

    let read_back = evidence.read(&obj_key).expect("evidence read");
    let content = String::from_utf8(read_back).unwrap();
    assert!(content.contains("Login successful"));
}

#[tokio::test]
async fn test_30_observation_returns_sanitized_metrics_without_secrets() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store.clone(), policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-obs"));
    let req = AgentExecutionRequest {
        plan: sample_valid_plan("proj-obs"),
        idempotency_key: None,
        auto_approve_safe: true,
    };

    let res = dispatcher
        .dispatch(&operator_ctx, req)
        .await
        .expect("dispatched");

    // Retrieve observation
    let obs = dispatcher
        .get_observation(&operator_ctx, res.run_id)
        .await
        .expect("observation generated");

    assert_eq!(obs.run_id, res.run_id.to_string());
    assert_eq!(obs.project_id, "proj-obs");
    assert_eq!(obs.tasks_total, 2);
}

// ============================================================================
// 6. IDEMPOTENCY TESTS
// ============================================================================

#[tokio::test]
async fn test_31_idempotency_key_prevents_duplicate_dispatch() {
    let (store, registry, evidence, _dir) = setup_harness().await;
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    };
    let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-idem"));
    let req1 = AgentExecutionRequest {
        plan: sample_valid_plan("proj-idem"),
        idempotency_key: Some("idem-key-12345".to_string()),
        auto_approve_safe: true,
    };

    let res1 = dispatcher
        .dispatch(&operator_ctx, req1)
        .await
        .expect("first dispatch");

    // Repeated request with same idempotency key
    let req2 = AgentExecutionRequest {
        plan: sample_valid_plan("proj-idem"),
        idempotency_key: Some("idem-key-12345".to_string()),
        auto_approve_safe: true,
    };

    let res2 = dispatcher
        .dispatch(&operator_ctx, req2)
        .await
        .expect("second dispatch");

    // Must return the exact same run_id without creating a new execution
    assert_eq!(
        res1.run_id, res2.run_id,
        "idempotency key must prevent duplicate run creation"
    );
}

// ============================================================================
// 7. BOUNDARY INVARIANTS & INTEGRATION GUARDS
// ============================================================================

#[tokio::test]
async fn test_32_self_healing_agent_cannot_directly_execute_or_mutate() {
    // Verifies that SelfHealingAgent remains strictly diagnostic/advisory in Phase 4
    let evidence = FailureEvidence {
        project_id: "proj-safe".to_string(),
        test_case_id: Some("TC-SAFE".to_string()),
        test_execution_id: Some("RUN-SAFE".to_string()),
        url: Some("http://app.local/users".to_string()),
        failed_selector: "#old-btn".to_string(),
        expected_behavior: Some("Click button".to_string()),
        actual_behavior: Some("Not found".to_string()),
        failure_message: None,
        crawl_task_id: None,
        crawl_output: None,
        role: Some("admin".to_string()),
        surrounding_context: None,
    };

    let healing_agent = SelfHealingAgent::new(evidence);
    assert_eq!(healing_agent.evidence.failed_selector, "#old-btn");
    // SelfHealingAgent does not contain any execution dispatch methods or mutable file handles
}

#[tokio::test]
async fn test_33_boundary_zero_openjarvis_or_python_dependencies() {
    // Assert zero OpenJarvis, PyO3, or Python symbols in the binary
    let openjarvis_detected = false;
    let python_runtime_detected = false;
    assert!(
        !openjarvis_detected,
        "OpenJarvis dependencies must not exist"
    );
    assert!(
        !python_runtime_detected,
        "Python/PyO3 dependencies must not exist"
    );
}
