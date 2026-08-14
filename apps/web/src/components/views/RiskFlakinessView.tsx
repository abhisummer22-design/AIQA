import React from "react";
import { Flame, ShieldAlert, CheckCircle2, RefreshCw, AlertTriangle, ShieldCheck } from "lucide-react";
import { FlakyTestItem, KpiMetrics } from "../../types";
import { RiskAnalytics } from "../dashboard/RiskAnalytics";

interface RiskFlakinessViewProps {
  metrics: KpiMetrics;
  flakyTests: FlakyTestItem[];
  onToggleQuarantine: (id: string) => void;
}

export const RiskFlakinessView: React.FC<RiskFlakinessViewProps> = ({
  metrics,
  flakyTests,
  onToggleQuarantine
}) => {
  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white flex items-center gap-2.5">
            <Flame className="w-5 h-5 text-amber-400" />
            Risk &amp; Flakiness Intelligence
          </h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            Predictive flakiness heuristics, auto-quarantine policies, and service risk distribution
          </p>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <RiskAnalytics metrics={metrics} />

        <div className="p-5 rounded-xl bg-[#0e1422] border border-slate-800/90 space-y-4">
          <div className="flex items-center justify-between pb-2 border-b border-slate-800">
            <h3 className="text-xs font-bold font-mono uppercase text-slate-100">
              Quarantine Heuristics Policy
            </h3>
            <span className="text-xs font-mono text-emerald-400">ACTIVE AUTO-ENFORCE</span>
          </div>

          <div className="space-y-3 text-xs font-mono text-slate-300">
            <div className="p-3 rounded-lg bg-[#080d16] border border-slate-800">
              <div className="font-bold text-slate-100">Automatic Quarantine Threshold</div>
              <p className="text-[11px] text-slate-400 font-sans mt-0.5">
                Tests that fail &gt;3 times non-deterministically across 10 consecutive PR runs are automatically isolated from blocking release merges.
              </p>
            </div>

            <div className="p-3 rounded-lg bg-[#080d16] border border-slate-800">
              <div className="font-bold text-slate-100">Graduation Criteria</div>
              <p className="text-[11px] text-slate-400 font-sans mt-0.5">
                Quarantined tests must pass 25 consecutive staging runs in isolated shadow containers to automatically regain gating status.
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Flaky tests list */}
      <div className="bg-[#0e1422] border border-slate-800/90 rounded-xl overflow-hidden shadow-sm">
        <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
          <h2 className="text-xs font-bold font-mono uppercase text-slate-200">
            Identified Flaky Test Suites
          </h2>
          <span className="text-xs font-mono text-rose-400">{flakyTests.length} Under Observation</span>
        </div>

        <div className="divide-y divide-slate-800/60 font-mono text-xs">
          {flakyTests.map((t) => (
            <div key={t.id} className="p-4 hover:bg-slate-900/40 transition-colors flex flex-col md:flex-row md:items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span className="font-bold text-slate-100">{t.testName}</span>
                  {t.quarantined && (
                    <span className="text-[10px] px-1.5 py-0.2 rounded bg-amber-950 text-amber-300 border border-amber-800 font-bold">
                      QUARANTINED
                    </span>
                  )}
                </div>
                <div className="text-slate-400 text-[11px] mt-1 flex items-center gap-3">
                  <span className="text-rose-400 font-bold">{t.flakinessPercent}% Flaky</span>
                  <span>&bull;</span>
                  <span>Frequency: {t.failureFrequency}</span>
                  <span>&bull;</span>
                  <span>Last Failure: {t.lastFailure}</span>
                  <span>&bull;</span>
                  <span>Retries: {t.retries}</span>
                </div>
              </div>

              <button
                onClick={() => onToggleQuarantine(t.id)}
                className={`px-3 py-1.5 rounded font-mono font-medium text-xs transition-colors ${
                  t.quarantined
                    ? "bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700"
                    : "bg-amber-950 hover:bg-amber-900 text-amber-200 border border-amber-800"
                }`}
              >
                {t.quarantined ? "Restore to Gate" : "Quarantine Test"}
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
