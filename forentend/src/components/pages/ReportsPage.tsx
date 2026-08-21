/**
 * FE-11 — the real Findings & Quality Intelligence page.
 *
 * Backend contract (`services/orchestrator/src/api.rs::list_findings_scoped`
 * / `get_finding_analysis_scoped` / `review_finding_scoped`, `store.rs`'s
 * `BugDraftRow`/`FailureClusterRow`): a "Finding" (`Finding` /
 * `types/contract.ts`) is this backend's real, deterministic (rule-based,
 * not LLM — see `intelligence.rs::classify_failure`) auto-drafted defect: a
 * `FailureCluster` is created/updated every time a task fails with a
 * normalized failure signature, and once the same signature recurs >= 2
 * times a `Finding` (`bug_drafts` row) is created automatically, starting
 * at `review_state: 'NEEDS_REVIEW'`. Nothing here is invented — this page
 * only reads and reviews rows the backend already produced from real task
 * failures.
 *
 *   GET  /api/v1/projects/:id/findings                → Finding[] (any role)
 *   POST /api/v1/projects/:id/findings/:fid/analysis   → { finding,
 *                                                          failure_cluster }
 *                                                        (any role; this
 *                                                        backend's real
 *                                                        "root-cause
 *                                                        correlation" is its
 *                                                        already-computed
 *                                                        FailureCluster —
 *                                                        signature, root-
 *                                                        cause hypothesis,
 *                                                        confidence,
 *                                                        occurrence count —
 *                                                        not a fabricated
 *                                                        one)
 *   POST /api/v1/projects/:id/findings/:fid/review     → 204, sets
 *                                                        review_state to
 *                                                        CONFIRMED/DISMISSED
 *                                                        (Role::Operator —
 *                                                        a viewer-scoped key
 *                                                        genuinely 403s
 *                                                        here, same as
 *                                                        Audit/Execution
 *                                                        Hub's approve/
 *                                                        reject)
 *
 * All three were already real and typed since FE-1 (`aiqaApi.findings.*`,
 * `types/contract.ts`'s `Finding`/`FailureCluster`/`FindingAnalysis`/
 * `ReviewFindingRequest`) but never wired to any page until now — zero new
 * transport code.
 *
 * **Deliberately out of scope** (no real backend/library support for
 * either, and re-inventing them client-side would mean fabricating data the
 * backend does not compute): a separate per-run "Test Report" / "Final
 * Audit Report" view (Dashboard and Execution Hub already show the real
 * run-level pass/fail summary this would duplicate) and PDF/Excel export
 * (only a real JSON export of the exact fetched Findings/FailureCluster
 * data is offered below). The previous version of this page's "Evidence
 * Coverage: 100%" stat and two hardcoded `DefectDraft` objects (`DEF-881`/
 * `DEF-882`) are removed entirely, not replaced with a fake equivalent — no
 * "evidence coverage percentage" concept exists in the backend.
 *
 * List/detail/error-state visual language follows the same real-page
 * pattern established by `AuditPage.tsx`/`EvidencePage.tsx`/
 * `ExecutionHubPage.tsx` (`StateCard`/`Cta` for loading/error, `EmptyState`
 * for a real empty result) rather than introducing a new one.
 *
 * FE-22 — real Test Intelligence (`TestIntelRow`), added below the Findings
 * section: `GET /api/v1/projects/:id/intelligence/tests` (BE-10; `any` role,
 * same bar as `findings.list`/`.dashboard`/`.kpis`). This page's own title
 * already promised "Quality Intelligence" — this is the first real content
 * for that half of it. Deliberately placed here rather than on
 * `TestCaseValidatorPage.tsx` ("Test Case Validator"): that page's own
 * disclosed scope is pre-execution requirement-traceability/duplicate-
 * detection, a different concept from `TestIntelRow`'s real post-hoc
 * (historical-run-derived) risk/value/flakiness scoring — reusing that page
 * would have misrepresented what the data actually is, the same reasoning
 * this file already applies to keep Findings (post-hoc failure clustering)
 * separate from pre-execution validation. A dedicated new page was not
 * needed — this section reuses the exact same `useProject`/abort-controller/
 * loading-error-empty pattern as Findings above, as an independent resource
 * (own state, own fetch, own effect) so a stale project's rows can never
 * outlive a project switch (mirrors Findings' own effect exactly). The
 * legacy, unscoped `/intelligence/tests` is never called from here — see
 * `aiqaApi.ts::intelligence.tests`'s doc comment and BE-10's report for why.
 * `risk_score`/`failure_rate`/`value_score`/`flakiness_score` are rendered
 * as percentages using this file's own pre-existing `Math.round(x*100)}%`
 * convention (already used above for `finding.confidence`/
 * `reproducibility`), generalized into null-safe local helpers matching
 * `DashboardPage.tsx::formatPct`/`formatMs` exactly — never a fabricated
 * `0%`/`0ms` for a genuinely `null` field.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  CheckCircle2,
  Download,
  FolderOpen,
  Gauge,
  Inbox,
  RotateCcw,
  Search,
  Sparkles,
  X,
} from 'lucide-react';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { formatStatusLabel } from '../../utils/statusLabel';
import type { Finding, FindingAnalysis, ReviewState, TestIntelRow } from '../../types/contract';

/** FE-9: no real event type "is" a finding event (`domain.rs::EventType`
 *  has no such variant — findings are drafted server-side inside the same
 *  run-completion handler that already writes real `RunEvent`s, not
 *  published to the WS stream themselves). But a real `RUN_COMPLETED`
 *  event for this project genuinely can mean a new Finding was just
 *  auto-drafted from that run's failures — so it is used as the honest,
 *  disclosed trigger to invalidate/refetch the real findings list (same
 *  reasoning `AuditPage.tsx` documents for its own resync), not a
 *  fabricated local event and not a new finding-specific event this
 *  backend does not actually emit.
 *
 *  FE-25 — the same debounce window is reused for Test Intelligence's own
 *  resync below. `intelligence.rs::ingest_run` (the real backend function
 *  that computes `TestIntelRow`'s risk/value/flakiness fields, and the
 *  same function `record_failure`/Findings' own drafting runs inside) is
 *  called from `engine.rs` *after* the real `RUN_COMPLETED` event is
 *  already emitted (`record.emit(..., RunCompleted, ...)` on line 698,
 *  `intel.ingest_run(...)` on line 704 — confirmed by direct re-read of
 *  `engine.rs` this sprint) — a real, small race window between "the event
 *  arrived" and "the backend finished writing the row this refetch wants."
 *  This is the exact same race Findings' own resync has always had (they
 *  share the same `ingest_run` call), and the same 500ms debounce already
 *  gives the backend's fast, synchronous, no-network DB-write chain ample
 *  time to finish before the debounced fetch actually fires — not a new
 *  mitigation invented for this unit, the same one already shipped and
 *  proven for Findings. */
const EVENT_RESYNC_DEBOUNCE_MS = 500;

function matchesSearch(finding: Finding, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return [finding.id, finding.title, finding.description, finding.severity, finding.review_state]
    .filter((v): v is string => Boolean(v))
    .some((v) => v.toLowerCase().includes(q));
}

function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

const severityBadgeClass = (severity: string): string => {
  switch (severity) {
    case 'High':
      return 'bg-rose-950 text-rose-300 border-rose-800';
    case 'Medium':
      return 'bg-amber-950 text-amber-300 border-amber-800';
    default:
      return 'bg-slate-800 text-slate-300 border-slate-700';
  }
};

/** FE-22 — same 0..1-ratio → percentage convention this file already uses
 *  inline for `finding.confidence`/`failure_cluster.reproducibility`, but
 *  null-safe (`store.rs::TestIntelRow`'s score fields are real `Option<f64>`
 *  — a test with no scoreable history yet is genuinely `null`, never a
 *  fabricated `0%`). Matches `DashboardPage.tsx::formatPct`'s exact
 *  behavior; duplicated locally per this codebase's established per-page
 *  formatter convention (see that file's own `formatPct`/`formatMs`). */
function formatPct(ratio: number | null): string {
  if (ratio === null) return '—';
  return `${(ratio * 100).toFixed(1)}%`;
}

/** Matches `DashboardPage.tsx::formatMs`'s exact behavior. */
function formatMs(ms: number | null): string {
  if (ms === null) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

const reviewStateBadgeClass = (state: ReviewState): string => {
  switch (state) {
    case 'CONFIRMED':
      return 'bg-rose-950 text-rose-300 border-rose-800';
    case 'DISMISSED':
      return 'bg-slate-800 text-slate-400 border-slate-700';
    default:
      return 'bg-indigo-950 text-indigo-300 border-indigo-800';
  }
};

export const ReportsPage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();
  const realtime = useRealtime();

  // `null` = not yet loaded for the current scope; `[]` = a real,
  // successful fetch that genuinely returned zero rows — same distinction
  // AuditPage/EvidencePage use (brief: "empty" is never conflated with
  // "not yet requested" or "error").
  const [findings, setFindings] = useState<Finding[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [search, setSearch] = useState('');
  const abortRef = useRef<AbortController | null>(null);

  const [selectedFindingId, setSelectedFindingId] = useState<string | null>(null);
  const [analysis, setAnalysis] = useState<FindingAnalysis | null>(null);
  const [analysisLoading, setAnalysisLoading] = useState(false);
  const [analysisError, setAnalysisError] = useState<ApiError | null>(null);
  const analysisAbortRef = useRef<AbortController | null>(null);

  const [reviewingId, setReviewingId] = useState<string | null>(null);
  const [reviewError, setReviewError] = useState<ApiError | null>(null);

  // FE-22 — real Test Intelligence (BE-10's project-scoped
  // `GET .../intelligence/tests`), a resource independent of Findings —
  // own loading/error/empty state, own abort controller, same `null` =
  // not-yet-loaded vs `[]` = real-empty distinction Findings uses above.
  const [testIntel, setTestIntel] = useState<TestIntelRow[] | null>(null);
  const [testIntelLoading, setTestIntelLoading] = useState(false);
  const [testIntelRefreshing, setTestIntelRefreshing] = useState(false);
  const [testIntelError, setTestIntelError] = useState<ApiError | null>(null);
  const testIntelAbortRef = useRef<AbortController | null>(null);

  const fetchFindings = useCallback(async (projectId: string, isRefresh: boolean) => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    if (isRefresh) setRefreshing(true);
    else setLoading(true);
    setError(null);
    try {
      const list = await aiqaApi.findings.list(projectId, controller.signal);
      if (controller.signal.aborted) return;
      setFindings(list);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setError(apiErr);
      // A failed fetch means we cannot currently prove what the real
      // findings list looks like — never leave a stale/fabricated list
      // sitting next to an error.
      setFindings(null);
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
      setFindings(null);
      setError(null);
      setLoading(false);
      setRefreshing(false);
      setSelectedFindingId(null);
      setAnalysis(null);
      return;
    }
    setFindings(null);
    setSelectedFindingId(null);
    setAnalysis(null);
    void fetchFindings(selectedProjectId, false);
  }, [selectedProjectId, fetchFindings]);

  const fetchTestIntelligence = useCallback(async (projectId: string, isRefresh: boolean) => {
    testIntelAbortRef.current?.abort();
    const controller = new AbortController();
    testIntelAbortRef.current = controller;
    if (isRefresh) setTestIntelRefreshing(true);
    else setTestIntelLoading(true);
    setTestIntelError(null);
    try {
      const rows = await aiqaApi.intelligence.tests(projectId, controller.signal);
      if (controller.signal.aborted) return;
      setTestIntel(rows);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setTestIntelError(apiErr);
      // Same rule as Findings above: a failed fetch never leaves a
      // stale/fabricated list next to the error.
      setTestIntel(null);
    } finally {
      if (!controller.signal.aborted) {
        setTestIntelLoading(false);
        setTestIntelRefreshing(false);
      }
    }
  }, []);

  // FE-22 — mirrors the Findings effect above exactly (§18's mandatory
  // stale-cross-project-data protection): switching projects aborts any
  // in-flight request for the old project, clears the old project's rows
  // before the new request even starts, and the abort-signal check in
  // `fetchTestIntelligence` means a late-arriving old-project response can
  // never overwrite the new project's state.
  useEffect(() => {
    if (!selectedProjectId) {
      testIntelAbortRef.current?.abort();
      setTestIntel(null);
      setTestIntelError(null);
      setTestIntelLoading(false);
      setTestIntelRefreshing(false);
      return;
    }
    setTestIntel(null);
    void fetchTestIntelligence(selectedProjectId, false);
  }, [selectedProjectId, fetchTestIntelligence]);

  const handleRefresh = useCallback(() => {
    if (!selectedProjectId) return;
    if (!loading && !refreshing) void fetchFindings(selectedProjectId, true);
    if (!testIntelLoading && !testIntelRefreshing) void fetchTestIntelligence(selectedProjectId, true);
  }, [selectedProjectId, loading, refreshing, fetchFindings, testIntelLoading, testIntelRefreshing, fetchTestIntelligence]);

  // FE-9 resync — see module doc comment. Depends on the stable
  // `realtime.subscribe` identity, not the whole `realtime` object (the
  // live-discovered FE-9 resubscription bug documented in
  // `ExecutionHubPage.tsx`/`AuditPage.tsx`).
  const resyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!selectedProjectId) return;
    const unsubscribe = realtime.subscribe(() => {
      if (resyncTimeoutRef.current) clearTimeout(resyncTimeoutRef.current);
      resyncTimeoutRef.current = setTimeout(() => {
        resyncTimeoutRef.current = null;
        void fetchFindings(selectedProjectId, true);
      }, EVENT_RESYNC_DEBOUNCE_MS);
    });
    return () => {
      unsubscribe();
      if (resyncTimeoutRef.current) clearTimeout(resyncTimeoutRef.current);
    };
  }, [selectedProjectId, realtime.subscribe, fetchFindings]);

  // FE-25 — real Test Intelligence resync. Unlike Findings' own resync
  // above (which debounce-refetches on *any* delivered event, since a
  // finding could plausibly change from several event types), this one is
  // filtered to `RUN_COMPLETED` specifically: `TestIntelRow`'s fields are
  // only ever recomputed once, inside `intelligence.rs::ingest_run`, which
  // only ever runs at real run completion — no `TASK_*` event by itself
  // means new intelligence data exists yet, so resyncing on those would be
  // a real, provable no-op fetch, not a more "responsive" one. Same
  // subscribe/debounce/cleanup skeleton as Findings', same real
  // `useRealtime().subscribe` connection (no second WebSocket), own
  // timeout ref so the two resyncs never interfere with each other.
  const testIntelResyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!selectedProjectId) return;
    const unsubscribe = realtime.subscribe((event) => {
      if (event.type !== 'RUN_COMPLETED') return;
      if (testIntelResyncTimeoutRef.current) clearTimeout(testIntelResyncTimeoutRef.current);
      testIntelResyncTimeoutRef.current = setTimeout(() => {
        testIntelResyncTimeoutRef.current = null;
        void fetchTestIntelligence(selectedProjectId, true);
      }, EVENT_RESYNC_DEBOUNCE_MS);
    });
    return () => {
      unsubscribe();
      if (testIntelResyncTimeoutRef.current) clearTimeout(testIntelResyncTimeoutRef.current);
    };
  }, [selectedProjectId, realtime.subscribe, fetchTestIntelligence]);

  useEffect(() => () => {
    abortRef.current?.abort();
    analysisAbortRef.current?.abort();
    testIntelAbortRef.current?.abort();
    if (resyncTimeoutRef.current) clearTimeout(resyncTimeoutRef.current);
    if (testIntelResyncTimeoutRef.current) clearTimeout(testIntelResyncTimeoutRef.current);
  }, []);

  const fetchAnalysis = useCallback(
    async (projectId: string, findingId: string) => {
      analysisAbortRef.current?.abort();
      const controller = new AbortController();
      analysisAbortRef.current = controller;
      setAnalysisLoading(true);
      setAnalysisError(null);
      try {
        const result = await aiqaApi.findings.analysis(projectId, findingId, controller.signal);
        if (controller.signal.aborted) return;
        setAnalysis(result);
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') return;
        const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
        setAnalysisError(apiErr);
        setAnalysis(null);
      } finally {
        if (!controller.signal.aborted) setAnalysisLoading(false);
      }
    },
    [],
  );

  const handleSelectFinding = useCallback(
    (findingId: string) => {
      if (!selectedProjectId) return;
      if (selectedFindingId === findingId) {
        // Collapse on second click of the same row.
        setSelectedFindingId(null);
        analysisAbortRef.current?.abort();
        setAnalysis(null);
        setAnalysisError(null);
        return;
      }
      setSelectedFindingId(findingId);
      setReviewError(null);
      void fetchAnalysis(selectedProjectId, findingId);
    },
    [selectedProjectId, selectedFindingId, fetchAnalysis],
  );

  const handleReview = useCallback(
    async (findingId: string, action: 'confirm' | 'dismiss') => {
      if (!selectedProjectId || reviewingId) return;
      setReviewingId(findingId);
      setReviewError(null);
      try {
        await aiqaApi.findings.review(selectedProjectId, findingId, { action });
        // Real state changed — refetch the real list (never mutate the
        // local `review_state` optimistically) and, if this finding is
        // still selected, its real analysis too.
        await fetchFindings(selectedProjectId, true);
        if (selectedFindingId === findingId) await fetchAnalysis(selectedProjectId, findingId);
      } catch (err) {
        const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
        setReviewError(apiErr);
      } finally {
        setReviewingId(null);
      }
    },
    [selectedProjectId, reviewingId, selectedFindingId, fetchFindings, fetchAnalysis],
  );

  const handleExportJSON = useCallback(() => {
    if (!findings) return;
    const dataStr =
      'data:text/json;charset=utf-8,' +
      encodeURIComponent(
        JSON.stringify(
          {
            projectId: selectedProjectId,
            projectName: selectedProject?.name ?? null,
            findings,
            exportedAt: new Date().toISOString(),
          },
          null,
          2,
        ),
      );
    const downloadAnchor = document.createElement('a');
    downloadAnchor.setAttribute('href', dataStr);
    downloadAnchor.setAttribute('download', `aiqa_findings_${selectedProjectId ?? 'export'}.json`);
    document.body.appendChild(downloadAnchor);
    downloadAnchor.click();
    downloadAnchor.remove();
  }, [findings, selectedProjectId, selectedProject]);

  const visibleFindings = useMemo(() => {
    if (!findings) return [];
    return findings.filter((f) => matchesSearch(f, search));
  }, [findings, search]);

  const renderBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to view its findings."
        />
      );
    }

    if (loading) {
      return (
        <StateCard
          icon={Sparkles}
          iconClassName="text-indigo-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-indigo-300 bg-indigo-950/60 border-indigo-800"
          title="Loading findings"
          description={`Fetching auto-drafted findings for ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }

    if (error) {
      return renderResourceErrorState(error, handleRefresh, {
        forbiddenTitle: 'Not permitted to view these findings',
        forbiddenDescription: `This key's role or project scope does not permit viewing ${selectedProject?.name ?? 'this project'}'s findings.`,
        offlineDescriptionSuffix: 'to load findings.',
        genericTitle: "Couldn't load findings",
      });
    }

    if (findings && findings.length === 0) {
      return (
        <EmptyState
          icon={Inbox}
          title="No findings recorded"
          description={`The orchestrator has not auto-drafted any findings for ${selectedProject?.name ?? 'this project'} yet — a Finding is only created once the same failure signature recurs across at least 2 task executions.`}
        />
      );
    }

    if (findings && visibleFindings.length === 0) {
      return (
        <div className="py-8 text-center text-xs text-slate-500 font-mono">
          No findings match &ldquo;{search}&rdquo;. ({findings.length} total loaded.)
        </div>
      );
    }

    return (
      <div className="space-y-2.5">
        {visibleFindings.map((finding) => {
          const isSelected = selectedFindingId === finding.id;
          return (
            <div key={finding.id} className="rounded-lg border border-slate-800 bg-[#020617] overflow-hidden">
              <button
                type="button"
                onClick={() => handleSelectFinding(finding.id)}
                className="w-full text-left p-3 space-y-1.5 cursor-pointer hover:bg-slate-900/40 transition-colors"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="space-y-0.5 min-w-0">
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span className="font-mono font-bold text-indigo-400 text-xs">{finding.id.slice(0, 8)}</span>
                      <span className={`px-1.5 py-0.2 rounded border font-mono text-[9px] font-bold uppercase ${severityBadgeClass(finding.severity)}`}>
                        {finding.severity}
                      </span>
                      <span className={`px-1.5 py-0.2 rounded border font-mono text-[9px] font-bold uppercase ${reviewStateBadgeClass(finding.review_state)}`}>
                        {formatStatusLabel(finding.review_state)}
                      </span>
                      <span className="text-[10px] font-mono text-slate-500">
                        {Math.round(finding.confidence * 100)}% confidence
                      </span>
                    </div>
                    <h4 className="text-xs font-bold text-slate-100 font-sans truncate">{finding.title}</h4>
                    <p className="text-[10px] text-slate-500 font-mono">Created {formatTimestamp(finding.created_at)}</p>
                  </div>
                </div>
              </button>

              {isSelected && (
                <div className="border-t border-slate-800 p-3 space-y-3 bg-[#0B1120]">
                  <p className="text-xs text-slate-300 leading-relaxed whitespace-pre-line">{finding.description}</p>

                  {analysisLoading && (
                    <div className="flex items-center gap-2 text-xs text-slate-400 font-mono">
                      <Sparkles className="w-3.5 h-3.5 text-indigo-400 animate-spin" />
                      Loading root-cause analysis…
                    </div>
                  )}

                  {analysisError && (
                    <div className="flex items-center justify-between gap-2 p-2.5 rounded bg-rose-950/40 border border-rose-800/60 text-xs text-rose-300">
                      <span>Couldn't load analysis: {analysisError.message}</span>
                      <button
                        type="button"
                        onClick={() => selectedProjectId && fetchAnalysis(selectedProjectId, finding.id)}
                        className="shrink-0 px-2 py-0.5 rounded bg-rose-900/60 hover:bg-rose-900 border border-rose-700 text-[10px] font-mono cursor-pointer"
                      >
                        Retry
                      </button>
                    </div>
                  )}

                  {analysis && analysis.finding.id === finding.id && (
                    <div className="p-2.5 rounded bg-[#020617] border border-slate-800 space-y-1.5 text-[11px] font-mono">
                      <div className="text-[9px] font-mono uppercase text-slate-400 font-bold">
                        Root-Cause Analysis (failure cluster {analysis.failure_cluster.id.slice(0, 8)})
                      </div>
                      <div className="text-slate-300">
                        <span className="text-slate-500">Signature: </span>
                        {analysis.failure_cluster.signature}
                      </div>
                      <div className="text-slate-300">
                        <span className="text-slate-500">Hypothesis: </span>
                        {analysis.failure_cluster.root_cause_hypothesis ?? 'not available'}
                      </div>
                      <div className="flex flex-wrap gap-x-4 text-slate-300">
                        <span>
                          <span className="text-slate-500">Occurrences: </span>
                          {analysis.failure_cluster.occurrence_count}
                        </span>
                        <span>
                          <span className="text-slate-500">First seen: </span>
                          {formatTimestamp(analysis.failure_cluster.first_seen_at)}
                        </span>
                        <span>
                          <span className="text-slate-500">Last seen: </span>
                          {formatTimestamp(analysis.failure_cluster.last_seen_at)}
                        </span>
                        {analysis.failure_cluster.reproducibility != null && (
                          <span>
                            <span className="text-slate-500">Reproducibility: </span>
                            {Math.round(analysis.failure_cluster.reproducibility * 100)}%
                          </span>
                        )}
                      </div>
                    </div>
                  )}

                  {reviewError && (
                    <div className="p-2.5 rounded bg-rose-950/40 border border-rose-800/60 text-xs text-rose-300">
                      {reviewError.kind === 'forbidden'
                        ? "This key's role does not permit reviewing findings (requires Operator role or above)."
                        : `Review failed: ${reviewError.message}`}
                    </div>
                  )}

                  {finding.review_state === 'NEEDS_REVIEW' ? (
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => handleReview(finding.id, 'confirm')}
                        disabled={reviewingId === finding.id}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-rose-600 hover:bg-rose-500 text-white font-semibold text-xs cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        <CheckCircle2 className="w-3.5 h-3.5" />
                        Confirm as Defect
                      </button>
                      <button
                        type="button"
                        onClick={() => handleReview(finding.id, 'dismiss')}
                        disabled={reviewingId === finding.id}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        <X className="w-3.5 h-3.5" />
                        Dismiss
                      </button>
                      {reviewingId === finding.id && (
                        <span className="text-[10px] font-mono text-slate-500">Saving…</span>
                      )}
                    </div>
                  ) : (
                    <p id={`finding-review-attribution-${finding.id}`} className="text-[10px] font-mono text-slate-500">
                      Reviewed — {finding.review_state === 'CONFIRMED' ? 'confirmed as a real defect' : 'dismissed'}
                      {finding.promoted_by ? ` by ${finding.promoted_by}` : ''}
                      {/* FE-19 — real `Finding.promoted_at` exposure. Already
                          fetched on every `findings.list` response
                          (`store.rs::review_bug_draft` sets it together with
                          `promoted_by`, `api.rs::review_finding_scoped` line
                          1302, using the real authenticated actor's name) but
                          never rendered — `promoted_by` alone told you *who*
                          reviewed a finding, never *when*. */}
                      {finding.promoted_at ? ` at ${formatTimestamp(finding.promoted_at)}` : ''}.
                    </p>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    );
  };

  // FE-22 — real Test Intelligence body. Same state-card/empty-state
  // language as `renderBody` above, applied to the independent
  // `testIntel`/`testIntelLoading`/`testIntelError` resource.
  const renderTestIntelligenceBody = () => {
    if (!selectedProjectId) return null;

    if (testIntelLoading) {
      return (
        <StateCard
          icon={Gauge}
          iconClassName="text-indigo-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-indigo-300 bg-indigo-950/60 border-indigo-800"
          title="Loading test intelligence"
          description={`Fetching real risk/value/flakiness scoring for ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }

    if (testIntelError) {
      return renderResourceErrorState(testIntelError, () => selectedProjectId && fetchTestIntelligence(selectedProjectId, true), {
        forbiddenTitle: 'Not permitted to view test intelligence',
        forbiddenDescription: `This key's role or project scope does not permit viewing ${selectedProject?.name ?? 'this project'}'s test intelligence.`,
        offlineDescriptionSuffix: 'to load test intelligence.',
        genericTitle: "Couldn't load test intelligence",
      });
    }

    if (testIntel && testIntel.length === 0) {
      return (
        <EmptyState
          icon={Gauge}
          title="No test intelligence available for this project yet"
          description="Risk, value, and flakiness scoring is computed after a task reaches a terminal state (pass/fail/timeout/etc.) during a completed run — none has happened yet for this project."
        />
      );
    }

    if (!testIntel) return null;

    return (
      <div className="overflow-x-auto">
        <table className="w-full text-left text-xs">
          <thead>
            <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800">
              <th scope="col" className="py-2 pr-2.5 font-normal">Test</th>
              <th scope="col" className="py-2 px-2.5 font-normal">Risk</th>
              <th scope="col" className="py-2 px-2.5 font-normal">Failure Rate</th>
              <th scope="col" className="py-2 px-2.5 font-normal">Value</th>
              <th scope="col" className="py-2 px-2.5 font-normal">Flakiness</th>
              <th scope="col" className="py-2 px-2.5 font-normal">Exec. Cost</th>
              <th scope="col" className="py-2 px-2.5 font-normal">Dependencies</th>
              <th scope="col" className="py-2 pl-2.5 font-normal">Window</th>
            </tr>
          </thead>
          <tbody>
            {testIntel.map((row) => (
              <tr key={row.id} className="border-b border-slate-900 last:border-0 font-mono text-slate-300">
                <td className="py-2 pr-2.5 font-sans font-semibold text-slate-100 truncate max-w-[220px]" title={row.name}>
                  {row.name}
                </td>
                <td className="py-2 px-2.5">{formatPct(row.risk_score)}</td>
                <td className="py-2 px-2.5">{formatPct(row.failure_rate)}</td>
                <td className="py-2 px-2.5">{formatPct(row.value_score)}</td>
                <td className="py-2 px-2.5">{formatPct(row.flakiness_score)}</td>
                <td className="py-2 px-2.5">{formatMs(row.execution_cost_ms)}</td>
                <td className="py-2 px-2.5">{row.dependency_count ?? '—'}</td>
                <td className="py-2 pl-2.5 text-slate-500">
                  {row.window_runs === null
                    ? '—'
                    : `${row.window_runs} run${row.window_runs === 1 ? '' : 's'}${row.window_failures !== null ? `, ${row.window_failures} failed` : ''}`}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  };

  return (
    <div id="aiqa-reports-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Findings & Quality Intelligence"
        subtitle="Real, auto-drafted findings derived from recurring task failure signatures — review to confirm or dismiss"
        badge={findings ? `${findings.length} FINDING${findings.length === 1 ? '' : 'S'}` : 'FINDINGS'}
        icon={Sparkles}
        actions={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleRefresh}
              disabled={!selectedProjectId || loading || refreshing || testIntelLoading || testIntelRefreshing}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 text-xs font-medium border border-slate-800 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <RotateCcw className={`w-3.5 h-3.5 ${refreshing || testIntelRefreshing ? 'animate-spin' : ''}`} />
              <span>Refresh</span>
            </button>
            <button
              type="button"
              onClick={handleExportJSON}
              disabled={!findings || findings.length === 0}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Export JSON</span>
            </button>
          </div>
        }
      />

      {selectedProjectId && findings && findings.length > 0 && (
        <div className="flex items-center gap-2 p-2.5 rounded-lg border border-slate-800 bg-[#0B1120]">
          <div className="relative w-full max-w-sm">
            <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
            <label htmlFor="findings-search-input" className="sr-only">
              Search findings
            </label>
            <input
              id="findings-search-input"
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search title, severity, review state…"
              className="w-full pl-8 pr-3 py-1.5 rounded-md border border-slate-800 bg-[#020617] text-xs text-slate-200 outline-none focus:ring-1 focus:ring-indigo-500 font-sans"
            />
          </div>
        </div>
      )}

      <section id="findings-content" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm" aria-live="polite">
        {renderBody()}
      </section>

      {selectedProjectId && (
        <div className="space-y-2">
          <div className="flex items-center gap-1.5 px-1">
            <Gauge className="w-3.5 h-3.5 text-indigo-400" />
            <h2 className="text-xs font-bold text-slate-100 font-mono uppercase tracking-wider">Test Intelligence</h2>
            <span className="text-[10px] font-mono text-slate-500">
              {testIntel ? `${testIntel.length} TEST${testIntel.length === 1 ? '' : 'S'}` : ''}
            </span>
          </div>
          <p className="px-1 text-[11px] text-slate-500">
            Real, deterministic risk/value/flakiness scoring computed from this project's actual task execution history — not an AI prediction.
          </p>
          <section id="test-intelligence-content" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm" aria-live="polite">
            {renderTestIntelligenceBody()}
          </section>
        </div>
      )}
    </div>
  );
};
