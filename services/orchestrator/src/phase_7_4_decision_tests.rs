//! Phase 7.4 — tests for the approved risk-metric implementation.
//!
//! | Test group | Step 5 item | What it pins |
//! |---|---|---|
//! | `p74_a_*` | **A** blocked outcomes | adding `BLOCKED` rows moves the all-recorded rate and does **not** move the completed rate |
//! | `p74_b_*` | **B** `INFRA_ERROR` | counted separately; in neither half of the completed rate; never silently a pass; no new weight |
//! | `p74_c_*` | **C** mixed outcomes | the full outcome vocabulary, with `TIMEOUT`'s contribution stated explicitly |
//! | `p74_d_*` | **D** no completed observation | the factor stays **unmeasured**; no fabricated `0.0` |
//! | `p74_e_*` | **E** risk-score impact | every converted path consumes the shared completed rate; weights and thresholds unchanged; unrelated factors unchanged |
//! | `p74_f_*` | **F** rate compatibility | reporting/history consumers still receive the all-recorded rate |
//!
//! ## Discipline
//!
//! Every row is written by a public `Store` writer or `Intelligence::ingest_run`, and every number
//! asserted is read back through the **production aggregation and scoring code** — `test_risk`,
//! `module_risk`, `release_risk`, `test_prioritisation`, or the real HTTP handler. No test here
//! re-implements the arithmetic it is checking: a copied formula would pass while the shipped one
//! is wrong, which is exactly the failure this phase exists to close.
//!
//! Where a test compares against a hand-computed expectation, the expectation is derived from the
//! contract documented on `QaMemoryReader::completed_failure_rate` (numerator = `FAILED` +
//! `TIMEOUT`, denominator = `PASSED` + `FAILED` + `TIMEOUT`), not from the code under test.
//!
//! Step 5 item **G** (the regression guards for project isolation, missing evidence, storage-read
//! failures, the safety floors, change-impact resolution, the Phase 7.1 integrity fixes and the
//! previously corrected metric consistency) is not re-implemented here — those are existing
//! suites, and Phase 7.4 runs them rather than duplicating them. See the report.

use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
};
use crate::intelligence::Intelligence;
use crate::qa_memory::QaMemoryReader;
use crate::qa_memory_risk::Factor;
use crate::store::{ExecutionTelemetryRow, Store};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Db {
    store: Store,
    #[allow(dead_code)]
    dir: TempDir,
}

impl Db {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("phase74.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, dir }
    }
}

async fn make_project(store: &Store, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, None)
        .await
        .expect("insert_project");
    id
}

async fn seed_case(store: &Store, project_id: Uuid, title: &str) -> Uuid {
    store
        .insert_test_case(
            &project_id.to_string(),
            &CreateTestCaseRequest {
                title: title.to_string(),
                description: format!("{title} for a phase 7.4 test"),
                category: "Functional".into(),
                priority: "HIGH".into(),
                severity: "MAJOR".into(),
                preconditions: vec![],
                steps: vec![],
                expected_result: Some("the step completes".into()),
                assertions: vec![],
                test_data: None,
                source_document: Some("p74.md".into()),
                requirement_reference: Some("FR-074".into()),
                source_requirement: Some("FR-074".into()),
                traceability: None,
                risk_factor: None,
                tags: vec![],
                suite_id: None,
            },
            Some("phase74"),
        )
        .await
        .expect("insert_test_case")
}

/// One run whose steps all belong to `case`, recorded through the production writers.
///
/// `steps` is `(label, status, reason)`. The status passed here is the one that reaches
/// `execution_telemetry.status`, so it is the classification the aggregation under test sees.
///
/// The `label` is used only to name the step; the task id written to the database is derived
/// from the run and the step's position. `tasks` is keyed `(run_id, id)`, so a caller that
/// reused one label for several steps of the same run would hit a UNIQUE constraint rather than
/// the assertion it meant to write -- deriving the id removes that whole class of mistake.
async fn record_run(
    store: &Store,
    project_id: &str,
    case: Uuid,
    steps: &[(&str, TaskStatus, Option<&str>)],
) -> Uuid {
    record_run_for_release(store, project_id, case, steps, None).await
}

/// As [`record_run`], optionally attached to a release so `release_risk` can find the run.
async fn record_run_for_release(
    store: &Store,
    project_id: &str,
    case: Uuid,
    steps: &[(&str, TaskStatus, Option<&str>)],
    release: Option<Uuid>,
) -> Uuid {
    let run_id = Uuid::new_v4();
    let task_ids: Vec<String> = (0..steps.len()).map(|i| format!("t{i}-{run_id}")).collect();
    let plan = RunPlan {
        name: "phase 7.4 run".to_string(),
        tasks: steps
            .iter()
            .zip(&task_ids)
            .map(|((label, _, _), id)| TaskDef {
                id: id.clone(),
                name: format!("step {label}"),
                prerequisites: vec![],
                simulate: Some(SimSpec {
                    outcome: SimOutcome::Pass,
                    delay_ms: 1,
                }),
                executor: Some("simulated".into()),
                params: json!({}),
                test_case_id: Some(case.to_string()),
            })
            .collect(),
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(project_id),
            Some("phase74"),
            RunState::Running,
            None,
            release.map(|r| r.to_string()).as_deref(),
        )
        .await
        .expect("insert_run_for_release");
    for ((_, status, reason), id) in steps.iter().zip(&task_ids) {
        store
            .set_task_status(run_id, id, TaskStatus::Running, None, true, false)
            .await
            .expect("start");
        store
            .set_task_status(run_id, id, *status, *reason, false, true)
            .await
            .expect("finish");
    }
    let states: Vec<TaskState> = steps
        .iter()
        .zip(&task_ids)
        .map(|((label, status, reason), id)| TaskState {
            id: id.clone(),
            name: format!("step {label}"),
            prerequisites: vec![],
            status: *status,
            reason: (*reason).map(ToString::to_string),
            started_at: None,
            finished_at: None,
            test_case_id: Some(case.to_string()),
        })
        .collect();
    Intelligence::new(store)
        .ingest_run(Some(run_id), Some(project_id), &plan, &states)
        .await
        .expect("ingest_run");
    run_id
}

async fn telemetry_for(store: &Store, project_id: &str) -> Vec<ExecutionTelemetryRow> {
    store
        .list_execution_telemetry(project_id, 500)
        .await
        .expect("list_execution_telemetry")
}

fn case_rows<'a>(all: &'a [ExecutionTelemetryRow], case: Uuid) -> Vec<&'a ExecutionTelemetryRow> {
    all.iter()
        .filter(|r| r.test_case_id == Some(case))
        .collect()
}

/// The `failure_rate` factor off a real scoring result, or a panic naming what was there instead.
fn failure_factor(factors: &[Factor]) -> &Factor {
    factors
        .iter()
        .find(|f| f.name == "failure_rate")
        .unwrap_or_else(|| {
            panic!(
                "the failure_rate factor must be present; got {:?}",
                factors.iter().map(|f| &f.name).collect::<Vec<_>>()
            )
        })
}

/// The infrastructure-error count for one case, through the production helper.
async fn infra_errors_for(store: &Store, project_id: &str, case: Uuid) -> usize {
    let rows = telemetry_for(store, project_id).await;
    QaMemoryReader::infra_errors(&case_rows(&rows, case))
}

/// `test_risk` for one case, through the production reader.
async fn risk_for(store: &Store, project_id: &str, case: Uuid) -> crate::qa_memory_risk::TestRisk {
    QaMemoryReader::new(store.clone())
        .test_risk(project_id, &case.to_string())
        .await
        .expect("test_risk")
        .view
        .unwrap_or_else(|| panic!("test_risk must produce a view for a case with recorded rows"))
}

/// A project whose case has the same three completed outcomes plus whatever `extra` adds.
///
/// Used by the "only `failure_rate` moved" comparison: the completed evidence is held constant
/// and only the non-completing noise varies, so any factor that differs was affected by this
/// phase.
async fn build_fixture(
    store: &Store,
    name: &'static str,
    extra: Vec<(&'static str, TaskStatus, Option<&'static str>)>,
) -> (String, Uuid, crate::qa_memory_risk::TestRisk) {
    let pid = make_project(store, name).await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Search").await;
    let mut steps: Vec<(&str, TaskStatus, Option<&str>)> = vec![
        ("t", TaskStatus::Failed, Some("Element not found: #q")),
        ("t", TaskStatus::Passed, None),
        ("t", TaskStatus::Failed, Some("Assertion failed: title")),
    ];
    steps.extend(extra);
    record_run(store, &pid_s, case, &steps).await;
    let risk = risk_for(store, &pid_s, case).await;
    (pid_s, case, risk)
}

// ===========================================================================
// A — BLOCKED outcomes must not dilute the completed rate
// ===========================================================================

/// Step 5 A. Two projects, identical completed evidence, differing only in how many `BLOCKED`
/// rows they also carry. The completed-execution factor must be **identical**; the all-recorded
/// figures must differ.
///
/// Read through `test_risk`, the production path -- not through the metric helper -- so this
/// proves the scoring code consumes it, not merely that the helper computes it.
#[tokio::test]
async fn p74_a_blocked_outcomes_do_not_dilute_the_completed_rate() {
    let db = Db::new().await;
    let store = &db.store;

    // --- project "clean": one failure, one pass. Nothing that did not complete. ---
    let clean = make_project(store, "P74 A clean").await;
    let clean_s = clean.to_string();
    let clean_case = seed_case(store, clean, "Search").await;
    record_run(
        store,
        &clean_s,
        clean_case,
        &[("t", TaskStatus::Failed, Some("Element not found: #x"))],
    )
    .await;
    record_run(
        store,
        &clean_s,
        clean_case,
        &[("t", TaskStatus::Passed, None)],
    )
    .await;

    // --- project "blocked": the SAME two completed outcomes, plus two blocked ones. ---
    let blocked = make_project(store, "P74 A blocked").await;
    let blocked_s = blocked.to_string();
    let blocked_case = seed_case(store, blocked, "Search").await;
    record_run(
        store,
        &blocked_s,
        blocked_case,
        &[("t", TaskStatus::Failed, Some("Element not found: #x"))],
    )
    .await;
    record_run(
        store,
        &blocked_s,
        blocked_case,
        &[("t", TaskStatus::Passed, None)],
    )
    .await;
    record_run(
        store,
        &blocked_s,
        blocked_case,
        &[("t", TaskStatus::Blocked, Some("prerequisite did not pass"))],
    )
    .await;
    record_run(
        store,
        &blocked_s,
        blocked_case,
        &[("t", TaskStatus::Blocked, Some("environment unavailable"))],
    )
    .await;

    let clean_risk = risk_for(store, &clean_s, clean_case).await;
    let blocked_risk = risk_for(store, &blocked_s, blocked_case).await;

    let a = failure_factor(&clean_risk.factors);
    let b = failure_factor(&blocked_risk.factors);

    // The contract, computed by hand from the documented numerator/denominator:
    // 1 FAILED of (1 PASSED + 1 FAILED) = 0.5 in both projects.
    assert_eq!(
        a.measured,
        Some(0.5),
        "one failure of two completed executions"
    );
    assert_eq!(
        b.measured,
        Some(0.5),
        "Step 5 A: two BLOCKED rows must not dilute the completed rate. Before Decision 1 this \
         factor read 1 failure / 4 recorded rows = {} for this project.",
        b.measured.unwrap()
    );
    assert_eq!(
        a.measured, b.measured,
        "Step 5 A: identical completed evidence must produce an identical risk factor, whatever \
         else was recorded alongside it."
    );

    // The completed denominator is the same in both, and is what `evidence` counts.
    assert_eq!(a.evidence, 2);
    assert_eq!(
        b.evidence, 2,
        "evidence is the completed count, not the 4 recorded rows"
    );

    // The blocked rows are counted and visible -- they are just not in the rate.
    //
    // NOTE: `TestStabilityView::failure_rate` was **already** completed-based before Phase 7.4
    // (Phase 7.1, P7-D2, changed exactly this denominator). So it is not an independent check
    // that this phase did anything; it is a check that the risk factor and the stability view
    // now agree, instead of the view being completed-based while the factor was not. Before this
    // phase these two disagreed for every project containing a blocked step.
    let clean_view = QaMemoryReader::new(store.clone())
        .test_stability(&clean_s, &clean_case.to_string())
        .await
        .expect("clean stability")
        .view
        .expect("clean stability view");
    let blocked_view = QaMemoryReader::new(store.clone())
        .test_stability(&blocked_s, &blocked_case.to_string())
        .await
        .expect("blocked stability")
        .view
        .expect("blocked stability view");

    assert_eq!(clean_view.executions, 2, "sanity: two recorded rows");
    assert_eq!(blocked_view.executions, 4, "sanity: four recorded rows");
    assert_eq!(clean_view.other_outcomes, 0);
    assert_eq!(
        blocked_view.other_outcomes, 2,
        "the blocked rows are recorded and reported, not dropped"
    );
    assert_eq!(
        clean_view.failure_rate,
        Some(50.0),
        "completed contract: 1 failure of 2 completed"
    );
    assert_eq!(
        blocked_view.failure_rate,
        Some(50.0),
        "Step 5 A: 2 blocked rows must not dilute the completed rate here either"
    );

    // The ALL-RECORDED rate still exists and still DID move, read through the production
    // `QaMemoryReader::rate` helper over the real stored rows.
    //
    // (`module_risk_history` takes a module ref rather than a case id, so it is not the right
    // reader here; the all-recorded figures that *are* keyed by case are asserted where those
    // views are built -- `AffectedTestRisk::failure_rate` in the module test, and
    // `ReleaseRisk::failure_rate` plus the trend points in the release test.)
    let clean_all = telemetry_for(store, &clean_s).await;
    let blocked_all = telemetry_for(store, &blocked_s).await;
    let clean_rows = case_rows(&clean_all, clean_case);
    let blocked_rows = case_rows(&blocked_all, blocked_case);
    let all_recorded = |rows: &[&ExecutionTelemetryRow]| {
        QaMemoryReader::rate(
            rows.iter()
                .filter(|r| QaMemoryReader::is_failure(r))
                .count(),
            rows.len(),
        )
    };

    assert_eq!(clean_rows.len(), 2);
    assert_eq!(blocked_rows.len(), 4);
    assert_eq!(
        all_recorded(&clean_rows),
        Some(50.0),
        "all-recorded contract preserved: 1 failure of 2 recorded rows"
    );
    assert_eq!(
        all_recorded(&blocked_rows),
        Some(25.0),
        "Step 5 A: the all-recorded rate keeps its meaning and DOES count blocked rows \
         (1 of 4 = 25.0%) -- which is exactly why it may not feed the risk factor."
    );
    db.store.pool.close().await;
}

// ===========================================================================
// B — INFRA_ERROR
// ===========================================================================

/// Step 5 B, all five bullets at once.
///
/// Contract: numerator = `FAILED` + `TIMEOUT`; denominator = `PASSED` + `FAILED` + `TIMEOUT`.
/// Fixture: 1 `FAILED`, 1 `PASSED`, 1 `INFRA_ERROR` → `1 / 2 = 0.5`.
///
/// The decisive check is the *pair*: with the infra row present the factor must equal the same
/// fixture built without it. That single equality proves the infra row is in neither the
/// numerator (else the value would rise) nor the denominator (else it would fall) -- and since
/// `0.5 != 0.0`, it also cannot have become a pass.
#[tokio::test]
async fn p74_b_infra_error_is_counted_separately_and_in_neither_half() {
    let db = Db::new().await;
    let store = &db.store;

    // --- reference: 1 FAILED + 1 PASSED, no infrastructure error. ---
    let ref_p = make_project(store, "P74 B reference").await;
    let ref_s = ref_p.to_string();
    let ref_case = seed_case(store, ref_p, "Checkout").await;
    record_run(
        store,
        &ref_s,
        ref_case,
        &[("t", TaskStatus::Failed, Some("Element not found: #pay"))],
    )
    .await;
    record_run(store, &ref_s, ref_case, &[("t", TaskStatus::Passed, None)]).await;

    // --- same completed evidence, plus one INFRA_ERROR from the executor. ---
    let infra_p = make_project(store, "P74 B infra").await;
    let infra_s = infra_p.to_string();
    let infra_case = seed_case(store, infra_p, "Checkout").await;
    record_run(
        store,
        &infra_s,
        infra_case,
        &[("t", TaskStatus::Failed, Some("Element not found: #pay"))],
    )
    .await;
    record_run(
        store,
        &infra_s,
        infra_case,
        &[("t", TaskStatus::Passed, None)],
    )
    .await;
    record_run(
        store,
        &infra_s,
        infra_case,
        &[("t", TaskStatus::InfraError, Some("no browser available"))],
    )
    .await;

    let ref_risk = risk_for(store, &ref_s, ref_case).await;
    let infra_risk = risk_for(store, &infra_s, infra_case).await;
    let a = failure_factor(&ref_risk.factors);
    let b = failure_factor(&infra_risk.factors);

    // 1. It does not increment the product failure numerator.
    // 2. It does not increment the completed pass/fail denominator.
    // 3. It does not silently become a pass.
    assert_eq!(
        a.measured,
        b.measured,
        "Step 5 B: one INFRA_ERROR changed neither the numerator nor the denominator of the \
         completed failure rate ({} vs {})",
        a.measured.unwrap(),
        b.measured.unwrap()
    );
    assert_eq!(
        b.measured,
        Some(0.5),
        "1 FAILED of (1 PASSED + 1 FAILED). Before Decision 2 this factor counted INFRA_ERROR in \
         the numerator, reading 2/3 = {}.",
        b.measured.unwrap()
    );
    assert_ne!(
        b.measured,
        Some(0.0),
        "Step 5 B: an INFRA_ERROR must not silently become a pass -- if it had been treated as a \
         pass the rate would read 1/3, and if it were excluded from both halves it would read 1/2."
    );
    assert_eq!(
        b.evidence, 2,
        "Step 5 B: the completed denominator excludes the INFRA_ERROR row"
    );

    // 4. It is still separately counted, and still visible. Nothing was erased.
    //
    // (`TestRisk` has no `infrastructure_errors` field -- the count lives on the
    // module/release views and on the API response. It is read here from the stored rows
    // through the production helper, which is the same number those surfaces report.)
    assert_eq!(
        infra_errors_for(store, &infra_s, infra_case).await,
        1,
        "Step 5 B: the infrastructure error must remain independently visible"
    );
    assert_eq!(
        infra_errors_for(store, &ref_s, ref_case).await,
        0,
        "and only where it actually occurred"
    );
    let rows = telemetry_for(store, &infra_s).await;
    let mine = case_rows(&rows, infra_case);
    assert_eq!(
        QaMemoryReader::infra_errors(&mine),
        1,
        "the count is derivable from the stored rows through the production helper"
    );
    assert_eq!(
        mine.iter().filter(|r| r.status == "INFRA_ERROR").count(),
        1,
        "and the row is still stored with its own status -- it was not rewritten to PASSED"
    );

    // 5. No new risk weight was introduced for infrastructure reliability.
    assert_eq!(
        infra_risk.factors.len(),
        6,
        "Step 5 B / Decision 2: the model still has exactly its six factors; infrastructure \
         errors gained no weight. Got {:?}.",
        infra_risk
            .factors
            .iter()
            .map(|f| (f.name.clone(), f.weight))
            .collect::<Vec<_>>()
    );
    assert!(
        !infra_risk
            .factors
            .iter()
            .any(|f| f.name.contains("infra") || f.name.contains("infrastructure")),
        "Step 5 B: no factor named after infrastructure exists: {:?}",
        infra_risk
            .factors
            .iter()
            .map(|f| &f.name)
            .collect::<Vec<_>>()
    );
    db.store.pool.close().await;
}

// ===========================================================================
// C — MIXED OUTCOMES, with TIMEOUT's contribution stated
// ===========================================================================

/// Step 5 C. The full outcome vocabulary in one fixture, expected value computed from the
/// documented contract rather than from the code.
///
/// **How `TIMEOUT` contributes, per the established implementation:**
/// `QaMemoryReader::is_failure` matches `FAILED` **or** `TIMEOUT`, so a timeout is in the
/// **numerator**; `is_pass` matches only `PASSED`, and the denominator is `is_pass || is_failure`,
/// so a timeout is in the **denominator** as well. A timeout is therefore a completed execution
/// that did not pass — which is why it stays in, while `INFRA_ERROR` (neither pass nor failure) and
/// `BLOCKED` (never ran) stay out. This test does not change timeout semantics; it records them.
///
/// Fixture: PASSED, FAILED, TIMEOUT, INFRA_ERROR, BLOCKED, SKIPPED, CANCELLED
/// → numerator 2, denominator 3 → **0.6667**.
#[tokio::test]
async fn p74_c_mixed_outcomes_follow_the_established_contract() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P74 C mixed").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Mixed").await;

    record_run(
        store,
        &pid_s,
        case,
        &[
            ("a", TaskStatus::Passed, None),
            ("b", TaskStatus::Failed, Some("Element not found: #a")),
            ("c", TaskStatus::Timeout, Some("exceeded 30000ms")),
            ("d", TaskStatus::InfraError, Some("no browser available")),
            ("e", TaskStatus::Blocked, Some("prerequisite did not pass")),
            (
                "f",
                TaskStatus::Skipped,
                Some("not applicable in this environment"),
            ),
            ("g", TaskStatus::Cancelled, Some("superseded by a rerun")),
        ],
    )
    .await;

    let rows = telemetry_for(store, &pid_s).await;
    let mine = case_rows(&rows, case);
    assert_eq!(mine.len(), 7, "all seven outcomes were persisted");

    // Read from the production helpers, so the contract under test is the shipped one.
    let completed = QaMemoryReader::completed_count(&mine);
    let failures = mine
        .iter()
        .filter(|r| QaMemoryReader::is_failure(r))
        .count();
    assert_eq!(
        completed, 3,
        "TIMEOUT counts as completed; INFRA_ERROR, BLOCKED, SKIPPED and CANCELLED do not"
    );
    assert_eq!(
        failures, 2,
        "FAILED and TIMEOUT are both failures by the established `is_failure`"
    );

    // And the same thing again through the scoring path.
    let risk = risk_for(store, &pid_s, case).await;
    let f = failure_factor(&risk.factors);
    assert!(
        (f.measured.unwrap() - 2.0 / 3.0).abs() < 1e-12,
        "Step 5 C: 2 failures (FAILED + TIMEOUT) of 3 completed = 0.6667, got {:?}",
        f.measured
    );
    assert_eq!(
        f.evidence, 3,
        "the completed denominator is 3, not 7 recorded rows"
    );
    assert_eq!(
        infra_errors_for(store, &pid_s, case).await,
        1,
        "the INFRA_ERROR is still counted and still visible"
    );
    // The all-recorded view keeps its own, different answer over all seven.
    let view = QaMemoryReader::new(store.clone())
        .test_stability(&pid_s, &case.to_string())
        .await
        .expect("stability")
        .view
        .expect("stability view");
    // `TestStabilityView` already reported the COMPLETED rate (Phase 7.1), so it cannot
    // demonstrate the all-recorded contract -- but it does show the split the completed rate is
    // built from, and that all seven rows are still recorded.
    assert_eq!(
        view.executions, 7,
        "all seven recorded rows are still counted"
    );
    assert_eq!(view.passed, 1);
    assert_eq!(view.failed, 2, "FAILED and TIMEOUT are both failures");
    assert_eq!(
        view.other_outcomes, 4,
        "INFRA_ERROR, BLOCKED, SKIPPED and CANCELLED are reported as 'other', never folded in"
    );
    assert_eq!(
        view.failure_rate,
        Some(2.0 / 3.0 * 100.0),
        "2 failures of 3 completed, as a percentage -- matching the risk factor's 2/3"
    );
    // The all-recorded rate over the same seven rows, through the shared helper.
    assert_eq!(
        QaMemoryReader::rate(failures, mine.len()),
        Some(2.0 / 7.0 * 100.0),
        "Step 5 F: the all-recorded rate still counts every outcome -- 2 of 7."
    );
    db.store.pool.close().await;
}

// ===========================================================================
// D — no eligible completed observation stays UNMEASURED
// ===========================================================================

/// Step 5 D. `INFRA_ERROR` alone must leave the factor **unmeasured**, not measured at `0.0`.
///
/// This is the case Phase 7.3 could not fix and Decision 1 now does: a case whose every execution
/// was infrastructure-errored used to report a *perfect* `0.0` failure rate -- a test that has
/// never passed, presented as a test that never fails. Requirement 6 forbids converting missing
/// evidence into a fabricated measured zero.
#[tokio::test]
async fn p74_d_nothing_completed_stays_unmeasured_and_is_not_a_fabricated_zero() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P74 D unmeasured").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Never ran").await;

    record_run(
        store,
        &pid_s,
        case,
        &[("t", TaskStatus::InfraError, Some("no browser available"))],
    )
    .await;

    let risk = risk_for(store, &pid_s, case).await;
    let f = failure_factor(&risk.factors);

    assert_eq!(
        f.measured, None,
        "Step 5 D: with no completed pass/fail observation there is no failure rate to measure. \
         A measured 0.0 here would claim a perfect record for a test that has never run."
    );
    assert_eq!(f.evidence, 0, "an unmeasured factor carries no evidence");
    assert_ne!(
        f.basis, "derived",
        "the basis must say why, not assert a derivation: {}",
        f.basis
    );

    // The weight is still 0.35 -- the model did not shrink, only the measurement is absent.
    assert_eq!(
        f.weight, 0.35,
        "Decision 4: the factor keeps its weight; only its measurement is absent."
    );

    // The consequence is visible and honest: this fixture backs **no** factor at all, because a
    // single infrastructure-errored execution gives no failure rate, no flakiness (one sample
    // cannot show an outcome flip) and no failure severity. Before Phase 7.4 it claimed 0.35 of
    // measured weight on the strength of a fabricated 0.0.
    assert_eq!(
        risk.measured_weight, 0.0,
        "the model now backs none of its weight here, and says so"
    );
    assert_eq!(
        risk.unmeasured_weight, 1.0,
        "all six factors are unmeasured: failure_rate has no completed observation, flakiness has \
         one sample, failure_severity has no categorised failure, and the remaining three have no \
         source in this schema"
    );
    assert!(risk.partial, "and the view reports itself as partial");

    // And the view field agrees with the factor rather than contradicting it.
    assert_eq!(
        risk.failure_rate, None,
        "the view field and the factor must not disagree"
    );
    db.store.pool.close().await;
}

// ===========================================================================
// E — the scoring paths actually consume the completed rate
// ===========================================================================

/// Step 5 E for `test_risk` and, in the same pass, the before/after numbers.
///
/// These are the figures the temporary Phase 7.4 probe measured through the real `test_risk`
/// aggregation, immediately before and after the change. They are pinned here so the report's
/// table cannot drift from the shipped behaviour.
///
/// | fixture | factor before | factor after | why |
/// |---|---|---|---|
/// | 1x `FAILED` | 1.0 | 1.0 | control: every row completed |
/// | `FAILED`, `PASSED` | 0.5 | 0.5 | control: every row completed |
/// | `FAILED`, `PASSED`, `BLOCKED` | 0.3333 | 0.5 | blocked no longer dilutes |
/// | `FAILED`, `PASSED`, 2x `BLOCKED` | 0.25 | 0.5 | blocked no longer dilutes |
/// | 2x `FAILED`, `INFRA_ERROR` | 0.6667 | 1.0 | infra no longer counts as a product failure |
/// | `FAILED`, `TIMEOUT`, `BLOCKED` | 0.6667 | 1.0 | timeout stays; blocked leaves |
/// | `BLOCKED` only | 0.0 (measured) | unmeasured | no fabricated zero |
/// | `INFRA_ERROR` only | 0.0 (measured) | unmeasured | no fabricated zero |
#[tokio::test]
async fn p74_e_test_risk_factor_is_the_completed_rate_with_recorded_before_and_after() {
    let db = Db::new().await;
    let store = &db.store;

    // (label, steps, expected factor after the change)
    let fixtures: Vec<(&str, Vec<(&str, TaskStatus, Option<&str>)>, Option<f64>)> = vec![
        (
            "1x FAILED",
            vec![("t", TaskStatus::Failed, Some("Element not found: #a"))],
            Some(1.0),
        ),
        (
            "FAILED + PASSED",
            vec![
                ("t", TaskStatus::Failed, Some("Element not found: #a")),
                ("t", TaskStatus::Passed, None),
            ],
            Some(0.5),
        ),
        (
            "FAILED + PASSED + BLOCKED",
            vec![
                ("t", TaskStatus::Failed, Some("Element not found: #a")),
                ("t", TaskStatus::Passed, None),
                ("t", TaskStatus::Blocked, Some("prereq")),
            ],
            Some(0.5),
        ),
        (
            "FAILED + PASSED + 2x BLOCKED",
            vec![
                ("t", TaskStatus::Failed, Some("Element not found: #a")),
                ("t", TaskStatus::Passed, None),
                ("t", TaskStatus::Blocked, Some("prereq")),
                ("t", TaskStatus::Blocked, Some("prereq")),
            ],
            Some(0.5),
        ),
        (
            "2x FAILED + INFRA_ERROR",
            vec![
                ("t", TaskStatus::Failed, Some("Element not found: #a")),
                ("t", TaskStatus::Failed, Some("Element not found: #b")),
                ("t", TaskStatus::InfraError, Some("no browser available")),
            ],
            Some(1.0),
        ),
        (
            "FAILED + TIMEOUT + BLOCKED",
            vec![
                ("t", TaskStatus::Failed, Some("Element not found: #a")),
                ("t", TaskStatus::Timeout, None),
                ("t", TaskStatus::Blocked, Some("prereq")),
            ],
            Some(1.0),
        ),
        (
            "BLOCKED only",
            vec![("t", TaskStatus::Blocked, Some("prereq"))],
            None,
        ),
        (
            "INFRA_ERROR only",
            vec![("t", TaskStatus::InfraError, Some("no browser available"))],
            None,
        ),
    ];

    for (i, (label, steps, expected)) in fixtures.iter().enumerate() {
        let pid = make_project(store, &format!("P74 E fixture {i}")).await;
        let pid_s = pid.to_string();
        let case = seed_case(store, pid, label).await;
        record_run(store, &pid_s, case, steps).await;

        let risk = risk_for(store, &pid_s, case).await;
        let f = failure_factor(&risk.factors);
        assert_eq!(
            f.measured, *expected,
            "Step 5 E, fixture `{label}`: expected the completed-execution rate"
        );
        assert_eq!(
            risk.failure_rate, *expected,
            "fixture `{label}`: the view field must agree with the factor"
        );
        assert_eq!(
            f.weight, 0.35,
            "fixture `{label}`: Decision 4 -- the weight is 0.35 in every fixture"
        );
    }
    db.store.pool.close().await;
}

/// Step 5 E for `module_risk`, reached through `test_prioritisation`'s real module aggregation.
///
/// This is the path that would have been easiest to miss: `module_risk` builds its own `rows`
/// from the `covers` relation, so it is a genuinely different aggregation from `test_risk`.
#[tokio::test]
async fn p74_e_module_risk_factor_is_the_completed_rate() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P74 E module").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Search").await;

    // A module node, with the `covers` relation that `module_risk` follows to find cases.
    let module_node = store
        .upsert_quality_node(&pid_s, "module", "search-module", "Search module")
        .await
        .expect("upsert module node");
    let test_node = store
        .upsert_quality_node(
            &pid_s,
            crate::store::TEST_NODE_TYPE,
            &case.to_string(),
            "Search",
        )
        .await
        .expect("upsert test node");
    store
        .upsert_quality_relation(test_node, module_node, "covers")
        .await
        .expect("insert covers relation");

    record_run(
        store,
        &pid_s,
        case,
        &[
            ("t", TaskStatus::Failed, Some("Element not found: #q")),
            ("t", TaskStatus::Passed, None),
            ("t", TaskStatus::Blocked, Some("prereq")),
            ("t", TaskStatus::InfraError, Some("no browser available")),
        ],
    )
    .await;

    let module = QaMemoryReader::new(store.clone())
        .module_risk(&pid_s, "search-module")
        .await
        .expect("module_risk")
        .view
        .expect("module risk view");

    let f = failure_factor(&module.factors);
    assert_eq!(
        f.measured,
        Some(0.5),
        "Step 5 E: module_risk must consume the completed rate -- 1 FAILED of \
         (1 PASSED + 1 FAILED); the BLOCKED and INFRA_ERROR rows are in neither half. The \
         all-recorded denominator would have given {}.",
        1.0 / 4.0
    );
    assert_eq!(
        f.evidence, 2,
        "two completed executions back this module's rate"
    );
    assert_eq!(f.weight, 0.35, "Decision 4: unchanged weight");
    assert!(
        f.basis.contains("completed executions"),
        "the basis names the denominator: {}",
        f.basis
    );

    // The module's REPORTED rate keeps the all-recorded contract (Step 5 F).
    //
    // `ModuleRisk` itself has no `failure_rate` field -- the all-recorded figure this phase
    // deliberately preserved is carried per case, on `AffectedTestRisk`.
    let affected = module
        .affected_tests
        .iter()
        .find(|t| t.test_case_id == case.to_string())
        .expect("the case must be listed among the module's affected tests");
    assert_eq!(
        affected.failure_rate,
        Some(25.0),
        "1 failure of 4 recorded executions = 25.0%, the all-recorded meaning, unchanged."
    );
    assert_eq!(
        affected.completed_failure_rate_percent,
        Some(50.0),
        "and the completed rate is reported beside it: 1 of 2 completed."
    );
    assert_eq!(
        affected.infrastructure_errors, 1,
        "the INFRA_ERROR stays visible on the same record"
    );
    db.store.pool.close().await;
}

/// Step 5 E for `release_risk`, including the **trend loop** — the one converted site whose
/// factor and whose reported figure were previously the same variable.
///
/// The trend loop computes each *other* release's factor through `release_factors`. If the
/// conversion had missed that call, the target release's own factor would be right and every
/// historical point would still be wrong, which is the kind of gap a single-assertion test
/// cannot see.
#[tokio::test]
async fn p74_e_release_risk_and_trend_factors_are_the_completed_rate() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P74 E release").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Checkout").await;

    // Two releases: the target and an earlier one. Both carry the same completed evidence plus a
    // blocked row, so both the target factor and the trend-point factor must read 0.5.
    let older = store
        .create_release(&pid_s, Some("v1.0.0"), Some("v1.0.0"), None, None)
        .await
        .expect("create_release v1");
    let target = store
        .create_release(&pid_s, Some("v2.0.0"), Some("v2.0.0"), None, None)
        .await
        .expect("create_release v2");

    for release in [older.id, target.id] {
        record_run_for_release(
            store,
            &pid_s,
            case,
            &[
                (
                    "t",
                    TaskStatus::Failed,
                    Some("ASSERTION_FAILURE: Element not found: #pay"),
                ),
                ("t", TaskStatus::Passed, None),
                ("t", TaskStatus::Blocked, Some("prereq")),
            ],
            Some(release),
        )
        .await;
    }

    let risk = QaMemoryReader::new(store.clone())
        .release_risk(&pid_s, target.id)
        .await
        .expect("release_risk")
        .view
        .expect("release risk view");

    let f = failure_factor(&risk.factors);
    assert_eq!(
        f.measured,
        Some(0.5),
        "Step 5 E: the target release's factor is the completed rate; the all-recorded \
         denominator would have given {}.",
        1.0 / 3.0
    );
    assert_eq!(f.evidence, 2);
    assert_eq!(f.weight, 0.35, "Decision 4: unchanged weight");

    // The reported figure keeps the all-recorded contract (Step 5 F).
    // `rate()` computes `(1.0/3.0)*100.0`, which is not bit-identical to `100.0/3.0`, so this
    // compares within a tolerance rather than exactly.
    assert!(
        (risk.failure_rate.unwrap() - 100.0 / 3.0).abs() < 1e-9,
        "1 failure of 3 recorded executions, all-recorded meaning preserved: {:?}",
        risk.failure_rate
    );

    // The trend contains a point for the *other* release -- the one computed by the loop.
    let other = risk
        .trend_points
        .iter()
        .find(|p| p.release_id == older.id.to_string())
        .expect("the older release must appear in the target's trend");
    assert_eq!(
        other.executions, 3,
        "the trend point still reports its recorded executions"
    );
    assert!(
        (other.failure_rate.unwrap() - 100.0 / 3.0).abs() < 1e-9,
        "Step 5 F: trend points keep the all-recorded rate, so the trend stays internally \
         comparable with the target's own point: {:?}",
        other.failure_rate
    );

    // ---- the decisive check: the trend loop's *factor* ----
    //
    // `ReleaseTrendPoint` exposes only the reported all-recorded rate, so the loop's risk factor
    // is checked through the one thing it does reach: `risk_score`. `calibrate_or_none` only
    // returns a calibration when all three of the trend's factors are measured, so the fixture
    // uses a categorised failure to make that happen. The other two factors are identical for
    // both releases (same rows), so their values can be read off the target's own factors and
    // the expected score computed from the documented formula.
    let measured = |name: &str| -> f64 {
        risk.factors
            .iter()
            .find(|x| x.name == name)
            .and_then(|x| x.measured)
            .unwrap_or_else(|| {
                panic!(
                    "{name} must be measured on this fixture, or calibrate_or_none refuses and \
                     this check cannot be made at all"
                )
            })
    };
    let flakiness = measured("flakiness_score");
    let severity = measured("failure_severity");

    let observed = other
        .risk_score
        .expect("all three trend factors measure, so a score must be produced");
    let with_completed = (0.35 * 0.5 + 0.20 * flakiness + 0.15 * severity).clamp(0.0, 1.0);
    let with_all_recorded =
        (0.35 * (1.0 / 3.0) + 0.20 * flakiness + 0.15 * severity).clamp(0.0, 1.0);
    let round2 = |v: f64| (v * 100.0).round() / 100.0;

    assert_eq!(
        observed,
        round2(with_completed),
        "Step 5 E: the trend loop fed `calibrate_or_none` the COMPLETED rate (0.35 x 0.5)"
    );
    assert_ne!(
        observed,
        round2(with_all_recorded),
        "and specifically not the all-recorded rate the loop used before (0.35 x 1/3)"
    );
    db.store.pool.close().await;
}

/// Step 5 E: Decision 4 — weights and thresholds are provably unchanged.
///
/// This does not assert "I did not edit them"; it asserts the shipped constants still hold the
/// values the model has always used, so a future edit that moves one fails here.
#[test]
fn p74_e_weights_and_thresholds_are_unchanged() {
    use crate::adaptive_intelligence::RISK_LEVEL_THRESHOLDS;

    assert_eq!(
        RISK_LEVEL_THRESHOLDS,
        [
            ("CRITICAL", 0.70),
            ("HIGH", 0.50),
            ("MEDIUM", 0.25),
            ("LOW", 0.0),
        ],
        "Decision 4: the risk bands are unchanged"
    );

    let f = Factor::measured("failure_rate", 0.5, "test", 2);
    assert_eq!(f.weight, 0.35, "failure_rate weight");
    assert_eq!(
        Factor::measured("flakiness_score", 0.0, "t", 0).weight,
        0.20,
        "flakiness_score weight"
    );
    assert_eq!(
        Factor::measured("failure_severity", 0.0, "t", 0).weight,
        0.15,
        "failure_severity weight"
    );
    assert_eq!(
        Factor::measured("dependency_count", 0.0, "t", 0).weight,
        0.10,
        "dependency_count weight"
    );
    assert_eq!(
        Factor::measured("healing_regression_rate", 0.0, "t", 0).weight,
        0.10,
        "healing_regression_rate weight"
    );
    assert_eq!(
        Factor::measured("security_sensitivity", 0.0, "t", 0).weight,
        0.10,
        "security_sensitivity weight"
    );

    // An unmeasured factor carries the same weight -- Decision 1 changed the *measurement*, and
    // the model must still know how much weight that factor would have had.
    assert_eq!(
        Factor::unmeasured("failure_rate", "nothing completed").weight,
        0.35,
        "Decision 4: an unmeasured failure_rate keeps its declared weight"
    );
}

/// Step 5 E: only the failure-rate factor moved. The other five are untouched by this phase.
///
/// Two fixtures with the same *completed* evidence but different blocked/infra rows: every factor
/// except `failure_rate` must agree. If the change had leaked into `flakiness_score` or
/// `failure_severity`, this is where it would show.
#[tokio::test]
async fn p74_e_only_the_failure_rate_factor_is_affected_by_this_change() {
    let db = Db::new().await;
    let store = &db.store;

    let (plain_s, plain_case, plain) = build_fixture(
        store,
        "P74 E plain",
        vec![("t", TaskStatus::Blocked, Some("prereq"))],
    )
    .await;
    let (noisy_s, noisy_case, noisy) = build_fixture(
        store,
        "P74 E noisy",
        vec![
            ("t", TaskStatus::Blocked, Some("prereq")),
            ("t", TaskStatus::Blocked, Some("prereq")),
            ("t", TaskStatus::InfraError, Some("no browser available")),
            ("t", TaskStatus::Skipped, Some("n/a")),
        ],
    )
    .await;

    let by_name = |risk: &crate::qa_memory_risk::TestRisk| -> Vec<(String, Option<f64>)> {
        risk.factors
            .iter()
            .map(|f| (f.name.clone(), f.measured))
            .collect()
    };
    let a = by_name(&plain);
    let b = by_name(&noisy);

    assert_eq!(a.len(), b.len(), "the same six factors in both fixtures");
    for ((an, av), (bn, bv)) in a.iter().zip(b.iter()) {
        assert_eq!(an, bn, "the same factor names in the same order");
        if an == "failure_rate" {
            // This one is *supposed* to be identical here, because the completed evidence is
            // identical -- which is precisely the point of the change.
            assert_eq!(
                av, bv,
                "failure_rate tracks completed evidence, so identical completed evidence gives \
                 an identical factor regardless of the noise around it"
            );
        } else {
            assert_eq!(
                av, bv,
                "Step 5 E: factor `{an}` must be unaffected by Phase 7.4"
            );
        }
    }
    assert_eq!(
        infra_errors_for(store, &plain_s, plain_case).await,
        0,
        "sanity: the plain fixture had no infra error"
    );
    assert_eq!(
        infra_errors_for(store, &noisy_s, noisy_case).await,
        1,
        "sanity: the noisy fixture had one, and it is counted without touching any other factor"
    );
    db.store.pool.close().await;
}

// ===========================================================================
// F — the all-recorded rate keeps its meaning everywhere it is reported
// ===========================================================================

/// Step 5 F, over HTTP. The project-risk response carries both rates and both must be right.
///
/// The response key `completed_failure_rate` is a **0..1 fraction** (unchanged from Phase 7.3);
/// the identically-named field on the QA-memory view structs is a **0..100 percentage**, because
/// it shares `rate()` with the all-recorded metric. That scale difference is pre-existing and is
/// recorded here rather than silently unified -- unifying it is a breaking response change and is
/// not authorised here.
#[tokio::test]
async fn p74_f_project_risk_response_keeps_both_rates_with_their_own_scales() {
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
    use crate::evidence::LocalEvidenceStore;
    use crate::executor::{ExecutorRegistry, SimulatedExecutor};
    use crate::policy::PolicyConfig;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use tower::ServiceExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase74api.sqlite3"))
        .await
        .expect("connect + migrate");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register("maestro", Arc::new(SimulatedExecutor));
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    };
    let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
    executors.insert("mock", Arc::new(FakeExecutor::ok("{}")));
    let state = AppState::new(
        Arc::new(registry),
        store.clone(),
        policy,
        evidence,
        Arc::new(AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "mock",
                vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
            )))],
            executors,
        )),
        Arc::new(FakeEmbedder::unconfigured()),
    );

    let raw = crate::auth::generate_key();
    // Scoped to no single project: this test reads two of them, and a project-scoped key would
    // answer 403 for the second -- which is the endpoint's isolation rule working correctly, not
    // a defect, and is asserted on its own in the Phase 7.3 suite.
    store
        .insert_api_key(
            Uuid::new_v4(),
            "phase74-key",
            &crate::auth::hash_key(&raw),
            "admin",
            None,
        )
        .await
        .expect("insert_api_key");

    // Two fixtures, and deliberately NOT one fixture carrying both defects.
    //
    // The mutation exercise caught this: a single fixture of FAILED + PASSED + BLOCKED +
    // INFRA_ERROR reads 0.5 under the *correct* completed contract, and also 0.5 under the old
    // one (2 failures of 4 recorded -- the INFRA_ERROR inflating the numerator exactly cancels
    // the BLOCKED diluting the denominator). A test built on that fixture passes whether or not
    // the fix is present. The two error directions are therefore separated so neither can mask
    // the other:
    //
    // | project | rows | completed | old all-recorded |
    // |---|---|---|---|
    // | `diluted` | F, P, BLOCKED, BLOCKED | 1/2 = 0.5 | 1/4 = 0.25 |
    // | `inflated` | F, F, P, INFRA_ERROR | 2/3 = 0.667 | 3/4 = 0.75 |
    let diluted = make_project(&store, "P74 F diluted").await;
    let diluted_case = seed_case(&store, diluted, "Diluted").await;
    record_run(
        &store,
        &diluted.to_string(),
        diluted_case,
        &[
            ("t", TaskStatus::Failed, Some("Element not found: #q")),
            ("t", TaskStatus::Passed, None),
            ("t", TaskStatus::Blocked, Some("prereq")),
            ("t", TaskStatus::Blocked, Some("environment unavailable")),
        ],
    )
    .await;

    let inflated = make_project(&store, "P74 F inflated").await;
    let inflated_case = seed_case(&store, inflated, "Inflated").await;
    record_run(
        &store,
        &inflated.to_string(),
        inflated_case,
        &[
            ("t", TaskStatus::Failed, Some("Element not found: #a")),
            ("t", TaskStatus::Failed, Some("Element not found: #b")),
            ("t", TaskStatus::Passed, None),
            ("t", TaskStatus::InfraError, Some("no browser available")),
        ],
    )
    .await;

    let app = crate::api::router(state);

    // --- the BLOCKED fixture: proves a never-ran step cannot deflate the factor ---
    let fetch = move |pid: Uuid| {
        let app = app.clone();
        let raw = raw.clone();
        async move {
            let resp = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/api/v1/projects/{pid}/intelligence/risk"))
                        .header("Authorization", format!("Bearer {raw}"))
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("router call");
            assert_eq!(resp.status(), StatusCode::OK);
            let bytes = resp.into_body().collect().await.expect("body").to_bytes();
            serde_json::from_slice::<serde_json::Value>(&bytes).expect("json")
        }
    };

    let body = fetch(diluted).await;
    assert_eq!(
        body["completed_failure_rate"].as_f64(),
        Some(0.5),
        "Step 5 F: the completed rate is a 0..1 fraction and excludes BLOCKED: {body}"
    );
    assert_eq!(
        body["completed_executions"].as_u64(),
        Some(2),
        "and the endpoint says how many outcomes it took the rate over: {body}"
    );
    let factor = body["factors_detail"]
        .as_array()
        .expect("factors_detail")
        .iter()
        .find(|f| f["name"] == "failure_rate")
        .expect("the failure_rate factor is reported");
    assert_eq!(
        factor["measured"].as_f64(),
        Some(0.5),
        "Step 5 E: the reported factor IS the completed rate. Two BLOCKED rows must not pull it \
         to 0.25: {factor}"
    );
    assert_ne!(
        factor["measured"].as_f64(),
        Some(0.25),
        "and specifically not the all-recorded value the handler used before this phase"
    );
    assert_eq!(
        factor["weight"].as_f64(),
        Some(0.35),
        "Decision 4: the weight is still 0.35: {factor}"
    );

    // --- the INFRA_ERROR fixture: proves an executor failure cannot inflate the factor ---
    let body = fetch(inflated).await;
    // Tolerance, not equality: the handler computes `(2/3)*100/100`, which is not bit-identical
    // to `2.0/3.0`.
    assert!(
        (body["completed_failure_rate"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9,
        "Step 5 B: INFRA_ERROR is in neither half: {body}"
    );
    assert_eq!(
        body["infrastructure_errors"].as_u64(),
        Some(1),
        "Step 5 B: the infra error remains counted and reported: {body}"
    );
    let factor = body["factors_detail"]
        .as_array()
        .expect("factors_detail")
        .iter()
        .find(|f| f["name"] == "failure_rate")
        .expect("the failure_rate factor is reported");
    assert!(
        (factor["measured"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9,
        "Step 5 E: the reported factor is the completed rate: {factor}"
    );
    assert_ne!(
        factor["measured"].as_f64(),
        Some(0.75),
        "and specifically not the all-recorded value (3 of 4, counting the INFRA_ERROR as a \
         product failure) the handler used before this phase"
    );

    // The endpoint still reports a score, and the response is otherwise intact.
    let body = fetch(diluted).await;
    assert!(
        body["risk_score"].is_number(),
        "the endpoint still returns a score: {body}"
    );
    store.pool.close().await;
}

/// Step 5 F, structural. The reported all-recorded figures must still be computed over **all**
/// recorded rows, at every site the audit found, so that this phase's change cannot be undone by
/// a later reader who assumes they are all completed-based.
#[tokio::test]
async fn p74_f_all_recorded_reporting_still_counts_every_row() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P74 F recorded").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Search").await;

    record_run(
        store,
        &pid_s,
        case,
        &[
            ("t", TaskStatus::Failed, Some("Element not found: #q")),
            ("t", TaskStatus::Passed, None),
            ("t", TaskStatus::Blocked, Some("prereq")),
            ("t", TaskStatus::InfraError, Some("no browser available")),
        ],
    )
    .await;

    let rows = telemetry_for(store, &pid_s).await;
    let mine = case_rows(&rows, case);
    let all_recorded = mine.len();
    let all_failures = mine
        .iter()
        .filter(|r| QaMemoryReader::is_failure(r))
        .count();

    let view = QaMemoryReader::new(store.clone())
        .test_stability(&pid_s, &case.to_string())
        .await
        .expect("stability")
        .view
        .expect("view");

    assert_eq!(view.executions, all_recorded, "all four rows counted");
    assert_eq!(view.failed, all_failures, "one genuine failure");
    assert_eq!(
        view.other_outcomes, 2,
        "the blocked and infra rows are counted as 'other'"
    );
    assert_eq!(
        view.failure_rate,
        Some(all_failures as f64 / 2.0 * 100.0),
        "Step 5 F: TestStabilityView::failure_rate is the COMPLETED rate -- 1 of 2 -- which is \\
         what Phase 7.1 established and what the risk factor now agrees with"
    );
    // The all-recorded rate over the same rows, via the shared helper. This is the figure that
    // still belongs on `ModuleRiskHistory::failure_rate`, `AffectedTestRisk::failure_rate`,
    // `ReleaseRisk::failure_rate` and `ReleaseTrendPoint::failure_rate`, all of which are
    // asserted at those sites in this file.
    assert_eq!(
        QaMemoryReader::rate(all_failures, all_recorded),
        Some(25.0),
        "Step 5 F: 1 failure of 4 recorded rows, the all-recorded meaning, still available"
    );
    assert_eq!(
        QaMemoryReader::infra_errors(&mine),
        1,
        "with the infra error still counted independently"
    );
    db.store.pool.close().await;
}
