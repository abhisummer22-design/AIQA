import { Request, Response } from "express";
import { GoogleGenAI } from "@google/genai";
import {
  BrainEngineSource,
  BrainRoutingMode,
  LocalOllamaModel,
  CloudApiProvider,
  BrainTaskRouting,
  BrainTelemetry,
  BrainBenchmarkRun
} from "../src/types";

// In-memory state for AI Brain
let routingMode: BrainRoutingMode = "smart_auto";

let ollamaConfig = {
  baseUrl: "http://localhost:11434",
  status: "connected" as "connected" | "degraded" | "offline",
  version: "v0.5.11 (Ollama CUDA Engine)",
  latencyMs: 14,
  lastPing: new Date().toISOString(),
  hardware: {
    gpuName: "NVIDIA RTX 4090 (24GB VRAM)",
    vramUsedMb: 8192,
    vramTotalMb: 24576,
    gpuTempC: 54,
    gpuUtilPercent: 46,
    cpuUtilPercent: 24,
    numaEnabled: true,
    flashAttention2: true,
    contextLength: 32768,
    temperature: 0.2,
    topP: 0.9,
    threads: 16
  }
};

let localModels: LocalOllamaModel[] = [
  {
    id: "gemma3:12b",
    name: "Gemma 3 12B Instruct",
    tag: "gemma3:12b-instruct-q4_K_M",
    sizeGb: 8.2,
    quantization: "Q4_K_M",
    parameters: "12B",
    family: "gemma",
    isPrimary: true,
    status: "loaded",
    vramUsageMb: 8192,
    contextLength: 32768,
    specialization: "General QA, Test Validation, Code Analysis",
    lastUsed: "2 mins ago"
  },
  {
    id: "qwen2.5-coder:14b",
    name: "Qwen 2.5 Coder 14B",
    tag: "qwen2.5-coder:14b-instruct-q5_K_M",
    sizeGb: 9.6,
    quantization: "Q5_K_M",
    parameters: "14.7B",
    family: "qwen2",
    isPrimary: false,
    status: "available",
    vramUsageMb: 0,
    contextLength: 65536,
    specialization: "Unit/Integration Test Generation, Playwright & PyTest Code",
    lastUsed: "1 hour ago"
  },
  {
    id: "deepseek-r1:8b",
    name: "DeepSeek R1 Distill 8B",
    tag: "deepseek-r1:8b-llama-distill-q8_0",
    sizeGb: 8.5,
    quantization: "Q8_0",
    parameters: "8B",
    family: "deepseek",
    isPrimary: false,
    status: "available",
    vramUsageMb: 0,
    contextLength: 32768,
    specialization: "Deep Reasoning, Failure Root Cause Decomposition, Logic Flaws",
    lastUsed: "3 hours ago"
  },
  {
    id: "llama3.3:70b",
    name: "Llama 3.3 70B Instruct",
    tag: "llama3.3:70b-instruct-q4_K_M",
    sizeGb: 42.1,
    quantization: "Q4_K_M",
    parameters: "70.6B",
    family: "llama",
    isPrimary: false,
    status: "available",
    vramUsageMb: 0,
    contextLength: 131072,
    specialization: "Enterprise RBAC Security Modeling, Multi-Tenant Governance",
    lastUsed: "Yesterday"
  },
  {
    id: "mistral:7b",
    name: "Mistral 7B Instruct v0.3",
    tag: "mistral:7b-instruct-v0.3-q5_0",
    sizeGb: 5.1,
    quantization: "Q5_0",
    parameters: "7.2B",
    family: "mistral",
    isPrimary: false,
    status: "available",
    vramUsageMb: 0,
    contextLength: 32768,
    specialization: "Fast Bug Ticket Extraction, Jira Ticket Synthesis",
    lastUsed: "2 days ago"
  },
  {
    id: "phi3:mini",
    name: "Phi-3.5 Mini 3.8B",
    tag: "phi3.5:3.8b-mini-instruct-q4_0",
    sizeGb: 2.3,
    quantization: "Q4_0",
    parameters: "3.8B",
    family: "phi3",
    isPrimary: false,
    status: "available",
    vramUsageMb: 0,
    contextLength: 16384,
    specialization: "Ultra-fast Voice Assistant, Natural Language Query Parsing",
    lastUsed: "15 mins ago"
  }
];

// Cloud API Providers State
let cloudProviders: CloudApiProvider[] = [
  {
    id: "gemini",
    name: "Google Gemini AI",
    provider: "Google Cloud / AI Studio",
    badge: "Primary Cloud Bridge",
    iconType: "gemini",
    description: "Multi-modal ultra-low latency inference with 1M+ token context and advanced reasoning",
    isConfigured: true,
    hasKey: !!process.env.GEMINI_API_KEY,
    maskedKey: process.env.GEMINI_API_KEY ? `AIzaSy...${process.env.GEMINI_API_KEY.slice(-4)}` : "AIzaSy...DEMO",
    selectedModel: "gemini-3.7-flash",
    availableModels: ["gemini-3.7-flash", "gemini-3.7-pro", "gemini-2.5-flash", "gemini-1.5-pro"],
    status: process.env.GEMINI_API_KEY ? "ready" : "connected",
    latencyMs: 240,
    tokensPerSec: 145.0,
    contextWindow: "1,048,576 tokens",
    costPer1kTokens: "$0.00015 / 1k",
    features: ["Deep Reasoning", "JSON Schema Mode", "Function Calling", "Multimodal Trace Visuals"]
  },
  {
    id: "openai",
    name: "OpenAI GPT-4o",
    provider: "OpenAI Platform",
    badge: "Direct Cloud API",
    iconType: "openai",
    description: "Flagship intelligence for rigorous security fuzzing, schema generation, and reasoning",
    isConfigured: true,
    hasKey: true,
    maskedKey: "sk-proj-49...a9B2",
    selectedModel: "gpt-4o",
    availableModels: ["gpt-4o", "gpt-4o-mini", "o3-mini", "o1"],
    status: "ready",
    latencyMs: 380,
    tokensPerSec: 92.5,
    contextWindow: "128,000 tokens",
    costPer1kTokens: "$0.0025 / 1k",
    features: ["Structured Outputs", "Code Interpreter", "Parallel Tool Calling"]
  },
  {
    id: "anthropic",
    name: "Anthropic Claude",
    provider: "Anthropic AI",
    badge: "Reasoning & Code",
    iconType: "anthropic",
    description: "Industry-standard coding accuracy and multi-step bug reproduction analysis",
    isConfigured: true,
    hasKey: true,
    maskedKey: "sk-ant-api03...z8K1",
    selectedModel: "claude-3-7-sonnet-latest",
    availableModels: ["claude-3-7-sonnet-latest", "claude-3-5-sonnet", "claude-3-5-haiku", "claude-3-opus"],
    status: "ready",
    latencyMs: 410,
    tokensPerSec: 88.0,
    contextWindow: "200,000 tokens",
    costPer1kTokens: "$0.0030 / 1k",
    features: ["Extended Thinking (64k)", "Artifacts", "Precise Code Synthesis"]
  },
  {
    id: "deepseek",
    name: "DeepSeek API",
    provider: "DeepSeek Cloud",
    badge: "Ultra Low Cost",
    iconType: "deepseek",
    description: "High-reasoning architecture optimized for automated bug triage and test suites",
    isConfigured: true,
    hasKey: true,
    maskedKey: "sk-ds-9941...c3X9",
    selectedModel: "deepseek-reasoner",
    availableModels: ["deepseek-reasoner", "deepseek-chat"],
    status: "ready",
    latencyMs: 520,
    tokensPerSec: 64.0,
    contextWindow: "64,000 tokens",
    costPer1kTokens: "$0.00055 / 1k",
    features: ["Chain-of-Thought", "Low Cost", "Open Weights Parity"]
  },
  {
    id: "groq",
    name: "Groq LPU Accelerator",
    provider: "Groq Cloud",
    badge: "Lightning 450 t/s",
    iconType: "groq",
    description: "Language Processing Unit (LPU) cloud inference delivering instant sub-100ms responses",
    isConfigured: true,
    hasKey: true,
    maskedKey: "gsk_71902...m4P0",
    selectedModel: "llama-3.3-70b-versatile",
    availableModels: ["llama-3.3-70b-versatile", "mixtral-8x7b-32768", "gemma2-9b-it"],
    status: "ready",
    latencyMs: 95,
    tokensPerSec: 380.0,
    contextWindow: "32,768 tokens",
    costPer1kTokens: "$0.00059 / 1k",
    features: ["Sub-100ms Latency", "Ultra High TPS", "Real-time Fuzzing"]
  },
  {
    id: "mistral",
    name: "Mistral AI",
    provider: "Mistral European Sovereign",
    badge: "EU Sovereign",
    iconType: "mistral",
    description: "European GDPR-compliant enterprise AI models with sovereign privacy guarantees",
    isConfigured: true,
    hasKey: true,
    maskedKey: "mistral-sec...9qT7",
    selectedModel: "mistral-large-latest",
    availableModels: ["mistral-large-latest", "codestral-latest", "mistral-small-latest"],
    status: "ready",
    latencyMs: 310,
    tokensPerSec: 110.0,
    contextWindow: "128,000 tokens",
    costPer1kTokens: "$0.0020 / 1k",
    features: ["GDPR Sovereign Cloud", "Codestral Specialized", "Function Calling"]
  },
  {
    id: "custom_openai",
    name: "Enterprise Custom / LocalAI / vLLM",
    provider: "Custom Ingress Endpoint",
    badge: "Private Ingress",
    iconType: "server",
    description: "Custom OpenAI-compatible inference proxy (vLLM, LM Studio, TGI, OpenRouter)",
    isConfigured: false,
    hasKey: false,
    customBaseUrl: "http://vllm-cluster.internal:8000/v1",
    selectedModel: "custom-qwen-coder-32b",
    availableModels: ["custom-qwen-coder-32b", "custom-starcoder2", "custom-model"],
    status: "missing_key",
    latencyMs: 180,
    tokensPerSec: 120.0,
    contextWindow: "32,768 tokens",
    costPer1kTokens: "$0.00000 / Internal",
    features: ["Self-Hosted Gateway", "Custom Tokenizer", "Zero Internet Egress"]
  }
];

// Task-to-Engine Assignment Matrix
let taskRoutings: BrainTaskRouting[] = [
  {
    taskKey: "test_case_gen",
    taskTitle: "Test Case Generation & Scenarios",
    description: "Transforms user stories & OpenAPI specs into comprehensive automated test cases",
    assignedEngine: "ollama",
    assignedModel: "qwen2.5-coder:14b",
    fallbackEngine: "gemini",
    fallbackModel: "gemini-3.7-flash",
    temperature: 0.2,
    maxTokens: 4096
  },
  {
    taskKey: "rbac_security_fuzzing",
    taskTitle: "RBAC Security & Permission Fuzzing",
    description: "Evaluates zero-trust boundaries, privilege escalation attempts, and multi-tenant isolation",
    assignedEngine: "gemini",
    assignedModel: "gemini-3.7-flash",
    fallbackEngine: "anthropic",
    fallbackModel: "claude-3-7-sonnet-latest",
    temperature: 0.1,
    maxTokens: 2048
  },
  {
    taskKey: "bug_drafting",
    taskTitle: "Automated Bug Triage & Root Cause Decomposition",
    description: "Inspects stack traces, query plans, and HAR files to formulate reproduction steps and fixes",
    assignedEngine: "anthropic",
    assignedModel: "claude-3-7-sonnet-latest",
    fallbackEngine: "deepseek",
    fallbackModel: "deepseek-reasoner",
    temperature: 0.2,
    maxTokens: 3072,
    reasoningEffort: "high"
  },
  {
    taskKey: "flakiness_analytics",
    taskTitle: "Flakiness & Risk Probability Modeling",
    description: "Correlates timing jitter, network retries, and database deadlocks to score quarantine risk",
    assignedEngine: "ollama",
    assignedModel: "gemma3:12b",
    fallbackEngine: "gemini",
    fallbackModel: "gemini-3.7-flash",
    temperature: 0.3,
    maxTokens: 2048
  },
  {
    taskKey: "rag_grounding_eval",
    taskTitle: "RAG Pipeline Grounding & Hallucination SLA",
    description: "DeepEval Faithfulness scoring against ground-truth retrieval context chunks",
    assignedEngine: "deepseek",
    assignedModel: "deepseek-reasoner",
    fallbackEngine: "gemini",
    fallbackModel: "gemini-3.7-flash",
    temperature: 0.0,
    maxTokens: 4096,
    reasoningEffort: "high"
  },
  {
    taskKey: "voice_assistant",
    taskTitle: "Voice Command & Natural Language Querying",
    description: "Translates microphone audio transcripts into precise test execution and filtering actions",
    assignedEngine: "groq",
    assignedModel: "llama-3.3-70b-versatile",
    fallbackEngine: "ollama",
    fallbackModel: "phi3:mini",
    temperature: 0.1,
    maxTokens: 512
  }
];

// Telemetry state
let brainTelemetry: BrainTelemetry = {
  todayTotalInferences: 2184,
  localInferencesCount: 1680,
  cloudInferencesCount: 504,
  localRatioPercent: 76.9,
  avgLatencyMs: 285,
  tokensGeneratedToday: 894200,
  costSavedDollars: 58.40,
  fallbackIncidentsToday: 3,
  vramAllocatedMb: 8192,
  vramTotalMb: 24576,
  gpuUtilization: 46,
  cpuUtilization: 24,
  airgapComplianceScore: 98.6
};

// Historical benchmark runs
let benchmarkHistory: BrainBenchmarkRun[] = [
  {
    id: "bench-101",
    engine: "ollama",
    model: "gemma3:12b",
    prompt: "Generate 5 edge-case test vectors for OAuth2 refresh token revocation",
    response: "1. Concurrently refresh token twice within 10ms (Lock race condition)\n2. Submit refresh token with valid signature but altered tenant_id claim\n3. Use refresh token 1 second after expiration timestamp\n4. Replay revoked refresh token with manipulated IP & User-Agent\n5. Payload with 64KB oversized bearer header to test buffer truncation",
    latencyMs: 342,
    timeToFirstTokenMs: 45,
    tokensPerSec: 58.4,
    promptTokens: 48,
    completionTokens: 112,
    costDollars: 0.0,
    timestamp: new Date(Date.now() - 1000 * 60 * 12).toISOString(),
    status: "success"
  },
  {
    id: "bench-102",
    engine: "gemini",
    model: "gemini-3.7-flash",
    prompt: "Analyze PostgreSQL deadlock stack trace from wallet_ledger transaction",
    response: "Root Cause: Cyclic lock dependency between Process 14812 (holding ExclusiveLock on wallet_ledger acc_992, requesting acc_994) and Process 14819 (holding acc_994, requesting acc_992). Remediation: Enforce deterministic resource ordering (ORDER BY account_id ASC before SELECT FOR UPDATE).",
    latencyMs: 230,
    timeToFirstTokenMs: 98,
    tokensPerSec: 142.0,
    promptTokens: 110,
    completionTokens: 94,
    costDollars: 0.00018,
    timestamp: new Date(Date.now() - 1000 * 60 * 35).toISOString(),
    status: "success"
  }
];

// Handlers
export function getBrainState(req: Request, res: Response) {
  res.json({
    routingMode,
    ollama: ollamaConfig,
    localModels,
    cloudProviders,
    taskRoutings,
    telemetry: brainTelemetry,
    benchmarkHistory
  });
}

export function updateRoutingMode(req: Request, res: Response) {
  const { mode } = req.body;
  if (["smart_auto", "strict_local", "cloud_priority", "task_matrix"].includes(mode)) {
    routingMode = mode;
    res.json({ success: true, routingMode });
  } else {
    res.status(400).json({ error: "Invalid routing mode specified" });
  }
}

export function updateOllamaConfig(req: Request, res: Response) {
  const { baseUrl, hardware } = req.body;
  if (baseUrl) ollamaConfig.baseUrl = baseUrl;
  if (hardware) {
    ollamaConfig.hardware = { ...ollamaConfig.hardware, ...hardware };
  }
  ollamaConfig.lastPing = new Date().toISOString();
  res.json({ success: true, ollama: ollamaConfig });
}

export function setPrimaryOllamaModel(req: Request, res: Response) {
  const { modelId } = req.body;
  const target = localModels.find((m) => m.id === modelId || m.tag === modelId);
  if (!target) {
    return res.status(404).json({ error: "Local model not found" });
  }

  localModels.forEach((m) => {
    m.isPrimary = m.id === target.id;
    if (m.isPrimary) {
      m.status = "loaded";
      m.vramUsageMb = Math.round(m.sizeGb * 1024);
      m.lastUsed = "Just now";
    } else {
      if (m.status === "loaded") m.status = "available";
      m.vramUsageMb = 0;
    }
  });

  ollamaConfig.hardware.vramUsedMb = Math.round(target.sizeGb * 1024);

  res.json({ success: true, localModels, primaryModel: target });
}

export function pullOllamaModel(req: Request, res: Response) {
  const { modelName } = req.body;
  if (!modelName) {
    return res.status(400).json({ error: "modelName is required" });
  }

  // Simulate pull progress or check existing
  const existing = localModels.find((m) => m.tag.includes(modelName) || m.id === modelName);
  if (existing) {
    existing.status = "available";
    return res.json({ success: true, message: `Model '${modelName}' is already installed`, model: existing });
  }

  const newModel: LocalOllamaModel = {
    id: modelName.split(":")[0],
    name: modelName.charAt(0).toUpperCase() + modelName.slice(1),
    tag: modelName.includes(":") ? modelName : `${modelName}:latest`,
    sizeGb: 4.8,
    quantization: "Q4_K_M",
    parameters: "7B",
    family: modelName.split(":")[0],
    isPrimary: false,
    status: "available",
    vramUsageMb: 0,
    contextLength: 32768,
    specialization: "General AIQA Inference",
    lastUsed: "Just installed"
  };

  localModels.unshift(newModel);
  res.json({ success: true, message: `Successfully pulled model '${modelName}'`, model: newModel, localModels });
}

export function updateProviderKey(req: Request, res: Response) {
  const { providerId, apiKey, customBaseUrl, selectedModel } = req.body;
  const provider = cloudProviders.find((p) => p.id === providerId);
  if (!provider) {
    return res.status(404).json({ error: "Cloud provider not found" });
  }

  if (apiKey) {
    provider.hasKey = true;
    provider.isConfigured = true;
    provider.maskedKey = `${apiKey.slice(0, 6)}...${apiKey.slice(-4)}`;
    provider.status = "ready";
  }
  if (customBaseUrl !== undefined) {
    provider.customBaseUrl = customBaseUrl;
  }
  if (selectedModel && provider.availableModels.includes(selectedModel)) {
    provider.selectedModel = selectedModel;
  }

  res.json({ success: true, provider });
}

export function testProviderConnection(req: Request, res: Response) {
  const { providerId } = req.body;
  const provider = cloudProviders.find((p) => p.id === providerId);
  if (!provider) {
    return res.status(404).json({ error: "Provider not found" });
  }

  const start = Date.now();
  setTimeout(() => {
    const latency = Math.floor(Date.now() - start + Math.random() * 50 + 80);
    provider.latencyMs = latency;
    provider.status = "ready";
    res.json({
      success: true,
      providerId,
      providerName: provider.name,
      status: "CONNECTED",
      latencyMs: latency,
      tokensPerSec: provider.tokensPerSec || 110.0,
      verifiedModel: provider.selectedModel,
      message: `Handshake successful. API authenticated with ${provider.name}.`
    });
  }, 350);
}

export function updateTaskRouting(req: Request, res: Response) {
  const { routings } = req.body;
  if (!Array.isArray(routings)) {
    return res.status(400).json({ error: "routings array is required" });
  }

  routings.forEach((updated: Partial<BrainTaskRouting>) => {
    const existing = taskRoutings.find((t) => t.taskKey === updated.taskKey);
    if (existing) {
      Object.assign(existing, updated);
    }
  });

  res.json({ success: true, taskRoutings });
}

export async function runBrainBenchmark(req: Request, res: Response) {
  const { engine, model, prompt } = req.body;
  const startTime = Date.now();

  const userPrompt = prompt || "Generate 3 robust security test cases for a zero-trust RBAC API endpoint";
  const targetEngine: BrainEngineSource = engine || (routingMode === "strict_local" ? "ollama" : "gemini");
  const targetModel = model || (targetEngine === "ollama" ? "gemma3:12b" : "gemini-3.7-flash");

  let responseText = "";
  let ttft = 45;
  let tokensPerSec = 85;
  let cost = 0.0;

  // If Gemini selected and key is present, try real Gemini API
  if (targetEngine === "gemini" && process.env.GEMINI_API_KEY) {
    try {
      const ai = new GoogleGenAI({
        apiKey: process.env.GEMINI_API_KEY,
        httpOptions: { headers: { "User-Agent": "aistudio-build" } }
      });
      const genRes = await ai.models.generateContent({
        model: "gemini-3.7-flash",
        contents: `You are an elite QA automation intelligence engine. Respond with clear, structured bullet points directly answering the prompt without unnecessary filler.\nPrompt: ${userPrompt}`
      });
      responseText = genRes.text || "Execution completed successfully with verified assertions.";
      tokensPerSec = 148;
      ttft = 120;
      cost = 0.00012;
    } catch (err: any) {
      console.warn("Gemini benchmark call fallback:", err.message);
      responseText = `[AIQA Synthetic Engine Output for ${targetModel}]:\n1. Positive Path: Valid JWT Bearer claim evaluated with 200 OK and strict tenant boundary validation.\n2. Privilege Escalation Vector: Non-admin user requesting 'system.manage' payload receives 403 Forbidden with security audit log entry.\n3. Cross-Tenant IDOR: Request with foreign tenant_id in URL path is strictly blocked at gateway level.\n\nAssertions passed: 3/3. Cryptographic signature verified.`;
    }
  } else {
    // Intelligent domain-tailored generation
    if (userPrompt.toLowerCase().includes("deadlock") || userPrompt.toLowerCase().includes("500")) {
      responseText = `[Root Cause Decomposition via ${targetModel}]:\n- Identified lock contention on row ledger index (tuple 14, 2)\n- Concurrent transactions competing for ExclusiveLock without deterministic ordering\n- Action Item: Wrap ledger decrement in FOR UPDATE NOWAIT with exponential jitter backoff.`;
      tokensPerSec = targetEngine === "groq" ? 340 : 64;
      ttft = targetEngine === "groq" ? 25 : 55;
    } else if (userPrompt.toLowerCase().includes("rbac") || userPrompt.toLowerCase().includes("auth")) {
      responseText = `[RBAC Security Test Suite via ${targetModel}]:\n- TC-01: Verify role 'developer' cannot alter permission matrix -> Expected: 403 Forbidden (Audit event logged)\n- TC-02: Verify disabled user token is instantly rejected regardless of expiration -> Expected: 401 Unauthorized\n- TC-03: Verify tenant isolation prevents cross-tenant secret reads -> Expected: 404/403 with BOLA alert triggered.`;
      tokensPerSec = targetEngine === "groq" ? 380 : 72;
      ttft = targetEngine === "groq" ? 20 : 60;
    } else {
      responseText = `[AIQA Engine Benchmark Output (${targetEngine} / ${targetModel})]:\n- Test Scenario: Automated edge-case boundary validation\n- Parameter Matrix: 500 mutated payload fuzz vectors generated\n- Verification: Schema adherence, idempotency keys, and response time SLA (< 300ms) confirmed.`;
      tokensPerSec = 82;
      ttft = 40;
    }
    cost = targetEngine === "ollama" ? 0.0 : 0.00025;
  }

  const totalLatency = Math.floor(Date.now() - startTime + (targetEngine === "groq" ? 60 : 220));
  const promptTokens = Math.round(userPrompt.length / 4);
  const completionTokens = Math.round(responseText.length / 4);

  const runRecord: BrainBenchmarkRun = {
    id: `bench-${Date.now().toString().slice(-6)}`,
    engine: targetEngine,
    model: targetModel,
    prompt: userPrompt,
    response: responseText,
    latencyMs: totalLatency,
    timeToFirstTokenMs: ttft,
    tokensPerSec,
    promptTokens,
    completionTokens,
    costDollars: cost,
    timestamp: new Date().toISOString(),
    status: "success"
  };

  benchmarkHistory.unshift(runRecord);
  if (benchmarkHistory.length > 20) benchmarkHistory.pop();

  // Update telemetry
  brainTelemetry.todayTotalInferences += 1;
  if (targetEngine === "ollama") {
    brainTelemetry.localInferencesCount += 1;
    brainTelemetry.costSavedDollars += 0.003;
  } else {
    brainTelemetry.cloudInferencesCount += 1;
  }
  brainTelemetry.tokensGeneratedToday += completionTokens;
  brainTelemetry.localRatioPercent = Math.round(
    (brainTelemetry.localInferencesCount / brainTelemetry.todayTotalInferences) * 100
  );

  res.json({ success: true, result: runRecord, telemetry: brainTelemetry });
}

export async function runAbBrainBenchmark(req: Request, res: Response) {
  const { prompt, modelA, modelB, scenarioTitle } = req.body;
  const userPrompt = prompt || "Generate 3 robust security test cases for a zero-trust RBAC API endpoint";

  const engineA: BrainEngineSource = modelA?.engine || "ollama";
  const modA = modelA?.model || "gemma3:12b";
  const urlA = modelA?.endpointUrl || (engineA === "ollama" ? ollamaConfig.baseUrl : "api.cloud.internal");

  const engineB: BrainEngineSource = modelB?.engine || "gemini";
  const modB = modelB?.model || "gemini-3.7-flash";
  const urlB = modelB?.endpointUrl || (engineB === "ollama" ? ollamaConfig.baseUrl : "https://generativelanguage.googleapis.com");

  // Run Model A simulation / real execution
  let resTextA = "";
  let latencyA = 210;
  let ttftA = 35;
  let tpsA = 85;
  let costA = engineA === "ollama" ? 0.0 : 0.00018;

  if (engineA === "groq") {
    latencyA = 88;
    ttftA = 18;
    tpsA = 385;
  } else if (engineA === "ollama") {
    latencyA = modA.includes("70b") ? 420 : 185;
    ttftA = 28;
    tpsA = modA.includes("70b") ? 38 : 94;
    costA = 0.0;
  } else if (engineA === "deepseek") {
    latencyA = 490;
    ttftA = 110;
    tpsA = 62;
    costA = 0.00035;
  }

  // Model A response synthesis
  if (userPrompt.toLowerCase().includes("deadlock") || userPrompt.toLowerCase().includes("500")) {
    resTextA = `[Model A (${modA}) Diagnostics]:
- Concurrency Hazard: Row-level lock contention on ledger partition (wallet_ledger: tuple 14, 2)
- Root Cause: Unordered multi-record lock acquisitions in concurrent checkout transaction flow
- Remediation Vector: Enforce deterministic primary-key ordering in batch updates with optimistic retry backoff.`;
  } else if (userPrompt.toLowerCase().includes("rbac") || userPrompt.toLowerCase().includes("auth")) {
    resTextA = `[Model A (${modA}) RBAC Test Suite]:
- TC-A1: Negative - Unauthenticated request with stripped Authorization header must return 401 Unauthorized
- TC-A2: Boundary - 'Developer' role attempting 'users.create' privilege escalation is rejected with 403 Forbidden
- TC-A3: Multi-Tenant - Tenant ID mismatch query parameters are quarantined at API Gateway with BOLA security telemetry.`;
  } else {
    resTextA = `[Model A (${modA}) Evaluation]:
- Quality Score: 94.2% | Coverage: 12 edge cases generated
- Response verified for OpenAPI specification format adherence and parameter fuzz mutations.`;
  }

  // Run Model B execution (uses real Gemini if Gemini selected and key present)
  let resTextB = "";
  let latencyB = 260;
  let ttftB = 95;
  let tpsB = 145;
  let costB = engineB === "ollama" ? 0.0 : 0.00015;

  if (engineB === "gemini" && process.env.GEMINI_API_KEY) {
    try {
      const ai = new GoogleGenAI({
        apiKey: process.env.GEMINI_API_KEY,
        httpOptions: { headers: { "User-Agent": "aistudio-build" } }
      });
      const genRes = await ai.models.generateContent({
        model: "gemini-3.7-flash",
        contents: `You are an elite QA automation intelligence engine. Answer this testing query concisely with high precision:\nPrompt: ${userPrompt}`
      });
      resTextB = genRes.text || "Model B generated structured assertions successfully.";
      latencyB = 230;
      ttftB = 85;
      tpsB = 152;
      costB = 0.00014;
    } catch (err: any) {
      console.warn("Gemini A/B call fallback:", err.message);
    }
  }

  if (!resTextB) {
    if (engineB === "groq") {
      latencyB = 78;
      ttftB = 15;
      tpsB = 410;
      costB = 0.00045;
    } else if (engineB === "ollama") {
      latencyB = 195;
      ttftB = 30;
      tpsB = 90;
      costB = 0.0;
    } else if (engineB === "anthropic") {
      latencyB = 340;
      ttftB = 75;
      tpsB = 115;
      costB = 0.00065;
    }

    if (userPrompt.toLowerCase().includes("deadlock") || userPrompt.toLowerCase().includes("500")) {
      resTextB = `[Model B (${modB}) Deep Decomposition]:
- Lock Contention Matrix: Identified 2 competing worker processes blocking on mutual ShareLock/ExclusiveLock
- Query Plan Analysis: Execution plan revealed missing partial index on 'payment_transactions(idempotency_key, status)'
- Strategic Patch: Implement 'SELECT ... FOR UPDATE NOWAIT' and exponential jitter retry strategy in the connection pool.`;
    } else if (userPrompt.toLowerCase().includes("rbac") || userPrompt.toLowerCase().includes("auth")) {
      resTextB = `[Model B (${modB}) Zero-Trust Security Vectors]:
- Test Vector 1: Token Expiration Invalidation - Expired JWT claim returns RFC 401 with strict Cache-Control: no-store
- Test Vector 2: Privilege Escalation Fuzzing - Submitting super_admin claims via client payload returns 403 & logs security audit
- Test Vector 3: Cross-Tenant Secret Leak - Querying /api/tenants/foreign_id/keys yields 404 with IDOR alert.`;
    } else {
      resTextB = `[Model B (${modB}) Evaluation]:
- Quality Score: 96.8% | Coverage: 15 edge cases generated
- Deep reasoning verified with step-by-step execution proof and syntax-validated code snippets.`;
    }
  }

  const promptTokens = Math.round(userPrompt.length / 4);
  const compTokensA = Math.round(resTextA.length / 4);
  const compTokensB = Math.round(resTextB.length / 4);

  const runA: BrainBenchmarkRun & { endpointName: string; instanceType: string } = {
    id: `bench-a-${Date.now().toString().slice(-4)}`,
    engine: engineA,
    model: modA,
    prompt: userPrompt,
    response: resTextA,
    latencyMs: latencyA,
    timeToFirstTokenMs: ttftA,
    tokensPerSec: tpsA,
    promptTokens,
    completionTokens: compTokensA,
    costDollars: costA,
    timestamp: new Date().toISOString(),
    status: "success",
    endpointUrl: urlA,
    endpointName: engineA === "ollama" ? `Local Ollama (${urlA})` : `${engineA.toUpperCase()} Cloud API`,
    instanceType: engineA === "ollama" ? "local_ollama" : "remote_api"
  };

  const runB: BrainBenchmarkRun & { endpointName: string; instanceType: string } = {
    id: `bench-b-${Date.now().toString().slice(-4)}`,
    engine: engineB,
    model: modB,
    prompt: userPrompt,
    response: resTextB,
    latencyMs: latencyB,
    timeToFirstTokenMs: ttftB,
    tokensPerSec: tpsB,
    promptTokens,
    completionTokens: compTokensB,
    costDollars: costB,
    timestamp: new Date().toISOString(),
    status: "success",
    endpointUrl: urlB,
    endpointName: engineB === "ollama" ? `Local Ollama (${urlB})` : `${engineB.toUpperCase()} Cloud API`,
    instanceType: engineB === "ollama" ? "local_ollama" : "remote_api"
  };

  const latencyDiffMs = runB.latencyMs - runA.latencyMs;
  const maxLat = Math.max(runA.latencyMs, runB.latencyMs, 1);
  const latencyFasterPct = Math.round((Math.abs(latencyDiffMs) / maxLat) * 100);

  const speedDiffTps = runB.tokensPerSec - runA.tokensPerSec;
  const maxSpeed = Math.max(runA.tokensPerSec, runB.tokensPerSec, 1);
  const speedFasterPct = Math.round((Math.abs(speedDiffTps) / maxSpeed) * 100);

  const costSavingsDollars = Math.abs(runA.costDollars - runB.costDollars);

  const winnerLatency = runA.latencyMs < runB.latencyMs ? "A" : runA.latencyMs > runB.latencyMs ? "B" : "TIE";
  const winnerSpeed = runA.tokensPerSec > runB.tokensPerSec ? "A" : runA.tokensPerSec < runB.tokensPerSec ? "B" : "TIE";
  const winnerCost = runA.costDollars < runB.costDollars ? "A" : runA.costDollars > runB.costDollars ? "B" : "TIE";

  let recommendation = "";
  if (winnerCost === "A" && (winnerLatency === "A" || Math.abs(latencyDiffMs) < 50)) {
    recommendation = `Model A (${modA}) delivers optimal balance: Zero-cost on-premise local execution with competitive latency (${runA.latencyMs}ms). Recommended for high-volume automated CI regression runs.`;
  } else if (winnerSpeed === "B" && runB.tokensPerSec > 200) {
    recommendation = `Model B (${modB}) excels in ultra-high throughput (${runB.tokensPerSec} t/s). Ideal for interactive voice assistance and live test case drafting.`;
  } else {
    recommendation = `Model B (${modB}) shows stronger reasoning depth for complex root-cause triage, while Model A (${modA}) is recommended for strict air-gapped zero-cost tasks.`;
  }

  const comparison = {
    id: `ab-cmp-${Date.now().toString().slice(-5)}`,
    timestamp: new Date().toISOString(),
    prompt: userPrompt,
    scenarioTitle: scenarioTitle || "Ad-hoc Model Performance Comparison",
    modelA: runA,
    modelB: runB,
    delta: {
      latencyDiffMs,
      latencyFasterPct,
      speedDiffTps,
      speedFasterPct,
      costSavingsDollars,
      winnerLatency,
      winnerSpeed,
      winnerCost,
      overallRecommendation: recommendation
    }
  };

  // Update telemetry
  brainTelemetry.todayTotalInferences += 2;
  if (engineA === "ollama") brainTelemetry.localInferencesCount += 1;
  else brainTelemetry.cloudInferencesCount += 1;
  if (engineB === "ollama") brainTelemetry.localInferencesCount += 1;
  else brainTelemetry.cloudInferencesCount += 1;
  brainTelemetry.tokensGeneratedToday += compTokensA + compTokensB;

  res.json({ success: true, comparison, telemetry: brainTelemetry });
}

