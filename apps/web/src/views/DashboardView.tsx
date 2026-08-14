/**
 * Dashboard — real counters only.
 *
 * Every number on this screen comes from one of two real sources:
 *   GET /metrics  (Prometheus text, computed on demand from the database)
 *   GET /runs     (the actual run list)
 *
 * Deliberately absent, because the orchestrator does not compute them and
 * inventing them was the single largest category of fake data in the
 * previous UI: "quality score", "pass percentage", trend arrows
 * ("+1.4%"), "executed today", risk level, flakiness percentage, open-bug
 * counts by severity, and the seven-day historical score series.
 */

import React from "react";
import { ArrowRight, Workflow } from "lucide-react";
import type { ViewId } from "../App";
import { useApiResource } from "../hooks/useApi";
import { getMetrics, listRuns } from "../services/orchestrator";
import type { OrchestratorMetrics, RunListItem, RunState } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { RunStateBadge } from "../components/common/Badges";

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
  const runs = useApiResource<RunListItem[]>((signal) => listRuns(10, signal), []);

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-lg font-bold tracking-tight text-slate-100">Dashboard</h1>
        <p className="mt-0.5 text-xs text-slate-500">
          Counters read live from <code className="font-mono text-slate-400">GET /metrics</code>. Only values the
          orchestrator computes are shown.
        </p>
      </header>

      {/* Counter tiles */}
      <ResourceView state={metrics} loadingLabel="Reading orchestrator metrics…">
        {(m) => <MetricTiles metrics={m} />}
      </ResourceView>

      {/* Recent runs */}
      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
        <div className="flex items-center justify-between border-b border-slate-800 px-4 py-2.5">
          <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
            <Workflow size={14} className="text-cyan-400" />
            Recent runs
          </h2>
          <button
            onClick={() => onNavigate("runs")}
            className="flex items-center gap-1 text-[11px] text-cyan-400 transition hover:text-cyan-300"
          >
            All runs <ArrowRight size={11} />
          </button>
        </div>

        <div className="p-3">
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
                        <div className="truncate font-medium text-slate-200">{run.name}</div>
                        <div className="font-mono text-[10px] text-slate-600">{run.run_id.slice(0, 8)}</div>
                      </td>
                      <td className="py-2 pr-3">
                        <RunStateBadge state={run.state as RunState} />
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
        </div>
      </section>
    </div>
  );
}

function MetricTiles({ metrics }: { metrics: OrchestratorMetrics }) {
  const sum = (rec: Record<string, number>) => Object.values(rec).reduce((a, b) => a + b, 0);

  const totalRuns = sum(metrics.runsByState);
  const totalTasks = sum(metrics.tasksByStatus);
  const running = metrics.runsByState["RUNNING"] ?? 0;
  const pendingApproval = metrics.runsByState["PENDING_APPROVAL"] ?? 0;
  const tasksPassed = metrics.tasksByStatus["PASSED"] ?? 0;
  const tasksFailed = metrics.tasksByStatus["FAILED"] ?? 0;
  const infraErrors = metrics.tasksByStatus["INFRA_ERROR"] ?? 0;
  const pendingDrafts = metrics.bugDraftsByReviewState["PENDING"] ?? 0;

  return (
    <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
      <Tile label="Runs total" value={totalRuns} sub={`${running} running`} />
      <Tile label="Tasks total" value={totalTasks} sub={`${tasksPassed} passed · ${tasksFailed} failed`} />
      <Tile
        label="Infra errors"
        value={infraErrors}
        sub="distinct from product failures"
        tone={infraErrors > 0 ? "warn" : "neutral"}
      />
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

function Tile({
  label,
  value,
  sub,
  tone = "neutral",
}: {
  label: string;
  value: number | null;
  sub?: string;
  tone?: "neutral" | "warn";
}) {
  return (
    <div className="rounded-xl border border-slate-800 bg-[#0a0f1a] px-3.5 py-3">
      <div className="font-mono text-[10px] uppercase tracking-wider text-slate-500">{label}</div>
      <div
        className={`mt-1 font-mono text-2xl font-bold ${
          value === null ? "text-slate-600" : tone === "warn" ? "text-amber-400" : "text-slate-100"
        }`}
      >
        {/* A metric the orchestrator did not emit shows as "—", never as 0. */}
        {value === null ? "—" : value}
      </div>
      {sub && <div className="mt-0.5 text-[10px] text-slate-600">{sub}</div>}
    </div>
  );
}
