import React from "react";
import { ShieldAlert, AlertTriangle, AlertCircle, CheckCircle2, TrendingUp, Info } from "lucide-react";
import { KpiMetrics } from "../../types";

interface RiskAnalyticsProps {
  metrics: KpiMetrics;
}

export const RiskAnalytics: React.FC<RiskAnalyticsProps> = ({ metrics }) => {
  const risk = metrics.riskScore;
  const totalRisks = risk.criticalCount + risk.highCount + risk.mediumCount + risk.lowCount;

  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 flex flex-col justify-between">
      <div>
        <div className="flex items-center justify-between pb-2 border-b border-slate-800">
          <div className="flex items-center gap-2">
            <ShieldAlert className="w-3.5 h-3.5 text-amber-400" />
            <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
              Risk Distribution
            </h3>
          </div>
          <span className="text-[10px] font-mono font-bold px-1.5 py-0.2 rounded bg-amber-950/80 text-amber-300 border border-amber-800/60">
            {risk.level} Profile
          </span>
        </div>

        {/* Breakdown bars */}
        <div className="mt-3 space-y-2.5">
          {/* Critical */}
          <div>
            <div className="flex items-center justify-between text-[11px] font-mono mb-1">
              <span className="text-rose-400 font-semibold flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-rose-500" /> Critical Risk
              </span>
              <span className="text-slate-300 font-bold">{risk.criticalCount} services</span>
            </div>
            <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
              <div
                className="bg-rose-500 h-full rounded-full transition-all"
                style={{ width: `${(risk.criticalCount / totalRisks) * 100}%` }}
              />
            </div>
          </div>

          {/* High */}
          <div>
            <div className="flex items-center justify-between text-[11px] font-mono mb-1">
              <span className="text-amber-400 font-semibold flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-amber-500" /> High Risk
              </span>
              <span className="text-slate-300 font-bold">{risk.highCount} services</span>
            </div>
            <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
              <div
                className="bg-amber-500 h-full rounded-full transition-all"
                style={{ width: `${(risk.highCount / totalRisks) * 100}%` }}
              />
            </div>
          </div>

          {/* Medium */}
          <div>
            <div className="flex items-center justify-between text-[11px] font-mono mb-1">
              <span className="text-indigo-400 font-semibold flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-indigo-500" /> Medium Risk
              </span>
              <span className="text-slate-300 font-bold">{risk.mediumCount} services</span>
            </div>
            <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
              <div
                className="bg-indigo-500 h-full rounded-full transition-all"
                style={{ width: `${(risk.mediumCount / totalRisks) * 100}%` }}
              />
            </div>
          </div>

          {/* Low */}
          <div>
            <div className="flex items-center justify-between text-[11px] font-mono mb-1">
              <span className="text-slate-400 font-semibold flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" /> Low Risk / Hardened
              </span>
              <span className="text-slate-300 font-bold">{risk.lowCount} services</span>
            </div>
            <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
              <div
                className="bg-emerald-500 h-full rounded-full transition-all"
                style={{ width: `${(risk.lowCount / totalRisks) * 100}%` }}
              />
            </div>
          </div>
        </div>
      </div>

      <div className="mt-3 pt-2.5 border-t border-slate-800 text-[10px] text-slate-400 font-mono flex items-center justify-between">
        <span>Production Gate:</span>
        <span className="text-amber-400 font-semibold">Conditional Approval</span>
      </div>
    </div>
  );
};

