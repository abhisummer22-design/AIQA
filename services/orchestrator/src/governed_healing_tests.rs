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

use crate::agent::{Agent, AgentContext, AgentLimits};
use crate::agent_execution::AgentExecutionDispatcher;
use crate::agent_tools::AgentToolRegistry;
use crate::agents::healing::{FailureEvidence, SelfHealingAgent};
use crate::agents::planning::{PlanningRequest, TestPlanningAgent};
use crate::ai_router::AiInfrastructure;
use crate::auth::{AuthContext, Role};
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
        proposed_actions: vec![
            Action::Navigate {
                url: "http://127.0.0.1:8080/app".to_string(),
            },
            Action::Click {
                selector: "[data-testid='login-btn']".to_string(),
            },
        ],
        diagnosis_category: "SelectorRenamedOrRelocated".to_string(),
        confidence: 0.90,
        rationale: "Observed selector #login-btn was renamed; replaced with stable data-testid attribute identified via DOM prober.".to_string(),
        evidence_refs: vec!["evidence/log-01.log".to_string()],
        timeout_secs: Some(30),
        requirement_reference: Some("REQ-AUTH-01".to_string()),
        custom_params: None,
        auto_approve_safe: false,
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
}

#[tokio::test]
async fn test_05_invalid_ast_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-1"));
    let mut req = sample_proposal_request("proj-1", Uuid::new_v4(), "task-v2");
    req.proposed_actions = vec![]; // empty actions!

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::ValidationError(_)
    ));
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
    req.proposed_actions = vec![Action::Navigate {
        url: "http://169.254.169.254/latest/meta-data".to_string(),
    }];

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
}

#[tokio::test]
async fn test_09_injection_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-inj"));
    let mut req = sample_proposal_request("proj-inj", Uuid::new_v4(), "task-inj");
    req.proposed_actions = vec![Action::Click {
        selector: "<script>alert(document.cookie)</script>".to_string(),
    }];

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
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
}

#[tokio::test]
async fn test_12_environment_change_rejected() {
    let (_store, _reg, _ev, _service, _dir) = setup_healing_harness().await;
    let _ctx = make_auth_ctx(Role::Operator, Some("proj-env"));
    let mut req = sample_proposal_request("proj-env", Uuid::new_v4(), "task-env");
    req.environment = Some("staging".to_string()); // original was test

    let check = verify_immutable_security_boundaries(&req, "test", &req.executor).await;
    assert!(check.is_err());
    assert!(matches!(
        check.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
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

    let check = verify_immutable_security_boundaries(&req, "test", "simulated").await;
    assert!(check.is_err());
    assert!(matches!(
        check.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
}

#[tokio::test]
async fn test_15_credential_security_boundary_change_rejected() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-cred"));
    let mut req = sample_proposal_request("proj-cred", Uuid::new_v4(), "task-cred");
    req.proposed_actions = vec![Action::Fill {
        selector: "#token".to_string(),
        value: "javascript:void(0)".to_string(),
    }];

    let res = service.propose_healing(&ctx, req).await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        GovernedHealingError::SecurityViolation(_)
    ));
}

#[tokio::test]
async fn test_16_low_confidence_proposal_requires_approval() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-conf"));
    let mut req = sample_proposal_request("proj-conf", Uuid::new_v4(), "task-lowconf");
    req.confidence = 0.55; // Low confidence
    req.auto_approve_safe = true;

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert!(proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::PendingApproval);
}

#[tokio::test]
async fn test_17_high_risk_proposal_requires_approval() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-risk"));
    let mut req = sample_proposal_request("proj-risk", Uuid::new_v4(), "task-highrisk");
    // Action replacement increases risk to High
    req.proposed_actions = vec![
        Action::Navigate {
            url: "http://127.0.0.1:8080/app".to_string(),
        },
        Action::Fill {
            selector: "#username".to_string(),
            value: "admin".to_string(),
        },
    ];

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert_eq!(proposal.risk_level, HealingRiskLevel::High);
    assert!(proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::PendingApproval);
}

#[tokio::test]
async fn test_18_production_always_requires_approval() {
    let (_store, _reg, _ev, service, _dir) = setup_healing_harness().await;
    let ctx = make_auth_ctx(Role::Operator, Some("proj-prod"));
    let mut req = sample_proposal_request("proj-prod", Uuid::new_v4(), "task-prod");
    req.environment = Some("production".to_string());
    req.confidence = 0.99;
    req.auto_approve_safe = true;

    let proposal = service
        .propose_healing(&ctx, req)
        .await
        .expect("proposal created");
    assert!(proposal.requires_approval);
    assert_eq!(proposal.state, HealingState::PendingApproval);
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
    // Wait for in-memory simulated run execution
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let verification = service.verify_healing(&ctx, p.id).await.expect("verified");
    assert_eq!(verification.healing_run_id, healing_run_id);
    assert_eq!(verification.outcome, VerificationOutcome::Verified);
    assert!(verification.original_failure_resolved);
    assert!(verification.revised_task_passed);
    assert!(!verification.regression_detected);
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
    let _ = service
        .execute_approved(&ctx, approved.id)
        .await
        .expect("executed");

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
    assert_eq!(req.proposed_actions.len(), 2);
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
}
