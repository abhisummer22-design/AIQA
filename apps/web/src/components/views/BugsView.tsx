import React from "react";
import { Bug, Sparkles, CheckCircle2, AlertOctagon, Plus, ExternalLink, ShieldAlert } from "lucide-react";
import { BugCandidate } from "../../types";

interface BugsViewProps {
  candidates: BugCandidate[];
  onReviewCandidate: (cand: BugCandidate) => void;
  onCreateBug: (cand: BugCandidate) => void;
}

const activeBugs = [
  { id: "BUG-892", title: "PostgreSQL Deadlock in concurrent checkout session creation", service: "Payment Gateway", severity: "Critical", priority: "P0", status: "In Progress", reporter: "AI Auto-Triage", jira: "JIRA-4021" },
  { id: "BUG-889", title: "Missing CORS header on /v1/search/rag-query with preflight OPTIONS", service: "Search Pipeline", severity: "High", priority: "P1", status: "Triage", reporter: "Schemathesis Fuzzer", jira: "JIRA-4018" },
  { id: "BUG-882", title: "RAG hallucination on unindexed terms in medical corpus", service: "Apex Agent", severity: "Medium", priority: "P2", status: "Resolved", reporter: "DeepEval", jira: "JIRA-3990" },
  { id: "BUG-875", title: "Ollama worker memory leak after 10k sequential inferences", service: "Model Runtime", severity: "High", priority: "P1", status: "Open", reporter: "Chaos Monkey", jira: "JIRA-3972" }
];

export const BugsView: React.FC<BugsViewProps> = ({
  candidates,
  onReviewCandidate,
  onCreateBug
}) => {
  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white flex items-center gap-2.5">
            <Bug className="w-5 h-5 text-rose-400" />
            Defect Tracking &amp; AI Bug Triage
          </h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            Automated bug candidate detection, human confirmation gate, and synchronized issue tracker
          </p>
        </div>
      </div>

      {/* AI Draft Candidates Queue */}
      <div className="bg-[#0e1422] border border-cyan-900/60 rounded-xl overflow-hidden shadow-sm">
        <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between bg-cyan-950/20">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-cyan-400" />
            <h2 className="text-xs font-bold font-mono uppercase text-cyan-300">
              AI Proposed Bug Candidates (Requires Human Confirmation)
            </h2>
          </div>
          <span className="text-xs font-mono text-cyan-400 font-bold">
            {candidates.filter((c) => c.status !== "created").length} Pending
          </span>
        </div>

        <div className="divide-y divide-slate-800/60 font-mono text-xs">
          {candidates.filter((c) => c.status !== "created").map((cand) => (
            <div key={cand.id} className="p-4 hover:bg-slate-900/40 transition-colors flex flex-col md:flex-row md:items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span className="font-bold text-slate-100">{cand.title}</span>
                  <span className="text-[10px] px-1.5 py-0.2 rounded bg-rose-950 text-rose-300 border border-rose-800 font-semibold">
                    {cand.severity}
                  </span>
                  <span className="text-[10px] px-1.5 py-0.2 rounded bg-cyan-950 text-cyan-300 border border-cyan-800 font-semibold">
                    {cand.confidence}% Confidence
                  </span>
                </div>
                <p className="text-slate-400 text-[11px] font-sans mt-1 max-w-2xl">{cand.description}</p>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                <button
                  onClick={() => onReviewCandidate(cand)}
                  className="px-3 py-1.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-200"
                >
                  Review
                </button>
                <button
                  onClick={() => onCreateBug(cand)}
                  className="px-3 py-1.5 rounded bg-emerald-950 hover:bg-emerald-900 text-emerald-200 font-bold border border-emerald-800"
                >
                  Confirm &amp; Create
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Active Bugs List */}
      <div className="bg-[#0e1422] border border-slate-800/90 rounded-xl overflow-hidden shadow-sm">
        <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
          <h2 className="text-xs font-bold font-mono uppercase text-slate-200">
            Active Confirmed Defects (Synced to Jira / Linear)
          </h2>
          <span className="text-xs font-mono text-slate-400">{activeBugs.length} Tracked</span>
        </div>

        <div className="divide-y divide-slate-800/60 font-mono text-xs">
          {activeBugs.map((b) => (
            <div key={b.id} className="p-4 hover:bg-slate-900/40 transition-colors flex flex-col md:flex-row md:items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-rose-400 font-bold">{b.id}</span>
                  <span className="font-bold text-slate-100">{b.title}</span>
                  <span className="text-[10px] px-1.5 py-0.2 rounded bg-slate-800 text-slate-300">
                    {b.priority}
                  </span>
                </div>
                <div className="text-slate-400 text-[11px] mt-1 flex items-center gap-3">
                  <span>Service: {b.service}</span>
                  <span>&bull;</span>
                  <span>Reporter: {b.reporter}</span>
                  <span>&bull;</span>
                  <span className="text-cyan-400 flex items-center gap-1">
                    <ExternalLink className="w-3 h-3" /> {b.jira}
                  </span>
                </div>
              </div>

              <span
                className={`text-[10px] px-2 py-0.5 rounded font-bold border ${
                  b.status === "Resolved"
                    ? "bg-emerald-950 text-emerald-300 border-emerald-800"
                    : "bg-amber-950 text-amber-300 border-amber-800"
                }`}
              >
                {b.status}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
