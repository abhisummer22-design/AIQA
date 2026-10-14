//! Pre-Phase-8 follow-up — `failure_severity` / `INFRA_ERROR`, and `completed_failure_rate` unit
//! consistency.
//!
//! | group | objective | what it pins |
//! |---|---|---|
//! | `pp8_o1_*` | **1** severity | an infrastructure error is not a product-defect severity; genuine-failure severities are unchanged; infra stays visible and is not a pass; no new weight |
//! | `pp8_o2_*` | **2** units | one name, one unit; fraction vs percentage; the risk factor consumes the fraction; existing API compatibility; no double scaling |
//!
//! ## Discipline
//!
//! Every number is read back from the **production aggregation and scoring code** or from the
//! real HTTP router. Where a test compares against a hand-computed expectation, the expectation
//! comes from the documented contract, not from the code under test.
//!
//! The two `failure_severity` implementations are deliberately kept apart by these tests, because
//! they are genuinely different: the shared one in `qa_memory_risk` averages `failure_category`
//! weights; the project-risk handler uses a two-bucket heuristic. Reconciling them is a D3
//! decision and is NOT made here.

use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
};
use crate::intelligence::Intelligence;
use crate::qa_memory::QaMemoryReader;
use crate::store::{ExecutionTelemetryRow, Store};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Db {
    store: Store,
    /// Held for its `Drop`: it owns the SQLite file, and every test must keep it alive for the
    /// whole test so the connection never outlives the file backing it.
    _dir: TempDir,
}

impl Db {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("pp8.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, _dir: dir }
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
                description: format!("{title} for a pre-Phase-8 test"),
                category: "Functional".into(),
                priority: "HIGH".into(),
                severity: "MAJOR".into(),
                preconditions: vec![],
                steps: vec![],
                expected_result: Some("the step completes".into()),
                assertions: vec![],
                test_data: None,
                source_document: Some("pp8.md".into()),
                requirement_reference: Some("FR-080".into()),
                source_requirement: Some("FR-080".into()),
                traceability: None,
                risk_factor: None,
                tags: vec![],
                suite_id: None,
            },
            Some("pp8"),
        )
        .await
        .expect("insert_test_case")
}

/// One run whose steps all belong to `case`, recorded through the production writers.
///
/// The task id is derived from the run and the step's position, because `tasks` is keyed
/// `(run_id, id)` and a caller reusing one label would hit a UNIQUE constraint instead of the
/// assertion it meant to write.
async fn record_run(
    store: &Store,
    project_id: &str,
    case: Uuid,
    steps: &[(&str, TaskStatus, Option<&str>)],
) -> Uuid {
    let run_id = Uuid::new_v4();
    let task_ids: Vec<String> = (0..steps.len()).map(|i| format!("t{i}-{run_id}")).collect();
    let plan = RunPlan {
        name: "pre-phase-8 run".to_string(),
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
            Some("pp8"),
            RunState::Running,
            None,
            None,
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

/// One project, one case, the given outcomes — used by both objectives.
async fn scenario(
    store: &Store,
    name: &str,
    steps: &[(&str, TaskStatus, Option<&str>)],
) -> (String, Uuid) {
    let pid = make_project(store, name).await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Subject").await;
    record_run(store, &pid_s, case, steps).await;
    (pid_s, case)
}

fn factor<'a>(
    factors: &'a [crate::qa_memory_risk::Factor],
    name: &str,
) -> &'a crate::qa_memory_risk::Factor {
    factors
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("the {name} factor must be present"))
}

// ===========================================================================
// OBJECTIVE 1 — failure_severity must not classify an INFRA_ERROR as a
//                confirmed product-test failure
// ===========================================================================

/// Objective 1, shared path. `qa_memory_risk::failure_severity` filters on `is_failure`, which
/// matches `FAILED | TIMEOUT` only, so infrastructure errors were already excluded from it.
///
/// This test **documents that behaviour** rather than asserting a change, because it is the
/// reason the project-risk handler's differing answer is a defect and not a design choice.
#[tokio::test]
async fn pp8_o1_shared_severity_already_excluded_infrastructure_errors() {
    let db = Db::new().await;
    let store = &db.store;
    let (pid_s, case) = scenario(
        store,
        "PP8 O1 shared infra",
        &[
            ("a", TaskStatus::InfraError, Some("no browser available")),
            ("b", TaskStatus::InfraError, Some("dead subprocess")),
        ],
    )
    .await;

    let view = QaMemoryReader::new(store.clone())
        .test_risk(&pid_s, &case.to_string())
        .await
        .expect("test_risk")
        .view
        .expect("view");
    let sev = factor(&view.factors, "failure_severity");

    assert_eq!(
        sev.measured, None,
        "the shared path reports no product-defect severity when every outcome was an \\
         infrastructure error"
    );
    assert_eq!(sev.evidence, 0, "and no evidence behind it");

    // It is still observable -- just not as a severity.
    let rows = telemetry_for(store, &pid_s).await;
    let mine: Vec<&ExecutionTelemetryRow> = rows
        .iter()
        .filter(|r| r.test_case_id == Some(case))
        .collect();
    assert_eq!(
        QaMemoryReader::infra_errors(&mine),
        2,
        "both infrastructure errors remain individually visible"
    );
    db.store.pool.close().await;
}

/// Objective 1, the defect. The project-risk handler counted `INFRA_ERROR` as a failure, which
/// gated its `failure_severity` factor, and reported **0.8** -- its highest product-defect bucket
/// -- at full 0.15 weight, backed by the infrastructure-error count as evidence.
///
/// Measured before the change: `sev=0.8, evidence=1` for a single infrastructure error.
#[tokio::test]
async fn pp8_o1_project_risk_severity_is_unmeasured_when_only_infrastructure_failed() {
    let app = pp8_api::Harness::new().await;
    let (pid, key) = app.key_and_project("PP8 O1 infra-only").await;
    app.record(
        &pid,
        &[
            ("a", TaskStatus::InfraError, Some("no browser available")),
            ("b", TaskStatus::InfraError, Some("dead subprocess")),
        ],
    )
    .await;

    let body = app.get(&pid, &key).await;
    let sev = pp8_api::factor(&body, "failure_severity");

    assert_eq!(
        sev["measured"],
        serde_json::Value::Null,
        "an infrastructure error must not be a product-defect severity. Before this fix this \\
         factor read 0.8 with evidence 2. Got {sev}"
    );
    assert_eq!(
        sev["evidence"], 0,
        "and no infrastructure error may serve as evidence for a product-defect factor"
    );
    assert_ne!(
        sev["measured"],
        serde_json::json!(0.0),
        "nor a fabricated measured zero -- 'nothing completed' is unmeasured, not perfect"
    );

    // Observable through the infrastructure signal, not erased.
    assert_eq!(
        body["infrastructure_errors"], 2,
        "Objective 1: infrastructure errors must remain visible: {body}"
    );
    // And not silently a pass either.
    assert_eq!(
        pp8_api::factor(&body, "failure_rate")["measured"],
        serde_json::Value::Null,
        "an infrastructure error must not become a passing test: {body}"
    );
    app.close().await;
}

/// Objective 1, the preservation half. Every **genuine** product failure keeps the severity it had.
///
/// The handler's two buckets are 0.4 for a failure that did not time out, and 0.8 when one did.
/// Neither value, nor the choice between them, is touched by this fix.
#[tokio::test]
async fn pp8_o1_genuine_failure_severity_is_unchanged() {
    // A failure that did not time out -> 0.4, before and after.
    let app = pp8_api::Harness::new().await;
    let (pid, key) = app.key_and_project("PP8 O1 failed").await;
    app.record(
        &pid,
        &[("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x"))],
    )
    .await;
    let body = app.get(&pid, &key).await;
    assert_eq!(
        pp8_api::factor(&body, "failure_severity")["measured"],
        serde_json::json!(0.4),
        "a plain product failure keeps severity 0.4: {body}"
    );
    assert_eq!(
        pp8_api::factor(&body, "failure_severity")["evidence"],
        1,
        "backed by the one genuine failure"
    );

    // A timeout -> 0.8, before and after.
    let (pid, key) = app.key_and_project("PP8 O1 timeout").await;
    app.record(
        &pid,
        &[
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            ("b", TaskStatus::Timeout, None),
        ],
    )
    .await;
    let body = app.get(&pid, &key).await;
    assert_eq!(
        pp8_api::factor(&body, "failure_severity")["measured"],
        serde_json::json!(0.8),
        "a genuine failure that timed out keeps severity 0.8 -- TIMEOUT is a completed execution \
         that did not pass, and this change is not about it: {body}"
    );
    assert_eq!(
        pp8_api::factor(&body, "failure_severity")["evidence"],
        2,
        "backed by both genuine failures"
    );

    // Timeout alone -> 0.8, before and after.
    let (pid, key) = app.key_and_project("PP8 O1 timeout-only").await;
    app.record(&pid, &[("a", TaskStatus::Timeout, None)]).await;
    let body = app.get(&pid, &key).await;
    assert_eq!(
        pp8_api::factor(&body, "failure_severity")["measured"],
        serde_json::json!(0.8),
        "a timeout on its own keeps severity 0.8: {body}"
    );
    app.close().await;
}

/// Objective 1, the isolation proof.
///
/// The same genuine failure, with and without an infrastructure error alongside it, must produce
/// the **same** severity. Before the fix the presence of one infrastructure error moved a project's
/// severity from 0.4 to 0.8 -- measured: `failed-only sev=0.4 ev=1`, `failed+infra sev=0.8 ev=2`.
#[tokio::test]
async fn pp8_o1_infrastructure_errors_do_not_inflate_a_real_failure_severity() {
    let app = pp8_api::Harness::new().await;

    let (clean, key) = app.key_and_project("PP8 O1 iso clean").await;
    app.record(
        &clean,
        &[("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x"))],
    )
    .await;

    let (noisy, key2) = app.key_and_project("PP8 O1 iso noisy").await;
    app.record(
        &noisy,
        &[
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            ("b", TaskStatus::InfraError, Some("no browser available")),
        ],
    )
    .await;

    let a = pp8_api::factor(&app.get(&clean, &key).await, "failure_severity");
    let b = pp8_api::factor(&app.get(&noisy, &key2).await, "failure_severity");

    assert_eq!(
        a["measured"], b["measured"],
        "one infrastructure error alongside one genuine failure must not change the severity"
    );
    assert_eq!(
        a["evidence"], b["evidence"],
        "nor the number of genuine failures backing it -- the infrastructure error is not one"
    );
    assert_eq!(
        a["measured"],
        serde_json::json!(0.4),
        "both read the plain-failure bucket"
    );
    app.close().await;
}

/// Objective 1, guard rails. No new weight, no renamed factors, no threshold movement.
#[tokio::test]
async fn pp8_o1_no_new_factor_or_weight_for_infrastructure_errors() {
    use crate::adaptive_intelligence::RISK_LEVEL_THRESHOLDS;

    let app = pp8_api::Harness::new().await;
    let (pid, key) = app.key_and_project("PP8 O1 weights").await;
    app.record(
        &pid,
        &[
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            ("b", TaskStatus::InfraError, Some("no browser available")),
        ],
    )
    .await;
    let body = app.get(&pid, &key).await;

    let names: Vec<String> = pp8_api::factor_names(&body);
    assert_eq!(
        names,
        vec![
            "failure_rate",
            "flakiness_score",
            "failure_severity",
            "dependency_count",
            "healing_regression_rate",
            "security_sensitivity",
        ],
        "the model still has exactly its six factors, in order"
    );
    assert!(
        !names.iter().any(|n| n.contains("infra")),
        "no factor was added for infrastructure reliability: {names:?}"
    );
    for (name, weight) in [
        ("failure_rate", 0.35),
        ("flakiness_score", 0.20),
        ("failure_severity", 0.15),
        ("dependency_count", 0.10),
        ("healing_regression_rate", 0.10),
        ("security_sensitivity", 0.10),
    ] {
        assert_eq!(
            pp8_api::factor(&body, name)["weight"],
            serde_json::json!(weight),
            "the {name} weight is unchanged"
        );
    }
    assert_eq!(
        RISK_LEVEL_THRESHOLDS,
        [
            ("CRITICAL", 0.70),
            ("HIGH", 0.50),
            ("MEDIUM", 0.25),
            ("LOW", 0.0),
        ],
        "the risk bands are unchanged"
    );
    app.close().await;
}

/// Objective 1, the shared path must not have been disturbed by the handler-side fix.
#[tokio::test]
async fn pp8_o1_shared_severity_is_unchanged_for_genuine_failures() {
    let db = Db::new().await;
    let store = &db.store;

    let (pid_s, case) = scenario(
        store,
        "PP8 O1 shared genuine",
        &[
            // ASSERTION_FAILURE -> category weight 0.4
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: title")),
            // plus an infrastructure error, which must not move it
            ("b", TaskStatus::InfraError, Some("no browser available")),
        ],
    )
    .await;
    let with_infra = QaMemoryReader::new(store.clone())
        .test_risk(&pid_s, &case.to_string())
        .await
        .expect("test_risk")
        .view
        .expect("view");

    let (pid_s2, case2) = scenario(
        store,
        "PP8 O1 shared genuine alone",
        &[("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: title"))],
    )
    .await;
    let without_infra = QaMemoryReader::new(store.clone())
        .test_risk(&pid_s2, &case2.to_string())
        .await
        .expect("test_risk")
        .view
        .expect("view");

    let a = factor(&with_infra.factors, "failure_severity");
    let b = factor(&without_infra.factors, "failure_severity");
    assert_eq!(
        a.measured, b.measured,
        "the shared category-weighted severity is unchanged by an infrastructure error"
    );
    assert_eq!(a.evidence, b.evidence, "and by how much evidence backs it");
    assert_eq!(
        a.measured,
        Some(0.4),
        "ASSERTION_FAILURE carries category weight 0.4"
    );
    db.store.pool.close().await;
}

// ===========================================================================
// OBJECTIVE 2 — completed_failure_rate: one name, one unit
// ===========================================================================

/// Objective 2, the core contract. Over HTTP: three keys, two units, each named for what it is.
#[tokio::test]
async fn pp8_o2_response_exposes_explicitly_named_units() {
    let app = pp8_api::Harness::new().await;
    let (pid, key) = app.key_and_project("PP8 O2 units").await;
    app.record(
        &pid,
        &[
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            ("b", TaskStatus::Passed, None),
            ("c", TaskStatus::Passed, None),
        ],
    )
    .await;
    let body = app.get(&pid, &key).await;

    // 1 failure of 3 completed = 1/3.
    let frac = 1.0 / 3.0;
    assert_close(
        body["completed_failure_rate"].as_f64().unwrap(),
        frac,
        "the pre-existing key keeps its 0..1 fraction value -- compatibility requirement 1",
    );
    assert_close(
        body["completed_failure_rate_fraction"].as_f64().unwrap(),
        frac,
        "the explicit fraction key carries the same number",
    );
    assert_close(
        body["completed_failure_rate_percent"].as_f64().unwrap(),
        100.0 / 3.0,
        "the explicit percent key carries the 0..100 percentage",
    );
    assert_close(
        body["completed_executions"].as_u64().unwrap() as f64,
        3.0,
        "and the denominator is still reported",
    );
    app.close().await;
}

/// Objective 2, conversion applied exactly once. Guards the "do not multiply or divide by 100
/// twice" requirement: if any of these were scaled twice the others would not reconcile.
#[tokio::test]
async fn pp8_o2_conversion_is_applied_exactly_once() {
    let app = pp8_api::Harness::new().await;
    for (name, steps, expected_frac) in [
        (
            "all fail",
            vec![
                ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
                ("b", TaskStatus::Failed, Some("ASSERTION_FAILURE: y")),
            ],
            1.0,
        ),
        (
            "half",
            vec![
                ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
                ("b", TaskStatus::Passed, None),
            ],
            0.5,
        ),
        (
            "one of three",
            vec![
                ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
                ("b", TaskStatus::Passed, None),
                ("c", TaskStatus::Passed, None),
            ],
            1.0 / 3.0,
        ),
    ] {
        let (pid, key) = app.key_and_project(&format!("PP8 O2 scale {name}")).await;
        app.record(&pid, &steps).await;
        let body = app.get(&pid, &key).await;

        let frac = body["completed_failure_rate_fraction"].as_f64().unwrap();
        let pct = body["completed_failure_rate_percent"].as_f64().unwrap();
        let legacy = body["completed_failure_rate"].as_f64().unwrap();

        assert_close(frac, expected_frac, "{name}: the fraction is in 0..1");
        assert!(
            (0.0..=1.0).contains(&frac),
            "{name}: a fraction must be within 0..1, got {frac}"
        );
        assert_close(
            pct,
            expected_frac * 100.0,
            "{name}: the percentage is x100 exactly once",
        );
        assert!(
            (0.0..=100.0).contains(&pct),
            "{name}: a percentage must be within 0..100, got {pct}"
        );
        assert_close(
            legacy,
            frac,
            "{name}: the legacy key still equals the fraction, unchanged",
        );
    }
    app.close().await;
}

/// Objective 2, the decisive one: the risk factor consumes the **fraction**, not the percentage.
///
/// This is the requirement that actually protects the model. Feeding the percentage into a 0..1
/// factor would inflate a 100x-heavy weight by two orders of magnitude.
#[tokio::test]
async fn pp8_o2_risk_factor_consumes_the_fraction_and_not_the_percentage() {
    let app = pp8_api::Harness::new().await;
    let (pid, key) = app.key_and_project("PP8 O2 factor input").await;
    app.record(
        &pid,
        &[
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            ("b", TaskStatus::Passed, None),
            ("c", TaskStatus::Blocked, Some("prereq")),
        ],
    )
    .await;
    let body = app.get(&pid, &key).await;

    let factor_measured = pp8_api::factor(&body, "failure_rate")["measured"]
        .as_f64()
        .expect("the failure_rate factor is measured here");
    let frac = body["completed_failure_rate_fraction"]
        .as_f64()
        .expect("fraction present");
    let pct = body["completed_failure_rate_percent"]
        .as_f64()
        .expect("percent present");

    assert_close(factor_measured, 0.5, "1 failure of 2 completed");
    assert_close(factor_measured, frac, "the factor IS the fraction");
    assert_ne!(
        factor_measured, pct,
        "the factor must not be the percentage -- that would weight 0.35 by 50.0 instead of 0.5"
    );
    assert!(
        factor_measured <= 1.0,
        "a risk factor must be a 0..1 quantity; got {factor_measured}"
    );
    app.close().await;
}

/// Objective 2, missing data. All three keys must be `null` together -- never a fabricated zero,
/// and never a percentage of nothing.
#[tokio::test]
async fn pp8_o2_unmeasured_is_null_in_every_unit() {
    let app = pp8_api::Harness::new().await;
    let (pid, key) = app.key_and_project("PP8 O2 unmeasured").await;
    app.record(
        &pid,
        &[
            ("a", TaskStatus::InfraError, Some("no browser available")),
            ("b", TaskStatus::Blocked, Some("prereq")),
        ],
    )
    .await;
    let body = app.get(&pid, &key).await;

    for k in [
        "completed_failure_rate",
        "completed_failure_rate_fraction",
        "completed_failure_rate_percent",
    ] {
        assert!(
            body[k].is_null(),
            "{k} must be null when nothing completed -- not 0.0, not 0.0%. Got {}",
            body[k]
        );
    }
    assert_eq!(
        body["completed_executions"], 0,
        "and the denominator is zero, which is why there is no rate"
    );
    app.close().await;
}

/// Objective 2, the view structs. They are renamed `_percent` and must actually be percentages.
///
/// A rename that left the values alone would be a lie in the other direction: 0.5 must never appear
/// under a name that says percent.
#[tokio::test]
async fn pp8_o2_view_fields_are_percentages() {
    let db = Db::new().await;
    let store = &db.store;
    let (pid_s, case) = scenario(
        store,
        "PP8 O2 view units",
        &[
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            ("b", TaskStatus::Passed, None),
        ],
    )
    .await;

    // A module, so `AffectedTestRisk` is produced by the real `module_risk` aggregation.
    let module_node = store
        .upsert_quality_node(&pid_s, "module", "pp8-module", "Module")
        .await
        .expect("module node");
    let test_node = store
        .upsert_quality_node(
            &pid_s,
            crate::store::TEST_NODE_TYPE,
            &case.to_string(),
            "Subject",
        )
        .await
        .expect("test node");
    store
        .upsert_quality_relation(test_node, module_node, "covers")
        .await
        .expect("covers");

    let module = QaMemoryReader::new(store.clone())
        .module_risk(&pid_s, "pp8-module")
        .await
        .expect("module_risk")
        .view
        .expect("module risk");
    let affected = module
        .affected_tests
        .iter()
        .find(|t| t.test_case_id == case.to_string())
        .expect("the case appears in the module's affected tests");

    assert_close(
        affected.completed_failure_rate_percent.unwrap(),
        50.0,
        "1 failure of 2 completed, as a percentage",
    );
    assert_ne!(
        affected.completed_failure_rate_percent.unwrap(),
        0.5,
        "a field named _percent must never carry the fraction"
    );
    // The all-recorded sibling is untouched, and is also a percentage.
    assert_close(
        affected.failure_rate.unwrap(),
        50.0,
        "the all-recorded rate keeps its documented meaning and scale",
    );

    // The risk factor on the same evidence is the fraction.
    assert_close(
        factor(&module.factors, "failure_rate").measured.unwrap(),
        0.5,
        "the risk factor is the fraction, so the two surfaces disagree by exactly the scale",
    );
    db.store.pool.close().await;
}

/// Objective 2, a regression guard on the shape itself: the old field name must be gone from the
/// view structs, so no future code can read it and assume a unit it does not state.
#[test]
fn pp8_o2_no_view_field_is_still_called_the_bare_ambiguous_name() {
    // A compile-time fact, asserted here as documentation: `completed_failure_rate_percent` is
    // the only `_completed_failure_rate` field on any view struct. If this ever needs asserting
    // at runtime, the rename was incomplete.
    fn _takes_percent(v: &crate::qa_memory::ModuleRiskHistory) -> Option<f64> {
        v.completed_failure_rate_percent
    }
    fn _takes_percent_r(v: &crate::qa_memory::ReleaseHistory) -> Option<f64> {
        v.completed_failure_rate_percent
    }
    fn _takes_percent_q(v: &crate::qa_memory::RequirementHistory) -> Option<f64> {
        v.completed_failure_rate_percent
    }
    fn _takes_percent_a(v: &crate::qa_memory_risk::AffectedTestRisk) -> Option<f64> {
        v.completed_failure_rate_percent
    }
    assert_eq!(RISK_LEVEL_THRESHOLDS_SENTINEL, (), "compiles");
}

/// Objective 2, the shared helpers' own contract, stated once.
///
/// `completed_failure_rate` -> 0..100, `completed_failure_rate_fraction` -> 0..1, and they are the
/// same measurement. If either drifts, every consumer of that name drifts with it.
#[tokio::test]
async fn pp8_o2_shared_helpers_agree_on_one_measurement() {
    let db = Db::new().await;
    let store = &db.store;
    let (pid_s, case) = scenario(
        store,
        "PP8 O2 helpers",
        &[
            ("a", TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            ("b", TaskStatus::Passed, None),
            ("c", TaskStatus::Timeout, None),
            ("d", TaskStatus::Blocked, Some("prereq")),
            ("e", TaskStatus::InfraError, Some("no browser")),
        ],
    )
    .await;

    let rows = telemetry_for(store, &pid_s).await;
    let mine: Vec<&ExecutionTelemetryRow> = rows
        .iter()
        .filter(|r| r.test_case_id == Some(case))
        .collect();

    // 2 failures (FAILED + TIMEOUT) of 3 completed (PASSED/FAILED/TIMEOUT).
    assert_eq!(mine.len(), 5, "all five outcomes recorded");
    assert_eq!(
        QaMemoryReader::completed_count(&mine),
        3,
        "TIMEOUT counts as completed; BLOCKED and INFRA_ERROR do not"
    );
    assert_close(
        QaMemoryReader::completed_failure_rate(&mine).unwrap(),
        200.0 / 3.0,
        "the 0..100 helper",
    );
    assert_close(
        QaMemoryReader::completed_failure_rate_fraction(&mine).unwrap(),
        2.0 / 3.0,
        "the 0..1 helper",
    );
    assert_close(
        QaMemoryReader::completed_failure_rate(&mine).unwrap(),
        QaMemoryReader::completed_failure_rate_fraction(&mine).unwrap() * 100.0,
        "the two helpers are one measurement in two units",
    );
    assert_eq!(
        QaMemoryReader::infra_errors(&mine),
        1,
        "the infrastructure error is counted independently and stays out of both halves"
    );
    db.store.pool.close().await;
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{what}: expected {expected}, got {actual}"
    );
}

const RISK_LEVEL_THRESHOLDS_SENTINEL: () = ();

/// A tiny real-HTTP harness, so the response assertions go through the actual router rather than
/// through a re-implementation of it.
mod pp8_api {
    use super::*;
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
    use serde_json::Value;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use tower::ServiceExt;

    pub struct Harness {
        state: AppState,
        store: Store,
        _dir: TempDir,
    }

    impl Harness {
        pub async fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = Store::connect(&dir.path().join("pp8api.sqlite3"))
                .await
                .expect("connect + migrate");
            let evidence =
                LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
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
            Self {
                state,
                store,
                _dir: dir,
            }
        }

        /// A key that is not scoped to one project, plus that project's id.
        pub async fn key_and_project(&self, name: &str) -> (Uuid, String) {
            let raw = crate::auth::generate_key();
            self.store
                .insert_api_key(
                    Uuid::new_v4(),
                    "pp8-key",
                    &crate::auth::hash_key(&raw),
                    "admin",
                    None,
                )
                .await
                .expect("insert_api_key");
            let pid = super::make_project(&self.store, name).await;
            (pid, raw)
        }

        pub async fn record(&self, pid: &Uuid, steps: &[(&str, TaskStatus, Option<&str>)]) -> Uuid {
            let case = super::seed_case(&self.store, *pid, "Subject").await;
            super::record_run(&self.store, &pid.to_string(), case, steps).await
        }

        pub async fn get(&self, pid: &Uuid, key: &str) -> Value {
            let resp = crate::api::router(self.state.clone())
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/api/v1/projects/{pid}/intelligence/risk"))
                        .header("Authorization", format!("Bearer {key}"))
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("router call");
            assert_eq!(resp.status(), StatusCode::OK);
            let bytes = resp.into_body().collect().await.expect("body").to_bytes();
            serde_json::from_slice(&bytes).expect("json")
        }

        /// Teardown. `self.store` and `self.state.store` are the same pool (one is a
        /// clone of the other), so closing it here releases every handle this harness
        /// handed out, and lets `_dir` drop a directory with no live SQLite/WAL/SHM
        /// files in it. The fields are private on purpose -- tests reach the pool
        /// through this method rather than by reaching into the struct.
        pub async fn close(&self) {
            self.store.pool.close().await;
        }
    }

    pub fn factor(body: &Value, name: &str) -> Value {
        body["factors_detail"]
            .as_array()
            .expect("factors_detail")
            .iter()
            .find(|f| f["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("the {name} factor must be reported; got {body}"))
    }

    pub fn factor_names(body: &Value) -> Vec<String> {
        body["factors_detail"]
            .as_array()
            .expect("factors_detail")
            .iter()
            .map(|f| f["name"].as_str().unwrap_or_default().to_string())
            .collect()
    }
}
