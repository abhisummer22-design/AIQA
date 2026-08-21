use crate::ai_router::AiInfrastructure;
use crate::domain::RunId;
use crate::engine::{ProjectStream, RunRecord};
use crate::evidence::LocalEvidenceStore;
use crate::executor::ExecutorRegistry;
use crate::policy::PolicyConfig;
use crate::store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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
}

impl AppState {
    pub fn new(
        registry: Arc<ExecutorRegistry>,
        store: Store,
        policy: PolicyConfig,
        evidence: LocalEvidenceStore,
        ai_infra: Arc<AiInfrastructure>,
    ) -> Self {
        Self {
            runs: Arc::new(RwLock::new(HashMap::new())),
            registry,
            store,
            policy: Arc::new(policy),
            evidence: Arc::new(evidence),
            project_streams: Arc::new(RwLock::new(HashMap::new())),
            ai_infra,
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
        map.entry(project_id.to_string()).or_insert_with(|| Arc::new(ProjectStream::default())).clone()
    }
}
