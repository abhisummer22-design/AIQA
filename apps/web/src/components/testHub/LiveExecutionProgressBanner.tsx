import React from "react";
import { Play, CheckCircle2, XCircle, Clock, Square, Terminal, Loader2 } from "lucide-react";

interface LiveExecutionProgressBannerProps {
  isRunning: boolean;
  targetScope: string;
  total: number;
  completed: number;
  passed: number;
  failed: number;
  currentLog?: string;
  onCancel: () => void;
}

export const LiveExecutionProgressBanner: React.FC<LiveExecutionProgressBannerProps> = ({
  isRunning,
  targetScope,
  total,
  completed,
  passed,
  failed,
  currentLog,
  onCancel
}) => {
  if (!isRunning) return null;

  const percent = total > 0 ? Math.round((completed / total) * 100) : 0;

  return (
    <div className="bg-[#0b121e] border-2 border-cyan-500/80 rounded-xl p-4 shadow-lg font-mono space-y-3 animate-in fade-in duration-200">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-cyan-950 border border-cyan-700 text-cyan-400">
            <Loader2 className="w-5 h-5 animate-spin text-cyan-400" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="text-xs uppercase font-bold text-cyan-400 tracking-wider">
                EXECUTION IN PROGRESS
              </span>
              <span className="text-[10px] px-2 py-0.5 rounded bg-cyan-950 text-cyan-200 border border-cyan-800 font-bold uppercase">
                {targetScope}
              </span>
              <span className="text-[10px] text-slate-400">
                {completed} of {total} test cases dispatched
              </span>
            </div>
            <p className="text-xs text-slate-300 font-bold mt-0.5">
              Distributed runner executing test specifications on cluster workers...
            </p>
          </div>
        </div>

        <div className="flex items-center gap-3 shrink-0">
          <div className="flex items-center gap-2 text-xs font-bold">
            <span className="text-emerald-400 flex items-center gap-1">
              <CheckCircle2 className="w-3.5 h-3.5" />
              {passed} Passed
            </span>
            {failed > 0 && (
              <span className="text-rose-400 flex items-center gap-1">
                <XCircle className="w-3.5 h-3.5" />
                {failed} Failed
              </span>
            )}
          </div>

          <button
            onClick={onCancel}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-rose-950/80 hover:bg-rose-900 border border-rose-700 text-rose-200 text-xs font-bold transition-colors"
          >
            <Square className="w-3.5 h-3.5 fill-current" />
            <span>Cancel Run</span>
          </button>
        </div>
      </div>

      {/* Progress Bar */}
      <div className="space-y-1">
        <div className="flex items-center justify-between text-[11px] text-slate-400">
          <span>Cluster Progress: {percent}%</span>
          <span>Worker Pool: 4 Active Nodes</span>
        </div>
        <div className="w-full bg-slate-900 h-2.5 rounded-full overflow-hidden border border-slate-800">
          <div
            className="h-full bg-gradient-to-r from-cyan-500 via-sky-400 to-emerald-400 transition-all duration-300 rounded-full"
            style={{ width: `${percent}%` }}
          />
        </div>
      </div>

      {/* Live Log Stream Ticker */}
      {currentLog && (
        <div className="p-2 rounded bg-black/60 border border-slate-800 text-[11px] text-slate-300 flex items-center gap-2 overflow-hidden truncate">
          <Terminal className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
          <span className="text-slate-500 shrink-0 font-bold">[STDOUT]</span>
          <span className="truncate">{currentLog}</span>
        </div>
      )}
    </div>
  );
};
