//! Phase 5 (QA Intelligence Memory) — module & release risk.
//!
//! Master plan §7 "Testing", one test per line:
//!
//! | Required case                             | Test |
//! |-------------------------------------------|------|
//! | Module Risk with historical failure data  | `a_module_with_failing_history_reports_its_measured_factors` |
//! | Release Risk with regression history      | `a_release_whose_failures_got_worse_is_reported_as_worsening` |
//! | INSUFFICIENT_DATA when no history         | `no_history_means_no_risk_conclusion_not_a_low_one` (+3 more) |
//! | Risk increases with recent failures       | `a_module_with_more_recent_failures_weighs_more` |
//! | Risk scores are explainable               | `every_factor_names_itself_its_weight_and_its_evidence` |
//! | Comparisons work across releases/modules  | `releases_are_ranked_by_their_measured_contribution` |
//!
//! Plus the rule §7 states as a preference and this makes an invariant: a 0–1 risk
//! score is never emitted from partial evidence, and the coverage of the risk model
//! is always reported as a number.

use crate::adaptive_intelligence::ExecutionTelemetry;
use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskStatus};
use crate::qa_memory::{InsufficientData, QaMemoryReader};
use crate::qa_memory_risk::{FACTOR_WEIGHTS, STRUCTURALLY_UNAVAILABLE_WEIGHT};
use crate::store::Store;
use uuid::Uuid;

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("q5.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 5 fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

/// One execution, optionally on a release, optionally with a failure category.
async fn execute(
    store: &Store,
    pid: &str,
    case_id: Uuid,
    release: Option<Uuid>,
    status: TaskStatus,
    category: Option<&str>,
) -> Uuid {
    let run_id = Uuid::new_v4();
    let task_id = format!("t-{run_id}");
    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "p5".into(),
                tasks: vec![TaskDef {
                    id: task_id.clone(),
                    name: "Test Case: p5".into(),
                    prerequisites: Vec::new(),
                    simulate: Some(SimSpec {
                        outcome: SimOutcome::Pass,
                        delay_ms: 1,
                    }),
                    executor: Some("simulated".into()),
                    params: serde_json::json!({}),
                    test_case_id: Some(case_id.to_string()),
                }],
                concurrency: 1,
            },
            Some(pid),
            Some("fixture"),
            RunState::Completed,
            None,
            release.map(|r| r.to_string()).as_deref(),
        )
        .await
        .expect("run");

    let now = chrono::Utc::now();
    store
        .insert_execution_telemetry(&ExecutionTelemetry {
            id: Uuid::new_v4(),
            project_id: pid.to_string(),
            run_id,
            task_id,
            task_name: "Test Case: p5".into(),
            test_case_id: Some(case_id),
            execution_id: None,
            executor: "simulated".into(),
            environment: "test".into(),
            start_time: now,
            end_time: Some(now),
            duration_ms: 5u64,
            status,
            retry_count: 0,
            healing_attempt_count: 0,
            failure_category: category.map(crate::adaptive_intelligence::FailureCategory::from_str),
            failure_fingerprint: None,
            error_message: None,
            evidence_refs: Vec::new(),
            assertion_result: None,
            http_status: None,
            selector: None,
            verification_outcome: None,
            created_at: now,
        })
        .await
        .expect("telemetry");
    run_id
}

// ------------------------------------------------- module risk, real history

#[tokio::test]
async fn a_module_with_failing_history_reports_its_measured_factors() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Module risk").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "payment", "Payment", "Payment")
        .await
        .expect("link");

    for _ in 0..3 {
        execute(
            &store,
            &pid,
            case,
            None,
            TaskStatus::Failed,
            Some("AUTHENTICATION_FAILURE"),
        )
        .await;
    }
    for _ in 0..2 {
        execute(&store, &pid, case, None, TaskStatus::Passed, None).await;
    }

    let view = reader
        .module_risk(&pid, "payment")
        .await
        .expect("module_risk")
        .view
        .expect("found");

    assert_eq!(view.executions, 5);
    assert_eq!(view.failures, 3);

    let fr = view
        .factors
        .iter()
        .find(|f| f.name == "failure_rate")
        .expect("failure_rate factor");
    let measured = fr.measured.expect("measured");
    assert!(
        (measured - 0.6).abs() < 0.001,
        "3 of 5 failed, so failure_rate must be 0.6, was {measured}"
    );
    assert_eq!(fr.evidence, 5, "the factor must cite its rows");
    assert!(
        fr.basis.contains("3 failed of 5"),
        "the basis must carry the real numbers: {}",
        fr.basis
    );

    // Severity is measured here because the failures carry a category.
    assert!(
        view.factors
            .iter()
            .find(|f| f.name == "failure_severity")
            .and_then(|f| f.measured)
            .is_some(),
        "categorised failures must produce a measured severity"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn no_zero_to_one_score_is_invented_from_partial_evidence() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No invented score").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");
    // Every execution fails with the most severe category: the maximum the
    // measurable factors can express.
    for _ in 0..10 {
        execute(
            &store,
            &pid,
            case,
            None,
            TaskStatus::Failed,
            Some("DATA_FAILURE"),
        )
        .await;
    }

    let view = reader
        .module_risk(&pid, "m")
        .await
        .expect("module_risk")
        .view
        .expect("found");

    assert!(
        view.calibration.is_none(),
        "three of the six factors have nowhere to come from, so a 0-1 score would \
         silently assert 'no dependency risk, no security risk'"
    );
    assert!(view.partial);

    // The coverage of the model is stated as a number, not left to be inferred.
    assert!(
        (view.measured_weight - 0.70).abs() < 0.001,
        "measured_weight was {}",
        view.measured_weight
    );
    assert!(
        (view.unmeasured_weight - STRUCTURALLY_UNAVAILABLE_WEIGHT).abs() < 0.001,
        "unmeasured_weight was {}",
        view.unmeasured_weight
    );
    assert!(
        (view.measured_weight + view.unmeasured_weight - 1.0).abs() < 0.001,
        "the two must account for the whole model"
    );

    // And the measured part still reports real numbers. The exact value is
    // asserted rather than a threshold, because the arithmetic is the point:
    //   failure_rate    1.0 x 0.35 = 0.35   (10 of 10 failed)
    //   flakiness_score 0.0 x 0.20 = 0.00   (consistently failing is NOT flaky)
    //   failure_severity 1.0 x 0.15 = 0.15   (every failure is DATA_FAILURE)
    // = 0.50 of the 0.70 measured weight.
    //
    // The zero flakiness is a property of the existing `evaluate_flakiness`, not a
    // defect here: the risk model spends 20% of its weight on flakiness, so a
    // module that fails every single time scores *lower* on the measurable factors
    // than one that fails intermittently. That is the existing model's shape and
    // changing it is out of this phase's scope, so it is pinned here instead.
    assert!(
        (view.weighted_contribution - 0.50).abs() < 0.001,
        "weighted contribution was {}, expected 0.35 (rate) + 0.00 (not flaky) \
         + 0.15 (max severity)",
        view.weighted_contribution
    );
    assert!(
        view.factors
            .iter()
            .find(|f| f.name == "flakiness_score")
            .and_then(|f| f.measured)
            .map(|v| (v - 0.0).abs() < 1e-9)
            .unwrap_or(false),
        "ten identical failures must not register as flaky"
    );
    assert!(
        view.signals.iter().any(|s| s.kind == "partial_score"),
        "the missing 30% must be stated, not left for a reader to work out"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn every_factor_names_itself_its_weight_and_its_evidence() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Explainable").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");
    for _ in 0..4 {
        execute(
            &store,
            &pid,
            case,
            None,
            TaskStatus::Failed,
            Some("TIMEOUT"),
        )
        .await;
    }

    let view = reader
        .module_risk(&pid, "m")
        .await
        .expect("module_risk")
        .view
        .expect("found");
    assert_eq!(
        view.factors.len(),
        FACTOR_WEIGHTS.len(),
        "every factor of the model must be reported, measured or not"
    );

    for f in &view.factors {
        let expected = FACTOR_WEIGHTS
            .iter()
            .find(|(n, _)| *n == f.name)
            .map(|(_, w)| *w)
            .unwrap_or_else(|| panic!("{} is not a factor of the risk model", f.name));
        assert!(
            (f.weight - expected).abs() < 1e-9,
            "{} carries weight {} but the model gives it {expected}",
            f.name,
            f.weight
        );
        assert!(
            !f.basis.is_empty(),
            "{} must say what it was computed from, or why not",
            f.name
        );
        if f.measured.is_some() {
            assert!(f.evidence > 0, "{} claims a value with no rows", f.name);
        } else {
            assert_eq!(
                f.evidence, 0,
                "{} is unmeasured, so it cites nothing",
                f.name
            );
            assert!(
                f.contribution() == 0.0,
                "{} is unmeasured, so it must contribute nothing",
                f.name
            );
        }
    }
    store.pool.close().await;
}

#[tokio::test]
async fn the_weight_table_sums_to_one_and_matches_the_documented_formula() {
    // If `calibrate_risk` is ever reweighted, this fails rather than silently
    // producing a differently-scaled number here.
    let total: f64 = FACTOR_WEIGHTS.iter().map(|(_, w)| w).sum();
    assert!(
        (total - 1.0).abs() < 1e-9,
        "the transcribed weights must sum to 1.0, they sum to {total}"
    );
    for (name, weight) in FACTOR_WEIGHTS {
        let expected = match name {
            "failure_rate" => 0.35,
            "flakiness_score" => 0.20,
            "failure_severity" => 0.15,
            "dependency_count" | "healing_regression_rate" | "security_sensitivity" => 0.10,
            other => panic!("unexpected factor {other}"),
        };
        assert!(
            (weight - expected).abs() < 1e-9,
            "{name} is {weight}, not {expected}"
        );
    }
}

// --------------------------------------------- risk increases with failures

#[tokio::test]
async fn a_module_with_more_recent_failures_weighs_more() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Ordering").await;
    let reader = QaMemoryReader::new(store.clone());

    // "clean": 10 executions, none failed. "broken": 10 executions, all failed.
    let clean = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &clean.to_string(), "clean", "Clean", "Clean")
        .await
        .expect("link");
    for _ in 0..10 {
        execute(&store, &pid, clean, None, TaskStatus::Passed, None).await;
    }

    let broken = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &broken.to_string(), "broken", "Broken", "Broken")
        .await
        .expect("link");
    for _ in 0..10 {
        execute(
            &store,
            &pid,
            broken,
            None,
            TaskStatus::Failed,
            Some("SERVER_ERROR"),
        )
        .await;
    }

    let a = reader
        .module_risk(&pid, "broken")
        .await
        .expect("module_risk")
        .view
        .unwrap();
    let b = reader
        .module_risk(&pid, "clean")
        .await
        .expect("module_risk")
        .view
        .unwrap();

    assert!(
        a.weighted_contribution > b.weighted_contribution,
        "an always-failing module must weigh more than an always-passing one: \
         {} vs {}",
        a.weighted_contribution,
        b.weighted_contribution
    );
    assert_eq!(b.failures, 0);
    assert_eq!(
        b.weighted_contribution, 0.0,
        "no failures means nothing to weigh"
    );
    store.pool.close().await;
}

// ----------------------------------------------------------- release risk

#[tokio::test]
async fn a_release_with_regression_history_reports_its_own_counts() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Release risk").await;
    let reader = QaMemoryReader::new(store.clone());
    let release = store
        .create_release(&pid, Some("2026.10"), Some("2026.10.1"), None, None)
        .await
        .unwrap();
    let case = Uuid::new_v4();

    for _ in 0..4 {
        execute(
            &store,
            &pid,
            case,
            Some(release.id),
            TaskStatus::Failed,
            Some("DEPENDENCY_FAILURE"),
        )
        .await;
    }

    let view = reader
        .release_risk(&pid, release.id)
        .await
        .expect("release_risk")
        .view
        .expect("found");
    assert_eq!(view.label.as_deref(), Some("2026.10"));
    assert_eq!(view.version.as_deref(), Some("2026.10.1"));
    assert_eq!(view.executions, 4);
    assert_eq!(view.failures, 4);
    assert_eq!(view.runs, 4);
    assert!(
        (view.failure_rate.unwrap() - 100.0).abs() < 0.1,
        "4 of 4 failed"
    );
    assert!(
        view.trend.is_none(),
        "one release cannot show a trend; got {:?}",
        view.trend
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_release_whose_failures_got_worse_is_reported_as_worsening() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trend").await;
    let reader = QaMemoryReader::new(store.clone());

    // Three releases: two clean, one failing everything. Created in order so the
    // trend's "oldest first" sort has something to work with.
    let mut ids = Vec::new();
    for label in ["2026.09", "2026.10", "2026.11"] {
        ids.push(
            store
                .create_release(&pid, Some(label), None, None, None)
                .await
                .unwrap()
                .id,
        );
    }
    let case = Uuid::new_v4();

    for _ in 0..4 {
        execute(&store, &pid, case, Some(ids[0]), TaskStatus::Passed, None).await;
        execute(&store, &pid, case, Some(ids[1]), TaskStatus::Passed, None).await;
        execute(
            &store,
            &pid,
            case,
            Some(ids[2]),
            TaskStatus::Failed,
            Some("SERVER_ERROR"),
        )
        .await;
    }

    let view = reader
        .release_risk(&pid, ids[2])
        .await
        .expect("release_risk")
        .view
        .expect("found");

    assert_eq!(
        view.trend.as_deref(),
        Some("worsening"),
        "two clean releases then an all-failing one is a worsening trend: {:?} / {:?}",
        view.trend,
        view.trend_points
    );
    assert_eq!(
        view.trend_points.len(),
        3,
        "every release with executions appears, including the target"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_release_whose_failures_cleared_is_reported_as_improving() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Improving").await;
    let reader = QaMemoryReader::new(store.clone());

    let mut ids = Vec::new();
    for label in ["2026.09", "2026.10", "2026.11"] {
        ids.push(
            store
                .create_release(&pid, Some(label), None, None, None)
                .await
                .unwrap()
                .id,
        );
    }
    let case = Uuid::new_v4();

    for _ in 0..4 {
        execute(
            &store,
            &pid,
            case,
            Some(ids[0]),
            TaskStatus::Failed,
            Some("SERVER_ERROR"),
        )
        .await;
        execute(
            &store,
            &pid,
            case,
            Some(ids[1]),
            TaskStatus::Failed,
            Some("SERVER_ERROR"),
        )
        .await;
        execute(&store, &pid, case, Some(ids[2]), TaskStatus::Passed, None).await;
    }

    let view = reader
        .release_risk(&pid, ids[2])
        .await
        .expect("release_risk")
        .view
        .expect("found");
    assert_eq!(
        view.trend.as_deref(),
        Some("improving"),
        "two failing releases then a clean one is an improving trend: {:?}",
        view.trend
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_release_needing_the_same_rate_every_time_is_steady() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Steady").await;
    let reader = QaMemoryReader::new(store.clone());

    let mut ids = Vec::new();
    for label in ["2026.09", "2026.10", "2026.11"] {
        ids.push(
            store
                .create_release(&pid, Some(label), None, None, None)
                .await
                .unwrap()
                .id,
        );
    }
    let case = Uuid::new_v4();

    // Same 50% in every release: movement in either direction would be noise.
    for _ in 0..4 {
        for id in &ids {
            execute(&store, &pid, case, Some(*id), TaskStatus::Passed, None).await;
            execute(
                &store,
                &pid,
                case,
                Some(*id),
                TaskStatus::Failed,
                Some("CLIENT_ERROR"),
            )
            .await;
        }
    }

    let view = reader
        .release_risk(&pid, ids[2])
        .await
        .expect("release_risk")
        .view
        .unwrap();
    assert_eq!(
        view.trend.as_deref(),
        Some("steady"),
        "an unchanged failure rate is not a trend: {:?}",
        view.trend
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_release_with_no_executions_is_never_counted_as_a_clean_release() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Unrun release").await;
    let reader = QaMemoryReader::new(store.clone());

    let run = store
        .create_release(&pid, Some("exercised"), None, None, None)
        .await
        .unwrap();
    let empty = store
        .create_release(&pid, Some("never-run"), None, None, None)
        .await
        .unwrap();
    let case = Uuid::new_v4();
    for _ in 0..3 {
        execute(&store, &pid, case, Some(run.id), TaskStatus::Passed, None).await;
    }

    let view = reader
        .release_risk(&pid, run.id)
        .await
        .expect("release_risk")
        .view
        .unwrap();
    assert!(
        !view
            .trend_points
            .iter()
            .any(|p| p.release_id == empty.id.to_string()),
        "a release that never ran must not appear as a 0% failure release: {:?}",
        view.trend_points
    );

    // And asked about directly, it says it has nothing rather than reporting zero.
    let direct = reader
        .release_risk(&pid, empty.id)
        .await
        .expect("release_risk");
    assert_eq!(
        direct.insufficient_data,
        Some(InsufficientData::NoMatchingHistory)
    );
    assert!(direct.view.is_none());
    store.pool.close().await;
}

// ------------------------------------------------------------ comparisons

#[tokio::test]
async fn releases_are_ranked_by_their_measured_contribution() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Ranking").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();

    let mild = store
        .create_release(&pid, Some("mild"), None, None, None)
        .await
        .unwrap();
    let bad = store
        .create_release(&pid, Some("bad"), None, None, None)
        .await
        .unwrap();

    for _ in 0..3 {
        execute(&store, &pid, case, Some(mild.id), TaskStatus::Passed, None).await;
        execute(
            &store,
            &pid,
            case,
            Some(mild.id),
            TaskStatus::Failed,
            Some("CLIENT_ERROR"),
        )
        .await;
        execute(&store, &pid, case, Some(bad.id), TaskStatus::Passed, None).await;
        execute(
            &store,
            &pid,
            case,
            Some(bad.id),
            TaskStatus::Failed,
            Some("AUTHENTICATION_FAILURE"),
        )
        .await;
    }

    let overview = reader
        .release_risk_overview(&pid)
        .await
        .expect("release_risk_overview");
    assert_eq!(overview.len(), 2, "both releases have executions");
    assert_eq!(
        overview[0].label.as_deref(),
        Some("bad"),
        "the release with the heavier measured contribution ranks first: {:?}",
        overview.iter().map(|r| &r.label).collect::<Vec<_>>()
    );
    assert!(
        overview[0].weighted_contribution >= overview[1].weighted_contribution,
        "ranking must follow the measurement"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_module_that_does_not_exist_is_unknown_not_low_risk() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Unknown module").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "real", "Real", "Real")
        .await
        .expect("link");
    for _ in 0..3 {
        execute(&store, &pid, case, None, TaskStatus::Passed, None).await;
    }

    let result = reader
        .module_risk(&pid, "no-such-module")
        .await
        .expect("module_risk");
    assert_eq!(
        result.insufficient_data,
        Some(InsufficientData::UnknownSubject),
        "a module that does not exist must not be reported as low risk"
    );
    store.pool.close().await;
}

// ------------------------------------------------- insufficient data cases

#[tokio::test]
async fn no_history_means_no_risk_conclusion_not_a_low_one() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Empty").await;
    let reader = QaMemoryReader::new(store.clone());

    let module = reader
        .module_risk(&pid, "anything")
        .await
        .expect("module_risk");
    assert_eq!(
        module.insufficient_data,
        Some(InsufficientData::NoExecutions)
    );
    assert!(module.view.is_none());

    let release = reader
        .release_risk(&pid, Uuid::new_v4())
        .await
        .expect("release_risk");
    assert_eq!(
        release.insufficient_data,
        Some(InsufficientData::UnknownSubject),
        "an unknown release is unknown even on an empty project"
    );
    assert!(reader
        .release_risk_overview(&pid)
        .await
        .expect("release_risk_overview")
        .is_empty());
    store.pool.close().await;
}

#[tokio::test]
async fn a_module_whose_cases_never_ran_reports_no_matching_history() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Never ran").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");

    let result = reader.module_risk(&pid, "m").await.expect("module_risk");
    assert_eq!(
        result.insufficient_data,
        Some(InsufficientData::NoExecutions)
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_failure_without_a_category_leaves_severity_unmeasured() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No category").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");
    for _ in 0..3 {
        execute(&store, &pid, case, None, TaskStatus::Failed, None).await;
    }

    let view = reader
        .module_risk(&pid, "m")
        .await
        .expect("module_risk")
        .view
        .unwrap();
    let sev = view
        .factors
        .iter()
        .find(|f| f.name == "failure_severity")
        .expect("factor");
    assert!(
        sev.measured.is_none(),
        "with no category there is no severity to report"
    );
    assert!(
        sev.basis.contains("no failure"),
        "and it must say why: {}",
        sev.basis
    );
    // The failure rate is still measured and still reported.
    assert!(view
        .factors
        .iter()
        .find(|f| f.name == "failure_rate")
        .and_then(|f| f.measured)
        .is_some());
    store.pool.close().await;
}

// ---------------------------------------------------------------- isolation

#[tokio::test]
async fn one_projects_risk_is_never_visible_from_another() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Mine").await;
    let theirs = project(&store, "Theirs").await;
    let reader = QaMemoryReader::new(store.clone());

    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&mine, &case.to_string(), "secret", "Secret", "Secret")
        .await
        .expect("link");
    let release = store
        .create_release(&mine, Some("mine-only"), None, None, None)
        .await
        .unwrap();
    for _ in 0..4 {
        execute(
            &store,
            &mine,
            case,
            Some(release.id),
            TaskStatus::Failed,
            Some("DATA_FAILURE"),
        )
        .await;
    }

    assert!(
        reader
            .module_risk(&mine, "secret")
            .await
            .expect("module_risk")
            .is_found(),
        "the owning project must see its own module"
    );
    assert_eq!(
        reader
            .module_risk(&theirs, "secret")
            .await
            .expect("module_risk")
            .insufficient_data,
        Some(InsufficientData::NoExecutions),
        "another project must not see this module's failures"
    );
    assert_eq!(
        reader
            .release_risk(&theirs, release.id)
            .await
            .expect("release_risk")
            .insufficient_data,
        Some(InsufficientData::UnknownSubject)
    );
    assert!(reader
        .release_risk_overview(&theirs)
        .await
        .expect("release_risk_overview")
        .is_empty());
    store.pool.close().await;
}

// ------------------------------------------------- determinism & no CoT

#[tokio::test]
async fn repeated_risk_reads_return_identical_answers() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Deterministic").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");
    for i in 0..6 {
        execute(
            &store,
            &pid,
            case,
            None,
            if i % 2 == 0 {
                TaskStatus::Failed
            } else {
                TaskStatus::Passed
            },
            Some("TIMEOUT"),
        )
        .await;
    }

    let a =
        serde_json::to_string(&reader.module_risk(&pid, "m").await.expect("module_risk")).unwrap();
    let b =
        serde_json::to_string(&reader.module_risk(&pid, "m").await.expect("module_risk")).unwrap();
    assert_eq!(a, b, "risk must be reproducible from the same stored rows");
    store.pool.close().await;
}

#[tokio::test]
async fn no_risk_view_carries_a_field_that_looks_like_hidden_reasoning() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No CoT").await;
    let reader = QaMemoryReader::new(store.clone());
    let case = Uuid::new_v4();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "m", "M", "M")
        .await
        .expect("link");
    for _ in 0..3 {
        execute(
            &store,
            &pid,
            case,
            None,
            TaskStatus::Failed,
            Some("TIMEOUT"),
        )
        .await;
    }
    let release = store
        .create_release(&pid, Some("r"), None, None, None)
        .await
        .unwrap();

    let rendered = [
        serde_json::to_string(&reader.module_risk(&pid, "m").await.expect("module_risk")).unwrap(),
        serde_json::to_string(
            &reader
                .release_risk(&pid, release.id)
                .await
                .expect("release_risk"),
        )
        .unwrap(),
    ];

    for needle in [
        "chain_of_thought",
        "chainOfThought",
        "reasoning",
        "thinking",
        "scratchpad",
        "internal_notes",
    ] {
        for json in &rendered {
            assert!(
                !json.contains(needle),
                "a risk view exposed {needle}, which §1.5 forbids: {json}"
            );
        }
    }
    store.pool.close().await;
}

// ============================================================================
// R1b -- a failed read must never be reported as NO_EXECUTIONS
// ============================================================================

/// The three states that were previously conflated, all three now distinguished.
///
/// Before R1b, `QaMemoryReader::telemetry_for` and `test_evidence` returned plain collections
/// and swallowed their reads with `unwrap_or_default()`. A closed pool therefore produced an
/// **empty** evidence set, and the `evidence.all.is_empty()` guard that produces `NO_EXECUTIONS`
/// was consulted against evidence that had failed to load. A project with real executions was
/// reported as having none.
///
/// `store.pool.close()` is a genuine read failure -- the same technique `engine_tests` uses -- so
/// nothing here depends on a mock or on timing.
#[tokio::test]
async fn r1b_a_failed_read_is_never_reported_as_no_executions() {
    // --- state 1: healthy store, genuinely no history ---
    let (store, _d) = temp_store().await;
    let bare = project(&store, "R1b NoHistory").await;
    let reader = QaMemoryReader::new(store.clone());

    let per_case = reader
        .test_risk(&bare, &Uuid::new_v4().to_string())
        .await
        .expect("an empty project is not a failure");
    assert_eq!(
        per_case.insufficient_data,
        Some(InsufficientData::NoExecutions),
        "a project with no history still reports NO_EXECUTIONS, and that behaviour is preserved"
    );
    assert!(
        per_case.view.is_none(),
        "and still invents no ordering from no history"
    );

    // --- state 2: healthy store, real history ---
    let (store2, _d2) = temp_store().await;
    let with_history = project(&store2, "R1b WithHistory").await;
    let widget = Uuid::new_v4();
    execute(
        &store2,
        &with_history,
        widget,
        None,
        TaskStatus::Failed,
        Some("assertion"),
    )
    .await;
    let reader2 = QaMemoryReader::new(store2.clone());

    // `test_stability` matches on `execution_telemetry.test_case_id`, which `execute` set to
    // the `case_id` passed in -- so the lookup key is the case UUID string, not a task name.
    let real = reader2
        .test_stability(&with_history, &widget.to_string())
        .await
        .expect("a healthy read must succeed");
    assert!(
        real.view.is_some(),
        "with recorded executions a real view must be produced: {real:?}"
    );

    // --- state 3: the store cannot answer ---
    store2.pool.close().await;

    let failed = reader2
        .test_stability(&with_history, &widget.to_string())
        .await;
    assert!(
        failed.is_err(),
        "a failed read must be an error, never NO_EXECUTIONS for a project with history: {:?}",
        failed.as_ref().map(|r| r.insufficient_data)
    );
    store.pool.close().await;
    store2.pool.close().await;
}

/// The gate is that *no* caller may still convert a required read failure into an empty answer, so
/// every view whose return type changed is exercised here.
///
/// Each is asserted twice: once on a healthy store (so the fixture is known good, and the later
/// failure cannot pass for the wrong reason) and once with the pool closed.
#[tokio::test]
async fn r1b_no_affected_view_reports_a_failed_read_as_an_empty_answer() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "R1b AllViews").await;
    let gadget = Uuid::new_v4();
    execute(
        &store,
        &pid,
        gadget,
        None,
        TaskStatus::Failed,
        Some("assertion"),
    )
    .await;
    let reader = QaMemoryReader::new(store.clone());
    let release_id = Uuid::new_v4();
    let case_id = Uuid::new_v4().to_string();

    // Healthy: every one of these must succeed, or the fixture is wrong.
    reader.bug_history(&pid).await.expect("bug_history");
    reader
        .test_stability(&pid, "gadget")
        .await
        .expect("test_stability");
    reader
        .module_risk_history(&pid, "m1")
        .await
        .expect("module_risk_history");
    reader
        .release_history(&pid, release_id)
        .await
        .expect("release_history");
    reader
        .requirement_history(&pid, "REQ-1")
        .await
        .expect("requirement_history");
    reader
        .project_memory_summary(&pid)
        .await
        .expect("project_memory_summary");
    reader
        .planning_signals(&pid)
        .await
        .expect("planning_signals");
    reader.module_risk(&pid, "m1").await.expect("module_risk");
    reader.test_risk(&pid, &case_id).await.expect("test_risk");
    reader
        .release_risk(&pid, release_id)
        .await
        .expect("release_risk");
    reader
        .release_risk_overview(&pid)
        .await
        .expect("release_risk_overview");

    store.pool.close().await;

    // Every arm maps to one uniform type so the table has a single type.
    let cases: Vec<(&str, Result<bool, anyhow::Error>)> = vec![
        (
            "test_stability",
            reader
                .test_stability(&pid, &gadget.to_string())
                .await
                .map(|v| v.insufficient_data.is_some()),
        ),
        (
            "module_risk_history",
            reader
                .module_risk_history(&pid, "m1")
                .await
                .map(|v| v.insufficient_data.is_some()),
        ),
        (
            "release_history",
            reader
                .release_history(&pid, release_id)
                .await
                .map(|v| v.insufficient_data.is_some()),
        ),
        (
            "requirement_history",
            reader
                .requirement_history(&pid, "REQ-1")
                .await
                .map(|v| v.insufficient_data.is_some()),
        ),
        (
            "module_risk",
            reader
                .module_risk(&pid, "m1")
                .await
                .map(|v| v.insufficient_data.is_some()),
        ),
        (
            "test_risk",
            reader
                .test_risk(&pid, &case_id)
                .await
                .map(|v| v.insufficient_data.is_some()),
        ),
        (
            "release_risk",
            reader
                .release_risk(&pid, release_id)
                .await
                .map(|v| v.insufficient_data.is_some()),
        ),
        (
            "release_risk_overview",
            reader
                .release_risk_overview(&pid)
                .await
                .map(|v| v.is_empty()),
        ),
        (
            "bug_history",
            reader.bug_history(&pid).await.map(|v| v.is_empty()),
        ),
        (
            "project_memory_summary",
            reader
                .project_memory_summary(&pid)
                .await
                .map(|v| v.executions == 0),
        ),
        (
            "planning_signals",
            reader.planning_signals(&pid).await.map(|v| v.is_empty()),
        ),
    ];
    for (name, res) in cases {
        assert!(
            res.is_err(),
            "{name} converted a failed read into an empty answer: {:?}",
            res.as_ref().ok()
        );
    }
    store.pool.close().await;
}

/// R1b must not have disturbed Phase 4's refusal to score a partially measured model, nor turned an
/// unmeasured factor into a measured zero while propagating the error channel.
#[tokio::test]
async fn r1b_unmeasured_factors_stay_unmeasured_through_the_new_error_path() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "R1b Unmeasured").await;
    let thing = Uuid::new_v4();
    execute(
        &store,
        &pid,
        thing,
        None,
        TaskStatus::Failed,
        Some("assertion"),
    )
    .await;
    let reader = QaMemoryReader::new(store.clone());

    let risk = reader
        .test_risk(&pid, &thing.to_string())
        .await
        .expect("a healthy read must succeed");
    let view = risk.view.as_ref().expect("a risk view");
    assert!(
        view.calibration.is_none(),
        "Phase 4 still refuses a calibration from a partially measured model"
    );
    assert!(
        view.unmeasured_weight > 0.0,
        "and still reports the unbacked weight rather than absorbing it"
    );
    assert!(
        view.factors
            .iter()
            .any(|f| f.name == "security_sensitivity" && f.measured.is_none()),
        "security_sensitivity must still be unmeasured with a reason, never a fabricated zero"
    );
    store.pool.close().await;
}
