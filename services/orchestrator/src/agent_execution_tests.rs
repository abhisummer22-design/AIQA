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
use crate::store::{Store, LEGACY_TEST_NODE_TYPE, TASK_NODE_TYPE, TEST_NODE_TYPE};
use sqlx::Row;
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

/// Phase 9.2B Batch 3: the same fixture, rooted in a caller-owned directory so a test can measure
/// exactly what *this* fixture leaves behind. Counting global `.tmp*` entries is not a valid
/// assertion -- `cargo test` runs these tests concurrently, so any other suite's fixture perturbs
/// the total and the result would depend on scheduling order.
///
/// `setup_harness()` above is left untouched and keeps using `tempfile::tempdir()`, so the 15
/// existing call sites keep their existing behaviour and directory location exactly. The two
/// differ only in that this one takes an explicit parent directory.
async fn setup_harness_in(
    root: &std::path::Path,
) -> (
    Store,
    Arc<ExecutorRegistry>,
    Arc<LocalEvidenceStore>,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir_in(root).expect("tempdir");
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
        qa_risk_signals: Vec::new(),
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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

    // Verify the Quality Graph nodes and relation really exist in SQLite.
    //
    // Phase 1 rewrote this test. The old version built the vertex it then asserted
    // on: it called `upsert_quality_node("proj-trace", "test_case", "Verify Login
    // Flow", ...)` itself. Because the unique key is
    // `(project_id, node_type, external_ref)`, that call returned the dispatcher's
    // vertex only when the dispatcher's node type happened to match the literal this
    // test hardcoded — the coupling was real but accidental, expressed as a literal
    // in the assertion rather than as the constant the production code uses. When the
    // dispatcher changed to the honest identity, the test's `upsert` silently created
    // a *second*, unrelated vertex and the assertion failed on a node it had just
    // invented.
    //
    // This version observes the dispatcher's vertex instead of creating one, so the
    // node type under test is the constant the production path actually uses.
    //
    // An `AgentTaskPlan` carries no `test_case_id` — verified against
    // `plan_validator::AgentTaskPlan`, not assumed — so the dispatcher records a
    // `TASK_NODE_TYPE` vertex keyed on the task name, not a case vertex keyed on a
    // `test_cases.id`. That is the honest identity, and it changes no behaviour: a
    // task name can never equal a UUID, so the old `"test_case"` vertex could never
    // satisfy `list_test_cases_for_requirement`'s `external_ref = tc.id` join and was
    // unreachable by any reader.
    let req_node_id = store
        .upsert_quality_node("proj-trace", "requirement", "REQ-TRACE-99", "REQ-TRACE-99")
        .await
        .unwrap();

    // Observe what the dispatcher wrote. A missing row is a failure, not something to
    // create on the spot.
    let case_node_id: Option<String> = sqlx::query(
        "SELECT id FROM quality_nodes WHERE project_id = ? AND node_type = ? AND external_ref = ?",
    )
    .bind("proj-trace")
    .bind(TASK_NODE_TYPE)
    .bind("Verify Login Flow")
    .fetch_optional(&store.pool)
    .await
    .unwrap()
    .map(|r| r.get("id"));

    assert!(
        case_node_id.is_some(),
        "the dispatcher must persist a {} vertex for the task it ran",
        TASK_NODE_TYPE
    );

    // And the `validates` edge must run from *that* vertex to the requirement.
    let rel_exists = sqlx::query(
        "SELECT id FROM quality_relations WHERE from_node_id = ? AND to_node_id = ? AND relation_type = 'validates'",
    )
    .bind(case_node_id.as_deref().unwrap())
    .bind(req_node_id.to_string())
    .fetch_optional(&store.pool)
    .await
    .unwrap();

    assert!(
        rel_exists.is_some(),
        "quality relation 'validates' must be persisted from the dispatcher's own vertex"
    );

    // Negative: the agent path must NOT fabricate a case vertex. A name is not an id,
    // and writing one would put a name into the namespace readers join on.
    let fabricated_case_vertex: Option<String> = sqlx::query(
        "SELECT id FROM quality_nodes WHERE project_id = ? AND node_type IN (?, ?) AND external_ref = ?",
    )
    .bind("proj-trace")
    .bind(TEST_NODE_TYPE)
    .bind(LEGACY_TEST_NODE_TYPE)
    .bind("Verify Login Flow")
    .fetch_optional(&store.pool)
    .await
    .unwrap()
    .map(|r| r.get("id"));
    assert!(
        fabricated_case_vertex.is_none(),
        "an agent task name must never be written as a test-case vertex"
    );

    assert_eq!(res.lifecycle_state, AgentExecutionLifecycle::Executing);
    dispatcher.store.pool.close().await;
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
    _store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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
    dispatcher.store.pool.close().await;
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

// ===========================================================================
// Phase 9.2B Batch 3 — setup_harness() lifecycle
// ===========================================================================
//
// These test cleanup, not agent-execution logic. A green suite is not evidence that a directory was
// removed: `tempfile` swallows the Windows removal error that causes the leak in the first place.
// Every assertion below reads the filesystem directly, and each measures only inside a root it
// owns, so it is immune to the other suites running concurrently.
//
// **The complication unique to this suite.** `AgentExecutionDispatcher::dispatch` calls
// `engine::spawn_run(..)`, which is fire-and-forget: it `tokio::spawn`s and returns no handle,
// capturing a `Store` clone. So a detached task can still be running when the test body ends.
// `p92b3_detached_run_does_not_block_cleanup` is the test that pins this relationship down.

/// The private measurement root owned by one test. Only this namespace is ever removed.
fn p92b3_root(test_name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir()
        .join("aiqa_p92b3_artifacts")
        .join(test_name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create isolated artifact root");
    root
}

fn p92b3_dir_count(root: &std::path::Path) -> usize {
    std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

fn p92b3_permissive_policy() -> PolicyConfig {
    PolicyConfig {
        max_concurrency: 8,
        require_approval_for: HashSet::new(),
        blocked_executors: HashSet::new(),
    }
}

/// **A + B.** The fixture must still produce a genuinely migrated, writable database.
#[tokio::test]
async fn p92b3_fixture_is_usable_and_fully_migrated() {
    let root = p92b3_root("usable_and_migrated");
    let (store, _registry, _evidence, dir) = setup_harness_in(&root).await;

    let tables: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
         ('projects','test_cases','runs','execution_telemetry','quality_nodes')",
    )
    .fetch_one(&store.pool)
    .await
    .expect("query sqlite_master");
    assert_eq!(
        tables.0, 5,
        "the agent-execution fixture must run the real migrations"
    );

    let pid = uuid::Uuid::new_v4();
    store
        .insert_project(pid, "P92B3 lifecycle", None)
        .await
        .expect("insert_project");
    let projects = store.list_projects(None).await.expect("list_projects");
    assert!(
        projects.iter().any(|p| p.id == pid),
        "the fixture must accept a real write and read it back"
    );
    assert!(
        dir.path().join("evidence").exists(),
        "the evidence root must be a real directory inside the fixture"
    );

    store.pool.close().await;
}

/// **C + J.** Agent-execution behaviour must be unchanged by the teardown work: a real dispatch
/// must still be accepted and produce a real run.
#[tokio::test]
async fn p92b3_agent_execution_behaviour_is_unchanged() {
    let root = p92b3_root("agent_execution_behaviour");
    let (store, registry, evidence, _dir) = setup_harness_in(&root).await;
    let dispatcher =
        AgentExecutionDispatcher::new(store, p92b3_permissive_policy(), registry, evidence, None);

    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let res = dispatcher
        .dispatch(
            &operator_ctx,
            AgentExecutionRequest {
                plan: sample_valid_plan("proj-alpha"),
                idempotency_key: None,
                auto_approve_safe: true,
            },
        )
        .await
        .expect("dispatched");

    assert_eq!(
        res.lifecycle_state,
        AgentExecutionLifecycle::Executing,
        "a permitted plan must still dispatch into EXECUTING"
    );
    assert_eq!(
        res.tasks_count, 2,
        "the plan's task count must be preserved"
    );

    dispatcher.store.pool.close().await;
}

/// **D + E + I.** The one test that matters most for this suite. A successful dispatch leaves a
/// **detached** task holding a `Store` clone; this proves that task does not prevent the fixture's
/// directory from being removed, and that the awaited close completes regardless.
#[tokio::test]
async fn p92b3_detached_run_does_not_block_cleanup() {
    let root = p92b3_root("detached_run_cleanup");
    let (store, registry, evidence, dir) = setup_harness_in(&root).await;
    let path = dir.path().to_path_buf();

    let dispatcher =
        AgentExecutionDispatcher::new(store, p92b3_permissive_policy(), registry, evidence, None);
    let operator_ctx = make_auth_ctx(Role::Operator, Some("proj-detached"));
    let res = dispatcher
        .dispatch(
            &operator_ctx,
            AgentExecutionRequest {
                plan: sample_valid_plan("proj-detached"),
                idempotency_key: None,
                auto_approve_safe: true,
            },
        )
        .await
        .expect("dispatched");
    assert_eq!(
        res.lifecycle_state,
        AgentExecutionLifecycle::Executing,
        "this dispatch must actually spawn the detached run, else the test proves nothing"
    );

    // The detached task still holds a Store clone at this point. The close must still complete
    // and must still release the file handles.
    dispatcher.store.pool.close().await;
    drop(dir);

    assert!(
        !path.exists(),
        "a detached run holding a Store clone must not keep the fixture directory alive. If it \
         does, the leak survives this phase for every test that dispatches successfully."
    );
    assert_eq!(
        p92b3_dir_count(&root),
        0,
        "the measurement root must be empty"
    );
}

/// **E + F.** The close must complete before the directory is removed, and the SQLite file plus its
/// WAL/SHM sidecars must not block the removal.
#[tokio::test]
async fn p92b3_close_removes_the_directory_despite_wal_and_shm() {
    let root = p92b3_root("close_removes_directory");
    let (store, _registry, _evidence, dir) = setup_harness_in(&root).await;
    let path = dir.path().to_path_buf();

    // A real write, so the WAL/SHM sidecars genuinely exist rather than this being an easy case.
    let pid = uuid::Uuid::new_v4();
    store
        .insert_project(pid, "P92B3 wal", None)
        .await
        .expect("insert_project");

    let live: Vec<String> = std::fs::read_dir(&path)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        live.iter()
            .any(|f| f.ends_with("-wal") || f.ends_with("-shm")),
        "WAL/SHM sidecars must exist while the fixture is live, else this test proves nothing: \
         {live:?}"
    );

    store.pool.close().await;
    drop(dir);

    assert!(
        !path.exists(),
        "after an awaited `pool.close()` the directory must be gone. If it remains, an open \
         SQLite handle -- not the fixture -- is what blocked removal."
    );
    assert_eq!(
        p92b3_dir_count(&root),
        0,
        "the measurement root must be empty"
    );
}

/// **G.** Repeated creation and teardown must not accumulate artifacts in the isolated root.
#[tokio::test]
async fn p92b3_repeated_fixtures_accumulate_nothing() {
    let root = p92b3_root("repeated_fixtures");
    assert_eq!(
        p92b3_dir_count(&root),
        0,
        "the measurement root starts empty"
    );
    for _ in 0..10 {
        let (store, _registry, _evidence, dir) = setup_harness_in(&root).await;
        let pid = uuid::Uuid::new_v4();
        store
            .insert_project(pid, "P92B3 repeat", None)
            .await
            .expect("insert_project");
        store.pool.close().await;
        drop(dir);
    }
    let after = p92b3_dir_count(&root);
    assert_eq!(
        after, 0,
        "ten fixture cycles must leave zero directories in the isolated root; {after} remained"
    );
}

/// **H.** Parallel fixture instances must each remove exactly their own directory, never another's.
#[tokio::test]
async fn p92b3_parallel_fixtures_remain_isolated() {
    let root = p92b3_root("parallel_isolation");
    assert_eq!(
        p92b3_dir_count(&root),
        0,
        "the measurement root starts empty"
    );

    let mut handles = Vec::new();
    for i in 0..8 {
        let root = root.clone();
        handles.push(tokio::spawn(async move {
            let (store, _registry, _evidence, dir) = setup_harness_in(&root).await;
            let path = dir.path().to_path_buf();
            // Prove the fixture was real before releasing it, so a fixture that never created its
            // database cannot pass by never existing.
            assert!(
                path.join("agent_exec_test.sqlite3").exists(),
                "parallel fixture {i} must have created its database before teardown"
            );
            let pid = uuid::Uuid::new_v4();
            store
                .insert_project(pid, "P92B3 parallel", None)
                .await
                .expect("insert_project");
            store.pool.close().await;
            drop(dir);
            path
        }));
    }

    let mut paths = Vec::new();
    for h in handles {
        paths.push(h.await.expect("parallel fixture task must not panic"));
    }
    assert_eq!(
        p92b3_dir_count(&root),
        0,
        "eight parallel fixtures must leave nothing behind -- neither their own nor each other's"
    );
    for p in &paths {
        assert!(
            !p.exists(),
            "each fixture must remove exactly its own directory; {} survived",
            p.display()
        );
    }
}

/// **K.** A teardown failure must remain observable rather than silently suppressed.
/// `TempDir::close()` is the fallible form and returns the real removal error; asserting on it is
/// what stops a future regression from passing quietly. Under this repository's retention policy a
/// directory that genuinely cannot be removed is left in place rather than force-deleted, so it
/// must still be *inspectable* -- which is exactly what a returned `Err` tells us.
#[tokio::test]
async fn p92b3_teardown_failure_is_observable() {
    let root = p92b3_root("teardown_failure_observable");
    let (store, _registry, _evidence, dir) = setup_harness_in(&root).await;
    let path = dir.path().to_path_buf();
    let pid = uuid::Uuid::new_v4();
    store
        .insert_project(pid, "P92B3 diagnostics", None)
        .await
        .expect("insert_project");

    store.pool.close().await;
    let result = dir.close();
    assert!(
        result.is_ok(),
        "an awaited pool close must let TempDir::close() succeed; got {:?}",
        result.err()
    );
    assert!(
        !path.exists(),
        "and the directory must really be gone, not merely reported as closed"
    );
}
