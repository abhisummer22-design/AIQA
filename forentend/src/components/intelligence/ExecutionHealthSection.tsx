/**
 * Phase 7: Execution Health Metrics Section
 * Displays top-level health KPIs derived strictly from real execution telemetry and healing metrics.
 */

import React from 'react';
import { Activity, AlertTriangle, CheckCircle2, Clock, ShieldAlert, Sparkles, Wrench, XCircle } from 'lucide-react';
import type { ExecutionTelemetryRow, HealingStrategyMetricsRow, FlakinessMetricsRow } from '../../types/contract';

interface ExecutionHealthSectionProps {
  telemetry: ExecutionTelemetryRow[];
  strategies: HealingStrategyMetricsRow[];
  flakiness: FlakinessMetricsRow[];
  loading?: boolean;
}

export const ExecutionHealthSection: React.FC<ExecutionHealthSectionProps> = ({
  telemetry,
  strategies,
  flakiness,
  loading,
}) => {
  if (loading) {
    return (
      <div className="grid grid-cols-2 sm:grid-cols-2 lg:grid-cols-4 gap-3.5 mb-6 animate-pulse">
        {[...Array(8)].map((_, i) => (
          <div key={i} className="bg-slate-900/60 border border-slate-800/80 rounded-lg p-4 h-24" />
        ))}
      </div>
    );
  }

  const totalExecutions = telemetry.length;
  const passedCount = telemetry.filter((t) => t.status === 'PASSED').length;
  const failedCount = telemetry.filter((t) => ['FAILED', 'TIMEOUT', 'INFRA_ERROR'].includes(t.status)).length;

  const passRate = totalExecutions > 0 ? ((passedCount / totalExecutions) * 100).toFixed(1) + '%' : 'Insufficient data';
  const failureRate = totalExecutions > 0 ? ((failedCount / totalExecutions) * 100).toFixed(1) + '%' : 'Insufficient data';

  const totalDuration = telemetry.reduce((sum, t) => sum + (t.duration_ms || 0), 0);
  const avgDurationMs = totalExecutions > 0 ? Math.round(totalDuration / totalExecutions) : null;
  const avgDurationStr = avgDurationMs !== null ? `${avgDurationMs} ms` : 'Insufficient data';

  const flakyCount = flakiness.filter((f) => f.flakiness_score >= 0.15).length;

  const totalHealingAttempts = strategies.reduce((sum, s) => sum + s.attempts_count, 0);
  const totalVerifiedHeals = strategies.reduce((sum, s) => sum + s.verified_count, 0);
  const totalRegressions = strategies.reduce((sum, s) => sum + s.regression_count, 0);

  const kpis = [
    {
      label: 'Total Executions',
      value: totalExecutions > 0 ? totalExecutions.toLocaleString() : 'Insufficient data',
      icon: Activity,
      color: 'text-sky-400',
      border: 'border-sky-500/20',
      bg: 'bg-sky-500/5',
    },
    {
      label: 'Pass Rate',
      value: passRate,
      icon: CheckCircle2,
      color: passRate !== 'Insufficient data' ? 'text-emerald-400' : 'text-slate-400',
      border: 'border-emerald-500/20',
      bg: 'bg-emerald-500/5',
    },
    {
      label: 'Failure Rate',
      value: failureRate,
      icon: XCircle,
      color: failureRate !== 'Insufficient data' ? 'text-rose-400' : 'text-slate-400',
      border: 'border-rose-500/20',
      bg: 'bg-rose-500/5',
    },
    {
      label: 'Flaky Tests',
      value: flakiness.length > 0 ? flakyCount.toString() : '0 detected',
      icon: AlertTriangle,
      color: flakyCount > 0 ? 'text-amber-400' : 'text-slate-400',
      border: 'border-amber-500/20',
      bg: 'bg-amber-500/5',
    },
    {
      label: 'Avg Duration',
      value: avgDurationStr,
      icon: Clock,
      color: 'text-violet-400',
      border: 'border-violet-500/20',
      bg: 'bg-violet-500/5',
    },
    {
      label: 'Healing Attempts',
      value: strategies.length > 0 ? totalHealingAttempts.toString() : '0 recorded',
      icon: Wrench,
      color: 'text-indigo-400',
      border: 'border-indigo-500/20',
      bg: 'bg-indigo-500/5',
    },
    {
      label: 'Verified Heals',
      value: strategies.length > 0 ? totalVerifiedHeals.toString() : '0 recorded',
      icon: Sparkles,
      color: 'text-teal-400',
      border: 'border-teal-500/20',
      bg: 'bg-teal-500/5',
    },
    {
      label: 'Regressions Detected',
      value: strategies.length > 0 ? totalRegressions.toString() : '0 recorded',
      icon: ShieldAlert,
      color: totalRegressions > 0 ? 'text-rose-400' : 'text-emerald-400',
      border: 'border-slate-800',
      bg: 'bg-slate-900/40',
    },
  ];

  return (
    <div className="grid grid-cols-2 sm:grid-cols-2 lg:grid-cols-4 gap-3.5 mb-6" data-testid="execution-health-grid">
      {kpis.map((kpi, idx) => {
        const Icon = kpi.icon;
        return (
          <div
            key={idx}
            className={`rounded-lg border ${kpi.border} ${kpi.bg} p-3.5 flex flex-col justify-between transition-colors`}
          >
            <div className="flex items-center justify-between mb-1.5">
              <span className="text-xs font-medium text-slate-400 uppercase tracking-wider">{kpi.label}</span>
              <Icon className={`w-4 h-4 ${kpi.color}`} />
            </div>
            <div className="text-xl font-bold font-mono text-slate-100">{kpi.value}</div>
          </div>
        );
      })}
    </div>
  );
};
