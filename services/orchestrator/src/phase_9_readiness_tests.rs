//! Phase 9 — tests for release readiness intelligence.
//!
//! | group | roadmap scenario | what it pins |
//! |---|---|---|
//! | `p9_read_*` | complete evidence; product failures; infra-only; blocked-only; no history | the four verdicts and what separates them |
//! | `p9_evid_*` | unexecuted mandatory test; storage failure; stale/absent evidence | evidence problems never read as a pass |
//! | `p9_scope_*` | project isolation; release isolation | another project's release is never answered |
//! | `p9_contract_*` | output shape, rule version, no score, Phase 8 quality reuse | the roadmap's Output block |
//!
//! ## Discipline
//!
//! Fixtures are written through production writers only (`create_release`, `insert_run_for_release`,
//! `set_task_status`, `Intelligence::ingest_run`). Every verdict is read back from
//! `release_readiness::assess_release`. Expected values are derived from the documented rule order,
//! not from the implementation.

use crate::domain::{
    CreateTestCaseRequest, RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus,
    TestStep,
};
use crate::intelligence::Intelligence;
use crate::release_readiness::{
    assess_release, EvidenceScope, ReleaseReadiness, ReleaseRecommendation, RULE_VERSION,
};
use crate::store::Store;
use crate::test_quality::QualityInputs;
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
        let store = Store::connect(&dir.path().join("phase9.sqlite3"))
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
        let store = Store::connect(&dir.path().join("phase9.sqlite3"))
            .await
            .expect("connect + migrate");
        Self { store, dir }
    }
}

fn kinds() -> Vec<String> {
    vec!["maestro".to_string(), "simulated".to_string()]
}

fn inputs_for(k: &[String]) -> QualityInputs<'_> {
    QualityInputs { executor_kinds: k }
}

async fn project(store: &Store, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    store.insert_project(id, name, None).await.expect("project");
    id
}

async fn release(store: &Store, pid: &str, label: &str) -> Uuid {
    store
        .create_release(pid, Some(label), Some(label), None, None)
        .await
        .expect("create_release")
        .id
}

async fn seed_case(store: &Store, pid: Uuid) -> Uuid {
    store
        .insert_test_case(
            &pid.to_string(),
            &CreateTestCaseRequest {
                title: "Checkout".into(),
                description: "a concrete case".into(),
                category: "Functional".into(),
                priority: "HIGH".into(),
                severity: "MAJOR".into(),
                preconditions: vec![],
                steps: vec![TestStep {
                    step_number: 1,
                    action: "Navigate to https://example.com/login".into(),
                    expected_result: "Login form is shown".into(),
                }],
                expected_result: Some("Login form is shown".into()),
                assertions: vec!["the login form is visible".into()],
                test_data: None,
                source_document: None,
                requirement_reference: Some("FR-001".into()),
                source_requirement: None,
                traceability: None,
                risk_factor: None,
                tags: vec![],
                suite_id: None,
            },
            Some("phase9"),
        )
        .await
        .expect("insert_test_case")
}

/// Record one execution of `case` against `release`, through the production writers.
async fn execute(
    store: &Store,
    pid: &str,
    case: Uuid,
    rel: Uuid,
    index: usize,
    status: TaskStatus,
    reason: Option<&str>,
) {
    let run_id = Uuid::new_v4();
    let task_id = format!("t{index}-{run_id}");
    let plan = RunPlan {
        name: "phase 9".into(),
        tasks: vec![TaskDef {
            id: task_id.clone(),
            name: format!("step {index}"),
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
            Some("phase9"),
            RunState::Running,
            None,
            Some(&rel.to_string()),
        )
        .await
        .expect("insert_run_for_release");
    store
        .set_task_status(run_id, &task_id, TaskStatus::Running, None, true, false)
        .await
        .expect("start");
    store
        .set_task_status(run_id, &task_id, status, reason, false, true)
        .await
        .expect("finish");
    Intelligence::new(store)
        .ingest_run(
            Some(run_id),
            Some(pid),
            &plan,
            &[TaskState {
                id: task_id.clone(),
                name: format!("step {index}"),
                prerequisites: vec![],
                status,
                reason: reason.map(ToString::to_string),
                started_at: None,
                finished_at: None,
                test_case_id: Some(case.to_string()),
            }],
        )
        .await
        .expect("ingest");
}

async fn assess(store: &Store, pid: &str, rel: Uuid) -> ReleaseReadiness {
    assess_release(store, pid, rel, &inputs_for(&kinds()))
        .await
        .expect("assess_release")
}

fn signal<'a>(
    r: &'a ReleaseReadiness,
    kind: &str,
) -> Option<&'a crate::release_readiness::ReadinessSignal> {
    r.signals.iter().find(|s| s.kind == kind)
}

fn missing<'a>(
    r: &'a ReleaseReadiness,
    kind: &str,
) -> Option<&'a crate::release_readiness::MissingSignal> {
    r.missing_signals.iter().find(|m| m.kind == kind)
}

// ===========================================================================
// The four verdicts
// ===========================================================================

/// Roadmap required test: "all required checks pass".
///
/// The only shape that may reach `GO`: every gate read real evidence, the completed rate is 0.0,
/// and there is no infrastructure error smudging the result.
#[tokio::test]
async fn p9_read_a_release_with_complete_passing_evidence_is_go() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 go").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;

    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;
    execute(store, &pid_s, case, rel, 2, TaskStatus::Passed, None).await;

    let r = assess(store, &pid_s, rel).await;

    assert_eq!(
        r.recommendation,
        ReleaseRecommendation::Go,
        "two completed passes and no infrastructure error: {:?}",
        r.blocked_criteria
    );
    assert!(r.blocked_criteria.is_empty(), "no gate blocked");
    assert_eq!(
        r.rule_version, RULE_VERSION,
        "the deciding rule set is recorded"
    );
    let rate = signal(&r, "release.completed_failure_rate").expect("rate signal");
    assert_eq!(rate.scope, EvidenceScope::Release);
    assert!(
        rate.evidence
            .contains("completed_failure_rate_fraction=Some(0.0)"),
        "{}",
        rate.evidence
    );
    db.close().await;
}

/// Roadmap required test: "critical failure".
#[tokio::test]
async fn p9_read_a_release_with_a_product_failure_is_no_go() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 fail").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;

    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;
    execute(
        store,
        &pid_s,
        case,
        rel,
        2,
        TaskStatus::Failed,
        Some("ASSERTION_FAILURE: expected 200"),
    )
    .await;

    let r = assess(store, &pid_s, rel).await;

    assert_eq!(r.recommendation, ReleaseRecommendation::NoGo);
    assert!(
        r.blocked_criteria.iter().any(|b| b.rule_id == "R6"),
        "R6 must be the blocker: {:?}",
        r.blocked_criteria
            .iter()
            .map(|b| b.rule_id)
            .collect::<Vec<_>>()
    );
    let b = r
        .blocked_criteria
        .iter()
        .find(|b| b.rule_id == "R6")
        .unwrap();
    assert!(
        b.evidence.contains("1 FAILED/TIMEOUT of 2 completed"),
        "the evidence must name the counted outcome: {}",
        b.evidence
    );
    db.close().await;
}

/// Roadmap required test: infrastructure errors but no confirmed product failure.
///
/// Rule 4 forbids calling a failure a product defect; conversely the absence of a product failure
/// is not a clean `GO` when the evidence itself was damaged.
#[tokio::test]
async fn p9_read_infrastructure_errors_without_a_product_failure_is_conditional_go() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 infra").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;

    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;
    execute(
        store,
        &pid_s,
        case,
        rel,
        2,
        TaskStatus::InfraError,
        Some("no browser available"),
    )
    .await;
    execute(
        store,
        &pid_s,
        case,
        rel,
        3,
        TaskStatus::InfraError,
        Some("dead subprocess"),
    )
    .await;

    let r = assess(store, &pid_s, rel).await;

    assert_eq!(
        r.recommendation,
        ReleaseRecommendation::ConditionalGo,
        "no product defect, but the evidence is materially incomplete: {:?}",
        r.signals
            .iter()
            .map(|s| (s.kind.clone(), s.summary.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        r.blocked_criteria.is_empty(),
        "infrastructure is not a blocker"
    );
    let infra = signal(&r, "infrastructure.errors").expect("infra signal");
    assert!(
        infra.evidence.contains("infra_errors=2"),
        "{}",
        infra.evidence
    );
    // The product rate is untouched by the infrastructure errors.
    let rate = signal(&r, "release.completed_failure_rate").expect("rate");
    assert!(
        rate.evidence
            .contains("completed_failure_rate_fraction=Some(0.0)"),
        "the two infrastructure errors must not enter the completed rate: {}",
        rate.evidence
    );
    db.close().await;
}

/// Roadmap required test: "no historical data", and rule 3's core prohibition —
/// a release whose runs never reached a verdict is NOT a release that passed.
#[tokio::test]
async fn p9_read_a_release_with_no_completed_outcomes_is_insufficient_not_go() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 no verdict").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;

    execute(
        store,
        &pid_s,
        case,
        rel,
        1,
        TaskStatus::Blocked,
        Some("prereq"),
    )
    .await;
    execute(
        store,
        &pid_s,
        case,
        rel,
        2,
        TaskStatus::Blocked,
        Some("prereq"),
    )
    .await;

    let r = assess(store, &pid_s, rel).await;

    assert_eq!(
        r.recommendation,
        ReleaseRecommendation::InsufficientData,
        "blocked steps reached no verdict, so nothing can be concluded either way: {:?}",
        r.signals
            .iter()
            .map(|s| s.summary.clone())
            .collect::<Vec<_>>()
    );
    assert_ne!(
        r.recommendation,
        ReleaseRecommendation::Go,
        "rule 3: a release must never be called safe because nothing failed"
    );
    let hist = signal(&r, "evidence.execution_history").expect("history signal");
    assert!(
        hist.summary.contains("This is NOT a pass"),
        "the signal must say so in words: {}",
        hist.summary
    );
    db.close().await;
}

/// A release with no runs at all. `runs.release_id` is never auto-populated, so this is the
/// commonest real state and must be an evidence answer, not a verdict.
#[tokio::test]
async fn p9_read_a_release_with_no_runs_is_insufficient_data() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 empty").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "never-run").await;

    let r = assess(store, &pid_s, rel).await;
    assert_eq!(r.recommendation, ReleaseRecommendation::InsufficientData);
    assert!(
        r.signals[0].summary.contains("never auto-populated"),
        "the reason must be the schema's own contract: {}",
        r.signals[0].summary
    );
    assert!(missing(&r, "evidence.execution_history").is_some());
    db.close().await;
}

// ===========================================================================
// Evidence problems
// ===========================================================================

/// Roadmap required test: "unexecuted mandatory test".
///
/// There is NO declared mandatory flag in this schema, so the floor is derived exactly as
/// `qa_memory_risk` derives it: a test with recorded failure evidence. A floor test that belongs
/// to this project but has no completed outcome anywhere in this release's rows is a gap.
#[tokio::test]
async fn p9_evid_an_unexecuted_mandatory_floor_test_blocks_the_release() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 mandatory").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;

    // The floor case: it failed in an EARLIER release, so it is on the derived floor.
    let floor_case = seed_case(store, pid).await;
    let earlier = release(store, &pid_s, "v0").await;
    execute(
        store,
        &pid_s,
        floor_case,
        earlier,
        1,
        TaskStatus::Failed,
        Some("ASSERTION_FAILURE: real regression"),
    )
    .await;

    // The release under assessment runs a DIFFERENT case, cleanly.
    let clean_case = seed_case(store, pid).await;
    execute(store, &pid_s, clean_case, rel, 1, TaskStatus::Passed, None).await;

    let r = assess(store, &pid_s, rel).await;

    assert!(
        r.blocked_criteria.iter().any(|b| b.rule_id == "R5"),
        "the unexecuted floor test must block: {:?}",
        r.blocked_criteria
            .iter()
            .map(|b| b.rule_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(r.recommendation, ReleaseRecommendation::NoGo);
    let b = r
        .blocked_criteria
        .iter()
        .find(|b| b.rule_id == "R5")
        .unwrap();
    assert!(
        b.evidence.contains(&floor_case.to_string()),
        "the evidence must name the case: {}",
        b.evidence
    );
    assert!(
        b.why.contains("did not pass anything -- it did not run"),
        "and say why in words: {}",
        b.why
    );
    db.close().await;
}

/// The floor is a *derivation*, and must be reported as one -- never as user-declared policy.
#[tokio::test]
async fn p9_evid_the_mandatory_floor_is_reported_as_derived_not_declared() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 floor provenance").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;
    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;

    let r = assess(store, &pid_s, rel).await;
    let m = missing(&r, "regression.mandatory_floor").expect("floor must be reported as missing");
    assert_eq!(
        m.reason, "NO_SOURCE_IN_SCHEMA",
        "there is no declared mandatory flag, so this must be an honest schema gap"
    );
    assert!(
        m.detail
            .contains("NO declared mandatory-test flag anywhere in this schema"),
        "and must say exactly that: {}",
        m.detail
    );
    db.close().await;
}

/// Roadmap required test: required storage reads failing must not read as success.
#[tokio::test]
async fn p9_evid_a_storage_read_failure_is_never_a_successful_assessment() {
    // `db` must outlive the test: it owns the TempDir holding the SQLite file, and the store
    // handle below borrows nothing but must not outlive the file either.
    let db = Db::new().await;
    let store = db.store.clone();
    let pid = project(&store, "P9 storage").await;
    let pid_s = pid.to_string();
    let rel = release(&store, &pid_s, "v1").await;
    let case = seed_case(&store, pid).await;
    execute(&store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;

    let before = assess(&store, &pid_s, rel).await;
    assert_eq!(
        before.recommendation,
        ReleaseRecommendation::Go,
        "sanity: with a healthy store and one completed pass this is GO"
    );

    // Close the pool for real.
    store.pool.close().await;

    // **The established contract is propagation, not degradation.** Phase 7.1 changed every
    // affected reader so a failed store read returns an error rather than being reshaped into
    // `UnknownSubject` -- a masked fault used to look like an absence of data. `assess_release`
    // follows the same rule: `list_releases` is the first required read, so its failure aborts.
    //
    // Returning *no* assessment is the strongest possible form of "must not be presented as
    // complete": there is nothing a caller could mistake for a verdict. The first draft of this
    // test expected a degraded `INSUFFICIENT_DATA` instead, which would have been a weaker and
    // less honest contract than the codebase already uses.
    let after = assess_release(&store, &pid_s, rel, &inputs_for(&kinds())).await;

    assert!(
        after.is_err(),
        "a failed required read must propagate, not yield an assessment a caller could mistake \
         for a verdict: got {:?}",
        after.map(|a| a.recommendation)
    );
    db.close().await;
}

// ===========================================================================
// Isolation
// ===========================================================================

/// Roadmap required test: "different project isolation".
#[tokio::test]
async fn p9_scope_another_projects_release_is_never_answered_from_its_data() {
    let db = Db::new().await;
    let store = &db.store;
    let a = project(store, "P9 A").await;
    let b = project(store, "P9 B").await;
    let a_s = a.to_string();
    let b_s = b.to_string();

    let rel_a = release(store, &a_s, "v1").await;
    let case_a = seed_case(store, a).await;
    // Project A's release has a real product failure.
    execute(
        store,
        &a_s,
        case_a,
        rel_a,
        1,
        TaskStatus::Failed,
        Some("ASSERTION_FAILURE: A is broken"),
    )
    .await;

    // Project B has a clean release.
    let rel_b = release(store, &b_s, "v1").await;
    let case_b = seed_case(store, b).await;
    execute(store, &b_s, case_b, rel_b, 1, TaskStatus::Passed, None).await;

    assert_eq!(
        assess(store, &a_s, rel_a).await.recommendation,
        ReleaseRecommendation::NoGo
    );
    assert_eq!(
        assess(store, &b_s, rel_b).await.recommendation,
        ReleaseRecommendation::Go
    );

    // Now ask project B about project A's release id.
    let crossed = assess(store, &b_s, rel_a).await;
    assert_eq!(
        crossed.recommendation,
        ReleaseRecommendation::InsufficientData,
        "a release id from another project must be unknown, not answered from A's data"
    );
    assert!(
        crossed.signals[0]
            .summary
            .contains("Release ids are project-scoped"),
        "and the reason must be the isolation contract: {}",
        crossed.signals[0].summary
    );
    assert!(
        crossed.signals[0].evidence.contains(&rel_a.to_string()),
        "the evidence must name the id that was not found"
    );
    db.close().await;
}

/// Two projects, byte-identical case text, different outcomes — the evidence must not cross.
#[tokio::test]
async fn p9_scope_identical_cases_in_two_projects_do_not_share_release_evidence() {
    let db = Db::new().await;
    let store = &db.store;
    let a = project(store, "P9 iso A").await;
    let b = project(store, "P9 iso B").await;
    let a_s = a.to_string();
    let b_s = b.to_string();

    let rel_a = release(store, &a_s, "v1").await;
    let case_a = seed_case(store, a).await;
    execute(store, &a_s, case_a, rel_a, 1, TaskStatus::Passed, None).await;

    // Identical case text in B, but its release has no runs at all.
    let rel_b = release(store, &b_s, "v1").await;
    // Created so B holds byte-identical case content, then deliberately left unexecuted.
    let _case_b = seed_case(store, b).await;

    assert_eq!(
        assess(store, &a_s, rel_a).await.recommendation,
        ReleaseRecommendation::Go
    );
    assert_eq!(
        assess(store, &b_s, rel_b).await.recommendation,
        ReleaseRecommendation::InsufficientData,
        "A's completed pass must not be read as B's evidence"
    );
    db.close().await;
}

// ===========================================================================
// The output contract
// ===========================================================================

/// Roadmap Output block (L323–330): every field the roadmap asks for must be present, and there
/// must be no readiness score.
#[tokio::test]
async fn p9_contract_the_output_matches_the_roadmap_output_block() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 output").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;
    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;

    let r = assess(store, &pid_s, rel).await;
    let v = serde_json::to_value(&r).expect("serialize");

    // status
    assert!(
        v["recommendation"].is_string(),
        "a recommendation is present"
    );
    assert!(
        ["GO", "CONDITIONAL_GO", "NO_GO", "INSUFFICIENT_DATA"]
            .contains(&v["recommendation"].as_str().unwrap()),
        "and it is one of the four the roadmap defines: {}",
        v["recommendation"]
    );
    // contributing signals, evidence references, freshness, missing-signal notes, rule version
    assert!(v["signals"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(v["blocked_criteria"].is_array());
    assert!(v["evidence_refs"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(v["missing_signals"].is_array());
    assert_eq!(v["rule_version"], RULE_VERSION);

    // rule 1: no arbitrary 0-100 score as the decision.
    let obj = v.as_object().unwrap();
    for forbidden in [
        "readiness_score",
        "score",
        "readiness_percentage",
        "rating",
        "grade",
        "confidence",
    ] {
        assert!(
            !obj.contains_key(forbidden),
            "rule 1: no readiness score may appear; found `{forbidden}`"
        );
    }

    // Every signal carries its scope, so project evidence is never passed off as release evidence.
    for s in v["signals"].as_array().unwrap() {
        assert!(s["scope"].is_string(), "every signal states its scope: {s}");
        assert!(
            s["evidence"].is_string(),
            "every signal carries evidence: {s}"
        );
        assert!(!s["evidence"].as_str().unwrap().is_empty());
    }
    db.close().await;
}

/// Roadmap required test: "same metrics with different missing-data conditions".
///
/// Identical telemetry, different project history — the verdicts must differ, and the difference
/// must be explained by the missing evidence rather than by the numbers.
#[tokio::test]
async fn p9_contract_same_outcomes_different_missing_data_produce_different_verdicts() {
    let db = Db::new().await;
    let store = &db.store;

    // Both releases have exactly one completed PASS and one INFRA_ERROR.
    let a = project(store, "P9 same A").await;
    let b = project(store, "P9 same B").await;
    let a_s = a.to_string();
    let b_s = b.to_string();
    let rel_a = release(store, &a_s, "v1").await;
    let rel_b = release(store, &b_s, "v1").await;
    let case_a = seed_case(store, a).await;
    let case_b = seed_case(store, b).await;
    execute(store, &a_s, case_a, rel_a, 1, TaskStatus::Passed, None).await;
    execute(
        store,
        &a_s,
        case_a,
        rel_a,
        2,
        TaskStatus::InfraError,
        Some("no browser"),
    )
    .await;
    execute(store, &b_s, case_b, rel_b, 1, TaskStatus::Passed, None).await;
    execute(
        store,
        &b_s,
        case_b,
        rel_b,
        2,
        TaskStatus::InfraError,
        Some("no browser"),
    )
    .await;

    let ra = assess(store, &a_s, rel_a).await;
    let rb = assess(store, &b_s, rel_b).await;

    // Identical outcome evidence...
    let rate_a = signal(&ra, "release.completed_failure_rate").unwrap();
    let rate_b = signal(&rb, "release.completed_failure_rate").unwrap();
    assert_eq!(
        rate_a.evidence, rate_b.evidence,
        "the two projects have byte-identical outcome evidence"
    );
    // ...and the same verdict, because neither has a product failure.
    assert_eq!(ra.recommendation, ReleaseRecommendation::ConditionalGo);
    assert_eq!(rb.recommendation, ReleaseRecommendation::ConditionalGo);

    // Now give A an extra unexecuted floor test; the metrics stay identical, the verdict moves.
    let floor_a = seed_case(store, a).await;
    let earlier_a = release(store, &a_s, "v0").await;
    execute(
        store,
        &a_s,
        floor_a,
        earlier_a,
        1,
        TaskStatus::Failed,
        Some("ASSERTION_FAILURE: real"),
    )
    .await;

    let ra2 = assess(store, &a_s, rel_a).await;
    assert_eq!(
        ra2.recommendation,
        ReleaseRecommendation::NoGo,
        "the extra *missing* evidence, not a different rate, is what changed the verdict"
    );
    assert!(
        ra2.blocked_criteria.iter().any(|b| b.rule_id == "R5"),
        "and the change is explained by a named rule: {:?}",
        ra2.blocked_criteria
            .iter()
            .map(|b| b.rule_id)
            .collect::<Vec<_>>()
    );
    let rate_a2 = signal(&ra2, "release.completed_failure_rate").unwrap();
    assert_eq!(
        rate_a2.evidence, rate_b.evidence,
        "while the measured rate is unchanged -- proving the verdict came from evidence, not a \
         different number"
    );
    db.close().await;
}

/// Phase 8 quality must be carried per case, never aggregated, never a factor.
#[tokio::test]
async fn p9_contract_phase8_quality_is_carried_per_case_and_never_aggregated() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 quality").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;
    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;

    let r = assess(store, &pid_s, rel).await;

    assert_eq!(
        r.case_quality.len(),
        1,
        "the release's one case is assessed"
    );
    let note = &r.case_quality[0];
    assert_eq!(note.test_case_id, case.to_string());
    assert!(
        !note.unknown_dimensions.is_empty(),
        "the gaps must be visible, which is the point of carrying it whole: {:?}",
        note.unknown_dimensions
    );

    let v = serde_json::to_value(&r).expect("serialize");
    assert!(
        v["case_quality"].as_array().is_some_and(|a| a.len() == 1),
        "quality is a list, one entry per case"
    );
    // No aggregate quality number anywhere.
    let obj = v.as_object().unwrap();
    for forbidden in ["quality_score", "aggregate_quality", "quality_percentage"] {
        assert!(
            !obj.contains_key(forbidden),
            "brief: do not create a single quality number; found `{forbidden}`"
        );
    }
    // And quality did not become a blocking criterion.
    assert!(
        r.blocked_criteria
            .iter()
            .all(|b| !b.rule_id.starts_with("Q")),
        "Phase 8 quality is informative, never a release gate: {:?}",
        r.blocked_criteria
            .iter()
            .map(|b| b.rule_id)
            .collect::<Vec<_>>()
    );
    db.close().await;
}

/// Affected requirements / modules must only be populated where a real link exists (rule 7).
#[tokio::test]
async fn p9_contract_affected_links_are_never_invented() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 links").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await; // declares requirement_reference = FR-001
    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;

    let r = assess(store, &pid_s, rel).await;
    assert_eq!(
        r.affected_requirements,
        vec!["FR-001".to_string()],
        "the requirement comes from the case's own column, which is a real link"
    );
    assert!(
        r.affected_modules.is_empty(),
        "no `covers` edge exists, so no module may be named: {:?}",
        r.affected_modules
    );
    db.close().await;
}

/// D3: readiness must not depend on either `failure_severity` implementation.
#[tokio::test]
async fn p9_contract_readiness_does_not_depend_on_either_severity_implementation() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 D3").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;
    execute(
        store,
        &pid_s,
        case,
        rel,
        1,
        TaskStatus::Failed,
        Some("ASSERTION_FAILURE: x"),
    )
    .await;

    let r = assess(store, &pid_s, rel).await;

    // The readiness *decision* must not read a severity value.
    //
    // Note the subtlety this test initially got wrong: the serialized result DOES contain the
    // string `failure_severity`, because `risk.release_model` reports **which factors the
    // existing model measured**. Naming a factor is not consuming its value -- it is reporting
    // that the model is partly unmeasured, which is the honest disclosure the brief wants. The
    // property that actually matters is that no *gate* depends on one.
    for b in &r.blocked_criteria {
        assert!(
            !b.evidence.contains("failure_severity") && !b.why.contains("failure_severity"),
            "no blocking criterion may rest on a severity value: {}",
            b.evidence
        );
    }
    let b = r
        .blocked_criteria
        .iter()
        .find(|b| b.rule_id == "R6")
        .expect("R6 must be the blocker");
    assert!(
        b.evidence.contains("FAILED/TIMEOUT of"),
        "the blocker counts outcomes, not severities: {}",
        b.evidence
    );

    // And the factor list is reported with its measured/unmeasured split intact, so a reader sees
    // that the model is partial rather than a silently renormalised number.
    let model = signal(&r, "risk.release_model").expect("model signal");
    assert!(
        model.summary.contains("Supporting evidence only"),
        "rule 6: a risk estimate is supporting evidence, never the decision: {}",
        model.summary
    );
    assert!(
        model.evidence.contains("unmeasured=") || model.evidence.contains("measured="),
        "and the measured/unmeasured split must be visible: {}",
        model.evidence
    );
    db.close().await;
}

/// The roadmap's signals this schema cannot release-scope must be listed, not invented.
#[tokio::test]
async fn p9_contract_unavailable_roadmap_signals_are_declared_not_invented() {
    let db = Db::new().await;
    let store = &db.store;
    let pid = project(store, "P9 unavailable").await;
    let pid_s = pid.to_string();
    let rel = release(store, &pid_s, "v1").await;
    let case = seed_case(store, pid).await;
    execute(store, &pid_s, case, rel, 1, TaskStatus::Passed, None).await;

    let r = assess(store, &pid_s, rel).await;

    for kind in [
        "defects.critical_high",
        "failure.new_clusters",
        "change.impact_scope",
        "exploration.findings",
        "journey.outcomes",
    ] {
        let m = missing(&r, kind).unwrap_or_else(|| {
            panic!(
                "`{kind}` is a roadmap signal and must be declared, not \
                     silently dropped. Present: {:?}",
                r.missing_signals
                    .iter()
                    .map(|m| &m.kind)
                    .collect::<Vec<_>>()
            )
        });
        assert_eq!(
            m.reason, "NO_SOURCE_IN_SCHEMA",
            "`{kind}` has no release-scoped source in this schema"
        );
        assert!(!m.detail.is_empty(), "and must say why: {}", m.detail);
    }
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
        .join("aiqa_p92b_readiness_artifacts")
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
