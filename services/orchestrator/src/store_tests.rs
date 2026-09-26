//! Phase 1 regression tests for `store.rs`.
//!
//! Every test here was written to FAIL against the pre-Phase-1 code and to
//! pass only once the corresponding defect is fixed. Each names the blocker
//! it pins (B1/B3/B6/B14) so a future regression is immediately traceable to
//! the audit finding it reopens.
//!
//! Kept in its own file rather than inline in `store.rs` purely for size:
//! `store.rs` is already the largest module in the crate.

use crate::domain::{
    CreateTestCaseRequest, CreateTestPlanRequest, CreateTestSuiteRequest, RunPlan, RunState,
    SimOutcome, SimSpec, TaskDef, TaskStatus, TestStep,
};
use crate::store::Store;
use uuid::Uuid;

/// A real on-disk SQLite store in a fresh temp dir. The defects under test
/// are properties of the file-backed store (journal mode, busy timeout,
/// transactions), so an in-memory database would not exercise them.
async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.sqlite3");
    let store = Store::connect(&path).await.expect("connect");
    (store, dir)
}

fn task(id: &str, name: &str, prereqs: &[&str]) -> TaskDef {
    TaskDef {
        id: id.into(),
        name: name.into(),
        prerequisites: prereqs.iter().map(|s| s.to_string()).collect(),
        simulate: Some(SimSpec {
            outcome: SimOutcome::Pass,
            delay_ms: 0,
        }),
        ..Default::default()
    }
}

fn plan(name: &str, tasks: Vec<TaskDef>) -> RunPlan {
    RunPlan {
        name: name.into(),
        tasks,
        concurrency: 4,
    }
}

// -------------------------------------------------------------------- B1

/// B1: the store must run in WAL mode with a non-zero busy timeout.
/// Pre-fix this returned journal_mode="delete" and busy_timeout=0, the root
/// cause of the `database is locked` failures that silently corrupted
/// durable run state.
#[tokio::test]
async fn b1_wal_and_busy_timeout_are_configured() {
    let (store, _dir) = temp_store().await;

    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&store.pool)
        .await
        .expect("journal_mode");
    assert_eq!(
        journal.to_lowercase(),
        "wal",
        "store must use WAL so concurrent readers/writers do not block each other"
    );

    let busy: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&store.pool)
        .await
        .expect("busy_timeout");
    assert!(
        busy >= 1000,
        "busy_timeout must be set so a blocked writer waits instead of failing immediately (got {busy}ms)"
    );
}

/// B1: concurrent writers must not silently lose writes. Pre-fix a contended
/// write returned SQLITE_BUSY, the caller logged it and continued, and the
/// row simply never landed.
#[tokio::test]
async fn b1_concurrent_writes_do_not_silently_disappear() {
    let (store, _dir) = temp_store().await;
    const N: usize = 60;

    let run_id = Uuid::new_v4();
    let tasks: Vec<TaskDef> = (0..N)
        .map(|i| task(&format!("t{i}"), &format!("task-{i}"), &[]))
        .collect();
    store
        .insert_run(
            run_id,
            &plan("concurrent", tasks),
            Some("p1"),
            Some("tester"),
            RunState::Running,
            None,
        )
        .await
        .expect("insert_run");

    // Every write is issued concurrently against the same database.
    let mut handles = Vec::new();
    for i in 0..N {
        let s = store.clone();
        handles.push(tokio::spawn(async move {
            s.set_task_status(
                run_id,
                &format!("t{i}"),
                TaskStatus::Passed,
                None,
                false,
                true,
            )
            .await
        }));
    }

    let mut failures = Vec::new();
    for (i, h) in handles.into_iter().enumerate() {
        if let Err(e) = h.await.expect("join") {
            failures.push(format!("t{i}: {e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "concurrent writes returned errors: {failures:?}"
    );

    // And the writes must actually be on disk, not merely un-errored.
    let persisted = store.list_tasks_for_run(run_id).await.expect("list tasks");
    let passed = persisted
        .iter()
        .filter(|t| t.status == TaskStatus::Passed)
        .count();
    assert_eq!(
        passed, N,
        "every concurrent write must be durably persisted"
    );
}

// -------------------------------------------------------------------- B6

/// B6: insert_run must be atomic. Pre-fix each task/dependency was its own
/// implicit transaction, so a failure part-way through left a run row with
/// an incomplete task set and no rollback.
#[tokio::test]
async fn b6_insert_run_rolls_back_completely_on_failure() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();

    // Duplicate task ids violate the tasks PRIMARY KEY (run_id, id) part way
    // through the insert, forcing a mid-transaction failure.
    let bad = plan(
        "rollback",
        vec![
            task("a", "a", &[]),
            task("b", "b", &[]),
            task("a", "dup", &[]),
        ],
    );
    let result = store
        .insert_run(
            run_id,
            &bad,
            Some("p1"),
            Some("tester"),
            RunState::Scheduled,
            None,
        )
        .await;
    assert!(result.is_err(), "duplicate task id must fail the insert");

    // Nothing from the failed insert may survive.
    let tasks = store.list_tasks_for_run(run_id).await.expect("list tasks");
    assert!(
        tasks.is_empty(),
        "a failed insert_run must leave no tasks behind, found {}",
        tasks.len()
    );
    let summary = store.get_run_summary(run_id).await.expect("summary");
    assert!(
        summary.is_none(),
        "a failed insert_run must leave no run row behind"
    );
}

/// B6: large plans must insert successfully. Pre-fix ~100 tasks returned
/// HTTP 500 because every row was a separate transaction.
#[tokio::test]
async fn b6_insert_run_handles_large_plans() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();

    // Chain every task to its predecessor so dependency edges are exercised
    // at scale too, not just bare task rows.
    let mut tasks = vec![task("t0", "task-0", &[])];
    for i in 1..1000 {
        let prev = format!("t{}", i - 1);
        tasks.push(task(
            &format!("t{i}"),
            &format!("task-{i}"),
            &[prev.as_str()],
        ));
    }

    store
        .insert_run(
            run_id,
            &plan("large", tasks),
            Some("p1"),
            Some("tester"),
            RunState::Scheduled,
            None,
        )
        .await
        .expect("1000-task plan must insert");

    let persisted = store.list_tasks_for_run(run_id).await.expect("list tasks");
    assert_eq!(persisted.len(), 1000, "all tasks must persist");
    let with_prereqs = persisted
        .iter()
        .filter(|t| !t.prerequisites.is_empty())
        .count();
    assert_eq!(with_prereqs, 999, "every dependency edge must persist");
}

// -------------------------------------------------------------------- B3

/// B3: task history must not cross project boundaries. Pre-fix the query
/// filtered on task name only, so one project's failures polluted another
/// project's risk/flakiness scores.
#[tokio::test]
async fn b3_task_history_does_not_cross_projects() {
    let (store, _dir) = temp_store().await;
    const SHARED: &str = "shared-task-name";

    // Project alpha: three runs of SHARED, all failing.
    for i in 0..3 {
        let rid = Uuid::new_v4();
        store
            .insert_run(
                rid,
                &plan(&format!("a{i}"), vec![task("t1", SHARED, &[])]),
                Some("alpha"),
                Some("tester"),
                RunState::Running,
                None,
            )
            .await
            .expect("insert alpha");
        // Mirror the engine's own call pattern: start and finish are separate
        // writes — `set_task_status` treats the two flags as mutually
        // exclusive, so a combined (true, true) call never sets finished_at
        // and the row would be invisible to history queries.
        store
            .set_task_status(rid, "t1", TaskStatus::Running, None, true, false)
            .await
            .expect("start");
        store
            .set_task_status(rid, "t1", TaskStatus::Failed, Some("boom"), false, true)
            .await
            .expect("finish");
    }

    // Project beta: one run of the same task name, passing.
    let rid = Uuid::new_v4();
    store
        .insert_run(
            rid,
            &plan("b0", vec![task("t1", SHARED, &[])]),
            Some("beta"),
            Some("tester"),
            RunState::Running,
            None,
        )
        .await
        .expect("insert beta");
    store
        .set_task_status(rid, "t1", TaskStatus::Running, None, true, false)
        .await
        .expect("start");
    store
        .set_task_status(rid, "t1", TaskStatus::Passed, None, false, true)
        .await
        .expect("finish");

    let beta = store
        .task_history_by_name(SHARED, Some("beta"), 20)
        .await
        .expect("beta history");
    assert_eq!(
        beta.len(),
        1,
        "beta ran this task once; it must not see alpha's 3 executions"
    );
    assert!(
        beta.iter().all(|h| h.status == TaskStatus::Passed),
        "beta's only execution passed"
    );

    let alpha = store
        .task_history_by_name(SHARED, Some("alpha"), 20)
        .await
        .expect("alpha history");
    assert_eq!(
        alpha.len(),
        3,
        "alpha must see exactly its own 3 executions"
    );
    assert!(
        alpha.iter().all(|h| h.status == TaskStatus::Failed),
        "alpha's executions all failed"
    );
}

// ------------------------------------------------------------------- B14

/// B14: bug drafts are project-owned via their failure cluster. Pre-fix
/// `list_bug_drafts()` had no project filter at all, so a project-scoped
/// viewer key received every tenant's drafts.
#[tokio::test]
async fn b14_bug_drafts_are_project_scoped() {
    let (store, _dir) = temp_store().await;

    let (alpha_cluster, _) = store
        .upsert_failure_cluster("alpha", "alpha signature", "hypothesis", 0.5)
        .await
        .expect("alpha cluster");
    let (beta_cluster, _) = store
        .upsert_failure_cluster("beta", "beta signature", "hypothesis", 0.5)
        .await
        .expect("beta cluster");

    store
        .insert_bug_draft(alpha_cluster, "alpha draft", "d", "High", 0.5)
        .await
        .expect("alpha draft");
    store
        .insert_bug_draft(beta_cluster, "beta draft", "d", "High", 0.5)
        .await
        .expect("beta draft");

    let alpha = store
        .list_bug_drafts(Some("alpha"))
        .await
        .expect("alpha drafts");
    assert_eq!(alpha.len(), 1, "alpha must see only its own draft");
    assert_eq!(alpha[0].title, "alpha draft");

    let beta = store
        .list_bug_drafts(Some("beta"))
        .await
        .expect("beta drafts");
    assert_eq!(beta.len(), 1, "beta must see only its own draft");
    assert_eq!(beta[0].title, "beta draft");

    // A global/admin key (no project scope) still sees everything.
    let all = store.list_bug_drafts(None).await.expect("all drafts");
    assert_eq!(all.len(), 2, "an unscoped key sees every project's drafts");
}

/// B14: the project owning a draft must be resolvable server-side so the
/// review endpoint can reject a cross-project mutation.
#[tokio::test]
async fn b14_bug_draft_project_is_resolvable_for_authorization() {
    let (store, _dir) = temp_store().await;

    let (cluster, _) = store
        .upsert_failure_cluster("alpha", "sig", "hypothesis", 0.5)
        .await
        .expect("cluster");
    let draft = store
        .insert_bug_draft(cluster, "t", "d", "High", 0.5)
        .await
        .expect("draft");

    let owner = store.bug_draft_project(draft).await.expect("lookup");
    assert_eq!(
        owner.as_deref(),
        Some("alpha"),
        "draft must resolve to its owning project"
    );

    let missing = store
        .bug_draft_project(Uuid::new_v4())
        .await
        .expect("lookup missing");
    assert!(
        missing.is_none(),
        "an unknown draft id must not resolve to a project"
    );
}

// -------------------------------------------------------------------- BE-8

/// BE-8: a created project is durably persisted with the metadata the
/// caller supplied plus server-assigned `created_at`/`updated_at`, and is
/// retrievable both by direct id lookup and through an unscoped list.
#[tokio::test]
async fn be8_project_create_and_get_round_trip() {
    let (store, _dir) = temp_store().await;
    let id = Uuid::new_v4();

    store
        .insert_project(id, "Alpha Project", Some("first project"))
        .await
        .expect("insert_project");

    let fetched = store
        .get_project(id)
        .await
        .expect("get_project")
        .expect("project exists");
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.name, "Alpha Project");
    assert_eq!(fetched.description.as_deref(), Some("first project"));
    assert_eq!(
        fetched.created_at, fetched.updated_at,
        "a freshly created project has identical created/updated timestamps"
    );

    let missing = store
        .get_project(Uuid::new_v4())
        .await
        .expect("get_project on unknown id");
    assert!(
        missing.is_none(),
        "a project id that was never created must not resolve to a row"
    );
}

/// BE-8: `list_projects`'s `Some(pid)` filter must return *at most* the one
/// project matching that id — this is the store-level half of project
/// isolation for the new resource (the API-level half is `require_project_access`,
/// already covered by every other B3/B14 test; this pins the query itself).
#[tokio::test]
async fn be8_list_projects_respects_project_filter() {
    let (store, _dir) = temp_store().await;
    let alpha = Uuid::new_v4();
    let beta = Uuid::new_v4();
    store
        .insert_project(alpha, "Alpha", None)
        .await
        .expect("insert alpha");
    store
        .insert_project(beta, "Beta", None)
        .await
        .expect("insert beta");

    let scoped = store
        .list_projects(Some(&alpha.to_string()))
        .await
        .expect("scoped list");
    assert_eq!(
        scoped.len(),
        1,
        "a scoped filter must return only the matching project"
    );
    assert_eq!(scoped[0].id, alpha);

    let unscoped = store.list_projects(None).await.expect("unscoped list");
    assert_eq!(
        unscoped.len(),
        2,
        "an unscoped (global) caller must see every project"
    );

    let unknown = store
        .list_projects(Some(&Uuid::new_v4().to_string()))
        .await
        .expect("filter on unknown id");
    assert_eq!(
        unknown.len(),
        0,
        "a filter naming a project that was never created must return empty, not an error"
    );
}

/// BE-8: `update_project` leaves an omitted (`None`) field unchanged
/// (`COALESCE`) and updates only the field(s) actually supplied — the same
/// partial-update contract §5.4's `PATCH` semantics require.
#[tokio::test]
async fn be8_update_project_is_a_genuine_partial_update() {
    let (store, _dir) = temp_store().await;
    let id = Uuid::new_v4();
    store
        .insert_project(id, "Original Name", Some("original description"))
        .await
        .expect("insert");
    let before = store.get_project(id).await.expect("get").expect("exists");

    // Update only the name; description must survive untouched.
    let touched = store
        .update_project(id, Some("Renamed"), None)
        .await
        .expect("update name only");
    assert!(
        touched,
        "updating an existing project must report a row was affected"
    );
    let after_name_only = store.get_project(id).await.expect("get").expect("exists");
    assert_eq!(after_name_only.name, "Renamed");
    assert_eq!(
        after_name_only.description.as_deref(),
        Some("original description"),
        "an omitted field must not be clobbered"
    );
    assert!(
        after_name_only.updated_at >= before.updated_at,
        "updated_at must advance"
    );

    // Update only the description; the just-renamed name must survive.
    store
        .update_project(id, None, Some("new description"))
        .await
        .expect("update description only");
    let after_both = store.get_project(id).await.expect("get").expect("exists");
    assert_eq!(
        after_both.name, "Renamed",
        "a prior partial update must not be reverted by a later one"
    );
    assert_eq!(after_both.description.as_deref(), Some("new description"));

    // Updating an id that was never created must report no row affected —
    // this is what lets the API handler return 404 instead of a false "ok".
    let phantom = store
        .update_project(Uuid::new_v4(), Some("x"), None)
        .await
        .expect("update unknown id");
    assert!(
        !phantom,
        "updating a nonexistent project must report zero rows affected, not silently succeed"
    );
}

#[tokio::test]
async fn delete_project_removes_project_and_all_scoped_records_atomically() {
    let (store, _dir) = temp_store().await;
    let id = Uuid::new_v4();
    store
        .insert_project(id, "To Be Deleted", Some("temporary project"))
        .await
        .expect("insert project");

    assert!(store.get_project(id).await.expect("get").is_some());

    let deleted = store.delete_project(id).await.expect("delete_project");
    assert!(
        deleted,
        "delete_project must return true for existing project"
    );

    assert!(store.get_project(id).await.expect("get").is_none());

    let deleted_again = store
        .delete_project(id)
        .await
        .expect("delete_project again");
    assert!(
        !deleted_again,
        "delete_project must return false when project is absent"
    );
}

// ======================================================================
// Phase 5 (Output maturity) — plan -> suite -> case hierarchy.
// ======================================================================

fn minimal_case(title: &str, suite_id: Option<Uuid>) -> CreateTestCaseRequest {
    CreateTestCaseRequest {
        title: title.to_string(),
        description: "description".to_string(),
        category: "Functional".to_string(),
        priority: "MEDIUM".to_string(),
        severity: "MINOR".to_string(),
        preconditions: vec![],
        steps: vec![TestStep {
            step_number: 1,
            action: "Do X".to_string(),
            expected_result: "Y happens".to_string(),
        }],
        expected_result: Some("Y happens".to_string()),
        assertions: vec![],
        test_data: None,
        source_document: None,
        requirement_reference: None,
        source_requirement: None,
        traceability: None,
        risk_factor: None,
        tags: vec![],
        suite_id,
    }
}

fn minimal_plan(name: &str) -> CreateTestPlanRequest {
    CreateTestPlanRequest {
        name: name.to_string(),
        description: None,
        entry_criteria: None,
        exit_criteria: None,
    }
}

fn minimal_suite(name: &str, plan_id: Option<Uuid>) -> CreateTestSuiteRequest {
    CreateTestSuiteRequest {
        name: name.to_string(),
        description: None,
        plan_id,
    }
}

/// The real, full hierarchy in one round trip: a plan with two suites (one
/// assigned at creation, one reassigned afterward via
/// `set_test_suite_plan`), and three cases split across them (two assigned
/// at creation, one reassigned afterward via `set_test_case_suite`) — real,
/// query-time-computed `suite_count`/`case_count` must reflect exactly
/// this shape, and both `list_test_suites`/`list_test_cases`' own filters
/// must return exactly the right subset.
#[tokio::test]
async fn test_plan_suite_case_hierarchy_round_trip_with_real_counts() {
    let (store, _dir) = temp_store().await;
    let plan_id = store
        .insert_test_plan("proj-1", &minimal_plan("Release 1.0"), None)
        .await
        .expect("insert plan");

    let suite_a = store
        .insert_test_suite("proj-1", &minimal_suite("Auth", Some(plan_id)), None)
        .await
        .expect("insert suite_a");
    let suite_b = store
        .insert_test_suite("proj-1", &minimal_suite("Billing", None), None)
        .await
        .expect("insert suite_b");
    let reassigned = store
        .set_test_suite_plan(suite_b, Some(plan_id))
        .await
        .expect("set_test_suite_plan");
    assert!(reassigned);

    let case_1 = store
        .insert_test_case(
            "proj-1",
            &minimal_case("Login succeeds", Some(suite_a)),
            None,
        )
        .await
        .expect("insert case_1");
    let case_2 = store
        .insert_test_case(
            "proj-1",
            &minimal_case("Login rejects bad password", Some(suite_a)),
            None,
        )
        .await
        .expect("insert case_2");
    let case_3 = store
        .insert_test_case("proj-1", &minimal_case("Refund processed", None), None)
        .await
        .expect("insert case_3");
    let case_3_assigned = store
        .set_test_case_suite(case_3, Some(suite_b))
        .await
        .expect("set_test_case_suite");
    assert!(case_3_assigned);

    let plan = store
        .get_test_plan(plan_id)
        .await
        .expect("get_test_plan")
        .expect("plan exists");
    assert_eq!(plan.suite_count, 2, "plan={plan:?}");
    assert_eq!(plan.status, crate::domain::TestPlanStatus::Draft);

    let suite_a_row = store
        .get_test_suite(suite_a)
        .await
        .expect("get_test_suite")
        .expect("suite_a exists");
    assert_eq!(suite_a_row.case_count, 2, "suite_a={suite_a_row:?}");
    let suite_b_row = store
        .get_test_suite(suite_b)
        .await
        .expect("get_test_suite")
        .expect("suite_b exists");
    assert_eq!(suite_b_row.case_count, 1, "suite_b={suite_b_row:?}");
    assert_eq!(suite_b_row.plan_id, Some(plan_id));

    let suites_in_plan = store
        .list_test_suites("proj-1", Some(plan_id))
        .await
        .expect("list_test_suites");
    assert_eq!(suites_in_plan.len(), 2);

    let cases_in_suite_a = store
        .list_test_cases("proj-1", None, Some(suite_a))
        .await
        .expect("list_test_cases");
    let ids: Vec<Uuid> = cases_in_suite_a.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&case_1));
    assert!(ids.contains(&case_2));
    assert!(!ids.contains(&case_3));
}

#[tokio::test]
async fn deleting_a_plan_detaches_its_suites_rather_than_deleting_them() {
    let (store, _dir) = temp_store().await;
    let plan_id = store
        .insert_test_plan("proj-1", &minimal_plan("Doomed Plan"), None)
        .await
        .expect("insert plan");
    let suite_id = store
        .insert_test_suite("proj-1", &minimal_suite("Survives", Some(plan_id)), None)
        .await
        .expect("insert suite");

    let deleted = store
        .delete_test_plan(plan_id)
        .await
        .expect("delete_test_plan");
    assert!(deleted);

    assert!(
        store.get_test_plan(plan_id).await.expect("get").is_none(),
        "the plan itself must be gone"
    );
    let suite = store
        .get_test_suite(suite_id)
        .await
        .expect("get_test_suite")
        .expect(
            "the suite must still exist — a plan deletion detaches, it does not cascade-delete",
        );
    assert_eq!(
        suite.plan_id, None,
        "the surviving suite must be detached, not left pointing at a deleted plan"
    );
}

#[tokio::test]
async fn deleting_a_suite_detaches_its_cases_rather_than_deleting_them() {
    let (store, _dir) = temp_store().await;
    let suite_id = store
        .insert_test_suite("proj-1", &minimal_suite("Doomed Suite", None), None)
        .await
        .expect("insert suite");
    let case_id = store
        .insert_test_case("proj-1", &minimal_case("Survives", Some(suite_id)), None)
        .await
        .expect("insert case");

    let deleted = store
        .delete_test_suite(suite_id)
        .await
        .expect("delete_test_suite");
    assert!(deleted);

    assert!(
        store.get_test_suite(suite_id).await.expect("get").is_none(),
        "the suite itself must be gone"
    );
    let case = store
        .get_test_case(case_id)
        .await
        .expect("get_test_case")
        .expect(
            "the case must still exist — a suite deletion detaches, it does not cascade-delete",
        );
    assert_eq!(
        case.suite_id, None,
        "the surviving case must be detached, not left pointing at a deleted suite"
    );
}

#[tokio::test]
async fn set_test_case_suite_reassigns_and_clears() {
    let (store, _dir) = temp_store().await;
    let suite_1 = store
        .insert_test_suite("proj-1", &minimal_suite("Suite 1", None), None)
        .await
        .expect("insert suite_1");
    let suite_2 = store
        .insert_test_suite("proj-1", &minimal_suite("Suite 2", None), None)
        .await
        .expect("insert suite_2");
    let case_id = store
        .insert_test_case("proj-1", &minimal_case("Movable case", Some(suite_1)), None)
        .await
        .expect("insert case");

    assert_eq!(
        store
            .get_test_case(case_id)
            .await
            .unwrap()
            .unwrap()
            .suite_id,
        Some(suite_1)
    );

    store
        .set_test_case_suite(case_id, Some(suite_2))
        .await
        .expect("reassign");
    assert_eq!(
        store
            .get_test_case(case_id)
            .await
            .unwrap()
            .unwrap()
            .suite_id,
        Some(suite_2),
        "reassignment must genuinely move the case, not just add a second link"
    );
    assert_eq!(
        store
            .get_test_suite(suite_1)
            .await
            .unwrap()
            .unwrap()
            .case_count,
        0,
        "suite_1 must no longer count a case it no longer has"
    );

    store
        .set_test_case_suite(case_id, None)
        .await
        .expect("clear");
    assert_eq!(
        store
            .get_test_case(case_id)
            .await
            .unwrap()
            .unwrap()
            .suite_id,
        None,
        "explicit None must genuinely clear the assignment"
    );
}

// --------------------------------------- Vector Store v1 (test_case_embeddings)

#[tokio::test]
async fn test_case_embedding_upsert_and_list_round_trip() {
    let (store, _dir) = temp_store().await;
    let case_id = store
        .insert_test_case("proj-1", &minimal_case("Login case", None), None)
        .await
        .expect("insert case");

    store
        .upsert_test_case_embedding(
            case_id,
            "proj-1",
            "gemini-embedding-001",
            3,
            &[0.1, 0.2, 0.3],
        )
        .await
        .expect("upsert embedding");

    let rows = store
        .list_test_case_embeddings("proj-1")
        .await
        .expect("list embeddings");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].test_case_id, case_id);
    assert_eq!(rows[0].embedding, vec![0.1, 0.2, 0.3]);
}

#[tokio::test]
async fn upsert_test_case_embedding_replaces_not_duplicates() {
    let (store, _dir) = temp_store().await;
    let case_id = store
        .insert_test_case("proj-1", &minimal_case("Login case", None), None)
        .await
        .expect("insert case");

    store
        .upsert_test_case_embedding(
            case_id,
            "proj-1",
            "gemini-embedding-001",
            3,
            &[0.1, 0.2, 0.3],
        )
        .await
        .expect("first upsert");
    // Real re-embed after an edit (`api.rs::embed_approved_test_case`'s own
    // trigger) — same `test_case_id`, a genuinely different real vector.
    store
        .upsert_test_case_embedding(
            case_id,
            "proj-1",
            "gemini-embedding-001",
            3,
            &[0.9, 0.8, 0.7],
        )
        .await
        .expect("second upsert");

    let rows = store
        .list_test_case_embeddings("proj-1")
        .await
        .expect("list embeddings");
    assert_eq!(
        rows.len(),
        1,
        "a second upsert for the same test case must replace, never add a second row"
    );
    assert_eq!(
        rows[0].embedding,
        vec![0.9, 0.8, 0.7],
        "the stored vector must be the latest real one, not the stale first one"
    );
}

#[tokio::test]
async fn delete_test_case_also_deletes_its_embedding() {
    let (store, _dir) = temp_store().await;
    let case_id = store
        .insert_test_case("proj-1", &minimal_case("Login case", None), None)
        .await
        .expect("insert case");
    store
        .upsert_test_case_embedding(
            case_id,
            "proj-1",
            "gemini-embedding-001",
            3,
            &[0.1, 0.2, 0.3],
        )
        .await
        .expect("upsert embedding");
    assert_eq!(store.count_test_case_embeddings("proj-1").await.unwrap(), 1);

    let deleted = store.delete_test_case(case_id).await.expect("delete");
    assert!(deleted);

    assert_eq!(
        store.count_test_case_embeddings("proj-1").await.unwrap(),
        0,
        "deleting the test case must also delete its embedding row, never leave it orphaned"
    );
    assert!(store
        .list_test_case_embeddings("proj-1")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn count_test_case_embeddings_is_scoped_per_real_project() {
    let (store, _dir) = temp_store().await;
    let case_a = store
        .insert_test_case("proj-a", &minimal_case("Case A", None), None)
        .await
        .expect("insert case a");
    let case_b1 = store
        .insert_test_case("proj-b", &minimal_case("Case B1", None), None)
        .await
        .expect("insert case b1");
    let case_b2 = store
        .insert_test_case("proj-b", &minimal_case("Case B2", None), None)
        .await
        .expect("insert case b2");

    store
        .upsert_test_case_embedding(
            case_a,
            "proj-a",
            "gemini-embedding-001",
            3,
            &[0.1, 0.0, 0.0],
        )
        .await
        .expect("embed a");
    store
        .upsert_test_case_embedding(
            case_b1,
            "proj-b",
            "gemini-embedding-001",
            3,
            &[0.2, 0.0, 0.0],
        )
        .await
        .expect("embed b1");
    store
        .upsert_test_case_embedding(
            case_b2,
            "proj-b",
            "gemini-embedding-001",
            3,
            &[0.3, 0.0, 0.0],
        )
        .await
        .expect("embed b2");

    assert_eq!(store.count_test_case_embeddings("proj-a").await.unwrap(), 1);
    assert_eq!(store.count_test_case_embeddings("proj-b").await.unwrap(), 2);
    assert_eq!(
        store.count_test_case_embeddings("proj-c").await.unwrap(),
        0,
        "a project with no embedded cases at all must honestly report 0, not error"
    );
}

#[tokio::test]
async fn test_plan_update_is_a_genuine_partial_update_including_status() {
    let (store, _dir) = temp_store().await;
    let plan_id = store
        .insert_test_plan("proj-1", &minimal_plan("Original Name"), None)
        .await
        .expect("insert plan");

    store
        .update_test_plan(plan_id, None, None, Some("ACTIVE"), None, None)
        .await
        .expect("update status only");
    let after_status = store.get_test_plan(plan_id).await.unwrap().unwrap();
    assert_eq!(after_status.status, crate::domain::TestPlanStatus::Active);
    assert_eq!(
        after_status.name, "Original Name",
        "an omitted field must not be clobbered"
    );

    store
        .update_test_plan(plan_id, Some("Renamed"), None, None, None, None)
        .await
        .expect("update name only");
    let after_name = store.get_test_plan(plan_id).await.unwrap().unwrap();
    assert_eq!(after_name.name, "Renamed");
    assert_eq!(
        after_name.status,
        crate::domain::TestPlanStatus::Active,
        "a prior partial update must not be reverted by a later one"
    );
}

#[tokio::test]
async fn test_plans_and_suites_are_isolated_across_projects() {
    let (store, _dir) = temp_store().await;
    store
        .insert_test_plan("proj-1", &minimal_plan("Proj 1 Plan"), None)
        .await
        .expect("insert");
    store
        .insert_test_plan("proj-2", &minimal_plan("Proj 2 Plan"), None)
        .await
        .expect("insert");
    store
        .insert_test_suite("proj-1", &minimal_suite("Proj 1 Suite", None), None)
        .await
        .expect("insert");

    let proj_1_plans = store.list_test_plans("proj-1").await.expect("list");
    assert_eq!(proj_1_plans.len(), 1);
    assert_eq!(proj_1_plans[0].name, "Proj 1 Plan");

    let proj_2_suites = store.list_test_suites("proj-2", None).await.expect("list");
    assert!(proj_2_suites.is_empty());
}

#[tokio::test]
async fn deleting_a_project_removes_its_test_plans_and_suites_too() {
    let (store, _dir) = temp_store().await;
    let project_id = Uuid::new_v4();
    store
        .insert_project(project_id, "Doomed Project", None)
        .await
        .expect("insert project");
    let pid = project_id.to_string();
    store
        .insert_test_plan(&pid, &minimal_plan("Plan"), None)
        .await
        .expect("insert plan");
    store
        .insert_test_suite(&pid, &minimal_suite("Suite", None), None)
        .await
        .expect("insert suite");

    store
        .delete_project(project_id)
        .await
        .expect("delete_project");

    assert!(store.list_test_plans(&pid).await.expect("list").is_empty());
    assert!(store
        .list_test_suites(&pid, None)
        .await
        .expect("list")
        .is_empty());
}

// ---------------------------------------------------------- Phase 3 — requirements

fn parsed_doc(items: &[(&str, &str)]) -> crate::requirements::ParsedDocument {
    crate::requirements::ParsedDocument {
        requirements: items
            .iter()
            .enumerate()
            .map(|(i, (key, text))| crate::requirements::ParsedRequirement {
                key: key.to_string(),
                ordinal: i,
                section: Some("3.2 Login".to_string()),
                kind: crate::requirements::RequirementKind::Functional,
                text: text.to_string(),
            })
            .collect(),
        sections_count: 1,
        skipped_sections: vec!["Revision History".to_string()],
        furniture_lines: 4,
    }
}

async fn upsert_doc(
    store: &Store,
    project: &str,
    file_name: &str,
    hash: &str,
    parsed: &crate::requirements::ParsedDocument,
) -> Uuid {
    store
        .upsert_requirement_document(project, file_name, Some("FRD"), hash, "full text", parsed)
        .await
        .expect("upsert")
}

#[tokio::test]
async fn a_requirement_document_is_stored_with_its_requirements_in_document_order() {
    let (store, _dir) = temp_store().await;
    let parsed = parsed_doc(&[("FR-1", "First."), ("FR-2", "Second."), ("FR-3", "Third.")]);

    let id = upsert_doc(&store, "p1", "FRD.md", "h1", &parsed).await;

    let docs = store.list_requirement_documents("p1").await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, id);
    assert_eq!(docs[0].file_name, "FRD.md");
    assert_eq!(docs[0].doc_type.as_deref(), Some("FRD"));
    assert_eq!(docs[0].requirements_count, 3);
    assert_eq!(docs[0].sections_count, 1);
    assert_eq!(docs[0].skipped_sections, ["Revision History"]);
    assert_eq!(docs[0].furniture_lines, 4);
    assert_eq!(docs[0].char_count, "full text".chars().count() as i64);

    let reqs = store.list_requirements("p1", &[]).await.unwrap();
    let keys: Vec<&str> = reqs.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["FR-1", "FR-2", "FR-3"]);
    assert_eq!(reqs[1].text, "Second.");
    assert_eq!(reqs[1].kind, "functional");
    assert_eq!(reqs[1].section.as_deref(), Some("3.2 Login"));
    assert_eq!(reqs[1].document_id, id);
}

#[tokio::test]
async fn the_same_content_again_refreshes_the_document_and_leaves_its_requirements_alone() {
    let (store, _dir) = temp_store().await;
    let id = upsert_doc(&store, "p1", "FRD.md", "h1", &parsed_doc(&[("FR-1", "First."), ("FR-2", "Second.")])).await;
    sqlx::query("UPDATE requirements SET text = 'EDITED' WHERE ordinal = 1")
        .execute(&store.pool)
        .await
        .unwrap();

    // Same hash, different file name, and (hypothetically) different parse output.
    let again = upsert_doc(&store, "p1", "FRD-final.md", "h1", &parsed_doc(&[("X-1", "Other.")])).await;

    assert_eq!(again, id);
    let docs = store.list_requirement_documents("p1").await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].file_name, "FRD-final.md");
    let reqs = store.list_requirements("p1", &[]).await.unwrap();
    let texts: Vec<&str> = reqs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, ["First.", "EDITED"], "unchanged content is not parsed into the table twice");
}

#[tokio::test]
async fn requirements_written_by_older_parsing_rules_are_replaced_on_the_next_upload() {
    let (store, _dir) = temp_store().await;
    let id = upsert_doc(&store, "p1", "FRD.md", "h1", &parsed_doc(&[("FR-1", "Old one."), ("FR-2", "Old two.")])).await;
    sqlx::query("UPDATE requirement_documents SET parser_version = 0")
        .execute(&store.pool)
        .await
        .unwrap();

    let again = upsert_doc(&store, "p1", "FRD.md", "h1", &parsed_doc(&[("FR-1", "New one.")])).await;

    assert_eq!(again, id);
    let reqs = store.list_requirements("p1", &[]).await.unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].text, "New one.");
    let version: i64 = sqlx::query_scalar("SELECT parser_version FROM requirement_documents")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(version, crate::requirements::PARSER_VERSION);
}

#[tokio::test]
async fn requirements_are_listed_in_the_order_the_documents_were_asked_for() {
    let (store, _dir) = temp_store().await;
    let a = upsert_doc(&store, "p1", "A.md", "ha", &parsed_doc(&[("A-1", "a one"), ("A-2", "a two")])).await;
    let b = upsert_doc(&store, "p1", "B.md", "hb", &parsed_doc(&[("B-1", "b one")])).await;

    let asked_b_first: Vec<String> = store
        .list_requirements("p1", &[b, a])
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(asked_b_first, ["B-1", "A-1", "A-2"]);

    let only_a: Vec<String> = store
        .list_requirements("p1", &[a])
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(only_a, ["A-1", "A-2"]);

    let everything: Vec<String> = store
        .list_requirements("p1", &[])
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(everything, ["A-1", "A-2", "B-1"], "upload order when no ids are given");
}

#[tokio::test]
async fn another_projects_documents_and_requirements_are_never_returned() {
    let (store, _dir) = temp_store().await;
    let theirs = upsert_doc(&store, "p2", "Other.md", "h1", &parsed_doc(&[("O-1", "theirs")])).await;
    upsert_doc(&store, "p1", "Mine.md", "h2", &parsed_doc(&[("M-1", "mine")])).await;

    assert_eq!(store.list_requirement_documents("p1").await.unwrap().len(), 1);
    assert!(store.list_requirements("p1", &[theirs]).await.unwrap().is_empty());
    let mine: Vec<String> = store
        .list_requirements("p1", &[])
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(mine, ["M-1"]);

    // The same content in two projects is two documents, not one shared row.
    let same_in_p1 = upsert_doc(&store, "p1", "Other.md", "h1", &parsed_doc(&[("O-1", "theirs")])).await;
    assert_ne!(same_in_p1, theirs);
}

// ------------------------------------------------------------------ Phase 7B — what AI calls took

#[tokio::test]
async fn recorded_ai_calls_add_up_per_provider_model_and_task() {
    let (store, _dir) = temp_store().await;
    store.record_ai_call("ollama", "qwen", "TEST_CASE_GENERATION", 60_000, Some(3_000), Some(1_500)).await.unwrap();
    store.record_ai_call("ollama", "qwen", "TEST_CASE_GENERATION", 40_000, Some(1_000), Some(500)).await.unwrap();
    // Another model, another provider and another task are rows of their own.
    store.record_ai_call("ollama", "llama", "TEST_CASE_GENERATION", 10_000, Some(10), Some(10)).await.unwrap();
    store.record_ai_call("groq", "qwen", "TEST_CASE_GENERATION", 5_000, Some(20), Some(20)).await.unwrap();
    store.record_ai_call("ollama", "qwen", "TRACEABILITY", 7_000, Some(30), Some(30)).await.unwrap();

    let generation = store.ai_call_stats("TEST_CASE_GENERATION").await.unwrap();
    let names: Vec<(&str, &str)> = generation.iter().map(|r| (r.provider.as_str(), r.model.as_str())).collect();
    assert_eq!(names, [("groq", "qwen"), ("ollama", "llama"), ("ollama", "qwen")], "sorted by provider then model");
    let qwen = generation.iter().find(|r| r.provider == "ollama" && r.model == "qwen").unwrap();
    assert_eq!((qwen.calls, qwen.total_ms, qwen.token_calls), (2, 100_000, 2));
    assert_eq!((qwen.prompt_tokens, qwen.completion_tokens), (4_000, 2_000));
    assert_eq!(qwen.task, "TEST_CASE_GENERATION");
    let other_task = store.ai_call_stats("TRACEABILITY").await.unwrap();
    assert_eq!(other_task.len(), 1);
    assert_eq!(other_task[0].calls, 1);
    assert!(store.ai_call_stats("NOTHING").await.unwrap().is_empty());
}

#[tokio::test]
async fn a_call_that_reported_no_tokens_counts_as_a_call_but_never_dilutes_the_token_average() {
    let (store, _dir) = temp_store().await;
    store.record_ai_call("p", "m", "T", 1_000, Some(100), Some(50)).await.unwrap();
    store.record_ai_call("p", "m", "T", 3_000, None, None).await.unwrap();
    // Only one of the two figures: not a usable pair either.
    store.record_ai_call("p", "m", "T", 2_000, Some(999), None).await.unwrap();
    store.record_ai_call("p", "m", "T", 2_000, None, Some(999)).await.unwrap();

    let row = &store.ai_call_stats("T").await.unwrap()[0];
    assert_eq!((row.calls, row.total_ms), (4, 8_000));
    assert_eq!((row.token_calls, row.prompt_tokens, row.completion_tokens), (1, 100, 50));
}

#[tokio::test]
async fn a_latency_too_large_to_store_is_kept_as_the_largest_that_can_be() {
    let (store, _dir) = temp_store().await;
    store.record_ai_call("p", "m", "T", u64::MAX, None, None).await.unwrap();
    assert_eq!(store.ai_call_stats("T").await.unwrap()[0].total_ms, i64::MAX);
}

// ------------------------------------------------------------------ Phase 7C — a key found by its id

#[tokio::test]
async fn an_api_key_is_found_by_its_id_only_while_it_has_not_been_revoked() {
    let (store, _dir) = temp_store().await;
    let id = Uuid::new_v4();
    store.insert_api_key(id, "ci", "hash-1", "operator", Some("p1")).await.unwrap();
    store.insert_api_key(Uuid::new_v4(), "other", "hash-2", "viewer", None).await.unwrap();

    let key = store.find_active_api_key(id).await.unwrap().expect("active");
    assert_eq!((key.id, key.name.as_str(), key.role.as_str()), (id, "ci", "operator"));
    assert_eq!(key.project_id.as_deref(), Some("p1"));
    let global = store.find_api_key_by_hash("hash-2").await.unwrap().expect("by hash, as before");
    assert!(global.project_id.is_none());
    assert!(store.find_active_api_key(Uuid::new_v4()).await.unwrap().is_none(), "an id nobody has");

    store.revoke_api_key(id).await.unwrap();
    assert!(store.find_active_api_key(id).await.unwrap().is_none(), "revoked");
    assert!(store.find_api_key_by_hash("hash-1").await.unwrap().is_none(), "revoked by hash too");
}
