/**
 * Dashboard — real counters and real distributions only.
 *
 * Sources: `GET /metrics` (Prometheus counters computed on demand from the
 * database) and `GET /runs`.
 *
 * Deliberately absent, because the orchestrator does not compute them:
 * "quality score", pass percentage, trend arrows, "executed today", a risk
 * level, an overall flakiness percentage, and any time series. The backend
 * stores no historical snapshots, so a 7-day trend cannot be drawn honestly
 * — so it is not drawn.
 */

import React from "react";
import { ArrowRight, BarChart3, PieChart as PieIcon, Workflow } from "lucide-react";
import type { ViewId } from "../App";
import { useApiResource } from "../hooks/useApi";
import { getMetrics, listRuns } from "../services/orchestrator";
import type { OrchestratorMetrics, RunListItem } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { RunStateBadge } from "../components/common/Badges";
import { RunStateChart, TaskStatusChart } from "../components/common/Charts";
import { Card, Tile } from "../components/common/Card";

export function DashboardView({
  onOpenRun,
  onNavigate,
}: {
  onOpenRun: (runId: string) => void;
  onNavigate: (view: ViewId) => void;
}) {
  const metrics = useApiResource<OrchestratorMetrics>((signal) => getMetrics(signal), [], {
    isEmpty: () => false,
  });
  const runs = useApiResource<RunListItem[]>((signal) => listRuns(8, signal), []);

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-lg font-bold tracking-tight text-slate-100">Dashboard</h1>
        <p className="mt-0.5 text-xs text-slate-500">
          Read live from <code className="font-mono text-slate-400">GET /metrics</code> and{" "}
          <code className="font-mono text-slate-400">GET /runs</code>. Only values the orchestrator computes are shown.
        </p>
      </header>

      <ResourceView state={metrics} loadingLabel="Reading orchestrator metrics…">
        {(m) => (
          <>
            <MetricTiles metrics={m} />

            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <Card icon={<PieIcon size={14} />} title="Runs by state">
                <RunStateChart data={m.runsByState} />
              </Card>
              <Card icon={<BarChart3 size={14} />} title="Tasks by status">
                <TaskStatusChart data={m.tasksByStatus} />
                <p className="mt-2 font-mono text-[9px] text-slate-600">
                  FAILED counts product defects; INFRA_ERROR counts infrastructure faults. FRD §2 keeps them separate.
                </p>
              </Card>
            </div>
          </>
        )}
      </ResourceView>

      <Card
        icon={<Workflow size={14} />}
        title="Recent runs"
        action={
          <button
            onClick={() => onNavigate("runs")}
            className="flex items-center gap-1 text-[11px] text-cyan-400 transition hover:text-cyan-300"
          >
            All runs <ArrowRight size={11} />
          </button>
        }
      >
        <ResourceView
          state={runs}
          loadingLabel="Loading runs…"
          emptyTitle="No runs yet"
          emptyMessage="The orchestrator has no runs recorded. Create one from the Runs screen."
        >
          {(rows) => (
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-600">
                  <th className="pb-2 font-normal">Run</th>
                  <th className="pb-2 font-normal">State</th>
                  <th className="pb-2 text-right font-normal">Passed</th>
                  <th className="pb-2 text-right font-normal">Failed</th>
                  <th className="pb-2 text-right font-normal">Total</th>
                  <th className="pb-2" />
                </tr>
              </thead>
              <tbody>
                {rows.map((run) => (
                  <tr key={run.run_id} className="border-t border-slate-800/70">
                    <td className="py-2 pr-3">
                      <button
                        onClick={() => onOpenRun(run.run_id)}
                        className="truncate text-left font-medium text-slate-200 transition hover:text-cyan-300"
                      >
                        {run.name}
                      </button>
                      <div className="font-mono text-[10px] text-slate-600">{run.run_id.slice(0, 8)}</div>
                    </td>
                    <td className="py-2 pr-3">
                      <RunStateBadge state={run.state} />
                    </td>
                    <td className="py-2 pr-3 text-right font-mono text-emerald-400">{run.passed}</td>
                    <td className="py-2 pr-3 text-right font-mono text-rose-400">{run.failed}</td>
                    <td className="py-2 pr-3 text-right font-mono text-slate-400">{run.total}</td>
                    <td className="py-2 text-right">
                      <button
                        onClick={() => onOpenRun(run.run_id)}
                        className="text-[11px] text-cyan-400 transition hover:text-cyan-300"
                      >
                        Open
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </ResourceView>
      </Card>
    </div>
  );
}

function MetricTiles({ metrics }: { metrics: OrchestratorMetrics }) {
  const sum = (rec: Record<string, number>) => Object.values(rec).reduce((a, b) => a + b, 0);

  const totalRuns = sum(metrics.runsByState);
  const totalTasks = sum(metrics.tasksByStatus);
  const running = metrics.runsByState["RUNNING"] ?? 0;
  const pendingApproval = metrics.runsByState["PENDING_APPROVAL"] ?? 0;
  const passed = metrics.tasksByStatus["PASSED"] ?? 0;
  const failed = metrics.tasksByStatus["FAILED"] ?? 0;
  const infra = metrics.tasksByStatus["INFRA_ERROR"] ?? 0;
  const pendingDrafts = metrics.bugDraftsByReviewState["PENDING"] ?? 0;

  return (
    <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
      <Tile label="Runs total" value={totalRuns} sub={`${running} running`} />
      <Tile label="Tasks total" value={totalTasks} sub={`${passed} passed · ${failed} failed`} />
      <Tile label="Infra errors" value={infra} sub="distinct from product failures" tone={infra > 0 ? "warn" : "neutral"} />
      <Tile
        label="Pending approval"
        value={pendingApproval}
        sub="runs held by policy engine"
        tone={pendingApproval > 0 ? "warn" : "neutral"}
      />
      <Tile label="Failure clusters" value={metrics.failureClustersTotal} sub="from intelligence pipeline" />
      <Tile label="Bug drafts pending" value={pendingDrafts} sub="awaiting human review" />
      <Tile label="Active API keys" value={metrics.apiKeysActive} sub="non-revoked" />
      <Tile label="Audit entries" value={metrics.auditLogTotal} sub="total recorded" />
    </div>
  );
}
