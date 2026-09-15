/**
 * Phase 7: Planning Insights Section
 * TestPlanningAgent feedback surface: fragile tests, fragile selectors, and recommended test priorities.
 * STRICT CONSTRAINT: No LLM chain-of-thought or raw reasoning tokens are exposed here.
 */

import React, { useMemo } from 'react';
import { Sparkles, Target, AlertTriangle, Crosshair, CheckCircle2 } from 'lucide-react';
import type { ExecutionTelemetryRow, FlakinessMetricsRow, FailureClusterRow } from '../../types/contract';

interface PlanningInsightsSectionProps {
  telemetry: ExecutionTelemetryRow[];
  flakiness: FlakinessMetricsRow[];
  failures: FailureClusterRow[];
  loading?: boolean;
}

export const PlanningInsightsSection: React.FC<PlanningInsightsSectionProps> = ({
  telemetry,
  flakiness,
  failures,
  loading,
}) => {
  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5 animate-pulse" data-testid="planning-insights-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          {[...Array(3)].map((_, i) => (
            <div key={i} className="h-28 bg-slate-800/60 rounded" />
          ))}
        </div>
      </div>
    );
  }

  // 1. Fragile Tests: flakiness_score >= 0.20 or > 2 failures
  const fragileTests = useMemo(() => {
    return flakiness
      .filter((f) => f.flakiness_score >= 0.2)
      .sort((a, b) => b.flakiness_score - a.flakiness_score)
      .slice(0, 5);
  }, [flakiness]);

  // 2. Fragile Selectors: distinct selectors appearing in failure telemetry
  const fragileSelectors = useMemo(() => {
    const selectorCounts = new Map<string, number>();
    telemetry.forEach((t) => {
      if (t.selector && ['FAILED', 'TIMEOUT'].includes(t.status)) {
        selectorCounts.set(t.selector, (selectorCounts.get(t.selector) ?? 0) + 1);
      }
    });
    return Array.from(selectorCounts.entries())
      .map(([selector, count]) => ({ selector, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 5);
  }, [telemetry]);

  // 3. Recommended Regression Priorities
  const priorities = useMemo(() => {
    const list = [];
    if (failures.length > 0) {
      const topFail = failures[0];
      list.push({
        priority: 'P1 - High Recurrence Defect',
        title: `Cluster: ${topFail.signature}`,
        reason: `${topFail.occurrence_count} recurring failures detected. Review root cause hypothesis.`,
      });
    }
    if (fragileTests.length > 0) {
      list.push({
        priority: 'P2 - Flakiness Stabilization',
        title: `Test: ${fragileTests[0].node_id}`,
        reason: `Flakiness score at ${(fragileTests[0].flakiness_score * 100).toFixed(0)}%. Stabilize execution environment or assertions.`,
      });
    }
    if (fragileSelectors.length > 0) {
      list.push({
        priority: 'P3 - DOM Locator Remediation',
        title: `Selector: ${fragileSelectors[0].selector}`,
        reason: `Failed ${fragileSelectors[0].count} times. Update to resilient test ID or semantic locator.`,
      });
    }
    return list;
  }, [failures, fragileTests, fragileSelectors]);

  return (
    <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-5" data-testid="planning-insights-section">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-semibold text-slate-100 flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-amber-400" />
            <span>Test Planning Agent Feedback & Recommendations</span>
          </h3>
          <p className="text-xs text-slate-400 mt-0.5">
            Empirical planning guidance derived directly from telemetry and historical regression patterns.
          </p>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        {/* Fragile Tests */}
        <div className="bg-slate-950/60 border border-slate-800/80 rounded-lg p-4">
          <div className="flex items-center gap-2 mb-3 text-xs font-semibold text-amber-300">
            <AlertTriangle className="w-4 h-4" />
            <span>Fragile Tests ({fragileTests.length})</span>
          </div>
          {fragileTests.length === 0 ? (
            <div className="text-xs text-slate-500 italic py-4 text-center">
              No fragile tests identified in current window.
            </div>
          ) : (
            <div className="space-y-2">
              {fragileTests.map((ft) => (
                <div
                  key={ft.id}
                  className="bg-slate-900/60 border border-slate-800 p-2.5 rounded text-xs flex items-center justify-between"
                  data-testid={`fragile-test-${ft.node_id}`}
                >
                  <span className="font-mono text-slate-200 truncate max-w-[150px]" title={ft.node_id}>
                    {ft.node_id}
                  </span>
                  <span className="font-mono text-[11px] text-amber-400 bg-amber-950/40 px-1.5 py-0.5 rounded border border-amber-800/40">
                    {(ft.flakiness_score * 100).toFixed(0)}% Flaky
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Fragile Selectors */}
        <div className="bg-slate-950/60 border border-slate-800/80 rounded-lg p-4">
          <div className="flex items-center gap-2 mb-3 text-xs font-semibold text-rose-300">
            <Crosshair className="w-4 h-4" />
            <span>Fragile Selectors ({fragileSelectors.length})</span>
          </div>
          {fragileSelectors.length === 0 ? (
            <div className="text-xs text-slate-500 italic py-4 text-center">
              No broken or fragile element locators recorded.
            </div>
          ) : (
            <div className="space-y-2">
              {fragileSelectors.map((fs, idx) => (
                <div
                  key={idx}
                  className="bg-slate-900/60 border border-slate-800 p-2.5 rounded text-xs flex items-center justify-between"
                  data-testid={`fragile-selector-${idx}`}
                >
                  <code className="font-mono text-slate-300 truncate max-w-[170px]" title={fs.selector}>
                    {fs.selector}
                  </code>
                  <span className="font-mono text-[11px] text-rose-400 bg-rose-950/40 px-1.5 py-0.5 rounded border border-rose-800/40">
                    {fs.count} {fs.count === 1 ? 'fail' : 'fails'}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Recommended Regression Priorities */}
        <div className="bg-slate-950/60 border border-slate-800/80 rounded-lg p-4">
          <div className="flex items-center gap-2 mb-3 text-xs font-semibold text-teal-300">
            <Target className="w-4 h-4" />
            <span>Recommended Test Priorities</span>
          </div>
          {priorities.length === 0 ? (
            <div className="text-xs text-slate-500 italic py-4 text-center">
              System is stable. No critical remediation priorities.
            </div>
          ) : (
            <div className="space-y-2">
              {priorities.map((item, idx) => (
                <div
                  key={idx}
                  className="bg-slate-900/60 border border-slate-800 p-2.5 rounded text-xs"
                  data-testid={`planning-priority-${idx}`}
                >
                  <div className="flex items-center justify-between mb-1">
                    <span className="font-bold text-teal-400 text-[11px] font-mono">{item.priority}</span>
                  </div>
                  <div className="font-medium text-slate-200 mb-0.5">{item.title}</div>
                  <div className="text-[11px] text-slate-400 leading-tight">{item.reason}</div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
