//! Phase 8: Intelligent QA Operations & Cross-Environment Correlation Unit & Integration Tests.
//!
//! Architectural verification:
//! - Strictly zero OpenJarvis, zero Python runtimes, zero PyO3.
//! - Tenant & project isolation: operations data never leaks across projects.
//! - Deterministic cross-environment correlation & recurrence state logic.
//! - Safe, bounded, project-aware retention with dry-run and safety gates.
//! - Read-only operational investigation APIs with complete traceability chain.
//! - Prometheus metrics exposition.

use crate::adaptive_intelligence::{
    compute_telemetry_fingerprint, ExecutionTelemetry, FailureCategory,
};
use crate::domain::TaskStatus;
use crate::operations::{OperationsService, RecurrenceState, RetentionPolicy, TimelineEventType};
use crate::store::{HealingProposalRow, Store};
use chrono::{Duration, Utc};
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_store() -> (Store, TempDir) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test_ops.sqlite3");
    let store = Store::connect(&db_path).await.unwrap();
    (store, dir)
}

fn create_sample_telemetry(
    project_id: &str,
    run_id: Uuid,
    task_name: &str,
    environment: &str,
    category: Option<FailureCategory>,
    fingerprint: Option<String>,
    error_message: Option<String>,
    evidence_refs: Vec<String>,
    selector: Option<String>,
    created_at: chrono::DateTime<Utc>,
    status: TaskStatus,
) -> ExecutionTelemetry {
    ExecutionTelemetry {
        id: Uuid::new_v4(),
        project_id: project_id.to_string(),
        run_id,
        task_id: format!("task-{}", Uuid::new_v4()),
        task_name: task_name.to_string(),
        test_case_id: None,
        execution_id: None,
        executor: "browser_executor".to_string(),
        environment: environment.to_string(),
        start_time: created_at,
        end_time: Some(created_at + Duration::milliseconds(500)),
        duration_ms: 500,
        status,
        retry_count: 0,
        healing_attempt_count: 0,
        failure_category: category,
        failure_fingerprint: fingerprint,
        error_message,
        evidence_refs,
        assertion_result: None,
        http_status: None,
        selector,
        verification_outcome: None,
        created_at,
    }
}

// ============================================================================
// 1. CROSS-ENVIRONMENT CORRELATION TESTS
// ============================================================================

#[tokio::test]
async fn test_01_cross_environment_correlation_identical_fingerprint() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-alpha";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let category = FailureCategory::SelectorNotFound;
    let fingerprint =
        compute_telemetry_fingerprint(&category, "button not found", Some("#submit"), None);

    // Insert failure in development
    let t_dev = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "CheckoutFlow",
        "development",
        Some(category),
        Some(fingerprint.clone()),
        Some("button not found".to_string()),
        vec!["evidence/dev1.png".to_string()],
        Some("#submit".to_string()),
        Utc::now() - Duration::hours(3),
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t_dev).await.unwrap();

    // Insert failure in staging
    let t_stg = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "CheckoutFlow",
        "staging",
        Some(category),
        Some(fingerprint.clone()),
        Some("button not found".to_string()),
        vec!["evidence/stg1.png".to_string()],
        Some("#submit".to_string()),
        Utc::now() - Duration::hours(1),
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t_stg).await.unwrap();

    // Register cluster
    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, &fingerprint, "Selector changed", 0.9)
        .await
        .unwrap();

    let inv = ops
        .get_investigation(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .expect("investigation found");

    assert_eq!(inv.signature, fingerprint);
    assert!(inv.environment_matrix.cross_environment);
    assert_eq!(inv.environment_matrix.environments.len(), 2);
    assert_eq!(inv.environment_matrix.total_occurrences, 2);

    let env_names: Vec<&str> = inv
        .environment_matrix
        .environments
        .iter()
        .map(|e| e.environment.as_str())
        .collect();
    assert!(env_names.contains(&"development"));
    assert!(env_names.contains(&"staging"));
}

#[tokio::test]
async fn test_02_project_isolation_no_cross_project_leakage() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid_a = "proj-tenant-a";
    let pid_b = "proj-tenant-b";
    let _ = store.insert_project(Uuid::new_v4(), pid_a, None).await;
    let _ = store.insert_project(Uuid::new_v4(), pid_b, None).await;

    let category = FailureCategory::Timeout;
    let fingerprint =
        compute_telemetry_fingerprint(&category, "timeout waiting for load", None, None);

    // Tenant A telemetry
    let t_a = create_sample_telemetry(
        pid_a,
        Uuid::new_v4(),
        "LoadTest",
        "production",
        Some(category),
        Some(fingerprint.clone()),
        Some("timeout waiting for load".to_string()),
        vec![],
        None,
        Utc::now(),
        TaskStatus::Timeout,
    );
    store.insert_execution_telemetry(&t_a).await.unwrap();

    // Tenant B telemetry
    let t_b = create_sample_telemetry(
        pid_b,
        Uuid::new_v4(),
        "LoadTest",
        "staging",
        Some(category),
        Some(fingerprint.clone()),
        Some("timeout waiting for load".to_string()),
        vec![],
        None,
        Utc::now(),
        TaskStatus::Timeout,
    );
    store.insert_execution_telemetry(&t_b).await.unwrap();

    let (cluster_a, _) = store
        .upsert_failure_cluster(pid_a, &fingerprint, "Timeout hypothesis", 0.8)
        .await
        .unwrap();

    let inv_a = ops
        .get_investigation(pid_a, &cluster_a.to_string())
        .await
        .unwrap()
        .expect("investigation A");

    assert_eq!(inv_a.project_id, pid_a);
    assert_eq!(inv_a.environment_matrix.environments.len(), 1);
    assert_eq!(
        inv_a.environment_matrix.environments[0].environment,
        "production"
    );

    // Tenant B cannot see Tenant A's cluster
    let inv_b = ops
        .get_investigation(pid_b, &cluster_a.to_string())
        .await
        .unwrap();
    assert!(inv_b.is_none());
}

// ============================================================================
// 2. RECURRENCE INTELLIGENCE STATE TESTS
// ============================================================================

#[tokio::test]
async fn test_03_recurrence_state_new() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-rec";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let category = FailureCategory::AuthenticationFailure;
    let sig = "auth-failed-401";

    let t = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "LoginTest",
        "test",
        Some(category),
        Some(sig.to_string()),
        Some("401 Unauthorized".to_string()),
        vec![],
        None,
        Utc::now(),
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t).await.unwrap();

    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, sig, "Expired token", 0.9)
        .await
        .unwrap();

    let inv = ops
        .get_investigation(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        inv.environment_matrix.recurrence_state,
        RecurrenceState::New
    );
}

#[tokio::test]
async fn test_04_recurrence_state_recurring() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-rec";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let sig = "selector-error-dropdown";
    for i in 0..3 {
        let t = create_sample_telemetry(
            pid,
            Uuid::new_v4(),
            "DropdownTest",
            "test",
            Some(FailureCategory::SelectorNotFound),
            Some(sig.to_string()),
            Some("Dropdown not located".to_string()),
            vec![],
            Some("#menu".to_string()),
            Utc::now() - Duration::minutes(i * 10),
            TaskStatus::Failed,
        );
        store.insert_execution_telemetry(&t).await.unwrap();
    }

    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, sig, "Menu selector shifted", 0.8)
        .await
        .unwrap();

    let inv = ops
        .get_investigation(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        inv.environment_matrix.recurrence_state,
        RecurrenceState::Recurring
    );
}

#[tokio::test]
async fn test_05_recurrence_state_persistent() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-rec";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let sig = "persistent-db-lock";
    for i in 0..6 {
        let t = create_sample_telemetry(
            pid,
            Uuid::new_v4(),
            "TransactionTest",
            "staging",
            Some(FailureCategory::DataFailure),
            Some(sig.to_string()),
            Some("database lock timeout".to_string()),
            vec![],
            None,
            Utc::now() - Duration::hours(i),
            TaskStatus::Failed,
        );
        store.insert_execution_telemetry(&t).await.unwrap();
    }

    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, sig, "Connection pool exhausted", 0.85)
        .await
        .unwrap();

    let inv = ops
        .get_investigation(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        inv.environment_matrix.recurrence_state,
        RecurrenceState::Persistent
    );
}

#[tokio::test]
async fn test_06_recurrence_state_resolved_via_verified_healing() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-rec";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let sig = "resolved-selector";
    let t = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "SearchTest",
        "test",
        Some(FailureCategory::SelectorNotFound),
        Some(sig.to_string()),
        Some("Search button missing".to_string()),
        vec![],
        Some("#search-btn".to_string()),
        Utc::now() - Duration::hours(2),
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t).await.unwrap();

    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, sig, "Outdated selector", 0.9)
        .await
        .unwrap();

    // Insert verified healing proposal
    let proposal = HealingProposalRow {
        id: Uuid::new_v4(),
        project_id: pid.to_string(),
        run_id: t.run_id,
        task_id: t.task_id.clone(),
        failure_fingerprint: sig.to_string(),
        diagnosis_category: "SELECTOR_NOT_FOUND".to_string(),
        confidence: 0.95,
        risk_level: "LOW".to_string(),
        requires_approval: false,
        original_actions: "click(#search-btn)".to_string(),
        proposed_actions: "click([data-testid=search])".to_string(),
        ast_diff: "- click(#search-btn)\n+ click([data-testid=search])".to_string(),
        rationale: "Resilient attribute".to_string(),
        evidence_refs: "[]".to_string(),
        state: "VERIFIED".to_string(),
        idempotency_key: format!("idemp-{}", Uuid::new_v4()),
        healing_run_id: Some(Uuid::new_v4()),
        verification_result: Some("PASSED".to_string()),
        approved_by: Some("system".to_string()),
        approved_at: Some(Utc::now() - Duration::minutes(30)),
        rejection_reason: None,
        created_at: Utc::now() - Duration::hours(1),
        expires_at: Utc::now() + Duration::hours(24),
        updated_at: Utc::now() - Duration::minutes(20),
        context: None,
    };
    store.insert_healing_proposal(&proposal).await.unwrap();

    let inv = ops
        .get_investigation(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        inv.environment_matrix.recurrence_state,
        RecurrenceState::Resolved
    );
}

// ============================================================================
// 3. OPERATIONAL TIMELINE & TRACEABILITY TESTS
// ============================================================================

#[tokio::test]
async fn test_07_operational_timeline_chronological_ordering() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-timeline";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let sig = "timeline-test-sig";
    let t_fail = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "TimelineTask",
        "test",
        Some(FailureCategory::Timeout),
        Some(sig.to_string()),
        Some("Timeout during load".to_string()),
        vec!["evidence/snap.png".to_string()],
        None,
        Utc::now() - Duration::hours(2),
        TaskStatus::Timeout,
    );
    store.insert_execution_telemetry(&t_fail).await.unwrap();

    let proposal = HealingProposalRow {
        id: Uuid::new_v4(),
        project_id: pid.to_string(),
        run_id: t_fail.run_id,
        task_id: t_fail.task_id.clone(),
        failure_fingerprint: sig.to_string(),
        diagnosis_category: "TIMEOUT".to_string(),
        confidence: 0.88,
        risk_level: "MEDIUM".to_string(),
        requires_approval: true,
        original_actions: "wait(5000)".to_string(),
        proposed_actions: "wait(10000)".to_string(),
        ast_diff: "+ wait(10000)".to_string(),
        rationale: "Adjust timing".to_string(),
        evidence_refs: "[\"evidence/snap.png\"]".to_string(),
        state: "EXECUTED".to_string(),
        idempotency_key: format!("idemp-{}", Uuid::new_v4()),
        healing_run_id: Some(Uuid::new_v4()),
        verification_result: Some("VERIFIED".to_string()),
        approved_by: Some("alice@test.corp".to_string()),
        approved_at: Some(Utc::now() - Duration::minutes(80)),
        rejection_reason: None,
        created_at: Utc::now() - Duration::minutes(90),
        expires_at: Utc::now() + Duration::hours(24),
        updated_at: Utc::now() - Duration::minutes(60),
        context: None,
    };
    store.insert_healing_proposal(&proposal).await.unwrap();

    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, sig, "Load latency spike", 0.8)
        .await
        .unwrap();

    let timeline = ops
        .get_investigation_timeline(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .expect("timeline exists");

    assert!(!timeline.is_empty());
    // Verify chronological ascending order
    for i in 1..timeline.len() {
        assert!(timeline[i - 1].timestamp <= timeline[i].timestamp);
    }

    let event_types: Vec<TimelineEventType> = timeline.iter().map(|e| e.event_type).collect();
    assert!(event_types.contains(&TimelineEventType::FailureObserved));
    assert!(event_types.contains(&TimelineEventType::HealingProposed));
    assert!(event_types.contains(&TimelineEventType::HealingApproved));
    assert!(event_types.contains(&TimelineEventType::HealingExecuted));
    assert!(event_types.contains(&TimelineEventType::HealingVerified));
}

// ============================================================================
// 4. CONSERVATIVE REGRESSION & NON-CAUSAL CHANGE CORRELATION
// ============================================================================

#[tokio::test]
async fn test_08_conservative_regression_confirmed() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-regr";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let sig = "regression-sig";
    let t = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "RegressionTask",
        "qa",
        Some(FailureCategory::AssertionFailure),
        Some(sig.to_string()),
        Some("Expected 200 got 500".to_string()),
        vec![],
        None,
        Utc::now() - Duration::hours(2),
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t).await.unwrap();

    let proposal = HealingProposalRow {
        id: Uuid::new_v4(),
        project_id: pid.to_string(),
        run_id: t.run_id,
        task_id: t.task_id.clone(),
        failure_fingerprint: sig.to_string(),
        diagnosis_category: "ASSERTION_FAILURE".to_string(),
        confidence: 0.7,
        risk_level: "HIGH".to_string(),
        requires_approval: true,
        original_actions: "assert(200)".to_string(),
        proposed_actions: "assert(500)".to_string(),
        ast_diff: "bad change".to_string(),
        rationale: "wrong fix".to_string(),
        evidence_refs: "[]".to_string(),
        state: "FAILED".to_string(),
        idempotency_key: format!("idemp-{}", Uuid::new_v4()),
        healing_run_id: None,
        verification_result: Some("REGRESSION".to_string()),
        approved_by: Some("tester".to_string()),
        approved_at: Some(Utc::now() - Duration::hours(1)),
        rejection_reason: None,
        created_at: Utc::now() - Duration::hours(1),
        expires_at: Utc::now() + Duration::hours(24),
        updated_at: Utc::now() - Duration::minutes(30),
        context: None,
    };
    store.insert_healing_proposal(&proposal).await.unwrap();

    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, sig, "Status mismatch", 0.7)
        .await
        .unwrap();

    let inv = ops
        .get_investigation(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(inv.regression_status, "Confirmed regression");
}

#[tokio::test]
async fn test_09_non_causal_change_time_correlation_language() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-noncausal";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let sig = "noncausal-sig";
    let base_time = Utc::now() - Duration::hours(5);

    // 1 failure before healing proposal
    let t_before = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "ApiTask",
        "production",
        Some(FailureCategory::ServerError),
        Some(sig.to_string()),
        Some("500 internal server error".to_string()),
        vec![],
        None,
        base_time,
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t_before).await.unwrap();

    // Proposal created at base_time + 1 hour
    let proposal = HealingProposalRow {
        id: Uuid::new_v4(),
        project_id: pid.to_string(),
        run_id: t_before.run_id,
        task_id: t_before.task_id.clone(),
        failure_fingerprint: sig.to_string(),
        diagnosis_category: "SERVER_ERROR".to_string(),
        confidence: 0.6,
        risk_level: "HIGH".to_string(),
        requires_approval: false,
        original_actions: "get(/data)".to_string(),
        proposed_actions: "get(/v2/data)".to_string(),
        ast_diff: "+ get(/v2/data)".to_string(),
        rationale: "endpoint migration".to_string(),
        evidence_refs: "[]".to_string(),
        state: "EXECUTED".to_string(),
        idempotency_key: format!("idemp-{}", Uuid::new_v4()),
        healing_run_id: None,
        verification_result: None,
        approved_by: None,
        approved_at: None,
        rejection_reason: None,
        created_at: base_time + Duration::hours(1),
        expires_at: Utc::now() + Duration::hours(24),
        updated_at: base_time + Duration::hours(2),
        context: None,
    };
    store.insert_healing_proposal(&proposal).await.unwrap();

    // 3 failures after proposal created
    for i in 0..3 {
        let t_after = create_sample_telemetry(
            pid,
            Uuid::new_v4(),
            "ApiTask",
            "production",
            Some(FailureCategory::ServerError),
            Some(sig.to_string()),
            Some("500 internal server error".to_string()),
            vec![],
            None,
            base_time + Duration::hours(3 + i),
            TaskStatus::Failed,
        );
        store.insert_execution_telemetry(&t_after).await.unwrap();
    }

    let (cluster_id, _) = store
        .upsert_failure_cluster(pid, sig, "Server error spike", 0.6)
        .await
        .unwrap();

    let inv = ops
        .get_investigation(pid, &cluster_id.to_string())
        .await
        .unwrap()
        .unwrap();

    assert!(!inv.non_causal_changes.is_empty());
    for note in &inv.non_causal_changes {
        assert!(note.contains("Failure frequency increased after"));
        assert!(note.contains("correlation only; direct causality not confirmed"));
    }
}

// ============================================================================
// 5. SAFE, BOUNDED TELEMETRY RETENTION TESTS
// ============================================================================

#[tokio::test]
async fn test_10_retention_dry_run_preserves_data() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-retention";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    // Create telemetry older than 40 days
    let old_time = Utc::now() - Duration::days(45);
    for _ in 0..5 {
        let t = create_sample_telemetry(
            pid,
            Uuid::new_v4(),
            "OldTask",
            "test",
            Some(FailureCategory::Timeout),
            Some("old-sig".to_string()),
            Some("timeout".to_string()),
            vec![],
            None,
            old_time,
            TaskStatus::Timeout,
        );
        store.insert_execution_telemetry(&t).await.unwrap();
    }

    // Dry run with retention_days = 30
    let policy = RetentionPolicy {
        retention_days: 30,
        dry_run: true,
    };
    let report = ops.execute_retention(pid, &policy, "admin").await.unwrap();

    assert!(report.dry_run);
    assert_eq!(report.eligible_count, 5);
    assert_eq!(report.pruned_count, 0);

    // Verify records were NOT deleted
    let remaining = store.list_execution_telemetry(pid, 100).await.unwrap();
    assert_eq!(remaining.len(), 5);
}

#[tokio::test]
async fn test_11_retention_prune_deletes_eligible_records() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-retention";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let old_time = Utc::now() - Duration::days(45);
    for _ in 0..4 {
        let t = create_sample_telemetry(
            pid,
            Uuid::new_v4(),
            "OldTask",
            "test",
            Some(FailureCategory::Timeout),
            Some("old-sig".to_string()),
            Some("timeout".to_string()),
            vec![],
            None,
            old_time,
            TaskStatus::Timeout,
        );
        store.insert_execution_telemetry(&t).await.unwrap();
    }

    // Recent telemetry (5 days old)
    let recent_time = Utc::now() - Duration::days(5);
    let t_recent = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "RecentTask",
        "test",
        Some(FailureCategory::Timeout),
        Some("recent-sig".to_string()),
        Some("timeout".to_string()),
        vec![],
        None,
        recent_time,
        TaskStatus::Timeout,
    );
    store.insert_execution_telemetry(&t_recent).await.unwrap();

    // Live prune
    let policy = RetentionPolicy {
        retention_days: 30,
        dry_run: false,
    };
    let report = ops.execute_retention(pid, &policy, "admin").await.unwrap();

    assert!(!report.dry_run);
    assert_eq!(report.eligible_count, 4);
    assert_eq!(report.pruned_count, 4);

    // Only recent record remains
    let remaining = store.list_execution_telemetry(pid, 100).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].task_name, "RecentTask");
}

#[tokio::test]
async fn test_12_retention_safety_guard_preserves_active_runs_and_pending_heals() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-safety";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let old_time = Utc::now() - Duration::days(40);

    // 1. Telemetry for an active run (RUNNING)
    let active_run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO runs (id, name, project_id, state, concurrency, plan, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(active_run_id.to_string())
    .bind("Active Run")
    .bind(pid)
    .bind("RUNNING")
    .bind(1)
    .bind("[]")
    .bind(Utc::now().to_rfc3339())
    .execute(&store.pool)
    .await
    .unwrap();

    let t_active_run = create_sample_telemetry(
        pid,
        active_run_id,
        "ActiveRunTask",
        "staging",
        Some(FailureCategory::Timeout),
        Some("active-sig".to_string()),
        Some("timeout".to_string()),
        vec![],
        None,
        old_time,
        TaskStatus::Timeout,
    );
    store
        .insert_execution_telemetry(&t_active_run)
        .await
        .unwrap();

    // 2. Telemetry for a task with a pending healing proposal
    let pending_task_id = format!("task-pending-{}", Uuid::new_v4());
    let t_pending_heal = ExecutionTelemetry {
        id: Uuid::new_v4(),
        project_id: pid.to_string(),
        run_id: Uuid::new_v4(),
        task_id: pending_task_id.clone(),
        task_name: "PendingHealTask".to_string(),
        test_case_id: None,
        execution_id: None,
        executor: "browser_executor".to_string(),
        environment: "staging".to_string(),
        start_time: old_time,
        end_time: Some(old_time),
        duration_ms: 200,
        status: TaskStatus::Failed,
        retry_count: 0,
        healing_attempt_count: 0,
        failure_category: Some(FailureCategory::SelectorNotFound),
        failure_fingerprint: Some("pending-sig".to_string()),
        error_message: Some("selector missing".to_string()),
        evidence_refs: vec![],
        assertion_result: None,
        http_status: None,
        selector: None,
        verification_outcome: None,
        created_at: old_time,
    };
    store
        .insert_execution_telemetry(&t_pending_heal)
        .await
        .unwrap();

    // Register pending healing proposal
    let proposal = HealingProposalRow {
        id: Uuid::new_v4(),
        project_id: pid.to_string(),
        run_id: t_pending_heal.run_id,
        task_id: pending_task_id.clone(),
        failure_fingerprint: "pending-sig".to_string(),
        diagnosis_category: "SELECTOR_NOT_FOUND".to_string(),
        confidence: 0.9,
        risk_level: "LOW".to_string(),
        requires_approval: true,
        original_actions: "click()".to_string(),
        proposed_actions: "click(#new)".to_string(),
        ast_diff: "+ click(#new)".to_string(),
        rationale: "update".to_string(),
        evidence_refs: "[]".to_string(),
        state: "PENDING_APPROVAL".to_string(),
        idempotency_key: format!("idemp-{}", Uuid::new_v4()),
        healing_run_id: None,
        verification_result: None,
        approved_by: None,
        approved_at: None,
        rejection_reason: None,
        created_at: old_time,
        expires_at: Utc::now() + Duration::hours(24),
        updated_at: old_time,
        context: None,
    };
    store.insert_healing_proposal(&proposal).await.unwrap();

    // 3. Regular old telemetry eligible for pruning
    let t_normal_old = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "NormalOldTask",
        "staging",
        Some(FailureCategory::ServerError),
        Some("normal-sig".to_string()),
        Some("500 error".to_string()),
        vec![],
        None,
        old_time,
        TaskStatus::Failed,
    );
    store
        .insert_execution_telemetry(&t_normal_old)
        .await
        .unwrap();

    // Execute retention
    let policy = RetentionPolicy {
        retention_days: 30,
        dry_run: false,
    };
    let report = ops.execute_retention(pid, &policy, "admin").await.unwrap();

    // Exactly 1 eligible record pruned (the regular old one)
    assert_eq!(report.pruned_count, 1);

    let remaining = store.list_execution_telemetry(pid, 100).await.unwrap();
    let remaining_tasks: Vec<String> = remaining.into_iter().map(|r| r.task_name).collect();

    // Active run task & pending heal task MUST be preserved
    assert!(remaining_tasks.contains(&"ActiveRunTask".to_string()));
    assert!(remaining_tasks.contains(&"PendingHealTask".to_string()));
    assert!(!remaining_tasks.contains(&"NormalOldTask".to_string()));
}

#[tokio::test]
async fn test_13_retention_zero_days_rejected() {
    let (store, _dir) = setup_test_store().await;
    let ops = OperationsService::new(store.clone());

    let pid = "proj-retention-zero";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    let policy = RetentionPolicy {
        retention_days: 0,
        dry_run: true,
    };
    let res = ops.execute_retention(pid, &policy, "admin").await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("at least 1 day"));
}

// ============================================================================
// 6. PROMETHEUS METRICS EXPOSITION TESTS
// ============================================================================

#[tokio::test]
async fn test_14_prometheus_metrics_render_phase8_metrics() {
    let (store, _dir) = setup_test_store().await;

    let pid = "proj-metrics";
    let _ = store.insert_project(Uuid::new_v4(), pid, None).await;

    // Insert failure cluster with occurrence_count = 3
    let (_cluster_id, _) = store
        .upsert_failure_cluster(pid, "metric-sig-1", "hypothesis 1", 0.8)
        .await
        .unwrap();
    let _ = store
        .upsert_failure_cluster(pid, "metric-sig-1", "hypothesis 1", 0.8)
        .await
        .unwrap();
    let _ = store
        .upsert_failure_cluster(pid, "metric-sig-1", "hypothesis 1", 0.8)
        .await
        .unwrap();

    // Insert cross-environment telemetry
    let t_dev = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "MetricTask",
        "development",
        Some(FailureCategory::NetworkFailure),
        Some("metric-sig-cross".to_string()),
        Some("network error".to_string()),
        vec![],
        None,
        Utc::now(),
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t_dev).await.unwrap();

    let t_prod = create_sample_telemetry(
        pid,
        Uuid::new_v4(),
        "MetricTask",
        "production",
        Some(FailureCategory::NetworkFailure),
        Some("metric-sig-cross".to_string()),
        Some("network error".to_string()),
        vec![],
        None,
        Utc::now(),
        TaskStatus::Failed,
    );
    store.insert_execution_telemetry(&t_prod).await.unwrap();

    let metrics_text = crate::metrics::render(&store.pool).await;

    assert!(metrics_text.contains("aiqa_investigations_total"));
    assert!(metrics_text.contains("aiqa_correlated_failures_total"));
    assert!(metrics_text.contains("aiqa_cross_environment_correlations_total 1"));
    assert!(metrics_text.contains("aiqa_telemetry_pruned_total"));
    assert!(metrics_text.contains("aiqa_retention_runs_total"));
}
