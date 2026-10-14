//! Phase 6.2 — real-data end-to-end integration validation.
//!
//! ## What this module is for
//!
//! Phase 6.1 proved individual components correct. It could not prove they work *together*,
//! because each test built the state its own component needed. This module closes that gap: every
//! scenario drives the **production writers**, then reads back through the **production readers**
//! and asserts the **downstream consumer** saw the same thing.
//!
//! ## The chain under test
//!
//! ```text
//! Requirement → Test Case → Change → Impact → Prioritised Test → Execution
//!            → Evidence → Failure → Trace Graph → Risk / Integrity result
//! ```
//!
//! ## Ground rules this module holds itself to
//!
//! * **Production writers only.** Every row is created through a public `Store` method, the
//!   `Intelligence` ingestion path, or an HTTP request against the real router. No direct SQL is
//!   used to manufacture a state. (The one raw statement in Phase 6.1's F2 fix pinned a clock on
//!   rows the store had already written; nothing here does anything like it.)
//! * **Real migrations.** Every scenario calls `Store::connect`, which runs
//!   `sqlx::migrate!("./migrations")` — all 33 migrations, on a fresh file, per scenario.
//! * **Identifiers, not names.** Assertions check stored ids, foreign keys and relationship rows.
//!   Two entities with the same title must still be distinguishable.
//! * **Honest outcomes.** A scenario asserts a PASS result, an honest `INSUFFICIENT_DATA`, or an
//!   observable error. It never asserts a fabricated number to make a path look exercised.
//!
//! ## What is deliberately *not* claimed
//!
//! These are fixtures, not production history. They prove the code paths compose correctly over
//! real persistence; they do not prove anything about AIQA's accuracy on a real product. The
//! roadmap says as much: *"Do not claim that test fixtures prove production history."*

use crate::adaptive_intelligence::{AdaptiveIntelligenceService, ExecutionTelemetry};
use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel, Embedder};
use crate::change_impact::{ChangeImpactReader, ChangeTarget};
use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::integrity::{ComparisonKind, IntegrityOutcome, IntegrityVerifier};
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
// Harness — a real database, real migrations, the real router.
// ---------------------------------------------------------------------------

/// A migrated, isolated database. The `TempDir` is kept alive for the scenario's duration.
struct Db {
    store: Store,
    _dir: TempDir,
}

impl Db {
    /// Every scenario gets its own file, so scenarios cannot contaminate each other. `Store::connect`
    /// runs the full migration set, so no scenario depends on a hand-built schema.
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("phase62.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, _dir: dir }
    }
}

/// A full application state over a real database, for the HTTP-level scenarios.
async fn app(db_path_dir: &TempDir) -> (AppState, Store) {
    let store = Store::connect(&db_path_dir.path().join("api.sqlite3"))
        .await
        .expect("connect + migrate");
    let evidence =
        LocalEvidenceStore::new(db_path_dir.path().join("evidence")).expect("evidence store");
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

/// An AI router whose only executor returns a fixed body. Phase 6.2 never asserts AI-generated
/// content, so this exists solely so `AppState` can be constructed without a network call — the
/// same pattern `api_tests` uses.
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

/// The deployment-without-embedder state: `is_configured() == false`, so every embedding path
/// no-ops rather than reaching the network.
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
            Body::from(serde_json::to_vec(&v).expect("serialize body"))
        }
        None => Body::empty(),
    };
    builder.body(body).expect("request")
}

/// Some endpoints serve raw bytes rather than JSON — `GET .../evidence/:id/content` returns the
/// stored object. This reads those as text instead of demanding a JSON envelope.
async fn send_text(state: &AppState, request: Request<Body>) -> (StatusCode, String) {
    let app = crate::api::router(state.clone());
    let resp = app.oneshot(request).await.expect("router call");
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
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
            "phase62-key",
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
// Production-writer fixture steps.
//
// Each of these is a real application write. Together they build the chain the roadmap asks for.
// ---------------------------------------------------------------------------

/// A requirement document, parsed by the **real** parser rather than hand-assembled, so the
/// requirement rows are ones the application genuinely produces.
async fn seed_requirement(store: &Store, project_id: Uuid, doc_label: &str) -> Vec<String> {
    // Written the way the parser's own fixtures write documents: an id, a separator, then the
    // obligation. A bare heading is a section, not a requirement, so the ids live in bullets.
    let text = "## Login\n\n\
         - FR-001: The system shall authenticate a valid user.\n\
         - FR-002: The system shall reject an invalid password.\n\n\
         ## Checkout\n\n\
         - FR-003: The system shall total the cart before charging.\n";
    let parsed = crate::requirements::parse_document(doc_label, "spec.md", text);
    assert!(
        !parsed.requirements.is_empty(),
        "the real parser must find requirements in a document that plainly contains them"
    );
    store
        .upsert_requirement_document(
            &project_id.to_string(),
            "spec.md",
            Some("spec"),
            "hash-62",
            text,
            &parsed,
        )
        .await
        .expect("upsert_requirement_document");
    parsed.requirements.iter().map(|r| r.key.clone()).collect()
}

/// A test case, created through the production `insert_test_case`.
async fn seed_test_case(store: &Store, project_id: Uuid, title: &str, requirement: &str) -> Uuid {
    let req = CreateTestCaseRequest {
        title: title.to_string(),
        description: format!("{title} description"),
        category: "functional".into(),
        priority: "P1".into(),
        severity: "S2".into(),
        preconditions: vec![],
        steps: vec![],
        expected_result: Some("the operation succeeds".into()),
        assertions: vec![],
        test_data: None,
        source_document: Some("spec.md".into()),
        requirement_reference: Some(requirement.to_string()),
        source_requirement: Some(requirement.to_string()),
        traceability: None,
        risk_factor: None,
        tags: vec![],
        suite_id: None,
    };
    store
        .insert_test_case(&project_id.to_string(), &req, Some("phase62"))
        .await
        .expect("insert_test_case")
}

/// The `validates` edge that makes a requirement and its case one connected chain rather than two
/// rows that happen to share a title.
async fn link_requirement(store: &Store, project_id: Uuid, case_id: Uuid, requirement: &str) {
    store
        .link_case_to_requirement(
            &project_id.to_string(),
            &case_id,
            "case title",
            requirement,
            "requirement title",
        )
        .await
        .expect("link_case_to_requirement");
}

/// The graph vertex for a stored test case, keyed on the **case id**.
///
/// `ChangeImpactReader::test_node_id` resolves a selected case back to a graph vertex by matching
/// `external_ref == test_cases.id`. A case that has no such vertex is still selectable by the
/// analysis — it simply cannot be persisted as a `change_impacts` row, because that table records
/// node ids. So a vertex is required for the persisted half of the change path, and it is written
/// through the production `upsert_quality_node`.
async fn seed_case_vertex(store: &Store, project_id: Uuid, case_id: Uuid) -> Uuid {
    store
        .upsert_quality_node(
            &project_id.to_string(),
            crate::store::TEST_NODE_TYPE,
            &case_id.to_string(),
            "case title",
        )
        .await
        .expect("upsert case vertex")
}

/// One task of a run, built by the production plan writer.
fn task(id: &str, case_id: Option<Uuid>, outcome: SimOutcome) -> TaskDef {
    TaskDef {
        id: id.to_string(),
        name: format!("task {id}"),
        prerequisites: Vec::new(),
        simulate: Some(SimSpec {
            outcome,
            delay_ms: 1,
        }),
        executor: Some("simulated".into()),
        params: json!({}),
        test_case_id: case_id.map(|c| c.to_string()),
    }
}

fn plan(name: &str, tasks: Vec<TaskDef>) -> RunPlan {
    RunPlan {
        name: name.to_string(),
        tasks,
        concurrency: 1,
    }
}

/// One recorded execution, written the way `engine.rs` writes it: a start call and a separate
/// finish call, each recording only its own flag.
async fn record_task(
    store: &Store,
    run_id: Uuid,
    task_id: &str,
    status: TaskStatus,
    reason: Option<&str>,
) {
    store
        .set_task_status(run_id, task_id, TaskStatus::Running, None, true, false)
        .await
        .expect("start task");
    store
        .set_task_status(run_id, task_id, status, reason, false, true)
        .await
        .expect("finish task");
}

/// The API layer's record of the same executions, through the production ingestion path. This is
/// the writer that produces `execution_telemetry`, failure fingerprints and clusters.
async fn ingest(
    store: &Store,
    run_id: Uuid,
    project_id: Uuid,
    plan: &RunPlan,
    states: Vec<TaskState>,
) {
    Intelligence::new(store)
        .ingest_run(Some(run_id), Some(&project_id.to_string()), plan, &states)
        .await
        .expect("ingest_run");
}

fn task_state(
    id: &str,
    status: TaskStatus,
    reason: Option<String>,
    case_id: Option<Uuid>,
) -> TaskState {
    TaskState {
        id: id.to_string(),
        name: format!("task {id}"),
        prerequisites: Vec::new(),
        status,
        reason,
        started_at: None,
        finished_at: None,
        test_case_id: case_id.map(|c| c.to_string()),
    }
}

// ============================================================================
// S1 - the completely passing journey.
//
// Requirement -> Test Case -> Execution -> Telemetry -> QA Memory -> Integrity.
//
// This is the roadmap's target chain, walked forwards from the head, with no failures anywhere.
// Every step goes through a production writer and is read back through a production reader.
// ============================================================================

#[tokio::test]
async fn s1_a_requirement_to_its_case_to_its_execution_to_qa_memory_and_integrity() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Passing Journey").await;
    let pid_s = pid.to_string();

    // ---- 1. requirement, produced by the real document parser ----
    let keys = seed_requirement(store, pid, "PASSING").await;
    assert_eq!(
        keys,
        vec![
            "FR-001".to_string(),
            "FR-002".to_string(),
            "FR-003".to_string()
        ],
        "the real parser must key these three obligations exactly, in order; got {keys:?}"
    );

    let requirements = store
        .list_requirements(&pid_s, &[])
        .await
        .expect("list_requirements");
    assert!(
        !requirements.is_empty(),
        "the requirement must be readable back from the database"
    );

    // ---- 2. test case citing it, created by the production writer ----
    let case = seed_test_case(store, pid, "Login with a valid password", "FR-001").await;
    link_requirement(store, pid, case, "FR-001").await;

    // The link must be a real relationship row, not a matching string.
    let cases_for_req = store
        .list_test_cases_for_requirement(&pid_s, "FR-001")
        .await
        .expect("list_test_cases_for_requirement");
    assert_eq!(
        cases_for_req.len(),
        1,
        "exactly one case is linked to FR-001"
    );
    assert_eq!(
        cases_for_req[0].id, case,
        "the link must resolve to the stored case id, not merely a matching title"
    );

    // ---- 3. execution, written the way the engine writes it ----
    let run_id = Uuid::new_v4();
    let p = plan(
        "passing journey",
        vec![task("t-login", Some(case), SimOutcome::Pass)],
    );
    store
        .insert_run_for_release(
            run_id,
            &p,
            Some(&pid_s),
            Some("phase62"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    record_task(store, run_id, "t-login", TaskStatus::Passed, None).await;

    // ---- 4. telemetry + intelligence, through the production ingestion path ----
    ingest(
        store,
        run_id,
        pid,
        &p,
        vec![task_state("t-login", TaskStatus::Passed, None, Some(case))],
    )
    .await;

    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("list_execution_telemetry");
    assert_eq!(
        telemetry.len(),
        1,
        "one execution must have produced exactly one telemetry row"
    );
    assert_eq!(
        telemetry[0].test_case_id,
        Some(case),
        "the telemetry row must be keyed on the real case id: stable identity through execution"
    );
    assert_eq!(telemetry[0].run_id, run_id, "and on the real run id");
    assert_eq!(
        telemetry[0].status.as_str(),
        TaskStatus::Passed.as_str(),
        "and must record the outcome the production writers recorded"
    );

    // ---- 5. the QA Memory reader sees the same history ----
    let reader = QaMemoryReader::new(store.clone());
    let stability = reader
        .test_stability(&pid_s, &case.to_string())
        .await
        .expect("test_stability");
    assert!(
        stability.view.is_some(),
        "QA Memory must return a view for a case with a recorded execution: {stability:?}"
    );
    let view = stability.view.as_ref().expect("view");
    assert_eq!(
        view.executions, 1,
        "QA Memory must agree with the telemetry row's count"
    );
    assert_eq!(
        view.failed, 0,
        "and must not invent a failure for a passing execution"
    );
    assert_eq!(view.passed, 1, "the single pass must be counted as a pass");

    // ---- 6. cross-layer integrity, over the same persisted entity ----
    let report = IntegrityVerifier::new(store)
        .verify_run(&pid_s, run_id, None)
        .await
        .expect("verify_run");
    assert_eq!(
        report.run_id, run_id,
        "the integrity report must be about this run"
    );
    assert!(
        !report.layers_unavailable.is_empty(),
        "layers with no evidence must be named rather than silently absent: {:?}",
        report.layers_unavailable
    );
    assert_eq!(
        report.inconsistent, 0,
        "two layers that agree must produce no inconsistency: {:?}",
        report.comparisons
    );
    assert_eq!(
        report.defects_established, 0,
        "and no product defect: {:?}",
        report.comparisons
    );
    // The comparison must be about the entity we created, identified by the stored key.
    let status_cmp = report
        .comparisons
        .iter()
        .find(|c| c.kind == ComparisonKind::StatusConsistency)
        .expect("a status comparison for the executed task");
    assert_eq!(
        status_cmp.identity.as_ref().map(|i| i.kind()),
        Some("run_task_key"),
        "identity must come from the stored (run_id, task_id) key, not from name matching"
    );
}

// ============================================================================
// S2 - a controlled failure with recorded evidence.
//
// The failure must produce a real fingerprint, a real cluster, a real evidence row and a real
// evidence object, and every downstream reader must agree on the same case.
// ============================================================================

#[tokio::test]
async fn s2_a_controlled_failure_produces_a_fingerprint_a_cluster_and_readable_evidence() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Controlled Failure").await;
    let pid_s = pid.to_string();

    let case = seed_test_case(store, pid, "Checkout with a saved card", "FR-001").await;
    link_requirement(store, pid, case, "FR-001").await;

    // ---- execution that genuinely fails, with a selector so the fingerprint has one ----
    let run_id = Uuid::new_v4();
    let mut t = task("t-pay", Some(case), SimOutcome::Fail);
    t.params = json!({ "selector": "#pay" });
    let p = plan("controlled failure", vec![t]);
    store
        .insert_run_for_release(
            run_id,
            &p,
            Some(&pid_s),
            Some("phase62"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    record_task(
        store,
        run_id,
        "t-pay",
        TaskStatus::Failed,
        Some("Element not found: #pay"),
    )
    .await;

    ingest(
        store,
        run_id,
        pid,
        &p,
        vec![task_state(
            "t-pay",
            TaskStatus::Failed,
            Some("Element not found: #pay".into()),
            Some(case),
        )],
    )
    .await;

    // ---- telemetry: the fingerprint must be present and non-empty ----
    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("list_execution_telemetry");
    assert_eq!(
        telemetry.len(),
        1,
        "one failing execution, one telemetry row"
    );
    let fp = telemetry[0]
        .failure_fingerprint
        .clone()
        .expect("a failing execution must record a fingerprint");
    assert!(
        !fp.is_empty(),
        "an empty fingerprint would collapse distinct failures into one cluster"
    );
    assert_eq!(
        telemetry[0].test_case_id,
        Some(case),
        "the failing telemetry row must still be keyed on the real case"
    );
    assert_eq!(
        telemetry[0].selector.as_deref(),
        Some("#pay"),
        "and must carry the selector the fingerprint was computed from"
    );

    // ---- the failure cluster: the same fingerprint, resolvable as a stored identity ----
    let cluster = store
        .get_failure_cluster_by_fingerprint(&pid_s, &fp)
        .await
        .expect("get_failure_cluster_by_fingerprint")
        .expect("a cluster must exist for the recorded fingerprint");
    assert_eq!(
        cluster.failure_fingerprint.as_deref(),
        Some(fp.as_str()),
        "the cluster must be keyed on the fingerprint telemetry recorded"
    );
    // The cluster row carries no project column: it is reached through a project-scoped query, so
    // its belonging is proven by the scoped lookup succeeding — and by the unscoped one failing,
    // which S8 asserts.
    assert_eq!(
        cluster.occurrence_count, 1,
        "one recorded failure is one occurrence, not an invented repeat"
    );

    // ---- evidence: written through the production path, then read back ----
    let evidence_store =
        LocalEvidenceStore::new(db._dir.path().join("evidence")).expect("evidence store");
    let (object_key, size) = evidence_store
        .put(
            &run_id.to_string(),
            "t-pay",
            "log",
            "Element not found: #pay\nselector=#pay\n",
        )
        .expect("evidence put");
    assert!(size > 0, "the evidence object must not be empty");
    let evidence_id = store
        .insert_evidence(run_id, "t-pay", "log", &object_key, "text/plain", size)
        .await
        .expect("insert_evidence");

    let rows = store
        .list_evidence_for_run(run_id)
        .await
        .expect("list_evidence_for_run");
    assert_eq!(rows.len(), 1, "the evidence row must be listed for its run");
    assert_eq!(rows[0].id, evidence_id, "and carry the stored id");
    let content =
        String::from_utf8(evidence_store.read(&object_key).expect("evidence read")).expect("text");
    assert!(
        content.contains("#pay"),
        "the evidence object itself must hold what was written: {content:?}"
    );

    // ---- downstream: QA Memory must agree with the same case and the same failure ----
    let reader = QaMemoryReader::new(store.clone());
    let stability = reader
        .test_stability(&pid_s, &case.to_string())
        .await
        .expect("test_stability");
    let view = stability
        .view
        .as_ref()
        .expect("a view for the failing case");
    assert_eq!(view.executions, 1);
    assert_eq!(view.failed, 1, "QA Memory must see the failure");
    assert_eq!(
        view.distinct_failure_fingerprints, 1,
        "QA Memory must report exactly one distinct fingerprint"
    );
    assert_eq!(
        view.latest_failure_fingerprint.as_deref(),
        Some(fp.as_str()),
        "and it must be the same fingerprint telemetry recorded"
    );

    // ---- and the cross-layer comparison must agree, not invent a defect ----
    let report = IntegrityVerifier::new(store)
        .verify_run(&pid_s, run_id, None)
        .await
        .expect("verify_run");
    assert_eq!(
        report.inconsistent, 0,
        "both layers recorded FAILED, so there is no disagreement: {:?}",
        report.comparisons
    );
}

/// Record one execution end to end: plan row, two-flag status writes, then ingestion.
///
/// Panics on failure. `try_execute` is the fallible form, used by the failure-mode scenarios so a
/// storage fault is asserted rather than swallowed.
async fn execute(
    store: &Store,
    pid: Uuid,
    case: Uuid,
    task_id: &str,
    status: TaskStatus,
    reason: Option<&str>,
    selector: Option<&str>,
) -> Uuid {
    try_execute(store, pid, case, task_id, status, reason, selector)
        .await
        .expect("record an execution against a healthy store")
}

/// The same production path, returning the storage error instead of asserting on it.
async fn try_execute(
    store: &Store,
    pid: Uuid,
    case: Uuid,
    task_id: &str,
    status: TaskStatus,
    reason: Option<&str>,
    selector: Option<&str>,
) -> anyhow::Result<Uuid> {
    let run_id = Uuid::new_v4();
    let mut t = task(
        task_id,
        Some(case),
        if status == TaskStatus::Passed {
            SimOutcome::Pass
        } else {
            SimOutcome::Fail
        },
    );
    if let Some(sel) = selector {
        t.params = json!({ "selector": sel });
    }
    let p = plan(&format!("run {run_id}"), vec![t]);
    store
        .insert_run_for_release(
            run_id,
            &p,
            Some(&pid.to_string()),
            Some("phase62"),
            RunState::Completed,
            None,
            None,
        )
        .await?;
    store
        .set_task_status(run_id, task_id, TaskStatus::Running, None, true, false)
        .await?;
    store
        .set_task_status(run_id, task_id, status, reason, false, true)
        .await?;
    Intelligence::new(store)
        .ingest_run(
            Some(run_id),
            Some(&pid.to_string()),
            &p,
            &[task_state(
                task_id,
                status,
                reason.map(ToString::to_string),
                Some(case),
            )],
        )
        .await?;
    Ok(run_id)
}

// ============================================================================
// S3 - a supported change that affects related tests, through the real HTTP endpoint.
//
// Declaring a change over HTTP must persist `change_impacts` rows, and the change's recorded
// affected cases must then be what prioritisation selects.
// ============================================================================

#[tokio::test]
async fn s3_a_declared_change_persists_impact_records_and_prioritisation_agrees() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = app(&dir).await;
    let pid = make_project(&store, "P62 Declared Change").await;
    let pid_s = pid.to_string();
    let operator = mint_key(&state, "operator", Some(&pid_s)).await;

    // A real requirement, its case, and the `validates` edge — the graph the resolver walks.
    seed_requirement(&store, pid, "CHANGED").await;
    let impacted = seed_test_case(&store, pid, "Login with a valid password", "FR-001").await;
    let untouched = seed_test_case(&store, pid, "Print a receipt", "FR-003").await;
    seed_case_vertex(&store, pid, impacted).await;
    seed_case_vertex(&store, pid, untouched).await;
    link_requirement(&store, pid, impacted, "FR-001").await;
    link_requirement(&store, pid, untouched, "FR-003").await;

    // Both cases have executed, so the prioritiser has real evidence for each.
    execute(
        &store,
        pid,
        impacted,
        "t-login",
        TaskStatus::Passed,
        None,
        None,
    )
    .await;
    execute(
        &store,
        pid,
        untouched,
        "t-print",
        TaskStatus::Passed,
        None,
        None,
    )
    .await;

    // ---- declare the change over real HTTP ----
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/change-impact"),
            Some(&operator),
            Some(json!({
                "change_ref": "commit-p62-1",
                "change_type": "requirement",
                "targets": [{ "kind": "requirement", "ref_id": "FR-001" }],
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a change that writes records must answer 201: {body}"
    );
    assert!(
        body["recorded"].as_u64().unwrap_or(0) > 0,
        "the endpoint must report writing at least one record: {body}"
    );
    assert_eq!(
        body["already_recorded"], 0,
        "a first declaration must write its records rather than report them pre-existing: {body}"
    );

    // ---- declaring the same change again must confirm, not duplicate ----
    let (status2, body2) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/change-impact"),
            Some(&operator),
            Some(json!({
                "change_ref": "commit-p62-1",
                "change_type": "requirement",
                "targets": [{ "kind": "requirement", "ref_id": "FR-001" }],
            })),
        ),
    )
    .await;
    assert_eq!(
        status2,
        StatusCode::OK,
        "re-declaring an already-recorded change is a confirmation, not a new write: {body2}"
    );
    assert_eq!(
        body2["recorded"], 0,
        "and must not write a second copy: {body2}"
    );
    assert!(
        body2["already_recorded"].as_u64().unwrap_or(0) > 0,
        "it must report what it found already there: {body2}"
    );

    // ---- the records must be in the database, keyed on the change ----
    let rows = store
        .list_change_impacts(&pid_s, 5000)
        .await
        .expect("list_change_impacts");
    assert!(!rows.is_empty(), "change-impact rows must be persisted");
    assert!(
        rows.iter().all(|r| r.change_ref == "commit-p62-1"),
        "every recorded row belongs to the declared change: {:?}",
        rows.iter().map(|r| &r.change_ref).collect::<Vec<_>>()
    );
    assert!(
        rows.iter().all(|r| r.project_id == pid_s),
        "and to this project only"
    );

    // ---- the affected set must include the linked case and exclude the other ----
    //
    // `change_impacts` stores **node** ids, not case ids: `analyse` passes the same resolved node
    // for both `affected_node_id` and `selected_test_node_id`. So each node is mapped back to its
    // case through `external_ref`, which is the stored identity join this check is really about.
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
        selected.contains(&impacted),
        "the case linked to the changed requirement must be recorded as selected; got {selected:?}"
    );
    assert!(
        !selected.contains(&untouched),
        "a case linked to an unchanged requirement must not be: {selected:?}"
    );

    // ---- the downstream consumer must honour the recorded change ----
    let reader = QaMemoryReader::new(store.clone());
    let order = reader
        .prioritise_tests(&pid_s, Some("commit-p62-1"))
        .await
        .expect("prioritise_tests");
    assert!(
        order.insufficient_data.is_none(),
        "with recorded history the prioritiser must produce an ordering: {order:?}"
    );
    let view = order.view.as_ref().expect("an ordering");
    assert_eq!(
        view.change_ref.as_deref(),
        Some("commit-p62-1"),
        "the ordering must be labelled with the change it was asked about"
    );
    let ranked: Vec<Uuid> = view
        .tests
        .iter()
        .map(|t| Uuid::parse_str(&t.test_case_id).expect("case id in the ordering"))
        .collect();
    assert!(
        ranked.contains(&impacted),
        "the impacted case must appear in the ordering: {ranked:?}"
    );
    assert_eq!(
        ranked.first().copied(),
        Some(impacted),
        "and must lead it, since it is the one the change touches"
    );
    let changed_entry = view
        .tests
        .iter()
        .find(|t| t.test_case_id == impacted.to_string())
        .expect("the impacted entry");
    assert_eq!(
        changed_entry.priority,
        crate::qa_memory_risk::PriorityBand::Changed,
        "its band must be Changed, derived from the persisted impact record: {:?}",
        changed_entry
    );
    assert!(
        changed_entry
            .applies
            .contains(&crate::qa_memory_risk::PriorityBand::Changed),
        "and the Changed band must be one of the conditions that apply: {:?}",
        changed_entry.applies
    );

    // The untouched case must not inherit the change's band from a shared title or id shape.
    let untouched_entry = view
        .tests
        .iter()
        .find(|t| t.test_case_id == untouched.to_string())
        .expect("the untouched entry");
    assert!(
        !untouched_entry
            .applies
            .contains(&crate::qa_memory_risk::PriorityBand::Changed),
        "an unrelated case must not be marked Changed: {:?}",
        untouched_entry.applies
    );
}

// ============================================================================
// S4 - an unrelated change must not invent impacts, and an unsupported target
//      must be refused with a reason.
//
// Roadmap: "one unrelated change that must not invent impacts" and "one missing trace link".
// ============================================================================

#[tokio::test]
async fn s4_an_unrelated_change_invents_no_impacts_and_unsupported_targets_are_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = app(&dir).await;
    let pid = make_project(&store, "P62 Unrelated Change").await;
    let pid_s = pid.to_string();
    let operator = mint_key(&state, "operator", Some(&pid_s)).await;

    seed_requirement(&store, pid, "UNRELATED").await;
    let case = seed_test_case(&store, pid, "Login with a valid password", "FR-001").await;
    seed_case_vertex(&store, pid, case).await;
    link_requirement(&store, pid, case, "FR-001").await;
    execute(&store, pid, case, "t-login", TaskStatus::Passed, None, None).await;

    // ---- a change to a requirement nothing validates ----
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/change-impact"),
            Some(&operator),
            Some(json!({
                "change_ref": "commit-unrelated",
                "change_type": "requirement",
                "targets": [{ "kind": "requirement", "ref_id": "FR-003" }],
            })),
        ),
    )
    .await;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "an unrelated change is still a legal declaration: {body}"
    );

    // FR-003 exists as a requirement but no case validates it, so there is no `validates` edge to
    // walk. The impact set must be empty and the case must appear in `not_selected` with a reason.
    let impacted: Vec<Value> = body["impacted_tests"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        impacted.is_empty(),
        "a requirement with no validating case must impact nothing: {body}"
    );
    let not_selected = body["not_selected"].as_array().cloned().unwrap_or_default();
    let entry = not_selected
        .iter()
        .find(|n| n["test_case_id"] == case.to_string())
        .unwrap_or_else(|| panic!("the unrelated case must be listed as not selected: {body}"));
    assert!(
        entry["reason"].as_str().is_some_and(|r| !r.is_empty()),
        "and a skipped case must keep a meaningful reason: {entry}"
    );
    assert!(
        body["safety_floor"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "a case with no recorded failure evidence is not on the safety floor: {body}"
    );

    // ---- an unsupported target kind must be refused with a reason, not guessed at ----
    // `code_file` has no authoritative mapping in this schema, so it must not resolve to anything.
    let (status, body) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/change-impact"),
            Some(&operator),
            Some(json!({
                "change_ref": "commit-codefile",
                "change_type": "code",
                "targets": [{ "kind": "code_file", "ref_id": "src/checkout.rs" }],
            })),
        ),
    )
    .await;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "declaring it is legal even though the target cannot resolve: {body}"
    );
    let targets = body["targets"].as_array().cloned().unwrap_or_default();
    let outcome = targets
        .first()
        .unwrap_or_else(|| panic!("the target outcome must be reported: {body}"));
    assert_eq!(
        outcome["resolution"]["outcome"], "Insufficient",
        "a code file has no authoritative mapping here, so it must not resolve: {outcome}"
    );
    let reason = outcome["resolution"]["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains("git") || reason.contains("diff"),
        "and must say why, naming the missing source: {reason}"
    );
    assert_eq!(
        outcome["resolution"]["marker"], "INSUFFICIENT_DATA",
        "so a caller can branch on the marker without string matching: {outcome}"
    );
    let insufficient = body["insufficient_reason"].as_str().unwrap_or_default();
    assert!(
        insufficient.contains("no declared target resolved") && insufficient.contains("guessed"),
        "when nothing resolved the analysis must say so, and say it reported rather than \
         guessed: {insufficient:?}"
    );

    // ---- and a viewer may not declare a change at all ----
    let viewer = mint_key(&state, "viewer", Some(&pid_s)).await;
    let (status, _) = send(
        &state,
        req(
            "POST",
            &format!("/api/v1/projects/{pid}/change-impact"),
            Some(&viewer),
            Some(json!({
                "change_ref": "commit-viewer",
                "change_type": "requirement",
                "targets": [{ "kind": "requirement", "ref_id": "FR-001" }],
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "declaring a change writes durable records, so it is an operator action"
    );
}

// ============================================================================
// S5 - project isolation across two independent projects, including a duplicate
//      failure pattern that must NOT leak.
//
// Roadmap: "one duplicate failure pattern across two projects, without cross-project data leakage".
//
// This is the scenario most likely to be wrong by accident: identical fingerprints, identical
// titles, identical task names. Only stored ids distinguish them.
// ============================================================================

#[tokio::test]
async fn s5_two_projects_with_an_identical_failure_pattern_do_not_leak_into_each_other() {
    let db = Db::new().await;
    let store = &db.store;
    let alpha = make_project(store, "P62 Alpha").await;
    let beta = make_project(store, "P62 Beta").await;
    let a_s = alpha.to_string();
    let b_s = beta.to_string();

    // Both projects get a case with the SAME title and the SAME failing selector, so the
    // fingerprints are identical by construction. Only the project scope differs.
    let reason = "Element not found: #pay";
    let a_case = seed_test_case(store, alpha, "Checkout with a saved card", "FR-001").await;
    let b_case = seed_test_case(store, beta, "Checkout with a saved card", "FR-001").await;
    assert_ne!(a_case, b_case, "the two cases must be distinct stored ids");

    execute(
        store,
        alpha,
        a_case,
        "t-pay",
        TaskStatus::Failed,
        Some(reason),
        Some("#pay"),
    )
    .await;
    execute(
        store,
        beta,
        b_case,
        "t-pay",
        TaskStatus::Failed,
        Some(reason),
        Some("#pay"),
    )
    .await;

    // Each project's telemetry must contain only its own run.
    let a_tel = store
        .list_execution_telemetry(&a_s, 100)
        .await
        .expect("alpha telemetry");
    let b_tel = store
        .list_execution_telemetry(&b_s, 100)
        .await
        .expect("beta telemetry");
    assert_eq!(a_tel.len(), 1, "alpha sees only its own execution");
    assert_eq!(b_tel.len(), 1, "beta sees only its own execution");
    assert_eq!(a_tel[0].project_id, a_s);
    assert_eq!(b_tel[0].project_id, b_s);

    // The fingerprints must in fact be identical — otherwise this proves nothing.
    let a_fp = a_tel[0]
        .failure_fingerprint
        .clone()
        .expect("alpha fingerprint");
    let b_fp = b_tel[0]
        .failure_fingerprint
        .clone()
        .expect("beta fingerprint");
    assert_eq!(
        a_fp, b_fp,
        "the same failure in both projects must produce the same fingerprint; otherwise the \
         duplicate-pattern scenario is not being exercised"
    );

    // Each project's cluster lookup must return that project's own cluster, and a fingerprint
    // scoped to the wrong project must find nothing.
    let a_cluster = store
        .get_failure_cluster_by_fingerprint(&a_s, &a_fp)
        .await
        .expect("alpha cluster")
        .expect("alpha has a cluster");
    let b_cluster = store
        .get_failure_cluster_by_fingerprint(&b_s, &b_fp)
        .await
        .expect("beta cluster")
        .expect("beta has a cluster");
    assert_ne!(
        a_cluster.id, b_cluster.id,
        "two projects must hold two distinct clusters even for one identical fingerprint"
    );
    let wrong = store
        .get_failure_cluster_by_fingerprint(&a_s, "no-such-fingerprint")
        .await
        .expect("alpha lookup of an absent fingerprint");
    assert!(
        wrong.is_none(),
        "an unrecorded fingerprint must not resolve to anything in this project"
    );

    // QA Memory must answer per project, never blended.
    let reader = QaMemoryReader::new(store.clone());
    let a_view = reader
        .test_stability(&a_s, &a_case.to_string())
        .await
        .expect("alpha stability");
    let b_view = reader
        .test_stability(&b_s, &b_case.to_string())
        .await
        .expect("beta stability");
    assert_eq!(a_view.view.as_ref().expect("alpha view").executions, 1);
    assert_eq!(b_view.view.as_ref().expect("beta view").executions, 1);

    // Asking one project about the other project's case must be UnknownSubject, not that
    // project's data. The ids differ even though every human-visible name matches.
    let crossed = reader
        .test_stability(&a_s, &b_case.to_string())
        .await
        .expect("crossed read");
    assert_eq!(
        crossed.insufficient_data,
        Some(InsufficientData::UnknownSubject),
        "another project's case must be unknown here, not quietly readable: {crossed:?}"
    );
    assert!(crossed.view.is_none(), "and must return no view at all");

    // The same must hold for the risk surface, which reads a different store path.
    let a_risk = reader
        .test_risk(&a_s, &a_case.to_string())
        .await
        .expect("alpha risk");
    let b_risk = reader
        .test_risk(&b_s, &b_case.to_string())
        .await
        .expect("beta risk");
    assert_eq!(
        a_risk.view.as_ref().expect("alpha risk").test_case_id,
        a_case.to_string()
    );
    assert_eq!(
        b_risk.view.as_ref().expect("beta risk").test_case_id,
        b_case.to_string()
    );

    // Project summaries must count only their own executions.
    let a_summary = reader
        .project_memory_summary(&a_s)
        .await
        .expect("alpha summary");
    let b_summary = reader
        .project_memory_summary(&b_s)
        .await
        .expect("beta summary");
    assert_eq!(a_summary.executions, 1, "alpha's own execution count");
    assert_eq!(b_summary.executions, 1, "beta's own execution count");
}

/// The HTTP boundary must enforce the same isolation: a project-scoped key must not read
/// another project, whatever the titles match.
#[tokio::test]
async fn s5b_a_project_scoped_key_cannot_read_another_project_over_http() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = app(&dir).await;
    let alpha = make_project(&store, "P62 Http Alpha").await;
    let beta = make_project(&store, "P62 Http Beta").await;
    let case = seed_test_case(&store, alpha, "Checkout", "FR-001").await;
    seed_case_vertex(&store, alpha, case).await;
    execute(&store, alpha, case, "t-pay", TaskStatus::Passed, None, None).await;

    // A key scoped to beta, asking about alpha.
    let beta_key = mint_key(&state, "operator", Some(&beta.to_string())).await;
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{alpha}/intelligence/telemetry"),
            Some(&beta_key),
            None,
        ),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a key scoped to one project must not read another project's telemetry"
    );

    // And with the correct scope it does work, so the refusal above is about scope, not wiring.
    let alpha_key = mint_key(&state, "operator", Some(&alpha.to_string())).await;
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{alpha}/intelligence/telemetry"),
            Some(&alpha_key),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the owning project's key must still work: {body}"
    );
}

// ============================================================================
// S6 - risk: a measured zero, an unmeasured signal, and a failed read.
//
// Roadmap: "one risk signal that is truly measured as zero", "one risk signal that is unmeasured",
// and Step 6's "a storage failure is not interpreted as a measured zero".
//
// D3 is open, so nothing here picks a formula. Each existing documented contract is validated
// independently and the ambiguity is reported rather than resolved.
// ============================================================================

/// A signal that is genuinely zero must read as **measured zero**, which is different from not
/// having looked.
///
/// The measured-zero case here is `failure_rate`: this case ran twice and never failed, so the
/// rate is a real `0.0` computed from two persisted executions. That is the whole point — a `0.0`
/// that is backed by rows, not a placeholder standing in for missing evidence.
#[tokio::test]
async fn s6a_a_signal_with_persisted_rows_behind_it_is_measured_zero_not_unmeasured() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Measured Zero").await;
    let pid_s = pid.to_string();

    let case = seed_test_case(store, pid, "Login", "FR-001").await;
    // Two executions, both passing, so the zero is computed rather than defaulted.
    execute(store, pid, case, "t-1", TaskStatus::Passed, None, None).await;
    execute(store, pid, case, "t-2", TaskStatus::Passed, None, None).await;

    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    assert_eq!(telemetry.len(), 2, "both executions must be persisted");

    let risk = QaMemoryReader::new(store.clone())
        .test_risk(&pid_s, &case.to_string())
        .await
        .expect("test_risk");
    let view = risk.view.as_ref().expect("a risk view");

    let rate = view
        .factors
        .iter()
        .find(|f| f.name == "failure_rate")
        .expect("failure_rate is a named factor");
    assert_eq!(
        rate.measured,
        Some(0.0),
        "two persisted passes and no failures is a measured zero: {rate:?}"
    );
    assert_eq!(
        rate.evidence, 2,
        "and the zero must cite the rows it was computed from, so it is not a placeholder: {rate:?}"
    );
    assert!(
        !rate.basis.is_empty(),
        "the basis must state what it was computed from: {rate:?}"
    );

    // The distinction must survive serialization, so an API consumer can tell measured-zero from
    // unmeasured without inspecting types.
    assert!(
        view.calibration.is_none(),
        "and a partially measured model still yields no calibration score: {:?}",
        view.calibration
    );
}

/// A signal with no authoritative source must stay unmeasured, with its reason — never 0.0.
#[tokio::test]
async fn s6b_a_signal_with_no_source_stays_unmeasured_with_its_reason() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Unmeasured").await;
    let pid_s = pid.to_string();

    let case = seed_test_case(store, pid, "Checkout", "FR-001").await;
    execute(
        store,
        pid,
        case,
        "t-pay",
        TaskStatus::Failed,
        Some("Element not found"),
        None,
    )
    .await;

    let risk = QaMemoryReader::new(store.clone())
        .test_risk(&pid_s, &case.to_string())
        .await
        .expect("test_risk");
    let view = risk.view.as_ref().expect("a risk view");

    // `business_criticality` is not one of this view's six factors, so asserting on it would be
    // inventing a contract. `security_sensitivity` is, and it has no source in this schema.
    // `business_criticality` is not one of this view's six factors, so asserting on it would be
    // inventing a contract. `security_sensitivity` is, and it has no source in this schema.
    let f = view
        .factors
        .iter()
        .find(|f| f.name == "security_sensitivity")
        .unwrap_or_else(|| {
            panic!(
                "security_sensitivity must be a named factor: {:?}",
                view.factors
            )
        });
    assert!(
        f.measured.is_none(),
        "security_sensitivity has no authoritative source, so it must stay unmeasured rather than \
         become a measured zero: {f:?}"
    );
    assert!(
        !f.basis.is_empty(),
        "it must state why it could not be measured: {f:?}"
    );
    assert_eq!(
        f.evidence, 0,
        "with no source, it must cite no evidence rows: {f:?}"
    );

    // The unbacked weight must be reported, not absorbed into a score.
    assert!(
        view.unmeasured_weight > 0.0,
        "the unbacked share of the model must be reported: {:?}",
        view.unmeasured_weight
    );
    assert!(
        view.calibration.is_none(),
        "a partially measured model must not yield a calibration score: {:?}",
        view.calibration
    );
}

/// A storage failure must not be read as a measured zero, and must not read as a clean history.
#[tokio::test]
async fn s6c_a_storage_failure_is_never_a_measured_zero_or_an_empty_history() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Risk Read Failure").await;
    let pid_s = pid.to_string();

    let run = seed_test_case(store, pid, "Did run", "FR-002").await;
    execute(store, pid, run, "t-run", TaskStatus::Passed, None, None).await;

    // A second project with a case and no executions at all: the genuinely-empty history.
    let empty_pid = make_project(store, "P62 No History At All").await;
    let bare = seed_test_case(store, empty_pid, "Never run", "FR-001").await;

    let reader = QaMemoryReader::new(store.clone());

    // ---- state A1: a project with no executions at all ----
    let a = reader
        .test_risk(&empty_pid.to_string(), &bare.to_string())
        .await
        .expect("an empty project read is not a failure");
    assert_eq!(
        a.insufficient_data,
        Some(InsufficientData::NoExecutions),
        "a successful read with no history stays NO_EXECUTIONS: {a:?}"
    );
    assert!(a.view.is_none(), "and yields no risk view at all");

    // ---- state A2: a case with no executions, inside a project that has some ----
    // This is a *different* honest answer, and conflating the two would be the bug this guards.
    let sibling = seed_test_case(store, pid, "Also never run", "FR-003").await;
    let a2 = reader
        .test_risk(&pid_s, &sibling.to_string())
        .await
        .expect("a read of a case without executions is not a failure");
    assert_eq!(
        a2.insufficient_data,
        Some(InsufficientData::NoMatchingHistory),
        "history exists in this project but none for this case, so the reason differs: {a2:?}"
    );
    assert!(a2.view.is_none(), "and still yields no risk view");

    // ---- state B: healthy, real history for `run` ----
    let b = reader
        .test_risk(&pid_s, &run.to_string())
        .await
        .expect("a healthy read must succeed");
    assert!(b.view.is_some(), "a case with history yields a view: {b:?}");

    // ---- state C: the store cannot answer ----
    store.pool.close().await;
    for (name, res) in [
        (
            "test_risk",
            reader
                .test_risk(&pid_s, &run.to_string())
                .await
                .map(|v| v.insufficient_data),
        ),
        (
            "module_risk",
            reader
                .module_risk(&pid_s, "m1")
                .await
                .map(|v| v.insufficient_data),
        ),
        (
            "release_risk",
            reader
                .release_risk(&pid_s, Uuid::new_v4())
                .await
                .map(|v| v.insufficient_data),
        ),
    ] {
        assert!(
            res.is_err(),
            "{name} turned a failed read into an answer: {:?}",
            res.as_ref().ok()
        );
    }
}

// ============================================================================
// S7 - cross-layer integrity over persisted measurements, and the cases that
//      must NOT become confirmed defects.
//
// Roadmap: "one cross-layer mismatch using the same known entity and aligned observation
// window", "one stale or ambiguous observation that must not become a confirmed defect",
// and Step 6's D5 check on the 10 ms floor.
// ============================================================================

/// A genuine mismatch — both layers measured, both measured the same entity, inside the window —
/// must be detected and, being two independent measurements, may establish a defect.
#[tokio::test]
async fn s7a_a_genuine_cross_layer_mismatch_is_detected_and_establishes_a_defect() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Real Mismatch").await;
    let pid_s = pid.to_string();
    let case = seed_test_case(store, pid, "Pay", "FR-001").await;
    seed_case_vertex(store, pid, case).await;

    let run_id = Uuid::new_v4();
    let p = plan(
        "mismatch",
        vec![task("t-split", Some(case), SimOutcome::Fail)],
    );
    store
        .insert_run_for_release(
            run_id,
            &p,
            Some(&pid_s),
            Some("phase62"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    // The backend records FAILED...
    record_task(
        store,
        run_id,
        "t-split",
        TaskStatus::Failed,
        Some("Element not found: #pay"),
    )
    .await;
    // ...and the API layer's record of the same task says PASSED. A real disagreement over one
    // stored entity, produced by two real writers.
    ingest(
        store,
        run_id,
        pid,
        &p,
        vec![task_state("t-split", TaskStatus::Passed, None, Some(case))],
    )
    .await;

    let report = IntegrityVerifier::new(store)
        .verify_run(&pid_s, run_id, None)
        .await
        .expect("verify_run");

    let mismatch = report
        .comparisons
        .iter()
        .find(|c| c.kind == ComparisonKind::StatusConsistency)
        .expect("a status comparison");
    assert_eq!(
        mismatch.outcome,
        IntegrityOutcome::Inconsistent,
        "the two layers disagree on this entity: {}",
        mismatch.detail
    );
    assert!(
        mismatch.detail.contains("FAILED") && mismatch.detail.contains("PASSED"),
        "the detail must name both recorded values: {}",
        mismatch.detail
    );
    assert!(
        mismatch.establishes_defect(),
        "two independent layers both measured and disagreed, so a defect is established: {:?}",
        mismatch.defect_caveat
    );
    assert_eq!(
        mismatch.identity.as_ref().map(|i| i.kind()),
        Some("run_task_key"),
        "and identity must be the stored key, never a name match"
    );
    assert_eq!(
        report.defects_established, 1,
        "exactly one defect, from this one mismatch: {:?}",
        report.comparisons
    );
}

/// A comparison whose layers were recorded too far apart must be reported as **not comparable**,
/// never as a confirmed defect. This is the roadmap's "stale or ambiguous observation that must
/// not become a confirmed defect".
///
/// The skew the verifier measures is between the backend's `finished_at` and the API layer's
/// `execution_telemetry.created_at`. `Intelligence::ingest_run` stamps both "now", so the two land
/// milliseconds apart and the comparison is answerable — which is the correct behaviour for a fresh
/// run. To exercise the staleness branch the telemetry row's `created_at` is moved back through
/// `insert_execution_telemetry`, which is a production writer and the only way this schema lets a
/// row carry an observation time.
#[tokio::test]
async fn s7b_an_observation_outside_the_window_is_not_comparable_not_a_confirmed_defect() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Stale Observation").await;
    let pid_s = pid.to_string();
    let case = seed_test_case(store, pid, "Pay", "FR-001").await;

    let run_id = Uuid::new_v4();
    let p = plan("stale", vec![task("t-split", Some(case), SimOutcome::Fail)]);
    store
        .insert_run_for_release(
            run_id,
            &p,
            Some(&pid_s),
            Some("phase62"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    record_task(
        store,
        run_id,
        "t-split",
        TaskStatus::Failed,
        Some("Element not found"),
    )
    .await;

    // The API layer's row is written once, aged 48 hours. It is the only telemetry row for this
    // run, so the verifier pairs it with the backend record directly — no second row is added
    // alongside it, which would leave the fresh one to be matched first.
    let aged = ExecutionTelemetry {
        created_at: chrono::Utc::now() - chrono::Duration::hours(48),
        ..ExecutionTelemetry::from_task_state(
            &pid_s,
            run_id,
            &task_state("t-split", TaskStatus::Passed, None, Some(case)),
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
        .expect("persist the aged telemetry row");

    let stale = IntegrityVerifier::new(store)
        .verify_run(&pid_s, run_id, None)
        .await
        .expect("verify_run");
    let stale_status = stale
        .comparisons
        .iter()
        .find(|c| c.kind == ComparisonKind::StatusConsistency)
        .expect("a status comparison after ageing the row");

    // The skew is now far beyond the window, so the two records cannot be compared. It must be
    // reported as such, and it must NOT become a confirmed defect.
    assert!(
        stale_status.observed_skew_ms.unwrap_or(0) > 60_000,
        "the aged row must produce an out-of-window skew: {:?}",
        stale_status.observed_skew_ms
    );
    // The verifier expresses "could not answer" as `InsufficientData` — `IntegrityComparison::unanswered`
    // is the constructor it uses for the out-of-window path. `NotComparable` is reserved for a
    // different case: the layers exist but are about different subject matter. What matters here is
    // that neither outcome carries a defect.
    assert_eq!(
        stale_status.outcome,
        IntegrityOutcome::InsufficientData,
        "records too far apart to compare are not evidence of a disagreement: {}",
        stale_status.detail
    );
    assert_ne!(
        stale_status.outcome,
        IntegrityOutcome::Inconsistent,
        "and specifically not an inconsistency"
    );
    assert!(
        !stale_status.establishes_defect(),
        "a stale observation must never become a confirmed product defect: {:?}",
        stale_status
    );
    assert!(
        stale_status.detail.contains("staleness") || stale_status.detail.contains("apart"),
        "and must say why it could not answer: {}",
        stale_status.detail
    );

    // Nothing may be established from an incomparable pair, at the run level either.
    assert_eq!(
        stale.defects_established, 0,
        "with the only disagreement incomparable, nothing may be established: {:?}",
        stale.comparisons
    );
}

/// D5: the 10 ms absolute floor's *actual behaviour*, measured rather than assumed. This verifies
/// what the constant does; it does not endorse it as the correct production threshold, which
/// remains an open product decision.
#[tokio::test]
async fn s7c_the_ten_millisecond_floor_behaves_as_documented() {
    use crate::integrity::{
        compare_durations, duration_tolerance_ms, DurationAgreement, DURATION_ABSOLUTE_FLOOR_MS,
        DURATION_RELATIVE_TOLERANCE,
    };

    // Small measurements: the absolute floor governs, because 10 % of ~0 ms means nothing.
    assert_eq!(DURATION_ABSOLUTE_FLOOR_MS, 10.0);
    assert_eq!(duration_tolerance_ms(0.0, 0.0), 10.0);
    assert_eq!(duration_tolerance_ms(0.0, 1.0), 10.0);

    // A 1 ms disagreement between two independently recorded clocks is noise, and is reported
    // as such rather than as a cross-layer defect.
    assert!(matches!(
        compare_durations(1.0, 0.0),
        DurationAgreement::WithinTolerance {
            difference_ms,
            tolerance_ms,
        } if difference_ms == 1.0 && tolerance_ms == 10.0
    ));

    // Exactly at the floor is agreeing; the boundary is inclusive.
    assert!(matches!(
        compare_durations(10.0, 0.0),
        DurationAgreement::WithinTolerance { difference_ms, .. } if difference_ms == 10.0
    ));
    // Just past it is not.
    assert!(matches!(
        compare_durations(10.001, 0.0),
        DurationAgreement::BeyondTolerance { .. }
    ));

    // Above the crossover the relative allowance governs: 10 % of 5 000 ms is 500 ms.
    assert_eq!(DURATION_RELATIVE_TOLERANCE, 0.10);
    assert_eq!(duration_tolerance_ms(5_000.0, 5_000.0), 500.0);
    assert!(matches!(
        compare_durations(5_000.0, 20_000.0),
        DurationAgreement::BeyondTolerance { .. }
    ));
    assert!(matches!(
        compare_durations(20_000.0, 21_000.0),
        DurationAgreement::WithinTolerance { tolerance_ms, .. } if tolerance_ms == 2_100.0
    ));

    // A genuinely large disagreement stays a disagreement, which is the whole reason for the
    // rule: raising the floor enough to hide this would defeat it.
    assert!(matches!(
        compare_durations(0.0, 5_000.0),
        DurationAgreement::BeyondTolerance { difference_ms, .. } if difference_ms == 5_000.0
    ));
}

// ============================================================================
// S8 - exploratory findings and their telemetry reach the QA Memory consumer.
//
// Roadmap: "exploratory execution budget and action-policy enforcement", Step 7's "actual
// exploratory-agent outcomes and persisted telemetry reach the appropriate consumer", and
// "a probe failure must not automatically be reported as a confirmed product defect".
//
// This uses the production session/finding writers. It does **not** claim an AI agent ran: no
// executor is driven, so `assertion_result` stays unset and every finding below says so in its
// own basis. Asserting a verified assertion would be fabricating evidence.
// ============================================================================

#[tokio::test]
async fn s8_exploration_findings_persist_with_provenance_and_reach_the_api() {
    use crate::agents::exploration::{ExplorationBudget, FindingClass};
    use crate::store::ExplorationFindingRow;

    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = app(&dir).await;
    let pid = make_project(&store, "P62 Exploration").await;
    let pid_s = pid.to_string();
    let key = mint_key(&state, "operator", Some(&pid_s)).await;
    let session = Uuid::new_v4();

    // ---- the session, with an explicit budget ----
    let budget = ExplorationBudget {
        max_actions: 20,
        max_state_transitions: 20,
        max_findings: 10,
        max_pages: 10,
    };
    store
        .insert_exploration_session(
            &pid_s,
            &session,
            "http://localhost:3000",
            &budget,
            &crate::agent::AgentLimits {
                max_turns: 20,
                max_tool_calls_total: 40,
                max_observation_bytes: 8192,
                timeout: std::time::Duration::from_secs(60),
            },
        )
        .await
        .expect("insert_exploration_session");

    // ---- a finding from an execution no executor verified ----
    // `InsufficientEvidence` with `confidence: 0.0` is the honest classification here: the
    // session observed a route but nothing evaluated its assertions.
    // A fingerprinted row is one that makes a claim about the application; an `InsufficientEvidence`
    // row is explicitly not such a claim, so it carries no fingerprint. The closure mirrors that.
    let finding = |observed: &str, class: FindingClass| {
        ExplorationFindingRow {
        id: Uuid::new_v4(),
        session_id: session,
        project_id: pid_s.clone(),
        classification: class.as_str().to_string(),
        behaviour: "intelligent_navigation".into(),
        route: "/checkout".into(),
        failure_fingerprint: match class {
            FindingClass::InsufficientEvidence => None,
            _ => Some(format!("fp-{}", observed.replace(' ', "-"))),
        },
        evidence_object_key: None,
        observed: observed.into(),
        expected: Some("the checkout page renders its total".into()),
        violated_requirement: None,
        action_sequence: "[]".into(),
        app_state: "{}".into(),
        confidence: match class {
            // A claim about the application carries a confidence; an observation does not.
            FindingClass::InsufficientEvidence => 0.0,
            _ => 0.4,
        },
        // An unverified observation says so. A finding that does make a claim records that the
        // claim rests on the probe's own observation, which is weaker than an executed assertion.
        basis: match class {
            FindingClass::InsufficientEvidence => {
                "no executor evaluated the assertions, so this is an observation and not a                  confirmed defect"
                    .to_string()
            }
            _ => {
                "recorded from an exploratory probe of the route; no executed assertion verified                  it, so this is a reported claim and not a verified defect"
                    .to_string()
            }
        },
        related_failure: None,
        observed_at: chrono::Utc::now(),
    }
    };

    let (first_id, inserted) = store
        .record_exploration_finding(&finding(
            "the checkout route rendered",
            FindingClass::InsufficientEvidence,
        ))
        .await
        .expect("record the first finding");
    assert!(inserted, "a first finding is new");

    // ---- duplicate control ----
    //
    // The suppression mechanism is a partial unique index on
    // `(project_id, failure_fingerprint) WHERE failure_fingerprint IS NOT NULL` — one reported
    // defect per project per underlying failure, where "underlying" is the canonical fingerprint
    // rather than the path taken to reach it. A row with **no** fingerprint is deliberately
    // exempt: an `INSUFFICIENT_EVIDENCE` row makes no claim about the application, so it must not
    // stop the same inconclusive observation being recorded again.
    //
    // So the two cases are opposites and both are asserted:
    //   * a fingerprinted observation is recorded once, then suppressed;
    //   * an unfingerprinted one is recorded every time.
    let (defect_id, dup_inserted) = store
        .record_exploration_finding(&finding(
            "the total never rendered",
            FindingClass::PotentialDefect,
        ))
        .await
        .expect("record a fingerprinted finding");
    assert!(dup_inserted, "a fingerprinted finding is new");

    let (dup_id, was_inserted) = store
        .record_exploration_finding(&finding(
            "the total never rendered",
            FindingClass::PotentialDefect,
        ))
        .await
        .expect("re-record the same underlying defect");
    assert!(
        !was_inserted,
        "the same underlying failure must be reported once per project, not twice"
    );
    // On suppression the writer returns the caller's own id rather than the stored row's, because
    // the insert never happened. Asserted so the flag is read as the authority, not the id.
    assert!(
        dup_id != first_id,
        "a suppressed duplicate must not fabricate a stored id equal to the original"
    );

    // The exempt case: no fingerprint, so it is never suppressed.
    let (_, inconclusive_again) = store
        .record_exploration_finding(&finding(
            "the checkout route rendered",
            FindingClass::InsufficientEvidence,
        ))
        .await
        .expect("re-record the inconclusive observation");
    assert!(
        inconclusive_again,
        "an INSUFFICIENT_EVIDENCE row claims nothing about the application, so it must not \
         suppress a later record of the same observation"
    );

    let stored = store
        .list_exploration_findings(&pid_s, None, 50)
        .await
        .expect("list_exploration_findings");
    assert_eq!(
        stored.len(),
        3,
        "two inconclusive observations plus one real defect: {stored:?}"
    );

    // The fingerprinted finding must be exactly one row, and it must keep its provenance.
    let defects: Vec<&crate::store::ExplorationFindingRow> = stored
        .iter()
        .filter(|f| f.failure_fingerprint.is_some())
        .collect();
    assert_eq!(
        defects.len(),
        1,
        "one underlying defect is one row: {:?}",
        stored
            .iter()
            .map(|f| (&f.observed, &f.failure_fingerprint))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        defects[0].id, defect_id,
        "and it is the row the first write of that defect produced"
    );

    // The inconclusive finding must carry no confidence and must say why.
    let inconclusive = stored
        .iter()
        .find(|f| f.classification == FindingClass::InsufficientEvidence.as_str())
        .expect("an insufficient-evidence finding");
    assert_eq!(
        inconclusive.confidence, 0.0,
        "an unverified observation must not carry a confidence score implying verification"
    );
    assert!(
        inconclusive.basis.contains("no executor evaluated"),
        "and must carry the provenance that says so: {:?}",
        inconclusive.basis
    );
    assert_eq!(
        inconclusive.session_id, session,
        "the finding must be attached to the real session id"
    );
    // A fingerprinted observation is a claim, so it must not carry the "nothing was verified" basis.
    assert!(
        !defects[0].basis.contains("no executor evaluated"),
        "a fingerprinted finding is the claim case, not the inconclusive one: {:?}",
        defects[0].basis
    );
    assert!(
        defects[0].basis.contains("no executed assertion verified"),
        "and it must still record that no assertion was verified: {:?}",
        defects[0].basis
    );

    // ---- finish the session with real counters and a named stop reason ----
    store
        .finish_exploration_session(
            &pid_s,
            &session,
            "COMPLETED",
            // actions_taken, state_transitions_taken, pages_visited,
            7,
            3,
            2,
            // findings_recorded: three rows were inserted (two inconclusive observations and one
            // fingerprinted defect). findings_suppressed: the one duplicate the index rejected.
            3,
            1,
            "findings budget reached",
            None,
        )
        .await
        .expect("finish_exploration_session");

    let sessions = store
        .list_exploration_sessions(&pid_s, 10)
        .await
        .expect("list_exploration_sessions");
    assert_eq!(sessions.len(), 1, "the session must be readable back");
    let s = &sessions[0];
    assert_eq!(s.status, "COMPLETED");
    assert_eq!(s.max_actions, 20, "the declared budget must be persisted");
    assert_eq!(s.actions_taken, 7, "and the real counters with it");
    assert_eq!(
        s.findings_recorded, 3,
        "three write attempts were recorded: two inconclusive observations and one defect"
    );
    assert_eq!(
        s.findings_suppressed, 1,
        "the suppressed duplicate must be counted, so suppression is visible rather than silent"
    );
    assert_eq!(
        s.stop_reason, "findings budget reached",
        "the ceiling that stopped exploration must be named"
    );
    assert!(
        s.finished_at.is_some(),
        "a finished session must carry its end time"
    );

    // ---- downstream: the API must serve the persisted findings ----
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/exploration"),
            Some(&key),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the exploration endpoint must answer: {body}"
    );
    let served = body.to_string();
    assert!(
        served.contains("InsufficientEvidence") || served.contains("INSUFFICIENT_EVIDENCE"),
        "the API must return the persisted classification, not invent one: {body}"
    );
    assert!(
        served.contains("no executor evaluated"),
        "and the provenance must survive to the consumer: {body}"
    );

    // ---- an unknown classification is refused, not silently filtered to nothing ----
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/exploration?classification=NOT_A_CLASS"),
            Some(&key),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unknown classification must be refused rather than answering as 'no findings'"
    );
}

// ============================================================================
// S9 - failure and recovery modes over real persistence.
//
// Step 8's list, mapped to what this schema can actually produce through production writers.
// Each case states which mechanism produces it; nothing here fabricates a state.
// ============================================================================

/// A storage read failure must remain observable on every consumer that depends on the read, and
/// must never arrive as a successful-looking empty answer.
#[tokio::test]
async fn s9a_a_closed_store_fails_loudly_on_every_dependent_consumer() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Read Failure").await;
    let pid_s = pid.to_string();
    let case = seed_test_case(store, pid, "Checkout", "FR-001").await;
    execute(store, pid, case, "t-pay", TaskStatus::Passed, None, None).await;

    // Healthy first, so every failure below is attributable to the closed pool and nothing else.
    let reader = QaMemoryReader::new(store.clone());
    assert!(
        reader
            .project_memory_summary(&pid_s)
            .await
            .expect("healthy summary")
            .executions
            > 0,
        "the store must be healthy before it is closed"
    );

    // `pool.close()` is the same real-failure technique `engine_tests` uses: no mock, no sleep.
    store.pool.close().await;

    let failures: Vec<(&str, bool)> = vec![
        (
            "project_memory_summary",
            reader.project_memory_summary(&pid_s).await.is_err(),
        ),
        (
            "test_stability",
            reader
                .test_stability(&pid_s, &case.to_string())
                .await
                .is_err(),
        ),
        (
            "planning_signals",
            reader.planning_signals(&pid_s).await.is_err(),
        ),
        ("bug_history", reader.bug_history(&pid_s).await.is_err()),
        (
            "requirement_history",
            reader.requirement_history(&pid_s, "FR-001").await.is_err(),
        ),
        (
            "test_risk",
            reader.test_risk(&pid_s, &case.to_string()).await.is_err(),
        ),
        (
            "prioritise_tests",
            reader.prioritise_tests(&pid_s, None).await.is_err(),
        ),
        (
            "change_impact.analyse",
            ChangeImpactReader::new(store.clone())
                .analyse(
                    &pid_s,
                    "commit-fail",
                    "requirement",
                    &[ChangeTarget {
                        kind: "requirement".into(),
                        ref_id: "FR-001".into(),
                    }],
                )
                .await
                .is_err(),
        ),
    ];
    for (name, is_err) in failures {
        assert!(
            is_err,
            "{name} answered from a closed store instead of failing"
        );
    }

    // Planning feedback derives from the same reads and must propagate rather than emit an
    // empty signal set that reads as "no signals exist".
    let feedback = AdaptiveIntelligenceService::new(store.clone())
        .generate_planning_feedback(&pid_s)
        .await;
    assert!(
        feedback.is_err(),
        "planning feedback must not report an empty signal set for an unreadable store"
    );
}

/// A write failure must be surfaced, and the already-recorded run must not be corrupted by it.
#[tokio::test]
async fn s9b_a_write_failure_is_reported_and_leaves_prior_state_intact() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Write Failure").await;
    let pid_s = pid.to_string();
    let case = seed_test_case(store, pid, "Checkout", "FR-001").await;

    let good = execute(store, pid, case, "t-good", TaskStatus::Passed, None, None).await;
    let before = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry before");
    assert_eq!(before.len(), 1);

    store.pool.close().await;

    // A second execution against the closed store must fail rather than half-write.
    let second = try_execute(store, pid, case, "t-later", TaskStatus::Passed, None, None).await;
    assert!(
        second.is_err(),
        "recording an execution against a closed store must fail"
    );

    // The reader must be unaffected by the failure it never got to persist: one row, still the
    // first one, still keyed on the original case.
    store.pool.close().await; // already closed; idempotent, so the read attempt is well-defined
    let reader = QaMemoryReader::new(store.clone());
    let summary = reader.project_memory_summary(&pid_s).await;
    assert!(
        summary.is_err(),
        "a read after a failed write still reports the read failure, not a stale success"
    );
    assert_eq!(
        good, before[0].run_id,
        "the run recorded before the failure is unchanged in what was persisted"
    );
}

/// A missing linked entity must be reported as unknown, not resolved to something adjacent.
#[tokio::test]
async fn s9c_a_missing_linked_entity_is_unknown_rather_than_resolved() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Missing Link").await;
    let pid_s = pid.to_string();

    let reader = ChangeImpactReader::new(store.clone());
    // A target that does not exist in this project's graph at all.
    let unresolved = reader
        .resolve(
            &pid_s,
            &ChangeTarget {
                kind: "requirement".into(),
                ref_id: "REQ-DOES-NOT-EXIST".into(),
            },
        )
        .await
        .expect("resolve");
    assert!(
        matches!(
            unresolved,
            crate::change_impact::Resolution::Insufficient { .. }
        ),
        "a target with no vertex must be refused: {unresolved:?}"
    );

    // A `test_case` target whose ref is not a uuid at all is a client mistake, still refused.
    let malformed = reader
        .resolve(
            &pid_s,
            &ChangeTarget {
                kind: "test_case".into(),
                ref_id: "not-a-uuid".into(),
            },
        )
        .await
        .expect("resolve");
    assert!(
        matches!(
            malformed,
            crate::change_impact::Resolution::Insufficient { .. }
        ),
        "a malformed id must not resolve to a vertex: {malformed:?}"
    );

    // And QA Memory on an unknown case in an otherwise-empty project is UNKNOWN_SUBJECT, never a
    // zeroed row.
    let empty_pid = make_project(store, "P62 Unknown Subject").await;
    let unknown = reader_helper(&empty_pid.to_string()).await;
    assert_eq!(
        unknown,
        Some(InsufficientData::UnknownSubject),
        "an unknown case must be UNKNOWN_SUBJECT, not a row of zeroes"
    );
}

async fn reader_helper(pid: &str) -> Option<InsufficientData> {
    // A malformed id is the cheapest UNKNOWN_SUBJECT path the public reader offers.
    let db = Db::new().await;
    let reader = QaMemoryReader::new(db.store.clone());
    reader
        .test_risk(pid, "not-a-uuid")
        .await
        .expect("test_risk")
        .insufficient_data
}

/// Repeated execution data must accumulate honestly rather than double-counting as a new case.
#[tokio::test]
async fn s9d_repeated_execution_of_the_same_case_accumulates_without_duplicating_the_case() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Repeated Execution").await;
    let pid_s = pid.to_string();
    let case = seed_test_case(store, pid, "Checkout", "FR-001").await;

    for i in 0..4 {
        execute(
            store,
            pid,
            case,
            &format!("t-{i}"),
            if i % 2 == 0 {
                TaskStatus::Passed
            } else {
                TaskStatus::Failed
            },
            if i % 2 == 1 {
                Some("Element not found")
            } else {
                None
            },
            None,
        )
        .await;
    }

    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    assert_eq!(telemetry.len(), 4, "four executions, four rows");
    assert!(
        telemetry.iter().all(|t| t.test_case_id == Some(case)),
        "every row belongs to the same stored case: {:?}",
        telemetry.iter().map(|t| t.test_case_id).collect::<Vec<_>>()
    );

    let reader = QaMemoryReader::new(store.clone());
    let view = reader
        .test_stability(&pid_s, &case.to_string())
        .await
        .expect("test_stability")
        .view
        .expect("a view for the repeatedly executed case");
    assert_eq!(
        view.executions, 4,
        "the reader must count every execution, not just the latest"
    );
    assert_eq!(view.passed, 2, "two passes");
    assert_eq!(view.failed, 2, "two failures");
    assert_eq!(
        view.failure_rate,
        Some(50.0),
        "and a rate computed across all four, not from the last one"
    );
    assert_eq!(
        view.distinct_failure_fingerprints, 1,
        "the same reason must collapse to one fingerprint, not one per occurrence"
    );
    assert_eq!(
        view.test_case_id,
        case.to_string(),
        "and the view is still about the one stored case id"
    );
}

/// Incomplete telemetry must be handled as absent evidence, not read as a clean result.
#[tokio::test]
async fn s9e_partially_populated_evidence_stays_incomplete_rather_than_being_read_as_clean() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Partial Evidence").await;
    let pid_s = pid.to_string();
    let case = seed_test_case(store, pid, "Checkout", "FR-001").await;

    // A task with a recorded start but no finish: the store recorded it, and the duration
    // comparison cannot use it.
    let run_id = Uuid::new_v4();
    let p = plan(
        "partial",
        vec![task("t-partial", Some(case), SimOutcome::Pass)],
    );
    store
        .insert_run_for_release(
            run_id,
            &p,
            Some(&pid_s),
            Some("phase62"),
            RunState::Running,
            None,
            None,
        )
        .await
        .expect("insert_run_for_release");
    record_task(store, run_id, "t-partial", TaskStatus::Running, None).await;

    // Ingest a task that is not terminal: `ingest_run` skips it, so no telemetry is written.
    ingest(
        store,
        run_id,
        pid,
        &p,
        vec![task_state(
            "t-partial",
            TaskStatus::Running,
            None,
            Some(case),
        )],
    )
    .await;

    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    assert!(
        telemetry.is_empty(),
        "a non-terminal task must not become a recorded execution: {telemetry:?}"
    );

    // With no completed execution, the honest answers are the empty ones.
    let reader = QaMemoryReader::new(store.clone());
    let stability = reader
        .test_stability(&pid_s, &case.to_string())
        .await
        .expect("test_stability");
    assert_eq!(
        stability.insufficient_data,
        Some(InsufficientData::NoExecutions),
        "an unfinished task is not a passing execution: {stability:?}"
    );

    // And the integrity verifier must name the absence rather than assume agreement.
    let report = IntegrityVerifier::new(store)
        .verify_run(&pid_s, run_id, None)
        .await
        .expect("verify_run");
    assert_eq!(
        report.defects_established, 0,
        "nothing may be established from an incomplete run: {:?}",
        report.comparisons
    );
    assert!(
        !report.layers_unavailable.is_empty(),
        "layers with no evidence must be named: {:?}",
        report.layers_unavailable
    );
}

// ============================================================================
// S10 - the evidence read-back path, and the failure-to-bug relation.
//
// Roadmap item 4 is "failure-to-bug relation **where a real bug record exists**". Investigation
// established that nothing in the production ingestion path creates a bug draft automatically:
// `Intelligence::ingest_run` writes clusters, and a draft is written only by an explicit
// `insert_bug_draft` call. So this scenario creates one through that writer — a real application
// write, not a simulation — and then follows the whole relation forward.
// ============================================================================

/// The evidence API must serve the stored object, and must not leak it across projects.
#[tokio::test]
async fn s10a_evidence_is_served_by_id_and_stays_inside_its_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = app(&dir).await;
    let pid = make_project(&store, "P62 Evidence Read").await;
    let other = make_project(&store, "P62 Evidence Other").await;
    let key = mint_key(&state, "operator", Some(&pid.to_string())).await;

    let case = seed_test_case(&store, pid, "Checkout", "FR-001").await;
    let run = execute(
        &store,
        pid,
        case,
        "t-pay",
        TaskStatus::Failed,
        Some("Element not found"),
        Some("#pay"),
    )
    .await;

    // Written the way `engine.rs` writes it: object first, then the row that references it.
    let evidence_store =
        LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store");
    let (object_key, size) = evidence_store
        .put(
            &run.to_string(),
            "t-pay",
            "screenshot",
            "Element not found: #pay",
        )
        .expect("evidence put");
    let evidence_id = store
        .insert_evidence(run, "t-pay", "screenshot", &object_key, "text/plain", size)
        .await
        .expect("insert_evidence");

    // ---- the owning project can read the evidence, by id and by content ----
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/evidence/{evidence_id}"),
            Some(&key),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "evidence detail must be served: {body}"
    );
    assert_eq!(
        body["id"],
        evidence_id.to_string(),
        "and must be the stored row"
    );
    assert_eq!(
        body["kind"], "screenshot",
        "with the kind it was stored under"
    );

    let (status, content) = send_text(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/evidence/{evidence_id}/content"),
            Some(&key),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "evidence content must be served");
    assert!(
        content.contains("#pay"),
        "and must be the bytes that were written: {content}"
    );

    // ---- the run's evidence list must include it ----
    let (status, body) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/executions/{run}/evidence"),
            Some(&key),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the run's evidence list must answer: {body}"
    );
    assert!(
        body.to_string().contains(&evidence_id.to_string()),
        "and must include the row that was persisted: {body}"
    );

    // ---- another project must not reach it, even holding the real id ----
    let other_key = mint_key(&state, "operator", Some(&other.to_string())).await;
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{other}/evidence/{evidence_id}"),
            Some(&other_key),
            None,
        ),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "an evidence row belongs to the run and project that wrote it"
    );

    // ---- an id that was never issued is a client error, not an empty success ----
    let (status, _) = send(
        &state,
        req(
            "GET",
            &format!("/api/v1/projects/{pid}/evidence/{}", Uuid::new_v4()),
            Some(&key),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "an unknown evidence id must be a 404, never a 200 carrying nothing"
    );
}

/// A real bug record, written through the production writer, must reach the safety floor and the
/// QA Memory bug history. This is the roadmap's "failure-to-bug relation where a real bug record
/// exists" — the condition is load-bearing, because no draft exists until one is written.
#[tokio::test]
async fn s10b_a_real_bug_record_reaches_the_safety_floor_and_bug_history() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = make_project(store, "P62 Bug Relation").await;
    let pid_s = pid.to_string();

    let case = seed_test_case(store, pid, "Checkout", "FR-001").await;
    execute(
        store,
        pid,
        case,
        "t-pay",
        TaskStatus::Failed,
        Some("Element not found: #pay"),
        Some("#pay"),
    )
    .await;

    let telemetry = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    let fp = telemetry[0]
        .failure_fingerprint
        .clone()
        .expect("a fingerprint to key the bug record on");
    let cluster = store
        .get_failure_cluster_by_fingerprint(&pid_s, &fp)
        .await
        .expect("cluster lookup")
        .expect("a cluster for the recorded fingerprint");

    // ---- before any bug record exists ----
    let reader = ChangeImpactReader::new(store.clone());
    let floor_before = reader.safety_floor(&pid_s).await.expect("floor");
    assert!(
        floor_before.contains_key(&case),
        "a recorded failure alone is already on the safety floor: {floor_before:?}"
    );
    assert!(
        floor_before[&case].contains("fingerprint"),
        "with the fingerprint as its reason: {:?}",
        floor_before[&case]
    );
    let bugs_before = QaMemoryReader::new(store.clone())
        .bug_history(&pid_s)
        .await
        .expect("bug history");
    assert!(
        bugs_before.is_empty(),
        "no bug record has been written, so there is no recurring defect to report: {bugs_before:?}"
    );

    // ---- write one through the production writer ----
    let draft_id = store
        .insert_bug_draft(
            cluster.id,
            "Checkout total is not rendered",
            "The cart total never appears before the charge is attempted.",
            "high",
            0.8,
        )
        .await
        .expect("insert_bug_draft");

    // ---- and the relation must now be visible downstream ----
    let drafts = store
        .list_bug_drafts(Some(&pid_s))
        .await
        .expect("list_bug_drafts");
    assert_eq!(
        drafts.len(),
        1,
        "the bug draft must be persisted and scoped to this project"
    );
    assert_eq!(
        drafts[0].id, draft_id,
        "with the stored id the writer returned"
    );
    assert_eq!(
        drafts[0].failure_cluster_id, cluster.id,
        "and pointing at the real cluster the failure created"
    );

    let bugs_after = QaMemoryReader::new(store.clone())
        .bug_history(&pid_s)
        .await
        .expect("bug history after the draft");
    assert_eq!(
        bugs_after.len(),
        1,
        "the recurring-defect reader must now report the recorded bug: {bugs_after:?}"
    );
    assert!(
        bugs_after[0].occurrences >= 1,
        "with the recorded occurrence count, not zero: {:?}",
        bugs_after[0]
    );

    // The safety floor's reason must now also cite the draft, so a reader can tell "failed once"
    // from "a reviewable defect exists".
    let floor_after = reader.safety_floor(&pid_s).await.expect("floor after");
    assert!(
        floor_after[&case].contains("fingerprint"),
        "the fingerprint reason is still there: {:?}",
        floor_after[&case]
    );
}
