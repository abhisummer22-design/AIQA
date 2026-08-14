use aiqa_orchestrator::{
    api, auth,
    engine::recover_incomplete_runs,
    evidence::{default_root, LocalEvidenceStore},
    executor::{
        ExecutorRegistry, MaestroExecutor, NewmanExecutor, PlaywrightExecutor, PytestExecutor,
        SchemathesisExecutor, SimulatedExecutor,
    },
    policy::PolicyConfig,
    state::AppState,
    store::Store,
};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

/// `services/orchestrator` -> repo root -> `executors/<name>`.
fn executor_project_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../executors")
        .join(name)
}

/// SQLite file lives under `infra/data/`, next to the local evidence store
/// (`evidence::default_root`) — same "no container runtime available" reason
/// documented in `store.rs`'s module doc comment.
fn db_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../infra/data/orchestrator.sqlite3")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("aiqa_orchestrator=info,tower_http=info")),
        )
        .init();

    let store = Store::connect(&db_path())
        .await
        .expect("failed to open/migrate sqlite store");

    if let Some(key) = auth::bootstrap_admin_key_if_needed(&store)
        .await
        .expect("failed to bootstrap admin API key")
    {
        eprintln!(
            "\n================================================================\n\
             AIQA bootstrap admin API key (shown once, save it now):\n\n  {key}\n\n\
             Use it as: Authorization: Bearer {key}\n\
             ================================================================\n"
        );
        tracing::warn!("minted bootstrap admin API key (printed to stderr once)");
    }

    match recover_incomplete_runs(&store).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!("recovered {n} run(s) left non-terminal by a prior unclean shutdown"),
        Err(e) => tracing::error!("startup run-recovery pass failed: {e}"),
    }

    let policy = PolicyConfig::from_env();
    let evidence = LocalEvidenceStore::new(default_root()).expect("failed to init evidence store");

    let registry = ExecutorRegistry::new(Arc::new(SimulatedExecutor))
        .register(
            "playwright",
            Arc::new(PlaywrightExecutor {
                project_dir: executor_project_dir("playwright"),
            }),
        )
        .register(
            "newman",
            Arc::new(NewmanExecutor {
                project_dir: executor_project_dir("newman"),
            }),
        )
        .register(
            "pytest",
            Arc::new(PytestExecutor {
                project_dir: executor_project_dir("pytest"),
            }),
        )
        .register(
            "maestro",
            Arc::new(MaestroExecutor {
                project_dir: executor_project_dir("maestro"),
            }),
        )
        .register(
            "schemathesis",
            Arc::new(SchemathesisExecutor {
                project_dir: executor_project_dir("schemathesis"),
            }),
        );

    let state = AppState::new(Arc::new(registry), store, policy, evidence);
    let app = api::router(state).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("AIQA orchestrator listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
