/**
 * Phase 6 — Performance Testing: real Load/Stress/Spike/Volume workload
 * generation against a real target — real HTTP requests, real per-request
 * latency samples, real aggregated metrics (min/avg/max/P50/P95/P99/
 * throughput/error rate), real cancellation, real persisted run history.
 *
 * Had no route, no page, and no backend at all before this phase (see
 * `routeConfig.tsx`'s own doc comment) — the real backend
 * (`performance_testing.rs`, `POST/GET .../performance-testing/*` in
 * `api.rs`) reuses the exact Phase 4 per-request execution core
 * (`executor::execute_plan_once`, extracted from `CompiledApiExecutor` in
 * this same phase) as the repeated worker underneath a new, minimal
 * concurrency-bounded scheduler — no second HTTP execution engine, no
 * `setTimeout`-simulated load anywhere in this page.
 *
 * "Start" kicks off a real background workload and returns immediately;
 * this page then polls `.../status` for real, live progress (real elapsed
 * time, real sample count, real current concurrency, metrics recomputed
 * from the real samples gathered so far) — there is no fabricated
 * percentage-complete anywhere in this file. The chart below renders only
 * from real `samples`; "No performance data yet" is shown, verbatim, until
 * at least one real sample exists.
 */
import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ComposedChart, Line, Scatter, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { Gauge, AlertTriangle, Play, Square, Clock, Zap, TrendingUp, ListChecks, History, ShieldAlert } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';
import { PerformanceFlowRail } from '../performance/PerformanceFlowRail';
import { useProject } from '../../context/ProjectContext';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { PerformanceRunRow, PerformanceRunStatusResponse, PerformanceWorkloadSchedule } from '../../types/contract';

const TEST_TYPES = ['LOAD', 'STRESS', 'SPIKE', 'VOLUME'] as const;
type TestType = (typeof TEST_TYPES)[number];

const TEST_TYPE_BLURB: Record<TestType, string> = {
  LOAD: 'A steady number of concurrent workers fire requests back-to-back for a fixed duration (or until a request cap is hit).',
  STRESS: 'Concurrency starts low and genuinely ramps up in real steps over real time, up to a maximum — reveals where the target starts to degrade.',
  SPIKE: 'Concurrency jumps immediately from a baseline to a much higher level in one burst (not a gradual ramp), held briefly, to see how the target handles a sudden surge.',
  VOLUME: 'A fixed, moderate concurrency drives a specific total number of requests to completion — the metric of interest is real total throughput, not an increasing-load curve.',
};

const POLL_INTERVAL_MS = 1000;

export const PerformanceTestingPage: React.FC = () => {
  const navigate = useNavigate();
  const { selectedProjectId: realProjectId, selectedProject } = useProject();

  const [testType, setTestType] = useState<TestType>('LOAD');
  const [method, setMethod] = useState<'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'>('GET');
  const [url, setUrl] = useState('');
  const [expectedStatus, setExpectedStatus] = useState('200');

  const [concurrency, setConcurrency] = useState('5');
  const [durationSecs, setDurationSecs] = useState('15');
  const [maxRequests, setMaxRequests] = useState('');

  const [startConcurrency, setStartConcurrency] = useState('2');
  const [maxConcurrency, setMaxConcurrency] = useState('20');
  const [stepSize, setStepSize] = useState('2');
  const [stepIntervalSecs, setStepIntervalSecs] = useState('5');
  const [maxDurationSecs, setMaxDurationSecs] = useState('30');

  const [baselineConcurrency, setBaselineConcurrency] = useState('3');
  const [spikeConcurrency, setSpikeConcurrency] = useState('25');
  const [baselineSecs, setBaselineSecs] = useState('5');
  const [spikeSecs, setSpikeSecs] = useState('10');

  const [totalRequests, setTotalRequests] = useState('100');

  const [maxP95Ms, setMaxP95Ms] = useState('');
  const [maxErrorRatePercent, setMaxErrorRatePercent] = useState('');
  const [minThroughputRps, setMinThroughputRps] = useState('');

  const [runId, setRunId] = useState<string | null>(null);
  const [live, setLive] = useState<PerformanceRunStatusResponse | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [history, setHistory] = useState<PerformanceRunRow[]>([]);

  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const isRunning = live?.status === 'RUNNING';

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  const refreshHistory = useCallback(async () => {
    if (!realProjectId) return;
    try {
      const rows = await aiqaApi.performanceTesting.list(realProjectId);
      setHistory(rows);
    } catch {
      // History is a convenience panel — a failure here must not block the
      // live run experience above it.
    }
  }, [realProjectId]);

  useEffect(() => {
    refreshHistory();
  }, [refreshHistory]);

  useEffect(() => stopPolling, [stopPolling]);

  /** Returns `true` only when the real run is still `RUNNING` after this
   *  poll — callers use this to decide whether it's worth arming/keeping
   *  the recurring interval, rather than always scheduling one and relying
   *  on a later tick to notice the run already finished. */
  const poll = useCallback(
    async (id: string): Promise<boolean> => {
      if (!realProjectId) return false;
      try {
        const status = await aiqaApi.performanceTesting.status(realProjectId, id);
        setLive(status);
        if (status.status !== 'RUNNING') {
          stopPolling();
          refreshHistory();
          return false;
        }
        return true;
      } catch (err) {
        stopPolling();
        setErrorMessage(err instanceof ApiError ? err.message : 'Failed to fetch live performance test status.');
        return false;
      }
    },
    [realProjectId, stopPolling, refreshHistory],
  );

  const buildSchedule = (): PerformanceWorkloadSchedule | { error: string } => {
    if (testType === 'LOAD') {
      const c = Number(concurrency);
      const d = Number(durationSecs);
      if (!Number.isFinite(c) || c < 1) return { error: 'Concurrency must be at least 1.' };
      if (!Number.isFinite(d) || d < 1) return { error: 'Duration must be at least 1 second.' };
      const mr = maxRequests.trim() ? Number(maxRequests) : undefined;
      if (maxRequests.trim() && (!Number.isFinite(mr) || (mr as number) < 1)) return { error: 'Max requests, if set, must be at least 1.' };
      return { test_type: 'LOAD', concurrency: c, duration_secs: d, max_requests: mr };
    }
    if (testType === 'STRESS') {
      const sc = Number(startConcurrency);
      const mc = Number(maxConcurrency);
      const ss = Number(stepSize);
      const si = Number(stepIntervalSecs);
      const md = Number(maxDurationSecs);
      if (![sc, mc, ss, si, md].every((n) => Number.isFinite(n) && n >= 1)) return { error: 'All Stress fields must be numbers of at least 1.' };
      if (mc < sc) return { error: 'Max concurrency must be greater than or equal to start concurrency.' };
      return { test_type: 'STRESS', start_concurrency: sc, max_concurrency: mc, step_size: ss, step_interval_secs: si, max_duration_secs: md };
    }
    if (testType === 'SPIKE') {
      const bc = Number(baselineConcurrency);
      const spc = Number(spikeConcurrency);
      const bs = Number(baselineSecs);
      const ss = Number(spikeSecs);
      if (![bc, spc, bs, ss].every((n) => Number.isFinite(n) && n >= 1)) return { error: 'All Spike fields must be numbers of at least 1.' };
      if (spc <= bc) return { error: 'Spike concurrency must be greater than baseline concurrency.' };
      return { test_type: 'SPIKE', baseline_concurrency: bc, spike_concurrency: spc, baseline_secs: bs, spike_secs: ss };
    }
    const c = Number(concurrency);
    const t = Number(totalRequests);
    if (!Number.isFinite(c) || c < 1) return { error: 'Concurrency must be at least 1.' };
    if (!Number.isFinite(t) || t < 1) return { error: 'Total requests must be at least 1.' };
    return { test_type: 'VOLUME', concurrency: c, total_requests: t };
  };

  const handleStart = async () => {
    if (!realProjectId) return;
    if (!url.trim()) {
      setErrorMessage('INVALID_URL: enter a target URL before starting.');
      return;
    }
    const schedule = buildSchedule();
    if ('error' in schedule) {
      setErrorMessage(schedule.error);
      return;
    }
    const status = expectedStatus.trim() ? Number(expectedStatus.trim()) : undefined;
    if (expectedStatus.trim() && (!Number.isInteger(status) || (status as number) < 100)) {
      setErrorMessage('Expected status, if set, must be a valid HTTP status code.');
      return;
    }

    setErrorMessage(null);
    setStarting(true);
    setLive(null);
    setRunId(null);
    stopPolling();

    try {
      const response = await aiqaApi.performanceTesting.start(realProjectId, {
        request: { method, url: url.trim() },
        expected_status: status,
        schedule,
        max_p95_ms: maxP95Ms.trim() ? Number(maxP95Ms.trim()) : undefined,
        max_error_rate_percent: maxErrorRatePercent.trim() ? Number(maxErrorRatePercent.trim()) : undefined,
        min_throughput_rps: minThroughputRps.trim() ? Number(minThroughputRps.trim()) : undefined,
      });
      setRunId(response.run_id);
      const stillRunning = await poll(response.run_id);
      if (stillRunning) {
        pollRef.current = setInterval(() => poll(response.run_id), POLL_INTERVAL_MS);
      }
    } catch (err) {
      setErrorMessage(err instanceof ApiError ? err.message : 'Failed to start the performance test.');
    } finally {
      setStarting(false);
    }
  };

  const handleCancel = async () => {
    if (!realProjectId || !runId) return;
    setCancelling(true);
    try {
      const result = await aiqaApi.performanceTesting.cancel(realProjectId, runId);
      if (!result.cancelled && result.reason) {
        setErrorMessage(`Cancel did not take effect: ${result.reason}`);
      }
      await poll(runId);
    } catch (err) {
      setErrorMessage(err instanceof ApiError ? err.message : 'Failed to cancel the performance test.');
    } finally {
      setCancelling(false);
    }
  };

  const chartData = (live?.samples ?? []).map((s) => ({ offset_s: Math.round(s.offset_ms / 100) / 10, duration_ms: s.duration_ms, concurrency_at_start: s.concurrency_at_start, outcome: s.outcome }));
  const errorPoints = chartData.filter((d) => d.outcome !== 'SUCCESS');

  return (
    <div id="performance-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Performance Testing"
        subtitle="Real Load, Stress, Spike, and Volume workloads against a real target — real per-request latency, real throughput, real percentiles. No simulated timing, no fabricated charts."
        badge={live && live.status !== 'RUNNING' ? live.status : undefined}
        icon={Gauge}
      />

      {!realProjectId ? (
        <EmptyState
          icon={AlertTriangle}
          badge="NO ACTIVE PROJECT"
          title="Select a project before running a performance test"
          description="Performance tests are project-scoped, same as API Testing — choose a project from the selector before configuring a workload."
          actionLabel="Go to Dashboard"
          onAction={() => navigate('/dashboard')}
        />
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start">
          <div className="lg:col-span-5 space-y-4">
            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-xs font-mono text-slate-400">Project Context</span>
                <span className="text-xs font-mono font-semibold text-yellow-300">{selectedProject?.name ?? realProjectId}</span>
              </div>

              <div className="flex gap-2">
                <select
                  id="perf-method"
                  value={method}
                  onChange={(e) => setMethod(e.target.value as typeof method)}
                  disabled={isRunning}
                  className="px-2 py-2 rounded-md bg-[#020617] border border-slate-700 text-sm font-mono font-bold text-yellow-300 cursor-pointer disabled:opacity-60"
                >
                  {['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
                <input
                  id="perf-url"
                  type="text"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://api.example.com/v1/health"
                  disabled={isRunning}
                  className="flex-1 min-w-0 px-3 py-2 rounded-md bg-[#020617] border border-slate-700 text-sm font-mono text-slate-100 placeholder-slate-600 focus:outline-none focus:border-violet-500 disabled:opacity-60"
                />
              </div>
              <div className="flex items-center gap-2">
                <label htmlFor="perf-expected-status" className="text-xs font-mono text-slate-400 shrink-0">
                  Expected status (optional)
                </label>
                <input
                  id="perf-expected-status"
                  type="text"
                  value={expectedStatus}
                  onChange={(e) => setExpectedStatus(e.target.value)}
                  placeholder="200"
                  disabled={isRunning}
                  className="w-24 px-2 py-1.5 rounded-md bg-[#020617] border border-slate-700 text-xs font-mono text-slate-100 disabled:opacity-60"
                />
                <span className="text-[10px] font-mono text-content-muted">A mismatch marks that sample FAILED, not ERROR.</span>
              </div>
            </div>

            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
              <div className="flex items-center gap-1.5 flex-wrap">
                {TEST_TYPES.map((t) => (
                  <button
                    key={t}
                    type="button"
                    id={`perf-type-${t.toLowerCase()}`}
                    onClick={() => setTestType(t)}
                    disabled={isRunning}
                    className={`px-3 py-1.5 rounded-md text-xs font-mono font-bold cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                      testType === t ? 'bg-violet-600 text-white' : 'bg-slate-900 border border-slate-700 text-slate-300 hover:border-slate-500'
                    }`}
                  >
                    {t}
                  </button>
                ))}
              </div>
              <p className="text-[10px] font-mono text-content-muted">{TEST_TYPE_BLURB[testType]}</p>

              {testType === 'LOAD' && (
                <div className="grid grid-cols-3 gap-2">
                  <NumField id="perf-load-concurrency" label="Concurrency" value={concurrency} onChange={setConcurrency} disabled={isRunning} />
                  <NumField id="perf-load-duration" label="Duration (s)" value={durationSecs} onChange={setDurationSecs} disabled={isRunning} />
                  <NumField id="perf-load-max-requests" label="Max requests (optional)" value={maxRequests} onChange={setMaxRequests} disabled={isRunning} />
                </div>
              )}
              {testType === 'STRESS' && (
                <div className="grid grid-cols-3 gap-2">
                  <NumField id="perf-stress-start" label="Start concurrency" value={startConcurrency} onChange={setStartConcurrency} disabled={isRunning} />
                  <NumField id="perf-stress-max" label="Max concurrency" value={maxConcurrency} onChange={setMaxConcurrency} disabled={isRunning} />
                  <NumField id="perf-stress-step" label="Step size" value={stepSize} onChange={setStepSize} disabled={isRunning} />
                  <NumField id="perf-stress-step-interval" label="Step interval (s)" value={stepIntervalSecs} onChange={setStepIntervalSecs} disabled={isRunning} />
                  <NumField id="perf-stress-max-duration" label="Max duration (s)" value={maxDurationSecs} onChange={setMaxDurationSecs} disabled={isRunning} />
                </div>
              )}
              {testType === 'SPIKE' && (
                <div className="grid grid-cols-2 gap-2">
                  <NumField id="perf-spike-baseline-c" label="Baseline concurrency" value={baselineConcurrency} onChange={setBaselineConcurrency} disabled={isRunning} />
                  <NumField id="perf-spike-spike-c" label="Spike concurrency" value={spikeConcurrency} onChange={setSpikeConcurrency} disabled={isRunning} />
                  <NumField id="perf-spike-baseline-s" label="Baseline duration (s)" value={baselineSecs} onChange={setBaselineSecs} disabled={isRunning} />
                  <NumField id="perf-spike-spike-s" label="Spike duration (s)" value={spikeSecs} onChange={setSpikeSecs} disabled={isRunning} />
                </div>
              )}
              {testType === 'VOLUME' && (
                <div className="grid grid-cols-2 gap-2">
                  <NumField id="perf-volume-concurrency" label="Concurrency" value={concurrency} onChange={setConcurrency} disabled={isRunning} />
                  <NumField id="perf-volume-total" label="Total requests" value={totalRequests} onChange={setTotalRequests} disabled={isRunning} />
                </div>
              )}
            </div>

            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-2">
              <div className="flex items-center gap-1.5 text-xs font-mono text-slate-400">
                <ListChecks className="w-3.5 h-3.5" /> Pass/Fail Thresholds (optional)
              </div>
              <p className="text-[10px] font-mono text-content-muted">Leave all blank to get a real metrics report with no pass/fail judgement (status COMPLETED, not a false PASS).</p>
              <div className="grid grid-cols-3 gap-2">
                <NumField id="perf-threshold-p95" label="Max P95 (ms)" value={maxP95Ms} onChange={setMaxP95Ms} disabled={isRunning} />
                <NumField id="perf-threshold-error-rate" label="Max error rate (%)" value={maxErrorRatePercent} onChange={setMaxErrorRatePercent} disabled={isRunning} />
                <NumField id="perf-threshold-throughput" label="Min throughput (req/s)" value={minThroughputRps} onChange={setMinThroughputRps} disabled={isRunning} />
              </div>
            </div>

            <div className="flex items-center justify-between">
              {getStatusBadge(live)}
              <div className="flex items-center gap-2">
                {isRunning && (
                  <button
                    type="button"
                    id="btn-cancel-performance-test"
                    onClick={handleCancel}
                    disabled={cancelling}
                    className="flex items-center gap-1.5 px-4 py-2 rounded-md bg-rose-950 hover:bg-rose-900 border border-rose-700 text-rose-300 font-mono text-xs font-bold cursor-pointer transition-colors disabled:opacity-50"
                  >
                    <Square className="w-3.5 h-3.5" />
                    <span>{cancelling ? 'Cancelling…' : 'Cancel'}</span>
                  </button>
                )}
                <button
                  type="button"
                  id="btn-start-performance-test"
                  onClick={handleStart}
                  disabled={starting || isRunning}
                  className="flex items-center gap-1.5 px-5 py-2 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-mono text-xs font-bold cursor-pointer transition-colors shadow-md disabled:opacity-60 disabled:cursor-not-allowed"
                >
                  <Play className="w-4 h-4" />
                  <span>{starting ? 'Starting…' : isRunning ? 'Running…' : 'Start'}</span>
                </button>
              </div>
            </div>
            {errorMessage && (
              <div id="perf-error" className="p-3 rounded-md bg-rose-950/60 border border-rose-700 text-xs font-mono text-rose-300">
                {errorMessage}
              </div>
            )}

            <HistoryPanel history={history} />
          </div>

          <div className="lg:col-span-5 space-y-4">
            {live ? (
              <>
                <LiveMetricsPanel live={live} />
                {live.status === 'ERROR' && (
                  <div id="perf-execution-error" className="p-4 rounded-lg bg-rose-950/60 border border-rose-700 shadow-sm space-y-1.5">
                    <div className="flex items-center gap-1.5 text-xs font-mono font-bold text-rose-300">
                      <ShieldAlert className="w-3.5 h-3.5" /> Execution Error
                    </div>
                    <p className="text-[11px] font-mono text-rose-200">{live.error_reason ?? 'The performance execution task terminated unexpectedly.'}</p>
                    <p className="text-[10px] font-mono text-rose-400/80">
                      This is a genuine internal execution failure, distinct from a threshold FAIL or a per-request error — real samples collected before the failure are preserved above, not discarded.
                    </p>
                  </div>
                )}
                <ChartPanel chartData={chartData} errorPoints={errorPoints} />
                {live.status !== 'RUNNING' && live.status !== 'ERROR' && live.verdict_reasons.length > 0 && (
                  <div id="perf-verdict-reasons" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-1.5">
                    <div className="flex items-center gap-1.5 text-xs font-mono text-slate-400">
                      <TrendingUp className="w-3.5 h-3.5" /> Verdict Reasons
                    </div>
                    {live.verdict_reasons.map((r, i) => (
                      <p key={i} className="text-[11px] font-mono text-slate-300">
                        • {r}
                      </p>
                    ))}
                  </div>
                )}
              </>
            ) : (
              <div className="p-6 rounded-lg bg-[#0B1120] border border-slate-800 border-dashed text-center text-xs font-mono text-content-muted">Configure a workload and click Start to see real, live progress here.</div>
            )}
          </div>

          <div className="hidden lg:block lg:col-span-2 min-w-0">
            <PerformanceFlowRail testType={testType} method={method} live={live} />
          </div>
        </div>
      )}
    </div>
  );
};

const NumField: React.FC<{ id: string; label: string; value: string; onChange: (v: string) => void; disabled: boolean }> = ({ id, label, value, onChange, disabled }) => (
  <div>
    <label htmlFor={id} className="block text-[10px] font-mono text-content-muted mb-0.5">
      {label}
    </label>
    <input
      id={id}
      type="text"
      inputMode="numeric"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      disabled={disabled}
      className="w-full px-2 py-1.5 rounded-md bg-[#020617] border border-slate-700 text-xs font-mono text-slate-100 disabled:opacity-60"
    />
  </div>
);

function getStatusBadge(live: PerformanceRunStatusResponse | null) {
  if (!live) {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-semibold bg-slate-900 border border-slate-700 text-yellow-400">
        <span className="w-2 h-2 rounded-full bg-yellow-400 animate-pulse" />
        Ready
      </span>
    );
  }
  const map: Record<string, { cls: string; icon: React.ReactNode; label: string }> = {
    RUNNING: { cls: 'bg-purple-950/80 border-purple-700/80 text-purple-300', icon: <Zap className="w-3.5 h-3.5 animate-pulse" />, label: 'Running (real, live)' },
    COMPLETED: { cls: 'bg-slate-900 border-slate-600 text-slate-300', icon: <Clock className="w-3.5 h-3.5" />, label: live.verdict === 'PASSED' ? 'Passed' : live.verdict === 'FAILED' ? 'Failed' : 'Completed' },
    CANCELLED: { cls: 'bg-amber-950/80 border-amber-700/80 text-amber-300', icon: <Square className="w-3.5 h-3.5" />, label: 'Cancelled' },
    ERROR: { cls: 'bg-rose-950/80 border-rose-700/80 text-rose-300', icon: <ShieldAlert className="w-3.5 h-3.5" />, label: 'Error' },
    INTERRUPTED: { cls: 'bg-rose-950/80 border-rose-700/80 text-rose-300', icon: <ShieldAlert className="w-3.5 h-3.5" />, label: 'Interrupted' },
  };
  const effective = live.status === 'COMPLETED' && live.verdict === 'FAILED' ? 'COMPLETED' : live.status;
  const entry = map[effective] ?? map.COMPLETED;
  const cls = live.status === 'COMPLETED' && live.verdict === 'PASSED' ? 'bg-emerald-950/80 border-emerald-700/80 text-emerald-300' : live.status === 'COMPLETED' && live.verdict === 'FAILED' ? 'bg-rose-950/80 border-rose-700/80 text-rose-300' : entry.cls;
  return <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold border ${cls}`}>{entry.icon} {entry.label}</span>;
}

const LiveMetricsPanel: React.FC<{ live: PerformanceRunStatusResponse }> = ({ live }) => {
  const m = live.metrics;
  return (
    <div id="perf-live-metrics" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
      <div className="flex items-center justify-between pb-2 border-b border-slate-800/80">
        <span className="text-sm font-mono font-bold text-slate-200">{live.name}</span>
        <span className="text-[10px] font-mono text-content-muted">{live.live ? 'live, real-time' : 'from persisted history'}</span>
      </div>
      <div className="grid grid-cols-3 gap-3 text-xs font-mono">
        <Stat label="Elapsed" value={`${(live.elapsed_ms / 1000).toFixed(1)}s`} />
        <Stat label="Current concurrency" value={String(live.current_concurrency)} />
        <Stat label="Samples so far" value={String(live.sample_count)} />
      </div>
      {m ? (
        <div className="grid grid-cols-3 gap-3 text-xs font-mono pt-2 border-t border-slate-800/80">
          <Stat label="Total / Success / Failed / Error" value={`${m.total_requests} / ${m.successful} / ${m.failed} / ${m.errored}`} />
          <Stat label="Error rate" value={`${m.error_rate_percent.toFixed(2)}%`} />
          <Stat label="Success rate" value={`${m.success_rate_percent.toFixed(2)}%`} />
          <Stat label="Min / Avg / Max (ms)" value={`${m.min_ms} / ${m.avg_ms.toFixed(1)} / ${m.max_ms}`} />
          <Stat label="P50 / P95 / P99 (ms)" value={`${m.p50_ms} / ${m.p95_ms} / ${m.p99_ms}`} />
          <Stat label="Throughput" value={`${m.throughput_rps.toFixed(2)} req/s`} />
        </div>
      ) : (
        <p className="text-[11px] font-mono text-content-muted">No performance data yet.</p>
      )}
    </div>
  );
};

const Stat: React.FC<{ label: string; value: string }> = ({ label, value }) => (
  <div>
    <div className="text-content-muted mb-0.5">{label}</div>
    <div className="text-slate-200">{value}</div>
  </div>
);

const ChartPanel: React.FC<{ chartData: { offset_s: number; duration_ms: number; concurrency_at_start: number; outcome: string }[]; errorPoints: { offset_s: number; duration_ms: number }[] }> = ({ chartData, errorPoints }) => (
  <div id="perf-chart" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
    <div className="text-xs font-mono text-slate-400 mb-2">Latency &amp; Concurrency Over Time (real samples)</div>
    {chartData.length === 0 ? (
      <p className="text-[11px] font-mono text-content-muted py-10 text-center">No performance data yet.</p>
    ) : (
      <ResponsiveContainer width="100%" height={220}>
        <ComposedChart data={chartData} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#1e293b" />
          <XAxis dataKey="offset_s" tick={{ fontSize: 10, fill: '#64748b' }} label={{ value: 's', position: 'insideBottomRight', fontSize: 10, fill: '#64748b' }} />
          <YAxis yAxisId="left" tick={{ fontSize: 10, fill: '#64748b' }} label={{ value: 'ms', angle: -90, position: 'insideLeft', fontSize: 10, fill: '#64748b' }} />
          <YAxis yAxisId="right" orientation="right" tick={{ fontSize: 10, fill: '#64748b' }} label={{ value: 'concurrency', angle: 90, position: 'insideRight', fontSize: 10, fill: '#64748b' }} />
          <Tooltip contentStyle={{ background: '#0B1120', border: '1px solid #1e293b', fontSize: 11, fontFamily: 'monospace' }} />
          <Legend wrapperStyle={{ fontSize: 10, fontFamily: 'monospace' }} />
          <Line yAxisId="left" type="monotone" dataKey="duration_ms" name="latency (ms)" stroke="#a78bfa" dot={false} isAnimationActive={false} />
          <Line yAxisId="right" type="stepAfter" dataKey="concurrency_at_start" name="concurrency" stroke="#f59e0b" dot={false} isAnimationActive={false} />
          {errorPoints.length > 0 && <Scatter yAxisId="left" data={errorPoints} dataKey="duration_ms" name="failed/error" fill="#f87171" />}
        </ComposedChart>
      </ResponsiveContainer>
    )}
  </div>
);

const HistoryPanel: React.FC<{ history: PerformanceRunRow[] }> = ({ history }) => (
  <div id="perf-history" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-2">
    <div className="flex items-center gap-1.5 text-xs font-mono text-slate-400">
      <History className="w-3.5 h-3.5" /> Recent Runs (persisted)
    </div>
    {history.length === 0 ? (
      <p className="text-[11px] font-mono text-content-muted">No performance test runs yet for this project.</p>
    ) : (
      <div className="space-y-1 max-h-40 overflow-y-auto">
        {history.slice(0, 10).map((r) => (
          <div key={r.id} className="flex items-center justify-between px-2 py-1.5 rounded bg-[#020617] border border-slate-800 text-[11px] font-mono">
            <span className="text-slate-300 truncate">{r.name}</span>
            <span className={`shrink-0 ml-2 font-bold ${r.verdict === 'PASSED' ? 'text-emerald-400' : r.verdict === 'FAILED' ? 'text-rose-400' : r.status === 'RUNNING' ? 'text-purple-400' : 'text-slate-400'}`}>{r.verdict ?? r.status}</span>
          </div>
        ))}
      </div>
    )}
  </div>
);
