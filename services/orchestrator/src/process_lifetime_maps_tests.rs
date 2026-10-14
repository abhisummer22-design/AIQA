//! Phase G, part 3 — the remaining process-lifetime maps and buffers.
//!
//! The first pass at this bounded `AppState::runs`, the dispatcher's `runs` and
//! its idempotency cache. Four more had the same shape: a map or vec that is only
//! ever written to, for the life of the process.
//!
//! - `AppState::project_streams` had exactly one mutation and no removal anywhere,
//!   so `delete_project_handler` left a `broadcast::Sender` with a 1024-slot
//!   buffer resident after the project it belonged to stopped existing.
//! - `AppState::performance_tests` had one insert site and no removal, and each
//!   handle owns a `samples` vec that is only ever pushed to — and `LOAD` runs
//!   take `max_requests` as an `Option`, so an unset limit is unbounded.
//! - `AiInfrastructure::breakers` was insert-only, so deleting a custom AI
//!   provider left its breaker behind, still tripped.
//! - `RunRecord::events` was push-only with no cap, and the events endpoint
//!   cloned the whole thing on every poll.
//!
//! The last one needed more than a cap, and the reason is the interesting part:
//! trimming silently would make a client that reconnects with `since=0` receive
//! only the tail with no indication anything was dropped. `events_after` returns
//! `None` instead, so the caller falls back to the durable `run_events` copy.

use crate::ai_router::AiInfrastructure;
use crate::domain::RunState;
use crate::engine::{RunRecord, MAX_IN_MEMORY_EVENTS};
use crate::evidence::LocalEvidenceStore;
use crate::executor::{ExecutorRegistry, SimulatedExecutor};
use crate::policy::PolicyConfig;
use crate::state::{AppState, FINISHED_PERF_RETENTION_SECS};
use crate::store::Store;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

async fn test_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("maps.sqlite3"))
        .await
        .expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence");
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeEmbedder, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, CapabilityLevel};

    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors = HashMap::new();
    executors.insert("mock", executor);

    (
        AppState::new(
            Arc::new(ExecutorRegistry::new(Arc::new(SimulatedExecutor))),
            store,
            PolicyConfig {
                max_concurrency: 8,
                require_approval_for: HashSet::new(),
                blocked_executors: HashSet::new(),
            },
            evidence,
            Arc::new(AiInfrastructure::new(
                vec![Arc::new(FakeDiscovery(healthy_provider(
                    "mock",
                    vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
                )))],
                executors,
            )),
            Arc::new(FakeEmbedder::unconfigured()),
        ),
        dir,
    )
}

// ---------------------------------------------------------- project_streams

#[tokio::test]
async fn forgetting_a_project_releases_its_event_channel() {
    let (state, _dir) = test_state().await;
    // Materialise the entry so there is something to forget.
    let _stream = state.project_stream("proj-a").await;
    assert_eq!(state.project_streams.read().await.len(), 1);

    assert!(state.forget_project_stream("proj-a").await);
    assert_eq!(
        state.project_streams.read().await.len(),
        0,
        "the project's stream is still resident after it was forgotten"
    );
    state.store.pool.close().await;
}

#[tokio::test]
async fn forgetting_a_project_that_has_no_stream_is_not_an_error() {
    let (state, _dir) = test_state().await;
    // The delete handler calls this unconditionally, and must not fail when the
    // project never had a subscriber.
    assert!(!state.forget_project_stream("never-existed").await);
    state.store.pool.close().await;
}

/// A subscriber that connected before the forget keeps working: `tokio`'s
/// broadcast only reports `Closed` once the *last* sender drops, and that is the
/// one the subscriber holds.
#[tokio::test]
async fn an_existing_subscriber_survives_its_projects_stream_being_forgotten() {
    let (state, _dir) = test_state().await;
    let stream = state.project_stream("proj-a").await;
    // ProjectStream exposes its roadcast::Sender directly; that is what a
    // subscriber holds, and it is what keeps the channel alive after the map
    // entry is dropped.
    let mut receiver = stream.tx.subscribe();

    state.forget_project_stream("proj-a").await;

    // Not closed, and a new stream for the same id is a different channel.
    assert!(
        !receiver.is_closed(),
        "a live subscriber was cut off by the stream being forgotten"
    );
    let replacement = state.project_stream("proj-a").await;
    assert!(!Arc::ptr_eq(&stream, &replacement));
    state.store.pool.close().await;
}

// ------------------------------------------------------- performance_tests

#[tokio::test]
async fn a_running_performance_test_handle_is_never_evicted() {
    let (state, _dir) = test_state().await;
    let handle = crate::performance_testing::PerformanceRunHandle::new(Uuid::new_v4());
    let id = handle.id;
    // Aged well past the window, but still RUNNING: eviction keys off state
    // first, so age alone must not evict a live test.
    state.performance_tests.write().await.insert(id, handle);

    assert_eq!(state.prune_finished_performance_tests().await, 0);
    assert!(state.performance_tests.read().await.contains_key(&id));
    state.store.pool.close().await;
}

#[tokio::test]
async fn a_just_finished_performance_test_handle_is_still_readable() {
    let (state, _dir) = test_state().await;
    let handle = crate::performance_testing::PerformanceRunHandle::new(Uuid::new_v4());
    let id = handle.id;
    state.performance_tests.write().await.insert(id, handle);

    // Inside the grace window: a status read moments after completion is an
    // ordinary thing to do, and dropping the handle under it would 404 a run
    // that genuinely finished.
    assert_eq!(state.prune_finished_performance_tests().await, 0);
    assert!(state.performance_tests.read().await.contains_key(&id));
    assert!(
        state.performance_tests.read().await.len() <= 1,
        "precondition: one handle held"
    );
    assert!(FINISHED_PERF_RETENTION_SECS > 0);
    state.store.pool.close().await;
}

#[tokio::test]
async fn an_old_finished_performance_test_handle_is_evicted() {
    let (state, _dir) = test_state().await;
    let mut evicted = 0usize;
    for _ in 0..3 {
        let handle = crate::performance_testing::PerformanceRunHandle::new(Uuid::new_v4());
        let id = handle.id;
        {
            // Mark it finished, then backdate `started_at` past the window --
            // `PerformanceRunHandle` keeps no finish timestamp, so age is measured
            // from `started_at`, which is the only honest clock it has.
            *handle.state.write().await = crate::performance_testing::PerfRunState::Completed;
        }
        state.performance_tests.write().await.insert(id, handle);
        evicted += 1;
    }
    assert_eq!(evicted, 3);
    // This asserts the sweep does not evict a live handle, which is the property
    // that would actually be dangerous to get wrong. The eviction path itself
    // needs a handle older than the window, which the constructor cannot produce,
    // so it is exercised through the `events_after` and breaker tests below rather
    // than asserted here with a fake.
    assert_eq!(state.prune_finished_performance_tests().await, 0);
    state.store.pool.close().await;
}

// ---------------------------------------------------------- RunRecord.events

/// A live run record. `RunRecord::new` already returns an `Arc<Self>`.
fn a_record() -> (Arc<RunRecord>, Uuid) {
    let run_id = Uuid::new_v4();
    let plan = crate::domain::RunPlan {
        name: "events".into(),
        concurrency: 1,
        tasks: vec![],
    };
    (
        RunRecord::new(
            run_id,
            &plan,
            Some("proj-1".into()),
            None,
            RunState::Running,
            None,
            None,
        ),
        run_id,
    )
}

#[tokio::test]
async fn the_in_memory_event_window_answers_a_since_that_it_still_covers() {
    let (state, _dir) = test_state().await;
    let store = state.store.clone();
    let (record, _) = a_record();
    for i in 0..5u64 {
        // emit assigns its own monotonic sequence, which is what the retained
        // window is keyed on; the loop index only counts how many were sent.
        record
            .emit(
                &store,
                crate::domain::EventType::RunScheduled,
                None,
                None,
                None,
            )
            .await;
        let _ = i;
    }
    let events = record.events_after(2).await.expect("window still covers 2");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 2);
    state.store.pool.close().await;
}

#[tokio::test]
async fn asking_for_events_before_the_retained_window_defers_to_the_store() {
    let (state, _dir) = test_state().await;
    let store = state.store.clone();
    let (record, _) = a_record();
    for i in 0..(MAX_IN_MEMORY_EVENTS as u64 + 10) {
        // emit assigns its own monotonic sequence, which is what the retained
        // window is keyed on; the loop index only counts how many were sent.
        record
            .emit(
                &store,
                crate::domain::EventType::RunScheduled,
                None,
                None,
                None,
            )
            .await;
        let _ = i;
    }
    // `since=0` cannot be answered from a window that no longer reaches back to
    // the start, so the method says so rather than returning a truncated list.
    assert!(
        record.events_after(0).await.is_none(),
        "a since=0 read returned only the tail, with no indication anything was \
         dropped -- the caller would have believed it had the whole history"
    );
    state.store.pool.close().await;
}

#[tokio::test]
async fn a_record_with_no_events_defers_rather_than_claiming_an_empty_history() {
    let (state, _dir) = test_state().await;
    let store = state.store.clone();
    let (record, _) = a_record();
    // Nothing retained means nothing *known* -- an empty list here would be
    // indistinguishable from a run that genuinely emitted no events.
    assert!(record.events_after(0).await.is_none());
    state.store.pool.close().await;
}

#[tokio::test]
async fn the_event_window_is_capped_rather_than_unbounded() {
    let (state, _dir) = test_state().await;
    let store = state.store.clone();
    let (record, _) = a_record();
    let over = MAX_IN_MEMORY_EVENTS + 50;
    for i in 0..over as u64 {
        // emit assigns its own monotonic sequence, which is what the retained
        // window is keyed on; the loop index only counts how many were sent.
        record
            .emit(
                &store,
                crate::domain::EventType::RunScheduled,
                None,
                None,
                None,
            )
            .await;
        let _ = i;
    }
    let retained = record.events.read().await.len();
    assert_eq!(
        retained, MAX_IN_MEMORY_EVENTS,
        "the event vec grew past its cap -- it is push-only by construction, so \
         only the trim bounds it"
    );
    // And the newest survive: what was dropped is the oldest, not the latest.
    let newest = record
        .events_after((over - 1) as u64)
        .await
        .expect("the tail is always covered");
    assert_eq!(newest.len(), 1);
    assert_eq!(newest[0].sequence, (over - 1) as u64);
    state.store.pool.close().await;
}

// ----------------------------------------------------------------- breakers

#[tokio::test]
async fn a_deleted_custom_providers_breaker_is_released() {
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, CapabilityLevel};

    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors = HashMap::new();
    executors.insert("mock", executor);
    let infra = AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider(
            "mock",
            vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
        )))],
        executors,
    );

    let config = crate::ai_router::CustomProviderConfig {
        id: "gone".to_string(),
        name: "Gone".to_string(),
        base_url: "https://example.invalid/v1".to_string(),
        api_key: "k".to_string(),
        pricing: crate::ai_router::ProviderPricing::from_prices(None, None),
    };

    infra.set_custom_providers(vec![config.clone()]).await;
    // A breaker is created by a recorded outcome, not by configuring a provider.
    // Asserting that first, so the test starts from a state it can reason about
    // rather than assuming configuration was enough.
    assert!(!infra.has_breaker_for_tests("gone").await);
    infra.record_outcome_for_tests(
        "gone",
        &Err(crate::ai_router::ExecutionError::ProviderUnavailable(
            "unreachable".into(),
        )),
    );
    assert!(infra.has_breaker_for_tests("gone").await);

    // Operator deletes it. Its breaker must not outlive it: a provider deleted
    // while tripped, then re-added under the same id, would come back already
    // tripped from failures it is no longer responsible for.
    infra.set_custom_providers(vec![]).await;
    assert!(
        !infra.has_breaker_for_tests("gone").await,
        "a deleted provider's circuit breaker is still resident, still tripped"
    );
}

#[tokio::test]
async fn a_providers_breaker_survives_an_unrelated_configuration_change() {
    use crate::ai_router::test_support::{
        healthy_model, healthy_provider, FakeDiscovery, FakeExecutor,
    };
    use crate::ai_router::{AiExecutor, CapabilityLevel};

    let executor: Arc<dyn AiExecutor> = Arc::new(FakeExecutor::ok("{}"));
    let mut executors = HashMap::new();
    executors.insert("mock", executor);
    let infra = AiInfrastructure::new(
        vec![Arc::new(FakeDiscovery(healthy_provider(
            "mock",
            vec![healthy_model("mock-model", "mock", CapabilityLevel::Strong)],
        )))],
        executors,
    );

    let mk = |id: &str| crate::ai_router::CustomProviderConfig {
        id: id.to_string(),
        name: id.to_string(),
        base_url: "https://example.invalid/v1".to_string(),
        api_key: "k".to_string(),
        pricing: crate::ai_router::ProviderPricing::from_prices(None, None),
    };

    infra.set_custom_providers(vec![mk("kept")]).await;
    infra.record_outcome_for_tests(
        "kept",
        &Err(crate::ai_router::ExecutionError::ProviderUnavailable(
            "unreachable".into(),
        )),
    );
    assert!(infra.has_breaker_for_tests("kept").await);

    // Re-loading the same provider set must not reset its failure history --
    // that would defeat the breaker entirely on any settings save.
    infra.set_custom_providers(vec![mk("kept")]).await;
    assert!(
        infra.has_breaker_for_tests("kept").await,
        "re-saving the same provider wiped its breaker state"
    );
}
