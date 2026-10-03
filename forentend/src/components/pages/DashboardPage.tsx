/**
 * AIQA Dashboard Page — Main Content Experience
 * Futuristic AI Quality Engineering Control Plane
 *
 * FE-10 — real backend data, approved visual design preserved unchanged.
 *
 * Backend contract (re-verified this sprint, `services/orchestrator/src/
 * api.rs::get_project_dashboard`/`get_project_kpis`, `store.rs`'s
 * `dashboard_*` queries): three already-real, already-FE-1-exposed
 * endpoints, all reused as-is (`aiqaApi.dashboard.get`, `aiqaApi.dashboard.
 * kpis`, `aiqaApi.executions.list` — zero new transport code):
 *
 *   GET /api/v1/projects/:id/dashboard  → execution_counts (by RunState),
 *                                          task_counts (by TaskStatus),
 *                                          pass_fail, test_count (distinct
 *                                          `quality_nodes` of type 'test' —
 *                                          genuinely 0 today since Test
 *                                          Generation does not persist;
 *                                          that is the real, honest value,
 *                                          not a bug), generated_at.
 *   GET /api/v1/projects/:id/kpis       → execution_reliability (PASSED /
 *                                          (PASSED+FAILED+TIMEOUT), `null`
 *                                          until any task reaches one of
 *                                          those), execution_efficiency_ms
 *                                          (avg task duration, `null` until
 *                                          any task has both timestamps),
 *                                          execution_avg_run_duration_ms
 *                                          (FE-26/BE-12b — avg *run*
 *                                          duration, `null` until any run
 *                                          has both `started_at`/
 *                                          `finished_at`; no frontend
 *                                          calculation, no new request —
 *                                          the existing `aiqaApi.dashboard.
 *                                          kpis` call already returns it).
 *   GET /api/v1/projects/:id/executions → reused (already existed for FE-8)
 *                                          to find the real active (RUNNING)
 *                                          run, if any, for the "Current
 *                                          Execution Run" panel; its real
 *                                          detail (`executions.get`) is
 *                                          fetched once more, only when a
 *                                          RUNNING run actually exists.
 *
 * Every KPI card, the donut chart, and the "Current Execution Run" panel
 * now render exactly these real values — see the widget-by-widget mapping
 * in `AIQA_PHASE4_FE10_DASHBOARD_REAL_DATA_IMPLEMENTATION_REPORT.md` §3/§5.
 *
 * Five disclosed, minimal, documented adjustments (brief's own explicit
 * allowance: "unless a real backend requirement makes a minimal functional
 * adjustment necessary... must be documented") — no other markup, layout,
 * spacing, or styling was touched:
 *
 *  1. **Project status badge** (`ProjectStatus`) no longer claims
 *     Warning/Degraded/Critical/Offline — none of those are backend-
 *     computable (no health-scoring model exists; inventing thresholds
 *     for them would be exactly the fabricated "health score" brief §13
 *     forbids). Only two real states remain: `'Running'` (a real
 *     `RUNNING`-state execution genuinely exists) and `'Healthy'` (the
 *     neutral, least-fabricated available default when nothing is
 *     running — this project is reachable and returned real data; it is
 *     not "Offline", and nothing observed makes "Critical" true either).
 *  2. **Environment/Slug** (`selectedProject.environment`/`.slug`) do not
 *     exist on the real `Project` type (`{id, name, description,
 *     created_at, updated_at}`) — replaced in the same visual slots with
 *     the real `created_at` and a real, truncated project id.
 *  3. **"Last Activity"** (previously a fabricated relative-time string)
 *     is relabeled "Data as of" and shows the real `generated_at` the
 *     dashboard endpoint itself returned — an honest description of what
 *     that real timestamp actually is, not an invented activity feed.
 *  4. **"Est. Duration"** on the active-run panel (previously a fabricated
 *     ETA) is relabeled "Elapsed" and computed client-side from the real
 *     run's real `started_at` — no real ETA/estimate concept exists
 *     anywhere in the backend.
 *  5. **The header's own project dropdown** now lists FE-5's real
 *     `useProject().projects` and calls the real `selectProject()` —
 *     previously it listed `AIQAContext`'s fake `AIQAProject[]` completely
 *     independently of the real header selector (FE-5's own disclosed
 *     gap, `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md` §3.4).
 *     Leaving two independent "current project" concepts on one page once
 *     its KPIs are real would be actively misleading, not merely
 *     decorative, so this one is fixed rather than preserved.
 *
 * **FE-11 update**: the "Launch Run" / "Start Execution Run" buttons no
 * longer open `AIQAContext`'s fabricated `RunTriggerModal` (deleted this
 * sprint, along with the context state that opened it) — both now
 * `navigate('/execution')`, the same real execution-creation surface as
 * the unchanged "Execution Hub" quick-action, per brief §24's explicit
 * "do not maintain two competing execution-creation systems." This page no
 * longer reads `AIQAContext` at all. `AIQualityCoreOrb`'s decorative
 * animation and copy are unchanged in every respect except the two real
 * props it now receives (`hasActiveRun`, `status`) — see adjustment 1.
 *
 * **The "Execution Activity" timeline chart is always rendered with its
 * own pre-existing, real, honest empty state** (`ProjectExecutionGraph`'s
 * `hasData` branch, unmodified) — the real backend has no time-bucketed
 * history endpoint at all (only aggregate counts), and building one
 * client-side would mean inventing bucket boundaries/gap-filling this
 * backend does not define (brief §14: "do not fabricate a time series").
 * Disclosed as an explicitly unsupported widget, not silently left
 * ambiguous.
 *
 * No FRD/SRD/PRD/flow-diagram contradiction was found for Dashboard's real
 * KPI/chart data specifically — FRD-RT-02 ("update dashboards
 * incrementally") and Business Logic §10 ("events update the UI
 * incrementally... idempotent") describe exactly the realtime-revalidation
 * architecture below (§9 of the FE-10 report). The one already-known,
 * already-disclosed FE-9 gap (no real Dashboard/KPI-specific WebSocket
 * event type exists) is not re-litigated here — see that finding's own
 * treatment in `RealtimeContext.tsx`'s module doc comment.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Layers,
  Play,
  CheckCircle2,
  XCircle,
  Ban,
  HelpCircle,
  Activity,
  Server,
  Clock,
  ChevronDown,
  ArrowUpRight,
  Radio,
  RotateCcw,
  FolderOpen,
  Fingerprint,
} from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import { useAiStatus } from '../../hooks/useAiStatus';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { AIEnginePanel } from '../dashboard/AIEnginePanel';
import { ProjectStatusDonut } from '../dashboard/ProjectStatusDonut';
import { ProjectExecutionGraph } from '../dashboard/ProjectExecutionGraph';
import { OrchestratorLogsPanel } from '../dashboard/OrchestratorLogsPanel';
import type { ProjectMetrics, ProjectStatus } from '../../types';
import type { DashboardResponse, KpisResponse, RunListItem, RunDetail } from '../../types/contract';

/** A burst of task/run events for the same project collapses into one
 *  resync, never one per event (same pattern/reasoning as FE-9's own
 *  `EVENT_RESYNC_DEBOUNCE_MS`). */
const EVENT_RESYNC_DEBOUNCE_MS = 400;

function formatTimestamp(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { month: 'short', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function formatDate(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: '2-digit' });
}

/** Real client-side derivation from a real timestamp — never a fabricated
 *  ETA (no such concept exists in the backend contract). */
function formatElapsed(startedAtIso: string): string {
  const started = new Date(startedAtIso).getTime();
  if (Number.isNaN(started)) return '—';
  const ms = Math.max(0, Date.now() - started);
  const totalSeconds = Math.floor(ms / 1000);
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

function formatMs(ms: number | null): string {
  if (ms === null) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function formatPct(ratio: number | null): string {
  if (ratio === null) return '—';
  return `${(ratio * 100).toFixed(1)}%`;
}

function count(map: Record<string, number>, key: string): number {
  return map[key] ?? 0;
}

interface DashboardData {
  dashboard: DashboardResponse;
  kpis: KpisResponse;
  runs: RunListItem[];
  activeRunDetail: RunDetail | null;
}

export const DashboardPage: React.FC = () => {
  const { projects, selectedProjectId, selectedProject, selectProject } = useProject();
  const realtime = useRealtime();
  const navigate = useNavigate();

  const [data, setData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  /** Real AI engine state (`GET /api/v1/ai/status`), shared with the Test
   *  Case Generator's own AI Engine rail (`hooks/useAiStatus.ts`) so both
   *  pages read the same fetch/poll logic rather than two copies drifting
   *  apart. Platform-wide, not project-scoped: it must not be cleared or
   *  reloaded just because the selected project changed, and a failure to
   *  reach it must not blank out the project dashboard beside it. */
  const { status: aiStatus, loading: aiLoading, error: aiError, retry: loadAiStatus } = useAiStatus();

  const fetchAll = useCallback(async (projectId: string, isRefresh: boolean) => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    if (isRefresh) setRefreshing(true);
    else setLoading(true);
    setError(null);
    try {
      const [dashboard, kpis, runs] = await Promise.all([
        aiqaApi.dashboard.get(projectId, controller.signal),
        aiqaApi.dashboard.kpis(projectId, controller.signal),
        aiqaApi.executions.list(projectId, {}, controller.signal),
      ]);
      if (controller.signal.aborted) return;

      // Real active-run detail — one extra real request, only when a real
      // RUNNING run genuinely exists (never a request per widget/KPI).
      const runningRun = runs.find((r) => r.state === 'RUNNING');
      let activeRunDetail: RunDetail | null = null;
      if (runningRun) {
        try {
          activeRunDetail = await aiqaApi.executions.get(projectId, runningRun.run_id, controller.signal);
        } catch (err) {
          if (controller.signal.aborted) return;
          // The list already proved a running execution exists; a detail-
          // fetch failure just means the panel falls back to the list's
          // own real (if less detailed) fields below — never fabricated.
          if (!(err instanceof DOMException && err.name === 'AbortError')) activeRunDetail = null;
        }
      }
      if (controller.signal.aborted) return;

      setData({ dashboard, kpis, runs, activeRunDetail });
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setError(apiErr);
      // Real API failure — never keep a stale/fabricated dashboard on
      // screen next to an error state (brief §7: no fabricated fallback).
      setData(null);
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, []);

  useEffect(() => {
    if (!selectedProjectId) {
      abortRef.current?.abort();
      setData(null);
      setError(null);
      setLoading(false);
      return;
    }
    setData(null);
    void fetchAll(selectedProjectId, false);
  }, [selectedProjectId, fetchAll]);

  useEffect(() => () => abortRef.current?.abort(), []);

  const handleRefresh = useCallback(() => {
    if (!selectedProjectId || loading || refreshing) return;
    void fetchAll(selectedProjectId, true);
  }, [selectedProjectId, loading, refreshing, fetchAll]);

  // FE-9 realtime integration — any real event for this project could
  // change these aggregate counts, so any event is the honest trigger for
  // a debounced resync via the *same* real fetch above, never a per-widget
  // subscription and never a hand-patched partial update (dashboard/kpis
  // are aggregate SQL queries, not derivable from one event's payload).
  // Depends on `realtime.subscribe` specifically (stable identity), not
  // the whole `realtime` object — see FE-9's own live-discovered bug about
  // exactly this mistake (`RealtimeContext.tsx`'s module doc comment).
  const resyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!selectedProjectId) return;
    const unsubscribe = realtime.subscribe(() => {
      if (resyncTimeoutRef.current) clearTimeout(resyncTimeoutRef.current);
      resyncTimeoutRef.current = setTimeout(() => {
        resyncTimeoutRef.current = null;
        void fetchAll(selectedProjectId, true);
      }, EVENT_RESYNC_DEBOUNCE_MS);
    });
    return () => {
      unsubscribe();
      if (resyncTimeoutRef.current) clearTimeout(resyncTimeoutRef.current);
    };
  }, [selectedProjectId, realtime.subscribe, fetchAll]);

  const getStatusBadge = (status: ProjectStatus) => {
    if (status === 'Running') {
      return (
        <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-semibold bg-yellow-950/80 border border-yellow-700/80 text-yellow-300">
          <span className="w-1.5 h-1.5 rounded-full bg-yellow-400 animate-ping" />
          Running
        </span>
      );
    }
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-semibold bg-emerald-950/80 border border-emerald-700/80 text-emerald-300">
        <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
        Healthy
      </span>
    );
  };

  const renderBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector above (or the header) to view its real dashboard data."
        />
      );
    }

    if (loading) {
      return (
        <StateCard
          icon={Activity}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading dashboard"
          description={`Fetching real execution and quality data for ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }

    if (error) {
      return renderResourceErrorState(error, handleRefresh, {
        forbiddenTitle: 'Not permitted to view this dashboard',
        forbiddenDescription: `This key's role or project scope does not permit viewing ${selectedProject?.name ?? 'this project'}'s dashboard.`,
        offlineDescriptionSuffix: 'to load real dashboard data.',
        genericTitle: "Couldn't load the dashboard",
      });
    }

    if (!data) return null;

    const { dashboard, kpis, runs, activeRunDetail } = data;
    const totalTasks = Object.keys(dashboard.task_counts).reduce((sum, key) => sum + dashboard.task_counts[key], 0);

    const metrics: ProjectMetrics = {
      totalTestCases: totalTasks,
      activeRunsCount: count(dashboard.execution_counts, 'RUNNING'),
      passed: count(dashboard.task_counts, 'PASSED'),
      failed: count(dashboard.task_counts, 'FAILED'),
      blocked: count(dashboard.task_counts, 'BLOCKED'),
      needsReview: count(dashboard.task_counts, 'NEEDS_REVIEW'),
      skipped: count(dashboard.task_counts, 'SKIPPED'),
      running: count(dashboard.task_counts, 'RUNNING'),
      queued: count(dashboard.task_counts, 'QUEUED'),
    };

    const runningRun = runs.find((r) => r.state === 'RUNNING') ?? null;
    const activeRun = runningRun
      ? {
          id: runningRun.run_id,
          title: runningRun.name,
          status: runningRun.state,
          startedAt: activeRunDetail ? formatTimestamp(activeRunDetail.started_at) : '—',
          elapsed: activeRunDetail ? formatElapsed(activeRunDetail.started_at) : '—',
          testsExecuted: activeRunDetail
            ? activeRunDetail.passed +
              activeRunDetail.failed +
              activeRunDetail.blocked +
              activeRunDetail.skipped +
              activeRunDetail.cancelled +
              activeRunDetail.timeout +
              activeRunDetail.infra_error +
              activeRunDetail.needs_review
            : 0,
          totalTests: activeRunDetail?.total ?? runningRun.total,
          progressPct: activeRunDetail && activeRunDetail.total > 0
            ? Math.round(
                ((activeRunDetail.passed +
                  activeRunDetail.failed +
                  activeRunDetail.blocked +
                  activeRunDetail.skipped +
                  activeRunDetail.cancelled +
                  activeRunDetail.timeout +
                  activeRunDetail.infra_error +
                  activeRunDetail.needs_review) /
                  activeRunDetail.total) *
                  100,
              )
            : 0,
        }
      : null;

    const status: ProjectStatus = activeRun ? 'Running' : 'Healthy';
    const projectIdShort = dashboard.project_id.slice(0, 8);

    return (
      <>
        {/* 2. KPI Cards Row */}
        <section id="dashboard-kpi-row" aria-label="Key Performance Indicators">
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2.5">
            {/* Executed Tests.

                This card was previously labelled "Total Test Cases", which
                was wrong: `dashboard.test_count` is
                `COUNT(quality_nodes WHERE node_type = 'test')` — the number
                of distinct tests the execution engine has actually *run* in
                this project (upserted per task name by
                `store.rs::upsert_quality_node`). It has nothing to do with
                the `test_cases` table, so a project with zero saved test
                cases could still show a non-zero number here purely from
                execution runs — exactly the contradiction the old label
                produced. The value was always real; only the name was not. */}
            <div className="p-3 rounded-lg bg-surface-card border border-border-default hover:border-slate-700 transition-all shadow-sm flex flex-col justify-between">
              <div className="flex items-center justify-between text-content-secondary">
                <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                  Executed Tests
                </span>
                <Layers className="w-3.5 h-3.5 text-content-secondary" />
              </div>
              <div className="mt-2">
                <div className="text-xl font-bold font-mono text-content-primary">
                  {dashboard.test_count.toLocaleString()}
                </div>
                <div className="text-[10px] text-content-secondary font-sans truncate mt-0.5">
                  distinct tests run
                </div>
              </div>
            </div>

            {/* Active Runs */}
            <div
              className={`p-3 rounded-lg bg-surface-card border ${
                metrics.activeRunsCount > 0 ? 'border-yellow-800/80 bg-yellow-950/20' : 'border-border-default'
              } hover:border-slate-700 transition-all shadow-sm flex flex-col justify-between`}
            >
              <div className="flex items-center justify-between text-content-secondary">
                <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                  Active Runs
                </span>
                <Play
                  className={`w-3.5 h-3.5 ${metrics.activeRunsCount > 0 ? 'text-yellow-400 animate-pulse' : 'text-content-muted'}`}
                />
              </div>
              <div className="mt-2">
                <div className={`text-xl font-bold font-mono ${metrics.activeRunsCount > 0 ? 'text-yellow-300' : 'text-content-primary'}`}>
                  {metrics.activeRunsCount}
                </div>
                <div className="text-[10px] text-content-secondary font-sans truncate mt-0.5">
                  {metrics.activeRunsCount > 0 ? 'Executing queue' : 'Idle runner pool'}
                </div>
              </div>
            </div>

            {/* Passed */}
            <div className="p-3 rounded-lg bg-surface-card border border-emerald-900/60 bg-emerald-950/10 hover:border-emerald-700/60 transition-all shadow-sm flex flex-col justify-between">
              <div className="flex items-center justify-between text-emerald-400">
                <span className="text-[10px] uppercase font-mono font-medium tracking-wider">Passed</span>
                <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
              </div>
              <div className="mt-2">
                <div className="text-xl font-bold font-mono text-emerald-300">{metrics.passed.toLocaleString()}</div>
                <div className="text-[10px] text-emerald-400/80 font-sans truncate mt-0.5">
                  {kpis.execution_reliability !== null
                    ? `${formatPct(kpis.execution_reliability)} reliability`
                    : 'No terminal outcomes yet'}
                </div>
              </div>
            </div>

            {/* Failed */}
            <div
              className={`p-3 rounded-lg bg-surface-card border ${
                metrics.failed > 0 ? 'border-rose-900/60 bg-rose-950/20' : 'border-border-default'
              } hover:border-rose-700/60 transition-all shadow-sm flex flex-col justify-between`}
            >
              <div className="flex items-center justify-between text-rose-400">
                <span className="text-[10px] uppercase font-mono font-medium tracking-wider">Failed</span>
                <XCircle className="w-3.5 h-3.5 text-rose-400" />
              </div>
              <div className="mt-2">
                <div className={`text-xl font-bold font-mono ${metrics.failed > 0 ? 'text-rose-300' : 'text-content-secondary'}`}>
                  {metrics.failed.toLocaleString()}
                </div>
                <div className="text-[10px] text-rose-400/80 font-sans truncate mt-0.5">
                  {metrics.failed > 0 ? 'Defect triage needed' : 'Zero failures'}
                </div>
              </div>
            </div>

            {/* Blocked */}
            <div className="p-3 rounded-lg bg-surface-card border border-purple-900/50 bg-purple-950/10 hover:border-purple-700/60 transition-all shadow-sm flex flex-col justify-between">
              <div className="flex items-center justify-between text-purple-400">
                <span className="text-[10px] uppercase font-mono font-medium tracking-wider">Blocked</span>
                <Ban className="w-3.5 h-3.5 text-purple-400" />
              </div>
              <div className="mt-2">
                <div className="text-xl font-bold font-mono text-purple-300">{metrics.blocked.toLocaleString()}</div>
                <div className="text-[10px] text-purple-400/80 font-sans truncate mt-0.5">Dependency blocked</div>
              </div>
            </div>

            {/* Needs Review */}
            <div
              className={`p-3 rounded-lg bg-surface-card border ${
                metrics.needsReview > 0 ? 'border-amber-900/60 bg-amber-950/20' : 'border-border-default'
              } hover:border-amber-700/60 transition-all shadow-sm flex flex-col justify-between`}
            >
              <div className="flex items-center justify-between text-amber-400">
                <span className="text-[10px] uppercase font-mono font-medium tracking-wider">Needs Review</span>
                <HelpCircle className="w-3.5 h-3.5 text-amber-400" />
              </div>
              <div className="mt-2">
                <div className={`text-xl font-bold font-mono ${metrics.needsReview > 0 ? 'text-amber-300' : 'text-content-secondary'}`}>
                  {metrics.needsReview.toLocaleString()}
                </div>
                <div className="text-[10px] text-amber-400/80 font-sans truncate mt-0.5">AI anomaly check</div>
              </div>
            </div>
          </div>
        </section>

        {/* Removed: the "AIQA Neural Intelligence Graph" — a real (not
            fabricated) but genuinely redundant re-visualization of the exact
            same numbers the KPI row above it already shows, plus its own
            extra project-scoped fetches (test-cases/executions/audit) just
            to compute them a second time. Its node-click "detail" popups
            never showed more than a small subset of what the dedicated
            Test Case Generator/Validator, Execution Hub, Evidence Viewer,
            and Audit Trail pages already show in full, and it was read-only
            — no action was ever reachable from it that wasn't already
            reachable from those pages. Deleted entirely (component tree,
            `neuralGraph.config.ts`/`.types.ts`, `useNeuralGraphData.ts`),
            not just unmounted here — this codebase's own established
            convention for a feature confirmed to have no real remaining
            purpose (see e.g. FE-16's `runs`/`triggerNewRun` removal). */}

        {/* 3. Middle Section: Current Project Details + Futuristic AI Orb */}
        <section id="dashboard-middle-section" className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-stretch">
          {/* Left: Current Project Detail (7 Cols on desktop) */}
          <div
            id="current-project-detail-panel"
            /* `self-start` so this card sizes to its own content instead of
               being stretched to the height of its neighbour. `items-stretch`
               on the grid below is the CSS default for grid items, so removing
               it alone changes nothing — this card has to opt out explicitly.
               Measured before the fix: the AI Engine panel next to it is 798px
               tall once a project has several providers plus the full
               capability profile, which stretched this card to the same 798px
               while its own content occupied 290px — a 508px empty rectangle,
               64% of the primary dashboard card, and the exact symptom the
               `dd28222` commit claimed to have fixed. That commit removed
               `justify-between` (so the sections stopped being spread apart)
               but the stretch came from the grid cell, not from `justify-between`. */
            className="lg:col-span-7 self-start p-4 rounded-lg bg-surface-card border border-border-default flex flex-col space-y-4"
          >
            {/* Project Title & Metadata Header */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border-default/80">
              <div className="space-y-1">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-[10px] font-mono text-violet-400 uppercase font-bold bg-violet-950/60 px-1.5 py-0.5 rounded border border-violet-800/60">
                    PROJECT SPEC
                  </span>
                  <h2 className="text-sm font-bold text-content-primary font-sans">{dashboard.project_name}</h2>
                  {getStatusBadge(status)}
                </div>
                <div className="flex items-center gap-3 text-[11px] font-mono text-content-secondary">
                  <span className="flex items-center gap-1">
                    <Server className="w-3 h-3 text-content-muted" />
                    Created: <strong className="text-content-primary">{formatDate(selectedProject?.created_at ?? null)}</strong>
                  </span>
                  <span>•</span>
                  <span className="flex items-center gap-1">
                    <Fingerprint className="w-3 h-3 text-content-muted" />
                    ID: <strong className="text-content-secondary">{projectIdShort}</strong>
                  </span>
                </div>
              </div>

              {/* Quick Action Button to Execution Hub */}
              <button
                onClick={() => navigate('/execution')}
                className="flex items-center gap-1 text-xs text-violet-400 hover:text-violet-300 font-medium font-mono cursor-pointer self-start sm:self-center"
              >
                <span>Execution Hub</span>
                <ArrowUpRight className="w-3.5 h-3.5" />
              </button>
            </div>

            {/* Data-as-of Record */}
            <div className="p-2.5 rounded bg-surface-page border border-border-default/90 text-xs flex items-center justify-between">
              <div className="flex items-center gap-2 truncate">
                <Radio className="w-3.5 h-3.5 text-yellow-400 shrink-0" />
                <span className="font-mono text-content-secondary text-[11px]">Data as of:</span>
                <span className="font-medium text-content-primary truncate">{formatTimestamp(dashboard.generated_at)}</span>
              </div>
              <span className="text-[10px] font-mono text-content-muted shrink-0 ml-2">REAL BACKEND SNAPSHOT</span>
            </div>

            {/* Current Execution Run Status */}
            <div className="space-y-2">
              <div className="flex items-center justify-between text-[11px] font-mono">
                <span className="font-bold text-content-secondary uppercase tracking-wider">Current Execution Run</span>
                {activeRun ? (
                  <span className="text-yellow-400 font-semibold">{activeRun.id.slice(0, 8)}</span>
                ) : (
                  <span className="text-content-muted">IDLE</span>
                )}
              </div>

              {activeRun ? (
                <div className="p-3 rounded-lg bg-gradient-to-r from-yellow-950/20 via-[#020617] to-violet-950/20 border border-yellow-800/60 space-y-2.5">
                  <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-1.5">
                    <div className="font-bold text-xs text-content-primary font-sans">{activeRun.title}</div>
                    <div className="flex items-center gap-2 text-[10px] font-mono text-yellow-400 shrink-0">
                      <span className="w-1.5 h-1.5 rounded-full bg-yellow-400 animate-ping" />
                      <span>{activeRun.status}</span>
                    </div>
                  </div>

                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-[10px] font-mono text-content-secondary">
                    <div className="p-1.5 rounded bg-surface-card border border-border-default">
                      <span className="text-content-muted">Started:</span> {activeRun.startedAt}
                    </div>
                    <div className="p-1.5 rounded bg-surface-card border border-border-default">
                      <span className="text-content-muted">Elapsed:</span> {activeRun.elapsed}
                    </div>
                    <div className="p-1.5 rounded bg-surface-card border border-border-default">
                      <span className="text-content-muted">Executed:</span> {activeRun.testsExecuted} / {activeRun.totalTests}
                    </div>
                    <div className="p-1.5 rounded bg-surface-card border border-border-default">
                      <span className="text-content-muted">Progress:</span> {activeRun.progressPct}%
                    </div>
                  </div>

                  {/* Animated Progress Bar */}
                  <div className="space-y-1">
                    <div className="flex justify-between text-[10px] font-mono text-content-secondary">
                      <span>Execution Throughput</span>
                      <span className="text-yellow-300 font-bold">{activeRun.progressPct}%</span>
                    </div>
                    <div className="h-1.5 w-full bg-slate-800 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-gradient-to-r from-yellow-400 to-violet-500 rounded-full transition-all duration-500"
                        style={{ width: `${activeRun.progressPct}%` }}
                      />
                    </div>
                  </div>
                </div>
              ) : (
                <div className="p-4 rounded-lg bg-surface-page border border-border-default/80 text-center space-y-2">
                  <div className="text-xs font-semibold text-content-secondary font-mono">No Active Execution Running</div>
                  <p className="text-[10px] text-content-muted max-w-sm mx-auto">
                    All test suites are currently in standby. Launch an orchestration run to stream live diagnostics.
                  </p>
                  <button
                    onClick={() => navigate('/execution')}
                    className="px-3 py-1 rounded bg-violet-950 hover:bg-violet-900 border border-violet-700/60 text-violet-300 text-xs font-mono font-medium cursor-pointer transition-colors"
                  >
                    Start Execution Run
                  </button>
                </div>
              )}
            </div>
          </div>

          {/* Right: the real AI engine (5 Cols on desktop).

              Was `AIQualityCoreOrb`, a decorative orb whose "Autonomous
              Verification Engine" / "ACTIVE INFERENCE" / "CORE STABLE"
              labels were computed from `ProjectStatus` alone — no model, no
              inference, no engine behind any of it. This shows the real
              routing layer (`ai_router.rs`) instead: real provider health,
              the real model that would be routed to, its real score and
              reason, and its real capability levels. */}
          <div id="ai-engine-column" className="lg:col-span-5 flex flex-col">
            <AIEnginePanel status={aiStatus} loading={aiLoading} error={aiError} onRetry={loadAiStatus} />
          </div>
        </section>

        {/* Execution efficiency / Average run duration (KPI, real, shown
            once each — real backend concepts not present as KPI cards
            above). FE-26 — `execution_avg_run_duration_ms` (BE-12b) reuses
            this exact same row/formatter rather than a new card: it is the
            same "nullable real millisecond KPI" shape as
            `execution_efficiency_ms` directly above it, not a count like
            the 6 grid cards, so this is its natural, already-established
            home. */}
        <div className="px-1 -mt-1 text-[10px] font-mono text-content-muted flex items-center gap-3">
          <span>Execution efficiency: <strong className="text-content-secondary">{formatMs(kpis.execution_efficiency_ms)}</strong> avg task duration</span>
          <span>•</span>
          <span>Average run duration: <strong className="text-content-secondary">{formatMs(kpis.execution_avg_run_duration_ms)}</strong></span>
        </div>

        {/* 4. Real-Time Project Status Section */}
        <section id="dashboard-project-status-section" className="space-y-2">
          <div className="flex items-center justify-between px-1">
            <div>
              <h2 className="text-xs font-bold text-content-primary font-mono uppercase tracking-wider flex items-center gap-1.5">
                <Activity className="w-3.5 h-3.5 text-yellow-400" />
                <span>Project Status</span>
              </h2>
              <p className="text-[10px] text-content-secondary font-sans">Real execution and quality state</p>
            </div>
            <span className="text-[9px] font-mono text-emerald-400 bg-emerald-950/60 px-2 py-0.5 rounded border border-emerald-800/60 uppercase">
              {realtime.connectionState === 'connected' ? 'LIVE TELEMETRY' : 'REAL DATA'}
            </span>
          </div>

          {/* 2-Column Grid: Donut Chart + Execution Activity Graph */}
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-stretch">
            {/* Donut Chart (5 Cols) */}
            <div className="lg:col-span-5">
              <ProjectStatusDonut metrics={metrics} projectName={dashboard.project_name} />
            </div>

            {/* Execution Activity Graph (7 Cols) — no real time-bucketed
                history endpoint exists (see module doc comment); always
                renders this component's own real, honest empty state. */}
            <div className="lg:col-span-7">
              <ProjectExecutionGraph timelineData={[]} projectName={dashboard.project_name} />
            </div>
          </div>
        </section>
      </>
    );
  };

  return (
    <div id="aiqa-dashboard-main-content" className="space-y-4 animate-in fade-in duration-200 text-content-primary">
      {/* 1. Header with Project Selector */}
      <header
        id="dashboard-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-surface-card border border-border-default/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <Activity className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-content-primary font-sans tracking-tight">Dashboard</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              CONTROL PLANE
            </span>
          </div>
          <p className="text-xs text-content-secondary font-sans">Real quality intelligence for your project</p>
        </div>

        {/* Enterprise Project Selector — real projects, real selection (FE-5) */}
        <div className="flex items-center gap-3">
          <div className="flex flex-col items-start md:items-end">
            <span className="text-[9px] uppercase font-mono tracking-wider text-content-secondary">Active Project Context</span>
            <div className="relative mt-0.5">
              <select
                id="project-selector-dropdown"
                aria-label="Active Project Context"
                value={selectedProjectId ?? ''}
                onChange={(e) => selectProject(e.target.value)}
                disabled={projects.length === 0}
                className="appearance-none pl-3 pr-8 py-1.5 rounded-md bg-surface-page hover:bg-slate-900 border border-violet-700/60 hover:border-violet-500 text-xs font-semibold text-content-primary focus:outline-none focus:ring-1 focus:ring-violet-500 cursor-pointer font-sans shadow-sm disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {!selectedProjectId && <option value="">Select a project…</option>}
                {projects.map((p) => (
                  <option key={p.id} value={p.id} className="bg-surface-page text-content-primary py-1">
                    {p.name}
                  </option>
                ))}
              </select>
              <ChevronDown className="w-3.5 h-3.5 text-violet-400 absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
            </div>
          </div>

          {/* New Run Action — navigates to the real Execution Hub (FE-11) */}
          <button
            id="btn-launch-run"
            onClick={() => navigate('/execution')}
            className="flex items-center gap-1.5 px-3 py-2 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-semibold text-xs shadow-sm shadow-violet-950/60 cursor-pointer transition-colors shrink-0"
          >
            <Play className="w-3 h-3 fill-current" />
            <span className="hidden sm:inline">Launch Run</span>
          </button>
        </div>
      </header>

      {/* 2. Full-width dashboard content, then the real, live orchestrator
          log stream as its own section at the bottom — moved out of the
          former right-side sticky column so the whole page reads top to
          bottom, log activity last. */}
      <div className="space-y-4 min-w-0">{renderBody()}</div>

      {/* Real backend activity — history over REST, then a live WebSocket
          (`OrchestratorLogsPanel`, reused as-is: it already owns the real
          stream, reconnect backoff, id-based dedup, bounded buffer, and
          near-bottom-only auto-scroll). Scoped to the selected project
          when there is one; global orchestrator logs otherwise — the
          backend includes unscoped entries in a project stream on
          purpose (`log_bus.rs::recent_logs`). Fixed height (rather than the
          old sticky-sidebar's full-page height) since it now sits at the
          bottom of the page instead of running alongside everything else. */}
      <div id="dashboard-log-stream-section" className="min-w-0">
        <OrchestratorLogsPanel projectId={selectedProjectId} />
      </div>
    </div>
  );
};
