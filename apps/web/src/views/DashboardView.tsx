/**
 * Dashboard — real counters and real distributions only.
 *
 * Sources: `GET /metrics` (Prometheus counters computed on demand from the
 * database) and `GET /runs`.
 *
 * Only values the orchestrator computes are rendered.
 */

import React from "react";
import {
  ArrowRight,
  BarChart3,
  PieChart as PieIcon,
  Workflow,
  Plus,
  Play,
  Server,
  Activity,
  Bug,
  ShieldCheck,
  KeyRound,
  ScrollText,
} from "lucide-react";
import type { ViewId } from "../App";
import { useApiResource } from "../hooks/useApi";
import { getMetrics, listRuns } from "../services/orchestrator";
import type { OrchestratorMetrics, RunListItem } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { RunStateBadge } from "../components/common/Badges";
import { RunStateChart, TaskStatusChart } from "../components/common/Charts";
import { Card } from "../components/common/Card";
import { MetricCard } from "../components/common/MetricCard";
import { SectionHeader } from "../components/common/SectionHeader";

export function DashboardView({
  onOpenRun,
  onNavigate,
  onOpenNewRunModal,
}: {
  onOpenRun: (runId: string) => void;
  onNavigate: (view: ViewId) => void;
  onOpenNewRunModal: () => void;
}) {
  const metrics = useApiResource<OrchestratorMetrics>((signal) => getMetrics(signal), [], {
    isEmpty: () => false,
  });
  const runs = useApiResource<RunListItem[]>((signal) => listRuns(8, signal), []);

  return (
    <div className="space-y-4">
      <SectionHeader
        title="Quality Control Plane Dashboard"
        subtitle="Authoritative operational telemetry read directly from GET /metrics and GET /runs. No simulated numbers."
        badge="REALTIME BACKEND"
        badgeTone="live"
        action={
          <div className="flex items-center gap-2">
            <button
              onClick={() => onNavigate("runs")}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 bg-slate-900/60 hover:bg-slate-800 text-xs text-slate-300 transition-colors cursor-pointer"
            >
              <Workflow size={13} />
              <span>Execution Hub</span>
            </button>
            <button
              onClick={onOpenNewRunModal}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-cyan-600 hover:bg-cyan-500 text-white font-semibold text-xs transition-all shadow-sm shadow-cyan-950 cursor-pointer"
            >
              <Plus size={13} strokeWidth={2.5} />
              <span>Launch Run</span>
            </button>
          </div>
        }
      />

      <ResourceView state={metrics} loadingLabel="Reading orchestrator metrics…">
        {(m) => (
          <>
            <MetricTiles metrics={m} onNavigate={onNavigate} />

            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <Card icon={<PieIcon size={14} />} title="Execution Runs by State">
                <RunStateChart data={m.runsByState} />
              </Card>
              <Card icon={<BarChart3 size={14} />} title="Tasks by Execution Status">
                <TaskStatusChart data={m.tasksByStatus} />
                <p className="mt-2 font-mono text-[9px] text-slate-500 leading-relaxed border-t border-slate-800/80 pt-2">
                  FAILED represents product defects; INFRA_ERROR represents infrastructure/network faults. (FRD §2).
                </p>
              </Card>
            </div>
          </>
        )}
      </ResourceView>

      <Card
        icon={<Workflow size={14} />}
        title="Recent Execution Runs"
        action={
          <button
            onClick={() => onNavigate("runs")}
            className="flex items-center gap-1 text-[11px] text-cyan-400 transition hover:text-cyan-300 font-mono"
          >
            <span>View All Runs</span>
            <ArrowRight size={11} />
          </button>
        }
      >
        <ResourceView
          state={runs}
          loadingLabel="Loading runs…"
          emptyTitle="No runs recorded"
          emptyMessage="The orchestrator has no runs recorded yet. Launch a new run to dispatch workloads."
        >
          {(rows) => (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs min-w-[640px]">
                <thead>
                  <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800/80">
                    <th className="pb-2 font-normal">Run Plan</th>
                    <th className="pb-2 font-normal">Status</th>
                    <th className="pb-2 text-right font-normal">Passed</th>
                    <th className="pb-2 text-right font-normal">Failed</th>
                    <th className="pb-2 text-right font-normal">Total Tasks</th>
                    <th className="pb-2 text-right font-normal">Action</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((run) => (
                    <tr key={run.run_id} className="border-t border-slate-800/60 hover:bg-slate-900/30 transition-colors">
                      <td className="py-2.5 pr-3">
                        <button
                          onClick={() => onOpenRun(run.run_id)}
                          className="truncate text-left font-semibold text-slate-200 hover:text-cyan-300 transition-colors cursor-pointer"
                        >
                          {run.name}
                        </button>
                        <div className="font-mono text-[10px] text-slate-500">{run.run_id}</div>
                      </td>
                      <td className="py-2.5 pr-3">
                        <RunStateBadge state={run.state} />
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-emerald-400 font-semibold">{run.passed}</td>
                      <td className="py-2.5 pr-3 text-right font-mono text-rose-400 font-semibold">{run.failed}</td>
                      <td className="py-2.5 pr-3 text-right font-mono text-slate-300">{run.total}</td>
                      <td className="py-2.5 text-right">
                        <button
                          onClick={() => onOpenRun(run.run_id)}
                          className="px-2 py-1 rounded bg-cyan-950/60 border border-cyan-800/60 text-cyan-300 hover:bg-cyan-900/60 font-mono text-[10px] transition-colors cursor-pointer"
                        >
                          Inspect Run
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </ResourceView>
      </Card>
    </div>
  );
}

function MetricTiles({
  metrics,
  onNavigate,
}: {
  metrics: OrchestratorMetrics;
  onNavigate: (view: ViewId) => void;
}) {
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
      <MetricCard
        label="Total Execution Runs"
        value={totalRuns}
        subtext={`${running} active running now`}
        icon={<Workflow size={14} />}
        variant="info"
        onClick={() => onNavigate("runs")}
      />
      <MetricCard
        label="Tasks Dispatched"
        value={totalTasks}
        subtext={`${passed} passed · ${failed} failed`}
        icon={<Activity size={14} />}
        variant={failed > 0 ? "danger" : "success"}
      />
      <MetricCard
        label="Infra Errors"
        value={infra}
        subtext="Isolated from product defects"
        icon={<Server size={14} />}
        variant={infra > 0 ? "warning" : "default"}
      />
      <MetricCard
        label="Policy Gate Holds"
        value={pendingApproval}
        subtext="Pending human review"
        icon={<ShieldCheck size={14} />}
        variant={pendingApproval > 0 ? "warning" : "default"}
        onClick={() => onNavigate("runs")}
      />
      <MetricCard
        label="Failure Clusters"
        value={metrics.failureClustersTotal}
        subtext="From intelligence pipeline"
        icon={<Bug size={14} />}
        onClick={() => onNavigate("intelligence")}
      />
      <MetricCard
        label="Bug Drafts Pending"
        value={pendingDrafts}
        subtext="Awaiting defect promotion"
        icon={<Bug size={14} />}
        variant={pendingDrafts > 0 ? "warning" : "default"}
        onClick={() => onNavigate("bug-drafts")}
      />
      <MetricCard
        label="Active API Keys"
        value={metrics.apiKeysActive}
        subtext="Configured control plane keys"
        icon={<KeyRound size={14} />}
        onClick={() => onNavigate("settings")}
      />
      <MetricCard
        label="Audit Log Entries"
        value={metrics.auditLogTotal}
        subtext="Immutable server records"
        icon={<ScrollText size={14} />}
        onClick={() => onNavigate("audit-log")}
      />
    </div>
  );
}
