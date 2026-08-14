import React from "react";
import {
  Server,
  Database,
  Cpu,
  Boxes,
  Zap,
  Activity,
  Layers,
  Mic,
  AlertCircle,
  AlertTriangle,
  ExternalLink,
  RefreshCw
} from "lucide-react";
import { SystemHealthState } from "../../types";

interface SystemHealthBannerProps {
  systemHealth?: SystemHealthState;
  health?: SystemHealthState;
  onOpenDiagnostics: () => void;
}

export const SystemHealthBanner: React.FC<SystemHealthBannerProps> = ({
  systemHealth,
  health,
  onOpenDiagnostics
}) => {
  const currentHealth = systemHealth || health;
  if (!currentHealth) return null;

  // Check for any degraded or offline critical components
  const issues: { component: string; message: string; severity: "critical" | "warning" }[] = [];

  if (currentHealth.backend !== "online") {
    issues.push({
      component: "Backend",
      message: `CRITICAL: Backend gateway is ${currentHealth.backend.toUpperCase()}`,
      severity: "critical"
    });
  }
  if (currentHealth.database !== "connected") {
    issues.push({
      component: "Database",
      message: `CRITICAL: Primary PostgreSQL DB is ${currentHealth.database.toUpperCase()}`,
      severity: "critical"
    });
  }
  if (currentHealth.ollama === "offline") {
    issues.push({
      component: "Ollama",
      message: "DEGRADED: Ollama offline (AI requests auto-routed to Gemini 3.7 Cloud Fallback)",
      severity: "warning"
    });
  }
  if (currentHealth.testExecutor === "stopped") {
    issues.push({
      component: "Test Executor",
      message: "CRITICAL: Test execution engine is halted",
      severity: "critical"
    });
  }
  if (currentHealth.apiConnectivity < 98) {
    issues.push({
      component: "API Ingress",
      message: `DEGRADED: High packet drop detected (${currentHealth.apiConnectivity}% connectivity)`,
      severity: "warning"
    });
  }

  return (
    <div id="system-health-banner-container">
      {/* Alert Warning Box if any issues detected */}
      {issues.length > 0 && (
        <div
          id="system-health-critical-alert"
          className={`px-4 py-1.5 border-b flex items-center justify-between gap-3 text-[11px] font-mono ${
            issues.some((i) => i.severity === "critical")
              ? "bg-rose-950/60 border-rose-900/60 text-rose-200"
              : "bg-amber-950/60 border-amber-900/60 text-amber-200"
          }`}
        >
          <div className="flex items-center gap-2">
            {issues.some((i) => i.severity === "critical") ? (
              <AlertCircle className="w-3.5 h-3.5 text-rose-400 shrink-0" />
            ) : (
              <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
            )}
            <span className="font-semibold">{issues[0].message}</span>
          </div>
          <button
            id="btn-alert-view-diagnostics"
            onClick={onOpenDiagnostics}
            className="flex items-center gap-1 text-[10px] underline hover:text-white transition-colors"
          >
            <span>Remediation & Chaos Controls</span>
            <ExternalLink className="w-2.5 h-2.5" />
          </button>
        </div>
      )}

      {/* High Density Telemetry Strip */}
      <div
        id="system-component-status-strip"
        className="bg-indigo-900/10 border-b border-slate-800 px-4 py-1.5 flex items-center justify-between gap-4 overflow-x-auto text-[10px] font-mono text-slate-300 select-none"
      >
        <div className="flex items-center gap-5 shrink-0">
          <div className="opacity-40 uppercase font-bold text-[9px] text-slate-400 tracking-wider">
            System Telemetry:
          </div>

          {/* Backend */}
          <div className="flex items-center gap-1.5" title="Express API Gateway">
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                currentHealth.backend === "online"
                  ? "bg-emerald-500"
                  : currentHealth.backend === "degraded"
                  ? "bg-amber-500"
                  : "bg-rose-500 animate-ping"
              }`}
            />
            <span className="text-slate-400">Backend:</span>
            <span className={currentHealth.backend === "online" ? "text-slate-200 font-semibold" : "text-rose-400 font-bold"}>
              {currentHealth.backend === "online" ? "Online" : currentHealth.backend}
            </span>
          </div>

          {/* Database */}
          <div className="flex items-center gap-1.5" title="PostgreSQL Transaction Pool">
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                currentHealth.database === "connected"
                  ? "bg-emerald-500"
                  : currentHealth.database === "degraded"
                  ? "bg-amber-500"
                  : "bg-rose-500"
              }`}
            />
            <span className="text-slate-400">Database:</span>
            <span className={currentHealth.database === "connected" ? "text-slate-200 font-semibold" : "text-rose-400 font-bold"}>
              {currentHealth.database === "connected" ? "Connected" : currentHealth.database}
            </span>
          </div>

          {/* Ollama Local AI */}
          <div className="flex items-center gap-1.5" title="Local Inference Node (Gemma 3 12B)">
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                currentHealth.ollama === "connected"
                  ? "bg-emerald-500"
                  : "bg-amber-500"
              }`}
            />
            <span className="text-slate-400">Ollama:</span>
            <span className={currentHealth.ollama === "connected" ? "text-slate-200 font-semibold" : "text-amber-400 font-medium"}>
              {currentHealth.ollama === "connected" ? "Connected" : "Offline (Cloud Fallback)"}
            </span>
          </div>

          {/* Test Executor */}
          <div className="flex items-center gap-1.5" title="Distributed Test Runner Pods">
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                currentHealth.testExecutor === "running" ? "bg-emerald-500 animate-pulse" : "bg-rose-500"
              }`}
            />
            <span className="text-slate-400">Runners:</span>
            <span className="text-slate-200 font-semibold capitalize">{currentHealth.testExecutor}</span>
          </div>

          {/* Job Queue */}
          <div className="flex items-center gap-1.5" title="Job queue status">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
            <span className="text-slate-400">Queue:</span>
            <span className="text-slate-200 font-semibold">{currentHealth.queue.pending} pending</span>
          </div>

          {/* Active Workers */}
          <div className="flex items-center gap-1.5" title="Test worker nodes">
            <span className="text-slate-400">Workers:</span>
            <span className="text-indigo-400 font-semibold">{currentHealth.worker.active}/{currentHealth.worker.total} Active</span>
          </div>

          {/* Ingress Connectivity */}
          <div className="flex items-center gap-1.5" title="Network connectivity">
            <span className="text-slate-400">Connectivity:</span>
            <span className="text-emerald-400 font-semibold">{currentHealth.apiConnectivity}%</span>
          </div>
        </div>

        {/* Diagnostics Trigger on right */}
        <button
          id="btn-open-diagnostics-bar"
          onClick={onOpenDiagnostics}
          className="text-[10px] text-indigo-400 hover:text-indigo-300 hover:underline flex items-center gap-1 shrink-0 font-mono"
        >
          <Activity className="w-3 h-3" />
          <span>Chaos / Diagnostics</span>
        </button>
      </div>
    </div>
  );
};

