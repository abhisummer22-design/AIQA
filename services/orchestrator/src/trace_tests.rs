//! Phase 1 (Trace Graph Completion) — tests for the trace walk and the graph it reads.
//!
//! ## What these tests are for
//!
//! The Phase 0 audit found `quality_relations` holding **0 rows** in the live database
//! while the code contained three relation writers. Every test here exists to make one
//! of three things impossible to reintroduce:
//!
//! 1. **A relation that is written but unreachable.** The original defect was a writer
//!    emitting `node_type = "test_case"` while every reader looked for `"test"`, so
//!    rows existed and were invisible at the same time. Several tests below therefore
//!    assert the edge is not merely *present* but *readable through the public reader*.
//! 2. **A name used as an identity.** `unambiguous_task_ref` and the
//!    `link_case_*` tests pin this down.
//! 3. **A missing link quietly filled in.** The trace walk reports absence; these tests
//!    assert that absence stays visible.
//!
//! ## Fixture honesty
//!
//! The Phase 0 audit's central warning applies here: a fixture is not history. These
//! tests build real rows through real writers in a real migrated SQLite database, and
//! the report distinguishes "this code path persists" from "this is what a production
//! database currently holds". No test asserts a production row count.
//!
//! ## The negative assertions are the point
//!
//! Tests named `..._does_not_...` are not padding. Fabricated traceability is the exact
//! failure mode this phase guards against, and a suite that only checks happy paths
//! cannot detect it.

use crate::adaptive_intelligence::{
    AdaptiveIntelligenceService, ExecutionTelemetry, FailureCategory,
};
use crate::domain::CreateTestCaseRequest;
use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskStatus, TestStep};
use crate::intelligence::Intelligence;
use crate::store::{Store, LEGACY_TEST_NODE_TYPE, TASK_NODE_TYPE, TEST_NODE_TYPE};
use crate::trace::{TraceIndex, TraceReader};
use uuid::Uuid;

// ------------------------------------------------------------------ harness

/// A migrated store in a temp dir. Shared with the Phase 2 chain tests so both suites
/// assert against the same schema construction.
pub async fn temp_store_for_phase2() -> (Store, tempfile::TempDir) {
    temp_store().await
}

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase1-trace.sqlite3"))
        .await
        .expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 1 trace fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

/// A minimal but *complete* request — `CreateTestCaseRequest` has no `Default`, so
/// every field is spelled out. That is deliberate: a new required field should break
/// this fixture rather than be silently defaulted.
fn case_req(title: &str, requirement_reference: Option<&str>) -> CreateTestCaseRequest {
    CreateTestCaseRequest {
        title: title.to_string(),
        description: "Controlled test data for the Phase 1 trace graph.".into(),
        category: "functional".into(),
        priority: "P1".into(),
        severity: "medium".into(),
        preconditions: Vec::new(),
        steps: vec![TestStep {
            step_number: 1,
            action: "observe".into(),
            expected_result: "observed".into(),
        }],
        expected_result: Some("observed".into()),
        assertions: Vec::new(),
        test_data: None,
        source_document: None,
        requirement_reference: requirement_reference.map(ToString::to_string),
        source_requirement: None,
        traceability: None,
        risk_factor: None,
        tags: Vec::new(),
        suite_id: None,
    }
}

/// Row counts straight from SQLite. Used wherever a claim is about *persistence*
/// rather than about what a reader chose to report.
async fn count_relations(store: &Store, project_id: &str, relation_type: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM quality_relations rel \
         JOIN quality_nodes f ON f.id = rel.from_node_id \
         WHERE rel.relation_type = ? AND f.project_id = ?",
    )
    .bind(relation_type)
    .bind(project_id)
    .fetch_one(&store.pool)
    .await
    .expect("count relations")
}

async fn node_ref(store: &Store, project_id: &str, node_type: &str, ext: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT external_ref FROM quality_nodes WHERE project_id = ? AND node_type = ? AND external_ref = ?",
    )
    .bind(project_id)
    .bind(node_type)
    .bind(ext)
    .fetch_optional(&store.pool)
    .await
    .expect("query node")
}

// ------------------------------------------------------------------ unit: identity guard

/// The guard that stops a task name from claiming a real case's graph vertex.
#[test]
fn an_ordinary_task_name_is_its_own_key() {
    assert_eq!(
        crate::store::unambiguous_task_ref("Crawl: http://127.0.0.1:3000"),
        "Crawl: http://127.0.0.1:3000"
    );
    assert_eq!(
        crate::store::unambiguous_task_ref("Checkout Flow"),
        "Checkout Flow"
    );
}

/// A name that parses as a UUID is namespaced away. Without this, a client could
/// create a task literally named after a case id and silently attach its risk
/// assessment to someone else's test case.
#[test]
fn a_task_named_after_a_uuid_cannot_claim_that_case_vertex() {
    let case_id = Uuid::new_v4();
    let as_name = case_id.to_string();
    let key = crate::store::unambiguous_task_ref(&as_name);
    assert_ne!(key, as_name, "the name must not survive as a case key");
    assert!(key.starts_with("task:"));
    // The two namespaces can therefore never collide.
    assert_ne!(key, crate::store::unambiguous_task_ref("Checkout Flow"));
}

/// The three node types must stay distinct, or the two namespaces merge again.
#[test]
fn the_three_test_related_node_types_are_distinct() {
    let all = [TEST_NODE_TYPE, LEGACY_TEST_NODE_TYPE, TASK_NODE_TYPE];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(all[i], all[j], "{} vs {}", all[i], all[j]);
        }
    }
}

// ------------------------------------------------------------------ persistence: validates

/// The `validates` edge must persist **and** be readable through the public reader.
/// Persistence alone is what the original defect looked like: rows present, nothing
/// able to find them.
#[tokio::test]
async fn requirement_edge_persists_to_the_canonical_node_and_is_readable() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Validates").await;
    let intel = Intelligence::new(&store);

    let case_id = store
        .insert_test_case(
            &pid,
            &case_req("Decline an invalid card", Some("REQ-7")),
            None,
        )
        .await
        .expect("insert case");

    intel
        .link_test_case_to_requirement(
            &pid,
            case_id,
            "Decline an invalid card",
            Some("REQ-7"),
            None,
        )
        .await
        .expect("link");

    // 1. the row really exists
    assert_eq!(
        count_relations(&store, &pid, "validates").await,
        1,
        "the validates edge must be persisted, not merely computed"
    );

    // 2. it lands on the canonical node type, keyed on the stable case id
    assert!(
        node_ref(&store, &pid, TEST_NODE_TYPE, &case_id.to_string())
            .await
            .is_some(),
        "the case vertex must be keyed on test_cases.id under the canonical node type"
    );
    assert!(
        node_ref(&store, &pid, LEGACY_TEST_NODE_TYPE, &case_id.to_string())
            .await
            .is_none(),
        "the legacy node type must not be written any more"
    );

    // 3. and a reader can actually reach it - the step the original defect broke
    let found = store
        .list_test_cases_for_requirement(&pid, "REQ-7")
        .await
        .expect("requirement impact");
    assert_eq!(found.len(), 1, "the persisted edge must be readable");
    assert_eq!(found[0].id, case_id);
}

/// Edges written under the old node type must remain visible. Dropping the legacy
/// value would report "no tests validate this requirement" about rows that exist,
/// which is the same class of wrong answer this phase removes.
#[tokio::test]
async fn a_legacy_node_type_edge_is_still_readable() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Legacy").await;

    let case_id = store
        .insert_test_case(&pid, &case_req("Legacy case", Some("REQ-LEGACY")), None)
        .await
        .expect("insert case");

    // Write the edge exactly the way the pre-Phase-1 code wrote it.
    let req_node = store
        .upsert_quality_node(&pid, "requirement", "REQ-LEGACY", "REQ-LEGACY")
        .await
        .expect("req node");
    let case_node = store
        .upsert_quality_node(
            &pid,
            LEGACY_TEST_NODE_TYPE,
            &case_id.to_string(),
            "Legacy case",
        )
        .await
        .expect("case node");
    store
        .upsert_quality_relation(case_node, req_node, "validates")
        .await
        .expect("legacy relation");

    let found = store
        .list_test_cases_for_requirement(&pid, "REQ-LEGACY")
        .await
        .expect("requirement impact");
    assert_eq!(
        found.len(),
        1,
        "an edge persisted before Phase 1 must still resolve"
    );
    assert_eq!(found[0].id, case_id);
}

/// A case with no requirement writes nothing at all. This is pre-existing, correct
/// behaviour — the writer early-returns when there is no requirement identity — and
/// Phase 1 must not have "fixed" it into inventing a node.
#[tokio::test]
async fn a_case_with_no_requirement_writes_no_edge_at_all() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace No Req").await;
    let intel = Intelligence::new(&store);

    let case_id = store
        .insert_test_case(&pid, &case_req("Free-floating case", None), None)
        .await
        .expect("insert case");

    intel
        .link_test_case_to_requirement(&pid, case_id, "Free-floating case", None, None)
        .await
        .expect("link");

    assert_eq!(
        count_relations(&store, &pid, "validates").await,
        0,
        "no requirement means no requirement vertex, and no fabricated edge"
    );
}

// ------------------------------------------------------------------ persistence: covers

/// `Store::link_test_case_to_module` existed and had **no production caller**, so the
/// `covers` relation could never appear in the database. This pins the id-keyed
/// replacement and, crucially, that it lands on the *same* vertex as `validates` — the
/// property that was missing when each edge had its own idea of a case's identity.
///
/// It is a capability test, not a claim about production data. Nothing in the
/// production data model names a module, so this method still has no production caller
/// by design; see its doc comment. What this test protects is that when a caller does
/// arrive, it will be correct.
#[tokio::test]
async fn module_edge_persists_and_joins_the_same_vertex_as_the_requirement_edge() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Covers").await;
    let intel = Intelligence::new(&store);

    let case_id = store
        .insert_test_case(&pid, &case_req("Checkout", Some("REQ-9")), None)
        .await
        .expect("insert case");
    intel
        .link_test_case_to_requirement(&pid, case_id, "Checkout", Some("REQ-9"), None)
        .await
        .expect("validates");
    store
        .link_case_to_module(&pid, &case_id, "services/checkout", "Checkout service")
        .await
        .expect("covers");

    assert_eq!(
        count_relations(&store, &pid, "covers").await,
        1,
        "the covers edge must be persisted"
    );

    // Both edges must originate from the *same* vertex, or the trace is two
    // disconnected chains wearing one label.
    let from_nodes: Vec<String> = sqlx::query_scalar(
        "SELECT rel.from_node_id FROM quality_relations rel \
         WHERE rel.relation_type IN ('validates', 'covers')",
    )
    .fetch_all(&store.pool)
    .await
    .expect("from nodes");
    assert_eq!(from_nodes.len(), 2, "two edges expected");
    assert_eq!(
        from_nodes[0], from_nodes[1],
        "validates and covers must share the case vertex"
    );

    let trace = TraceReader::new(store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    let modules = trace.step("module").expect("module hop present");
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].label.as_deref(), Some("services/checkout"));
}

// ------------------------------------------------------------------ persistence: fails_with

fn telemetry(
    project_id: &str,
    case_id: Option<Uuid>,
    status: TaskStatus,
    category: Option<FailureCategory>,
    fingerprint: Option<&str>,
) -> ExecutionTelemetry {
    let now = chrono::Utc::now();
    ExecutionTelemetry {
        id: Uuid::new_v4(),
        project_id: project_id.to_string(),
        run_id: Uuid::new_v4(),
        task_id: format!("task-{}", Uuid::new_v4()),
        task_name: "Checkout flow".into(),
        test_case_id: case_id,
        execution_id: None,
        executor: "simulated".into(),
        environment: "staging".into(),
        start_time: now,
        end_time: Some(now),
        duration_ms: 12,
        status,
        retry_count: 0,
        healing_attempt_count: 0,
        failure_category: category,
        failure_fingerprint: fingerprint.map(ToString::to_string),
        error_message: Some("assertion failed".into()),
        evidence_refs: Vec::new(),
        assertion_result: None,
        http_status: None,
        selector: None,
        verification_outcome: None,
        created_at: now,
    }
}

/// The production failure writer must key its vertex on the stable case id, so the
/// `fails_with` edge joins the same vertex as `validates` and `covers`.
#[tokio::test]
async fn failure_edge_uses_the_stable_case_id_when_the_telemetry_row_carries_one() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Fails Case").await;
    let intel = Intelligence::new(&store);
    let svc = AdaptiveIntelligenceService::new(store.clone());

    let case_id = store
        .insert_test_case(&pid, &case_req("Checkout", Some("REQ-9")), None)
        .await
        .expect("insert case");
    intel
        .link_test_case_to_requirement(&pid, case_id, "Checkout", Some("REQ-9"), None)
        .await
        .expect("validates");

    svc.record_execution_telemetry(telemetry(
        &pid,
        Some(case_id),
        TaskStatus::Failed,
        Some(FailureCategory::AssertionFailure),
        Some("fp-abc-123"),
    ))
    .await
    .expect("record telemetry");

    assert_eq!(count_relations(&store, &pid, "fails_with").await, 1);
    assert!(
        node_ref(&store, &pid, TEST_NODE_TYPE, &case_id.to_string())
            .await
            .is_some(),
        "the failure must attach to the case vertex"
    );
    assert!(
        node_ref(&store, &pid, TASK_NODE_TYPE, "Checkout flow")
            .await
            .is_none(),
        "with a case id present, no name-keyed task vertex may be written"
    );
}

/// Without a case id there is no identity to use, so the vertex must be namespaced as
/// a task rather than smuggled into the case namespace under a name.
#[tokio::test]
async fn failure_edge_without_a_case_id_uses_the_task_namespace() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Fails Task").await;
    let svc = AdaptiveIntelligenceService::new(store.clone());

    svc.record_execution_telemetry(telemetry(
        &pid,
        None,
        TaskStatus::Failed,
        Some(FailureCategory::AssertionFailure),
        Some("fp-def-456"),
    ))
    .await
    .expect("record telemetry");

    assert_eq!(count_relations(&store, &pid, "fails_with").await, 1);
    assert!(
        node_ref(&store, &pid, TASK_NODE_TYPE, "Checkout flow")
            .await
            .is_some(),
        "an unlinked task must be recorded under the task namespace"
    );
}

/// Regression guard for the panic found in Phase 1.
///
/// The failure-graph writer built its cluster title with `&fingerprint[..12]`, a byte
/// slice on a `&String`. Any fingerprint under 12 bytes panicked the entire telemetry
/// ingest — losing the execution record and every edge it would have produced. It had
/// never fired only because every fingerprint the code produces is a 64-char SHA-256
/// hex digest, so the short case looked unreachable.
///
/// Both shapes are covered: shorter than 12 bytes, and multi-byte characters, since a
/// byte range can split a `char` even when the length is comfortable.
#[tokio::test]
async fn a_short_or_multibyte_fingerprint_does_not_panic_the_ingest() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Short FP").await;
    let svc = AdaptiveIntelligenceService::new(store.clone());

    for fingerprint in ["fp", "fp-abc-123", "a-fingerprint-longer-than-twelve"] {
        svc.record_execution_telemetry(telemetry(
            &pid,
            None,
            TaskStatus::Failed,
            Some(FailureCategory::AssertionFailure),
            Some(fingerprint),
        ))
        .await
        .unwrap_or_else(|e| panic!("ingest panicked or failed on {fingerprint:?}: {e}"));
    }

    // A multi-byte fingerprint: 12 characters, but more than 12 bytes.
    svc.record_execution_telemetry(telemetry(
        &pid,
        None,
        TaskStatus::Failed,
        Some(FailureCategory::AssertionFailure),
        Some("指纹指纹指纹指纹指纹指纹"),
    ))
    .await
    .expect("multibyte fingerprint must not split a char boundary");

    // All four must have produced graph edges - the ingest did not merely survive.
    assert_eq!(
        count_relations(&store, &pid, "fails_with").await,
        4,
        "every recorded failure must still produce its edge"
    );
}

/// A passing execution has no failure to link. No fingerprint means no `fails_with`
/// edge — not an edge with a null target.
#[tokio::test]
async fn a_passing_execution_writes_no_failure_edge() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Pass").await;
    let svc = AdaptiveIntelligenceService::new(store.clone());
    let case_id = Uuid::new_v4();

    svc.record_execution_telemetry(telemetry(
        &pid,
        Some(case_id),
        TaskStatus::Passed,
        None,
        None,
    ))
    .await
    .expect("record telemetry");

    assert_eq!(
        count_relations(&store, &pid, "fails_with").await,
        0,
        "a pass has no failure, and none may be invented"
    );
}

// ------------------------------------------------------------------ idempotence

/// Writing the same edge twice must be a no-op. `upsert_quality_relation` is a
/// read-then-insert against a `(from, to, type)` unique key; if that regressed, a
/// retried ingest would inflate the graph.
#[tokio::test]
async fn writing_the_same_edge_twice_is_idempotent() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Idempotent").await;
    let intel = Intelligence::new(&store);

    let case_id = store
        .insert_test_case(&pid, &case_req("Repeat me", Some("REQ-IDEM")), None)
        .await
        .expect("insert case");

    for _ in 0..3 {
        intel
            .link_test_case_to_requirement(&pid, case_id, "Repeat me", Some("REQ-IDEM"), None)
            .await
            .expect("link");
        store
            .link_case_to_module(&pid, &case_id, "services/repeat", "Repeat service")
            .await
            .expect("covers");
    }

    assert_eq!(count_relations(&store, &pid, "validates").await, 1);
    assert_eq!(count_relations(&store, &pid, "covers").await, 1);

    // And the reader agrees: one case, not three.
    let found = store
        .list_test_cases_for_requirement(&pid, "REQ-IDEM")
        .await
        .expect("requirement impact");
    assert_eq!(found.len(), 1);
}

// ------------------------------------------------------------------ isolation

/// One tenant's graph must be invisible to another, at the reader level and not just
/// by convention.
#[tokio::test]
async fn a_project_cannot_trace_another_projects_case() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Tenant A").await;
    let theirs = project(&store, "Tenant B").await;
    let intel = Intelligence::new(&store);

    let their_case = store
        .insert_test_case(
            &theirs,
            &case_req("Their checkout", Some("REQ-SHARED")),
            None,
        )
        .await
        .expect("insert their case");
    let my_case = store
        .insert_test_case(&mine, &case_req("My checkout", Some("REQ-SHARED")), None)
        .await
        .expect("insert my case");

    // Same requirement key in both projects - the collision case, not the easy one.
    intel
        .link_test_case_to_requirement(
            &theirs,
            their_case,
            "Their checkout",
            Some("REQ-SHARED"),
            None,
        )
        .await
        .expect("their link");
    intel
        .link_test_case_to_requirement(&mine, my_case, "My checkout", Some("REQ-SHARED"), None)
        .await
        .expect("my link");

    let found = store
        .list_test_cases_for_requirement(&mine, "REQ-SHARED")
        .await
        .expect("my impact");
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].id, my_case,
        "the other tenant's case must not appear"
    );

    // The trace index must not leak either.
    let index = TraceReader::new(store.clone()).index(&mine).await;
    let ids: Vec<&str> = index
        .test_cases
        .iter()
        .filter_map(|e| e.test_case_id.as_deref())
        .collect();
    assert!(ids.contains(&my_case.to_string().as_str()));
    assert!(
        !ids.contains(&their_case.to_string().as_str()),
        "the index leaked another tenant's case: {ids:?}"
    );

    // Walking their case id inside my project must return nothing attributed to me.
    let crossed = TraceReader::new(store.clone())
        .walk_test_case(&mine, &their_case)
        .await;
    assert!(
        crossed.step("requirement").is_none(),
        "cross-tenant walk must not resolve another tenant's requirement edge"
    );
}

// ------------------------------------------------------------------ the walk

/// End-to-end: a case created against a requirement, executed once successfully and
/// once with a failure that produced a real cluster, then walked. The returned chain
/// must be the one that is actually in the database.
#[tokio::test]
async fn the_walk_returns_the_chain_that_is_actually_persisted() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace E2E").await;
    let intel = Intelligence::new(&store);
    let svc = AdaptiveIntelligenceService::new(store.clone());
    let reader = TraceReader::new(store.clone());

    let case_id = store
        .insert_test_case(
            &pid,
            &case_req("Checkout with a saved card", Some("REQ-E2E-1")),
            None,
        )
        .await
        .expect("insert case");
    intel
        .link_test_case_to_requirement(
            &pid,
            case_id,
            "Checkout with a saved card",
            Some("REQ-E2E-1"),
            None,
        )
        .await
        .expect("validates");
    store
        .link_case_to_module(&pid, &case_id, "services/checkout", "Checkout service")
        .await
        .expect("covers");

    // A real cluster row, so the failure hop can resolve to an id rather than a
    // bare fingerprint. The telemetry fingerprint and the cluster signature are
    // deliberately different strings: migration 0030's `failure_fingerprint` is the
    // join, and a test that used one value for both would not catch that.
    let fingerprint = "fp-e2e-0001";
    store
        .upsert_failure_cluster(
            &pid,
            "signature-from-normaliser-v3",
            "the saved-card total no longer matches the charge",
            0.8,
            Some(fingerprint),
        )
        .await
        .expect("upsert cluster");

    svc.record_execution_telemetry(telemetry(
        &pid,
        Some(case_id),
        TaskStatus::Failed,
        Some(FailureCategory::AssertionFailure),
        Some(fingerprint),
    ))
    .await
    .expect("record failure");
    svc.record_execution_telemetry(telemetry(
        &pid,
        Some(case_id),
        TaskStatus::Passed,
        None,
        None,
    ))
    .await
    .expect("record pass");

    let trace = reader.walk_test_case(&pid, &case_id).await;

    // test - always the entry point
    let tests = trace.step("test").expect("test hop");
    assert_eq!(tests[0].id.as_deref(), Some(case_id.to_string().as_str()));

    // requirement - joined through the persisted validates edge
    let reqs = trace.step("requirement").expect("requirement hop");
    assert!(
        reqs.iter().any(|s| s.label.as_deref() == Some("REQ-E2E-1")),
        "the requirement must be reached through the persisted edge: {reqs:?}"
    );

    // module - through covers
    assert_eq!(trace.step("module").expect("module hop").len(), 1);

    // execution - both attempts, on the stable case id
    let execs = trace.step("execution").expect("execution hop");
    assert_eq!(execs.len(), 2, "both executions of this case must appear");

    // failure - resolved to the real cluster row, by fingerprint
    let failures = trace.step("failure").expect("failure hop");
    assert_eq!(failures.len(), 1);
    let resolved_id = failures[0].id.clone().expect("cluster id resolved");
    let cluster_row: Option<String> = sqlx::query_scalar(
        "SELECT id FROM failure_clusters WHERE project_id = ? AND failure_fingerprint = ?",
    )
    .bind(&pid)
    .bind(fingerprint)
    .fetch_optional(&store.pool)
    .await
    .expect("cluster lookup");
    assert_eq!(
        cluster_row.as_deref(),
        Some(resolved_id.as_str()),
        "the failure hop must name the real failure_clusters row, not the bare fingerprint"
    );
    assert_eq!(
        failures[0].label.as_deref(),
        Some("signature-from-normaliser-v3"),
        "the hop must carry the cluster signature"
    );

    // release / bug / fix / retest are absent here, and must be reported absent.
    //
    // Phase 2 widened these labels to carry *why* the hop is missing
    // ("fix (no healing proposal recorded against this failure)"), which is strictly more
    // useful than a bare "fix". The assertion is therefore on the prefix: it still proves
    // the hop is named, without pinning the wording. Pinning the exact string would have
    // failed here for a documentation improvement, which is the wrong reason for a test
    // to fail.
    for absent in ["bug", "fix", "retest", "release"] {
        assert!(
            trace.step(absent).is_none(),
            "`{absent}` has no evidence and must not appear"
        );
        assert!(
            trace
                .incomplete
                .iter()
                .any(|m| m == absent || m.starts_with(&format!("{absent} ("))),
            "`{absent}` must be named in `incomplete`, got {:?}",
            trace.incomplete
        );
    }
}

/// The index must describe the real graph, including which hops are absent.
#[tokio::test]
async fn the_index_lists_real_cases_and_their_gaps() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Index").await;
    let intel = Intelligence::new(&store);

    let traced = store
        .insert_test_case(&pid, &case_req("Fully traced", Some("REQ-I-1")), None)
        .await
        .expect("insert a");
    intel
        .link_test_case_to_requirement(&pid, traced, "Fully traced", Some("REQ-I-1"), None)
        .await
        .expect("link a");
    store
        .link_case_to_module(&pid, &traced, "services/a", "A")
        .await
        .expect("covers a");

    let bare = store
        .insert_test_case(&pid, &case_req("Bare", None), None)
        .await
        .expect("insert b");
    // A case with no graph vertex at all must not be invented into the index.
    assert!(
        store.get_test_case(bare).await.expect("case").is_some(),
        "the bare case exists as a row"
    );

    let index: TraceIndex = TraceReader::new(store.clone()).index(&pid).await;
    assert_eq!(index.project_id, pid);
    let entry = index
        .test_cases
        .iter()
        .find(|e| e.test_case_id.as_deref() == Some(traced.to_string().as_str()))
        .expect("traced case in index");
    assert!(
        entry.resolved,
        "the vertex resolves to a real test_cases row"
    );
    assert!(entry.has.contains(&"requirement".to_string()));
    assert!(entry.has.contains(&"module".to_string()));
    assert!(
        entry.missing.contains(&"failure".to_string()),
        "no execution means no failure edge, and the gap must be stated"
    );

    assert!(
        !index
            .test_cases
            .iter()
            .any(|e| e.test_case_id.as_deref() == Some(bare.to_string().as_str())),
        "a case with no graph vertex must not be fabricated into the index"
    );
}

/// An empty project reports an empty index, not a placeholder.
#[tokio::test]
async fn an_empty_project_reports_an_empty_index() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Empty").await;
    let index = TraceReader::new(store).index(&pid).await;
    assert!(
        index.test_cases.is_empty(),
        "an empty project has nothing to trace; it must not be summarised anyway"
    );
}

/// A case with no history must report every hop as missing rather than a short chain
/// that reads like a complete one.
#[tokio::test]
async fn a_case_with_no_history_names_every_missing_hop() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace No History").await;
    let case_id = store
        .insert_test_case(&pid, &case_req("Never executed", None), None)
        .await
        .expect("insert case");

    let trace = TraceReader::new(store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    assert_eq!(
        trace.test_case_id.as_deref(),
        Some(case_id.to_string().as_str())
    );
    assert!(
        !trace.is_complete(),
        "a never-executed case is not a complete trace"
    );
    for hop in [
        "requirement",
        "module",
        "execution",
        "failure",
        "bug",
        "fix",
        "retest",
        "release",
    ] {
        assert!(
            trace.step(hop).is_none(),
            "`{hop}` has no evidence and must be absent"
        );
    }
    assert_eq!(trace.incomplete.len(), 8, "every hop must be accounted for");
}

/// A fingerprint with no cluster row behind it is still reported as a failure hop,
/// but says so. The Phase 6 chain depends on a fingerprint and a cluster row existing
/// together; when only the first does, the trace must not imply the second.
#[tokio::test]
async fn a_fingerprint_with_no_cluster_row_is_reported_as_incomplete() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Orphan FP").await;
    let svc = AdaptiveIntelligenceService::new(store.clone());
    let case_id = Uuid::new_v4();

    svc.record_execution_telemetry(telemetry(
        &pid,
        Some(case_id),
        TaskStatus::Failed,
        Some(FailureCategory::AssertionFailure),
        Some("fp-with-no-cluster"),
    ))
    .await
    .expect("record");

    let trace = TraceReader::new(store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    let failures = trace.step("failure").expect("failure hop reported");
    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].id.is_none(),
        "there is no cluster row, so there is no id to report"
    );
    assert!(
        !failures[0].evidence.missing.is_empty(),
        "the absent cluster row must be stated, not glossed over"
    );
}

// ------------------------------------------------------------------ run/task linkage

/// The task<->case link migration 0028 added must resolve, project-scoped.
#[tokio::test]
async fn a_task_resolves_its_case_id_and_respects_the_project() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Task Link").await;
    let case_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let task_id = "task-link-1";

    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "trace task link run".into(),
                tasks: vec![TaskDef {
                    id: task_id.into(),
                    name: "Linked task".into(),
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
            Some(&pid),
            Some("trace fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");

    assert_eq!(
        store
            .get_task_test_case_id(&pid, &run_id, task_id)
            .await
            .expect("resolve"),
        Some(case_id),
        "the task must resolve to the case its plan named"
    );

    // Same task id, wrong tenant: the join to `runs` is what prevents this.
    let other = project(&store, "Trace Task Link Other").await;
    assert_eq!(
        store
            .get_task_test_case_id(&other, &run_id, task_id)
            .await
            .expect("resolve cross-tenant"),
        None,
        "another tenant must not resolve this run's task"
    );
}

/// A task with no case resolves to `None`, and stays `None` — not a guess.
#[tokio::test]
async fn a_task_with_no_case_resolves_to_none() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Trace Task No Case").await;
    let run_id = Uuid::new_v4();

    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "trace task no case".into(),
                tasks: vec![TaskDef {
                    id: "task-no-case".into(),
                    name: "Crawl".into(),
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
            },
            Some(&pid),
            Some("trace fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");

    assert_eq!(
        store
            .get_task_test_case_id(&pid, &run_id, "task-no-case")
            .await
            .expect("resolve"),
        None,
        "a crawl task has no case, and none may be inferred"
    );
}

// ------------------------------------------------------------------ migration safety

/// Phase 1 adds no migration. This asserts that as a fact rather than as a claim, so
/// a later phase cannot quietly add a destructive one under this heading.
///
/// The check is on the *live* migration count the store applies, not on a hardcoded
/// number, so adding a genuinely additive migration later will fail this test and have
/// to be justified — which is the point of pinning it.
#[tokio::test]
async fn phase_one_introduces_no_new_migration() {
    let (store, _d) = temp_store().await;
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&store.pool)
        .await
        .expect("migration count");

    // The Phase 1 report documents this number. If it changes, the report must too.
    assert_eq!(
        applied,
        // 32 since Phase 5: migration 0032 (`exploration_sessions` +
        // `exploration_findings`, both new and additive -- nothing altered or dropped).
        //
        // 31 since Phase 3: migration 0031 (`change_impacts` project scope +
        // unique key) is additive - ADD COLUMN and two indexes, nothing dropped or
        // rewritten. Phase 1 and Phase 2 each pinned 30 and each declared no migration;
        // this count moved exactly once, for one additive migration, and both guards
        // noticed rather than being quietly relaxed.
        32,
        "Phase 1 was specified as additive with no schema change; \
         if the migration count moved, Phase 1 did something it did not claim"
    );

    // And the columns the walk depends on must already exist. The table is named per
    // pair: getting this wrong is exactly the kind of wrong test premise that
    // "passes" by asserting nothing, so each pair is asserted against the table that
    // actually carries it.
    for (table, column) in [
        ("execution_telemetry", "test_case_id"),
        ("execution_telemetry", "failure_fingerprint"),
        ("tasks", "test_case_id"),
        ("failure_clusters", "failure_fingerprint"),
        // The release hop joins telemetry's run ids through `runs.release_id`, so the
        // column lives on `runs`. It was originally asserted on
        // `execution_telemetry`, which does not have it - a wrong premise that failed
        // rather than silently passing, which is the correct outcome.
        ("runs", "release_id"),
        ("test_cases", "executed_run_id"),
    ] {
        let present: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = ?")
                .bind(table)
                .bind(column)
                .fetch_one(&store.pool)
                .await
                .expect("column check");
        assert!(
            present > 0,
            "{table}.{column} is required by the walk and must already exist"
        );
    }
}
