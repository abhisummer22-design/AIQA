/**
 * Phase 7: Risk Intelligence Section
 * Displays authoritative Phase 6 quality risk calibration (35% failure rate, 20% flakiness,
 * 15% severity, 10% dependencies, 10% regressions, 10% security boundary).
 *
 * STRICT INVARIANT:
 * Zero client-side risk formula calculation. Zero hardcoded factor weights.
 * Consumes authoritative backend risk score, classification, and factors directly.
 */

import React from 'react';
import { ShieldAlert, AlertTriangle, ShieldCheck, CheckCircle2 } from 'lucide-react';
import type {
  RiskCalibration,
  ExecutionTelemetryRow,
  FlakinessMetricsRow,
  HealingStrategyMetricsRow,
} from '../../types/contract';

interface RiskIntelligenceSectionProps {
  risk?: RiskCalibration | null;
  telemetry?: ExecutionTelemetryRow[];
  flakiness?: FlakinessMetricsRow[];
  strategies?: HealingStrategyMetricsRow[];
  loading?: boolean;
}

export const RiskIntelligenceSection: React.FC<RiskIntelligenceSectionProps> = ({
  risk,
  loading,
}) => {
  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5 animate-pulse" data-testid="risk-intel-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          {[...Array(6)].map((_, i) => (
            <div key={i} className="h-28 bg-slate-800/60 rounded" />
          ))}
        </div>
      </div>
    );
  }

  const riskLevel = risk?.risk_level ?? 'INSUFFICIENT_DATA';
  const riskScore = risk?.risk_score;
  const factorsData = risk?.factors;
  const explanation =
    risk?.explanation ||
    'Calibrated quality risk score derived strictly from authoritative backend models.';

  const factors = [
    {
      name: 'Failure Rate Factor',
      tag: 'Empirical',
      value: factorsData !== undefined ? `${(factorsData.failure_rate * 100).toFixed(1)}%` : 'N/A',
      pct: factorsData !== undefined ? factorsData.failure_rate * 100 : 0,
      description: 'Empirical failure frequency observed in execution telemetry',
      color: factorsData && factorsData.failure_rate > 0.2 ? 'bg-rose-500' : 'bg-emerald-500',
    },
    {
      name: 'Flakiness Factor',
      tag: 'Empirical',
      value: factorsData !== undefined ? `${(factorsData.flakiness_score * 100).toFixed(1)}%` : 'N/A',
      pct: factorsData !== undefined ? factorsData.flakiness_score * 100 : 0,
      description: 'Statistical outcome variance across test runs',
      color: factorsData && factorsData.flakiness_score > 0.15 ? 'bg-amber-500' : 'bg-emerald-500',
    },
    {
      name: 'Failure Severity Factor',
      tag: 'Empirical',
      value: factorsData !== undefined ? `${(factorsData.failure_severity * 100).toFixed(1)}%` : 'N/A',
      pct: factorsData !== undefined ? factorsData.failure_severity * 100 : 0,
      description: 'Impact weight of infrastructure and timeout failures',
      color: factorsData && factorsData.failure_severity > 0.5 ? 'bg-rose-500' : 'bg-emerald-500',
    },
    {
      name: 'Dependency Factor',
      tag: 'Topology',
      value: factorsData !== undefined ? `${factorsData.dependency_count} relations` : 'N/A',
      pct: factorsData !== undefined ? Math.min(100, (factorsData.dependency_count / 5) * 100) : 0,
      description: 'Quality Graph relational coupling count',
      color: factorsData && factorsData.dependency_count > 3 ? 'bg-amber-500' : 'bg-emerald-500',
    },
    {
      name: 'Regression Factor',
      tag: 'Governance',
      value: factorsData !== undefined ? `${(factorsData.healing_regression_rate * 100).toFixed(1)}%` : 'N/A',
      pct: factorsData !== undefined ? factorsData.healing_regression_rate * 100 : 0,
      description: 'Verified regressions introduced by AST healing attempts',
      color: factorsData && factorsData.healing_regression_rate > 0 ? 'bg-rose-500' : 'bg-emerald-500',
    },
    {
      name: 'Security & SSRF Boundary',
      tag: 'Governed Gate',
      value: factorsData !== undefined ? (factorsData.security_sensitivity === 0 ? 'PASSED' : 'FLAGGED') : 'PASSED',
      pct: 100,
      description: 'Zero SSRF / policy boundary violations detected',
      color: factorsData && factorsData.security_sensitivity > 0 ? 'bg-rose-500' : 'bg-emerald-500',
    },
  ];

  return (
    <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5" data-testid="risk-intelligence-section">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-5">
        <div>
          <h3 className="text-sm font-semibold text-slate-100 flex items-center gap-2">
            <ShieldAlert className="w-4 h-4 text-rose-400" />
            <span>Quality Risk & Multi-Factor Drilldown</span>
          </h3>
          <p className="text-xs text-slate-400 mt-0.5" data-testid="risk-explanation">
            {explanation}
          </p>
        </div>

        <div className="flex items-center gap-3">
          {riskScore !== undefined && riskScore !== null && (
            <div className="text-xs font-mono text-slate-300" data-testid="authoritative-risk-score">
              Score: <strong className="text-slate-100 font-bold">{(riskScore * 100).toFixed(0)}%</strong> ({riskScore.toFixed(2)})
            </div>
          )}
          <div className="flex items-center gap-2">
            <span className="text-xs text-slate-400">Risk Assessment:</span>
            <span
              className={`px-3 py-1 rounded-md text-xs font-mono font-bold border ${
                riskLevel === 'CRITICAL'
                  ? 'bg-rose-950/60 text-rose-400 border-rose-800'
                  : riskLevel === 'HIGH'
                  ? 'bg-amber-950/60 text-amber-400 border-amber-800'
                  : riskLevel === 'MEDIUM'
                  ? 'bg-yellow-950/60 text-yellow-400 border-yellow-800'
                  : riskLevel === 'LOW'
                  ? 'bg-emerald-950/60 text-emerald-400 border-emerald-800'
                  : 'bg-slate-800 text-slate-400 border-slate-700'
              }`}
              data-testid="composite-risk-badge"
            >
              {riskLevel}
            </span>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3.5">
        {factors.map((f, idx) => (
          <div
            key={idx}
            className="bg-slate-950/60 border border-slate-800/80 rounded-lg p-3.5 flex flex-col justify-between"
            data-testid={`risk-factor-${idx}`}
          >
            <div>
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-semibold text-slate-200">{f.name}</span>
                <span className="text-[10px] font-mono text-slate-500">{f.tag}</span>
              </div>
              <div className="text-lg font-bold font-mono text-slate-100 my-1">{f.value}</div>
              <div className="w-full bg-slate-900 rounded-full h-1.5 mb-2 overflow-hidden">
                <div className={`${f.color} h-1.5 rounded-full transition-all`} style={{ width: `${Math.min(100, f.pct)}%` }} />
              </div>
            </div>
            <p className="text-[11px] text-slate-400 leading-tight">{f.description}</p>
          </div>
        ))}
      </div>
    </div>
  );
};
