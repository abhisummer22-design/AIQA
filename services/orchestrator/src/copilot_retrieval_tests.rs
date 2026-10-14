//! AIQA Phase 10 (Batch 2) — Tests for Project-Scoped Evidence Retrieval Engine.
//!
//! # Verification Coverage (10 Required Scenarios)
//! 1. `test_each_intent_requests_only_expected_reader_set`
//! 2. `test_valid_project_retrieves_only_its_own_evidence`
//! 3. `test_cross_project_and_tenant_evidence_never_returned`
//! 4. `test_missing_evidence_produces_insufficient_data`
//! 5. `test_reader_failures_propagate_error_never_disguised_as_success`
//! 6. `test_bounded_limits_are_enforced`
//! 7. `test_unsupported_and_clarification_results_skip_retrieval`
//! 8. `test_sql_and_tenant_injection_cannot_alter_reader_or_expose_data`
//! 9. `test_repeated_identical_inputs_are_deterministic`
//! 10. `test_resource_ownership_and_zero_test_artifact_leaks`

use crate::copilot_intent::{ClassificationResult, CopilotIntent, EvidenceReaderKind};
use crate::copilot_retrieval::{
    CopilotEvidenceCoordinator, RetrievalStatus, RetrievedEvidenceData, MAX_FAILURES_RETRIEVED,
};
use crate::domain::{CreateTestCaseRequest, TestStep};
use crate::store::Store;
use tempfile::TempDir;

/// RAII test database fixture ensuring explicit awaited pool closure.
struct Db {
    store: Store,
    #[allow(dead_code)]
    dir: TempDir,
}

impl Db {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("copilot_b2.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, dir }
    }

    async fn new_in(root: &std::path::Path) -> Self {
        let dir = tempfile::tempdir_in(root).expect("tempdir");
        let store = Store::connect(&dir.path().join("copilot_b2.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, dir }
    }

    async fn close(self) {
        self.store.pool.close().await;
    }
}

// ---------------------------------------------------------------------------
// 1. Each intent requests only its expected evidence-reader set
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_each_intent_requests_only_expected_reader_set() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());
    let pid = "proj-reader-check";

    // 1. ReleaseFailureReason -> ReleaseReadiness only
    let plan = CopilotIntent::ReleaseFailureReason { release_id: None }.dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![EvidenceReaderKind::ReleaseReadiness]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev
        .provenance
        .iter()
        .all(|p| p.reader == EvidenceReaderKind::ReleaseReadiness));

    // 2. NextTestRecommendation -> ChangeImpact, QaMemoryRisk
    let plan = CopilotIntent::NextTestRecommendation.dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![
            EvidenceReaderKind::ChangeImpact,
            EvidenceReaderKind::QaMemoryRisk
        ]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev.provenance.iter().all(|p| {
        p.reader == EvidenceReaderKind::ChangeImpact || p.reader == EvidenceReaderKind::QaMemoryRisk
    }));

    // 3. RegressionImpact -> ChangeImpact only
    let plan = CopilotIntent::RegressionImpact { change_ref: None }.dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![EvidenceReaderKind::ChangeImpact]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev
        .provenance
        .iter()
        .all(|p| p.reader == EvidenceReaderKind::ChangeImpact));

    // 4. TestRiskExplanation -> QaMemoryRisk only
    let plan = CopilotIntent::TestRiskExplanation { test_case_id: None }.dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![EvidenceReaderKind::QaMemoryRisk]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev.provenance.iter().all(|p| {
        p.reader == EvidenceReaderKind::QaMemoryRisk || p.reader == EvidenceReaderKind::TestQuality
    }));

    // 5. RecurringFailures -> FailureClusters only
    let plan = CopilotIntent::RecurringFailures.dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![EvidenceReaderKind::FailureClusters]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev
        .provenance
        .iter()
        .all(|p| p.reader == EvidenceReaderKind::FailureClusters));

    // 6. RequirementCoverageGaps -> RequirementsCoverage only
    let plan = CopilotIntent::RequirementCoverageGaps.dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![EvidenceReaderKind::RequirementsCoverage]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev
        .provenance
        .iter()
        .all(|p| p.reader == EvidenceReaderKind::RequirementsCoverage));

    // 7. InsufficientEvidenceReason -> IntegrityVerifier, TestQuality
    let plan = CopilotIntent::InsufficientEvidenceReason {
        finding_id: None,
        run_id: None,
    }
    .dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![
            EvidenceReaderKind::IntegrityVerifier,
            EvidenceReaderKind::TestQuality
        ]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev.provenance.iter().all(|p| {
        p.reader == EvidenceReaderKind::IntegrityVerifier
            || p.reader == EvidenceReaderKind::TestQuality
    }));

    // 8. ReleaseReadinessBlockers -> ReleaseReadiness only
    let plan = CopilotIntent::ReleaseReadinessBlockers { release_id: None }.dispatch_plan();
    assert_eq!(
        plan.required_readers,
        vec![EvidenceReaderKind::ReleaseReadiness]
    );
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert!(ev
        .provenance
        .iter()
        .all(|p| p.reader == EvidenceReaderKind::ReleaseReadiness));

    db.close().await;
}

// ---------------------------------------------------------------------------
// 2. A valid project retrieves only evidence belonging to that project
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_valid_project_retrieves_only_its_own_evidence() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());

    let pid_a = "proj-alpha";
    let pid_b = "proj-beta";

    // Seed failure clusters in both projects
    db.store
        .upsert_failure_cluster(
            pid_a,
            "sig-alpha-1",
            "Hypothesis Alpha",
            0.95,
            Some("fp-a1"),
        )
        .await
        .expect("cluster a");
    db.store
        .upsert_failure_cluster(pid_b, "sig-beta-1", "Hypothesis Beta", 0.85, Some("fp-b1"))
        .await
        .expect("cluster b");

    // Seed releases in both projects
    let rel_a = db
        .store
        .create_release(pid_a, Some("rel-alpha"), Some("1.0.0"), None, None)
        .await
        .expect("rel a");
    let _rel_b = db
        .store
        .create_release(pid_b, Some("rel-beta"), Some("2.0.0"), None, None)
        .await
        .expect("rel b");

    // Retrieve for proj-alpha
    let plan = CopilotIntent::RecurringFailures.dispatch_plan();
    let ev_failures = coordinator
        .retrieve(pid_a, &plan)
        .await
        .expect("retrieve failures");
    assert_eq!(ev_failures.project_id, pid_a);
    assert_eq!(ev_failures.status, RetrievalStatus::Complete);
    if let Some(RetrievedEvidenceData::FailureClusters(clusters)) = ev_failures.data {
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].signature, "sig-alpha-1");
    } else {
        panic!("expected FailureClusters payload");
    }

    let plan_rel = CopilotIntent::ReleaseFailureReason {
        release_id: Some(rel_a.id.to_string()),
    }
    .dispatch_plan();
    let ev_rel = coordinator
        .retrieve(pid_a, &plan_rel)
        .await
        .expect("retrieve rel");
    assert_eq!(ev_rel.project_id, pid_a);
    if let Some(RetrievedEvidenceData::ReleaseReadiness(r)) = ev_rel.data {
        assert_eq!(r.release_id, rel_a.id);
        assert_eq!(r.project_id, pid_a);
    } else {
        panic!("expected ReleaseReadiness payload");
    }

    db.close().await;
}

// ---------------------------------------------------------------------------
// 3. Cross-project and cross-tenant evidence is never returned
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_cross_project_and_tenant_evidence_never_returned() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());

    let pid_a = "proj-tenant-a";
    let pid_b = "proj-tenant-b";

    // Seed release in tenant B
    let rel_b = db
        .store
        .create_release(pid_b, Some("rel-b-private"), Some("b.0.1"), None, None)
        .await
        .expect("rel b");

    // Seed test case in tenant B
    let tc_b_id = db
        .store
        .insert_test_case(
            pid_b,
            &CreateTestCaseRequest {
                title: "Tenant B Secret Auth Test".to_string(),
                description: "Secret".to_string(),
                category: "security".to_string(),
                priority: "high".to_string(),
                severity: "critical".to_string(),
                preconditions: vec![],
                steps: vec![TestStep {
                    step_number: 1,
                    action: "authenticate".to_string(),
                    expected_result: "token granted".to_string(),
                }],
                expected_result: Some("pass".to_string()),
                assertions: vec!["assert 1".to_string()],
                test_data: None,
                source_document: None,
                requirement_reference: None,
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: vec![],
                suite_id: None,
            },
            None,
        )
        .await
        .expect("tc b");

    // Tenant A queries for Tenant B's release ID
    let plan_rel = CopilotIntent::ReleaseFailureReason {
        release_id: Some(rel_b.id.to_string()),
    }
    .dispatch_plan();
    let ev_rel = coordinator
        .retrieve(pid_a, &plan_rel)
        .await
        .expect("retrieve");
    assert_eq!(ev_rel.status, RetrievalStatus::InsufficientData);
    let reason = ev_rel
        .insufficient_data
        .expect("insufficient data info")
        .reason;
    assert!(reason.contains("Release evidence is project-scoped"));

    // Tenant A queries for Tenant B's test case ID
    let plan_tc = CopilotIntent::TestRiskExplanation {
        test_case_id: Some(tc_b_id.to_string()),
    }
    .dispatch_plan();
    let ev_tc = coordinator
        .retrieve(pid_a, &plan_tc)
        .await
        .expect("retrieve");
    assert_eq!(ev_tc.status, RetrievalStatus::InsufficientData);
    let reason = ev_tc
        .insufficient_data
        .expect("insufficient data info")
        .reason;
    assert!(reason.contains("Cross-project or non-existent cases are not retrievable"));

    db.close().await;
}

// ---------------------------------------------------------------------------
// 4. Missing evidence produces the documented insufficient-evidence representation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_missing_evidence_produces_insufficient_data() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());
    let pid = "proj-empty";

    // 1. Release readiness on empty project
    let plan = CopilotIntent::ReleaseFailureReason { release_id: None }.dispatch_plan();
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert_eq!(ev.status, RetrievalStatus::InsufficientData);
    assert!(ev.insufficient_data.is_some());
    assert!(ev.data.is_none());

    // 2. Recurring failures on empty project
    let plan = CopilotIntent::RecurringFailures.dispatch_plan();
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert_eq!(ev.status, RetrievalStatus::InsufficientData);
    assert!(ev.insufficient_data.is_some());

    // 3. Requirement coverage on empty project
    let plan = CopilotIntent::RequirementCoverageGaps.dispatch_plan();
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert_eq!(ev.status, RetrievalStatus::InsufficientData);
    assert!(ev.insufficient_data.is_some());

    // 4. Test recommendations on empty project
    let plan = CopilotIntent::NextTestRecommendation.dispatch_plan();
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert_eq!(ev.status, RetrievalStatus::InsufficientData);
    assert!(ev.insufficient_data.is_some());

    db.close().await;
}

// ---------------------------------------------------------------------------
// 5. Reader failures are not silently converted into successful results
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_reader_failures_propagate_error_never_disguised_as_success() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());
    let pid = "proj-fault";

    // Explicitly close the SQLite pool to inject connection failure
    db.store.pool.close().await;

    let plan = CopilotIntent::RecurringFailures.dispatch_plan();
    let result = coordinator.retrieve(pid, &plan).await;

    // Must be Err, NEVER disguised as Ok(RetrievedEvidence { status: InsufficientData, ... })
    assert!(
        result.is_err(),
        "expected connection failure to propagate as Err"
    );
}

// ---------------------------------------------------------------------------
// 6. Bounded limits are enforced
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_bounded_limits_are_enforced() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());
    let pid = "proj-bounds";

    // Seed 60 failure clusters (exceeding MAX_FAILURES_RETRIEVED = 50)
    for i in 0..60 {
        let sig = format!("sig-bound-{i:03}");
        let fp = format!("fp-bound-{i:03}");
        db.store
            .upsert_failure_cluster(pid, &sig, "Repeated defect", 0.5, Some(&fp))
            .await
            .expect("cluster");
    }

    let plan = CopilotIntent::RecurringFailures.dispatch_plan();
    let ev = coordinator.retrieve(pid, &plan).await.expect("retrieve");
    assert_eq!(ev.status, RetrievalStatus::Complete);
    assert_eq!(ev.record_count, MAX_FAILURES_RETRIEVED);
    assert!(
        ev.is_truncated,
        "expected is_truncated to be true when exceeding limit"
    );

    if let Some(RetrievedEvidenceData::FailureClusters(clusters)) = ev.data {
        assert_eq!(clusters.len(), MAX_FAILURES_RETRIEVED);
    } else {
        panic!("expected FailureClusters payload");
    }

    db.close().await;
}

// ---------------------------------------------------------------------------
// 7. Unsupported or clarification-required classification results skip retrieval
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_unsupported_and_clarification_results_skip_retrieval() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());
    let pid = "proj-skip";

    // Clarification required result
    let clarification = ClassificationResult::ClarificationRequired {
        reason: "Target not specified".to_string(),
        suggested_clarification: "Specify release id".to_string(),
        normalized_query: "why did it fail".to_string(),
    };
    let ev1 = coordinator
        .retrieve_for_classification(pid, &clarification)
        .await
        .expect("skip clarification");
    assert_eq!(ev1.status, RetrievalStatus::Skipped);
    assert!(ev1.provenance.is_empty());
    assert!(ev1.data.is_none());
    assert_eq!(ev1.record_count, 0);

    // Unsupported query result
    let unsupported = ClassificationResult::UnsupportedQuery {
        reason: "Disallowed database command syntax".to_string(),
        normalized_query: "drop table users".to_string(),
    };
    let ev2 = coordinator
        .retrieve_for_classification(pid, &unsupported)
        .await
        .expect("skip unsupported");
    assert_eq!(ev2.status, RetrievalStatus::Skipped);
    assert!(ev2.provenance.is_empty());
    assert!(ev2.data.is_none());
    assert_eq!(ev2.record_count, 0);

    db.close().await;
}

// ---------------------------------------------------------------------------
// 8. User-controlled text cannot inject SQL, select another tenant, or alter reader set
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_sql_and_tenant_injection_cannot_alter_reader_or_expose_data() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());

    // 1. Malicious project_id containing injection syntax
    let malicious_pid = "proj-1'; DROP TABLE runs; --";
    let plan = CopilotIntent::RecurringFailures.dispatch_plan();
    let res = coordinator.retrieve(malicious_pid, &plan).await;
    assert!(
        res.is_err(),
        "malicious project_id must be refused by coordinator"
    );

    // 2. Malicious change_ref inside regression impact
    let safe_pid = "proj-safe";
    let plan = CopilotIntent::RegressionImpact {
        change_ref: Some("PR-1'; DROP TABLE test_cases; --".to_string()),
    }
    .dispatch_plan();
    let ev = coordinator
        .retrieve(safe_pid, &plan)
        .await
        .expect("safe parameterized query");
    assert_eq!(ev.status, RetrievalStatus::InsufficientData);

    // Verify test_cases table was not dropped
    let check = db.store.list_test_cases(safe_pid, None, None).await;
    assert!(check.is_ok(), "test_cases table must remain fully intact");

    db.close().await;
}

// ---------------------------------------------------------------------------
// 9. Repeated identical inputs produce equivalent dispatch/retrieval plans
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_repeated_identical_inputs_are_deterministic() {
    let db = Db::new().await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());
    let pid = "proj-determ";

    // Seed data
    db.store
        .upsert_failure_cluster(pid, "sig-det-1", "Det hypo", 0.9, Some("fp-d1"))
        .await
        .expect("cluster");

    let plan = CopilotIntent::RecurringFailures.dispatch_plan();

    let ev1 = coordinator.retrieve(pid, &plan).await.expect("run 1");
    for _ in 0..10 {
        let ev = coordinator.retrieve(pid, &plan).await.expect("repeat run");
        assert_eq!(ev.status, ev1.status);
        assert_eq!(ev.record_count, ev1.record_count);
        assert_eq!(ev.provenance, ev1.provenance);
        assert_eq!(ev.is_truncated, ev1.is_truncated);
    }

    db.close().await;
}

// ---------------------------------------------------------------------------
// 10. All owned database pools and temporary fixtures are cleaned up correctly
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_resource_ownership_and_zero_test_artifact_leaks() {
    let root = tempfile::tempdir().expect("root tempdir");
    let initial_count = std::fs::read_dir(root.path()).expect("read_dir").count();
    assert_eq!(initial_count, 0, "root directory must start empty");

    // Create fixture rooted inside our owned test directory
    let db = Db::new_in(root.path()).await;
    let coordinator = CopilotEvidenceCoordinator::new(db.store.clone());
    let pid = "proj-leak-test";

    // Exercise retrieval coordinator
    let plan = CopilotIntent::RecurringFailures.dispatch_plan();
    let _ = coordinator.retrieve(pid, &plan).await.expect("retrieve");

    // Await pool closure explicitly to drop all open SQLite file locks
    db.close().await;

    // Clean up temporary root; will succeed without Windows sharing violation errors
    root.close()
        .expect("clean removal with zero leaked handles");
}
