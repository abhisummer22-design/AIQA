import React from "react";
import { Flame, ShieldAlert, RefreshCw, CheckCircle2, ArrowUpRight, ShieldCheck } from "lucide-react";
import { FlakyTestItem, ViewType } from "../../types";

interface FlakyTestsProps {
  flakyTests: FlakyTestItem[];
  onToggleQuarantine: (testId: string) => void;
  onNavigate: (view: ViewType) => void;
}

export const FlakyTests: React.FC<FlakyTestsProps> = ({
  flakyTests,
  onToggleQuarantine,
  onNavigate
}) => {
  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 flex flex-col justify-between">
      <div>
        <div className="flex items-center justify-between pb-2 border-b border-slate-800">
          <div className="flex items-center gap-2">
            <Flame className="w-3.5 h-3.5 text-rose-400" />
            <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
              High Flakiness Tests
            </h3>
          </div>
          <button
            onClick={() => onNavigate("risk-flakiness")}
            className="text-[10px] font-mono text-indigo-400 hover:underline flex items-center gap-1"
          >
            <span>Deep Analysis &rarr;</span>
          </button>
        </div>

        {/* Flaky tests list */}
        <div className="mt-2.5 divide-y divide-slate-800/60">
          {flakyTests.slice(0, 3).map((test) => (
            <div key={test.id} className="py-2 flex items-center justify-between gap-2.5">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-slate-200 truncate">{test.testName}</span>
                  {test.quarantined && (
                    <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-amber-950 text-amber-300 border border-amber-800/80 font-semibold">
                      Quarantined
                    </span>
                  )}
                </div>
                <div className="text-[10px] text-slate-400 font-mono mt-0.5 flex items-center gap-2.5">
                  <span className="text-rose-400 font-bold">{test.flakinessPercent}% Flaky</span>
                  <span>&bull;</span>
                  <span>{test.failureFrequency}</span>
                  <span>&bull;</span>
                  <span>{test.retries} Retries</span>
                </div>
              </div>

              {/* Quarantine Action Button */}
              <button
                onClick={() => onToggleQuarantine(test.id)}
                title={test.quarantined ? "Unquarantine test" : "Quarantine test from blocking PRs"}
                className={`px-2 py-0.5 rounded text-[10px] font-mono font-medium transition-colors shrink-0 ${
                  test.quarantined
                    ? "bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700"
                    : "bg-amber-950/60 hover:bg-amber-900/80 text-amber-300 border border-amber-800/80"
                }`}
              >
                {test.quarantined ? "Unquarantine" : "Quarantine"}
              </button>
            </div>
          ))}
        </div>
      </div>

      <div className="mt-2.5 pt-2 border-t border-slate-800 text-[10px] text-slate-400 font-mono flex items-center justify-between">
        <span>Auto-retry heuristics:</span>
        <span className="text-slate-200 font-semibold">Exponential Backoff (Max 3)</span>
      </div>
    </div>
  );
};

