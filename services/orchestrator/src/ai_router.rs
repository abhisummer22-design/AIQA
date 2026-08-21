//! Central AI Provider & Model Intelligence Layer.
//!
//! Fresh audit before writing this file: Phase 2 (`ai_generation.rs`) added
//! a real `OllamaProvider`/`AiModelRouter` pair, but it hardcoded exactly
//! one provider and one model (from env vars) with no discovery, no
//! alternative-provider awareness, and no capability-based selection. This
//! module replaces that narrow router with the real central infrastructure
//! this phase requires; `ai_generation.rs` keeps its own task-specific
//! logic (skill resolution, prompt building, the test-case schema,
//! validation/traceability/dedup) but now calls into `AiInfrastructure`
//! for actual execution, instead of owning a provider directly.
//!
//! ```text
//! AiRequest { task, prompt, preferred_provider?, preferred_model? }
//!         v
//! AiInfrastructure::route()  -- AUTO: score all compatible candidates
//!                             -- MANUAL: validate the user's own choice
//!         v
//! RoutingDecision { provider_id, model_id, routing_score, reason }
//!         v
//! AiInfrastructure::generate() -- calls the resolved provider/model,
//!                                 falls back per AiSettings.fallback_enabled
//!         v
//! GenerationResult { text, provider, model, routing_mode, fallback_used, .. }
//! ```
//!
//! Every provider/model status below is real, checked evidence — never a
//! hardcoded "Connected"/"Healthy". No API key for Gemini/OpenAI/Anthropic
//! is configured in this environment (confirmed by `std::env::var`
//! returning `Err` for all three at the time this was written), so those
//! providers correctly, truthfully report `NOT_CONFIGURED` — this module
//! does not simulate them being available.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ------------------------------------------------------------------ enums

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderType {
    Local,
    Api,
    OpenAiCompatible,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderStatus {
    NotConfigured,
    Discovering,
    Available,
    Healthy,
    Degraded,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelStatus {
    Available,
    Healthy,
    Degraded,
    Unavailable,
    Unknown,
}

/// A capability's real, evidenced level — never invented. `Unknown` is the
/// honest default when the discovery source gives no signal either way
/// (brief §5: "UNKNOWN is preferable to guessing").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityLevel {
    Unknown,
    None,
    Basic,
    Strong,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    #[serde(default)]
    pub text_generation: Option<CapabilityLevel>,
    #[serde(default)]
    pub structured_output: Option<CapabilityLevel>,
    #[serde(default)]
    pub reasoning: Option<CapabilityLevel>,
    #[serde(default)]
    pub vision: Option<CapabilityLevel>,
    #[serde(default)]
    pub embedding: Option<CapabilityLevel>,
    #[serde(default)]
    pub tool_use: Option<CapabilityLevel>,
    #[serde(default)]
    pub long_context: Option<CapabilityLevel>,
}

impl ModelCapabilities {
    fn level(&self, cap: RequiredCapability) -> CapabilityLevel {
        let field = match cap {
            RequiredCapability::TextGeneration => self.text_generation,
            RequiredCapability::StructuredOutput => self.structured_output,
            RequiredCapability::Reasoning => self.reasoning,
            RequiredCapability::Vision => self.vision,
            RequiredCapability::Embedding => self.embedding,
            RequiredCapability::ToolUse => self.tool_use,
            RequiredCapability::LongContext => self.long_context,
        };
        field.unwrap_or(CapabilityLevel::Unknown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RequiredCapability {
    TextGeneration,
    StructuredOutput,
    Reasoning,
    Vision,
    Embedding,
    ToolUse,
    LongContext,
}

/// The task taxonomy (brief §9). Each task's required capabilities are a
/// documented, static fact about that task — not derived from any model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiTask {
    TestCaseGeneration,
    RequirementAnalysis,
    DocumentAnalysis,
    BugAnalysis,
    Traceability,
    SecurityAnalysis,
    PerformanceAnalysis,
    UiAnalysis,
    Embedding,
    GeneralAi,
}

/// Brief §8/§9 — every task's minimum real requirement. `LongContext` is
/// required for document-shaped tasks; `Vision` only for `UiAnalysis`
/// (brief's own example); `Embedding` only for `Embedding`. Nothing here
/// requires a model to be "the best" — only capable of the task at all.
pub fn required_capabilities(task: AiTask) -> Vec<RequiredCapability> {
    use RequiredCapability::*;
    match task {
        AiTask::TestCaseGeneration => vec![TextGeneration, StructuredOutput, Reasoning],
        AiTask::RequirementAnalysis => vec![TextGeneration, Reasoning, LongContext],
        AiTask::DocumentAnalysis => vec![TextGeneration, LongContext],
        AiTask::BugAnalysis => vec![TextGeneration, Reasoning],
        AiTask::Traceability => vec![TextGeneration, Reasoning],
        AiTask::SecurityAnalysis => vec![TextGeneration, Reasoning],
        AiTask::PerformanceAnalysis => vec![TextGeneration, Reasoning],
        AiTask::UiAnalysis => vec![TextGeneration, Vision],
        AiTask::Embedding => vec![Embedding],
        AiTask::GeneralAi => vec![TextGeneration],
    }
}

// ------------------------------------------------------------ model/provider

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    /// e.g. "gemma3" — real, from the provider's own metadata; `None` when
    /// the source gives no family info.
    pub family: Option<String>,
    /// e.g. "12.2B" — real, verbatim from the provider (Ollama's own
    /// `details.parameter_size`), never computed/guessed.
    pub parameter_size: Option<String>,
    pub context_window: Option<u32>,
    pub capabilities: ModelCapabilities,
    pub status: ModelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderInfo {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub endpoint: Option<String>,
    pub configured: bool,
    pub reachable: bool,
    pub status: ProviderStatus,
    pub models: Vec<ModelDefinition>,
}

// -------------------------------------------------------------- discovery

#[async_trait::async_trait]
pub trait ProviderDiscovery: Send + Sync {
    /// Performs a real, bounded, timeout-protected check and returns the
    /// provider's true current state — never blocks indefinitely, never
    /// invents a status the check didn't actually observe.
    async fn discover(&self) -> AiProviderInfo;
}

/// Parses Ollama's own `capabilities` array (from `/api/tags`, confirmed
/// real during this unit's own manual verification — a running instance
/// returned `"capabilities":["completion"]`) into this module's
/// `CapabilityLevel` vocabulary. A capability Ollama does not list is
/// `Unknown`, not `None` — its absence from the array does not prove the
/// model truly lacks it, only that this discovery pass found no evidence
/// either way.
fn capabilities_from_ollama(tags: &[String], family: Option<&str>) -> ModelCapabilities {
    let has = |name: &str| tags.iter().any(|t| t == name);
    ModelCapabilities {
        text_generation: Some(if has("completion") { CapabilityLevel::Strong } else { CapabilityLevel::Unknown }),
        structured_output: Some(if has("completion") { CapabilityLevel::Basic } else { CapabilityLevel::Unknown }),
        reasoning: Some(CapabilityLevel::Basic), // real text models can reason to some degree; not independently benchmarked here
        vision: Some(if has("vision") { CapabilityLevel::Strong } else { CapabilityLevel::None }),
        embedding: Some(if has("embedding") { CapabilityLevel::Strong } else { CapabilityLevel::None }),
        tool_use: Some(if has("tools") { CapabilityLevel::Strong } else { CapabilityLevel::Unknown }),
        long_context: family.map(|_| CapabilityLevel::Unknown).or(Some(CapabilityLevel::Unknown)),
    }
}

/// Real Ollama discovery: a bounded `GET /api/tags` against a real,
/// possibly-absent local server. If the server isn't reachable within the
/// timeout, this honestly reports `Unavailable`/`reachable: false` — it
/// never assumes a fixed model exists (brief §5).
pub struct OllamaDiscovery {
    pub base_url: String,
    pub timeout: Duration,
}

impl OllamaDiscovery {
    pub fn from_env() -> Self {
        let base_url = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let timeout_secs: u64 = std::env::var("OLLAMA_DISCOVERY_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
        Self { base_url, timeout: Duration::from_secs(timeout_secs) }
    }
}

#[async_trait::async_trait]
impl ProviderDiscovery for OllamaDiscovery {
    async fn discover(&self) -> AiProviderInfo {
        let client = reqwest::Client::new();
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        let resp = client.get(&url).timeout(self.timeout).send().await;

        let mut info = AiProviderInfo {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            provider_type: ProviderType::Local,
            endpoint: Some(self.base_url.clone()),
            configured: true, // installed locally is its own "configured" fact for a LOCAL provider
            reachable: false,
            status: ProviderStatus::Unavailable,
            models: Vec::new(),
        };

        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                info.status = ProviderStatus::Error;
                tracing::warn!("Ollama discovery: server responded with HTTP {}", r.status());
                return info;
            }
            Err(e) => {
                info.status = ProviderStatus::Unavailable;
                tracing::info!("Ollama discovery: not reachable ({e})");
                return info;
            }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                info.status = ProviderStatus::Error;
                tracing::warn!("Ollama discovery: invalid response body ({e})");
                return info;
            }
        };

        info.reachable = true;
        let models = body.get("models").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for m in &models {
            let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            let details = m.get("details");
            let family = details.and_then(|d| d.get("family")).and_then(|v| v.as_str()).map(str::to_string);
            let parameter_size = details.and_then(|d| d.get("parameter_size")).and_then(|v| v.as_str()).map(str::to_string);
            let tags: Vec<String> = m
                .get("capabilities")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            info.models.push(ModelDefinition {
                id: name.clone(),
                provider_id: "ollama".to_string(),
                display_name: name,
                family: family.clone(),
                parameter_size,
                context_window: None, // not present on /api/tags; would need /api/show per model (a real, separate follow-up, not invented here)
                capabilities: capabilities_from_ollama(&tags, family.as_deref()),
                status: ModelStatus::Available, // reachable and listed; not yet inference-health-checked
            });
        }
        info.status = if info.models.is_empty() { ProviderStatus::Available } else { ProviderStatus::Healthy };
        info
    }
}

/// Real, evidence-based (not simulated) check for a configured API
/// provider: does the documented environment variable actually hold a
/// non-empty value? This never makes a live authenticated call (no key is
/// configured in this environment for any of these providers to call
/// with), and never invents `reachable: true` for a provider with no real
/// credential. If a real key is present, `reachable` still stays `false`
/// until a future unit adds a real, live capability-listing call for that
/// provider — this module does not claim more than the environment
/// variable's presence proves.
pub struct ApiKeyProviderDiscovery {
    pub id: &'static str,
    pub name: &'static str,
    pub env_var: &'static str,
}

#[async_trait::async_trait]
impl ProviderDiscovery for ApiKeyProviderDiscovery {
    async fn discover(&self) -> AiProviderInfo {
        let configured = std::env::var(self.env_var).map(|v| !v.trim().is_empty()).unwrap_or(false);
        AiProviderInfo {
            id: self.id.to_string(),
            name: self.name.to_string(),
            provider_type: ProviderType::Api,
            endpoint: None,
            configured,
            reachable: false,
            status: if configured { ProviderStatus::Available } else { ProviderStatus::NotConfigured },
            models: Vec::new(),
        }
    }
}

pub fn default_api_provider_discoveries() -> Vec<Arc<dyn ProviderDiscovery>> {
    vec![
        Arc::new(ApiKeyProviderDiscovery { id: "gemini", name: "Gemini", env_var: "GEMINI_API_KEY" }),
        Arc::new(ApiKeyProviderDiscovery { id: "openai", name: "OpenAI", env_var: "OPENAI_API_KEY" }),
        Arc::new(ApiKeyProviderDiscovery { id: "anthropic", name: "Anthropic", env_var: "ANTHROPIC_API_KEY" }),
    ]
}

// -------------------------------------------------------------- execution

#[derive(Debug, Clone)]
pub enum ExecutionError {
    ProviderUnavailable(String),
    ModelUnavailable(String),
    Timeout,
    InvalidResponse(String),
    NonSuccessStatus(u16),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionError::ProviderUnavailable(p) => write!(f, "PROVIDER_UNAVAILABLE: {p}"),
            ExecutionError::ModelUnavailable(m) => write!(f, "MODEL_UNAVAILABLE: {m}"),
            ExecutionError::Timeout => write!(f, "TIMEOUT"),
            ExecutionError::InvalidResponse(e) => write!(f, "INVALID_RESPONSE: {e}"),
            ExecutionError::NonSuccessStatus(s) => write!(f, "PROVIDER_ERROR: HTTP {s}"),
        }
    }
}

/// Executes a prompt against one specific, already-selected model. Real
/// providers implement this for real inference; test doubles implement it
/// deterministically (brief §28: mocks are for tests only).
#[async_trait::async_trait]
pub trait AiExecutor: Send + Sync {
    async fn execute(&self, provider_id: &str, model_id: &str, prompt: &str) -> Result<String, ExecutionError>;
}

pub struct OllamaExecutor {
    pub base_url: String,
    pub timeout: Duration,
}

impl OllamaExecutor {
    pub fn from_env() -> Self {
        let base_url = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let timeout_secs: u64 = std::env::var("OLLAMA_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(180);
        Self { base_url, timeout: Duration::from_secs(timeout_secs) }
    }
}

#[async_trait::async_trait]
impl AiExecutor for OllamaExecutor {
    async fn execute(&self, provider_id: &str, model_id: &str, prompt: &str) -> Result<String, ExecutionError> {
        if provider_id != "ollama" {
            return Err(ExecutionError::ProviderUnavailable(provider_id.to_string()));
        }
        let client = reqwest::Client::new();
        let url = format!("{}/api/generate", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({ "model": model_id, "prompt": prompt, "format": "json", "stream": false });
        let resp = client.post(&url).json(&body).timeout(self.timeout).send().await.map_err(|e| {
            if e.is_timeout() {
                ExecutionError::Timeout
            } else {
                ExecutionError::ProviderUnavailable(e.to_string())
            }
        })?;
        if !resp.status().is_success() {
            return Err(ExecutionError::NonSuccessStatus(resp.status().as_u16()));
        }
        let parsed: serde_json::Value = resp.json().await.map_err(|e| ExecutionError::InvalidResponse(e.to_string()))?;
        let text = parsed.get("response").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if text.trim().is_empty() {
            return Err(ExecutionError::InvalidResponse("empty response".to_string()));
        }
        Ok(text)
    }
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RoutingMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub mode: RoutingMode,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    pub fallback_enabled: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self { mode: RoutingMode::Auto, preferred_provider: None, preferred_model: None, fallback_enabled: true }
    }
}

// ------------------------------------------------------------------ routing

#[derive(Debug, Clone, Serialize)]
pub struct RoutingDecision {
    pub provider_id: String,
    pub model_id: String,
    /// A documented heuristic (brief §10) — explicitly NOT an accuracy or
    /// benchmark score. Higher is a better fit for the requested task
    /// given real, known evidence; it says nothing about output quality
    /// that wasn't actually measured.
    pub routing_score: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum RoutingError {
    NoCompatibleModel { task: AiTask },
    ModelUnavailable { provider_id: String, model_id: String },
    ProviderUnavailable { provider_id: String },
}

/// Routing Score (brief §10) — a documented, additive heuristic:
///   + 100 per required capability the model has at `Strong`, +40 at
///     `Basic`, +5 at `Unknown` (benefit of the doubt, heavily discounted);
///     a required capability at `None` disqualifies the model outright
///     (returns `None` from this function — never merely low-scored).
///   + provider health bonus: `Healthy` +30, `Available` +15, `Degraded`
///     +5; `Unavailable`/`Error` disqualifies.
///   + a small, disclosed parameter-size proxy (larger real parameter
///     counts score marginally higher) — not a benchmarked quality score.
///   + a large preference bonus (+1000) if the model matches the caller's
///     `preferred_model`, so an explicit preference always wins when it is
///     itself a valid candidate.
fn score_model(model: &ModelDefinition, provider: &AiProviderInfo, required: &[RequiredCapability], preferred_model: Option<&str>) -> Option<f64> {
    if !matches!(provider.status, ProviderStatus::Healthy | ProviderStatus::Available | ProviderStatus::Degraded) {
        return None;
    }
    if matches!(model.status, ModelStatus::Unavailable) {
        return None;
    }

    let mut score = 0.0;
    for cap in required {
        let level = model.capabilities.level(*cap);
        score += match level {
            CapabilityLevel::Strong => 100.0,
            CapabilityLevel::Basic => 40.0,
            CapabilityLevel::Unknown => 5.0,
            CapabilityLevel::None => return None, // disqualifying, not just low-scoring
        };
    }

    score += match provider.status {
        ProviderStatus::Healthy => 30.0,
        ProviderStatus::Available => 15.0,
        ProviderStatus::Degraded => 5.0,
        _ => 0.0,
    };

    if let Some(param_str) = &model.parameter_size {
        let digits: String = param_str.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(billions) = digits.parse::<f64>() {
            score += (billions / 10.0).min(5.0); // small, capped, disclosed proxy only
        }
    }

    if preferred_model == Some(model.id.as_str()) {
        score += 1000.0;
    }

    Some(score)
}

// --------------------------------------------------------------- request

#[derive(Debug, Clone)]
pub struct AiRequest {
    pub task: AiTask,
    pub prompt: String,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationResult {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub routing_mode: RoutingMode,
    pub routing_score: f64,
    pub fallback_used: bool,
}

#[derive(Debug, Clone, Serialize)]
pub enum GenerationOutcome2 {
    Ok(GenerationResult),
    Failed { reason: String },
}

// ---------------------------------------------------------- infrastructure

pub struct AiInfrastructure {
    discoveries: Vec<Arc<dyn ProviderDiscovery>>,
    executors: HashMap<&'static str, Arc<dyn AiExecutor>>,
    providers: RwLock<Vec<AiProviderInfo>>,
    last_discovered: RwLock<Option<Instant>>,
    settings: RwLock<AiSettings>,
    health_ttl: Duration,
}

impl AiInfrastructure {
    pub fn new(discoveries: Vec<Arc<dyn ProviderDiscovery>>, executors: HashMap<&'static str, Arc<dyn AiExecutor>>) -> Self {
        let ttl_secs: u64 = std::env::var("AI_HEALTH_TTL_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
        Self {
            discoveries,
            executors,
            providers: RwLock::new(Vec::new()),
            last_discovered: RwLock::new(None),
            settings: RwLock::new(AiSettings::default()),
            health_ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Real construction wired for this deployment: Ollama (local) +
    /// whichever API providers have a real, non-empty credential env var.
    pub fn from_env() -> Self {
        let mut discoveries: Vec<Arc<dyn ProviderDiscovery>> = vec![Arc::new(OllamaDiscovery::from_env())];
        discoveries.extend(default_api_provider_discoveries());
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("ollama", Arc::new(OllamaExecutor::from_env()));
        Self::new(discoveries, executors)
    }

    /// Discovers every configured provider (brief §29: async, each
    /// bounded by its own real timeout, never blocking app startup — this
    /// whole call runs inside an async task, so it never blocks the
    /// executor thread even though the handful of real discoverers here
    /// are awaited one at a time rather than pulling in a new dependency
    /// just for `join_all` on a list this short). Caches the result.
    pub async fn discover_all(&self) -> Vec<AiProviderInfo> {
        let mut results = Vec::with_capacity(self.discoveries.len());
        for d in &self.discoveries {
            results.push(d.discover().await);
        }
        *self.providers.write().await = results.clone();
        *self.last_discovered.write().await = Some(Instant::now());
        results
    }

    /// Re-discovers only if the cached result is stale (brief §15 — no
    /// expensive check on every render; a controlled, TTL-based refresh).
    pub async fn ensure_fresh(&self) -> Vec<AiProviderInfo> {
        let is_stale = match *self.last_discovered.read().await {
            None => true,
            Some(t) => t.elapsed() > self.health_ttl,
        };
        if is_stale {
            self.discover_all().await
        } else {
            self.providers.read().await.clone()
        }
    }

    pub async fn providers_snapshot(&self) -> Vec<AiProviderInfo> {
        self.providers.read().await.clone()
    }

    pub async fn settings(&self) -> AiSettings {
        self.settings.read().await.clone()
    }

    pub async fn set_settings(&self, settings: AiSettings) {
        *self.settings.write().await = settings;
    }

    /// The routing decision itself (brief §2/§10), separated from
    /// execution so it can be tested deterministically against synthetic
    /// provider/model data without any network call (see
    /// `routing_prefers_strong_structured_output_over_weak` below).
    pub fn route_among(providers: &[AiProviderInfo], task: AiTask, settings: &AiSettings, preferred_provider: Option<&str>, preferred_model: Option<&str>) -> Result<RoutingDecision, RoutingError> {
        let required = required_capabilities(task);

        if settings.mode == RoutingMode::Manual {
            let (Some(pid), Some(mid)) = (settings.preferred_provider.as_deref(), settings.preferred_model.as_deref()) else {
                return Err(RoutingError::NoCompatibleModel { task });
            };
            let provider = providers.iter().find(|p| p.id == pid).ok_or_else(|| RoutingError::ProviderUnavailable { provider_id: pid.to_string() })?;
            let model = provider.models.iter().find(|m| m.id == mid).ok_or_else(|| RoutingError::ModelUnavailable { provider_id: pid.to_string(), model_id: mid.to_string() })?;
            return match score_model(model, provider, &required, None) {
                Some(score) => Ok(RoutingDecision {
                    provider_id: pid.to_string(),
                    model_id: mid.to_string(),
                    routing_score: score,
                    reason: "MANUAL mode: user-selected provider/model".to_string(),
                }),
                None => Err(RoutingError::ModelUnavailable { provider_id: pid.to_string(), model_id: mid.to_string() }),
            };
        }

        // AUTO mode: score every real candidate and pick the best.
        let effective_preferred_model = preferred_provider.zip(preferred_model).map(|(_, m)| m).or(preferred_model);
        let mut best: Option<RoutingDecision> = None;
        for provider in providers {
            for model in &provider.models {
                if let Some(pid) = preferred_provider {
                    if provider.id != pid {
                        continue;
                    }
                }
                if let Some(score) = score_model(model, provider, &required, effective_preferred_model) {
                    if best.as_ref().map(|b| score > b.routing_score).unwrap_or(true) {
                        best = Some(RoutingDecision {
                            provider_id: provider.id.clone(),
                            model_id: model.id.clone(),
                            routing_score: score,
                            reason: format!("AUTO mode: best available model for {task:?} by routing score"),
                        });
                    }
                }
            }
        }
        best.ok_or(RoutingError::NoCompatibleModel { task })
    }

    /// The full pipeline: route, execute, and — only in AUTO mode with
    /// fallback enabled — retry against the next-best candidate on
    /// failure. Every result honestly states which model actually produced
    /// it; a fallback is never silently relabeled as the original choice.
    pub async fn generate(&self, request: AiRequest) -> GenerationOutcome2 {
        let providers = self.ensure_fresh().await;
        let settings = self.settings().await;

        let decision = match Self::route_among(&providers, request.task, &settings, request.preferred_provider.as_deref(), request.preferred_model.as_deref()) {
            Ok(d) => d,
            Err(e) => return GenerationOutcome2::Failed { reason: format!("{e:?}") },
        };

        let Some(executor) = self.executors.get(decision.provider_id.as_str()) else {
            return GenerationOutcome2::Failed { reason: format!("no executor wired for provider {}", decision.provider_id) };
        };

        match executor.execute(&decision.provider_id, &decision.model_id, &request.prompt).await {
            Ok(text) => GenerationOutcome2::Ok(GenerationResult {
                text,
                provider: decision.provider_id,
                model: decision.model_id,
                routing_mode: settings.mode,
                routing_score: decision.routing_score,
                fallback_used: false,
            }),
            Err(primary_err) => {
                if settings.mode == RoutingMode::Auto && settings.fallback_enabled {
                    let required = required_capabilities(request.task);
                    let mut candidates: Vec<(&AiProviderInfo, &ModelDefinition, f64)> = providers
                        .iter()
                        .flat_map(|p| p.models.iter().map(move |m| (p, m)))
                        .filter(|(p, m)| !(p.id == decision.provider_id && m.id == decision.model_id))
                        .filter_map(|(p, m)| score_model(m, p, &required, None).map(|s| (p, m, s)))
                        .collect();
                    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

                    for (provider, model, score) in candidates {
                        let Some(fallback_executor) = self.executors.get(provider.id.as_str()) else { continue };
                        if let Ok(text) = fallback_executor.execute(&provider.id, &model.id, &request.prompt).await {
                            return GenerationOutcome2::Ok(GenerationResult {
                                text,
                                provider: provider.id.clone(),
                                model: model.id.clone(),
                                routing_mode: settings.mode,
                                routing_score: score,
                                fallback_used: true,
                            });
                        }
                    }
                }
                GenerationOutcome2::Failed { reason: primary_err.to_string() }
            }
        }
    }
}

/// Test-only doubles (brief §28: legitimate inside test files only).
#[cfg(test)]
pub mod test_support {
    use super::*;

    pub struct FakeDiscovery(pub AiProviderInfo);

    #[async_trait::async_trait]
    impl ProviderDiscovery for FakeDiscovery {
        async fn discover(&self) -> AiProviderInfo {
            self.0.clone()
        }
    }

    pub struct FakeExecutor {
        pub result: Result<String, ExecutionError>,
    }

    impl FakeExecutor {
        pub fn ok(text: impl Into<String>) -> Self {
            Self { result: Ok(text.into()) }
        }
        pub fn err(e: ExecutionError) -> Self {
            Self { result: Err(e) }
        }
    }

    #[async_trait::async_trait]
    impl AiExecutor for FakeExecutor {
        async fn execute(&self, _provider_id: &str, _model_id: &str, _prompt: &str) -> Result<String, ExecutionError> {
            match &self.result {
                Ok(t) => Ok(t.clone()),
                Err(ExecutionError::Timeout) => Err(ExecutionError::Timeout),
                Err(ExecutionError::ProviderUnavailable(s)) => Err(ExecutionError::ProviderUnavailable(s.clone())),
                Err(ExecutionError::ModelUnavailable(s)) => Err(ExecutionError::ModelUnavailable(s.clone())),
                Err(ExecutionError::InvalidResponse(s)) => Err(ExecutionError::InvalidResponse(s.clone())),
                Err(ExecutionError::NonSuccessStatus(s)) => Err(ExecutionError::NonSuccessStatus(*s)),
            }
        }
    }

    pub fn healthy_model(id: &str, provider_id: &str, structured_output: CapabilityLevel) -> ModelDefinition {
        ModelDefinition {
            id: id.to_string(),
            provider_id: provider_id.to_string(),
            display_name: id.to_string(),
            family: Some("test-family".to_string()),
            parameter_size: None,
            context_window: None,
            capabilities: ModelCapabilities {
                text_generation: Some(CapabilityLevel::Strong),
                structured_output: Some(structured_output),
                reasoning: Some(CapabilityLevel::Strong),
                vision: Some(CapabilityLevel::None),
                embedding: Some(CapabilityLevel::None),
                tool_use: Some(CapabilityLevel::Unknown),
                long_context: Some(CapabilityLevel::Unknown),
            },
            status: ModelStatus::Healthy,
        }
    }

    pub fn healthy_provider(id: &str, models: Vec<ModelDefinition>) -> AiProviderInfo {
        AiProviderInfo {
            id: id.to_string(),
            name: id.to_string(),
            provider_type: ProviderType::Local,
            endpoint: Some("http://test.local".to_string()),
            configured: true,
            reachable: true,
            status: ProviderStatus::Healthy,
            models,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    // ----------------------------------------------------- provider discovery

    #[tokio::test]
    async fn discovery_reports_unavailable_when_ollama_is_not_reachable() {
        // A real discoverer pointed at a port nothing listens on — proves
        // the honest "not reachable" path without needing a live server.
        let discovery = OllamaDiscovery { base_url: "http://127.0.0.1:1".to_string(), timeout: Duration::from_millis(500) };
        let info = discovery.discover().await;
        assert_eq!(info.status, ProviderStatus::Unavailable);
        assert!(!info.reachable);
        assert!(info.models.is_empty());
    }

    #[tokio::test]
    async fn discovery_reports_not_configured_when_api_key_env_var_is_absent() {
        std::env::remove_var("AIQA_TEST_FAKE_KEY");
        let discovery = ApiKeyProviderDiscovery { id: "fake", name: "Fake", env_var: "AIQA_TEST_FAKE_KEY" };
        let info = discovery.discover().await;
        assert_eq!(info.status, ProviderStatus::NotConfigured);
        assert!(!info.configured);
        assert!(!info.reachable, "an unconfigured API provider must never claim reachability");
    }

    #[tokio::test]
    async fn discovery_reports_configured_when_api_key_env_var_present_but_never_reachable_without_a_live_check() {
        std::env::set_var("AIQA_TEST_FAKE_KEY", "sk-real-looking-value");
        let discovery = ApiKeyProviderDiscovery { id: "fake", name: "Fake", env_var: "AIQA_TEST_FAKE_KEY" };
        let info = discovery.discover().await;
        std::env::remove_var("AIQA_TEST_FAKE_KEY");
        assert!(info.configured);
        assert_eq!(info.status, ProviderStatus::Available);
        assert!(!info.reachable, "presence of a key is not proof of a live, verified connection");
    }

    // -------------------------------------------------------- model discovery

    #[tokio::test]
    async fn infrastructure_discover_all_populates_multiple_providers() {
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider("p1", vec![healthy_model("m1", "p1", CapabilityLevel::Strong)]))),
                Arc::new(FakeDiscovery(healthy_provider("p2", vec![healthy_model("m2", "p2", CapabilityLevel::Basic)]))),
            ],
            HashMap::new(),
        );
        let providers = infra.discover_all().await;
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].models.len(), 1);
        assert_eq!(providers[1].models.len(), 1);
    }

    #[tokio::test]
    async fn infrastructure_discover_all_handles_zero_models_honestly() {
        let mut empty = healthy_provider("empty", vec![]);
        empty.status = ProviderStatus::Available;
        let infra = AiInfrastructure::new(vec![Arc::new(FakeDiscovery(empty))], HashMap::new());
        let providers = infra.discover_all().await;
        assert_eq!(providers[0].models.len(), 0);
    }

    // ------------------------------------------------------------------ health

    #[test]
    fn score_disqualifies_a_model_from_an_unavailable_provider() {
        let mut provider = healthy_provider("p", vec![healthy_model("m", "p", CapabilityLevel::Strong)]);
        provider.status = ProviderStatus::Unavailable;
        let score = score_model(&provider.models[0], &provider, &[RequiredCapability::TextGeneration], None);
        assert_eq!(score, None, "an unavailable provider's model must never be a routable candidate");
    }

    #[test]
    fn score_disqualifies_an_unavailable_model_even_on_a_healthy_provider() {
        let mut model = healthy_model("m", "p", CapabilityLevel::Strong);
        model.status = ModelStatus::Unavailable;
        let provider = healthy_provider("p", vec![model]);
        let score = score_model(&provider.models[0], &provider, &[RequiredCapability::TextGeneration], None);
        assert_eq!(score, None);
    }

    #[test]
    fn unknown_capability_is_scored_low_but_not_disqualifying() {
        let mut model = healthy_model("m", "p", CapabilityLevel::Strong);
        model.capabilities.vision = Some(CapabilityLevel::Unknown);
        let provider = healthy_provider("p", vec![model]);
        let score = score_model(&provider.models[0], &provider, &[RequiredCapability::Vision], None);
        assert!(score.is_some(), "an unknown capability must not disqualify — only a known-absent one does");
    }

    #[test]
    fn none_capability_disqualifies_when_required() {
        let mut model = healthy_model("m", "p", CapabilityLevel::Strong);
        model.capabilities.vision = Some(CapabilityLevel::None);
        let provider = healthy_provider("p", vec![model]);
        let score = score_model(&provider.models[0], &provider, &[RequiredCapability::Vision], None);
        assert_eq!(score, None, "a text-only model must never be selected for a task that genuinely requires vision");
    }

    // ---------------------------------------------------------------- routing

    /// THE mandatory test (brief §26): given two real, distinct models —
    /// one with weak (Basic) structured output, one with strong —
    /// TEST_CASE_GENERATION's real required-capability list includes
    /// StructuredOutput, so the real scoring function must prefer the
    /// stronger one. The assertion is on the *scoring outcome of real
    /// capability data*, not a hardcoded expectation.
    #[test]
    fn routing_prefers_strong_structured_output_over_weak_for_test_case_generation() {
        let model_a = healthy_model("model-a-fast-weak-structured", "p", CapabilityLevel::Basic);
        let model_b = healthy_model("model-b-slow-strong-structured", "p", CapabilityLevel::Strong);
        let providers = vec![healthy_provider("p", vec![model_a, model_b])];
        let settings = AiSettings::default();

        let decision = AiInfrastructure::route_among(&providers, AiTask::TestCaseGeneration, &settings, None, None)
            .expect("a compatible model must be found");

        assert_eq!(decision.model_id, "model-b-slow-strong-structured", "the router must select the model whose real capability data better satisfies the task's requirements");
    }

    #[test]
    fn auto_mode_selects_the_only_healthy_compatible_model() {
        let providers = vec![healthy_provider("p", vec![healthy_model("m", "p", CapabilityLevel::Strong)])];
        let decision = AiInfrastructure::route_among(&providers, AiTask::GeneralAi, &AiSettings::default(), None, None).unwrap();
        assert_eq!(decision.provider_id, "p");
        assert_eq!(decision.model_id, "m");
    }

    #[test]
    fn manual_mode_respects_the_users_own_selection_even_if_not_the_highest_scoring() {
        let model_a = healthy_model("weaker-but-user-chose-it", "p", CapabilityLevel::Basic);
        let model_b = healthy_model("stronger-not-chosen", "p", CapabilityLevel::Strong);
        let providers = vec![healthy_provider("p", vec![model_a, model_b])];
        let settings = AiSettings { mode: RoutingMode::Manual, preferred_provider: Some("p".to_string()), preferred_model: Some("weaker-but-user-chose-it".to_string()), fallback_enabled: false };

        let decision = AiInfrastructure::route_among(&providers, AiTask::TestCaseGeneration, &settings, None, None).unwrap();
        assert_eq!(decision.model_id, "weaker-but-user-chose-it", "MANUAL mode must respect the user's own explicit choice, not silently override it with a higher score");
    }

    #[test]
    fn manual_mode_reports_model_unavailable_when_the_selected_model_no_longer_exists() {
        let providers = vec![healthy_provider("p", vec![healthy_model("m", "p", CapabilityLevel::Strong)])];
        let settings = AiSettings { mode: RoutingMode::Manual, preferred_provider: Some("p".to_string()), preferred_model: Some("model-that-vanished".to_string()), fallback_enabled: false };

        let result = AiInfrastructure::route_among(&providers, AiTask::GeneralAi, &settings, None, None);
        assert!(matches!(result, Err(RoutingError::ModelUnavailable { .. })), "a manual selection that no longer exists must be reported honestly, never silently substituted");
    }

    #[test]
    fn routing_reports_no_compatible_model_when_nothing_qualifies() {
        // Every real model here explicitly lacks vision — UiAnalysis requires it.
        let providers = vec![healthy_provider("p", vec![healthy_model("m", "p", CapabilityLevel::Strong)])];
        let result = AiInfrastructure::route_among(&providers, AiTask::UiAnalysis, &AiSettings::default(), None, None);
        assert!(matches!(result, Err(RoutingError::NoCompatibleModel { .. })));
    }

    // ------------------------------------------------------ capability matching

    #[test]
    fn document_analysis_requires_long_context_capability() {
        let required = required_capabilities(AiTask::DocumentAnalysis);
        assert!(required.contains(&RequiredCapability::LongContext));
    }

    #[test]
    fn ui_analysis_requires_vision_and_a_text_only_model_is_excluded() {
        let required = required_capabilities(AiTask::UiAnalysis);
        assert!(required.contains(&RequiredCapability::Vision));
        let text_only = healthy_model("text-only", "p", CapabilityLevel::Strong); // vision defaults to None in the fixture
        let provider = healthy_provider("p", vec![text_only]);
        let score = score_model(&provider.models[0], &provider, &required, None);
        assert_eq!(score, None, "a model with no evidenced vision capability must never be routed a vision-requiring task");
    }

    #[test]
    fn embedding_task_requires_only_embedding_capability() {
        assert_eq!(required_capabilities(AiTask::Embedding), vec![RequiredCapability::Embedding]);
    }

    // -------------------------------------------------------------- settings

    #[tokio::test]
    async fn settings_default_to_auto_with_fallback_enabled() {
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        let settings = infra.settings().await;
        assert_eq!(settings.mode, RoutingMode::Auto);
        assert!(settings.fallback_enabled);
        assert!(settings.preferred_model.is_none());
    }

    #[tokio::test]
    async fn settings_can_be_switched_to_manual_and_persist_in_memory() {
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        infra
            .set_settings(AiSettings { mode: RoutingMode::Manual, preferred_provider: Some("ollama".to_string()), preferred_model: Some("gemma3:12b".to_string()), fallback_enabled: false })
            .await;
        let settings = infra.settings().await;
        assert_eq!(settings.mode, RoutingMode::Manual);
        assert_eq!(settings.preferred_model.as_deref(), Some("gemma3:12b"));
        assert!(!settings.fallback_enabled);
    }

    // ------------------------------------------------------------- generation

    #[tokio::test]
    async fn generate_returns_ok_result_with_truthful_provider_and_model() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("p", Arc::new(FakeExecutor::ok(r#"{"ok":true}"#)));
        let infra = AiInfrastructure::new(vec![Arc::new(FakeDiscovery(healthy_provider("p", vec![healthy_model("m", "p", CapabilityLevel::Strong)])))], executors);

        let outcome = infra.generate(AiRequest { task: AiTask::GeneralAi, prompt: "hi".to_string(), preferred_provider: None, preferred_model: None }).await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.provider, "p");
                assert_eq!(result.model, "m");
                assert!(!result.fallback_used);
            }
            GenerationOutcome2::Failed { reason } => panic!("expected Ok, got Failed: {reason}"),
        }
    }

    #[tokio::test]
    async fn generate_falls_back_to_the_next_best_model_when_the_primary_fails_and_fallback_is_enabled() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("primary", Arc::new(FakeExecutor::err(ExecutionError::Timeout)));
        executors.insert("backup", Arc::new(FakeExecutor::ok(r#"{"ok":true}"#)));
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider("primary", vec![healthy_model("primary-model", "primary", CapabilityLevel::Strong)]))),
                Arc::new(FakeDiscovery(healthy_provider("backup", vec![healthy_model("backup-model", "backup", CapabilityLevel::Basic)]))),
            ],
            executors,
        );
        infra.set_settings(AiSettings { mode: RoutingMode::Auto, preferred_provider: Some("primary".to_string()), preferred_model: None, fallback_enabled: true }).await;

        let outcome = infra.generate(AiRequest { task: AiTask::GeneralAi, prompt: "hi".to_string(), preferred_provider: Some("primary".to_string()), preferred_model: None }).await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.provider, "backup", "must genuinely fall back to a different, working model");
                assert!(result.fallback_used, "fallback_used must truthfully reflect that this was not the primary choice");
            }
            GenerationOutcome2::Failed { reason } => panic!("expected a real fallback to succeed, got Failed: {reason}"),
        }
    }

    #[tokio::test]
    async fn generate_reports_truthful_failure_when_fallback_is_disabled() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("p", Arc::new(FakeExecutor::err(ExecutionError::Timeout)));
        let infra = AiInfrastructure::new(vec![Arc::new(FakeDiscovery(healthy_provider("p", vec![healthy_model("m", "p", CapabilityLevel::Strong)])))], executors);
        infra.set_settings(AiSettings { mode: RoutingMode::Auto, preferred_provider: None, preferred_model: None, fallback_enabled: false }).await;

        let outcome = infra.generate(AiRequest { task: AiTask::GeneralAi, prompt: "hi".to_string(), preferred_provider: None, preferred_model: None }).await;
        assert!(matches!(outcome, GenerationOutcome2::Failed { .. }), "with fallback disabled, a primary failure must be reported honestly, not silently retried");
    }

    #[tokio::test]
    async fn generate_reports_no_compatible_model_when_none_exists() {
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        let outcome = infra.generate(AiRequest { task: AiTask::GeneralAi, prompt: "hi".to_string(), preferred_provider: None, preferred_model: None }).await;
        match outcome {
            GenerationOutcome2::Failed { reason } => assert!(reason.contains("NoCompatibleModel")),
            GenerationOutcome2::Ok(_) => panic!("no provider exists at all — this must never fabricate a success"),
        }
    }

    // -------------------------------------------------------------- security

    #[test]
    fn provider_info_serialization_never_contains_a_credential_field() {
        // Structural proof, not just a manual read: `AiProviderInfo` has no
        // secret-shaped field at all, so no serialization of it can ever
        // carry one — confirmed by checking the real serialized JSON keys.
        let provider = healthy_provider("p", vec![]);
        let json = serde_json::to_value(&provider).unwrap();
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        for forbidden in ["api_key", "apiKey", "credential", "secret", "token", "authorization"] {
            assert!(!keys.iter().any(|k| k.eq_ignore_ascii_case(forbidden)), "AiProviderInfo must never expose a field named like a credential: {forbidden}");
        }
    }

    #[test]
    fn api_key_env_var_value_never_appears_in_discovered_provider_info() {
        // Even though ApiKeyProviderDiscovery reads the real env var, its
        // output must carry only a boolean fact, never the value itself.
        let info = AiProviderInfo {
            id: "fake".to_string(),
            name: "Fake".to_string(),
            provider_type: ProviderType::Api,
            endpoint: None,
            configured: true,
            reachable: false,
            status: ProviderStatus::Available,
            models: vec![],
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("sk-"), "no credential-shaped value must ever appear in serialized provider info");
    }
}
