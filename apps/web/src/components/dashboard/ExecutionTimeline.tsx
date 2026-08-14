import React from "react";
import { Clock, CheckCircle2, AlertTriangle, XCircle, Sparkles, Network, ShieldCheck, Activity } from "lucide-react";
import { TimelineEvent, ViewType } from "../../types";

interface ExecutionTimelineProps {
  timeline: TimelineEvent[];
  onNavigate: (view: ViewType) => void;
}

export const ExecutionTimeline: React.FC<ExecutionTimelineProps> = ({ timeline, onNavigate }) => {
  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 space-y-2.5">
      {/* Header */}
      <div className="flex items-center justify-between pb-2 border-b border-slate-800">
        <div className="flex items-center gap-2">
          <Clock className="w-3.5 h-3.5 text-indigo-400" />
          <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
            Execution Stream
          </h3>
        </div>
        <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 font-semibold animate-pulse">
          LIVE
        </span>
      </div>

      {/* Timeline Stream */}
      <div className="space-y-2 relative before:absolute before:left-2 before:top-2 before:bottom-2 before:w-[1px] before:bg-slate-800">
        {timeline.slice(0, 6).map((item, idx) => (
          <div key={idx} className="relative flex items-start gap-2.5 pl-5 text-xs font-mono">
            {/* Dot marker */}
            <span
              className={`absolute left-1.5 top-1.5 w-1.5 h-1.5 rounded-full ring-2 ring-[#0D0D0E] ${
                item.level === "error"
                  ? "bg-rose-400"
                  : item.level === "warn"
                  ? "bg-amber-400"
                  : "bg-indigo-400"
              }`}
            />
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between">
                <span className="font-bold text-slate-200 text-[11px]">{item.event}</span>
                <span className="text-[9px] text-slate-500">{item.time}</span>
              </div>
              <p className="text-[10px] text-slate-400 truncate mt-0.5">{item.detail}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

