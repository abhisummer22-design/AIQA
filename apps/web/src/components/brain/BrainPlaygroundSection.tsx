import React, { useState } from "react";
import {
  Play,
  Zap,
  Clock,
  Coins,
  Cpu,
  Layers,
  Sparkles,
  CheckCircle2,
  AlertCircle,
  Copy,
  RotateCcw,
  Gauge,
  Sliders,
  ArrowRightLeft,
  Trophy,
  TrendingUp,
  Award,
  Globe,
  Server,
  FileSpreadsheet,
  Download,
  Share2,
  Check,
  SplitSquareVertical,
  Scale
} from "lucide-react";
import {
  BrainBenchmarkRun,
  LocalOllamaModel,
  CloudApiProvider,
  ModelEndpointItem,
  AbBenchmarkComparison
} from "../../types";
import { ModelEndpointSearchDropdown } from "./ModelEndpointSearchDropdown";
import { runAbBrainBenchmark } from "../../services/api";

interface BrainPlaygroundSectionProps {
  localModels: LocalOllamaModel[];
  cloudProviders: CloudApiProvider[];
  history: BrainBenchmarkRun[];
  ollamaBaseUrl?: string;
  onRunBenchmark: (payload: { engine: string; model: string; prompt: string }) => Promise<any>;
}

export const BrainPlaygroundSection: React.FC<BrainPlaygroundSectionProps> = ({
  localModels,
  cloudProviders,
  history,
  ollamaBaseUrl = "http://localhost:11434",
  onRunBenchmark
}) => {
  // Benchmark Mode: "ab_test" (Default) or "single"
  const [benchMode, setBenchMode] = useState<"ab_test" | "single">("ab_test");

  // Model A and Model B selection
  const [modelA, setModelA] = useState<ModelEndpointItem>({
    id: "ollama-gemma3:12b",
    name: "Gemma 3 12B Instruct",
    engine: "ollama",
    modelTag: "gemma3:12b",
    endpointUrl: ollamaBaseUrl,
    instanceType: "local_ollama",
    instanceLabel: `Local Ollama (${ollamaBaseUrl})`,
    parameters: "12B",
    quantization: "Q4_K_M",
    contextWindow: "32k tokens",
    latencyMs: 185,
    tokensPerSec: 94,
    costPer1k: "$0.0000 (Free Local)",
    status: "loaded",
    isPrimary: true,
    specialization: "General QA, Test Validation, Code Analysis",
    providerName: "Ollama CUDA Engine"
  });

  const [modelB, setModelB] = useState<ModelEndpointItem>({
    id: "gemini-gemini-3.7-flash",
    name: "Google Gemini AI - gemini-3.7-flash",
    engine: "gemini",
    modelTag: "gemini-3.7-flash",
    endpointUrl: "generativelanguage.googleapis.com",
    instanceType: "remote_api",
    instanceLabel: "Google Cloud / AI Studio",
    parameters: "Pro/Flash",
    quantization: "FP16 / Server",
    contextWindow: "1,048,576 tokens",
    latencyMs: 230,
    tokensPerSec: 152,
    costPer1k: "$0.00015 / 1k",
    status: "ready",
    isPrimary: false,
    specialization: "Multi-modal ultra-low latency inference with 1M+ context",
    providerName: "Google Gemini AI"
  });

  // Single Model selection
  const [singleModel, setSingleModel] = useState<ModelEndpointItem>(modelA);

  const [prompt, setPrompt] = useState<string>(
    "Generate 3 robust security test cases for a zero-trust RBAC API endpoint evaluating privilege escalation and multi-tenant isolation"
  );
  const [isRunning, setIsRunning] = useState(false);
  const [abComparison, setAbComparison] = useState<AbBenchmarkComparison | null>(null);
  const [singleResult, setSingleResult] = useState<BrainBenchmarkRun | null>(
    history.length > 0 ? history[0] : null
  );
  const [abHistory, setAbHistory] = useState<AbBenchmarkComparison[]>([]);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const handleSwapModels = () => {
    const temp = modelA;
    setModelA(modelB);
    setModelB(temp);
  };

  const handleRunAbTest = async () => {
    if (!prompt.trim() || isRunning) return;
    setIsRunning(true);
    try {
      const res = await runAbBrainBenchmark({
        prompt: prompt.trim(),
        modelA: { engine: modelA.engine, model: modelA.modelTag, endpointUrl: modelA.endpointUrl },
        modelB: { engine: modelB.engine, model: modelB.modelTag, endpointUrl: modelB.endpointUrl },
        scenarioTitle: "A/B Performance & Reasoning Comparison"
      });

      if (res.comparison) {
        setAbComparison(res.comparison);
        setAbHistory((prev) => [res.comparison, ...prev.slice(0, 9)]);
      }
    } catch (err) {
      console.error("A/B Benchmark error:", err);
    } finally {
      setIsRunning(false);
    }
  };

  const handleRunSingle = async () => {
    if (!prompt.trim() || isRunning) return;
    setIsRunning(true);
    try {
      const res = await onRunBenchmark({
        engine: singleModel.engine,
        model: singleModel.modelTag,
        prompt: prompt.trim()
      });
      if (res.result) {
        setSingleResult(res.result);
      }
    } catch (err) {
      console.error("Single benchmark error:", err);
    } finally {
      setIsRunning(false);
    }
  };

  const copyToClipboard = (text: string, key: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(key);
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const promptPresets = [
    {
      title: "Zero-Trust RBAC Fuzzing",
      text: "Generate 3 robust security test cases for a zero-trust RBAC API endpoint evaluating privilege escalation and multi-tenant isolation"
    },
    {
      title: "PostgreSQL Deadlock Triage",
      text: "Analyze PostgreSQL deadlock stack trace from wallet_ledger transaction concurrent update and formulate immediate SQL index remediation"
    },
    {
      title: "Mobile Gestures Test Suite",
      text: "Draft Appium / Playwright mobile test script verifying pinch-to-zoom map gesture with orientation change on iPad Air"
    },
    {
      title: "OAuth2 Token Race Condition",
      text: "Generate 5 edge-case test vectors for OAuth2 refresh token revocation under extreme concurrent network jitter"
    },
    {
      title: "RAG Grounding & Hallucination SLA",
      text: "Evaluate customer support SLA retrieval chunks and verify faithfulness score against generated enterprise policy summary"
    }
  ];

  const abPresets = [
    {
      label: "Local Gemma 3 12B vs Gemini 3.7 Flash",
      desc: "Air-Gapped Local CUDA vs Google Cloud Frontier",
      setup: () => {
        const local = localModels.find((m) => m.id.includes("gemma3")) || localModels[0];
        if (local) {
          setModelA({
            id: `ollama-${local.id}`,
            name: local.name,
            engine: "ollama",
            modelTag: local.id,
            endpointUrl: ollamaBaseUrl,
            instanceType: "local_ollama",
            instanceLabel: `Local Ollama (${ollamaBaseUrl})`,
            parameters: local.parameters,
            quantization: local.quantization,
            contextWindow: `${Math.round(local.contextLength / 1024)}k tokens`,
            latencyMs: 185,
            tokensPerSec: 94,
            costPer1k: "$0.0000 (Free Local)",
            status: "loaded",
            isPrimary: true,
            specialization: local.specialization,
            providerName: "Ollama CUDA Engine"
          });
        }
        setModelB({
          id: "gemini-gemini-3.7-flash",
          name: "Google Gemini AI - gemini-3.7-flash",
          engine: "gemini",
          modelTag: "gemini-3.7-flash",
          endpointUrl: "generativelanguage.googleapis.com",
          instanceType: "remote_api",
          instanceLabel: "Google Cloud / AI Studio",
          parameters: "Pro/Flash",
          quantization: "FP16 / Server",
          contextWindow: "1,048,576 tokens",
          latencyMs: 230,
          tokensPerSec: 152,
          costPer1k: "$0.00015 / 1k",
          status: "ready",
          isPrimary: false,
          specialization: "Ultra-low latency inference",
          providerName: "Google Gemini AI"
        });
      }
    },
    {
      label: "Local Qwen 2.5 Coder 14B vs Groq LPU 70B",
      desc: "On-Premises Test Gen vs 380 t/s LPU Inference",
      setup: () => {
        const local = localModels.find((m) => m.id.includes("qwen")) || localModels[0];
        if (local) {
          setModelA({
            id: `ollama-${local.id}`,
            name: local.name,
            engine: "ollama",
            modelTag: local.id,
            endpointUrl: ollamaBaseUrl,
            instanceType: "local_ollama",
            instanceLabel: `Local Ollama (${ollamaBaseUrl})`,
            parameters: local.parameters,
            quantization: local.quantization,
            contextWindow: `${Math.round(local.contextLength / 1024)}k tokens`,
            latencyMs: 195,
            tokensPerSec: 88,
            costPer1k: "$0.0000 (Free Local)",
            status: "available",
            isPrimary: false,
            specialization: local.specialization,
            providerName: "Ollama CUDA Engine"
          });
        }
        setModelB({
          id: "groq-llama-3.3-70b-versatile",
          name: "Groq LPU Accelerator - llama-3.3-70b-versatile",
          engine: "groq",
          modelTag: "llama-3.3-70b-versatile",
          endpointUrl: "api.groq.com/openai/v1",
          instanceType: "remote_api",
          instanceLabel: "Groq Cloud LPU",
          parameters: "70B / LPU",
          quantization: "FP16 / Server",
          contextWindow: "32,768 tokens",
          latencyMs: 78,
          tokensPerSec: 410,
          costPer1k: "$0.00059 / 1k",
          status: "ready",
          isPrimary: false,
          specialization: "Ultra High TPS",
          providerName: "Groq LPU Accelerator"
        });
      }
    },
    {
      label: "Local DeepSeek R1 8B vs OpenAI GPT-4o",
      desc: "Local Distilled Reasoning vs Cloud Flagship",
      setup: () => {
        const local = localModels.find((m) => m.id.includes("deepseek")) || localModels[0];
        if (local) {
          setModelA({
            id: `ollama-${local.id}`,
            name: local.name,
            engine: "ollama",
            modelTag: local.id,
            endpointUrl: ollamaBaseUrl,
            instanceType: "local_ollama",
            instanceLabel: `Local Ollama (${ollamaBaseUrl})`,
            parameters: local.parameters,
            quantization: local.quantization,
            contextWindow: `${Math.round(local.contextLength / 1024)}k tokens`,
            latencyMs: 240,
            tokensPerSec: 62,
            costPer1k: "$0.0000 (Free Local)",
            status: "available",
            isPrimary: false,
            specialization: local.specialization,
            providerName: "Ollama CUDA Engine"
          });
        }
        setModelB({
          id: "openai-gpt-4o",
          name: "OpenAI GPT-4o - gpt-4o",
          engine: "openai",
          modelTag: "gpt-4o",
          endpointUrl: "api.openai.com/v1",
          instanceType: "remote_api",
          instanceLabel: "OpenAI Platform",
          parameters: "High Reasoning",
          quantization: "FP16 / Server",
          contextWindow: "128,000 tokens",
          latencyMs: 310,
          tokensPerSec: 95,
          costPer1k: "$0.0050 / 1k",
          status: "ready",
          isPrimary: false,
          specialization: "Flagship intelligence",
          providerName: "OpenAI GPT-4o"
        });
      }
    }
  ];

  return (
    <div className="space-y-6">
      {/* Top Banner & A/B Mode Header */}
      <div className="p-5 rounded-2xl bg-gradient-to-r from-indigo-950/40 via-zinc-900/90 to-cyan-950/30 border border-indigo-500/30 shadow-xl backdrop-blur-md">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
          <div className="space-y-1">
            <div className="flex items-center gap-2.5">
              <div className="w-9 h-9 rounded-xl bg-indigo-500/20 border border-indigo-500/40 flex items-center justify-center text-indigo-400">
                <Scale className="w-5 h-5" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-bold text-white font-mono">
                    Model Performance &amp; A/B Inference Bench
                  </h2>
                  <span className="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-indigo-500/20 text-indigo-300 border border-indigo-500/40">
                    SEARCHABLE MULTI-ENDPOINT ROUTER
                  </span>
                </div>
                <p className="text-xs text-slate-400 font-mono mt-0.5">
                  Seamlessly search and switch between multiple local Ollama daemons, LAN nodes, or remote API endpoints for rapid head-to-head evaluation
                </p>
              </div>
            </div>
          </div>

          {/* Mode Switcher Pills */}
          <div className="flex items-center gap-2 bg-zinc-950 p-1.5 rounded-xl border border-zinc-800 self-start lg:self-auto">
            <button
              type="button"
              onClick={() => setBenchMode("ab_test")}
              className={`flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-xs font-mono font-bold transition-all ${
                benchMode === "ab_test"
                  ? "bg-indigo-600 text-white shadow-md shadow-indigo-600/30"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              <SplitSquareVertical className="w-3.5 h-3.5" />
              <span>A/B Comparison (Dual Test)</span>
            </button>

            <button
              type="button"
              onClick={() => setBenchMode("single")}
              className={`flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-xs font-mono font-bold transition-all ${
                benchMode === "single"
                  ? "bg-indigo-600 text-white shadow-md shadow-indigo-600/30"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              <Zap className="w-3.5 h-3.5" />
              <span>Single Model Deep Probe</span>
            </button>
          </div>
        </div>
      </div>

      {/* ========================================================================= */}
      {/* --- A/B TESTING WORKBENCH (DUAL SEARCHABLE MODEL & ENDPOINT TEST BENCH) --- */}
      {/* ========================================================================= */}
      {benchMode === "ab_test" && (
        <div className="space-y-6">
          {/* Quick Comparison Preset Ribbons */}
          <div className="flex flex-wrap items-center gap-2.5">
            <span className="text-[11px] font-mono font-bold text-slate-400 uppercase flex items-center gap-1.5">
              <Sparkles className="w-3.5 h-3.5 text-amber-400" />
              <span>Quick A/B Pairings:</span>
            </span>
            {abPresets.map((preset) => (
              <button
                key={preset.label}
                type="button"
                onClick={preset.setup}
                className="px-3 py-1.5 rounded-xl bg-zinc-900/80 hover:bg-zinc-800 border border-zinc-800 hover:border-indigo-500/50 text-xs font-mono text-slate-300 hover:text-white transition-all flex items-center gap-2 shadow-sm"
              >
                <span className="font-bold text-indigo-300">{preset.label}</span>
                <span className="text-[10px] text-slate-500 hidden sm:inline">({preset.desc})</span>
              </button>
            ))}
          </div>

          {/* Model A vs Model B Searchable Dropdown Selector Card */}
          <div className="p-5 rounded-2xl bg-zinc-900/80 border border-zinc-800 shadow-xl backdrop-blur-md space-y-4">
            <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
              <div className="flex items-center gap-2">
                <Sliders className="w-4 h-4 text-indigo-400" />
                <span className="text-xs font-bold font-mono text-white uppercase tracking-wider">
                  Target Model &amp; Endpoint Instance Configuration
                </span>
              </div>
              <button
                type="button"
                onClick={handleSwapModels}
                className="px-3 py-1 rounded-lg bg-zinc-800 hover:bg-indigo-600 text-slate-300 hover:text-white text-xs font-mono font-bold transition-colors flex items-center gap-1.5"
                title="Swap Model A and Model B"
              >
                <ArrowRightLeft className="w-3.5 h-3.5" />
                <span>Swap A / B</span>
              </button>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
              {/* Model A Searchable Dropdown Column */}
              <div className="p-4 rounded-xl bg-indigo-950/20 border border-indigo-500/30 space-y-3">
                <ModelEndpointSearchDropdown
                  localModels={localModels}
                  cloudProviders={cloudProviders}
                  ollamaBaseUrl={ollamaBaseUrl}
                  selectedModelTag={modelA.modelTag}
                  selectedEngine={modelA.engine}
                  onSelect={(item) => setModelA(item)}
                  label="Model A (Baseline / Local Ollama Instance)"
                  variant="modelA"
                  placeholder="Search Model A (e.g. gemma3, qwen, llama3, localhost:11434)..."
                />

                <div className="grid grid-cols-3 gap-2 text-[10px] font-mono pt-1">
                  <div className="p-2 rounded-lg bg-zinc-950 border border-zinc-800 text-center">
                    <span className="text-slate-500 block">Latency</span>
                    <span className="font-bold text-emerald-400 mt-0.5 block">{modelA.latencyMs} ms</span>
                  </div>
                  <div className="p-2 rounded-lg bg-zinc-950 border border-zinc-800 text-center">
                    <span className="text-slate-500 block">Speed</span>
                    <span className="font-bold text-indigo-300 mt-0.5 block">{modelA.tokensPerSec} t/s</span>
                  </div>
                  <div className="p-2 rounded-lg bg-zinc-950 border border-zinc-800 text-center">
                    <span className="text-slate-500 block">Cost / 1k</span>
                    <span className="font-bold text-slate-300 mt-0.5 block truncate">{modelA.costPer1k}</span>
                  </div>
                </div>
              </div>

              {/* Model B Searchable Dropdown Column */}
              <div className="p-4 rounded-xl bg-cyan-950/20 border border-cyan-500/30 space-y-3">
                <ModelEndpointSearchDropdown
                  localModels={localModels}
                  cloudProviders={cloudProviders}
                  ollamaBaseUrl={ollamaBaseUrl}
                  selectedModelTag={modelB.modelTag}
                  selectedEngine={modelB.engine}
                  onSelect={(item) => setModelB(item)}
                  label="Model B (Challenger / Frontier Cloud Endpoint)"
                  variant="modelB"
                  placeholder="Search Model B (e.g. gemini-3.7, gpt-4o, groq, deepseek)..."
                />

                <div className="grid grid-cols-3 gap-2 text-[10px] font-mono pt-1">
                  <div className="p-2 rounded-lg bg-zinc-950 border border-zinc-800 text-center">
                    <span className="text-slate-500 block">Latency</span>
                    <span className="font-bold text-emerald-400 mt-0.5 block">{modelB.latencyMs} ms</span>
                  </div>
                  <div className="p-2 rounded-lg bg-zinc-950 border border-zinc-800 text-center">
                    <span className="text-slate-500 block">Speed</span>
                    <span className="font-bold text-cyan-300 mt-0.5 block">{modelB.tokensPerSec} t/s</span>
                  </div>
                  <div className="p-2 rounded-lg bg-zinc-950 border border-zinc-800 text-center">
                    <span className="text-slate-500 block">Cost / 1k</span>
                    <span className="font-bold text-slate-300 mt-0.5 block truncate">{modelB.costPer1k}</span>
                  </div>
                </div>
              </div>
            </div>

            {/* Prompt Input & Trigger Area */}
            <div className="pt-2 space-y-3">
              <div className="flex items-center justify-between text-[11px] font-mono font-bold text-slate-300">
                <span className="flex items-center gap-1.5">
                  <span>Benchmarking Prompt / QA Scenario Payload</span>
                </span>
                <span className="text-slate-500">{prompt.length} characters</span>
              </div>

              <textarea
                rows={3}
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                placeholder="Enter prompt to dispatch simultaneously to Model A and Model B..."
                className="w-full p-3 rounded-xl bg-zinc-950 border border-zinc-800 text-slate-200 text-xs font-mono focus:outline-none focus:border-indigo-500 resize-none"
              />

              {/* Quick Preset Selector Buttons */}
              <div className="flex flex-wrap gap-1.5 pt-1">
                {promptPresets.map((preset) => (
                  <button
                    key={preset.title}
                    type="button"
                    onClick={() => setPrompt(preset.text)}
                    className="px-2.5 py-1 rounded-lg bg-zinc-950 hover:bg-zinc-800 border border-zinc-800 text-[10px] font-mono text-slate-400 hover:text-slate-200 transition-colors"
                  >
                    {preset.title}
                  </button>
                ))}
              </div>

              {/* Execution Action Button */}
              <button
                type="button"
                onClick={handleRunAbTest}
                disabled={isRunning || !prompt.trim()}
                className={`w-full py-3.5 rounded-xl font-mono font-bold text-xs flex items-center justify-center gap-2 transition-all shadow-xl ${
                  isRunning || !prompt.trim()
                    ? "bg-zinc-800 text-slate-500 cursor-not-allowed"
                    : "bg-gradient-to-r from-indigo-600 via-indigo-500 to-cyan-600 hover:opacity-95 text-white shadow-indigo-950/60 active:scale-[0.99]"
                }`}
              >
                <Play className={`w-4 h-4 ${isRunning ? "animate-spin" : ""}`} />
                <span>
                  {isRunning
                    ? "Executing Parallel Neural A/B Benchmark (Model A & B)..."
                    : `Run Head-to-Head A/B Test (${modelA.modelTag} vs ${modelB.modelTag})`}
                </span>
              </button>
            </div>
          </div>

          {/* ========================================================= */}
          {/* --- A/B HEAD-TO-HEAD PERFORMANCE SCORECARD & DELTA --- */}
          {/* ========================================================= */}
          {abComparison && (
            <div className="space-y-6 animate-in fade-in zoom-in-95 duration-200">
              {/* Scorecard Hero Banner */}
              <div className="p-5 rounded-2xl bg-zinc-900/90 border border-zinc-700/80 shadow-2xl space-y-4 font-mono">
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-zinc-800">
                  <div className="flex items-center gap-2.5">
                    <div className="w-8 h-8 rounded-xl bg-amber-500/20 text-amber-300 border border-amber-500/40 flex items-center justify-center font-bold">
                      <Trophy className="w-4 h-4" />
                    </div>
                    <div>
                      <h3 className="text-sm font-bold text-white">
                        Head-to-Head Performance Scorecard
                      </h3>
                      <span className="text-[10px] text-slate-400">
                        {abComparison.modelA.model} vs {abComparison.modelB.model} • Verified at{" "}
                        {new Date(abComparison.timestamp).toLocaleTimeString()}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      onClick={() =>
                        copyToClipboard(
                          `### AIQA Model A/B Benchmark Report\n\n- Scenario: ${abComparison.scenarioTitle}\n- Model A: ${abComparison.modelA.model} (${abComparison.modelA.latencyMs}ms, ${abComparison.modelA.tokensPerSec} t/s, $${abComparison.modelA.costDollars.toFixed(5)})\n- Model B: ${abComparison.modelB.model} (${abComparison.modelB.latencyMs}ms, ${abComparison.modelB.tokensPerSec} t/s, $${abComparison.modelB.costDollars.toFixed(5)})\n- Latency Winner: Model ${abComparison.delta.winnerLatency} (${abComparison.delta.latencyFasterPct}% faster)\n- Throughput Winner: Model ${abComparison.delta.winnerSpeed} (${abComparison.delta.speedFasterPct}% speed delta)\n- Cost Winner: Model ${abComparison.delta.winnerCost} (Zero-cost on-prem vs Cloud)\n- Recommendation: ${abComparison.delta.overallRecommendation}`,
                          "report"
                        )
                      }
                      className="px-3 py-1.5 rounded-lg bg-zinc-950 hover:bg-zinc-800 border border-zinc-800 text-xs text-slate-300 hover:text-white flex items-center gap-1.5 transition-colors"
                    >
                      {copiedKey === "report" ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                      <span>{copiedKey === "report" ? "Report Copied" : "Copy Comparison Report"}</span>
                    </button>
                  </div>
                </div>

                {/* 4 Metric Comparative Pillars */}
                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3.5">
                  {/* Metric 1: Total Latency */}
                  <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/80 space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] text-slate-500 uppercase font-bold">Total Latency</span>
                      <span
                        className={`text-[9px] font-bold px-1.5 py-0.2 rounded border ${
                          abComparison.delta.winnerLatency === "A"
                            ? "bg-indigo-500/20 text-indigo-300 border-indigo-500/40"
                            : "bg-cyan-500/20 text-cyan-300 border-cyan-500/40"
                        }`}
                      >
                        WINNER: MODEL {abComparison.delta.winnerLatency}
                      </span>
                    </div>
                    <div className="flex items-baseline justify-between">
                      <div>
                        <span className="text-[10px] text-indigo-400 block">Model A</span>
                        <span className="text-sm font-bold text-white">{abComparison.modelA.latencyMs} ms</span>
                      </div>
                      <div className="text-right">
                        <span className="text-[10px] text-cyan-400 block">Model B</span>
                        <span className="text-sm font-bold text-white">{abComparison.modelB.latencyMs} ms</span>
                      </div>
                    </div>
                    <div className="text-[10px] text-slate-400 pt-1 border-t border-zinc-900">
                      Delta: <strong className="text-emerald-400">{Math.abs(abComparison.delta.latencyDiffMs)} ms</strong> ({abComparison.delta.latencyFasterPct}% difference)
                    </div>
                  </div>

                  {/* Metric 2: Generation Speed / TPS */}
                  <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/80 space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] text-slate-500 uppercase font-bold">Tokens / Sec (TPS)</span>
                      <span
                        className={`text-[9px] font-bold px-1.5 py-0.2 rounded border ${
                          abComparison.delta.winnerSpeed === "A"
                            ? "bg-indigo-500/20 text-indigo-300 border-indigo-500/40"
                            : "bg-cyan-500/20 text-cyan-300 border-cyan-500/40"
                        }`}
                      >
                        WINNER: MODEL {abComparison.delta.winnerSpeed}
                      </span>
                    </div>
                    <div className="flex items-baseline justify-between">
                      <div>
                        <span className="text-[10px] text-indigo-400 block">Model A</span>
                        <span className="text-sm font-bold text-white">{abComparison.modelA.tokensPerSec} t/s</span>
                      </div>
                      <div className="text-right">
                        <span className="text-[10px] text-cyan-400 block">Model B</span>
                        <span className="text-sm font-bold text-white">{abComparison.modelB.tokensPerSec} t/s</span>
                      </div>
                    </div>
                    <div className="text-[10px] text-slate-400 pt-1 border-t border-zinc-900">
                      Throughput Delta: <strong className="text-indigo-300">{Math.abs(abComparison.delta.speedDiffTps)} t/s</strong> ({abComparison.delta.speedFasterPct}% speed advantage)
                    </div>
                  </div>

                  {/* Metric 3: Time To First Token */}
                  <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/80 space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] text-slate-500 uppercase font-bold">Time To First Token</span>
                      <span className="text-[9px] font-bold px-1.5 py-0.2 rounded bg-zinc-900 text-slate-400 border border-zinc-800">
                        TTFT
                      </span>
                    </div>
                    <div className="flex items-baseline justify-between">
                      <div>
                        <span className="text-[10px] text-indigo-400 block">Model A</span>
                        <span className="text-sm font-bold text-white">{abComparison.modelA.timeToFirstTokenMs} ms</span>
                      </div>
                      <div className="text-right">
                        <span className="text-[10px] text-cyan-400 block">Model B</span>
                        <span className="text-sm font-bold text-white">{abComparison.modelB.timeToFirstTokenMs} ms</span>
                      </div>
                    </div>
                    <div className="text-[10px] text-slate-400 pt-1 border-t border-zinc-900">
                      Initial Streaming Delay: <strong className="text-slate-300">{Math.abs(abComparison.modelA.timeToFirstTokenMs - abComparison.modelB.timeToFirstTokenMs)} ms</strong>
                    </div>
                  </div>

                  {/* Metric 4: Cost & Unit Economics */}
                  <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/80 space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] text-slate-500 uppercase font-bold">Cost Economics</span>
                      <span
                        className={`text-[9px] font-bold px-1.5 py-0.2 rounded border ${
                          abComparison.delta.winnerCost === "A"
                            ? "bg-emerald-500/20 text-emerald-300 border-emerald-500/40"
                            : "bg-cyan-500/20 text-cyan-300 border-cyan-500/40"
                        }`}
                      >
                        WINNER: MODEL {abComparison.delta.winnerCost}
                      </span>
                    </div>
                    <div className="flex items-baseline justify-between">
                      <div>
                        <span className="text-[10px] text-indigo-400 block">Model A</span>
                        <span className="text-sm font-bold text-emerald-400">
                          {abComparison.modelA.costDollars === 0 ? "$0.00 (Local)" : `$${abComparison.modelA.costDollars.toFixed(5)}`}
                        </span>
                      </div>
                      <div className="text-right">
                        <span className="text-[10px] text-cyan-400 block">Model B</span>
                        <span className="text-sm font-bold text-white">
                          {abComparison.modelB.costDollars === 0 ? "$0.00 (Local)" : `$${abComparison.modelB.costDollars.toFixed(5)}`}
                        </span>
                      </div>
                    </div>
                    <div className="text-[10px] text-slate-400 pt-1 border-t border-zinc-900">
                      Savings: <strong className="text-emerald-400">${(abComparison.delta.costSavingsDollars * 1000).toFixed(4)}</strong> / 1k runs
                    </div>
                  </div>
                </div>

                {/* AI Engineering Recommendation Box */}
                <div className="p-3.5 rounded-xl bg-indigo-950/30 border border-indigo-500/30 flex items-start gap-3">
                  <Sparkles className="w-4 h-4 text-indigo-400 shrink-0 mt-0.5" />
                  <div className="space-y-0.5">
                    <span className="text-xs font-bold text-indigo-300 block">
                      Enterprise Pipeline Recommendation:
                    </span>
                    <p className="text-xs text-slate-300 leading-relaxed">
                      {abComparison.delta.overallRecommendation}
                    </p>
                  </div>
                </div>
              </div>

              {/* Side-by-Side Model Output Columns */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 font-mono">
                {/* Model A Output Card */}
                <div className="p-5 rounded-2xl bg-zinc-900/80 border border-indigo-500/40 shadow-xl space-y-3">
                  <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
                    <div className="flex items-center gap-2">
                      <div className="w-6 h-6 rounded-lg bg-indigo-500/20 text-indigo-300 flex items-center justify-center font-bold text-xs">
                        A
                      </div>
                      <div>
                        <h4 className="text-xs font-bold text-white">{abComparison.modelA.model}</h4>
                        <span className="text-[10px] text-slate-400">{abComparison.modelA.endpointName}</span>
                      </div>
                    </div>

                    <button
                      type="button"
                      onClick={() => copyToClipboard(abComparison.modelA.response, "outA")}
                      className="p-1.5 rounded-lg bg-zinc-950 hover:bg-zinc-800 text-slate-400 hover:text-white transition-colors"
                      title="Copy Model A Output"
                    >
                      {copiedKey === "outA" ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                    </button>
                  </div>

                  <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/90 min-h-[220px] max-h-[360px] overflow-y-auto">
                    <pre className="text-xs text-slate-200 whitespace-pre-wrap leading-relaxed">
                      {abComparison.modelA.response}
                    </pre>
                  </div>

                  <div className="flex items-center justify-between text-[10px] text-slate-500 pt-1">
                    <span>Latency: {abComparison.modelA.latencyMs}ms | TPS: {abComparison.modelA.tokensPerSec}</span>
                    <span>{abComparison.modelA.completionTokens} tokens generated</span>
                  </div>
                </div>

                {/* Model B Output Card */}
                <div className="p-5 rounded-2xl bg-zinc-900/80 border border-cyan-500/40 shadow-xl space-y-3">
                  <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
                    <div className="flex items-center gap-2">
                      <div className="w-6 h-6 rounded-lg bg-cyan-500/20 text-cyan-300 flex items-center justify-center font-bold text-xs">
                        B
                      </div>
                      <div>
                        <h4 className="text-xs font-bold text-white">{abComparison.modelB.model}</h4>
                        <span className="text-[10px] text-slate-400">{abComparison.modelB.endpointName}</span>
                      </div>
                    </div>

                    <button
                      type="button"
                      onClick={() => copyToClipboard(abComparison.modelB.response, "outB")}
                      className="p-1.5 rounded-lg bg-zinc-950 hover:bg-zinc-800 text-slate-400 hover:text-white transition-colors"
                      title="Copy Model B Output"
                    >
                      {copiedKey === "outB" ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                    </button>
                  </div>

                  <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/90 min-h-[220px] max-h-[360px] overflow-y-auto">
                    <pre className="text-xs text-slate-200 whitespace-pre-wrap leading-relaxed">
                      {abComparison.modelB.response}
                    </pre>
                  </div>

                  <div className="flex items-center justify-between text-[10px] text-slate-500 pt-1">
                    <span>Latency: {abComparison.modelB.latencyMs}ms | TPS: {abComparison.modelB.tokensPerSec}</span>
                    <span>{abComparison.modelB.completionTokens} tokens generated</span>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Historical A/B Comparisons Table */}
          {abHistory.length > 1 && (
            <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-3 font-mono">
              <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
                <span className="text-xs font-bold text-slate-300 uppercase tracking-wider">
                  Recent A/B Benchmarks Run History
                </span>
                <span className="text-[10px] text-slate-500">{abHistory.length} comparisons saved</span>
              </div>

              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <thead>
                    <tr className="text-[10px] text-slate-500 border-b border-zinc-800">
                      <th className="pb-2">Time</th>
                      <th className="pb-2">Model A</th>
                      <th className="pb-2">Model B</th>
                      <th className="pb-2">Latency Delta</th>
                      <th className="pb-2">Speed Winner</th>
                      <th className="pb-2">Cost Winner</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-900/60 text-slate-300">
                    {abHistory.map((h) => (
                      <tr key={h.id} className="hover:bg-zinc-950/40">
                        <td className="py-2.5 text-slate-500">{new Date(h.timestamp).toLocaleTimeString()}</td>
                        <td className="py-2.5 font-bold text-indigo-300">{h.modelA.model}</td>
                        <td className="py-2.5 font-bold text-cyan-300">{h.modelB.model}</td>
                        <td className="py-2.5 text-emerald-400">{Math.abs(h.delta.latencyDiffMs)} ms</td>
                        <td className="py-2.5">
                          <span className="px-1.5 py-0.2 rounded text-[10px] bg-zinc-800 font-bold">
                            Model {h.delta.winnerSpeed}
                          </span>
                        </td>
                        <td className="py-2.5">
                          <span className="px-1.5 py-0.2 rounded text-[10px] bg-emerald-500/10 text-emerald-400 font-bold">
                            Model {h.delta.winnerCost}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}

      {/* ========================================================================= */}
      {/* --- SINGLE MODEL PROBE & BENCHMARK MODE (ALTERNATIVE VIEW) --- */}
      {/* ========================================================================= */}
      {benchMode === "single" && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
          {/* Left Side: Configuration & Searchable Selector (5 cols) */}
          <div className="lg:col-span-5 space-y-4">
            <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-4 font-mono text-xs">
              <div className="flex items-center gap-2 pb-2 border-b border-zinc-800 text-indigo-400 font-bold uppercase text-xs">
                <Sliders className="w-4 h-4" />
                <span>Single Target Model or Endpoint</span>
              </div>

              {/* Searchable Dropdown for single model */}
              <ModelEndpointSearchDropdown
                localModels={localModels}
                cloudProviders={cloudProviders}
                ollamaBaseUrl={ollamaBaseUrl}
                selectedModelTag={singleModel.modelTag}
                selectedEngine={singleModel.engine}
                onSelect={(item) => setSingleModel(item)}
                label="Target Inference Endpoint"
                variant="default"
              />

              <div>
                <div className="flex items-center justify-between text-[10px] text-slate-500 uppercase font-bold mb-1">
                  <span>Prompt / Instruction</span>
                  <span className="text-slate-600">{prompt.length} chars</span>
                </div>
                <textarea
                  rows={4}
                  value={prompt}
                  onChange={(e) => setPrompt(e.target.value)}
                  placeholder="Type QA prompt, test scenario request, or security fuzz vector..."
                  className="w-full p-3 rounded-xl bg-zinc-950 border border-zinc-800 text-slate-200 text-xs focus:outline-none focus:border-indigo-500 resize-none"
                />
              </div>

              {/* Quick Preset Buttons */}
              <div>
                <span className="text-[10px] text-slate-500 uppercase font-bold block mb-1.5">
                  Quick QA Presets:
                </span>
                <div className="space-y-1.5">
                  {promptPresets.map((preset) => (
                    <button
                      key={preset.title}
                      type="button"
                      onClick={() => setPrompt(preset.text)}
                      className="w-full text-left p-2 rounded-lg bg-zinc-950/80 hover:bg-zinc-800 border border-zinc-800/80 text-[11px] text-slate-300 hover:text-white transition-colors flex items-center justify-between"
                    >
                      <span className="font-bold">{preset.title}</span>
                      <span className="text-[9px] text-slate-500">&rarr; Apply</span>
                    </button>
                  ))}
                </div>
              </div>

              <div className="pt-2">
                <button
                  type="button"
                  onClick={handleRunSingle}
                  disabled={isRunning || !prompt.trim()}
                  className={`w-full py-3 rounded-xl font-bold flex items-center justify-center gap-2 transition-all shadow-lg text-xs ${
                    isRunning || !prompt.trim()
                      ? "bg-zinc-800 text-slate-500 cursor-not-allowed"
                      : "bg-indigo-600 hover:bg-indigo-500 text-white shadow-indigo-950/50 active:scale-95"
                  }`}
                >
                  <Play className={`w-4 h-4 ${isRunning ? "animate-spin" : ""}`} />
                  <span>{isRunning ? "Running Neural Inference..." : `Benchmark ${singleModel.name}`}</span>
                </button>
              </div>
            </div>
          </div>

          {/* Right Side: Live Metrics & Generated Response (7 cols) */}
          <div className="lg:col-span-7 space-y-4">
            <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-4 font-mono">
              <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-zinc-800">
                <div className="flex items-center gap-2">
                  <Sparkles className="w-4 h-4 text-indigo-400" />
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider">
                    Inference Output &amp; Telemetry
                  </h3>
                </div>

                {singleResult && (
                  <button
                    type="button"
                    onClick={() => copyToClipboard(singleResult.response, "single")}
                    className="px-2.5 py-1 rounded-lg bg-zinc-950 hover:bg-zinc-800 border border-zinc-800 text-slate-300 text-[11px] flex items-center gap-1 transition-colors"
                  >
                    <Copy className="w-3 h-3" />
                    <span>{copiedKey === "single" ? "Copied!" : "Copy Output"}</span>
                  </button>
                )}
              </div>

              {singleResult && (
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5 text-xs">
                  <div className="p-2.5 rounded-xl bg-zinc-950 border border-zinc-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase font-bold">Latency</span>
                    <div className="flex items-center gap-1.5 mt-0.5">
                      <Clock className="w-3.5 h-3.5 text-cyan-400" />
                      <span className="text-white font-bold">{singleResult.latencyMs} ms</span>
                    </div>
                  </div>

                  <div className="p-2.5 rounded-xl bg-zinc-950 border border-zinc-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase font-bold">Speed</span>
                    <div className="flex items-center gap-1.5 mt-0.5">
                      <Zap className="w-3.5 h-3.5 text-amber-400" />
                      <span className="text-emerald-400 font-bold">{singleResult.tokensPerSec} t/s</span>
                    </div>
                  </div>

                  <div className="p-2.5 rounded-xl bg-zinc-950 border border-zinc-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase font-bold">TTFT</span>
                    <div className="flex items-center gap-1.5 mt-0.5">
                      <Gauge className="w-3.5 h-3.5 text-indigo-400" />
                      <span className="text-white font-bold">{singleResult.timeToFirstTokenMs} ms</span>
                    </div>
                  </div>

                  <div className="p-2.5 rounded-xl bg-zinc-950 border border-zinc-800/80">
                    <span className="text-[9px] text-slate-500 block uppercase font-bold">Cost</span>
                    <div className="flex items-center gap-1.5 mt-0.5">
                      <Coins className="w-3.5 h-3.5 text-emerald-400" />
                      <span className="text-white font-bold">
                        {singleResult.costDollars === 0 ? "$0.00 (Local)" : `$${singleResult.costDollars.toFixed(5)}`}
                      </span>
                    </div>
                  </div>
                </div>
              )}

              <div className="p-4 rounded-xl bg-zinc-950 border border-zinc-800/90 min-h-[220px] max-h-[360px] overflow-y-auto">
                {isRunning ? (
                  <div className="flex flex-col items-center justify-center h-[200px] space-y-3 text-slate-400">
                    <div className="w-8 h-8 rounded-full border-2 border-indigo-500 border-t-transparent animate-spin" />
                    <span className="text-xs">Streaming response from {singleModel.name}...</span>
                  </div>
                ) : singleResult?.response ? (
                  <pre className="text-xs text-slate-200 whitespace-pre-wrap font-mono leading-relaxed">
                    {singleResult.response}
                  </pre>
                ) : (
                  <div className="flex flex-col items-center justify-center h-[200px] text-slate-500 text-xs">
                    <span>No benchmark execution yet.</span>
                    <span className="text-[10px] mt-1">Select a model or endpoint and click &quot;Benchmark&quot; above.</span>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
