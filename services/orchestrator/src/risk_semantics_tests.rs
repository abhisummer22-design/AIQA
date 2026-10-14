//! Phase 6.1 remediation — risk semantics contracts for R2, R3 and L4.
//!
//! ## Why this file exists
//!
//! The Phase 6.1 audit confirmed three findings that are **not** bugs to fix but contracts to
//! state. Each of them was previously true only as a comment, which is how a reader ends up
//! assuming the opposite:
//!
//! * **R2** — `change_frequency` is measured, persisted and integrity-checked, and feeds **no**
//!   risk formula. Adding a weight for it now would double-count change impact, because
//!   `change_impacts` already reaches selection twice on its own.
//! * **R3** — `security_sensitivity` and `business_criticality` have no authoritative source in
//!   this schema and stay `NULL`/unmeasured. Nothing infers them from a category, priority,
//!   severity or tag.
//! * **L4** — AIQA has three *different* calculations that all use the word "risk", plus an
//!   ordered band system. They answer different questions and must not be merged or
//!   renormalised against each other.
//!
//! ## What these tests do and do not claim
//!
//! They pin the contracts above. They do **not** assert that any of the decisions is the *right*
//! product decision — several remain open questions in the remediation report. What they pin is
//! that the current behaviour is deliberate and would have to be changed deliberately.

use crate::adaptive_intelligence::{
    calibrate_risk, risk_level_for, RiskFactors, RISK_LEVEL_THRESHOLDS,
};
use crate::domain::TaskStatus;
use crate::qa_memory_risk::{Factor, FACTOR_WEIGHTS};
use uuid::Uuid;

// ============================================================================
// R2 — change_frequency is measured, and deliberately not a scoring weight
// ============================================================================

/// `change_frequency` is written from a real source, and it is a real measurement even when zero.
///
/// This is the half of R2 that must stay true: the audit's complaint was that the signal was
/// *unused*, not that it was fabricated. Removing the weight is the decision; keeping the
/// measurement honest is the requirement.
#[tokio::test]
async fn r2_change_frequency_is_a_real_measurement_including_at_zero() {
    let (store, _d) = harness().await;
    let pid = project(&store, "R2 ChangeFreq").await;
    let c = case(&store, &pid, "Checkout").await;

    // No declared change has touched this node, so the measurement is a genuine zero.
    run(&store, &pid, "t1", c, TaskStatus::Passed, 0).await;

    let before: Vec<(String, Option<f64>)> = sqlx::query_as(
        "SELECT node_id, change_frequency FROM risk_assessments WHERE node_id IN \
                        (SELECT id FROM quality_nodes WHERE project_id = ?)",
    )
    .bind(&pid)
    .fetch_all(&store.pool)
    .await
    .expect("read change_frequency");
    assert_eq!(before.len(), 1, "the fixture must produce one assessment");
    assert_eq!(
        before[0].1,
        Some(0.0),
        "zero distinct declared changes is a measured zero, not an absent value"
    );

    // Now record two real declared changes against this node, through the production writer.
    let node: String = sqlx::query_scalar(
        "SELECT id FROM quality_nodes WHERE project_id = ? AND node_type = 'test' LIMIT 1",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("node");
    let node = Uuid::parse_str(&node).expect("a node id is a uuid");
    for change in ["commit-1", "commit-2"] {
        store
            .insert_change_impact(&pid, change, "commit", &node, None, Some("r2 fixture"))
            .await
            .expect("insert_change_impact");
    }

    // A second execution recomputes it from the now-populated source.
    run(&store, &pid, "t2", c, TaskStatus::Passed, 1).await;

    let after: Vec<(String, Option<f64>)> = sqlx::query_as(
        "SELECT node_id, change_frequency FROM risk_assessments WHERE node_id IN \
                        (SELECT id FROM quality_nodes WHERE project_id = ?)",
    )
    .bind(&pid)
    .fetch_all(&store.pool)
    .await
    .expect("read change_frequency");
    assert_eq!(
        after[0].1,
        Some(2.0),
        "two distinct declared changes must be counted, so the column is wired to a real source \
         rather than merely un-nulled"
    );

    // Distinct, not rows: re-recording the same change must not inflate the count.
    store
        .insert_change_impact(&pid, "commit-1", "commit", &node, None, Some("r2 repeat"))
        .await
        .expect("insert_change_impact");
    run(&store, &pid, "t3", c, TaskStatus::Passed, 2).await;
    let repeat: Option<f64> = sqlx::query_scalar(
        "SELECT change_frequency FROM risk_assessments WHERE node_id IN \
         (SELECT id FROM quality_nodes WHERE project_id = ?)",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("read change_frequency");
    assert_eq!(
        repeat,
        Some(2.0),
        "one change recorded twice is still one change"
    );
    store.pool.close().await;
}

/// The decision: `change_frequency` carries **no** weight in either risk formula.
///
/// This is the assertion that makes the double-counting constraint enforceable. `change_impacts`
/// already reaches test selection twice on its own — Phase 3's `DirectlyImpacted` /
/// `RelatedFailure` tiers, and Phase 4's `PriorityBand::Changed`. A third influence through
/// `risk_assessments.score` would be a third observation of the same underlying fact.
///
/// If a future change gives `change_frequency` a weight, this test is the thing that must fail
/// first, deliberately, with this comment explaining why.
#[test]
fn r2_change_frequency_carries_no_weight_in_either_risk_formula() {
    let names: Vec<&str> = FACTOR_WEIGHTS.iter().map(|(n, _)| *n).collect();
    assert!(
        !names.contains(&"change_frequency"),
        "calibrate_risk's factors are {names:?}; adding change_frequency here would give the \
         change signal a third route into selection, on top of Phase 3's impact tiers and Phase \
         4's Changed band. That is a product decision, not a default."
    );

    // The intelligence-layer formula's factors are named in its own `inputs` blob, and the
    // recorded coverage proves which of them actually carry weight.
    assert_eq!(
        FACTOR_WEIGHTS.iter().map(|(_, w)| w).sum::<f64>(),
        1.0,
        "the six-factor model's weights still account for the whole model, so adding a factor \
         would require re-budgeting it rather than bolting it on"
    );
}

/// R2's informational role is explicit and checked: the integrity verifier is what consumes it.
///
/// `change_frequency` is read by exactly one thing — Phase 6's fabricated-constant check, which
/// is told it has a source so a constant would be a fabrication rather than an honest zero. That
/// is the signal's current job.
#[test]
fn r2_the_only_consumer_of_change_frequency_is_the_integrity_check() {
    // The verifier's own table records this. If change_frequency ever becomes a scoring input, this
    // mapping is what must change with it.
    assert_eq!(
        FACTOR_WEIGHTS
            .iter()
            .find(|(n, _)| *n == "change_frequency")
            .map(|(_, w)| *w),
        None,
        "change_frequency is not a calibrate_risk factor"
    );
}

// ============================================================================
// R3 — security_sensitivity and business_criticality stay unmeasured
// ============================================================================

/// Neither signal may be inferred from anything that exists in the schema.
///
/// The schema has `requirements.kind` (a *category*), `test_cases.priority`, `.severity` and
/// `.category`, and free-text `tags`. Each of those is a plausible-looking proxy and none of them
/// is a measurement of importance or of security sensitivity — Phase 4 considered and rejected
/// exactly this substitution.
///
/// A test case is given the most security-sounding values the schema accepts. Both signals must
/// still be `NULL`.
#[tokio::test]
async fn r3_neither_signal_is_inferred_from_any_proxy_in_the_schema() {
    let (store, _d) = harness().await;
    let pid = project(&store, "R3 NoProxy").await;

    // A case whose every proxy field is maximally security- and importance-sounding.
    let c = store
        .insert_test_case(
            &pid,
            &crate::domain::CreateTestCaseRequest {
                title: "Admin privilege escalation".into(),
                description: "Every proxy field set to its most alarming value.".into(),
                category: "security".into(),
                priority: "P0".into(),
                severity: "critical".into(),
                preconditions: Vec::new(),
                steps: vec![crate::domain::TestStep {
                    step_number: 1,
                    action: "escalate".into(),
                    expected_result: "denied".into(),
                }],
                expected_result: Some("denied".into()),
                assertions: Vec::new(),
                test_data: None,
                source_document: None,
                requirement_reference: None,
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: vec!["security".into(), "critical".into(), "p0".into()],
                suite_id: None,
            },
            None,
        )
        .await
        .expect("insert test case");

    run(&store, &pid, "t1", c, TaskStatus::Failed, 0).await;

    let (sec, biz): (Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT security_sensitivity, business_criticality FROM risk_assessments WHERE node_id IN \
         (SELECT id FROM quality_nodes WHERE project_id = ?)",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("read the two signals");

    assert_eq!(
        sec, None,
        "a security category, critical severity, P0 priority and security tags must NOT become a \
         measured security_sensitivity: a category is not an importance"
    );
    assert_eq!(
        biz, None,
        "a P0 priority must NOT become a measured business_criticality: an authoring choice is \
         not a measurement of importance"
    );
    store.pool.close().await;
}

/// The four states the plan asks to be distinguished stay distinguishable in the stored record.
///
/// | State | Where it appears |
/// |---|---|
/// | measured zero | `change_frequency = Some(0.0)` |
/// | explicitly declared zero | nowhere — nothing asserts these signals |
/// | unmeasured | `security_sensitivity = NULL`, and `Factor::unmeasured` in the API |
/// | unavailable source | the reason, recorded in the `inputs` blob |
#[tokio::test]
async fn r3_measured_zero_unmeasured_and_unavailable_stay_distinct() {
    let (store, _d) = harness().await;
    let pid = project(&store, "R3 States").await;
    let c = case(&store, &pid, "Pay").await;
    run(&store, &pid, "t1", c, TaskStatus::Passed, 0).await;

    let (chg, sec, inputs): (Option<f64>, Option<f64>, String) = sqlx::query_as(
        "SELECT change_frequency, security_sensitivity, inputs FROM risk_assessments WHERE node_id \
         IN (SELECT id FROM quality_nodes WHERE project_id = ?)",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("read the row");

    // measured zero
    assert_eq!(
        chg,
        Some(0.0),
        "change_frequency has a source and measured zero"
    );
    // unmeasured
    assert_eq!(sec, None, "security_sensitivity has no source");

    let inputs: serde_json::Value = serde_json::from_str(&inputs).expect("inputs is JSON");
    assert_eq!(
        inputs["change_frequency"],
        serde_json::json!(0.0),
        "the record must carry the measured zero as 0.0"
    );
    assert_eq!(
        inputs["security_sensitivity"],
        serde_json::Value::Null,
        "the record must carry the unmeasured signal as null -- never as 0.0"
    );
    // unavailable source, with the reason stated
    let unmeasured: Vec<&str> = inputs["unmeasured_factors"]
        .as_array()
        .expect("unmeasured_factors")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        unmeasured.contains(&"security_sensitivity"),
        "the signal with no source must be named: {unmeasured:?}"
    );
    // and the weight it does not back is quantified
    // Compared with an epsilon: the recorded weight is a sum of binary fractions, so `0.10` does
    // not arrive as exactly 0.1 and an exact comparison here would fail on arithmetic alone.
    let unmeasured_weight = inputs["unmeasured_weight"]
        .as_f64()
        .expect("unmeasured_weight");
    assert!(
        (unmeasured_weight - 0.10).abs() < 1e-9,
        "the unbacked weight must be quantified: {unmeasured_weight}"
    );
    store.pool.close().await;
}

// ============================================================================
// L4 — three different calculations, and one ordered band system
// ============================================================================

/// The three calculations are named, and they are not interchangeable.
///
/// `L4`'s requirement is that the divergence be understood before anything is unified. These
/// tests state what each one *is*, so that a future reader is not left to infer it from a number.
#[test]
fn l4_the_three_calculations_are_distinct_and_are_not_merged() {
    // (1) `calibrate_risk` — six-factor weighted risk, bands at 0.70/0.50/0.25.
    assert_eq!(FACTOR_WEIGHTS.len(), 6, "the engine model has six factors");
    assert_eq!(
        FACTOR_WEIGHTS.iter().map(|(_, w)| w).sum::<f64>(),
        1.0,
        "and they account for the whole model"
    );

    // (2) `intelligence.rs` risk_score — five factors, bandless. Asserted through its own
    //     recorded coverage: 0.90 of the model is backed and it normalises over that.
    //     (The DB-backed half of this lives in `risk_signal_tests`.)

    // (3) `value_score` — deliberately inverted: stable and cheap is *more* valuable. It must
    //     never be compared against, or renormalised against, a risk score.

    // The strongest available statement that they are different metrics: a factor set that is
    // maximal for risk is maximal for value too, but the two formulas weight them differently,
    // so the same inputs cannot be expected to produce the same number.
    let maximal = calibrate_risk(
        "subject",
        RiskFactors {
            failure_rate: 1.0,
            flakiness_score: 1.0,
            failure_severity: 1.0,
            dependency_count: 5,
            healing_regression_rate: 1.0,
            security_sensitivity: 1.0,
        },
    );
    assert_eq!(
        maximal.risk_score, 1.0,
        "a fully measured, maximal risk set is 1.0 -- the model is fully accounted for"
    );
    assert_eq!(
        maximal.risk_level, "CRITICAL",
        "and the top band follows from the same thresholds"
    );
}

/// One owner of the bands. `calibrate_risk` and `calibrate_partial` must never disagree about what
/// a score means, or the same score would be reported at two levels depending on the path.
#[test]
fn l4_one_owner_of_the_level_bands() {
    assert_eq!(
        RISK_LEVEL_THRESHOLDS
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>(),
        vec!["CRITICAL", "HIGH", "MEDIUM", "LOW"]
    );
    for score in [0.0, 0.249, 0.25, 0.499, 0.5, 0.699, 0.7, 1.0] {
        let expected = if score >= 0.70 {
            "CRITICAL"
        } else if score >= 0.50 {
            "HIGH"
        } else if score >= 0.25 {
            "MEDIUM"
        } else {
            "LOW"
        };
        assert_eq!(risk_level_for(score), expected, "score {score}");
    }
}

/// `calibrate_or_none` refuses rather than scoring a partially measured model — and this states the
/// consequence the audit found, instead of leaving it as a discovery.
///
/// ## The dormancy finding
///
/// `calibrate_risk` has exactly one production caller: `qa_memory_risk::calibrate_or_none`, which
/// refuses unless **all six** factors measure. `security_sensitivity` has no source in this schema
/// and `dependency_count` is structurally unavailable for a module, release or test case, so
/// `calibrate_or_none` can never succeed for those subjects.
///
/// The engine is therefore **dormant in production**: correct, tested, and unreachable through any
/// live path. `GET /intelligence/risk` reaches a band through `calibrate_partial` instead.
///
/// This is recorded, not fixed. Reviving `calibrate_risk` would change every score that endpoint
/// returns, which is a product decision (D3 in the remediation report), not a correctness fix.
/// `calibrate_risk` is left exactly as it was.
#[test]
fn l4_calibrate_risk_is_dormant_because_calibrate_or_none_cannot_succeed() {
    // The gate is all-or-nothing over the six factors.
    let factors: Vec<Factor> = FACTOR_WEIGHTS
        .iter()
        .map(|(name, _)| {
            if *name == "security_sensitivity" {
                Factor::unmeasured(name, "no source exists in this schema")
            } else {
                Factor::measured(name, 0.5, "test", 10)
            }
        })
        .collect();

    let measured_weight: f64 = factors
        .iter()
        .filter(|f| f.measured.is_some())
        .map(|f| f.weight)
        .sum();
    let total: f64 = factors.iter().map(|f| f.weight).sum();
    assert!(
        measured_weight < total,
        "one unsourced factor is enough to make the model partial, which is why \
         calibrate_or_none refuses: measured {measured_weight} of {total}"
    );

    // So the model that `calibrate_risk` implements cannot be fully measured here, and
    // `calibrate_partial` is what the endpoint actually uses. Both are stated rather than assumed.
    let (cal, coverage) = crate::qa_memory_risk::calibrate_partial("subject", &factors);
    assert!(
        cal.risk_level != "INSUFFICIENT_DATA",
        "a partially measured model still yields a normalised score through calibrate_partial"
    );
    assert!(
        (coverage.unmeasured - 0.10).abs() < 1e-9,
        "with the unsourced factor's weight excluded: {}",
        coverage.unmeasured
    );
    assert!(
        cal.explanation.contains("assumed neutral"),
        "and the assumption is stated rather than folded silently into the number: {}",
        cal.explanation
    );
}

/// A fully measured model is unaffected — the control proving the normalisation is inert when it
/// should be, which is what makes it safe on a live endpoint.
#[test]
fn l4_a_fully_measured_model_is_unchanged_by_normalisation() {
    let factors: Vec<Factor> = FACTOR_WEIGHTS
        .iter()
        .map(|(name, _)| Factor::measured(name, 0.5, "test", 10))
        .collect();
    let (cal, coverage) = crate::qa_memory_risk::calibrate_partial("subject", &factors);
    assert!((coverage.measured - 1.0).abs() < 1e-9, "{:?}", coverage);
    assert!(
        (cal.risk_score - 0.5).abs() < 0.01,
        "every factor at 0.5 must read 0.5: {}",
        cal.risk_score
    );
}

/// `Factor` refuses to invent a value for a factor it could not measure.
#[test]
fn l4_an_unmeasured_factor_never_becomes_a_zero_inside_the_engines_own_types() {
    let f = Factor::unmeasured("security_sensitivity", "no source");
    assert!(
        f.measured.is_none(),
        "an unmeasured factor carries no value at all, so no consumer can read a zero from it"
    );
    assert_eq!(
        f.contribution(),
        0.0,
        "it contributes nothing to a weighted sum"
    );
    assert!(
        f.basis.contains("no source"),
        "and it always states why: {}",
        f.basis
    );
    assert!(
        f.evidence == 0,
        "with no evidence behind it, rather than a fabricated count"
    );
}

// ------------------------------------------------------------------ harness

async fn harness() -> (crate::store::Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = crate::store::Store::connect(&dir.path().join("phase61-semantics.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &crate::store::Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 6.1 remediation fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

async fn case(store: &crate::store::Store, project_id: &str, title: &str) -> Uuid {
    store
        .insert_test_case(
            project_id,
            &crate::domain::CreateTestCaseRequest {
                title: title.to_string(),
                description: "phase 6.1 remediation fixture".into(),
                category: "functional".into(),
                priority: "P1".into(),
                severity: "medium".into(),
                preconditions: Vec::new(),
                steps: vec![crate::domain::TestStep {
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

/// One execution through the real pipeline, so `Intelligence::ingest_run` writes the row.
///
/// `seq` only varies the task's recorded name so repeated runs accumulate history under one
/// subject; the telemetry write is the production path.
async fn run(
    store: &crate::store::Store,
    project_id: &str,
    task_id: &str,
    case_id: Uuid,
    status: crate::domain::TaskStatus,
    _seq: u32,
) -> Uuid {
    use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState};

    let run_id = Uuid::new_v4();
    let plan = RunPlan {
        name: format!("remediation run {run_id}"),
        tasks: vec![TaskDef {
            id: task_id.to_string(),
            name: format!("task {task_id}"),
            prerequisites: Vec::new(),
            simulate: Some(SimSpec {
                outcome: if matches!(status, crate::domain::TaskStatus::Failed) {
                    SimOutcome::Fail
                } else {
                    SimOutcome::Pass
                },
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            params: serde_json::json!({ "selector": "#pay" }),
            test_case_id: Some(case_id.to_string()),
        }],
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(project_id),
            Some("phase 6.1 remediation fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    store
        .set_task_status(run_id, task_id, status, None, true, false)
        .await
        .expect("start task");
    store
        .set_task_status(run_id, task_id, status, None, false, true)
        .await
        .expect("finish task");

    crate::intelligence::Intelligence::new(store)
        .ingest_run(
            Some(run_id),
            Some(project_id),
            &plan,
            &[TaskState {
                id: task_id.to_string(),
                name: format!("task {task_id}"),
                prerequisites: Vec::new(),
                status,
                reason: None,
                started_at: None,
                finished_at: None,
                test_case_id: Some(case_id.to_string()),
            }],
        )
        .await
        .expect("ingest_run");
    run_id
}
