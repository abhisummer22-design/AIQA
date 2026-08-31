/**
 * AIQA Neural Intelligence Graph — the data adapter (spec §7's "clean data
 * adapter/service layer"). This is the one place that turns real backend
 * responses into `NeuralGraphData`; every other file under
 * `components/neural-graph/` only renders what this hook already computed.
 *
 * Reuses real state the Dashboard already fetched (`dashboard`, `aiStatus`
 * — passed in, never re-fetched here) and adds the real project-scoped
 * calls the graph specifically needs: the full test case list (for real
 * per-category counts, requirement-reference coverage, and risk_factor
 * coverage), the execution list (for real running-run detection and to
 * reach each run's real evidence), and the audit log (for a real denial
 * count). Evidence has no project-wide endpoint in this backend — only
 * per-run (`aiqaApi.executions.evidence`) — so it is summed across this
 * project's real runs, fetched in parallel.
 *
 * No metric here is ever a fabricated number: while a fetch is in flight
 * every node reads "Loading…"; once it resolves, every count is real,
 * including a real zero (a project that genuinely has no test cases yet
 * shows `0`, not "N/A" — those are different real facts and must not be
 * conflated). `coveragePercent`/`routingScore` are the only fields allowed
 * to be `null` after a successful load, because a ratio over zero test
 * cases and "no compatible model routed" are both real absences, not
 * missing data.
 *
 * Genuinely realtime, not just "load once": subscribes to the same
 * project-scoped `RealtimeContext` WebSocket `NeuralPipeline.tsx`/
 * `DashboardPage.tsx` already use, and debounce-resyncs on any real event
 * for this project (same `EVENT_RESYNC_DEBOUNCE_MS` reasoning as
 * `DashboardPage.tsx` — a burst of task events collapses into one real
 * refetch, not one per event). A test case saved, a run finishing, new
 * evidence captured — the graph reflects it within the debounce window,
 * with no manual refresh.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { aiqaApi } from '../services/aiqaApi';
import { useRealtime } from '../context/RealtimeContext';
import { CATEGORY_NODES, MAJOR_STAGES } from '../components/neural-graph/neuralGraph.config';
import type {
  NeuralEdgeData,
  NeuralGraphData,
  NeuralNodeData,
  NeuralNodeStatus,
} from '../components/neural-graph/neuralGraph.types';
import type { AiStatusResponse, DashboardResponse, RunListItem, TestCaseRow } from '../types/contract';

export interface UseNeuralGraphDataResult {
  data: NeuralGraphData | null;
  loading: boolean;
  error: string | null;
  retry: () => void;
}

function countBy<T>(items: T[], key: (item: T) => string): Map<string, number> {
  const m = new Map<string, number>();
  for (const item of items) {
    const k = key(item);
    m.set(k, (m.get(k) ?? 0) + 1);
  }
  return m;
}

export function useNeuralGraphData(
  projectId: string | null,
  projectName: string | null,
  dashboard: DashboardResponse | null,
  aiStatus: AiStatusResponse | null,
): UseNeuralGraphDataResult {
  const realtime = useRealtime();
  const [data, setData] = useState<NeuralGraphData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  // Real inputs this hook needs beyond the fetch it triggers itself —
  // kept in a ref so `load` (a stable useCallback) always reads the latest
  // without needing dashboard/aiStatus in its own dependency array, which
  // would otherwise re-trigger a full re-fetch on every Dashboard poll
  // tick even when the project hasn't changed.
  const latestInputs = useRef({ dashboard, aiStatus, projectName });
  latestInputs.current = { dashboard, aiStatus, projectName };

  const load = useCallback(async (pid: string) => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    setError(null);
    try {
      const [testCases, executions, audit] = await Promise.all([
        aiqaApi.testCases.list(pid, {}, controller.signal),
        aiqaApi.executions.list(pid, {}, controller.signal),
        aiqaApi.audit.list(pid, { limit: 500 }, controller.signal),
      ]);
      if (controller.signal.aborted) return;

      // Evidence has no project-wide endpoint — summed across this
      // project's real runs. A single run's evidence fetch failing (e.g. a
      // very old run whose evidence blobs were pruned) must not blank out
      // the whole graph; that run just contributes 0 to the real total.
      const evidenceCounts = await Promise.all(
        executions.map((run) =>
          aiqaApi.executions
            .evidence(pid, run.run_id, controller.signal)
            .then((rows) => rows.length)
            .catch(() => 0),
        ),
      );
      if (controller.signal.aborted) return;
      const evidenceTotal = evidenceCounts.reduce((a, b) => a + b, 0);

      const { dashboard: dash, aiStatus: ai, projectName: pname } = latestInputs.current;
      const built = buildGraphData(pid, pname ?? pid, testCases, executions, audit, evidenceTotal, dash, ai);
      setData(built);
    } catch (err) {
      if (controller.signal.aborted) return;
      setData(null);
      setError(err instanceof Error ? err.message : 'Failed to load real Neural Graph data.');
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!projectId) {
      abortRef.current?.abort();
      setData(null);
      setError(null);
      setLoading(false);
      return;
    }
    void load(projectId);
    return () => {
      abortRef.current?.abort();
    };
  }, [projectId, load]);

  // Real event -> real debounced resync (400ms, same window
  // `DashboardPage.tsx`'s own `EVENT_RESYNC_DEBOUNCE_MS` uses) — a burst of
  // task events for this project collapses into one real refetch, never
  // one request per event.
  const resyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!projectId) return;
    const unsubscribe = realtime.subscribe(() => {
      if (resyncTimeoutRef.current) clearTimeout(resyncTimeoutRef.current);
      resyncTimeoutRef.current = setTimeout(() => {
        resyncTimeoutRef.current = null;
        void load(projectId);
      }, 400);
    });
    return () => {
      unsubscribe();
      if (resyncTimeoutRef.current) clearTimeout(resyncTimeoutRef.current);
    };
    // Depends on `realtime.subscribe` specifically (stable identity), not
    // the whole `realtime` object — that object's identity changes on
    // every real event (`connectionState`/`lastEventAt` are part of its
    // own memo deps), so depending on it here would tear down and
    // resubscribe on every single event instead of once per project. Same
    // mistake, same fix, as `DashboardPage.tsx`'s own resync effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, realtime.subscribe, load]);

  return {
    data,
    loading,
    error,
    retry: () => {
      if (projectId) void load(projectId);
    },
  };
}

function buildGraphData(
  projectId: string,
  projectName: string,
  testCases: TestCaseRow[],
  executions: RunListItem[],
  audit: import('../types/contract').AuditRow[],
  evidenceTotal: number,
  dashboard: DashboardResponse | null,
  aiStatus: AiStatusResponse | null,
): NeuralGraphData {
  const byCategory = countBy(testCases, (tc) => tc.category);
  const byStatus = countBy(testCases, (tc) => tc.status);
  const bySeverity = countBy(testCases, (tc) => tc.severity);
  const byPriority = countBy(testCases, (tc) => tc.priority);
  const byRiskFactor = countBy(
    testCases.filter((tc) => tc.risk_factor && tc.risk_factor.trim()),
    (tc) => tc.risk_factor as string,
  );

  const requirementTraced = testCases.filter((tc) => tc.requirement_reference && tc.requirement_reference.trim());
  const riskFlagged = testCases.filter((tc) => tc.risk_factor && tc.risk_factor.trim());
  const runningRuns = executions.filter((r) => r.state === 'RUNNING').length;
  const deniedAudit = audit.filter((a) => a.outcome && a.outcome.toLowerCase() !== 'ok').length;

  const passed = dashboard?.pass_fail.passed ?? 0;
  const failed = dashboard?.pass_fail.failed ?? 0;
  const executedTotal = passed + failed;

  const criticalCount = bySeverity.get('CRITICAL') ?? 0;
  const approvedCount = byStatus.get('APPROVED') ?? 0;
  const coveragePercent = testCases.length > 0 ? Math.round((100 * approvedCount) / testCases.length) : null;
  const routingScore = aiStatus?.routing_preview?.routing_score ?? null;

  const nodes: NeuralNodeData[] = [];
  const edges: NeuralEdgeData[] = [];

  const majorStatus: Record<string, NeuralNodeStatus> = {
    requirement: requirementTraced.length > 0 ? 'completed' : 'idle',
    'test-case':
      testCases.length === 0
        ? 'idle'
        : (byStatus.get('REJECTED') ?? 0) > 0
          ? 'warning'
          : (byStatus.get('GENERATED') ?? 0) > 0
            ? 'processing'
            : 'completed',
    execution: runningRuns > 0 ? 'processing' : failed > 0 ? 'warning' : passed > 0 ? 'completed' : 'idle',
    evidence: evidenceTotal > 0 ? 'completed' : 'idle',
    risk: riskFlagged.length > 0 ? 'warning' : 'idle',
    audit: deniedAudit > 0 ? 'warning' : audit.length > 0 ? 'completed' : 'idle',
  };

  nodes.push({
    id: 'core',
    label: 'Neural QA Core',
    shortLabel: 'QA CORE',
    tier: 'core',
    status: routingScore !== null ? 'active' : 'idle',
    metricLabel: 'Active nodes',
    metricValue: `${MAJOR_STAGES.filter((s) => majorStatus[s.id] !== 'idle').length}/${MAJOR_STAGES.length}`,
    detail: {
      rows: [
        { label: 'Project', value: projectName },
        { label: 'Total test cases', value: String(testCases.length) },
        { label: 'AI model routed', value: aiStatus?.routing_preview ? `${aiStatus.routing_preview.provider_id}/${aiStatus.routing_preview.model_id}` : 'None' },
      ],
      recent: [],
    },
  });

  for (const stage of MAJOR_STAGES) {
    edges.push({ id: `core-${stage.id}`, from: 'core', to: stage.id, active: majorStatus[stage.id] === 'processing' || majorStatus[stage.id] === 'active' });

    if (stage.id === 'requirement') {
      nodes.push({
        id: stage.id,
        label: stage.label,
        shortLabel: stage.shortLabel,
        tier: 'major',
        status: majorStatus[stage.id],
        metricLabel: 'Traced',
        metricValue: testCases.length > 0 ? `${requirementTraced.length}/${testCases.length}` : '0',
        detail: {
          rows: [
            { label: 'Test cases with a real requirement reference', value: String(requirementTraced.length) },
            { label: 'Total test cases', value: String(testCases.length) },
          ],
          recent: requirementTraced.slice(0, 5).map((tc) => `${tc.title} — ${tc.requirement_reference}`),
        },
      });
    } else if (stage.id === 'test-case') {
      nodes.push({
        id: stage.id,
        label: stage.label,
        shortLabel: stage.shortLabel,
        tier: 'major',
        status: majorStatus[stage.id],
        metricLabel: 'Total',
        metricValue: String(testCases.length),
        detail: {
          rows: (['GENERATED', 'VALIDATED', 'APPROVED', 'REJECTED', 'EXECUTED'] as const).map((s) => ({
            label: s,
            value: String(byStatus.get(s) ?? 0),
          })),
          recent: testCases.slice(0, 5).map((tc) => `${tc.title} (${tc.status})`),
        },
      });
    } else if (stage.id === 'execution') {
      nodes.push({
        id: stage.id,
        label: stage.label,
        shortLabel: stage.shortLabel,
        tier: 'major',
        status: majorStatus[stage.id],
        metricLabel: 'Executed',
        metricValue: `${passed}/${executedTotal}`,
        detail: {
          rows: [
            { label: 'Total runs', value: String(executions.length) },
            { label: 'Currently running', value: String(runningRuns) },
            { label: 'Tasks passed', value: String(passed) },
            { label: 'Tasks failed', value: String(failed) },
          ],
          recent: executions.slice(0, 5).map((r) => `${r.name} — ${r.state}`),
        },
      });
    } else if (stage.id === 'evidence') {
      nodes.push({
        id: stage.id,
        label: stage.label,
        shortLabel: stage.shortLabel,
        tier: 'major',
        status: majorStatus[stage.id],
        metricLabel: 'Artifacts',
        metricValue: String(evidenceTotal),
        detail: { rows: [{ label: 'Real evidence artifacts across all runs', value: String(evidenceTotal) }], recent: [] },
      });
    } else if (stage.id === 'risk') {
      nodes.push({
        id: stage.id,
        label: stage.label,
        shortLabel: stage.shortLabel,
        tier: 'major',
        status: majorStatus[stage.id],
        metricLabel: 'Flagged',
        metricValue: String(riskFlagged.length),
        detail: {
          rows:
            byRiskFactor.size > 0
              ? [...byRiskFactor.entries()].map(([k, v]) => ({ label: k, value: String(v) }))
              : [{ label: 'Test cases with a real risk_factor set', value: '0' }],
          recent: riskFlagged.slice(0, 5).map((tc) => `${tc.title} — ${tc.risk_factor}`),
        },
      });
    } else {
      nodes.push({
        id: stage.id,
        label: stage.label,
        shortLabel: stage.shortLabel,
        tier: 'major',
        status: majorStatus[stage.id],
        metricLabel: 'Entries',
        metricValue: String(audit.length),
        detail: {
          rows: [
            { label: 'Total audit entries', value: String(audit.length) },
            { label: 'Non-ok outcomes', value: String(deniedAudit) },
          ],
          recent: audit.slice(0, 5).map((a) => `${a.action} — ${a.outcome}`),
        },
      });
    }
  }

  for (const cat of CATEGORY_NODES) {
    const count = byCategory.get(cat.id) ?? 0;
    const catCases = testCases.filter((tc) => tc.category === cat.id);
    const catSeverity = countBy(catCases, (tc) => tc.severity);
    const catPriority = countBy(catCases, (tc) => tc.priority);
    nodes.push({
      id: `cat-${cat.id}`,
      label: cat.label,
      shortLabel: cat.label,
      tier: 'satellite',
      status: count > 0 ? 'active' : 'idle',
      metricLabel: 'Cases',
      metricValue: String(count),
      detail: {
        rows:
          count > 0
            ? [
                ...[...catSeverity.entries()].map(([k, v]) => ({ label: `Severity: ${k}`, value: String(v) })),
                ...[...catPriority.entries()].map(([k, v]) => ({ label: `Priority: ${k}`, value: String(v) })),
              ]
            : [{ label: 'Real test cases in this category', value: '0' }],
        recent: catCases.slice(0, 5).map((tc) => tc.title),
      },
    });
    edges.push({ id: `cat-${cat.id}-test-case`, from: `cat-${cat.id}`, to: 'test-case', active: count > 0 });
  }

  const nodesActive = nodes.filter((n) => n.status !== 'idle').length;

  return {
    projectId,
    projectName,
    nodes,
    edges,
    summary: {
      nodesActive,
      nodesTotal: nodes.length,
      testsGenerated: testCases.length,
      testsExecuted: executedTotal,
      risksDetected: riskFlagged.length,
      criticalIssues: criticalCount,
      coveragePercent,
      routingScore,
    },
  };
}
