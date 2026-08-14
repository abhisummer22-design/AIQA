import React from "react";
import {
  Sparkles,
  Bug,
  CheckCircle,
  XCircle,
  FileCheck2,
  Layers,
  ArrowRight,
  ShieldAlert
} from "lucide-react";
import { BugCandidate, ViewType } from "../../types";

interface BugIntelligenceProps {
  candidates: BugCandidate[];
  onReviewCandidate: (candidate: BugCandidate) => void;
  onCreateBug: (candidate: BugCandidate) => void;
  onIgnoreCandidate: (candidateId: string) => void;
  onNavigate: (view: ViewType) => void;
}

export const BugIntelligence: React.FC<BugIntelligenceProps> = ({
  candidates,
  onReviewCandidate,
  onCreateBug,
  onIgnoreCandidate,
  onNavigate
}) => {
  const pendingCandidates = candidates.filter((c) => c.status !== "created");

  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded overflow-hidden">
      {/* Header */}
      <div className="px-3.5 py-2.5 border-b border-slate-800 bg-slate-900/40 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
          <h2 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
            AI Bug Intelligence
          </h2>
          <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-indigo-950 text-indigo-300 border border-indigo-800/60 font-semibold">
            {pendingCandidates.length} Pending
          </span>
        </div>
        <button
          onClick={() => onNavigate("bugs")}
          className="text-[11px] text-indigo-400 hover:text-indigo-300 font-medium font-mono"
        >
          All Bugs &rarr;
        </button>
      </div>

      {/* Candidates List */}
      <div className="divide-y divide-slate-800/60">
        {pendingCandidates.length === 0 ? (
          <div className="p-6 text-center text-slate-500 text-[11px] font-mono">
            All AI bug candidates have been triaged and reviewed.
          </div>
        ) : (
          pendingCandidates.map((cand) => (
            <div key={cand.id} className="p-3 hover:bg-slate-900/30 transition-colors space-y-2">
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-1.5">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <span className="text-xs font-bold text-slate-200 font-mono">{cand.title}</span>
                  <span
                    className={`text-[9px] font-mono px-1.5 py-0.2 rounded border font-semibold uppercase ${
                      cand.severity === "Critical"
                        ? "bg-rose-500/10 text-rose-400 border-rose-500/30"
                        : "bg-amber-500/10 text-amber-400 border-amber-500/30"
                    }`}
                  >
                    {cand.severity}
                  </span>
                  <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 font-semibold">
                    {cand.confidence}% Match
                  </span>
                  <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 border border-slate-700">
                    {cand.suggestedPriority}
                  </span>
                </div>
                <span className="text-[10px] text-slate-500 font-mono shrink-0">{cand.createdAt}</span>
              </div>

              <p className="text-[11px] text-slate-300 font-sans leading-relaxed">
                {cand.description}
              </p>

              {/* Metadata & Confirmation Actions */}
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pt-0.5">
                <div className="flex items-center gap-2.5 text-[10px] text-slate-400 font-mono">
                  <span>Service: <strong className="text-slate-200">{cand.relatedService}</strong></span>
                  <span>&bull;</span>
                  <span>Evidence: <strong className="text-indigo-400">{cand.evidenceCount} artifacts</strong></span>
                </div>

                <div className="flex items-center gap-1.5 shrink-0">
                  <button
                    onClick={() => onReviewCandidate(cand)}
                    className="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-200 text-[10px] font-mono font-medium border border-slate-700 transition-colors"
                  >
                    Review Details
                  </button>
                  <button
                    onClick={() => onIgnoreCandidate(cand.id)}
                    className="px-2 py-0.5 rounded bg-slate-900 hover:bg-slate-800 text-slate-400 hover:text-slate-300 text-[10px] font-mono border border-slate-800 transition-colors"
                  >
                    Ignore
                  </button>
                  <button
                    onClick={() => onCreateBug(cand)}
                    className="px-2.5 py-0.5 rounded bg-emerald-950 hover:bg-emerald-900 text-emerald-200 text-[10px] font-mono font-bold border border-emerald-800 transition-colors shadow-sm"
                  >
                    Confirm Bug
                  </button>
                </div>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};

