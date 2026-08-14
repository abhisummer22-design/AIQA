import React, { useState } from "react";
import {
  GitFork,
  CheckCircle2,
  Sliders,
  Sparkles,
  Shield,
  Bug,
  Activity,
  Mic,
  ArrowRight,
  RefreshCw,
  Cpu,
  Layers
} from "lucide-react";
import { BrainTaskRouting, LocalOllamaModel, CloudApiProvider } from "../../types";

interface TaskRoutingMatrixSectionProps {
  taskRoutings: BrainTaskRouting[];
  localModels: LocalOllamaModel[];
  cloudProviders: CloudApiProvider[];
  onSaveRoutings: (routings: BrainTaskRouting[]) => void;
}

export const TaskRoutingMatrixSection: React.FC<TaskRoutingMatrixSectionProps> = ({
  taskRoutings,
  localModels,
  cloudProviders,
  onSaveRoutings
}) => {
  const [routings, setRoutings] = useState<BrainTaskRouting[]>(taskRoutings);
  const [isSaved, setIsSaved] = useState(false);

  const getTaskIcon = (key: string) => {
    switch (key) {
      case "test_case_gen":
        return <Sparkles className="w-4 h-4 text-purple-400" />;
      case "rbac_security_fuzzing":
        return <Shield className="w-4 h-4 text-emerald-400" />;
      case "bug_drafting":
        return <Bug className="w-4 h-4 text-rose-400" />;
      case "flakiness_analytics":
        return <Activity className="w-4 h-4 text-amber-400" />;
      case "rag_grounding_eval":
        return <Layers className="w-4 h-4 text-blue-400" />;
      case "voice_assistant":
        return <Mic className="w-4 h-4 text-cyan-400" />;
      default:
        return <Cpu className="w-4 h-4 text-indigo-400" />;
    }
  };

  const getAvailableModelsForEngine = (engine: string) => {
    if (engine === "ollama") {
      return localModels.map((m) => m.id);
    }
    const provider = cloudProviders.find((p) => p.id === engine);
    return provider ? provider.availableModels : ["default-model"];
  };

  const handleEngineChange = (taskKey: string, isPrimary: boolean, newEngine: any) => {
    setRoutings((prev) =>
      prev.map((item) => {
        if (item.taskKey !== taskKey) return item;
        const available = getAvailableModelsForEngine(newEngine);
        if (isPrimary) {
          return {
            ...item,
            assignedEngine: newEngine,
            assignedModel: available[0] || "default"
          };
        } else {
          return {
            ...item,
            fallbackEngine: newEngine,
            fallbackModel: available[0] || "default"
          };
        }
      })
    );
  };

  const handleModelChange = (taskKey: string, isPrimary: boolean, newModel: string) => {
    setRoutings((prev) =>
      prev.map((item) => {
        if (item.taskKey !== taskKey) return item;
        return isPrimary
          ? { ...item, assignedModel: newModel }
          : { ...item, fallbackModel: newModel };
      })
    );
  };

  const handleSave = () => {
    onSaveRoutings(routings);
    setIsSaved(true);
    setTimeout(() => setIsSaved(false), 2200);
  };

  return (
    <div className="space-y-6">
      {/* Top Overview Banner */}
      <div className="p-5 rounded-2xl bg-gradient-to-r from-purple-950/40 via-zinc-900/80 to-indigo-950/30 border border-purple-500/30 shadow-xl backdrop-blur-md">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
          <div className="space-y-1.5">
            <div className="flex items-center gap-2.5">
              <div className="w-9 h-9 rounded-xl bg-purple-500/20 border border-purple-500/40 flex items-center justify-center text-purple-400">
                <GitFork className="w-5 h-5" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-bold text-white font-mono">
                    Neural Task Dispatch Matrix &amp; Workload Policies
                  </h2>
                  <span className="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-purple-500/10 text-purple-300 border border-purple-500/30">
                    6 WORKLOAD PIPELINES
                  </span>
                </div>
                <p className="text-xs text-slate-400 font-mono mt-0.5">
                  Route specific AIQA analytical tasks to optimal specialized models with automated zero-downtime failover
                </p>
              </div>
            </div>
          </div>

          <button
            type="button"
            onClick={handleSave}
            className="px-5 py-2.5 rounded-xl bg-purple-600 hover:bg-purple-500 text-white font-mono text-xs font-bold flex items-center gap-2 shadow-lg shadow-purple-950/50 transition-all active:scale-95"
          >
            <CheckCircle2 className="w-4 h-4" />
            <span>{isSaved ? "Dispatch Rules Deployed!" : "Save Routing Matrix"}</span>
          </button>
        </div>
      </div>

      {/* Task Routing Cards List */}
      <div className="space-y-4">
        {routings.map((task) => {
          const primaryModels = getAvailableModelsForEngine(task.assignedEngine);
          const fallbackModels = getAvailableModelsForEngine(task.fallbackEngine);

          return (
            <div
              key={task.taskKey}
              className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 hover:border-zinc-700 transition-all duration-200"
            >
              <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4 pb-4 border-b border-zinc-800/80">
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded-xl bg-zinc-950 border border-zinc-800 flex items-center justify-center">
                    {getTaskIcon(task.taskKey)}
                  </div>
                  <div>
                    <h3 className="text-sm font-bold font-mono text-white">{task.taskTitle}</h3>
                    <p className="text-xs text-slate-400 font-sans mt-0.5">{task.description}</p>
                  </div>
                </div>

                <div className="flex items-center gap-2 text-[10px] font-mono text-slate-400">
                  <span>Temp: <strong className="text-indigo-300">{task.temperature}</strong></span>
                  <span>•</span>
                  <span>Max Tokens: <strong className="text-indigo-300">{task.maxTokens}</strong></span>
                  {task.reasoningEffort && (
                    <>
                      <span>•</span>
                      <span className="text-amber-400 uppercase font-bold">Thinking: {task.reasoningEffort}</span>
                    </>
                  )}
                </div>
              </div>

              {/* Engine & Fallback Assignment Controls */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6 pt-4 font-mono text-xs">
                {/* Primary Engine */}
                <div className="p-3 rounded-xl bg-zinc-950 border border-indigo-900/40 space-y-2">
                  <div className="flex items-center justify-between text-[10px] uppercase font-bold text-indigo-400">
                    <span>1. Primary Engine</span>
                    <span className="text-slate-500">Tier 1 Fast Dispatch</span>
                  </div>

                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="text-[9px] text-slate-500 block mb-1">Provider Source</label>
                      <select
                        value={task.assignedEngine}
                        onChange={(e) => handleEngineChange(task.taskKey, true, e.target.value)}
                        className="w-full p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-white text-xs focus:outline-none focus:border-indigo-500"
                      >
                        <option value="ollama">Local Ollama (CUDA)</option>
                        <option value="gemini">Google Gemini AI</option>
                        <option value="openai">OpenAI GPT-4o</option>
                        <option value="anthropic">Anthropic Claude</option>
                        <option value="deepseek">DeepSeek AI</option>
                        <option value="groq">Groq LPU (Ultra Fast)</option>
                        <option value="mistral">Mistral AI (EU)</option>
                        <option value="custom_openai">Custom OpenAI Proxy</option>
                      </select>
                    </div>

                    <div>
                      <label className="text-[9px] text-slate-500 block mb-1">Assigned Model</label>
                      <select
                        value={task.assignedModel}
                        onChange={(e) => handleModelChange(task.taskKey, true, e.target.value)}
                        className="w-full p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-white text-xs focus:outline-none focus:border-indigo-500"
                      >
                        {primaryModels.map((m) => (
                          <option key={m} value={m}>
                            {m}
                          </option>
                        ))}
                      </select>
                    </div>
                  </div>
                </div>

                {/* Fallback Engine */}
                <div className="p-3 rounded-xl bg-zinc-950 border border-amber-900/40 space-y-2">
                  <div className="flex items-center justify-between text-[10px] uppercase font-bold text-amber-400">
                    <span>2. Automatic Fallback</span>
                    <span className="text-slate-500">Tier 2 Failover Backup</span>
                  </div>

                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="text-[9px] text-slate-500 block mb-1">Fallback Source</label>
                      <select
                        value={task.fallbackEngine}
                        onChange={(e) => handleEngineChange(task.taskKey, false, e.target.value)}
                        className="w-full p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-white text-xs focus:outline-none focus:border-amber-500"
                      >
                        <option value="gemini">Google Gemini AI</option>
                        <option value="ollama">Local Ollama (CUDA)</option>
                        <option value="openai">OpenAI GPT-4o</option>
                        <option value="anthropic">Anthropic Claude</option>
                        <option value="deepseek">DeepSeek AI</option>
                        <option value="groq">Groq LPU (Ultra Fast)</option>
                        <option value="mistral">Mistral AI (EU)</option>
                        <option value="custom_openai">Custom OpenAI Proxy</option>
                      </select>
                    </div>

                    <div>
                      <label className="text-[9px] text-slate-500 block mb-1">Fallback Model</label>
                      <select
                        value={task.fallbackModel}
                        onChange={(e) => handleModelChange(task.taskKey, false, e.target.value)}
                        className="w-full p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-white text-xs focus:outline-none focus:border-amber-500"
                      >
                        {fallbackModels.map((m) => (
                          <option key={m} value={m}>
                            {m}
                          </option>
                        ))}
                      </select>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
