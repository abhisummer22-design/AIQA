//! Phase 2 (End-to-End QA Traceability) — the full chain test suite.
//!
//! ## The rule this module exists to enforce
//!
//! Phase 2 found that the middle of the chain — `Failure → Bug → Fix → Retest → Release`
//! — was **structurally impossible** to traverse, because two "failure fingerprints" in
//! the codebase hashed different things. Phase 1's tests had passed anyway, and they
//! passed *because of the bug*: they inserted a healing proposal carrying a
//! hand-written fingerprint equal to the telemetry row's, which is a state the real
//! system can never produce.
//!
//! So the rule is: **every row in these tests is produced by a real production writer.**
//! `Intelligence::ingest_run` writes the telemetry and the cluster and the bug draft.
//! `GovernedHealingService::propose_healing` writes the proposal. Nothing here
//! fabricates a fingerprint to make a join succeed — that is the exact failure the phase
//! is fixing, and a test that does it would hide the regression it is meant to catch.
//!
//! ## What each test proves
//!
//! | Area | Tests |
//! |---|---|
//! | Fingerprint unification | the three producers agree; the retired scheme provably disagreed |
//! | Cluster ↔ telemetry | agree for a **selector-bearing** failure (the case that was broken) |
//! | Bug → Fix | a real proposal joins a real bug through the shared fingerprint |
//! | Fix → Retest | the verification is deserialized structurally, and a failed retest is reported |
//! | Retest → Release | the retest's own run resolves to a real release |
//! | Missing links | each absent hop is named, never filled in |
//! | Isolation | another tenant's cluster, bug, proposal and release never appear |
//! | Idempotence | re-ingesting does not duplicate edges; re-proposing is still idempotent |
//! | Migration | a fresh database has every table and column the chain needs |

use crate::adaptive_intelligence::{canonical_failure_fingerprint, FailureCategory};
use crate::auth::{AuthContext, Role};
use crate::domain::CreateTestCaseRequest;
use crate::domain::{
    RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskState, TaskStatus, TestStep,
};
use crate::evidence::LocalEvidenceStore;
use crate::execution_plan::Action;
use crate::executor::ExecutorRegistry;
use crate::governed_healing::*;
use crate::intelligence::Intelligence;
use crate::policy::PolicyConfig;
use crate::store::Store;
use crate::trace::TraceReader;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

// ------------------------------------------------------------------ harness

fn make_auth_ctx(role: Role, project_id: Option<&str>) -> AuthContext {
    AuthContext {
        key_id: Uuid::new_v4(),
        name: format!("user-{}", role.as_str()),
        role,
        project_id: project_id.map(ToString::to_string),
    }
}

/// The real governed-healing stack, not a stand-in: `propose_healing` is a production
/// writer and this is the only way to exercise it honestly.
struct Harness {
    store: Store,
    healing: GovernedHealingService,
    _dir: tempfile::TempDir,
}

async fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase2-chain.sqlite3"))
        .await
        .expect("connect");

    let evidence =
        Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store"));
    let registry = Arc::new(
        ExecutorRegistry::new(Arc::new(crate::executor::SimulatedExecutor))
            .register("simulated", Arc::new(crate::executor::SimulatedExecutor)),
    );
    let mut policy = PolicyConfig::from_env();
    policy.require_approval_for = HashSet::new();
    let policy_arc = Arc::new(policy.clone());

    let dispatcher = crate::agent_execution::AgentExecutionDispatcher::new(
        store.clone(),
        policy,
        registry,
        evidence.clone(),
        None,
    );
    let healing =
        GovernedHealingService::new(store.clone(), dispatcher, evidence, policy_arc, None);
    Harness {
        store,
        healing,
        _dir: dir,
    }
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 2 chain fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

fn case_req(title: &str, requirement_reference: Option<&str>) -> CreateTestCaseRequest {
    CreateTestCaseRequest {
        title: title.to_string(),
        description: "Controlled data for the Phase 2 end-to-end chain.".into(),
        category: "functional".into(),
        priority: "P0".into(),
        severity: "high".into(),
        preconditions: Vec::new(),
        steps: vec![TestStep {
            step_number: 1,
            action: "click login".into(),
            expected_result: "the dashboard loads".into(),
        }],
        expected_result: Some("the dashboard loads".into()),
        assertions: vec!["dashboard heading is visible".into()],
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

/// The single defect this suite tracks. Fixed strings, so both ingests and the healing
/// proposal describe the *same* failure.
const SELECTOR: &str = "#login-btn";
const REASON: &str = "Element not found: #login-btn";
const RUN_TASK: &str = "task-login";

/// Persist a run row, then drive the **real** ingest path for it.
///
/// `ingest_run` is what writes `execution_telemetry`, upserts `failure_clusters`, updates
/// the quality graph and — on the second identical occurrence — drafts a bug. Calling it
/// is the only honest way to produce the middle of the chain.
async fn ingest_failing_run(store: &Store, project_id: &str, case_id: Uuid, attempt: u32) -> Uuid {
    let run_id = Uuid::new_v4();
    let plan = RunPlan {
        name: format!("chain run {attempt}"),
        tasks: vec![TaskDef {
            id: RUN_TASK.into(),
            name: "Test Case: login with a saved card".into(),
            prerequisites: Vec::new(),
            simulate: Some(SimSpec {
                outcome: SimOutcome::Fail,
                delay_ms: 1,
            }),
            executor: Some("simulated".into()),
            // The selector is the field Phase 2 found being dropped from one of the two
            // fingerprint computations. It is set here so the test would catch a repeat.
            params: serde_json::json!({ "selector": SELECTOR }),
            test_case_id: Some(case_id.to_string()),
        }],
        concurrency: 1,
    };
    store
        .insert_run_for_release(
            run_id,
            &plan,
            Some(project_id),
            Some("phase 2 chain fixture"),
            RunState::Completed,
            None,
            None,
        )
        .await
        .expect("insert run");

    let states = vec![TaskState {
        id: RUN_TASK.into(),
        name: "Test Case: login with a saved card".into(),
        prerequisites: Vec::new(),
        status: TaskStatus::Failed,
        reason: Some(REASON.into()),
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

fn proposal_request(project_id: &str, run_id: Uuid) -> HealingProposalRequest {
    HealingProposalRequest {
        project_id: project_id.to_string(),
        run_id,
        task_id: RUN_TASK.into(),
        task_name: "Test Case: login with a saved card".into(),
        executor: "simulated".into(),
        environment: Some("test".into()),
        target_url: Some("http://127.0.0.1:8080/login".into()),
        // Same selector and reason as the failing execution, because it is the same
        // failure. Nothing else would make the proposal join the cluster.
        failed_selector: Some(SELECTOR.into()),
        failure_reason: REASON.into(),
        original_actions: vec![Action::Navigate {
            url: "http://127.0.0.1:8080/login".into(),
        }],
        proposed_actions: Some(vec![Action::Navigate {
            url: "http://127.0.0.1:8080/login".into(),
        }]),
        diagnosis_category: Some("SelectorRenamedOrRelocated".into()),
        confidence: Some(0.9),
        rationale: Some("observed selector was relocated".into()),
        evidence_refs: vec![],
        timeout_secs: Some(30),
        requirement_reference: Some("REQ-CHAIN-1".into()),
        custom_params: None,
        auto_approve_safe: false,
        crawl_task_id: None,
        crawl_output: None,
    }
}

/// Drive the case, both failing runs, and a real proposal. Returns
/// `(case_id, run_id, proposal_id)`.
async fn build_chain_to_fix(h: &Harness, project_id: &str) -> (Uuid, Uuid, Uuid) {
    let intel = Intelligence::new(&h.store);
    let case_id = h
        .store
        .insert_test_case(
            project_id,
            &case_req("Login with a saved card", Some("REQ-CHAIN-1")),
            None,
        )
        .await
        .expect("insert case");
    intel
        .link_test_case_to_requirement(
            project_id,
            case_id,
            "Login with a saved card",
            Some("REQ-CHAIN-1"),
            None,
        )
        .await
        .expect("validates");

    // Twice, so the failure is *recurring* — which is the precondition the real bug-draft
    // writer has (`occurrence_count >= 2`).
    let first = ingest_failing_run(&h.store, project_id, case_id, 1).await;
    ingest_failing_run(&h.store, project_id, case_id, 2).await;

    let ctx = make_auth_ctx(Role::Operator, Some(project_id));
    let proposal = h
        .healing
        .propose_healing(&ctx, proposal_request(project_id, first))
        .await
        .expect("propose_healing");

    (case_id, first, proposal.id)
}

/// A serialized `HealingVerification` — the exact shape
/// `verify_healing` persists — so the reader is exercised against real data rather than
/// a convenient summary string.
fn verification_json(
    proposal_id: Uuid,
    original_run_id: Uuid,
    healing_run_id: Uuid,
    outcome: VerificationOutcome,
    revised_passed: bool,
    resolved: bool,
) -> String {
    serde_json::to_string(&HealingVerification {
        proposal_id,
        original_run_id,
        original_task_id: RUN_TASK.into(),
        healing_run_id,
        healing_task_id: format!("heal-{RUN_TASK}"),
        outcome,
        original_failure_resolved: resolved,
        revised_task_passed: revised_passed,
        new_failures_introduced: 0,
        regression_detected: false,
        evidence_refs: vec!["evidence/retest.log".into()],
        message: "controlled retest".into(),
        verified_at: chrono::Utc::now(),
    })
    .expect("serialize verification")
}

/// Create a real run and attach it to a real release, returning the release id.
///
/// `insert_run_for_release` refuses a release the project does not own, so this also
/// exercises that guard.
async fn release_for_run(store: &Store, project_id: &str, run_id: Uuid) -> Uuid {
    let release = store
        .create_release(
            project_id,
            Some("2026.02 - login hardening"),
            Some("2.1.0"),
            Some("Phase 2 chain fixture"),
            Some("phase2"),
        )
        .await
        .expect("create release");

    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "retest run".into(),
                tasks: vec![TaskDef {
                    id: format!("heal-{RUN_TASK}"),
                    name: "Healed: Test Case: login with a saved card".into(),
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
            Some(project_id),
            Some("phase 2 chain fixture"),
            RunState::Completed,
            None,
            Some(&release.id.to_string()),
        )
        .await
        .expect("attach run to release");

    release.id
}

// ------------------------------------------------------------------ fingerprint unification

/// The retired scheme is provably incompatible with the live one.
///
/// This is the regression's root cause stated as a test. `compute_failure_fingerprint`
/// hashed `run_id`, so even for the identical selector and reason it produces a value the
/// telemetry fingerprint can never equal — and it changes when only the run id changes,
/// which is why the same defect could not be grouped across runs.
#[test]
fn the_retired_fingerprint_scheme_could_never_join_the_telemetry_one() {
    let run_a = Uuid::new_v4();
    let run_b = Uuid::new_v4();

    let retired_a = compute_failure_fingerprint(&run_a, RUN_TASK, Some(SELECTOR), REASON);
    let retired_b = compute_failure_fingerprint(&run_b, RUN_TASK, Some(SELECTOR), REASON);
    assert_ne!(
        retired_a, retired_b,
        "the retired scheme was unique per run, so it could not group a recurring defect"
    );

    let (_, category, canonical) =
        canonical_failure_fingerprint(REASON, Some(SELECTOR), None, None);
    assert_ne!(
        canonical, retired_a,
        "the retired scheme never produced the value the chain joins on"
    );
    assert_eq!(category, categorize(REASON));
}

/// The live scheme is stable across runs and sensitive to the failure itself — the two
/// properties a join key needs and the retired one lacked.
#[test]
fn the_canonical_fingerprint_groups_a_defect_across_runs() {
    let (_, _, first) = canonical_failure_fingerprint(REASON, Some(SELECTOR), None, None);
    let (_, _, again) = canonical_failure_fingerprint(REASON, Some(SELECTOR), None, None);
    assert_eq!(
        first, again,
        "the same defect must fingerprint identically every time"
    );

    // Different selector, different failure — so the key is not merely constant.
    let (_, _, other_selector) =
        canonical_failure_fingerprint(REASON, Some("#signup-btn"), None, None);
    assert_ne!(
        first, other_selector,
        "a different failing selector is a different defect"
    );

    // Dynamic values are masked, so two runs whose ids differ still agree. This is the
    // property that makes dedup work at all.
    let (_, _, with_uuid) = canonical_failure_fingerprint(
        "request 550e8400-e29b-41d4-a716-446655440000 failed",
        Some(SELECTOR),
        None,
        None,
    );
    let (_, _, with_other_uuid) = canonical_failure_fingerprint(
        "request 6ba7b810-9dad-11d1-80b4-00c04fd430c8 failed",
        Some(SELECTOR),
        None,
        None,
    );
    assert_eq!(
        with_uuid, with_other_uuid,
        "request ids must be masked out, or nothing would ever dedup"
    );
}

fn categorize(reason: &str) -> FailureCategory {
    let (_, category, _) = canonical_failure_fingerprint(reason, None, None, None);
    category
}

// ------------------------------------------------------------------ cluster <-> telemetry

/// The defect Phase 2 found: the cluster's fingerprint omitted the selector that the
/// telemetry row's included, so for an ordinary Playwright failure the two could not be
/// joined and every defect reported "this touched no tests".
#[tokio::test]
async fn a_cluster_and_its_telemetry_agree_even_with_a_failing_selector() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Cluster Telemetry").await;
    let case_id = Uuid::new_v4();

    ingest_failing_run(&h.store, &pid, case_id, 1).await;
    ingest_failing_run(&h.store, &pid, case_id, 2).await;

    let telemetry_fp: String = sqlx::query_scalar(
        "SELECT failure_fingerprint FROM execution_telemetry \
         WHERE project_id = ? AND test_case_id = ? AND failure_fingerprint IS NOT NULL LIMIT 1",
    )
    .bind(&pid)
    .bind(case_id.to_string())
    .fetch_one(&h.store.pool)
    .await
    .expect("telemetry fingerprint");

    let cluster_fp: Option<String> =
        sqlx::query_scalar("SELECT failure_fingerprint FROM failure_clusters WHERE project_id = ?")
            .bind(&pid)
            .fetch_optional(&h.store.pool)
            .await
            .expect("cluster fingerprint")
            .expect("the recurring failure must have produced a cluster");

    assert_eq!(
        cluster_fp.as_deref(),
        Some(telemetry_fp.as_str()),
        "the cluster must describe the executions that produced it; a selector-bearing \
         failure previously produced two different fingerprints"
    );
    h.store.pool.close().await;
}

// ------------------------------------------------------------------ bug -> fix

/// The headline join of Phase 2, driven entirely by production writers.
///
/// Nothing in this test writes a fingerprint. The cluster's comes from `ingest_run`, the
/// proposal's from `propose_healing`, and the test's only claim is that they are equal —
/// which was false before this phase and is the reason `Bug -> Fix` could never be walked.
#[tokio::test]
async fn a_real_healing_proposal_joins_a_real_bug_through_the_cluster() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Bug To Fix").await;
    let (case_id, _, proposal_id) = build_chain_to_fix(&h, &pid).await;

    // Preconditions the join depends on, asserted rather than assumed.
    let bug_id: String = sqlx::query_scalar(
        "SELECT id FROM bug_drafts WHERE failure_cluster_id IN \
           (SELECT id FROM failure_clusters WHERE project_id = ?) LIMIT 1",
    )
    .bind(&pid)
    .fetch_optional(&h.store.pool)
    .await
    .expect("bug lookup")
    .expect("two identical failures must have drafted a bug");

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;

    let fixes = trace.step("fix").expect("fix hop must be present");
    assert_eq!(fixes.len(), 1, "the proposal must be reachable: {fixes:?}");
    assert_eq!(
        fixes[0].id.as_deref(),
        Some(proposal_id.to_string().as_str())
    );

    // And the fix must name the bug it addresses, so `Bug -> Fix` is a real step rather
    // than two independently-correct lists sitting next to each other.
    let addresses = fixes[0]
        .payload
        .as_ref()
        .and_then(|p| p.get("addresses_bugs"));
    let addresses = addresses
        .and_then(|v| v.as_array())
        .expect("addresses_bugs array");
    assert_eq!(
        addresses
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec![bug_id.as_str()],
        "the fix must name the bug draft it addresses"
    );

    // The proposal's own fingerprint must equal the cluster's.
    let proposal_fp: String =
        sqlx::query_scalar("SELECT failure_fingerprint FROM healing_proposals WHERE id = ?")
            .bind(proposal_id.to_string())
            .fetch_one(&h.store.pool)
            .await
            .expect("proposal fingerprint");
    let cluster_fp: String =
        sqlx::query_scalar("SELECT failure_fingerprint FROM failure_clusters WHERE project_id = ?")
            .bind(&pid)
            .fetch_one(&h.store.pool)
            .await
            .expect("cluster fingerprint");
    assert_eq!(
        proposal_fp, cluster_fp,
        "Bug -> Fix joins on this value; if it differs the chain is broken again"
    );
    h.store.pool.close().await;
}

// ------------------------------------------------------------------ the whole chain

/// End-to-end: Requirement → Test → Execution → Failure → Bug → Fix → Retest → Release,
/// every hop produced by a real writer and every hop asserted against the database rather
/// than against the walk's own opinion of itself.
#[tokio::test]
async fn the_whole_chain_is_persisted_and_retrievable_end_to_end() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Full").await;
    let (case_id, original_run, proposal_id) = build_chain_to_fix(&h, &pid).await;

    // A real retest run, attached to a real release, plus the verification that
    // `verify_healing` persists.
    let healing_run = Uuid::new_v4();
    let release_id = release_for_run(&h.store, &pid, healing_run).await;
    h.store
        .update_healing_proposal_state(
            proposal_id,
            "VERIFIED",
            Some("operator"),
            Some(chrono::Utc::now()),
            None,
            Some(healing_run),
            Some(&verification_json(
                proposal_id,
                original_run,
                healing_run,
                VerificationOutcome::Verified,
                true,
                true,
            )),
        )
        .await
        .expect("persist verification");

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;

    // ---- 1. Requirement ----
    let reqs = trace.step("requirement").expect("requirement hop");
    assert!(
        reqs.iter()
            .any(|s| s.label.as_deref() == Some("REQ-CHAIN-1")),
        "requirement: {reqs:?}"
    );

    // ---- 2. Test ----
    assert_eq!(
        trace.step("test").expect("test hop")[0].id.as_deref(),
        Some(case_id.to_string().as_str())
    );

    // ---- 3. Execution (both failing runs) ----
    let execs = trace.step("execution").expect("execution hop");
    assert_eq!(execs.len(), 2, "both executions of this case must appear");

    // ---- 4. Failure ----
    let failures = trace.step("failure").expect("failure hop");
    assert_eq!(failures.len(), 1, "one distinct fingerprint, one hop");

    // ---- 5. Bug ----
    let bugs = trace.step("bug").expect("bug hop");
    assert_eq!(bugs.len(), 1);

    // ---- 6. Fix ----
    let fixes = trace.step("fix").expect("fix hop");
    assert_eq!(fixes.len(), 1);

    // ---- 7. Retest ----
    let retests = trace.step("retest").expect("retest hop");
    assert_eq!(retests.len(), 1);
    assert_eq!(
        retests[0].id.as_deref(),
        Some(healing_run.to_string().as_str()),
        "the retest hop must name the real healing run"
    );
    assert_eq!(
        retests[0]
            .payload
            .as_ref()
            .and_then(|p| p.get("passed"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "a Verified outcome with a passing revised task and a resolved failure is a pass"
    );

    // ---- 8. Release ----
    let releases = trace.step("release").expect("release hop");
    assert!(
        releases
            .iter()
            .any(|r| r.id.as_deref() == Some(release_id.to_string().as_str())),
        "the retest run's release must be reachable: {releases:?}"
    );

    // `module` is the one hop deliberately absent — Phase 1 established there is no
    // module identity in the domain model, and Phase 2 must not have invented one.
    assert!(
        trace.step("module").is_none(),
        "module must stay unreported: no evidence exists for it"
    );
    assert!(
        trace.incomplete.iter().any(|m| m.starts_with("module")),
        "module must be named as missing: {:?}",
        trace.incomplete
    );
    h.store.pool.close().await;
}

// ------------------------------------------------------------------ retest honesty

/// A fix that was never executed has no retest. The hop must be named, not inferred.
#[tokio::test]
async fn a_fix_that_was_never_verified_has_no_retest() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Unverified").await;
    let (case_id, _, _) = build_chain_to_fix(&h, &pid).await;

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    assert!(trace.step("fix").is_some(), "the proposal itself is real");
    assert!(
        trace.step("retest").is_none(),
        "an unexecuted fix has no retest and none may be invented"
    );
    assert!(
        trace
            .incomplete
            .iter()
            .any(|m| m.starts_with("retest") && m.contains("never executed")),
        "the reason must be stated: {:?}",
        trace.incomplete
    );
    h.store.pool.close().await;
}

/// A retest that **failed** is real evidence and must be reported, not hidden.
///
/// This is the test that would have passed under Phase 1's substring check in the wrong
/// direction: `"not verified"` does not contain "pass", so a failed verification would
/// have been silently dropped and the chain would have looked unresolved-but-clean.
#[tokio::test]
async fn a_failed_retest_is_reported_rather_than_hidden() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Failed Retest").await;
    let (case_id, original_run, proposal_id) = build_chain_to_fix(&h, &pid).await;

    let healing_run = Uuid::new_v4();
    h.store
        .update_healing_proposal_state(
            proposal_id,
            "APPROVED",
            Some("operator"),
            Some(chrono::Utc::now()),
            None,
            Some(healing_run),
            Some(&verification_json(
                proposal_id,
                original_run,
                healing_run,
                VerificationOutcome::NotVerified,
                false,
                false,
            )),
        )
        .await
        .expect("persist verification");

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    let retests = trace
        .step("retest")
        .expect("a failed retest is still a retest");
    assert_eq!(retests.len(), 1);
    assert_eq!(
        retests[0]
            .payload
            .as_ref()
            .and_then(|p| p.get("passed"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "the hop must state that the retest did not pass"
    );
    assert!(
        retests[0]
            .label
            .as_deref()
            .is_some_and(|l| l.contains("NOT_VERIFIED")),
        "the outcome must be visible on the hop: {:?}",
        retests[0].label
    );
    h.store.pool.close().await;
}

/// A regression introduced by a fix is the most important negative result in the chain.
#[tokio::test]
async fn a_regression_introduced_by_a_fix_is_surfaced_as_not_passed() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Regression").await;
    let (case_id, original_run, proposal_id) = build_chain_to_fix(&h, &pid).await;

    let healing_run = Uuid::new_v4();
    h.store
        .update_healing_proposal_state(
            proposal_id,
            "APPROVED",
            Some("operator"),
            Some(chrono::Utc::now()),
            None,
            Some(healing_run),
            Some(&verification_json(
                proposal_id,
                original_run,
                healing_run,
                VerificationOutcome::RegressionIntroduced,
                true,
                false,
            )),
        )
        .await
        .expect("persist verification");

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    let retests = trace.step("retest").expect("regression hop");
    assert_eq!(
        retests[0]
            .payload
            .as_ref()
            .and_then(|p| p.get("passed"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "a passed revised task does not make the retest a pass when the original \
         failure was not resolved and a regression was introduced"
    );
    h.store.pool.close().await;
}

/// An unreadable verification must be surfaced, not treated as "no retest happened".
#[tokio::test]
async fn a_corrupt_verification_record_is_reported_not_dropped() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Corrupt").await;
    let (case_id, _, proposal_id) = build_chain_to_fix(&h, &pid).await;

    h.store
        .update_healing_proposal_state(
            proposal_id,
            "APPROVED",
            Some("operator"),
            Some(chrono::Utc::now()),
            None,
            None,
            Some("{ this is not valid json"),
        )
        .await
        .expect("persist verification");

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    let retests = trace
        .step("retest")
        .expect("a corrupt record must still produce a hop");
    assert_eq!(retests.len(), 1);
    assert!(
        !retests[0].evidence.missing.is_empty(),
        "the corruption must be stated on the hop: {:?}",
        retests[0].evidence
    );
    assert_eq!(
        retests[0]
            .payload
            .as_ref()
            .and_then(|p| p.get("passed"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "an unreadable verification is not a pass"
    );
    h.store.pool.close().await;
}

// ------------------------------------------------------------------ missing links

/// A fix with no bug behind it is still a fix — a single occurrence can be healed before
/// it recurs enough to draft. It must appear, with the gap noted.
#[tokio::test]
async fn a_fix_without_a_bug_draft_is_still_reported_with_the_gap_named() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Fix No Bug").await;
    let intel = Intelligence::new(&h.store);

    let case_id = h
        .store
        .insert_test_case(&pid, &case_req("Single failure", Some("REQ-ONCE")), None)
        .await
        .expect("insert case");
    intel
        .link_test_case_to_requirement(&pid, case_id, "Single failure", Some("REQ-ONCE"), None)
        .await
        .expect("validates");

    // Exactly one occurrence, so no bug draft is drafted (the real threshold is 2).
    let run_id = ingest_failing_run(&h.store, &pid, case_id, 1).await;

    let ctx = make_auth_ctx(Role::Operator, Some(&pid));
    h.healing
        .propose_healing(&ctx, proposal_request(&pid, run_id))
        .await
        .expect("propose_healing");

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;

    // An absent hop is absent from `steps` entirely — the walk inserts a hop only when it
    // has at least one item, and otherwise names it in `incomplete`. Asserting
    // `steps["bug"]` exists-and-is-empty would be asserting a shape the reader
    // deliberately does not produce.
    assert!(
        trace.step("bug").is_none(),
        "one occurrence must not draft a bug"
    );
    assert!(
        trace.incomplete.iter().any(|m| m == "bug"),
        "the absent bug must be named: {:?}",
        trace.incomplete
    );

    let fixes = trace.step("fix").expect("fix is real");
    assert_eq!(fixes.len(), 1, "the proposal exists and must be reported");
    assert!(
        !fixes[0].evidence.missing.is_empty(),
        "the absent bug must be named on the fix hop"
    );
    h.store.pool.close().await;
}

// ------------------------------------------------------------------ isolation

/// One tenant's cluster, bug, proposal and release must not leak into another's walk.
#[tokio::test]
async fn the_chain_does_not_cross_projects() {
    let h = harness().await;
    let mine = project(&h.store, "Chain Tenant A").await;
    let theirs = project(&h.store, "Chain Tenant B").await;

    let (their_case, their_original, their_proposal) = build_chain_to_fix(&h, &theirs).await;
    let their_healing_run = Uuid::new_v4();
    let their_release = release_for_run(&h.store, &theirs, their_healing_run).await;
    h.store
        .update_healing_proposal_state(
            their_proposal,
            "VERIFIED",
            Some("operator"),
            Some(chrono::Utc::now()),
            None,
            Some(their_healing_run),
            Some(&verification_json(
                their_proposal,
                their_original,
                their_healing_run,
                VerificationOutcome::Verified,
                true,
                true,
            )),
        )
        .await
        .expect("persist verification");

    // Walk *their* case from *our* project. Nothing may resolve.
    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&mine, &their_case)
        .await;
    for hop in [
        "requirement",
        "execution",
        "failure",
        "bug",
        "fix",
        "retest",
        "release",
    ] {
        assert!(
            trace.step(hop).is_none(),
            "`{hop}` leaked another tenant's data: {:?}",
            trace.step(hop)
        );
    }
    // And the release id we just created must not be reachable from our project.
    let index = TraceReader::new(h.store.clone()).index(&mine).await;
    assert!(
        !index
            .test_cases
            .iter()
            .any(|e| e.test_case_id.as_deref() == Some(their_case.to_string().as_str())),
        "another tenant's case must not appear in our index"
    );
    let _ = their_release;
    h.store.pool.close().await;
}

/// A proposal from another project must never attach to our failure, even when both
/// projects hit the identical defect.
///
/// ## Why this test asserts what it asserts
///
/// The first draft of this test asserted that our fix hop's fingerprint differs from
/// theirs. It failed, and it was **wrong to assert**: a failure fingerprint identifies a
/// failure *pattern* — category, masked message, selector — and deliberately carries no
/// tenant, no run and no task. Two projects that hit the same defect must produce the
/// same value, because that is what makes the pattern comparable at all.
///
/// Isolation therefore cannot come from the fingerprint; it comes from every read being
/// project-scoped (`list_healing_proposals` is `WHERE project_id = ?`). So that is what
/// is asserted here: our hop names **our** proposal, and theirs never appears. Which is
/// the property that actually protects the boundary.
#[tokio::test]
async fn a_proposal_from_another_project_never_joins_our_failure() {
    let h = harness().await;
    let mine = project(&h.store, "Chain Iso A").await;
    let theirs = project(&h.store, "Chain Iso B").await;

    let (_, _, their_proposal) = build_chain_to_fix(&h, &theirs).await;
    let (my_case, _, my_proposal) = build_chain_to_fix(&h, &mine).await;

    // Same defect, so the same fingerprint value - asserted, because if this ever stops
    // being true the fingerprints have acquired a tenant and isolation has been faked.
    let their_fp: String = sqlx::query_scalar(
        "SELECT failure_fingerprint FROM healing_proposals \
         WHERE project_id = ? AND failure_fingerprint IS NOT NULL LIMIT 1",
    )
    .bind(&theirs)
    .fetch_one(&h.store.pool)
    .await
    .expect("their proposal fingerprint");
    let my_fp: String = sqlx::query_scalar(
        "SELECT failure_fingerprint FROM healing_proposals \
         WHERE project_id = ? AND failure_fingerprint IS NOT NULL LIMIT 1",
    )
    .bind(&mine)
    .fetch_one(&h.store.pool)
    .await
    .expect("my proposal fingerprint");
    assert_eq!(
        my_fp, their_fp,
        "a failure fingerprint identifies a failure pattern, not a tenant; \
         isolation must come from the project-scoped reads, never from the value"
    );

    // Our walk must resolve to our proposal only.
    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&mine, &my_case)
        .await;
    let fixes = trace.step("fix").expect("our own fix");
    assert_eq!(fixes.len(), 1, "exactly our proposal: {fixes:?}");
    assert_eq!(
        fixes[0].id.as_deref(),
        Some(my_proposal.to_string().as_str()),
        "our walk must name our proposal"
    );
    assert_ne!(
        fixes[0].id.as_deref(),
        Some(their_proposal.to_string().as_str()),
        "another tenant's proposal attached to our failure"
    );
    h.store.pool.close().await;
}

// ------------------------------------------------------------------ idempotence

/// Re-ingesting the same failure must not duplicate edges, and re-proposing must still be
/// idempotent. Both are properties the chain depends on: a duplicated hop set would read
/// as "this happened twice" when it happened once.
#[tokio::test]
async fn repeating_the_chain_does_not_duplicate_relations() {
    let h = harness().await;
    let pid = project(&h.store, "Chain Idempotent").await;
    let (case_id, run_id, proposal_id) = build_chain_to_fix(&h, &pid).await;

    // Re-propose for the same run+task: the idempotency key must return the same proposal.
    let ctx = make_auth_ctx(Role::Operator, Some(&pid));
    let again = h
        .healing
        .propose_healing(&ctx, proposal_request(&pid, run_id))
        .await
        .expect("second propose");
    assert_eq!(
        again.id, proposal_id,
        "the same failure in the same run must not raise a second proposal"
    );

    let proposal_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM healing_proposals WHERE project_id = ?")
            .bind(&pid)
            .fetch_one(&h.store.pool)
            .await
            .expect("proposal count");
    assert_eq!(proposal_count, 1);

    // Re-ingesting the same failing run must not duplicate the graph edges.
    ingest_failing_run(&h.store, &pid, case_id, 3).await;

    let validates: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quality_relations rel \
         JOIN quality_nodes f ON f.id = rel.from_node_id \
         WHERE rel.relation_type = 'validates' AND f.project_id = ?",
    )
    .bind(&pid)
    .fetch_one(&h.store.pool)
    .await
    .expect("validates count");
    assert_eq!(
        validates, 1,
        "the validates edge must still be a single row"
    );

    let trace = TraceReader::new(h.store.clone())
        .walk_test_case(&pid, &case_id)
        .await;
    assert_eq!(
        trace.step("fix").map(|v| v.len()),
        Some(1),
        "one failure, one fix hop"
    );
    h.store.pool.close().await;
}

// ------------------------------------------------------------------ migration

/// A fresh database must contain everything the chain traverses, with no Phase 2 DDL.
///
/// Phase 2 changed no schema: it removed a divergence between two computations. This test
/// pins both halves of that claim.
#[tokio::test]
async fn a_fresh_migration_supplies_every_table_the_chain_traverses() {
    let (store, _d) = crate::trace_tests::temp_store_for_phase2().await;

    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&store.pool)
        .await
        .expect("migration count");
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
        "Phase 2 added no migration; if this moved, the report must be updated"
    );

    for table in [
        "test_cases",
        "execution_telemetry",
        "failure_clusters",
        "bug_drafts",
        "healing_proposals",
        "runs",
        "qa_releases",
        "quality_nodes",
        "quality_relations",
    ] {
        let present: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name IS NOT NULL")
                .bind(table)
                .fetch_one(&store.pool)
                .await
                .unwrap_or_default();
        assert!(present > 0, "{table} is required by the chain");
    }

    // The three columns the joins ride on, each against the table that carries it.
    for (table, column) in [
        ("execution_telemetry", "test_case_id"),
        ("failure_clusters", "failure_fingerprint"),
        ("healing_proposals", "failure_fingerprint"),
        ("healing_proposals", "verification_result"),
        ("runs", "release_id"),
    ] {
        let present: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = ?")
                .bind(table)
                .bind(column)
                .fetch_one(&store.pool)
                .await
                .expect("column check");
        assert!(present > 0, "{table}.{column} is required by the chain");
    }
    store.pool.close().await;
}
