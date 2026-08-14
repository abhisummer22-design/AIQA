import React from "react";
import { ShieldCheck, Plus, AlertOctagon, CheckCircle2, Lock, ShieldAlert } from "lucide-react";
import { GovernancePolicy } from "../../types";

interface PoliciesViewProps {
  policies: GovernancePolicy[];
}

export const PoliciesView: React.FC<PoliciesViewProps> = ({ policies }) => {
  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white flex items-center gap-2.5">
            <ShieldCheck className="w-5 h-5 text-cyan-400" />
            Quality Governance &amp; CI/CD Gate Policies
          </h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            Enforce compliance rules, SLA thresholds, security scan mandates, and production release blockers
          </p>
        </div>
        <button className="flex items-center gap-2 px-4 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-mono font-semibold shadow-sm">
          <Plus className="w-4 h-4" />
          <span>Add Custom Policy</span>
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {policies.map((p) => (
          <div
            key={p.id}
            className="p-5 rounded-xl bg-[#0e1422] border border-slate-800/90 space-y-3.5 shadow-sm"
          >
            <div className="flex items-start justify-between">
              <div>
                <h3 className="font-bold text-sm text-slate-100">{p.name}</h3>
                <p className="text-xs text-slate-400 font-sans mt-1">{p.description}</p>
              </div>
              <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-emerald-950 text-emerald-300 border border-emerald-800 font-bold">
                {p.status}
              </span>
            </div>

            <div className="p-3 rounded-lg bg-[#080d16] border border-slate-800 flex items-center justify-between text-xs font-mono">
              <span className="text-slate-400">Target Environment:</span>
              <span className="text-slate-200 font-bold">{p.targetEnv}</span>
            </div>

            <div className="flex items-center justify-between pt-1 text-xs font-mono">
              <span
                className={`font-bold ${
                  p.violations > 0 ? "text-rose-400" : "text-emerald-400"
                }`}
              >
                {p.violations} Blocking Violations
              </span>
              <button className="text-cyan-400 hover:underline">Edit Rule &rarr;</button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
