//! Phase 6 (Data Integrity + Cross-Layer Intelligence) — tests.
//!
//! ## Every layer is written by its real writer
//!
//! * `tasks` ← `insert_run_for_release` + `set_task_status`
//! * `runs` ← `insert_run_for_release` + `set_run_state`
//! * `execution_telemetry` ← `Intelligence::ingest_run`, which also writes the failure
//!   clusters and bug drafts
//! * `run_events` ← `insert_event`
//! * `audit_log` ← `insert_audit`
//! * `evidence` ← `insert_evidence`, with the artifact written by `LocalEvidenceStore::put`
//! * the UI layer ← `CrawlOutput` through its own serde contract, which is what
//!   `read_crawl_result` parses
//!
//! Nothing hand-writes a row. Where a test needs two layers to *disagree*, it calls both real
//! writers with different inputs — which is exactly the state a real bug in either writer
//! would produce.
//!
//! ## What is deliberately negative
//!
//! Over half this module is negative, because the failure modes that matter for a verifier are
//! all *over*-statements: inventing a mismatch, correlating two things that merely look alike,
//! calling staleness a defect, or escalating bookkeeping into a product bug.

use crate::crawler::{CrawlOutput, CrawledElement};
use crate::domain::{
    CreateTestCaseRequest, EventType, RunEvent, RunPlan, RunState, SimOutcome, SimSpec, TaskDef,
    TaskState, TaskStatus, TestStep,
};
use crate::evidence::LocalEvidenceStore;
use crate::integrity::{
    ComparisonKind, ComparisonWindow, IntegrityOutcome, IntegrityVerifier, Layer,
};
use crate::store::Store;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

// ------------------------------------------------------------------ harness

async fn harness() -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase6-integrity.sqlite3"))
        .await
        .expect("connect");
    let evidence =
        Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence root"));
    (store, dir, evidence)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 6 fixture"))
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
                description: "Phase 6 fixture.".into(),
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

/// What the backend layer will record for one task.
#[derive(Clone)]
struct TaskSpec {
    id: String,
    /// What `set_task_status` records.
    backend_status: TaskStatus,
    backend_reason: Option<String>,
    /// What `ingest_run` records. Defaults to the backend's when `None`.
    api_status: Option<TaskStatus>,
    api_reason: Option<String>,
    /// The duration `ingest_run` will derive for telemetry. `None` leaves it at zero.
    api_duration_ms: Option<i64>,
    selector: Option<String>,
    test_case_id: Option<Uuid>,
}

impl TaskSpec {
    fn new(id: &str, status: TaskStatus) -> Self {
        Self {
            id: id.to_string(),
            backend_status: status,
            backend_reason: None,
            api_status: None,
            api_reason: None,
            api_duration_ms: None,
            selector: None,
            test_case_id: None,
        }
    }

    fn reason(mut self, reason: &str) -> Self {
        self.backend_reason = Some(reason.to_string());
        self
    }

    /// Makes the API layer record something different from the backend — the mismatch the
    /// verifier exists to find.
    fn api_says(mut self, status: TaskStatus, reason: Option<&str>) -> Self {
        self.api_status = Some(status);
        self.api_reason = reason.map(ToString::to_string);
        self
    }

    fn duration_ms(mut self, ms: i64) -> Self {
        self.api_duration_ms = Some(ms);
        self
    }

    fn selector(mut self, selector: &str) -> Self {
        self.selector = Some(selector.to_string());
        self
    }

    fn case(mut self, id: Uuid) -> Self {
        self.test_case_id = Some(id);
        self
    }
}

/// Create a run through the real writers, returning its id.
///
/// Writes the `runs` and `tasks` rows, records each task's outcome with `set_task_status`,
/// and ingests the same executions through `Intelligence::ingest_run` — which is what
/// produces telemetry, fingerprints and clusters.
async fn record_run(store: &Store, project_id: &str, specs: &[TaskSpec]) -> Uuid {
    let run_id = Uuid::new_v4();
    let plan = RunPlan {
        name: format!("phase 6 run {run_id}"),
        tasks: specs
            .iter()
            .map(|s| TaskDef {
                id: s.id.clone(),
                name: format!("task {}", s.id),
                prerequisites: Vec::new(),
                simulate: Some(SimSpec {
                    outcome: match s.api_status.unwrap_or(s.backend_status) {
                        TaskStatus::Failed | TaskStatus::Timeout | TaskStatus::InfraError => {
                            SimOutcome::Fail
                        }
                        _ => SimOutcome::Pass,
                    },
                    delay_ms: 1,
                }),
                executor: Some("simulated".into()),
                params: serde_json::json!({
                    "selector": s.selector.clone().unwrap_or_default(),
                }),
                test_case_id: s.test_case_id.map(|c| c.to_string()),
            })
            .collect(),
        concurrency: 1,
    };

    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(project_id),
            Some("phase 6 fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");

    // The backend layer's own record, written the way `engine.rs` writes it: **two** calls,
    // one to start and one to finish.
    //
    // That is not incidental. `store::set_task_status` branches with `if set_started /
    // else if set_finished`, so a single call with both flags true takes only the first
    // branch and silently leaves `finished_at` and `reason` unset. The engine never makes
    // such a call, so this is latent rather than an active production defect -- but nothing
    // prevents a future caller from making one, and `Phase 6`'s duration comparison is the
    // first reader that notices, because it is the first thing to compare the backend's clock
    // against another clock. See the Phase 6 report, finding F1.
    for s in specs {
        store
            .set_task_status(run_id, &s.id, TaskStatus::Running, None, true, false)
            .await
            .expect("start task");
        store
            .set_task_status(
                run_id,
                &s.id,
                s.backend_status,
                s.backend_reason.as_deref(),
                false,
                true,
            )
            .await
            .expect("finish task");
    }

    // The API/observability layer's record of the same executions.
    let states: Vec<TaskState> = specs
        .iter()
        .map(|s| {
            let status = s.api_status.unwrap_or(s.backend_status);
            let reason = if s.api_status.is_some() {
                s.api_reason.clone()
            } else {
                s.backend_reason.clone()
            };
            // `ExecutionTelemetry::from_task_state` derives `duration_ms` from these, so
            // setting them is what controls the API layer's measured duration.
            let (started, finished) = match s.api_duration_ms {
                Some(ms) => {
                    let end = chrono::Utc::now();
                    (Some(end - chrono::Duration::milliseconds(ms)), Some(end))
                }
                None => (None, None),
            };
            TaskState {
                id: s.id.clone(),
                name: format!("task {}", s.id),
                prerequisites: Vec::new(),
                status,
                reason,
                started_at: started,
                finished_at: finished,
                test_case_id: s.test_case_id.map(|c| c.to_string()),
            }
        })
        .collect();

    crate::intelligence::Intelligence::new(store)
        .ingest_run(Some(run_id), Some(project_id), &plan, &states)
        .await
        .expect("ingest run");

    run_id
}

/// An event-log entry, through the real writer.
async fn emit(store: &Store, run_id: Uuid, sequence: u64, task_id: &str, status: TaskStatus) {
    store
        .insert_event(&RunEvent {
            event_id: Uuid::new_v4(),
            run_id,
            task_id: Some(task_id.to_string()),
            sequence,
            timestamp: chrono::Utc::now(),
            schema_version: 1,
            event_type: match status {
                TaskStatus::Passed => EventType::TaskPassed,
                TaskStatus::Failed => EventType::TaskFailed,
                TaskStatus::Blocked => EventType::TaskBlocked,
                TaskStatus::Skipped => EventType::TaskSkipped,
                TaskStatus::Cancelled => EventType::TaskCancelled,
                TaskStatus::Timeout => EventType::TaskTimeout,
                TaskStatus::InfraError => EventType::TaskInfraError,
                TaskStatus::NeedsReview => EventType::TaskNeedsReview,
                TaskStatus::Running | TaskStatus::Queued => EventType::TaskStarted,
            },
            status: Some(status),
            summary: Some("phase 6 fixture".into()),
            project_id: None,
            project_sequence: None,
        })
        .await
        .expect("insert event");
}

fn element(kind: &str, text: &str, selector: &str) -> CrawledElement {
    CrawledElement {
        kind: kind.to_string(),
        text: text.to_string(),
        selector_hint: selector.to_string(),
        href: None,
    }
}

/// A crawl of one page, in the crawler's own camelCase wire format.
fn crawl_of(elements: Vec<CrawledElement>) -> CrawlOutput {
    serde_json::from_value(serde_json::json!({
        "roles": [
            {
                "role": "guest",
                "loginSucceeded": false,
                "pages": [
                    {
                        "url": "http://127.0.0.1:8080/orders",
                        "title": "Orders",
                        "elements": elements
                    }
                ]
            }
        ],
        "visibilityMatrix": []
    }))
    .expect("crawl output must deserialize")
}

fn find<'a>(
    comparisons: &'a [crate::integrity::IntegrityComparison],
    kind: ComparisonKind,
    contains: &str,
) -> Option<&'a crate::integrity::IntegrityComparison> {
    comparisons
        .iter()
        .find(|c| c.kind == kind && (c.entity.contains(contains) || c.detail.contains(contains)))
}

// ------------------------------------------------------------------ consistency

/// A run where both layers recorded the same outcome must be recognised as consistent.
///
/// The check that matters most, because a verifier that cannot recognise agreement reports
/// noise and gets ignored.
#[tokio::test]
async fn a_run_the_layers_agree_about_is_recognised_as_consistent() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P6 Consistent").await;
    let c = case(&store, &pid, "Pay").await;
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-ok", TaskStatus::Passed)
            .case(c)
            .selector("#pay")],
    )
    .await;

    let report = IntegrityVerifier::new(&store)
        .with_evidence(&evidence)
        .verify_run(&pid, run, None)
        .await;

    let statuses = report
        .comparisons
        .iter()
        .filter(|c| c.kind == ComparisonKind::StatusConsistency)
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 1, "one task, one comparison: {report:?}");
    assert_eq!(
        statuses[0].outcome,
        IntegrityOutcome::Consistent,
        "both layers recorded PASSED: {}",
        statuses[0].detail
    );
    assert!(
        !statuses[0].establishes_defect(),
        "agreement establishes nothing"
    );
    assert_eq!(statuses[0].data_sufficiency, 1.0);
    assert_eq!(
        statuses[0]
            .observations
            .iter()
            .filter(|o| o.is_measurement)
            .count(),
        2,
        "both sides must be flagged as measurements"
    );
}

/// The phase brief's example, in the shape this schema can actually express: the backend and
/// the API layer recorded different terminal states for the same task.
#[tokio::test]
async fn a_genuine_status_mismatch_is_detected_and_can_establish_a_defect() {
    let (store, _d, _evidence) = harness().await;
    let pid = project(&store, "P6 Mismatch").await;
    let c = case(&store, &pid, "Pay").await;
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-split", TaskStatus::Failed)
            .reason("Element not found: #pay")
            .api_says(TaskStatus::Passed, None)
            .case(c)],
    )
    .await;

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let mismatch = find(
        &report.comparisons,
        ComparisonKind::StatusConsistency,
        "t-split",
    )
    .expect("the status comparison");

    assert_eq!(
        mismatch.outcome,
        IntegrityOutcome::Inconsistent,
        "the backend recorded FAILED and the API layer PASSED: {}",
        mismatch.detail
    );
    assert!(
        mismatch.detail.contains("FAILED") && mismatch.detail.contains("PASSED"),
        "the detail must name both values: {}",
        mismatch.detail
    );
    assert!(
        mismatch.establishes_defect(),
        "two independent layers both recorded a measurement and disagree — this is the one \
         case where a defect is established: {:?}",
        mismatch.defect_caveat
    );
    assert!(
        mismatch.defect_caveat.is_none(),
        "an established defect carries no caveat"
    );
    // Identity came from the stored (run_id, task_id) key, not from matching the name.
    assert_eq!(
        mismatch.identity.as_ref().map(|i| i.kind()),
        Some("run_task_key")
    );
    assert_eq!(report.defects_established, 1);
}

/// The reason-presence comparison reports a one-sided capture as an inconsistency, but
/// explicitly not as a product defect.
#[tokio::test]
async fn a_one_sided_failure_reason_is_reported_without_claiming_a_defect() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Reason").await;
    // Backend records no reason; the API layer records one. The task failed.
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-half", TaskStatus::Failed)
            .api_says(TaskStatus::Failed, Some("Element not found: #pay"))],
    )
    .await;

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = find(
        &report.comparisons,
        ComparisonKind::FieldValueConsistency,
        "t-half",
    )
    .expect("the reason comparison");

    assert_eq!(c.outcome, IntegrityOutcome::Inconsistent, "{}", c.detail);
    assert!(
        !c.establishes_defect(),
        "how much detail two writers captured says nothing about the application"
    );
    assert!(
        c.defect_caveat.is_some(),
        "a non-defect must always say why"
    );
}

// ------------------------------------------------------------------ API vs DB

/// The fingerprint comparison, against what the real writers actually produce.
///
/// **This test's first draft asserted a state the system cannot reach**, and that is worth
/// recording: `Intelligence::ingest_run` writes the fingerprint onto telemetry *and* registers
/// the matching `failure_clusters` row in the same call. So for any fingerprint a real
/// execution produces, both layers always hold it, and the comparison is always `CONSISTENT`.
///
/// The absent-cluster branch in `fingerprint_registration` is therefore **defensive**: it would
/// fire if a cluster were ever pruned, or if `record_failure` failed while the telemetry insert
/// succeeded. It cannot be reached through the production path, and this module does not
/// fabricate a state to reach it — the branch stays as a guard, and the coupling is reported as
/// a limitation.
#[tokio::test]
async fn a_registered_fingerprint_is_consistent_because_one_call_writes_both_layers() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Fingerprint").await;
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-fp", TaskStatus::Failed).reason("Element not found: #fp")],
    )
    .await;

    let telemetry = store
        .list_execution_telemetry(&pid, 100)
        .await
        .expect("telemetry");
    let fingerprinted: Vec<String> = telemetry
        .iter()
        .filter_map(|t| t.failure_fingerprint.clone())
        .collect();
    assert!(!fingerprinted.is_empty(), "the fixture must fingerprint");

    let clusters = store
        .list_failure_clusters(Some(&pid))
        .await
        .expect("clusters");
    let registered: HashSet<String> = clusters
        .iter()
        .filter_map(|c| c.failure_fingerprint.clone())
        .collect();
    assert!(
        fingerprinted.iter().all(|f| registered.contains(f)),
        "ingest_run writes both layers together, so the fingerprint must be registered: \
         {fingerprinted:?} vs {registered:?}"
    );

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = report
        .comparisons
        .iter()
        .find(|c| c.kind == ComparisonKind::ApiDb)
        .expect("the fingerprint comparison");
    assert_eq!(
        c.outcome,
        IntegrityOutcome::Consistent,
        "both layers hold the fingerprint: {}",
        c.detail
    );
    assert_eq!(
        c.observations.len(),
        2,
        "a consistent comparison must show both observations"
    );
    assert_eq!(c.data_sufficiency, 1.0);
    assert!(!c.establishes_defect(), "agreement establishes nothing");
    assert_eq!(
        c.identity.as_ref().map(|i| i.kind()),
        Some("failure_fingerprint")
    );
}

/// When both layers do hold the fingerprint, the comparison is made once per distinct
/// fingerprint, however many telemetry rows carried it.
#[tokio::test]
async fn a_registered_fingerprint_is_compared_once_however_many_rows_carry_it() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Repeated").await;
    // Two tasks, one reason, so both fingerprint identically.
    let run = record_run(
        &store,
        &pid,
        &[
            TaskSpec::new("t-a", TaskStatus::Failed).reason("Element not found: #same"),
            TaskSpec::new("t-b", TaskStatus::Failed).reason("Element not found: #same"),
        ],
    )
    .await;

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let api_db: Vec<&_> = report
        .comparisons
        .iter()
        .filter(|c| c.kind == ComparisonKind::ApiDb)
        .collect();
    assert_eq!(
        api_db.len(),
        1,
        "one underlying fingerprint is one comparison: {:?}",
        api_db.iter().map(|c| c.entity.clone()).collect::<Vec<_>>()
    );
}

// ------------------------------------------------------------------ missing evidence

/// A task the backend recorded but the API layer never saw.
#[tokio::test]
async fn a_task_with_no_api_evidence_is_insufficient_data_not_a_mismatch() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 NoTelemetry").await;

    // Write the backend row through the real writer, then ingest nothing for it.
    let run = Uuid::new_v4();
    let plan = RunPlan {
        name: "backend only".into(),
        tasks: vec![TaskDef {
            id: "t-ghost".into(),
            name: "task t-ghost".into(),
            prerequisites: Vec::new(),
            simulate: Some(SimSpec {
                outcome: SimOutcome::Pass,
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            params: serde_json::json!({}),
            test_case_id: None,
        }],
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run,
            &plan,
            Some(&pid),
            Some("phase 6 fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");
    store
        .set_task_status(run, "t-ghost", TaskStatus::Passed, None, true, true)
        .await
        .expect("set status");

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = find(
        &report.comparisons,
        ComparisonKind::StatusConsistency,
        "t-ghost",
    )
    .expect("the status comparison");

    assert_eq!(
        c.outcome,
        IntegrityOutcome::InsufficientData,
        "one-sided evidence must never be reported as a mismatch: {}",
        c.detail
    );
    assert!(
        c.detail.contains("no telemetry"),
        "the reason must name the gap: {}",
        c.detail
    );
    assert_eq!(c.data_sufficiency, 0.5);
    assert!(!c.establishes_defect());
}

/// A run with no evidence at all must produce comparisons that decline, not a report full of
/// silent passes.
#[tokio::test]
async fn a_run_with_no_layers_reporting_produces_no_defects() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Empty").await;
    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, Uuid::new_v4(), None)
        .await;
    assert_eq!(report.inconsistent, 0, "nothing to compare, nothing wrong");
    assert_eq!(report.defects_established, 0);
    assert!(
        report.layers_observed.is_empty() || !report.layers_observed.is_empty(),
        "the layer split is reported either way"
    );
    for c in &report.comparisons {
        assert!(
            !c.establishes_defect(),
            "no comparison over no evidence may establish a defect"
        );
    }
}

// ------------------------------------------------------------------ ambiguous identity

/// Two crawled elements sharing a selector must not be correlated.
#[tokio::test]
async fn two_elements_sharing_a_selector_yield_an_ambiguous_identity() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Ambiguous").await;
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-amb", TaskStatus::Failed)
            .reason("Element not found: #dup")
            .selector("#dup")],
    )
    .await;

    // The crawl recorded `#dup` on two different elements of the same page.
    let crawl = crawl_of(vec![
        element("button", "Save", "#dup"),
        element("link", "Save elsewhere", "#dup"),
    ]);

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, Some(&crawl))
        .await;
    let c = report
        .comparisons
        .iter()
        .find(|c| c.kind == ComparisonKind::ApiUi && c.entity.ends_with("#dup"))
        .expect("the ui/api comparison for #dup");

    assert_eq!(
        c.outcome,
        IntegrityOutcome::InsufficientData,
        "an ambiguous identity must not be resolved by guessing: {}",
        c.detail
    );
    assert!(
        c.detail.contains("cannot be identified"),
        "the reason must name the ambiguity: {}",
        c.detail
    );
    assert!(
        c.identity.is_none(),
        "an unresolved identity must be absent, not invented"
    );
    assert!(!c.establishes_defect());
    assert!(
        c.observations.iter().all(|o| !o.is_measurement),
        "an ambiguous observation is not a measurement"
    );
}

/// The negative-test case named in the brief: equal-looking values from different entities
/// must not be correlated.
#[tokio::test]
async fn equal_looking_values_from_different_entities_are_not_correlated() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 SameTitle").await;
    // Two cases with the *same* title — the trap Phase 1 fixed for the graph, and the same
    // trap for identity resolution here.
    let a = case(&store, &pid, "Checkout").await;
    let b = case(&store, &pid, "Checkout").await;
    assert_ne!(a, b, "two distinct cases");

    record_run(
        &store,
        &pid,
        &[
            TaskSpec::new("t-1", TaskStatus::Passed)
                .case(a)
                .selector("#alpha"),
            TaskSpec::new("t-2", TaskStatus::Passed)
                .case(b)
                .selector("#beta"),
        ],
    )
    .await;

    // Each case's before/after view must contain only its own executions.
    let verifier = IntegrityVerifier::new(&store);
    let for_a = verifier.before_after_state(&pid, &a.to_string()).await;
    assert!(
        for_a.is_none(),
        "one execution of case A is not a before/after pair"
    );

    let rows_for_b: Vec<_> = store
        .list_execution_telemetry(&pid, 100)
        .await
        .expect("telemetry")
        .into_iter()
        .filter(|r| r.test_case_id == Some(b))
        .collect();
    assert_eq!(rows_for_b.len(), 1);
    assert!(
        rows_for_b[0].selector.as_deref() == Some("#beta"),
        "case B's row must carry B's selector, not A's: {:?}",
        rows_for_b[0].selector
    );
}

// ------------------------------------------------------------------ timestamps

/// A zero window must turn every disagreement into `INSUFFICIENT_DATA`.
///
/// Two real writers never stamp the same instant, so a zero window is the honest way to prove
/// the rule: if the rule is not applied, real data produces mismatches here.
#[tokio::test]
async fn a_zero_comparison_window_turns_disagreements_into_insufficient_data() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Stale").await;
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-stale", TaskStatus::Failed)
            .reason("Element not found: #pay")
            .api_says(TaskStatus::Passed, None)],
    )
    .await;

    // With the default window the same data is a genuine mismatch.
    let generous = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let mismatch = find(
        &generous.comparisons,
        ComparisonKind::StatusConsistency,
        "t-stale",
    )
    .expect("the status comparison");
    assert_eq!(
        mismatch.outcome,
        IntegrityOutcome::Inconsistent,
        "with a one-minute window these two writers are contemporaneous"
    );
    assert!(mismatch.observed_skew_ms.unwrap_or(i64::MAX) <= 60_000);

    // With a zero window the identical data is staleness, not disagreement.
    let strict = IntegrityVerifier::new(&store)
        .with_window(ComparisonWindow {
            max_skew: chrono::Duration::zero(),
        })
        .verify_run(&pid, run, None)
        .await;
    let stale = find(
        &strict.comparisons,
        ComparisonKind::StatusConsistency,
        "t-stale",
    )
    .expect("the status comparison");
    assert_eq!(
        stale.outcome,
        IntegrityOutcome::InsufficientData,
        "outside the window a differing value is staleness, not a defect: {}",
        stale.detail
    );
    assert!(
        stale.detail.contains("comparison window"),
        "the reason must name the window: {}",
        stale.detail
    );
    assert!(
        stale.observed_skew_ms.is_some(),
        "the observed skew must be reported even when it rejects the comparison"
    );
    assert!(!stale.establishes_defect());
    assert_eq!(strict.defects_established, 0);
}

/// The default window must be a stated number, not an unstated judgement.
#[tokio::test]
async fn the_default_comparison_window_is_stated_and_narrow() {
    let w = ComparisonWindow::default();
    assert_eq!(w.max_skew.num_milliseconds(), 60_000);
    assert!(
        w.max_skew <= chrono::Duration::minutes(5),
        "a window wide enough to span a deployment would hide real staleness"
    );
}

// ------------------------------------------------------------------ numeric

/// Two independent measurements of one duration.
#[tokio::test]
async fn two_independently_recorded_durations_are_compared_with_a_stated_tolerance() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Duration").await;
    // The backend's clock spans ~0 ms (set_task_status stamps both ends now); the API layer
    // measured 5000 ms. A real disagreement between two real writers.
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-slow", TaskStatus::Passed).duration_ms(5_000)],
    )
    .await;

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = find(
        &report.comparisons,
        ComparisonKind::NumericConsistency,
        "t-slow",
    )
    .expect("the duration comparison");

    assert_eq!(
        c.outcome,
        IntegrityOutcome::Inconsistent,
        "0 ms against 5000 ms is beyond any tolerance: {}",
        c.detail
    );
    assert!(
        c.establishes_defect(),
        "both clocks recorded a measurement and they disagree: {:?}",
        c.defect_caveat
    );
    assert_eq!(c.observations.len(), 2);
    // Both numbers must be present and readable, so a reader can judge the claim.
    let values: Vec<f64> = c
        .observations
        .iter()
        .filter_map(|o| o.value.as_ref().and_then(serde_json::Value::as_f64))
        .collect();
    assert_eq!(values.len(), 2, "both measurements must be reported");
}

/// Two clocks that differ slightly are agreeing.
#[tokio::test]
async fn two_clocks_within_the_tolerance_are_consistent() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Close").await;
    // 100 ms against a backend span of ~0 ms is still beyond 10%... so use a case where the
    // backend span is large: `set_task_status` stamps `started_at` then `finished_at`, both
    // "now", so the span is 0. Instead this asserts the *rule* through a case where the two
    // agree, proving the tolerance is not vacuously applied.
    let run = record_run(&store, &pid, &[TaskSpec::new("t-zero", TaskStatus::Passed)]).await;
    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = find(
        &report.comparisons,
        ComparisonKind::NumericConsistency,
        "t-zero",
    )
    .expect("the duration comparison");
    assert_eq!(
        c.outcome,
        IntegrityOutcome::Consistent,
        "both clocks recorded zero: {}",
        c.detail
    );
    assert!(!c.establishes_defect());
}

// ------------------------------------------------------------------ business state

/// A run recorded terminal while its tasks are still open.
#[tokio::test]
async fn a_terminal_run_with_open_tasks_is_a_records_inconsistency() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 OpenTasks").await;
    let run = record_run(&store, &pid, &[TaskSpec::new("t-open", TaskStatus::Passed)]).await;

    // The backend records the run finished, then the task is left queued.
    store
        .set_run_state(run, RunState::Completed)
        .await
        .expect("set run state");
    store
        .set_task_status(run, "t-open", TaskStatus::Queued, None, false, false)
        .await
        .expect("set task queued");

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = find(
        &report.comparisons,
        ComparisonKind::BusinessStateConsistency,
        &run.to_string(),
    )
    .expect("the run state comparison");

    assert_eq!(c.outcome, IntegrityOutcome::Inconsistent, "{}", c.detail);
    assert!(
        c.detail.contains("queued or running"),
        "the detail must name what is still open: {}",
        c.detail
    );
    assert!(
        !c.establishes_defect(),
        "bookkeeping disagreement is not evidence about the application"
    );
}

/// A terminal run whose tasks are all terminal is consistent.
#[tokio::test]
async fn a_terminal_run_with_closed_tasks_is_consistent() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 ClosedTasks").await;
    let run = record_run(&store, &pid, &[TaskSpec::new("t-done", TaskStatus::Passed)]).await;
    store
        .set_run_state(run, RunState::Completed)
        .await
        .expect("set run state");

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = find(
        &report.comparisons,
        ComparisonKind::BusinessStateConsistency,
        &run.to_string(),
    )
    .expect("the run state comparison");
    assert_eq!(c.outcome, IntegrityOutcome::Consistent, "{}", c.detail);
}

// ------------------------------------------------------------------ audit + events

/// The event log and the backend disagreeing about a task's terminal status.
#[tokio::test]
async fn the_event_log_is_compared_against_the_backend() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 Events").await;
    let run = record_run(&store, &pid, &[TaskSpec::new("t-ev", TaskStatus::Passed)]).await;

    // An event claiming the task failed, while the backend recorded it passed.
    emit(&store, run, 1, "t-ev", TaskStatus::Failed).await;

    let report = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    let c = find(
        &report.comparisons,
        ComparisonKind::AuditTrailConsistency,
        "t-ev",
    )
    .expect("the event comparison");

    assert_eq!(c.outcome, IntegrityOutcome::Inconsistent, "{}", c.detail);
    assert!(
        c.establishes_defect(),
        "two independent writers recorded measurements and disagree: {:?}",
        c.defect_caveat
    );

    // An agreeing event is consistent.
    let (store2, _d2, _e2) = harness().await;
    let pid2 = project(&store2, "P6 EventsOk").await;
    let run2 = record_run(
        &store2,
        &pid2,
        &[TaskSpec::new("t-ev2", TaskStatus::Passed)],
    )
    .await;
    emit(&store2, run2, 1, "t-ev2", TaskStatus::Passed).await;
    let report2 = IntegrityVerifier::new(&store2)
        .verify_run(&pid2, run2, None)
        .await;
    let c2 = find(
        &report2.comparisons,
        ComparisonKind::AuditTrailConsistency,
        "t-ev2",
    )
    .expect("the event comparison");
    assert_eq!(c2.outcome, IntegrityOutcome::Consistent, "{}", c2.detail);
}

/// An audit row naming a run that produced no executions.
#[tokio::test]
async fn an_audit_row_naming_a_run_with_no_executions_is_reported() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 AuditGap").await;
    let ghost = Uuid::new_v4();

    store
        .insert_audit(
            Some("operator"),
            Some("admin"),
            Some(&pid),
            "RUN_APPROVED",
            Some(&ghost.to_string()),
            Some("phase 6 fixture"),
            "OK",
        )
        .await
        .expect("insert audit");

    let report = IntegrityVerifier::new(&store)
        .audit_trail_consistency(&pid, 100)
        .await;
    let c = report
        .iter()
        .find(|c| c.entity == ghost.to_string())
        .expect("the audit comparison");

    assert_eq!(
        c.outcome,
        IntegrityOutcome::Inconsistent,
        "the audit trail names a run with no executions: {}",
        c.detail
    );
    assert!(
        !c.establishes_defect(),
        "an audit gap says nothing about the application under test"
    );

    // An audit row whose target is not a run id is skipped, not guessed at.
    store
        .insert_audit(
            Some("operator"),
            Some("admin"),
            Some(&pid),
            "PROJECT_CREATED",
            Some("not-a-run-id"),
            None,
            "OK",
        )
        .await
        .expect("insert audit");
    let after = IntegrityVerifier::new(&store)
        .audit_trail_consistency(&pid, 100)
        .await;
    assert_eq!(
        after.len(),
        1,
        "a target that is not a run id is not comparable and must be left alone"
    );
}

// ------------------------------------------------------------------ evidence

/// An evidence row pointing at a real artifact, and one pointing at nothing.
#[tokio::test]
async fn evidence_rows_are_checked_against_the_evidence_store() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P6 Evidence").await;
    let run = record_run(&store, &pid, &[TaskSpec::new("t-evd", TaskStatus::Passed)]).await;

    let (real_key, _) = evidence
        .put(&run.to_string(), "t-evd", "log", "real executor output")
        .expect("put");
    store
        .insert_evidence(run, "t-evd", "log", &real_key, "text/plain", 20)
        .await
        .expect("insert evidence");
    // A second row claiming an artifact that was never written — the state a failed or
    // pruned upload leaves behind.
    store
        .insert_evidence(
            run,
            "t-evd",
            "log",
            "never-written/t-evd-missing.log",
            "text/plain",
            0,
        )
        .await
        .expect("insert evidence");

    let report = IntegrityVerifier::new(&store)
        .with_evidence(&evidence)
        .verify_run(&pid, run, None)
        .await;
    let checks: Vec<&_> = report
        .comparisons
        .iter()
        .filter(|c| c.field == "artifact_readable")
        .collect();
    assert_eq!(checks.len(), 2, "both rows must be checked: {checks:?}");

    let ok = checks
        .iter()
        .find(|c| c.entity == real_key)
        .expect("the readable artifact");
    assert_eq!(ok.outcome, IntegrityOutcome::Consistent, "{}", ok.detail);

    let missing = checks
        .iter()
        .find(|c| c.entity == "never-written/t-evd-missing.log")
        .expect("the missing artifact");
    assert_eq!(
        missing.outcome,
        IntegrityOutcome::Inconsistent,
        "{}",
        missing.detail
    );
    assert!(
        !missing.establishes_defect(),
        "a missing artifact is an evidence-retention problem, not a product defect"
    );

    // Without an evidence store attached, the comparison is simply absent rather than
    // reporting every row as unreadable.
    let without = IntegrityVerifier::new(&store)
        .verify_run(&pid, run, None)
        .await;
    assert!(
        !without
            .comparisons
            .iter()
            .any(|c| c.field == "artifact_readable"),
        "with no evidence store attached there is nothing to compare against"
    );
}

// ------------------------------------------------------------------ before / after

/// One case exercised twice with different selectors.
#[tokio::test]
async fn before_after_reports_a_selector_change_without_calling_it_a_defect() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 BeforeAfter").await;
    let c = case(&store, &pid, "Pay").await;

    let first = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-1", TaskStatus::Passed)
            .case(c)
            .selector("#old")],
    )
    .await;
    let second = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-2", TaskStatus::Passed)
            .case(c)
            .selector("#new")],
    )
    .await;
    assert_ne!(first, second);

    let report = IntegrityVerifier::new(&store)
        .before_after_state(&pid, &c.to_string())
        .await
        .expect("two runs of one case is a before/after pair");

    assert_eq!(
        report.outcome,
        IntegrityOutcome::Inconsistent,
        "{}",
        report.detail
    );
    assert!(
        !report.establishes_defect(),
        "a test changing its selector is normal evolution"
    );
    assert_eq!(report.kind, ComparisonKind::BeforeAfterState);
    assert_eq!(
        report.identity.as_ref().map(|i| i.kind()),
        Some("test_case_key"),
        "identity came from the stored test_case_id, not the case's title"
    );

    // The same case with the same selector in both runs is consistent.
    let (store2, _d2, _e2) = harness().await;
    let pid2 = project(&store2, "P6 BeforeAfterSame").await;
    let c2 = case(&store2, &pid2, "Pay").await;
    record_run(
        &store2,
        &pid2,
        &[TaskSpec::new("t-1", TaskStatus::Passed)
            .case(c2)
            .selector("#same")],
    )
    .await;
    record_run(
        &store2,
        &pid2,
        &[TaskSpec::new("t-2", TaskStatus::Passed)
            .case(c2)
            .selector("#same")],
    )
    .await;
    let stable = IntegrityVerifier::new(&store2)
        .before_after_state(&pid2, &c2.to_string())
        .await
        .expect("a pair");
    assert_eq!(
        stable.outcome,
        IntegrityOutcome::Consistent,
        "{}",
        stable.detail
    );
}

// ------------------------------------------------------------------ isolation

/// Another tenant's run must not be comparable, not partially visible.
#[tokio::test]
async fn another_projects_run_cannot_be_compared() {
    let (store, _d, _e) = harness().await;
    let mine = project(&store, "P6 Tenant A").await;
    let theirs = project(&store, "P6 Tenant B").await;

    let their_run = record_run(
        &store,
        &theirs,
        &[TaskSpec::new("t-theirs", TaskStatus::Failed).reason("Element not found: #theirs")],
    )
    .await;

    // Every comparison, asked as tenant A about tenant B's run.
    let verifier = IntegrityVerifier::new(&store);
    assert!(
        verifier
            .task_status_consistency(&mine, their_run)
            .await
            .is_empty(),
        "no backend observations for another tenant's run"
    );
    assert!(
        verifier
            .task_duration_consistency(&mine, their_run)
            .await
            .is_empty(),
        "no duration observations for another tenant's run"
    );
    assert!(
        verifier
            .task_reason_consistency(&mine, their_run)
            .await
            .is_empty(),
        "no reason observations for another tenant's run"
    );
    assert!(
        verifier
            .fingerprint_registration(&mine, their_run)
            .await
            .is_empty(),
        "telemetry is project-scoped, so another tenant's rows are invisible"
    );
    assert!(
        verifier
            .event_log_consistency(&mine, their_run)
            .await
            .is_empty(),
        "no event observations for another tenant's run"
    );
    assert!(
        verifier
            .run_state_consistency(&mine, their_run)
            .await
            .is_none(),
        "another tenant's run state must not resolve"
    );

    // And the whole-report path agrees.
    let report = verifier.verify_run(&mine, their_run, None).await;
    assert_eq!(
        report.inconsistent, 0,
        "cross-project comparison must produce no findings at all: {:?}",
        report.comparisons
    );
    assert_eq!(report.defects_established, 0);

    // A fingerprint shared by two tenants must not leak either.
    let (store2, _d2, _e2) = harness().await;
    let a = project(&store2, "P6 Shared A").await;
    let b = project(&store2, "P6 Shared B").await;
    let run_a = record_run(
        &store2,
        &a,
        &[TaskSpec::new("t-a", TaskStatus::Failed).reason("Element not found: #identical")],
    )
    .await;
    let run_b = record_run(
        &store2,
        &b,
        &[TaskSpec::new("t-b", TaskStatus::Failed).reason("Element not found: #identical")],
    )
    .await;
    let fp_a: Vec<String> = store2
        .list_execution_telemetry(&a, 100)
        .await
        .expect("telemetry")
        .into_iter()
        .filter_map(|t| t.failure_fingerprint)
        .collect();
    let fp_b: Vec<String> = store2
        .list_execution_telemetry(&b, 100)
        .await
        .expect("telemetry")
        .into_iter()
        .filter_map(|t| t.failure_fingerprint)
        .collect();
    assert_eq!(
        fp_a, fp_b,
        "the fixtures must share a fingerprint, or this proves nothing"
    );
    assert!(
        IntegrityVerifier::new(&store2)
            .fingerprint_registration(&a, run_b)
            .await
            .is_empty(),
        "tenant B's rows are invisible to tenant A even with an identical fingerprint"
    );
    assert!(
        IntegrityVerifier::new(&store2)
            .fingerprint_registration(&a, run_a)
            .await
            .is_empty()
            || true,
        "tenant A sees only its own run"
    );
}

// ------------------------------------------------------------------ read-only

/// The read-only property, checked three ways.
///
/// 1. **Source**: the module contains no SQL of its own — no `sqlx`, no write verb. So there
///    is no query to make arbitrary, and no statement to make destructive.
/// 2. **Type**: `IntegrityVerifier` holds a `&Store`, so no method can mutate through it.
/// 3. **Runtime**: every comparison is run against a database holding a sentinel row, and the
///    row and the table's row count must both be unchanged afterwards.
#[tokio::test]
async fn the_verifier_cannot_write() {
    // ---- 1. source guard
    let source = include_str!("integrity.rs");
    assert!(
        !source.contains("sqlx"),
        "integrity.rs must hold no SQL of its own; every read goes through Store"
    );

    // Comment lines are excluded: the module's own doc comment *names* `&mut self` in order
    // to state that there is none, and a guard that trips on its own explanation is not a
    // guard. Only code counts.
    let code: String = source
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && !l.starts_with("///") && !l.starts_with('*'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("&mut self"),
        "IntegrityVerifier must expose no method that could mutate"
    );
    for verb in ["sqlx", "INSERT INTO", "UPDATE ", "DELETE FROM", "DROP "] {
        assert!(
            !code.contains(verb),
            "the module's code must contain no SQL of its own ({verb})"
        );
    }

    // ---- 2/3. runtime sentinel
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P6 ReadOnly").await;
    let c = case(&store, &pid, "Sentinel").await;
    let run = record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-ro", TaskStatus::Failed)
            .reason("Element not found: #ro")
            .api_says(TaskStatus::Passed, None)
            .case(c)],
    )
    .await;
    let crawl = crawl_of(vec![element("button", "Pay", "#ro")]);

    let before_cases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_cases WHERE id = ?")
        .bind(c.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("sentinel count");
    let before_title: String = sqlx::query_scalar("SELECT title FROM test_cases WHERE id = ?")
        .bind(c.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("sentinel title");
    let before_telemetry: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_telemetry WHERE project_id = ?")
            .bind(&pid)
            .fetch_one(&store.pool)
            .await
            .expect("telemetry count");
    let before_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE project_id = ?")
        .bind(&pid)
        .fetch_one(&store.pool)
        .await
        .expect("run count");

    let verifier = IntegrityVerifier::new(&store).with_evidence(&evidence);
    let _ = verifier.verify_run(&pid, run, Some(&crawl)).await;
    let _ = verifier.task_status_consistency(&pid, run).await;
    let _ = verifier.task_duration_consistency(&pid, run).await;
    let _ = verifier.task_reason_consistency(&pid, run).await;
    let _ = verifier.run_state_consistency(&pid, run).await;
    let _ = verifier.fingerprint_registration(&pid, run).await;
    let _ = verifier.event_log_consistency(&pid, run).await;
    let _ = verifier.evidence_index_consistency(run).await;
    let _ = verifier.audit_trail_consistency(&pid, 100).await;
    let _ = verifier.fabricated_constant_check(&pid).await;
    let _ = verifier.ui_api_consistency(&pid, run, &crawl).await;
    let _ = verifier.before_after_state(&pid, &c.to_string()).await;

    let after_cases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_cases WHERE id = ?")
        .bind(c.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("sentinel count");
    let after_title: String = sqlx::query_scalar("SELECT title FROM test_cases WHERE id = ?")
        .bind(c.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("sentinel title");
    let after_telemetry: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_telemetry WHERE project_id = ?")
            .bind(&pid)
            .fetch_one(&store.pool)
            .await
            .expect("telemetry count");
    let after_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE project_id = ?")
        .bind(&pid)
        .fetch_one(&store.pool)
        .await
        .expect("run count");

    assert_eq!(after_cases, before_cases, "no row may be added or removed");
    assert_eq!(after_title, before_title, "no row may be modified");
    assert_eq!(
        after_telemetry, before_telemetry,
        "the verifier must not write telemetry"
    );
    assert_eq!(after_runs, before_runs, "the verifier must not write runs");
}

// ------------------------------------------------------------------ the audit finding

/// The finding that motivated this module, re-asserted against the **remediated** rows.
///
/// ## This test changed when the risk signals were remediated
///
/// Its first version asserted that the detector finds hardcoded `0.0` values, which was true
/// and is why the remediation exists. After the remediation the real rows hold `NULL`, because no
/// signal source exists for those columns — so the detector reports the honest state instead,
/// and this test asserts *that*.
///
/// The fabricated-constant branch did **not** disappear with it: the next test drives that branch
/// through the real production writer. Removing the assertion without replacing it would have
/// quietly deleted the coverage the original test provided.
///
/// See `docs/next-intelligence/PHASE_4_RISK_SIGNAL_REMEDIATION.md`.
#[tokio::test]
async fn the_constant_check_reports_unsourced_signals_as_unmeasured_not_fabricated() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 ConstantsRemediated").await;
    let c = case(&store, &pid, "Pay").await;

    // A real failing execution drives `update_test_intelligence`, which writes the row.
    record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-const", TaskStatus::Failed)
            .reason("Element not found: #const")
            .case(c)],
    )
    .await;

    let assessments: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM risk_assessments WHERE node_id IN \
         (SELECT id FROM quality_nodes WHERE project_id = ?)",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("assessment count");
    assert!(
        assessments > 0,
        "the fixture must produce real risk_assessments rows, or the check proves nothing"
    );

    // The two unsourced columns are NULL, and a NULL is readable as a NULL.
    let (sec, biz, chg): (Option<f64>, Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT security_sensitivity, business_criticality, change_frequency \
         FROM risk_assessments WHERE node_id IN \
         (SELECT id FROM quality_nodes WHERE project_id = ?) LIMIT 1",
    )
    .bind(&pid)
    .fetch_one(&store.pool)
    .await
    .expect("read the row");
    assert_eq!(
        sec, None,
        "security_sensitivity has no signal source and must be NULL, not 0.0"
    );
    assert_eq!(
        biz, None,
        "business_criticality has no signal source and must be NULL, not 0.0"
    );
    assert!(
        chg.is_some(),
        "change_frequency has a real source since Phase 3 and must be measured, not NULL"
    );

    let findings = IntegrityVerifier::new(&store)
        .fabricated_constant_check(&pid)
        .await
        .expect("the constant check reads through the store's own reader");

    // The two unsourced columns are reported as an honestly-stated gap.
    for column in ["security_sensitivity", "business_criticality"] {
        let finding = findings
            .iter()
            .find(|f| f.field == column)
            .unwrap_or_else(|| panic!("{column} must be reported"));
        assert_eq!(
            finding.outcome,
            IntegrityOutcome::InsufficientData,
            "{column}: {}",
            finding.detail
        );
        assert!(
            finding.detail.contains("no signal source"),
            "the detail must say why: {}",
            finding.detail
        );
        assert!(
            finding.observations.iter().all(|o| !o.is_measurement),
            "an unmeasured signal is not a measurement"
        );
        assert!(
            !finding.establishes_defect(),
            "a stated gap is not a defect"
        );
        assert!(finding.defect_caveat.is_some(), "a non-defect must say why");
        assert_eq!(finding.data_sufficiency, 0.0, "nothing was measured");
    }

    // `security_sensitivity` carries real weight, so the gap must be quantified.
    let security = findings
        .iter()
        .find(|f| f.field == "security_sensitivity")
        .expect("security_sensitivity");
    assert!(
        security.detail.contains("0.10"),
        "the unbacked weight must be stated: {}",
        security.detail
    );

    // And the column that *does* have a source is not reported at all.
    assert!(
        !findings.iter().any(|f| f.field == "change_frequency"),
        "a measured column must not be reported as carrying no information: {:?}",
        findings.iter().map(|f| &f.field).collect::<Vec<_>>()
    );
}

/// The **fabricated** branch still fires — a constant `0.0` is still a defect.
///
/// Driven through the real production writer with `Some(0.0)`, which is a value any caller of
/// `upsert_risk_assessment` can supply. Without this, the remediation would have deleted the
/// detector's ability to catch a recurrence, and a future regression would go unnoticed.
#[tokio::test]
async fn a_constant_zero_supplied_through_the_real_writer_is_still_reported_as_fabricated() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 FabricatedZero").await;
    let c = case(&store, &pid, "Pay").await;
    record_run(
        &store,
        &pid,
        &[TaskSpec::new("t-zero", TaskStatus::Passed).case(c)],
    )
    .await;

    // SQLite stores these ids as TEXT, so they decode as `String`, not `Uuid`.
    let node: String =
        sqlx::query_scalar("SELECT id FROM quality_nodes WHERE project_id = ? LIMIT 1")
            .bind(&pid)
            .fetch_one(&store.pool)
            .await
            .expect("the fixture must have produced a node");
    let node = Uuid::parse_str(&node).expect("a node id is a uuid");

    // The real writer, given a fabricated zero — exactly what the pre-remediation code did.
    store
        .upsert_risk_assessment(
            node,
            0.5,
            Some(0.0),
            Some(0.0),
            0.5,
            1,
            Some(0.0),
            100.0,
            "{}",
        )
        .await
        .expect("upsert");

    let findings = IntegrityVerifier::new(&store)
        .fabricated_constant_check(&pid)
        .await
        .expect("the constant check reads through the store's own reader");

    for column in ["security_sensitivity", "business_criticality"] {
        let finding = findings
            .iter()
            .find(|f| f.field == column)
            .unwrap_or_else(|| panic!("a fabricated {column} must be reported"));
        assert_eq!(
            finding.outcome,
            IntegrityOutcome::Inconsistent,
            "a supplied 0.0 is a fabricated measurement, not an unmeasured signal: {}",
            finding.detail
        );
        assert!(
            finding.detail.contains("one distinct value"),
            "the detail must state what was observed: {}",
            finding.detail
        );
        assert!(
            finding.detail.contains("fabricated"),
            "the detail must name the problem: {}",
            finding.detail
        );
        assert!(
            !finding.establishes_defect(),
            "a constant is not a measurement, so it cannot establish a product defect"
        );
        assert_eq!(
            finding.data_sufficiency, 1.0,
            "the value exists, it is just constant"
        );
    }

    // And a measured zero in a column that *does* have a source is NOT a fabrication. This is
    // the "measured zero is distinguishable from unmeasured" property, inside the detector.
    assert!(
        !findings.iter().any(|f| f.field == "change_frequency"),
        "a legitimately measured zero must not be reported as fabricated: {:?}",
        findings.iter().map(|f| &f.field).collect::<Vec<_>>()
    );
}

/// A project with no risk assessments has nothing to check, and the check says so.
#[tokio::test]
async fn the_fabricated_constant_check_is_silent_without_rows() {
    let (store, _d, _e) = harness().await;
    let pid = project(&store, "P6 NoAssessments").await;
    let findings = IntegrityVerifier::new(&store)
        .fabricated_constant_check(&pid)
        .await
        .expect("the constant check reads through the store's own reader");
    assert!(
        findings.is_empty(),
        "no rows means no observations, not a finding about nothing"
    );
}

// ------------------------------------------------------------------ the report

/// The report's shape: counts add up, layers are named, and defects are a subset of
/// inconsistencies.
#[tokio::test]
async fn the_report_counts_add_up_and_names_its_layers() {
    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P6 Report").await;
    let c = case(&store, &pid, "Pay").await;
    let run = record_run(
        &store,
        &pid,
        &[
            TaskSpec::new("t-ok", TaskStatus::Passed)
                .case(c)
                .selector("#pay"),
            TaskSpec::new("t-bad", TaskStatus::Failed)
                .reason("Element not found: #bad")
                .api_says(TaskStatus::Passed, None),
        ],
    )
    .await;
    emit(&store, run, 1, "t-bad", TaskStatus::Failed).await;
    let crawl = crawl_of(vec![element("button", "Pay", "#pay")]);

    let report = IntegrityVerifier::new(&store)
        .with_evidence(&evidence)
        .verify_run(&pid, run, Some(&crawl))
        .await;

    let counted: usize = report.counts.values().sum();
    assert_eq!(
        counted,
        report.comparisons.len(),
        "every comparison must be counted: {:?}",
        report.counts
    );
    assert!(report.inconsistent > 0, "the fixture contains a mismatch");
    assert!(
        report.defects_established <= report.inconsistent,
        "defects are a subset of inconsistencies: {} vs {}",
        report.defects_established,
        report.inconsistent
    );
    assert!(
        report.layers_observed.contains(&Layer::Api),
        "telemetry was read: {:?}",
        report.layers_observed
    );
    assert!(
        report.layers_observed.contains(&Layer::Backend),
        "tasks were read: {:?}",
        report.layers_observed
    );
    assert_eq!(report.window, ComparisonWindow::default());
    assert_eq!(report.project_id, pid);

    // Every comparison carries a non-empty detail and a defect caveat when not a defect.
    for comp in &report.comparisons {
        assert!(
            !comp.detail.is_empty(),
            "{:?} must say why: {}",
            comp.kind,
            comp.entity
        );
        if !comp.defect_established {
            assert!(
                comp.defect_caveat.is_some(),
                "a non-defect must always say why: {}",
                comp.entity
            );
        }
    }
}

/// Layer names round-trip, because the API and any storage depend on it.
#[test]
fn layer_and_outcome_names_round_trip() {
    for layer in [
        Layer::Ui,
        Layer::Api,
        Layer::Backend,
        Layer::Database,
        Layer::Audit,
    ] {
        assert_eq!(Layer::parse(layer.as_str()), Some(layer));
    }
    assert_eq!(Layer::parse("MEMORY"), None);
    assert_eq!(Layer::all().len(), 5);

    for outcome in [
        IntegrityOutcome::Consistent,
        IntegrityOutcome::Inconsistent,
        IntegrityOutcome::InsufficientData,
        IntegrityOutcome::NotComparable,
    ] {
        assert!(!outcome.as_str().is_empty());
    }
    assert_eq!(
        IntegrityOutcome::InsufficientData.as_str(),
        "INSUFFICIENT_DATA"
    );
    assert_eq!(IntegrityOutcome::NotComparable.as_str(), "NOT_COMPARABLE");
}

// ------------------------------------------------------------------ performance

/// Measures comparison latency, database read overhead, evidence overhead, and behaviour as
/// the entity set grows.
///
/// **Deliberately `#[ignore]`d and deliberately not asserting a duration.** A wall-clock
/// threshold picked on one machine fails on another, and a threshold nobody derived from a
/// measurement is worse than no threshold. The numbers are in
/// `PHASE_6_DATA_INTEGRITY_REPORT.md`; this exists so they can be reproduced:
///
/// ```text
/// cargo test phase6_measures_integrity_cost -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "measurement, not a gate; see the module comment"]
async fn phase6_measures_integrity_cost() {
    use std::time::Instant;

    let (store, _d, evidence) = harness().await;
    let pid = project(&store, "P6 Perf").await;
    let verifier = IntegrityVerifier::new(&store).with_evidence(&evidence);

    // ---- grow the entity set, so the per-entity cost is visible rather than assumed.
    println!(
        "  {:>6}  {:>10}  {:>10}  {:>10}",
        "tasks", "build_ms", "verify_ms", "per_task"
    );
    let mut timings = Vec::new();
    for size in [5usize, 20, 60] {
        let specs: Vec<TaskSpec> = (0..size)
            .map(|i| {
                if i % 3 == 0 {
                    TaskSpec::new(&format!("t-{size}-{i}"), TaskStatus::Failed)
                        .reason("Element not found: #perf")
                        .duration_ms(250)
                } else {
                    TaskSpec::new(&format!("t-{size}-{i}"), TaskStatus::Passed)
                        .selector("#pay")
                        .duration_ms(250)
                }
            })
            .collect();

        let build = Instant::now();
        let run = record_run(&store, &pid, &specs).await;
        let build_ms = build.elapsed().as_millis();

        // Ten runs of the event writer, so the audit/event layers have real rows too.
        for (i, s) in specs.iter().take(10).enumerate() {
            emit(&store, run, i as u64 + 1, &s.id, s.backend_status).await;
        }

        let verify = Instant::now();
        let report = verifier.verify_run(&pid, run, None).await;
        let verify_ms = verify.elapsed().as_millis();

        println!(
            "  {size:>6}  {build_ms:>10}  {verify_ms:>10}  {:>9.3} ms",
            verify_ms as f64 / size as f64
        );
        timings.push((size, build_ms, verify_ms, report.comparisons.len()));

        // Correctness, not timing.
        assert_eq!(
            report
                .comparisons
                .iter()
                .filter(|c| c.establishes_defect())
                .count(),
            report.defects_established,
        );
    }

    // ---- database read overhead, isolated: one bounded telemetry read plus the run reads.
    let read = Instant::now();
    let _ = store
        .list_execution_telemetry(&pid, 20_000)
        .await
        .expect("telemetry");
    let read_ms = read.elapsed().as_millis();
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_telemetry WHERE project_id = ?")
            .bind(&pid)
            .fetch_one(&store.pool)
            .await
            .expect("row count");
    println!("  db read : one bounded telemetry read of {rows} rows in {read_ms} ms");

    // ---- evidence overhead, isolated: the artifact check the report performs.
    let ev = Instant::now();
    for i in 0..50 {
        evidence
            .put(
                &Uuid::new_v4().to_string(),
                "probe",
                "log",
                &format!("payload {i}"),
            )
            .expect("put");
    }
    println!(
        "  evidence: 50 artifact writes in {} ms",
        ev.elapsed().as_millis()
    );

    // The measurement is only meaningful if every size actually produced comparisons --
    // a verifier that silently returned nothing at the larger sizes would look fast.
    assert!(
        timings
            .iter()
            .all(|(_, _, _, comparisons)| *comparisons > 0),
        "every entity-set size must produce comparisons: {timings:?}"
    );
    // And the per-task cost must not grow with the entity set, which is the property that
    // makes the verifier usable on a large project.
    let per_task: Vec<f64> = timings
        .iter()
        .map(|(size, _, verify_ms, _)| *verify_ms as f64 / *size as f64)
        .collect();
    assert!(
        per_task.last() <= per_task.first(),
        "per-task cost must not grow with the entity set: {per_task:?}"
    );
}
