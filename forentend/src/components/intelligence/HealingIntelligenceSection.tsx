/**
 * Phase 7: Healing Intelligence & Safety Governance Section
 * Displays strategy reliability metrics and enforces explicit governance chain visibility.
 */

import React from 'react';
import { Wrench, Shield, CheckCircle2, AlertTriangle, ArrowRight, Lock, Check } from 'lucide-react';
import type { HealingStrategyMetricsRow } from '../../types/contract';

interface HealingIntelligenceSectionProps {
  strategies: HealingStrategyMetricsRow[];
  loading?: boolean;
}

export const HealingIntelligenceSection: React.FC<HealingIntelligenceSectionProps> = ({
  strategies,
  loading,
}) => {
  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5 animate-pulse" data-testid="healing-intel-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="h-16 bg-slate-800/40 rounded mb-4" />
        <div className="space-y-2">
          {[...Array(3)].map((_, i) => (
            <div key={i} className="h-12 bg-slate-800/60 rounded" />
          ))}
        </div>
      </div>
    );
  }

  const governanceSteps = [
    { label: 'Intelligence', desc: 'Failure analysis & signal detection' },
    { label: 'Recommendation', desc: 'Synthesized strategy proposal' },
    { label: 'Plan Validation', desc: 'Deterministic AST verification' },
    { label: 'Policy Gate', desc: 'SSRF & safety boundary checks' },
    { label: 'RBAC Check', desc: 'Operator role enforcement' },
    { label: 'Approval', desc: 'Human authorization gate' },
    { label: 'Execution', desc: 'Bounded single-attempt run' },
  ];

  return (
    <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5" data-testid="healing-intelligence-section">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-semibold text-slate-100 flex items-center gap-2">
            <Wrench className="w-4 h-4 text-indigo-400" />
            <span>Healing Strategy Reliability & Governance</span>
            <span className="text-xs px-2 py-0.5 rounded-full bg-slate-800 text-slate-300 font-mono">
              {strategies.length} {strategies.length === 1 ? 'strategy' : 'strategies'}
            </span>
          </h3>
          <p className="text-xs text-slate-400 mt-0.5">
            Calibrated empirical confidence and safety metrics for AST healing proposals.
          </p>
        </div>
      </div>

      {/* Safety Governance Architecture Banner */}
      <div
        className="mb-6 p-4 rounded-xl border border-indigo-500/20 bg-indigo-950/20"
        data-testid="healing-governance-flow"
      >
        <div className="flex items-center gap-2 mb-3">
          <Shield className="w-4 h-4 text-indigo-400" />
          <span className="text-xs font-semibold text-indigo-200 uppercase tracking-wider">
            Governed Self-Healing Pipeline
          </span>
          <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-indigo-500/10 text-indigo-300 border border-indigo-500/30 ml-auto flex items-center gap-1">
            <Lock className="w-3 h-3" /> Strict Human Gate Enforced
          </span>
        </div>

        {/* Steps Flow */}
        <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-7 gap-2">
          {governanceSteps.map((step, idx) => (
            <div
              key={idx}
              className="bg-slate-950/80 border border-indigo-900/50 p-2.5 rounded-lg flex flex-col justify-between relative"
            >
              <div className="flex items-center justify-between mb-1">
                <span className="text-[10px] font-mono text-indigo-400 font-bold">0{idx + 1}</span>
                {idx < governanceSteps.length - 1 && (
                  <ArrowRight className="w-3 h-3 text-indigo-600 hidden lg:block -mr-1" />
                )}
              </div>
              <div className="text-xs font-semibold text-slate-200">{step.label}</div>
              <div className="text-[10px] text-slate-400 mt-0.5 leading-tight">{step.desc}</div>
            </div>
          ))}
        </div>

        <div className="mt-3 text-[11px] text-indigo-300/80 bg-slate-950/60 p-2.5 rounded border border-indigo-900/40">
          <strong>Safety Boundary:</strong> All healing recommendations require AST diff review, plan validation,
          and operator sign-off before application. No unverified autonomous code alteration is permitted.
        </div>
      </div>

      {/* Strategies Table */}
      {strategies.length === 0 ? (
        <div className="py-12 text-center text-slate-400 text-xs border border-dashed border-slate-800/80 rounded-lg" data-testid="healing-strategies-empty">
          No healing strategy history recorded yet for this project.
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs text-slate-300">
            <thead className="bg-slate-950/60 text-slate-400 uppercase font-mono text-[10px] border-b border-slate-800">
              <tr>
                <th className="py-2.5 px-3">Strategy Name</th>
                <th className="py-2.5 px-3">Category</th>
                <th className="py-2.5 px-3">Attempts</th>
                <th className="py-2.5 px-3">Verified</th>
                <th className="py-2.5 px-3">Not Verified</th>
                <th className="py-2.5 px-3">Regressions</th>
                <th className="py-2.5 px-3 text-right">Calibrated Confidence</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60 font-mono">
              {strategies.map((row) => {
                const confPct = Math.round(row.calibrated_confidence * 100);
                const isHighConf = row.calibrated_confidence >= 0.7;
                return (
                  <tr key={row.id} className="hover:bg-slate-800/30 transition-colors" data-testid={`strategy-row-${row.id}`}>
                    <td className="py-2.5 px-3 font-semibold text-slate-100 font-sans">{row.strategy}</td>
                    <td className="py-2.5 px-3">
                      <span className="text-[11px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-300 border border-slate-700">
                        {row.failure_category}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">{row.attempts_count}</td>
                    <td className="py-2.5 px-3 text-emerald-400 font-bold">{row.verified_count}</td>
                    <td className="py-2.5 px-3 text-slate-400">{row.not_verified_count}</td>
                    <td className="py-2.5 px-3">
                      <span
                        className={`font-bold ${
                          row.regression_count > 0 ? 'text-rose-400' : 'text-slate-400'
                        }`}
                      >
                        {row.regression_count}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-right">
                      <span
                        className={`inline-flex items-center gap-1 px-2 py-0.5 rounded font-bold text-[11px] ${
                          isHighConf
                            ? 'bg-teal-500/10 text-teal-400 border border-teal-500/20'
                            : 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                        }`}
                      >
                        {confPct}%
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};
