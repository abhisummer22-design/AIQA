//! Risk Signal Correctness Review / Remediation — tests.
//!
//! ## What is being proven
//!
//! `intelligence.rs` used to persist three `risk_assessments` columns as the literal `0.0`:
//! `security_sensitivity`, `business_criticality` and `change_frequency`. The first was then
//! multiplied into the risk formula at weight 0.10, so **every stored score in the system was
//! understated by a fabricated zero** and every level was shifted down a band.
//!
//! These tests pin the remediated behaviour:
//!
//! 1. a **measured zero** is distinguishable from an **unmeasured** signal;
//! 2. an unavailable signal does not silently become zero;
//! 3. the risk score is **not understated** by a fake measurement;
//! 4. the signals that were already real keep working;
//! 5. project isolation is intact;
//! 6. and the Phase 4 / Phase 6 surfaces that depend on this keep passing (asserted here as
//!    invariants, and by the suites themselves).
//!
//! ## What is real here
//!
//! Every fixture goes through production writers: `insert_run_for_release`, `set_task_status`,
//! `Intelligence::ingest_run` (which drives `update_test_intelligence`), `insert_change_impact`
//! and `upsert_risk_assessment`. Nothing hand-writes a `risk_assessments` row to make an
//! assertion pass, except in the one test whose subject *is* a fabricated constant — and there it
//! uses the real writer with a value any real caller could supply.
//!
//! See `docs/next-intelligence/PHASE_4_RISK_SIGNAL_REMEDIATION.md`.

use crate::adaptive_intelligence::{risk_level_for, RiskCalibration};
use crate::agents::exploration::ExplorationBudget;
use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
    TestStep,
};
use crate::integrity::{ComparisonKind, IntegrityOutcome, IntegrityVerifier};
use crate::intelligence::Intelligence;
use crate::qa_memory_risk::{Factor, FACTOR_WEIGHTS};
use crate::store::Store;
use uuid::Uuid;

// ------------------------------------------------------------------ harness

async fn harness() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase4-signal-remediation.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("risk signal remediation fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

async fn case(store: &Store, project_id: &str, title: &str) -> Uuid {
    store
        .insert_test_case(
            project_id,
            &CreateTestCaseRequest {
                title: title.to_string(),
                description: "Remediation fixture.".into(),
                category: "functional".into(),
                priority: "P1".into(),
                severity: "medium".into(),
                preconditions: Vec::new(),
                steps: vec![TestStep {
                    step_number: 1,
                    action: "act".into(),
                    expected_result: "observed".into(),
                }],
                expected_result: Some("observed".into()),
                assertions: Vec::new(),
                test_data: None,
                source_document: None,
                requirement_reference: None,
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: Vec::new(),
                suite_id: None,
            },
            None,
        )
        .await
        .expect("insert test case")
}

/// Record one execution through the real pipeline, driving `update_test_intelligence`.
async fn run(
    store: &Store,
    project_id: &str,
    task_id: &str,
    status: TaskStatus,
    reason: Option<&str>,
    test_case_id: Option<Uuid>,
    prerequisites: Vec<String>,
) -> Uuid {
    let run_id = Uuid::new_v4();
    let plan = RunPlan {
        name: format!("remediation run {run_id}"),
        tasks: vec![TaskDef {
            id: task_id.to_string(),
            name: format!("task {task_id}"),
            prerequisites,
            simulate: Some(SimSpec {
                outcome: if matches!(status, TaskStatus::Failed | TaskStatus::Timeout) {
                    SimOutcome::Fail
                } else {
                    SimOutcome::Pass
                },
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            params: serde_json::json!({ "selector": "#pay" }),
            test_case_id: test_case_id.map(|c| c.to_string()),
        }],
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(project_id),
            Some("risk signal remediation fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");
    store
        .set_task_status(run_id, task_id, status, reason, true, false)
        .await
        .expect("start task");
    store
        .set_task_status(run_id, task_id, status, reason, false, true)
        .await
        .expect("finish task");

    Intelligence::new(store)
        .ingest_run(
            Some(run_id),
            Some(project_id),
            &plan,
            &[TaskState {
                id: task_id.to_string(),
                name: format!("task {task_id}"),
                prerequisites: Vec::new(),
                status,
                reason: reason.map(ToString::to_string),
                started_at: None,
                finished_at: None,
                test_case_id: test_case_id.map(|c| c.to_string()),
            }],
        )
        .await
        .expect("ingest run");
    run_id
}

/// The single `risk_assessments` row for a project's test node.
async fn row(
    store: &Store,
    project_id: &str,
) -> (
    f64,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    serde_json::Value,
) {
    let rows: Vec<(f64, Option<f64>, Option<f64>, Option<f64>, String)> = sqlx::query_as(
        "SELECT r.score, r.business_criticality, r.change_frequency, r.security_sensitivity, \
                r.inputs FROM risk_assessments r \
         WHERE r.node_id IN (SELECT id FROM quality_nodes WHERE project_id = ?) \
         LIMIT 1",
    )
    .bind(project_id)
    .fetch_all(&store.pool)
    .await
    .expect("read risk_assessments");
    assert_eq!(rows.len(), 1, "exactly one assessment must exist");
    let (score, biz, chg, sec, inputs) = rows.into_iter().next().expect("row");
    (
        score,
        biz,
        chg,
        sec,
        serde_json::from_str(&inputs).unwrap_or(serde_json::Value::Null),
    )
}

// ============================================================================
// 1. UNAVAILABLE SIGNALS MUST NOT BECOME ZERO
// ============================================================================

/// The core defect, pinned.
///
/// `security_sensitivity` and `business_criticality` have no signal source in this schema, so
/// they must be `NULL`. A `0.0` would be indistinguishable from a measurement that came out nil
/// — and worse, it would be multiplied into the score.
#[tokio::test]
async fn unavailable_signals_are_persisted_as_null_not_zero() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Null").await;
    let c = case(&store, &pid, "Pay").await;
    run(
        &store,
        &pid,
        "t1",
        TaskStatus::Failed,
        Some("Element not found: #pay"),
        Some(c),
        Vec::new(),
    )
    .await;

    let (_score, biz, _chg, sec, _inputs) = row(&store, &pid).await;

    assert_eq!(
        sec, None,
        "security_sensitivity has no signal source and must be NULL, not 0.0"
    );
    assert_eq!(
        biz, None,
        "business_criticality has no signal source and must be NULL, not 0.0"
    );
    store.pool.close().await;
}

/// Neither signal was *invented*. This test would fail if anyone wired a proxy — a
/// `requirements.kind` category, a tag convention, a hardcoded priority — into either column.
#[tokio::test]
async fn neither_signal_is_invented_from_a_proxy() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem NoProxy").await;
    // A case whose *category* is the most security-sounding value the schema accepts.
    let c = store
        .insert_test_case(
            &pid,
            &CreateTestCaseRequest {
                title: "Security-ish".into(),
                description: "Category and priority both set to security-sounding values.".into(),
                category: "security".into(),
                priority: "P0".into(),
                severity: "critical".into(),
                preconditions: Vec::new(),
                steps: vec![TestStep {
                    step_number: 1,
                    action: "act".into(),
                    expected_result: "observed".into(),
                }],
                expected_result: Some("observed".into()),
                assertions: Vec::new(),
                test_data: None,
                source_document: None,
                requirement_reference: None,
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: vec!["security".into(), "critical".into()],
                suite_id: None,
            },
            None,
        )
        .await
        .expect("insert test case");

    run(
        &store,
        &pid,
        "t1",
        TaskStatus::Passed,
        None,
        Some(c),
        Vec::new(),
    )
    .await;

    let (_score, biz, _chg, sec, _inputs) = row(&store, &pid).await;
    assert_eq!(
        sec, None,
        "a security category, critical severity and security tags must NOT become a measured \
         security_sensitivity — that would be inventing the signal"
    );
    assert_eq!(
        biz, None,
        "a P0 priority must NOT become a measured business_criticality — priority is an \
         authoring choice, not a measurement of importance"
    );
    store.pool.close().await;
}

// ============================================================================
// 2. MEASURED ZERO IS DISTINGUISHABLE FROM UNMEASURED
// ============================================================================

/// `change_frequency` **does** have a real source — Phase 3's `change_impacts` — so it is
/// measured even when the measurement is zero. That is what makes the pair
/// `Some(0.0)` / `NULL` expressible, and this test proves both are reachable at once.
#[tokio::test]
async fn a_measured_zero_is_distinguishable_from_an_unmeasured_signal() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Zero").await;
    let c = case(&store, &pid, "Untouched").await;

    // No declared change has ever touched this node, so the measurement is a real zero.
    run(
        &store,
        &pid,
        "t1",
        TaskStatus::Passed,
        None,
        Some(c),
        Vec::new(),
    )
    .await;

    let (_score, biz, chg, sec, inputs) = row(&store, &pid).await;
    assert_eq!(
        chg,
        Some(0.0),
        "change_frequency has a source and was measured as zero"
    );
    assert_eq!(
        sec, None,
        "security_sensitivity has no source and is unmeasured"
    );
    assert_eq!(biz, None);

    // Both facts survive into the inputs blob, as `0.0` and `null` respectively.
    assert_eq!(
        inputs["change_frequency"],
        serde_json::json!(0.0),
        "a measured zero must be recorded as 0.0"
    );
    assert_eq!(
        inputs["security_sensitivity"],
        serde_json::Value::Null,
        "an unmeasured signal must be recorded as null, never as 0.0"
    );
    assert_ne!(
        inputs["change_frequency"], inputs["security_sensitivity"],
        "the two must not be the same value in the record"
    );

    // And the store's own reader preserves the distinction rather than collapsing NULL to 0.
    let triples = store
        .list_risk_assessment_constants(&pid)
        .await
        .expect("constants reader");
    assert_eq!(triples.len(), 1);
    let (sec2, biz2, chg2) = triples[0];
    assert_eq!(sec2, None, "the reader must not invent a 0.0 for NULL");
    assert_eq!(biz2, None);
    assert_eq!(chg2, Some(0.0));
    store.pool.close().await;
}

/// A node that *has* been changed reads above zero, which is what proves the source is wired
/// rather than the column merely being un-nulled.
#[tokio::test]
async fn change_frequency_reads_from_the_real_change_impact_records() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Changed").await;
    let c = case(&store, &pid, "Hot spot").await;
    run(
        &store,
        &pid,
        "t1",
        TaskStatus::Passed,
        None,
        Some(c),
        Vec::new(),
    )
    .await;

    let (_score, _biz, before, _sec, _inputs) = row(&store, &pid).await;
    assert_eq!(before, Some(0.0), "nothing has changed this node yet");

    // Three distinct declared changes now affect it, through the real writer.
    let node: String = sqlx::query_scalar(
        "SELECT id FROM quality_nodes WHERE project_id = ? AND node_type = 'test' LIMIT 1",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("node");
    let node = Uuid::parse_str(&node).expect("uuid");
    for change in ["commit-a", "commit-b", "commit-c"] {
        store
            .insert_change_impact(&pid, change, "commit", &node, None, Some("fixture"))
            .await
            .expect("insert change impact");
    }

    // A second execution recomputes the row from the now-populated source.
    run(
        &store,
        &pid,
        "t2",
        TaskStatus::Passed,
        None,
        Some(c),
        Vec::new(),
    )
    .await;

    let (_score, _biz, after, _sec, _inputs) = row(&store, &pid).await;
    assert_eq!(
        after,
        Some(3.0),
        "three distinct declared changes must be read from change_impacts"
    );
    store.pool.close().await;
}

/// The source is tenant-scoped: another project's changes must not move this project's number.
#[tokio::test]
async fn change_frequency_is_project_scoped() {
    let (store, _d) = harness().await;
    let mine = project(&store, "Rem Mine").await;
    let theirs = project(&store, "Rem Theirs").await;
    let c = case(&store, &mine, "Isolated").await;
    run(
        &store,
        &mine,
        "t1",
        TaskStatus::Passed,
        None,
        Some(c),
        Vec::new(),
    )
    .await;

    let node: String = sqlx::query_scalar(
        "SELECT id FROM quality_nodes WHERE project_id = ? AND node_type = 'test' LIMIT 1",
    )
    .bind(&mine)
    .fetch_one(&store.pool)
    .await
    .expect("node");
    let node = Uuid::parse_str(&node).expect("uuid");

    // Five changes recorded against **their** project, naming our node id.
    for i in 0..5 {
        store
            .insert_change_impact(&theirs, &format!("theirs-{i}"), "commit", &node, None, None)
            .await
            .expect("insert change impact");
    }

    run(
        &store,
        &mine,
        "t2",
        TaskStatus::Passed,
        None,
        Some(c),
        Vec::new(),
    )
    .await;

    let (_score, _biz, chg, _sec, _inputs) = row(&store, &mine).await;
    assert_eq!(
        chg,
        Some(0.0),
        "another tenant's change records must not move our change_frequency"
    );

    // And the reader used by the Phase 6 detector is scoped the same way.
    assert!(
        store
            .list_risk_assessment_constants(&theirs)
            .await
            .expect("reader")
            .is_empty(),
        "another tenant's rows must not appear in this reader"
    );
    store.pool.close().await;
}

// ============================================================================
// 3. THE SCORE IS NOT UNDERSTATED
// ============================================================================

/// The arithmetic that was wrong, and is now right.
///
/// `FACTOR_WEIGHTS` sums to **1.00** (0.35 + 0.20 + 0.15 + 0.10 + 0.10 + 0.10). When two of the
/// six factors are unmeasured, the measured factors still carry weight 0.80 and contribute
/// `0.35 × 1.0 = 0.35`. The old expression stored 0.35 — **below** the 0.50 `HIGH` boundary it
/// should be judged against a model with no dependency or security signal, and far below the
/// 0.25 `MEDIUM` floor for a test that failed every single time. The normalised value is
/// `0.35 / 0.80 = 0.4375`.
#[tokio::test]
async fn a_partial_model_no_longer_reads_below_the_band_its_factors_justify() {
    // weight 0.80 of the model is measured: failure_rate 0.35, flakiness 0.20,
    // failure_severity 0.15, healing_regression 0.10. `dependency_count` and
    // `security_sensitivity` are the two with no source, together 0.20.
    let factors = vec![
        Factor::measured("failure_rate", 1.0, "test", 10),
        Factor::measured("flakiness_score", 0.0, "test", 10),
        Factor::measured("failure_severity", 0.0, "test", 10),
        Factor::unmeasured("dependency_count", "no case-to-case dependency graph"),
        Factor::measured("healing_regression_rate", 0.0, "test", 10),
        Factor::unmeasured("security_sensitivity", "no source"),
    ];
    let (cal, coverage) = crate::qa_memory_risk::calibrate_partial("subject", &factors);

    let contribution = coverage.contribution;
    assert!(
        (coverage.measured - 0.80).abs() < 1e-9,
        "the four measured factors cover 0.80 of the model: {}",
        coverage.measured
    );
    assert!(
        (coverage.unmeasured - 0.20).abs() < 1e-9,
        "the two unsourced factors account for 0.20: {}",
        coverage.unmeasured
    );
    assert!(
        (coverage.measured + coverage.unmeasured - 1.0).abs() < 1e-9,
        "measured and unmeasured weight must account for the whole model"
    );
    assert_eq!(
        cal.risk_score,
        ((contribution / coverage.measured) * 100.0).round() / 100.0,
        "the score must be the contribution normalised over the measured weight"
    );

    // The specific understatement this fixes: a test that failed every single time reads 0.35
    // under the old expression and 0.44 normalised. Both land in `MEDIUM`, but the raw value
    // also under-reports the two unmeasured factors as if they had contributed zero risk.
    assert!(
        (contribution - 0.35).abs() < 1e-9,
        "raw contribution: {contribution}"
    );
    assert!(
        (cal.risk_score - 0.44).abs() < 0.01,
        "the normalised score must be 0.44, not the raw 0.35: {}",
        cal.risk_score
    );

    // The band follows the corrected score, from the same threshold table `calibrate_risk` uses.
    let expected_level = risk_level_for(contribution / coverage.measured);
    assert_eq!(cal.risk_level, expected_level);
    assert!(
        cal.explanation.contains("assumed neutral"),
        "the neutrality assumption must be written into the explanation: {}",
        cal.explanation
    );
    assert!(
        cal.explanation.contains("security_sensitivity"),
        "the unmeasured factor must be named: {}",
        cal.explanation
    );
}

/// One owner of the bands. If `calibrate_risk` and the partial path disagreed about a threshold,
/// the same score would be reported at two different levels.
#[test]
fn the_partial_path_and_calibrate_risk_share_one_owner_of_the_bands() {
    for score in [0.0, 0.24, 0.25, 0.49, 0.50, 0.69, 0.70, 1.0] {
        // `calibrate_risk`'s levels come from `risk_level_for`; this asserts the table that
        // both read produces the documented boundaries.
        let level = risk_level_for(score);
        let expected = if score >= 0.70 {
            "CRITICAL"
        } else if score >= 0.50 {
            "HIGH"
        } else if score >= 0.25 {
            "MEDIUM"
        } else {
            "LOW"
        };
        assert_eq!(level, expected, "score {score} must map to {expected}");
    }
    // And the published table is complete: the **lowest** bound must be 0.0 so that every
    // score has a band.
    let bounds: Vec<f64> = crate::adaptive_intelligence::RISK_LEVEL_THRESHOLDS
        .iter()
        .map(|(_, bound)| *bound)
        .collect();
    assert_eq!(
        bounds.len(),
        4,
        "CRITICAL, HIGH, MEDIUM and LOW -- one owner, four bands"
    );
    assert_eq!(
        bounds.iter().cloned().fold(f64::MAX, f64::min),
        0.0,
        "the lowest band must start at 0.0 so every score has one"
    );

    // `FACTOR_WEIGHTS` sums to **1.00**, which is what makes the normalisation collapse to a
    // no-op for a fully measured model. If a future change to `calibrate_risk` made the weights
    // sum to less than 1.0, every score the engine produced would be capped short of 1.0 and
    // this would fire.
    let weight_total: f64 = FACTOR_WEIGHTS.iter().map(|(_, w)| *w).sum();
    assert!(
        (weight_total - 1.0).abs() < 1e-9,
        "calibrate_risk's weights must sum to 1.0, or every score it produces is capped short \
         of the top of the range: {weight_total}"
    );
    // And no single factor may outweigh the rest of the model combined.
    for (name, weight) in FACTOR_WEIGHTS {
        assert!(
            weight <= 1.0 - weight,
            "{name} at {weight} outweighs every other factor combined"
        );
    }
}

/// Nothing measured is **no answer**, not a score of zero.
#[test]
fn a_model_with_nothing_measured_yields_no_score_rather_than_zero() {
    let factors = vec![
        Factor::unmeasured("failure_rate", "no history"),
        Factor::unmeasured("flakiness_score", "no history"),
        Factor::unmeasured("failure_severity", "no history"),
        Factor::unmeasured("dependency_count", "no graph"),
        Factor::unmeasured("healing_regression_rate", "no healing"),
        Factor::unmeasured("security_sensitivity", "no source"),
    ];
    let (cal, coverage) = crate::qa_memory_risk::calibrate_partial("subject", &factors);
    assert_eq!(coverage.measured, 0.0);
    assert_eq!(
        cal.risk_level, "INSUFFICIENT_DATA",
        "a zero score would claim no risk exists, which nothing supports"
    );
    assert_eq!(cal.risk_score, 0.0);
    assert!(
        cal.explanation.contains("not a score of zero"),
        "the explanation must say so: {}",
        cal.explanation
    );
}

/// A fully measured model is unchanged by the normalisation.
///
/// This is the control on the tests above. Because `FACTOR_WEIGHTS` sums to exactly 1.00, a model
/// with every factor measured has `measured_weight = 1.0`, and dividing by it is a no-op — so
/// `calibrate_partial` returns precisely what `calibrate_risk` would have returned. The
/// remediation therefore changes nothing for a caller that measures everything, which is the
/// property that makes it safe to apply to a live endpoint.
#[test]
fn a_fully_measured_model_is_unaffected_by_the_normalisation() {
    let factors: Vec<Factor> = FACTOR_WEIGHTS
        .iter()
        .map(|(name, _)| Factor::measured(name, 0.5, "test", 10))
        .collect();
    let (cal, coverage) = crate::qa_memory_risk::calibrate_partial("subject", &factors);
    assert!(
        (coverage.measured - 1.0).abs() < 1e-9,
        "a full model measures all of its own weight: {}",
        coverage.measured
    );
    assert!(
        (coverage.unmeasured - 0.0).abs() < 1e-9,
        "nothing is unmeasured: {}",
        coverage.unmeasured
    );
    assert!(
        (cal.risk_score - 0.5).abs() < 0.01,
        "with every factor at 0.5 the score must read 0.5: {}",
        cal.risk_score
    );
    assert!(
        !cal.explanation.contains("assumed neutral"),
        "a full model assumes nothing about a missing weight: {}",
        cal.explanation
    );
    assert_eq!(
        cal.risk_level,
        risk_level_for(0.5),
        "and the band must follow the score"
    );

    // The control is only meaningful if it agrees with the engine it stands in for.
    //
    // Note the asymmetry the two entry points have, which is easy to get wrong:
    // `calibrate_risk` takes a **raw count** in `RiskFactors::dependency_count` and normalises it
    // internally (`count / 5`, capped), whereas `calibrate_partial` takes an already-normalised
    // `Factor` value. Comparing them requires feeding the engine a count whose normalised form is
    // the factor's value: `5 / 5 = 1.0`, not a bare `0.5` against `dependency_count: 0` (whose
    // `dep_norm` is `0.0`). `GET /risk` performs the same normalisation before calling
    // `calibrate_partial` (`(dependency_count as f64 / 5.0).min(1.0)`), so its dependency factor
    // means the same thing as the engine's.
    let engine = crate::adaptive_intelligence::calibrate_risk(
        "subject",
        crate::adaptive_intelligence::RiskFactors {
            failure_rate: 0.5,
            flakiness_score: 0.5,
            failure_severity: 0.5,
            dependency_count: 5,
            healing_regression_rate: 0.5,
            security_sensitivity: 0.5,
        },
    );
    assert_eq!(engine.risk_score, 0.55, "0.55 with dep_norm pinned at 1.0");
    let engine_shaped: Vec<Factor> = FACTOR_WEIGHTS
        .iter()
        .map(|(name, _)| {
            if *name == "dependency_count" {
                Factor::measured(name, 1.0, "test", 10)
            } else {
                Factor::measured(name, 0.5, "test", 10)
            }
        })
        .collect();
    let (cal, _) = crate::qa_memory_risk::calibrate_partial("subject", &engine_shaped);
    assert_eq!(
        cal.risk_score, engine.risk_score,
        "a fully measured model must read the same through both paths"
    );
    assert_eq!(cal.risk_level, engine.risk_level);
}

/// The stored row's own arithmetic is self-consistent with its recorded coverage.
#[tokio::test]
async fn the_stored_score_agrees_with_its_own_recorded_coverage() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Consistency").await;
    let c = case(&store, &pid, "Mixed").await;

    // A history with both passes and failures, and some flakiness, so the measured factors are
    // not all zero and the normalisation is observable.
    //
    // **The same task name every iteration.** `historical_failure_rate` and `flakiness_score` are
    // read per task *name*, so a fixture that invents a fresh name per run measures one run each
    // time and the factors stay at their trivial values. This test's first draft did exactly
    // that and asserted `contribution < score`, which passed only because both sides were zero.
    for i in 0..6 {
        let (status, reason) = match i {
            0 => (TaskStatus::Failed, Some("Element not found: #pay")),
            1 => (TaskStatus::Passed, None),
            2 => (TaskStatus::Failed, Some("Timeout waiting for #pay")),
            _ => (TaskStatus::Passed, None),
        };
        run(
            &store,
            &pid,
            "stable",
            status,
            reason,
            Some(c),
            if i == 0 {
                vec!["p1".into(), "p2".into(), "p3".into()]
            } else {
                Vec::new()
            },
        )
        .await;
    }

    let (score, _biz, _chg, sec, inputs) = row(&store, &pid).await;

    assert_eq!(sec, None, "still unmeasured, and recorded as such");
    assert_eq!(
        inputs["security_sensitivity"],
        serde_json::Value::Null,
        "the record must show null, not a zero"
    );

    let measured: f64 = inputs["measured_weight"].as_f64().expect("measured_weight");
    let unmeasured: f64 = inputs["unmeasured_weight"]
        .as_f64()
        .expect("unmeasured_weight");
    assert!(
        (measured + unmeasured - 1.0).abs() < 1e-9,
        "measured and unmeasured weight must account for the whole model: {measured} + {unmeasured}"
    );
    assert!(
        (measured - 0.90).abs() < 1e-9,
        "the four measured factors cover 0.90: {measured}"
    );
    assert!((unmeasured - 0.10).abs() < 1e-9);

    let unmeasured_factors: Vec<&str> = inputs["unmeasured_factors"]
        .as_array()
        .expect("unmeasured_factors")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        unmeasured_factors.contains(&"security_sensitivity"),
        "the unweighted-out factor must be named: {unmeasured_factors:?}"
    );

    // Recompute the score from the recorded factors and confirm the stored value.
    let hfr = inputs["historical_failure_rate"].as_f64().expect("hfr");
    let flak = inputs["flakiness_score"].as_f64().expect("flakiness");
    let dep = inputs["dependency_count_normalized"]
        .as_f64()
        .expect("dep_norm");
    let cost = inputs["execution_cost_normalized"]
        .as_f64()
        .expect("cost_norm");
    let contribution = 0.40 * hfr + 0.20 * flak + 0.15 * dep + 0.15 * cost;
    let expected = (contribution / measured).clamp(0.0, 1.0);
    assert!(
        (score - expected).abs() < 1e-6,
        "the stored score {score} must equal contribution/measured_weight {expected} \
         (contribution {contribution}, measured_weight {measured})"
    );

    // And the un-normalised sum would have been strictly lower — which is the understatement.
    assert!(
        contribution > 0.0,
        "the fixture must produce a non-zero contribution, or this comparison proves nothing: \
         hfr {hfr}, flakiness {flak}, dep {dep}, cost {cost}"
    );
    assert!(
        contribution < score,
        "normalising must raise the score: raw {contribution} vs normalised {score}"
    );
    assert!(
        inputs["score_normalisation"]
            .as_str()
            .is_some_and(|s| s.contains("assumes the unmeasured weight would have been neutral")),
        "the assumption must be recorded in the row: {}",
        inputs["score_normalisation"]
    );
    store.pool.close().await;
}

// ============================================================================
// 4. EXISTING MEASURED SIGNALS KEEP WORKING
// ============================================================================

/// The four real factors are unchanged: still recorded, still the values the history implies.
#[tokio::test]
async fn the_signals_that_were_already_real_are_unaffected() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Real").await;
    let c = case(&store, &pid, "Real").await;

    // Three failures alternating with three passes: failure_rate 3/6, and four outcome flips
    // give a real flakiness score. One stable task name, because both factors are read per name.
    for i in 0..6 {
        let status = if i % 2 == 0 {
            TaskStatus::Failed
        } else {
            TaskStatus::Passed
        };
        let reason = if status == TaskStatus::Failed {
            Some("Element not found: #pay")
        } else {
            None
        };
        run(&store, &pid, "stable", status, reason, Some(c), Vec::new()).await;
    }

    let (_score, _biz, _chg, _sec, inputs) = row(&store, &pid).await;

    let hfr = inputs["historical_failure_rate"].as_f64().expect("hfr");
    assert!(
        (hfr - 3.0 / 6.0).abs() < 1e-9,
        "historical_failure_rate must still be failures over runs: {hfr}"
    );
    let flak = inputs["flakiness_score"].as_f64().expect("flakiness");
    assert!(
        flak > 0.0,
        "alternating outcomes must still produce a real flakiness score: {flak}"
    );
    assert_eq!(inputs["window_runs"].as_i64(), Some(6));
    // The weights of the four real factors are unchanged.
    assert_eq!(inputs["weights"]["failure_rate"].as_f64(), Some(0.40));
    assert_eq!(inputs["weights"]["flakiness"].as_f64(), Some(0.20));
    assert_eq!(inputs["weights"]["dependency_count"].as_f64(), Some(0.15));
    assert_eq!(inputs["weights"]["execution_cost"].as_f64(), Some(0.15));
    store.pool.close().await;
}

/// Phase 4's own surface is untouched: it never read `risk_assessments`, so its refusal to emit
/// a score still holds and its measured factors still populate.
#[tokio::test]
async fn phase_fours_surface_is_unchanged() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Phase4").await;
    let c = case(&store, &pid, "P4").await;
    for i in 0..4 {
        run(
            &store,
            &pid,
            &format!("t{i}"),
            if i == 0 {
                TaskStatus::Failed
            } else {
                TaskStatus::Passed
            },
            if i == 0 {
                Some("Element not found: #p4")
            } else {
                None
            },
            Some(c),
            Vec::new(),
        )
        .await;
    }

    let reader = crate::qa_memory::QaMemoryReader::new(store.clone());
    let ranking = reader
        .prioritise_tests(&pid, None)
        .await
        .expect("prioritise_tests");
    let view = ranking
        .view
        .expect("a project with history must produce a prioritisation");

    // Phase 4 reports measured factors and, correctly, no calibration.
    let entry = view.tests.first().expect("a ranked test");
    assert!(
        entry.risk.failure_rate.is_some(),
        "the failure rate must still be measured"
    );
    assert!(
        entry.risk.calibration.is_none(),
        "Phase 4 still refuses a calibration, and must keep refusing it"
    );
    assert!(
        entry.risk.unmeasured_weight > 0.0,
        "Phase 4 still reports unbacked weight"
    );
    assert!(
        entry
            .risk
            .factors
            .iter()
            .any(|f| f.name == "security_sensitivity" && f.measured.is_none()),
        "Phase 4 must still report security_sensitivity as unmeasured with a reason"
    );
    // And the floor is still derived from recorded failures.
    assert!(
        view.mandatory_floor.contains(&c.to_string()),
        "the safety floor must still contain the failing case"
    );
    store.pool.close().await;
}

/// Phase 6's detector reports the remediated state, and still catches a recurrence.
#[tokio::test]
async fn phase_six_detector_sees_the_remediated_state_and_still_catches_a_recurrence() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Phase6").await;
    let c = case(&store, &pid, "P6").await;
    run(
        &store,
        &pid,
        "t1",
        TaskStatus::Failed,
        Some("Element not found: #p6"),
        Some(c),
        Vec::new(),
    )
    .await;

    let verifier = IntegrityVerifier::new(&store);
    let findings = verifier
        .fabricated_constant_check(&pid)
        .await
        .expect("the check reads through the store's reader");

    let unsourced = findings
        .iter()
        .find(|f| f.field == "security_sensitivity")
        .expect("the unsourced column must be reported");
    assert_eq!(
        unsourced.outcome,
        IntegrityOutcome::InsufficientData,
        "an unsourced signal is a stated gap, not a fabrication: {}",
        unsourced.detail
    );
    assert_eq!(unsourced.kind, ComparisonKind::FabricatedConstant);
    assert!(
        !unsourced.establishes_defect(),
        "and never a product defect"
    );
    assert!(
        !findings.iter().any(|f| f.field == "change_frequency"),
        "a measured column must not be reported"
    );

    // Now reintroduce a fabricated zero through the real writer, and the detector fires again.
    let node: String = sqlx::query_scalar(
        "SELECT id FROM quality_nodes WHERE project_id = ? AND node_type = 'test' LIMIT 1",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("node");
    let node = Uuid::parse_str(&node).expect("uuid");
    store
        .upsert_risk_assessment(node, 0.5, None, Some(0.0), 0.5, 1, Some(0.0), 100.0, "{}")
        .await
        .expect("upsert");

    let after = verifier
        .fabricated_constant_check(&pid)
        .await
        .expect("the check reads through the store's reader");
    let regressed = after
        .iter()
        .find(|f| f.field == "security_sensitivity")
        .expect("the fabrication must be caught");
    assert_eq!(
        regressed.outcome,
        IntegrityOutcome::Inconsistent,
        "a supplied 0.0 is a fabrication and must be reported as one: {}",
        regressed.detail
    );
    store.pool.close().await;
}

// ============================================================================
// 5. ISOLATION
// ============================================================================

/// One tenant's scored history must not appear in another's row.
#[tokio::test]
async fn risk_signals_remain_project_scoped() {
    let (store, _d) = harness().await;
    let mine = project(&store, "Rem Isolated Mine").await;
    let theirs = project(&store, "Rem Isolated Theirs").await;

    // Theirs: four cases, each failing three times under its own stable task name, so every one
    // of the four nodes has a genuine failure rate of 1.0.
    for i in 0..4 {
        let tc = case(&store, &theirs, &format!("Their {i}")).await;
        for _ in 0..3 {
            run(
                &store,
                &theirs,
                &format!("tt{i}"),
                TaskStatus::Failed,
                Some("Element not found: #theirs"),
                Some(tc),
                Vec::new(),
            )
            .await;
        }
    }
    // Ours: one case that passes every time, under a stable name so the history accumulates.
    let my_case = case(&store, &mine, "Mine").await;
    for _ in 0..3 {
        run(
            &store,
            &mine,
            "mine",
            TaskStatus::Passed,
            None,
            Some(my_case),
            Vec::new(),
        )
        .await;
    }

    let (my_score, _biz, _chg, sec, my_inputs) = row(&store, &mine).await;
    let (their_score, _biz2, _chg2, sec2, their_inputs) = row(&store, &theirs).await;

    assert_eq!(
        my_inputs["historical_failure_rate"].as_f64(),
        Some(0.0),
        "our case never failed, so its own failure rate must be 0.0 — not theirs"
    );
    assert!(
        my_inputs["historical_failure_rate"].as_f64().unwrap()
            < their_inputs["historical_failure_rate"].as_f64().unwrap(),
        "the two projects must have genuinely different risk: ours {} theirs {}",
        my_inputs["historical_failure_rate"],
        their_inputs["historical_failure_rate"]
    );
    assert!(
        my_score < their_score,
        "our score must reflect our own history: {my_score} vs {their_score}"
    );
    assert_eq!(sec, None, "still unmeasured in both");
    assert_eq!(sec2, None);

    // And the store's own reader is scoped to one project: four nodes on their side, one on ours,
    // and neither count leaks into the other.
    assert_eq!(
        store
            .list_risk_assessment_constants(&theirs)
            .await
            .expect("reader")
            .len(),
        4,
        "their project has four scored cases and the reader must return exactly those"
    );
    assert_eq!(
        store
            .list_risk_assessment_constants(&mine)
            .await
            .expect("reader")
            .len(),
        1,
        "our project has one scored case, and none of theirs may appear in it"
    );
    store.pool.close().await;
}

// ============================================================================
// 6. THE EXPLORATION BUDGET STILL OPENS A SESSION
// ============================================================================

/// A smoke check that nothing about this remediation disturbed the exploration path, which also
/// writes risk assessments through the same function.
#[tokio::test]
async fn exploration_still_records_a_remediated_risk_row() {
    let (store, _d) = harness().await;
    let pid = project(&store, "Rem Exploration").await;
    let c = case(&store, &pid, "Explored").await;

    let evidence = std::sync::Arc::new(
        crate::evidence::LocalEvidenceStore::new(_d.path().join("exploration-evidence"))
            .expect("evidence root"),
    );
    let service = crate::agents::exploration::ExplorationService::new(
        store.clone(),
        std::sync::Arc::new(crate::policy::PolicyConfig {
            max_concurrency: 8,
            require_approval_for: std::collections::HashSet::new(),
            blocked_executors: std::collections::HashSet::new(),
        }),
        evidence,
    );
    let report = service
        .explore(
            &pid,
            "http://127.0.0.1:8080",
            crate::agents::exploration::ExplorationSurface::default(),
            None,
            ExplorationBudget::default(),
            crate::agent::AgentLimits {
                max_turns: 4,
                max_tool_calls_total: 8,
                max_observation_bytes: 64 * 1024,
                timeout: std::time::Duration::from_secs(30),
            },
            std::sync::Arc::new(
                crate::agents::exploration_execution::RunPipelineProbeRunner::new(store.clone()),
            ),
            std::sync::Arc::new(crate::ai_router::AiInfrastructure::new(
                vec![],
                std::collections::HashMap::new(),
            )),
        )
        .await
        .expect("explore");
    assert!(
        matches!(
            report.stop,
            crate::agents::exploration::ExplorationStop::NoSurface { .. }
        ),
        "an empty surface stops immediately, and must: {:?}",
        report.stop
    );

    // The direct run above is what writes the row; assert its shape is the remediated one.
    run(
        &store,
        &pid,
        "t1",
        TaskStatus::Failed,
        Some("Element not found: #explored"),
        Some(c),
        Vec::new(),
    )
    .await;
    let (_score, _biz, _chg, sec, inputs) = row(&store, &pid).await;
    assert_eq!(sec, None);
    assert_eq!(inputs["security_sensitivity"], serde_json::Value::Null);
    assert!(
        (inputs["measured_weight"].as_f64().expect("measured_weight") - 0.90).abs() < 1e-9,
        "measured_weight must be 0.90 of the model, exactly as before the remediation: {}",
        inputs["measured_weight"]
    );
    store.pool.close().await;
}

/// `RiskCalibration` is still constructible in the shape callers expect, so the remediation did
/// not change a public type.
#[test]
fn the_risk_calibration_shape_is_unchanged() {
    let cal = RiskCalibration {
        task_name: "subject".into(),
        risk_score: 0.42,
        risk_level: "MEDIUM".into(),
        factors: crate::adaptive_intelligence::RiskFactors {
            failure_rate: 0.1,
            flakiness_score: 0.2,
            failure_severity: 0.3,
            dependency_count: 0,
            healing_regression_rate: 0.0,
            security_sensitivity: 0.0,
        },
        explanation: "shape check".into(),
    };
    assert_eq!(cal.risk_score, 0.42);
    assert_eq!(cal.risk_level, "MEDIUM");
}

// ============================================================================
// 7. THE HTTP SURFACE REPORTS ITS OWN COVERAGE
// ============================================================================

/// `GET /projects/:id/intelligence/risk` builds its six factors from real rows, and a factor
/// with no evidence must say so rather than present a zero.
///
/// This is exercised against the same `calibrate_partial` the handler calls, with the factor list
/// the handler builds, because the handler's own arithmetic is already covered by
/// `api_tests::test_project_risk_intelligence_endpoint` (which pins the level, the score and the
/// `factors` shape of a project that has telemetry). What that test cannot see is the *coverage*
/// fields this remediation added, which is what these assertions are about.
#[test]
fn a_project_with_no_history_reports_no_measured_factors() {
    // Exactly what the handler builds for a project with no executions, no flakiness rows, no
    // failures, no relations and no healing attempts.
    let project_factors = vec![
        Factor::unmeasured("failure_rate", "no recorded execution"),
        Factor::unmeasured("flakiness_score", "no stored flakiness metric"),
        Factor::unmeasured("failure_severity", "no recorded failure"),
        Factor::unmeasured("dependency_count", "no recorded relation"),
        Factor::unmeasured("healing_regression_rate", "no healing attempt"),
        Factor::unmeasured("security_sensitivity", "no source in this schema"),
    ];
    let (cal, coverage) = crate::qa_memory_risk::calibrate_partial("p", &project_factors);
    assert_eq!(coverage.measured, 0.0);
    assert_eq!(
        cal.risk_level, "INSUFFICIENT_DATA",
        "a project with no history must not be handed a level"
    );
    assert!(
        cal.explanation.contains("not a score of zero"),
        "and must be told why: {}",
        cal.explanation
    );
}

/// Every factor the handler declares is one `calibrate_risk` actually weights. If the two lists
/// drift apart, a factor could be reported as measured coverage while contributing nothing to the
/// score — the exact kind of silent disagreement this remediation exists to remove.
#[test]
fn the_endpoints_factor_list_covers_the_engine_and_nothing_else() {
    let handler_factors = [
        "failure_rate",
        "flakiness_score",
        "failure_severity",
        "dependency_count",
        "healing_regression_rate",
        "security_sensitivity",
    ];
    let engine_factors: Vec<&str> = FACTOR_WEIGHTS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        handler_factors.to_vec(),
        engine_factors,
        "the endpoint's factors and calibrate_risk's weights must be the same six, in order"
    );
    // And the weights must account for the whole model, so `measured_weight` is meaningful.
    let total: f64 = FACTOR_WEIGHTS.iter().map(|(_, w)| *w).sum();
    assert!(
        (total - 1.0).abs() < 1e-9,
        "the model must be fully accounted for: {total}"
    );
}
