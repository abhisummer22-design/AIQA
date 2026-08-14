import React, { useState } from "react";
import {
  Play,
  Pause,
  Terminal,
  Clock,
  Layers,
  CheckCircle2,
  XCircle,
  ExternalLink,
  ChevronDown,
  ChevronUp,
  Cpu,
  RefreshCw,
  Plus
} from "lucide-react";
import { LiveExecution, ViewType } from "../../types";

interface LiveTestExecutionProps {
  executions: LiveExecution[];
  onTriggerNew: () => void;
  onNavigate: (view: ViewType) => void;
}

function formatDuration(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

export const LiveTestExecution: React.FC<LiveTestExecutionProps> = ({
  executions,
  onTriggerNew,
  onNavigate
}) => {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const toggleExpand = (id: string) => {
    setExpandedId((prev) => (prev === id ? null : id));
  };

  return (
    <div
      id="panel-live-test-execution"
      className="bg-[#0D0D0E] border border-slate-800 rounded overflow-hidden"
    >
      {/* Header */}
      <div className="px-3.5 py-2.5 border-b border-slate-800 bg-slate-900/40 flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <div className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse shadow-[0_0_8px_rgba(16,185,129,0.5)]" />
          <h2 className="text-xs font-bold uppercase text-white font-mono tracking-wider">
            Live Test Execution
          </h2>
          <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-indigo-950 text-indigo-400 border border-indigo-800/60 font-semibold">
            {executions.filter((e) => e.status === "running").length} Active Runners
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            id="btn-view-all-runs"
            onClick={() => onNavigate("test-runs")}
            className="text-[11px] text-slate-400 hover:text-slate-200 transition-colors font-medium"
          >
            Execution Hub &rarr;
          </button>
          <button
            id="btn-live-panel-trigger"
            onClick={onTriggerNew}
            className="flex items-center gap-1 px-2.5 py-1 rounded bg-indigo-600 hover:bg-indigo-500 text-white text-[10px] font-semibold transition-colors shadow-sm"
          >
            <Plus className="w-3 h-3" />
            <span>New Execution</span>
          </button>
        </div>
      </div>

      {/* Execution List */}
      <div className="divide-y divide-slate-800/60 text-[11px]">
        {executions.length === 0 ? (
          <div className="p-6 text-center text-slate-500 text-[11px]">
            No active test runs in progress. Trigger a suite to monitor live execution.
          </div>
        ) : (
          executions.map((exec) => {
            const isExpanded = expandedId === exec.id;
            return (
              <div
                key={exec.id}
                id={`execution-row-${exec.id}`}
                className="hover:bg-slate-900/30 transition-colors"
              >
                <div className="px-3.5 py-2.5 flex flex-col lg:flex-row lg:items-center justify-between gap-2.5">
                  {/* Left info: Name, Project, Executor */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-xs text-slate-200 truncate hover:text-indigo-400 transition-colors cursor-pointer">
                        {exec.testName}
                      </span>
                      <span
                        className={`text-[9px] font-mono px-1.5 py-0.2 rounded border font-semibold uppercase ${
                          exec.environment === "Production"
                            ? "bg-rose-500/10 text-rose-400 border-rose-500/30"
                            : exec.environment === "Staging"
                            ? "bg-indigo-500/10 text-indigo-400 border-indigo-500/30"
                            : "bg-slate-800 text-slate-300 border-slate-700"
                        }`}
                      >
                        {exec.environment}
                      </span>
                    </div>

                    <div className="mt-0.5 flex items-center gap-2.5 text-[10px] text-slate-400 font-mono">
                      <span className="text-slate-300 flex items-center gap-1">
                        <Layers className="w-2.5 h-2.5 text-slate-500" />
                        {exec.project}
                      </span>
                      <span className="text-slate-700">&bull;</span>
                      <span className="text-slate-400 flex items-center gap-1">
                        <Cpu className="w-2.5 h-2.5 text-indigo-400" />
                        {exec.executor}
                      </span>
                    </div>
                  </div>

                  {/* Middle / Right: Duration, Counts, Live Progress Bar */}
                  <div className="flex items-center gap-4 shrink-0">
                    {/* Duration */}
                    <div className="flex items-center gap-1 font-mono text-[11px] text-slate-300">
                      <Clock className="w-3 h-3 text-indigo-400" />
                      <span>{formatDuration(exec.durationSec)}</span>
                    </div>

                    {/* Passed / Failed counts */}
                    <div className="font-mono text-[11px] flex items-center gap-2">
                      <span className="text-emerald-400 font-semibold">{exec.passed} passed</span>
                      {exec.failed > 0 && (
                        <span className="text-rose-400 font-semibold bg-rose-500/10 border border-rose-500/30 px-1 py-0.2 rounded text-[10px]">
                          {exec.failed} failed
                        </span>
                      )}
                    </div>

                    {/* Progress Bar */}
                    <div className="w-28 hidden sm:block">
                      <div className="flex items-center justify-between text-[9px] font-mono text-slate-400 mb-0.5">
                        <span>Progress</span>
                        <span className="text-indigo-400 font-semibold">{exec.progress}%</span>
                      </div>
                      <div className="w-full bg-slate-800 h-1 rounded-full overflow-hidden">
                        <div
                          className="bg-indigo-500 h-full rounded-full transition-all duration-300"
                          style={{ width: `${exec.progress}%` }}
                        />
                      </div>
                    </div>

                    {/* Toggle Logs Button */}
                    <button
                      id={`btn-toggle-logs-${exec.id}`}
                      onClick={() => toggleExpand(exec.id)}
                      className="p-1 px-1.5 rounded bg-slate-800/80 hover:bg-slate-800 text-slate-300 text-[10px] font-mono flex items-center gap-1 border border-slate-700/80 transition-colors"
                      title="Inspect live console logs"
                    >
                      <Terminal className="w-3 h-3 text-indigo-400" />
                      <span className="hidden md:inline">Logs</span>
                      {isExpanded ? (
                        <ChevronUp className="w-2.5 h-2.5 text-slate-400" />
                      ) : (
                        <ChevronDown className="w-2.5 h-2.5 text-slate-400" />
                      )}
                    </button>
                  </div>
                </div>

                {/* Collapsible Live Log Inspector */}
                {isExpanded && (
                  <div
                    id={`logs-container-${exec.id}`}
                    className="px-4 py-2.5 bg-[#070708] border-t border-slate-800 font-mono text-[10px] text-slate-300 space-y-0.5"
                  >
                    <div className="flex items-center justify-between text-slate-500 text-[9px] pb-1 border-b border-slate-800 mb-1">
                      <span>RUNNER STDOUT STREAM &bull; WORKER PID #4182</span>
                      <span className="text-indigo-400 animate-pulse">&bull; STREAMING</span>
                    </div>
                    {exec.logs.map((log, idx) => (
                      <div
                        key={idx}
                        className={`leading-tight ${
                          log.includes("[FAIL]") || log.includes("failure")
                            ? "text-rose-400 font-semibold"
                            : log.includes("[WARN]")
                            ? "text-amber-300"
                            : "text-slate-400"
                        }`}
                      >
                        {log}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};

