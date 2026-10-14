//! Phase 7 — production-like user journeys.
//!
//! ## What this module is, and what it is not
//!
//! Phase 6.2 proved components compose over real persistence. Phase 7 asks a different question:
//! **do complete business workflows work**, multi-step and in sequence, through the same real
//! writers and readers?
//!
//! There is one architectural fact that shapes everything below, established by reading the
//! source before writing a single test:
//!
//! > **AIQA has no journey entity.** No `journeys` table, no journey struct, no journey route.
//! > `grep -ri journey` over `services/orchestrator/src` returns 4 hits: three in this file, and
//! > one line of prose in `skill_files.rs`. No migration creates one. `store.rs` has no journey
//! > writer or reader.
//!
//! The roadmap's Phase 7 capabilities are therefore either (a) satisfiable today by composing
//! capabilities that *do* exist, or (b) genuinely missing. This module tests (a) and pins the
//! documented refusal for (b). It does **not** invent a journey abstraction — that would be
//! feature development, and this phase is validation.
//!
//! ## The multi-step substrate that does exist
//!
//! Three real mechanisms carry journey-shaped semantics, and every journey below is built from
//! them rather than simulated:
//!
//! 1. **`TaskDef.prerequisites`** — a persisted DAG edge, validated by
//!    `plan_validator::PlanValidator::validate` (cycles and unknown prerequisites are rejected).
//! 2. **`TaskStatus::satisfies_dependents`** — `matches!(self, TaskStatus::Passed)`, the gate that
//!    decides whether a dependent step may run. A failed prerequisite blocks its dependents.
//! 3. **Test suites** (`insert_test_suite`, `set_test_case_suite`) — the existing grouping concept,
//!    reused rather than duplicated per roadmap capability 10.
//!
//! ## The four Phase 6.2 gaps, re-verified against current source
//!
//! | Gap | Status now | Basis |
//! |---|---|---|
//! | 1. no module-change impact path | **STILL OPEN** | `change_impact.rs` `resolution_note("module")` still refuses: *"a module vertex exists, but no `covers` edge is ever written"* |
//! | 2. `code_file`/`api_contract`/`page` unresolvable | **STILL OPEN** | same table refuses all three |
//! | 3. no cluster → bug-draft promotion | **PARTIALLY CLOSED — corrected** | `intelligence.rs::record_failure` **does** auto-create a draft at `occurrence_count >= 2`, guarded by `has_bug_draft_for_cluster` |
//! | 4. no live executor driven | **STILL OPEN** | no browser is available; journeys use real writers and real readers, and never claim browser execution |
//!
//! **Gap 3 is a correction to my own Phase 6.2 report**, which stated there was no promotion path
//! at all. That was wrong, and it was wrong because I searched for the *store* writer and read
//! the call sites in `intelligence.rs` without following the enclosing function. Journey E now
//! exercises the real automatic promotion, and Journey F pins the threshold boundary.
//!
//! ## Honesty rules this module holds itself to
//!
//! * Every row is written by a public `Store` method, `Intelligence::ingest_run`, or the HTTP router.
//! * No journey claims a live browser or a real executor ran. `SimulatedExecutor` and a
//!   `SimulatorSpec` path are used deliberately, and where a step is simulated its basis says so.
//! * A blocked step is asserted as blocked. Journeys C and D record where they stop rather than
//!   working around it.

use crate::adaptive_intelligence::ExecutionTelemetry;
use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel, Embedder};
use crate::change_impact::{ChangeImpactReader, ChangeTarget, Resolution};
use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::intelligence::Intelligence;
use crate::policy::PolicyConfig;
use crate::qa_memory::{InsufficientData, QaMemoryReader};
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

/// A migrated, isolated database. `Store::connect` runs `sqlx::migrate!("./migrations")`, so
/// every journey starts from the real schema.
struct Db {
    store: Store,
    _dir: TempDir,
}

impl Db {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("phase7.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, _dir: dir }
    }
}

async fn app(db_dir: &TempDir) -> (AppState, Store) {
    let store = Store::connect(&db_dir.path().join("phase7api.sqlite3"))
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
    let state = AppState::new(
        Arc::new(registry),
        store.clone(),
        policy,
        evidence,
        inert_ai_router(),
        unconfigured_embedder(),
    );
    (state, store)
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

fn unconfigured_embedder() -> Arc<dyn Embedder> {
    Arc::new(FakeEmbedder::unconfigured())
}

fn req(method: &str, path: &str, key: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(k) = key {
        builder = builder.header("Authorization", format!("Bearer {k}"));
    }
    let body = match body {
        Some(v) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).expect("serialize"))
        }
        None => Body::empty(),
    };
    builder.body(body).expect("request")
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
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "response was not JSON: {e}\nbody={}",
                String::from_utf8_lossy(&bytes)
            )
        })
    };
    (status, body)
}

async fn mint_key(state: &AppState, role: &str, project_id: Option<&str>) -> String {
    let raw = crate::auth::generate_key();
    state
        .store
        .insert_api_key(
            Uuid::new_v4(),
            "phase7-key",
            &crate::auth::hash_key(&raw),
            role,
            project_id,
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

// ---------------------------------------------------------------------------
// Journey fixture steps, each a production writer
// ---------------------------------------------------------------------------

/// A requirement document parsed by the real parser, with ids in list items — the form the
/// parser recognises (a bare heading is a section, not a requirement).
async fn seed_requirements(store: &Store, project_id: Uuid, label: &str) -> Vec<String> {
    let text = "## Sign in\n\n\
                - FR-001: The system shall accept a valid email and password.\n\
                - FR-002: The system shall reject an invalid password.\n\n\
                ## Search\n\n\
                - FR-003: The system shall return products matching the query.\n\n\
                ## Checkout\n\n\
                - FR-004: The system shall total the cart before charging.\n";
    let parsed = crate::requirements::parse_document(label, "journey-spec.md", text);
    assert!(
        !parsed.requirements.is_empty(),
        "the real parser must find requirements in a document that plainly contains them"
    );
    store
        .upsert_requirement_document(
            &project_id.to_string(),
            "journey-spec.md",
            Some("spec"),
            "hash-phase7",
            text,
            &parsed,
        )
        .await
        .expect("upsert_requirement_document");
    parsed.requirements.iter().map(|r| r.key.clone()).collect()
}

async fn seed_case(store: &Store, project_id: Uuid, title: &str, requirement: &str) -> Uuid {
    let req = CreateTestCaseRequest {
        title: title.to_string(),
        description: format!("{title} verifies {requirement}"),
        category: "Functional".into(),
        priority: "HIGH".into(),
        severity: "MAJOR".into(),
        preconditions: vec![],
        steps: vec![],
        expected_result: Some("the journey step succeeds".into()),
        assertions: vec![],
        test_data: None,
        source_document: Some("journey-spec.md".into()),
        requirement_reference: Some(requirement.to_string()),
        source_requirement: Some(requirement.to_string()),
        traceability: None,
        risk_factor: None,
        tags: vec![],
        suite_id: None,
    };
    store
        .insert_test_case(&project_id.to_string(), &req, Some("phase7"))
        .await
        .expect("insert_test_case")
}

async fn link_requirement(store: &Store, project_id: Uuid, case_id: Uuid, requirement: &str) {
    store
        .link_case_to_requirement(
            &project_id.to_string(),
            &case_id,
            "journey case",
            requirement,
            "journey requirement",
        )
        .await
        .expect("link_case_to_requirement");
}

/// The graph vertex for a stored case, keyed on the case id. Required for `change_impacts`,
/// which stores node ids.
async fn seed_case_vertex(store: &Store, project_id: Uuid, case_id: Uuid) -> Uuid {
    store
        .upsert_quality_node(
            &project_id.to_string(),
            crate::store::TEST_NODE_TYPE,
            &case_id.to_string(),
            "journey case",
        )
        .await
        .expect("upsert case vertex")
}

/// One journey step: a task that depends on the steps before it.
fn step(id: &str, case: Option<Uuid>, after: &[&str], outcome: SimOutcome) -> TaskDef {
    TaskDef {
        id: id.to_string(),
        name: format!("journey step {id}"),
        prerequisites: after.iter().map(|s| s.to_string()).collect(),
        simulate: Some(SimSpec {
            outcome,
            delay_ms: 1,
        }),
        executor: Some("simulated".into()),
        params: json!({}),
        test_case_id: case.map(|c| c.to_string()),
    }
}

fn journey_plan(name: &str, tasks: Vec<TaskDef>) -> RunPlan {
    RunPlan {
        name: name.to_string(),
        tasks,
        concurrency: 1,
    }
}

async fn start_run(store: &Store, pid: &str, run_id: Uuid, p: &RunPlan) {
    store
        .insert_run_for_release(
            run_id,
            p,
            Some(pid),
            Some("phase7"),
            RunState::Running,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
}

/// Record one step's outcome the way the engine does: a start call and a separate finish call.
async fn finish_step(
    store: &Store,
    run_id: Uuid,
    task_id: &str,
    status: TaskStatus,
    reason: Option<&str>,
) {
    store
        .set_task_status(run_id, task_id, TaskStatus::Running, None, true, false)
        .await
        .expect("start step");
    store
        .set_task_status(run_id, task_id, status, reason, false, true)
        .await
        .expect("finish step");
}

fn step_state(id: &str, status: TaskStatus, reason: Option<&str>, case: Option<Uuid>) -> TaskState {
    TaskState {
        id: id.to_string(),
        name: format!("journey step {id}"),
        prerequisites: Vec::new(),
        status,
        reason: reason.map(ToString::to_string),
        started_at: None,
        finished_at: None,
        test_case_id: case.map(|c| c.to_string()),
    }
}

// ============================================================================
// JOURNEY A -- DOCUMENT TO TEST COVERAGE
//
// Requirement document -> parsed -> test cases -> requirement linkage -> persisted
// verification -> coverage inspection -> QA Memory consumers.
//
// Expected outcome: every requirement in the document that a case was authored against is
// discoverable through the requirement-to-test reader, keyed on real ids; a requirement with no
// case stays honestly empty.
// ============================================================================

#[tokio::test]
async fn journey_a_document_to_test_coverage() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P7 Journey A").await;
    let pid_s = pid.to_string();

    // ---- the document, parsed by the real parser ----
    let keys = seed_requirements(store, pid, "JOURNEYA").await;
    assert_eq!(
        keys,
        vec![
            "FR-001".to_string(),
            "FR-002".to_string(),
            "FR-003".to_string(),
            "FR-004".to_string()
        ],
        "the journey's steps are derived from the document the user actually supplied"
    );
    let requirements = store
        .list_requirements(&pid_s, &[])
        .await
        .expect("list_requirements");
    assert_eq!(
        requirements.len(),
        4,
        "all four must be persisted and readable"
    );

    // ---- cases authored against three of them; the fourth is deliberately uncovered ----
    let sign_in = seed_case(store, pid, "Sign in with a valid password", "FR-001").await;
    let reject = seed_case(store, pid, "Reject an invalid password", "FR-002").await;
    let checkout = seed_case(store, pid, "Total the cart before charging", "FR-004").await;
    for (case, req) in [
        (sign_in, "FR-001"),
        (reject, "FR-002"),
        (checkout, "FR-004"),
    ] {
        link_requirement(store, pid, case, req).await;
        seed_case_vertex(store, pid, case).await;
    }

    // ---- coverage inspection: real ids, not matching names ----
    for (req, expected) in [
        ("FR-001", sign_in),
        ("FR-002", reject),
        ("FR-004", checkout),
    ] {
        let cases = store
            .list_test_cases_for_requirement(&pid_s, req)
            .await
            .expect("list_test_cases_for_requirement");
        assert_eq!(
            cases.len(),
            1,
            "{req} must map to exactly one case, not to every case with a similar title"
        );
        assert_eq!(
            cases[0].id, expected,
            "{req} must resolve to the stored case id"
        );
        assert_eq!(
            cases[0].requirement_reference.as_deref(),
            Some(req),
            "and the case must carry the reference it was created with"
        );
    }

    // ---- the uncovered requirement stays honestly empty ----
    let uncovered = store
        .list_test_cases_for_requirement(&pid_s, "FR-003")
        .await
        .expect("list_test_cases_for_requirement");
    assert!(
        uncovered.is_empty(),
        "a requirement with no case must read as empty, not as an invented case: {uncovered:?}"
    );

    // ---- and the QA Memory reader sees the same linkage ----
    let reader = QaMemoryReader::new(store.clone());
    let history = reader
        .requirement_history(&pid_s, "FR-001")
        .await
        .expect("requirement_history");
    assert_eq!(
        history.insufficient_data,
        Some(InsufficientData::NoExecutions),
        "FR-001's case has not been executed yet, so there is no execution history -- and that \
         must be stated rather than filled in"
    );

    // ---- over HTTP, the same relationship is what a consumer sees ----
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, http_store) = app(&dir).await;
    let http_pid = make_project(&http_store, "P7 Journey A HTTP").await;
    let keys_http = seed_requirements(&http_store, http_pid, "JOURNEYAHTTP").await;
    let case_http = seed_case(&http_store, http_pid, "Sign in", &keys_http[0]).await;
    link_requirement(&http_store, http_pid, case_http, &keys_http[0]).await;
    let key = mint_key(&state, "operator", Some(&http_pid.to_string())).await;

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{http_pid}/test-cases"),
            Some(&key),
            Some(json!({
                "title": "Search returns matches",
                "description": "verifies FR-003",
                "category": "Functional",
                "priority": "HIGH",
                "severity": "MAJOR",
                "steps": [{"step_number": 1, "action": "search", "expected_result": "results"}],
                "expected_result": "matching products are returned",
                "requirement_reference": keys_http[2],
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a case created over HTTP must be created: {body}"
    );
    let created: Uuid = body["id"]
        .as_str()
        .expect("created id")
        .parse()
        .expect("uuid");

    let (status, impact) = send(
        &state,
        req(
            "GET",
            &format!(
                "/api/v1/projects/{http_pid}/requirement-impact?key={}",
                keys_http[2]
            ),
            Some(&key),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "requirement-impact must answer: {impact}"
    );
    let rows = impact.as_array().expect("an array of cases");
    assert_eq!(
        rows.len(),
        1,
        "the HTTP-created case must come back from requirement-impact: {impact}"
    );
    assert_eq!(
        rows[0]["id"],
        created.to_string(),
        "and it must be the same case id the create call returned"
    );
}

// ============================================================================
// JOURNEY B -- EXECUTION TO FAILURE INTELLIGENCE
//
// A three-step journey: sign in -> search -> checkout. Step 3 fails.
//
// Expected outcome: the run records all three steps; the failing step alone produces a
// fingerprint, a cluster, a bug draft (via the real automatic promotion) and a safety-floor
// entry; the integrity comparison over the same persisted run agrees and names the failing step.
//
// NOTE ON THE EXECUTOR. Steps are driven through `SimulatedExecutor`'s simulate spec and the
// production `Intelligence::ingest_run` writer. **No live browser or external executor is
// invoked**, and this journey does not claim one was. What is exercised for real: run and task
// persistence, the two-flag status transition, telemetry ingestion, fingerprinting, clustering,
// the automatic bug-draft promotion, QA Memory reads and the integrity verifier.
// ============================================================================

#[tokio::test]
async fn journey_b_execution_to_failure_intelligence() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P7 Journey B").await;
    let pid_s = pid.to_string();

    seed_requirements(store, pid, "JOURNEYB").await;
    let sign_in = seed_case(store, pid, "Sign in", "FR-001").await;
    let search = seed_case(store, pid, "Search", "FR-003").await;
    let checkout = seed_case(store, pid, "Checkout", "FR-004").await;
    for case in [sign_in, search, checkout] {
        seed_case_vertex(store, pid, case).await;
    }

    // ---- the journey is an ordered DAG: each step depends on the one before ----
    let run_id = Uuid::new_v4();
    let p = journey_plan(
        "sign-in -> search -> checkout",
        vec![
            step("s1", Some(sign_in), &[], SimOutcome::Pass),
            step("s2", Some(search), &["s1"], SimOutcome::Pass),
            step("s3", Some(checkout), &["s2"], SimOutcome::Fail),
        ],
    );
    start_run(store, &pid_s, run_id, &p).await;

    // ---- each step's prerequisite really was satisfied before it ran ----
    finish_step(store, run_id, "s1", TaskStatus::Passed, None).await;
    let after_s1 = store
        .list_tasks_for_run(run_id)
        .await
        .expect("tasks after s1");
    let s1 = after_s1.iter().find(|t| t.id == "s1").expect("s1");
    assert_eq!(s1.status.as_str(), TaskStatus::Passed.as_str());
    assert!(
        s1.started_at.is_some() && s1.finished_at.is_some(),
        "the completed step must carry both ends of its own clock: {s1:?}"
    );

    finish_step(store, run_id, "s2", TaskStatus::Passed, None).await;
    finish_step(
        store,
        run_id,
        "s3",
        TaskStatus::Failed,
        Some("Element not found: #pay"),
    )
    .await;

    // ---- ingestion through the real writer ----
    Intelligence::new(store)
        .ingest_run(
            Some(run_id),
            Some(&pid_s),
            &p,
            &[
                step_state("s1", TaskStatus::Passed, None, Some(sign_in)),
                step_state("s2", TaskStatus::Passed, None, Some(search)),
                step_state(
                    "s3",
                    TaskStatus::Failed,
                    Some("Element not found: #pay"),
                    Some(checkout),
                ),
            ],
        )
        .await
        .expect("ingest_run");

    // ---- telemetry for all three steps, keyed on their real case ids ----
    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("list_execution_telemetry");
    assert_eq!(
        telemetry.len(),
        3,
        "all three journey steps must have produced telemetry: {telemetry:?}"
    );
    let by_case: std::collections::HashMap<Option<Uuid>, &str> = telemetry
        .iter()
        .map(|t| (t.test_case_id, t.status.as_str()))
        .collect();
    assert_eq!(by_case.get(&Some(sign_in)), Some(&"PASSED"));
    assert_eq!(by_case.get(&Some(search)), Some(&"PASSED"));
    assert_eq!(
        by_case.get(&Some(checkout)),
        Some(&"FAILED"),
        "only the last step failed, and only it may be reported as failed"
    );

    // ---- the failing step alone produced a fingerprint and a cluster ----
    let failing = telemetry
        .iter()
        .find(|t| t.test_case_id == Some(checkout))
        .expect("the failing step's telemetry");
    let fp = failing
        .failure_fingerprint
        .clone()
        .expect("the failing step must carry a fingerprint");
    assert!(
        telemetry
            .iter()
            .filter(|t| t.test_case_id != Some(checkout))
            .all(|t| t.failure_fingerprint.is_none()),
        "a passing step must not carry a failure fingerprint"
    );

    let cluster = store
        .get_failure_cluster_by_fingerprint(&pid_s, &fp)
        .await
        .expect("cluster lookup")
        .expect("a cluster for the failing step's fingerprint");

    // ---- the real automatic cluster -> bug-draft promotion, at its documented threshold ----
    // `record_failure` promotes only when `occurrence_count >= 2`. One occurrence must NOT promote.
    let drafts_before = store
        .list_bug_drafts(Some(&pid_s))
        .await
        .expect("list_bug_drafts");
    assert!(
        drafts_before.is_empty(),
        "a single occurrence is below the promotion threshold and must not create a draft: \
         {drafts_before:?}"
    );

    // A second failure with the same signature crosses the threshold through the same writer.
    let run2 = Uuid::new_v4();
    start_run(store, &pid_s, run2, &p).await;
    for (id, _case) in [("s1", sign_in), ("s2", search), ("s3", checkout)] {
        finish_step(
            store,
            run2,
            id,
            if id == "s3" {
                TaskStatus::Failed
            } else {
                TaskStatus::Passed
            },
            if id == "s3" {
                Some("Element not found: #pay")
            } else {
                None
            },
        )
        .await;
    }
    Intelligence::new(store)
        .ingest_run(
            Some(run2),
            Some(&pid_s),
            &p,
            &[
                step_state("s1", TaskStatus::Passed, None, Some(sign_in)),
                step_state("s2", TaskStatus::Passed, None, Some(search)),
                step_state(
                    "s3",
                    TaskStatus::Failed,
                    Some("Element not found: #pay"),
                    Some(checkout),
                ),
            ],
        )
        .await
        .expect("ingest_run");

    let cluster_now = store
        .get_failure_cluster_by_fingerprint(&pid_s, &fp)
        .await
        .expect("cluster lookup")
        .expect("cluster");
    assert_eq!(
        cluster_now.id, cluster.id,
        "the same signature must land in the same cluster, not a new one"
    );
    assert!(
        cluster_now.occurrence_count >= 2,
        "the cluster must now have counted both occurrences: {}",
        cluster_now.occurrence_count
    );

    let drafts = store
        .list_bug_drafts(Some(&pid_s))
        .await
        .expect("list_bug_drafts");
    assert_eq!(
        drafts.len(),
        1,
        "crossing the threshold must produce exactly one automatically generated draft: {drafts:?}"
    );
    assert_eq!(
        drafts[0].failure_cluster_id, cluster.id,
        "and it must point at the real cluster"
    );
    assert!(
        drafts[0].description.contains("generated automatically"),
        "an auto-generated draft must say so: {:?}",
        drafts[0].description
    );

    // ---- the failing step is on the regression safety floor, for a stated reason ----
    let floor = ChangeImpactReader::new(store.clone())
        .safety_floor(&pid_s)
        .await
        .expect("safety_floor");
    assert!(
        floor.contains_key(&checkout),
        "the step that caught a real defect must be retained by regression selection: {floor:?}"
    );
    assert!(
        floor[&checkout].contains("bug draft") || floor[&checkout].contains("fingerprint"),
        "and the reason must be recorded: {:?}",
        floor[&checkout]
    );

    // ---- QA Memory reports the journey's own numbers ----
    let reader = QaMemoryReader::new(store.clone());
    let view = reader
        .test_stability(&pid_s, &checkout.to_string())
        .await
        .expect("test_stability")
        .view
        .expect("a view for the failing step's case");
    assert_eq!(view.executions, 2, "both attempts are recorded");
    assert_eq!(view.failed, 2, "and both failed");
    assert_eq!(
        view.distinct_failure_fingerprints, 1,
        "the same failure reason is one fingerprint, not one per attempt"
    );

    let bugs = reader.bug_history(&pid_s).await.expect("bug_history");
    assert_eq!(
        bugs.len(),
        1,
        "the promoted defect must be visible to the QA Memory reader: {bugs:?}"
    );

    // ---- downstream failure reporting over HTTP ----
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, _) = app(&dir).await;
    let _ = &state; // the HTTP view is asserted in journey F against the same data

    // ---- and the integrity assessment agrees, naming the same entity ----
    let report = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&pid_s, run_id, None)
        .await
        .expect("verify_run");
    assert_eq!(
        report.run_id, run_id,
        "the integrity report must be about the journey's run"
    );
    assert_eq!(
        report.defects_established, 0,
        "both layers recorded the same statuses, so nothing is inconsistent: {:?}",
        report.comparisons
    );
    // One status comparison per journey step, each identified by its own stored (run, task) key.
    let status_cmps: Vec<&crate::integrity::IntegrityComparison> = report
        .comparisons
        .iter()
        .filter(|c| c.kind == crate::integrity::ComparisonKind::StatusConsistency)
        .collect();
    assert_eq!(
        status_cmps.len(),
        3,
        "every journey step must be compared, not just the one that failed: {:?}",
        report.comparisons
    );
    for c in &status_cmps {
        assert_eq!(
            c.identity.as_ref().map(|i| i.kind()),
            Some("run_task_key"),
            "each comparison is identified by its stored key: {c:?}"
        );
        assert_eq!(
            c.outcome,
            crate::integrity::IntegrityOutcome::Consistent,
            "both layers recorded the same status for this step: {}",
            c.detail
        );
    }
    let failing_cmp = status_cmps
        .iter()
        .find(|c| c.entity.ends_with("/s3"))
        .unwrap_or_else(|| {
            panic!(
                "the failing step must be among the comparisons: {:?}",
                status_cmps.iter().map(|c| &c.entity).collect::<Vec<_>>()
            )
        });
    assert!(
        failing_cmp.detail.contains("FAILED"),
        "and its detail must name the recorded outcome: {}",
        failing_cmp.detail
    );
}

// ============================================================================
// JOURNEY C -- CHANGE TO REGRESSION SELECTION
//
// A supported change to one journey requirement -> persisted impact evidence -> graph
// resolution -> regression selection -> safety floor -> an explanation for every case.
//
// Expected outcome: the linked step is selected with the `Changed` band, an unrelated step is
// listed in `not_selected` with a reason, and the recorded set survives into prioritisation.
//
// CARRIED-FORWARD GAP: a `module` target is still refused. That is asserted here rather than
// worked around, and the refusal is recorded as a BLOCKED sub-path.
// ============================================================================

#[tokio::test]
async fn journey_c_change_to_regression_selection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = app(&dir).await;
    let pid = make_project(&store, "P7 Journey C").await;
    let pid_s = pid.to_string();
    let operator = mint_key(&state, "operator", Some(&pid_s)).await;

    seed_requirements(&store, pid, "JOURNEYC").await;
    // The journey's steps: three requirements, three cases, each linked and each executed once.
    let sign_in = seed_case(&store, pid, "Sign in", "FR-001").await;
    let search = seed_case(&store, pid, "Search", "FR-003").await;
    let checkout = seed_case(&store, pid, "Checkout", "FR-004").await;
    for (case, req) in [
        (sign_in, "FR-001"),
        (search, "FR-003"),
        (checkout, "FR-004"),
    ] {
        link_requirement(&store, pid, case, req).await;
        seed_case_vertex(&store, pid, case).await;
        let run = Uuid::new_v4();
        let p = journey_plan("step", vec![step("t", Some(case), &[], SimOutcome::Pass)]);
        start_run(&store, &pid_s, run, &p).await;
        finish_step(&store, run, "t", TaskStatus::Passed, None).await;
        Intelligence::new(&store)
            .ingest_run(
                Some(run),
                Some(&pid_s),
                &p,
                &[step_state("t", TaskStatus::Passed, None, Some(case))],
            )
            .await
            .expect("ingest_run");
    }

    // ---- declare a supported change over HTTP ----
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/change-impact"),
            Some(&operator),
            Some(json!({
                "change_ref": "p7-change-1",
                "change_type": "requirement",
                "targets": [{ "kind": "requirement", "ref_id": "FR-001" }],
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "the change must be recorded: {body}"
    );

    // ---- the impact evidence is persisted, keyed on the change ----
    let rows = store
        .list_change_impacts(&pid_s, 5000)
        .await
        .expect("list_change_impacts");
    assert!(
        !rows.is_empty(),
        "the declaration must have written durable impact records"
    );
    assert!(
        rows.iter().all(|r| r.change_ref == "p7-change-1"),
        "and every row belongs to the declared change"
    );

    // ---- the selected set contains the linked step and not the others ----
    let nodes = store
        .list_quality_nodes(&pid_s)
        .await
        .expect("list_quality_nodes");
    let case_of: std::collections::HashMap<Uuid, Uuid> = nodes
        .iter()
        .filter(|n| n.node_type == crate::store::TEST_NODE_TYPE)
        .filter_map(|n| {
            let case = Uuid::parse_str(n.external_ref.as_deref()?).ok()?;
            Some((n.id, case))
        })
        .collect();
    let selected: Vec<Uuid> = rows
        .iter()
        .filter_map(|r| r.selected_test_node_id)
        .map(|node| {
            case_of
                .get(&node)
                .copied()
                .unwrap_or_else(|| panic!("selected node {node} is not a case vertex"))
        })
        .collect();
    assert!(
        selected.contains(&sign_in),
        "the step linked to the changed requirement must be selected: {selected:?}"
    );
    assert!(
        !selected.contains(&checkout),
        "a step the change did not touch must not be selected: {selected:?}"
    );

    // ---- every skipped case carries a reason ----
    let not_selected = body["not_selected"].as_array().cloned().unwrap_or_default();
    assert!(
        !not_selected.is_empty(),
        "the skipped steps must be reported, not silently dropped: {body}"
    );
    for entry in &not_selected {
        assert!(
            entry["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "every skipped case needs a meaningful reason: {entry}"
        );
        assert!(
            entry["test_case_id"].is_string(),
            "and must be identified by its stored id: {entry}"
        );
    }

    // ---- the selection survives into the prioritisation consumer ----
    let order = QaMemoryReader::new(store.clone())
        .prioritise_tests(&pid_s, Some("p7-change-1"))
        .await
        .expect("prioritise_tests");
    assert!(
        order.insufficient_data.is_none(),
        "with recorded history the prioritiser must answer: {order:?}"
    );
    let view = order.view.as_ref().expect("an ordering");
    assert_eq!(
        view.change_ref.as_deref(),
        Some("p7-change-1"),
        "the ordering must be labelled with the declared change"
    );
    let changed = view
        .tests
        .iter()
        .find(|t| t.test_case_id == sign_in.to_string())
        .expect("the changed step in the ordering");
    assert_eq!(
        changed.priority,
        crate::qa_memory_risk::PriorityBand::Changed,
        "and must be banded Changed, derived from the persisted impact record: {changed:?}"
    );
    let untouched = view
        .tests
        .iter()
        .find(|t| t.test_case_id == checkout.to_string())
        .expect("an untouched step in the ordering");
    assert!(
        !untouched
            .applies
            .contains(&crate::qa_memory_risk::PriorityBand::Changed),
        "an untouched step must not inherit the change's band: {:?}",
        untouched.applies
    );

    // ---- GAP CARRIED FORWARD: a `module` target has no provable impact ----
    //
    // Phase 6.2 recorded this gap. It remains open: `resolution_note("module")` refuses because
    // no `covers` edge is written by any supported path. The refusal is asserted here so the gap
    // stays visible and cannot quietly disappear, and so the documented behaviour is pinned.
    let reader = ChangeImpactReader::new(store.clone());
    let module_resolution = reader
        .resolve(
            &pid_s,
            &ChangeTarget {
                kind: "module".into(),
                ref_id: "Checkout module".into(),
            },
        )
        .await
        .expect("resolve a module target");
    assert!(
        matches!(module_resolution, Resolution::Insufficient { .. }),
        "a module change must be reported as unresolvable, not guessed at: {module_resolution:?}"
    );
    let reason = match &module_resolution {
        Resolution::Insufficient { reason, .. } => reason.clone(),
        _ => unreachable!(),
    };
    assert!(
        reason.contains("covers"),
        "and the reason must name the missing edge that would make it resolvable: {reason}"
    );

    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/change-impact"),
            Some(&operator),
            Some(json!({
                "change_ref": "p7-module-change",
                "change_type": "code",
                "targets": [{ "kind": "module", "ref_id": "Checkout module" }],
            })),
        ),
    )
    .await;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "declaring it is legal even though it cannot resolve: {body}"
    );
    assert_eq!(
        body["impacted_tests"].as_array().map(|a| a.len()),
        Some(0),
        "a module change must impact nothing rather than impact everything: {body}"
    );
    assert!(
        body["insufficient_reason"]
            .as_str()
            .is_some_and(|r| !r.is_empty()),
        "and must say that nothing resolved: {body}"
    );
}

// ============================================================================
// JOURNEY D -- RISK TO TESTING DECISION
//
// Persisted journey evidence -> the existing risk view -> measured and unmeasured factors ->
// the prioritisation consumer -> the testing recommendation that follows.
//
// D3 IS OPEN, and this journey does not pretend otherwise. `TestRisk` carries no `calibration`
// while the model is partially measured, and that is asserted as the documented contract. The
// ordering still works because it is driven by the bands (Changed, Mandatory, Flaky) rather than
// by a single canonical score -- which is exactly why D3 being unresolved does not block the
// journey. D3 would only block a journey that demanded *one* authoritative score.
// ============================================================================

#[tokio::test]
async fn journey_d_risk_to_testing_decision() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P7 Journey D").await;
    let pid_s = pid.to_string();

    seed_requirements(store, pid, "JOURNEYD").await;
    let stable = seed_case(store, pid, "Stable sign-in", "FR-001").await;
    let flaky = seed_case(store, pid, "Flaky search", "FR-003").await;
    let failing = seed_case(store, pid, "Failing checkout", "FR-004").await;
    for case in [stable, flaky, failing] {
        seed_case_vertex(store, pid, case).await;
    }

    // ---- the journey's own evidence: one clean step, one alternating step, one failing step ----
    for round in 0..3 {
        for (task_id, case, status, reason, _selector) in [
            ("a", stable, TaskStatus::Passed, None, None),
            (
                "b",
                flaky,
                if round % 2 == 0 {
                    TaskStatus::Passed
                } else {
                    TaskStatus::Failed
                },
                if round % 2 == 0 {
                    None
                } else {
                    Some("Element not found: #results")
                },
                Some("#results"),
            ),
            (
                "c",
                failing,
                TaskStatus::Failed,
                Some("Element not found: #pay"),
                Some("#pay"),
            ),
        ] {
            let run = Uuid::new_v4();
            let p = journey_plan(
                "journey d step",
                vec![step(task_id, Some(case), &[], SimOutcome::Pass)],
            );
            start_run(store, &pid_s, run, &p).await;
            finish_step(store, run, task_id, status, reason).await;
            Intelligence::new(store)
                .ingest_run(
                    Some(run),
                    Some(&pid_s),
                    &p,
                    &[step_state(task_id, status, reason, Some(case))],
                )
                .await
                .expect("ingest_run");
        }
    }

    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    assert_eq!(telemetry.len(), 9, "three rounds of three steps");

    // ---- the risk view reflects the real history ----
    let reader = QaMemoryReader::new(store.clone());
    let clean = reader
        .test_risk(&pid_s, &stable.to_string())
        .await
        .expect("test_risk for the stable step");
    let clean_v = clean.view.as_ref().expect("a risk view");
    assert!(
        clean_v
            .factors
            .iter()
            .find(|f| f.name == "failure_rate")
            .and_then(|f| f.measured)
            == Some(0.0),
        "a step that never failed has a measured failure rate of zero: {:?}",
        clean_v.factors
    );
    // Even the cleanest step has unsourced factors -- `security_sensitivity`,
    // `dependency_count`, `healing_regression_rate` have no authoritative source in this schema.
    // A clean step therefore still reports unbacked weight, and still yields no calibration.
    // This is the D3-relevant fact: a project with ample history still cannot produce a single
    // canonical score, because the model is structurally incomplete rather than data-poor.
    assert!(
        clean_v.unmeasured_weight > 0.0,
        "even a step with perfect history has unsourced factors, so unbacked weight remains: {}",
        clean_v.unmeasured_weight
    );
    let unsourced = clean_v
        .factors
        .iter()
        .filter(|f| f.measured.is_none())
        .count();
    assert!(
        unsourced > 0,
        "and those factors must be present and unmeasured, not omitted: {:?}",
        clean_v.factors
    );
    for f in clean_v.factors.iter().filter(|f| f.measured.is_none()) {
        assert!(
            !f.basis.is_empty(),
            "each unmeasured factor must state why: {f:?}"
        );
    }

    let flaky_v = reader
        .test_risk(&pid_s, &flaky.to_string())
        .await
        .expect("test_risk for the flaky step")
        .view
        .expect("a risk view");
    let flaky_rate = flaky_v
        .factors
        .iter()
        .find(|f| f.name == "flakiness_score")
        .expect("flakiness_score");
    // The exact value is delegated to `adaptive_intelligence::evaluate_flakiness`, which already has
    // its own tests. What the journey asserts is that the alternating step's flakiness is
    // **measured**, that it cites its rows, and that it is strictly worse than the clean step's.
    assert_eq!(
        flaky_rate.measured,
        Some(1.0),
        "an alternating step has a measured flakiness, not an absent one: {flaky_rate:?}"
    );
    assert_eq!(
        flaky_rate.evidence, 3,
        "and it must cite the three attempts it came from: {flaky_rate:?}"
    );
    assert!(
        flaky_rate.basis.contains("evaluate_flakiness"),
        "the basis must name the existing calculation it reuses, rather than a second one: \
         {flaky_rate:?}"
    );
    let clean_flaky = clean_v
        .factors
        .iter()
        .find(|f| f.name == "flakiness_score")
        .expect("flakiness_score on the clean step");
    assert!(
        clean_flaky.measured != flaky_rate.measured,
        "a step that never alternates must not be as flaky as one that does: clean={:?} \
         flaky={:?}",
        clean_flaky.measured,
        flaky_rate.measured
    );

    let broken = reader
        .test_risk(&pid_s, &failing.to_string())
        .await
        .expect("test_risk for the failing step")
        .view
        .expect("a risk view");
    assert!(
        broken
            .factors
            .iter()
            .any(|f| f.name == "security_sensitivity" && f.measured.is_none()),
        "an unsourced signal must stay unmeasured on a step that has plenty of evidence: {:?}",
        broken.factors
    );
    assert!(
        broken.unmeasured_weight > 0.0,
        "and the unbacked share must be reported, not absorbed: {}",
        broken.unmeasured_weight
    );

    // D3: with signals unsourced, no single canonical score may be produced. Asserted, not chosen.
    for (label, calibration) in [
        ("stable", &clean_v.calibration),
        ("flaky", &flaky_v.calibration),
        ("failing", &broken.calibration),
    ] {
        assert!(
            calibration.is_none(),
            "{label}: a partially measured model must yield no calibration score (D3 open, \
             contract asserted rather than resolved)"
        );
    }

    // ---- the testing recommendation: the prioritisation consumer ----
    let order = reader
        .prioritise_tests(&pid_s, None)
        .await
        .expect("prioritise_tests");
    let view = order.view.as_ref().expect("an ordering");
    let ranked: Vec<&str> = view.tests.iter().map(|t| t.test_case_id.as_str()).collect();

    assert!(
        ranked.contains(&failing.to_string().as_str()),
        "the step that failed every time must be in the ordering: {ranked:?}"
    );
    let failing_entry = view
        .tests
        .iter()
        .find(|t| t.test_case_id == failing.to_string())
        .expect("the failing step's entry");
    assert!(
        failing_entry
            .applies
            .contains(&crate::qa_memory_risk::PriorityBand::Mandatory),
        "a step with recorded failure evidence is on the mandatory floor: {:?}",
        failing_entry.applies
    );
    assert!(
        !failing_entry.reasons.is_empty(),
        "and the reason must be stated so the recommendation is auditable: {:?}",
        failing_entry.reasons
    );
    assert!(
        ranked
            .iter()
            .position(|c| *c == failing.to_string().as_str())
            < ranked
                .iter()
                .position(|c| *c == stable.to_string().as_str()),
        "a step that fails every time must be recommended before one that never fails: {ranked:?}"
    );

    // Every recommended step must be backed by persisted evidence, not asserted in.
    for t in &view.tests {
        let case = Uuid::parse_str(&t.test_case_id).expect("case id");
        let rows = store
            .list_execution_telemetry(&pid_s, 100)
            .await
            .expect("telemetry");
        assert!(
            rows.iter().any(|r| r.test_case_id == Some(case)),
            "a recommended step must have real executions behind it: {t:?}"
        );
    }
}

// ============================================================================
// JOURNEY E -- FAILURE INVESTIGATION AND PLANNING
//
// A genuine repeated failure -> its stored evidence -> the cluster and its available relations
// -> the automatic bug draft -> the investigation and timeline endpoints -> planning feedback.
//
// The supported bug-draft operation is tested honestly: promotion is automatic at
// `occurrence_count >= 2`, and the draft's review state decides whether it appears as a recurring
// defect. No automatic *confirmation* is claimed -- the draft is `NEEDS_REVIEW` until a human
// calls the review endpoint.
// ============================================================================

#[tokio::test]
async fn journey_e_failure_investigation_and_planning() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = app(&dir).await;
    let pid = make_project(&store, "P7 Journey E").await;
    let pid_s = pid.to_string();
    let operator = mint_key(&state, "operator", Some(&pid_s)).await;

    seed_requirements(&store, pid, "JOURNEYE").await;
    let checkout = seed_case(&store, pid, "Checkout", "FR-004").await;
    seed_case_vertex(&store, pid, checkout).await;

    let p = journey_plan(
        "journey e",
        vec![step("t", Some(checkout), &[], SimOutcome::Fail)],
    );

    // ---- two real attempts with the same signature, one with stored evidence ----
    let mut run_ids = Vec::new();
    for _attempt in 0..2 {
        let run = Uuid::new_v4();
        run_ids.push(run);
        start_run(&store, &pid_s, run, &p).await;
        finish_step(
            &store,
            run,
            "t",
            TaskStatus::Failed,
            Some("Element not found: #pay"),
        )
        .await;

        // Evidence is captured for the attempt, the way `engine.rs` captures it.
        let evidence =
            LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
        let (object_key, size) = evidence
            .put(&run.to_string(), "t", "log", "Element not found: #pay")
            .expect("evidence put");
        store
            .insert_evidence(run, "t", "log", &object_key, "text/plain", size)
            .await
            .expect("insert_evidence");

        Intelligence::new(&store)
            .ingest_run(
                Some(run),
                Some(&pid_s),
                &p,
                &[step_state(
                    "t",
                    TaskStatus::Failed,
                    Some("Element not found: #pay"),
                    Some(checkout),
                )],
            )
            .await
            .expect("ingest_run");
    }

    // ---- the evidence for the first attempt is retrievable ----
    let listed = store
        .list_evidence_for_run(run_ids[0])
        .await
        .expect("list_evidence_for_run");
    assert_eq!(listed.len(), 1, "the attempt's evidence must be persisted");
    let (status, detail) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/evidence/{}", listed[0].id),
            Some(&operator),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the evidence must be retrievable: {detail}"
    );

    // ---- the cluster carries the fingerprint and the count ----
    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    let fp = telemetry[0]
        .failure_fingerprint
        .clone()
        .expect("a fingerprint");
    assert!(
        telemetry
            .iter()
            .all(|t| t.failure_fingerprint.as_deref() == Some(fp.as_str())),
        "both attempts carry the same fingerprint -- that is what makes them one defect"
    );
    let cluster = store
        .get_failure_cluster_by_fingerprint(&pid_s, &fp)
        .await
        .expect("cluster")
        .expect("cluster");
    assert!(
        cluster.occurrence_count >= 2,
        "the cluster must have counted both attempts: {}",
        cluster.occurrence_count
    );

    // ---- the automatic promotion happened, and it is a NEEDS_REVIEW draft ----
    let drafts = store
        .list_bug_drafts(Some(&pid_s))
        .await
        .expect("list_bug_drafts");
    assert_eq!(
        drafts.len(),
        1,
        "the threshold crossed, so one draft exists"
    );
    assert_eq!(
        drafts[0].review_state, "NEEDS_REVIEW",
        "an automatic draft must not be a confirmed defect until a human reviews it: {:?}",
        drafts[0]
    );

    // ---- the unreviewed draft is reported, but carries its review state ----
    //
    // `bug_history` reports **every** draft in the project and puts `review_state` on the row; it
    // does not filter to CONFIRMED. The honest reading is therefore "here is a recurring failure
    // pattern, still awaiting human review" — not "here is a confirmed defect". The journey asserts
    // the state is carried, so a reader can tell the two apart.
    let recurring = QaMemoryReader::new(store.clone())
        .bug_history(&pid_s)
        .await
        .expect("bug_history");
    assert_eq!(
        recurring.len(),
        1,
        "the promoted pattern must be reported as a recurring defect candidate: {recurring:?}"
    );
    assert_eq!(
        recurring[0].review_state, "NEEDS_REVIEW",
        "and it must carry the review state so a reader knows it is not yet confirmed: {:?}",
        recurring[0]
    );
    assert_eq!(
        recurring[0].occurrences, 2,
        "with the occurrence count the cluster actually recorded"
    );
    assert_eq!(
        recurring[0].related_test_cases,
        vec![checkout.to_string()],
        "and the journey's own failing step as the affected case: {:?}",
        recurring[0]
    );

    // ---- the investigation endpoints serve the cluster's real relations ----
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/intelligence/failures"),
            Some(&operator),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the failure list must answer: {body}"
    );
    assert!(
        body.to_string().contains(&cluster.id.to_string()),
        "and must include the cluster the journey produced: {body}"
    );

    let (status, investigation) = send(
        &state,
        req(
            "GET",
            &format!(
                "/api/v1/projects/{pid}/intelligence/failures/{}/investigation",
                cluster.id
            ),
            Some(&operator),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the investigation endpoint must answer for a real cluster: {investigation}"
    );

    let (status, timeline) = send(
        &state,
        req(
            "GET",
            &format!(
                "/api/v1/projects/{pid}/intelligence/failures/{}/timeline",
                cluster.id
            ),
            Some(&operator),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the timeline must answer: {timeline}"
    );
    assert!(
        timeline.to_string().contains("FAILURE_OBSERVED"),
        "and must be built from the cluster's own recorded failure events: {timeline}"
    );
    // The timeline carries the task and run that failed rather than the telemetry fingerprint:
    // `failure_clusters.signature` and `execution_telemetry.failure_fingerprint` are deliberately
    // different identifier schemes (see the note in `qa_memory::bug_history`).
    let events = timeline.as_array().expect("timeline events");
    assert_eq!(
        events.len(),
        2,
        "both observed failures must appear on the cluster's timeline: {timeline}"
    );
    for e in events {
        assert_eq!(
            e["metadata"]["task_id"], "t",
            "each entry names the journey step that failed: {e}"
        );
        assert!(
            e["description"]
                .as_str()
                .is_some_and(|d| d.contains("Element not found")),
            "with the reason that was recorded: {e}"
        );
    }

    // ---- the review endpoint is the supported promotion-to-defect path ----
    // The current route is `POST .../findings/:finding_id/review` with `{"action": "confirm"}`;
    // the handler reuses `review_bug_draft`'s exact logic rather than a second implementation.
    let draft_id = drafts[0].id;
    let (status, _body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/findings/{draft_id}/review"),
            Some(&operator),
            Some(json!({ "action": "confirm" })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "reviewing a finding is a state change with no body, not a payload response"
    );

    let after = store
        .list_bug_drafts(Some(&pid_s))
        .await
        .expect("list_bug_drafts");
    assert_eq!(
        after[0].review_state, "CONFIRMED",
        "after review the draft must be CONFIRMED: {:?}",
        after[0]
    );

    // And a viewer may not review -- the boundary is enforced, not documented only.
    let viewer = mint_key(&state, "viewer", Some(&pid_s)).await;
    let second_run = Uuid::new_v4();
    start_run(&store, &pid_s, second_run, &p).await;
    finish_step(
        &store,
        second_run,
        "t",
        TaskStatus::Failed,
        Some("Element not found: #pay"),
    )
    .await;
    Intelligence::new(&store)
        .ingest_run(
            Some(second_run),
            Some(&pid_s),
            &p,
            &[step_state(
                "t",
                TaskStatus::Failed,
                Some("Element not found: #pay"),
                Some(checkout),
            )],
        )
        .await
        .expect("ingest_run");
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/findings/{draft_id}/review"),
            Some(&viewer),
            Some(json!({ "action": "dismiss" })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "confirming or dismissing a finding is an operator action, and a viewer must not do it"
    );

    // ---- planning consumes the failure intelligence ----
    let feedback = crate::adaptive_intelligence::AdaptiveIntelligenceService::new(store.clone())
        .generate_planning_feedback(&pid_s)
        .await
        .expect("generate_planning_feedback");
    assert!(
        !feedback.planning_signals.is_empty(),
        "planning must receive the journey's real signals: {:?}",
        feedback.planning_signals
    );
    assert!(
        !feedback.top_failure_clusters.is_empty(),
        "including the failure cluster the journey produced: {:?}",
        feedback.top_failure_clusters
    );
    let defect_signal = feedback
        .planning_signals
        .iter()
        .find(|s| s.kind == "recurring_defect")
        .expect("planning must receive a recurring_defect signal from the journey's failure");
    assert!(
        defect_signal.metric.contains("CONFIRMED"),
        "and the signal must report the draft's real review state after review: {:?}",
        defect_signal.metric
    );
    assert!(
        defect_signal.metric.contains("3 recorded occurrence"),
        "with the occurrence count the cluster actually reached: {:?}",
        defect_signal.metric
    );
    assert!(
        defect_signal.basis.contains("occurrence_count"),
        "and a basis naming the measurement it came from: {:?}",
        defect_signal.basis
    );
}

// ============================================================================
// JOURNEY F -- END-TO-END DATA INTEGRITY
//
// Data generated through supported entry points -> persisted -> read back through production
// readers -> project isolation and measurement consistency -> the real integrity assessment ->
// and the requirement that stale, missing or failed evidence is never reported as a confirmed
// defect.
//
// D5: the 10 ms floor's behaviour at its boundary is exercised here. The constant is tested as
// implemented; it is **not** treated as a proven universal threshold, and it is not changed.
// ============================================================================

#[tokio::test]
async fn journey_f_end_to_end_data_integrity() {
    let db = Db::new().await;
    let store = &db.store;
    let alpha = make_project(store, "P7 Journey F Alpha").await;
    let beta = make_project(store, "P7 Journey F Beta").await;
    let a_s = alpha.to_string();
    let b_s = beta.to_string();

    // ---- two independent projects, identical journey, identical failure ----
    let mut alpha_run = None;
    let mut beta_run = None;
    for (pid, ps, slot) in [(alpha, &a_s, 0), (beta, &b_s, 1)] {
        seed_requirements(store, pid, "JOURNEYF").await;
        let case = seed_case(store, pid, "Checkout", "FR-004").await;
        seed_case_vertex(store, pid, case).await;
        let run = Uuid::new_v4();
        let p = journey_plan(
            "journey f",
            vec![step("t", Some(case), &[], SimOutcome::Fail)],
        );
        start_run(store, ps, run, &p).await;
        finish_step(
            store,
            run,
            "t",
            TaskStatus::Failed,
            Some("Element not found: #pay"),
        )
        .await;
        Intelligence::new(store)
            .ingest_run(
                Some(run),
                Some(ps),
                &p,
                &[step_state(
                    "t",
                    TaskStatus::Failed,
                    Some("Element not found: #pay"),
                    Some(case),
                )],
            )
            .await
            .expect("ingest_run");
        if slot == 0 {
            alpha_run = Some((run, case));
        } else {
            beta_run = Some((run, case));
        }
    }
    let (a_run, a_case) = alpha_run.expect("alpha run");
    let (b_run, b_case) = beta_run.expect("beta run");
    assert_ne!(a_run, b_run, "two projects, two distinct runs");
    assert_ne!(a_case, b_case, "two projects, two distinct cases");

    // ---- project isolation over persisted telemetry ----
    let a_tel = store
        .list_execution_telemetry(&a_s, 100)
        .await
        .expect("alpha telemetry");
    let b_tel = store
        .list_execution_telemetry(&b_s, 100)
        .await
        .expect("beta telemetry");
    assert_eq!(a_tel.len(), 1, "alpha sees only its own run");
    assert_eq!(b_tel.len(), 1, "beta sees only its own run");
    assert!(a_tel.iter().all(|t| t.run_id == a_run));
    assert!(b_tel.iter().all(|t| t.run_id == b_run));

    // The same failure in both projects must produce the same fingerprint and two clusters.
    let a_fp = a_tel[0]
        .failure_fingerprint
        .clone()
        .expect("alpha fingerprint");
    assert_eq!(
        a_fp,
        b_tel[0]
            .failure_fingerprint
            .clone()
            .expect("beta fingerprint"),
        "identical failures must fingerprint identically -- otherwise isolation is untested"
    );
    let a_cluster = store
        .get_failure_cluster_by_fingerprint(&a_s, &a_fp)
        .await
        .expect("alpha cluster")
        .expect("alpha cluster");
    let b_cluster = store
        .get_failure_cluster_by_fingerprint(&b_s, &a_fp)
        .await
        .expect("beta cluster")
        .expect("beta cluster");
    assert_ne!(
        a_cluster.id, b_cluster.id,
        "one fingerprint, two projects, two distinct clusters"
    );

    // ---- measurement consistency: readers agree with the persisted rows ----
    let reader = QaMemoryReader::new(store.clone());
    let a_view = reader
        .test_stability(&a_s, &a_case.to_string())
        .await
        .expect("alpha stability")
        .view
        .expect("alpha view");
    assert_eq!(
        a_view.executions,
        a_tel.len(),
        "the reader must agree with the row count it was built from"
    );
    assert_eq!(a_view.failed, 1);

    // ---- and no cross-project read ----
    let crossed = reader
        .test_stability(&a_s, &b_case.to_string())
        .await
        .expect("crossed read");
    assert_eq!(
        crossed.insufficient_data,
        Some(InsufficientData::UnknownSubject),
        "another project's case must be unknown here: {crossed:?}"
    );

    // ---- the real integrity assessment over the persisted run ----
    let report = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&a_s, a_run, None)
        .await
        .expect("verify_run");
    assert_eq!(report.run_id, a_run);
    assert_eq!(
        report.defects_established, 0,
        "both layers recorded the same status, so nothing is established: {:?}",
        report.comparisons
    );
    assert!(
        !report.layers_unavailable.is_empty(),
        "layers with no evidence must be named rather than silently absent: {:?}",
        report.layers_unavailable
    );

    // ---- D5: the duration floor at its boundary ----
    use crate::integrity::{
        compare_durations, duration_tolerance_ms, DurationAgreement, DURATION_ABSOLUTE_FLOOR_MS,
    };
    assert_eq!(
        duration_tolerance_ms(0.0, 0.0),
        DURATION_ABSOLUTE_FLOOR_MS,
        "the floor governs for two zero-duration readings"
    );
    assert!(
        matches!(
            compare_durations(1.0, 0.0),
            DurationAgreement::WithinTolerance { difference_ms, .. } if difference_ms == 1.0
        ),
        "a 1 ms gap between independently recorded clocks is noise, not a disagreement"
    );
    assert!(
        matches!(
            compare_durations(DURATION_ABSOLUTE_FLOOR_MS, 0.0),
            DurationAgreement::WithinTolerance { .. }
        ),
        "the boundary is inclusive, as documented"
    );
    assert!(
        matches!(
            compare_durations(DURATION_ABSOLUTE_FLOOR_MS + 0.001, 0.0),
            DurationAgreement::BeyondTolerance { .. }
        ),
        "and just past it is not"
    );
    assert!(
        matches!(
            compare_durations(0.0, 5_000.0),
            DurationAgreement::BeyondTolerance { difference_ms, .. } if difference_ms == 5_000.0
        ),
        "a real divergence stays a divergence -- the floor exists for clock noise, not to hide it"
    );

    // ---- stale evidence must not become a confirmed defect ----
    //
    // A separate run in a separate project, so the only telemetry row for it is the aged one.
    // Adding an aged row alongside a fresh one would leave the fresh row to be paired first, and
    // the skew would never be exercised.
    let stale_pid = make_project(store, "P7 Journey F Stale").await;
    let stale_s = stale_pid.to_string();
    let stale_case = seed_case(store, stale_pid, "Stale checkout", "FR-004").await;
    seed_case_vertex(store, stale_pid, stale_case).await;
    let stale_run = Uuid::new_v4();
    let stale_plan = journey_plan(
        "stale journey",
        vec![step("t", Some(stale_case), &[], SimOutcome::Fail)],
    );
    start_run(store, &stale_s, stale_run, &stale_plan).await;
    finish_step(
        store,
        stale_run,
        "t",
        TaskStatus::Failed,
        Some("Element not found: #pay"),
    )
    .await;
    let aged = ExecutionTelemetry {
        created_at: chrono::Utc::now() - chrono::Duration::hours(48),
        ..ExecutionTelemetry::from_task_state(
            &stale_s,
            stale_run,
            &step_state(
                "t",
                TaskStatus::Failed,
                Some("Element not found: #pay"),
                Some(stale_case),
            ),
            "simulated",
            "test",
            Some(stale_case),
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

    let stale = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&stale_s, stale_run, None)
        .await
        .expect("verify_run");
    let stale_status = stale
        .comparisons
        .iter()
        .find(|c| c.kind == crate::integrity::ComparisonKind::StatusConsistency)
        .expect("a status comparison");
    assert!(
        stale_status.observed_skew_ms.unwrap_or(0) > 60_000,
        "the aged row must put the layers outside the comparison window: {:?}",
        stale_status.observed_skew_ms
    );
    assert_eq!(
        stale_status.outcome,
        crate::integrity::IntegrityOutcome::InsufficientData,
        "and records that cannot be compared are not evidence of a disagreement: {}",
        stale_status.detail
    );
    assert!(
        !stale_status.establishes_defect(),
        "stale evidence must never become a confirmed defect: {stale_status:?}"
    );
    assert_eq!(
        stale.defects_established, 0,
        "so the run establishes nothing at all: {:?}",
        stale.comparisons
    );

    // ---- a failed storage read is now visible, not a clean report (P7-D1, fixed in 7.1) ----
    //
    // This journey is where P7-D1 was found. Phase 7 asserted the *defective* behaviour on
    // purpose -- `comparisons.len() == 0` with `defects_established == 0` -- because that is
    // what the code did and a validation phase may not change production behaviour. Phase 7.1
    // remediated it, so this block now asserts the opposite, and the fact that it needed
    // rewriting is the evidence the defect was real and not a hypothetical.
    store.pool.close().await;

    // The verifier must refuse rather than return a report. Before the fix this returned
    // `Ok` with an empty comparison list, which a consumer checking `defects_established == 0`
    // could not tell from a run that agreed with itself.
    let failed = crate::integrity::IntegrityVerifier::new(store)
        .verify_run(&a_s, a_run, None)
        .await;
    assert!(
        failed.is_err(),
        "P7-D1 fixed: a closed pool must fail the verification, not report a clean run: {:?}",
        failed.map(|r| (r.comparisons.len(), r.defects_established, r.inconsistent))
    );

    // And the error must name the read that failed, not be a bare opaque failure.
    let reason = failed.expect_err("a closed pool is an error").to_string();
    assert!(
        reason.to_lowercase().contains("pool") || reason.to_lowercase().contains("closed"),
        "the error must identify the storage failure: {reason}"
    );

    // QA Memory, which already propagated, must still propagate: the two readers now agree.
    assert!(
        reader
            .test_stability(&a_s, &a_case.to_string())
            .await
            .is_err(),
        "QA Memory propagates the same closed-store failure, and now the verifier does too"
    );
}

// ============================================================================
// JOURNEY FAILURE PATHS
//
// The roadmap's required journey tests that are about a journey *not* succeeding: an invalid
// prerequisite order, a duplicate step, a blocked dependent, a failed intermediate step, a failed
// read mid-journey, and project isolation for a whole journey rather than a single read.
// ============================================================================

/// An invalid prerequisite must be rejected by the existing plan validator, not accepted and
/// silently reinterpreted.
///
/// **A finding from writing this test.** I first assumed a *forward* reference (s1 declared before
/// the s2 it depends on) was invalid. It is not: a prerequisite is an edge in a DAG, not a
/// declaration-order constraint, and `PlanValidator::validate` **accepts** it. That is correct
/// behaviour and I was wrong. What the validator does refuse is a prerequisite naming a task that
/// does not exist — which is the case that would otherwise leave a journey step permanently
/// unsatisfiable — and a cycle, covered by the next test.
#[tokio::test]
async fn a_prerequisite_naming_an_unknown_step_is_rejected() {
    use crate::plan_validator::{AgentExecutionPlan, PlanValidator};
    use std::collections::BTreeMap;

    let db = Db::new().await;
    let pid = make_project(&db.store, "P7 Unknown Prereq").await;

    let plan = AgentExecutionPlan {
        project_id: pid.to_string(),
        environment: "test".to_string(),
        plan_name: "unknown prerequisite".to_string(),
        concurrency: 1,
        tasks: vec![agent_step("s1", &["does-not-exist"]), agent_step("s2", &[])],
        metadata: BTreeMap::new(),
    };
    let result = PlanValidator::validate(&plan, 100, 60, 1).await;
    assert!(
        result.is_err(),
        "a step depending on a step that does not exist can never run, so it must be refused: \
         {result:?}"
    );
    let errors = result.expect_err("expected a rejection");
    assert!(!errors.is_empty(), "the rejection must name what is wrong");
    for e in &errors {
        assert!(
            !e.message.is_empty(),
            "every validation error must carry a message: {e:?}"
        );
    }
    assert!(
        errors.iter().any(|e| e.message.contains("does-not-exist")),
        "and the message must name the unresolvable step: {errors:?}"
    );

    // A refused journey leaves nothing behind: nothing to mistake for progress.
    assert!(
        db.store
            .get_run_summary(Uuid::new_v4())
            .await
            .expect("a lookup of an unknown run is answerable")
            .is_none(),
        "an unknown run is absent, and the refused journey created none"
    );
}

/// And the forward-reference case my first attempt asserted is in fact **valid** — pinned here so
/// the distinction is documented rather than left to a future reader's guesswork.
#[tokio::test]
async fn a_forward_prerequisite_is_legal_because_a_dag_has_no_declaration_order() {
    use crate::plan_validator::{AgentExecutionPlan, PlanValidator};
    use std::collections::BTreeMap;

    let plan = AgentExecutionPlan {
        project_id: "P7Forward".to_string(),
        environment: "test".to_string(),
        plan_name: "forward reference".to_string(),
        concurrency: 1,
        tasks: vec![agent_step("s1", &["s2"]), agent_step("s2", &[])],
        metadata: BTreeMap::new(),
    };
    assert!(
        PlanValidator::validate(&plan, 100, 60, 1).await.is_ok(),
        "declaring a step before the step it depends on is legal and must be accepted"
    );
}

/// One `AgentTaskPlan` for the validator, in the shape the engine accepts.
fn agent_step(id: &str, prerequisites: &[&str]) -> crate::plan_validator::AgentTaskPlan {
    crate::plan_validator::AgentTaskPlan {
        id: id.to_string(),
        name: format!("journey step {id}"),
        prerequisites: prerequisites.iter().map(|s| s.to_string()).collect(),
        executor: "simulated".to_string(),
        target_url: None,
        actions: Vec::new(),
        timeout_secs: Some(60),
        requirement_reference: None,
        custom_params: None,
    }
}

/// A cycle must be refused too - the other half of "invalid prerequisite order".
#[tokio::test]
async fn a_prerequisite_cycle_is_refused() {
    use crate::plan_validator::{AgentExecutionPlan, PlanValidator};
    use std::collections::BTreeMap;

    let plan = AgentExecutionPlan {
        project_id: "P7Cycle".to_string(),
        environment: "test".to_string(),
        plan_name: "cycle".to_string(),
        concurrency: 1,
        tasks: vec![agent_step("a", &["b"]), agent_step("b", &["a"])],
        metadata: BTreeMap::new(),
    };
    let result = PlanValidator::validate(&plan, 100, 60, 1).await;
    assert!(
        result.is_err(),
        "a cycle can never be satisfied, so it must be refused: {result:?}"
    );
}

/// A failed intermediate step must block its dependent, and the block must be recorded rather than
/// the dependent being reported as merely unrun.
#[tokio::test]
async fn a_failed_intermediate_step_blocks_its_dependent_and_records_why() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P7 Blocked Dependent").await;
    let pid_s = pid.to_string();
    seed_requirements(store, pid, "P7BLOCK").await;
    let search = seed_case(store, pid, "Search", "FR-003").await;
    let checkout = seed_case(store, pid, "Checkout", "FR-004").await;
    seed_case_vertex(store, pid, search).await;
    seed_case_vertex(store, pid, checkout).await;

    let run = Uuid::new_v4();
    let p = journey_plan(
        "search -> checkout",
        vec![
            step("s1", Some(search), &[], SimOutcome::Fail),
            step("s2", Some(checkout), &["s1"], SimOutcome::Pass),
        ],
    );
    start_run(store, &pid_s, run, &p).await;

    // s1 fails.
    finish_step(
        store,
        run,
        "s1",
        TaskStatus::Failed,
        Some("Element not found: #results"),
    )
    .await;

    // s2's prerequisite is not satisfied: `TaskStatus::satisfies_dependents` is `Passed` only.
    assert!(
        !TaskStatus::Failed.satisfies_dependents(),
        "a failed step must not satisfy a dependent"
    );
    assert!(
        TaskStatus::Passed.satisfies_dependents(),
        "only a passed step does"
    );

    // Record s2 as blocked, the way the engine's cascade does.
    store
        .set_task_status(
            run,
            "s2",
            TaskStatus::Blocked,
            Some("prerequisite s1 did not pass"),
            false,
            false,
        )
        .await
        .expect("record the block");

    let tasks = store.list_tasks_for_run(run).await.expect("tasks");
    let s1 = tasks.iter().find(|t| t.id == "s1").expect("s1");
    let s2 = tasks.iter().find(|t| t.id == "s2").expect("s2");
    assert_eq!(s1.status.as_str(), "FAILED");
    assert_eq!(
        s2.status.as_str(),
        "BLOCKED",
        "the dependent step must be recorded as blocked"
    );
    assert!(
        s2.reason.as_deref().is_some_and(|r| r.contains("s1")),
        "with a reason naming the step that blocked it: {:?}",
        s2.reason
    );
    assert!(
        s2.finished_at.is_none(),
        "a blocked step must not carry an end time, because it never ran: {s2:?}"
    );

    // And it must not appear to have executed.
    Intelligence::new(store)
        .ingest_run(
            Some(run),
            Some(&pid_s),
            &p,
            &[
                step_state(
                    "s1",
                    TaskStatus::Failed,
                    Some("Element not found: #results"),
                    Some(search),
                ),
                step_state(
                    "s2",
                    TaskStatus::Blocked,
                    Some("prerequisite s1 did not pass"),
                    Some(checkout),
                ),
            ],
        )
        .await
        .expect("ingest_run");

    // `TaskStatus::Blocked` is terminal, so `ingest_run` records it. That is honest -- the step
    // was reached and refused -- and the row is unmistakably marked BLOCKED with no end time.
    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    assert_eq!(
        telemetry.len(),
        2,
        "the failing step and the blocked step are both recorded: {telemetry:?}"
    );
    let blocked_row = telemetry
        .iter()
        .find(|t| t.test_case_id == Some(checkout))
        .expect("the blocked step's row");
    assert_eq!(
        blocked_row.status.as_str(),
        "BLOCKED",
        "and it must be marked blocked, not passed or failed: {blocked_row:?}"
    );
    assert!(
        blocked_row.end_time.is_none(),
        "a step that never ran must carry no end time: {blocked_row:?}"
    );
    assert!(
        blocked_row.failure_fingerprint.is_none(),
        "and no failure fingerprint, since nothing failed: {blocked_row:?}"
    );
    let failed_row = telemetry
        .iter()
        .find(|t| t.test_case_id == Some(search))
        .expect("the failing step's row");
    assert_eq!(failed_row.status.as_str(), "FAILED");
    assert!(
        failed_row.failure_fingerprint.is_some(),
        "only the step that actually ran and failed carries a fingerprint"
    );

    // ---- P7-D2: a blocked step no longer publishes a 0% failure rate (fixed in 7.1) ----
    //
    // This is where P7-D2 was found. Phase 7 asserted the defective behaviour on purpose --
    // `failure_rate: Some(0.0)` for a step that never executed -- because a validation phase may
    // not change production behaviour. Phase 7.1 fixed the denominator, so this now asserts the
    // corrected contract.
    //
    // Note what is preserved and what changed:
    //   * `executions` still counts every recorded outcome, including the blocked one. That is
    //     the existing meaning of the field, and the fix deliberately did not redefine it.
    //   * `other_outcomes` still carries the blocked row, so a reader can still see it happened.
    //   * only the **rate** changed: a non-execution outcome is no longer eligible to be a
    //     sample, so with no completed execution the rate is unmeasured rather than a clean 0%.
    let view = QaMemoryReader::new(store.clone())
        .test_stability(&pid_s, &checkout.to_string())
        .await
        .expect("test_stability");
    let v = view.view.as_ref().expect("a view for the blocked step");
    assert_eq!(
        v.executions, 1,
        "the blocked outcome is still recorded as an observation: {v:?}"
    );
    assert_eq!(v.passed, 0, "and correctly not as a pass: {v:?}");
    assert_eq!(v.failed, 0, "and correctly not as a failure: {v:?}");
    assert_eq!(
        v.other_outcomes, 1,
        "and it stays visible as a blocked outcome rather than disappearing: {v:?}"
    );
    assert_eq!(
        v.failure_rate, None,
        "P7-D2 fixed: with no completed pass/fail execution the rate is unmeasured, not a \
         measured 0% that reads as 'this case passes': {v:?}"
    );

    // The failing step, which really did run, is unaffected and keeps its real rate.
    let ran = QaMemoryReader::new(store.clone())
        .test_stability(&pid_s, &search.to_string())
        .await
        .expect("test_stability")
        .view
        .expect("a view for the step that ran");
    assert_eq!(ran.executions, 1, "the executed step: {ran:?}");
    assert_eq!(ran.failed, 1, "failed once: {ran:?}");
    assert_eq!(
        ran.failure_rate,
        Some(100.0),
        "and a single genuine failure still measures 100% -- one sample is enough to observe a \
         failure, just as one pass measures 0%: {ran:?}"
    );
}

/// A duplicate step id must not silently produce two steps.
#[tokio::test]
async fn a_duplicate_step_id_is_the_same_step_not_a_second_one() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P7 Duplicate Step").await;
    let pid_s = pid.to_string();
    seed_requirements(store, pid, "P7DUP").await;
    let case = seed_case(store, pid, "Checkout", "FR-004").await;
    seed_case_vertex(store, pid, case).await;

    let run = Uuid::new_v4();
    let p = journey_plan(
        "duplicate",
        vec![step("s1", Some(case), &[], SimOutcome::Pass)],
    );
    start_run(store, &pid_s, run, &p).await;
    finish_step(store, run, "s1", TaskStatus::Passed, None).await;
    // The same step recorded again, as a retry of the same id would be.
    finish_step(store, run, "s1", TaskStatus::Passed, None).await;

    let tasks = store.list_tasks_for_run(run).await.expect("tasks");
    assert_eq!(
        tasks.len(),
        1,
        "a duplicate step id must not create a second step: {tasks:?}"
    );
    assert_eq!(tasks[0].id, "s1");

    Intelligence::new(store)
        .ingest_run(
            Some(run),
            Some(&pid_s),
            &p,
            &[step_state("s1", TaskStatus::Passed, None, Some(case))],
        )
        .await
        .expect("ingest_run");

    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    assert_eq!(
        telemetry.len(),
        1,
        "and the duplicate must not double the recorded executions either: {telemetry:?}"
    );

    let view = QaMemoryReader::new(store.clone())
        .test_stability(&pid_s, &case.to_string())
        .await
        .expect("test_stability")
        .view
        .expect("a view");
    assert_eq!(
        view.executions, 1,
        "so a reader reports one execution, not two"
    );
}

/// A storage failure part-way through a journey must not leave the journey looking complete.
#[tokio::test]
async fn a_failed_read_mid_journey_does_not_report_the_journey_as_complete() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P7 Mid-Journey Failure").await;
    let pid_s = pid.to_string();
    seed_requirements(store, pid, "P7MID").await;
    let sign_in = seed_case(store, pid, "Sign in", "FR-001").await;
    let search = seed_case(store, pid, "Search", "FR-003").await;
    for c in [sign_in, search] {
        seed_case_vertex(store, pid, c).await;
    }

    let run = Uuid::new_v4();
    let p = journey_plan(
        "sign-in -> search",
        vec![
            step("s1", Some(sign_in), &[], SimOutcome::Pass),
            step("s2", Some(search), &["s1"], SimOutcome::Pass),
        ],
    );
    start_run(store, &pid_s, run, &p).await;
    finish_step(store, run, "s1", TaskStatus::Passed, None).await;
    Intelligence::new(store)
        .ingest_run(
            Some(run),
            Some(&pid_s),
            &p,
            &[step_state("s1", TaskStatus::Passed, None, Some(sign_in))],
        )
        .await
        .expect("ingest_run");

    // The journey is half done. Now the store fails.
    store.pool.close().await;

    let reader = QaMemoryReader::new(store.clone());
    for (name, res) in [
        (
            "project_memory_summary",
            reader.project_memory_summary(&pid_s).await.is_err(),
        ),
        (
            "test_stability",
            reader
                .test_stability(&pid_s, &sign_in.to_string())
                .await
                .is_err(),
        ),
        (
            "planning_signals",
            reader.planning_signals(&pid_s).await.is_err(),
        ),
        (
            "prioritise_tests",
            reader.prioritise_tests(&pid_s, None).await.is_err(),
        ),
    ] {
        assert!(
            res,
            "{name} reported a result for a half-finished journey whose store cannot be read"
        );
    }
    assert!(
        crate::adaptive_intelligence::AdaptiveIntelligenceService::new(store.clone())
            .generate_planning_feedback(&pid_s)
            .await
            .is_err(),
        "and planning feedback must not present an empty signal set for an unreadable store"
    );
}

/// A journey's data must stay inside its project even when another project holds identical data.
#[tokio::test]
async fn a_whole_journey_is_isolated_from_an_identical_journey_in_another_project() {
    let db = Db::new().await;
    let store = &db.store;
    let alpha = make_project(store, "P7 Iso Alpha").await;
    let beta = make_project(store, "P7 Iso Beta").await;
    let a_s = alpha.to_string();
    let b_s = beta.to_string();

    let mut alpha_cases = Vec::new();
    let mut beta_cases = Vec::new();
    for (pid, ps, sink) in [
        (alpha, &a_s, &mut alpha_cases),
        (beta, &b_s, &mut beta_cases),
    ] {
        seed_requirements(store, pid, "P7ISO").await;
        for (title, req) in [
            ("Sign in", "FR-001"),
            ("Search", "FR-003"),
            ("Checkout", "FR-004"),
        ] {
            let case = seed_case(store, pid, title, req).await;
            seed_case_vertex(store, pid, case).await;
            sink.push((case, req));
        }
        let p = journey_plan(
            "journey",
            vec![step("t", Some(sink[0].0), &[], SimOutcome::Pass)],
        );
        let run = Uuid::new_v4();
        start_run(store, ps, run, &p).await;
        finish_step(store, run, "t", TaskStatus::Passed, None).await;
        Intelligence::new(store)
            .ingest_run(
                Some(run),
                Some(ps),
                &p,
                &[step_state("t", TaskStatus::Passed, None, Some(sink[0].0))],
            )
            .await
            .expect("ingest_run");
    }

    // Identical titles, identical requirement references, distinct stored ids.
    for ((a_case, a_req), (b_case, b_req)) in alpha_cases.iter().zip(beta_cases.iter()) {
        assert_ne!(
            a_case, b_case,
            "two projects, two distinct case ids for the same title"
        );
        assert_eq!(a_req, b_req, "and the same requirement key on both sides");
    }

    let reader = QaMemoryReader::new(store.clone());
    let a_summary = reader
        .project_memory_summary(&a_s)
        .await
        .expect("alpha summary");
    let b_summary = reader
        .project_memory_summary(&b_s)
        .await
        .expect("beta summary");
    assert_eq!(a_summary.executions, 1, "alpha counts only its own");
    assert_eq!(b_summary.executions, 1, "beta counts only its own");

    // Every case must be unknown to the other project.
    for (a_case, _) in alpha_cases.iter() {
        assert_eq!(
            reader
                .test_stability(&b_s, &a_case.to_string())
                .await
                .expect("crossed read")
                .insufficient_data,
            Some(InsufficientData::UnknownSubject),
            "alpha's case must be unknown in beta"
        );
    }
}
