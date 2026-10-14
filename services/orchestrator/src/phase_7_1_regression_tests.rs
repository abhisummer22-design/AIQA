//! Phase 7.1 — regression tests for the two production defects Phase 7 found.
//!
//! | Defect | Symptom before | Assertion here |
//! |---|---|---|
//! | **P7-D1** | `IntegrityVerifier::verify_run` masked 16 store reads, so a closed pool produced `Ok` with `comparisons: 0`, `inconsistent: 0`, `defects_established: 0` | a failed read is `Err`; every comparator propagates; genuine empty data and genuine clean data still return `Ok`; a real inconsistency and a stale observation are still detected; the API does not turn a storage failure into a 200 |
//! | **P7-D2** | `TestStabilityView.failure_rate` divided by *every* recorded outcome, so a step blocked by a failed prerequisite measured `Some(0.0)` — a 0% failure rate for a case that never ran | a blocked outcome yields `None`; genuine pass/fail rates are unchanged; non-execution outcomes do not dilute a rate; the prerequisite DAG is untouched |
//!
//! ## Why these are real tests and not characterisations
//!
//! Phase 7 pinned the *defective* behaviour deliberately, because a validation phase may not
//! change production code. Both of those blocks in `journey_tests.rs` had to be **rewritten** to
//! assert the corrected behaviour, which is itself the evidence that the defects were real. These
//! tests go further: they pin the neighbouring behaviours that must **not** change, so the fixes
//! cannot be "satisfied" by making the reader lazier or the verifier blunter.
//!
//! ## Setup discipline
//!
//! Every row is written by a public `Store` writer or by `Intelligence::ingest_run` -- the same
//! path the engine uses. A `TempDir` per test, migrated by `Store::connect`. No direct SQL to
//! manufacture state. `store.pool.close()` is the sanctioned way to produce a genuine read
//! failure (already used by `engine_tests`, the Phase 6.1 R1b work, and Phase 6.2).

use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::intelligence::Intelligence;
use crate::policy::PolicyConfig;
use crate::qa_memory::QaMemoryReader;
use crate::state::AppState;
use crate::store::Store;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A migrated, isolated database. `Store::connect` runs `sqlx::migrate!("./migrations")`.
struct Db {
    store: Store,
    _dir: TempDir,
}

impl Db {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("phase71.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, _dir: dir }
    }
}

fn inert_ai_router() -> Arc<AiInfrastructure> {
    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> =
        std::collections::HashMap::new();
    executors.insert("mock", executor);
    Arc::new(AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider(
            "mock",
            vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
        )))],
        executors,
    ))
}

async fn app(db_dir: &TempDir) -> AppState {
    let store = Store::connect(&db_dir.path().join("phase71api.sqlite3"))
        .await
        .expect("connect + migrate");
    let evidence = LocalEvidenceStore::new(db_dir.path().join("evidence")).expect("evidence store");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register("maestro", Arc::new(SimulatedExecutor));
    let policy = PolicyConfig {
        max_concurrency: 8,
        require_approval_for: ["maestro".to_string()].into_iter().collect(),
        blocked_executors: HashSet::new(),
    };
    AppState::new(
        Arc::new(registry),
        store,
        policy,
        evidence,
        inert_ai_router(),
        Arc::new(FakeEmbedder::unconfigured()),
    )
}

fn req(method: &str, path: &str, key: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(k) = key {
        builder = builder.header("Authorization", format!("Bearer {k}"));
    }
    builder.body(Body::empty()).expect("request")
}

async fn send(state: &AppState, request: Request<Body>) -> (StatusCode, Value) {
    let app = crate::api::router(state.clone());
    let resp = app.oneshot(request).await.expect("router call");
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

async fn mint_key(state: &AppState, role: &str, project_id: &str) -> String {
    let raw = crate::auth::generate_key();
    state
        .store
        .insert_api_key(
            Uuid::new_v4(),
            "phase71-key",
            &crate::auth::hash_key(&raw),
            role,
            Some(project_id),
        )
        .await
        .expect("insert_api_key");
    raw
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
                description: format!("{title} for a phase 7.1 regression test"),
                category: "Functional".into(),
                priority: "HIGH".into(),
                severity: "MAJOR".into(),
                preconditions: vec![],
                steps: vec![],
                expected_result: Some("the step completes".into()),
                assertions: vec![],
                test_data: None,
                source_document: None,
                requirement_reference: None,
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: vec![],
                suite_id: None,
            },
            Some("phase71"),
        )
        .await
        .expect("insert_test_case")
}

/// Record `steps` for one run exactly as the engine would: each task's own start/finish, then a
/// single `ingest_run` that derives telemetry through the production chain.
async fn record_run(
    store: &Store,
    project_id: &str,
    case: Uuid,
    steps: &[(&str, TaskStatus, Option<&str>)],
) -> Uuid {
    let run_id = Uuid::new_v4();
    let plan = RunPlan {
        name: "phase 7.1 regression run".to_string(),
        tasks: steps
            .iter()
            .map(|(id, _, _)| TaskDef {
                id: (*id).to_string(),
                name: format!("step {id}"),
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
            Some("phase71"),
            RunState::Running,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");

    for (id, status, reason) in steps {
        store
            .set_task_status(run_id, id, TaskStatus::Running, None, true, false)
            .await
            .expect("start step");
        store
            .set_task_status(run_id, id, *status, *reason, false, true)
            .await
            .expect("finish step");
    }

    let states: Vec<TaskState> = steps
        .iter()
        .map(|(id, status, reason)| TaskState {
            id: (*id).to_string(),
            name: format!("step {id}"),
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

async fn stability(
    store: &Store,
    project_id: &str,
    case: Uuid,
) -> crate::qa_memory::TestStabilityView {
    QaMemoryReader::new(store.clone())
        .test_stability(project_id, &case.to_string())
        .await
        .expect("test_stability")
        .view
        .expect("a stability view for a case that has telemetry")
}

// ===========================================================================
// P7-D1 — the integrity verifier must not convert a failed read into a clean run
// ===========================================================================

/// The defect itself. With the pool closed, `verify_run` used to answer `Ok` with an empty
/// comparison list -- the same answer a clean run gives.
#[tokio::test]
async fn p71_d1_a_closed_pool_fails_the_verification_rather_than_reporting_clean() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D1 closed").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "checkout").await;
    let run = record_run(store, &pid_s, case, &[("t", TaskStatus::Passed, None)]).await;

    // Sanity: it verifies cleanly while the store is readable. Without this, an always-failing
    // verifier would also pass this test.
    let ok = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, run, None)
        .await
        .expect("a readable store verifies");
    assert!(
        ok.is_complete(),
        "a report from readable storage is complete: {:?}",
        ok.read_failures
    );

    store.pool.close().await;

    let failed = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, run, None)
        .await;
    let failure = failed.expect_err(
        "P7-D1: an unreadable store must not produce a successful report. Before the fix this \
         returned Ok with comparisons: 0, inconsistent: 0 and defects_established: 0 -- \
         indistinguishable from a clean run.",
    );
    assert!(
        !failure.to_string().is_empty(),
        "the error must carry a message, not be an opaque unit"
    );
    db.store.pool.close().await;
}

/// Every required comparator must propagate, not just `verify_run`. If only the top level
/// propagated, a caller using a comparator directly would still get a silent empty vector.
#[tokio::test]
async fn p71_d1_every_required_comparator_propagates_a_failed_read() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D1 comparators").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "checkout").await;
    let run = record_run(store, &pid_s, case, &[("t", TaskStatus::Passed, None)]).await;

    // `evidence_index_consistency` answers empty *without reading* when no evidence store was
    // attached -- a genuine "this verifier cannot see evidence" answer, not a masked failure. So
    // to exercise the read itself the verifier has to be built with one. Getting this wrong was a
    // real mistake in the first draft of this test: it failed against correct code.
    let evidence =
        LocalEvidenceStore::new(db._dir.path().join("evidence")).expect("an evidence store");

    store.pool.close().await;
    let v = crate::integrity::IntegrityVerifier::new(store).with_evidence(&evidence);

    let results: Vec<(&str, bool)> = vec![
        (
            "task_status_consistency",
            v.task_status_consistency(&pid_s, run).await.is_err(),
        ),
        (
            "task_duration_consistency",
            v.task_duration_consistency(&pid_s, run).await.is_err(),
        ),
        (
            "task_reason_consistency",
            v.task_reason_consistency(&pid_s, run).await.is_err(),
        ),
        (
            "run_state_consistency",
            v.run_state_consistency(&pid_s, run).await.is_err(),
        ),
        (
            "fingerprint_registration",
            v.fingerprint_registration(&pid_s, run).await.is_err(),
        ),
        (
            "event_log_consistency",
            v.event_log_consistency(&pid_s, run).await.is_err(),
        ),
        (
            "audit_trail_consistency",
            v.audit_trail_consistency(&pid_s, 100).await.is_err(),
        ),
        (
            "evidence_index_consistency",
            v.evidence_index_consistency(run).await.is_err(),
        ),
    ];
    for (name, propagated) in results {
        assert!(
            propagated,
            "P7-D1: {name} masked a failed read into an empty answer; it must propagate"
        );
    }

    // `before_after_state` and `ui_api_consistency` read telemetry too, through the same helper.
    assert!(
        v.before_after_state(&pid_s, &case.to_string())
            .await
            .is_err(),
        "P7-D1: before_after_state masked a failed read"
    );
    db.store.pool.close().await;
}

/// Distinguishes the three outcomes the fix had to keep apart.
///
/// A. successful verification, no defects.
/// B. successful verification, genuinely insufficient evidence.
/// C. verification that could not complete.
#[tokio::test]
async fn p71_d1_a_genuinely_empty_run_is_insufficient_evidence_not_an_error() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D1 empty").await;
    let pid_s = pid.to_string();

    // A run that exists and recorded no observations at all.
    let plan = RunPlan {
        name: "empty".to_string(),
        tasks: vec![],
        concurrency: 1,
    };
    let run = Uuid::new_v4();
    store
        .insert_run_for_release(
            run,
            &plan,
            Some(&pid_s),
            Some("phase71"),
            RunState::Running,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");

    let report = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, run, None)
        .await
        .expect("B: insufficient evidence is a successful verification, not a failed one");

    assert!(
        report.is_complete(),
        "and it is complete -- the reads succeeded, there was simply nothing to compare: {:?}",
        report.read_failures
    );
    assert_eq!(
        report.defects_established, 0,
        "nothing is established when there is nothing to compare"
    );
    assert!(
        !report.layers_unavailable.is_empty(),
        "and the absent layers are named, which is what distinguishes B from A: {:?}",
        report.layers_unavailable
    );

    // An unknown run id is also an empty answer, not an error: "no such run" is a fact.
    let unknown = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, Uuid::new_v4(), None)
        .await
        .expect("an unknown run is answerable, not an error");
    assert!(unknown.is_complete());
    assert!(unknown.comparisons.is_empty());
    db.store.pool.close().await;
}

/// A: a genuine, fully-read run with consistent layers must still verify.
#[tokio::test]
async fn p71_d1_a_consistent_run_still_verifies_and_completes() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D1 consistent").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "checkout").await;
    let run = record_run(
        store,
        &pid_s,
        case,
        &[
            ("t1", TaskStatus::Passed, None),
            ("t2", TaskStatus::Failed, Some("Element not found: #pay")),
        ],
    )
    .await;

    let report = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, run, None)
        .await
        .expect("a readable store verifies");

    assert!(report.is_complete(), "{:?}", report.read_failures);
    assert!(
        !report.comparisons.is_empty(),
        "a run with two recorded steps must produce comparisons, not an empty list"
    );
    assert_eq!(report.run_id, run, "and the report must be about this run");
    assert!(
        report
            .layers_observed
            .contains(&crate::integrity::Layer::Backend),
        "the recorded task rows must register as an observed layer: {:?}",
        report.layers_observed
    );
    db.store.pool.close().await;
}

/// The fix must not blunt the verifier. A disagreement between the task row and the telemetry
/// row is a real inconsistency and must still be reported as one.
#[tokio::test]
async fn p71_d1_a_genuine_inconsistency_is_still_detected() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D1 mismatch").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "checkout").await;

    // The backend records the step as FAILED while ingestion writes telemetry saying PASSED --
    // the two layers genuinely disagree, and only one of them is a measurement.
    let run = record_run(
        store,
        &pid_s,
        case,
        &[("t", TaskStatus::Failed, Some("boom"))],
    )
    .await;
    store
        .set_task_status(run, "t", TaskStatus::Passed, None, false, true)
        .await
        .expect("rewrite the task row");

    let report = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, run, None)
        .await
        .expect("a readable store verifies");

    // Rewriting the task as PASSED makes it disagree with its own telemetry on more than one
    // field -- status, duration, and whether a reason was recorded. All of that is real, so the
    // assertion is that a status disagreement is among them, not that it is the only one.
    let mismatch = report
        .comparisons
        .iter()
        .find(|c| c.kind == crate::integrity::ComparisonKind::StatusConsistency)
        .expect("a status comparison");
    assert_eq!(
        mismatch.outcome,
        crate::integrity::IntegrityOutcome::Inconsistent,
        "a genuine backend-vs-telemetry disagreement must still be reported: {mismatch:?}"
    );
    assert_eq!(
        report.inconsistent,
        report
            .comparisons
            .iter()
            .filter(|c| c.outcome == crate::integrity::IntegrityOutcome::Inconsistent)
            .count(),
        "and the report's own counter must agree with its contents: {:?}",
        report.counts
    );
    assert!(
        mismatch.establishes_defect(),
        "both sides are measurements of the same field, which is the stricter case this module \
         exists for, so it must still establish a defect: {mismatch:?}"
    );
    assert!(
        report.defects_established >= 1,
        "the verifier is not blunted by the error-handling change: {:?}",
        report.counts
    );
    // And a comparison where only one layer has a reading is still refused as a defect.
    let reason = report
        .comparisons
        .iter()
        .find(|c| c.field == "failure_reason")
        .expect("a failure_reason comparison");
    assert!(
        !reason.establishes_defect(),
        "a difference in how much failure detail two layers captured is still not a product \
         defect: {reason:?}"
    );
    db.store.pool.close().await;
}

/// The staleness policy must be untouched by the error-handling change.
#[tokio::test]
async fn p71_d1_a_stale_observation_is_still_insufficient_data_not_a_defect() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D1 stale").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "checkout").await;

    // The run and its task row are written, but **no** telemetry is ingested. The aged row below
    // must be the only one: with a second fresh row present the verifier pairs that one instead,
    // the skew is never exercised, and the test passes for the wrong reason. (Phase 6.2's S7b
    // documented this trap; the first draft of this test walked straight into it again.)
    let run = Uuid::new_v4();
    let plan = RunPlan {
        name: "stale".to_string(),
        tasks: vec![TaskDef {
            id: "t".to_string(),
            name: "step t".to_string(),
            prerequisites: vec![],
            simulate: Some(SimSpec {
                outcome: SimOutcome::Fail,
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            params: json!({}),
            test_case_id: Some(case.to_string()),
        }],
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run,
            &plan,
            Some(&pid_s),
            Some("phase71"),
            RunState::Running,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    store
        .set_task_status(run, "t", TaskStatus::Running, None, true, false)
        .await
        .expect("start");
    store
        .set_task_status(run, "t", TaskStatus::Failed, Some("boom"), false, true)
        .await
        .expect("finish");

    // One row only, aged well outside the comparison window, written through the production
    // writer.
    let aged = crate::adaptive_intelligence::ExecutionTelemetry {
        created_at: chrono::Utc::now() - chrono::Duration::hours(48),
        ..crate::adaptive_intelligence::ExecutionTelemetry::from_task_state(
            &pid_s,
            run,
            &TaskState {
                id: "t".into(),
                name: "step t".into(),
                prerequisites: vec![],
                status: TaskStatus::Failed,
                reason: Some("boom".into()),
                started_at: None,
                finished_at: None,
                test_case_id: Some(case.to_string()),
            },
            "simulated",
            "test",
            Some(case),
            Vec::new(),
            None,
            None,
            None,
        )
    };
    store
        .insert_execution_telemetry(&aged)
        .await
        .expect("persist the aged row");

    let report = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, run, None)
        .await
        .expect("a readable store verifies");

    assert_eq!(
        report.defects_established, 0,
        "stale evidence must never establish a defect: {:?}",
        report.comparisons
    );
    let stale = report
        .comparisons
        .iter()
        .find(|c| c.kind == crate::integrity::ComparisonKind::StatusConsistency)
        .expect("a status comparison");
    assert_eq!(
        stale.outcome,
        crate::integrity::IntegrityOutcome::InsufficientData,
        "records that fall outside the comparison window are not comparable, so they are not a \
         disagreement: {stale:?}"
    );
    assert!(!stale.establishes_defect(), "{stale:?}");
    db.store.pool.close().await;
}

/// The API boundary. A verification that could not run must not reach a caller as a 200 carrying
/// `defects_established: 0` -- that is the shape the defect took in production.
///
/// **A limitation worth stating.** Replacing the handler's `map_err(to_500)?` with a substitute
/// empty report is *not* caught by this test, and cannot be: with the pool closed the preceding
/// `require_scoped_project` read fails first, so the request is already a 5xx before the verifier
/// runs. That is why the mapping is asserted by reading the handler rather than by mutation here.
/// The reachable half of the contract -- a readable store yields a real report, an unreadable one
/// yields no report at all -- is what this test proves.
#[tokio::test]
async fn p71_d1_the_api_does_not_convert_a_storage_failure_into_a_successful_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = app(&dir).await;
    let pid = make_project(&state.store, "P71 D1 api").await;
    let pid_s = pid.to_string();
    let case = seed_case(&state.store, pid, "checkout").await;
    let run = record_run(
        &state.store,
        &pid_s,
        case,
        &[("t", TaskStatus::Passed, None)],
    )
    .await;
    let key = mint_key(&state, "viewer", &pid_s).await;

    let path = format!("/api/v1/projects/{pid}/integrity/{run}");

    // Readable storage: a real report.
    let (status, body) = send(&state, req("GET", &path, Some(&key))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["report"]["run_id"].as_str() == Some(run.to_string().as_str()),
        "and it really is this run's report: {body}"
    );
    // The report's own completeness signal must reach the client, not stay internal: a reader has
    // to be able to tell a clean report from one whose checks could not all run.
    assert!(
        body["report"].get("read_failures").is_some(),
        "read_failures must be serialised so a client can see an incomplete report: {body}"
    );
    assert_eq!(
        body["report"]["read_failures"].as_array().map(Vec::len),
        Some(0),
        "and with fully readable storage it must be empty: {body}"
    );

    // Unreadable storage: an error, never a clean report.
    state.store.pool.close().await;
    let (status, body) = send(&state, req("GET", &path, Some(&key))).await;
    assert!(
        status.is_server_error(),
        "an unreadable store must not produce a 200: got {status} with {body}"
    );
    assert!(
        body.get("report").is_none(),
        "and must not carry an integrity report at all, clean or otherwise: {body}"
    );
    assert!(
        !body.to_string().contains("defects_established\": 0"),
        "specifically, the false all-clear must not reach the client: {body}"
    );
    state.store.pool.close().await;
}

// ===========================================================================
// P7-D2 — a blocked step must not publish a 0% failure rate
// ===========================================================================

/// A, B, C. The prerequisite fails first, so the dependent never executes.
#[tokio::test]
async fn p71_d2_a_blocked_step_is_reported_as_blocked_and_its_rate_is_unmeasured() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D2 blocked").await;
    let pid_s = pid.to_string();
    let blocked_case = seed_case(store, pid, "dependent").await;
    // The blocking step belongs to its own case, as it would in a real journey: a search step
    // that failed, and the checkout step that never ran because of it. Sharing one case would
    // put the failure and the block in the same view and measure something else entirely --
    // also a real mistake in the first draft of this test.
    let failed_case = seed_case(store, pid, "blocking").await;

    record_run(
        store,
        &pid_s,
        failed_case,
        &[(
            "first",
            TaskStatus::Failed,
            Some("Element not found: #results"),
        )],
    )
    .await;
    record_run(
        store,
        &pid_s,
        blocked_case,
        &[(
            "second",
            TaskStatus::Blocked,
            Some("prerequisite first did not pass"),
        )],
    )
    .await;

    // B: the outcome is recorded, and unmistakably as blocked.
    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("list_execution_telemetry");
    let blocked_row = telemetry
        .iter()
        .find(|r| r.status == "BLOCKED")
        .expect("the blocked step's row");
    assert_eq!(
        blocked_row.test_case_id,
        Some(blocked_case),
        "and it belongs to the dependent step's case"
    );

    // C: and no rate is published for it.
    let v = stability(store, &pid_s, blocked_case).await;
    assert_eq!(v.passed, 0, "not a pass: {v:?}");
    assert_eq!(v.failed, 0, "not a failure: {v:?}");
    assert_eq!(
        v.other_outcomes, 1,
        "preserved as a blocked outcome rather than dropped: {v:?}"
    );
    assert_eq!(
        v.failure_rate, None,
        "P7-D2: a step that never executed has no failure rate. Before the fix the denominator \
         was every recorded outcome, so this reported Some(0.0) -- indistinguishable from a case \
         that passes."
    );

    // The step that did run and fail keeps its real measurement, in its own case.
    let ran = stability(store, &pid_s, failed_case).await;
    assert_eq!(ran.executions, 1, "{ran:?}");
    assert_eq!(ran.failed, 1, "it really failed: {ran:?}");
    assert_eq!(
        ran.failure_rate,
        Some(100.0),
        "and a completed execution still measures its real rate: {ran:?}"
    );
    db.store.pool.close().await;
}

/// D. Genuine passes are unchanged. One pass is still a measured 0% -- an existing tested
/// contract, and the fix deliberately did not invent a minimum-sample rule.
#[tokio::test]
async fn p71_d2_genuine_passes_still_measure_a_real_zero_rate() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D2 passes").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "passing").await;

    let v = {
        record_run(store, &pid_s, case, &[("t1", TaskStatus::Passed, None)]).await;
        stability(store, &pid_s, case).await
    };
    assert_eq!(v.passed, 1, "{v:?}");
    assert_eq!(
        v.failure_rate,
        Some(0.0),
        "D: one genuine pass measures 0% failures -- preserved from before the fix: {v:?}"
    );

    record_run(store, &pid_s, case, &[("t2", TaskStatus::Passed, None)]).await;
    let v2 = stability(store, &pid_s, case).await;
    assert_eq!(v2.passed, 2, "{v2:?}");
    assert_eq!(
        v2.failure_rate,
        Some(0.0),
        "still 0% over two passes: {v2:?}"
    );
    db.store.pool.close().await;
}

/// E. Genuine failures are unchanged.
#[tokio::test]
async fn p71_d2_genuine_failures_still_measure_a_real_hundred_percent() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D2 failures").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "failing").await;

    record_run(
        store,
        &pid_s,
        case,
        &[("t1", TaskStatus::Failed, Some("Element not found: #a"))],
    )
    .await;
    let v = stability(store, &pid_s, case).await;
    assert_eq!(v.failed, 1, "{v:?}");
    assert_eq!(
        v.failure_rate,
        Some(100.0),
        "E: one genuine failure measures 100% -- preserved: {v:?}"
    );

    record_run(store, &pid_s, case, &[("t2", TaskStatus::Timeout, None)]).await;
    let v2 = stability(store, &pid_s, case).await;
    assert_eq!(
        v2.failed, 2,
        "a TIMEOUT counts as a completed failure: {v2:?}"
    );
    assert_eq!(v2.failure_rate, Some(100.0), "{v2:?}");
    db.store.pool.close().await;
}

/// F. A mixed set of eligible outcomes produces the correct rate.
#[tokio::test]
async fn p71_d2_mixed_eligible_outcomes_produce_the_correct_rate() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D2 mixed").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "mixed").await;

    // Three passes, one failure.
    for (i, status) in [
        (1, TaskStatus::Passed),
        (2, TaskStatus::Passed),
        (3, TaskStatus::Passed),
        (4, TaskStatus::Failed),
    ]
    .into_iter()
    {
        let id = format!("t{i}");
        let reason = if status == TaskStatus::Failed {
            Some("Element not found: #x")
        } else {
            None
        };
        record_run(store, &pid_s, case, &[(id.as_str(), status, reason)]).await;
    }

    let v = stability(store, &pid_s, case).await;
    assert_eq!(v.passed, 3, "{v:?}");
    assert_eq!(v.failed, 1, "{v:?}");
    assert_eq!(v.executions, 4, "{v:?}");
    assert_eq!(
        v.failure_rate,
        Some(25.0),
        "F: one failure in four completed executions is 25%: {v:?}"
    );
    db.store.pool.close().await;
}

/// A regression test for a defect **I introduced while fixing P7-D2**: `test_stability` reports the
/// same rate twice, as the `failure_rate` field and as a `failure_rate` signal, and the first
/// version of the fix corrected only the field. The signal kept dividing by every recorded row, so
/// a case with one failure and one blocked step reported `failure_rate: Some(100.0)` alongside a
/// signal saying *"1 of 2 recorded executions failed (50.0%)"* — two different numbers for the
/// same metric in the same struct.
#[tokio::test]
async fn p71_d2_the_failure_rate_field_and_its_signal_always_agree() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D2 agreement").await;
    let pid_s = pid.to_string();

    // Three shapes, chosen so a disagreement is unavoidable if the two rates are computed
    // separately: only non-executions, only a genuine pass, and a genuine mix.
    // Only shapes with at least one *completed* execution have a rate at all; the shapes that do
    // not are asserted separately below, because there the correct behaviour is the absence of
    // both the field and the signal.
    for (label, steps) in [
        ("one pass", vec![("t", TaskStatus::Passed, None)]),
        (
            "pass and fail",
            vec![
                ("a", TaskStatus::Passed, None),
                ("b", TaskStatus::Failed, Some("Element not found: #z")),
            ],
        ),
        (
            "pass plus a blocked step",
            vec![
                ("a", TaskStatus::Passed, None),
                ("b", TaskStatus::Blocked, Some("prerequisite did not pass")),
            ],
        ),
    ] {
        let case = seed_case(store, pid, label).await;
        record_run(store, &pid_s, case, &steps).await;
        let v = stability(store, &pid_s, case).await;

        let signal = v
            .signals
            .iter()
            .find(|s| s.kind == "failure_rate")
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "{label}: this shape has completed executions, so a rate must exist -- and a \
                     rate that exists must be reported, not silently dropped. signals={:?} \
                     view={:?}",
                    v.signals, v
                )
            });
        let field = v.failure_rate.expect("a rate");
        let completed = v.passed + v.failed;
        assert!(
            signal.statement.contains(&format!("{field:.1}%")),
            "{label}: the field says {field:.1}% but the signal disagrees -- the same metric \
             reported two ways must not disagree. field={field} statement={}",
            signal.statement
        );
        assert!(
            signal.statement.contains(&format!("of {completed}")),
            "{label}: the statement must count completed executions ({completed}), not raw rows: {:?}",
            signal.statement
        );
        // The signal must also account for what it excluded, rather than quietly dropping it.
        assert!(
            signal.basis.contains(&v.other_outcomes.to_string()) || v.other_outcomes == 0,
            "{label}: the signal must name the {} non-execution outcome(s) it excluded: {:?}",
            v.other_outcomes,
            signal.basis
        );
    }

    // And the converse, for every shape with nothing completed: there is no rate anywhere -- not
    // `Some(0.0)` in the field, and not a signal claiming one. An unmeasured outcome stays
    // unmeasured, in both of the two places this view reports the metric.
    for (label, steps) in [
        (
            "blocked only",
            vec![("x", TaskStatus::Blocked, Some("prerequisite did not pass"))],
        ),
        (
            "blocked and skipped",
            vec![
                ("x", TaskStatus::Blocked, Some("prerequisite did not pass")),
                ("y", TaskStatus::Skipped, Some("not applicable")),
            ],
        ),
    ] {
        let case = seed_case(&db.store, pid, label).await;
        record_run(&db.store, &pid_s, case, &steps).await;
        let v = stability(&db.store, &pid_s, case).await;
        assert_eq!(v.passed, 0, "{label}: {v:?}");
        assert_eq!(v.failed, 0, "{label}: {v:?}");
        assert_eq!(
            v.other_outcomes,
            steps.len(),
            "{label}: the outcomes are still recorded and classified: {v:?}"
        );
        assert_eq!(
            v.failure_rate, None,
            "{label}: with nothing completed the field must not claim a rate: {v:?}"
        );
        assert!(
            !v.signals.iter().any(|s| s.kind == "failure_rate"),
            "{label}: and must not publish a rate signal either -- a signal here would be the \
             exact defect P7-D2 was, in its second location: {:?}",
            v.signals
        );
    }
    db.store.pool.close().await;
}
/// G. Outcomes where the step did not run must not dilute the denominator.
#[tokio::test]
async fn p71_d2_non_execution_outcomes_do_not_distort_the_failure_rate_denominator() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D2 dilution").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "dilution").await;

    // One genuine pass and one genuine failure, then three steps that never ran.
    record_run(store, &pid_s, case, &[("a", TaskStatus::Passed, None)]).await;
    record_run(
        store,
        &pid_s,
        case,
        &[("b", TaskStatus::Failed, Some("Element not found: #y"))],
    )
    .await;
    for i in 0..3 {
        let id = format!("blocked{i}");
        record_run(
            store,
            &pid_s,
            case,
            &[(
                id.as_str(),
                TaskStatus::Blocked,
                Some("prerequisite did not pass"),
            )],
        )
        .await;
    }

    let v = stability(store, &pid_s, case).await;
    assert_eq!(v.passed, 1, "{v:?}");
    assert_eq!(v.failed, 1, "{v:?}");
    assert_eq!(
        v.other_outcomes, 3,
        "the three blocked outcomes are still reported: {v:?}"
    );
    assert_eq!(v.executions, 5, "and still counted as observations: {v:?}");
    assert_eq!(
        v.failure_rate,
        Some(50.0),
        "G: the rate is over the two executions that completed. Counting the three blocked \
         rows in the denominator would report 20% -- silently making a half-failing case look \
         five times healthier: {v:?}"
    );
    db.store.pool.close().await;
}

/// H. With no eligible evidence the answer stays unmeasured, and stays `None` rather than `0.0`.
#[tokio::test]
async fn p71_d2_insufficient_eligible_evidence_stays_unmeasured() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P71 D2 unmeasured").await;
    let pid_s = pid.to_string();

    for label in ["blocked", "skipped", "cancelled"] {
        let case = seed_case(store, pid, label).await;
        let status = match label {
            "blocked" => TaskStatus::Blocked,
            "skipped" => TaskStatus::Skipped,
            _ => TaskStatus::Cancelled,
        };
        record_run(store, &pid_s, case, &[("t", status, Some("did not run"))]).await;
        let v = stability(store, &pid_s, case).await;
        assert_eq!(v.executions, 1, "{label}: the outcome is recorded: {v:?}");
        assert_eq!(
            v.other_outcomes, 1,
            "{label}: and classified as an outcome that is neither pass nor fail: {v:?}"
        );
        assert_eq!(
            v.failure_rate, None,
            "H: a {label} step has no completed execution, so its rate is unmeasured -- not 0%"
        );
    }
    db.store.pool.close().await;
}

/// I. The prerequisite DAG is untouched. The metric fix changed one denominator expression and
/// nothing about how a blocked status comes to exist in the first place.
#[tokio::test]
async fn p71_d2_the_prerequisite_dag_behaviour_is_unchanged() {
    use crate::plan_validator::{AgentExecutionPlan, AgentTaskPlan, PlanValidator};
    use std::collections::BTreeMap;

    let agent_step = |id: &str, deps: &[&str]| AgentTaskPlan {
        id: id.to_string(),
        name: format!("step {id}"),
        prerequisites: deps.iter().map(|s| s.to_string()).collect(),
        executor: "simulated".to_string(),
        target_url: None,
        actions: Vec::new(),
        timeout_secs: Some(60),
        requirement_reference: None,
        custom_params: None,
    };
    let mk = |tasks: Vec<AgentTaskPlan>| AgentExecutionPlan {
        project_id: "P71Dag".to_string(),
        environment: "test".to_string(),
        plan_name: "dag".to_string(),
        concurrency: 1,
        tasks,
        metadata: BTreeMap::new(),
    };

    // Legal forward reference: still accepted.
    assert!(
        PlanValidator::validate(
            &mk(vec![agent_step("s1", &["s2"]), agent_step("s2", &[])]),
            100,
            60,
            1
        )
        .await
        .is_ok(),
        "I: a DAG has no declaration order, so a forward reference stays legal"
    );

    // Unknown prerequisite: still refused.
    assert!(
        PlanValidator::validate(
            &mk(vec![agent_step("s1", &["nope"]), agent_step("s2", &[])]),
            100,
            60,
            1
        )
        .await
        .is_err(),
        "I: a prerequisite naming a step that does not exist stays refused"
    );

    // Cycle: still refused.
    assert!(
        PlanValidator::validate(
            &mk(vec![agent_step("a", &["b"]), agent_step("b", &["a"])]),
            100,
            60,
            1
        )
        .await
        .is_err(),
        "I: a cycle stays refused"
    );

    // And the gate that produces a `Blocked` status in the first place is unchanged.
    assert!(
        !TaskStatus::Failed.satisfies_dependents(),
        "I: a failed step still does not satisfy a dependent"
    );
    assert!(
        TaskStatus::Passed.satisfies_dependents(),
        "I: only a passed step does"
    );
    assert!(
        !TaskStatus::Blocked.satisfies_dependents(),
        "I: a blocked step is not itself a satisfied prerequisite"
    );
}
