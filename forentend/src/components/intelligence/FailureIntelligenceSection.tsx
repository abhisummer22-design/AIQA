/**
 * Phase 7: Failure Intelligence Section
 * Displays normalized failure clusters, root cause hypotheses, and recurrence frequencies.
 */

import React from 'react';
import { Layers, AlertTriangle, CheckCircle, HelpCircle, ShieldAlert } from 'lucide-react';
import type { FailureClusterRow } from '../../types/contract';

interface FailureIntelligenceSectionProps {
  failures: FailureClusterRow[];
  loading?: boolean;
}

export const FailureIntelligenceSection: React.FC<FailureIntelligenceSectionProps> = ({
  failures,
  loading,
}) => {
  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5 animate-pulse" data-testid="failure-intel-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="space-y-3">
          {[...Array(3)].map((_, i) => (
            <div key={i} className="h-20 bg-slate-800/60 rounded" />
          ))}
        </div>
      </div>
    );
  }

  const maxOccurrences = Math.max(1, ...failures.map((f) => f.occurrence_count));

  return (
    <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5" data-testid="failure-intelligence-section">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-semibold text-slate-100 flex items-center gap-2">
            <Layers className="w-4 h-4 text-violet-400" />
            <span>Failure Intelligence & Clusters</span>
            <span className="text-xs px-2 py-0.5 rounded-full bg-slate-800 text-slate-300 font-mono">
              {failures.length} {failures.length === 1 ? 'cluster' : 'clusters'}
            </span>
          </h3>
          <p className="text-xs text-slate-400 mt-0.5">
            Normalized failure fingerprints clustered by signature and recurrence frequency.
          </p>
        </div>
      </div>

      {failures.length === 0 ? (
        <div className="py-12 text-center text-slate-400 text-xs border border-dashed border-slate-800/80 rounded-lg" data-testid="failure-clusters-empty">
          <CheckCircle className="w-8 h-8 text-emerald-400 mx-auto mb-2 opacity-60" />
          No recurring failure clusters detected. Test executions are healthy.
        </div>
      ) : (
        <div className="space-y-3">
          {failures.map((cluster) => {
            const pct = Math.round((cluster.occurrence_count / maxOccurrences) * 100);
            return (
              <div
                key={cluster.id}
                className="bg-slate-950/60 border border-slate-800/80 hover:border-slate-700/80 rounded-lg p-4 transition-colors"
                data-testid={`failure-cluster-${cluster.id}`}
              >
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 mb-2">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-xs font-bold text-rose-300 bg-rose-950/40 px-2 py-0.5 rounded border border-rose-800/50">
                      {cluster.signature}
                    </span>
                    <span
                      className={`text-[10px] font-mono px-2 py-0.5 rounded border ${
                        cluster.review_state === 'CONFIRMED'
                          ? 'bg-emerald-950/40 text-emerald-400 border-emerald-800/50'
                          : cluster.review_state === 'DISMISSED'
                          ? 'bg-slate-800/60 text-slate-400 border-slate-700'
                          : 'bg-amber-950/40 text-amber-400 border-amber-800/50'
                      }`}
                    >
                      {cluster.review_state}
                    </span>
                  </div>
                  <div className="text-xs font-mono text-slate-400 flex items-center gap-3">
                    <span>
                      Occurrences: <strong className="text-slate-100">{cluster.occurrence_count}</strong>
                    </span>
                    {cluster.confidence !== null && (
                      <span>
                        Confidence: <strong className="text-slate-100">{(cluster.confidence * 100).toFixed(0)}%</strong>
                      </span>
                    )}
                  </div>
                </div>

                {/* Progress bar representing frequency */}
                <div className="w-full bg-slate-900 rounded-full h-1.5 mb-2.5 overflow-hidden">
                  <div
                    className="bg-gradient-to-r from-amber-500 to-rose-500 h-1.5 rounded-full transition-all duration-300"
                    style={{ width: `${pct}%` }}
                  />
                </div>

                {/* Root Cause Hypothesis */}
                {cluster.root_cause_hypothesis ? (
                  <p className="text-xs text-slate-300 bg-slate-900/60 p-2.5 rounded border border-slate-800/60 font-sans">
                    <strong className="text-slate-400 font-medium">Hypothesis: </strong>
                    {cluster.root_cause_hypothesis}
                  </p>
                ) : (
                  <p className="text-xs text-slate-500 italic">No automated hypothesis established yet.</p>
                )}

                {/* Timestamp metadata */}
                <div className="mt-2 text-[10px] font-mono text-slate-500 flex items-center gap-4">
                  <span>First seen: {new Date(cluster.first_seen_at).toLocaleDateString()}</span>
                  <span>Last seen: {new Date(cluster.last_seen_at).toLocaleDateString()}</span>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
