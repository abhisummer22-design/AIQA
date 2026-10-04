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
 * full before writing this file: the pure state/status COUNT
 * gauges/counters -- `aiqa_runs_total{state=...}`, `aiqa_tasks_total{status=...}`,
 * `aiqa_bug_drafts_total{review_state=...}`, `aiqa_failure_clusters_total`,
 * `aiqa_api_keys_active`, `aiqa_audit_log_total`.
 *
 * CORRECTION. What follows used to say there was "no latency, no response-time, no
 * duration, no histogram of any kind - not even a sum that could yield an average,
 * let alone buckets a real p95/p99 could be computed from", and the page repeated
 * that in its own UI copy. It was accurate when written and is now false:
 * `metrics.rs` grew `http_request_duration_seconds`, a full Prometheus latency
 * histogram with cumulative buckets, `_sum` and `_count`, in commit 6f1efa6
 * ("Phase G, part 2: audit retention, latency histogram, ..."). So a healthy
 * orchestrator was being told it had no latency data while `/metrics` was serving
 * hundreds of observations. `summariseLatency` now reads it.
 *
 * A percentile still cannot be recovered exactly from a histogram, and the page
 * does not pretend otherwise: p50/p95/p99 are rendered as the bucket range each
 * falls into. The mean (`_sum / _count`) is exact and is labelled as the one figure
 * here that is not a bound.
 *
 * ACC-013's own Acceptance Criteria ask for "latency, response time, p95, p99,
 * errors, timeouts". Of those, latency/p50/p95/p99 now come from the real histogram;
 * `timeouts` maps precisely to `aiqa_tasks_total{status="TIMEOUT"}`; and `errors`
 * maps only loosely, to task-level `FAILED`/`INFRA_ERROR` counts rather than
 * HTTP-request-level ones. What remains genuinely absent is latency *broken down
 * by route, status code or project*: the histogram is one platform-wide series, so
 * a slow endpoint cannot be told from a fast one here. The page says that too.
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
import { Activity, Gauge, Inbox, Info, RotateCcw } from 'lucide-react';
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

  /**
   * A percentile read off a histogram, as a *range* rather than a point.
   *
   * `http_request_duration_seconds` is a Prometheus histogram: cumulative bucket
   * counts, a `_sum` and a `_count`, and nothing else. No percentile is stored, so
   * one can only be bounded by the bucket it falls in. Reporting the bucket's upper
   * bound as "the p95" would claim the request was faster than it might have been;
   * reporting a midpoint would claim precision the data does not carry. So both
   * ends are kept and the UI shows the range the observation is actually in.
   */
  export interface QuantileRange {
    /** Lower bound, exclusive, in seconds. 0 means "under the first bucket". */
    lowerSeconds: number;
    /** Upper bound, inclusive, in seconds. `null` is the `+Inf` bucket, which means
     *  "above the last real bound" and cannot be quantified any further. */
    upperSeconds: number | null;
    /** Observations at or below the upper bound -- the whole point of a bucket. */
    observations: number;
  }

  export interface LatencySummary {
    count: number;
    sumSeconds: number;
    /** Exact, not estimated: `_sum / _count`. A mean is the one figure a histogram
     *  does give outright. */
    meanMs: number | null;
    p50: QuantileRange | null;
    p95: QuantileRange | null;
    p99: QuantileRange | null;
  }

  /**
   * Reads the real histogram out of a `/metrics` scrape.
   *
   * `null` means there is no histogram at all; a summary with `count: 0` means the
   * histogram exists but nothing has been observed. Those are different situations
   * and the page distinguishes them rather than showing one blank panel for both.
   */
  export function summariseLatency(samples: MetricSample[]): LatencySummary | null {
    const buckets = samples
      .filter((s) => s.name === 'http_request_duration_seconds_bucket')
      .map((s) => ({ le: s.labels.le, cumulative: s.value }))
      // `+Inf` has to be handled explicitly: Number('+Inf') is NaN, so a plain
      // numeric sort would drop it to an arbitrary position.
      .sort((a, b) => {
        if (a.le === '+Inf') return 1;
        if (b.le === '+Inf') return -1;
        return Number(a.le) - Number(b.le);
      });
    if (buckets.length === 0) return null;

    const countSample = samples.find((s) => s.name === 'http_request_duration_seconds_count');
    const sumSample = samples.find((s) => s.name === 'http_request_duration_seconds_sum');
    const count = countSample?.value ?? buckets[buckets.length - 1].cumulative;
    const sumSeconds = sumSample?.value ?? 0;

    const quantile = (q: number): QuantileRange | null => {
      if (count <= 0) return null;
      // First bucket whose cumulative count reaches the target. Prometheus buckets
      // are cumulative, so the preceding bound is the exclusive lower one.
      const at = buckets.find((b) => b.cumulative >= q * count);
      if (!at) return null;
      const index = buckets.indexOf(at);
      const previous = index > 0 ? Number(buckets[index - 1].le) : 0;
      return {
        lowerSeconds: Number.isFinite(previous) ? previous : 0,
        upperSeconds: at.le === '+Inf' ? null : Number(at.le),
        observations: at.cumulative,
      };
    };

    return {
      count,
      sumSeconds,
      meanMs: count > 0 ? (sumSeconds / count) * 1000 : null,
      p50: quantile(0.5),
      p95: quantile(0.95),
      p99: quantile(0.99),
    };
  }

  /** `≤ 25 ms`, `100–250 ms`, `> 2.5 s` -- the bounds a histogram really gives. */
  export function formatSeconds(s: number): string {
    if (s >= 1) return `${s} s`;
    if (s >= 0.001) return `${Math.round(s * 1000)} ms`;
    return `${Math.round(s * 1e6)} µs`;
  }

  export function formatQuantile(q: QuantileRange | null): string {
    if (!q) return '—';
    if (q.upperSeconds === null) return `> ${formatSeconds(q.lowerSeconds)}`;
    if (q.lowerSeconds === 0) return `≤ ${formatSeconds(q.upperSeconds)}`;
    return `${formatSeconds(q.lowerSeconds)}–${formatSeconds(q.upperSeconds)}`;
  }

// -------------------------------------------------------------------- UI bits

const BAR_COLORS: Record<string, string> = {
  COMPLETED: 'bg-emerald-500', PASSED: 'bg-emerald-500',
  FAILED: 'bg-rose-500', REJECTED: 'bg-rose-500',
  TIMEOUT: 'bg-amber-500', INFRA_ERROR: 'bg-amber-500',
  RUNNING: 'bg-yellow-500', SCHEDULED: 'bg-yellow-500', QUEUED: 'bg-yellow-500',
  CANCELLED: 'bg-slate-500', SKIPPED: 'bg-slate-500', BLOCKED: 'bg-slate-500',
  PENDING_APPROVAL: 'bg-violet-500', NEEDS_REVIEW: 'bg-violet-500', CREATED: 'bg-violet-500', PARTIAL: 'bg-violet-500',
};

function barRow(label: string, value: number, max: number): React.ReactElement {
  const pct = max > 0 ? Math.max((value / max) * 100, value > 0 ? 3 : 0) : 0;
  const color = BAR_COLORS[label] ?? 'bg-violet-500';
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
      <span className="text-[10px] font-mono uppercase tracking-wider text-content-muted font-semibold">{title}</span>
      <div className="space-y-1.5">{series.map((s) => barRow(s.label, s.value, max))}</div>
    </div>
  );
}

function scalarTile(label: string, value: number | null): React.ReactElement {
  return (
    <div className="p-3.5 rounded-lg bg-[#0B1120] border border-slate-800 space-y-1">
      <span className="text-[10px] font-mono uppercase tracking-wider text-content-muted font-semibold">{label}</span>
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
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
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
    const latency = summariseLatency(samples);

    const hasAnyData =
      runsByState.length > 0 || tasksByStatus.length > 0 || bugDraftsByReviewState.length > 0 ||
      (failureClusters ?? 0) > 0 || (activeApiKeys ?? 0) > 0 || (auditLogTotal ?? 0) > 0 ||
      // A process that has served a request has latency, even if no run, task
      // or audit row exists yet. Treating the histogram as data is what stops this
      // page reporting "nothing recorded" on a busy orchestrator.
      (latency?.count ?? 0) > 0;

    /** Which of ACC-013's real "timeouts"/"errors" task statuses actually
     *  appear in this snapshot — used so the note below can only name series
     *  the page is really rendering. */
    const presentStatuses = ['TIMEOUT', 'FAILED', 'INFRA_ERROR'].filter((s) =>
      tasksByStatus.some((row) => row.label === s),
    );

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
        {/* Scope note, deliberately NOT styled as a warning. This used to be
            an amber `AlertTriangle` panel, which made a page where nothing is
            actually wrong lead with what reads as a fault — the most
            prominent element on a healthy page looked like an error. The
            content is unchanged and just as explicit; only the severity
            signal was wrong. */}
        {latency && latency.count > 0 && (
          <div className="p-3.5 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2.5">
            <div className="flex items-baseline justify-between gap-2">
              <span className="text-[10px] font-mono uppercase tracking-wider text-content-muted font-semibold">
                Request Latency
              </span>
              <span className="text-[10px] font-mono text-content-muted">
                {latency.count.toLocaleString()} request{latency.count === 1 ? '' : 's'}
              </span>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
              {([
                ['mean', latency.meanMs === null ? '—' : `${latency.meanMs.toFixed(1)} ms`],
                ['p50', formatQuantile(latency.p50)],
                ['p95', formatQuantile(latency.p95)],
                ['p99', formatQuantile(latency.p99)],
              ] as Array<[string, string]>).map(([label, value]) => (
                <div key={label} className="rounded border border-slate-800 bg-slate-900/40 px-2 py-1.5">
                  <div className="text-[10px] font-mono uppercase text-content-muted">{label}</div>
                  <div className="text-sm font-bold font-mono text-slate-100">{value}</div>
                </div>
              ))}
            </div>
            <p className="text-[10px] font-mono text-content-muted">
              {/*
               * The percentiles are bucket ranges, not point values, and saying so
               * is the whole point. `http_request_duration_seconds` stores cumulative
               * bucket counts; it does not store a percentile, so the only honest
               * figure is the bucket a request actually fell into. The mean is exact
               * (`_sum / _count`) and is the only single number here that is not a
               * bound. Platform-wide, and counting every route this process serves
               * including `/metrics` itself.
               */}
              Percentiles are the bucket each request fell into, read from the real
              <code> http_request_duration_seconds</code> histogram — bounded, not a stored
              value. Mean is exact. Platform-wide, all routes.
            </p>
          </div>
        )}


        <div id="telemetry-disclosure" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 text-[11px] font-mono text-slate-400 flex items-start gap-2">
          <Info className="w-3.5 h-3.5 shrink-0 mt-0.5 text-content-muted" />
          <span>
            {latency && latency.count > 0 ? (
              <>
                <strong className="text-slate-300">What this does not show:</strong> latency
                split by route, status code or project. The histogram above is one
                platform-wide series over every request this process has served, so a
                slow endpoint cannot be told from a fast one here. Real average run and
                task duration is per project on the{' '}
                <button type="button" onClick={() => navigate('/dashboard')} className="underline text-violet-400 hover:text-violet-300 cursor-pointer">
                  Dashboard
                </button>{' '}
                — a different real endpoint, not repeated here.
              </>
            ) : (
              <>
                <strong className="text-slate-300">What this does not show:</strong> latency,
                response time, p95, or p99 <em>for this process</em> — the real{' '}
                <code>/metrics</code> endpoint does carry an{' '}
                <code>http_request_duration_seconds</code> histogram, but this process has not
                served a request through it yet, so there is no real figure to show rather
                than an invented one.
              </>
            )}
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
            {/* Only name the series that are genuinely rendered above. The
                previous copy said "the real TIMEOUT series above"
                unconditionally, so whenever no task had actually timed out —
                the normal case — it pointed at a row that was not on screen. */}
            <p className="mt-1.5 text-[10px] font-mono text-content-muted px-1">
              {presentStatuses.length > 0 ? (
                <>
                  Closest real signal to ACC-013&apos;s &quot;timeouts&quot;/&quot;errors&quot; — the{' '}
                  {presentStatuses.map((s, i) => (
                    <React.Fragment key={s}>
                      {i > 0 && ' / '}
                      <code>{s}</code>
                    </React.Fragment>
                  ))}{' '}
                  {presentStatuses.length === 1 ? 'series' : 'series'} above. Task-level, not
                  HTTP-request-level.
                </>
              ) : (
                <>
                  No <code>TIMEOUT</code>, <code>FAILED</code>, or <code>INFRA_ERROR</code> tasks recorded, so
                  ACC-013&apos;s &quot;timeouts&quot;/&quot;errors&quot; signal has nothing to show yet.
                </>
              )}
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
          subtitle="Real /metrics — platform-wide, not project-scoped: state counts plus the real request-latency histogram."
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

      <section id="telemetry-content" className="space-y-3">
        {renderBody()}
      </section>
    </div>
  );
};
