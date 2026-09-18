/**
 * Phase 7: Trends Section
 * Directional trend metrics with historical comparisons and sample counters.
 */

import React from 'react';
import { TrendingUp, TrendingDown, Minus, HelpCircle, Activity } from 'lucide-react';
import type { TrendAnalysis, TrendDirection } from '../../types/contract';

interface TrendsSectionProps {
  trends: TrendAnalysis | null;
  loading?: boolean;
}

export const TrendsSection: React.FC<TrendsSectionProps> = ({ trends, loading }) => {
  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-border-default rounded-xl p-5 animate-pulse" data-testid="trends-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          {[...Array(3)].map((_, i) => (
            <div key={i} className="h-28 bg-slate-800/60 rounded" />
          ))}
        </div>
      </div>
    );
  }

  const renderDirectionBadge = (direction: TrendDirection) => {
    switch (direction) {
      case 'IMPROVING':
        return (
          <span className="inline-flex items-center gap-1 text-emerald-400 bg-emerald-950/40 border border-emerald-800/50 px-2 py-0.5 rounded text-xs font-mono font-bold">
            <TrendingUp className="w-3.5 h-3.5" /> IMPROVING
          </span>
        );
      case 'DEGRADING':
        return (
          <span className="inline-flex items-center gap-1 text-rose-400 bg-rose-950/40 border border-rose-800/50 px-2 py-0.5 rounded text-xs font-mono font-bold">
            <TrendingDown className="w-3.5 h-3.5" /> DEGRADING
          </span>
        );
      case 'STABLE':
        return (
          <span className="inline-flex items-center gap-1 text-sky-400 bg-sky-950/40 border border-sky-800/50 px-2 py-0.5 rounded text-xs font-mono font-bold">
            <Minus className="w-3.5 h-3.5" /> STABLE
          </span>
        );
      case 'INSUFFICIENT_DATA':
      default:
        return (
          <span className="inline-flex items-center gap-1 text-content-secondary bg-slate-800/60 border border-slate-700 px-2 py-0.5 rounded text-xs font-mono font-bold">
            <HelpCircle className="w-3.5 h-3.5" /> INSUFFICIENT DATA
          </span>
        );
    }
  };

  return (
    <div className="bg-slate-900/40 border border-border-default rounded-xl p-5" data-testid="trends-section">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-semibold text-content-primary flex items-center gap-2">
            <Activity className="w-4 h-4 text-teal-400" />
            <span>Quality Trends & Telemetry Trajectory</span>
          </h3>
          <p className="text-xs text-content-secondary mt-0.5">
            Statistical regression and improvement velocity across consecutive run windows.
          </p>
        </div>
      </div>

      {!trends || trends.direction === 'INSUFFICIENT_DATA' || trends.sample_count < 2 ? (
        <div className="py-10 text-center text-content-secondary text-xs border border-dashed border-border-default/80 rounded-lg p-6" data-testid="trends-insufficient">
          <HelpCircle className="w-8 h-8 text-content-muted mx-auto mb-2 opacity-60" />
          <div className="font-semibold text-content-secondary mb-1">Insufficient Historical Samples</div>
          <p className="max-w-md mx-auto text-content-secondary">
            {trends?.notes ||
              'A minimum of 2 consecutive execution windows is required to establish empirical trend directions.'}
          </p>
          <div className="mt-3 font-mono text-[11px] text-content-muted">
            Current sample count: {trends?.sample_count ?? 0}
          </div>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4" data-testid="trends-cards-grid">
          {/* Main Trend Card */}
          <div className="bg-slate-950/60 border border-border-default/80 rounded-lg p-4 flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-semibold text-content-primary capitalize">
                  {trends.metric_name.replace(/_/g, ' ')}
                </span>
                {renderDirectionBadge(trends.direction)}
              </div>
              <div className="grid grid-cols-2 gap-2 my-3 font-mono">
                <div className="bg-slate-900/60 p-2 rounded border border-border-default">
                  <span className="text-[10px] text-content-muted uppercase block">Current</span>
                  <span className="text-content-primary font-bold text-sm">
                    {typeof trends.current_value === 'number'
                      ? trends.current_value.toFixed(2)
                      : trends.current_value}
                  </span>
                </div>
                <div className="bg-slate-900/60 p-2 rounded border border-border-default">
                  <span className="text-[10px] text-content-muted uppercase block">Previous</span>
                  <span className="text-content-secondary font-bold text-sm">
                    {typeof trends.previous_value === 'number'
                      ? trends.previous_value.toFixed(2)
                      : trends.previous_value}
                  </span>
                </div>
              </div>
            </div>
            <div>
              <p className="text-[11px] text-content-secondary">{trends.notes}</p>
              <div className="mt-2 text-[10px] font-mono text-content-muted">
                Sample window: {trends.sample_count} runs
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
