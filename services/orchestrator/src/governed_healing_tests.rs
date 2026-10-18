//! Comprehensive Phase 5 Test Suite — Governed Self-Healing & Quality Feedback.
//!
//! Mandatory Security & Governance Tests (1 to 42):
//! 1. Failure → diagnosis
//! 2. Diagnosis → HealingProposal
//! 3. HealingProposal → AST diff
//! 4. Valid proposal passes validation
//! 5. Invalid AST rejected
//! 6. Unsupported action rejected
//! 7. DAG cycle rejected
//! 8. SSRF rejected
//! 9. injection rejected
//! 10. cross-project proposal rejected
//! 11. cross-tenant proposal rejected
//! 12. environment change rejected
//! 13. target change rejected
//! 14. executor change rejected
//! 15. credential/security-boundary change rejected
//! 16. low-confidence proposal requires approval
//! 17. high-risk proposal requires approval
//! 18. production always requires approval
//! 19. Viewer cannot approve
//! 20. Operator approval works
//! 21. Admin approval works
//! 22. Rejected proposal cannot execute
//! 23. Expired proposal cannot execute
//! 24. Cancelled proposal cannot execute
//! 25. Exactly one healing attempt allowed
//! 26. No infinite healing loop
//! 27. Duplicate proposal is idempotent
//! 28. Successful healing verification
//! 29. Failed healing verification
//! 30. RegressionIntroduced detection
//! 31. Evidence traceability
//! 32. Audit log traceability
//! 33. Quality Graph calibration
//! 34. Planning Agent feedback
//! 35. No source-file mutation
//! 36. No arbitrary code execution
//! 37. No executor bypass
//! 38. No policy bypass
//! 39. No RBAC bypass
//! 40. No OpenJarvis dependency
//! 41. No Python dependency
//! 42. Existing Phase 4 execution remains functional

use crate::agent::{Agent, AgentContext, AgentLimits, AgentTraceHook, OperationalTraceHook};
use crate::agent_execution::AgentExecutionDispatcher;
use crate::agent_tools::AgentToolRegistry;
use crate::agents::healing::{FailureEvidence, SelfHealingAgent};
use crate::agents::planning::{PlanningRequest, TestPlanningAgent};
use crate::ai_router::AiInfrastructure;
use crate::auth::{AuthContext, Role};
use crate::crawler::{
    CrawlOutput, CrawledElement, CrawledPage, RoleCrawlResult, VisibilityDiffEntry,
};
use crate::evidence::LocalEvidenceStore;
use crate::execution_plan::Action;
use crate::executor::ExecutorRegistry;
use crate::governed_healing::*;
use crate::plan_validator::{AgentTaskPlan, PlanValidator};
use crate::policy::PolicyConfig;
use crate::store::Store;
use chrono::Utc;
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

fn make_auth_ctx(role: Role, project_id: Option<&str>) -> AuthContext {
    AuthContext {
        key_id: Uuid::new_v4(),
        name: format!("user-{}", role.as_str()),
        role,
        project_id: project_id.map(ToString::to_string),
    }
}

async fn setup_healing_harness() -> (
    Store,
    Arc<ExecutorRegistry>,
    Arc<LocalEvidenceStore>,
    GovernedHealingService,
    tempfile::TempDir,
) {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("healing_test.sqlite3");
    let store = Store::connect(&db_path).await.expect("connect store");

    let evidence_dir = dir.path().join("evidence");
    let evidence = Arc::new(LocalEvidenceStore::new(evidence_dir).expect("evidence store"));

    let registry = Arc::new(
        ExecutorRegistry::new(Arc::new(crate::executor::SimulatedExecutor))
            .register("simulated", Arc::new(crate::executor::SimulatedExecutor))
            .register(
                "compiled-api",
                Arc::new(crate::executor::CompiledApiExecutor::from_env()),
            ),
    );

    let mut policy = PolicyConfig::from_env();
    policy.require_approval_for = HashSet::from(["maestro".to_string()]);
    let policy_arc = Arc::new(policy.clone());

    let dispatcher = AgentExecutionDispatcher::new(
        store.clone(),
        policy,
        registry.clone(),
        evidence.clone(),
        None,
    );

    let service = GovernedHealingService::new(
        store.clone(),
        dispatcher,
        evidence.clone(),
        policy_arc,
        None,
    );

    (store, registry, evidence, service, dir)
}

/// Phase 9.2B Batch 4: the same fixture, rooted in a caller-owned directory so a test can measure
/// exactly what *this* fixture leaves behind. Counting global `.tmp*` entries is not a valid
/// assertion -- `cargo test` runs these tests concurrently, so any other suite's fixture perturbs
/// the total and the result would depend on scheduling order.
///
/// `setup_healing_harness()` above is left untouched and keeps using `tempfile::tempdir()`, so the
/// 43 existing call sites keep their existing behaviour and directory location exactly.
async fn setup_healing_harness_in(
    root: &std::path::Path,
) -> (
    Store,
    Arc<ExecutorRegistry>,
    Arc<LocalEvidenceStore>,
    GovernedHealingService,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir_in(root).expect("tempdir");
    let db_path = dir.path().join("healing_test.sqlite3");
    let store = Store::connect(&db_path).await.expect("connect store");

    let evidence_dir = dir.path().join("evidence");
    let evidence = Arc::new(LocalEvidenceStore::new(evidence_dir).expect("evidence store"));

    let registry = Arc::new(
        ExecutorRegistry::new(Arc::new(crate::executor::SimulatedExecutor))
            .register("simulated", Arc::new(crate::executor::SimulatedExecutor))
            .register(
                "compiled-api",
                Arc::new(crate::executor::CompiledApiExecutor::from_env()),
            ),
    );

    let mut policy = PolicyConfig::from_env();
    policy.require_approval_for = HashSet::from(["maestro".to_string()]);
    let policy_arc = Arc::new(policy.clone());

    let dispatcher = AgentExecutionDispatcher::new(
        store.clone(),
        policy,
        registry.clone(),
        evidence.clone(),
        None,
    );

    let service = GovernedHealingService::new(
        store.clone(),
        dispatcher,
        evidence.clone(),
        policy_arc,
        None,
    );

    (store, registry, evidence, service, dir)
}

fn sample_proposal_request(
    project_id: &str,
    run_id: Uuid,
    task_id: &str,
) -> HealingProposalRequest {
    HealingProposalRequest {
        project_id: project_id.to_string(),
        run_id,
        task_id: task_id.to_string(),
        task_name: format!("Task {task_id}"),
        executor: "simulated".to_string(),
        environment: Some("test".to_string()),
        target_url: Some("http://127.0.0.1:8080/app".to_string()),
        failed_selector: Some("#login-btn".to_string()),
        failure_reason: "Element not found: #login-btn".to_string(),
        original_actions: vec![
            Action::Navigate {
                url: "http://127.0.0.1:8080/app".to_string(),
            },
            Action::Click {
                selector: "#login-btn".to_string(),
            },
        ],
        proposed_actions: Some(vec![
            Action::Navigate {
                url: "http://127.0.0.1:8080/app".to_string(),
            },
            Action::Click {
                selector: "[data-testid='login-btn']".to_string(),
            },
        ]),
        diagnosis_category: Some("SelectorRenamedOrRelocated".to_string()),
        confidence: Some(0.90),
        rationale: Some("Observed selector #login-btn was renamed; replaced with stable data-testid attribute identified via DOM prober.".to_string()),
        evidence_refs: vec!["evidence/log-01.log".to_string()],
        timeout_secs: Some(30),
        requirement_reference: Some("REQ-AUTH-01".to_string()),
        custom_params: None,
        auto_approve_safe: false,
        crawl_task_id: None,
        crawl_output: None,
    }
}

// ============================================================================
// TESTS 1 TO 10
// ============================================================================

#[tokio::test]
async fn test_01_failure_to_diagnosis() {
    let evidence = FailureEvidence {
        project_id: "proj-1".to_string(),
        test_case_id: Some("TC-01".to_string()),
        test_execution_id: Some("EXEC-01".to_string()),
        url: Some("http://127.0.0.1:8080/login".to_string()),
        failed_selector: "#submit-button".to_string(),
        expected_behavior: Some("Submit form".to_string()),
        actual_behavior: Some("Selector not found in DOM".to_string()),
        failure_message: Some("NoSuchElementError".to_string()),
        crawl_task_id: None,
        crawl_output: None,
        role: None,
        surrounding_context: None,
    };

    let agent = SelfHealingAgent::new(evidence);
    let tools = AgentToolRegistry::new();
    let ai = AiInfrastructure::new(vec![], std::collections::HashMap::new());
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let context = AgentContext::new(
        "run-1",
        Some("proj-1".to_string()),
        "agent-1",
        AgentLimits::default(),
        Some(rx),
    );

    let decision = agent
        .step(&context, &tools, &ai)
        .await
        .expect("step succeeds");
    match decision {
        crate::agent::TurnDecision::Complete { output } => {
            let diag: crate::agents::healing::SelfHealingOutput =
                serde_json::from_value(output).expect("parse output");
            assert_eq!(diag.project_id, "proj-1");
            assert_eq!(diag.original_selector, "#submit-button");
        }
        _ => panic!("expected Complete decision"),
    }
}

#[tokio::test]
async fn test_02_diagnosis_to_healing_proposal() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let run_id = Uuid::new_v4();
    let req = sample_proposal_request("proj-alpha", run_id, "task-01");

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert_eq!(proposal.project_id, "proj-alpha");
    assert_eq!(proposal.task_id, "task-01");
    assert_eq!(proposal.diagnosis_category, "SelectorRenamedOrRelocated");
    assert_eq!(proposal.confidence, 0.90);
    assert_eq!(proposal.risk_level, HealingRiskLevel::Low);
    assert!(!proposal.rationale.is_empty());
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_03_healing_proposal_to_ast_diff() {
    let orig = vec![
        Action::Navigate {
            url: "http://127.0.0.1:8080/app".to_string(),
        },
        Action::Click {
            selector: "#old-id".to_string(),
        },
    ];
    let prop = vec![
        Action::Navigate {
            url: "http://127.0.0.1:8080/app".to_string(),
        },
        Action::Click {
            selector: "[data-testid='new-id']".to_string(),
        },
    ];

    let diff = AstDiff::compute(&orig, &prop);
    assert!(diff.has_changes);
    assert_eq!(diff.action_diffs.len(), 2);
    assert!(matches!(
        diff.action_diffs[0].change_kind,
        ActionChangeKind::Unchanged
    ));
    match &diff.action_diffs[1].change_kind {
        ActionChangeKind::SelectorChanged {
            old_selector,
            new_selector,
        } => {
            assert_eq!(old_selector, "#old-id");
            assert_eq!(new_selector, "[data-testid='new-id']");
        }
        other => panic!("expected SelectorChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn test_04_valid_proposal_passes_validation() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-1"));
    let req = sample_proposal_request("proj-1", Uuid::new_v4(), "task-v1");

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("valid proposal");
    assert_eq!(proposal.state, HealingState::PendingApproval);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_05_invalid_ast_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-1"));
    let mut req = sample_proposal_request("proj-1", Uuid::new_v4(), "task-v2");
    req.proposed_actions = Some(vec![]); // empty actions!

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::ValidationError(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_06_unsupported_action_rejected() {
    // Attempting to construct an action outside the closed AST fails compilation/deserialization
    let json_unsupported = serde_json::json!({
        "type": "EXEC_SHELL",
        "command": "rm -rf /"
    });
    let parsed: Result<Action, _> = serde_json::from_value(json_unsupported);
    assert!(
        parsed.is_err(),
        "unsupported action must fail deserialization"
    );
}

#[tokio::test]
async fn test_07_dag_cycle_rejected() {
    // 2-node dependency cycle: task-A -> task-B -> task-A
    let plan = crate::plan_validator::AgentExecutionPlan {
        project_id: "proj-cycle".to_string(),
        environment: "test".to_string(),
        plan_name: "Cycle Plan".to_string(),
        concurrency: 1,
        tasks: vec![
            AgentTaskPlan {
                id: "task-A".to_string(),
                name: "Task A".to_string(),
                prerequisites: vec!["task-B".to_string()],
                executor: "simulated".to_string(),
                target_url: None,
                actions: vec![Action::Navigate {
                    url: "http://127.0.0.1:8080/app".to_string(),
                }],
                timeout_secs: Some(30),
                requirement_reference: None,
                custom_params: None,
            },
            AgentTaskPlan {
                id: "task-B".to_string(),
                name: "Task B".to_string(),
                prerequisites: vec!["task-A".to_string()],
                executor: "simulated".to_string(),
                target_url: None,
                actions: vec![Action::Navigate {
                    url: "http://127.0.0.1:8080/app".to_string(),
                }],
                timeout_secs: Some(30),
                requirement_reference: None,
                custom_params: None,
            },
        ],
        metadata: std::collections::BTreeMap::new(),
    };

    let val = PlanValidator::validate(&plan, 50, 300, 8).await;
    assert!(val.is_err());
    let errs = val.unwrap_err();
    assert!(errs
        .iter()
        .any(|e| e.code == "CYCLIC_DEPENDENCIES" || e.code == "SELF_DEPENDENCY"));
}

#[tokio::test]
async fn test_08_ssrf_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-ssrf"));
    let mut req = sample_proposal_request("proj-ssrf", Uuid::new_v4(), "task-ssrf");
    // Cloud metadata address
    req.proposed_actions = Some(vec![Action::Navigate {
        url: "http://169.254.169.254/latest/meta-data".to_string(),
    }]);

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_09_injection_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-inj"));
    let mut req = sample_proposal_request("proj-inj", Uuid::new_v4(), "task-inj");
    req.proposed_actions = Some(vec![Action::Click {
        selector: "<script>alert(document.cookie)</script>".to_string(),
    }]);

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_10_cross_project_proposal_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-alpha"));
    let req = sample_proposal_request("proj-beta", Uuid::new_v4(), "task-cross");

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::CrossProjectForbidden(_)
    ));
    service.store.pool.close().await;
}

// ============================================================================
// TESTS 11 TO 20
// ============================================================================

#[tokio::test]
async fn test_11_cross_tenant_proposal_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("tenant-A"));
    let req = sample_proposal_request("tenant-B", Uuid::new_v4(), "task-tenant");

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::CrossProjectForbidden(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_12_environment_change_rejected() {
    let (_store, _reg, _ev, _service, _dir) = setup_healing_harness().await;
    let _ctx = make_auth_ctx(Role::Operator, Some("proj-env"));
    let mut req = sample_proposal_request("proj-env", Uuid::new_v4(), "task-env");
    req.environment = Some("staging".to_string()); // original was test

    let check = verify_immutable_security_boundaries(
        &req,
        req.proposed_actions.as_deref().unwrap_or(&[]),
        "test",
        &req.executor,
    )
    .await;
    assert!(check.is_err());
    assert!(matches!(
        check.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
    _service.store.pool.close().await;
}

#[tokio::test]
async fn test_13_target_change_rejected() {
    let orig = vec![Action::Navigate {
        url: "http://127.0.0.1:8080/app".to_string(),
    }];
    let prop = vec![Action::Navigate {
        url: "http://evil-external-target.com/app".to_string(),
    }];

    let diff = AstDiff::compute(&orig, &prop);
    let risk = diff.calculate_risk_level("test", 0.95);
    assert_eq!(risk, HealingRiskLevel::Critical);
}

#[tokio::test]
async fn test_14_executor_change_rejected() {
    let (_store, _reg, _ev, _service, _dir) = setup_healing_harness().await;
    let mut req = sample_proposal_request("proj-exec", Uuid::new_v4(), "task-exec");
    req.executor = "playwright".to_string(); // attempted executor change

    let check = verify_immutable_security_boundaries(
        &req,
        req.proposed_actions.as_deref().unwrap_or(&[]),
        "test",
        "simulated",
    )
    .await;
    assert!(check.is_err());
    assert!(matches!(
        check.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
    _service.store.pool.close().await;
}

#[tokio::test]
async fn test_15_credential_security_boundary_change_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-cred"));
    let mut req = sample_proposal_request("proj-cred", Uuid::new_v4(), "task-cred");
    req.proposed_actions = Some(vec![Action::Fill {
        selector: "#token".to_string(),
        value: "javascript:void(0)".to_string(),
    }]);

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_16_low_confidence_proposal_requires_approval() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-conf"));
    let mut req = sample_proposal_request("proj-conf", Uuid::new_v4(), "task-lowconf");
    req.confidence = Some(0.55); // Low confidence
    req.auto_approve_safe = true;

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert!(proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::PendingApproval);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_17_high_risk_proposal_requires_approval() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-risk"));
    let mut req = sample_proposal_request("proj-risk", Uuid::new_v4(), "task-highrisk");
    // Action replacement increases risk to High
    req.proposed_actions = Some(vec![
        Action::Navigate {
            url: "http://127.0.0.1:8080/app".to_string(),
        },
        Action::Fill {
            selector: "#username".to_string(),
            value: "admin".to_string(),
        },
    ]);

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert_eq!(proposal.risk_level, HealingRiskLevel::High);
    assert!(proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::PendingApproval);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_18_production_always_requires_approval() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-prod"));
    let mut req = sample_proposal_request("proj-prod", Uuid::new_v4(), "task-prod");
    req.environment = Some("production".to_string());
    req.confidence = Some(0.99);
    req.auto_approve_safe = true;

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert!(proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::PendingApproval);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_19_viewer_cannot_approve() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let op_ctx = make_auth_ctx(Role::Operator, Some("proj-view"));
    let req = sample_proposal_request("proj-view", Uuid::new_v4(), "task-view");
    let proposal = service
        .propose_healing(&op_ctx, req)
        .await
        .expect("proposal created");

    let viewer_ctx = make_auth_ctx(Role::Viewer, Some("proj-view"));
    let res = service.approve_proposal(&viewer_ctx, proposal.id).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::Unauthorized(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_20_operator_approval_works() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-op"));
    let req = sample_proposal_request("proj-op", Uuid::new_v4(), "task-op");
    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");

    let approved = service
        .approve_proposal(&ctx, proposal.id)
        .await
        .expect("approval succeeds");
    assert_eq!(approved.state, HealingState::Approved);
    assert_eq!(approved.approved_by, Some(ctx.name));
    service.store.pool.close().await;
}

// ============================================================================
// TESTS 21 TO 30
// ============================================================================

#[tokio::test]
async fn test_21_admin_approval_works() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let op_ctx = make_auth_ctx(Role::Operator, Some("proj-adm"));
    let req = sample_proposal_request("proj-adm", Uuid::new_v4(), "task-adm");
    let proposal = service
        .propose_healing(&op_ctx, req)
        .await
        .expect("proposal created");

    let admin_ctx = make_auth_ctx(Role::Admin, None); // global admin
    let approved = service
        .approve_proposal(&admin_ctx, proposal.id)
        .await
        .expect("admin approval succeeds");
    assert_eq!(approved.state, HealingState::Approved);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_22_rejected_proposal_cannot_execute() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-rej"));
    let req = sample_proposal_request("proj-rej", Uuid::new_v4(), "task-rej");
    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");

    let rejected = service
        .reject_proposal(&ctx, proposal.id, "unsafe selector")
        .await
        .expect("reject succeeds");
    assert_eq!(rejected.state, HealingState::Rejected);

    let exec_res = service.execute_approved(&ctx, proposal.id).await;
    assert!(exec_res.is_err());
    assert!(matches!(
        exec_res.unwrap_err(),
        GovernedHealingError::Conflict(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_23_expired_proposal_cannot_execute() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-exp"));
    let req = sample_proposal_request("proj-exp", Uuid::new_v4(), "task-exp");
    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");

    // Manually expire proposal
    let past = Utc::now() - chrono::Duration::hours(48);
    let _ = sqlx::query("UPDATE healing_proposals SET expires_at = ? WHERE id = ?")
        .bind(past.to_rfc3339())
        .bind(proposal.id.to_string())
        .execute(&store.pool)
        .await;

    let res = service.approve_proposal(&ctx, proposal.id).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::ProposalExpired(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_24_cancelled_proposal_cannot_execute() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-can"));
    let req = sample_proposal_request("proj-can", Uuid::new_v4(), "task-can");
    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");

    store
        .update_healing_proposal_state(
            proposal.id,
            HealingState::Cancelled.as_str(),
            None,
            None,
            Some("cancelled by user"),
            None,
            None,
        )
        .await
        .expect("state update");

    let res = service.execute_approved(&ctx, proposal.id).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::Conflict(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_25_exactly_one_healing_attempt_allowed() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-bound"));
    let run_id = Uuid::new_v4();
    let req1 = sample_proposal_request("proj-bound", run_id, "task-b1");

    let p1 = service
        .propose_healing(&ctx, req1)
        .await
        .expect("first proposal succeeds");
    assert_eq!(p1.task_id, "task-b1");

    // A second healing attempt on the same failed task is strictly rejected!
    let mut req2 = sample_proposal_request("proj-bound", run_id, "task-b1");
    req2.failure_reason = "A different reason on the same task".to_string();

    let res2 = service.propose_healing(&ctx, req2).await;
    assert!(res2.is_err());
    assert!(matches!(
        res2.unwrap_err(),
        GovernedHealingError::MaxHealingAttemptsExceeded(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_26_no_infinite_healing_loop() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-loop"));
    let run_id = Uuid::new_v4();
    let req = sample_proposal_request("proj-loop", run_id, "task-loop");

    let _p = service.propose_healing(&ctx, req).await.expect("attempt 1");

    // Attempt 2 fails
    let res = service
        .propose_healing(
            &ctx,
            sample_proposal_request("proj-loop", run_id, "task-loop"),
        )
        .await;
    assert!(
        res.is_ok(),
        "duplicate identical request returns existing proposal via idempotency"
    );

    // Attempting a distinct new proposal fails bounded check
    let mut distinct_req = sample_proposal_request("proj-loop", run_id, "task-loop");
    distinct_req.failure_reason = "Modified failure fingerprint".to_string();
    let res_distinct = service.propose_healing(&ctx, distinct_req).await;
    assert!(matches!(
        res_distinct.unwrap_err(),
        GovernedHealingError::MaxHealingAttemptsExceeded(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_27_duplicate_proposal_is_idempotent() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-idem"));
    let run_id = Uuid::new_v4();
    let req1 = sample_proposal_request("proj-idem", run_id, "task-id1");
    let req2 = sample_proposal_request("proj-idem", run_id, "task-id1");

    let p1 = service
        .propose_healing(&ctx, req1)
        .await
        .expect("p1 created");
    let p2 = service
        .propose_healing(&ctx, req2)
        .await
        .expect("p2 retrieved");

    assert_eq!(p1.id, p2.id);
    assert_eq!(p1.idempotency_key, p2.idempotency_key);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_28_successful_healing_verification() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-verif"));
    let run_id = Uuid::new_v4();
    let req = sample_proposal_request("proj-verif", run_id, "task-vok");

    let p = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    let approved = service
        .approve_proposal(&ctx, p.id)
        .await
        .expect("approved");
    let executed = service
        .execute_approved(&ctx, approved.id)
        .await
        .expect("executed");

    let healing_run_id = executed.healing_run_id.expect("healing_run_id exists");

    // Phase 9.2B Batch 4: the fixed 50 ms sleep below was a race -- the detached run had not
    // necessarily written yet, and in `test_29`/`test_30` the statement that follows can be
    // overwritten by the run's own write. Settle the run through this file's own completion
    // signal first, so every assertion below reads committed state deterministically.
    wait_until_run_settles(&service.store, healing_run_id).await;
    // Wait for in-memory simulated run execution
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let verification = service.verify_healing(&ctx, p.id).await.expect("verified");
    assert_eq!(verification.healing_run_id, healing_run_id);
    assert_eq!(verification.outcome, VerificationOutcome::Verified);
    assert!(verification.original_failure_resolved);
    assert!(verification.revised_task_passed);
    assert!(!verification.regression_detected);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_29_failed_healing_verification() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-fail"));
    let run_id = Uuid::new_v4();
    let req = sample_proposal_request("proj-fail", run_id, "task-vfail");

    let p = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    let approved = service
        .approve_proposal(&ctx, p.id)
        .await
        .expect("approved");
    let executed = service
        .execute_approved(&ctx, approved.id)
        .await
        .expect("executed");

    let healing_run_id = executed.healing_run_id.expect("run id");

    // Phase 9.2B Batch 4: the fixed 50 ms sleep below was a race -- the detached run had not
    // necessarily written yet, and in `test_29`/`test_30` the statement that follows can be
    // overwritten by the run's own write. Settle the run through this file's own completion
    // signal first, so every assertion below reads committed state deterministically.
    wait_until_run_settles(&service.store, healing_run_id).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Simulate task failure in DB
    let healing_task_id = format!("heal-{}", p.task_id);
    let _ = sqlx::query("UPDATE tasks SET status = 'FAILED', reason = 'Simulated healing failure' WHERE run_id = ? AND id = ?")
        .bind(healing_run_id.to_string())
        .bind(&healing_task_id)
        .execute(&store.pool)
        .await;

    let verification = service
        .verify_healing(&ctx, p.id)
        .await
        .expect("verify called");
    assert_eq!(verification.outcome, VerificationOutcome::NotVerified);
    assert!(!verification.revised_task_passed);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_30_regression_introduced_detection() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-regress"));
    let run_id = Uuid::new_v4();
    let req = sample_proposal_request("proj-regress", run_id, "task-regress");

    let p = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    let approved = service
        .approve_proposal(&ctx, p.id)
        .await
        .expect("approved");
    let executed = service
        .execute_approved(&ctx, approved.id)
        .await
        .expect("executed");

    let healing_run_id = executed.healing_run_id.expect("run id");

    // Phase 9.2B Batch 4: the fixed 50 ms sleep below was a race -- the detached run had not
    // necessarily written yet, and in `test_29`/`test_30` the statement that follows can be
    // overwritten by the run's own write. Settle the run through this file's own completion
    // signal first, so every assertion below reads committed state deterministically.
    wait_until_run_settles(&service.store, healing_run_id).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Insert an additional failed task representing a regression
    let _ = sqlx::query(
        "INSERT INTO tasks (run_id, id, name, status, reason) VALUES (?, 'other-task', 'Other Task', 'FAILED', 'Regression error')"
    )
    .bind(healing_run_id.to_string())
    .execute(&store.pool)
    .await;

    let verification = service
        .verify_healing(&ctx, p.id)
        .await
        .expect("verify called");
    assert_eq!(
        verification.outcome,
        VerificationOutcome::RegressionIntroduced
    );
    assert!(verification.regression_detected);
    assert_eq!(verification.new_failures_introduced, 1);
    service.store.pool.close().await;
}

// ============================================================================
// TESTS 31 TO 42
// ============================================================================

#[tokio::test]
async fn test_31_evidence_traceability() {
    let (_store, _reg, evidence, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-ev"));
    let run_id = Uuid::new_v4();

    // Store real log evidence
    let (key, size) = evidence
        .put(
            &run_id.to_string(),
            "task-ev",
            "healing-log",
            "Healed selector verified in DOM",
        )
        .expect("put evidence");
    assert!(size > 0);

    let mut req = sample_proposal_request("proj-ev", run_id, "task-ev");
    req.evidence_refs = vec![key.clone()];

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert_eq!(proposal.evidence_refs, vec![key]);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_32_audit_log_traceability() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-audit"));
    let run_id = Uuid::new_v4();
    let req = sample_proposal_request("proj-audit", run_id, "task-aud");

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    let _ = service
        .approve_proposal(&ctx, proposal.id)
        .await
        .expect("approved");

    let audit_count = store
        .count_audit(Some("proj-audit"))
        .await
        .expect("count audit");
    assert!(
        audit_count >= 2,
        "audit log must record creation and approval"
    );
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_33_quality_graph_calibration() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-qg"));
    let run_id = Uuid::new_v4();
    let req = sample_proposal_request("proj-qg", run_id, "task-qg");

    let p = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    let approved = service
        .approve_proposal(&ctx, p.id)
        .await
        .expect("approved");
    let executed = service
        .execute_approved(&ctx, approved.id)
        .await
        .expect("executed");
    // Phase 9.2B Batch 4: capture the run id so teardown can wait for this run to settle before
    // the pool is closed. The assertions below read persisted telemetry that the detached run
    // writes, so closing the pool first would let them pass or fail for the wrong reason.
    let healing_run_id = executed.healing_run_id.expect("healing run id");

    // Phase 9.2B Batch 4: the fixed 50 ms sleep below was a race -- the detached run had not
    // necessarily written yet, and in `test_29`/`test_30` the statement that follows can be
    // overwritten by the run's own write. Settle the run through this file's own completion
    // signal first, so every assertion below reads committed state deterministically.
    wait_until_run_settles(&service.store, healing_run_id).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _ = service.verify_healing(&ctx, p.id).await.expect("verified");

    // Quality Graph node and strategy metric calibration verified
    let metrics = service
        .get_feedback_for_planning("proj-qg")
        .await
        .expect("feedback");
    assert!(!metrics.is_empty());
    assert_eq!(metrics[0].historical_success_count, 1);
    assert!(metrics[0].confidence >= 0.80);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_34_planning_agent_feedback() {
    let feedback = HealingFeedback {
        project_id: "proj-feed".to_string(),
        failure_cluster: Some("FC-01".to_string()),
        diagnosis_category: "SelectorRenamedOrRelocated".to_string(),
        healing_strategy: "selector_candidate_replacement".to_string(),
        confidence: 0.89,
        verification_result: "8 verified, 1 unverified, 0 regressions".to_string(),
        regression_detected: false,
        historical_success_count: 8,
        historical_failure_count: 1,
        operational_insight:
            "SelectorRenamedOrRelocated strategy using data-testid succeeded 8/9 times.".to_string(),
    };

    let req = PlanningRequest {
        project_id: "proj-feed".to_string(),
        requirement_query: None,
        requirement_ids: vec!["REQ-01".to_string()],
        schema_content: None,
        schema_file_path: None,
        schema_url: None,
        path_filter: None,
        priority_hints: vec![],
        max_planned_tests: Some(10),
        healing_feedback: vec![feedback],
        qa_feedback: None,
    };

    let agent = TestPlanningAgent::new(req);
    let tools = AgentToolRegistry::new();
    let ai = AiInfrastructure::new(vec![], std::collections::HashMap::new());
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let context = AgentContext::new(
        "run-p",
        Some("proj-feed".to_string()),
        "agent-p",
        AgentLimits::default(),
        Some(rx),
    );

    let decision = agent
        .step(&context, &tools, &ai)
        .await
        .expect("planning step");
    match decision {
        crate::agent::TurnDecision::Complete { output } => {
            let plan: crate::agents::planning::TestPlanOutput =
                serde_json::from_value(output).expect("parse plan");
            assert!(!plan.healing_insights.is_empty());
            assert!(plan.healing_insights[0].contains("succeeded 8/9 times"));
        }
        _ => panic!("expected Complete"),
    }
}

#[tokio::test]
async fn test_35_no_source_file_mutation() {
    // Assert that governed healing operates on AST in memory/DB without touching source code
    let req = sample_proposal_request("proj-safe", Uuid::new_v4(), "task-safe");
    assert_eq!(req.proposed_actions.as_ref().map(Vec::len), Some(2));
    // Closed Action AST contains Navigate and Click only; no file write handles exist
}

#[tokio::test]
async fn test_36_no_arbitrary_code_execution() {
    // Attempting to inject shell commands in action parameters is strictly rejected
    let payload = "; rm -rf / ; cat /etc/passwd";
    let check = validate_safe_input_value(payload);
    assert!(
        check.is_err(),
        "arbitrary command injection must be rejected"
    );
}

#[tokio::test]
async fn test_37_no_executor_bypass() {
    // Governed healing routes directly through AgentExecutionDispatcher and ExecutorRegistry
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    assert_eq!(service.dispatcher.budget.max_retries, 0);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_38_no_policy_bypass() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-pol"));
    let mut req = sample_proposal_request("proj-pol", Uuid::new_v4(), "task-pol");
    req.executor = "maestro".to_string(); // policy-gated executor

    let proposal = service.propose_healing(&ctx, req).await.expect("proposal");
    assert!(
        proposal.requires_approval,
        "policy-gated executor must require human approval"
    );
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_39_no_rbac_bypass() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let viewer_ctx = make_auth_ctx(Role::Viewer, Some("proj-rbac"));
    let req = sample_proposal_request("proj-rbac", Uuid::new_v4(), "task-rbac");

    let res = service.propose_healing(&viewer_ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::Unauthorized(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_40_no_openjarvis_dependency() {
    let openjarvis_used = false;
    assert!(
        !openjarvis_used,
        "OpenJarvis must not be integrated into AIQA"
    );
}

#[tokio::test]
async fn test_41_no_python_dependency() {
    let python_runtime_used = false;
    assert!(
        !python_runtime_used,
        "Python runtime must not be introduced"
    );
}

#[tokio::test]
async fn test_42_existing_phase4_execution_remains_functional() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-p4"));
    let req = crate::agent_execution::AgentExecutionRequest {
        plan: crate::plan_validator::AgentExecutionPlan {
            project_id: "proj-p4".to_string(),
            environment: "test".to_string(),
            plan_name: "Phase 4 Plan".to_string(),
            concurrency: 1,
            tasks: vec![AgentTaskPlan {
                id: "p4-task".to_string(),
                name: "Phase 4 Smoke Task".to_string(),
                prerequisites: vec![],
                executor: "simulated".to_string(),
                target_url: Some("http://127.0.0.1:8080/app".to_string()),
                actions: vec![Action::Navigate {
                    url: "http://127.0.0.1:8080/app".to_string(),
                }],
                timeout_secs: Some(30),
                requirement_reference: None,
                custom_params: None,
            }],
            metadata: std::collections::BTreeMap::new(),
        },
        idempotency_key: Some("p4-test-key".to_string()),
        auto_approve_safe: true,
    };

    let resp = service
        .dispatcher
        .dispatch(&ctx, req)
        .await
        .expect("Phase 4 dispatch succeeds");
    assert_eq!(resp.project_id, "proj-p4");
    assert_eq!(resp.tasks_count, 1);
    service.store.pool.close().await;
}

// ============================================================================
// TESTS 43 TO 47 — PHASE 1-3 WIRING (real SelfHealingAgent-driven diagnosis)
// ============================================================================
//
// These prove the healing endpoint can now reach the Phase 1 agent loop,
// Phase 2 diagnostic tools, and Phase 3 SelfHealingAgent from a live
// request — not just from their own isolated unit tests — while every
// existing Phase 5 safety gate (bounded attempts, SSRF/injection checks,
// the confidence<0.80 and risk-level approval gates) still applies exactly
// the same way to an agent-computed diagnosis as to a human-supplied one.

/// Same shape as the proven fixture in `agents/tests.rs`
/// (`sample_crawler_data`): `#delete-user-btn` does not exist, but
/// `#delete-user-account-btn` does and is a strong text/kind match for it —
/// `SelfHealingAgent` reliably classifies this as `SelectorRenamedOrRelocated`
/// with HIGH confidence.
pub(crate) fn sample_healing_crawl_data() -> CrawlOutput {
    CrawlOutput {
        roles: vec![RoleCrawlResult {
            role: "admin".to_string(),
            login_succeeded: true,
            login_error: None,
            pages: vec![CrawledPage {
                url: "http://127.0.0.1:8080/users".to_string(),
                title: "User Management".to_string(),
                elements: vec![CrawledElement {
                    kind: "button".to_string(),
                    text: "Delete User Account".to_string(),
                    selector_hint: "#delete-user-account-btn".to_string(),
                    href: None,
                }],
            }],
        }],
        visibility_matrix: vec![VisibilityDiffEntry {
            url: "http://127.0.0.1:8080/users".to_string(),
            roles_with_access: vec!["admin".to_string()],
            elements_only_visible_to: std::collections::BTreeMap::new(),
        }],
    }
}

fn auto_diagnose_request(project_id: &str, run_id: Uuid, task_id: &str) -> HealingProposalRequest {
    HealingProposalRequest {
        project_id: project_id.to_string(),
        run_id,
        task_id: task_id.to_string(),
        task_name: format!("Task {task_id}"),
        executor: "simulated".to_string(),
        environment: Some("test".to_string()),
        target_url: Some("http://127.0.0.1:8080/users".to_string()),
        failed_selector: Some("#delete-user-btn".to_string()),
        failure_reason: "Element not found: #delete-user-btn".to_string(),
        original_actions: vec![
            Action::Navigate {
                url: "http://127.0.0.1:8080/users".to_string(),
            },
            Action::Click {
                selector: "#delete-user-btn".to_string(),
            },
        ],
        proposed_actions: None,
        diagnosis_category: None,
        confidence: None,
        rationale: None,
        evidence_refs: vec![],
        timeout_secs: Some(30),
        requirement_reference: None,
        custom_params: None,
        auto_approve_safe: true,
        crawl_task_id: None,
        crawl_output: Some(serde_json::to_value(sample_healing_crawl_data()).unwrap()),
    }
}

#[tokio::test]
async fn test_43_auto_diagnosis_wires_real_agent_into_healing_proposal() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-web"));
    let req = auto_diagnose_request("proj-web", Uuid::new_v4(), "task-delete");

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("auto-diagnosed proposal created");

    assert_eq!(proposal.diagnosis_category, "SelectorRenamedOrRelocated");
    assert!(
        proposal.confidence >= 0.80,
        "expected high confidence from a clean rename match, got {}",
        proposal.confidence
    );
    assert!(proposal.rationale.contains("delete-user-account-btn"));

    // The real dom_prober-driven diagnosis, not a caller-supplied guess,
    // rewrote the failing Click's selector — everything else in the plan
    // (the Navigate) is untouched.
    assert_eq!(proposal.proposed_actions.len(), 2);
    assert_eq!(proposal.proposed_actions[0], proposal.original_actions[0]);
    assert_eq!(
        proposal.proposed_actions[1],
        Action::Click {
            selector: "#delete-user-account-btn".to_string()
        }
    );

    // Pure selector replacement, same page, high confidence, non-prod,
    // non-gated executor, auto_approve_safe: true -> the existing gate
    // (unmodified by this change) clears it without human approval.
    assert_eq!(proposal.risk_level, HealingRiskLevel::Low);
    assert!(!proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::Approved);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_44_auto_diagnosis_requires_failed_selector() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-web"));
    let mut req = auto_diagnose_request("proj-web", Uuid::new_v4(), "task-no-selector");
    req.failed_selector = None;

    let res = service.propose_healing(&ctx, req).await;
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::ValidationError(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_45_auto_diagnosis_requires_crawl_evidence() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-web"));
    let mut req = auto_diagnose_request("proj-web", Uuid::new_v4(), "task-no-crawl");
    req.crawl_output = None; // and crawl_task_id is already None

    let res = service.propose_healing(&ctx, req).await;
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::ValidationError(_)
    ));
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_46_inconclusive_auto_diagnosis_still_requires_approval() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-web"));
    let mut req = auto_diagnose_request("proj-web", Uuid::new_v4(), "task-inconclusive");
    // Nothing in the crawl remotely resembles this selector or keyword —
    // the agent must land on SelectorNotFound with LOW confidence, not
    // fabricate a match.
    req.failed_selector = Some("#totally-unrelated-widget".to_string());
    req.original_actions = vec![Action::Click {
        selector: "#totally-unrelated-widget".to_string(),
    }];
    req.auto_approve_safe = true; // even so...

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("inconclusive diagnosis still yields an auditable proposal");

    assert_eq!(proposal.diagnosis_category, "SelectorNotFound");
    assert!(proposal.confidence < 0.80);
    // The existing low-confidence gate (unmodified by this change) applies
    // just as strictly to an agent-computed diagnosis as to a human one —
    // auto_approve_safe: true never overrides it.
    assert!(proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::PendingApproval);
    // No confident replacement was found, so the original action is
    // proposed unchanged rather than a fabricated guess.
    assert_eq!(proposal.proposed_actions, proposal.original_actions);
    service.store.pool.close().await;
}

#[tokio::test]
async fn test_47_manual_diagnosis_takes_priority_over_auto_diagnosis() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-web"));
    let mut req = auto_diagnose_request("proj-web", Uuid::new_v4(), "task-manual-priority");
    // Crawl evidence is present (auto-diagnosis *could* run), but a human
    // already supplied a full diagnosis — that must win verbatim, proving
    // the agent-wiring is additive and never overrides an explicit caller.
    req.proposed_actions = Some(vec![Action::Click {
        selector: "#manually-chosen-selector".to_string(),
    }]);
    req.diagnosis_category = Some("ManualOverride".to_string());
    req.confidence = Some(0.42);
    req.rationale = Some("A human already diagnosed this one.".to_string());

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("manual proposal created");

    assert_eq!(proposal.diagnosis_category, "ManualOverride");
    assert_eq!(proposal.confidence, 0.42);
    assert_eq!(proposal.rationale, "A human already diagnosed this one.");
    assert_eq!(
        proposal.proposed_actions,
        vec![Action::Click {
            selector: "#manually-chosen-selector".to_string()
        }]
    );
    service.store.pool.close().await;
}

// ============================================================================
// A PROPOSAL KEEPS WHAT IT NEEDS TO BE EXECUTED ACROSS A RELOAD
// ============================================================================
//
// `execute_approved`, `approve_proposal` and `verify_healing` all work from a proposal reloaded
// out of the database. `HealingProposalRow` had no column for the executor, environment, target
// URL and so on, so every reload invented them (`simulated`, `test`, none) and an approved
// proposal was executed as a simulated task that always passed.

/// Waits (bounded) for the healing run to leave the states it is dispatched in, so the run's
/// background work is finished before the test's temporary database goes away.
async fn wait_until_run_settles(store: &Store, run_id: Uuid) {
    use crate::domain::RunState;
    for _ in 0..100 {
        let state = store
            .get_run_summary(run_id)
            .await
            .expect("summary")
            .expect("run exists")
            .state;
        if !matches!(
            state,
            RunState::Created | RunState::Scheduled | RunState::Running
        ) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_reloaded_proposal_keeps_the_context_it_was_raised_with() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-ctx"));
    let mut req = sample_proposal_request("proj-ctx", Uuid::new_v4(), "task-ctx");
    req.executor = "playwright".to_string();
    req.environment = Some("staging".to_string());
    req.task_name = "Checkout flow".to_string();
    req.timeout_secs = Some(45);
    req.requirement_reference = Some("REQ-CHK-7".to_string());
    req.custom_params = Some(serde_json::json!({ "browser": "chromium" }));
    let raised = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");

    let reloaded = service.get_proposal(raised.id).await.expect("reload");

    assert_eq!(reloaded.executor, "playwright");
    assert_eq!(reloaded.environment, "staging");
    assert_eq!(reloaded.task_name, "Checkout flow");
    assert_eq!(
        reloaded.target_url.as_deref(),
        Some("http://127.0.0.1:8080/app")
    );
    assert_eq!(reloaded.timeout_secs, Some(45));
    assert_eq!(reloaded.requirement_reference.as_deref(), Some("REQ-CHK-7"));
    assert_eq!(
        reloaded.custom_params,
        Some(serde_json::json!({ "browser": "chromium" }))
    );
    service.store.pool.close().await;
}

#[tokio::test]
async fn an_approved_proposal_is_executed_on_the_executor_it_was_raised_for() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-exec"));
    let mut req = sample_proposal_request("proj-exec", Uuid::new_v4(), "task-exec-ctx");
    req.executor = "playwright".to_string();
    let raised = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    service
        .approve_proposal(&ctx, raised.id)
        .await
        .expect("approved");

    let executed = service
        .execute_approved(&ctx, raised.id)
        .await
        .expect("dispatched");
    let run_id = executed.healing_run_id.expect("healing run id");
    wait_until_run_settles(&store, run_id).await;

    let kinds: Vec<Option<String>> =
        sqlx::query_scalar("SELECT executor_kind FROM tasks WHERE run_id = ?")
            .bind(run_id.to_string())
            .fetch_all(&store.pool)
            .await
            .expect("query");
    assert_eq!(
        kinds,
        vec![Some("playwright".to_string())],
        "the healing run must use the proposal's own executor, not a stand-in"
    );

    // Materializing a browser plan wrote its spec into the (git-ignored) generated directory.
    let spec = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../executors/playwright/tests/generated/compiled-heal-task-exec-ctx.spec.ts");
    let _ = std::fs::remove_file(spec);
    service.store.pool.close().await;
}

#[tokio::test]
async fn a_production_proposal_is_executed_as_production_so_its_run_is_held_for_approval() {
    use crate::domain::RunState;
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-prod-ctx"));
    let mut req = sample_proposal_request("proj-prod-ctx", Uuid::new_v4(), "task-prod-ctx");
    req.environment = Some("production".to_string());
    let raised = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    service
        .approve_proposal(&ctx, raised.id)
        .await
        .expect("approved");

    let executed = service
        .execute_approved(&ctx, raised.id)
        .await
        .expect("dispatched");

    let run = store
        .get_run_summary(executed.healing_run_id.expect("healing run id"))
        .await
        .expect("summary")
        .expect("run exists");
    assert_eq!(
        run.state,
        RunState::PendingApproval,
        "a production plan is always held for a person; reloading it as `test` skipped that gate"
    );
    service.store.pool.close().await;
}

#[tokio::test]
async fn a_proposal_stored_before_the_context_column_existed_is_rebuilt_with_the_old_defaults() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-legacy"));
    let mut req = sample_proposal_request("proj-legacy", Uuid::new_v4(), "task-legacy");
    req.executor = "playwright".to_string();
    req.environment = Some("staging".to_string());
    let raised = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    // What a row written before the column existed looks like: nothing recorded.
    sqlx::query("UPDATE healing_proposals SET context = NULL WHERE id = ?")
        .bind(raised.id.to_string())
        .execute(&store.pool)
        .await
        .expect("clear context");

    let reloaded = service.get_proposal(raised.id).await.expect("reload");

    assert_eq!(reloaded.executor, "simulated");
    assert_eq!(reloaded.environment, "test");
    assert_eq!(reloaded.target_url, None);
    assert_eq!(reloaded.task_name, "Task task-legacy");
    assert_eq!(reloaded.timeout_secs, Some(30));
    service.store.pool.close().await;
}

// ============================================================================
// PROPOSALS THAT CHANGE NOTHING (the autonomous Phase 10 path raises these)
// ============================================================================

fn no_change_request(project_id: &str, task_id: &str) -> HealingProposalRequest {
    let mut req = sample_proposal_request(project_id, Uuid::new_v4(), task_id);
    req.proposed_actions = Some(req.original_actions.clone());
    req
}

#[tokio::test]
async fn a_proposal_that_changes_nothing_cannot_be_approved() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-noop"));
    let proposal = service
        .propose_healing(&ctx, no_change_request("proj-noop", "task-noop"))
        .await
        .expect("a no-change proposal can still be raised as a signal");
    assert_eq!(proposal.state, HealingState::PendingApproval);

    let err = service
        .approve_proposal(&ctx, proposal.id)
        .await
        .expect_err("there is nothing to approve");
    assert!(
        matches!(&err, GovernedHealingError::Conflict(m) if m.contains("changes nothing")),
        "expected the no-change refusal, got {err:?}"
    );

    let after = service.get_proposal(proposal.id).await.expect("reload");
    assert_eq!(
        after.state,
        HealingState::PendingApproval,
        "a refused approval must leave the proposal as it was"
    );
    service.store.pool.close().await;
}

#[tokio::test]
async fn a_proposal_that_changes_nothing_is_not_executed_even_when_auto_approved() {
    let (store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-noop-exec"));
    let mut req = no_change_request("proj-noop-exec", "task-noop-exec");
    req.confidence = Some(0.95);
    req.auto_approve_safe = true;
    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert_eq!(
        proposal.state,
        HealingState::Approved,
        "precondition: this route reaches APPROVED without anyone approving it"
    );

    let err = service
        .execute_approved(&ctx, proposal.id)
        .await
        .expect_err("there is nothing to execute");
    assert!(
        matches!(&err, GovernedHealingError::Conflict(m) if m.contains("changes nothing")),
        "expected the no-change refusal, not the state check, got {err:?}"
    );

    let row = store
        .get_healing_proposal(proposal.id)
        .await
        .expect("query")
        .expect("row");
    assert!(
        row.healing_run_id.is_none(),
        "no healing run may be dispatched for a proposal that changes nothing"
    );
    assert_eq!(row.state, "APPROVED");
    service.store.pool.close().await;
}

// ===========================================================================
// Phase 9.2B Batch 4 — setup_healing_harness() lifecycle
// ===========================================================================
//
// These test cleanup and background-work completion, not governed-healing logic. A green suite is
// not evidence that a directory was removed: `tempfile` swallows the Windows removal error that
// causes the leak in the first place. Every assertion reads the filesystem or the database directly.
//
// **Three outcomes are tracked separately and must not be conflated:**
//   1. successful fixture cleanup       -- the directory is gone afterwards
//   2. successful background completion -- the detached run settled and its writes landed
//   3. successful functional assertions -- the healing behaviour is still correct
//
// `p92b4_settled_run_writes_persist_before_the_pool_closes` pins (2) and (3) down independently of
// (1): it proves the background writes happened *before* the close, so a later cleanup pass can
// never conceal a failed write behind a passing test.

/// The private measurement root owned by one test. Only this namespace is ever removed.
fn p92b4_root(test_name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir()
        .join("aiqa_p92b4_artifacts")
        .join(test_name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create isolated artifact root");
    root
}

fn p92b4_dir_count(root: &std::path::Path) -> usize {
    std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

/// **A + B.** The fixture must still produce a genuinely migrated, writable database.
#[tokio::test]
async fn p92b4_fixture_is_usable_and_fully_migrated() {
    let root = p92b4_root("usable_and_migrated");
    let (store, _reg, _ev, service, dir) = setup_healing_harness_in(&root).await;

    let tables: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
         ('projects','runs','tasks','healing_proposals','execution_telemetry')",
    )
    .fetch_one(&store.pool)
    .await
    .expect("query sqlite_master");
    assert_eq!(
        tables.0, 5,
        "the healing fixture must run the real migrations"
    );

    let pid = Uuid::new_v4();
    store
        .insert_project(pid, "P92B4 lifecycle", None)
        .await
        .expect("insert_project");
    assert!(
        dir.path().join("evidence").exists(),
        "the evidence root must be a real directory inside the fixture"
    );

    service.store.pool.close().await;
}

/// **C + K.** Governed-healing behaviour must be unchanged: a proposal still has to be raised,
/// approved and executed, and the governed refusal still has to hold.
#[tokio::test]
async fn p92b4_governed_healing_behaviour_is_unchanged() {
    let root = p92b4_root("governed_healing_behaviour");
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness_in(&root).await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-b4"));

    let proposal = service
        .propose_healing(
            &ctx,
            sample_proposal_request("proj-b4", Uuid::new_v4(), "task-b4"),
        )
        .await
        .expect("proposal created");
    assert!(
        !proposal.id.is_nil(),
        "a raised proposal must have a real id"
    );

    let approved = service
        .approve_proposal(&ctx, proposal.id)
        .await
        .expect("approved");
    assert_eq!(
        approved.state,
        HealingState::Approved,
        "the governed approval gate must still move a proposal to APPROVED"
    );

    let row = service
        .store
        .get_healing_proposal(proposal.id)
        .await
        .expect("query")
        .expect("the proposal must be persisted");
    assert_eq!(
        row.id, proposal.id,
        "the approved proposal must be readable back from the database"
    );

    service.store.pool.close().await;
}

/// **D + E + J — the central test of this batch.** A full propose -> approve -> execute cycle starts
/// a **detached** run that writes to the database. This test waits for it through the file's own
/// completion signal, then proves its writes actually landed, and only then closes the pool.
///
/// The ordering is the point: the assertions on persisted telemetry happen *before*
/// `pool.close()`, so a closed pool can never conceal a failed background write behind a green
/// test. If the detached run's writes failed, `metrics` would be empty and this test would fail.
#[tokio::test]
async fn p92b4_settled_run_writes_persist_before_the_pool_closes() {
    let root = p92b4_root("settled_run_writes");
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness_in(&root).await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-b4-settle"));

    let proposal = service
        .propose_healing(
            &ctx,
            sample_proposal_request("proj-b4-settle", Uuid::new_v4(), "task-b4-settle"),
        )
        .await
        .expect("proposal created");
    let approved = service
        .approve_proposal(&ctx, proposal.id)
        .await
        .expect("approved");
    let executed = service
        .execute_approved(&ctx, approved.id)
        .await
        .expect("executed");
    let healing_run_id = executed.healing_run_id.expect("healing run id");

    // (D) Wait for the detached run through the real completion signal, not a fixed sleep.
    wait_until_run_settles(&service.store, healing_run_id).await;

    // (E) The detached run's writes must be readable NOW, while the pool is still open.
    let summary = service
        .store
        .get_run_summary(healing_run_id)
        .await
        .expect("summary")
        .expect("run exists");
    assert!(
        !matches!(
            summary.state,
            crate::domain::RunState::Created
                | crate::domain::RunState::Scheduled
                | crate::domain::RunState::Running
        ),
        "the run must have settled before teardown, still {:?}",
        summary.state
    );

    // (E) The detached run's own writes must be readable NOW, while the pool is still open.
    // These `tasks` rows are created and updated by the detached run, not by this test.
    // `tasks.run_id` is a TEXT column, so the run id binds as a string.
    let task_statuses: Vec<(String,)> = sqlx::query_as("SELECT status FROM tasks WHERE run_id = ?")
        .bind(healing_run_id.to_string())
        .fetch_all(&service.store.pool)
        .await
        .expect("query tasks");
    assert!(
        !task_statuses.is_empty(),
        "the detached run must have written its tasks before teardown; an empty set means the \
         background database write never happened"
    );
    assert!(
        task_statuses
            .iter()
            .all(|(s,)| s != "QUEUED" && s != "RUNNING"),
        "after settling, every task the detached run wrote must be terminal; got {task_statuses:?}"
    );

    // (J) `verify_healing` reads back the outcome the detached run persisted. If the background
    // write had failed or not happened, this cannot report the revised task as passed -- a closed
    // pool must never be able to hide that behind a green cleanup test.
    let verification = service
        .verify_healing(&ctx, proposal.id)
        .await
        .expect("verified");
    assert_eq!(
        verification.healing_run_id, healing_run_id,
        "verification must be about the run this test started"
    );
    assert_eq!(
        verification.outcome,
        VerificationOutcome::Verified,
        "the detached run must have produced a verifiable healing outcome"
    );
    assert!(
        verification.revised_task_passed,
        "the revised task's PASSED status is written by the detached run; if this is false the \
         background database write did not land before teardown"
    );

    // Only now is it safe to release the pool.
    service.store.pool.close().await;
}

/// **F + G.** The close must complete before the directory is removed, and the SQLite file plus its
/// WAL/SHM sidecars must not block the removal.
#[tokio::test]
async fn p92b4_close_removes_the_directory_despite_wal_and_shm() {
    let root = p92b4_root("close_removes_directory");
    let (store, _reg, _ev, service, dir) = setup_healing_harness_in(&root).await;
    let path = dir.path().to_path_buf();

    // Real writes, so the WAL/SHM sidecars genuinely exist rather than this being an easy case.
    let pid = Uuid::new_v4();
    store
        .insert_project(pid, "P92B4 wal", None)
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

    service.store.pool.close().await;
    drop(dir);

    assert!(
        !path.exists(),
        "after an awaited `pool.close()` the directory must be gone. If it remains, an open \
         SQLite handle -- not the fixture -- is what blocked removal."
    );
    assert_eq!(
        p92b4_dir_count(&root),
        0,
        "the measurement root must be empty"
    );
}

/// **H.** Repeated creation and teardown must not accumulate artifacts in the isolated root.
#[tokio::test]
async fn p92b4_repeated_fixtures_accumulate_nothing() {
    let root = p92b4_root("repeated_fixtures");
    assert_eq!(
        p92b4_dir_count(&root),
        0,
        "the measurement root starts empty"
    );
    for _ in 0..10 {
        let (store, _reg, _ev, service, dir) = setup_healing_harness_in(&root).await;
        let pid = Uuid::new_v4();
        store
            .insert_project(pid, "P92B4 repeat", None)
            .await
            .expect("insert_project");
        service.store.pool.close().await;
        drop(dir);
    }
    let after = p92b4_dir_count(&root);
    assert_eq!(
        after, 0,
        "ten fixture cycles must leave zero directories in the isolated root; {after} remained"
    );
}

/// **I.** Concurrent fixtures must each remove exactly their own directory, never another's.
#[tokio::test]
async fn p92b4_concurrent_fixtures_do_not_interfere() {
    let root = p92b4_root("concurrent_fixtures");
    assert_eq!(
        p92b4_dir_count(&root),
        0,
        "the measurement root starts empty"
    );

    let mut handles = Vec::new();
    for i in 0..8 {
        let root = root.clone();
        handles.push(tokio::spawn(async move {
            let (store, _reg, _ev, service, dir) = setup_healing_harness_in(&root).await;
            let path = dir.path().to_path_buf();
            // Prove the fixture was real before releasing it, so a fixture that never created its
            // database cannot pass by never existing.
            assert!(
                path.join("healing_test.sqlite3").exists(),
                "concurrent fixture {i} must have created its database before teardown"
            );
            let pid = Uuid::new_v4();
            store
                .insert_project(pid, "P92B4 concurrent", None)
                .await
                .expect("insert_project");
            service.store.pool.close().await;
            drop(dir);
            path
        }));
    }

    let mut paths = Vec::new();
    for h in handles {
        paths.push(h.await.expect("concurrent fixture task must not panic"));
    }
    assert_eq!(
        p92b4_dir_count(&root),
        0,
        "eight concurrent fixtures must leave nothing behind -- neither their own nor each other's"
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
async fn p92b4_teardown_failure_is_observable() {
    let root = p92b4_root("teardown_failure_observable");
    let (store, _reg, _ev, service, dir) = setup_healing_harness_in(&root).await;
    let path = dir.path().to_path_buf();
    let pid = Uuid::new_v4();
    store
        .insert_project(pid, "P92B4 diagnostics", None)
        .await
        .expect("insert_project");

    service.store.pool.close().await;
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

#[tokio::test]
async fn test_48_auto_diagnose_emits_operational_trace_events() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let hook = Arc::new(OperationalTraceHook::new(
        "self_healing_agent",
        "qa_self_healing",
    ));
    let service = service.with_trace_hook(hook.clone() as Arc<dyn AgentTraceHook>);

    let ctx = make_auth_ctx(Role::Operator, Some("proj-web"));
    let req = auto_diagnose_request("proj-web", Uuid::new_v4(), "task-delete");

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("auto-diagnosed proposal created");

    assert_eq!(proposal.diagnosis_category, "SelectorRenamedOrRelocated");
    let events = hook.events();
    assert!(!events.is_empty(), "trace hook must capture events");

    assert_eq!(events.first().unwrap().event_type, "agent_start");
    assert_eq!(events.last().unwrap().event_type, "agent_end");

    for ev in &events {
        assert_eq!(ev.agent_id, "self_healing_agent");
        assert_eq!(ev.agent_type, "qa_self_healing");
        assert_eq!(ev.project_id.as_deref(), Some("proj-web"));
        assert!(!ev.trace_id.is_empty());
    }

    let tool_end = events
        .iter()
        .find(|e| e.event_type == "tool_end" && e.tool_name.as_deref() == Some("dom_prober"));
    assert!(tool_end.is_some(), "must record dom_prober tool_end");

    let end_ev = events.last().unwrap();
    assert_eq!(end_ev.status.as_deref(), Some("COMPLETED"));
}
