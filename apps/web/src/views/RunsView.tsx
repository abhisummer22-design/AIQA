/**
 * Runs list + real run creation and governance.
 *
 * Backed by `GET /runs`, `POST /runs`, `POST /runs/:id/approve`, `POST /runs/:id/reject`.
 */

import React, { useState } from "react";
import { Play, Plus, ShieldAlert, Workflow, Filter, Search, RotateCcw } from "lucide-react";
import { useApiAction, useApiResource } from "../hooks/useApi";
import { approveRun, createRun, listRuns, rejectRun } from "../services/orchestrator";
import type { RunListItem, RunPlan, TaskDef } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { RunStateBadge } from "../components/common/Badges";
import { SectionHeader } from "../components/common/SectionHeader";
import { Card } from "../components/common/Card";

const STATE_FILTERS: Array<"ALL" | RunListItem["state"]> = [
  "ALL",
  "RUNNING",
  "PENDING_APPROVAL",
  "PARTIAL",
  "COMPLETED",
  "FAILED",
  "REJECTED",
];

export function RunsView({
  onOpenRun,
  onOpenNewRunModal,
}: {
  onOpenRun: (runId: string) => void;
  onOpenNewRunModal?: () => void;
}) {
  const [showCreateFallback, setShowCreateFallback] = useState(false);
  const [query, setQuery] = useState("");
  const [stateFilter, setStateFilter] = useState<(typeof STATE_FILTERS)[number]>("ALL");
  const runs = useApiResource<RunListItem[]>((signal) => listRuns(100, signal), []);

  const approve = useApiAction((id: string) => approveRun(id));
  const reject = useApiAction((id: string) => rejectRun(id, "rejected from AIQA web UI"));

  const afterGovernance = async (fn: () => Promise<unknown>) => {
    await fn();
    runs.reload();
  };

  const handleOpenCreate = () => {
    if (onOpenNewRunModal) {
      onOpenNewRunModal();
    } else {
      setShowCreateFallback(true);
    }
  };

  return (
    <div className="space-y-4">
      <SectionHeader
        title="Test Execution Hub"
        subtitle="Live orchestrator execution dispatch, running workloads, and policy approval gates."
        badge="REALTIME BACKEND"
        badgeTone="live"
        action={
          <div className="flex items-center gap-2">
            <button
              onClick={() => runs.reload()}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 bg-slate-900/60 hover:bg-slate-800 text-xs text-slate-300 transition-colors cursor-pointer"
            >
              <RotateCcw size={12} />
              <span>Refresh</span>
            </button>
            <button
              onClick={handleOpenCreate}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-cyan-600 hover:bg-cyan-500 text-white font-semibold text-xs transition-all shadow-sm shadow-cyan-950 cursor-pointer"
            >
              <Plus size={13} strokeWidth={2.5} />
              <span>Launch Execution Run</span>
            </button>
          </div>
        }
      />

      {(approve.error || reject.error) && (
        <div className="rounded-lg border border-rose-800/60 bg-rose-950/30 px-3 py-2 text-xs text-rose-300">
          {(approve.error ?? reject.error)?.message}
        </div>
      )}

      {/* Filter and Search Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 p-2.5 rounded-xl border border-slate-800 bg-[#0a0f1a]">
        <div className="flex items-center gap-2 flex-1 max-w-sm">
          <div className="relative w-full">
            <Search size={13} className="absolute left-2.5 top-2.5 text-slate-500" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search by run name or id…"
              className="w-full pl-8 pr-3 py-1.5 rounded-md border border-slate-700 bg-slate-900/80 text-xs text-slate-200 outline-none focus:border-cyan-500 font-sans"
            />
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="font-mono text-[10px] uppercase text-slate-500 mr-1 flex items-center gap-1">
            <Filter size={11} /> Filter:
          </span>
          {STATE_FILTERS.map((s) => (
            <button
              key={s}
              onClick={() => setStateFilter(s)}
              className={`rounded px-2 py-1 font-mono text-[10px] uppercase tracking-wider transition-colors cursor-pointer ${
                stateFilter === s
                  ? "border border-cyan-700/80 bg-cyan-950/60 text-cyan-300 font-semibold"
                  : "border border-slate-800/80 bg-slate-900/40 text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
              }`}
            >
              {s === "ALL" ? "All States" : s.replace("_", " ")}
            </button>
          ))}
        </div>
      </div>

      <div className="rounded-xl border border-slate-800 bg-[#0a0f1a] p-3 shadow-lg">
        <ResourceView
          state={runs}
          loadingLabel="Loading execution runs…"
          emptyTitle="No execution runs"
          emptyMessage="The orchestrator has no runs recorded yet. Click Launch Run to dispatch a workload."
        >
          {(allRows) => {
            const q = query.trim().toLowerCase();
            const rows = allRows.filter(
              (r) =>
                (stateFilter === "ALL" || r.state === stateFilter) &&
                (!q || r.name.toLowerCase().includes(q) || r.run_id.toLowerCase().includes(q)),
            );

            if (rows.length === 0) {
              return (
                <div className="py-12 text-center text-xs text-slate-500 font-mono">
                  No execution runs match your filter criteria ({allRows.length} runs loaded).
                </div>
              );
            }

            return (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs min-w-[760px]">
                  <thead>
                    <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800/80">
                      <th className="pb-2.5 font-normal">Run Identifier</th>
                      <th className="pb-2.5 font-normal">State</th>
                      <th className="pb-2.5 font-normal">Project Scope</th>
                      <th className="pb-2.5 text-right font-normal">Passed</th>
                      <th className="pb-2.5 text-right font-normal">Failed</th>
                      <th className="pb-2.5 text-right font-normal">Total Tasks</th>
                      <th className="pb-2.5 text-right font-normal">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((run) => (
                      <tr key={run.run_id} className="border-t border-slate-800/60 hover:bg-slate-900/40 transition-colors">
                        <td className="py-3 pr-3">
                          <button
                            onClick={() => onOpenRun(run.run_id)}
                            className="text-left font-semibold text-slate-200 hover:text-cyan-300 transition-colors cursor-pointer"
                          >
                            {run.name}
                          </button>
                          <div className="font-mono text-[10px] text-slate-500">{run.run_id}</div>
                        </td>
                        <td className="py-3 pr-3">
                          <RunStateBadge state={run.state} />
                        </td>
                        <td className="py-3 pr-3 font-mono text-[10px] text-slate-400">
                          {run.project_id ? (
                            <span className="px-1.5 py-0.5 rounded bg-slate-900 border border-slate-800 text-slate-300">
                              {run.project_id}
                            </span>
                          ) : (
                            <span className="text-slate-600">global</span>
                          )}
                        </td>
                        <td className="py-3 pr-3 text-right font-mono text-emerald-400 font-semibold">{run.passed}</td>
                        <td className="py-3 pr-3 text-right font-mono text-rose-400 font-semibold">{run.failed}</td>
                        <td className="py-3 pr-3 text-right font-mono text-slate-300">{run.total}</td>
                        <td className="py-3 text-right">
                          {run.state === "PENDING_APPROVAL" ? (
                            <div className="flex justify-end gap-1.5">
                              <button
                                disabled={approve.isRunning}
                                onClick={() => afterGovernance(() => approve.run(run.run_id))}
                                className="rounded border border-emerald-700/60 bg-emerald-950/40 px-2.5 py-1 text-[10px] font-mono text-emerald-300 transition hover:bg-emerald-900/60 disabled:opacity-40 cursor-pointer"
                              >
                                Approve
                              </button>
                              <button
                                disabled={reject.isRunning}
                                onClick={() => afterGovernance(() => reject.run(run.run_id))}
                                className="rounded border border-rose-700/60 bg-rose-950/40 px-2.5 py-1 text-[10px] font-mono text-rose-300 transition hover:bg-rose-900/60 disabled:opacity-40 cursor-pointer"
                              >
                                Reject
                              </button>
                            </div>
                          ) : (
                            <button
                              onClick={() => onOpenRun(run.run_id)}
                              className="px-2.5 py-1 rounded bg-cyan-950/60 border border-cyan-800/60 text-cyan-300 hover:bg-cyan-900/60 font-mono text-[10px] transition-colors cursor-pointer"
                            >
                              Inspect Details
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          }}
        </ResourceView>
      </div>
    </div>
  );
}
