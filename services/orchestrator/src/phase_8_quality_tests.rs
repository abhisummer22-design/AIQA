//! Phase 8 — tests for test-case quality intelligence.
//!
//! | group | Step 7 scenario | what it pins |
//! |---|---|---|
//! | `p8_quality_*` | complete case, missing expected result, missing linkage, no execution | the four judgements that must differ |
//! | `p8_infra_*` | repeated infrastructure errors, blocked-only | infrastructure is never a product defect, and never a pass |
//! | `p8_scope_*` | two projects with identical case text | project isolation |
//! | `p8_dup_*` | duplicate / near-duplicate cases | similarity uses the existing embeddings and cosine |
//! | `p8_contract_*` | missing evidence, no aggregate score, D3 independence | the contract itself |
//!
//! ## Discipline
//!
//! Every fixture is written through the production writers (`Store::insert_test_case`,
//! `Intelligence::ingest_run`) and every verdict is read back from
//! `test_quality::assess_case`. No test re-implements a dimension's rule; where one is restated in
//! a test, it is the *roadmap's* rule, not the module's code.
//!
//! Expectations are computed by hand from the documented contract, not read out of the
//! implementation.

use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
    TestStep,
};
use crate::intelligence::Intelligence;
use crate::qa_memory::QaMemoryReader;
use crate::store::Store;
use crate::test_quality::{
    assess_case, assess_project, QualityInputs, QualityVerdict, NEAR_DUPLICATE_SIMILARITY,
};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

struct Db {
    store: Store,
    #[allow(dead_code)]
    dir: TempDir,
}

impl Db {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::connect(&dir.path().join("phase8.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, dir }
    }

    /// Phase 9.2B: release the SQLite pool *before* this fixture's `TempDir` is dropped.
    ///
    /// On Windows an open SQLite handle makes `TempDir`'s `remove_dir_all` fail, and `tempfile`
    /// swallows that error, so the directory leaks silently. Only an **awaited** `Pool::close()`
    /// releases those handles: `close()` returns a `Future` the caller must poll
    /// (sqlx-core-0.9.0, `src/pool/mod.rs:441`), so it cannot be deferred to a `Drop` impl --
    /// `impl Drop` would construct that `Future` and drop it unpolled, releasing nothing.
    ///
    /// Consuming `self` is deliberate. The `TempDir` cannot be dropped until this returns, which
    /// makes the ordering structural rather than a convention a later edit could quietly break.
    async fn close(self) {
        self.store.pool.close().await;
    }

    /// Phase 9.2B: the same fixture, rooted in a caller-owned directory so a test can measure
    /// exactly what *this* fixture leaves behind. Counting global `.tmp*` entries would not be a
    /// valid assertion -- `cargo test` runs these tests concurrently, so any other fixture
    /// perturbs the total and the result would depend on scheduling order.
    async fn new_in(root: &std::path::Path) -> Self {
        let dir = tempfile::tempdir_in(root).expect("tempdir");
        let store = Store::connect(&dir.path().join("phase8.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, dir }
    }
}

async fn project(store: &Store, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    store.insert_project(id, name, None).await.expect("project");
    id
}

/// The shape a case should have when it is genuinely well-formed.
#[derive(Default)]
struct CaseSpec {
    title: &'static str,
    description: &'static str,
    requirement_reference: Option<&'static str>,
    expected_result: Option<&'static str>,
    assertions: Vec<&'static str>,
    steps: Vec<(&'static str, &'static str)>, // (action, expected_result)
}

async fn seed_case(store: &Store, pid: Uuid, spec: CaseSpec) -> Uuid {
    let default_steps = |v: &Vec<(&'static str, &'static str)>| -> Vec<TestStep> {
        v.iter()
            .enumerate()
            .map(|(i, (a, e))| TestStep {
                step_number: i as i64 + 1,
                action: (*a).to_string(),
                expected_result: (*e).to_string(),
            })
            .collect()
    };
    store
        .insert_test_case(
            &pid.to_string(),
            &CreateTestCaseRequest {
                title: spec.title.to_string(),
                description: spec.description.to_string(),
                category: "Functional".into(),
                priority: "HIGH".into(),
                severity: "MAJOR".into(),
                preconditions: vec![],
                steps: default_steps(&spec.steps),
                expected_result: spec.expected_result.map(str::to_string),
                assertions: spec.assertions.iter().map(|a| (*a).to_string()).collect(),
                test_data: None,
                source_document: None,
                requirement_reference: spec.requirement_reference.map(str::to_string),
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: vec![],
                suite_id: None,
            },
            Some("phase8"),
        )
        .await
        .expect("insert_test_case")
}

/// A case whose steps the existing deterministic compiler can actually build a plan from.
///
/// The step shape is taken verbatim from the compiler's own test
/// (`deterministic_compiles_a_clear_navigate_step`): a `Navigate to <url>` action with a
/// `Functional` category. Using the engine's own accepted shape rather than an invented one is the
/// point -- the fixture must exercise the real contract, not a guess at it.
fn concrete_spec(title: &'static str, requirement: Option<&'static str>) -> CaseSpec {
    CaseSpec {
        title,
        description: "a well-formed case with a concrete, compilable step",
        requirement_reference: requirement,
        expected_result: Some("Login form is shown"),
        assertions: vec!["the login form is visible"],
        steps: vec![(
            "Navigate to https://example.com/login",
            "Login form is shown",
        )],
    }
}

async fn run_outcomes(
    store: &Store,
    pid: &str,
    case: Uuid,
    outcomes: &[(TaskStatus, Option<&str>)],
) {
    for (i, (status, reason)) in outcomes.iter().enumerate() {
        let run_id = Uuid::new_v4();
        let task_id = format!("t{i}-{run_id}");
        let plan = RunPlan {
            name: "phase 8".into(),
            tasks: vec![TaskDef {
                id: task_id.clone(),
                name: format!("step {i}"),
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
                run_id,
                &plan,
                Some(pid),
                Some("phase8"),
                RunState::Running,
                None,
                None,
            )
            .await
            .expect("run");
        store
            .set_task_status(run_id, &task_id, TaskStatus::Running, None, true, false)
            .await
            .expect("start");
        store
            .set_task_status(run_id, &task_id, *status, *reason, false, true)
            .await
            .expect("finish");
        Intelligence::new(store)
            .ingest_run(
                Some(run_id),
                Some(pid),
                &plan,
                &[TaskState {
                    id: task_id.clone(),
                    name: format!("step {i}"),
                    prerequisites: vec![],
                    status: *status,
                    reason: (*reason).map(ToString::to_string),
                    started_at: None,
                    finished_at: None,
                    test_case_id: Some(case.to_string()),
                }],
            )
            .await
            .expect("ingest");
    }
}

/// The fixture's registered executor kinds.
fn kinds() -> Vec<String> {
    vec!["maestro".to_string(), "simulated".to_string()]
}

/// Build the inputs from a slice the caller keeps alive for the call.
/// Seed a real requirement through the production parser and writer, so a case's declared
/// `requirement_reference` has something genuine to resolve against.
async fn seed_requirement(store: &Store, pid: &str, key: &'static str, text: &'static str) {
    let parsed =
        crate::requirements::parse_document("P8", "p8-spec.md", &format!("- {key}: {text}"));
    store
        .upsert_requirement_document(pid, "p8-spec.md", Some("spec"), "h-p8", text, &parsed)
        .await
        .expect("upsert_requirement_document");
}

fn inputs_for(k: &[String]) -> QualityInputs<'_> {
    QualityInputs { executor_kinds: k }
}

/// Load a case row for assessment.
///
/// Note the signature: `Store::get_test_case` takes **only** the id, so the row load itself is not
/// tenant-scoped. That is a property of the existing store, not something Phase 8 changes. Tenant
/// isolation for the *assessment* is enforced inside `assess_case`, which passes `project_id` to
/// every read it makes -- and `p8_scope_two_projects_with_identical_case_text_do_not_share_evidence`
/// is what proves it, by giving two projects byte-identical case text.
async fn load(store: &Store, case: Uuid) -> crate::store::TestCaseRow {
    store
        .get_test_case(case)
        .await
        .expect("get case")
        .expect("case exists")
}

async fn assess(store: &Store, pid: &str, case: Uuid) -> crate::test_quality::TestCaseQuality {
    let row = load(store, case).await;
    assess_case(store, &row, pid, &inputs_for(&kinds())).await
}

fn verdict(q: &crate::test_quality::TestCaseQuality, key: &str) -> QualityVerdict {
    q.get(key)
        .unwrap_or_else(|| panic!("dimension `{key}` must exist; got {:?}", q.unknown_keys()))
        .verdict
}

// ===========================================================================
// A complete case vs. an incomplete one
// ===========================================================================

/// Step 7: a complete test case with a linked requirement.
///
/// The point of this test is not that the case looks good -- it is that every dimension that
/// *can* be measured is measured, and the ones that cannot say so rather than defaulting to a
/// flattering answer.
#[tokio::test]
async fn p8_quality_a_complete_case_is_measured_not_assumed() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 complete").await;
    let pid_s = pid.to_string();
    seed_requirement(
        store,
        &pid_s,
        "FR-001",
        "The system shall present a login form at /login.",
    )
    .await;
    let case = seed_case(
        store,
        pid,
        concrete_spec("Health returns 200", Some("FR-001")),
    )
    .await;

    let q = assess(store, &pid_s, case).await;

    // The dimensions that read the case definition itself are measured.
    assert_eq!(
        verdict(&q, "expected_result_present"),
        QualityVerdict::Adequate,
        "the case states an expected result"
    );
    assert_eq!(
        verdict(&q, "concrete_steps"),
        QualityVerdict::Adequate,
        "the existing deterministic compiler builds a real plan from these steps"
    );
    assert_eq!(
        verdict(&q, "automation_feasibility"),
        QualityVerdict::Adequate,
        "executors are registered"
    );

    assert_eq!(
        verdict(&q, "requirement_linkage"),
        QualityVerdict::Adequate,
        "the declared requirement_reference resolves to a real requirement row"
    );

    // The dimensions that need evidence this case has not produced must say so.
    for key in [
        "duplicate_similarity",
        "change_relevance",
        "execution_reliability",
    ] {
        assert_ne!(
            verdict(&q, key),
            QualityVerdict::Adequate,
            "`{key}` has no evidence for a case that has never run or been linked; it must not \
             report a clean bill of health"
        );
    }
    assert_eq!(
        verdict(&q, "negative_boundary_coverage"),
        QualityVerdict::NotAssessed,
        "the schema has no source for this dimension, and that is reported rather than guessed"
    );

    // And the honest combination is not "adequate".
    assert_eq!(
        q.overall(),
        QualityVerdict::InsufficientData,
        "most dimensions are unmeasured, so the case cannot be called adequate"
    );
    db.close().await;
}

/// Step 7: a test case with missing steps or expected results.
///
/// This is the one case that is a *measured defect* rather than missing evidence: the row itself
/// is authoritative and it is genuinely empty.
#[tokio::test]
async fn p8_quality_an_incomplete_case_is_weak_not_unmeasured() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 incomplete").await;
    let pid_s = pid.to_string();
    let case = seed_case(
        store,
        pid,
        CaseSpec {
            title: "vague",
            description: "",
            requirement_reference: None,
            expected_result: None,
            assertions: vec![],
            steps: vec![],
        },
    )
    .await;

    let q = assess(store, &pid_s, case).await;

    assert_eq!(
        verdict(&q, "expected_result_present"),
        QualityVerdict::Weak,
        "no expected result and no assertion: nothing declares what an observer should see"
    );
    assert_eq!(
        verdict(&q, "concrete_steps"),
        QualityVerdict::Weak,
        "no steps at all is a fact about the row, not missing evidence"
    );
    assert_eq!(
        verdict(&q, "requirement_linkage"),
        QualityVerdict::Weak,
        "no requirement reference and no covers edge"
    );
    assert_eq!(q.overall(), QualityVerdict::Weak);
    assert!(q.weak_count() >= 3, "got {:?}", q.dimensions);
    db.close().await;
}

/// Step 7: an existing test with insufficient execution history.
#[tokio::test]
async fn p8_quality_no_execution_history_is_unmeasured_not_perfect_and_not_defective() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 no history").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Never run", None)).await;

    let q = assess(store, &pid_s, case).await;
    let d = q.get("execution_reliability").expect("dimension");

    assert_eq!(
        d.verdict,
        QualityVerdict::InsufficientData,
        "brief rule 9: never label a case defective merely because nothing was recorded"
    );
    assert_ne!(
        d.verdict,
        QualityVerdict::Adequate,
        "and equally: never grant a perfect score merely because nothing failed"
    );
    assert!(
        d.evidence.contains("recorded rows=0"),
        "the evidence must say what was actually read: {}",
        d.evidence
    );
    db.close().await;
}

// ===========================================================================
// Infrastructure is never a product defect, and never a pass
// ===========================================================================

/// Step 8 negative test: infrastructure failures must not be read as product-test failures.
#[tokio::test]
async fn p8_infra_repeated_infrastructure_errors_are_not_a_product_defect() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 infra").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Needs a browser", None)).await;
    run_outcomes(
        store,
        &pid_s,
        case,
        &[
            (TaskStatus::InfraError, Some("no browser available")),
            (TaskStatus::InfraError, Some("no browser available")),
            (TaskStatus::InfraError, Some("dead subprocess")),
        ],
    )
    .await;

    let q = assess(store, &pid_s, case).await;
    let d = q.get("execution_reliability").expect("dimension");

    assert_eq!(
        d.verdict,
        QualityVerdict::InsufficientData,
        "three infrastructure errors and no product outcome: reliability is UNKNOWN, not bad"
    );
    assert_ne!(
        d.verdict,
        QualityVerdict::Weak,
        "the case is not defective because the executor could not start"
    );
    assert!(
        d.evidence.contains("infrastructure_errors=3"),
        "the infrastructure problem stays visible: {}",
        d.evidence
    );
    assert!(
        d.evidence.contains("completed(PASSED|FAILED|TIMEOUT)=0"),
        "and it is outside the completed denominator: {}",
        d.evidence
    );
    db.close().await;
}

/// Step 7: a test case with only blocked executions -- the same rule, a different non-completion.
#[tokio::test]
async fn p8_infra_blocked_only_executions_are_unmeasured() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 blocked").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Blocked", None)).await;
    run_outcomes(
        store,
        &pid_s,
        case,
        &[
            (TaskStatus::Blocked, Some("prerequisite did not pass")),
            (TaskStatus::Blocked, Some("prerequisite did not pass")),
        ],
    )
    .await;

    let q = assess(store, &pid_s, case).await;
    let d = q.get("execution_reliability").expect("dimension");
    assert_eq!(
        d.verdict,
        QualityVerdict::InsufficientData,
        "blocked steps never reached a verdict"
    );
    assert!(d.evidence.contains("recorded rows=2"), "{}", d.evidence);
    db.close().await;
}

/// A genuine repeated failure IS measured, and is not confused with the above.
#[tokio::test]
async fn p8_infra_genuine_repeated_failures_are_measured() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 real failures").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Genuinely broken", None)).await;
    run_outcomes(
        store,
        &pid_s,
        case,
        &[
            (TaskStatus::Failed, Some("ASSERTION_FAILURE: expected 200")),
            (TaskStatus::Failed, Some("ASSERTION_FAILURE: expected 200")),
        ],
    )
    .await;

    let q = assess(store, &pid_s, case).await;
    let d = q.get("execution_reliability").expect("dimension");
    assert_eq!(
        d.verdict,
        QualityVerdict::Adequate,
        "two completed failures is a real measurement: {}",
        d.evidence
    );
    assert!(d.summary.contains("2 of 2"), "{}", d.summary);
    db.close().await;
}

// ===========================================================================
// Project isolation (Step 8 negative test)
// ===========================================================================

/// Step 8 negative test: cross-project evidence contamination.
#[tokio::test]
async fn p8_scope_two_projects_with_identical_case_text_do_not_share_evidence() {
    let db = Db::new().await;
    let store = &db.store;
    let a = project(store, "P8 scope A").await;
    let b = project(store, "P8 scope B").await;
    let a_s = a.to_string();
    let b_s = b.to_string();

    // Byte-identical case text in both projects.
    let case_a = seed_case(store, a, concrete_spec("Identical", None)).await;
    let case_b = seed_case(store, b, concrete_spec("Identical", None)).await;

    // Only A has any execution history.
    run_outcomes(
        store,
        &a_s,
        case_a,
        &[(TaskStatus::Failed, Some("ASSERTION_FAILURE: only in A"))],
    )
    .await;

    let qa = assess(store, &a_s, case_a).await;
    let qb = assess(store, &b_s, case_b).await;

    assert_eq!(
        verdict(&qa, "execution_reliability"),
        QualityVerdict::Adequate,
        "project A has a completed failure"
    );
    assert_eq!(
        verdict(&qb, "execution_reliability"),
        QualityVerdict::InsufficientData,
        "project B's case has identical text but NO history -- evidence must not leak across the \
         tenant boundary. Got {}",
        qb.get("execution_reliability").unwrap().evidence
    );
    assert_ne!(
        qa.test_case_id, qb.test_case_id,
        "and they are genuinely different cases"
    );
    assert_eq!(
        qb.project_id, b_s,
        "each assessment is labelled with its own project"
    );
    db.close().await;
}

/// The project-wide sweep must not include another project's cases.
#[tokio::test]
async fn p8_scope_assess_project_returns_only_that_projects_cases() {
    let db = Db::new().await;
    let store = &db.store;
    let a = project(store, "P8 sweep A").await;
    let b = project(store, "P8 sweep B").await;
    for p in [a, b] {
        for i in 0..3 {
            seed_case(store, p, concrete_spec("Case", None)).await;
            let _ = i;
        }
    }

    let results = assess_project(store, &a.to_string(), &inputs_for(&kinds())).await;
    assert_eq!(results.len(), 3, "project A has exactly 3 cases");
    assert!(
        results.iter().all(|q| q.project_id == a.to_string()),
        "every assessment is labelled with project A"
    );
    assert!(
        results.iter().all(|q| q.dimensions.len() == 8),
        "every case gets all eight dimensions"
    );
    db.close().await;
}

// ===========================================================================
// Duplicate similarity
// ===========================================================================

/// Step 7: duplicate and near-duplicate cases, through the real embedding table.
#[tokio::test]
async fn p8_dup_near_duplicate_cases_are_flagged_and_distinct_ones_are_not() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 dup").await;
    let pid_s = pid.to_string();

    let a = seed_case(store, pid, concrete_spec("Duplicate", None)).await;
    let b = seed_case(store, pid, concrete_spec("Duplicate", None)).await;
    let c = seed_case(store, pid, concrete_spec("Distinct", None)).await;

    // Real embeddings through the production writer. Unit vectors: a and b are parallel, c is
    // orthogonal, so the cosine result is exact and not a rounding accident.
    store
        .upsert_test_case_embedding(a, &pid_s, "test-model", 3, &[1.0, 0.0, 0.0])
        .await
        .expect("embed a");
    store
        .upsert_test_case_embedding(b, &pid_s, "test-model", 3, &[1.0, 0.0, 0.0])
        .await
        .expect("embed b");
    store
        .upsert_test_case_embedding(c, &pid_s, "test-model", 3, &[0.0, 1.0, 0.0])
        .await
        .expect("embed c");

    let qa = assess(store, &pid_s, a).await;
    let qd = verdict(&qa, "duplicate_similarity");
    assert_eq!(
        qd,
        QualityVerdict::Weak,
        "a parallel embedding is similarity 1.0, at or above {}: {}",
        NEAR_DUPLICATE_SIMILARITY,
        qa.get("duplicate_similarity").unwrap().evidence
    );
    assert!(
        qa.get("duplicate_similarity")
            .unwrap()
            .evidence
            .contains(&b.to_string()),
        "and it names the neighbour: {}",
        qa.get("duplicate_similarity").unwrap().evidence
    );

    let qc = assess(store, &pid_s, c).await;
    assert_eq!(
        verdict(&qc, "duplicate_similarity"),
        QualityVerdict::Adequate,
        "an orthogonal embedding is similarity 0.0, well below the threshold"
    );
    db.close().await;
}

/// A case with no embedding row cannot be judged for duplication -- not "no duplicates".
#[tokio::test]
async fn p8_dup_a_case_with_no_embedding_is_unmeasured() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 dup missing").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Not embedded", None)).await;

    let q = assess(store, &pid_s, case).await;
    let d = q.get("duplicate_similarity").expect("dimension");
    assert_eq!(
        d.verdict,
        QualityVerdict::InsufficientData,
        "`test_case_embeddings` only holds APPROVED cases, so this one has no row"
    );
    assert!(
        !d.summary.contains("no duplicates"),
        "must not claim a clean result"
    );
    db.close().await;
}

// ===========================================================================
// The contract itself
// ===========================================================================

/// Scoring rule 4 and the brief: there is no aggregate number, and that is stated on the object.
#[tokio::test]
async fn p8_contract_there_is_no_aggregate_quality_score() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 contract").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Contract", None)).await;

    let q = assess(store, &pid_s, case).await;

    assert!(
        q.aggregate_basis
            .contains("no weights and no aggregation formula"),
        "the absence of a score must be explained on the object, not merely noticed: {}",
        q.aggregate_basis
    );

    // The serialized form must not smuggle a score out under any name.
    let json = serde_json::to_value(&q).expect("serialize");
    let obj = json.as_object().expect("object");
    for forbidden in [
        "quality_score",
        "score",
        "overall_score",
        "quality_percentage",
        "rating",
        "grade",
    ] {
        assert!(
            !obj.contains_key(forbidden),
            "no aggregate score may appear in the serialized form; found `{forbidden}`"
        );
    }
    assert!(
        obj.contains_key("dimensions"),
        "the per-dimension detail is what is reported"
    );
    assert_eq!(
        obj["dimensions"].as_array().map(|a| a.len()),
        Some(8),
        "all eight roadmap dimensions are represented"
    );
    db.close().await;
}

/// Every dimension explains itself: rule 3, "explain every score and deduction".
#[tokio::test]
async fn p8_contract_every_dimension_explains_itself() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 explain").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Explained", None)).await;

    let q = assess(store, &pid_s, case).await;
    for d in &q.dimensions {
        assert!(
            d.summary.len() > 20,
            "dimension `{}` has an unusable summary: {:?}",
            d.key,
            d.summary
        );
        assert!(
            !d.evidence.is_empty(),
            "dimension `{}` reports a verdict with no evidence",
            d.key
        );
        assert!(d.dimension.len() > 5, "dimension `{}` has no name", d.key);
        // Rule 2: any heuristic must be labelled. Phase 8 ships none.
        assert!(
            d.deterministic,
            "Phase 8 introduces no heuristic; dimension `{}` claims to be non-deterministic",
            d.key
        );
    }
    db.close().await;
}

/// D3: Phase 8 must not consume a `failure_severity` value from either implementation.
#[tokio::test]
async fn p8_contract_phase8_does_not_depend_on_either_severity_implementation() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 D3").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("D3", None)).await;
    run_outcomes(
        store,
        &pid_s,
        case,
        &[
            (TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            (TaskStatus::Timeout, None),
        ],
    )
    .await;

    let q = assess(store, &pid_s, case).await;
    let serialized = serde_json::to_string(&q).expect("serialize");

    assert!(
        !serialized.contains("failure_severity"),
        "Phase 8 must not read a severity value, so D3 cannot block it: {serialized}"
    );
    assert!(
        !serialized.contains("category_weight"),
        "and must not reach the category-weighted algorithm either"
    );
    // The execution dimension is driven by the completed-rate helpers, not by severity.
    let d = q.get("execution_reliability").expect("dimension");
    assert!(
        d.evidence.contains("completed_failure_rate_fraction"),
        "the contract actually used must be named: {}",
        d.evidence
    );
    db.close().await;
}

/// Phase 7.4 contracts are preserved: the completed rate is a fraction and stays excluded from
/// both halves by `INFRA_ERROR`.
#[tokio::test]
async fn p8_contract_the_phase_7_4_metric_contracts_are_preserved() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P8 7.4").await;
    let pid_s = pid.to_string();
    let case = seed_case(store, pid, concrete_spec("Contracts", None)).await;
    run_outcomes(
        store,
        &pid_s,
        case,
        &[
            (TaskStatus::Failed, Some("ASSERTION_FAILURE: x")),
            (TaskStatus::Passed, None),
            (TaskStatus::InfraError, Some("no browser available")),
            (TaskStatus::Blocked, Some("prereq")),
        ],
    )
    .await;

    let rows: Vec<crate::store::ExecutionTelemetryRow> = store
        .list_execution_telemetry(&pid_s, 100)
        .await
        .expect("telemetry");
    let mine: Vec<&crate::store::ExecutionTelemetryRow> = rows
        .iter()
        .filter(|r| r.test_case_id == Some(case))
        .collect();

    // 1 FAILED + 1 PASSED completed; INFRA_ERROR and BLOCKED excluded from both halves.
    assert_eq!(QaMemoryReader::completed_count(&mine), 2);
    assert_eq!(QaMemoryReader::infra_errors(&mine), 1);
    let frac = QaMemoryReader::completed_failure_rate_fraction(&mine).expect("fraction");
    assert!(
        (frac - 0.5).abs() < 1e-9,
        "the completed rate is the 0..1 fraction the risk model expects, got {frac}"
    );

    let q = assess(store, &pid_s, case).await;
    let d = q.get("execution_reliability").expect("dimension");
    assert!(
        d.evidence.contains("infrastructure_errors=1"),
        "and Phase 8 reports the infrastructure count without folding it in: {}",
        d.evidence
    );
    db.close().await;
}

/// Step 8 negative test: a store read failure must be reported as unknown, never as a clean or
/// Step 8 negative test: a storage-read failure must be reported as unknown, never as a clean or
/// empty assessment.
///
/// **This test was rewritten after the mutation exercise found a gap in it.** The first version
/// loaded the case row *after* closing the pool, so `list_test_cases` failed, the row was `None`,
/// and the function returned early without ever reaching the failure path. It passed while the
/// implementation was mutated to report a read failure as a clean `Adequate`. The row is now
/// loaded first, so the assessment genuinely runs against a dead pool.
#[tokio::test]
async fn p8_contract_a_storage_read_failure_is_not_disguised_as_an_empty_assessment() {
    let db = Db::new().await;
    let store = db.store.clone();
    let pid = project(&store, "P8 storage").await;
    let pid_s = pid.to_string();
    let case = seed_case(&store, pid, concrete_spec("Storage", None)).await;

    // Load the row FIRST, while the pool is still alive -- otherwise there is nothing to assess.
    let row = load(&store, case).await;
    assert_eq!(row.id, case);

    // Before the fault, the telemetry-backed dimension is a real (unmeasured) read.
    let before = assess_case(&store, &row, &pid_s, &inputs_for(&kinds())).await;
    let before_d = before.get("execution_reliability").expect("dimension");
    assert_eq!(
        before_d.verdict,
        QualityVerdict::InsufficientData,
        "a case with no executions is unmeasured even with a healthy store"
    );
    assert!(
        before_d.evidence.contains("recorded rows=0"),
        "and the evidence names what was read: {}",
        before_d.evidence
    );

    // Now break the store for real.
    store.pool.close().await;

    let after = assess_case(&store, &row, &pid_s, &inputs_for(&kinds())).await;
    let after_d = after.get("execution_reliability").expect("dimension");

    assert_ne!(
        after_d.verdict,
        QualityVerdict::Adequate,
        "a failed telemetry read must never be reported as a clean measurement. Got: {:?} / {}",
        after_d.verdict,
        after_d.summary
    );
    assert_eq!(
        after_d.verdict,
        QualityVerdict::InsufficientData,
        "it must be reported as unknown"
    );
    assert!(
        after_d.evidence.contains("store read failed"),
        "and the failure must be visible in the evidence, not swallowed: {}",
        after_d.evidence
    );
    assert_ne!(
        after_d.evidence, before_d.evidence,
        "the evidence must distinguish 'nothing recorded' from 'could not read'"
    );

    // Every dimension whose source is the store must also refuse to claim a measurement.
    for key in [
        "duplicate_similarity",
        "change_relevance",
        "requirement_linkage",
    ] {
        let d = after.get(key).expect("dimension");
        assert_ne!(
            d.verdict,
            QualityVerdict::Adequate,
            "dimension `{key}` claimed Adequate after its store read failed: {}",
            d.evidence
        );
        assert!(
            d.evidence.contains("store read failed"),
            "dimension `{key}` must name the failure: {}",
            d.evidence
        );
    }

    // Dimensions that read only the already-loaded row are still allowed to answer, and must.
    assert_eq!(
        after.get("expected_result_present").unwrap().verdict,
        QualityVerdict::Adequate,
        "this dimension reads the loaded row, not the store, so it is unaffected"
    );
    db.close().await;
}

// ===========================================================================
// Phase 9.2B — fixture lifecycle (Stage A, Batch 1)
// ===========================================================================
//
// These do not test quality or readiness logic. They test that `Db::close()` actually releases the
// SQLite pool, because a green suite is not evidence that a directory was removed -- `tempfile`
// swallows the Windows removal error that causes the leak in the first place. Every assertion below
// reads the filesystem directly, and each measures only inside a root it owns, so it is immune to
// the other fixtures running concurrently.

/// The private measurement root owned by one test. Only this namespace is ever removed.
fn p92b_root(test_name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir()
        .join("aiqa_p92b_quality_artifacts")
        .join(test_name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create isolated artifact root");
    root
}

fn p92b_dir_count(root: &std::path::Path) -> usize {
    std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

/// Steps 4.1 & 4.2: the fixture produces a genuinely migrated, writable database.
#[tokio::test]
async fn p92b_fixture_is_usable_and_fully_migrated() {
    let root = p92b_root("usable_and_migrated");
    let db = Db::new_in(&root).await;

    let tables: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
         ('projects','test_cases','execution_telemetry','runs','quality_nodes')",
    )
    .fetch_one(&db.store.pool)
    .await
    .expect("query sqlite_master");
    assert_eq!(tables.0, 5, "every fixture must run the real migrations");

    let pid = project(&db.store, "P92B lifecycle").await;
    let cases = db
        .store
        .list_test_cases(&pid.to_string(), None, None)
        .await
        .expect("list_test_cases");
    assert!(
        cases.is_empty(),
        "a fresh migrated fixture starts with no cases"
    );

    db.close().await;
}

/// Steps 4.4 & 4.5: the close is awaited before the `TempDir` is removed, and the SQLite file plus
/// its WAL/SHM sidecars are not what blocks the removal.
#[tokio::test]
async fn p92b_close_removes_the_owned_directory_despite_wal_and_shm() {
    let root = p92b_root("close_removes_directory");
    let db = Db::new_in(&root).await;
    let path = db.dir.path().to_path_buf();

    // Real writes, so the WAL/SHM sidecars genuinely exist rather than this being an easy case.
    project(&db.store, "P92B wal").await;
    let live = std::fs::read_dir(&path)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        live.iter()
            .any(|f| f.ends_with("-wal") || f.ends_with("-shm")),
        "WAL/SHM sidecars must exist while the fixture is live, else this test proves nothing: \
         {live:?}"
    );

    db.close().await; // awaiting this is the whole mechanism

    assert!(
        !path.exists(),
        "after an awaited `Db::close()` the directory must be gone. If it remains, an open SQLite \
         handle -- not the fixture -- is what blocked removal."
    );
    assert_eq!(
        p92b_dir_count(&root),
        0,
        "the measurement root must be empty"
    );
}

/// Step 4.6: repeated create/destroy leaves no new artifacts in the isolated root.
#[tokio::test]
async fn p92b_repeated_fixtures_accumulate_nothing() {
    let root = p92b_root("repeated_fixtures");
    assert_eq!(
        p92b_dir_count(&root),
        0,
        "the measurement root starts empty"
    );
    for _ in 0..10 {
        Db::new_in(&root).await.close().await;
    }
    let after = p92b_dir_count(&root);
    assert_eq!(
        after, 0,
        "ten fixture cycles must leave zero directories in the isolated root; {after} remained"
    );
}

/// Step 4.7: concurrent fixtures each remove exactly their own directory. Tasks are spawned onto
/// the test runtime and awaited together, so they genuinely interleave at their `.await` points.
#[tokio::test]
async fn p92b_concurrent_fixtures_do_not_interfere() {
    let root = p92b_root("concurrent_fixtures");
    assert_eq!(
        p92b_dir_count(&root),
        0,
        "the measurement root starts empty"
    );

    let mut handles = Vec::new();
    for i in 0..8 {
        let root = root.clone();
        handles.push(tokio::spawn(async move {
            let db = Db::new_in(&root).await;
            let path = db.dir.path().to_path_buf();
            // Prove the fixture was real before releasing it, so a fixture that never created its
            // database cannot pass by never existing.
            assert!(
                std::fs::read_dir(&path).unwrap().count() > 0,
                "concurrent fixture {i} must have created its database before teardown"
            );
            db.close().await;
            path
        }));
    }

    let mut paths = Vec::new();
    for h in handles {
        paths.push(h.await.expect("concurrent fixture task must not panic"));
    }
    assert_eq!(
        p92b_dir_count(&root),
        0,
        "eight concurrent fixtures must leave nothing behind -- neither their own nor each \
         other's"
    );
    for p in &paths {
        assert!(
            !p.exists(),
            "each fixture must remove exactly its own directory; {} survived",
            p.display()
        );
    }
}

/// Step 4.9: a teardown failure must be observable. `TempDir::close()` is the fallible form and
/// returns the real removal error; asserting on it is what stops a regression being silent.
#[tokio::test]
async fn p92b_teardown_failure_is_observable() {
    let root = p92b_root("teardown_failure_observable");
    let db = Db::new_in(&root).await;
    let path = db.dir.path().to_path_buf();

    // Same teardown as `Db::close`, but reaching into the fields so the fallible removal can be
    // inspected. If the pool were still holding the file this returns Err.
    let Db { store, dir } = db;
    store.pool.close().await;
    let result = dir.close();
    assert!(
        result.is_ok(),
        "an awaited pool close must let TempDir::close() succeed; got {:?}",
        result.err()
    );
    assert!(
        !path.exists(),
        "and the directory must really be gone, not merely reported as closed"
    );
}
