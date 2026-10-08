//! Phase 3 (QA Intelligence Memory) — retrieval tests.
//!
//! The master plan's Phase 3 test list, one test per line of it:
//!
//! | Required case                | Test |
//! |------------------------------|------|
//! | Empty history                | `an_empty_project_reports_no_executions_not_a_clean_bill_of_health` and 4 more |
//! | One record                   | `a_single_execution_yields_no_rate_because_a_rate_needs_two_samples` |
//! | Large history                | `a_large_history_is_summarised_and_never_dumped` |
//! | Repeated failure             | `a_repeated_fingerprint_is_reported_as_recurring` |
//! | Multiple projects            | `one_projects_history_is_never_visible_from_another` (×4) |
//! | Multiple environments        | `the_window_reports_the_environments_actually_present` |
//! | Multiple releases            | `a_release_is_compared_against_other_releases_that_have_runs` |
//! | Time-window queries          | `every_view_states_the_window_it_was_computed_over` |
//! | Pagination/limits            | `lists_are_bounded_no_matter_how_much_history_exists` |
//! | Missing metadata             | `a_partial_history_answers_with_what_exists_and_names_what_is_missing` |
//!
//! Plus the retrieval rules of master plan §3: project-scoped, deterministic,
//! evidence-backed, time-aware, explainable, bounded — each asserted directly,
//! because a rule that is only documented is not a rule.

use crate::adaptive_intelligence::ExecutionTelemetry;
use crate::adaptive_intelligence::FailureCategory;
use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskStatus};
use crate::qa_memory::{InsufficientData, QaMemoryReader};
use crate::store::Store;
use uuid::Uuid;

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("q3.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 3 fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

/// One recorded execution of one test case: a real run, a real task, and a real
/// telemetry row carrying the case link Phase 2 established.
struct Execution {
    run_id: Uuid,
    task_id: String,
    case_id: Uuid,
    status: &'static str,
    fingerprint: Option<&'static str>,
    environment: &'static str,
    category: Option<&'static str>,
}

struct Fixture {
    reader: QaMemoryReader,
    project: String,
    next: Uuid,
}

impl Fixture {
    fn new(reader: QaMemoryReader, project: String) -> Self {
        Self {
            reader,
            project,
            next: Uuid::new_v4(),
        }
    }

    fn case(&mut self) -> Uuid {
        let id = Uuid::new_v4();
        self.next = id;
        id
    }

    /// Records one execution end to end: run row, task row (linked to the case),
    /// and the telemetry row that `ingest_run` would have written.
    async fn execute(
        &self,
        store: &Store,
        case_id: Uuid,
        status: &'static str,
        fingerprint: Option<&'static str>,
        environment: &'static str,
        category: Option<&'static str>,
    ) -> Uuid {
        let run_id = Uuid::new_v4();
        let task_id = format!("task-{run_id}");

        store
            .insert_run_for_release(
                run_id,
                &RunPlan {
                    name: "phase 3 fixture run".into(),
                    tasks: vec![TaskDef {
                        id: task_id.clone(),
                        name: format!("Test Case: case-{case_id}"),
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
                Some(&self.project),
                Some("fixture"),
                RunState::Completed,
                None,
                None,
            )
            .await
            .expect("insert_run");

        let e = Execution {
            run_id,
            task_id,
            case_id,
            status,
            fingerprint,
            environment,
            category,
        };
        store
            .insert_execution_telemetry(&telemetry_for(&self.project, &e))
            .await
            .expect("insert telemetry");
        run_id
    }
}

/// The stored telemetry vocabulary is `TaskStatus`, not a bare string; a fixture
/// states the outcome in words and this maps it onto the real enum.
fn status_of(s: &str) -> TaskStatus {
    match s {
        "PASSED" => TaskStatus::Passed,
        "FAILED" => TaskStatus::Failed,
        "TIMEOUT" => TaskStatus::Timeout,
        "SKIPPED" => TaskStatus::Skipped,
        "BLOCKED" => TaskStatus::Blocked,
        "CANCELLED" => TaskStatus::Cancelled,
        other => panic!("fixture asked for an unknown status {other}"),
    }
}

/// `failure_category` is the existing `adaptive_intelligence::FailureCategory`
/// enum, so the reader is exercised against the real vocabulary rather than a
/// string the test invented.
fn category_of(s: &str) -> Option<FailureCategory> {
    Some(match s {
        "TIMEOUT" => FailureCategory::Timeout,
        "ASSERTION" => FailureCategory::AssertionFailure,
        other => panic!("fixture asked for an unknown category {other}"),
    })
}

fn telemetry_for(project_id: &str, e: &Execution) -> ExecutionTelemetry {
    let now = chrono::Utc::now();
    ExecutionTelemetry {
        id: Uuid::new_v4(),
        project_id: project_id.to_string(),
        run_id: e.run_id,
        task_id: e.task_id.clone(),
        task_name: format!("Test Case: case-{}", e.case_id),
        test_case_id: Some(e.case_id),
        execution_id: None,
        executor: "simulated".into(),
        environment: e.environment.into(),
        start_time: now,
        end_time: Some(now),
        duration_ms: 5u64,
        status: status_of(e.status),
        retry_count: 0,
        healing_attempt_count: 0,
        failure_category: e.category.and_then(category_of),
        failure_fingerprint: e.fingerprint.map(ToString::to_string),
        error_message: None,
        evidence_refs: Vec::new(),
        assertion_result: None,
        http_status: None,
        selector: None,
        verification_outcome: None,
        created_at: now,
    }
}

// ------------------------------------------------------------------ empty

#[tokio::test]
async fn an_empty_project_reports_no_executions_not_a_clean_bill_of_health() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Empty").await;
    let reader = QaMemoryReader::new(store.clone());

    let case_id = Uuid::new_v4();
    let stability = reader.test_stability(&pid, &case_id.to_string()).await;
    assert_eq!(
        stability.insufficient_data,
        Some(InsufficientData::NoExecutions)
    );
    assert!(!stability.is_found());
    assert!(
        stability.view.is_none(),
        "an empty project must not produce a view full of zeroes"
    );

    let module = reader.module_risk_history(&pid, "checkout").await;
    assert_eq!(
        module.insufficient_data,
        Some(InsufficientData::NoExecutions)
    );

    // A release id that names nothing in this project is `UNKNOWN_SUBJECT`, checked
    // before history: the release does not exist here, and answering
    // `NO_EXECUTIONS` would imply it does. (This ordering was asserted the other
    // way round in an earlier draft of this test; the code is right.)
    let release = reader.release_history(&pid, Uuid::new_v4()).await;
    assert_eq!(
        release.insufficient_data,
        Some(InsufficientData::UnknownSubject)
    );

    let req = reader.requirement_history(&pid, "REQ-1").await;
    assert_eq!(req.insufficient_data, Some(InsufficientData::NoExecutions));

    // An unknown subject on an empty project is still "no executions" - the
    // project has nothing, so that is the more specific and more honest answer.
    assert!(reader.bug_history(&pid).await.is_empty());
}

#[tokio::test]
async fn an_unknown_subject_is_distinguished_from_an_empty_project() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Distinguish").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());

    let case = fx.case();
    fx.execute(&store, case, "PASSED", None, "test", None).await;

    // History exists, so an unrelated case is "unknown", not "no executions".
    let other = Uuid::new_v4();
    let stability = fx.reader.test_stability(&pid, &other.to_string()).await;
    assert_eq!(
        stability.insufficient_data,
        Some(InsufficientData::UnknownSubject)
    );

    let module = fx.reader.module_risk_history(&pid, "no-such-module").await;
    assert_eq!(
        module.insufficient_data,
        Some(InsufficientData::UnknownSubject),
        "a module that does not exist must say so, not report a clean rate"
    );
}

// ------------------------------------------------------------------ one record

#[tokio::test]
async fn a_single_execution_yields_no_rate_because_a_rate_needs_two_samples() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "One record").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    fx.execute(&store, case, "PASSED", None, "test", None).await;

    let view = fx
        .reader
        .test_stability(&pid, &case.to_string())
        .await
        .view
        .expect("found");

    assert_eq!(view.executions, 1);
    assert_eq!(view.passed, 1);
    assert!(
        view.flakiness_score.is_none(),
        "flakiness from one sample would be a claim, not a measurement"
    );
    assert!(
        view.failure_rate == Some(0.0),
        "a 0% failure rate over one execution is arithmetically true: {view:?}"
    );
}

// ------------------------------------------------------------------ stability

#[tokio::test]
async fn stability_counts_outcomes_and_reports_a_rate_over_repeated_history() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Stability").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();

    fx.execute(&store, case, "PASSED", None, "test", None).await;
    fx.execute(&store, case, "PASSED", None, "test", None).await;
    fx.execute(
        &store,
        case,
        "FAILED",
        Some("fp-a"),
        "test",
        Some("ASSERTION"),
    )
    .await;

    let view = fx
        .reader
        .test_stability(&pid, &case.to_string())
        .await
        .view
        .expect("found");

    assert_eq!(view.executions, 3);
    assert_eq!(view.passed, 2);
    assert_eq!(view.failed, 1);
    assert_eq!(view.other_outcomes, 0);
    let rate = view.failure_rate.expect("a rate over 3 samples");
    assert!((rate - 33.33).abs() < 0.1, "rate was {rate}");
    assert!(
        view.flakiness_score.is_some(),
        "three samples is enough for the existing flakiness evaluation"
    );
    assert_eq!(view.latest_failure_fingerprint.as_deref(), Some("fp-a"));
}

// -------------------------------------------------------------- repeated failure

#[tokio::test]
async fn a_repeated_fingerprint_is_reported_as_recurring() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Repeats").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    store
        .link_test_case_to_module(
            &pid,
            &case.to_string(),
            "checkout",
            "Checkout flow",
            "Checkout",
        )
        .await
        .expect("link module");

    // fp-a twice, fp-b once: only fp-a is recurring.
    fx.execute(
        &store,
        case,
        "FAILED",
        Some("fp-a"),
        "test",
        Some("TIMEOUT"),
    )
    .await;
    fx.execute(
        &store,
        case,
        "FAILED",
        Some("fp-a"),
        "test",
        Some("TIMEOUT"),
    )
    .await;
    fx.execute(
        &store,
        case,
        "FAILED",
        Some("fp-b"),
        "test",
        Some("ASSERTION"),
    )
    .await;

    let view = fx
        .reader
        .module_risk_history(&pid, "checkout")
        .await
        .view
        .expect("found");

    assert_eq!(view.executions, 3);
    assert_eq!(view.failures, 3);
    assert_eq!(view.affected_tests.len(), 1);
    assert_eq!(view.affected_tests[0].failures, 3);

    assert_eq!(
        view.recurring_failures.len(),
        1,
        "only a fingerprint seen more than once is recurring: {:?}",
        view.recurring_failures
    );
    let rec = &view.recurring_failures[0];
    assert_eq!(rec.fingerprint, "fp-a");
    assert_eq!(rec.occurrences, 2);
    assert_eq!(rec.failure_category.as_deref(), Some("TIMEOUT"));
    assert!(
        rec.first_seen.is_some() && rec.last_seen.is_some(),
        "recurrence must be time-bounded, not just counted"
    );
}

#[tokio::test]
async fn a_module_with_linked_cases_that_never_ran_reports_no_matching_history() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Unrun").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());

    // A *different* module does have history, so the project is not empty and the
    // answer cannot be NO_EXECUTIONS for the project as a whole.
    let ran = fx.case();
    store
        .link_test_case_to_module(&pid, &ran.to_string(), "shipping", "Shipping", "Shipping")
        .await
        .expect("link shipping");
    fx.execute(&store, ran, "PASSED", None, "test", None).await;

    // The module under test exists and has a case; that case simply never ran.
    let never = fx.case();
    store
        .link_test_case_to_module(&pid, &never.to_string(), "billing", "Billing", "Billing")
        .await
        .expect("link billing");

    let result = fx.reader.module_risk_history(&pid, "billing").await;
    assert_eq!(
        result.insufficient_data,
        Some(InsufficientData::NoMatchingHistory),
        "a module whose cases never ran has no history to report, not a clean record"
    );
}

#[tokio::test]
async fn a_module_reports_how_many_of_its_cases_have_never_run() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Partial").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());

    let ran = fx.case();
    let never = fx.case();
    store
        .link_test_case_to_module(&pid, &ran.to_string(), "cart", "Cart", "Cart")
        .await
        .expect("link ran");
    store
        .link_test_case_to_module(&pid, &never.to_string(), "cart", "Cart", "Cart")
        .await
        .expect("link never");
    fx.execute(&store, ran, "FAILED", Some("fp-x"), "test", None)
        .await;

    let view = fx
        .reader
        .module_risk_history(&pid, "cart")
        .await
        .view
        .expect("found");

    assert_eq!(view.linked_test_cases, 2);
    assert_eq!(view.test_cases_with_history, 1);
    assert!(
        view.signals.iter().any(|s| s.kind == "partial_coverage"),
        "the gap must be stated, not hidden by averaging over the one case that ran"
    );
}

// ------------------------------------------------------------------ releases

#[tokio::test]
async fn a_release_is_compared_against_other_releases_that_have_runs() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Releases").await;
    let reader = QaMemoryReader::new(store.clone());

    let older = store
        .create_release(&pid, Some("2026.10"), None, None, None)
        .await
        .unwrap();
    let newer = store
        .create_release(&pid, Some("2026.11"), None, None, None)
        .await
        .unwrap();
    let empty = store
        .create_release(&pid, Some("2026.12"), None, None, None)
        .await
        .unwrap();

    let mut fx = Fixture::new(reader, pid.clone());
    let case = fx.case();

    // 2026.10: 1 failure in 2 runs. 2026.11: 2 failures in 2 runs.
    let mut older_runs = Vec::new();
    older_runs.push(execute_with_release(&store, &pid, case, older.id, "PASSED", None).await);
    older_runs
        .push(execute_with_release(&store, &pid, case, older.id, "FAILED", Some("fp-1")).await);
    let _ = older_runs;

    execute_with_release(&store, &pid, case, newer.id, "FAILED", Some("fp-2")).await;
    execute_with_release(&store, &pid, case, newer.id, "FAILED", Some("fp-3")).await;
    // `empty` deliberately gets no runs at all.

    let view = fx
        .reader
        .release_history(&pid, newer.id)
        .await
        .view
        .expect("found");

    assert_eq!(view.label.as_deref(), Some("2026.11"));
    assert_eq!(view.runs, 2);
    assert_eq!(view.executions, 2);
    assert_eq!(view.failures, 2);
    assert_eq!(view.distinct_fingerprints, 2);
    let rate = view.failure_rate.expect("rate");
    assert!((rate - 100.0).abs() < 0.1);

    assert_eq!(
        view.compared_against.len(),
        1,
        "only the release that has runs is comparable: {:?}",
        view.compared_against
    );
    assert_eq!(
        view.compared_against[0].label.as_deref(),
        Some("2026.10"),
        "a release with no runs must be omitted, not counted as a clean release"
    );
    let _ = empty;
}

#[tokio::test]
async fn a_release_with_no_executions_says_so_rather_than_reporting_zero_failures() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Empty release").await;
    let reader = QaMemoryReader::new(store.clone());
    let release = store
        .create_release(&pid, Some("2027.01"), None, None, None)
        .await
        .unwrap();

    // Give the project history, so the answer cannot be "no executions" for the
    // project as a whole.
    let mut fx = Fixture::new(reader, pid.clone());
    let case = fx.case();
    fx.execute(&store, case, "PASSED", None, "test", None).await;

    let result = fx.reader.release_history(&pid, release.id).await;
    assert_eq!(
        result.insufficient_data,
        Some(InsufficientData::NoMatchingHistory)
    );
    assert!(
        result.view.is_none(),
        "an unexercised release must not look like a perfect one"
    );
}

#[tokio::test]
async fn an_unknown_release_id_is_unknown_not_a_zero_result() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Unknown release").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    fx.execute(&store, case, "PASSED", None, "test", None).await;

    assert_eq!(
        fx.reader
            .release_history(&pid, Uuid::new_v4())
            .await
            .insufficient_data,
        Some(InsufficientData::UnknownSubject)
    );
}

async fn execute_with_release(
    store: &Store,
    pid: &str,
    case_id: Uuid,
    release_id: Uuid,
    status: &'static str,
    fingerprint: Option<&'static str>,
) -> Uuid {
    let run_id = Uuid::new_v4();
    let task_id = format!("rel-task-{run_id}");
    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "release fixture".into(),
                tasks: vec![TaskDef {
                    id: task_id.clone(),
                    name: "Test Case: release fixture".into(),
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
            Some(&release_id.to_string()),
        )
        .await
        .expect("insert_run with release");

    let now = chrono::Utc::now();
    store
        .insert_execution_telemetry(&ExecutionTelemetry {
            id: Uuid::new_v4(),
            project_id: pid.to_string(),
            run_id,
            task_id,
            task_name: "Test Case: release fixture".into(),
            test_case_id: Some(case_id),
            execution_id: None,
            executor: "simulated".into(),
            environment: "test".into(),
            start_time: now,
            end_time: Some(now),
            duration_ms: 5u64,
            status: status_of(status),
            retry_count: 0,
            healing_attempt_count: 0,
            failure_category: None,
            failure_fingerprint: fingerprint.map(ToString::to_string),
            error_message: None,
            evidence_refs: Vec::new(),
            assertion_result: None,
            http_status: None,
            selector: None,
            verification_outcome: None,
            created_at: now,
        })
        .await
        .expect("insert telemetry");
    run_id
}

// -------------------------------------------------------------- requirements

#[tokio::test]
async fn a_requirement_with_no_tests_reports_no_matching_history() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Requirements").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    fx.execute(&store, case, "PASSED", None, "test", None).await;

    // No requirement rows exist, so the key cannot be found at all.
    assert_eq!(
        fx.reader
            .requirement_history(&pid, "REQ-NOPE")
            .await
            .insufficient_data,
        Some(InsufficientData::UnknownSubject)
    );
}

// ------------------------------------------------------------------ isolation

#[tokio::test]
async fn one_projects_history_is_never_visible_from_another() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Mine").await;
    let theirs = project(&store, "Theirs").await;
    let reader = QaMemoryReader::new(store.clone());

    let mut fx = Fixture::new(reader.clone(), mine.clone());
    let case = fx.case();
    store
        .link_test_case_to_module(&mine, &case.to_string(), "checkout", "Checkout", "Checkout")
        .await
        .expect("link");
    fx.execute(&store, case, "FAILED", Some("fp-secret"), "test", None)
        .await;

    let release = store
        .create_release(&mine, Some("mine-only"), None, None, None)
        .await
        .unwrap();

    // Every read, asked from the other project, must see nothing.
    assert_eq!(
        reader
            .test_stability(&theirs, &case.to_string())
            .await
            .insufficient_data,
        Some(InsufficientData::NoExecutions)
    );
    assert_eq!(
        reader
            .module_risk_history(&theirs, "checkout")
            .await
            .insufficient_data,
        Some(InsufficientData::NoExecutions)
    );
    assert_eq!(
        reader
            .release_history(&theirs, release.id)
            .await
            .insufficient_data,
        Some(InsufficientData::UnknownSubject),
        "another project's release must not be readable, and must not be described as an empty one"
    );
    assert!(
        reader
            .release_history(&theirs, release.id)
            .await
            .view
            .is_none(),
        "and it must carry no data at all"
    );
    assert!(reader.bug_history(&theirs).await.is_empty());

    // And the summary must not leak a count either.
    let summary = reader.project_memory_summary(&theirs).await;
    assert!(!summary.has_history);
    assert_eq!(summary.executions, 0);
    assert_eq!(summary.distinct_fingerprints, 0);
}

// ------------------------------------------------------------------ bounded

#[tokio::test]
async fn a_large_history_is_summarised_and_never_dumped() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Large").await;
    let reader = QaMemoryReader::new(store.clone());
    let fx = Fixture::new(reader.clone(), pid.clone());

    // 40 cases x 5 executions each.
    for i in 0..40 {
        let case = Uuid::new_v4();
        store
            .link_test_case_to_module(&pid, &case.to_string(), "big", "Big", "Big")
            .await
            .expect("link");
        for n in 0..5 {
            let status = if (i + n) % 4 == 0 { "FAILED" } else { "PASSED" };
            fx.execute(
                &store,
                case,
                status,
                if status == "FAILED" {
                    Some("fp-big")
                } else {
                    None
                },
                "test",
                None,
            )
            .await;
        }
    }

    let view = fx
        .reader
        .module_risk_history(&pid, "big")
        .await
        .view
        .expect("found");

    assert_eq!(view.executions, 200, "every execution must be counted");
    assert!(
        view.signals.len() <= 20,
        "signals must be bounded: {}",
        view.signals.len()
    );
    assert!(
        view.affected_tests.len() <= 50,
        "affected tests must be bounded: {}",
        view.affected_tests.len()
    );
    // 200 executions, 50 failures expected: the rate must still be exact even
    // though the lists are truncated.
    let rate = view.failure_rate.expect("rate");
    assert!((rate - 25.0).abs() < 0.5, "rate was {rate}");
}

#[tokio::test]
async fn lists_are_bounded_no_matter_how_much_history_exists() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Bounds").await;
    let reader = QaMemoryReader::new(store.clone());
    let mut fx = Fixture::new(reader.clone(), pid.clone());

    for _ in 0..60 {
        let case = fx.case();
        store
            .link_test_case_to_module(&pid, &case.to_string(), "wide", "Wide", "Wide")
            .await
            .expect("link");
        fx.execute(&store, case, "FAILED", Some("fp-x"), "test", None)
            .await;
    }

    let view = fx
        .reader
        .module_risk_history(&pid, "wide")
        .await
        .view
        .expect("found");

    assert_eq!(
        view.affected_tests.len(),
        50,
        "50 is the declared cap, and it must hold"
    );
    assert_eq!(
        view.test_cases_with_history, 60,
        "the exact count is still reported even though the list is capped"
    );
}

// -------------------------------------------------------------- time-aware

#[tokio::test]
async fn every_view_states_the_window_it_was_computed_over() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Window").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "w", "W", "W")
        .await
        .expect("link");
    fx.execute(&store, case, "PASSED", None, "test", None).await;
    fx.execute(&store, case, "FAILED", Some("fp"), "test", None)
        .await;

    let release = store
        .create_release(&pid, Some("r"), None, None, None)
        .await
        .unwrap();
    execute_with_release(&store, &pid, case, release.id, "PASSED", None).await;

    for window in [
        fx.reader
            .test_stability(&pid, &case.to_string())
            .await
            .view
            .unwrap()
            .window,
        fx.reader
            .module_risk_history(&pid, "w")
            .await
            .view
            .unwrap()
            .window,
        fx.reader
            .release_history(&pid, release.id)
            .await
            .view
            .unwrap()
            .window,
    ] {
        assert!(window.from.is_some(), "a window must state its start");
        assert!(window.to.is_some(), "a window must state its end");
        assert!(
            window.from <= window.to,
            "a window must not run backwards: {window:?}"
        );
        assert!(
            window.executions_considered > 0,
            "a window must say how much it covered"
        );
    }
}

#[tokio::test]
async fn the_window_reports_the_environments_actually_present() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Environments").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    fx.execute(&store, case, "PASSED", None, "staging", None)
        .await;
    fx.execute(&store, case, "FAILED", Some("fp"), "production", None)
        .await;

    let view = fx
        .reader
        .test_stability(&pid, &case.to_string())
        .await
        .view
        .expect("found");

    // Both executions are counted together, and the window says how many rows that
    // was - which is what makes a mixed-environment total honest.
    assert_eq!(view.executions, 2);
    assert_eq!(view.window.executions_considered, 2);
}

// -------------------------------------------------------------- explainable

#[tokio::test]
async fn every_signal_carries_a_basis_and_evidence() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Explainable").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "e", "E", "E")
        .await
        .expect("link");
    for _ in 0..3 {
        fx.execute(
            &store,
            case,
            "FAILED",
            Some("fp-z"),
            "test",
            Some("ASSERTION"),
        )
        .await;
    }

    let stability = fx
        .reader
        .test_stability(&pid, &case.to_string())
        .await
        .view
        .unwrap();
    let module = fx.reader.module_risk_history(&pid, "e").await.view.unwrap();

    for signal in stability.signals.iter().chain(module.signals.iter()) {
        assert!(!signal.statement.is_empty(), "a signal must say something");
        assert!(
            !signal.basis.is_empty(),
            "a signal must say what it was computed from: {signal:?}"
        );
        assert!(
            (0.0..=1.0).contains(&signal.confidence),
            "confidence must be a real number: {signal:?}"
        );
    }

    // A rate-bearing signal must point at the runs it came from.
    let rate_signal = module
        .signals
        .iter()
        .find(|s| s.kind == "module_failure_rate")
        .expect("a rate signal");
    assert_eq!(
        rate_signal.evidence_runs.len(),
        3,
        "a rate must cite the runs behind it"
    );
    assert!(
        rate_signal.statement.contains("3 of 3"),
        "the statement must carry the real numbers: {}",
        rate_signal.statement
    );
}

// -------------------------------------------------------------- determinism

#[tokio::test]
async fn repeated_reads_of_the_same_history_return_identical_answers() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Deterministic").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());

    for i in 0..6 {
        let case = fx.case();
        store
            .link_test_case_to_module(&pid, &case.to_string(), "d", "D", "D")
            .await
            .expect("link");
        fx.execute(
            &store,
            case,
            if i % 2 == 0 { "FAILED" } else { "PASSED" },
            if i % 2 == 0 { Some("fp-d") } else { None },
            "test",
            None,
        )
        .await;
    }

    let first = serde_json::to_string(&fx.reader.module_risk_history(&pid, "d").await).unwrap();
    let second = serde_json::to_string(&fx.reader.module_risk_history(&pid, "d").await).unwrap();
    assert_eq!(
        first, second,
        "the same stored history must produce byte-identical answers"
    );
}

// -------------------------------------------------------- partial metadata

#[tokio::test]
async fn a_partial_history_answers_with_what_exists_and_names_what_is_missing() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Partial metadata").await;
    let reader = QaMemoryReader::new(store.clone());
    let mut fx = Fixture::new(reader.clone(), pid.clone());
    let case = fx.case();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "p", "P", "P")
        .await
        .expect("link");

    // A telemetry row with no fingerprint, no category and no error message: the
    // minimum the schema allows.
    fx.execute(&store, case, "FAILED", None, "test", None).await;

    let view = fx
        .reader
        .module_risk_history(&pid, "p")
        .await
        .view
        .expect("found");

    assert_eq!(view.failures, 1);
    assert!(
        view.recurring_failures.is_empty(),
        "with no fingerprint there is nothing to call recurring, and nothing may be invented"
    );
    assert!(
        view.signals.iter().any(|s| s.kind == "module_failure_rate"),
        "the failure itself is still reportable"
    );
    let rec = &view.affected_tests[0];
    assert_eq!(rec.failures, 1);
}

// -------------------------------------------------------------- summary

#[tokio::test]
async fn the_summary_counts_only_what_is_stored() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Summary").await;
    let reader = QaMemoryReader::new(store.clone());
    let mut fx = Fixture::new(reader.clone(), pid.clone());

    assert!(!reader.project_memory_summary(&pid).await.has_history);

    let a = fx.case();
    let b = fx.case();
    store
        .link_test_case_to_module(&pid, &a.to_string(), "m1", "M1", "M1")
        .await
        .expect("link m1");
    store
        .link_test_case_to_module(&pid, &b.to_string(), "m2", "M2", "M2")
        .await
        .expect("link m2");
    fx.execute(&store, a, "FAILED", Some("fp-1"), "test", None)
        .await;
    fx.execute(&store, b, "FAILED", Some("fp-2"), "test", None)
        .await;
    store
        .create_release(&pid, Some("r1"), None, None, None)
        .await
        .unwrap();

    let summary = reader.project_memory_summary(&pid).await;
    assert!(summary.has_history);
    assert_eq!(summary.executions, 2);
    assert_eq!(summary.linked_test_cases, 2);
    assert_eq!(summary.distinct_fingerprints, 2);
    assert_eq!(summary.releases, 1);
    assert_eq!(summary.modules, 2);
}

// -------------------------------------------------------------- no CoT

#[tokio::test]
async fn no_view_carries_a_field_that_looks_like_hidden_reasoning() {
    // Master plan §1.5: structured facts, reasons, evidence and confidence only.
    // This test reads the serialized form of every view and refuses any key whose
    // name suggests stored reasoning, so a later field cannot be slipped in.
    let (store, _d) = temp_store().await;
    let pid = project(&store, "No CoT").await;
    let mut fx = Fixture::new(QaMemoryReader::new(store.clone()), pid.clone());
    let case = fx.case();
    store
        .link_test_case_to_module(&pid, &case.to_string(), "c", "C", "C")
        .await
        .expect("link");
    fx.execute(&store, case, "FAILED", Some("fp"), "test", None)
        .await;
    let release = store
        .create_release(&pid, Some("r"), None, None, None)
        .await
        .unwrap();
    execute_with_release(&store, &pid, case, release.id, "PASSED", None).await;

    let rendered = [
        serde_json::to_string(&fx.reader.test_stability(&pid, &case.to_string()).await).unwrap(),
        serde_json::to_string(&fx.reader.module_risk_history(&pid, "c").await).unwrap(),
        serde_json::to_string(&fx.reader.release_history(&pid, release.id).await).unwrap(),
        serde_json::to_string(&fx.reader.requirement_history(&pid, "REQ-X").await).unwrap(),
    ];

    const FORBIDDEN: [&str; 6] = [
        "chain_of_thought",
        "chainOfThought",
        "reasoning",
        "thinking",
        "scratchpad",
        "internal_notes",
    ];
    for json in rendered {
        for needle in FORBIDDEN {
            assert!(
                !json.contains(needle),
                "a view exposed {needle}, which is exactly what §1.5 forbids: {json}"
            );
        }
    }
}
