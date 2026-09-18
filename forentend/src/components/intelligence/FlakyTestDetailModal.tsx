/**
 * Phase 7: Flaky Test Detail Modal
 * Shows deep drilldown on a flaky test with execution window history and environment correlation.
 */

import React from 'react';
import { X, AlertTriangle, CheckCircle2, XCircle, Layers, Activity } from 'lucide-react';
import type { FlakinessMetricsRow } from '../../types/contract';

interface FlakyTestDetailModalProps {
  metrics: FlakinessMetricsRow | null;
  onClose: () => void;
}

export const FlakyTestDetailModal: React.FC<FlakyTestDetailModalProps> = ({ metrics, onClose }) => {
  if (!metrics) return null;

  const scorePct = Math.round(metrics.flakiness_score * 100);
  const isHigh = metrics.flakiness_score >= 0.4;
  const isModerate = metrics.flakiness_score >= 0.15 && metrics.flakiness_score < 0.4;

  // Synthesize historical outcome dots based on window_runs and window_failures for visualization
  const historyOutcomes = [];
  const failCount = metrics.window_failures;
  const passCount = Math.max(0, metrics.window_runs - failCount);
  for (let i = 0; i < failCount; i++) historyOutcomes.push('FAIL');
  for (let i = 0; i < passCount; i++) historyOutcomes.push('PASS');
  // Alternating shuffle for realistic timeline preview
  const interleavedOutcomes: string[] = [];
  let p = 0, f = 0;
  while (p < passCount || f < failCount) {
    if (f < failCount) {
      interleavedOutcomes.push('FAIL');
      f++;
    }
    if (p < passCount) {
      interleavedOutcomes.push('PASS');
      p++;
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
      data-testid="flaky-test-detail-modal"
    >
      <div className="bg-surface-card border border-border-default rounded-xl shadow-2xl max-w-lg w-full overflow-hidden animate-in fade-in zoom-in-95 duration-150">
        <div className="p-4 border-b border-border-default flex items-center justify-between bg-slate-900/40">
          <div className="flex items-center gap-2.5">
            <div className={`p-1.5 rounded-md ${isHigh ? 'bg-rose-500/10 text-rose-400' : 'bg-amber-500/10 text-amber-400'}`}>
              <AlertTriangle className="w-5 h-5" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-content-primary font-mono">Test Node: {metrics.node_id}</h3>
              <p className="text-xs text-content-secondary">Flakiness Score: {scorePct}%</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-md text-content-secondary hover:text-content-primary hover:bg-slate-800 transition-colors"
            title="Close"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <div className="p-5 space-y-4 text-xs">
          {/* Status banner */}
          <div
            className={`p-3 rounded-lg border flex items-center gap-3 ${
              isHigh
                ? 'bg-rose-500/5 border-rose-500/20 text-rose-300'
                : isModerate
                ? 'bg-amber-500/5 border-amber-500/20 text-amber-300'
                : 'bg-emerald-500/5 border-emerald-500/20 text-emerald-300'
            }`}
          >
            <Activity className="w-5 h-5 flex-shrink-0" />
            <div>
              <div className="font-semibold">
                {isHigh ? 'Critical Flakiness' : isModerate ? 'Moderate Flakiness' : 'Stable Behavior'}
              </div>
              <div className="text-[11px] text-content-secondary">
                {isHigh
                  ? 'High variance across consecutive runs. Quarantining or healing investigation recommended.'
                  : isModerate
                  ? 'Intermittent failures detected within recent window.'
                  : 'Test meets reliability stability thresholds.'}
              </div>
            </div>
          </div>

          {/* Metrics summary */}
          <div className="grid grid-cols-2 gap-3 bg-slate-900/40 border border-border-default p-3 rounded-lg font-mono">
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Window Runs</span>
              <span className="text-content-primary font-bold text-sm">{metrics.window_runs}</span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Window Failures</span>
              <span className="text-rose-400 font-bold text-sm">{metrics.window_failures}</span>
            </div>
          </div>

          {/* Sequence visualization */}
          <div>
            <div className="text-content-secondary text-xs font-medium mb-2 flex items-center justify-between">
              <span>Recent Outcome Distribution</span>
              <span className="text-[10px] font-mono text-content-muted">
                {passCount} PASS / {failCount} FAIL
              </span>
            </div>
            <div className="flex flex-wrap gap-1.5 p-3 bg-slate-950/60 border border-border-default rounded-lg">
              {interleavedOutcomes.map((outcome, idx) => (
                <span
                  key={idx}
                  className={`px-2 py-0.5 rounded text-[10px] font-mono font-semibold ${
                    outcome === 'PASS'
                      ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                      : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                  }`}
                >
                  {outcome}
                </span>
              ))}
            </div>
          </div>

          {/* Environment Correlation */}
          <div>
            <div className="text-content-secondary text-xs font-medium mb-1 flex items-center gap-1.5">
              <Layers className="w-3.5 h-3.5 text-violet-400" />
              <span>Environment Correlation</span>
            </div>
            <div className="bg-slate-900/40 border border-border-default p-3 rounded-lg font-mono text-[11px] text-content-secondary">
              {metrics.environment_correlation || 'No environment correlation identified.'}
            </div>
          </div>
        </div>

        <div className="p-3 border-t border-border-default bg-slate-900/20 flex justify-end">
          <button
            onClick={onClose}
            className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-content-primary rounded text-xs transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
