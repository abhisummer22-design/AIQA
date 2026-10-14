//! Phase 4 (AI Risk-Based Testing + Test Prioritization) — tests.
//!
//! ## The rule
//!
//! Every row here is produced by a real production writer: `insert_test_case` for the
//! case, `Intelligence::ingest_run` for the telemetry, the cluster, the `fails_with`
//! edge and — on a repeat — the bug draft, `insert_healing_proposal` for the fix, and
//! `update_healing_proposal_state` for the verification. Nothing hand-writes a
//! fingerprint or a risk score.
//!
//! ## What is deliberately negative
//!
//! Over half this module is negative testing, because the failure modes that matter for
//! risk are all *over*-statements: inventing risk where there is none, letting one
//! tenant's history colour another's, letting missing evidence default to HIGH. A suite
//! that only checks that a risky test ranks high cannot detect any of them.

use crate::adaptive_intelligence::FailureCategory;
use crate::domain::CreateTestCaseRequest;
use crate::domain::{
    RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus, TestStep,
};
use crate::governed_healing::{HealingVerification, VerificationOutcome};
use crate::intelligence::Intelligence;
use crate::qa_memory::{InsufficientData, QaMemoryReader, Retrieved};
use crate::qa_memory_risk::{PriorityBand, TestRisk, FLAKY_THRESHOLD};
use crate::store::{HealingProposalRow, Store};
use uuid::Uuid;

/// `Retrieved` has exactly three fields, so a `let … = … else { panic!() }` that binds a
/// `..` is unreachable by construction — clippy is right, and the guard did nothing.
/// These two make "is there a view?" an ordinary value a test asserts on, which is what
/// it actually is.
fn present<T>(r: Retrieved<T>) -> T {
    assert!(
        r.insufficient_data.is_none(),
        "this test asserts a view exists, but the reader reported {:?}",
        r.insufficient_data.map(|i| i.as_str())
    );
    r.view.expect("a view was reported")
}

/// For the tests whose subject is the *absence* of a view.
fn absent<T>(r: Retrieved<T>) -> (Option<InsufficientData>, Option<T>) {
    (r.insufficient_data, r.view)
}

// ------------------------------------------------------------------ harness

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase4-risk.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 4 fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

fn case_req(title: &str) -> CreateTestCaseRequest {
    CreateTestCaseRequest {
        title: title.to_string(),
        description: "Controlled data for Phase 4.".into(),
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
        requirement_reference: Some("REQ-P4-1".into()),
        source_requirement: None,
        traceability: None,
        risk_factor: None,
        tags: Vec::new(),
        suite_id: None,
    }
}

/// A persisted case, linked to a requirement by the real writer.
async fn case(store: &Store, project_id: &str, title: &str) -> Uuid {
    let id = store
        .insert_test_case(project_id, &case_req(title), None)
        .await
        .expect("insert case");
    Intelligence::new(store)
        .link_test_case_to_requirement(project_id, id, title, Some("REQ-P4-1"), None)
        .await
        .expect("link requirement");
    id
}

/// Record one execution of a case through the real ingest path.
async fn run(
    store: &Store,
    project_id: &str,
    case_id: Uuid,
    status: TaskStatus,
    reason: Option<&str>,
    selector: &str,
) -> Uuid {
    let run_id = Uuid::new_v4();
    let task_id = format!("task-{run_id}");
    let plan = RunPlan {
        name: format!("phase 4 run {run_id}"),
        tasks: vec![TaskDef {
            id: task_id.clone(),
            name: format!("Test Case: {case_id}"),
            prerequisites: Vec::new(),
            simulate: Some(SimSpec {
                outcome: if status == TaskStatus::Passed {
                    SimOutcome::Pass
                } else {
                    SimOutcome::Fail
                },
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            params: serde_json::json!({ "selector": selector }),
            test_case_id: Some(case_id.to_string()),
        }],
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(project_id),
            Some("phase 4 fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");
    Intelligence::new(store)
        .ingest_run(
            Some(run_id),
            Some(project_id),
            &plan,
            &[TaskState {
                id: task_id,
                name: format!("Test Case: {case_id}"),
                prerequisites: Vec::new(),
                status,
                reason: reason.map(ToString::to_string),
                started_at: None,
                finished_at: None,
                test_case_id: Some(case_id.to_string()),
            }],
        )
        .await
        .expect("ingest_run");
    run_id
}

/// A failing run with a recorded category, so `failure_severity` is measurable.
async fn failing(store: &Store, project_id: &str, case_id: Uuid, n: u32) {
    for _ in 0..n {
        run(
            store,
            project_id,
            case_id,
            TaskStatus::Failed,
            Some("Element not found: #pay"),
            "#pay",
        )
        .await;
    }
}

fn reader(store: &Store) -> QaMemoryReader {
    QaMemoryReader::new(store.clone())
}

// ------------------------------------------------------------------ the score refusal

/// The central finding: **no risk score is produced, and that is correct.**
///
/// `calibrate_risk` needs six factors. Two have no column anywhere in this schema, so
/// `calibrate_or_none` refuses. Pinning `calibration.is_none()` would freeze a
/// limitation; the assertions that matter are below — the factors are reported with
/// reasons, the measured share is stated, and no number stands in for the gap.
#[tokio::test]
async fn no_risk_score_is_invented_for_the_factors_that_have_no_column() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 NoScore").await;
    let c = case(&store, &pid, "A failing case").await;
    failing(&store, &pid, c, 3).await;

    let risk: TestRisk = present(
        reader(&store)
            .test_risk(&pid, &c.to_string())
            .await
            .expect("test_risk"),
    );

    assert!(
        risk.calibration.is_none(),
        "a partial weighted sum must never be emitted as a score"
    );
    assert!(
        risk.partial,
        "the response must say the model was only partly applied"
    );
    assert!(
        risk.unmeasured_weight > 0.0,
        "the unmeasured share must be stated, not implied"
    );
    assert!(
        (risk.measured_weight + risk.unmeasured_weight - 1.0).abs() < 1e-9,
        "measured and unmeasured weight must account for the whole model: {} + {}",
        risk.measured_weight,
        risk.unmeasured_weight
    );

    let by_name = |n: &str| {
        risk.factors
            .iter()
            .find(|f| f.name == n)
            .unwrap_or_else(|| panic!("factor {n} must be reported"))
            .clone()
    };
    assert!(by_name("failure_rate").measured.is_some());
    assert!(
        by_name("failure_rate").evidence > 0,
        "a measured factor must say how many rows backed it"
    );
    assert_eq!(
        by_name("dependency_count").measured,
        None,
        "there is no case-to-case dependency graph to measure"
    );
    assert!(
        by_name("dependency_count")
            .basis
            .contains("dependency graph"),
        "the reason must be concrete: {}",
        by_name("dependency_count").basis
    );
    assert_eq!(
        by_name("security_sensitivity").measured,
        None,
        "there is no security_sensitivity field for a test case"
    );
    assert_eq!(by_name("dependency_count").evidence, 0);
    store.pool.close().await;
}

// ------------------------------------------------------------------ insufficient data

/// An empty project must report `INSUFFICIENT_DATA`, never a fabricated LOW risk.
#[tokio::test]
async fn an_empty_project_reports_insufficient_data_not_low_risk() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Empty").await;

    let (marker, view) = absent(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    assert_eq!(marker, Some(InsufficientData::NoExecutions));
    assert!(
        view.is_none(),
        "no ordering may be invented from no history"
    );

    // Same for a single case in a project with no telemetry at all.
    let (marker, view) = absent(
        reader(&store)
            .test_risk(&pid, &Uuid::new_v4().to_string())
            .await
            .expect("test_risk"),
    );
    assert_eq!(marker, Some(InsufficientData::NoExecutions));
    assert!(view.is_none());
    store.pool.close().await;
}

/// A case with no executions must report `NO_MATCHING_HISTORY`, not a low score.
///
/// "We looked and found nothing about this case" is not the same as "this case is low
/// risk", and conflating them is how a genuinely unexercised test gets deprioritised.
#[tokio::test]
async fn a_case_with_no_executions_is_not_low_risk() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 NoHistory").await;
    let executed = case(&store, &pid, "Executed").await;
    let never = case(&store, &pid, "Never executed").await;
    run(&store, &pid, executed, TaskStatus::Passed, None, "#x").await;

    let (marker, view) = absent(
        reader(&store)
            .test_risk(&pid, &never.to_string())
            .await
            .expect("test_risk"),
    );
    assert_eq!(marker, Some(InsufficientData::NoMatchingHistory));
    assert!(
        view.is_none(),
        "an unexecuted case must not receive a risk view at all"
    );

    // And it must not appear in the ordering either, because nothing is known about it.
    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    let ids: Vec<&str> = order
        .tests
        .iter()
        .map(|t| t.test_case_id.as_str())
        .collect();
    assert!(ids.contains(&executed.to_string().as_str()));
    assert!(
        !ids.contains(&never.to_string().as_str()),
        "a case with no evidence must not be ranked: {ids:?}"
    );
    store.pool.close().await;
}

/// A malformed case id is `UNKNOWN_SUBJECT`, not a crash or a silent no-match.
#[tokio::test]
async fn a_malformed_case_id_is_unknown_subject() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 BadId").await;
    let c = case(&store, &pid, "Anything").await;
    run(&store, &pid, c, TaskStatus::Passed, None, "#x").await;

    let (marker, view) = absent(
        reader(&store)
            .test_risk(&pid, "not-a-uuid")
            .await
            .expect("test_risk"),
    );
    assert_eq!(marker, Some(InsufficientData::UnknownSubject));
    assert!(view.is_none());
    store.pool.close().await;
}

// ------------------------------------------------------------------ prioritization

/// A case with recorded failures outranks one with a clean record, with a stated reason.
#[tokio::test]
async fn a_failing_case_outranks_a_clean_one_with_a_stated_reason() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Order").await;
    let broken = case(&store, &pid, "Broken checkout").await;
    let healthy = case(&store, &pid, "Healthy checkout").await;

    failing(&store, &pid, broken, 4).await;
    for _ in 0..4 {
        run(&store, &pid, healthy, TaskStatus::Passed, None, "#x").await;
    }

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );

    let first = order.tests.first().expect("at least one test");
    assert_eq!(
        first.test_case_id,
        broken.to_string(),
        "the failing case must lead: {:?}",
        order.tests
    );
    assert_eq!(first.priority, PriorityBand::Mandatory);
    assert!(
        first.reasons.iter().any(|r| r.contains("safety floor")),
        "the reason must name the floor: {:?}",
        first.reasons
    );
    assert!(first.applies.contains(&PriorityBand::Mandatory));

    let last = order.tests.last().expect("at least two tests");
    assert_eq!(last.test_case_id, healthy.to_string());
    assert!(
        !last.applies.contains(&PriorityBand::Mandatory),
        "a clean case must not be on the floor"
    );
    store.pool.close().await;
}

/// Every case with history appears — prioritisation orders, it does not exclude.
#[tokio::test]
async fn prioritisation_lists_every_case_and_excludes_nothing() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 AllListed").await;
    let mut made = Vec::new();
    for i in 0..6 {
        let c = case(&store, &pid, &format!("Case {i}")).await;
        made.push(c);
        if i % 2 == 0 {
            failing(&store, &pid, c, 2).await;
        } else {
            run(&store, &pid, c, TaskStatus::Passed, None, "#x").await;
        }
    }

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    assert_eq!(order.tests.len(), 6, "all six cases must be listed");
    for c in &made {
        assert!(
            order.tests.iter().any(|t| t.test_case_id == c.to_string()),
            "case {c} was dropped from the ordering"
        );
    }
    let counted: usize = order.band_counts.values().sum();
    assert_eq!(counted, order.tests.len());
    assert_eq!(
        order.mandatory_floor.len(),
        3,
        "three cases failed: {:?}",
        order.mandatory_floor
    );
    store.pool.close().await;
}

/// An alternating case meets the flakiness threshold.
#[tokio::test]
async fn an_alternating_case_is_reported_flaky() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Flaky").await;
    let c = case(&store, &pid, "Flaky login").await;
    for i in 0..8 {
        let status = if i % 2 == 0 {
            TaskStatus::Failed
        } else {
            TaskStatus::Passed
        };
        run(
            &store,
            &pid,
            c,
            status,
            if status == TaskStatus::Failed {
                Some("Element not found: #flaky")
            } else {
                None
            },
            "#flaky",
        )
        .await;
    }

    let risk = present(
        reader(&store)
            .test_risk(&pid, &c.to_string())
            .await
            .expect("test_risk"),
    );
    let flaky = risk
        .flakiness_score
        .expect("8 samples can measure flakiness");
    assert!(
        flaky >= FLAKY_THRESHOLD,
        "an alternating case must meet the threshold: {flaky} vs {FLAKY_THRESHOLD}"
    );

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    let entry = order
        .tests
        .iter()
        .find(|t| t.test_case_id == c.to_string())
        .expect("the flaky case");
    assert!(
        entry.applies.contains(&PriorityBand::Flaky),
        "the flaky band must apply: {:?}",
        entry.applies
    );
    store.pool.close().await;
}

/// A failure that recurred enough to draft a bug is reported as a recurring defect,
/// reached through the real draft and cluster writers.
#[tokio::test]
async fn a_recurring_failure_is_reported_as_a_recurring_defect() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Recurring").await;
    let c = case(&store, &pid, "Always breaks").await;
    // Two identical failures is the real threshold in `record_failure`.
    failing(&store, &pid, c, 2).await;

    let drafts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bug_drafts WHERE failure_cluster_id IS NOT NULL")
            .fetch_one(&store.pool)
            .await
            .expect("count drafts");
    assert_eq!(drafts, 1, "the second occurrence must have drafted a bug");

    let risk = present(
        reader(&store)
            .test_risk(&pid, &c.to_string())
            .await
            .expect("test_risk"),
    );
    assert_eq!(risk.recurring_defects, 1);
    assert!(
        risk.signals.iter().any(|s| s.kind == "failure_recurrence"),
        "the recurrence must be a reported signal: {:?}",
        risk.signals
    );

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    assert!(
        order.tests[0]
            .applies
            .contains(&PriorityBand::RecurringDefect),
        "a recurring defect must be reported: {:?}",
        order.tests[0].applies
    );
    store.pool.close().await;
}

/// A stable, high-volume passing case must NOT be reported risky.
///
/// The other half of the over-statement problem: a test that has run often and never
/// failed is evidence of health, and treating "lots of data" as "high risk" would invert
/// the whole feature.
#[tokio::test]
async fn a_stable_passing_case_is_not_reported_risky() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Stable").await;
    let stable = case(&store, &pid, "Rock solid").await;
    for _ in 0..10 {
        run(&store, &pid, stable, TaskStatus::Passed, None, "#x").await;
    }

    let risk = present(
        reader(&store)
            .test_risk(&pid, &stable.to_string())
            .await
            .expect("test_risk"),
    );
    assert_eq!(risk.failures, 0);
    assert_eq!(risk.failure_rate, Some(0.0));
    assert_eq!(risk.recurring_defects, 0);
    assert_eq!(risk.healing_attempts, 0);

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    let entry = order
        .tests
        .iter()
        .find(|t| t.test_case_id == stable.to_string())
        .expect("the stable case");
    assert_eq!(
        entry.priority,
        PriorityBand::Routine,
        "a clean, well-exercised case is routine, not risky: {:?}",
        entry.applies
    );
    assert_eq!(entry.risk.measured_ratio, Some(0.0));
    store.pool.close().await;
}

// ------------------------------------------------------------------ change interaction

/// Naming a change promotes the cases Phase 3 recorded for it, and demotes nothing.
#[tokio::test]
async fn a_named_change_promotes_its_cases_without_demoting_others() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Change").await;
    let changed = case(&store, &pid, "In the change").await;
    let untouched = case(&store, &pid, "Not in the change").await;
    run(&store, &pid, changed, TaskStatus::Passed, None, "#x").await;
    run(&store, &pid, untouched, TaskStatus::Passed, None, "#x").await;

    // Record the impact through Phase 3's real analysis, not by hand.
    let impact = crate::change_impact::ChangeImpactReader::new(store.clone())
        .analyse(
            &pid,
            "commit-p4",
            "requirement",
            &[crate::change_impact::ChangeTarget {
                kind: "requirement".into(),
                ref_id: "REQ-P4-1".into(),
            }],
        )
        .await
        .expect("analyse");
    assert!(impact.recorded > 0, "the impact must have been recorded");

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, Some("commit-p4"))
            .await
            .expect("prioritise_tests"),
    );
    assert_eq!(order.change_ref.as_deref(), Some("commit-p4"));

    // Both cases validate REQ-P4-1, so both are in the impact set.
    for t in &order.tests {
        assert!(
            t.applies.contains(&PriorityBand::Changed),
            "both cases validate the changed requirement: {:?}",
            t.applies
        );
    }
    let _ = untouched;

    // An unknown change reference must promote nothing.
    let none = present(
        reader(&store)
            .prioritise_tests(&pid, Some("commit-that-never-happened"))
            .await
            .expect("prioritise_tests"),
    );
    assert!(
        none.tests
            .iter()
            .all(|t| !t.applies.contains(&PriorityBand::Changed)),
        "an unknown change must promote nothing"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ isolation

/// One tenant's history cannot colour another's risk — including when both hit the *same*
/// failure, so the fingerprint is byte-identical.
#[tokio::test]
async fn the_same_fingerprint_in_two_projects_does_not_bleed_risk() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "P4 Tenant A").await;
    let theirs = project(&store, "P4 Tenant B").await;

    let my_case = case(&store, &mine, "My clean case").await;
    let their_case = case(&store, &theirs, "Their broken case").await;

    // Identical reason and selector, so identical canonical fingerprint.
    failing(&store, &mine, my_case, 4).await;
    failing(&store, &theirs, their_case, 4).await;

    let their_fp: String = sqlx::query_scalar(
        "SELECT failure_fingerprint FROM failure_clusters WHERE project_id = ? LIMIT 1",
    )
    .bind(&theirs)
    .fetch_one(&store.pool)
    .await
    .expect("their fingerprint");
    let my_fp: String = sqlx::query_scalar(
        "SELECT failure_fingerprint FROM failure_clusters WHERE project_id = ? LIMIT 1",
    )
    .bind(&mine)
    .fetch_one(&store.pool)
    .await
    .expect("my fingerprint");
    assert_eq!(
        my_fp, their_fp,
        "the fixtures must share a fingerprint, or this proves nothing"
    );

    let my_order = present(
        reader(&store)
            .prioritise_tests(&mine, None)
            .await
            .expect("prioritise_tests"),
    );
    assert_eq!(my_order.tests.len(), 1, "only our own case may be listed");
    assert_eq!(my_order.tests[0].test_case_id, my_case.to_string());

    let their_order = present(
        reader(&store)
            .prioritise_tests(&theirs, None)
            .await
            .expect("prioritise_tests"),
    );
    assert_eq!(their_order.tests.len(), 1);
    assert_eq!(their_order.tests[0].test_case_id, their_case.to_string());

    // A case id from the other tenant must not resolve in ours.
    let (marker, view) = absent(
        reader(&store)
            .test_risk(&mine, &their_case.to_string())
            .await
            .expect("test_risk"),
    );
    assert_eq!(
        marker,
        Some(InsufficientData::NoMatchingHistory),
        "another tenant's case must not resolve here"
    );
    assert!(view.is_none());
    store.pool.close().await;
}

/// An empty project must not inherit the mandatory floor from a busy neighbour.
#[tokio::test]
async fn an_empty_neighbour_project_has_no_floor_and_no_risk() {
    let (store, _d) = temp_store().await;
    let busy = project(&store, "P4 Busy").await;
    let empty = project(&store, "P4 Idle").await;
    let c = case(&store, &busy, "Breaks").await;
    failing(&store, &busy, c, 3).await;

    let order = present(
        reader(&store)
            .prioritise_tests(&busy, None)
            .await
            .expect("prioritise_tests"),
    );
    assert_eq!(order.mandatory_floor.len(), 1);

    let (marker, view) = absent(
        reader(&store)
            .prioritise_tests(&empty, None)
            .await
            .expect("prioritise_tests"),
    );
    assert_eq!(marker, Some(InsufficientData::NoExecutions));
    assert!(view.is_none(), "the idle project must inherit nothing");
    store.pool.close().await;
}

// ------------------------------------------------------------------ safety floor

/// The safety floor is reported and is never removed by being low risk.
#[tokio::test]
async fn the_safety_floor_is_reported_and_cannot_be_suppressed() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Floor").await;
    // Failed once, then passed many times: statistically low risk, but it has failed.
    // Exactly the case a naive optimiser would drop.
    let once_failed = case(&store, &pid, "Failed once then passed").await;
    run(
        &store,
        &pid,
        once_failed,
        TaskStatus::Failed,
        Some("Element not found: #once"),
        "#once",
    )
    .await;
    for _ in 0..20 {
        run(&store, &pid, once_failed, TaskStatus::Passed, None, "#once").await;
    }
    let clean = case(&store, &pid, "Never failed").await;
    for _ in 0..20 {
        run(&store, &pid, clean, TaskStatus::Passed, None, "#y").await;
    }

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );

    let floor = order
        .tests
        .iter()
        .find(|t| t.test_case_id == once_failed.to_string())
        .expect("the once-failed case must be listed");
    assert_eq!(
        floor.priority,
        PriorityBand::Mandatory,
        "a case that has failed is on the floor however well it has behaved since"
    );
    assert!(
        order.mandatory_floor.contains(&once_failed.to_string()),
        "the floor must be reported: {:?}",
        order.mandatory_floor
    );

    let clean_entry = order
        .tests
        .iter()
        .find(|t| t.test_case_id == clean.to_string())
        .expect("the clean case");
    assert!(
        floor.priority.rank() < clean_entry.priority.rank(),
        "the floor case must outrank the never-failed one"
    );
    assert!(
        !order.mandatory_floor.contains(&clean.to_string()),
        "a never-failed case is not on the floor"
    );
    store.pool.close().await;
}

/// The floor a Phase 4 ordering reports must be exactly the floor Phase 3 enforces.
///
/// Two features that both claim to protect mandatory coverage must not disagree about
/// which cases that is.
#[tokio::test]
async fn phase_four_and_phase_three_agree_on_the_safety_floor() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Floor Agree").await;
    let fragile = case(&store, &pid, "Fragile").await;
    let changed = case(&store, &pid, "Changed").await;
    failing(&store, &pid, fragile, 2).await;
    run(&store, &pid, changed, TaskStatus::Passed, None, "#z").await;

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    let p4_floor: Vec<String> = order.mandatory_floor.clone();

    // Phase 3's derivation: "has recorded failure evidence".
    let floor_map = crate::change_impact::ChangeImpactReader::new(store.clone())
        .safety_floor(&pid)
        .await
        .expect("safety_floor");
    let p3_floor: Vec<String> = floor_map.keys().map(ToString::to_string).collect();

    assert_eq!(
        p4_floor, p3_floor,
        "Phase 4's floor and Phase 3's floor must be the same set"
    );
    assert!(p3_floor.contains(&fragile.to_string()));
    assert!(!p3_floor.contains(&changed.to_string()));
    store.pool.close().await;
}

/// Narrowing a real regression scope to a Phase 4 selection still keeps the floor.
///
/// The end-to-end safety guarantee: prioritisation orders, and the engine's filter is
/// what enforces retention.
#[tokio::test]
async fn narrowing_a_regression_scope_to_a_selection_still_keeps_the_floor() {
    use crate::domain::TaskDef as RTDef;
    use crate::regression::ReplayedCase;
    use crate::regression_runner::Scope;
    use std::collections::BTreeSet;

    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Narrow").await;
    let fragile = case(&store, &pid, "Fragile").await;
    let healthy = case(&store, &pid, "Healthy").await;
    failing(&store, &pid, fragile, 2).await;
    run(&store, &pid, healthy, TaskStatus::Passed, None, "#z").await;

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    // Select only the healthy case - the worst possible choice for safety.
    let selected: BTreeSet<String> = [healthy.to_string()].into_iter().collect();
    let mandatory: BTreeSet<String> = order.mandatory_floor.iter().cloned().collect();

    let task = |id: &str| RTDef {
        id: id.into(),
        name: format!("task {id}"),
        prerequisites: Vec::new(),
        simulate: Some(SimSpec {
            outcome: SimOutcome::Pass,
            delay_ms: 1,
        }),
        executor: Some("simulated".into()),
        params: serde_json::json!({}),
        test_case_id: None,
    };
    let full = Scope {
        tasks: vec![
            (
                ReplayedCase {
                    case_id: fragile.to_string(),
                    task_id: "t-f".into(),
                    title: "fragile".into(),
                    source_run_id: Uuid::new_v4().to_string(),
                },
                task("t-f"),
            ),
            (
                ReplayedCase {
                    case_id: healthy.to_string(),
                    task_id: "t-h".into(),
                    title: "healthy".into(),
                    source_run_id: Uuid::new_v4().to_string(),
                },
                task("t-h"),
            ),
        ],
        skipped: Vec::new(),
    };
    let narrowed = full.restricted_to(&selected, &mandatory);
    let kept: Vec<&str> = narrowed
        .tasks
        .iter()
        .map(|(c, _)| c.case_id.as_str())
        .collect();

    assert!(
        kept.contains(&fragile.to_string().as_str()),
        "the floor case must survive a selection that excluded it: {kept:?}"
    );
    assert!(
        narrowed
            .skipped
            .iter()
            .all(|s| s.case_id != healthy.to_string()),
        "the dropped case must be accounted for"
    );
    assert!(
        narrowed.skipped.is_empty(),
        "nothing should be dropped when both are retained or selected"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ conflicting signals

/// Conflicting evidence must produce a *lower* measurement, not a louder one.
#[tokio::test]
async fn conflicting_signals_rank_by_evidence_rather_than_by_alarm() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Conflict").await;

    let mostly_passes = case(&store, &pid, "Mostly passes").await;
    for i in 0..20 {
        run(
            &store,
            &pid,
            mostly_passes,
            if i == 0 {
                TaskStatus::Failed
            } else {
                TaskStatus::Passed
            },
            if i == 0 {
                Some("Element not found: #rare")
            } else {
                None
            },
            "#rare",
        )
        .await;
    }

    let always_fails = case(&store, &pid, "Always fails").await;
    failing(&store, &pid, always_fails, 10).await;

    let order = present(
        reader(&store)
            .prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );

    let a = order
        .tests
        .iter()
        .find(|t| t.test_case_id == always_fails.to_string())
        .expect("always-failing case");
    let b = order
        .tests
        .iter()
        .find(|t| t.test_case_id == mostly_passes.to_string())
        .expect("mostly-passing case");

    let ra = a.risk.measured_ratio.expect("a ratio");
    let rb = b.risk.measured_ratio.expect("a ratio");
    assert!(
        ra > rb,
        "the always-failing case must measure higher: {ra} vs {rb}"
    );
    // Both are on the floor - both have failed - and the ratio still separates them.
    assert!(
        a.priority.rank() <= b.priority.rank(),
        "the always-failing case must not rank below the mostly-passing one"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ shape

/// The `TestRisk` shape must expose data sufficiency rather than a bare score, and its
/// arithmetic must be internally consistent.
#[tokio::test]
async fn the_risk_shape_states_its_own_data_sufficiency() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Shape").await;
    let c = case(&store, &pid, "Shaped").await;
    failing(&store, &pid, c, 2).await;

    let risk: TestRisk = present(
        reader(&store)
            .test_risk(&pid, &c.to_string())
            .await
            .expect("test_risk"),
    );
    assert_eq!(risk.executions, 2);
    assert_eq!(risk.failures, 2);
    assert_eq!(risk.failure_rate, Some(1.0));
    assert!(
        risk.factors.iter().all(|f| !f.basis.is_empty()),
        "every factor must carry a basis, measured or not"
    );
    assert!(
        risk.window.to > risk.window.from,
        "the window must be stated so no figure reads as all-time by accident"
    );
    for f in &risk.factors {
        let expected = f.weight * f.measured.unwrap_or(0.0);
        assert!(
            (f.contribution() - expected).abs() < 1e-9,
            "factor {} contribution must be weight x measured",
            f.name
        );
    }
    let summed: f64 = risk.factors.iter().map(|f| f.contribution()).sum();
    assert!(
        (summed - risk.weighted_contribution).abs() < 1e-9,
        "weighted_contribution must be the sum of the measured factors"
    );
    store.pool.close().await;
}

/// A healing proposal whose verification recorded a regression makes
/// `healing_regression_rate` measurable — a factor module and release risk never have.
#[tokio::test]
async fn a_healing_regression_makes_its_factor_measurable() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Healing").await;
    let c = case(&store, &pid, "Healable").await;
    failing(&store, &pid, c, 2).await;

    let fingerprint: String = sqlx::query_scalar(
        "SELECT failure_fingerprint FROM failure_clusters WHERE project_id = ? LIMIT 1",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("fingerprint");

    let proposal_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let task_id = "task-heal";
    let plan = RunPlan {
        name: "phase 4 healing".into(),
        tasks: vec![TaskDef {
            id: task_id.into(),
            name: "Healable".into(),
            prerequisites: Vec::new(),
            simulate: Some(SimSpec {
                outcome: SimOutcome::Pass,
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            params: serde_json::json!({}),
            test_case_id: Some(c.to_string()),
        }],
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(&pid),
            Some("phase 4 fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");

    let verification = serde_json::to_string(&HealingVerification {
        proposal_id,
        original_run_id: run_id,
        original_task_id: task_id.into(),
        healing_run_id: Uuid::new_v4(),
        healing_task_id: format!("heal-{task_id}"),
        outcome: VerificationOutcome::RegressionIntroduced,
        original_failure_resolved: false,
        revised_task_passed: true,
        new_failures_introduced: 1,
        regression_detected: true,
        evidence_refs: vec![],
        message: "phase 4 fixture".into(),
        verified_at: chrono::Utc::now(),
    })
    .expect("serialize");

    store
        .insert_healing_proposal(&HealingProposalRow {
            id: proposal_id,
            project_id: pid.clone(),
            run_id,
            task_id: task_id.into(),
            failure_fingerprint: fingerprint,
            diagnosis_category: FailureCategory::AssertionFailure.as_str().to_string(),
            confidence: 0.9,
            risk_level: "Medium".into(),
            requires_approval: false,
            original_actions: "[]".into(),
            proposed_actions: "[]".into(),
            ast_diff: "{}".into(),
            rationale: "phase 4 fixture".into(),
            evidence_refs: "[]".into(),
            state: "APPROVED".into(),
            idempotency_key: format!("heal:{run_id}:{task_id}"),
            healing_run_id: None,
            verification_result: Some(verification),
            approved_by: Some("operator".into()),
            approved_at: Some(chrono::Utc::now()),
            rejection_reason: None,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            context: None,
        })
        .await
        .expect("insert proposal");

    let risk = present(
        reader(&store)
            .test_risk(&pid, &c.to_string())
            .await
            .expect("test_risk"),
    );
    assert_eq!(risk.healing_attempts, 1);
    assert_eq!(risk.healing_regressions, 1);
    let f = risk
        .factors
        .iter()
        .find(|f| f.name == "healing_regression_rate")
        .expect("the factor must be present");
    assert_eq!(
        f.measured,
        Some(1.0),
        "one regression out of one attempt is 1.0"
    );
    assert_eq!(f.evidence, 1);
    // With this factor measured, more of the model is backed than for a module.
    assert!(
        risk.measured_weight > 0.70,
        "the healing factor should push measured weight past the module ceiling: {}",
        risk.measured_weight
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ performance

/// Measures prioritisation on a realistic dataset.
///
/// **Deliberately `#[ignore]`d and deliberately not asserting a duration.** Pinning a
/// wall-clock number in CI would make the suite fail on a slow machine or a busy CI
/// runner, and a threshold nobody chose from evidence is worse than no threshold. The
/// numbers are reported in `PHASE_4_RISK_BASED_TESTING_REPORT.md`; this test exists so
/// the measurement can be reproduced on demand:
///
/// ```text
/// cargo test phase4_measures_prioritisation_cost -- --ignored --nocapture
/// ```
///
/// The shape it builds is the one that matters for the scan question: many cases, each
/// with several executions, so the per-case multiplier is what shows up.
#[tokio::test]
#[ignore = "measurement, not a gate; see the module comment"]
async fn phase4_measures_prioritisation_cost() {
    use std::time::Instant;

    let (store, _d) = temp_store().await;
    let pid = project(&store, "P4 Perf").await;

    const CASES: u32 = 200;
    const EXECS_PER_CASE: u32 = 5;

    let build = Instant::now();
    for i in 0..CASES {
        let c = case(&store, &pid, &format!("Perf case {i}")).await;
        for e in 0..EXECS_PER_CASE {
            let failing = (i + e) % 3 == 0;
            run(
                &store,
                &pid,
                c,
                if failing {
                    TaskStatus::Failed
                } else {
                    TaskStatus::Passed
                },
                if failing {
                    Some("Element not found: #perf")
                } else {
                    None
                },
                "#perf",
            )
            .await;
        }
    }
    let build_ms = build.elapsed().as_millis();

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_telemetry WHERE project_id = ?")
            .bind(&pid)
            .fetch_one(&store.pool)
            .await
            .expect("row count");

    let r = QaMemoryReader::new(store.clone());
    let started = Instant::now();
    let order = present(
        r.prioritise_tests(&pid, None)
            .await
            .expect("prioritise_tests"),
    );
    let elapsed = started.elapsed();

    let failures: usize = order.tests.iter().filter(|t| t.risk.failures > 0).count();
    println!(
        "  dataset : {CASES} cases, {EXECS_PER_CASE} executions each, {rows} telemetry rows \
         (built in {build_ms} ms)",
    );
    println!("  ordered : {} cases in {:?}", order.tests.len(), elapsed);
    println!(
        "  floor   : {} cases; {failures} have recorded failures",
        order.mandatory_floor.len(),
    );
    println!(
        "  per case: {:.2} ms, {:.3} ms per telemetry row",
        elapsed.as_secs_f64() * 1000.0 / order.tests.len() as f64,
        elapsed.as_secs_f64() * 1000.0 / rows as f64,
    );

    assert_eq!(
        order.tests.len(),
        CASES as usize,
        "every case must be ordered"
    );

    // The comparison that shows what the shared read bought. Looping the single-case
    // path is exactly the implementation Phase 4 replaced: one bounded telemetry scan,
    // one healing-proposal read, one bug-draft read and one cluster lookup *per case*.
    // The per-case path, measured for real: each call re-reads telemetry, healing
    // proposals and bug drafts for one case. This is what `prioritise_tests` replaced.
    let per_case = Instant::now();
    for t in &order.tests {
        let _ = r.test_risk(&pid, &t.test_case_id).await.expect("test_risk");
    }
    let per_case_elapsed = per_case.elapsed();
    println!(
        "  comparison: {} single-case reads took {:?}; one whole-project order took {:?} \
         ({:.1}x)",
        order.tests.len(),
        per_case_elapsed,
        elapsed,
        per_case_elapsed.as_secs_f64() / elapsed.as_secs_f64().max(f64::EPSILON)
    );
    assert!(
        elapsed.as_millis() < 5_000,
        "one ordering must not cost more than a few seconds for 200 cases"
    );
    store.pool.close().await;
}

/// A storage failure during prioritisation is reported, never turned into `NO_EXECUTIONS`.
///
/// ## What this test asserted before, and why it changed
///
/// Its previous version asserted the opposite. When `QaMemoryReader::telemetry_for` and
/// `test_evidence` returned plain `Vec`s and swallowed their reads with `unwrap_or_default()`, a
/// closed pool produced an empty evidence set, `prioritise_tests` hit its "is there any history?"
/// guard against evidence that had **failed to load**, and returned `Ok(NO_EXECUTIONS)` for a
/// project that had executions.
///
/// That version was a characterisation test, written deliberately to record the gap rather than
/// hide it, and it said so: *"When R1b is addressed, this test is the place to strengthen into an
/// end-to-end failure assertion."* R1b is now addressed, so this is that assertion.
///
/// ## The three states, all distinguished
///
/// | State | Result |
/// |---|---|
/// | healthy store, no history | `Ok(NO_EXECUTIONS)` — a true answer |
/// | healthy store, with history | `Ok(view)` — the ranking |
/// | **closed store** | **`Err`** — never `NO_EXECUTIONS` |
///
/// `store.pool.close()` is a real read failure, the same technique `engine_tests` uses. No mock,
/// no sleep, and the healthy paths are asserted first so the failure cannot pass for the wrong
/// reason.
#[tokio::test]
async fn a_storage_failure_is_reported_rather_than_no_executions() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P61b StorageFail").await;
    let c = case(&store, &pid, "Checkout").await;
    run(&store, &pid, c, TaskStatus::Passed, None, "#pay").await;

    // --- state 2: healthy store, real history ---
    let healthy = reader(&store)
        .prioritise_tests(&pid, Some("commit-p4"))
        .await
        .expect("a healthy read must succeed");
    assert!(
        healthy.insufficient_data.is_none(),
        "with history recorded the reader must produce an ordering: {healthy:?}"
    );
    let order = healthy.view.expect("a view when history exists");
    assert_eq!(
        order.change_ref.as_deref(),
        Some("commit-p4"),
        "the declared change is carried into the answer"
    );

    // --- state 1: healthy store, genuinely no history ---
    let empty_pid = project(&store, "P61b NoHistory").await;
    let empty = reader(&store)
        .prioritise_tests(&empty_pid, None)
        .await
        .expect("an empty project is not a failure");
    assert_eq!(
        empty.insufficient_data,
        Some(crate::qa_memory::InsufficientData::NoExecutions),
        "a project with no history still reports NO_EXECUTIONS -- that behaviour is preserved"
    );

    // --- state 3: the store cannot answer ---
    store.pool.close().await;

    let res = reader(&store)
        .prioritise_tests(&pid, Some("commit-p4"))
        .await;
    assert!(
        res.is_err(),
        "a closed pool must be an error, not NO_EXECUTIONS for a project that has history: {:?}",
        res.as_ref().map(|r| r.insufficient_data)
    );
    store.pool.close().await;
}

/// The reader must still work normally after the failure path is proven — a `Result` that is
/// always `Err` would satisfy the test above.
#[tokio::test]
async fn a_healthy_change_path_still_reports_its_ordinary_answers() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P61 ChangeHealthy").await;

    let res = reader(&store)
        .prioritise_tests(&pid, Some("commit-p4"))
        .await
        .expect("a healthy read must succeed");
    assert!(
        res.insufficient_data.is_some(),
        "no telemetry means no prioritisation, which is a stated answer rather than a failure"
    );
    assert!(
        res.view.is_none(),
        "and no ordering may be invented from no history"
    );
    store.pool.close().await;
}
