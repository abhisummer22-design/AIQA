use crate::domain::RunId;
use crate::engine::RunRecord;
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
}

impl AppState {
    pub fn new(
        registry: Arc<ExecutorRegistry>,
        store: Store,
        policy: PolicyConfig,
        evidence: LocalEvidenceStore,
    ) -> Self {
        Self {
            runs: Arc::new(RwLock::new(HashMap::new())),
            registry,
            store,
            policy: Arc::new(policy),
            evidence: Arc::new(evidence),
        }
    }
}
