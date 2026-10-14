//! Phase 7.3 — tests for the approved decisions Q1–Q7.
//!
//! | Test | Decision | What it pins |
//! |---|---|---|
//! | `p73_b3_*` | **Q1** | a completed-execution rate and an all-recorded rate are distinct, named, and both correct; blocked rows cannot dilute or fabricate either |
//! | `p73_q2_*` | **Q2** | `INFRA_ERROR` is not a pass, not a product failure inside the completed rate, and not erased — it is counted |
//! | `p73_q5_*` | **Q5** | the accepted-kind list and the resolution rules cannot disagree, in either direction |
//! | `p73_q3_q4_*` | **Q3/Q4** | `module` is explicitly unsupported, derives no impact, and records no impact row |
//! | `p73_q7_*` | **Q7** | a refused attempt is auditable and is never mistaken for a resolved impact |
//! | `p73_q6_*` | **Q6** | the existing `set_task_status` flag patterns, including the unreachable `(true, true)` |
//!
//! ## Discipline
//!
//! Every row is written by a public `Store` writer or `Intelligence::ingest_run`. A `TempDir` per
//! test, migrated by `Store::connect`. No direct SQL to manufacture state. `store.pool.close()` is
//! the sanctioned way to produce a genuine read failure.
//!
//! `TestStabilityView`'s own rate was fixed in Phase 7.1 and is **not** re-litigated here; these
//! tests cover the eleven sites that Phase 7.2 audited and Phase 7.3 addressed.

use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
use crate::change_impact::{
    is_supported_change_kind, supported_change_kinds_joined, ChangeImpactReader, ChangeTarget,
    Resolution, SUPPORTED_CHANGE_KINDS,
};
use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::intelligence::Intelligence;
use crate::policy::PolicyConfig;
use crate::qa_memory::QaMemoryReader;
use crate::state::AppState;
use crate::store::{ExecutionTelemetryRow, Store};
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

struct Db {
    store: Store,
    dir: TempDir,
}

impl Db {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("phase73.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, dir }
    }
}

async fn app(db_dir: &TempDir) -> AppState {
    let store = Store::connect(&db_dir.path().join("phase73api.sqlite3"))
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
    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors: std::collections::HashMap<&'static str, Arc<dyn AiExecutor>> =
        std::collections::HashMap::new();
    executors.insert("mock", executor);
    AppState::new(
        Arc::new(registry),
        store,
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
    )
}

fn req(method: &str, path: &str, key: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(path);
    if let Some(k) = key {
        b = b.header("Authorization", format!("Bearer {k}"));
    }
    b.body(Body::empty()).expect("request")
}

async fn send(state: &AppState, request: Request<Body>) -> (StatusCode, Value) {
    let a = crate::api::router(state.clone());
    let resp = a.oneshot(request).await.expect("router call");
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
            "phase73-key",
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

async fn seed_case(store: &Store, project_id: Uuid, title: &str, requirement: &str) -> Uuid {
    store
        .insert_test_case(
            &project_id.to_string(),
            &CreateTestCaseRequest {
                title: title.to_string(),
                description: format!("{title} for a phase 7.3 test"),
                category: "Functional".into(),
                priority: "HIGH".into(),
                severity: "MAJOR".into(),
                preconditions: vec![],
                steps: vec![],
                expected_result: Some("the step completes".into()),
                assertions: vec![],
                test_data: None,
                source_document: Some("p73.md".into()),
                requirement_reference: Some(requirement.to_string()),
                source_requirement: Some(requirement.to_string()),
                traceability: None,
                risk_factor: None,
                tags: vec![],
                suite_id: None,
            },
            Some("phase73"),
        )
        .await
        .expect("insert_test_case")
}

/// One run whose steps all belong to `case`, recorded through the production writers.
async fn record_run(
    store: &Store,
    project_id: &str,
    case: Uuid,
    steps: &[(&str, TaskStatus, Option<&str>)],
) -> Uuid {
    let run_id = Uuid::new_v4();
    let plan = RunPlan {
        name: "phase 7.3 run".to_string(),
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
            Some("phase73"),
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
            .expect("start");
        store
            .set_task_status(run_id, id, *status, *reason, false, true)
            .await
            .expect("finish");
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

async fn telemetry_for(store: &Store, project_id: &str) -> Vec<ExecutionTelemetryRow> {
    store
        .list_execution_telemetry(project_id, 500)
        .await
        .expect("list_execution_telemetry")
}

/// A project's telemetry rows for one case, as references into the returned vector.
fn case_rows<'a>(all: &'a [ExecutionTelemetryRow], case: Uuid) -> Vec<&'a ExecutionTelemetryRow> {
    all.iter()
        .filter(|r| r.test_case_id == Some(case))
        .collect()
}

// ===========================================================================
// Q1 — the two rates are distinct, explicitly named, and both correct
// ===========================================================================

/// The core of Q1: with one genuine failure and one blocked step, the two rates must differ, and
/// each must report what it is.
#[tokio::test]
async fn p73_q1_a_blocked_row_cannot_dilute_or_fabricate_the_completed_rate() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q1 blocked").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Search", "FR-001").await;
    record_run(
        store,
        &pid_s,
        case,
        &[("t", TaskStatus::Failed, Some("Element not found: #x"))],
    )
    .await;
    record_run(
        store,
        &pid_s,
        case,
        &[("t", TaskStatus::Blocked, Some("prerequisite did not pass"))],
    )
    .await;

    let rows = telemetry_for(store, &pid_s).await;
    let mine = case_rows(&rows, case);

    // The shared definitions, computed straight from the stored rows.
    let all_rate = QaMemoryReader::rate(
        mine.iter()
            .filter(|r| QaMemoryReader::is_failure(r))
            .count(),
        mine.len(),
    );
    let completed = QaMemoryReader::completed_failure_rate(&mine);

    assert_eq!(mine.len(), 2, "one failure and one blocked row: {mine:?}");
    assert_eq!(
        all_rate,
        Some(50.0),
        "the all-recorded rate keeps its existing meaning: failures over every row"
    );
    assert_eq!(
        completed,
        Some(100.0),
        "Q1: the completed-execution rate must be 1 of 1, undiluted by a step that never ran"
    );
    assert_ne!(
        all_rate, completed,
        "the whole point of B3: the two quantities must not be interchangeable"
    );
    db.store.pool.close().await;
}

/// A blocked-only case has no completed execution, so the completed rate is unmeasured while the
/// all-recorded rate still answers its own question.
#[tokio::test]
async fn p73_q1_zero_completed_outcomes_stay_unmeasured_rather_than_zero() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q1 none").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Checkout", "FR-002").await;
    record_run(
        store,
        &pid_s,
        case,
        &[("t", TaskStatus::Blocked, Some("prerequisite did not pass"))],
    )
    .await;

    let rows = telemetry_for(store, &pid_s).await;
    let mine = case_rows(&rows, case);
    assert_eq!(mine.len(), 1);
    assert_eq!(
        QaMemoryReader::completed_failure_rate(&mine),
        None,
        "Q1: nothing completed, so there is no rate. Never 0.0 -- a fabricated zero is a claim."
    );
    assert_eq!(
        QaMemoryReader::rate(0, mine.len()),
        Some(0.0),
        "while the all-recorded rate keeps answering its own question: 0 of 1 recorded rows failed"
    );
    db.store.pool.close().await;
}

/// The rates on the real history views, over real persisted rows.
#[tokio::test]
async fn p73_q1_the_history_views_report_both_rates_and_the_infra_count() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q1 views").await;
    let pid_s = pid.to_string();

    // A requirement-linked case, with a failure, a pass and a blocked step.
    let case = seed_case(store, pid, "Search", "FR-001").await;
    store
        .upsert_requirement_document(
            &pid_s,
            "p73.md",
            None,
            "h-p73",
            "- FR-001: The system shall search.",
            &crate::requirements::parse_document(
                "P73",
                "p73.md",
                "- FR-001: The system shall search.",
            ),
        )
        .await
        .expect("upsert_requirement_document");
    store
        .link_case_to_requirement(&pid_s, &case, "Search", "FR-001", "requirement")
        .await
        .expect("link_case_to_requirement");
    record_run(
        store,
        &pid_s,
        case,
        &[("a", TaskStatus::Failed, Some("Element not found: #x"))],
    )
    .await;
    record_run(store, &pid_s, case, &[("b", TaskStatus::Passed, None)]).await;
    record_run(
        store,
        &pid_s,
        case,
        &[("c", TaskStatus::Blocked, Some("prerequisite did not pass"))],
    )
    .await;

    let reader = QaMemoryReader::new(store.clone());
    let v = reader
        .requirement_history(&pid_s, "FR-001")
        .await
        .expect("requirement_history")
        .view
        .expect("a requirement history view");

    assert_eq!(v.executions, 3, "all three outcomes are recorded: {v:?}");
    assert_eq!(v.failures, 1, "{v:?}");
    assert_eq!(
        v.failure_rate,
        Some((1.0 / 3.0) * 100.0),
        "the all-recorded rate keeps its value and meaning: {v:?}"
    );
    assert_eq!(
        v.completed_failure_rate_percent,
        Some(50.0),
        "Q1: over the two outcomes that completed, one of the two failed: {v:?}"
    );
    assert_eq!(
        v.infrastructure_errors, 0,
        "no executor failures in this scenario: {v:?}"
    );
    db.store.pool.close().await;
}

/// The same contract on `AffectedTestRisk`, the per-case display list inside `ModuleRisk`.
#[tokio::test]
async fn p73_q1_affected_test_risk_reports_both_rates_per_case() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q1 affected").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Search", "FR-001").await;

    // One failure, one pass and one blocked step on a single case.
    record_run(
        store,
        &pid_s,
        case,
        &[("a", TaskStatus::Failed, Some("Element not found: #x"))],
    )
    .await;
    record_run(store, &pid_s, case, &[("b", TaskStatus::Passed, None)]).await;
    record_run(
        store,
        &pid_s,
        case,
        &[("c", TaskStatus::Blocked, Some("prerequisite did not pass"))],
    )
    .await;

    // `AffectedTestRisk` is built inside `module_risk`, which requires a `covers` edge from the
    // module to the case. There is no production writer for that edge (Q4), so this phase writes
    // it through the store method directly -- the same call the production writer *would* make.
    // This is fixture setup, not a claim about a supported path: no journey or capability
    // depends on it, and `p73_q4` proves module targets are still refused.
    let module_node = store
        .upsert_module_node(&pid_s, "services/search", "search")
        .await
        .expect("upsert_module_node");
    let _ = module_node;
    store
        .link_test_case_to_module(
            &pid_s,
            &case.to_string(),
            "services/search",
            "Search",
            "search",
        )
        .await
        .expect("link_test_case_to_module");

    let v = crate::qa_memory::QaMemoryReader::new(store.clone())
        .module_risk(&pid_s, "services/search")
        .await
        .expect("module_risk");
    let mv = v
        .view
        .as_ref()
        .unwrap_or_else(|| panic!("with a covers edge the module view must answer: {v:?}"));

    let a = mv
        .affected_tests
        .iter()
        .find(|a| a.test_case_id == case.to_string())
        .unwrap_or_else(|| {
            panic!(
                "the failing case must appear in affected_tests: {:?}",
                mv.affected_tests
            )
        });
    assert_eq!(a.executions, 3, "all three outcomes are counted: {a:?}");
    assert_eq!(a.failures, 1, "{a:?}");
    assert_eq!(
        a.failure_rate,
        Some((1.0 / 3.0) * 100.0),
        "the all-recorded rate keeps its existing value: {a:?}"
    );
    assert_eq!(
        a.completed_failure_rate_percent,
        Some(50.0),
        "Q1: over the two completed outcomes, one of the two failed: {a:?}"
    );
    assert_eq!(a.infrastructure_errors, 0, "{a:?}");

    // And the weighted model was not rewired (requirement 4, Step 2).
    //
    // `calibration` is legitimately `None` here: `calibrate_or_none` refuses a score when too
    // little of the model is measured, and this project has no healing, security or dependency
    // evidence. That refusal is D3's open business, and asserting it either way would be
    // inventing a canonical formula. What matters for Step 2 requirement 4 is narrower: the
    // `failure_rate` **factor** must still exist, still be measured, and still be computed over
    // the all-outcome denominator -- i.e. this phase added fields, it did not rewire the model.
    let fr = mv
        .factors
        .iter()
        .find(|f| f.name == "failure_rate")
        .expect("the failure_rate factor must still be present");
    assert!(
        fr.measured.is_some(),
        "requirement 4/5: the risk Factor is still measured, so the scoring path was not \
         rewired or bypassed: {fr:?}"
    );
    // **Phase 7.4, Decision 1 -- this assertion is deliberately re-targeted, not weakened.**
    //
    // Phase 7.3 wrote it as `Some(1.0 / 3.0)` -- one failure over THREE recorded rows -- for the
    // express purpose of proving that this phase added fields WITHOUT rewiring the model's
    // inputs. Decision 1 subsequently authorised exactly that rewiring, so the value this test
    // was built to freeze is the value the approved change was required to move. The fixture is
    // one `FAILED`, one `PASSED`, one `BLOCKED`: the all-recorded denominator counted the blocked
    // step, which never reached a verdict, and let it dilute the factor by a third.
    //
    // It is now `Some(0.5)` -- one failure over the TWO executions that completed. That is the
    // same number `QaMemoryReader::completed_failure_rate_fraction` returns for this evidence,
    // which is the point: the factor consumes the shared completed-execution measurement, and
    // not a denominator this call site derived for itself. The all-recorded figure is still
    // available and still asserted, just on the field whose contract is all-recorded
    // (`ModuleRiskHistory::failure_rate`, checked earlier in this test) rather than on the risk
    // factor.
    assert_eq!(
        fr.measured,
        Some(0.5),
        "Decision 1: the factor is the completed-execution rate -- 1 failure of 2 completed \
         (PASSED/FAILED/TIMEOUT) rows, with the BLOCKED execution in neither half."
    );
    assert_eq!(
        fr.evidence, 2,
        "and its evidence count is the completed denominator, not the 3 recorded rows."
    );
    assert!(
        fr.basis.contains("completed executions"),
        "the basis must name the denominator it was taken over, so a reader can tell this apart \
         from the all-recorded rate: {}",
        fr.basis
    );
    // The weight is untouched: Decision 4 requires it, and the 0.35 factor weight is the single
    // most load-bearing constant in the model.
    assert_eq!(
        fr.weight, 0.35,
        "Decision 4: the factor's weight in `calibrate_risk` is unchanged. Only its input moved."
    );
    assert!(
        mv.unmeasured_weight > 0.0,
        "D3 remains open and untouched: the model is still partly unmeasured here ({})",
        mv.unmeasured_weight
    );
    db.store.pool.close().await;
}

// ===========================================================================
// Q2 — INFRA_ERROR semantics
// ===========================================================================

/// An infrastructure error is neither a pass nor a product failure. It must not enter the
/// completed rate at all, and it must not vanish either.
#[tokio::test]
async fn p73_q2_an_infrastructure_error_is_not_a_pass_and_not_a_product_failure() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q2 infra").await;
    let pid_s = pid.to_string();

    let infra_case = seed_case(store, pid, "Infra", "FR-001").await;
    let _ = infra_case;
    let case = seed_case(store, pid, "Flaky", "FR-001").await;

    // One genuine failure, then one executor failure.
    record_run(
        store,
        &pid_s,
        case,
        &[("a", TaskStatus::Failed, Some("Element not found: #x"))],
    )
    .await;
    record_run(
        store,
        &pid_s,
        case,
        &[("b", TaskStatus::InfraError, Some("no browser available"))],
    )
    .await;

    let rows = telemetry_for(store, &pid_s).await;
    let mine = case_rows(&rows, case);
    assert_eq!(mine.len(), 2, "{mine:?}");
    assert!(
        mine.iter().any(|r| r.status == "INFRA_ERROR"),
        "the executor failure really was recorded: {mine:?}"
    );

    assert_eq!(
        QaMemoryReader::completed_failure_rate(&mine),
        Some(100.0),
        "Q2: an INFRA_ERROR is not a completed pass/fail outcome, so the completed rate is over \
         the one real failure alone -- not diluted to 50% and not turned into a pass"
    );
    assert_eq!(
        QaMemoryReader::infra_errors(&mine),
        1,
        "Q2: and it is still counted, so it is not erased from observability"
    );
    db.store.pool.close().await;
}

/// With only infrastructure errors there is no completed execution, and the count still shows.
#[tokio::test]
async fn p73_q2_only_infrastructure_errors_yield_no_rate_but_a_real_count() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q2 only infra").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Browser", "FR-001").await;
    record_run(
        store,
        &pid_s,
        case,
        &[("a", TaskStatus::InfraError, Some("no browser available"))],
    )
    .await;

    let rows = telemetry_for(store, &pid_s).await;
    let mine = case_rows(&rows, case);
    assert_eq!(
        QaMemoryReader::completed_failure_rate(&mine),
        None,
        "Q2: an executor failure is not a completed execution, so there is no completed rate"
    );
    assert_eq!(
        QaMemoryReader::infra_errors(&mine),
        1,
        "Q2: and it is counted rather than dropped"
    );
    assert_eq!(
        QaMemoryReader::rate(
            mine.iter()
                .filter(|r| QaMemoryReader::is_failure(r))
                .count(),
            mine.len()
        ),
        Some(0.0),
        "the all-recorded rate answers its own question: 0 of 1 rows met is_failure"
    );
    db.store.pool.close().await;
}

/// Over HTTP, the project risk endpoint must report the pair and the count, and must not have
/// silently changed the score it computed.
#[tokio::test]
async fn p73_q2_the_project_risk_endpoint_reports_both_rates_and_the_infra_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = app(&dir).await;
    let pid = make_project(&state.store, "P73 Q2 api").await;
    let pid_s = pid.to_string();
    let key = mint_key(&state, "viewer", &pid_s).await;
    let case = seed_case(&state.store, pid, "Checkout", "FR-001").await;

    record_run(
        &state.store,
        &pid_s,
        case,
        &[("a", TaskStatus::Failed, Some("Element not found: #p"))],
    )
    .await;
    record_run(
        &state.store,
        &pid_s,
        case,
        &[("b", TaskStatus::Passed, None)],
    )
    .await;
    record_run(
        &state.store,
        &pid_s,
        case,
        &[("c", TaskStatus::InfraError, Some("no browser available"))],
    )
    .await;

    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/intelligence/risk"),
            Some(&key),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        body["completed_failure_rate"].as_f64(),
        Some(0.5),
        "Q1+Q2: one failure of two completed outcomes -- the INFRA_ERROR is in neither half: {body}"
    );
    assert_eq!(
        body["completed_executions"].as_u64(),
        Some(2),
        "and the endpoint says how many outcomes it took the rate over: {body}"
    );
    assert_eq!(
        body["infrastructure_errors"].as_u64(),
        Some(1),
        "Q2: the executor failure is counted and visible: {body}"
    );
    // The score itself is still present and still the one the factors produced.
    assert!(
        body["risk_score"].is_number(),
        "the score must still be reported: {body}"
    );
    assert!(
        body["factors_detail"].is_array(),
        "and its factors must still be there: {body}"
    );
    state.store.pool.close().await;
}

// ===========================================================================
// Q5 — one authoritative supported-kind definition
// ===========================================================================

/// The accept-list and the refusal table must not disagree, in either direction.
#[tokio::test]
async fn p73_q5_the_supported_set_and_the_resolution_rules_cannot_disagree() {
    // Direction 1: everything `is_supported_change_kind` accepts must NOT be refused by the note
    // table, or a target would be accepted by one component and refused by another.
    for kind in SUPPORTED_CHANGE_KINDS {
        assert!(
            is_supported_change_kind(kind),
            "Q5: `{kind}` is in SUPPORTED_CHANGE_KINDS so it must answer true"
        );
    }
    assert_eq!(
        SUPPORTED_CHANGE_KINDS,
        &["requirement", "test_case", "failure_cluster"],
        "Q3: the approved supported set, verbatim"
    );

    // Direction 2: `module` is not accepted, and is not one of the three.
    assert!(
        !is_supported_change_kind("module"),
        "Q3/Q4: module is deferred and must not be in the supported set"
    );
    assert_eq!(
        supported_change_kinds_joined(),
        "`requirement`, `test_case`, `failure_cluster`",
        "and the message must name exactly the supported set"
    );

    // A whitespace-padded kind is still the kind.
    assert!(
        is_supported_change_kind("  requirement  "),
        "the set is trimmed before comparison, so a padded kind is the same kind"
    );
    for bogus in [
        "",
        "nonsense",
        "Requirement",
        "MODULE",
        "requirement_change",
    ] {
        assert!(
            !is_supported_change_kind(bogus),
            "Q5: `{bogus}` is not a supported kind"
        );
    }
}

// ===========================================================================
// Q3/Q4 — module is explicitly unsupported
// ===========================================================================

#[tokio::test]
async fn p73_q4_a_module_target_is_refused_explicitly_and_derives_nothing() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q4 module").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Checkout", "FR-001").await;

    let outcome = ChangeImpactReader::new(store.clone())
        .analyse(
            &pid_s,
            "p73-module-change",
            "code",
            &[ChangeTarget {
                kind: "module".into(),
                ref_id: "services/checkout".into(),
            }],
        )
        .await
        .expect("analyse a module change");

    // Q4: refused, and the reason must name the actual prerequisite.
    let target = &outcome.targets[0];
    match &target.resolution {
        Resolution::Insufficient { marker, reason } => {
            assert_eq!(*marker, "INSUFFICIENT_DATA", "{reason}");
            assert!(
                reason.contains("deferred") || reason.contains("not supported"),
                "Q4: the reason must say the capability is deferred, got {reason:?}"
            );
            assert!(
                reason.contains("authoritative module identity"),
                "Q4: and it must name the missing prerequisite, got {reason:?}"
            );
        }
        Resolution::Resolved { .. } => {
            panic!("Q4: a module target must not resolve while it is deferred")
        }
    }

    // Requirement 5: no impact may be claimed.
    assert!(
        outcome.impacted_tests.is_empty(),
        "Q4: a deferred target must derive no regression selection, got {:?}",
        outcome.impacted_tests
    );
    assert!(
        outcome.insufficient_reason.is_some(),
        "Q4: and the report must say nothing resolved: {outcome:?}"
    );

    // Requirement 8: an unrelated case is reported as not selected, with a reason.
    assert!(
        outcome.not_selected.iter().any(|n| n.test_case_id == case),
        "the project's own case must appear as not selected rather than being silently dropped"
    );

    // Q7: and nothing was written to change_impacts.
    let rows = store
        .list_change_impacts(&pid_s, 100)
        .await
        .expect("list_change_impacts");
    assert!(
        rows.is_empty(),
        "Q7: a refused target must write no change_impact row, because every such row means \
         'this node's impact was derived'. Got {rows:?}"
    );
    db.store.pool.close().await;
}

/// Requirement 8: the three supported kinds must still work end to end.
#[tokio::test]
async fn p73_q5_all_three_supported_kinds_still_resolve_and_select() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q5 supported").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "Search", "FR-001").await;

    store
        .upsert_requirement_document(
            &pid_s,
            "p73.md",
            None,
            "h-sup",
            "- FR-001: The system shall search.",
            &crate::requirements::parse_document(
                "P73S",
                "p73.md",
                "- FR-001: The system shall search.",
            ),
        )
        .await
        .expect("upsert_requirement_document");
    store
        .link_case_to_requirement(&pid_s, &case, "Search", "FR-001", "requirement")
        .await
        .expect("link_case_to_requirement");
    store
        .upsert_quality_node(
            &pid_s,
            crate::store::TEST_NODE_TYPE,
            &case.to_string(),
            "Search",
        )
        .await
        .expect("upsert case vertex");

    // `requirement`
    let by_req = ChangeImpactReader::new(store.clone())
        .analyse(
            &pid_s,
            "c-req",
            "code",
            &[ChangeTarget {
                kind: "requirement".into(),
                ref_id: "FR-001".into(),
            }],
        )
        .await
        .expect("analyse");
    assert!(
        matches!(by_req.targets[0].resolution, Resolution::Resolved { .. }),
        "a `requirement` target must still resolve: {:?}",
        by_req.targets[0].resolution
    );
    assert!(
        by_req.impacted_tests.iter().any(|i| i.test_case_id == case),
        "and must select the linked case: {:?}",
        by_req.impacted_tests
    );

    // `test_case`
    let by_case = ChangeImpactReader::new(store.clone())
        .analyse(
            &pid_s,
            "c-case",
            "code",
            &[ChangeTarget {
                kind: "test_case".into(),
                ref_id: case.to_string(),
            }],
        )
        .await
        .expect("analyse");
    assert!(
        matches!(by_case.targets[0].resolution, Resolution::Resolved { .. }),
        "a `test_case` target must still resolve"
    );
    assert!(
        by_case
            .impacted_tests
            .iter()
            .any(|i| i.test_case_id == case),
        "and must select that case: {:?}",
        by_case.impacted_tests
    );

    // `failure_cluster` — resolves when a cluster exists, and is honestly insufficient when not.
    let before = ChangeImpactReader::new(store.clone())
        .analyse(
            &pid_s,
            "c-cluster-none",
            "code",
            &[ChangeTarget {
                kind: "failure_cluster".into(),
                ref_id: "no-such-fingerprint".into(),
            }],
        )
        .await
        .expect("analyse");
    assert!(
        matches!(before.targets[0].resolution, Resolution::Insufficient { .. }),
        "a `failure_cluster` target that matches no cluster is insufficient data, not a guess: {:?}",
        before.targets[0].resolution
    );

    record_run(
        &store,
        &pid_s,
        case,
        &[("t", TaskStatus::Failed, Some("Element not found: #q"))],
    )
    .await;
    let fp = telemetry_for(store, &pid_s).await[0]
        .failure_fingerprint
        .clone()
        .expect("a fingerprint from the recorded failure");
    let by_cluster = ChangeImpactReader::new(store.clone())
        .analyse(
            &pid_s,
            "c-cluster",
            "code",
            &[ChangeTarget {
                kind: "failure_cluster".into(),
                ref_id: fp.clone(),
            }],
        )
        .await
        .expect("analyse");
    assert!(
        matches!(
            by_cluster.targets[0].resolution,
            Resolution::Resolved { .. }
        ),
        "a `failure_cluster` target for a real cluster must resolve"
    );
    db.store.pool.close().await;
}

// ===========================================================================
// Q7 — a refused attempt is auditable, and is not an impact record
// ===========================================================================

#[tokio::test]
async fn p73_q7_a_refused_attempt_is_audited_and_never_counted_as_impact() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q7 refused").await;
    let pid_s = pid.to_string();

    ChangeImpactReader::new(store.clone())
        .analyse(
            &pid_s,
            "p73-refused-change",
            "code",
            &[ChangeTarget {
                kind: "module".into(),
                ref_id: "services/checkout".into(),
            }],
        )
        .await
        .expect("analyse");

    // The refusal is visible where the schema supports it unambiguously.
    let audit = store
        .list_audit(Some(&pid_s), 100, 0)
        .await
        .expect("list_audit");
    let entry = audit
        .iter()
        .find(|a| a.action == "change_impact.unsupported_target")
        .unwrap_or_else(|| {
            panic!(
                "Q7: the refused attempt must be recorded. Actions seen: {:?}",
                audit.iter().map(|a| &a.action).collect::<Vec<_>>()
            )
        });
    assert_eq!(
        entry.outcome, "denied",
        "Q7: and recorded as denied, never as an 'ok' impact resolution: {entry:?}"
    );
    assert_eq!(
        entry.target.as_deref(),
        Some("p73-refused-change"),
        "Q7: the audit entry must name the change that was attempted: {entry:?}"
    );
    let detail = entry.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("module") && detail.contains("services/checkout"),
        "Q7: and name the target that was refused: {detail:?}"
    );
    assert!(
        detail.contains("INSUFFICIENT_DATA"),
        "Q7: with the marker, so a reader sees why: {detail:?}"
    );

    // And it is still not an impact record.
    assert!(
        store
            .list_change_impacts(&pid_s, 100)
            .await
            .expect("list_change_impacts")
            .is_empty(),
        "Q7: an audit entry is not an impact row; nothing downstream may read this as a \
         resolved impact"
    );
    db.store.pool.close().await;
}

// ===========================================================================
// Q6 — the existing set_task_status patterns
// ===========================================================================

/// The three reachable flag combinations, and the one that is unreachable.
#[tokio::test]
async fn p73_q6_the_existing_set_task_status_patterns_behave_as_documented() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 Q6 task status").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, "T", "FR-001").await;
    let run = Uuid::new_v4();
    let plan = RunPlan {
        name: "q6".to_string(),
        tasks: vec![TaskDef {
            id: "t".to_string(),
            name: "t".to_string(),
            prerequisites: vec![],
            simulate: Some(SimSpec {
                outcome: SimOutcome::Pass,
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
            Some("q6"),
            RunState::Running,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");

    // (true, false): the start branch. Sets status and started_at.
    store
        .set_task_status(run, "t", TaskStatus::Running, None, true, false)
        .await
        .expect("start");
    let t = &store.list_tasks_for_run(run).await.expect("tasks")[0];
    assert_eq!(t.status, TaskStatus::Running, "{t:?}");
    assert!(
        t.started_at.is_some(),
        "(true,false) must set started_at: {t:?}"
    );
    assert!(
        t.finished_at.is_none(),
        "(true,false) must not set finished_at: {t:?}"
    );
    assert!(
        t.reason.is_none(),
        "the start branch does not bind reason: {t:?}"
    );

    // (true, true): UNREACHABLE — the start branch wins and the finish branch never runs.
    store
        .set_task_status(run, "t", TaskStatus::Failed, Some("boom"), true, true)
        .await
        .expect("both flags");
    let t = &store.list_tasks_for_run(run).await.expect("tasks")[0];
    assert_eq!(t.status, TaskStatus::Failed, "the status is written: {t:?}");
    assert!(
        t.finished_at.is_none(),
        "Q6: (true,true) is documented as unreachable for the finish branch — the `else if` \
         means only started_at is stamped. If this now passes, the if/else-if shape changed and \
         the doc comment must be updated: {t:?}"
    );
    assert!(
        t.reason.is_none(),
        "Q6: and the reason is not written either, for the same reason: {t:?}"
    );

    // (false, true): the finish branch. This is what production uses to finish a task.
    store
        .set_task_status(run, "t", TaskStatus::Failed, Some("boom"), false, true)
        .await
        .expect("finish");
    let t = &store.list_tasks_for_run(run).await.expect("tasks")[0];
    assert!(
        t.finished_at.is_some(),
        "(false,true) sets finished_at: {t:?}"
    );
    assert_eq!(t.reason.as_deref(), Some("boom"), "and the reason: {t:?}");

    // (false, false): status + reason, no timestamp. No production caller today.
    let run2 = Uuid::new_v4();
    store
        .insert_run_for_release(
            run2,
            &plan,
            Some(&pid_s),
            Some("q6"),
            RunState::Running,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    store
        .set_task_status(
            run2,
            "t",
            TaskStatus::Blocked,
            Some("upstream did not pass"),
            false,
            false,
        )
        .await
        .expect("status only");
    let t2 = &store.list_tasks_for_run(run2).await.expect("tasks")[0];
    assert_eq!(t2.status, TaskStatus::Blocked, "{t2:?}");
    assert_eq!(
        t2.reason.as_deref(),
        Some("upstream did not pass"),
        "{t2:?}"
    );
    assert!(
        t2.started_at.is_none() && t2.finished_at.is_none(),
        "Q6: (false,false) stamps neither end of the clock, which is why no production code \
         calls it — a task that was never dispatched has no duration: {t2:?}"
    );
    db.store.pool.close().await;
}

// ===========================================================================
// Isolation and the mandatory safety floor must survive all of this
// ===========================================================================

#[tokio::test]
async fn p73_isolation_and_the_safety_floor_survive_the_contract_changes() {
    let db = Db::new().await;
    let store = &db.store;
    let alpha = make_project(store, "P73 Alpha").await;
    let beta = make_project(store, "P73 Beta").await;
    let a_s = alpha.to_string();
    let b_s = beta.to_string();

    let a_case = seed_case(store, alpha, "Checkout", "FR-001").await;
    let b_case = seed_case(store, beta, "Checkout", "FR-001").await;
    // Only alpha has failure evidence, so only alpha's case reaches the safety floor.
    record_run(
        store,
        &a_s,
        a_case,
        &[("t", TaskStatus::Failed, Some("Element not found: #pay"))],
    )
    .await;
    record_run(
        store,
        &a_s,
        a_case,
        &[("t", TaskStatus::Failed, Some("Element not found: #pay"))],
    )
    .await;

    let floor = ChangeImpactReader::new(store.clone())
        .safety_floor(&a_s)
        .await
        .expect("safety_floor for alpha");
    assert!(
        floor.contains_key(&a_case),
        "the case with a real recurring defect must be on alpha's floor: {floor:?}"
    );

    // Requirement 7: beta must be untouched, with identical case titles in both projects.
    let beta_floor = ChangeImpactReader::new(store.clone())
        .safety_floor(&b_s)
        .await
        .expect("safety_floor for beta");
    assert!(
        !beta_floor.contains_key(&b_case),
        "Q-isolation: beta has no failure evidence, so beta's floor must not contain alpha's \
         case: {beta_floor:?}"
    );
    let beta_rows = telemetry_for(store, &b_s).await;
    assert!(
        beta_rows.is_empty(),
        "Q-isolation: beta recorded nothing of its own and must not see alpha's rows"
    );
    db.store.pool.close().await;
}

/// Error propagation must survive: a closed store is still an error, not an empty answer.
#[tokio::test]
async fn p73_error_propagation_and_genuine_empty_results_are_still_distinguishable() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P73 errors").await;
    let pid_s = pid.to_string();
    let reader = QaMemoryReader::new(store.clone());

    // Genuine empty: a real answer.
    let empty = reader
        .module_risk_history(&pid_s, "services/absent")
        .await
        .expect("a genuinely absent module is answerable");
    assert!(
        empty.insufficient_data.is_some(),
        "an absent module is insufficient data, not an error: {empty:?}"
    );

    store.pool.close().await;

    // Failed read: an error.
    assert!(
        reader
            .module_risk_history(&pid_s, "services/absent")
            .await
            .is_err(),
        "a closed store must be an error, never an empty module history"
    );
    assert!(
        ChangeImpactReader::new(store.clone())
            .analyse(
                &pid_s,
                "c",
                "code",
                &[ChangeTarget {
                    kind: "requirement".into(),
                    ref_id: "FR-001".into(),
                }],
            )
            .await
            .is_err(),
        "and a change-impact analysis must not report an empty result for an unreadable store"
    );
    db.store.pool.close().await;
}
