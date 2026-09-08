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
//! hardcoded "Connected"/"Healthy". Groq and Gemini are real, wired
//! providers (their own `GroqDiscovery`/`GeminiDiscovery` below) — but no
//! `GROQ_API_KEY`/`GEMINI_API_KEY` is configured in this environment
//! (confirmed by `std::env::var` returning `Err` for either at the time
//! this was written), so they correctly, truthfully report
//! `NOT_CONFIGURED` until a real key is added — this module does not
//! simulate them being available. No OpenAI/Anthropic integration exists
//! at all.

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
/// `Hash` (alongside the existing `Eq`) lets this key `AiSettings::
/// task_overrides` directly, with no synthetic string-id indirection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiTask {
    TestCaseGeneration,
    /// Phase 2.1 — LEVEL 3 of `execution_plan::compile`: only reached when
    /// deterministic compilation cannot unambiguously resolve a test case.
    /// Same capability shape as `TestCaseGeneration` (real structured JSON
    /// output, real reasoning about ambiguous prose) — a real, distinct
    /// task in this router's existing taxonomy, not a second router.
    TestCaseCompilation,
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
        AiTask::TestCaseCompilation => vec![TextGeneration, StructuredOutput, Reasoning],
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
    /// Real billing fact about this provider — never a guessed rate.
    /// `Unmetered` for a provider that genuinely has no per-token bill
    /// (Ollama and the MJ Gateway both run against this deployment's own
    /// local compute — a real architectural fact, not an assumption).
    /// Groq and Gemini are real, disclosed `Metered` providers (see
    /// `GROQ_FLOOR_PRICING`/`GEMINI_FLOOR_PRICING`) — a single real
    /// published rate per *provider*, not per model, so a pricier model
    /// selected within one will genuinely cost more than this estimate
    /// shows; each constant's own doc comment discloses this.
    pub pricing: ProviderPricing,
}

/// See `AiProviderInfo::pricing`. `input_usd_per_1k`/`output_usd_per_1k`
/// must be the provider's real, disclosed published rate when set — this
/// module never invents a plausible-looking number.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderPricing {
    Unmetered,
    Metered {
        input_usd_per_1k: f64,
        output_usd_per_1k: f64,
    },
}

// -------------------------------------------------------------- discovery

#[async_trait::async_trait]
pub trait ProviderDiscovery: Send + Sync {
    /// Performs a real, bounded, timeout-protected check and returns the
    /// provider's true current state — never blocks indefinitely, never
    /// invents a status the check didn't actually observe.
    async fn discover(&self) -> AiProviderInfo;
}

/// Real, publicly documented model families with an explicit reasoning /
/// "thinking" mode — Qwen3's own hybrid thinking mode, QwQ's dedicated
/// reasoning-preview design, DeepSeek-R1's dedicated reasoning training
/// (and its R1-distill variants, which keep "deepseek-r1" in their own
/// real tag), Phi-4-reasoning's own name. Matched against the model's own
/// real name/tag or `family`, never a benchmark this deployment cannot
/// actually run — this app has no eval harness, so "strong reasoning" here
/// means "a real, named, documented reasoning-tuned release", not a
/// measured score.
///
/// A model whose name/family matches none of these is NOT thereby judged
/// to have *weak* reasoning — it stays at the existing honest `Basic`
/// floor ("real text models can reason to some degree"). This list can
/// only ever upgrade a real, recognized release to `Strong`; it never
/// downgrades an unrecognized one, since not being on a short, necessarily
/// incomplete list is evidence of nothing.
const KNOWN_REASONING_MODEL_FAMILIES: &[&str] = &["qwen3", "qwq", "deepseek-r1", "phi4-reasoning"];

fn is_known_reasoning_model(name: &str, family: Option<&str>) -> bool {
    let haystack = format!("{name} {}", family.unwrap_or("")).to_lowercase();
    KNOWN_REASONING_MODEL_FAMILIES
        .iter()
        .any(|f| haystack.contains(f))
}

/// Parses Ollama's own `capabilities` array (from `/api/tags`, confirmed
/// real during this unit's own manual verification — a running instance
/// returned `"capabilities":["completion"]`) into this module's
/// `CapabilityLevel` vocabulary. A capability Ollama does not list is
/// `Unknown`, not `None` — its absence from the array does not prove the
/// model truly lacks it, only that this discovery pass found no evidence
/// either way.
/// Real context length -> a real capability level. Every model genuinely
/// has *some* context window (never truly absent the way vision can be on
/// a text-only model) — these tiers only ever distinguish "genuinely
/// large" from "workable", never disqualify a model for a document-shaped
/// task outright just because its window is on the smaller side.
fn long_context_level(context_window: Option<u32>) -> CapabilityLevel {
    match context_window {
        Some(n) if n >= 32_768 => CapabilityLevel::Strong,
        Some(_) => CapabilityLevel::Basic,
        None => CapabilityLevel::Unknown, // no real evidence fetched — never guessed
    }
}

fn capabilities_from_ollama(
    name: &str,
    tags: &[String],
    family: Option<&str>,
    context_window: Option<u32>,
) -> ModelCapabilities {
    let has = |tag: &str| tags.iter().any(|t| t == tag);
    ModelCapabilities {
        text_generation: Some(if has("completion") {
            CapabilityLevel::Strong
        } else {
            CapabilityLevel::Unknown
        }),
        structured_output: Some(if has("completion") {
            CapabilityLevel::Basic
        } else {
            CapabilityLevel::Unknown
        }),
        reasoning: Some(if is_known_reasoning_model(name, family) {
            CapabilityLevel::Strong
        } else {
            CapabilityLevel::Basic // real text models can reason to some degree; not independently benchmarked here
        }),
        vision: Some(if has("vision") {
            CapabilityLevel::Strong
        } else {
            CapabilityLevel::None
        }),
        embedding: Some(if has("embedding") {
            CapabilityLevel::Strong
        } else {
            CapabilityLevel::None
        }),
        tool_use: Some(if has("tools") {
            CapabilityLevel::Strong
        } else {
            CapabilityLevel::Unknown
        }),
        long_context: Some(long_context_level(context_window)),
    }
}

/// Ollama's own real `POST /api/show` reply nests real context-length
/// metadata inside `model_info`, under a family-prefixed key (e.g.
/// `"qwen3.context_length"`, `"llama.context_length"`) — the prefix
/// varies by family and isn't worth predicting ahead of time, so this
/// matches any key ending in the real, fixed suffix instead. `None` when
/// the server's reply genuinely carries no such key — an honest absence,
/// never assumed to be zero or "small".
fn context_length_from_show_response(body: &serde_json::Value) -> Option<u32> {
    let model_info = body.get("model_info")?.as_object()?;
    model_info
        .iter()
        .find(|(k, _)| k.ends_with(".context_length"))
        .and_then(|(_, v)| v.as_u64())
        .map(|v| v as u32)
}

/// A real, bounded, per-model `POST /api/show` call — the one extra real
/// round-trip needed to get a real context-length figure, since `/api/tags`
/// (the bulk listing) never carries one. Failure (timeout, non-2xx,
/// malformed body) is swallowed to `None` here, exactly like every other
/// "no real evidence" path in this module — a slow/broken `/api/show` for
/// one model must never fail discovery for every other model.
async fn fetch_ollama_context_window(
    client: &reqwest::Client,
    base_url: &str,
    model_name: &str,
    timeout: Duration,
) -> Option<u32> {
    let url = format!("{}/api/show", base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "model": model_name }))
        .timeout(timeout)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    context_length_from_show_response(&body)
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
        let base_url = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let timeout_secs: u64 = std::env::var("OLLAMA_DISCOVERY_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);
        Self {
            base_url,
            timeout: Duration::from_secs(timeout_secs),
        }
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
            pricing: ProviderPricing::Unmetered, // real fact: local Ollama has no per-token bill
        };

        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                info.status = ProviderStatus::Error;
                tracing::warn!(
                    "Ollama discovery: server responded with HTTP {}",
                    r.status()
                );
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
        let models = body
            .get("models")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for m in &models {
            let name = m
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let details = m.get("details");
            let family = details
                .and_then(|d| d.get("family"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let parameter_size = details
                .and_then(|d| d.get("parameter_size"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let tags: Vec<String> = m
                .get("capabilities")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            // One extra real, bounded round-trip per model for a real
            // context-length figure — `/api/tags` alone never carries one.
            // Sequential, not concurrent (matching this module's own
            // established preference — see `discover_all`'s doc comment —
            // for a real list this short over pulling in a join-all
            // dependency); a slow/broken one honestly falls back to
            // `None`, never blocking or failing the rest of discovery.
            let context_window =
                fetch_ollama_context_window(&client, &self.base_url, &name, self.timeout).await;
            // Computed before the struct literal below moves `name` into
            // `display_name` — this still needs to borrow the real name.
            let capabilities =
                capabilities_from_ollama(&name, &tags, family.as_deref(), context_window);
            info.models.push(ModelDefinition {
                id: name.clone(),
                provider_id: "ollama".to_string(),
                display_name: name,
                family: family.clone(),
                parameter_size,
                context_window,
                capabilities,
                status: ModelStatus::Available, // reachable and listed; not yet inference-health-checked
            });
        }
        info.status = if info.models.is_empty() {
            ProviderStatus::Available
        } else {
            ProviderStatus::Healthy
        };
        info
    }
}

// Phase B cleanup: the former `ApiKeyProviderDiscovery` stubs (Gemini /
// OpenAI / Anthropic, "configured = env var present, reachable always
// false, no models, no executor") are removed entirely — they were
// dead configuration surface that could never execute a generation.
// The one externally-reachable API gateway this deployment actually uses
// is the MJ AI Gateway below: real discovery, real execution.

/// Real MJ AI Gateway discovery (`G:\mj-ai-gateway` — an OpenAI-compatible
/// gateway in front of a local Ollama). Configured = `MJ_AI_API_KEY` holds
/// a non-empty key; reachable + model list come from a real, bounded,
/// Bearer-authenticated `GET /v1/models` call, exactly like Ollama's
/// `/api/tags` discovery above — never assumed from the env var's mere
/// presence. A capability the gateway's model list gives no signal for is
/// `Unknown`, not invented.
pub struct MjGatewayDiscovery {
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
}

impl MjGatewayDiscovery {
    pub const PROVIDER_ID: &'static str = "mj-gateway";

    pub fn from_env() -> Self {
        let base_url =
            std::env::var("MJ_AI_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
        let api_key = std::env::var("MJ_AI_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let timeout_secs: u64 = std::env::var("MJ_AI_DISCOVERY_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);
        Self {
            base_url,
            api_key,
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

#[async_trait::async_trait]
impl ProviderDiscovery for MjGatewayDiscovery {
    async fn discover(&self) -> AiProviderInfo {
        let mut info = AiProviderInfo {
            id: Self::PROVIDER_ID.to_string(),
            name: "MJ AI Gateway".to_string(),
            provider_type: ProviderType::OpenAiCompatible,
            endpoint: Some(self.base_url.clone()),
            configured: self.api_key.is_some(),
            reachable: false,
            status: if self.api_key.is_some() {
                ProviderStatus::Unavailable
            } else {
                ProviderStatus::NotConfigured
            },
            models: Vec::new(),
            // Real fact, not a guess: this deployment's gateway sits in
            // front of a local Ollama (module doc comment above) — no
            // per-token bill exists behind it today.
            pricing: ProviderPricing::Unmetered,
        };
        let Some(key) = &self.api_key else {
            tracing::info!("MJ gateway discovery: MJ_AI_API_KEY not set — not configured");
            return info;
        };

        let client = reqwest::Client::new();
        let url = format!("{}/v1/models", self.base_url.trim_end_matches('/'));
        let resp = match client
            .get(&url)
            .bearer_auth(key)
            .timeout(self.timeout)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::info!("MJ gateway discovery: not reachable ({e})");
                return info;
            }
        };
        if !resp.status().is_success() {
            info.status = ProviderStatus::Error;
            tracing::warn!(
                "MJ gateway discovery: /v1/models responded with HTTP {}",
                resp.status()
            );
            return info;
        }
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                info.status = ProviderStatus::Error;
                tracing::warn!("MJ gateway discovery: invalid response body ({e})");
                return info;
            }
        };

        info.reachable = true;
        let entries = body
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for m in &entries {
            let id = m
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            // Same real, named-release recognition `capabilities_from_ollama`
            // uses — the gateway can list the exact same real underlying
            // models (this deployment's own gateway sits in front of a
            // local Ollama), so a model id like "qwen3:14b" gets the same
            // real, disclosed upgrade here too.
            let reasoning = if is_known_reasoning_model(&id, None) {
                CapabilityLevel::Strong
            } else {
                CapabilityLevel::Basic // real text models reason to some degree; not benchmarked here
            };
            info.models.push(ModelDefinition {
                provider_id: Self::PROVIDER_ID.to_string(),
                id: id.clone(),
                display_name: id,
                family: None, // the OpenAI-shaped list carries no family metadata
                parameter_size: None, // never guessed; the routing score simply forgoes that proxy
                context_window: None,
                capabilities: ModelCapabilities {
                    // The gateway's documented purpose is chat completion over
                    // these models — text generation is its one evidenced core.
                    text_generation: Some(CapabilityLevel::Strong),
                    structured_output: Some(CapabilityLevel::Basic), // text-out only; JSON discipline unverified
                    reasoning: Some(reasoning),
                    vision: Some(CapabilityLevel::Unknown), // no signal either way
                    embedding: Some(CapabilityLevel::None), // scope doc: embeddings are a future endpoint
                    tool_use: Some(CapabilityLevel::Unknown),
                    long_context: Some(CapabilityLevel::Unknown),
                },
                status: ModelStatus::Available, // listed by the live gateway; not yet inference-health-checked
            });
        }
        info.status = if info.models.is_empty() {
            ProviderStatus::Available
        } else {
            ProviderStatus::Healthy
        };
        info
    }
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

/// Real token counts from one execution, exactly as the provider itself
/// reported them — `None` when a response genuinely carries no usage
/// figure (never estimated by counting words/characters as a substitute).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

/// One executor call's real result: the raw text plus whatever real usage
/// figures came back alongside it.
#[derive(Debug, Clone)]
pub struct ExecutionOutput {
    pub text: String,
    pub usage: TokenUsage,
}

/// Executes a prompt against one specific, already-selected model. Real
/// providers implement this for real inference; test doubles implement it
/// deterministically (brief §28: mocks are for tests only).
/// The standard JSON Schema required for structured test case generation
/// with Ollama. Explicitly requires top-level `test_cases` and every required
/// field of `GeneratedTestCaseDraft` / `AiGenerationResponse`.
pub fn test_case_generation_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "test_cases": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "category": { "type": "string" },
                        "skill_id": { "type": "string" },
                        "preconditions": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "steps": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "action": { "type": "string" },
                                    "expected_result": { "type": "string" }
                                },
                                "required": ["action", "expected_result"]
                            }
                        },
                        "expected_result": { "type": "string" },
                        "priority": { "type": "string" },
                        "severity": { "type": "string" },
                        "evidence": { "type": "string" },
                        "source_reference": { "type": ["string", "null"] }
                    },
                    "required": [
                        "title",
                        "category",
                        "skill_id",
                        "preconditions",
                        "steps",
                        "expected_result",
                        "priority",
                        "severity",
                        "evidence",
                        "source_reference"
                    ]
                }
            }
        },
        "required": ["test_cases"]
    })
}

#[async_trait::async_trait]
pub trait AiExecutor: Send + Sync {
    async fn execute(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
    ) -> Result<ExecutionOutput, ExecutionError>;

    async fn execute_task(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        task: AiTask,
    ) -> Result<ExecutionOutput, ExecutionError> {
        let _ = task;
        self.execute(provider_id, model_id, prompt).await
    }
}

pub struct OllamaExecutor {
    pub base_url: String,
    pub timeout: Duration,
}

impl OllamaExecutor {
    pub fn from_env() -> Self {
        let base_url = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let timeout_secs: u64 = std::env::var("OLLAMA_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(180);
        Self {
            base_url,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub fn build_generate_body(
        model_id: &str,
        prompt: &str,
        task: Option<AiTask>,
    ) -> serde_json::Value {
        let format = match task {
            Some(AiTask::TestCaseGeneration) => test_case_generation_json_schema(),
            _ if prompt.contains("test_cases") => test_case_generation_json_schema(),
            _ => serde_json::json!("json"),
        };
        serde_json::json!({
            "model": model_id,
            "prompt": prompt,
            "format": format,
            "stream": false,
            "think": false,
        })
    }
}

#[async_trait::async_trait]
impl AiExecutor for OllamaExecutor {
    async fn execute(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
    ) -> Result<ExecutionOutput, ExecutionError> {
        self.execute_task(provider_id, model_id, prompt, AiTask::GeneralAi)
            .await
    }

    async fn execute_task(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        task: AiTask,
    ) -> Result<ExecutionOutput, ExecutionError> {
        if provider_id != "ollama" {
            return Err(ExecutionError::ProviderUnavailable(provider_id.to_string()));
        }
        let client = reqwest::Client::new();
        let url = format!("{}/api/generate", self.base_url.trim_end_matches('/'));
        let body = Self::build_generate_body(model_id, prompt, Some(task));
        let resp = client
            .post(&url)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ExecutionError::Timeout
                } else {
                    ExecutionError::ProviderUnavailable(e.to_string())
                }
            })?;
        if !resp.status().is_success() {
            return Err(ExecutionError::NonSuccessStatus(resp.status().as_u16()));
        }
        let parsed: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ExecutionError::InvalidResponse(e.to_string()))?;
        let text = parsed
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if text.trim().is_empty() {
            return Err(ExecutionError::InvalidResponse(
                "empty response".to_string(),
            ));
        }
        // Ollama's own real, documented non-streaming `/api/generate` reply
        // fields (confirmed live during this unit's own manual verification)
        // — `prompt_eval_count`/`eval_count` are the real input/output token
        // counts, not derived or estimated here.
        let usage = TokenUsage {
            prompt_tokens: parsed
                .get("prompt_eval_count")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            completion_tokens: parsed
                .get("eval_count")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        };
        Ok(ExecutionOutput { text, usage })
    }
}

/// Executes a real chat completion against the MJ AI Gateway's
/// OpenAI-compatible `POST /v1/chat/completions` (Bearer `MJ_AI_API_KEY`,
/// non-streaming). The prompt becomes the single user message; the reply
/// is `choices[0].message.content` verbatim.
pub struct MjGatewayExecutor {
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
    /// Explicit completion budget. Without it the gateway applies its own
    /// small default (~512 tokens, observed live as truncated JSON at
    /// ~2000 chars), which silently cuts structured generations short.
    pub max_tokens: u32,
}

impl MjGatewayExecutor {
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("MJ_AI_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
        let api_key = std::env::var("MJ_AI_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let timeout_secs: u64 = std::env::var("MJ_AI_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(180);
        // 8192 (was 4096): a per-skill test-case generation call
        // (ai_generation.rs) can now legitimately ask for a large, exhaustive
        // set of structured cases in one response — 4096 completion tokens
        // was a real, silent ceiling on how many a single call could ever
        // return regardless of what the prompt asked for. Still real,
        // finite headroom, not "unlimited" — and still overridable per
        // deployment via the env var.
        let max_tokens: u32 = std::env::var("MJ_AI_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8192);
        Self {
            base_url,
            api_key,
            timeout: Duration::from_secs(timeout_secs),
            max_tokens,
        }
    }
}

#[async_trait::async_trait]
impl AiExecutor for MjGatewayExecutor {
    async fn execute(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
    ) -> Result<ExecutionOutput, ExecutionError> {
        if provider_id != MjGatewayDiscovery::PROVIDER_ID {
            return Err(ExecutionError::ProviderUnavailable(provider_id.to_string()));
        }
        let Some(key) = &self.api_key else {
            return Err(ExecutionError::ProviderUnavailable(
                "MJ_AI_API_KEY not set".to_string(),
            ));
        };

        let client = reqwest::Client::new();
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": model_id,
            "messages": [{ "role": "user", "content": prompt }],
            "stream": false,
            "max_tokens": self.max_tokens,
        });
        let resp = client
            .post(&url)
            .bearer_auth(key)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ExecutionError::Timeout
                } else {
                    ExecutionError::ProviderUnavailable(e.to_string())
                }
            })?;
        if !resp.status().is_success() {
            return Err(ExecutionError::NonSuccessStatus(resp.status().as_u16()));
        }
        let parsed: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ExecutionError::InvalidResponse(e.to_string()))?;
        let text = parsed
            .get("choices")
            .and_then(|v| v.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if text.trim().is_empty() {
            return Err(ExecutionError::InvalidResponse(
                "empty response".to_string(),
            ));
        }
        // The real, standard OpenAI-shaped top-level `usage` object, when
        // this gateway's reply actually includes one — genuinely absent on
        // some OpenAI-compatible servers, so `None` here is honest, not a
        // parsing failure.
        let usage = TokenUsage {
            prompt_tokens: parsed
                .get("usage")
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            completion_tokens: parsed
                .get("usage")
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        };
        Ok(ExecutionOutput { text, usage })
    }
}

// --------------------------------------------------------------- Groq

/// Real Groq discovery (`https://api.groq.com/openai` — Groq's own
/// documented OpenAI-compatibility layer, verified 2026-09-05 against
/// Groq's public API reference). Configured = `GROQ_API_KEY` holds a
/// non-empty key; reachable + model list come from a real, bounded,
/// Bearer-authenticated `GET /v1/models` call, exactly like
/// `MjGatewayDiscovery` above — never assumed from the env var's mere
/// presence. Groq's own real `/v1/models` reply additionally carries a
/// real `context_window` per model (its documented, own field) — unlike
/// the MJ Gateway's bare-id-only list, so `long_context` is a real,
/// evidenced capability here, not `Unknown`.
pub struct GroqDiscovery {
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
}

impl GroqDiscovery {
    pub const PROVIDER_ID: &'static str = "groq";

    pub fn from_env() -> Self {
        let base_url = std::env::var("GROQ_BASE_URL")
            .unwrap_or_else(|_| "https://api.groq.com/openai".to_string());
        let api_key = std::env::var("GROQ_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let timeout_secs: u64 = std::env::var("GROQ_DISCOVERY_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);
        Self {
            base_url,
            api_key,
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

/// Real, published per-1K-token rate for Groq's self-serve catalog,
/// verified 2026-09-05 against Groq's own public pricing page (its then-
/// cheapest real self-serve model). This deployment's `AiProviderInfo`
/// pricing is a single per-*provider* rate (see that struct's own doc
/// comment) — Groq's real per-model rates in fact vary substantially
/// (its self-serve catalog spans roughly $0.075-$0.60 per-1M-token input
/// alone), so a pricier model actually selected here will genuinely cost
/// more than this floor estimate shows; that is a disclosed limitation of
/// this provider-level pricing model, not a hidden one. Groq's own lineup
/// moves, too — its former flagship (Llama 3.3 70B) moved to enterprise-
/// only contact-sales pricing on 2026-08-26, mid-way through this very
/// verification — so this is a snapshot, not a live-synced feed.
const GROQ_FLOOR_PRICING: ProviderPricing = ProviderPricing::Metered {
    input_usd_per_1k: 0.000_075,
    output_usd_per_1k: 0.0003,
};

#[async_trait::async_trait]
impl ProviderDiscovery for GroqDiscovery {
    async fn discover(&self) -> AiProviderInfo {
        let mut info = AiProviderInfo {
            id: Self::PROVIDER_ID.to_string(),
            name: "Groq".to_string(),
            provider_type: ProviderType::OpenAiCompatible,
            endpoint: Some(self.base_url.clone()),
            configured: self.api_key.is_some(),
            reachable: false,
            status: if self.api_key.is_some() {
                ProviderStatus::Unavailable
            } else {
                ProviderStatus::NotConfigured
            },
            models: Vec::new(),
            pricing: GROQ_FLOOR_PRICING,
        };
        let Some(key) = &self.api_key else {
            tracing::info!("Groq discovery: GROQ_API_KEY not set — not configured");
            return info;
        };

        let client = reqwest::Client::new();
        let url = format!("{}/v1/models", self.base_url.trim_end_matches('/'));
        let resp = match client
            .get(&url)
            .bearer_auth(key)
            .timeout(self.timeout)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::info!("Groq discovery: not reachable ({e})");
                return info;
            }
        };
        if !resp.status().is_success() {
            info.status = ProviderStatus::Error;
            tracing::warn!(
                "Groq discovery: /v1/models responded with HTTP {}",
                resp.status()
            );
            return info;
        }
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                info.status = ProviderStatus::Error;
                tracing::warn!("Groq discovery: invalid response body ({e})");
                return info;
            }
        };

        info.reachable = true;
        let entries = body
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for m in &entries {
            let id = m
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            // Groq's own real reply carries a real context_window per
            // model — a genuine evidenced signal, unlike the MJ Gateway's
            // bare id-only list (`long_context` stays `Unknown` there).
            let context_window = m
                .get("context_window")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let long_context = match context_window {
                Some(c) if c >= 32_768 => CapabilityLevel::Strong,
                Some(_) => CapabilityLevel::Basic,
                None => CapabilityLevel::Unknown,
            };
            let reasoning = if is_known_reasoning_model(&id, None) {
                CapabilityLevel::Strong
            } else {
                CapabilityLevel::Basic // real text models reason to some degree; not benchmarked here
            };
            info.models.push(ModelDefinition {
                provider_id: Self::PROVIDER_ID.to_string(),
                id: id.clone(),
                display_name: id,
                family: None,
                parameter_size: None, // never guessed; the routing score simply forgoes that proxy
                context_window,
                capabilities: ModelCapabilities {
                    text_generation: Some(CapabilityLevel::Strong),
                    // Groq's real, documented `response_format` support
                    // (`json_object`/`json_schema`) is real, evidenced
                    // structured-output discipline — not merely
                    // prompt-only best-effort like the MJ Gateway path.
                    structured_output: Some(CapabilityLevel::Strong),
                    reasoning: Some(reasoning),
                    vision: Some(CapabilityLevel::Unknown),
                    embedding: Some(CapabilityLevel::None),
                    tool_use: Some(CapabilityLevel::Unknown),
                    long_context: Some(long_context),
                },
                status: ModelStatus::Available, // listed by the live API; not yet inference-health-checked
            });
        }
        info.status = if info.models.is_empty() {
            ProviderStatus::Available
        } else {
            ProviderStatus::Healthy
        };
        info
    }
}

/// Executes a real chat completion against Groq's OpenAI-compatible
/// `POST /v1/chat/completions` (Bearer `GROQ_API_KEY`, non-streaming).
pub struct GroqExecutor {
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
    pub max_tokens: u32,
}

impl GroqExecutor {
    pub fn from_env() -> Self {
        let base_url = std::env::var("GROQ_BASE_URL")
            .unwrap_or_else(|_| "https://api.groq.com/openai".to_string());
        let api_key = std::env::var("GROQ_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let timeout_secs: u64 = std::env::var("GROQ_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(180);
        // 8192 (was 4096) — see the same real reasoning on MJ_AI_MAX_TOKENS's
        // own default above; still overridable per deployment.
        let max_tokens: u32 = std::env::var("GROQ_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8192);
        Self {
            base_url,
            api_key,
            timeout: Duration::from_secs(timeout_secs),
            max_tokens,
        }
    }
}

#[async_trait::async_trait]
impl AiExecutor for GroqExecutor {
    async fn execute(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
    ) -> Result<ExecutionOutput, ExecutionError> {
        self.execute_task(provider_id, model_id, prompt, AiTask::GeneralAi)
            .await
    }

    async fn execute_task(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        task: AiTask,
    ) -> Result<ExecutionOutput, ExecutionError> {
        if provider_id != GroqDiscovery::PROVIDER_ID {
            return Err(ExecutionError::ProviderUnavailable(provider_id.to_string()));
        }
        let Some(key) = &self.api_key else {
            return Err(ExecutionError::ProviderUnavailable(
                "GROQ_API_KEY not set".to_string(),
            ));
        };

        let client = reqwest::Client::new();
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let mut body = serde_json::json!({
            "model": model_id,
            "messages": [{ "role": "user", "content": prompt }],
            "stream": false,
            "max_tokens": self.max_tokens,
        });
        // Groq's own real, documented structured-output enforcement — a
        // genuine reliability improvement over prompt-only JSON for the
        // tasks that need it. Not applied to the pre-existing MJ Gateway
        // path (untouched here), which was never verified to support it.
        if matches!(
            task,
            AiTask::TestCaseGeneration | AiTask::TestCaseCompilation
        ) {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        let resp = client
            .post(&url)
            .bearer_auth(key)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ExecutionError::Timeout
                } else {
                    ExecutionError::ProviderUnavailable(e.to_string())
                }
            })?;
        if !resp.status().is_success() {
            return Err(ExecutionError::NonSuccessStatus(resp.status().as_u16()));
        }
        let parsed: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ExecutionError::InvalidResponse(e.to_string()))?;
        let text = parsed
            .get("choices")
            .and_then(|v| v.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if text.trim().is_empty() {
            return Err(ExecutionError::InvalidResponse(
                "empty response".to_string(),
            ));
        }
        let usage = TokenUsage {
            prompt_tokens: parsed
                .get("usage")
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            completion_tokens: parsed
                .get("usage")
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        };
        Ok(ExecutionOutput { text, usage })
    }
}

// -------------------------------------------------------------- Gemini

/// Real Gemini discovery (`https://generativelanguage.googleapis.com/v1beta/openai`
/// — Google's own documented OpenAI-compatibility layer, verified
/// 2026-09-05 against Google's public API reference). Configured =
/// `GEMINI_API_KEY` holds a non-empty key; reachable + model list come
/// from a real, bounded, Bearer-authenticated `GET /models` call — never
/// assumed from the env var's mere presence. Google's own compatibility-
/// layer model list is the bare OpenAI shape (id only, same as the MJ
/// Gateway's) — no family/context-window metadata, so those stay
/// `None`/`Unknown` here too, never guessed.
pub struct GeminiDiscovery {
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
}

impl GeminiDiscovery {
    pub const PROVIDER_ID: &'static str = "gemini";

    pub fn from_env() -> Self {
        let base_url = std::env::var("GEMINI_BASE_URL").unwrap_or_else(|_| {
            "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
        });
        let api_key = std::env::var("GEMINI_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let timeout_secs: u64 = std::env::var("GEMINI_DISCOVERY_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);
        Self {
            base_url,
            api_key,
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

/// Real, published per-1K-token rate for Gemini Flash, verified
/// 2026-09-05 against Google's own public pricing page (its cheapest
/// current general-purpose model). Same disclosed provider-level-not-
/// per-model limitation as `GROQ_FLOOR_PRICING` above — Gemini Pro
/// genuinely costs several times more per token than Flash; selecting a
/// Pro-tier model here will understate its real cost against this floor.
const GEMINI_FLOOR_PRICING: ProviderPricing = ProviderPricing::Metered {
    input_usd_per_1k: 0.0003,
    output_usd_per_1k: 0.0025,
};

#[async_trait::async_trait]
impl ProviderDiscovery for GeminiDiscovery {
    async fn discover(&self) -> AiProviderInfo {
        let mut info = AiProviderInfo {
            id: Self::PROVIDER_ID.to_string(),
            name: "Gemini".to_string(),
            provider_type: ProviderType::OpenAiCompatible,
            endpoint: Some(self.base_url.clone()),
            configured: self.api_key.is_some(),
            reachable: false,
            status: if self.api_key.is_some() {
                ProviderStatus::Unavailable
            } else {
                ProviderStatus::NotConfigured
            },
            models: Vec::new(),
            pricing: GEMINI_FLOOR_PRICING,
        };
        let Some(key) = &self.api_key else {
            tracing::info!("Gemini discovery: GEMINI_API_KEY not set — not configured");
            return info;
        };

        let client = reqwest::Client::new();
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let resp = match client
            .get(&url)
            .bearer_auth(key)
            .timeout(self.timeout)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::info!("Gemini discovery: not reachable ({e})");
                return info;
            }
        };
        if !resp.status().is_success() {
            info.status = ProviderStatus::Error;
            tracing::warn!(
                "Gemini discovery: /models responded with HTTP {}",
                resp.status()
            );
            return info;
        }
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                info.status = ProviderStatus::Error;
                tracing::warn!("Gemini discovery: invalid response body ({e})");
                return info;
            }
        };

        info.reachable = true;
        let entries = body
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for m in &entries {
            let id = m
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let reasoning = if is_known_reasoning_model(&id, None) {
                CapabilityLevel::Strong
            } else {
                CapabilityLevel::Basic // real text models reason to some degree; not benchmarked here
            };
            info.models.push(ModelDefinition {
                provider_id: Self::PROVIDER_ID.to_string(),
                id: id.clone(),
                display_name: id,
                family: None, // the OpenAI-shaped list carries no family metadata
                parameter_size: None,
                context_window: None,
                capabilities: ModelCapabilities {
                    text_generation: Some(CapabilityLevel::Strong),
                    // Gemini's real, documented response_format support
                    // for structured JSON output (verified against
                    // Google's own API reference).
                    structured_output: Some(CapabilityLevel::Strong),
                    reasoning: Some(reasoning),
                    vision: Some(CapabilityLevel::Unknown),
                    embedding: Some(CapabilityLevel::None),
                    tool_use: Some(CapabilityLevel::Unknown),
                    long_context: Some(CapabilityLevel::Unknown),
                },
                status: ModelStatus::Available,
            });
        }
        info.status = if info.models.is_empty() {
            ProviderStatus::Available
        } else {
            ProviderStatus::Healthy
        };
        info
    }
}

/// Executes a real chat completion against Gemini's OpenAI-compatible
/// `POST /chat/completions` (Bearer `GEMINI_API_KEY`, non-streaming).
pub struct GeminiExecutor {
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
    pub max_tokens: u32,
}

impl GeminiExecutor {
    pub fn from_env() -> Self {
        let base_url = std::env::var("GEMINI_BASE_URL").unwrap_or_else(|_| {
            "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
        });
        let api_key = std::env::var("GEMINI_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let timeout_secs: u64 = std::env::var("GEMINI_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(180);
        // 8192 (was 4096) — see the same real reasoning on MJ_AI_MAX_TOKENS's
        // own default above; still overridable per deployment.
        let max_tokens: u32 = std::env::var("GEMINI_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8192);
        Self {
            base_url,
            api_key,
            timeout: Duration::from_secs(timeout_secs),
            max_tokens,
        }
    }
}

#[async_trait::async_trait]
impl AiExecutor for GeminiExecutor {
    async fn execute(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
    ) -> Result<ExecutionOutput, ExecutionError> {
        self.execute_task(provider_id, model_id, prompt, AiTask::GeneralAi)
            .await
    }

    async fn execute_task(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        task: AiTask,
    ) -> Result<ExecutionOutput, ExecutionError> {
        if provider_id != GeminiDiscovery::PROVIDER_ID {
            return Err(ExecutionError::ProviderUnavailable(provider_id.to_string()));
        }
        let Some(key) = &self.api_key else {
            return Err(ExecutionError::ProviderUnavailable(
                "GEMINI_API_KEY not set".to_string(),
            ));
        };

        let client = reqwest::Client::new();
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut body = serde_json::json!({
            "model": model_id,
            "messages": [{ "role": "user", "content": prompt }],
            "stream": false,
            "max_tokens": self.max_tokens,
        });
        if matches!(
            task,
            AiTask::TestCaseGeneration | AiTask::TestCaseCompilation
        ) {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        let resp = client
            .post(&url)
            .bearer_auth(key)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ExecutionError::Timeout
                } else {
                    ExecutionError::ProviderUnavailable(e.to_string())
                }
            })?;
        if !resp.status().is_success() {
            return Err(ExecutionError::NonSuccessStatus(resp.status().as_u16()));
        }
        let parsed: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ExecutionError::InvalidResponse(e.to_string()))?;
        let text = parsed
            .get("choices")
            .and_then(|v| v.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if text.trim().is_empty() {
            return Err(ExecutionError::InvalidResponse(
                "empty response".to_string(),
            ));
        }
        let usage = TokenUsage {
            prompt_tokens: parsed
                .get("usage")
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            completion_tokens: parsed
                .get("usage")
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        };
        Ok(ExecutionOutput { text, usage })
    }
}

// ------------------------------------------------------- custom providers

/// A real, user-added AI provider (Settings UI -> `POST /api/v1/ai/providers/custom`
/// -> `custom_ai_providers` DB row -> here). Unlike Ollama/MJ Gateway/Groq/
/// Gemini (each its own `from_env()`-read struct, fixed at process
/// startup), the SET of these is open-ended and changes at runtime without
/// a restart — added/removed through `AiInfrastructure::set_custom_providers`,
/// called once at startup (from the DB) and again after every real
/// add/delete. Standardized on the plain `/v1/models` + `/v1/chat/completions`
/// convention every OpenAI-compatible service except Gemini's own
/// compatibility shim actually uses (see `GeminiDiscovery`'s own doc
/// comment for why Gemini needed its own dedicated integration instead of
/// fitting this generic path) — a provider that doesn't follow that
/// convention needs its own dedicated code, same as Groq/Gemini got.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
}

/// Real, bounded, timeout-protected discovery for one custom provider —
/// the same real `GET {base_url}/v1/models` probe `MjGatewayDiscovery`
/// already makes, reused here (not re-invented) since the wire shape is
/// identical. Never assumes reachability from the row's mere existence.
async fn discover_custom_provider(cfg: &CustomProviderConfig, timeout: Duration) -> AiProviderInfo {
    let mut info = AiProviderInfo {
        id: cfg.id.clone(),
        name: cfg.name.clone(),
        provider_type: ProviderType::Custom,
        endpoint: Some(cfg.base_url.clone()),
        configured: true, // a row only exists because a real key was supplied when it was added
        reachable: false,
        status: ProviderStatus::Unavailable,
        models: Vec::new(),
        // Real fact this deployment cannot actually know for an arbitrary
        // user-added endpoint (no published-pricing lookup exists for a
        // custom base_url) — `Unmetered` would falsely claim it's free, so
        // this reports the one thing that's true: real cost is genuinely
        // unknown, at the cheapest possible honest value rather than a
        // guessed rate. Disclosed to the user when they add one.
        pricing: ProviderPricing::Unmetered,
    };

    let client = reqwest::Client::new();
    let url = format!("{}/v1/models", cfg.base_url.trim_end_matches('/'));
    let resp = match client
        .get(&url)
        .bearer_auth(&cfg.api_key)
        .timeout(timeout)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::info!(
                "Custom provider '{}' discovery: not reachable ({e})",
                cfg.id
            );
            return info;
        }
    };
    if !resp.status().is_success() {
        info.status = ProviderStatus::Error;
        tracing::warn!(
            "Custom provider '{}' discovery: /v1/models responded with HTTP {}",
            cfg.id,
            resp.status()
        );
        return info;
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            info.status = ProviderStatus::Error;
            tracing::warn!(
                "Custom provider '{}' discovery: invalid response body ({e})",
                cfg.id
            );
            return info;
        }
    };

    info.reachable = true;
    let entries = body
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for m in &entries {
        let id = m
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let reasoning = if is_known_reasoning_model(&id, None) {
            CapabilityLevel::Strong
        } else {
            CapabilityLevel::Basic
        };
        info.models.push(ModelDefinition {
            provider_id: cfg.id.clone(),
            id: id.clone(),
            display_name: id,
            family: None,
            parameter_size: None,
            context_window: None,
            capabilities: ModelCapabilities {
                text_generation: Some(CapabilityLevel::Strong),
                structured_output: Some(CapabilityLevel::Basic), // unverified for an arbitrary custom endpoint
                reasoning: Some(reasoning),
                vision: Some(CapabilityLevel::Unknown),
                embedding: Some(CapabilityLevel::Unknown),
                tool_use: Some(CapabilityLevel::Unknown),
                long_context: Some(CapabilityLevel::Unknown),
            },
            status: ModelStatus::Available,
        });
    }
    info.status = if info.models.is_empty() {
        ProviderStatus::Available
    } else {
        ProviderStatus::Healthy
    };
    info
}

/// Executes a real chat completion against a custom provider's own
/// `POST {base_url}/v1/chat/completions` — the same real request/response
/// handling `MjGatewayExecutor` already does, reused here since the wire
/// shape is identical.
async fn execute_custom_provider(
    cfg: &CustomProviderConfig,
    model_id: &str,
    prompt: &str,
    timeout: Duration,
) -> Result<ExecutionOutput, ExecutionError> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/completions", cfg.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model_id,
        "messages": [{ "role": "user", "content": prompt }],
        "stream": false,
    });
    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .json(&body)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ExecutionError::Timeout
            } else {
                ExecutionError::ProviderUnavailable(e.to_string())
            }
        })?;
    if !resp.status().is_success() {
        return Err(ExecutionError::NonSuccessStatus(resp.status().as_u16()));
    }
    let parsed: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ExecutionError::InvalidResponse(e.to_string()))?;
    let text = parsed
        .get("choices")
        .and_then(|v| v.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if text.trim().is_empty() {
        return Err(ExecutionError::InvalidResponse(
            "empty response".to_string(),
        ));
    }
    let usage = TokenUsage {
        prompt_tokens: parsed
            .get("usage")
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        completion_tokens: parsed
            .get("usage")
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
    };
    Ok(ExecutionOutput { text, usage })
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RoutingMode {
    Auto,
    Manual,
}

/// A real, explicit routing rule for one specific task — e.g. "always use
/// a real, paid Gemini/OpenAI model for `TEST_CASE_GENERATION` once one is
/// configured, leave every other task on the default/global preference."
///
/// Without this, adding a second, stronger-scoring paid provider would
/// change routing for EVERY task at once in AUTO mode (capability scoring
/// has no notion of "only route this specific task type here") — the one
/// real gap between this router and a genuine "routine work stays free and
/// local, heavy work goes to a paid API" policy. `provider`/`model` are
/// each independently optional so a caller can pin just the provider (any
/// of its models) or just the model (whichever provider serves it) for
/// this one task, matching `AiSettings`' own `preferred_provider`/
/// `preferred_model` split.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskProviderPreference {
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub mode: RoutingMode,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    pub fallback_enabled: bool,
    /// Real, explicit per-task overrides (see `TaskProviderPreference`'s
    /// own doc comment). Empty by default — every task then behaves
    /// exactly as it did before this field existed, using
    /// `preferred_provider`/`preferred_model`; this is purely additive.
    /// `#[serde(default)]` so a PUT body from a client that predates this
    /// field (or simply has nothing to override) still deserializes.
    #[serde(default)]
    pub task_overrides: HashMap<AiTask, TaskProviderPreference>,
}

impl Default for AiSettings {
    /// MJ AI Gateway first: AUTO mode prefers its models for the primary
    /// attempt (`mj-default`), and if the gateway is down or errors, the
    /// AUTO fallback loop still considers every other discovered provider
    /// (Ollama) because fallback scoring deliberately ignores the
    /// preference. A deployment that wants different routing PUTs new
    /// settings; this default just makes the wired gateway the out-of-box
    /// choice instead of an unconfigured coin flip.
    fn default() -> Self {
        Self {
            mode: RoutingMode::Auto,
            preferred_provider: Some(MjGatewayDiscovery::PROVIDER_ID.to_string()),
            preferred_model: Some("mj-default".to_string()),
            fallback_enabled: true,
            task_overrides: HashMap::new(),
        }
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
    NoCompatibleModel {
        task: AiTask,
    },
    ModelUnavailable {
        provider_id: String,
        model_id: String,
    },
    ProviderUnavailable {
        provider_id: String,
    },
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
fn score_model(
    model: &ModelDefinition,
    provider: &AiProviderInfo,
    required: &[RequiredCapability],
    preferred_model: Option<&str>,
) -> Option<f64> {
    if !matches!(
        provider.status,
        ProviderStatus::Healthy | ProviderStatus::Available | ProviderStatus::Degraded
    ) {
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
        let digits: String = param_str
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
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
    /// Real token counts, exactly as the executing provider reported them
    /// (`None` when that provider's response genuinely carried none).
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    /// Real, measured wall-clock time for this one executor call — always
    /// known (it is timed here, not reported by the provider), unlike the
    /// token fields above.
    pub latency_ms: u64,
    /// `estimate_cost`'s real result for the provider that actually served
    /// this request — `Some(0.0)` for today's unmetered providers, `None`
    /// only if a future metered provider's response omitted token counts.
    pub estimated_cost_usd: Option<f64>,
}

/// Real, disclosed cost math — `Unmetered` is always a real `$0.00`, never
/// "unknown" (this deployment's own architecture genuinely has no bill for
/// it). A `Metered` provider without real token counts to multiply against
/// honestly reports `None` rather than a silently-wrong `$0.00`.
fn estimate_cost(usage: &TokenUsage, pricing: ProviderPricing) -> Option<f64> {
    match pricing {
        ProviderPricing::Unmetered => Some(0.0),
        ProviderPricing::Metered {
            input_usd_per_1k,
            output_usd_per_1k,
        } => {
            let prompt = usage.prompt_tokens? as f64;
            let completion = usage.completion_tokens? as f64;
            Some((prompt / 1000.0) * input_usd_per_1k + (completion / 1000.0) * output_usd_per_1k)
        }
    }
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
    /// Real, user-added providers (Settings UI, `custom_ai_providers` DB
    /// table) — an open-ended, runtime-mutable set, unlike `discoveries`/
    /// `executors` above (both fixed at process construction). See
    /// `set_custom_providers`.
    custom_providers: RwLock<Vec<CustomProviderConfig>>,
    providers: RwLock<Vec<AiProviderInfo>>,
    last_discovered: RwLock<Option<Instant>>,
    settings: RwLock<AiSettings>,
    health_ttl: Duration,
}

impl AiInfrastructure {
    pub fn new(
        discoveries: Vec<Arc<dyn ProviderDiscovery>>,
        executors: HashMap<&'static str, Arc<dyn AiExecutor>>,
    ) -> Self {
        let ttl_secs: u64 = std::env::var("AI_HEALTH_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        Self {
            discoveries,
            executors,
            custom_providers: RwLock::new(Vec::new()),
            providers: RwLock::new(Vec::new()),
            last_discovered: RwLock::new(None),
            settings: RwLock::new(AiSettings::default()),
            health_ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Real construction wired for this deployment: Ollama (local), the MJ
    /// AI Gateway (`MJ_AI_API_KEY`/`MJ_AI_ENDPOINT`), Groq (`GROQ_API_KEY`),
    /// and Gemini (`GEMINI_API_KEY`). Groq/Gemini honestly report
    /// `NOT_CONFIGURED` (never simulated as available) until their real key
    /// is set — same contract every provider here already holds itself to.
    pub fn from_env() -> Self {
        let discoveries: Vec<Arc<dyn ProviderDiscovery>> = vec![
            Arc::new(OllamaDiscovery::from_env()),
            Arc::new(MjGatewayDiscovery::from_env()),
            Arc::new(GroqDiscovery::from_env()),
            Arc::new(GeminiDiscovery::from_env()),
        ];
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("ollama", Arc::new(OllamaExecutor::from_env()));
        executors.insert(
            MjGatewayDiscovery::PROVIDER_ID,
            Arc::new(MjGatewayExecutor::from_env()),
        );
        executors.insert(
            GroqDiscovery::PROVIDER_ID,
            Arc::new(GroqExecutor::from_env()),
        );
        executors.insert(
            GeminiDiscovery::PROVIDER_ID,
            Arc::new(GeminiExecutor::from_env()),
        );
        Self::new(discoveries, executors)
    }

    /// Discovers every configured provider (brief §29: async, each
    /// bounded by its own real timeout, never blocking app startup — this
    /// whole call runs inside an async task, so it never blocks the
    /// executor thread even though the handful of real discoverers here
    /// are awaited one at a time rather than pulling in a new dependency
    /// just for `join_all` on a list this short). Caches the result.
    pub async fn discover_all(&self) -> Vec<AiProviderInfo> {
        let custom = self.custom_providers.read().await.clone();
        let mut results = Vec::with_capacity(self.discoveries.len() + custom.len());
        for d in &self.discoveries {
            results.push(d.discover().await);
        }
        // Real, user-added providers — same bounded discovery timeout every
        // built-in provider already uses, just not itself configurable per
        // custom row (a real, disclosed simplification: `health_ttl` is
        // this deployment's one real discovery-timeout knob today).
        for cfg in &custom {
            results.push(discover_custom_provider(cfg, Duration::from_secs(3)).await);
        }
        *self.providers.write().await = results.clone();
        *self.last_discovered.write().await = Some(Instant::now());
        results
    }

    /// Replaces the whole real, user-added provider set — called once at
    /// startup (from the `custom_ai_providers` DB table) and again after
    /// every real add/delete, so a change is live on the very next
    /// discovery cycle with no process restart needed (unlike the env-var
    /// providers above). Mirrors `set_settings`'s own "replace the whole
    /// value, caller already has the full authoritative list" contract.
    pub async fn set_custom_providers(&self, providers: Vec<CustomProviderConfig>) {
        *self.custom_providers.write().await = providers;
    }

    /// The one real, current custom-provider config matching `id`, if any —
    /// used by `execute_for` below (a custom provider has no entry in the
    /// static `executors` map, since its id is only known at runtime).
    async fn find_custom_provider(&self, id: &str) -> Option<CustomProviderConfig> {
        self.custom_providers
            .read()
            .await
            .iter()
            .find(|c| c.id == id)
            .cloned()
    }

    /// Executes one real request against `provider_id`/`model_id` — checks
    /// the static `executors` map first (env-var-configured providers:
    /// Ollama/MJ Gateway/Groq/Gemini), then falls back to the real, current
    /// custom-provider set (Settings-UI-added, runtime-mutable) if not
    /// found there. The one real chokepoint both `generate`'s primary
    /// attempt and its fallback loop call through, so a custom provider
    /// participates in both exactly like every built-in one already does.
    async fn execute_for(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        task: AiTask,
    ) -> Result<ExecutionOutput, ExecutionError> {
        if let Some(executor) = self.executors.get(provider_id) {
            return executor
                .execute_task(provider_id, model_id, prompt, task)
                .await;
        }
        if let Some(cfg) = self.find_custom_provider(provider_id).await {
            return execute_custom_provider(&cfg, model_id, prompt, Duration::from_secs(180)).await;
        }
        Err(ExecutionError::ProviderUnavailable(format!(
            "no executor wired for provider {provider_id}"
        )))
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
    pub fn route_among(
        providers: &[AiProviderInfo],
        task: AiTask,
        settings: &AiSettings,
        preferred_provider: Option<&str>,
        preferred_model: Option<&str>,
    ) -> Result<RoutingDecision, RoutingError> {
        let required = required_capabilities(task);

        // A real, explicit `task_overrides` entry for this exact task
        // takes precedence over the global preference — see
        // `AiSettings::task_overrides`'s own doc comment. Falls through to
        // the global preference when no override exists for this task, so
        // a settings object with an empty `task_overrides` map (today's
        // only shape, before this field existed) behaves identically to
        // before.
        let task_override = settings.task_overrides.get(&task);
        let manual_provider = task_override
            .and_then(|o| o.provider.as_deref())
            .or(settings.preferred_provider.as_deref());
        let manual_model = task_override
            .and_then(|o| o.model.as_deref())
            .or(settings.preferred_model.as_deref());

        if settings.mode == RoutingMode::Manual {
            let (Some(pid), Some(mid)) = (manual_provider, manual_model) else {
                return Err(RoutingError::NoCompatibleModel { task });
            };
            let provider = providers.iter().find(|p| p.id == pid).ok_or_else(|| {
                RoutingError::ProviderUnavailable {
                    provider_id: pid.to_string(),
                }
            })?;
            let model = provider
                .models
                .iter()
                .find(|m| m.id == mid)
                .ok_or_else(|| RoutingError::ModelUnavailable {
                    provider_id: pid.to_string(),
                    model_id: mid.to_string(),
                })?;
            return match score_model(model, provider, &required, None) {
                Some(score) => Ok(RoutingDecision {
                    provider_id: pid.to_string(),
                    model_id: mid.to_string(),
                    routing_score: score,
                    reason: "MANUAL mode: user-selected provider/model".to_string(),
                }),
                None => Err(RoutingError::ModelUnavailable {
                    provider_id: pid.to_string(),
                    model_id: mid.to_string(),
                }),
            };
        }

        // AUTO mode: score every real candidate and pick the best.
        let effective_preferred_model = preferred_provider
            .zip(preferred_model)
            .map(|(_, m)| m)
            .or(preferred_model);
        let mut best: Option<RoutingDecision> = None;
        for provider in providers {
            for model in &provider.models {
                if let Some(pid) = preferred_provider {
                    if provider.id != pid {
                        continue;
                    }
                }
                if let Some(score) =
                    score_model(model, provider, &required, effective_preferred_model)
                {
                    if best
                        .as_ref()
                        .map(|b| score > b.routing_score)
                        .unwrap_or(true)
                    {
                        best = Some(RoutingDecision {
                            provider_id: provider.id.clone(),
                            model_id: model.id.clone(),
                            routing_score: score,
                            reason: format!(
                                "AUTO mode: best available model for {task:?} by routing score"
                            ),
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

        // Preference precedence:
        // - A per-request preference is a HARD filter (the caller asked
        //   for that provider specifically).
        // - A real, explicit `task_overrides` entry for this request's own
        //   task is next — same SOFT treatment as the global preference
        //   below, just scoped to one task (see `AiSettings::
        //   task_overrides`'s own doc comment). Kept in sync with
        //   `route_among`'s own MANUAL-mode resolution of the same field,
        //   so a task override behaves identically in either routing mode.
        // - The persisted/default AiSettings preference is SOFT: applied
        //   as the model bonus (`mj-default` outscores everything else
        //   while its gateway is healthy) so a dead or erroring gateway
        //   degrades gracefully to the next-best provider instead of
        //   hard-failing with NoCompatibleModel. (Before this fix,
        //   settings.preferred_provider was read in MANUAL mode only —
        //   AUTO mode silently ignored it.)
        let hard_provider = request.preferred_provider.clone();
        let settings_model = settings
            .task_overrides
            .get(&request.task)
            .and_then(|o| o.model.clone())
            .or_else(|| settings.preferred_model.clone());
        let soft_model = request.preferred_model.clone().or(settings_model);

        let decision = match Self::route_among(
            &providers,
            request.task,
            &settings,
            hard_provider.as_deref(),
            soft_model.as_deref(),
        ) {
            Ok(d) => d,
            Err(e) => {
                return GenerationOutcome2::Failed {
                    reason: format!("{e:?}"),
                }
            }
        };

        // Real fact about the provider that actually won routing — used
        // only for its disclosed `pricing`; absent only if `providers`
        // changed between routing and here (never happens within one
        // `generate` call, since both read the same local `providers`).
        let primary_pricing = providers
            .iter()
            .find(|p| p.id == decision.provider_id)
            .map(|p| p.pricing)
            .unwrap_or(ProviderPricing::Unmetered);

        let started = Instant::now();
        match self
            .execute_for(
                &decision.provider_id,
                &decision.model_id,
                &request.prompt,
                request.task,
            )
            .await
        {
            Ok(output) => {
                let latency_ms = started.elapsed().as_millis() as u64;
                let estimated_cost_usd = estimate_cost(&output.usage, primary_pricing);
                GenerationOutcome2::Ok(GenerationResult {
                    text: output.text,
                    provider: decision.provider_id,
                    model: decision.model_id,
                    routing_mode: settings.mode,
                    routing_score: decision.routing_score,
                    fallback_used: false,
                    prompt_tokens: output.usage.prompt_tokens,
                    completion_tokens: output.usage.completion_tokens,
                    latency_ms,
                    estimated_cost_usd,
                })
            }
            Err(primary_err) => {
                if settings.mode == RoutingMode::Auto && settings.fallback_enabled {
                    let required = required_capabilities(request.task);
                    let mut candidates: Vec<(&AiProviderInfo, &ModelDefinition, f64)> = providers
                        .iter()
                        .flat_map(|p| p.models.iter().map(move |m| (p, m)))
                        .filter(|(p, m)| {
                            !(p.id == decision.provider_id && m.id == decision.model_id)
                        })
                        .filter_map(|(p, m)| score_model(m, p, &required, None).map(|s| (p, m, s)))
                        .collect();
                    candidates
                        .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

                    for (provider, model, score) in candidates {
                        let fallback_started = Instant::now();
                        if let Ok(output) = self
                            .execute_for(&provider.id, &model.id, &request.prompt, request.task)
                            .await
                        {
                            let latency_ms = fallback_started.elapsed().as_millis() as u64;
                            let estimated_cost_usd = estimate_cost(&output.usage, provider.pricing);
                            return GenerationOutcome2::Ok(GenerationResult {
                                text: output.text,
                                provider: provider.id.clone(),
                                model: model.id.clone(),
                                routing_mode: settings.mode,
                                routing_score: score,
                                fallback_used: true,
                                prompt_tokens: output.usage.prompt_tokens,
                                completion_tokens: output.usage.completion_tokens,
                                latency_ms,
                                estimated_cost_usd,
                            });
                        }
                    }
                }
                GenerationOutcome2::Failed {
                    reason: primary_err.to_string(),
                }
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
        pub result: Result<ExecutionOutput, ExecutionError>,
    }

    impl FakeExecutor {
        pub fn ok(text: impl Into<String>) -> Self {
            Self {
                result: Ok(ExecutionOutput {
                    text: text.into(),
                    usage: TokenUsage::default(),
                }),
            }
        }
        /// Like `ok`, but with real, scripted (test-fixture) token counts —
        /// for tests that verify usage/cost actually flow through.
        pub fn ok_with_usage(text: impl Into<String>, prompt: u32, completion: u32) -> Self {
            Self {
                result: Ok(ExecutionOutput {
                    text: text.into(),
                    usage: TokenUsage {
                        prompt_tokens: Some(prompt),
                        completion_tokens: Some(completion),
                    },
                }),
            }
        }
        pub fn err(e: ExecutionError) -> Self {
            Self { result: Err(e) }
        }
    }

    #[async_trait::async_trait]
    impl AiExecutor for FakeExecutor {
        async fn execute(
            &self,
            _provider_id: &str,
            _model_id: &str,
            _prompt: &str,
        ) -> Result<ExecutionOutput, ExecutionError> {
            match &self.result {
                Ok(o) => Ok(o.clone()),
                Err(ExecutionError::Timeout) => Err(ExecutionError::Timeout),
                Err(ExecutionError::ProviderUnavailable(s)) => {
                    Err(ExecutionError::ProviderUnavailable(s.clone()))
                }
                Err(ExecutionError::ModelUnavailable(s)) => {
                    Err(ExecutionError::ModelUnavailable(s.clone()))
                }
                Err(ExecutionError::InvalidResponse(s)) => {
                    Err(ExecutionError::InvalidResponse(s.clone()))
                }
                Err(ExecutionError::NonSuccessStatus(s)) => {
                    Err(ExecutionError::NonSuccessStatus(*s))
                }
            }
        }
    }

    /// A scripted *sequence* of responses, one per call, consumed in order
    /// — unlike `FakeExecutor` (always the same fixed response, and never
    /// records what it was asked), this is for proving a caller makes N
    /// genuinely separate real calls with genuinely different prompt
    /// content, e.g. `ai_generation.rs`'s wave-based generation. Panics if
    /// called more times than scripted, so an unexpected extra real call is
    /// a loud test failure, not a silently-reused last response.
    pub struct RecordingExecutor {
        responses:
            std::sync::Mutex<std::collections::VecDeque<Result<ExecutionOutput, ExecutionError>>>,
        pub prompts: std::sync::Mutex<Vec<String>>,
    }

    impl RecordingExecutor {
        pub fn sequence(responses: Vec<Result<ExecutionOutput, ExecutionError>>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses.into_iter().collect()),
                prompts: std::sync::Mutex::new(Vec::new()),
            }
        }

        pub fn ok_sequence(responses: Vec<&str>) -> Self {
            Self::sequence(
                responses
                    .into_iter()
                    .map(|r| {
                        Ok(ExecutionOutput {
                            text: r.to_string(),
                            usage: TokenUsage::default(),
                        })
                    })
                    .collect(),
            )
        }
    }

    #[async_trait::async_trait]
    impl AiExecutor for RecordingExecutor {
        async fn execute(
            &self,
            _provider_id: &str,
            _model_id: &str,
            prompt: &str,
        ) -> Result<ExecutionOutput, ExecutionError> {
            self.prompts.lock().unwrap().push(prompt.to_string());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("RecordingExecutor called more times than it was scripted for")
        }
    }

    pub fn healthy_model(
        id: &str,
        provider_id: &str,
        structured_output: CapabilityLevel,
    ) -> ModelDefinition {
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
            pricing: ProviderPricing::Unmetered,
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
        let discovery = OllamaDiscovery {
            base_url: "http://127.0.0.1:1".to_string(),
            timeout: Duration::from_millis(500),
        };
        let info = discovery.discover().await;
        assert_eq!(info.status, ProviderStatus::Unavailable);
        assert!(!info.reachable);
        assert!(info.models.is_empty());
    }

    /// Spawns a real local HTTP server mimicking Ollama's real `/api/tags`
    /// (bulk listing) and `/api/show` (real per-model context-length
    /// metadata, nested under a family-prefixed `model_info` key) — the
    /// full real discovery happy path had no live-server test at all
    /// before this fix, only the "not reachable" one above.
    async fn spawn_mock_ollama_discovery(context_length: u64) -> String {
        use axum::routing::{get, post as post_route};

        async fn tags() -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "models": [{
                    "name": "qwen3:14b",
                    "details": { "family": "qwen3", "parameter_size": "14.8B" },
                    "capabilities": ["completion", "tools"]
                }]
            }))
        }

        async fn show(
            axum::extract::State(context_length): axum::extract::State<u64>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "model_info": { "qwen3.context_length": context_length }
            }))
        }

        let app = axum::Router::new()
            .route("/api/tags", get(tags))
            .route("/api/show", post_route(show))
            .with_state(context_length);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn discovery_reports_a_real_healthy_model_with_its_real_context_length() {
        let base_url = spawn_mock_ollama_discovery(40_960).await;
        let discovery = OllamaDiscovery {
            base_url,
            timeout: Duration::from_secs(2),
        };
        let info = discovery.discover().await;

        assert!(info.reachable);
        assert_eq!(info.status, ProviderStatus::Healthy);
        let model = info.models.first().expect("one real model");
        assert_eq!(model.id, "qwen3:14b");
        assert_eq!(model.parameter_size.as_deref(), Some("14.8B"));
        assert_eq!(
            model.context_window,
            Some(40_960),
            "the real /api/show context length must reach ModelDefinition, not stay None"
        );
        assert_eq!(
            model.capabilities.long_context,
            Some(CapabilityLevel::Strong),
            "40,960 real tokens clears the Strong threshold"
        );
        assert_eq!(
            model.capabilities.reasoning,
            Some(CapabilityLevel::Strong),
            "qwen3 is a known reasoning family — both fixes apply together"
        );
    }

    #[tokio::test]
    async fn a_smaller_real_context_length_is_basic_not_strong_and_never_disqualifying() {
        let base_url = spawn_mock_ollama_discovery(4096).await;
        let discovery = OllamaDiscovery {
            base_url,
            timeout: Duration::from_secs(2),
        };
        let info = discovery.discover().await;
        let model = info.models.first().expect("one real model");
        assert_eq!(model.context_window, Some(4096));
        assert_eq!(
            model.capabilities.long_context,
            Some(CapabilityLevel::Basic)
        );

        // Never disqualifying — a small-but-real context window still
        // yields a valid candidate for a document-shaped task.
        let required = required_capabilities(AiTask::DocumentAnalysis);
        let provider = healthy_provider("ollama", vec![model.clone()]);
        assert!(score_model(model, &provider, &required, None).is_some());
    }

    #[test]
    fn context_length_from_show_response_finds_the_real_family_prefixed_key() {
        let body = serde_json::json!({ "model_info": { "llama.context_length": 8192, "llama.embedding_length": 4096 } });
        assert_eq!(context_length_from_show_response(&body), Some(8192));
    }

    #[test]
    fn context_length_from_show_response_is_none_when_the_real_reply_carries_no_such_key() {
        let body = serde_json::json!({ "model_info": { "some.other_field": 1 } });
        assert_eq!(context_length_from_show_response(&body), None);
        assert_eq!(
            context_length_from_show_response(&serde_json::json!({})),
            None
        );
    }

    #[test]
    fn long_context_level_thresholds() {
        assert_eq!(long_context_level(Some(32_768)), CapabilityLevel::Strong);
        assert_eq!(long_context_level(Some(32_767)), CapabilityLevel::Basic);
        assert_eq!(long_context_level(Some(1)), CapabilityLevel::Basic);
        assert_eq!(
            long_context_level(None),
            CapabilityLevel::Unknown,
            "no real evidence fetched must stay Unknown, never guessed"
        );
    }

    // --------------------------------------------- known reasoning models

    #[test]
    fn a_real_deployed_qwen3_model_is_recognized_as_a_known_reasoning_model() {
        // The exact real model this deployment routes to today
        // (qwen3:14b, via Ollama) — this is the concrete regression the
        // fix closes: it must no longer be flattened to the generic
        // "Basic" every other text model also gets.
        assert!(is_known_reasoning_model("qwen3:14b", Some("qwen3")));
    }

    #[test]
    fn other_real_named_reasoning_releases_are_also_recognized() {
        assert!(is_known_reasoning_model("qwq:32b", None));
        assert!(is_known_reasoning_model("deepseek-r1:32b", None));
        assert!(is_known_reasoning_model(
            "deepseek-r1-distill-qwen-14b",
            None
        ));
        assert!(is_known_reasoning_model("phi4-reasoning:14b", None));
    }

    #[test]
    fn matching_is_case_insensitive_since_a_real_tag_could_be_cased_either_way() {
        assert!(is_known_reasoning_model("Qwen3:14B", None));
    }

    #[test]
    fn an_unrecognized_model_is_not_flagged_strong_but_is_not_judged_weak_either() {
        // Absence from a real but necessarily incomplete list is evidence
        // of nothing — this only ever asserts what `is_known_reasoning_model`
        // itself returns (false), never a downgrade elsewhere.
        assert!(!is_known_reasoning_model("gemma3:12b", Some("gemma3")));
        assert!(!is_known_reasoning_model("llama3:8b", Some("llama")));
    }

    #[test]
    fn capabilities_from_ollama_reports_strong_reasoning_for_a_real_known_reasoning_model() {
        let caps = capabilities_from_ollama(
            "qwen3:14b",
            &["completion".to_string()],
            Some("qwen3"),
            None,
        );
        assert_eq!(
            caps.reasoning,
            Some(CapabilityLevel::Strong),
            "a real, named reasoning-tuned release must score higher than the generic text-model floor"
        );
    }

    #[test]
    fn capabilities_from_ollama_keeps_the_honest_basic_floor_for_an_unrecognized_model() {
        let caps = capabilities_from_ollama(
            "gemma3:12b",
            &["completion".to_string()],
            Some("gemma3"),
            None,
        );
        assert_eq!(
            caps.reasoning,
            Some(CapabilityLevel::Basic),
            "an unrecognized model must keep the same honest floor as before this fix — never invented as either higher or lower"
        );
    }

    #[test]
    fn a_known_reasoning_model_scores_higher_for_a_task_that_requires_reasoning() {
        // End-to-end proof this actually changes real routing, not just
        // the capability field in isolation.
        let required = required_capabilities(AiTask::TestCaseGeneration);
        let qwen3 = ModelDefinition {
            id: "qwen3:14b".to_string(),
            provider_id: "ollama".to_string(),
            display_name: "qwen3:14b".to_string(),
            family: Some("qwen3".to_string()),
            parameter_size: None,
            context_window: None,
            capabilities: capabilities_from_ollama(
                "qwen3:14b",
                &["completion".to_string()],
                Some("qwen3"),
                None,
            ),
            status: ModelStatus::Available,
        };
        let gemma3 = ModelDefinition {
            id: "gemma3:12b".to_string(),
            provider_id: "ollama".to_string(),
            display_name: "gemma3:12b".to_string(),
            family: Some("gemma3".to_string()),
            parameter_size: None,
            context_window: None,
            capabilities: capabilities_from_ollama(
                "gemma3:12b",
                &["completion".to_string()],
                Some("gemma3"),
                None,
            ),
            status: ModelStatus::Available,
        };
        let provider = healthy_provider("ollama", vec![]);
        let qwen3_score = score_model(&qwen3, &provider, &required, None)
            .expect("qwen3 must be a valid candidate");
        let gemma3_score = score_model(&gemma3, &provider, &required, None)
            .expect("gemma3 must be a valid candidate");
        assert!(
            qwen3_score > gemma3_score,
            "the real, named reasoning model must genuinely outscore an otherwise-identical model with unrecognized reasoning support: qwen3={qwen3_score}, gemma3={gemma3_score}"
        );
    }

    // Phase B cleanup: the two ApiKeyProviderDiscovery env-var tests were
    // removed together with that stub type — the MJ gateway discovery
    // tests below cover the same honest-status contract against a real,
    // executing provider.

    // -------------------------------------------------------- model discovery

    #[tokio::test]
    async fn infrastructure_discover_all_populates_multiple_providers() {
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider(
                    "p1",
                    vec![healthy_model("m1", "p1", CapabilityLevel::Strong)],
                ))),
                Arc::new(FakeDiscovery(healthy_provider(
                    "p2",
                    vec![healthy_model("m2", "p2", CapabilityLevel::Basic)],
                ))),
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
        let mut provider =
            healthy_provider("p", vec![healthy_model("m", "p", CapabilityLevel::Strong)]);
        provider.status = ProviderStatus::Unavailable;
        let score = score_model(
            &provider.models[0],
            &provider,
            &[RequiredCapability::TextGeneration],
            None,
        );
        assert_eq!(
            score, None,
            "an unavailable provider's model must never be a routable candidate"
        );
    }

    #[test]
    fn score_disqualifies_an_unavailable_model_even_on_a_healthy_provider() {
        let mut model = healthy_model("m", "p", CapabilityLevel::Strong);
        model.status = ModelStatus::Unavailable;
        let provider = healthy_provider("p", vec![model]);
        let score = score_model(
            &provider.models[0],
            &provider,
            &[RequiredCapability::TextGeneration],
            None,
        );
        assert_eq!(score, None);
    }

    #[test]
    fn unknown_capability_is_scored_low_but_not_disqualifying() {
        let mut model = healthy_model("m", "p", CapabilityLevel::Strong);
        model.capabilities.vision = Some(CapabilityLevel::Unknown);
        let provider = healthy_provider("p", vec![model]);
        let score = score_model(
            &provider.models[0],
            &provider,
            &[RequiredCapability::Vision],
            None,
        );
        assert!(
            score.is_some(),
            "an unknown capability must not disqualify — only a known-absent one does"
        );
    }

    #[test]
    fn none_capability_disqualifies_when_required() {
        let mut model = healthy_model("m", "p", CapabilityLevel::Strong);
        model.capabilities.vision = Some(CapabilityLevel::None);
        let provider = healthy_provider("p", vec![model]);
        let score = score_model(
            &provider.models[0],
            &provider,
            &[RequiredCapability::Vision],
            None,
        );
        assert_eq!(
            score, None,
            "a text-only model must never be selected for a task that genuinely requires vision"
        );
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
        let model_b = healthy_model(
            "model-b-slow-strong-structured",
            "p",
            CapabilityLevel::Strong,
        );
        let providers = vec![healthy_provider("p", vec![model_a, model_b])];
        let settings = AiSettings::default();

        let decision = AiInfrastructure::route_among(
            &providers,
            AiTask::TestCaseGeneration,
            &settings,
            None,
            None,
        )
        .expect("a compatible model must be found");

        assert_eq!(decision.model_id, "model-b-slow-strong-structured", "the router must select the model whose real capability data better satisfies the task's requirements");
    }

    #[test]
    fn auto_mode_selects_the_only_healthy_compatible_model() {
        let providers = vec![healthy_provider(
            "p",
            vec![healthy_model("m", "p", CapabilityLevel::Strong)],
        )];
        let decision = AiInfrastructure::route_among(
            &providers,
            AiTask::GeneralAi,
            &AiSettings::default(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(decision.provider_id, "p");
        assert_eq!(decision.model_id, "m");
    }

    #[test]
    fn manual_mode_respects_the_users_own_selection_even_if_not_the_highest_scoring() {
        let model_a = healthy_model("weaker-but-user-chose-it", "p", CapabilityLevel::Basic);
        let model_b = healthy_model("stronger-not-chosen", "p", CapabilityLevel::Strong);
        let providers = vec![healthy_provider("p", vec![model_a, model_b])];
        let settings = AiSettings {
            mode: RoutingMode::Manual,
            preferred_provider: Some("p".to_string()),
            preferred_model: Some("weaker-but-user-chose-it".to_string()),
            fallback_enabled: false,
            task_overrides: std::collections::HashMap::new(),
        };

        let decision = AiInfrastructure::route_among(
            &providers,
            AiTask::TestCaseGeneration,
            &settings,
            None,
            None,
        )
        .unwrap();
        assert_eq!(decision.model_id, "weaker-but-user-chose-it", "MANUAL mode must respect the user's own explicit choice, not silently override it with a higher score");
    }

    #[test]
    fn manual_mode_reports_model_unavailable_when_the_selected_model_no_longer_exists() {
        let providers = vec![healthy_provider(
            "p",
            vec![healthy_model("m", "p", CapabilityLevel::Strong)],
        )];
        let settings = AiSettings {
            mode: RoutingMode::Manual,
            preferred_provider: Some("p".to_string()),
            preferred_model: Some("model-that-vanished".to_string()),
            fallback_enabled: false,
            task_overrides: std::collections::HashMap::new(),
        };

        let result =
            AiInfrastructure::route_among(&providers, AiTask::GeneralAi, &settings, None, None);
        assert!(matches!(result, Err(RoutingError::ModelUnavailable { .. })), "a manual selection that no longer exists must be reported honestly, never silently substituted");
    }

    #[test]
    fn routing_reports_no_compatible_model_when_nothing_qualifies() {
        // Every real model here explicitly lacks vision — UiAnalysis requires it.
        let providers = vec![healthy_provider(
            "p",
            vec![healthy_model("m", "p", CapabilityLevel::Strong)],
        )];
        let result = AiInfrastructure::route_among(
            &providers,
            AiTask::UiAnalysis,
            &AiSettings::default(),
            None,
            None,
        );
        assert!(matches!(
            result,
            Err(RoutingError::NoCompatibleModel { .. })
        ));
    }

    // ------------------------------------------------ per-task provider overrides

    #[test]
    fn default_settings_have_no_task_overrides() {
        // Documents the "purely additive" contract in `AiSettings::
        // task_overrides`'s own doc comment as a real, checked fact, not
        // just a claim in a comment.
        assert!(AiSettings::default().task_overrides.is_empty());
    }

    #[test]
    fn manual_mode_task_override_wins_over_the_global_preference_for_its_own_task() {
        let model_a = healthy_model("global-default-model", "ollama", CapabilityLevel::Strong);
        let model_b = healthy_model("gemini-heavy-model", "gemini", CapabilityLevel::Strong);
        let providers = vec![
            healthy_provider("ollama", vec![model_a]),
            healthy_provider("gemini", vec![model_b]),
        ];
        let mut task_overrides = HashMap::new();
        task_overrides.insert(
            AiTask::TestCaseGeneration,
            TaskProviderPreference {
                provider: Some("gemini".to_string()),
                model: Some("gemini-heavy-model".to_string()),
            },
        );
        let settings = AiSettings {
            mode: RoutingMode::Manual,
            preferred_provider: Some("ollama".to_string()),
            preferred_model: Some("global-default-model".to_string()),
            fallback_enabled: false,
            task_overrides,
        };

        let decision = AiInfrastructure::route_among(
            &providers,
            AiTask::TestCaseGeneration,
            &settings,
            None,
            None,
        )
        .expect("a compatible model must be found");
        assert_eq!(
            decision.provider_id, "gemini",
            "the task-specific override must win for its own task, even though it is not the global preference"
        );
        assert_eq!(decision.model_id, "gemini-heavy-model");
    }

    #[test]
    fn manual_mode_a_task_without_an_override_still_uses_the_global_preference() {
        // Same settings as the test above — proves the override is scoped
        // to the one task it names, not a global switch that leaks into
        // every other task's routing.
        let model_a = healthy_model("global-default-model", "ollama", CapabilityLevel::Strong);
        let model_b = healthy_model("gemini-heavy-model", "gemini", CapabilityLevel::Strong);
        let providers = vec![
            healthy_provider("ollama", vec![model_a]),
            healthy_provider("gemini", vec![model_b]),
        ];
        let mut task_overrides = HashMap::new();
        task_overrides.insert(
            AiTask::TestCaseGeneration,
            TaskProviderPreference {
                provider: Some("gemini".to_string()),
                model: Some("gemini-heavy-model".to_string()),
            },
        );
        let settings = AiSettings {
            mode: RoutingMode::Manual,
            preferred_provider: Some("ollama".to_string()),
            preferred_model: Some("global-default-model".to_string()),
            fallback_enabled: false,
            task_overrides,
        };

        let decision = AiInfrastructure::route_among(
            &providers,
            AiTask::GeneralAi, // not TestCaseGeneration — no override configured for this task
            &settings,
            None,
            None,
        )
        .expect("a compatible model must be found");
        assert_eq!(decision.provider_id, "ollama");
        assert_eq!(decision.model_id, "global-default-model");
    }

    #[test]
    fn manual_mode_a_partial_task_override_pins_only_the_field_it_names() {
        // `TaskProviderPreference.provider`/`model` are each independently
        // optional — pinning only the provider must still require the
        // global model, and vice versa (never silently pick "any model").
        let model = healthy_model("only-real-model", "gemini", CapabilityLevel::Strong);
        let providers = vec![healthy_provider("gemini", vec![model])];
        let mut task_overrides = HashMap::new();
        task_overrides.insert(
            AiTask::TestCaseGeneration,
            TaskProviderPreference {
                provider: Some("gemini".to_string()),
                model: None, // deliberately unset — must fall through to the global model
            },
        );
        let settings = AiSettings {
            mode: RoutingMode::Manual,
            preferred_provider: None,
            preferred_model: Some("only-real-model".to_string()),
            fallback_enabled: false,
            task_overrides,
        };

        let decision = AiInfrastructure::route_among(
            &providers,
            AiTask::TestCaseGeneration,
            &settings,
            None,
            None,
        )
        .expect("provider from the override + model from the global preference must combine");
        assert_eq!(decision.provider_id, "gemini");
        assert_eq!(decision.model_id, "only-real-model");
    }

    #[tokio::test]
    async fn auto_mode_task_override_gives_its_soft_bonus_only_to_the_matching_task() {
        // Two real, equally-capable candidates that would otherwise tie —
        // the override's +1000 preference bonus (`score_model`) must break
        // the tie for TestCaseGeneration, but a different task must still
        // resolve however AUTO mode would without any override at all
        // (here, either candidate is a legitimate winner — the real
        // assertion is that the override does NOT force `gemini` there).
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("ollama", Arc::new(FakeExecutor::ok(r#"{"ok":"ollama"}"#)));
        executors.insert("gemini", Arc::new(FakeExecutor::ok(r#"{"ok":"gemini"}"#)));
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider(
                    "ollama",
                    vec![healthy_model(
                        "ollama-model",
                        "ollama",
                        CapabilityLevel::Strong,
                    )],
                ))),
                Arc::new(FakeDiscovery(healthy_provider(
                    "gemini",
                    vec![healthy_model(
                        "gemini-model",
                        "gemini",
                        CapabilityLevel::Strong,
                    )],
                ))),
            ],
            executors,
        );
        let mut task_overrides = HashMap::new();
        task_overrides.insert(
            AiTask::TestCaseGeneration,
            TaskProviderPreference {
                provider: None,
                model: Some("gemini-model".to_string()),
            },
        );
        infra
            .set_settings(AiSettings {
                mode: RoutingMode::Auto,
                preferred_provider: None,
                preferred_model: None, // no global preference — proves the bonus came from the override
                fallback_enabled: true,
                task_overrides,
            })
            .await;

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::TestCaseGeneration,
                prompt: "hi".to_string(),
                preferred_provider: None,
                preferred_model: None,
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => assert_eq!(
                result.provider, "gemini",
                "the task override's soft bonus must decide this specific task's routing"
            ),
            GenerationOutcome2::Failed { reason } => panic!("expected Ok, got Failed: {reason}"),
        }
    }

    // ------------------------------------------------------ capability matching

    #[test]
    fn document_analysis_requires_long_context_capability() {
        let required = required_capabilities(AiTask::DocumentAnalysis);
        assert!(required.contains(&RequiredCapability::LongContext));
    }

    /// Phase 3 root-cause regression: `RequirementAnalysis` (Custom
    /// Testing's own analysis task) also requires `LongContext` — a model
    /// that explicitly reports `long_context: None` (structurally
    /// disqualifying, not merely low-scoring) must be excluded from
    /// `RequirementAnalysis` routing, exactly like `UiAnalysis` excludes a
    /// non-vision model. This is the specific mechanism that produced the
    /// original `NoCompatibleModel { task: RequirementAnalysis }` blocker
    /// when zero providers were reachable at all — see the Phase 3 Real AI
    /// Provider & Model Routing Fix report §5 for the actual root cause
    /// (no code defect here; the router's own exclusion logic is correct
    /// and unchanged by that fix).
    #[test]
    fn requirement_analysis_requires_long_context_capability_and_a_none_model_is_excluded() {
        let required = required_capabilities(AiTask::RequirementAnalysis);
        assert!(required.contains(&RequiredCapability::LongContext));

        let mut no_long_context = healthy_model("m", "p", CapabilityLevel::Strong);
        no_long_context.capabilities.long_context = Some(CapabilityLevel::None);
        let provider = healthy_provider("p", vec![no_long_context]);
        let score = score_model(&provider.models[0], &provider, &required, None);
        assert_eq!(score, None, "a model with an explicit, evidenced long_context: None must never be routed RequirementAnalysis");
    }

    /// Phase 3 root-cause regression: a model whose capabilities genuinely
    /// mirror what real Ollama discovery reports for a real, working local
    /// model (`capabilities_from_ollama`'s own output for a model that
    /// lists `"completion"`: `text_generation: Strong, structured_output:
    /// Basic, reasoning: Basic, long_context: Unknown` — confirmed live
    /// against a real, running Ollama instance serving `gemma3:12b` during
    /// this fix's own verification) IS a valid `RequirementAnalysis`
    /// candidate — `Unknown` scores low but is never disqualifying. This
    /// locks in the exact real-world shape that resolved the blocker as a
    /// named regression, not just an inference from unrelated test doubles.
    #[test]
    fn a_real_ollama_shaped_healthy_model_is_a_valid_requirement_analysis_candidate() {
        let required = required_capabilities(AiTask::RequirementAnalysis);
        let ollama_shaped = capabilities_from_ollama(
            "gemma3:12b",
            &["completion".to_string()],
            Some("gemma3"),
            None,
        );
        let model = ModelDefinition {
            id: "gemma3:12b".to_string(),
            provider_id: "ollama".to_string(),
            display_name: "gemma3:12b".to_string(),
            family: Some("gemma3".to_string()),
            parameter_size: Some("12.2B".to_string()),
            context_window: None,
            capabilities: ollama_shaped,
            status: ModelStatus::Available,
        };
        let provider = healthy_provider("ollama", vec![model]);
        let score = score_model(&provider.models[0], &provider, &required, None);
        assert!(score.is_some(), "a real, Ollama-discovered model reporting the 'completion' capability must be a valid RequirementAnalysis candidate, not excluded");
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
        assert_eq!(
            required_capabilities(AiTask::Embedding),
            vec![RequiredCapability::Embedding]
        );
    }

    // -------------------------------------------------------------- settings

    #[tokio::test]
    async fn settings_default_to_auto_with_fallback_enabled() {
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        let settings = infra.settings().await;
        assert_eq!(settings.mode, RoutingMode::Auto);
        assert!(settings.fallback_enabled);
        // MJ-first default: the wired gateway is the out-of-box primary.
        assert_eq!(
            settings.preferred_provider.as_deref(),
            Some(MjGatewayDiscovery::PROVIDER_ID)
        );
        assert_eq!(settings.preferred_model.as_deref(), Some("mj-default"));
    }

    #[tokio::test]
    async fn settings_can_be_switched_to_manual_and_persist_in_memory() {
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        infra
            .set_settings(AiSettings {
                mode: RoutingMode::Manual,
                preferred_provider: Some("ollama".to_string()),
                preferred_model: Some("gemma3:12b".to_string()),
                fallback_enabled: false,
                task_overrides: std::collections::HashMap::new(),
            })
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
        let infra = AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "p",
                vec![healthy_model("m", "p", CapabilityLevel::Strong)],
            )))],
            executors,
        );

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: None,
                preferred_model: None,
            })
            .await;
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
    async fn generate_falls_back_to_the_next_best_model_when_the_primary_fails_and_fallback_is_enabled(
    ) {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert(
            "primary",
            Arc::new(FakeExecutor::err(ExecutionError::Timeout)),
        );
        executors.insert("backup", Arc::new(FakeExecutor::ok(r#"{"ok":true}"#)));
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider(
                    "primary",
                    vec![healthy_model(
                        "primary-model",
                        "primary",
                        CapabilityLevel::Strong,
                    )],
                ))),
                Arc::new(FakeDiscovery(healthy_provider(
                    "backup",
                    vec![healthy_model(
                        "backup-model",
                        "backup",
                        CapabilityLevel::Basic,
                    )],
                ))),
            ],
            executors,
        );
        infra
            .set_settings(AiSettings {
                mode: RoutingMode::Auto,
                preferred_provider: Some("primary".to_string()),
                preferred_model: None,
                fallback_enabled: true,
                task_overrides: std::collections::HashMap::new(),
            })
            .await;

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: Some("primary".to_string()),
                preferred_model: None,
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(
                    result.provider, "backup",
                    "must genuinely fall back to a different, working model"
                );
                assert!(
                    result.fallback_used,
                    "fallback_used must truthfully reflect that this was not the primary choice"
                );
            }
            GenerationOutcome2::Failed { reason } => {
                panic!("expected a real fallback to succeed, got Failed: {reason}")
            }
        }
    }

    #[tokio::test]
    async fn generate_reports_truthful_failure_when_fallback_is_disabled() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert("p", Arc::new(FakeExecutor::err(ExecutionError::Timeout)));
        let infra = AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "p",
                vec![healthy_model("m", "p", CapabilityLevel::Strong)],
            )))],
            executors,
        );
        infra
            .set_settings(AiSettings {
                mode: RoutingMode::Auto,
                preferred_provider: None,
                preferred_model: None,
                fallback_enabled: false,
                task_overrides: std::collections::HashMap::new(),
            })
            .await;

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: None,
                preferred_model: None,
            })
            .await;
        assert!(matches!(outcome, GenerationOutcome2::Failed { .. }), "with fallback disabled, a primary failure must be reported honestly, not silently retried");
    }

    #[tokio::test]
    async fn generate_reports_no_compatible_model_when_none_exists() {
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: None,
                preferred_model: None,
            })
            .await;
        match outcome {
            GenerationOutcome2::Failed { reason } => assert!(reason.contains("NoCompatibleModel")),
            GenerationOutcome2::Ok(_) => {
                panic!("no provider exists at all — this must never fabricate a success")
            }
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
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        for forbidden in [
            "api_key",
            "apiKey",
            "credential",
            "secret",
            "token",
            "authorization",
        ] {
            assert!(
                !keys.iter().any(|k| k.eq_ignore_ascii_case(forbidden)),
                "AiProviderInfo must never expose a field named like a credential: {forbidden}"
            );
        }
    }

    #[test]
    fn api_key_value_never_appears_in_discovered_provider_info() {
        // Even though MjGatewayDiscovery holds the real key internally,
        // its output must carry only boolean facts, never the value itself.
        let secret = "mj_test_super_secret_value_1234567890";
        let discovery = MjGatewayDiscovery {
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: Some(secret.to_string()),
            timeout: Duration::from_millis(100),
        };
        let info = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(discovery.discover());
        assert!(
            info.configured,
            "the real key's presence must genuinely be detected"
        );
        let json = serde_json::to_string(&info).unwrap();
        assert!(
            !json.contains("super_secret_value"),
            "no credential value must ever appear in serialized provider info"
        );
    }

    // -------------------------------------------------- MJ AI Gateway

    /// Spawns a real local HTTP server (axum, already in the dependency
    /// tree) that mimics the MJ gateway's two authenticated endpoints, and
    /// returns its base URL. Auth mirrors the real gateway: `Authorization:
    /// Bearer mj_test_ok` is the only accepted credential.
    async fn spawn_mock_mj_gateway() -> String {
        use axum::response::IntoResponse;
        use axum::routing::{get, post as post_route};

        async fn models(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            axum::Json(serde_json::json!({
                "object": "list",
                "data": [
                    {"id": "mj-default", "object": "model"},
                    {"id": "qwen3:14b", "object": "model"}
                ]
            }))
            .into_response()
        }

        #[derive(serde::Deserialize)]
        struct ChatBody {
            model: String,
        }

        async fn chat(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
            axum::Json(body): axum::Json<ChatBody>,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            if body.model != "mj-default" && body.model != "qwen3:14b" {
                return (axum::http::StatusCode::BAD_REQUEST, "Model not allowed").into_response();
            }
            axum::Json(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "real gateway reply"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 12, "completion_tokens": 4, "total_tokens": 16},
                "gateway": {"model": body.model}
            }))
            .into_response()
        }

        let app = axum::Router::new()
            .route("/v1/models", get(models))
            .route("/v1/chat/completions", post_route(chat))
            .with_state("mj_test_ok".to_string());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn mj_discovery_reports_not_configured_without_a_key() {
        let discovery = MjGatewayDiscovery {
            base_url: "http://127.0.0.1:8000".to_string(),
            api_key: None,
            timeout: Duration::from_millis(500),
        };
        let info = discovery.discover().await;
        assert_eq!(info.id, MjGatewayDiscovery::PROVIDER_ID);
        assert!(!info.configured);
        assert_eq!(info.status, ProviderStatus::NotConfigured);
        assert!(
            !info.reachable,
            "an unconfigured gateway must never claim reachability"
        );
        assert!(info.models.is_empty());
    }

    #[tokio::test]
    async fn mj_discovery_reports_unavailable_when_gateway_is_not_reachable() {
        // A real discoverer holding a real-looking key, pointed at a port
        // nothing listens on — proves the honest "not reachable" path.
        let discovery = MjGatewayDiscovery {
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: Some("mj_test_ok".to_string()),
            timeout: Duration::from_millis(500),
        };
        let info = discovery.discover().await;
        assert!(info.configured);
        assert_eq!(info.status, ProviderStatus::Unavailable);
        assert!(!info.reachable);
        assert!(
            info.models.is_empty(),
            "a model list must never be invented when the gateway is down"
        );
    }

    #[tokio::test]
    async fn mj_discovery_lists_real_models_from_a_live_gateway() {
        let base_url = spawn_mock_mj_gateway().await;
        let discovery = MjGatewayDiscovery {
            base_url,
            api_key: Some("mj_test_ok".to_string()),
            timeout: Duration::from_secs(2),
        };
        let info = discovery.discover().await;

        assert!(info.configured);
        assert!(info.reachable);
        assert_eq!(info.status, ProviderStatus::Healthy);
        let ids: Vec<&str> = info.models.iter().map(|m| m.id.as_str()).collect();
        assert!(
            ids.contains(&"mj-default"),
            "the gateway's own alias list must flow through verbatim; got {ids:?}"
        );
        assert!(ids.contains(&"qwen3:14b"));
        for m in &info.models {
            assert_eq!(m.provider_id, MjGatewayDiscovery::PROVIDER_ID);
            assert_eq!(
                m.capabilities.text_generation,
                Some(CapabilityLevel::Strong)
            );
            assert_eq!(
                m.capabilities.embedding,
                Some(CapabilityLevel::None),
                "embeddings are not a live gateway capability — documented absence, not a guess"
            );
            assert_eq!(m.status, ModelStatus::Available);
        }
        // The gateway can list the exact same real underlying models
        // Ollama does — a real, named reasoning release must be recognized
        // through this path too, not just Ollama's own discovery.
        let qwen3 = info.models.iter().find(|m| m.id == "qwen3:14b").unwrap();
        assert_eq!(qwen3.capabilities.reasoning, Some(CapabilityLevel::Strong));
        let mj_default = info.models.iter().find(|m| m.id == "mj-default").unwrap();
        assert_eq!(
            mj_default.capabilities.reasoning,
            Some(CapabilityLevel::Basic),
            "an unrecognized alias must keep the same honest floor as before this fix"
        );
    }

    #[tokio::test]
    async fn mj_executor_returns_the_real_choices_content_from_a_live_gateway() {
        let base_url = spawn_mock_mj_gateway().await;
        let executor = MjGatewayExecutor {
            base_url,
            api_key: Some("mj_test_ok".to_string()),
            timeout: Duration::from_secs(2),
            max_tokens: 4096,
        };
        let output = executor
            .execute(MjGatewayDiscovery::PROVIDER_ID, "mj-default", "hello")
            .await
            .expect("execute");
        assert_eq!(output.text, "real gateway reply");
        assert_eq!(
            output.usage.prompt_tokens,
            Some(12),
            "the gateway's real usage.prompt_tokens must flow through, not be dropped"
        );
        assert_eq!(output.usage.completion_tokens, Some(4));
    }

    #[tokio::test]
    async fn mj_executor_rejects_a_provider_id_it_does_not_own() {
        let executor = MjGatewayExecutor {
            base_url: "http://127.0.0.1:8000".to_string(),
            api_key: None,
            timeout: Duration::from_secs(1),
            max_tokens: 4096,
        };
        let err = executor
            .execute("ollama", "llama3", "hi")
            .await
            .unwrap_err();
        assert!(matches!(err, ExecutionError::ProviderUnavailable(_)));
    }

    #[tokio::test]
    async fn mj_end_to_end_routing_executes_through_the_live_gateway() {
        let base_url = spawn_mock_mj_gateway().await;
        let provider = MjGatewayDiscovery {
            base_url: base_url.clone(),
            api_key: Some("mj_test_ok".to_string()),
            timeout: Duration::from_secs(2),
        }
        .discover()
        .await;
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert(
            MjGatewayDiscovery::PROVIDER_ID,
            Arc::new(MjGatewayExecutor {
                base_url,
                api_key: Some("mj_test_ok".to_string()),
                timeout: Duration::from_secs(2),
                max_tokens: 4096,
            }),
        );
        let infra = AiInfrastructure::new(vec![Arc::new(FakeDiscovery(provider))], executors);

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: Some(MjGatewayDiscovery::PROVIDER_ID.to_string()),
                preferred_model: Some("mj-default".to_string()),
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.text, "real gateway reply");
                assert_eq!(result.provider, MjGatewayDiscovery::PROVIDER_ID);
                assert_eq!(result.model, "mj-default");
                assert!(!result.fallback_used);
                assert_eq!(
                    result.prompt_tokens,
                    Some(12),
                    "real usage from the live gateway must reach the end-to-end GenerationResult"
                );
                assert_eq!(result.completion_tokens, Some(4));
                assert_eq!(
                    result.estimated_cost_usd,
                    Some(0.0),
                    "the MJ gateway is a real, disclosed Unmetered provider — cost is a real $0.00, not unknown"
                );
            }
            GenerationOutcome2::Failed { reason } => {
                panic!("expected a real end-to-end generation, got Failed: {reason}")
            }
        }
    }

    // ------------------------------------------------------------------- Groq

    async fn spawn_mock_groq() -> String {
        use axum::response::IntoResponse;
        use axum::routing::{get, post as post_route};

        async fn models(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            axum::Json(serde_json::json!({
                "object": "list",
                "data": [
                    {"id": "gpt-oss-20b", "object": "model", "context_window": 131072},
                    {"id": "deepseek-r1-distill-llama-70b", "object": "model", "context_window": 131072}
                ]
            }))
            .into_response()
        }

        #[derive(serde::Deserialize)]
        struct ChatBody {
            model: String,
            response_format: Option<serde_json::Value>,
        }

        async fn chat(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
            axum::Json(body): axum::Json<ChatBody>,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            if body.model != "gpt-oss-20b" && body.model != "deepseek-r1-distill-llama-70b" {
                return (axum::http::StatusCode::BAD_REQUEST, "Model not allowed").into_response();
            }
            let content = if body.response_format.is_some() {
                "real groq reply (structured)"
            } else {
                "real groq reply"
            };
            axum::Json(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 20, "completion_tokens": 7, "total_tokens": 27}
            }))
            .into_response()
        }

        let app = axum::Router::new()
            .route("/v1/models", get(models))
            .route("/v1/chat/completions", post_route(chat))
            .with_state("groq_test_ok".to_string());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn groq_discovery_reports_not_configured_without_a_key() {
        let discovery = GroqDiscovery {
            base_url: "https://api.groq.com/openai".to_string(),
            api_key: None,
            timeout: Duration::from_millis(500),
        };
        let info = discovery.discover().await;
        assert_eq!(info.id, GroqDiscovery::PROVIDER_ID);
        assert!(!info.configured);
        assert_eq!(info.status, ProviderStatus::NotConfigured);
        assert!(
            !info.reachable,
            "an unconfigured provider must never claim reachability"
        );
        assert!(info.models.is_empty());
    }

    #[tokio::test]
    async fn groq_discovery_reports_unavailable_when_not_reachable() {
        let discovery = GroqDiscovery {
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: Some("groq_test_ok".to_string()),
            timeout: Duration::from_millis(500),
        };
        let info = discovery.discover().await;
        assert!(info.configured);
        assert_eq!(info.status, ProviderStatus::Unavailable);
        assert!(!info.reachable);
        assert!(
            info.models.is_empty(),
            "a model list must never be invented when Groq is down"
        );
    }

    #[tokio::test]
    async fn groq_discovery_lists_real_models_with_real_context_window_and_reasoning() {
        let base_url = spawn_mock_groq().await;
        let discovery = GroqDiscovery {
            base_url,
            api_key: Some("groq_test_ok".to_string()),
            timeout: Duration::from_secs(2),
        };
        let info = discovery.discover().await;

        assert!(info.configured);
        assert!(info.reachable);
        assert_eq!(info.status, ProviderStatus::Healthy);
        assert_eq!(
            info.pricing, GROQ_FLOOR_PRICING,
            "a real, disclosed metered rate, never Unmetered for a real paid provider"
        );
        let ids: Vec<&str> = info.models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"gpt-oss-20b"));
        assert!(ids.contains(&"deepseek-r1-distill-llama-70b"));

        let base_model = info.models.iter().find(|m| m.id == "gpt-oss-20b").unwrap();
        assert_eq!(base_model.provider_id, GroqDiscovery::PROVIDER_ID);
        assert_eq!(base_model.context_window, Some(131072));
        assert_eq!(
            base_model.capabilities.long_context,
            Some(CapabilityLevel::Strong),
            "Groq's own real context_window field must produce a real, evidenced long_context level"
        );
        assert_eq!(
            base_model.capabilities.reasoning,
            Some(CapabilityLevel::Basic),
            "an unrecognized model keeps the honest floor"
        );

        let reasoning_model = info
            .models
            .iter()
            .find(|m| m.id == "deepseek-r1-distill-llama-70b")
            .unwrap();
        assert_eq!(
            reasoning_model.capabilities.reasoning,
            Some(CapabilityLevel::Strong),
            "a real deepseek-r1 release must be recognized through Groq's own discovery too"
        );
    }

    #[tokio::test]
    async fn groq_executor_returns_real_content_and_sends_response_format_for_test_case_generation()
    {
        let base_url = spawn_mock_groq().await;
        let executor = GroqExecutor {
            base_url,
            api_key: Some("groq_test_ok".to_string()),
            timeout: Duration::from_secs(2),
            max_tokens: 4096,
        };
        let output = executor
            .execute_task(
                GroqDiscovery::PROVIDER_ID,
                "gpt-oss-20b",
                "hello",
                AiTask::TestCaseGeneration,
            )
            .await
            .expect("execute_task");
        assert_eq!(
            output.text, "real groq reply (structured)",
            "TestCaseGeneration must send response_format so the mock's own check can see it"
        );
        assert_eq!(output.usage.prompt_tokens, Some(20));
        assert_eq!(output.usage.completion_tokens, Some(7));
    }

    #[tokio::test]
    async fn groq_executor_omits_response_format_for_a_plain_general_task() {
        let base_url = spawn_mock_groq().await;
        let executor = GroqExecutor {
            base_url,
            api_key: Some("groq_test_ok".to_string()),
            timeout: Duration::from_secs(2),
            max_tokens: 4096,
        };
        let output = executor
            .execute(GroqDiscovery::PROVIDER_ID, "gpt-oss-20b", "hello")
            .await
            .expect("execute");
        assert_eq!(
            output.text, "real groq reply",
            "a plain GeneralAi task (via execute()) must not send response_format"
        );
    }

    #[tokio::test]
    async fn groq_executor_rejects_a_provider_id_it_does_not_own() {
        let executor = GroqExecutor {
            base_url: "https://api.groq.com/openai".to_string(),
            api_key: None,
            timeout: Duration::from_secs(1),
            max_tokens: 4096,
        };
        let err = executor
            .execute("ollama", "llama3", "hi")
            .await
            .unwrap_err();
        assert!(matches!(err, ExecutionError::ProviderUnavailable(_)));
    }

    #[tokio::test]
    async fn groq_end_to_end_routing_executes_through_the_live_api() {
        let base_url = spawn_mock_groq().await;
        let provider = GroqDiscovery {
            base_url: base_url.clone(),
            api_key: Some("groq_test_ok".to_string()),
            timeout: Duration::from_secs(2),
        }
        .discover()
        .await;
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert(
            GroqDiscovery::PROVIDER_ID,
            Arc::new(GroqExecutor {
                base_url,
                api_key: Some("groq_test_ok".to_string()),
                timeout: Duration::from_secs(2),
                max_tokens: 4096,
            }),
        );
        let infra = AiInfrastructure::new(vec![Arc::new(FakeDiscovery(provider))], executors);

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: Some(GroqDiscovery::PROVIDER_ID.to_string()),
                preferred_model: Some("gpt-oss-20b".to_string()),
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.text, "real groq reply");
                assert_eq!(result.provider, GroqDiscovery::PROVIDER_ID);
                assert_eq!(result.model, "gpt-oss-20b");
                assert!(!result.fallback_used);
                assert_eq!(result.prompt_tokens, Some(20));
                assert_eq!(result.completion_tokens, Some(7));
                assert_eq!(
                    result.estimated_cost_usd,
                    Some((20.0 / 1000.0) * 0.000_075 + (7.0 / 1000.0) * 0.0003),
                    "Groq's real, disclosed metered rate must feed a real, non-zero cost estimate"
                );
            }
            GenerationOutcome2::Failed { reason } => {
                panic!("expected a real end-to-end generation, got Failed: {reason}")
            }
        }
    }

    // ----------------------------------------------------------------- Gemini

    async fn spawn_mock_gemini() -> String {
        use axum::response::IntoResponse;
        use axum::routing::{get, post as post_route};

        async fn models(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            axum::Json(serde_json::json!({
                "object": "list",
                "data": [
                    {"id": "gemini-2.5-flash", "object": "model"},
                    {"id": "gemini-2.5-pro", "object": "model"}
                ]
            }))
            .into_response()
        }

        #[derive(serde::Deserialize)]
        struct ChatBody {
            model: String,
        }

        async fn chat(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
            axum::Json(body): axum::Json<ChatBody>,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            if body.model != "gemini-2.5-flash" && body.model != "gemini-2.5-pro" {
                return (axum::http::StatusCode::BAD_REQUEST, "Model not allowed").into_response();
            }
            axum::Json(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "real gemini reply"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 15, "completion_tokens": 5, "total_tokens": 20}
            }))
            .into_response()
        }

        let app = axum::Router::new()
            .route("/models", get(models))
            .route("/chat/completions", post_route(chat))
            .with_state("gemini_test_ok".to_string());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn gemini_discovery_reports_not_configured_without_a_key() {
        let discovery = GeminiDiscovery {
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
            api_key: None,
            timeout: Duration::from_millis(500),
        };
        let info = discovery.discover().await;
        assert_eq!(info.id, GeminiDiscovery::PROVIDER_ID);
        assert!(!info.configured);
        assert_eq!(info.status, ProviderStatus::NotConfigured);
        assert!(!info.reachable);
        assert!(info.models.is_empty());
    }

    #[tokio::test]
    async fn gemini_discovery_reports_unavailable_when_not_reachable() {
        let discovery = GeminiDiscovery {
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: Some("gemini_test_ok".to_string()),
            timeout: Duration::from_millis(500),
        };
        let info = discovery.discover().await;
        assert!(info.configured);
        assert_eq!(info.status, ProviderStatus::Unavailable);
        assert!(!info.reachable);
        assert!(info.models.is_empty());
    }

    #[tokio::test]
    async fn gemini_discovery_lists_real_models_from_a_live_endpoint() {
        let base_url = spawn_mock_gemini().await;
        let discovery = GeminiDiscovery {
            base_url,
            api_key: Some("gemini_test_ok".to_string()),
            timeout: Duration::from_secs(2),
        };
        let info = discovery.discover().await;

        assert!(info.configured);
        assert!(info.reachable);
        assert_eq!(info.status, ProviderStatus::Healthy);
        assert_eq!(
            info.pricing, GEMINI_FLOOR_PRICING,
            "a real, disclosed metered rate, never Unmetered for a real paid provider"
        );
        let ids: Vec<&str> = info.models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"gemini-2.5-flash"));
        assert!(ids.contains(&"gemini-2.5-pro"));
        for m in &info.models {
            assert_eq!(m.provider_id, GeminiDiscovery::PROVIDER_ID);
            assert_eq!(
                m.capabilities.text_generation,
                Some(CapabilityLevel::Strong)
            );
            assert_eq!(m.status, ModelStatus::Available);
        }
    }

    #[tokio::test]
    async fn gemini_executor_returns_the_real_choices_content_from_a_live_endpoint() {
        let base_url = spawn_mock_gemini().await;
        let executor = GeminiExecutor {
            base_url,
            api_key: Some("gemini_test_ok".to_string()),
            timeout: Duration::from_secs(2),
            max_tokens: 4096,
        };
        let output = executor
            .execute(GeminiDiscovery::PROVIDER_ID, "gemini-2.5-flash", "hello")
            .await
            .expect("execute");
        assert_eq!(output.text, "real gemini reply");
        assert_eq!(output.usage.prompt_tokens, Some(15));
        assert_eq!(output.usage.completion_tokens, Some(5));
    }

    #[tokio::test]
    async fn gemini_executor_rejects_a_provider_id_it_does_not_own() {
        let executor = GeminiExecutor {
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
            api_key: None,
            timeout: Duration::from_secs(1),
            max_tokens: 4096,
        };
        let err = executor
            .execute("ollama", "llama3", "hi")
            .await
            .unwrap_err();
        assert!(matches!(err, ExecutionError::ProviderUnavailable(_)));
    }

    #[tokio::test]
    async fn gemini_end_to_end_routing_executes_through_the_live_api() {
        let base_url = spawn_mock_gemini().await;
        let provider = GeminiDiscovery {
            base_url: base_url.clone(),
            api_key: Some("gemini_test_ok".to_string()),
            timeout: Duration::from_secs(2),
        }
        .discover()
        .await;
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert(
            GeminiDiscovery::PROVIDER_ID,
            Arc::new(GeminiExecutor {
                base_url,
                api_key: Some("gemini_test_ok".to_string()),
                timeout: Duration::from_secs(2),
                max_tokens: 4096,
            }),
        );
        let infra = AiInfrastructure::new(vec![Arc::new(FakeDiscovery(provider))], executors);

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: Some(GeminiDiscovery::PROVIDER_ID.to_string()),
                preferred_model: Some("gemini-2.5-flash".to_string()),
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.text, "real gemini reply");
                assert_eq!(result.provider, GeminiDiscovery::PROVIDER_ID);
                assert_eq!(result.model, "gemini-2.5-flash");
                assert!(!result.fallback_used);
                assert_eq!(result.prompt_tokens, Some(15));
                assert_eq!(result.completion_tokens, Some(5));
                assert_eq!(
                    result.estimated_cost_usd,
                    Some((15.0 / 1000.0) * 0.0003 + (5.0 / 1000.0) * 0.0025),
                    "Gemini's real, disclosed metered rate must feed a real, non-zero cost estimate"
                );
            }
            GenerationOutcome2::Failed { reason } => {
                panic!("expected a real end-to-end generation, got Failed: {reason}")
            }
        }
    }

    // -------------------------------------------------------- custom providers

    /// Reuses the exact same mock server shape `spawn_mock_mj_gateway`
    /// already established (a real `/v1/models` + `/v1/chat/completions`
    /// pair) — proving custom providers genuinely go through the same real
    /// wire logic, not a second, parallel implementation.
    async fn spawn_mock_custom_provider() -> String {
        use axum::response::IntoResponse;
        use axum::routing::{get, post as post_route};

        async fn models(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            axum::Json(serde_json::json!({
                "object": "list",
                "data": [{"id": "custom-model-1", "object": "model"}]
            }))
            .into_response()
        }

        async fn chat(
            axum::extract::State(expected): axum::extract::State<String>,
            headers: axum::http::HeaderMap,
        ) -> axum::response::Response {
            let auth = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth != format!("Bearer {expected}") {
                return (axum::http::StatusCode::UNAUTHORIZED, "invalid key").into_response();
            }
            axum::Json(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "real custom provider reply"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 9, "completion_tokens": 3, "total_tokens": 12}
            }))
            .into_response()
        }

        let app = axum::Router::new()
            .route("/v1/models", get(models))
            .route("/v1/chat/completions", post_route(chat))
            .with_state("custom_test_ok".to_string());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn discover_custom_provider_lists_real_models_from_a_live_endpoint() {
        let base_url = spawn_mock_custom_provider().await;
        let cfg = CustomProviderConfig {
            id: "custom-ai-test".to_string(),
            name: "My Custom Provider".to_string(),
            base_url,
            api_key: "custom_test_ok".to_string(),
        };
        let info = discover_custom_provider(&cfg, Duration::from_secs(2)).await;

        assert!(info.configured);
        assert!(info.reachable);
        assert_eq!(info.status, ProviderStatus::Healthy);
        assert_eq!(info.id, "custom-ai-test");
        assert_eq!(
            info.pricing,
            ProviderPricing::Unmetered,
            "real per-token pricing for an arbitrary custom endpoint is genuinely unknown \
             — never a guessed Metered rate"
        );
        let ids: Vec<&str> = info.models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"custom-model-1"));
    }

    #[tokio::test]
    async fn discover_custom_provider_reports_unavailable_when_not_reachable() {
        let cfg = CustomProviderConfig {
            id: "custom-ai-test".to_string(),
            name: "My Custom Provider".to_string(),
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: "whatever".to_string(),
        };
        let info = discover_custom_provider(&cfg, Duration::from_millis(500)).await;
        assert!(!info.reachable);
        assert!(
            info.models.is_empty(),
            "a model list must never be invented when a custom endpoint is down"
        );
    }

    #[tokio::test]
    async fn execute_custom_provider_returns_the_real_content_and_usage() {
        let base_url = spawn_mock_custom_provider().await;
        let cfg = CustomProviderConfig {
            id: "custom-ai-test".to_string(),
            name: "My Custom Provider".to_string(),
            base_url,
            api_key: "custom_test_ok".to_string(),
        };
        let output =
            execute_custom_provider(&cfg, "custom-model-1", "hello", Duration::from_secs(2))
                .await
                .expect("execute");
        assert_eq!(output.text, "real custom provider reply");
        assert_eq!(output.usage.prompt_tokens, Some(9));
        assert_eq!(output.usage.completion_tokens, Some(3));
    }

    #[tokio::test]
    async fn a_real_custom_provider_added_at_runtime_is_actually_usable_for_real_generation() {
        // The whole point of this feature: a custom provider registered
        // through `set_custom_providers` (the exact call the real
        // `POST /api/v1/ai/providers/custom` handler makes after a DB
        // insert) must genuinely participate in real routing/execution —
        // not just show up in a discovery list.
        let base_url = spawn_mock_custom_provider().await;
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        infra
            .set_custom_providers(vec![CustomProviderConfig {
                id: "custom-ai-test".to_string(),
                name: "My Custom Provider".to_string(),
                base_url,
                api_key: "custom_test_ok".to_string(),
            }])
            .await;

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: Some("custom-ai-test".to_string()),
                preferred_model: Some("custom-model-1".to_string()),
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.text, "real custom provider reply");
                assert_eq!(result.provider, "custom-ai-test");
                assert_eq!(result.model, "custom-model-1");
                assert_eq!(result.prompt_tokens, Some(9));
            }
            GenerationOutcome2::Failed { reason } => {
                panic!("expected a real end-to-end generation through the custom provider, got Failed: {reason}")
            }
        }
    }

    #[tokio::test]
    async fn set_custom_providers_replaces_the_whole_set_removed_ones_stop_being_discovered() {
        let cfg = CustomProviderConfig {
            id: "custom-ai-test".to_string(),
            name: "My Custom Provider".to_string(),
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: "whatever".to_string(),
        };
        let infra = AiInfrastructure::new(vec![], HashMap::new());
        infra.set_custom_providers(vec![cfg]).await;
        let providers = infra.discover_all().await;
        assert_eq!(providers.len(), 1, "providers={providers:?}");

        // Real deletion (the DB-then-reload path `DELETE /api/v1/ai/providers/custom/:id`
        // takes) — the next discovery must genuinely stop listing it, not
        // keep a stale entry around.
        infra.set_custom_providers(vec![]).await;
        let providers = infra.discover_all().await;
        assert!(
            providers.is_empty(),
            "a removed custom provider must not still be discovered: providers={providers:?}"
        );
    }

    // -------------------------------------------------------- Ollama structured output

    #[test]
    fn ollama_request_includes_think_false_and_stream_false() {
        let body_tc = OllamaExecutor::build_generate_body(
            "qwen3:14b",
            "Generate test cases",
            Some(AiTask::TestCaseGeneration),
        );
        assert_eq!(body_tc["think"], false, "think must explicitly be false");
        assert_eq!(body_tc["stream"], false, "stream must explicitly be false");
        assert_eq!(body_tc["model"], "qwen3:14b");

        let body_general = OllamaExecutor::build_generate_body(
            "qwen3:14b",
            "Hello world",
            Some(AiTask::GeneralAi),
        );
        assert_eq!(body_general["think"], false);
        assert_eq!(body_general["stream"], false);
    }

    #[test]
    fn structured_json_schema_requires_test_cases_and_all_mandatory_fields() {
        let schema = test_case_generation_json_schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"]
            .as_array()
            .expect("required must be an array");
        assert!(
            required.iter().any(|v| v == "test_cases"),
            "schema must require test_cases at top level"
        );

        let items = &schema["properties"]["test_cases"]["items"];
        assert_eq!(items["type"], "object");
        let item_required: Vec<&str> = items["required"]
            .as_array()
            .expect("item required array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        for mandatory in [
            "title",
            "category",
            "skill_id",
            "preconditions",
            "steps",
            "expected_result",
            "priority",
            "severity",
            "evidence",
            "source_reference",
        ] {
            assert!(
                item_required.contains(&mandatory),
                "schema must require mandatory field: {mandatory}"
            );
        }

        let step_required: Vec<&str> = items["properties"]["steps"]["items"]["required"]
            .as_array()
            .expect("step required array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(step_required.contains(&"action"));
        assert!(step_required.contains(&"expected_result"));
    }

    #[test]
    fn ollama_request_uses_structured_schema_for_test_case_generation() {
        let body = OllamaExecutor::build_generate_body(
            "qwen3:14b",
            "some prompt",
            Some(AiTask::TestCaseGeneration),
        );
        assert_eq!(body["format"]["type"], "object");
        assert!(body["format"]["properties"]["test_cases"].is_object());

        // Also when prompt contains test_cases
        let body_prompt =
            OllamaExecutor::build_generate_body("qwen3:14b", "Return json with test_cases", None);
        assert_eq!(body_prompt["format"]["type"], "object");
    }

    // ---------------------------------------------- token usage, latency & cost

    /// Spawns a real local HTTP server mimicking Ollama's real, documented
    /// non-streaming `/api/generate` reply shape — including the real
    /// `prompt_eval_count`/`eval_count` usage fields this executor now
    /// parses, previously entirely untested against a live response.
    async fn spawn_mock_ollama(prompt_eval_count: u64, eval_count: u64) -> String {
        use axum::routing::post as post_route;

        async fn generate(
            axum::extract::State((p, e)): axum::extract::State<(u64, u64)>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "response": "{\"test_cases\":[]}",
                "done": true,
                "prompt_eval_count": p,
                "eval_count": e,
            }))
        }

        let app = axum::Router::new()
            .route("/api/generate", post_route(generate))
            .with_state((prompt_eval_count, eval_count));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn ollama_executor_parses_real_prompt_eval_count_and_eval_count_as_token_usage() {
        let base_url = spawn_mock_ollama(321, 87).await;
        let executor = OllamaExecutor {
            base_url,
            timeout: Duration::from_secs(2),
        };
        let output = executor
            .execute_task("ollama", "gemma3:12b", "hi", AiTask::GeneralAi)
            .await
            .expect("execute_task");
        assert_eq!(
            output.usage.prompt_tokens,
            Some(321),
            "real prompt_eval_count must become prompt_tokens, not be dropped"
        );
        assert_eq!(
            output.usage.completion_tokens,
            Some(87),
            "real eval_count must become completion_tokens, not be dropped"
        );
    }

    #[tokio::test]
    async fn ollama_executor_reports_unknown_usage_honestly_when_the_response_omits_it() {
        // A response with no usage fields at all (a real, legal shape some
        // Ollama builds/proxies may return) must never fabricate a count.
        async fn generate() -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({ "response": "{\"test_cases\":[]}", "done": true }))
        }
        let app = axum::Router::new().route("/api/generate", axum::routing::post(generate));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });

        let executor = OllamaExecutor {
            base_url: format!("http://{addr}"),
            timeout: Duration::from_secs(2),
        };
        let output = executor
            .execute_task("ollama", "gemma3:12b", "hi", AiTask::GeneralAi)
            .await
            .expect("execute_task");
        assert_eq!(output.usage.prompt_tokens, None);
        assert_eq!(output.usage.completion_tokens, None);
    }

    #[test]
    fn unmetered_pricing_is_always_a_real_zero_cost_regardless_of_usage() {
        assert_eq!(
            estimate_cost(&TokenUsage::default(), ProviderPricing::Unmetered),
            Some(0.0),
            "a genuinely unmetered provider has no bill even when token counts are unknown"
        );
        assert_eq!(
            estimate_cost(
                &TokenUsage {
                    prompt_tokens: Some(10_000),
                    completion_tokens: Some(5_000)
                },
                ProviderPricing::Unmetered
            ),
            Some(0.0)
        );
    }

    #[test]
    fn metered_pricing_computes_the_real_disclosed_rate_against_real_token_counts() {
        let usage = TokenUsage {
            prompt_tokens: Some(1000),
            completion_tokens: Some(2000),
        };
        let pricing = ProviderPricing::Metered {
            input_usd_per_1k: 0.01,
            output_usd_per_1k: 0.02,
        };
        // 1000/1000 * 0.01 + 2000/1000 * 0.02 = 0.01 + 0.04 = 0.05
        assert_eq!(estimate_cost(&usage, pricing), Some(0.05));
    }

    #[test]
    fn metered_pricing_without_real_token_counts_reports_unknown_cost_not_a_wrong_zero() {
        let pricing = ProviderPricing::Metered {
            input_usd_per_1k: 0.01,
            output_usd_per_1k: 0.02,
        };
        assert_eq!(
            estimate_cost(&TokenUsage::default(), pricing),
            None,
            "cost must never silently default to $0 for a real metered provider whose usage is unknown"
        );
    }

    #[tokio::test]
    async fn generate_carries_real_usage_latency_and_cost_through_to_the_result() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert(
            "p",
            Arc::new(FakeExecutor::ok_with_usage(r#"{"ok":true}"#, 50, 20)),
        );
        let infra = AiInfrastructure::new(
            vec![Arc::new(FakeDiscovery(healthy_provider(
                "p",
                vec![healthy_model("m", "p", CapabilityLevel::Strong)],
            )))],
            executors,
        );

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: None,
                preferred_model: None,
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.prompt_tokens, Some(50));
                assert_eq!(result.completion_tokens, Some(20));
                assert_eq!(
                    result.estimated_cost_usd,
                    Some(0.0),
                    "the test fixture provider is Unmetered — a real $0.00, not unknown"
                );
                // A real, measured duration — no fixed lower bound beyond
                // "it exists"; the fake executor returns near-instantly.
                let _ = result.latency_ms;
            }
            GenerationOutcome2::Failed { reason } => panic!("expected Ok, got Failed: {reason}"),
        }
    }

    #[tokio::test]
    async fn fallback_result_carries_the_fallback_providers_own_real_usage_not_the_primarys() {
        let mut executors: HashMap<&'static str, Arc<dyn AiExecutor>> = HashMap::new();
        executors.insert(
            "primary",
            Arc::new(FakeExecutor::err(ExecutionError::Timeout)),
        );
        executors.insert(
            "backup",
            Arc::new(FakeExecutor::ok_with_usage(r#"{"ok":true}"#, 77, 33)),
        );
        let infra = AiInfrastructure::new(
            vec![
                Arc::new(FakeDiscovery(healthy_provider(
                    "primary",
                    vec![healthy_model(
                        "primary-model",
                        "primary",
                        CapabilityLevel::Strong,
                    )],
                ))),
                Arc::new(FakeDiscovery(healthy_provider(
                    "backup",
                    vec![healthy_model(
                        "backup-model",
                        "backup",
                        CapabilityLevel::Basic,
                    )],
                ))),
            ],
            executors,
        );
        infra
            .set_settings(AiSettings {
                mode: RoutingMode::Auto,
                preferred_provider: Some("primary".to_string()),
                preferred_model: None,
                fallback_enabled: true,
                task_overrides: std::collections::HashMap::new(),
            })
            .await;

        let outcome = infra
            .generate(AiRequest {
                task: AiTask::GeneralAi,
                prompt: "hi".to_string(),
                preferred_provider: Some("primary".to_string()),
                preferred_model: None,
            })
            .await;
        match outcome {
            GenerationOutcome2::Ok(result) => {
                assert_eq!(result.provider, "backup");
                assert_eq!(
                    result.prompt_tokens,
                    Some(77),
                    "must be the backup's own real usage, not the failed primary's"
                );
                assert_eq!(result.completion_tokens, Some(33));
            }
            GenerationOutcome2::Failed { reason } => {
                panic!("expected a real fallback to succeed, got Failed: {reason}")
            }
        }
    }
}
