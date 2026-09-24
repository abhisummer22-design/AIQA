use aiqa_orchestrator::{
    ai_router::{AiInfrastructure, Embedder, GeminiEmbedder},
    api, auth,
    engine::recover_incomplete_runs,
    evidence::{default_root, LocalEvidenceStore},
    executor::{
        CompiledApiExecutor, ExecutorRegistry, MaestroExecutor, NewmanExecutor, PlaywrightExecutor,
        PytestExecutor, SchemathesisExecutor, SimulatedExecutor,
    },
    policy::PolicyConfig,
    state::AppState,
    store::Store,
};
use std::path::PathBuf;
use std::sync::Arc;
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../infra/data/orchestrator.sqlite3")
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

    match aiqa_orchestrator::skill_files::ensure_skills_dir() {
        Ok(dir) => tracing::info!(
            "real filesystem skills folder ready at {} — drop SKILL.md files there to use them in generation",
            dir.display()
        ),
        Err(e) => tracing::error!("could not create the filesystem skills folder: {e}"),
    }

    match recover_incomplete_runs(&store).await {
        Ok(0) => {}
        Ok(n) => {
            tracing::warn!("recovered {n} run(s) left non-terminal by a prior unclean shutdown")
        }
        Err(e) => tracing::error!("startup run-recovery pass failed: {e}"),
    }

    match aiqa_orchestrator::performance_testing::recover_incomplete_performance_runs(&store).await
    {
        Ok(0) => {}
        Ok(n) => tracing::warn!(
            "marked {n} performance test run(s) INTERRUPTED after a prior unclean shutdown"
        ),
        Err(e) => tracing::error!("startup performance-run-recovery pass failed: {e}"),
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
        )
        // Phase 2.1 — real, native (no subprocess) executor for a compiled
        // ExecutionPlan (`execution_plan::materialize_task_def`'s Api arm).
        // No `project_dir` needed: it makes real HTTP calls directly via
        // `reqwest`, it does not shell out to a fixture project.
        .register("compiled-api", Arc::new(CompiledApiExecutor::from_env()));

    // Central AI Provider & Model Intelligence Layer. Real discovery
    // (Ollama + the MJ AI Gateway) runs in a background task, not awaited
    // here — this app must start and serve real traffic even if no AI
    // provider is reachable at all (brief §3: "must NOT block the entire
    // application if no AI provider exists");
    // `AiInfrastructure::generate` re-discovers lazily (TTL-bounded) on
    // first real use regardless.
    let ai_infra = Arc::new(AiInfrastructure::from_env());
    // Real, persisted settings (brief §21) — loaded once at startup from
    // the `ai_settings` singleton row; `AiSettings::default()` (AUTO,
    // MJ-gateway-first via `mj-default`, fallback enabled) if no row
    // exists yet.
    if let Ok(Some((
        mode,
        preferred_provider,
        preferred_model,
        fallback_enabled,
        task_overrides_json,
    ))) = store.get_ai_settings().await
    {
        let mode = if mode == "MANUAL" {
            aiqa_orchestrator::ai_router::RoutingMode::Manual
        } else {
            aiqa_orchestrator::ai_router::RoutingMode::Auto
        };
        // Real per-task overrides, restored the same way the rest of this
        // row is — a genuinely absent/corrupt value falls back to an
        // honest empty map (no overrides) rather than failing startup.
        let task_overrides = task_overrides_json
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default();
        ai_infra
            .set_settings(aiqa_orchestrator::ai_router::AiSettings {
                mode,
                preferred_provider,
                preferred_model,
                fallback_enabled,
                task_overrides,
            })
            .await;
    }
    // Real, user-added custom providers (Settings UI, `custom_ai_providers`
    // table) — loaded once here so they're included in the very first real
    // discovery pass below, exactly like the persisted `ai_settings` row
    // just above. A read failure degrades to an honest empty set (no
    // custom providers this run) rather than failing startup.
    if let Ok(rows) = store.list_custom_ai_providers().await {
        let configs = rows
            .into_iter()
            .map(|r| aiqa_orchestrator::ai_router::CustomProviderConfig {
                id: r.id,
                name: r.name,
                base_url: r.base_url,
                api_key: r.api_key,
                pricing: aiqa_orchestrator::ai_router::ProviderPricing::from_prices(
                    r.input_usd_per_1k,
                    r.output_usd_per_1k,
                ),
            })
            .collect();
        ai_infra.set_custom_providers(configs).await;
    }
    {
        let ai_infra = ai_infra.clone();
        tokio::spawn(async move {
            let providers = ai_infra.discover_all().await;
            let healthy = providers
                .iter()
                .filter(|p| {
                    matches!(
                        p.status,
                        aiqa_orchestrator::ai_router::ProviderStatus::Healthy
                    )
                })
                .count();
            tracing::info!(
                "AI infrastructure discovery complete: {} provider(s) checked, {healthy} healthy",
                providers.len()
            );
        });
    }
    let embedder: Arc<dyn Embedder> = Arc::new(GeminiEmbedder::from_env());
    let state = AppState::new(
        Arc::new(registry),
        store,
        policy,
        evidence,
        ai_infra,
        embedder,
    );
    // B12 — explicit origin allow-list, not a wildcard. See
    // `api::cors_layer_from_env`'s doc comment for the default and the
    // `AIQA_CORS_ALLOWED_ORIGINS` override.
    let app = api::router(state).layer(api::cors_layer_from_env());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("AIQA orchestrator listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
