use crate::ai_router::{AiInfrastructure, Embedder};
use crate::domain::RunId;
use crate::engine::{ProjectStream, RunRecord};
use crate::evidence::LocalEvidenceStore;
use crate::executor::ExecutorRegistry;
use crate::log_bus::LogBus;
use crate::performance_testing::PerformanceRunHandle;
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
}
