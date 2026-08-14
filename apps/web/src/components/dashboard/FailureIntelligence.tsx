import React from "react";
import {
  AlertOctagon,
  Sparkles,
  FileCheck2,
  ArrowRight,
  ShieldAlert,
  Clock,
  Terminal
} from "lucide-react";
import { FailureItem, ViewType } from "../../types";

interface FailureIntelligenceProps {
  failures: FailureItem[];
  onDraftBug: (failure: FailureItem) => void;
  onViewEvidence?: (failure: FailureItem) => void;
  onNavigate: (view: ViewType) => void;
}

export const FailureIntelligence: React.FC<FailureIntelligenceProps> = ({
  failures,
  onDraftBug,
  onViewEvidence,
  onNavigate
}) => {
  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded overflow-hidden">
      {/* Header */}
      <div className="px-3.5 py-2.5 border-b border-slate-800 bg-slate-900/40 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <AlertOctagon className="w-3.5 h-3.5 text-rose-400" />
          <h2 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
            Recent Failure Intelligence
          </h2>
          <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-rose-950 text-rose-300 border border-rose-800/60 font-semibold">
            {failures.length} Classified
          </span>
        </div>
        <button
          onClick={() => onNavigate("test-runs")}
          className="text-[11px] text-indigo-400 hover:text-indigo-300 font-medium font-mono"
        >
          View All &rarr;
        </button>
      </div>

      {/* Failure Cards List */}
      <div className="divide-y divide-slate-800/60">
        {failures.map((fail) => (
          <div key={fail.id} className="p-3 hover:bg-slate-900/30 transition-colors space-y-2">
            {/* Top row */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-1.5">
              <div className="flex items-center gap-1.5 flex-wrap">
                <span className="text-xs font-bold text-slate-200 font-mono">{fail.testName}</span>
                <span
                  className={`text-[9px] font-mono px-1.5 py-0.2 rounded border font-semibold uppercase ${
                    fail.riskLevel === "Critical"
                      ? "bg-rose-500/10 text-rose-400 border-rose-500/30"
                      : fail.riskLevel === "High"
                      ? "bg-amber-500/10 text-amber-400 border-amber-500/30"
                      : "bg-indigo-500/10 text-indigo-400 border-indigo-500/30"
                  }`}
                >
                  {fail.riskLevel} Risk
                </span>
                <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 border border-slate-700">
                  {fail.failureType}
                </span>
                {fail.httpStatus && (
                  <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-rose-950/60 text-rose-400 border border-rose-800/40">
                    HTTP {fail.httpStatus}
                  </span>
                )}
              </div>
              <span className="text-[10px] text-slate-500 font-mono shrink-0">{fail.timestamp} &bull; {fail.environment}</span>
            </div>

            {/* Error message box */}
            <div className="p-2 rounded bg-[#070708] border border-slate-800 text-[11px] font-mono text-rose-300/90 leading-relaxed overflow-x-auto">
              {fail.errorMessage}
            </div>

            {/* Suggested Remediation & Actions */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pt-0.5">
              <div className="text-[10px] text-slate-400 font-mono flex items-center gap-1">
                <span className="text-indigo-400 font-semibold">Suggested Action:</span>
                <span className="text-slate-300">{fail.suggestedAction}</span>
              </div>

              <div className="flex items-center gap-1.5 shrink-0">
                {onViewEvidence && (
                  <button
                    onClick={() => onViewEvidence(fail)}
                    className="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 text-[10px] font-mono flex items-center gap-1 border border-slate-700 transition-colors"
                  >
                    <FileCheck2 className="w-2.5 h-2.5 text-indigo-400" />
                    <span>Evidence</span>
                  </button>
                )}
                <button
                  onClick={() => onDraftBug(fail)}
                  className="px-2 py-0.5 rounded bg-rose-950/80 hover:bg-rose-900 text-rose-200 text-[10px] font-mono font-semibold flex items-center gap-1 border border-rose-800 transition-colors shadow-sm"
                >
                  <Sparkles className="w-2.5 h-2.5 text-rose-400" />
                  <span>Draft AI Bug</span>
                </button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

