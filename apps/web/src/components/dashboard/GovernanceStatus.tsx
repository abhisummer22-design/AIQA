import React from "react";
import { ShieldCheck, AlertOctagon, Lock, ShieldAlert, ArrowRight } from "lucide-react";
import { GovernancePolicy, ViewType } from "../../types";

interface GovernanceStatusProps {
  policies: GovernancePolicy[];
  onNavigate: (view: ViewType) => void;
}

export const GovernanceStatus: React.FC<GovernanceStatusProps> = ({ policies, onNavigate }) => {
  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 space-y-2.5">
      {/* Header */}
      <div className="flex items-center justify-between pb-2 border-b border-slate-800">
        <div className="flex items-center gap-2">
          <ShieldCheck className="w-3.5 h-3.5 text-indigo-400" />
          <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
            Governance & Policies
          </h3>
        </div>
        <button
          onClick={() => onNavigate("policies")}
          className="text-[10px] font-mono text-indigo-400 hover:underline"
        >
          Manage &rarr;
        </button>
      </div>

      {/* Policy highlights */}
      <div className="space-y-2">
        {policies.slice(0, 3).map((policy) => (
          <div
            key={policy.id}
            className="p-2.5 rounded bg-[#070708] border border-slate-800 flex items-center justify-between gap-2.5 text-xs font-mono"
          >
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="font-bold text-slate-200 truncate">{policy.name}</span>
                <span className="text-[9px] px-1.5 py-0.2 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 font-semibold uppercase">
                  {policy.status}
                </span>
              </div>
              <p className="text-[10px] text-slate-400 truncate mt-0.5">{policy.description}</p>
            </div>

            <div className="shrink-0 text-right">
              <span
                className={`text-[10px] font-bold px-1.5 py-0.2 rounded ${
                  policy.violations > 0
                    ? "bg-rose-500/10 text-rose-400 border border-rose-500/30"
                    : "bg-slate-800 text-slate-400"
                }`}
              >
                {policy.violations} Violations
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

