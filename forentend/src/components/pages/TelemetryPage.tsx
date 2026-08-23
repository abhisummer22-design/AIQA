/**
 * System Telemetry — build order item 2, 22 Aug 2026
 * (`AIQA_SCOPE_RECOMMENDATIONS_REPORT.md`). Genuinely new page: ACC-013
 * ("API Latency/Telemetry") was never one of FE-11's 7 Case-C stub pages,
 * so there was nothing to redirect — this is a real build onto the real,
 * pre-existing `GET /metrics` route (`services/orchestrator/src/
 * metrics.rs`, `api.rs::get_metrics`), investigated fresh before writing
 * any of this: authenticated (any role), platform-wide (not project-
 * scoped — the SQL behind it has no `project_id` filter anywhere), and
 * emitting Prometheus text, not JSON.
 *
 * What `/metrics` actually contains, confirmed by reading `metrics.rs` in
 * full before writing this file: **six pure state/status COUNT
 * gauges/counter — `aiqa_runs_total{state=...}`, `aiqa_tasks_total{status=...}`,
 * `aiqa_bug_drafts_total{review_state=...}`, `aiqa_failure_clusters_total`,
 * `aiqa_api_keys_active`, `aiqa_audit_log_total`. There is no latency, no
 * response-time, no duration, no histogram of any kind — not even a sum
 * that could yield an average, let alone buckets a real p95/p99 could be
 * computed from.** ACC-013's own Acceptance Criteria ask for "latency,
 * response time, p95, p99, errors, timeouts" — of those six, only
 * `timeouts` maps precisely (a real, named `aiqa_tasks_total{status="TIMEOUT"}`
 * series) and `errors` maps loosely (task-level `FAILED`/`INFRA_ERROR`
 * counts, not HTTP-request-level errors). Latency/response-time/p95/p99
 * are not fabricated here — this page says so explicitly, in its own UI
 * copy, rather than inventing a number.
 *
 * Real run/task *duration* data does exist elsewhere in this system
 * (`execution_avg_run_duration_ms`/`execution_efficiency_ms`, BE-12b, real
 * SQL aggregates) — but it is project-scoped, lives on a different
 * endpoint (`GET .../kpis`), and is already displayed on the Dashboard
 * (FE-26). This page links to it rather than re-fetching and re-rendering
 * the same numbers a second time.
 *
 * No charting library was added: `/metrics`'s real data is six small,
 * fully-enumerable label sets — a simple, real, proportional-width bar row
 * per label (real DOM, real text, no ResizeObserver/canvas dependency) is
 * more honest and more testable than reaching for `recharts` (already a
 * dependency, unused anywhere else in this app) for six short bar lists.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Activity, AlertTriangle, Gauge, Inbox, RotateCcw } from 'lucide-react';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { SectionHeader } from '../common/SectionHeader';

// ---------------------------------------------------------- Prometheus parsing

interface MetricSample {
  name: string;
  labels: Record<string, string>;
  value: number;
}

/** Parses exactly the shape `metrics.rs::render` actually emits: `# HELP`/
 *  `# TYPE` comment lines (skipped), then either `name{label="value"} n` or
 *  a bare `name n` per real line. No assumption beyond that — an
 *  unparseable line is skipped, never guessed at. */
export function parsePrometheusText(text: string): MetricSample[] {
  const samples: MetricSample[] = [];
  for (const rawLine of text.split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;
    const match = line.match(/^([a-zA-Z_:][a-zA-Z0-9_:]*)(\{([^}]*)\})?\s+([0-9.eE+-]+)\s*$/);
    if (!match) continue;
    const [, name, , labelStr, valueStr] = match;
    const labels: Record<string, string> = {};
    if (labelStr) {
      const labelRe = /([a-zA-Z_][a-zA-Z0-9_]*)="((?:[^"\\]|\\.)*)"/g;
      let lm: RegExpExecArray | null;
      while ((lm = labelRe.exec(labelStr))) {
        labels[lm[1]] = lm[2];
      }
    }
    const value = Number(valueStr);
    if (!Number.isFinite(value)) continue;
    samples.push({ name, labels, value });
  }
  return samples;
}

function seriesFor(samples: MetricSample[], name: string, labelKey: string): Array<{ label: string; value: number }> {
  return samples
    .filter((s) => s.name === name)
    .map((s) => ({ label: s.labels[labelKey] ?? '(unlabeled)', value: s.value }))
    .sort((a, b) => b.value - a.value);
}

function scalarFor(samples: MetricSample[], name: string): number | null {
  const s = samples.find((sample) => sample.name === name);
  return s ? s.value : null;
}

// -------------------------------------------------------------------- UI bits

const BAR_COLORS: Record<string, string> = {
  COMPLETED: 'bg-emerald-500', PASSED: 'bg-emerald-500',
  FAILED: 'bg-rose-500', REJECTED: 'bg-rose-500',
  TIMEOUT: 'bg-amber-500', INFRA_ERROR: 'bg-amber-500',
  RUNNING: 'bg-cyan-500', SCHEDULED: 'bg-cyan-500', QUEUED: 'bg-cyan-500',
  CANCELLED: 'bg-slate-500', SKIPPED: 'bg-slate-500', BLOCKED: 'bg-slate-500',
  PENDING_APPROVAL: 'bg-indigo-500', NEEDS_REVIEW: 'bg-indigo-500', CREATED: 'bg-indigo-500', PARTIAL: 'bg-indigo-500',
};

function barRow(label: string, value: number, max: number): React.ReactElement {
  const pct = max > 0 ? Math.max((value / max) * 100, value > 0 ? 3 : 0) : 0;
  const color = BAR_COLORS[label] ?? 'bg-indigo-500';
  return (
    <div key={label} className="flex items-center gap-2.5 text-xs font-mono">
      <span className="w-32 shrink-0 truncate text-slate-400" title={label}>{label}</span>
      <div className="flex-1 h-4 rounded bg-[#020617] border border-slate-800 overflow-hidden">
        <div className={`h-full ${color}`} style={{ width: `${pct}%` }} />
      </div>
      <span className="w-10 shrink-0 text-right font-semibold text-slate-200">{value}</span>
    </div>
  );
}

function seriesCard(title: string, series: Array<{ label: string; value: number }>): React.ReactElement {
  const max = series.reduce((m, s) => Math.max(m, s.value), 0);
  return (
    <div className="p-3.5 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2.5">
      <span className="text-[10px] font-mono uppercase tracking-wider text-slate-500 font-semibold">{title}</span>
      <div className="space-y-1.5">{series.map((s) => barRow(s.label, s.value, max))}</div>
    </div>
  );
}

function scalarTile(label: string, value: number | null): React.ReactElement {
  return (
    <div className="p-3.5 rounded-lg bg-[#0B1120] border border-slate-800 space-y-1">
      <span className="text-[10px] font-mono uppercase tracking-wider text-slate-500 font-semibold">{label}</span>
      <div className="text-xl font-bold font-mono text-slate-100">{value ?? '—'}</div>
    </div>
  );
}

// ------------------------------------------------------------------------ page

export const TelemetryPage: React.FC = () => {
  const navigate = useNavigate();
  const [text, setText] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const fetchMetrics = useCallback(async (isRefresh: boolean) => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    if (isRefresh) setRefreshing(true);
    else setLoading(true);
    setError(null);
    try {
      const raw = await aiqaApi.metrics.get(controller.signal);
      if (controller.signal.aborted) return;
      setText(raw);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setError(apiErr);
      setText(null);
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, []);

  useEffect(() => {
    void fetchMetrics(false);
    return () => abortRef.current?.abort();
  }, [fetchMetrics]);

  const handleRefresh = useCallback(() => {
    if (loading || refreshing) return;
    void fetchMetrics(true);
  }, [loading, refreshing, fetchMetrics]);

  const renderBody = () => {
    if (loading) {
      return (
        <StateCard
          icon={Gauge}
          iconClassName="text-indigo-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-indigo-300 bg-indigo-950/60 border-indigo-800"
          title="Loading telemetry"
          description="Fetching the real orchestrator /metrics snapshot…"
        />
      );
    }

    if (error) {
      return renderResourceErrorState(error, handleRefresh, {
        forbiddenTitle: 'Not permitted to view telemetry',
        forbiddenDescription: "This key's role does not permit reading /metrics.",
        offlineDescriptionSuffix: 'to load telemetry.',
        genericTitle: "Couldn't load telemetry",
      });
    }

    if (text === null) return null;

    const samples = parsePrometheusText(text);
    const runsByState = seriesFor(samples, 'aiqa_runs_total', 'state');
    const tasksByStatus = seriesFor(samples, 'aiqa_tasks_total', 'status');
    const bugDraftsByReviewState = seriesFor(samples, 'aiqa_bug_drafts_total', 'review_state');
    const failureClusters = scalarFor(samples, 'aiqa_failure_clusters_total');
    const activeApiKeys = scalarFor(samples, 'aiqa_api_keys_active');
    const auditLogTotal = scalarFor(samples, 'aiqa_audit_log_total');

    const hasAnyData =
      runsByState.length > 0 || tasksByStatus.length > 0 || bugDraftsByReviewState.length > 0 ||
      (failureClusters ?? 0) > 0 || (activeApiKeys ?? 0) > 0 || (auditLogTotal ?? 0) > 0;

    if (!hasAnyData) {
      return (
        <EmptyState
          icon={Inbox}
          title="No telemetry recorded yet"
          description="The real /metrics endpoint returned a genuinely empty snapshot — no runs, tasks, bug drafts, failure clusters, active keys, or audit entries exist yet on this orchestrator."
        />
      );
    }

    return (
      <div className="space-y-3">
        <div id="telemetry-disclosure" className="p-3 rounded-lg bg-amber-950/20 border border-amber-800/50 text-[11px] font-mono text-amber-200/90 flex items-start gap-2">
          <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5 text-amber-400" />
          <span>
            <strong>What this does not show:</strong> latency, response time, p95, or p99 — the real{' '}
            <code>/metrics</code> endpoint has no timing data of any kind today, only state/status counts. Real
            average run/task duration exists on the{' '}
            <button type="button" onClick={() => navigate('/dashboard')} className="underline text-amber-300 hover:text-amber-100 cursor-pointer">
              Dashboard
            </button>{' '}
            (project-scoped, a different real endpoint) — not repeated here.
          </span>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {scalarTile('Failure Clusters', failureClusters)}
          {scalarTile('Active API Keys', activeApiKeys)}
          {scalarTile('Audit Log Entries', auditLogTotal)}
        </div>

        {runsByState.length > 0 && seriesCard('Runs by State', runsByState)}
        {tasksByStatus.length > 0 && (
          <div>
            {seriesCard('Tasks by Status', tasksByStatus)}
            <p className="mt-1.5 text-[10px] font-mono text-slate-500 px-1">
              Closest real signal to ACC-013&apos;s &quot;timeouts&quot; (the real <code>TIMEOUT</code> series
              above) and &quot;errors&quot; (<code>FAILED</code>/<code>INFRA_ERROR</code>) — task-level, not
              HTTP-request-level.
            </p>
          </div>
        )}
        {bugDraftsByReviewState.length > 0 && seriesCard('Bug Drafts by Review State', bugDraftsByReviewState)}
      </div>
    );
  };

  return (
    <div id="telemetry-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="System Telemetry"
        subtitle="Real /metrics counts — platform-wide, not project-scoped. No latency/response-time data exists yet."
        badge="LIVE METRICS"
        icon={Activity}
        actions={
          <button
            type="button"
            id="telemetry-refresh-button"
            onClick={handleRefresh}
            disabled={loading || refreshing}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <RotateCcw className={`w-3.5 h-3.5 ${refreshing ? 'animate-spin' : ''}`} />
            <span>Refresh</span>
          </button>
        }
      />

      <section id="telemetry-content" className="space-y-3" aria-live="polite">
        {renderBody()}
      </section>
    </div>
  );
};
