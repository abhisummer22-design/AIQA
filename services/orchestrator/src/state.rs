use crate::ai_router::{AiInfrastructure, Embedder};
use crate::domain::RunId;
use crate::engine::{ProjectStream, RunRecord};
use crate::evidence::LocalEvidenceStore;
use crate::executor::ExecutorRegistry;
use crate::log_bus::LogBus;
use crate::performance_testing::{PerfRunState, PerformanceRunHandle};
use crate::policy::PolicyConfig;
use crate::store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    /// Active/recent runs the *current process* has driven — the fast path
    /// for live status/WS streaming. A run that finished before this
    /// process last started (or was created by a different process) won't
    /// be here; `store` is the source of truth those fall back to (see
    /// `api.rs::get_run`).
    pub runs: Arc<RwLock<HashMap<RunId, Arc<RunRecord>>>>,
    pub registry: Arc<ExecutorRegistry>,
    pub store: Store,
    pub policy: Arc<PolicyConfig>,
    pub evidence: Arc<LocalEvidenceStore>,
    /// BE-14 (§14) — one `ProjectStream` per project, lazily created on
    /// first need (a run created in that project, or a subscriber
    /// connecting to it, whichever happens first). Every run belonging to
    /// a project fans its real events into the same `ProjectStream`, so a
    /// `GET /api/v1/projects/{id}/events` WS subscriber sees that
    /// project's activity across all of its runs, not just one.
    pub project_streams: Arc<RwLock<HashMap<String, Arc<ProjectStream>>>>,
    /// Central AI Provider & Model Intelligence Layer (`ai_router.rs`).
    /// Constructed once (real `reqwest::Client` reuse per discovery/
    /// executor, same reasoning as `registry`/`store`), threaded through
    /// so every AI-consuming handler shares one discovery cache/settings
    /// store rather than each building its own.
    pub ai_infra: Arc<AiInfrastructure>,
    /// Vector Store / RAG Retrieval v1 (`ai_router.rs::Embedder`'s own doc
    /// comment). A trait object — not `ai_infra`'s concrete
    /// `GeminiEmbedder` directly — for exactly the reason `ai_infra`
    /// itself is built on `Arc<dyn AiExecutor>`: tests inject
    /// `ai_router::test_support::FakeEmbedder` instead of ever making a
    /// real network call.
    pub embedder: Arc<dyn Embedder>,
    /// Phase 6 — live, in-process handles for running (or just-finished)
    /// performance tests. Session-only, same honest limitation as `runs`
    /// above: a handle here does not survive a restart (see
    /// `performance_testing::recover_incomplete_performance_runs`, called
    /// once at startup, for how an orphaned `RUNNING` DB row from a prior
    /// process is honestly marked `INTERRUPTED` rather than left stuck).
    pub performance_tests: Arc<RwLock<HashMap<Uuid, Arc<PerformanceRunHandle>>>>,
    /// Real-time central log bus & ring buffer for live AIQA orchestrator activity.
    pub log_bus: Arc<LogBus>,
    /// Phase 7 — which autopilot runs this process is working on (`autopilot_runner.rs`). What a run has done is in
    /// the database; this only stops two tasks from working on one run.
    pub autopilot: Arc<crate::autopilot_runner::AutopilotRegistry>,
    /// Real, live-toggleable "show the browser window while a Playwright run executes"
    /// preference (`GET`/`PUT /api/v1/execution-settings`). Off by default — every earlier
    /// release ran headless unconditionally. `main.rs` loads the persisted value into this same
    /// `Arc` at startup, before constructing the real `PlaywrightExecutor` the registry holds
    /// with a clone of it, so a change made through this state's own `store` and this atomic
    /// (the PUT handler updates both) is what that executor's very next run actually reads —
    /// never a setting that "looks" saved but does not change execution behavior.
    pub show_browser_window: Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    pub fn new(
        registry: Arc<ExecutorRegistry>,
        store: Store,
        policy: PolicyConfig,
        evidence: LocalEvidenceStore,
        ai_infra: Arc<AiInfrastructure>,
        embedder: Arc<dyn Embedder>,
    ) -> Self {
        // Every call the AI makes from now on is counted (`ai_usage.rs`): where the app is built is the one place both the
        // running server and the tests pass through.
        ai_infra.set_usage_sink(Arc::new(crate::ai_usage::UsageLedger::new(store.clone())));
        Self {
            runs: Arc::new(RwLock::new(HashMap::new())),
            registry,
            store,
            policy: Arc::new(policy),
            evidence: Arc::new(evidence),
            project_streams: Arc::new(RwLock::new(HashMap::new())),
            ai_infra,
            embedder,
            performance_tests: Arc::new(RwLock::new(HashMap::new())),
            log_bus: Arc::new(LogBus::new(300)),
            autopilot: Arc::default(),
            show_browser_window: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Gets or creates the `ProjectStream` for `project_id`. Called both by
    /// run creation (`api.rs::create_run_core`, to hand the new
    /// `RunRecord` something real to publish into) and by a subscriber
    /// connecting before any run in that project has ever existed yet (so
    /// a subscription never fails just because it was first).
    pub async fn project_stream(&self, project_id: &str) -> Arc<ProjectStream> {
        if let Some(ps) = self.project_streams.read().await.get(project_id) {
            return ps.clone();
        }
        let mut map = self.project_streams.write().await;
        map.entry(project_id.to_string())
            .or_insert_with(|| Arc::new(ProjectStream::default()))
            .clone()
    }

    /// Drops a project's stream, returning whether there was one to drop.
    ///
    /// This map had exactly one mutation -- the `entry().or_insert_with()` above --
    /// and no `.remove()` anywhere in the crate, so it grew by one
    /// `Arc<ProjectStream>` (a `broadcast::Sender` with capacity 1024 plus its
    /// `AtomicU64`) for every distinct project id ever mentioned, for the lifetime
    /// of the process.
    ///
    /// The leak that actually mattered was not steady-state growth but deletion:
    /// `delete_project_handler` removed the project's rows from the database and
    /// never touched this map, so a project deleted and then recreated under the
    /// same id -- or simply deleted -- left its broadcast channel resident
    /// forever, holding a buffer nothing will ever read again.
    ///
    /// Safe to call for a project that has no entry: `project_stream` recreates it
    /// on the next subscribe or run, so there is no window in which a live
    /// subscriber loses its channel. Existing subscribers hold their own `Arc`, so
    /// removing the map entry does not close the channel underneath them either --
    /// `tokio::sync::broadcast` only reports `Closed` once the last sender is
    /// dropped, and the last sender is the one those subscribers are holding.
    pub async fn forget_project_stream(&self, project_id: &str) -> bool {
        self.project_streams
            .write()
            .await
            .remove(project_id)
            .is_some()
    }

    /// Evicts finished performance-test handles from the in-memory map, returning
    /// how many went.
    ///
    /// Same defect as `project_streams`, and worth spelling out because the cost
    /// per entry is not fixed: `PerformanceRunHandle::samples` is a `Vec` that is
    /// only ever pushed to and never trimmed. For a `VOLUME` run that is bounded
    /// by `max_total_requests` (default 5000), but `LOAD` runs take `max_requests`
    /// as `Option` (`performance_testing.rs`), so an unset limit means one
    /// `RequestSample` per request, kept for the process lifetime. The two status
    /// endpoints then `clone()` that whole `Vec` on every poll.
    ///
    /// Only *finished* handles are dropped, and only after a grace window, for the
    /// same reason `prune_finished_runs` has one: a status request issued
    /// immediately after a test finishes is a normal thing to do, and dropping the
    /// handle out from under it would turn a completed run into a 404.
    ///
    /// The durable record is the `performance_runs` row either way, so nothing
    /// observable is lost.
    pub async fn prune_finished_performance_tests(&self) -> usize {
        let mut map = self.performance_tests.write().await;
        let doomed: Vec<Uuid> = map
            .iter()
            .filter_map(|(id, handle)| {
                // `try_read` rather than `read`: a run finalising right now holds
                // the write lock, and a sweep must never block or fail a start.
                // A handle we cannot inspect is left for the next sweep.
                let Ok(state) = handle.state.try_read() else {
                    return None;
                };
                if matches!(*state, PerfRunState::Running) {
                    return None;
                }
                // Terminal, but inside the grace window: a status request issued
                // moments after a test finishes is an ordinary thing to do, and
                // dropping the handle under it would turn a completed run into a
                // 404. `started_at` bounds how long the handle has existed, which
                // is the closest thing to a finish time this struct keeps --
                // `PerformanceRunHandle` has no `finished_at`, only
                // `finished_elapsed_ms`, and comparing that against wall-clock
                // `now` is exactly the bug that field's own doc comment warns
                // about.
                let aged = (chrono::Utc::now() - handle.started_at).num_seconds()
                    >= FINISHED_PERF_RETENTION_SECS;
                if aged {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        for id in &doomed {
            map.remove(id);
        }
        doomed.len()
    }

    /// Evicts terminal runs from the in-memory map, returning how many went.
    ///
    /// This map had no eviction at all: four `.insert()` sites, zero
    /// `.remove()`/`retain()`, so it grew for the lifetime of the process. Each
    /// entry is an `Arc<RunRecord>` holding the whole `RunPlan`, a
    /// `RunEngine`, and a `Vec<RunEvent>` that is itself never trimmed -- so this
    /// is not a fixed cost per run, it grows with the work each run did. Two
    /// polled endpoints (`get_orchestrator_state`, `get_neural_pipeline`) then
    /// iterate the whole map, so their cost grew with total lifetime runs rather
    /// than with concurrent ones.
    ///
    /// Evicting is safe by the map's own documented contract above: the store is
    /// the source of truth for anything not in memory, so a terminal in-memory
    /// record is a cache entry and nothing durable is lost. This map is the
    /// *live* view; the project-scoped execution endpoints are where a finished
    /// run's history lives.
    ///
    /// Two rules keep this from changing observable behaviour:
    ///
    /// - **Non-terminal runs are never removed.** A run still executing, or one
    ///   held in `PENDING_APPROVAL` awaiting a human, must stay addressable or
    ///   `approve`/`reject`/`cancel` would have nothing to act on.
    /// - **A finished run stays visible for `FINISHED_RUN_RETENTION_SECS`.**
    ///   Dropping a record the instant it reaches a terminal state would make a
    ///   run vanish from the two in-process views before the UI showing it has
    ///   had a chance to poll again. The grace window is what prevents that.
    ///
    /// `try_read` is used rather than `read` so a sweep can never block or fail
    /// a dispatch that is mid-finalisation; a record that cannot be inspected is
    /// left for the next sweep.
    pub async fn prune_finished_runs(&self) -> usize {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(FINISHED_RUN_RETENTION_SECS);
        let mut runs = self.runs.write().await;
        let doomed: Vec<RunId> = runs
            .iter()
            .filter_map(|(id, record)| {
                let Ok(state) = record.state.try_read() else {
                    return None;
                };
                if !state.is_terminal() {
                    return None;
                }
                // Inside the grace window, or with no recorded finish time, keep
                // it -- a record that never recorded a finish cannot be shown to
                // have aged out.
                let finished = record.finished_at.try_read().ok().and_then(|g| *g);
                match finished {
                    Some(at) if at > cutoff => None,
                    Some(_) => Some(*id),
                    None => None,
                }
            })
            .collect();
        for id in &doomed {
            runs.remove(id);
        }
        doomed.len()
    }
}

/// How long a finished run stays in `AppState::runs` after reaching a terminal
/// state, in seconds. Long enough that the orchestrator and neural-pipeline views
/// keep showing a run that has just completed across an ordinary poll interval,
/// short enough that the map's footprint is bounded in practice.
pub const FINISHED_RUN_RETENTION_SECS: i64 = 600;

/// Grace window before a finished performance-test handle is evicted, in seconds.
/// Shorter than FINISHED_RUN_RETENTION_SECS because a performance test's own
/// status endpoint is polled far more often than a run's.
pub const FINISHED_PERF_RETENTION_SECS: i64 = 300;
