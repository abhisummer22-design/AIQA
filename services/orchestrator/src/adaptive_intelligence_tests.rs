//! Comprehensive Unit and Integration Tests for Phase 6: Adaptive QA Intelligence & Observability.

use crate::adaptive_intelligence::*;
use crate::domain::{TaskDef, TaskState, TaskStatus};
use crate::store::Store;
use chrono::Utc;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_store() -> (Store, TempDir) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test_adaptive.sqlite3");
    let store = Store::connect(&db_path).await.unwrap();
    (store, dir)
}

#[tokio::test]
async fn test_1_real_execution_result_to_normalized_telemetry() {
    let (store, _dir) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store);

    let run_id = Uuid::new_v4();
    let mut task = TaskState::new("task-1", "Login Test");
    task.started_at = Some(Utc::now());
    task.finished_at = Some(Utc::now() + chrono::Duration::milliseconds(250));
    task.status = TaskStatus::Failed;
    task.reason = Some("Element not found: #submit-button".to_string());

    let telemetry = ExecutionTelemetry::from_task_state(
        "proj-1",
        run_id,
        &task,
        "playwright",
        "staging",
        None,
        vec!["ev-1".to_string()],
        Some("#submit-button".to_string()),
        None,
        None,
    );

    assert_eq!(telemetry.project_id, "proj-1");
    assert_eq!(telemetry.executor, "playwright");
    assert_eq!(telemetry.environment, "staging");
    assert_eq!(telemetry.status, TaskStatus::Failed);
    assert_eq!(
        telemetry.failure_category,
        Some(FailureCategory::SelectorNotFound)
    );
    assert!(telemetry.failure_fingerprint.is_some());

    service.record_execution_telemetry(telemetry).await.unwrap();
}

#[tokio::test]
async fn test_2_telemetry_persistence() {
    let (store, _dir) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store.clone());

    let run_id = Uuid::new_v4();
    let task = TaskState::new("task-2", "API Test");

    let telemetry = ExecutionTelemetry::from_task_state(
        "proj-pers",
        run_id,
        &task,
        "compiled-api",
        "qa",
        None,
        Vec::new(),
        None,
        Some(200),
        None,
    );

    service.record_execution_telemetry(telemetry).await.unwrap();

    let loaded = store
        .list_execution_telemetry("proj-pers", 10)
        .await
        .unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].task_name, "API Test");
}

#[tokio::test]
async fn test_3_telemetry_project_isolation() {
    let (store, _dir) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store.clone());

    let t1 = ExecutionTelemetry::from_task_state(
        "proj-A",
        Uuid::new_v4(),
        &TaskState::new("t1", "Task A"),
        "simulated",
        "test",
        None,
        Vec::new(),
        None,
        None,
        None,
    );
    let t2 = ExecutionTelemetry::from_task_state(
        "proj-B",
        Uuid::new_v4(),
        &TaskState::new("t2", "Task B"),
        "simulated",
        "test",
        None,
        Vec::new(),
        None,
        None,
        None,
    );

    service.record_execution_telemetry(t1).await.unwrap();
    service.record_execution_telemetry(t2).await.unwrap();

    let list_a = store.list_execution_telemetry("proj-A", 10).await.unwrap();
    let list_b = store.list_execution_telemetry("proj-B", 10).await.unwrap();

    assert_eq!(list_a.len(), 1);
    assert_eq!(list_a[0].task_name, "Task A");
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].task_name, "Task B");
}

#[tokio::test]
async fn test_4_telemetry_tenant_isolation() {
    let (store, _dir) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store.clone());

    let t = ExecutionTelemetry::from_task_state(
        "tenant-1",
        Uuid::new_v4(),
        &TaskState::new("t", "Tenant Task"),
        "simulated",
        "prod",
        None,
        Vec::new(),
        None,
        None,
        None,
    );
    service.record_execution_telemetry(t).await.unwrap();

    let other = store
        .list_execution_telemetry("tenant-2", 10)
        .await
        .unwrap();
    assert!(other.is_empty());
}

#[test]
fn test_5_failure_normalization() {
    assert_eq!(
        categorize_failure("Unauthenticated access", Some(401)),
        FailureCategory::AuthenticationFailure
    );
    assert_eq!(
        categorize_failure("Permission denied for resource", Some(403)),
        FailureCategory::AuthorizationFailure
    );
    assert_eq!(
        categorize_failure("Connection refused ECONNREFUSED", None),
        FailureCategory::NetworkFailure
    );
    assert_eq!(
        categorize_failure("Operation timed out after 30000ms", None),
        FailureCategory::Timeout
    );
    assert_eq!(
        categorize_failure("Waiting for locator('#btn')", None),
        FailureCategory::SelectorNotFound
    );
}

#[test]
fn test_6_stable_failure_fingerprint() {
    let cat = FailureCategory::SelectorNotFound;
    let fp1 = compute_telemetry_fingerprint(&cat, "element not found <uuid>", Some("#btn"), None);
    let fp2 = compute_telemetry_fingerprint(&cat, "element not found <uuid>", Some("#btn"), None);

    assert_eq!(fp1, fp2);
    assert_eq!(fp1.len(), 64);
}

#[test]
fn test_7_dynamic_error_normalization() {
    let raw = "Error in request 550e8400-e29b-41d4-a716-446655440000 at 2026-09-15T18:40:00Z on 192.168.1.1:8080 (0x7fff)";
    let masked = mask_dynamic_tokens(raw);

    assert!(masked.contains("<uuid>"));
    assert!(masked.contains("<timestamp>"));
    assert!(masked.contains("<ip>"));
    assert!(masked.contains("<hex>"));
}

#[test]
fn test_8_failure_clustering() {
    let cat = FailureCategory::Timeout;
    let fp1 = compute_telemetry_fingerprint(&cat, "timeout waiting for <n>ms", None, None);
    let fp2 = compute_telemetry_fingerprint(&cat, "timeout waiting for <n>ms", None, None);

    assert_eq!(fp1, fp2);
}

#[test]
fn test_9_similar_failures_cluster_together() {
    let cat = FailureCategory::Timeout;
    let msg1 = mask_dynamic_tokens("Timeout waiting 5000ms");
    let msg2 = mask_dynamic_tokens("Timeout waiting 10000ms");

    let fp1 = compute_telemetry_fingerprint(&cat, &msg1, None, None);
    let fp2 = compute_telemetry_fingerprint(&cat, &msg2, None, None);

    assert_eq!(fp1, fp2);
}

#[test]
fn test_10_different_failures_remain_separate() {
    let fp1 = compute_telemetry_fingerprint(&FailureCategory::Timeout, "timeout", None, None);
    let fp2 = compute_telemetry_fingerprint(
        &FailureCategory::AuthenticationFailure,
        "unauthorized",
        None,
        None,
    );

    assert_ne!(fp1, fp2);
}

#[test]
fn test_11_flakiness_requires_historical_evidence() {
    // Only 1 execution -> insufficient data -> Stable by default
    let analysis = evaluate_flakiness("task-1", "staging", &[TaskStatus::Failed]);
    assert_eq!(analysis.flakiness_score, 0.0);
    assert_eq!(analysis.classification, FlakinessClassification::Stable);
}

#[test]
fn test_12_stable_test_remains_stable() {
    let outcomes = vec![
        TaskStatus::Passed,
        TaskStatus::Passed,
        TaskStatus::Passed,
        TaskStatus::Passed,
        TaskStatus::Passed,
    ];
    let analysis = evaluate_flakiness("task-pass", "staging", &outcomes);
    assert_eq!(analysis.flakiness_score, 0.0);
    assert_eq!(analysis.classification, FlakinessClassification::Stable);
}

#[test]
fn test_13_mixed_outcomes_detect_potential_flakiness() {
    let outcomes = vec![
        TaskStatus::Passed,
        TaskStatus::Failed,
        TaskStatus::Passed,
        TaskStatus::Failed,
        TaskStatus::Passed,
    ];
    let analysis = evaluate_flakiness("flaky-task", "staging", &outcomes);
    assert!(analysis.flakiness_score > 0.15);
    assert!(matches!(
        analysis.classification,
        FlakinessClassification::PotentiallyFlaky | FlakinessClassification::HighlyFlaky
    ));
}

#[test]
fn test_14_environment_aware_flakiness() {
    let staging_outcomes = vec![TaskStatus::Passed, TaskStatus::Passed, TaskStatus::Passed];
    let prod_outcomes = vec![TaskStatus::Failed, TaskStatus::Failed, TaskStatus::Failed];

    let analysis_stg = evaluate_flakiness("task-env", "staging", &staging_outcomes);
    let analysis_prd = evaluate_flakiness("task-env", "production", &prod_outcomes);

    assert_eq!(analysis_stg.flakiness_score, 0.0);
    assert_eq!(analysis_prd.flakiness_score, 0.0); // 100% consistent failure is a real bug, not flaky!
}

#[test]
fn test_15_strategy_metrics_update() {
    let metrics = vec![crate::store::HealingStrategyMetricsRow {
        id: Uuid::new_v4(),
        project_id: "p1".to_string(),
        strategy: "selector_update".to_string(),
        failure_category: "SELECTOR_NOT_FOUND".to_string(),
        attempts_count: 10,
        verified_count: 8,
        not_verified_count: 1,
        regression_count: 1,
        calibrated_confidence: 0.80,
        updated_at: Utc::now(),
    }];

    let rec = score_healing_strategy(&FailureCategory::SelectorNotFound, &metrics);
    assert_eq!(rec.recommended_strategy, "selector_update");
    assert!(rec.confidence > 0.60);
    assert_eq!(rec.total_attempts, 10);
    assert_eq!(rec.successful_heals, 8);
}

#[test]
fn test_16_verified_healing_improves_strategy_evidence() {
    let low_exp = vec![crate::store::HealingStrategyMetricsRow {
        id: Uuid::new_v4(),
        project_id: "p1".to_string(),
        strategy: "selector_update".to_string(),
        failure_category: "SELECTOR_NOT_FOUND".to_string(),
        attempts_count: 3,
        verified_count: 3,
        not_verified_count: 0,
        regression_count: 0,
        calibrated_confidence: 0.85,
        updated_at: Utc::now(),
    }];

    let high_exp = vec![crate::store::HealingStrategyMetricsRow {
        id: Uuid::new_v4(),
        project_id: "p1".to_string(),
        strategy: "selector_update".to_string(),
        failure_category: "SELECTOR_NOT_FOUND".to_string(),
        attempts_count: 20,
        verified_count: 20,
        not_verified_count: 0,
        regression_count: 0,
        calibrated_confidence: 0.95,
        updated_at: Utc::now(),
    }];

    let rec_low = score_healing_strategy(&FailureCategory::SelectorNotFound, &low_exp);
    let rec_high = score_healing_strategy(&FailureCategory::SelectorNotFound, &high_exp);

    assert!(rec_high.confidence > rec_low.confidence);
}

#[test]
fn test_17_regression_decreases_strategy_reliability() {
    let clean = vec![crate::store::HealingStrategyMetricsRow {
        id: Uuid::new_v4(),
        project_id: "p1".to_string(),
        strategy: "selector_update".to_string(),
        failure_category: "SELECTOR_NOT_FOUND".to_string(),
        attempts_count: 10,
        verified_count: 10,
        not_verified_count: 0,
        regression_count: 0,
        calibrated_confidence: 0.90,
        updated_at: Utc::now(),
    }];

    let regression = vec![crate::store::HealingStrategyMetricsRow {
        id: Uuid::new_v4(),
        project_id: "p1".to_string(),
        strategy: "selector_update".to_string(),
        failure_category: "SELECTOR_NOT_FOUND".to_string(),
        attempts_count: 10,
        verified_count: 5,
        not_verified_count: 0,
        regression_count: 5,
        calibrated_confidence: 0.40,
        updated_at: Utc::now(),
    }];

    let rec_clean = score_healing_strategy(&FailureCategory::SelectorNotFound, &clean);
    let rec_reg = score_healing_strategy(&FailureCategory::SelectorNotFound, &regression);

    assert!(rec_clean.confidence > rec_reg.confidence);
}

#[test]
fn test_18_insufficient_data_does_not_create_false_trend() {
    let trend = calculate_trend("duration", &[10.0, 12.0]);
    assert_eq!(trend.direction, TrendDirection::InsufficientData);
}

#[test]
fn test_19_improving_trend_detection() {
    let values = vec![100.0, 95.0, 90.0, 80.0, 70.0, 60.0];
    let trend = calculate_trend("duration", &values);
    assert_eq!(trend.direction, TrendDirection::Improving);
}

#[test]
fn test_20_degrading_trend_detection() {
    let values = vec![50.0, 60.0, 70.0, 80.0, 90.0, 110.0];
    let trend = calculate_trend("duration", &values);
    assert_eq!(trend.direction, TrendDirection::Degrading);
}

#[test]
fn test_21_stable_trend_detection() {
    let values = vec![50.0, 50.5, 49.8, 50.1, 50.2, 49.9];
    let trend = calculate_trend("duration", &values);
    assert_eq!(trend.direction, TrendDirection::Stable);
}

#[tokio::test]
async fn test_22_performance_duration_telemetry() {
    let mut task = TaskState::new("t-dur", "Perf Task");
    let start = Utc::now();
    let end = start + chrono::Duration::milliseconds(1250);
    task.started_at = Some(start);
    task.finished_at = Some(end);

    let telemetry = ExecutionTelemetry::from_task_state(
        "p",
        Uuid::new_v4(),
        &task,
        "k6",
        "perf",
        None,
        Vec::new(),
        None,
        None,
        None,
    );

    assert_eq!(telemetry.duration_ms, 1250);
}

#[test]
fn test_23_sensitive_header_sanitization() {
    let raw = "Authorization: Bearer my-secret-token-1234567890";
    let sanitized = sanitize_telemetry_text(raw);
    assert!(!sanitized.contains("my-secret-token"));
    assert!(sanitized.contains("[REDACTED]"));
}

#[test]
fn test_24_api_key_sanitization() {
    let raw = "GET /api?api_key=secret_key_abcdef123456789";
    let sanitized = sanitize_telemetry_text(raw);
    assert!(!sanitized.contains("secret_key_abcdef123456789"));
}

#[test]
fn test_25_password_sanitization() {
    let raw = "payload password: SuperSecretPassword123!";
    let sanitized = sanitize_telemetry_text(raw);
    assert!(!sanitized.contains("SuperSecretPassword123!"));
    assert!(sanitized.contains("[REDACTED]"));
}

#[test]
fn test_26_cookie_sanitization() {
    let raw = "Cookie: sessionid=abcdef1234567890xyz";
    let sanitized = sanitize_telemetry_text(raw);
    assert!(!sanitized.contains("abcdef1234567890xyz"));
    assert!(sanitized.contains("[REDACTED]"));
}

#[tokio::test]
async fn test_27_cross_project_isolation() {
    let (store, _dir) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store.clone());

    let feedback_a = service.generate_planning_feedback("proj-X").await.unwrap();
    let feedback_b = service.generate_planning_feedback("proj-Y").await.unwrap();

    assert!(feedback_a.fragile_tests.is_empty());
    assert!(feedback_b.fragile_tests.is_empty());
}

#[tokio::test]
async fn test_28_cross_tenant_isolation() {
    let (store, _dir) = setup_test_store().await;

    let rows_t1 = store
        .list_execution_telemetry("tenant-alpha", 10)
        .await
        .unwrap();
    let rows_t2 = store
        .list_execution_telemetry("tenant-beta", 10)
        .await
        .unwrap();

    assert!(rows_t1.is_empty());
    assert!(rows_t2.is_empty());
}

#[tokio::test]
async fn test_29_planning_agent_receives_structured_feedback() {
    let (store, _dir) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store);

    // Record two failures for task "Brittle Test"
    for i in 0..2 {
        let mut task = TaskState::new(format!("t-b-{i}"), "Brittle Test");
        task.status = TaskStatus::Failed;
        task.reason = Some("Selector #btn not found".to_string());

        let telemetry = ExecutionTelemetry::from_task_state(
            "proj-plan",
            Uuid::new_v4(),
            &task,
            "playwright",
            "staging",
            None,
            Vec::new(),
            Some("#btn".to_string()),
            None,
            None,
        );
        service.record_execution_telemetry(telemetry).await.unwrap();
    }

    let feedback = service
        .generate_planning_feedback("proj-plan")
        .await
        .unwrap();
    assert_eq!(feedback.fragile_tests.len(), 1);
    assert!(feedback.fragile_tests[0].contains("Brittle Test"));
    assert_eq!(feedback.fragile_selectors.len(), 1);
    assert!(feedback.fragile_selectors[0].contains("#btn"));
}

#[test]
fn test_30_risk_calibration_uses_real_evidence() {
    let factors = RiskFactors {
        failure_rate: 0.90,
        flakiness_score: 0.50,
        failure_severity: 0.90,
        dependency_count: 3,
        healing_regression_rate: 0.10,
        security_sensitivity: 0.90,
    };

    let cal = calibrate_risk("Auth Service", factors);
    assert_eq!(cal.risk_level, "CRITICAL");
    assert!(cal.risk_score >= 0.70);
}

#[test]
fn test_30b_risk_calibration_levels_and_authoritative_weights() {
    // 1. LOW risk (< 0.25)
    let low_factors = RiskFactors {
        failure_rate: 0.10,
        flakiness_score: 0.10,
        failure_severity: 0.10,
        dependency_count: 1,
        healing_regression_rate: 0.0,
        security_sensitivity: 0.0,
    };
    let cal_low = calibrate_risk("Low Service", low_factors);
    assert_eq!(cal_low.risk_level, "LOW");
    assert!(cal_low.risk_score < 0.25);

    // 2. MEDIUM risk (0.25..0.50)
    let med_factors = RiskFactors {
        failure_rate: 0.50,
        flakiness_score: 0.30,
        failure_severity: 0.40,
        dependency_count: 2,
        healing_regression_rate: 0.10,
        security_sensitivity: 0.0,
    };
    let cal_med = calibrate_risk("Med Service", med_factors);
    assert_eq!(cal_med.risk_level, "MEDIUM");
    assert!(cal_med.risk_score >= 0.25 && cal_med.risk_score < 0.50);

    // 3. HIGH risk (0.50..0.70)
    let high_factors = RiskFactors {
        failure_rate: 0.70,
        flakiness_score: 0.60,
        failure_severity: 0.80,
        dependency_count: 5,
        healing_regression_rate: 0.0,
        security_sensitivity: 0.0,
    };
    let cal_high = calibrate_risk("High Service", high_factors);
    assert_eq!(cal_high.risk_level, "HIGH");
    assert!(cal_high.risk_score >= 0.50 && cal_high.risk_score < 0.70);
}

#[test]
fn test_31_intelligence_cannot_bypass_policy() {
    let rec = score_healing_strategy(&FailureCategory::SelectorNotFound, &[]);
    assert_eq!(rec.recommended_strategy, "selector_update");
}

// The eight tests below used to have empty bodies whose only content was the
// comment `// Structural proof`, while their names asserted hard security
// properties — "no arbitrary code execution", "no autonomous healing loop",
// "intelligence cannot bypass RBAC". An empty body proves nothing about any of
// them: `test_33_intelligence_cannot_directly_execute() { /* nothing */ }`
// passes identically if the property it names is false. Worse, `test_32` sat
// next to `test_31_intelligence_cannot_bypass_policy`, which *does* assert,
// which made the gap invisible.
//
// They are replaced here by tests that actually assert something, or deleted
// where this module cannot honestly assert the claim. Where a property is
// genuinely enforced elsewhere, that is named rather than re-asserted weakly.
//
// `test_34_no_autonomous_healing_loop`, `test_33_intelligence_cannot_directly_execute`
// and `test_38_no_second_ai_router` are DELETED, not rewritten: each needs a
// live `AgentExecutionDispatcher` / `AiInfrastructure` and an approval row to be
// worth testing, which belongs to `agent_execution_tests.rs` and
// `governed_healing_tests.rs`. Asserting something weaker here to keep the
// names would have recreated the original problem. Their real coverage is
// `agent_execution_tests.rs::test_27_cross_project_approval_rejected`,
// `api_tests.rs::aud005_approve_without_validate_first_is_rejected_with_409`,
// and `api_tests.rs::phase2_1_handoff_with_an_unregistered_executor_name_is_a_real_honest_infra_error_not_a_silent_simulate`.

#[tokio::test]
async fn test_32_intelligence_cannot_bypass_rbac() {
    // The property this name claims is really "telemetry is scoped, and the
    // intelligence layer cannot be addressed by another project's identifier".
    // What is assertable here, without a live `AuthContext`, is the scoping
    // half: every telemetry record carries the project it was ingested for, and
    // reading back is scoped to that project rather than returning everything.
    let (store, _dir) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store.clone());

    let make = |project: &str, run: Uuid, task_name: &str| {
        let mut task = TaskState::new(task_name, "Login Test");
        task.started_at = Some(Utc::now());
        task.finished_at = Some(Utc::now());
        task.status = TaskStatus::Failed;
        task.reason = Some("Element not found: #submit-button".to_string());
        ExecutionTelemetry::from_task_state(
            project,
            run,
            &task,
            "playwright",
            "staging",
            None,
            vec!["ev-1".to_string()],
            Some("#submit-button".to_string()),
            None,
            None,
        )
    };

    let mine = make("proj-mine", Uuid::new_v4(), "task-a");
    let theirs = make("proj-theirs", Uuid::new_v4(), "task-b");
    assert_eq!(mine.project_id, "proj-mine");

    service.record_execution_telemetry(mine).await.unwrap();
    service.record_execution_telemetry(theirs).await.unwrap();

    let mine_rows = store
        .list_execution_telemetry("proj-mine", 100)
        .await
        .unwrap();
    assert_eq!(mine_rows.len(), 1, "one record was written for proj-mine");
    assert_eq!(
        mine_rows[0].project_id, "proj-mine",
        "reading one project's telemetry must never return another project's rows"
    );
}

#[test]
fn test_35_no_source_file_mutation() {
    // A source-level invariant, asserted for real: this module computes
    // fingerprints, clusters and trends. It must not contain a filesystem write
    // or a process spawn, because it runs on the ingest path of every completed
    // run. Reading its own source is the honest way to assert that here.
    let source = include_str!("adaptive_intelligence.rs");
    for forbidden in [
        "std::fs::write",
        "fs::write",
        "File::create",
        "std::fs::remove",
        "OpenOptions",
    ] {
        assert!(
            !source.contains(forbidden),
            "adaptive_intelligence.rs must not write to the filesystem ({forbidden} found)"
        );
    }
}

#[test]
fn test_36_no_arbitrary_code_execution() {
    // The real, enforceable version of "no arbitrary code execution": the closed
    // Action AST has no variant that can carry a shell command, and an action
    // name outside that vocabulary is a deserialization failure rather than
    // something executed or coerced. An AI response naming `EXEC_SHELL` is
    // rejected by serde, so no downstream component ever sees a shell verb.
    let hostile = serde_json::json!({
        "type": "EXEC_SHELL",
        "command": "rm -rf /"
    });
    let parsed: Result<crate::execution_plan::Action, _> = serde_json::from_value(hostile);
    assert!(
        parsed.is_err(),
        "an action outside the closed vocabulary must fail to deserialize, not be executed or coerced"
    );

    // And the vocabulary itself has no code-execution verb in it at all.
    let source = include_str!("execution_plan.rs");
    for verb in ["Shell", "Exec", "Eval", "Script", "Python"] {
        assert!(
            !source.contains(&format!("{verb} {{")),
            "the closed Action AST must not gain a `{verb}` variant (execution_plan.rs)"
        );
    }
}

#[test]
fn test_37_no_executor_bypass() {
    // A caller-supplied executor name can only ever reach one of these. An
    // unrecognised name is a real error, never a silent substitution — which is
    // what stops a request naming a "tool" that does not exist from quietly
    // running something else.
    let registry =
        crate::executor::ExecutorRegistry::new(Arc::new(crate::executor::SimulatedExecutor))
            .register("playwright", Arc::new(NoopExecutor))
            .register("compiled-api", Arc::new(NoopExecutor));
    assert_eq!(
        registry.registered_kinds(),
        vec!["compiled-api".to_string(), "playwright".to_string()],
        "only the kinds actually registered may be routable"
    );

    let task = TaskDef {
        id: "t".into(),
        name: "t".into(),
        executor: Some("not-a-real-executor".into()),
        ..Default::default()
    };
    assert!(
        registry.resolve(&task).is_err(),
        "an unregistered executor name must be an error, never a silent fallback"
    );

    // A task naming no executor at all is still the pre-existing, disclosed
    // default — the boundary the above rule must not break.
    let unnamed = TaskDef {
        id: "t".into(),
        name: "t".into(),
        executor: None,
        ..Default::default()
    };
    assert!(registry.resolve(&unnamed).is_ok());
}

#[test]
fn test_39_no_python_openjarvis() {
    // Asserted as the source invariant it actually is: this crate's dependency
    // set contains no Python interpreter bridge and no OpenJarvis dependency, so
    // the AIQA runtime cannot shell out to one. Reading `Cargo.toml` is the
    // honest form of this claim — an empty test body is not.
    let manifest = include_str!("../Cargo.toml");
    for forbidden in ["pyo3", "openjarvis", "pyo3-async-runtimes", "python"] {
        assert!(
            !manifest.to_lowercase().contains(forbidden),
            "Cargo.toml must not declare a Python/OpenJarvis dependency ({forbidden})"
        );
    }
}

/// A no-op `Executor` used only to populate the registry `test_37` inspects.
struct NoopExecutor;

#[async_trait::async_trait]
impl crate::executor::Executor for NoopExecutor {
    async fn execute(&self, _task: &crate::domain::TaskDef) -> crate::executor::ExecutionOutcome {
        crate::executor::ExecutionOutcome {
            status: TaskStatus::Passed,
            reason: None,
            evidence: None,
        }
    }
}

#[tokio::test]
async fn test_40_existing_phase1_5_tests_remain_functional() {
    let (store, _dir) = setup_test_store().await;

    let node_id = store
        .upsert_quality_node("p", "test", "tc-1", "Test Case 1")
        .await
        .unwrap();
    assert_ne!(node_id, Uuid::nil());
}

// ============================================================================
// R1b -- the planning-feedback endpoint contract
// ============================================================================

/// ## What this test does and does not prove
///
/// `generate_test_plan_scoped` grounds a plan in two readers and must not return a plan it could
/// not ground. This pins that endpoint contract: a store that cannot answer produces an error
/// response, never a `200` carrying a plan.
///
/// **It does not isolate the `planning_signals` read inside `generate_planning_feedback`, and this
/// test was checked by mutation to confirm that.** Removing that read's propagation — reverting it
/// to `unwrap_or_default()` — leaves this test still passing, because
/// `generate_planning_feedback` reads `list_execution_telemetry` *first*, and a closed pool fails
/// there before `planning_signals` is ever reached. A closed pool cannot single out one read in a
/// function that makes several; distinguishing them would need a mock the store does not offer.
///
/// The `planning_signals` propagation itself is pinned structurally instead: the audit in the R1b
/// report records zero remaining masked store reads across every `Result`-returning view, and
/// `qa_memory_risk_tests::r1b_no_affected_view_reports_a_failed_read_as_an_empty_answer` drives
/// `planning_signals` directly against a closed pool. That test was confirmed to fail when
/// `bug_history` was still masking, so it discriminates.
///
/// Asserted here anyway, because the endpoint contract is worth pinning regardless of which read
/// fails first: the store is closed, and the caller must not receive a plan.
#[tokio::test]
async fn r1b_planning_feedback_reports_a_failed_read_rather_than_no_signals() {
    let (store, _d) = setup_test_store().await;
    let service = AdaptiveIntelligenceService::new(store.clone());

    // --- state 1: healthy store, genuinely no history ---
    let empty = service
        .generate_planning_feedback("proj-no-history")
        .await
        .expect("a project with no history is not a failure");
    assert!(
        empty.planning_signals.is_empty(),
        "no history must still produce an honest empty signal list: {:?}",
        empty.planning_signals
    );

    // --- state 2: the store cannot answer ---
    store.pool.close().await;

    let failed = service.generate_planning_feedback("proj-no-history").await;
    assert!(
        failed.is_err(),
        "a failed read must be an error, not an empty signal list: {:?}",
        failed.as_ref().map(|f| f.planning_signals.len())
    );
}
