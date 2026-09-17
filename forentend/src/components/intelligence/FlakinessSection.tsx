/**
 * Phase 7: Flakiness Intelligence Section
 * Displays stability and flakiness scores with outcome drilldown modal.
 */

import React, { useState } from 'react';
import { AlertTriangle, Activity, CheckCircle2, Eye } from 'lucide-react';
import type { FlakinessMetricsRow } from '../../types/contract';
import { FlakyTestDetailModal } from './FlakyTestDetailModal';

interface FlakinessSectionProps {
  flakiness: FlakinessMetricsRow[];
  loading?: boolean;
}

export const FlakinessSection: React.FC<FlakinessSectionProps> = ({ flakiness, loading }) => {
  const [selectedItem, setSelectedItem] = useState<FlakinessMetricsRow | null>(null);

  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5 animate-pulse" data-testid="flakiness-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="space-y-2">
          {[...Array(3)].map((_, i) => (
            <div key={i} className="h-12 bg-slate-800/60 rounded" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5" data-testid="flakiness-section">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-semibold text-slate-100 flex items-center gap-2">
            <AlertTriangle className="w-4 h-4 text-amber-400" />
            <span>Flakiness Intelligence</span>
            <span className="text-xs px-2 py-0.5 rounded-full bg-slate-800 text-slate-300 font-mono">
              {flakiness.length} {flakiness.length === 1 ? 'test tracked' : 'tests tracked'}
            </span>
          </h3>
          <p className="text-xs text-slate-400 mt-0.5">
            Statistical variance in test outcomes across execution windows.
          </p>
        </div>
      </div>

      {flakiness.length === 0 ? (
        <div className="py-12 text-center text-slate-400 text-xs border border-dashed border-slate-800/80 rounded-lg" data-testid="flakiness-empty">
          <CheckCircle2 className="w-8 h-8 text-emerald-400 mx-auto mb-2 opacity-60" />
          No flaky test behavior detected in current analysis window. All tests stable.
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs text-slate-300">
            <thead className="bg-slate-950/60 text-slate-400 uppercase font-mono text-[10px] border-b border-slate-800">
              <tr>
                <th className="py-2.5 px-3">Test Node ID</th>
                <th className="py-2.5 px-3">Flakiness Score</th>
                <th className="py-2.5 px-3">Window Runs</th>
                <th className="py-2.5 px-3">Window Failures</th>
                <th className="py-2.5 px-3">Environment Correlation</th>
                <th className="py-2.5 px-3 text-right">Drill-down</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60 font-mono">
              {flakiness.map((row) => {
                const score = row.flakiness_score;
                const isStable = score < 0.15;
                const isModerate = score >= 0.15 && score < 0.4;
                const isCritical = score >= 0.4;

                return (
                  <tr
                    key={row.id}
                    className="hover:bg-slate-800/30 transition-colors cursor-pointer"
                    onClick={() => setSelectedItem(row)}
                    data-testid={`flakiness-row-${row.id}`}
                  >
                    <td className="py-2.5 px-3 font-semibold text-slate-100">{row.node_id}</td>
                    <td className="py-2.5 px-3">
                      <span
                        className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-semibold ${
                          isStable
                            ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                            : isModerate
                            ? 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                            : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                        }`}
                      >
                        {(score * 100).toFixed(1)}% {isStable ? '(Stable)' : isModerate ? '(Flaky)' : '(High)'}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">{row.window_runs}</td>
                    <td className="py-2.5 px-3 text-rose-400 font-bold">{row.window_failures}</td>
                    <td className="py-2.5 px-3 text-slate-400 font-sans truncate max-w-[200px]">
                      {row.environment_correlation ?? 'None'}
                    </td>
                    <td className="py-2.5 px-3 text-right">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setSelectedItem(row);
                        }}
                        className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-slate-100 transition-colors"
                        title="View Drilldown"
                        data-testid={`flakiness-drilldown-${row.id}`}
                      >
                        <Eye className="w-3.5 h-3.5" />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {selectedItem && (
        <FlakyTestDetailModal metrics={selectedItem} onClose={() => setSelectedItem(null)} />
      )}
    </div>
  );
};
