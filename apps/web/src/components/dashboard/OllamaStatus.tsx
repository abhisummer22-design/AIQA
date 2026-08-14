import React from "react";
import { Server, Activity, HardDrive, Cpu, Zap, AlertTriangle, CheckCircle2 } from "lucide-react";
import { ModelRouterState, SystemHealthState, ViewType } from "../../types";

interface OllamaStatusProps {
  modelRouter: ModelRouterState;
  systemHealth: SystemHealthState;
  onNavigate?: (view: ViewType) => void;
}

export const OllamaStatus: React.FC<OllamaStatusProps> = ({ modelRouter, systemHealth, onNavigate }) => {
  const isOnline = systemHealth.ollama === "connected";
  const primary = modelRouter.primaryModel;

  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 space-y-2.5">
      {/* Header */}
      <div className="flex items-center justify-between pb-2 border-b border-slate-800">
        <div className="flex items-center gap-2">
          <Server className="w-3.5 h-3.5 text-indigo-400" />
          <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
            Local AI Runtime (Ollama)
          </h3>
        </div>
        <div className="flex items-center gap-2">
          {onNavigate && (
            <button
              onClick={() => onNavigate("ai-brain")}
              className="text-[10px] font-mono text-indigo-400 hover:underline"
            >
              Manage &rarr;
            </button>
          )}
          <span
            className={`text-[9px] font-mono font-bold px-1.5 py-0.2 rounded border ${
              isOnline
                ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
                : "bg-rose-500/10 text-rose-400 border-rose-500/30"
            }`}
          >
            {isOnline ? "● Connected" : "▲ Offline"}
          </span>
        </div>
      </div>

      {/* Model & Load State */}
      <div className="grid grid-cols-2 gap-2 text-xs font-mono">
        <div className="p-2 rounded bg-[#070708] border border-slate-800">
          <span className="text-[9px] text-slate-500 uppercase block">Model Loaded</span>
          <span className="font-bold text-slate-200 truncate block mt-0.5 text-[11px]">{primary.name}</span>
        </div>
        <div className="p-2 rounded bg-[#070708] border border-slate-800">
          <span className="text-[9px] text-slate-500 uppercase block">Load Time</span>
          <span className="font-bold text-indigo-400 block mt-0.5 text-[11px]">{primary.modelLoadTimeMs} ms</span>
        </div>
      </div>

      {/* Hardware Utilization Metrics */}
      <div className="space-y-1.5 text-xs font-mono">
        {/* GPU */}
        <div>
          <div className="flex items-center justify-between text-[10px] text-slate-400 mb-0.5">
            <span className="flex items-center gap-1">
              <Zap className="w-2.5 h-2.5 text-indigo-400" /> GPU VRAM Allocation
            </span>
            <span className="text-slate-200 font-semibold">{primary.memoryUsageMb} MB ({primary.gpuPercent}%)</span>
          </div>
          <div className="w-full bg-slate-800 h-1 rounded-full overflow-hidden">
            <div
              className="bg-indigo-500 h-full rounded-full transition-all"
              style={{ width: `${primary.gpuPercent}%` }}
            />
          </div>
        </div>

        {/* CPU */}
        <div>
          <div className="flex items-center justify-between text-[10px] text-slate-400 mb-0.5">
            <span className="flex items-center gap-1">
              <Cpu className="w-2.5 h-2.5 text-slate-400" /> Host CPU Usage
            </span>
            <span className="text-slate-200 font-semibold">{primary.cpuPercent}%</span>
          </div>
          <div className="w-full bg-slate-800 h-1 rounded-full overflow-hidden">
            <div
              className="bg-slate-500 h-full rounded-full transition-all"
              style={{ width: `${primary.cpuPercent}%` }}
            />
          </div>
        </div>
      </div>

      <div className="pt-1.5 border-t border-slate-800 text-[10px] text-slate-500 font-mono flex items-center justify-between">
        <span>Inference Precision:</span>
        <span className="text-slate-300 font-medium">Q4_K_M (Flash Attention v2)</span>
      </div>
    </div>
  );
};

