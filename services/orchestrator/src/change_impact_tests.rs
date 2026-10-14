//! Phase 3 (Change Impact Intelligence + Smart Regression) — end-to-end tests.
//!
//! ## The rule
//!
//! Every row in these tests is produced by a real production writer. `insert_test_case`
//! writes the case, `Intelligence::link_test_case_to_requirement` writes the `validates`
//! edge, and `Intelligence::ingest_run` writes the telemetry, the cluster and the bug
//! draft. Nothing here hand-writes a `quality_relations` row or a fingerprint, because a
//! fixture-shaped graph would let an impact resolver pass on links production cannot make.
//!
//! ## What each test is guarding
//!
//! | Concern | Tests |
//! |---|
//! | direct impact | a changed requirement reaches its cases; a changed case reaches itself |
//! | indirect impact | a case sharing a recorded failure signature is reached |
//! | unrelated change | a change to nothing yields no impact and says so |
//! | missing links | every unresolvable kind returns `INSUFFICIENT_DATA` with a reason |
//! | no heuristic links | a near-miss requirement key does **not** match |
//! | multiple projects | tenant A's change never reaches tenant B |
//! | empty history | a project with no cases yields an empty selection, not a guess |
//! | duplicate records | re-analysing one `change_ref` is idempotent |
//! | mandatory retention | the safety floor cannot be subtracted by a change |
//! | existing engine | `Scope::restricted_to` reuses the regression scope |

use crate::change_impact::{
    ChangeImpactReader, ChangeTarget, Resolution, Tier, HIGH_RISK_THRESHOLD,
};
use crate::domain::CreateTestCaseRequest;
use crate::domain::{
    RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus, TestStep,
};
use crate::intelligence::Intelligence;
use crate::regression::SkippedCase;
use crate::regression_runner::Scope;
use crate::store::Store;
use std::collections::BTreeSet;
use uuid::Uuid;

// ------------------------------------------------------------------ harness

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase3-impact.sqlite3"))
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

fn case_req(title: &str, requirement_reference: Option<&str>) -> CreateTestCaseRequest {
    CreateTestCaseRequest {
        title: title.to_string(),
        description: "Controlled data for Phase 3.".into(),
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
        requirement_reference: requirement_reference.map(ToString::to_string),
        source_requirement: None,
        traceability: None,
        risk_factor: None,
        tags: Vec::new(),
        suite_id: None,
    }
}

/// A persisted case linked to a requirement, through the real writers.
async fn case_on_requirement(
    store: &Store,
    project_id: &str,
    title: &str,
    requirement: &str,
) -> Uuid {
    let intel = Intelligence::new(store);
    let id = store
        .insert_test_case(project_id, &case_req(title, Some(requirement)), None)
        .await
        .expect("insert case");
    intel
        .link_test_case_to_requirement(project_id, id, title, Some(requirement), None)
        .await
        .expect("link requirement");
    id
}

/// Drive the real ingest path so a failing execution, its cluster and (on the second
/// occurrence) its bug draft all exist.
async fn ingest_failure(
    store: &Store,
    project_id: &str,
    case_id: Uuid,
    reason: &str,
    selector: &str,
    attempt: u32,
) -> Uuid {
    let run_id = Uuid::new_v4();
    let task_id = format!("task-{run_id}");
    let plan = RunPlan {
        name: format!("phase 3 run {attempt}"),
        tasks: vec![TaskDef {
            id: task_id.clone(),
            name: format!("Test Case: {case_id}"),
            prerequisites: Vec::new(),
            simulate: Some(SimSpec {
                outcome: SimOutcome::Fail,
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
            Some("phase 3 fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");
    let states = vec![TaskState {
        id: task_id,
        name: format!("Test Case: {case_id}"),
        prerequisites: Vec::new(),
        status: TaskStatus::Failed,
        reason: Some(reason.to_string()),
        started_at: None,
        finished_at: None,
        test_case_id: Some(case_id.to_string()),
    }];
    Intelligence::new(store)
        .ingest_run(Some(run_id), Some(project_id), &plan, &states)
        .await
        .expect("ingest_run");
    run_id
}

fn target(kind: &str, ref_id: &str) -> ChangeTarget {
    ChangeTarget {
        kind: kind.to_string(),
        ref_id: ref_id.to_string(),
    }
}

// ------------------------------------------------------------------ direct impact

/// A changed requirement reaches every case linked to it by a stored `validates` edge.
#[tokio::test]
async fn a_changed_requirement_reaches_its_linked_cases() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Direct").await;
    let a = case_on_requirement(&store, &pid, "Checkout with a saved card", "REQ-P3-1").await;
    let b = case_on_requirement(&store, &pid, "Decline an invalid card", "REQ-P3-1").await;
    case_on_requirement(&store, &pid, "Unrelated billing case", "REQ-P3-2").await;

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-abc123",
            "requirement",
            &[target("requirement", "REQ-P3-1")],
        )
        .await
        .expect("analyse");

    assert!(
        analysis.insufficient_reason.is_none(),
        "REQ-P3-1 resolves: {:?}",
        analysis.insufficient_reason
    );
    let ids: Vec<Uuid> = analysis
        .impacted_tests
        .iter()
        .map(|i| i.test_case_id)
        .collect();
    assert!(
        ids.contains(&a) && ids.contains(&b),
        "both linked cases: {ids:?}"
    );

    let unrelated = analysis
        .impacted_tests
        .iter()
        .position(|i| i.title == "Unrelated billing case");
    assert!(
        unrelated.is_none(),
        "a case on a different requirement must not be reached"
    );

    for i in analysis
        .impacted_tests
        .iter()
        .filter(|i| i.test_case_id == a)
    {
        assert_eq!(i.tier, Tier::DirectlyImpacted);
        assert!(
            i.via.contains("validates"),
            "the claim must name the stored edge it used: {}",
            i.via
        );
    }
    store.pool.close().await;
}

/// A changed test case reaches itself, and only itself as *direct* impact.
#[tokio::test]
async fn a_changed_test_case_reaches_itself() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Direct Case").await;
    let a = case_on_requirement(&store, &pid, "Login succeeds", "REQ-P3-3").await;
    case_on_requirement(&store, &pid, "Login rejects a bad password", "REQ-P3-4").await;

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-def456",
            "code",
            &[target("test_case", &a.to_string())],
        )
        .await
        .expect("analyse");

    let direct: Vec<Uuid> = analysis
        .impacted_tests
        .iter()
        .filter(|i| i.tier == Tier::DirectlyImpacted)
        .map(|i| i.test_case_id)
        .collect();
    assert_eq!(
        direct,
        vec![a],
        "only the changed case is directly impacted: {direct:?}"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ indirect impact

/// Indirect impact is one hop of *recorded* dependency: two cases that failed with the
/// same signature. Both the clusters and both edges are real rows.
#[tokio::test]
async fn a_case_sharing_a_recorded_failure_is_reached_indirectly() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Indirect").await;

    let target_case = case_on_requirement(&store, &pid, "The changed case", "REQ-P3-5").await;
    let sibling = case_on_requirement(&store, &pid, "The sibling case", "REQ-P3-6").await;

    // Same reason, same selector, so both produce the *same* canonical fingerprint.
    let reason = "Element not found: #login-btn";
    ingest_failure(&store, &pid, target_case, reason, "#login-btn", 1).await;
    ingest_failure(&store, &pid, target_case, reason, "#login-btn", 2).await;
    ingest_failure(&store, &pid, sibling, reason, "#login-btn", 1).await;
    ingest_failure(&store, &pid, sibling, reason, "#login-btn", 2).await;

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-ghi789",
            "code",
            &[target("test_case", &target_case.to_string())],
        )
        .await
        .expect("analyse");

    let sibling_hop = analysis
        .impacted_tests
        .iter()
        .find(|i| i.test_case_id == sibling)
        .expect("the sibling case shares a recorded failure and must be reached");
    assert_eq!(
        sibling_hop.tier,
        Tier::RelatedFailure,
        "the sibling enters by recorded dependency, not as a direct hit"
    );
    assert!(
        sibling_hop.via.contains("fails_with"),
        "the claim must name the stored edge: {}",
        sibling_hop.via
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ missing links

/// Kinds the schema cannot ground must be refused with a reason, never guessed at.
///
/// This is the test that would fail if someone "fixed" an empty impact report by matching
/// a target URL or a title substring.
#[tokio::test]
async fn every_unresolvable_change_kind_is_refused_with_a_reason() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Kinds").await;
    case_on_requirement(&store, &pid, "Anything", "REQ-P3-7").await;
    let reader = ChangeImpactReader::new(store.clone());

    for (kind, ref_id) in [
        ("code_file", "src/checkout.rs"),
        ("api_contract", "POST /payment"),
        ("page", "/billing"),
        ("configuration", "timeout_ms=30"),
        ("dependency", "playwright@1.4"),
        ("module", "services/checkout"),
        ("nonsense", "whatever"),
    ] {
        let outcome = reader
            .analyse(
                &pid,
                &format!("commit-{kind}"),
                "code",
                &[target(kind, ref_id)],
            )
            .await
            .expect("analyse");
        let t = &outcome.targets[0];
        match &t.resolution {
            Resolution::Insufficient { marker, reason } => {
                assert_eq!(
                    *marker, "INSUFFICIENT_DATA",
                    "{kind} must report insufficient data"
                );
                assert!(
                    reason.len() > 20,
                    "{kind} must state *why* it cannot be resolved, got {reason:?}"
                );
            }
            Resolution::Resolved { .. } => {
                panic!("{kind} must not resolve; the schema has no vertex for it")
            }
        }
        assert!(
            outcome.impacted_tests.is_empty(),
            "{kind} must not manufacture impact"
        );
    }
    store.pool.close().await;
}

/// A requirement key that merely *resembles* a real one must not match.
///
/// The single most likely way a change-impact feature starts lying.
#[tokio::test]
async fn a_near_miss_requirement_key_does_not_match() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 NearMiss").await;
    let a = case_on_requirement(&store, &pid, "Real case", "REQ-P3-8").await;

    let reader = ChangeImpactReader::new(store.clone());
    for near_miss in ["REQ-P3-8 ", "req-p3-8", "REQ-P3-80", "REQ-P3-", "XREQ-P3-8"] {
        let outcome = reader
            .analyse(
                &pid,
                "commit-near",
                "requirement",
                &[target("requirement", near_miss)],
            )
            .await
            .expect("analyse");
        // A trailing space is trimmed by design; everything else is a genuine near miss.
        if near_miss.trim() == near_miss {
            assert!(
                outcome.impacted_tests.iter().all(|i| i.test_case_id != a),
                "`{near_miss}` must not match REQ-P3-8"
            );
            assert!(
                matches!(
                    outcome.targets[0].resolution,
                    Resolution::Insufficient { .. }
                ),
                "`{near_miss}` must be reported unresolved, got {:?}",
                outcome.targets[0].resolution
            );
        }
    }
    store.pool.close().await;
}

/// A malformed test-case id is a clear refusal, not a silent no-match.
#[tokio::test]
async fn a_malformed_test_case_target_is_refused() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 BadId").await;
    let reader = ChangeImpactReader::new(store.clone());
    let outcome = reader
        .analyse(
            &pid,
            "commit-bad",
            "code",
            &[target("test_case", "not-a-uuid")],
        )
        .await
        .expect("analyse");
    match &outcome.targets[0].resolution {
        Resolution::Insufficient { reason, .. } => assert!(
            reason.contains("UUID"),
            "the reason should say what a test_case ref must be: {reason}"
        ),
        other => panic!("expected insufficient, got {other:?}"),
    }
    store.pool.close().await;
}

// ------------------------------------------------------------------ unrelated / empty

/// A change that touches nothing resolves to nothing, and says so.
#[tokio::test]
async fn an_unrelated_change_yields_no_impact() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Unrelated").await;
    case_on_requirement(&store, &pid, "Some case", "REQ-P3-9").await;

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-unrelated",
            "requirement",
            &[target("requirement", "REQ-NOT-IN-THIS-PROJECT")],
        )
        .await
        .expect("analyse");
    assert!(analysis.impacted_tests.is_empty());
    assert!(
        analysis.insufficient_reason.is_some(),
        "nothing resolved, so the answer must be reported as insufficient"
    );
    store.pool.close().await;
}

/// A project with no cases and no history yields an empty selection.
#[tokio::test]
async fn an_empty_project_yields_an_empty_selection() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Empty").await;
    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-empty",
            "code",
            &[target("code_file", "src/x.rs")],
        )
        .await
        .expect("analyse");
    assert!(analysis.impacted_tests.is_empty());
    assert!(analysis.not_selected.is_empty());
    assert!(analysis.safety_floor.is_empty());
    store.pool.close().await;
}

// ------------------------------------------------------------------ safety floor

/// A case with recorded failure evidence is retained even when nothing links it to the
/// change.
///
/// This is the guarantee behind "never remove mandatory safety tests only to reduce test
/// count". Because no mandatory flag exists in the schema, the floor is *derived* — and a
/// derived floor must be retained by construction, not by hope.
#[tokio::test]
async fn a_failing_case_is_retained_even_when_the_change_does_not_touch_it() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Floor").await;

    let changed = case_on_requirement(&store, &pid, "The changed case", "REQ-P3-A").await;
    let fragile = case_on_requirement(&store, &pid, "The fragile unrelated case", "REQ-P3-B").await;
    case_on_requirement(&store, &pid, "The quiet unrelated case", "REQ-P3-C").await;

    // Only the fragile case has ever failed.
    ingest_failure(&store, &pid, fragile, "Element not found: #pay", "#pay", 1).await;
    ingest_failure(&store, &pid, fragile, "Element not found: #pay", "#pay", 2).await;

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-floor",
            "requirement",
            &[target("requirement", "REQ-P3-A")],
        )
        .await
        .expect("analyse");

    let selected: Vec<Uuid> = analysis
        .impacted_tests
        .iter()
        .map(|i| i.test_case_id)
        .collect();
    assert!(selected.contains(&changed), "the changed case is selected");
    assert!(
        selected.contains(&fragile),
        "a case with recorded failure evidence must survive an unrelated change: {selected:?}"
    );
    assert!(
        analysis.safety_floor.contains(&fragile),
        "the floor must be reported so a reader can audit it"
    );

    let floor_hop = analysis
        .impacted_tests
        .iter()
        .find(|i| i.test_case_id == fragile)
        .expect("floor case");
    assert_eq!(floor_hop.tier, Tier::SafetyFloor);
    assert!(
        floor_hop.via.contains("fingerprint"),
        "the floor claim must name the recorded evidence: {}",
        floor_hop.via
    );

    // The quiet case has no evidence at all, so it is legitimately dropped — and the drop
    // is recorded with a reason rather than vanishing.
    assert!(
        analysis
            .not_selected
            .iter()
            .any(|n| n.reason.contains("no stored relation")),
        "a dropped case must state why: {:?}",
        analysis.not_selected
    );
    assert_eq!(
        analysis.not_selected.len(),
        1,
        "exactly the one evidence-free case is dropped: {:?}",
        analysis.not_selected
    );
    store.pool.close().await;
}

/// The risk tier retains a case on its own recorded risk.
#[tokio::test]
async fn a_high_risk_case_is_retained_on_its_own_risk() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Risk").await;
    case_on_requirement(&store, &pid, "Changed case", "REQ-P3-R1").await;
    let risky = case_on_requirement(&store, &pid, "Risky but unrelated", "REQ-P3-R2").await;
    let quiet = case_on_requirement(&store, &pid, "Quiet and unrelated", "REQ-P3-R3").await;

    // A recorded risk assessment on the risky case's own vertex. Written through the
    // existing store method, which is what `update_test_intelligence` calls.
    let node_id: String = sqlx::query_scalar(
        "SELECT id FROM quality_nodes WHERE project_id = ? AND external_ref = ? AND node_type = 'test'",
    )
    .bind(&pid)
    .bind(risky.to_string())
    .fetch_one(&store.pool)
    .await
    .expect("risky case vertex");
    let node_id = Uuid::parse_str(&node_id).expect("quality_nodes.id is a UUID string");
    store
        .upsert_risk_assessment(
            node_id,
            HIGH_RISK_THRESHOLD + 0.1,
            // `None`, not `0.0`: this fixture supplies no source for business criticality,
            // change frequency or security sensitivity, and the remediation's whole point is
            // that "no source" must be expressible. `score` is passed explicitly here, so the
            // Phase 3 high-risk tier is unaffected by the intelligence-layer normalisation.
            None,
            None,
            0.5,
            1,
            None,
            100.0,
            "{}",
        )
        .await
        .expect("upsert risk");

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-risk",
            "requirement",
            &[target("requirement", "REQ-P3-R1")],
        )
        .await
        .expect("analyse");

    let selected: Vec<Uuid> = analysis
        .impacted_tests
        .iter()
        .map(|i| i.test_case_id)
        .collect();
    assert!(
        selected.contains(&risky),
        "a high-risk case is retained: {selected:?}"
    );
    assert!(
        !selected.contains(&quiet),
        "an evidence-free case is still dropped"
    );
    let hop = analysis
        .impacted_tests
        .iter()
        .find(|i| i.test_case_id == risky)
        .expect("risk hop");
    assert_eq!(hop.tier, Tier::HighRisk);
    assert!(hop.via.contains("risk score"), "{}", hop.via);
    store.pool.close().await;
}

// ------------------------------------------------------------------ persistence

/// The records must actually be written, and re-analysing the same change must not
/// duplicate them.
#[tokio::test]
async fn impact_records_are_written_once_per_change() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Persist").await;
    let a = case_on_requirement(&store, &pid, "Case one", "REQ-P3-P").await;
    case_on_requirement(&store, &pid, "Case two", "REQ-P3-P").await;

    let reader = ChangeImpactReader::new(store.clone());
    let targets = [target("requirement", "REQ-P3-P")];

    let first = reader
        .analyse(&pid, "commit-persist", "requirement", &targets)
        .await
        .expect("analyse");
    assert!(first.recorded > 0, "the first analysis must write rows");
    assert_eq!(first.already_recorded, 0);
    assert_eq!(first.impacted_tests.len(), 2);

    let after_first = store
        .count_change_impacts(&pid, "commit-persist")
        .await
        .expect("count");
    assert_eq!(after_first, 2, "one row per selected case: {first:?}");

    // Re-analyse: idempotent, not additive.
    let second = reader
        .analyse(&pid, "commit-persist", "requirement", &targets)
        .await
        .expect("analyse");
    assert_eq!(second.recorded, 0, "a repeat must not write new rows");
    assert_eq!(second.already_recorded, 2);
    assert_eq!(second.impacted_tests.len(), 2, "the answer is unchanged");

    let after_second = store
        .count_change_impacts(&pid, "commit-persist")
        .await
        .expect("count");
    assert_eq!(after_second, 2, "still exactly two rows");

    // And the history reads back.
    let rows = store.list_change_impacts(&pid, 100).await.expect("list");
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.project_id, pid, "every row states its project");
        assert_eq!(row.change_ref, "commit-persist");
        assert_eq!(row.change_type, "requirement");
        assert!(
            row.rationale.is_some(),
            "a record must say why the test was selected"
        );
    }
    let selected: Vec<Uuid> = rows
        .iter()
        .filter_map(|r| r.selected_test_node_id)
        .collect();
    assert_eq!(selected.len(), 2);
    let _ = a;
    store.pool.close().await;
}

/// A resolved node with no linked test is recorded as a negative result, not skipped.
#[tokio::test]
async fn a_resolved_node_with_no_linked_test_is_recorded_as_a_negative_result() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P3 Negative").await;

    // Create the requirement vertex without any case validating it.
    store
        .upsert_quality_node(&pid, "requirement", "REQ-P3-N1", "REQ-P3-N1")
        .await
        .expect("requirement vertex");

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &pid,
            "commit-negative",
            "requirement",
            &[target("requirement", "REQ-P3-N1")],
        )
        .await
        .expect("analyse");

    assert!(
        analysis.impacted_tests.is_empty(),
        "nothing is linked, so nothing is impacted"
    );
    assert_eq!(
        analysis.recorded, 1,
        "the negative result itself is a record"
    );

    let rows = store.list_change_impacts(&pid, 10).await.expect("list");
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].selected_test_node_id.is_none(),
        "no test was selected"
    );
    assert!(
        rows[0]
            .rationale
            .as_deref()
            .is_some_and(|r| r.contains("no test case is linked")),
        "the record must say why: {:?}",
        rows[0].rationale
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ isolation

/// One tenant's change must not reach another tenant's cases, records, or floor.
#[tokio::test]
async fn change_impact_does_not_cross_projects() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "P3 Tenant A").await;
    let theirs = project(&store, "P3 Tenant B").await;

    let their_case = case_on_requirement(&store, &theirs, "Their case", "REQ-P3-X").await;
    let my_case = case_on_requirement(&store, &mine, "My case", "REQ-P3-X").await;

    // Same requirement key in both projects: the collision case, not the easy one.
    ingest_failure(
        &store,
        &theirs,
        their_case,
        "Element not found: #a",
        "#a",
        1,
    )
    .await;
    ingest_failure(
        &store,
        &theirs,
        their_case,
        "Element not found: #a",
        "#a",
        2,
    )
    .await;

    let reader = ChangeImpactReader::new(store.clone());
    let analysis = reader
        .analyse(
            &mine,
            "commit-iso",
            "requirement",
            &[target("requirement", "REQ-P3-X")],
        )
        .await
        .expect("analyse");

    let selected: Vec<Uuid> = analysis
        .impacted_tests
        .iter()
        .map(|i| i.test_case_id)
        .collect();
    assert!(selected.contains(&my_case), "our own case is selected");
    assert!(
        !selected.contains(&their_case),
        "another tenant's case must not be selected: {selected:?}"
    );
    assert!(
        !analysis.safety_floor.contains(&their_case),
        "another tenant's floor must not leak"
    );

    // Their records must not appear in our history.
    let rows = store.list_change_impacts(&mine, 100).await.expect("list");
    for row in &rows {
        assert_eq!(row.project_id, mine);
    }
    let their_rows = store
        .list_change_impacts(&theirs, 100)
        .await
        .expect("list theirs");
    assert!(their_rows.is_empty(), "we wrote nothing in their project");

    // And resolving their case from our project finds nothing.
    let crossed = reader
        .analyse(
            &mine,
            "commit-crossed",
            "code",
            &[target("test_case", &their_case.to_string())],
        )
        .await
        .expect("analyse");
    assert!(crossed.impacted_tests.is_empty());
    store.pool.close().await;
}

// ------------------------------------------------------------------ the existing engine

/// Smart regression narrows the **existing** scope; it does not replace it.
///
/// The properties asserted here are the ones a second engine would break: a dropped case
/// is accounted for, the mandatory set cannot be subtracted, and the engine's own case
/// order survives selection.
#[tokio::test]
async fn restricted_to_narrows_the_existing_scope_without_losing_accounting() {
    use crate::domain::TaskDef as RTDef;
    use crate::regression::ReplayedCase;
    use std::collections::BTreeMap;

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
    let case = |id: &str| ReplayedCase {
        case_id: id.to_string(),
        task_id: format!("task-{id}"),
        title: format!("case {id}"),
        source_run_id: Uuid::new_v4().to_string(),
    };

    let full = Scope {
        tasks: vec![
            (case("a"), task("task-a")),
            (case("b"), task("task-b")),
            (case("c"), task("task-c")),
        ],
        skipped: vec![SkippedCase {
            case_id: "d".into(),
            title: "case d".into(),
            reason: "another case was last run as the same task".into(),
        }],
    };

    let selected: BTreeSet<String> = ["a".to_string()].into_iter().collect();
    let mandatory: BTreeSet<String> = ["c".to_string()].into_iter().collect();
    let narrowed = full.restricted_to(&selected, &mandatory);

    let kept: Vec<&str> = narrowed
        .tasks
        .iter()
        .map(|(c, _)| c.case_id.as_str())
        .collect();
    assert_eq!(
        kept,
        vec!["a", "c"],
        "selected plus mandatory, in engine order"
    );

    // Everything dropped is accounted for, with the engine's own skip shape.
    let dropped: Vec<&SkippedCase> = narrowed
        .skipped
        .iter()
        .filter(|s| s.reason.contains("change-impact"))
        .collect();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].case_id, "b");

    // The pre-existing skip reason survives untouched.
    assert!(
        narrowed
            .skipped
            .iter()
            .any(|s| s.case_id == "d" && s.reason.contains("same task")),
        "selection must not rewrite the engine's own reasons"
    );

    // The mandatory case cannot be subtracted, even by naming it nowhere.
    let without_mandatory = full.restricted_to(&selected, &BTreeSet::new());
    assert!(
        !without_mandatory
            .tasks
            .iter()
            .any(|(c, _)| c.case_id == "c"),
        "without the floor, `c` is legitimately dropped"
    );
    let _ = BTreeMap::<String, RunPlan>::new();
}

/// Selecting nothing must not empty the run: the floor always survives.
#[tokio::test]
async fn an_empty_selection_still_retains_the_mandatory_set() {
    use crate::domain::TaskDef as RTDef;
    use crate::regression::ReplayedCase;
    let task = RTDef {
        id: "task-m".into(),
        name: "mandatory".into(),
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
        tasks: vec![(
            ReplayedCase {
                case_id: "m".into(),
                task_id: "task-m".into(),
                title: "mandatory".into(),
                source_run_id: Uuid::new_v4().to_string(),
            },
            task,
        )],
        skipped: Vec::new(),
    };
    let mandatory: BTreeSet<String> = ["m".to_string()].into_iter().collect();
    let narrowed = full.restricted_to(&BTreeSet::new(), &mandatory);
    assert_eq!(
        narrowed.tasks.len(),
        1,
        "an empty impact selection must not empty a regression run"
    );
}

// ------------------------------------------------------------------ migration

/// Phase 3 adds exactly one migration, and it is additive.
///
/// Asserted rather than claimed: the count moved from 30 to 31, and this pins both the
/// count and the shape of what was added.
#[tokio::test]
async fn phase_three_added_one_additive_migration() {
    let (store, _d) = temp_store().await;

    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&store.pool)
        .await
        .expect("migration count");
    // 33 since AI Usage Monitoring Phase 3: migration 0033 added `ai_usage` error rollups
    // (http_429_calls, http_5xx_calls, timeout_calls) -- additive columns with default 0.
    //
    // 32 since Phase 5: migration 0032 added `exploration_sessions` and
    // `exploration_findings`. Both are brand-new tables -- nothing altered, nothing
    // dropped, no table recreated -- so this guard moves rather than being silenced.
    assert_eq!(
        applied, 33,
        "30 plus one Phase 3 migration, plus one Phase 5 migration, plus one AI monitoring migration",
    );

    // The columns the feature relies on.
    for (table, column) in [
        ("change_impacts", "project_id"),
        ("change_impacts", "change_ref"),
        ("change_impacts", "affected_node_id"),
        ("change_impacts", "selected_test_node_id"),
        ("change_impacts", "rationale"),
        ("change_impacts", "computed_at"),
    ] {
        let present: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = ?")
                .bind(table)
                .bind(column)
                .fetch_one(&store.pool)
                .await
                .expect("column check");
        assert!(present > 0, "{table}.{column} is required");
    }

    // The unique key that makes re-analysis idempotent, including the NULL arm.
    let idx: String = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'change_impacts' AND name = 'uq_change_impacts_record'",
    )
    .fetch_optional(&store.pool)
    .await
    .expect("index lookup")
    .expect("the unique index must exist, or duplicate records are unbounded");
    assert_eq!(idx, "uq_change_impacts_record");
    store.pool.close().await;
}

/// A storage failure must not be reported as "this change affects nothing".
///
/// This is the difference between "there is no impact" and "we could not look". Before the
/// remediation every read in this reader used `.unwrap_or_default()`, so a closed pool produced a
/// clean, empty, entirely plausible `ChangeImpact` — and, worst, an **empty safety floor**. The
/// floor is the tier that exists so a case with recorded failure evidence is never dropped for
/// speed; silently emptying it means a storage fault makes AIQA test *less*.
///
/// `store.pool.close()` is the same technique `engine_tests` uses to induce a genuine persistence
/// failure: it is a real read failure, not a mock.
#[tokio::test]
async fn a_storage_failure_is_reported_rather_than_returning_an_empty_selection() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P61 StorageFail").await;
    let reader = crate::change_impact::ChangeImpactReader::new(store.clone());

    // A real declaration against a real project, so the only thing missing is the store.
    let res = reader
        .analyse(
            &pid,
            "commit-storage",
            "requirement",
            &[crate::change_impact::ChangeTarget {
                kind: "requirement".into(),
                ref_id: "REQ-P61-1".into(),
            }],
        )
        .await;
    assert!(
        res.is_ok(),
        "a healthy store must still produce an analysis: {:?}",
        res.err()
    );

    store.pool.close().await;

    let res = reader
        .analyse(
            &pid,
            "commit-storage-2",
            "requirement",
            &[crate::change_impact::ChangeTarget {
                kind: "requirement".into(),
                ref_id: "REQ-P61-1".into(),
            }],
        )
        .await;

    assert!(
        res.is_err(),
        "a storage failure must be an error, never an empty ChangeImpact: {:?}",
        res.as_ref().err()
    );
    store.pool.close().await;
}

/// The same for the safety floor on its own, because it is the tier that must never be silently
/// emptied.
#[tokio::test]
async fn a_storage_failure_cannot_silently_empty_the_safety_floor() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P61 FloorFail").await;
    let reader = crate::change_impact::ChangeImpactReader::new(store.clone());

    assert!(
        reader
            .safety_floor(&pid)
            .await
            .expect("healthy read")
            .is_empty(),
        "the fixture has no failures yet, so the floor starts empty for a real reason"
    );

    store.pool.close().await;

    assert!(
        reader.safety_floor(&pid).await.is_err(),
        "an unreadable floor must be an error, not an empty map that reads as \"nothing has \
         ever failed\""
    );
    store.pool.close().await;
}

/// And for the risk tier: a failed read must not select zero high-risk cases while reporting a
/// successful analysis.
#[tokio::test]
async fn a_storage_failure_cannot_silently_empty_the_high_risk_tier() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "P61 RiskFail").await;
    let reader = crate::change_impact::ChangeImpactReader::new(store.clone());

    assert!(
        reader
            .risk_scores(&pid)
            .await
            .expect("healthy read")
            .is_empty(),
        "no scored cases yet"
    );

    store.pool.close().await;

    assert!(
        reader.risk_scores(&pid).await.is_err(),
        "an unreadable risk read must be an error, not an empty map that selects nothing"
    );
    store.pool.close().await;
}
