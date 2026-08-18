//! Phase 1 regression tests for `engine.rs`.
//!
//! Each test was written to FAIL against the pre-Phase-1 code. They pin
//! B2 (recovery state derivation), B4 (task start timing) and B5 (executor
//! timeout) — the three defects that live in the async driver rather than
//! in the pure scheduler, which is why none of the existing 12 tests caught
//! them.

use crate::domain::{RunPlan, RunState, SimOutcome, SimSpec, TaskDef, TaskStatus};
use crate::engine::{approve_and_spawn, recover_incomplete_runs, reject_run, spawn_run, RunRecord};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutionOutcome, Executor, ExecutorRegistry, SimulatedExecutor};
use crate::store::Store;
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("engine.sqlite3")).await.expect("connect");
    (store, dir)
}

fn evidence_store(dir: &tempfile::TempDir) -> Arc<LocalEvidenceStore> {
    Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence store"))
}

fn sim_task(id: &str, outcome: SimOutcome, delay_ms: u64) -> TaskDef {
    TaskDef {
        id: id.into(),
        name: id.into(),
        prerequisites: vec![],
        simulate: Some(SimSpec { outcome, delay_ms }),
        ..Default::default()
    }
}

fn plan(name: &str, tasks: Vec<TaskDef>, concurrency: usize) -> RunPlan {
    RunPlan { name: name.into(), tasks, concurrency }
}

/// Drives a run to completion and returns the final persisted summary.
async fn run_to_completion(
    store: &Store,
    evidence: Arc<LocalEvidenceStore>,
    registry: Arc<ExecutorRegistry>,
    plan: RunPlan,
    task_timeout: std::time::Duration,
) -> Uuid {
    let run_id = Uuid::new_v4();
    store
        .insert_run(run_id, &plan, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");
    let record = RunRecord::new(run_id, &plan, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);
    let concurrency = plan.concurrency;
    spawn_run(record.clone(), registry, concurrency, store.clone(), evidence, task_timeout);

    // Poll until terminal rather than sleeping a fixed amount.
    for _ in 0..300 {
        if record.engine.read().await.is_finished() {
            // Give the finalisation tail (state write, RunCompleted) a moment.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    run_id
}

// -------------------------------------------------------------------- B2

/// B2: a run whose tasks all PASSED but whose run-level completion write was
/// lost must recover as COMPLETED. Pre-fix, `recover_incomplete_runs` used
/// `if touched_any { Partial } else { Failed }` — so exactly this case (no
/// task needed forcing, only the run row was stale) became FAILED.
#[tokio::test]
async fn b2_recovery_of_all_passed_run_yields_completed_not_failed() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();

    let p = plan("all-passed", vec![sim_task("t1", SimOutcome::Pass, 0), sim_task("t2", SimOutcome::Pass, 0)], 2);
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Running, None)
        .await
        .expect("insert");

    // Every task finished successfully...
    for id in ["t1", "t2"] {
        store.set_task_status(run_id, id, TaskStatus::Running, None, true, false).await.unwrap();
        store.set_task_status(run_id, id, TaskStatus::Passed, None, false, true).await.unwrap();
    }
    // ...but the run-level completion write never landed (the B1 scenario),
    // so the run is still RUNNING in the database.

    let recovered = recover_incomplete_runs(&store).await.expect("recovery");
    assert_eq!(recovered, 1, "the stale run must be picked up by recovery");

    let summary = store.get_run_summary(run_id).await.expect("summary").expect("run exists");
    assert_eq!(
        summary.state,
        RunState::Completed,
        "a run whose tasks all passed must recover as COMPLETED, never FAILED"
    );
    assert_eq!(summary.passed, 2, "task states must be preserved by recovery");
}

/// B2: a genuinely interrupted run (tasks left QUEUED/RUNNING) must still be
/// marked as an infrastructure outcome, and its already-terminal tasks must
/// be preserved.
#[tokio::test]
async fn b2_recovery_marks_orphaned_tasks_and_preserves_terminal_ones() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();

    let p = plan(
        "interrupted",
        vec![
            sim_task("done", SimOutcome::Pass, 0),
            sim_task("orphan", SimOutcome::Pass, 0),
        ],
        2,
    );
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Running, None)
        .await
        .expect("insert");

    store.set_task_status(run_id, "done", TaskStatus::Running, None, true, false).await.unwrap();
    store.set_task_status(run_id, "done", TaskStatus::Passed, None, false, true).await.unwrap();
    // "orphan" is left RUNNING — the process died mid-execution.
    store.set_task_status(run_id, "orphan", TaskStatus::Running, None, true, false).await.unwrap();

    recover_incomplete_runs(&store).await.expect("recovery");

    let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
    let done = tasks.iter().find(|t| t.id == "done").expect("done task");
    let orphan = tasks.iter().find(|t| t.id == "orphan").expect("orphan task");

    assert_eq!(done.status, TaskStatus::Passed, "an already-terminal task must not be rewritten");
    assert_eq!(
        orphan.status,
        TaskStatus::InfraError,
        "a task left RUNNING by an unclean shutdown is an infrastructure outcome"
    );

    let summary = store.get_run_summary(run_id).await.expect("summary").expect("run");
    assert_eq!(
        summary.state,
        RunState::Partial,
        "a run with an infrastructure outcome recovers as PARTIAL"
    );
}

/// B2: a run containing a real product failure must recover as FAILED, not
/// be flattened into COMPLETED.
#[tokio::test]
async fn b2_recovery_of_run_with_product_failure_yields_failed() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();

    let p = plan("has-failure", vec![sim_task("ok", SimOutcome::Pass, 0), sim_task("bad", SimOutcome::Fail, 0)], 2);
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Running, None)
        .await
        .expect("insert");

    store.set_task_status(run_id, "ok", TaskStatus::Running, None, true, false).await.unwrap();
    store.set_task_status(run_id, "ok", TaskStatus::Passed, None, false, true).await.unwrap();
    store.set_task_status(run_id, "bad", TaskStatus::Running, None, true, false).await.unwrap();
    store.set_task_status(run_id, "bad", TaskStatus::Failed, Some("assert"), false, true).await.unwrap();

    recover_incomplete_runs(&store).await.expect("recovery");

    let summary = store.get_run_summary(run_id).await.expect("summary").expect("run");
    assert_eq!(
        summary.state,
        RunState::Failed,
        "a recovered run containing a product failure must be FAILED"
    );
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.passed, 1);
}

// -------------------------------------------------------------------- B4

/// B4: `started_at` must reflect when a task actually began executing, not
/// when the scheduler selected it. Pre-fix every eligible task was marked
/// started in one batch before any acquired a concurrency permit, so with
/// concurrency=1 all four tasks reported the same start instant and
/// durations that included their queue wait.
#[tokio::test]
async fn b4_started_at_reflects_real_execution_start_under_concurrency_1() {
    let (store, dir) = temp_store().await;
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));

    const DELAY_MS: u64 = 300;
    let tasks: Vec<TaskDef> = (0..4)
        .map(|i| sim_task(&format!("s{i}"), SimOutcome::Pass, DELAY_MS))
        .collect();

    let run_id = run_to_completion(
        &store,
        evidence_store(&dir),
        registry,
        plan("serial", tasks, 1), // concurrency = 1 => strictly serialised
        std::time::Duration::from_secs(30),
    )
    .await;

    let mut persisted = store.list_tasks_for_run(run_id).await.expect("tasks");
    persisted.sort_by_key(|t| t.started_at);

    let starts: Vec<_> = persisted.iter().filter_map(|t| t.started_at).collect();
    assert_eq!(starts.len(), 4, "every task must have a start time");

    let spread_ms = (starts[3] - starts[0]).num_milliseconds();
    assert!(
        spread_ms >= (3 * DELAY_MS as i64) / 2,
        "with concurrency=1 and {DELAY_MS}ms tasks, starts must be staggered across the run \
         (expected >= {}ms spread, got {spread_ms}ms)",
        (3 * DELAY_MS) / 2
    );

    // And each task's own duration must be close to its real work, not
    // inflated by time spent waiting for a permit.
    for t in &persisted {
        let (Some(s), Some(f)) = (t.started_at, t.finished_at) else {
            panic!("task {} missing timing", t.id)
        };
        let dur = (f - s).num_milliseconds();
        assert!(
            dur < (DELAY_MS as i64) * 3,
            "task {} reported {dur}ms for {DELAY_MS}ms of work — queue wait leaked into the duration",
            t.id
        );
    }
}

// -------------------------------------------------------------------- B5

/// An executor that never returns, to prove the timeout path.
struct HangingExecutor;

#[async_trait]
impl Executor for HangingExecutor {
    async fn execute(&self, _task: &TaskDef) -> ExecutionOutcome {
        // Far longer than any test timeout; the engine must cut this off.
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        unreachable!("the engine must time this out before it ever returns")
    }
}

/// B5: a hung executor must time out, become TIMEOUT, release its permit and
/// let the rest of the run proceed. Pre-fix there was no timeout anywhere in
/// `executor.rs`, so this run would hang forever and the permit would never
/// be released.
#[tokio::test]
async fn b5_hung_executor_times_out_releases_permit_and_run_continues() {
    let (store, dir) = temp_store().await;

    let registry = Arc::new(
        ExecutorRegistry::new(Arc::new(SimulatedExecutor)).register("hang", Arc::new(HangingExecutor)),
    );

    let hanging = TaskDef {
        id: "hang".into(),
        name: "hanging-task".into(),
        prerequisites: vec![],
        simulate: None,
        executor: Some("hang".into()),
        ..Default::default()
    };
    // concurrency = 1 so the hung task must release its permit before the
    // second task can run at all.
    let p = plan("timeout", vec![hanging, sim_task("after", SimOutcome::Pass, 0)], 1);

    let run_id = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        // A short ceiling so the hung executor is cut off quickly; the
        // point under test is that the timeout fires at all.
        run_to_completion(&store, evidence_store(&dir), registry, p, std::time::Duration::from_secs(2)),
    )
    .await
    .expect("the run must finish; if this times out the executor timeout is not working");

    let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
    let hung = tasks.iter().find(|t| t.id == "hang").expect("hanging task");
    let after = tasks.iter().find(|t| t.id == "after").expect("subsequent task");

    assert_eq!(hung.status, TaskStatus::Timeout, "a hung executor must be recorded as TIMEOUT");
    assert_eq!(
        after.status,
        TaskStatus::Passed,
        "the permit must be released so unrelated work still runs (continue-on-failure)"
    );
}

// -------------------------------------------------------------------- D1

/// D1: when approval persistence fails, the error must NOT be swallowed,
/// the in-memory record must NOT be marked approved, the run must NOT be
/// spawned, and the failure must be recorded on the record.
#[tokio::test]
async fn d1_approval_persistence_failure_is_propagated_and_does_not_spawn_or_approve_in_memory() {
    let (store, dir) = temp_store().await;
    let run_id = Uuid::new_v4();
    let p = plan("gated-approval-fail", vec![sim_task("t1", SimOutcome::Pass, 0)], 1);

    store
        .insert_run(run_id, &p, Some("p1"), Some("requester"), RunState::PendingApproval, Some("held for review"))
        .await
        .expect("insert_run");

    let record = RunRecord::new(
        run_id,
        &p,
        Some("p1".into()),
        Some("requester".into()),
        RunState::PendingApproval,
        Some("held for review".into()),
    );
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    // Induce a genuine persistence failure by closing the store's connection pool.
    store.pool.close().await;

    let res = approve_and_spawn(
        record.clone(),
        registry,
        p.concurrency,
        store.clone(),
        evidence,
        "approver_alice",
        std::time::Duration::from_secs(10),
    )
    .await;

    assert!(res.is_err(), "approve_and_spawn must return Err when DB update fails");
    let failure_msg = record.persistence_error().await;
    assert!(
        failure_msg.is_some(),
        "persistence failure must be recorded on the run record via note_persistence_failure"
    );
    assert!(
        failure_msg.unwrap().contains("approve_run"),
        "recorded failure must identify the approve_run context"
    );

    // In-memory state must NOT be treated as approved.
    assert_eq!(
        *record.approved_by.read().await,
        None,
        "approved_by must remain None when persistence fails"
    );
    assert_eq!(
        *record.approved_at.read().await,
        None,
        "approved_at must remain None when persistence fails"
    );
    assert_eq!(
        *record.state.read().await,
        RunState::PendingApproval,
        "state must remain PendingApproval when persistence fails"
    );

    // No tasks should have been started.
    let engine = record.engine.read().await;
    assert_eq!(
        engine.tasks[&"t1".to_string()].status,
        TaskStatus::Queued,
        "tasks must remain Queued; execution must not be spawned on approval failure"
    );
}

/// D1: when rejection persistence fails, the error must NOT be swallowed,
/// the in-memory record must NOT transition to Rejected, and no RunCompleted
/// event should be emitted.
#[tokio::test]
async fn d1_rejection_persistence_failure_is_propagated_and_does_not_mutate_in_memory_state() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();
    let p = plan("gated-rejection-fail", vec![sim_task("t1", SimOutcome::Pass, 0)], 1);

    store
        .insert_run(run_id, &p, Some("p1"), Some("requester"), RunState::PendingApproval, Some("held for review"))
        .await
        .expect("insert_run");

    let record = RunRecord::new(
        run_id,
        &p,
        Some("p1".into()),
        Some("requester".into()),
        RunState::PendingApproval,
        Some("held for review".into()),
    );

    // Induce a genuine persistence failure by closing the store's connection pool.
    store.pool.close().await;

    let res = reject_run(record.clone(), &store, "rejector_bob", "security policy breach").await;

    assert!(res.is_err(), "reject_run must return Err when DB update fails");
    let failure_msg = record.persistence_error().await;
    assert!(
        failure_msg.is_some(),
        "persistence failure must be recorded on the run record via note_persistence_failure"
    );
    assert!(
        failure_msg.unwrap().contains("reject_run"),
        "recorded failure must identify the reject_run context"
    );

    // In-memory state must NOT be mutated to Rejected.
    assert_eq!(
        *record.state.read().await,
        RunState::PendingApproval,
        "state must remain PendingApproval when rejection persistence fails"
    );
    assert_eq!(
        *record.finished_at.read().await,
        None,
        "finished_at must remain None when persistence fails"
    );
    assert_eq!(
        *record.approved_by.read().await,
        None,
        "approved_by must remain None when rejection persistence fails"
    );
    assert_eq!(
        *record.policy_notes.read().await,
        Some("held for review".to_string()),
        "policy_notes must not be overwritten with the rejection reason on persistence failure"
    );
    assert_eq!(
        record.events.read().await.len(),
        0,
        "no RunCompleted event should be emitted when rejection persistence fails"
    );
}

/// D1: successful approval persists durably and spawns execution.
#[tokio::test]
async fn d1_successful_approval_persists_and_spawns() {
    let (store, dir) = temp_store().await;
    let run_id = Uuid::new_v4();
    let p = plan("gated-success", vec![sim_task("t1", SimOutcome::Pass, 0)], 1);

    store
        .insert_run(run_id, &p, Some("p1"), Some("requester"), RunState::PendingApproval, Some("held for review"))
        .await
        .expect("insert_run");

    let record = RunRecord::new(
        run_id,
        &p,
        Some("p1".into()),
        Some("requester".into()),
        RunState::PendingApproval,
        Some("held for review".into()),
    );
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    let res = approve_and_spawn(
        record.clone(),
        registry,
        p.concurrency,
        store.clone(),
        evidence,
        "approver_alice",
        std::time::Duration::from_secs(10),
    )
    .await;

    assert!(res.is_ok(), "approve_and_spawn must succeed when DB write succeeds");
    assert_eq!(
        *record.approved_by.read().await,
        Some("approver_alice".to_string())
    );
    assert!(record.approved_at.read().await.is_some());

    // Wait for the background run to finish
    for _ in 0..100 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let summary = store.get_run_summary(run_id).await.expect("summary").expect("run exists");
    assert_eq!(summary.approved_by, Some("approver_alice".to_string()));
    assert!(summary.approved_at.is_some());
}

/// D1: successful rejection persists durably and updates in-memory state.
#[tokio::test]
async fn d1_successful_rejection_persists_and_mutates_state() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();
    let p = plan("gated-reject-success", vec![sim_task("t1", SimOutcome::Pass, 0)], 1);

    store
        .insert_run(run_id, &p, Some("p1"), Some("requester"), RunState::PendingApproval, Some("held for review"))
        .await
        .expect("insert_run");

    let record = RunRecord::new(
        run_id,
        &p,
        Some("p1".into()),
        Some("requester".into()),
        RunState::PendingApproval,
        Some("held for review".into()),
    );

    let res = reject_run(record.clone(), &store, "rejector_bob", "policy violation").await;

    assert!(res.is_ok(), "reject_run must succeed when DB write succeeds");
    assert_eq!(*record.state.read().await, RunState::Rejected);
    assert_eq!(*record.approved_by.read().await, Some("rejector_bob".to_string()));
    assert_eq!(*record.policy_notes.read().await, Some("policy violation".to_string()));
    assert_eq!(record.events.read().await.len(), 1);

    let summary = store.get_run_summary(run_id).await.expect("summary").expect("run exists");
    assert_eq!(summary.state, RunState::Rejected);
    assert_eq!(summary.approved_by, Some("rejector_bob".to_string()));
    assert_eq!(summary.policy_notes, Some("policy violation".to_string()));
}

// -------------------------------------------------------------------- D2

/// D2: When a durable persistence failure occurs during run execution, the
/// failure reason recorded in `policy_notes` must be persisted into SQLite
/// with the terminal FAILED state. After orchestrator restart, the failure
/// reason must be restored from disk rather than being lost as NULL.
#[tokio::test]
async fn d2_persistence_failure_reason_is_durable_and_survives_restart() {
    let (store, dir) = temp_store().await;
    let db_path = dir.path().join("engine.sqlite3");
    let run_id = Uuid::new_v4();

    let p = plan("persistence-fail-durable", vec![sim_task("t1", SimOutcome::Pass, 10)], 1);

    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");

    let record = RunRecord::new(
        run_id,
        &p,
        Some("p1".into()),
        Some("tester".into()),
        RunState::Scheduled,
        None,
    );
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    // Inject a genuine SQLite write failure on event sequence 1.
    sqlx::query(
        "CREATE TRIGGER fail_event_persistence BEFORE INSERT ON run_events WHEN NEW.sequence = 1 BEGIN SELECT RAISE(ABORT, 'injected disk write failure on event sequence 1'); END;",
    )
    .execute(&store.pool)
    .await
    .expect("create trigger");

    spawn_run(
        record.clone(),
        registry,
        p.concurrency,
        store.clone(),
        evidence,
        std::time::Duration::from_secs(10),
    );

    // Wait for the run loop to finish
    for _ in 0..100 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // 1. In-memory verification
    let memory_summary = record.summary().await;
    assert_eq!(
        memory_summary.state,
        RunState::Failed,
        "run with persistence failure must be marked FAILED in memory"
    );
    assert!(
        memory_summary.policy_notes.is_some(),
        "in-memory policy_notes must contain failure diagnostic"
    );
    let memory_reason = memory_summary.policy_notes.clone().unwrap();
    assert!(
        memory_reason.contains("injected disk write failure on event sequence 1"),
        "in-memory note must describe the actual injected failure"
    );

    // 2. Direct database inspection BEFORE restart
    let db_summary = store
        .get_run_summary(run_id)
        .await
        .expect("get_run_summary")
        .expect("run exists in db");
    assert_eq!(
        db_summary.state,
        RunState::Failed,
        "database run state must be FAILED"
    );
    assert_eq!(
        db_summary.policy_notes,
        Some(memory_reason.clone()),
        "database policy_notes must match in-memory policy_notes exactly (memory reason == database reason)"
    );

    // 3. Simulate orchestrator process restart: close old store and connect fresh
    store.pool.close().await;
    let restarted_store = Store::connect(&db_path).await.expect("reconnect to db");

    // Run startup recovery
    let _ = recover_incomplete_runs(&restarted_store).await.expect("recover_incomplete_runs");

    // 4. Verify durable persistence survived restart
    let recovered_summary = restarted_store
        .get_run_summary(run_id)
        .await
        .expect("get_run_summary after restart")
        .expect("run exists in restarted db");

    assert_eq!(
        recovered_summary.state,
        RunState::Failed,
        "recovered run state must remain FAILED after restart"
    );
    assert_eq!(
        recovered_summary.policy_notes,
        Some(memory_reason.clone()),
        "recovered policy_notes after restart must match original failure reason (recovered reason == database reason)"
    );
    assert_eq!(
        recovered_summary.policy_notes,
        db_summary.policy_notes,
        "recovered reason must be identical to pre-restart database reason"
    );
}

// -------------------------------------------------------------------- D3

/// D3: a task's own terminal-status UPDATE can fail durably (distinct from
/// D2, where only an *event* insert failed) while the run it belongs to
/// still gets escalated to FAILED by B1/D2. Before this fix,
/// `list_incomplete_runs` selected runs purely by the RUN row's own state,
/// so once the run was durably FAILED it was permanently invisible to
/// `recover_incomplete_runs` — leaving "target" stuck at RUNNING in the
/// database forever, even across restarts.
#[tokio::test]
async fn d3_orphaned_task_under_terminal_run_is_repaired_by_recovery() {
    let (store, dir) = temp_store().await;
    let db_path = dir.path().join("engine.sqlite3");
    let run_id = Uuid::new_v4();

    let p = plan(
        "task-write-fails",
        vec![sim_task("ok", SimOutcome::Pass, 0), sim_task("target", SimOutcome::Pass, 10)],
        2,
    );
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");

    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    // Inject a genuine SQLite write failure on "target"'s terminal-status
    // UPDATE only. Its start-of-execution UPDATE never touches
    // `finished_at`, so the WHEN clause leaves that write alone and the
    // task genuinely starts and genuinely finishes in memory — only the
    // durable write recording that outcome fails.
    sqlx::query(
        "CREATE TRIGGER fail_target_finish BEFORE UPDATE ON tasks \
         WHEN NEW.id = 'target' AND NEW.finished_at IS NOT NULL \
         BEGIN SELECT RAISE(ABORT, 'injected disk write failure finishing target'); END;",
    )
    .execute(&store.pool)
    .await
    .expect("create trigger");

    spawn_run(
        record.clone(),
        registry,
        p.concurrency,
        store.clone(),
        evidence,
        std::time::Duration::from_secs(10),
    );

    for _ in 0..200 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // In memory, "target" reached a genuine terminal outcome even though its
    // durable write failed — that gap between memory and disk is the root
    // of the defect.
    {
        let engine = record.engine.read().await;
        assert_eq!(engine.tasks[&"target".to_string()].status, TaskStatus::Passed);
    }

    let memory_summary = record.summary().await;
    assert_eq!(memory_summary.state, RunState::Failed, "the run must be escalated per B1/D2");
    assert!(
        memory_summary
            .policy_notes
            .as_deref()
            .unwrap_or("")
            .contains("injected disk write failure finishing target"),
        "policy_notes must describe the actual injected failure"
    );

    // Confirm the defect directly against the database, matching the live
    // reproduction: the run is durably terminal (FAILED, correct per B1/D2)
    // but "target" never made it out of RUNNING.
    let db_summary_before = store.get_run_summary(run_id).await.expect("summary").expect("exists");
    assert_eq!(db_summary_before.state, RunState::Failed, "run must already be durably FAILED (B1/D2, unaffected by D3)");

    let tasks_before = store.list_tasks_for_run(run_id).await.expect("tasks");
    let target_before = tasks_before.iter().find(|t| t.id == "target").expect("target");
    assert_eq!(
        target_before.status,
        TaskStatus::Running,
        "the task's terminal write failed, so before repair it must still read RUNNING on disk"
    );

    // The fault was transient (in the live system: a lock held for a few
    // seconds) — drop it before simulating the restart, otherwise recovery's
    // own repair write would hit the same trigger.
    sqlx::query("DROP TRIGGER fail_target_finish")
        .execute(&store.pool)
        .await
        .expect("drop trigger");

    // Simulate an orchestrator process restart: close this connection pool,
    // reconnect fresh, and run the exact startup recovery pass.
    store.pool.close().await;
    let restarted_store = Store::connect(&db_path).await.expect("reconnect");
    let recovered_count = recover_incomplete_runs(&restarted_store).await.expect("recovery");

    assert_eq!(
        recovered_count, 1,
        "recovery must find this run even though its OWN row is already terminal (FAILED), \
         because one of its tasks is still stuck RUNNING"
    );

    // Task integrity: the orphan must not remain RUNNING, and the
    // already-correct task must not be rewritten.
    let tasks_after = restarted_store.list_tasks_for_run(run_id).await.expect("tasks");
    let target_after = tasks_after.iter().find(|t| t.id == "target").expect("target");
    assert_ne!(
        target_after.status,
        TaskStatus::Running,
        "recovery must repair the orphaned task rather than leaving it RUNNING forever"
    );
    assert_eq!(
        target_after.status,
        TaskStatus::InfraError,
        "an orphaned task's true durable outcome is unknown, so recovery marks it INFRA_ERROR \
         — the exact vocabulary already used for every other orphan-recovery case (B2)"
    );
    let ok_after = tasks_after.iter().find(|t| t.id == "ok").expect("ok");
    assert_eq!(ok_after.status, TaskStatus::Passed, "an already-correct terminal task must not be rewritten");

    // Run integrity: durable run state must be consistent with the
    // now-durable task states, via the same `derive_run_state` recovery
    // already uses for every other case (B2) — no new state-transition rule.
    let recovered_summary = restarted_store.get_run_summary(run_id).await.expect("summary").expect("exists");
    assert_eq!(
        recovered_summary.state,
        RunState::Partial,
        "one PASSED + one INFRA_ERROR task must derive PARTIAL, consistent with derive_run_state"
    );
    // D2 must not regress: the original diagnostic is preserved verbatim.
    assert_eq!(
        recovered_summary.policy_notes, db_summary_before.policy_notes,
        "the original persistence-failure diagnostic must survive this second recovery pass (D2 must not regress)"
    );

    // Event integrity: recovery's own reconciliation event must be durably
    // recorded (not silently dropped), and no existing sequence was
    // duplicated or renumbered by the repair.
    let events_after = restarted_store.list_events_since(run_id, 0).await.expect("events");
    let mut seqs: Vec<u64> = events_after.iter().map(|e| e.sequence).collect();
    let before_dedup = seqs.len();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(seqs.len(), before_dedup, "no duplicate/renumbered event sequences were introduced by recovery");
    assert!(
        events_after
            .iter()
            .any(|e| e.event_type == crate::domain::EventType::RunCompleted
                && e.summary.as_deref().unwrap_or("").contains("run recovered at startup")),
        "recovery's own reconciliation must be durably auditable as an event, not silent"
    );
}

// -------------------------------------------------------------------- D4

/// Seeds a *separate* run with the given already-terminal task snapshot in
/// `RUNNING` state, then runs the real startup-recovery pass over it and
/// returns the state it derives. `derive_run_state` itself is a private
/// function in `engine.rs` with no test-only visibility carve-out, so this
/// exercises it the same way production does — through its one real entry
/// point (`recover_incomplete_runs`) — rather than calling it directly.
/// Using a separate run (not the run under test) keeps this an independent
/// check: it proves what the canonical function *would* derive for this
/// exact task snapshot, without depending on whichever code path the run
/// under test itself took to reach its live state.
async fn recovery_derived_state(store: &Store, name: &str, tasks: &[(&str, TaskStatus)]) -> RunState {
    let run_id = Uuid::new_v4();
    let p = plan(name, tasks.iter().map(|(id, _)| sim_task(id, SimOutcome::Pass, 0)).collect(), 2);
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Running, None)
        .await
        .expect("insert_run");
    for (id, status) in tasks {
        store.set_task_status(run_id, id, *status, None, false, true).await.expect("seed task status");
    }
    recover_incomplete_runs(store).await.expect("recovery");
    store.get_run_summary(run_id).await.expect("summary").expect("run exists").state
}

/// D4 Test 1 — FAILED: a run containing a real product failure must reach
/// the same live final state recovery would derive for the identical task
/// snapshot. Pre-fix, live finalisation only checked for `InfraError` and
/// otherwise always reported `Completed`, so this run reported COMPLETED
/// live while recovery — using `derive_run_state` — would derive FAILED for
/// the exact same two-PASSED-one-FAILED task set.
#[tokio::test]
async fn d4_live_run_with_failed_task_matches_recovery_derived_state() {
    let (store, dir) = temp_store().await;
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    let p = plan(
        "d4-failed",
        vec![
            sim_task("ok1", SimOutcome::Pass, 0),
            sim_task("ok2", SimOutcome::Pass, 0),
            sim_task("bad", SimOutcome::Fail, 0),
        ],
        4,
    );
    let run_id = Uuid::new_v4();
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);
    spawn_run(record.clone(), registry, p.concurrency, store.clone(), evidence, std::time::Duration::from_secs(10));

    for _ in 0..200 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // 1. Task reaches the expected terminal state.
    {
        let engine = record.engine.read().await;
        assert_eq!(engine.tasks[&"bad".to_string()].status, TaskStatus::Failed);
        assert_eq!(engine.tasks[&"ok1".to_string()].status, TaskStatus::Passed);
        assert_eq!(engine.tasks[&"ok2".to_string()].status, TaskStatus::Passed);
    }

    // 2. Live finalisation must derive FAILED, not COMPLETED.
    let live_state = record.summary().await.state;
    assert_eq!(live_state, RunState::Failed, "a run containing a FAILED task must not be reported COMPLETED live");

    // 3. The database must durably agree with the live state.
    let db_state = store.get_run_summary(run_id).await.expect("summary").expect("exists").state;
    assert_eq!(db_state, live_state, "database run state must match the live state exactly");

    // 4. Recovery, via the canonical derive_run_state, must reach the same
    //    verdict for the identical task snapshot.
    let recovered = recovery_derived_state(
        &store,
        "d4-failed-shadow",
        &[("ok1", TaskStatus::Passed), ("ok2", TaskStatus::Passed), ("bad", TaskStatus::Failed)],
    )
    .await;
    assert_eq!(recovered, live_state, "recovery-derived state must match live-finalisation state for the same tasks");
}

/// D4 Test 2 — BLOCKED: a dependent task blocked by a failed prerequisite
/// must reach the same live final state recovery would derive. This task
/// set also exercises `derive_run_state`'s documented precedence — FAILED
/// outranks BLOCKED — so the correct state here is FAILED, not PARTIAL.
/// Pre-fix, live finalisation reported COMPLETED regardless.
#[tokio::test]
async fn d4_live_run_with_blocked_task_matches_recovery_derived_state() {
    let (store, dir) = temp_store().await;
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    let root = sim_task("root", SimOutcome::Fail, 0);
    let dependent = TaskDef {
        id: "dependent".into(),
        name: "dependent".into(),
        prerequisites: vec!["root".into()],
        simulate: Some(SimSpec { outcome: SimOutcome::Pass, delay_ms: 0 }),
        ..Default::default()
    };
    let p = plan("d4-blocked", vec![root, dependent], 2);
    let run_id = Uuid::new_v4();
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);
    spawn_run(record.clone(), registry, p.concurrency, store.clone(), evidence, std::time::Duration::from_secs(10));

    for _ in 0..200 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // 1. Task reaches the expected terminal state.
    {
        let engine = record.engine.read().await;
        assert_eq!(engine.tasks[&"root".to_string()].status, TaskStatus::Failed);
        assert_eq!(engine.tasks[&"dependent".to_string()].status, TaskStatus::Blocked);
    }

    // 2. Live finalisation must derive FAILED (precedence over BLOCKED), not COMPLETED.
    let live_state = record.summary().await.state;
    assert_eq!(
        live_state,
        RunState::Failed,
        "a run with a FAILED prerequisite and a BLOCKED dependent must not be reported COMPLETED live"
    );

    // 3. The database must durably agree with the live state.
    let db_state = store.get_run_summary(run_id).await.expect("summary").expect("exists").state;
    assert_eq!(db_state, live_state, "database run state must match the live state exactly");

    // 4. Recovery must reach the same verdict for the identical task snapshot.
    let recovered = recovery_derived_state(
        &store,
        "d4-blocked-shadow",
        &[("root", TaskStatus::Failed), ("dependent", TaskStatus::Blocked)],
    )
    .await;
    assert_eq!(recovered, live_state, "recovery-derived state must match live-finalisation state for the same tasks");
}

/// D4 Test 3 — TIMEOUT: a run whose only infrastructure outcome is a
/// TIMEOUT (not INFRA_ERROR) must reach the same live final state recovery
/// would derive. Pre-fix, live finalisation's inline check only tested for
/// `InfraError`, so a TIMEOUT task was invisible to it and the run reported
/// COMPLETED live while recovery would derive PARTIAL.
#[tokio::test]
async fn d4_live_run_with_timeout_task_matches_recovery_derived_state() {
    let (store, dir) = temp_store().await;
    let registry = Arc::new(
        ExecutorRegistry::new(Arc::new(SimulatedExecutor)).register("hang", Arc::new(HangingExecutor)),
    );
    let evidence = evidence_store(&dir);

    let hanging = TaskDef {
        id: "slow".into(),
        name: "slow".into(),
        prerequisites: vec![],
        simulate: None,
        executor: Some("hang".into()),
        ..Default::default()
    };
    let p = plan("d4-timeout", vec![hanging, sim_task("fast", SimOutcome::Pass, 0)], 2);
    let run_id = Uuid::new_v4();
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);
    spawn_run(
        record.clone(),
        registry,
        p.concurrency,
        store.clone(),
        evidence,
        std::time::Duration::from_secs(2), // short ceiling so the hang times out quickly
    );

    let deadline = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if record.engine.read().await.is_finished() {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(deadline.is_ok(), "the run must finish; if this times out the executor timeout is not firing");

    // 1. Task reaches the expected terminal state.
    {
        let engine = record.engine.read().await;
        assert_eq!(engine.tasks[&"slow".to_string()].status, TaskStatus::Timeout);
        assert_eq!(engine.tasks[&"fast".to_string()].status, TaskStatus::Passed);
    }

    // 2. Live finalisation must derive PARTIAL, not COMPLETED.
    let live_state = record.summary().await.state;
    assert_eq!(live_state, RunState::Partial, "a run containing a TIMEOUT task must not be reported COMPLETED live");

    // 3. The database must durably agree with the live state.
    let db_state = store.get_run_summary(run_id).await.expect("summary").expect("exists").state;
    assert_eq!(db_state, live_state, "database run state must match the live state exactly");

    // 4. Recovery must reach the same verdict for the identical task snapshot.
    let recovered = recovery_derived_state(
        &store,
        "d4-timeout-shadow",
        &[("fast", TaskStatus::Passed), ("slow", TaskStatus::Timeout)],
    )
    .await;
    assert_eq!(recovered, live_state, "recovery-derived state must match live-finalisation state for the same tasks");
}

// -------------------------------------------------------------------- D5

/// Always returns real captured "log" text, so `spawn_run`'s
/// `evidence.put` + `store.insert_evidence` path is actually exercised —
/// `SimulatedExecutor` always returns `evidence: None` and never reaches it.
struct EvidenceProducingExecutor;

#[async_trait]
impl Executor for EvidenceProducingExecutor {
    async fn execute(&self, _task: &TaskDef) -> ExecutionOutcome {
        ExecutionOutcome {
            status: TaskStatus::Passed,
            reason: None,
            evidence: Some("captured stdout: all assertions ok\ncaptured stderr: (empty)\n".to_string()),
        }
    }
}

/// D5-A — `insert_evidence`: a real SQLite write failure on the `evidence`
/// table. Confirms: the error is escalated (not silently dropped), the task's
/// own outcome is unaffected (evidence is captured *after* the outcome is
/// already decided), the run is durably marked FAILED rather than falsely
/// reporting success over evidence that never made it to the database, and —
/// the "no false success" check — the `evidence` table genuinely has zero
/// rows for this run even though the executor "produced" evidence and the
/// log file was actually written to the filesystem (proving the divergence
/// is real: a file exists that the database has no record of).
#[tokio::test]
async fn d5_insert_evidence_failure_is_escalated_with_no_false_success_row() {
    let (store, dir) = temp_store().await;
    let registry = Arc::new(
        ExecutorRegistry::new(Arc::new(SimulatedExecutor)).register("evidence-producer", Arc::new(EvidenceProducingExecutor)),
    );
    let evidence = evidence_store(&dir);

    let task = TaskDef {
        id: "with-evidence".into(),
        name: "with-evidence".into(),
        prerequisites: vec![],
        simulate: None,
        executor: Some("evidence-producer".into()),
        ..Default::default()
    };
    let p = plan("d5-evidence-fail", vec![task], 1);
    let run_id = Uuid::new_v4();
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);

    // Real SQLite write failure on every INSERT into `evidence` — not a mock,
    // not a manually-set flag.
    sqlx::query("CREATE TRIGGER fail_evidence_insert BEFORE INSERT ON evidence BEGIN SELECT RAISE(ABORT, 'injected evidence write failure'); END;")
        .execute(&store.pool)
        .await
        .expect("create trigger");

    spawn_run(record.clone(), registry, p.concurrency, store.clone(), evidence, std::time::Duration::from_secs(10));

    for _ in 0..200 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // The task's own outcome is decided before evidence is persisted, so a
    // failure here must not corrupt or block the task's real result.
    {
        let engine = record.engine.read().await;
        assert_eq!(engine.tasks[&"with-evidence".to_string()].status, TaskStatus::Passed);
    }

    // Error propagation: the failure must be escalated, not swallowed.
    let failure = record.persistence_error().await;
    assert!(failure.is_some(), "an insert_evidence failure must be recorded via note_persistence_failure");
    assert!(
        failure.unwrap().contains("insert_evidence"),
        "the recorded failure must identify insert_evidence as the context"
    );

    // No false success: the run must not be reported COMPLETED over evidence
    // that never made it to disk.
    let live_state = record.summary().await.state;
    assert_eq!(live_state, RunState::Failed, "a run with a lost evidence write must not be reported COMPLETED");
    let db_state = store.get_run_summary(run_id).await.expect("summary").expect("exists").state;
    assert_eq!(db_state, live_state, "database state must durably agree with the live state");

    // Database verification: genuinely zero evidence rows for this run —
    // not a partial row, not a row with a dangling object_key.
    let evidence_rows = store.list_evidence_for_run(run_id).await.expect("list_evidence_for_run");
    assert_eq!(evidence_rows.len(), 0, "no evidence row may exist when its insert genuinely failed");

    // The filesystem write, by contrast, really did happen — this is the
    // concrete divergence the escalation exists to catch: content exists
    // that the database has no knowledge of.
    let task_dir = dir.path().join("evidence").join(run_id.to_string());
    let has_log_file = std::fs::read_dir(&task_dir).map(|mut it| it.next().is_some()).unwrap_or(false);
    assert!(has_log_file, "the evidence file itself must have been written (only the DB row failed)");
}

/// D5-B — `set_run_state`: a real SQLite write failure on the transition to
/// `RUNNING`. Confirms: the error is escalated, execution is not silently
/// blocked by it (best-effort, matching this write's documented semantics),
/// and the run's durable outcome reflects the loss rather than reporting a
/// clean completion.
#[tokio::test]
async fn d5_set_run_state_running_failure_is_escalated_and_run_finishes_failed() {
    let (store, dir) = temp_store().await;
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    let p = plan("d5-run-state-fail", vec![sim_task("t1", SimOutcome::Pass, 0)], 1);
    let run_id = Uuid::new_v4();
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);

    // Fails only the UPDATE that sets state to RUNNING — set_run_finished's
    // UPDATE never sets state to 'RUNNING', so it is untouched by this trigger.
    sqlx::query(
        "CREATE TRIGGER fail_run_state_running BEFORE UPDATE ON runs \
         WHEN NEW.state = 'RUNNING' \
         BEGIN SELECT RAISE(ABORT, 'injected run-state write failure'); END;",
    )
    .execute(&store.pool)
    .await
    .expect("create trigger");

    spawn_run(record.clone(), registry, p.concurrency, store.clone(), evidence, std::time::Duration::from_secs(10));

    for _ in 0..200 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // Execution must not be silently blocked by this failure — the task
    // still genuinely ran to completion.
    {
        let engine = record.engine.read().await;
        assert_eq!(engine.tasks[&"t1".to_string()].status, TaskStatus::Passed, "execution must proceed despite the lost state write");
    }

    let failure = record.persistence_error().await;
    assert!(failure.is_some(), "a set_run_state failure must be recorded via note_persistence_failure");
    assert!(
        failure.unwrap().contains("set_run_state"),
        "the recorded failure must identify set_run_state as the context"
    );

    let live_state = record.summary().await.state;
    assert_eq!(live_state, RunState::Failed, "a run whose RUNNING transition never persisted must not report COMPLETED");
    let db_state = store.get_run_summary(run_id).await.expect("summary").expect("exists").state;
    assert_eq!(db_state, live_state, "database state must durably agree with the live state");
}

/// D5-C — `set_run_started`: a real SQLite write failure on the write that
/// records when execution actually began. Confirms: the error is escalated,
/// execution does not silently proceed as though the durable start succeeded
/// (the run still ends up durably FAILED, not a clean COMPLETED with a NULL
/// `started_at` nobody is told about), and `runs.started_at` genuinely stays
/// NULL in the database — verified directly, not inferred.
#[tokio::test]
async fn d5_set_run_started_failure_is_escalated_and_started_at_stays_null() {
    let (store, dir) = temp_store().await;
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));
    let evidence = evidence_store(&dir);

    let p = plan("d5-run-started-fail", vec![sim_task("t1", SimOutcome::Pass, 0)], 1);
    let run_id = Uuid::new_v4();
    store
        .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None)
        .await
        .expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);

    // set_run_finished's UPDATE never touches started_at, so NEW.started_at
    // during that write just carries over the (still-NULL) old value — this
    // trigger only ever fires for set_run_started's own write.
    sqlx::query(
        "CREATE TRIGGER fail_run_started BEFORE UPDATE ON runs \
         WHEN NEW.started_at IS NOT NULL \
         BEGIN SELECT RAISE(ABORT, 'injected run-started write failure'); END;",
    )
    .execute(&store.pool)
    .await
    .expect("create trigger");

    spawn_run(record.clone(), registry, p.concurrency, store.clone(), evidence, std::time::Duration::from_secs(10));

    for _ in 0..200 {
        if record.engine.read().await.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    {
        let engine = record.engine.read().await;
        assert_eq!(engine.tasks[&"t1".to_string()].status, TaskStatus::Passed, "execution must proceed despite the lost started_at write");
    }

    let failure = record.persistence_error().await;
    assert!(failure.is_some(), "a set_run_started failure must be recorded via note_persistence_failure");
    assert!(
        failure.unwrap().contains("set_run_started"),
        "the recorded failure must identify set_run_started as the context"
    );

    let live_state = record.summary().await.state;
    assert_eq!(live_state, RunState::Failed, "a run whose start was never durably recorded must not report COMPLETED");
    let db_summary = store.get_run_summary(run_id).await.expect("summary").expect("exists");
    assert_eq!(db_summary.state, live_state, "database state must durably agree with the live state");

    // Direct database verification: started_at genuinely never made it in,
    // even though the run demonstrably executed and finished.
    let row_started_at: Option<String> = sqlx::query_scalar("SELECT started_at FROM runs WHERE id = ?")
        .bind(run_id.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("query started_at");
    assert_eq!(row_started_at, None, "started_at must remain NULL in the database — the write that would have set it genuinely failed");
}

/// D5-D — concurrent recovery: two recovery passes racing against the same
/// database, simulating what happens if `recover_incomplete_runs` is ever
/// invoked twice concurrently against one store (e.g. two orchestrator
/// instances mistakenly pointed at the same database file). Seeds *two*
/// independent orphaned runs so the test can distinguish "this run's own
/// state came out correct" from "one caller's early failure on run A's
/// duplicate event silently stopped it from ever reaching run B" — the
/// latter would be a real defect (one racer's harmless collision on one run
/// blocking recovery of an unrelated run), not the double-execution/
/// corruption scenario the race might first suggest.
#[tokio::test]
async fn d5_concurrent_recovery_of_multiple_orphaned_runs_is_consistent_and_non_blocking() {
    let (store, _dir) = temp_store().await;

    async fn seed_orphan(store: &Store, name: &str) -> Uuid {
        let run_id = Uuid::new_v4();
        let p = plan(name, vec![sim_task("done", SimOutcome::Pass, 0), sim_task("orphan", SimOutcome::Pass, 0)], 2);
        store
            .insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Running, None)
            .await
            .expect("insert_run");
        store.set_task_status(run_id, "done", TaskStatus::Running, None, true, false).await.unwrap();
        store.set_task_status(run_id, "done", TaskStatus::Passed, None, false, true).await.unwrap();
        // "orphan" is left RUNNING, exactly like an unclean shutdown.
        store.set_task_status(run_id, "orphan", TaskStatus::Running, None, true, false).await.unwrap();
        run_id
    }

    let run_a = seed_orphan(&store, "d5-concurrent-a").await;
    let run_b = seed_orphan(&store, "d5-concurrent-b").await;

    // Two concurrent recovery passes against the SAME connection pool/file —
    // the real concurrency primitive, not two sequential calls.
    let (result_a, result_b) = tokio::join!(recover_incomplete_runs(&store), recover_incomplete_runs(&store));

    // Whichever caller "loses" a duplicate-event race is allowed to report
    // that in its own return value — what must NOT happen is that loss
    // silently costing an unrelated run its recovery. So: don't assert both
    // calls return Ok; assert the durable outcome for BOTH runs is correct
    // regardless of which call(s) reported Ok or Err.
    println!("recovery A: {result_a:?}");
    println!("recovery B: {result_b:?}");

    for (label, run_id) in [("A", run_a), ("B", run_b)] {
        let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
        let done = tasks.iter().find(|t| t.id == "done").expect("done task");
        let orphan = tasks.iter().find(|t| t.id == "orphan").expect("orphan task");
        assert_eq!(done.status, TaskStatus::Passed, "run {label}: an already-terminal task must not be corrupted by the race");
        assert_eq!(
            orphan.status,
            TaskStatus::InfraError,
            "run {label}: the orphaned task must be repaired exactly once, to the correct terminal status, regardless of which racer did it"
        );

        let summary = store.get_run_summary(run_id).await.expect("summary").expect("run exists");
        assert_eq!(
            summary.state,
            RunState::Partial,
            "run {label}: the final durable run state must be the single correct value, not corrupted by the race"
        );

        // No true duplicate terminal event: the UNIQUE(run_id, sequence)
        // constraint should mean at most one "recovered at startup" row
        // made it to disk even though both racers attempted to insert one.
        let events = store.list_events_since(run_id, 0).await.expect("events");
        let recovery_events = events
            .iter()
            .filter(|e| e.summary.as_deref().unwrap_or("").contains("run recovered at startup"))
            .count();
        assert_eq!(
            recovery_events, 1,
            "run {label}: exactly one recovery-completion event may durably exist, never zero and never a true duplicate"
        );
    }
}

// ------------------------------------------------------------------ BE-13

/// BE-13 — cancelling a run with a genuinely in-flight task must actually
/// interrupt it (drop the executor future, which real adapters wire to
/// `kill_on_drop`), not just relabel it while the background work keeps
/// running for its full duration. Proven here by timing: the task's own
/// simulated delay is 2s, cancellation is requested after ~30ms, and the
/// whole run must reach a terminal state in well under 2s — if cancellation
/// were fake (a status flip with the task still running to completion in
/// the background), this would take ~2s instead.
#[tokio::test]
async fn be13_cancel_actually_interrupts_an_in_flight_task_rather_than_waiting_it_out() {
    let (store, dir) = temp_store().await;
    let evidence = evidence_store(&dir);
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));

    let p = plan("cancel-in-flight", vec![sim_task("slow", SimOutcome::Pass, 2_000)], 1);
    let run_id = Uuid::new_v4();
    store.insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None).await.expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);
    spawn_run(record.clone(), registry, p.concurrency, store.clone(), evidence, std::time::Duration::from_secs(30));

    // Give the task a moment to actually be dispatched and start running.
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert_eq!(record.engine.read().await.tasks["slow"].status, TaskStatus::Running, "task must be genuinely in-flight before cancelling");

    let start = std::time::Instant::now();
    record.request_cancel("tester").await;

    let mut finished = false;
    for _ in 0..100 {
        if record.engine.read().await.is_finished() {
            finished = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let elapsed = start.elapsed();
    assert!(finished, "run must reach terminal state after cancellation");
    assert!(
        elapsed < std::time::Duration::from_millis(1_000),
        "cancellation must actually interrupt the in-flight task (took {elapsed:?}, task's own delay was 2000ms) — a fake cancel would take ~2000ms"
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await; // let the finalisation tail persist
    assert_eq!(record.engine.read().await.tasks["slow"].status, TaskStatus::Cancelled);
    assert_eq!(*record.state.read().await, RunState::Cancelled);

    let summary = store.get_run_summary(run_id).await.expect("summary").expect("run exists");
    assert_eq!(summary.state, RunState::Cancelled, "durable state must also be CANCELLED");
    let tasks = store.list_tasks_for_run(run_id).await.expect("tasks");
    assert_eq!(tasks[0].status, TaskStatus::Cancelled, "durable task status must also be CANCELLED");
}

/// BE-13 — a task that was never dispatched (still QUEUED, waiting for a
/// concurrency permit behind an in-flight task) must never start running at
/// all once cancellation is requested — proven by asserting it never
/// transitions through RUNNING, only straight to CANCELLED.
#[tokio::test]
async fn be13_cancel_prevents_a_not_yet_started_queued_task_from_ever_running() {
    let (store, dir) = temp_store().await;
    let evidence = evidence_store(&dir);
    let registry = Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor)));

    // concurrency 1: "first" occupies the only permit; "second" stays
    // QUEUED behind it the whole time.
    let p = plan(
        "cancel-queued",
        vec![sim_task("first", SimOutcome::Pass, 500), sim_task("second", SimOutcome::Pass, 0)],
        1,
    );
    let run_id = Uuid::new_v4();
    store.insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Scheduled, None).await.expect("insert_run");
    let record = RunRecord::new(run_id, &p, Some("p1".into()), Some("tester".into()), RunState::Scheduled, None);
    spawn_run(record.clone(), registry, p.concurrency, store.clone(), evidence, std::time::Duration::from_secs(30));

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert_eq!(record.engine.read().await.tasks["first"].status, TaskStatus::Running);
    assert_eq!(record.engine.read().await.tasks["second"].status, TaskStatus::Queued, "second must still be waiting on the permit, never started");

    record.request_cancel("tester").await;

    for _ in 0..100 {
        if record.engine.read().await.is_finished() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let engine = record.engine.read().await;
    assert_eq!(engine.tasks["first"].status, TaskStatus::Cancelled, "in-flight task interrupted");
    assert_eq!(engine.tasks["second"].status, TaskStatus::Cancelled, "never-started task cancelled directly, never RUNNING");
    assert!(engine.tasks["second"].started_at.is_none(), "a task cancelled before dispatch must never have a started_at");
}

/// BE-13 durability — a `CANCELLED` task status, once actually persisted,
/// must survive a process restart and still report the run as CANCELLED
/// (not COMPLETED/PARTIAL), via the exact same `recover_incomplete_runs`
/// path B2/D-series already rely on. This is what makes cancellation
/// restart-safe: the in-memory `RunRecord::cancel` signal is lost on
/// restart, but a task status that was actually written to disk is not.
#[tokio::test]
async fn be13_recovery_reports_cancelled_run_as_cancelled_not_completed() {
    let (store, _dir) = temp_store().await;
    let run_id = Uuid::new_v4();
    let p = plan("cancel-recovery", vec![sim_task("t1", SimOutcome::Pass, 0), sim_task("t2", SimOutcome::Pass, 0)], 2);
    // Simulates: process died right after persisting one task as CANCELLED
    // (the completion path) but before the run-level completion write.
    store.insert_run(run_id, &p, Some("p1"), Some("tester"), RunState::Running, None).await.expect("insert");
    store
        .set_task_status(run_id, "t1", TaskStatus::Cancelled, Some("run cancelled while this task was executing"), false, true)
        .await
        .unwrap();
    store.set_task_status(run_id, "t2", TaskStatus::Running, None, true, false).await.unwrap(); // orphaned mid-flight

    recover_incomplete_runs(&store).await.expect("recovery");

    let summary = store.get_run_summary(run_id).await.expect("summary").expect("run exists");
    assert_eq!(summary.state, RunState::Cancelled, "a run with any CANCELLED task must recover as CANCELLED, not COMPLETED/PARTIAL");
}
