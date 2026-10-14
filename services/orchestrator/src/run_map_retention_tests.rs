//! Phase G — the in-memory run maps.
//!
//! `AppState::runs` had four `.insert()` sites and no `.remove()` or `.retain()`
//! anywhere, so it grew for the lifetime of the process. Each entry is an
//! `Arc<RunRecord>` holding a whole `RunPlan`, a `RunEngine` and a
//! `Vec<RunEvent>` that is itself never trimmed — the cost per entry is not
//! fixed, it scales with the work that run did. Two polled endpoints
//! (`get_orchestrator_state`, `get_neural_pipeline`) iterate the whole map, so
//! their cost grew with total lifetime runs rather than concurrent ones.
//!
//! `AgentExecutionDispatcher` had the same shape in two places: its own `runs`
//! map, and an `idempotency_cache` with one entry per distinct key ever used.
//!
//! Eviction is only safe because the map's own documented contract says the
//! store is the source of truth for anything not held in memory — a terminal
//! in-memory record is a cache entry. These tests pin the two rules that keep
//! this from changing behaviour: a live or awaiting-approval run is never
//! dropped, and a run that has only just finished is still visible.

use crate::agent_execution::{
    AgentExecutionDispatcher, AgentExecutionLifecycle, AgentExecutionResponse,
};
use crate::ai_router::test_support::{
    healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
};
use crate::ai_router::{AiExecutor, AiInfrastructure, CapabilityLevel};
use crate::domain::{RunPlan, RunState, TaskDef};
use crate::engine::RunRecord;
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::policy::PolicyConfig;
use crate::state::{AppState, FINISHED_RUN_RETENTION_SECS};
use crate::store::Store;
use std::collections::HashSet;
use std::sync::Arc;

/// A router with one healthy mock provider, built the same way `api_tests.rs`
/// builds its inert one. No real provider is reachable in these tests and none is
/// needed -- the properties under test never reach the model.
fn test_router() -> Arc<AiInfrastructure> {
    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors = std::collections::HashMap::new();
    executors.insert("mock", executor);
    Arc::new(AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider(
            "mock",
            vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
        )))],
        executors,
    ))
}

async fn test_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("prune.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence");
    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor));
    (
        AppState::new(
            Arc::new(registry),
            store,
            PolicyConfig {
                max_concurrency: 8,
                require_approval_for: HashSet::new(),
                blocked_executors: HashSet::new(),
            },
            evidence,
            test_router(),
            Arc::new(FakeEmbedder::unconfigured()),
        ),
        dir,
    )
}

fn one_task_plan() -> RunPlan {
    RunPlan {
        name: "prune-test".into(),
        concurrency: 1,
        tasks: vec![TaskDef {
            id: "t1".into(),
            name: "t1".into(),
            ..Default::default()
        }],
    }
}

/// Inserts a run already sitting in `state`, aged by `age` seconds.
///
/// `finished_at` is set directly rather than by driving a real run to
/// completion: the property under test is the eviction decision, not the engine.
async fn insert_run(state: &AppState, run_state: RunState, age_secs: i64) -> Arc<RunRecord> {
    let run_id = uuid::Uuid::new_v4();
    let record = RunRecord::new(
        run_id,
        &one_task_plan(),
        Some("proj-1".into()),
        None,
        run_state,
        None,
        None,
    );
    *record.state.write().await = run_state;
    *record.finished_at.write().await =
        Some(chrono::Utc::now() - chrono::Duration::seconds(age_secs));
    state.runs.write().await.insert(run_id, record.clone());
    record
}

#[tokio::test]
async fn a_finished_run_is_evicted_once_it_has_aged_out() {
    let (state, _dir) = test_state().await;
    let record = insert_run(
        &state,
        RunState::Completed,
        FINISHED_RUN_RETENTION_SECS + 60,
    )
    .await;
    assert_eq!(
        state.runs.read().await.len(),
        1,
        "precondition: one run held"
    );

    let removed = state.prune_finished_runs().await;
    assert_eq!(removed, 1);
    assert!(
        !state.runs.read().await.contains_key(&record.run_id),
        "a terminal run older than the retention window must not stay in the map"
    );
    state.store.pool.close().await;
}

#[tokio::test]
async fn a_run_that_has_only_just_finished_is_still_visible() {
    let (state, _dir) = test_state().await;
    let record = insert_run(&state, RunState::Completed, 0).await;

    let removed = state.prune_finished_runs().await;
    assert_eq!(removed, 0);
    assert!(
        state.runs.read().await.contains_key(&record.run_id),
        "a run that completed within the retention window must stay in the live \
         view -- dropping it immediately would make it vanish from the orchestrator \
         and neural-pipeline views before the UI showing it polls again"
    );
    state.store.pool.close().await;
}

#[tokio::test]
async fn a_live_or_awaiting_approval_run_is_never_evicted() {
    let (state, _dir) = test_state().await;
    let running = insert_run(&state, RunState::Running, 86_400).await;
    let scheduled = insert_run(&state, RunState::Scheduled, 86_400).await;
    let held = insert_run(&state, RunState::PendingApproval, 86_400).await;
    let created = insert_run(&state, RunState::Created, 86_400).await;

    let removed = state.prune_finished_runs().await;
    assert_eq!(removed, 0, "no non-terminal run may be evicted");
    let runs = state.runs.read().await;
    for id in [
        running.run_id,
        scheduled.run_id,
        held.run_id,
        created.run_id,
    ] {
        assert!(
            runs.contains_key(&id),
            "{id} is not finished and must stay addressable -- approve/reject/cancel \
             would otherwise have nothing to act on"
        );
    }
    state.store.pool.close().await;
}

#[tokio::test]
async fn pruning_keeps_the_finished_run_and_drops_only_the_stale_ones() {
    let (state, _dir) = test_state().await;
    let fresh = insert_run(&state, RunState::Completed, 5).await;
    let stale_a = insert_run(&state, RunState::Failed, FINISHED_RUN_RETENTION_SECS * 2).await;
    let stale_b = insert_run(&state, RunState::Cancelled, FINISHED_RUN_RETENTION_SECS * 3).await;
    let live = insert_run(&state, RunState::Running, 86_400).await;

    let removed = state.prune_finished_runs().await;
    assert_eq!(removed, 2, "only the two aged-out terminal runs");
    let runs = state.runs.read().await;
    assert_eq!(runs.len(), 2);
    assert!(runs.contains_key(&fresh.run_id));
    assert!(runs.contains_key(&live.run_id));
    assert!(!runs.contains_key(&stale_a.run_id));
    assert!(!runs.contains_key(&stale_b.run_id));
    state.store.pool.close().await;
}

/// A record that reached a terminal state but recorded no finish time cannot be
/// shown to have aged out, so it is kept rather than guessed at.
#[tokio::test]
async fn a_terminal_run_with_no_recorded_finish_time_is_kept() {
    let (state, _dir) = test_state().await;
    let run_id = uuid::Uuid::new_v4();
    let record = RunRecord::new(
        run_id,
        &one_task_plan(),
        Some("proj-1".into()),
        None,
        RunState::Completed,
        None,
        None,
    );
    assert!(record.finished_at.read().await.is_none());
    state.runs.write().await.insert(run_id, record);

    assert_eq!(state.prune_finished_runs().await, 0);
    assert!(state.runs.read().await.contains_key(&run_id));
    state.store.pool.close().await;
}

#[tokio::test]
async fn the_idempotency_cache_is_bounded_rather_than_growing_per_key_forever() {
    use crate::agent_execution::IDEMPOTENCY_CACHE_MAX;

    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("idem.sqlite3"))
        .await
        .expect("connect");
    let dispatcher = AgentExecutionDispatcher::new(
        store.clone(),
        PolicyConfig {
            max_concurrency: 8,
            require_approval_for: HashSet::new(),
            blocked_executors: HashSet::new(),
        },
        Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor))),
        Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence")),
        None,
    );

    // Insert well past the cap directly, which is what repeated distinct
    // `idempotency_key` values across a long-lived process would do.
    {
        let mut cache = dispatcher.idempotency_cache.write().await;
        for i in 0..(IDEMPOTENCY_CACHE_MAX + 500) {
            cache.insert(
                format!("proj-1:key-{i}"),
                (
                    uuid::Uuid::new_v4(),
                    AgentExecutionResponse {
                        run_id: uuid::Uuid::new_v4(),
                        project_id: "proj-1".to_string(),
                        lifecycle_state: AgentExecutionLifecycle::Executing,
                        policy_decision: "ALLOW".to_string(),
                        requires_human_approval: false,
                        policy_notes: None,
                        tasks_count: 1,
                        message: String::new(),
                    },
                ),
            );
        }
        assert!(
            cache.len() > IDEMPOTENCY_CACHE_MAX,
            "precondition: the cache is over its cap"
        );
    }

    // The eviction lives in `cache_idempotent`, which is the one place the map
    // is written, so this asserts on that directly rather than trying to drive a
    // whole dispatch to its final step.
    dispatcher
        .cache_idempotent(
            "proj-1:trigger-eviction".to_string(),
            uuid::Uuid::new_v4(),
            AgentExecutionResponse {
                run_id: uuid::Uuid::new_v4(),
                project_id: "proj-1".to_string(),
                lifecycle_state: AgentExecutionLifecycle::Executing,
                policy_decision: "ALLOW".to_string(),
                requires_human_approval: false,
                policy_notes: None,
                tasks_count: 1,
                message: String::new(),
            },
        )
        .await;

    let len = dispatcher.idempotency_cache.read().await.len();
    assert!(
        len <= IDEMPOTENCY_CACHE_MAX + 1,
        "the cache grew to {len} entries, past the {IDEMPOTENCY_CACHE_MAX} cap"
    );
    store.pool.close().await;
}
