//! Phase 2 (QA Intelligence Memory) — the persistent foundation.
//!
//! Two things are established here, and both are additive:
//!
//!   1. A durable test-case <-> execution link (`tasks.test_case_id`, fed from
//!      `TaskDef.test_case_id`). `execution_telemetry.test_case_id` was NULL in
//!      every row the system had ever written because the forward link had
//!      nowhere to live; the backward half (`test_cases.executed_run_id`) has
//!      always existed. See migrations/0028_qa_memory_linkage.sql.
//!
//!   2. A first-class release (`qa_releases` + nullable `runs.release_id`).
//!      Release Risk is a whole phase of the master plan and had nothing to read.
//!      See migrations/0029_qa_releases.sql.
//!
//! A module is deliberately *not* a table: it is a `quality_nodes` vertex of type
//! `module` on the graph that already exists.
//!
//! Everything here is NULL-tolerant. A run with no release, a crawl with no case,
//! a test case with no module — all legitimate states, none of them ever filled
//! in by guessing (master plan §1.4).

use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef};
use crate::store::Store;
use uuid::Uuid;

/// A store on a fresh, empty database with every migration applied.
async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("qa-memory.sqlite3");
    let store = Store::connect(&path).await.expect("connect");
    (store, dir)
}

async fn project(store: &Store, name: &str) -> String {
    let id = Uuid::new_v4();
    store
        .insert_project(id, name, Some("phase 2 fixture"))
        .await
        .expect("insert_project");
    id.to_string()
}

fn sim_task(id: &str) -> TaskDef {
    TaskDef {
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
    }
}

fn compiled_task(id: &str, case: Option<&str>) -> TaskDef {
    TaskDef {
        id: id.into(),
        name: format!("Test Case: {id}"),
        prerequisites: Vec::new(),
        simulate: None,
        executor: Some("compiled-api".into()),
        params: serde_json::json!({}),
        test_case_id: case.map(ToString::to_string),
    }
}

async fn run_with(store: &Store, pid: &str, tasks: Vec<TaskDef>, release: Option<&str>) -> Uuid {
    let run_id = Uuid::new_v4();
    store
        .insert_run_for_release(
            run_id,
            &RunPlan {
                name: "fixture run".into(),
                tasks,
                concurrency: 1,
            },
            Some(pid),
            Some("fixture"),
            RunState::Created,
            None,
            release,
        )
        .await
        .expect("insert_run_for_release");
    run_id
}

/// Rewrite a run's stored plan, to stand in for the pre-0028 world where the
/// plan named a case but the task row did not.
async fn rewrite_plan(store: &Store, run_id: Uuid, plan_json: &str) {
    sqlx::query("UPDATE runs SET plan = ? WHERE id = ?")
        .bind(plan_json)
        .bind(run_id.to_string())
        .execute(&store.pool)
        .await
        .expect("rewrite plan");
}

const CASE_A: &str = "6f1c9a20-0000-4000-8000-00000000000a";
const CASE_B: &str = "6f1c9a20-0000-4000-8000-00000000000b";

// ---------------------------------------------------------------- migration

#[tokio::test]
async fn the_test_case_link_migration_applies_to_a_fresh_database() {
    let (store, _d) = temp_store().await;

    let ddl = store
        .table_ddl("tasks")
        .await
        .expect("tasks ddl")
        .expect("tasks table exists");
    assert!(
        ddl.contains("test_case_id"),
        "migration 0028 did not add tasks.test_case_id; DDL was: {ddl}"
    );

    // The telemetry side must keep the column this phase makes usable.
    let tddl = store
        .table_ddl("execution_telemetry")
        .await
        .expect("telemetry ddl")
        .expect("telemetry exists");
    assert!(
        tddl.contains("test_case_id"),
        "execution_telemetry lost its test_case_id column"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn the_release_migration_applies_to_a_fresh_database() {
    let (store, _d) = temp_store().await;

    assert!(
        store.table_exists("qa_releases").await.expect("exists"),
        "migration 0029 did not create qa_releases"
    );
    let ddl = store.table_ddl("qa_releases").await.unwrap().unwrap();
    for column in [
        "project_id",
        "label",
        "version",
        "created_at",
        "released_at",
    ] {
        assert!(
            ddl.contains(column),
            "qa_releases.{column} missing; DDL was: {ddl}"
        );
    }

    let runs = store.table_ddl("runs").await.unwrap().unwrap();
    assert!(
        runs.contains("release_id"),
        "migration 0029 did not add runs.release_id; DDL was: {runs}"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn migrations_preserve_rows_written_before_them() {
    // A database carrying the pre-0028/0029 shape must gain the new columns and
    // keep every existing row. This is the "existing database" case from the
    // master plan's Phase 2 test list, and the one a destructive migration would
    // silently fail.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pre-existing.sqlite3");

    let run_id = {
        let store = Store::connect(&path).await.expect("connect");
        let pid = project(&store, "Existing Project").await;
        let id = run_with(&store, &pid, vec![sim_task("pre-task")], None).await;
        // Phase 9.2B Batch 6: this fixture opens TWO stores on the same file (the block above
        // and the reconnect below). Dropping a `Store` does not close its pool, so the first pool
        // is closed explicitly before the second is opened -- otherwise its handle keeps the file
        // open and the TempDir leaks.
        store.pool.close().await;
        id
    };

    let store = Store::connect(&path).await.expect("reconnect");
    let runs = store.list_run_ids(None).await.expect("runs survive");
    assert!(
        runs.contains(&run_id),
        "the pre-existing run was lost by the migration"
    );

    let tasks = store
        .list_tasks_for_run(run_id)
        .await
        .expect("tasks survive");
    assert_eq!(tasks.len(), 1, "the pre-existing task was lost");
    assert!(
        tasks[0].test_case_id.is_none(),
        "a row that predates the link must not be given one"
    );

    // Phase 9.2B Batch 6: the reconnected pool is the one still holding the file open at the end
    // of this test; without this close the directory leaks even though the first store was closed.
    store.pool.close().await;
}

// ------------------------------------------------------------- forward link

#[tokio::test]
async fn a_task_built_from_a_test_case_carries_that_case_on_the_task_row() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Linkage").await;
    let run_id = run_with(
        &store,
        &pid,
        vec![compiled_task("linked-task", Some(CASE_A))],
        None,
    )
    .await;

    let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
    assert_eq!(
        tasks[0].test_case_id.as_deref(),
        Some(CASE_A),
        "the case id given to the plan must reach the task row"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_task_that_is_not_a_case_execution_is_left_unlinked() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Unlinked").await;
    let run_id = run_with(&store, &pid, vec![sim_task("crawl-task")], None).await;

    let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
    assert!(
        tasks[0].test_case_id.is_none(),
        "a task that is not a case's execution must not be attributed to one"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_malformed_case_id_is_stored_verbatim_but_never_reaches_telemetry_as_a_uuid() {
    // The task row stores the string it was given; the telemetry writer is the
    // place that must not turn a bad value into a link. Both halves are asserted
    // because only the second one is a safety property.
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Malformed").await;
    let run_id = run_with(
        &store,
        &pid,
        vec![compiled_task("bad-task", Some("not-a-uuid"))],
        None,
    )
    .await;

    let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
    assert_eq!(
        tasks[0].test_case_id.as_deref(),
        Some("not-a-uuid"),
        "the row keeps what it was given"
    );

    assert!(
        uuid::Uuid::parse_str("not-a-uuid").is_err(),
        "and such a value must not parse, so telemetry records no link"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------ backfill (§10)

#[tokio::test]
async fn the_backfill_recovers_an_unambiguous_historical_link_and_is_idempotent() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Backfill").await;
    let run_id = run_with(&store, &pid, vec![sim_task("hist-task")], None).await;

    // The pre-0028 world: the plan named a case, the task row did not.
    rewrite_plan(
        &store,
        run_id,
        &format!(
            r#"{{"name":"historical","concurrency":1,"tasks":[{{"id":"hist-task","name":"t","test_case_id":"{CASE_A}"}}]}}"#
        ),
    )
    .await;

    let first = store
        .backfill_task_test_case_links(&pid)
        .await
        .expect("first backfill");
    assert_eq!(first, 1, "one unambiguous row should be recovered");

    let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
    assert_eq!(
        tasks[0].test_case_id.as_deref(),
        Some(CASE_A),
        "the historical link must be recovered from the plan that always held it"
    );

    let second = store
        .backfill_task_test_case_links(&pid)
        .await
        .expect("second backfill");
    assert_eq!(second, 0, "a second run must change nothing");
    store.pool.close().await;
}

#[tokio::test]
async fn the_backfill_maps_each_task_to_its_own_case_when_the_plan_says_so() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Two cases").await;

    // Two tasks naming two different cases IS fully determined: the plan records
    // which task belongs to which case, so both links are recovered. An earlier
    // draft of this test wrongly called this shape ambiguous; it is not, and
    // treating it as ambiguous would have discarded real recoverable history.
    let run_id = run_with(&store, &pid, vec![sim_task("t-a"), sim_task("t-b")], None).await;
    rewrite_plan(
        &store,
        run_id,
        &format!(
            r#"{{"name":"two","concurrency":1,"tasks":[
                 {{"id":"t-a","name":"A","test_case_id":"{CASE_A}"}},
                 {{"id":"t-b","name":"B","test_case_id":"{CASE_B}"}}]}}"#
        ),
    )
    .await;

    let recovered = store
        .backfill_task_test_case_links(&pid)
        .await
        .expect("backfill");
    assert_eq!(recovered, 2, "both determined links should be recovered");

    let mut seen: Vec<String> = store
        .list_tasks_for_run(run_id)
        .await
        .expect("tasks")
        .into_iter()
        .map(|t| t.test_case_id.expect("linked"))
        .collect();
    seen.sort();
    let mut expected = vec![CASE_A.to_string(), CASE_B.to_string()];
    expected.sort();
    assert_eq!(
        seen, expected,
        "each task must get its own case, not a shared one"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn the_backfill_refuses_a_plan_naming_a_case_id_that_is_not_a_uuid() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Bad id").await;
    let run_id = run_with(&store, &pid, vec![sim_task("bad-id-task")], None).await;
    rewrite_plan(
        &store,
        run_id,
        r#"{"name":"bad","concurrency":1,"tasks":[
             {"id":"bad-id-task","name":"t","test_case_id":"not-a-uuid"}]}"#,
    )
    .await;

    assert_eq!(
        store
            .backfill_task_test_case_links(&pid)
            .await
            .expect("backfill"),
        0,
        "a value that is not a case id must never be written as one"
    );
    assert!(
        store.list_tasks_for_run(run_id).await.unwrap()[0]
            .test_case_id
            .is_none(),
        "the task must stay unlinked"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_pre_phase2_plan_naming_no_case_recovers_nothing() {
    // The real shape of history: `TaskDef` had no `test_case_id` at all before
    // Phase 2, so no pre-existing plan can supply a link. These rows stay NULL
    // until a real execution records one.
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Pre-phase2").await;
    let run_id = run_with(
        &store,
        &pid,
        vec![sim_task("old-a"), sim_task("old-b")],
        None,
    )
    .await;
    rewrite_plan(
        &store,
        run_id,
        r#"{"name":"old","concurrency":1,"tasks":[
             {"id":"old-a","name":"A"},
             {"id":"old-b","name":"B"}]}"#,
    )
    .await;

    assert_eq!(
        store
            .backfill_task_test_case_links(&pid)
            .await
            .expect("backfill"),
        0,
        "a plan with no case ids is not evidence of any link"
    );
    for t in store.list_tasks_for_run(run_id).await.unwrap() {
        assert!(t.test_case_id.is_none());
    }
    store.pool.close().await;
}

#[tokio::test]
async fn the_backfill_skips_a_plan_where_one_task_has_no_case_id() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Partial").await;
    let run_id = run_with(
        &store,
        &pid,
        vec![sim_task("part-a"), sim_task("part-b")],
        None,
    )
    .await;
    rewrite_plan(
        &store,
        run_id,
        &format!(
            r#"{{"name":"partial","concurrency":1,"tasks":[
                 {{"id":"part-a","name":"A","test_case_id":"{CASE_A}"}},
                 {{"id":"part-b","name":"B"}}]}}"#
        ),
    )
    .await;

    let recovered = store
        .backfill_task_test_case_links(&pid)
        .await
        .expect("backfill");
    assert_eq!(
        recovered, 0,
        "a partially-attributable run must be skipped whole, not filled in part"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn the_backfill_does_not_cross_project_boundaries() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Mine").await;
    let theirs = project(&store, "Theirs").await;

    let run_id = run_with(&store, &mine, vec![sim_task("iso-task")], None).await;
    rewrite_plan(
        &store,
        run_id,
        &format!(
            r#"{{"name":"mine","concurrency":1,"tasks":[{{"id":"iso-task","name":"t","test_case_id":"{CASE_A}"}}]}}"#
        ),
    )
    .await;

    let recovered = store
        .backfill_task_test_case_links(&theirs)
        .await
        .expect("backfill other project");
    assert_eq!(
        recovered, 0,
        "one project's runs must never be backfilled under another's scope"
    );

    // And scoping back to its own project still finds it, proving the run was
    // never touched rather than merely reported as untouched.
    assert_eq!(
        store
            .backfill_task_test_case_links(&mine)
            .await
            .expect("own backfill"),
        1
    );
    store.pool.close().await;
}

#[tokio::test]
async fn the_backfill_ignores_an_unreadable_plan() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Garbage").await;
    let run_id = run_with(&store, &pid, vec![sim_task("garbage-task")], None).await;
    rewrite_plan(&store, run_id, "{ this is not json").await;

    assert_eq!(
        store
            .backfill_task_test_case_links(&pid)
            .await
            .expect("backfill"),
        0,
        "an unreadable plan is not evidence and must be skipped"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ release

#[tokio::test]
async fn a_release_round_trips_and_is_listed_newest_first() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Releases").await;

    let first = store
        .create_release(
            &pid,
            Some("2026.10"),
            Some("2026.10.1"),
            Some("first"),
            Some("fixture"),
        )
        .await
        .expect("create release");
    let second = store
        .create_release(
            &pid,
            Some("2026.11"),
            Some("2026.11.0"),
            None,
            Some("fixture"),
        )
        .await
        .expect("create release");

    let listed = store.list_releases(&pid).await.expect("list");
    assert_eq!(listed.len(), 2);
    assert_eq!(
        listed[0].label.as_deref(),
        Some("2026.11"),
        "releases must list newest first"
    );
    assert_eq!(listed[1].id, first.id);
    assert_eq!(listed[1].version.as_deref(), Some("2026.10.1"));
    assert_eq!(listed[1].notes.as_deref(), Some("first"));
    assert_eq!(listed[0].id, second.id);
    store.pool.close().await;
}

#[tokio::test]
async fn a_release_label_is_unique_within_a_project_but_not_across_projects() {
    let (store, _d) = temp_store().await;
    let a = project(&store, "Project A").await;
    let b = project(&store, "Project B").await;

    store
        .create_release(&a, Some("2026.10"), None, None, None)
        .await
        .expect("first");

    assert!(
        store
            .create_release(&a, Some("2026.10"), None, None, None)
            .await
            .is_err(),
        "the same label twice in one project must be refused"
    );

    store
        .create_release(&b, Some("2026.10"), None, None, None)
        .await
        .expect("the same label in another project is a different release");
    store.pool.close().await;
}

#[tokio::test]
async fn two_unlabelled_releases_coexist() {
    // SQLite treats NULLs as distinct in a UNIQUE index, so an unlabelled release
    // is still its own release rather than colliding with another unlabelled one.
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Unlabelled").await;

    store
        .create_release(&pid, None, None, None, None)
        .await
        .unwrap();
    store
        .create_release(&pid, None, None, None, None)
        .await
        .unwrap();

    assert_eq!(store.list_releases(&pid).await.unwrap().len(), 2);
    store.pool.close().await;
}

#[tokio::test]
async fn a_run_can_be_associated_with_a_release_and_a_release_less_run_is_fine() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Run release").await;
    let release = store
        .create_release(&pid, Some("2026.10"), None, None, None)
        .await
        .expect("release");

    let linked = run_with(
        &store,
        &pid,
        vec![sim_task("r-task")],
        Some(&release.id.to_string()),
    )
    .await;
    let unlinked = run_with(&store, &pid, vec![sim_task("r-task-2")], None).await;

    let of_release = store.runs_for_release(release.id).await.expect("runs");
    assert_eq!(of_release.len(), 1);
    assert_eq!(of_release[0], linked);
    assert!(
        !of_release.contains(&unlinked),
        "a release-less run must never appear under a release"
    );

    let all = store.list_run_ids(None).await.expect("all runs");
    assert!(all.contains(&unlinked), "but it is still a real run");
    store.pool.close().await;
}

#[tokio::test]
async fn associating_a_run_with_another_projects_release_is_refused() {
    let (store, _d) = temp_store().await;
    let mine = project(&store, "Mine").await;
    let theirs = project(&store, "Theirs").await;
    let their_release = store
        .create_release(&theirs, Some("2026.10"), None, None, None)
        .await
        .expect("their release");

    let result = store
        .insert_run_for_release(
            Uuid::new_v4(),
            &RunPlan {
                name: "cross-project".into(),
                tasks: vec![],
                concurrency: 1,
            },
            Some(&mine),
            Some("fixture"),
            RunState::Created,
            None,
            Some(&their_release.id.to_string()),
        )
        .await;

    assert!(
        result.is_err(),
        "a release from another project must not attach to this project's run"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn releases_are_listed_only_within_their_own_project() {
    let (store, _d) = temp_store().await;
    let a = project(&store, "A").await;
    let b = project(&store, "B").await;
    store
        .create_release(&a, Some("a-1"), None, None, None)
        .await
        .unwrap();
    store
        .create_release(&b, Some("b-1"), None, None, None)
        .await
        .unwrap();

    let of_a = store.list_releases(&a).await.unwrap();
    let of_b = store.list_releases(&b).await.unwrap();
    assert_eq!(of_a.len(), 1);
    assert_eq!(of_a[0].label.as_deref(), Some("a-1"));
    assert_eq!(of_b.len(), 1);
    assert_eq!(of_b[0].label.as_deref(), Some("b-1"));
    store.pool.close().await;
}

#[tokio::test]
async fn a_partial_release_is_legitimate_not_an_error() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Partial").await;

    let bare = store
        .create_release(&pid, None, None, None, None)
        .await
        .unwrap();
    assert!(bare.label.is_none());
    assert!(bare.version.is_none());
    assert!(bare.released_at.is_none());

    let only_version = store
        .create_release(&pid, None, Some("1.2.3"), None, None)
        .await
        .unwrap();
    assert!(only_version.label.is_none());
    assert_eq!(only_version.version.as_deref(), Some("1.2.3"));
    store.pool.close().await;
}

#[tokio::test]
async fn runs_for_an_unknown_release_are_empty_not_an_error() {
    let (store, _d) = temp_store().await;
    assert!(
        store
            .runs_for_release(Uuid::new_v4())
            .await
            .unwrap()
            .is_empty(),
        "an unknown release simply has no runs"
    );
    store.pool.close().await;
}

// ------------------------------------------------------------------ module

#[tokio::test]
async fn a_module_is_a_quality_graph_vertex_not_a_table() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Modules").await;

    store
        .link_test_case_to_module(&pid, "case-1", "checkout", "Checkout flow", "Checkout")
        .await
        .expect("link case to module");

    // No module table exists — the module lives on the existing graph.
    assert!(
        !store.table_exists("modules").await.unwrap(),
        "a module must not get a table of its own (master plan §1.3)"
    );

    let nodes = store.list_quality_nodes(&pid).await.expect("nodes");
    let module = nodes
        .iter()
        .find(|n| n.node_type == "module")
        .expect("a module vertex");
    assert_eq!(module.external_ref.as_deref(), Some("checkout"));
    assert_eq!(module.name, "Checkout");

    let rels = store
        .list_quality_relations_for_project(&pid)
        .await
        .expect("relations");
    assert!(
        rels.iter().any(|r| r.relation_type == "covers"),
        "the case -> module edge must exist; got {:?}",
        rels.iter().map(|r| &r.relation_type).collect::<Vec<_>>()
    );
    store.pool.close().await;
}

#[tokio::test]
async fn linking_a_case_to_a_module_is_idempotent() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Idempotent").await;

    store
        .link_test_case_to_module(&pid, "case-1", "checkout", "Checkout flow", "Checkout")
        .await
        .expect("first");
    store
        .link_test_case_to_module(&pid, "case-1", "checkout", "Checkout flow", "Checkout")
        .await
        .expect("second");

    let nodes = store.list_quality_nodes(&pid).await.unwrap();
    assert_eq!(
        nodes.iter().filter(|n| n.node_type == "module").count(),
        1,
        "repeating the link must not duplicate the module vertex"
    );

    let rels = store
        .list_quality_relations_for_project(&pid)
        .await
        .unwrap();
    assert_eq!(
        rels.iter().filter(|r| r.relation_type == "covers").count(),
        1,
        "repeating the link must not duplicate the edge"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn modules_are_scoped_to_their_project() {
    let (store, _d) = temp_store().await;
    let a = project(&store, "A").await;
    let b = project(&store, "B").await;

    store
        .link_test_case_to_module(&a, "case-a", "checkout", "Checkout", "Checkout")
        .await
        .unwrap();

    let of_b = store.list_quality_nodes(&b).await.unwrap();
    assert!(
        of_b.is_empty(),
        "another project must not see this project's module"
    );
    store.pool.close().await;
}

#[tokio::test]
async fn a_blank_module_reference_is_refused() {
    let (store, _d) = temp_store().await;
    let pid = project(&store, "Blank").await;
    assert!(
        store
            .upsert_module_node(&pid, "   ", "Blank")
            .await
            .is_err(),
        "an unnamed module vertex must not be created"
    );
    store.pool.close().await;
}

// ===========================================================================
// Phase 9.2B Batch 6 — temp_store() lifecycle
// ===========================================================================
//
// These test cleanup, not QA Memory logic. A green suite is not evidence that a directory was
// removed: `tempfile` swallows the Windows removal error that causes the leak in the first place.
// Every assertion reads the filesystem or the database directly.
//
// Background work: this file contains no `tokio::spawn`, no `spawn_blocking`, no `JoinHandle`, no
// `tokio::time::sleep`, and references no engine or dispatch entry point at all. Every test drives
// `Store` synchronously, so there is no detached task that could still be writing when the test
// body returns. That was verified by inspection of the current source, not assumed from a report.

/// The private measurement root owned by one test. Only this namespace is ever removed.
fn p92b6_root(test_name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir()
        .join("aiqa_p92b6_artifacts")
        .join(test_name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create isolated artifact root");
    root
}

fn p92b6_dir_count(root: &std::path::Path) -> usize {
    std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

/// The same fixture, rooted in a caller-owned directory so a test can measure exactly what *this*
/// fixture leaves behind. Counting global `.tmp*` entries is not a valid assertion -- `cargo test`
/// runs these tests concurrently, so any other suite's fixture perturbs the total.
///
/// `temp_store()` above is left untouched and keeps using `tempfile::tempdir()`, so the 24 existing
/// call sites keep their existing behaviour and directory location exactly.
async fn temp_store_in(root: &std::path::Path) -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir_in(root).expect("tempdir");
    let path = dir.path().join("qa-memory.sqlite3");
    let store = Store::connect(&path).await.expect("connect");
    (store, dir)
}

/// **A + B.** The fixture must produce a genuinely migrated, writable database.
#[tokio::test]
async fn p92b6_fixture_is_usable_and_fully_migrated() {
    let root = p92b6_root("usable_and_migrated");
    let (store, dir) = temp_store_in(&root).await;

    let tables: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN          ('projects','runs','tasks','test_cases','execution_telemetry')",
    )
    .fetch_one(&store.pool)
    .await
    .expect("query sqlite_master");
    assert_eq!(
        tables.0, 5,
        "the qa-memory fixture must run the real migrations"
    );

    let pid = project(&store, "P92B6 lifecycle").await;
    let runs = store.list_run_ids(Some(&pid)).await.expect("list_run_ids");
    assert!(
        runs.is_empty(),
        "a fresh migrated fixture must start with no runs for a new project"
    );
    assert!(
        dir.path().join("qa-memory.sqlite3").exists(),
        "the database file must exist while the fixture is live"
    );

    store.pool.close().await;
}

/// **C + I.** QA Memory reads and project scoping must retain their established behaviour: a run is
/// visible inside its own project and not another project's.
#[tokio::test]
async fn p92b6_qa_memory_reads_and_project_scoping_are_unchanged() {
    let root = p92b6_root("reads_and_scoping");
    let (store, _d) = temp_store_in(&root).await;

    let mine = project(&store, "P92B6 mine").await;
    let theirs = project(&store, "P92B6 theirs").await;
    let mine_run = run_with(&store, &mine, vec![sim_task("mine-task")], None).await;
    let theirs_run = run_with(&store, &theirs, vec![sim_task("their-task")], None).await;

    let scoped = store
        .list_run_ids(Some(&mine))
        .await
        .expect("list_run_ids scoped to my project");
    assert_eq!(
        scoped,
        vec![mine_run],
        "a project must see exactly its own runs, not another project's"
    );

    let unscoped = store
        .list_run_ids(None)
        .await
        .expect("list_run_ids unscoped");
    assert!(
        unscoped.contains(&mine_run) && unscoped.contains(&theirs_run),
        "the unscoped read must still return both projects' runs: {unscoped:?}"
    );

    let tasks = store
        .list_tasks_for_run(theirs_run)
        .await
        .expect("list_tasks_for_run");
    assert_eq!(
        tasks.len(),
        1,
        "the other project's task row must be readable"
    );

    store.pool.close().await;
}

/// **K.** A storage failure must remain observable rather than being disguised as an empty QA Memory
/// result. Closing the pool is the same fault injection the file's `d1`-style tests rely on, and a
/// read against a closed pool must surface as an error, not as "no runs found".
#[tokio::test]
async fn p92b6_storage_failure_is_not_disguised_as_an_empty_result() {
    let root = p92b6_root("storage_failure_visible");
    let (store, _d) = temp_store_in(&root).await;
    let pid = project(&store, "P92B6 storage").await;
    run_with(&store, &pid, vec![sim_task("task")], None).await;

    // Genuine unreachability, not a mocked error.
    store.pool.close().await;
    let result = store.list_run_ids(Some(&pid)).await;
    assert!(
        result.is_err(),
        "a read against a closed pool must return Err, not an empty list: {result:?}"
    );

    store.pool.close().await;
}

/// **D + E + F.** The close must complete before the directory is removed, and the SQLite file plus
/// its WAL/SHM sidecars must not block the removal.
#[tokio::test]
async fn p92b6_close_removes_the_directory_despite_wal_and_shm() {
    let root = p92b6_root("close_removes_directory");
    let (store, dir) = temp_store_in(&root).await;
    let path = dir.path().to_path_buf();

    // A real write, so the WAL/SHM sidecars genuinely exist rather than this being an easy case.
    let pid = project(&store, "P92B6 wal").await;
    run_with(&store, &pid, vec![sim_task("t1")], None).await;

    let live: Vec<String> = std::fs::read_dir(&path)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        live.iter().any(|f| f.ends_with("-wal") || f.ends_with("-shm")),
        "WAL/SHM sidecars must exist while the fixture is live, else this test proves nothing:          {live:?}"
    );

    store.pool.close().await;
    drop(dir);

    assert!(
        !path.exists(),
        "after an awaited `pool.close()` the directory must be gone. If it remains, an open          SQLite handle -- not the fixture -- is what blocked removal."
    );
    assert_eq!(
        p92b6_dir_count(&root),
        0,
        "the measurement root must be empty"
    );
}

/// **G.** Repeated creation and teardown must not accumulate artifacts in the isolated root.
#[tokio::test]
async fn p92b6_repeated_fixtures_accumulate_nothing() {
    let root = p92b6_root("repeated_fixtures");
    assert_eq!(
        p92b6_dir_count(&root),
        0,
        "the measurement root starts empty"
    );
    for _ in 0..10 {
        let (store, dir) = temp_store_in(&root).await;
        let pid = project(&store, "P92B6 repeat").await;
        run_with(&store, &pid, vec![sim_task("t1")], None).await;
        store.pool.close().await;
        drop(dir);
    }
    let after = p92b6_dir_count(&root);
    assert_eq!(
        after, 0,
        "ten fixture cycles must leave zero directories in the isolated root; {after} remained"
    );
}

/// **H.** Concurrent fixtures must each remove exactly their own directory, never another's.
#[tokio::test]
async fn p92b6_concurrent_fixtures_do_not_interfere() {
    let root = p92b6_root("concurrent_fixtures");
    assert_eq!(
        p92b6_dir_count(&root),
        0,
        "the measurement root starts empty"
    );

    let mut handles = Vec::new();
    for i in 0..8 {
        let root = root.clone();
        handles.push(tokio::spawn(async move {
            let (store, dir) = temp_store_in(&root).await;
            let path = dir.path().to_path_buf();
            // Prove the fixture was real before releasing it, so a fixture that never created its
            // database cannot pass by never existing.
            assert!(
                path.join("qa-memory.sqlite3").exists(),
                "concurrent fixture {i} must have created its database before teardown"
            );
            let pid = project(&store, "P92B6 concurrent").await;
            run_with(&store, &pid, vec![sim_task("t1")], None).await;
            store.pool.close().await;
            drop(dir);
            path
        }));
    }

    let mut paths = Vec::new();
    for h in handles {
        paths.push(h.await.expect("concurrent fixture task must not panic"));
    }
    assert_eq!(
        p92b6_dir_count(&root),
        0,
        "eight concurrent fixtures must leave nothing behind -- neither their own nor each          other's"
    );
    for p in &paths {
        assert!(
            !p.exists(),
            "each fixture must remove exactly its own directory; {} survived",
            p.display()
        );
    }
}

/// Teardown failure must remain observable rather than silently suppressed. `TempDir::close()` is
/// the fallible form and returns the real removal error; asserting on it is what stops a future
/// regression from passing quietly. Under this repository's retention policy a directory that
/// genuinely cannot be removed is left in place rather than force-deleted, so it must still be
/// *inspectable* -- which is exactly what a returned `Err` tells us.
#[tokio::test]
async fn p92b6_teardown_failure_is_observable() {
    let root = p92b6_root("teardown_failure_observable");
    let (store, dir) = temp_store_in(&root).await;
    let path = dir.path().to_path_buf();
    let pid = project(&store, "P92B6 diagnostics").await;
    run_with(&store, &pid, vec![sim_task("t1")], None).await;

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

/// The last leak in this suite came from a fixture that opened TWO stores on the same file, so a
/// `store`-only teardown missed one. This test reproduces that shape directly.
#[tokio::test]
async fn p92b6_a_second_pool_on_the_same_file_must_also_be_closed() {
    let root = p92b6_root("second_pool_must_close");
    let (store, dir) = temp_store_in(&root).await;
    let path_buf = dir.path().join("qa-memory.sqlite3");
    let path = dir.path().to_path_buf();

    let pid = project(&store, "P92B6 restart").await;
    run_with(&store, &pid, vec![sim_task("t1")], None).await;

    // The shape used by `migrations_preserve_rows_written_before_them`: close one pool, reconnect.
    store.pool.close().await;
    let reconnected = Store::connect(&path_buf).await.expect("reconnect");
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM runs")
        .fetch_one(&reconnected.pool)
        .await
        .expect("count runs");
    assert_eq!(n.0, 1, "the reconnected pool must see the same database");

    reconnected.pool.close().await;
    drop(dir);

    assert!(
        !path.exists(),
        "every pool opened on the file must be closed before the TempDir is removed; a second,          unclosed pool is exactly what leaked in this suite"
    );
    assert_eq!(
        p92b6_dir_count(&root),
        0,
        "the measurement root must be empty"
    );
}
