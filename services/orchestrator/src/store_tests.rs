//! Phase 1 regression tests for `store.rs`.
//!
//! Every test here was written to FAIL against the pre-Phase-1 code and to
//! pass only once the corresponding defect is fixed. Each names the blocker
//! it pins (B1/B3/B6/B14) so a future regression is immediately traceable to
//! the audit finding it reopens.
//!
//! Kept in its own file rather than inline in `store.rs` purely for size:
//! `store.rs` is already the largest module in the crate.

use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskStatus};
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
        simulate: Some(SimSpec { outcome: SimOutcome::Pass, delay_ms: 0 }),
        ..Default::default()
    }
}

fn plan(name: &str, tasks: Vec<TaskDef>) -> RunPlan {
    RunPlan { name: name.into(), tasks, concurrency: 4 }
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
        .insert_run(run_id, &plan("concurrent", tasks), Some("p1"), Some("tester"), RunState::Running, None)
        .await
        .expect("insert_run");

    // Every write is issued concurrently against the same database.
    let mut handles = Vec::new();
    for i in 0..N {
        let s = store.clone();
        handles.push(tokio::spawn(async move {
            s.set_task_status(run_id, &format!("t{i}"), TaskStatus::Passed, None, false, true).await
        }));
    }

    let mut failures = Vec::new();
    for (i, h) in handles.into_iter().enumerate() {
        if let Err(e) = h.await.expect("join") {
            failures.push(format!("t{i}: {e}"));
        }
    }
    assert!(failures.is_empty(), "concurrent writes returned errors: {failures:?}");

    // And the writes must actually be on disk, not merely un-errored.
    let persisted = store.list_tasks_for_run(run_id).await.expect("list tasks");
    let passed = persisted.iter().filter(|t| t.status == TaskStatus::Passed).count();
    assert_eq!(passed, N, "every concurrent write must be durably persisted");
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
        vec![task("a", "a", &[]), task("b", "b", &[]), task("a", "dup", &[])],
    );
    let result = store
        .insert_run(run_id, &bad, Some("p1"), Some("tester"), RunState::Scheduled, None)
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
    assert!(summary.is_none(), "a failed insert_run must leave no run row behind");
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
        tasks.push(task(&format!("t{i}"), &format!("task-{i}"), &[prev.as_str()]));
    }

    store
        .insert_run(run_id, &plan("large", tasks), Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("1000-task plan must insert");

    let persisted = store.list_tasks_for_run(run_id).await.expect("list tasks");
    assert_eq!(persisted.len(), 1000, "all tasks must persist");
    let with_prereqs = persisted.iter().filter(|t| !t.prerequisites.is_empty()).count();
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
    assert_eq!(beta.len(), 1, "beta ran this task once; it must not see alpha's 3 executions");
    assert!(
        beta.iter().all(|h| h.status == TaskStatus::Passed),
        "beta's only execution passed"
    );

    let alpha = store
        .task_history_by_name(SHARED, Some("alpha"), 20)
        .await
        .expect("alpha history");
    assert_eq!(alpha.len(), 3, "alpha must see exactly its own 3 executions");
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

    let alpha = store.list_bug_drafts(Some("alpha")).await.expect("alpha drafts");
    assert_eq!(alpha.len(), 1, "alpha must see only its own draft");
    assert_eq!(alpha[0].title, "alpha draft");

    let beta = store.list_bug_drafts(Some("beta")).await.expect("beta drafts");
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

    let missing = store.bug_draft_project(Uuid::new_v4()).await.expect("lookup missing");
    assert!(missing.is_none(), "an unknown draft id must not resolve to a project");
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

    let fetched = store.get_project(id).await.expect("get_project").expect("project exists");
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.name, "Alpha Project");
    assert_eq!(fetched.description.as_deref(), Some("first project"));
    assert_eq!(fetched.created_at, fetched.updated_at, "a freshly created project has identical created/updated timestamps");

    let missing = store.get_project(Uuid::new_v4()).await.expect("get_project on unknown id");
    assert!(missing.is_none(), "a project id that was never created must not resolve to a row");
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
    store.insert_project(alpha, "Alpha", None).await.expect("insert alpha");
    store.insert_project(beta, "Beta", None).await.expect("insert beta");

    let scoped = store.list_projects(Some(&alpha.to_string())).await.expect("scoped list");
    assert_eq!(scoped.len(), 1, "a scoped filter must return only the matching project");
    assert_eq!(scoped[0].id, alpha);

    let unscoped = store.list_projects(None).await.expect("unscoped list");
    assert_eq!(unscoped.len(), 2, "an unscoped (global) caller must see every project");

    let unknown = store.list_projects(Some(&Uuid::new_v4().to_string())).await.expect("filter on unknown id");
    assert_eq!(unknown.len(), 0, "a filter naming a project that was never created must return empty, not an error");
}

/// BE-8: `update_project` leaves an omitted (`None`) field unchanged
/// (`COALESCE`) and updates only the field(s) actually supplied — the same
/// partial-update contract §5.4's `PATCH` semantics require.
#[tokio::test]
async fn be8_update_project_is_a_genuine_partial_update() {
    let (store, _dir) = temp_store().await;
    let id = Uuid::new_v4();
    store.insert_project(id, "Original Name", Some("original description")).await.expect("insert");
    let before = store.get_project(id).await.expect("get").expect("exists");

    // Update only the name; description must survive untouched.
    let touched = store.update_project(id, Some("Renamed"), None).await.expect("update name only");
    assert!(touched, "updating an existing project must report a row was affected");
    let after_name_only = store.get_project(id).await.expect("get").expect("exists");
    assert_eq!(after_name_only.name, "Renamed");
    assert_eq!(after_name_only.description.as_deref(), Some("original description"), "an omitted field must not be clobbered");
    assert!(after_name_only.updated_at >= before.updated_at, "updated_at must advance");

    // Update only the description; the just-renamed name must survive.
    store.update_project(id, None, Some("new description")).await.expect("update description only");
    let after_both = store.get_project(id).await.expect("get").expect("exists");
    assert_eq!(after_both.name, "Renamed", "a prior partial update must not be reverted by a later one");
    assert_eq!(after_both.description.as_deref(), Some("new description"));

    // Updating an id that was never created must report no row affected —
    // this is what lets the API handler return 404 instead of a false "ok".
    let phantom = store.update_project(Uuid::new_v4(), Some("x"), None).await.expect("update unknown id");
    assert!(!phantom, "updating a nonexistent project must report zero rows affected, not silently succeed");
}
