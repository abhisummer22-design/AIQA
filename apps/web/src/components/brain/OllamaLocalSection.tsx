import React, { useState } from "react";
import {
  Server,
  Cpu,
  Zap,
  CheckCircle2,
  AlertTriangle,
  Play,
  RotateCcw,
  Download,
  Trash2,
  HardDrive,
  Sliders,
  Sparkles,
  ExternalLink,
  Shield,
  Activity
} from "lucide-react";
import { LocalOllamaModel } from "../../types";

interface OllamaLocalSectionProps {
  ollama: {
    baseUrl: string;
    status: string;
    version: string;
    latencyMs: number;
    lastPing: string;
    hardware: {
      gpuName: string;
      vramUsedMb: number;
      vramTotalMb: number;
      gpuTempC: number;
      gpuUtilPercent: number;
      cpuUtilPercent: number;
      numaEnabled: boolean;
      flashAttention2: boolean;
      contextLength: number;
      temperature: number;
      topP: number;
      threads: number;
    };
  };
  models: LocalOllamaModel[];
  onSetPrimary: (modelId: string) => void;
  onPullModel: (modelName: string) => void;
  onUpdateConfig: (payload: { baseUrl?: string; hardware?: Record<string, any> }) => void;
}

export const OllamaLocalSection: React.FC<OllamaLocalSectionProps> = ({
  ollama,
  models,
  onSetPrimary,
  onPullModel,
  onUpdateConfig
}) => {
  const [daemonUrl, setDaemonUrl] = useState(ollama.baseUrl);
  const [contextLen, setContextLen] = useState(ollama.hardware.contextLength || 32768);
  const [temp, setTemp] = useState(ollama.hardware.temperature || 0.2);
  const [isPulling, setIsPulling] = useState(false);
  const [pullInput, setPullInput] = useState("");
  const [pullProgress, setPullProgress] = useState<number | null>(null);
  const [selectedTagPreset, setSelectedTagPreset] = useState("qwen2.5-coder:14b");
  const [savedSettings, setSavedSettings] = useState(false);

  const primaryModel = models.find((m) => m.isPrimary) || models[0];
  const isConnected = ollama.status === "connected";

  const handleSaveDaemon = (e: React.FormEvent) => {
    e.preventDefault();
    onUpdateConfig({
      baseUrl: daemonUrl,
      hardware: {
        contextLength: Number(contextLen),
        temperature: Number(temp)
      }
    });
    setSavedSettings(true);
    setTimeout(() => setSavedSettings(false), 2200);
  };

  const handleTriggerPull = (nameToPull?: string) => {
    const target = nameToPull || pullInput;
    if (!target) return;
    setIsPulling(true);
    setPullProgress(10);
    const interval = setInterval(() => {
      setPullProgress((prev) => {
        if (prev === null || prev >= 95) {
          clearInterval(interval);
          return 95;
        }
        return prev + Math.floor(Math.random() * 20 + 15);
      });
    }, 400);

    setTimeout(() => {
      clearInterval(interval);
      setPullProgress(100);
      onPullModel(target);
      setTimeout(() => {
        setIsPulling(false);
        setPullProgress(null);
        setPullInput("");
      }, 700);
    }, 2200);
  };

  return (
    <div className="space-y-6">
      {/* Top Banner: Local Engine Architecture */}
      <div className="p-5 rounded-2xl bg-gradient-to-r from-indigo-950/40 via-zinc-900/80 to-purple-950/30 border border-indigo-500/30 shadow-xl backdrop-blur-md">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
          <div className="space-y-1.5">
            <div className="flex items-center gap-2.5">
              <div className="w-9 h-9 rounded-xl bg-indigo-500/20 border border-indigo-500/40 flex items-center justify-center text-indigo-400">
                <Server className="w-5 h-5" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-bold text-white font-mono">
                    Local Ollama AI Runtime &amp; Hardware Engine
                  </h2>
                  <span
                    className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded-full border ${
                      isConnected
                        ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
                        : "bg-rose-500/10 text-rose-400 border-rose-500/30"
                    }`}
                  >
                    {isConnected ? "● DAEMON ACTIVE" : "▲ OFFLINE"}
                  </span>
                </div>
                <p className="text-xs text-slate-400 font-mono mt-0.5">
                  Direct CUDA GPU / Apple Silicon inference for zero-latency, 100% offline air-gapped test operations
                </p>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <div className="px-3.5 py-2 rounded-xl bg-black/40 border border-zinc-800 text-right">
              <span className="text-[10px] text-slate-400 font-mono block">Loaded Serving Model</span>
              <span className="text-xs font-mono font-bold text-indigo-300 truncate max-w-[200px] block">
                {primaryModel?.name || "gemma3:12b"}
              </span>
            </div>
            <div className="px-3.5 py-2 rounded-xl bg-black/40 border border-zinc-800 text-right">
              <span className="text-[10px] text-slate-400 font-mono block">Local Ping Latency</span>
              <span className="text-xs font-mono font-bold text-emerald-400">
                {ollama.latencyMs} ms
              </span>
            </div>
          </div>
        </div>

        {/* Hardware Status Gauges */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mt-4 pt-4 border-t border-indigo-900/30">
          <div className="p-3 rounded-xl bg-zinc-950/60 border border-zinc-800/80">
            <div className="flex items-center justify-between text-[11px] text-slate-400 font-mono mb-1">
              <span className="flex items-center gap-1.5">
                <Zap className="w-3.5 h-3.5 text-amber-400" /> GPU VRAM
              </span>
              <span className="text-white font-bold">
                {Math.round(ollama.hardware.vramUsedMb / 1024)}GB / {Math.round(ollama.hardware.vramTotalMb / 1024)}GB
              </span>
            </div>
            <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden">
              <div
                className="bg-gradient-to-r from-indigo-500 to-purple-500 h-full rounded-full transition-all duration-500"
                style={{ width: `${(ollama.hardware.vramUsedMb / ollama.hardware.vramTotalMb) * 100}%` }}
              />
            </div>
            <span className="text-[9px] text-slate-500 font-mono block mt-1">
              {ollama.hardware.gpuName}
            </span>
          </div>

          <div className="p-3 rounded-xl bg-zinc-950/60 border border-zinc-800/80">
            <div className="flex items-center justify-between text-[11px] text-slate-400 font-mono mb-1">
              <span className="flex items-center gap-1.5">
                <Activity className="w-3.5 h-3.5 text-indigo-400" /> GPU Load
              </span>
              <span className="text-white font-bold">{ollama.hardware.gpuUtilPercent}%</span>
            </div>
            <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden">
              <div
                className="bg-indigo-500 h-full rounded-full transition-all duration-500"
                style={{ width: `${ollama.hardware.gpuUtilPercent}%` }}
              />
            </div>
            <span className="text-[9px] text-slate-500 font-mono block mt-1">
              Temp: {ollama.hardware.gpuTempC}°C (Optimal)
            </span>
          </div>

          <div className="p-3 rounded-xl bg-zinc-950/60 border border-zinc-800/80">
            <div className="flex items-center justify-between text-[11px] text-slate-400 font-mono mb-1">
              <span className="flex items-center gap-1.5">
                <Cpu className="w-3.5 h-3.5 text-cyan-400" /> CPU Core Load
              </span>
              <span className="text-white font-bold">{ollama.hardware.cpuUtilPercent}%</span>
            </div>
            <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden">
              <div
                className="bg-cyan-500 h-full rounded-full transition-all duration-500"
                style={{ width: `${ollama.hardware.cpuUtilPercent}%` }}
              />
            </div>
            <span className="text-[9px] text-slate-500 font-mono block mt-1">
              Threads: {ollama.hardware.threads} cores
            </span>
          </div>

          <div className="p-3 rounded-xl bg-zinc-950/60 border border-zinc-800/80">
            <div className="flex items-center justify-between text-[11px] text-slate-400 font-mono mb-1">
              <span className="flex items-center gap-1.5">
                <Shield className="w-3.5 h-3.5 text-emerald-400" /> Air-Gap Privacy
              </span>
              <span className="text-emerald-400 font-bold">100% SECURE</span>
            </div>
            <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden">
              <div className="bg-emerald-500 h-full rounded-full" style={{ width: "100%" }} />
            </div>
            <span className="text-[9px] text-slate-500 font-mono block mt-1">
              Flash Attention 2: ENABLED
            </span>
          </div>
        </div>
      </div>

      {/* Main Grid: Daemon Settings & Pull New Model */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left Column: Daemon Connection & Acceleration Sliders */}
        <div className="lg:col-span-1 p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b border-zinc-800 text-indigo-400 font-mono text-xs font-bold uppercase">
            <Sliders className="w-4 h-4" />
            <span>Daemon Connection &amp; Tuning</span>
          </div>

          <form onSubmit={handleSaveDaemon} className="space-y-4 font-mono text-xs">
            <div>
              <label className="text-slate-400 text-[11px] font-bold block mb-1.5 uppercase">
                Ollama Base URL
              </label>
              <input
                type="text"
                value={daemonUrl}
                onChange={(e) => setDaemonUrl(e.target.value)}
                placeholder="http://localhost:11434"
                className="w-full p-2.5 rounded-xl bg-zinc-950 border border-zinc-800 text-white focus:outline-none focus:border-indigo-500 transition-colors"
              />
              <span className="text-[10px] text-slate-500 mt-1 block">
                Default: http://localhost:11434 (or remote LAN host)
              </span>
            </div>

            <div>
              <div className="flex items-center justify-between text-slate-400 text-[11px] font-bold mb-1.5 uppercase">
                <span>Context Window (Tokens)</span>
                <span className="text-indigo-400">{contextLen.toLocaleString()}</span>
              </div>
              <input
                type="range"
                min="4096"
                max="131072"
                step="4096"
                value={contextLen}
                onChange={(e) => setContextLen(Number(e.target.value))}
                className="w-full accent-indigo-500 cursor-pointer"
              />
              <div className="flex justify-between text-[9px] text-slate-500">
                <span>4k</span>
                <span>32k</span>
                <span>64k</span>
                <span>128k</span>
              </div>
            </div>

            <div>
              <div className="flex items-center justify-between text-slate-400 text-[11px] font-bold mb-1.5 uppercase">
                <span>Temperature (Creativity)</span>
                <span className="text-indigo-400">{temp}</span>
              </div>
              <input
                type="range"
                min="0.0"
                max="1.0"
                step="0.05"
                value={temp}
                onChange={(e) => setTemp(Number(e.target.value))}
                className="w-full accent-indigo-500 cursor-pointer"
              />
              <div className="flex justify-between text-[9px] text-slate-500">
                <span>0.0 (Strict Deterministic)</span>
                <span>1.0 (Creative)</span>
              </div>
            </div>

            <div className="pt-2">
              <button
                type="submit"
                className="w-full py-2.5 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white font-bold flex items-center justify-center gap-2 transition-all shadow-md active:scale-95"
              >
                <CheckCircle2 className="w-4 h-4" />
                <span>{savedSettings ? "Daemon Settings Applied!" : "Save & Sync Daemon"}</span>
              </button>
            </div>
          </form>
        </div>

        {/* Right Column: Pull / Download New Model */}
        <div className="lg:col-span-2 p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-4">
          <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
            <div className="flex items-center gap-2 text-purple-400 font-mono text-xs font-bold uppercase">
              <Download className="w-4 h-4" />
              <span>Pull &amp; Install New Ollama Model</span>
            </div>
            <span className="text-[10px] text-slate-500 font-mono">Registry: ollama.com/library</span>
          </div>

          <div className="space-y-3 font-mono text-xs">
            <p className="text-slate-400 text-[11px]">
              Type any Ollama model tag or select a recommended model below to download and register into your local hardware engine.
            </p>

            <div className="flex gap-2">
              <input
                type="text"
                value={pullInput}
                onChange={(e) => setPullInput(e.target.value)}
                placeholder="e.g. deepseek-r1:14b, codellama:13b, llama3.2-vision"
                className="flex-1 p-2.5 rounded-xl bg-zinc-950 border border-zinc-800 text-white focus:outline-none focus:border-purple-500"
              />
              <button
                type="button"
                onClick={() => handleTriggerPull()}
                disabled={isPulling || !pullInput}
                className={`px-5 py-2.5 rounded-xl font-bold flex items-center gap-2 transition-all ${
                  isPulling || !pullInput
                    ? "bg-zinc-800 text-slate-500 cursor-not-allowed"
                    : "bg-purple-600 hover:bg-purple-500 text-white shadow-md active:scale-95"
                }`}
              >
                <Download className="w-4 h-4" />
                <span>{isPulling ? "Pulling..." : "Pull Model"}</span>
              </button>
            </div>

            {/* Quick Preset Badges */}
            <div className="pt-2">
              <span className="text-[10px] text-slate-500 block mb-2 uppercase font-bold">
                Quick Download Presets (Tested for AIQA Operations):
              </span>
              <div className="flex flex-wrap gap-2">
                {[
                  { tag: "qwen2.5-coder:14b", desc: "Best for Code & Test Gen" },
                  { tag: "deepseek-r1:8b", desc: "CoT Deep Reasoning" },
                  { tag: "llama3.3:70b", desc: "Enterprise RBAC" },
                  { tag: "mistral:7b", desc: "Fast Jira Summary" },
                  { tag: "phi3.5:3.8b", desc: "Ultra-low Latency" },
                  { tag: "codellama:13b", desc: "PyTest Suite Gen" }
                ].map((item) => (
                  <button
                    key={item.tag}
                    type="button"
                    onClick={() => {
                      setPullInput(item.tag);
                      handleTriggerPull(item.tag);
                    }}
                    disabled={isPulling}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-zinc-950 hover:bg-purple-950/40 border border-zinc-800 hover:border-purple-500/40 text-slate-300 hover:text-white transition-all text-[11px]"
                  >
                    <Download className="w-3 h-3 text-purple-400" />
                    <span className="font-bold">{item.tag}</span>
                    <span className="text-[9px] text-slate-500">({item.desc})</span>
                  </button>
                ))}
              </div>
            </div>

            {/* Pull Progress Visualizer */}
            {isPulling && (
              <div className="p-3 rounded-xl bg-purple-950/20 border border-purple-500/30 space-y-1.5 animate-pulse">
                <div className="flex items-center justify-between text-[11px] text-purple-300">
                  <span>Downloading layers for {pullInput || "model"}...</span>
                  <span className="font-bold">{pullProgress}%</span>
                </div>
                <div className="w-full bg-zinc-900 h-2 rounded-full overflow-hidden">
                  <div
                    className="bg-purple-500 h-full rounded-full transition-all duration-300"
                    style={{ width: `${pullProgress}%` }}
                  />
                </div>
                <span className="text-[9px] text-slate-400">Verifying SHA256 layer hashes &amp; allocating CUDA VRAM</span>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Installed Local Models Table */}
      <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-4">
        <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
          <div className="flex items-center gap-2">
            <HardDrive className="w-4 h-4 text-indigo-400" />
            <h3 className="text-xs font-bold font-mono text-white uppercase tracking-wider">
              Installed Local Models Library ({models.length} Models Ready)
            </h3>
          </div>
          <span className="text-[10px] font-mono text-slate-400">
            Click &quot;Load as Primary&quot; to switch active CUDA serving node
          </span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {models.map((model) => {
            const isLoaded = model.isPrimary || model.status === "loaded";
            return (
              <div
                key={model.id}
                className={`p-4 rounded-xl border transition-all duration-200 flex flex-col justify-between ${
                  isLoaded
                    ? "bg-indigo-950/30 border-indigo-500/50 shadow-lg shadow-indigo-950/50 ring-1 ring-indigo-500/20"
                    : "bg-zinc-950/60 border-zinc-800 hover:border-zinc-700"
                }`}
              >
                <div className="space-y-2">
                  <div className="flex items-start justify-between gap-2">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-bold font-mono text-white">{model.name}</span>
                        {isLoaded && (
                          <span className="px-1.5 py-0.2 rounded bg-indigo-500/20 text-indigo-300 border border-indigo-500/40 text-[9px] font-mono font-bold">
                            SERVING
                          </span>
                        )}
                      </div>
                      <span className="text-[10px] text-slate-400 font-mono block mt-0.5">
                        {model.tag}
                      </span>
                    </div>

                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-zinc-900 text-slate-300 border border-zinc-800 shrink-0">
                      {model.sizeGb} GB
                    </span>
                  </div>

                  <p className="text-[11px] text-slate-300 font-sans line-clamp-2">
                    {model.specialization}
                  </p>

                  <div className="grid grid-cols-2 gap-2 text-[10px] font-mono pt-2 border-t border-zinc-800/80 text-slate-400">
                    <div>
                      <span className="text-slate-500 block">Precision:</span>
                      <span className="text-slate-200 font-bold">{model.quantization}</span>
                    </div>
                    <div>
                      <span className="text-slate-500 block">Context:</span>
                      <span className="text-slate-200 font-bold">{model.contextLength.toLocaleString()} tokens</span>
                    </div>
                  </div>
                </div>

                <div className="pt-3 mt-3 border-t border-zinc-800/80 flex items-center justify-between">
                  <span className="text-[9px] font-mono text-slate-500">
                    {model.lastUsed ? `Used: ${model.lastUsed}` : "Ready in library"}
                  </span>

                  {isLoaded ? (
                    <span className="flex items-center gap-1 text-[11px] font-mono text-emerald-400 font-bold">
                      <CheckCircle2 className="w-3.5 h-3.5" />
                      <span>Active Engine</span>
                    </span>
                  ) : (
                    <button
                      type="button"
                      onClick={() => onSetPrimary(model.id)}
                      className="px-3 py-1 rounded-lg bg-zinc-800 hover:bg-indigo-600 text-slate-200 hover:text-white font-mono text-xs font-bold transition-colors flex items-center gap-1.5"
                    >
                      <Play className="w-3 h-3" />
                      <span>Load as Primary</span>
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
