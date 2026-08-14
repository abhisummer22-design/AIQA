/**
 * Runs list + real run creation.
 *
 * Backed by `GET /runs` and `POST /runs`. Creating a run submits a genuine
 * `RunPlan` to the orchestrator, which validates it (`scheduler::validate_plan`),
 * passes it through the policy engine, and either schedules it or holds it in
 * PENDING_APPROVAL. The previous UI's "trigger test" button pushed an object
 * into an in-memory array and never invoked an executor.
 *
 * Approve/reject are wired to the real governance endpoints and are only
 * offered for runs actually in PENDING_APPROVAL.
 */

import React, { useState } from "react";
import { Play, Plus, ShieldAlert, X } from "lucide-react";
import { useApiAction, useApiResource } from "../hooks/useApi";
import { approveRun, createRun, listRuns, rejectRun } from "../services/orchestrator";
import type { RunListItem, RunPlan, TaskDef } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { RunStateBadge } from "../components/common/Badges";

export function RunsView({ onOpenRun }: { onOpenRun: (runId: string) => void }) {
  const [showCreate, setShowCreate] = useState(false);
  const runs = useApiResource<RunListItem[]>((signal) => listRuns(100, signal), []);

  const approve = useApiAction((id: string) => approveRun(id));
  const reject = useApiAction((id: string) => rejectRun(id, "rejected from AIQA web UI"));

  const afterGovernance = async (fn: () => Promise<unknown>) => {
    await fn();
    runs.reload();
  };

  return (
    <div className="space-y-4">
      <header className="flex items-start justify-between">
        <div>
          <h1 className="text-lg font-bold tracking-tight text-slate-100">Runs</h1>
          <p className="mt-0.5 text-xs text-slate-500">
            Live from <code className="font-mono text-slate-400">GET /runs</code>. Creating a run submits a real plan
            to the orchestrator.
          </p>
        </div>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-1.5 rounded-md border border-cyan-700/60 bg-cyan-950/40 px-3 py-1.5 text-xs font-medium text-cyan-300 transition hover:bg-cyan-900/40"
        >
          <Plus size={13} /> New run
        </button>
      </header>

      {(approve.error || reject.error) && (
        <div className="rounded-lg border border-rose-800/60 bg-rose-950/30 px-3 py-2 text-xs text-rose-300">
          {(approve.error ?? reject.error)?.message}
        </div>
      )}

      <div className="rounded-xl border border-slate-800 bg-[#0a0f1a] p-3">
        <ResourceView
          state={runs}
          loadingLabel="Loading runs…"
          emptyTitle="No runs"
          emptyMessage="The orchestrator has no runs recorded yet."
        >
          {(rows) => (
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-600">
                  <th className="pb-2 font-normal">Run</th>
                  <th className="pb-2 font-normal">State</th>
                  <th className="pb-2 font-normal">Project</th>
                  <th className="pb-2 text-right font-normal">Passed</th>
                  <th className="pb-2 text-right font-normal">Failed</th>
                  <th className="pb-2 text-right font-normal">Total</th>
                  <th className="pb-2 text-right font-normal">Actions</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((run) => (
                  <tr key={run.run_id} className="border-t border-slate-800/70">
                    <td className="py-2 pr-3">
                      <button
                        onClick={() => onOpenRun(run.run_id)}
                        className="text-left font-medium text-slate-200 transition hover:text-cyan-300"
                      >
                        {run.name}
                      </button>
                      <div className="font-mono text-[10px] text-slate-600">{run.run_id}</div>
                    </td>
                    <td className="py-2 pr-3">
                      <RunStateBadge state={run.state} />
                    </td>
                    <td className="py-2 pr-3 font-mono text-[10px] text-slate-500">{run.project_id ?? "—"}</td>
                    <td className="py-2 pr-3 text-right font-mono text-emerald-400">{run.passed}</td>
                    <td className="py-2 pr-3 text-right font-mono text-rose-400">{run.failed}</td>
                    <td className="py-2 pr-3 text-right font-mono text-slate-400">{run.total}</td>
                    <td className="py-2 text-right">
                      {run.state === "PENDING_APPROVAL" ? (
                        <div className="flex justify-end gap-1.5">
                          <button
                            disabled={approve.isRunning}
                            onClick={() => afterGovernance(() => approve.run(run.run_id))}
                            className="rounded border border-emerald-700/60 px-2 py-0.5 text-[10px] text-emerald-300 transition hover:bg-emerald-950/40 disabled:opacity-40"
                          >
                            Approve
                          </button>
                          <button
                            disabled={reject.isRunning}
                            onClick={() => afterGovernance(() => reject.run(run.run_id))}
                            className="rounded border border-rose-700/60 px-2 py-0.5 text-[10px] text-rose-300 transition hover:bg-rose-950/40 disabled:opacity-40"
                          >
                            Reject
                          </button>
                        </div>
                      ) : (
                        <button
                          onClick={() => onOpenRun(run.run_id)}
                          className="text-[11px] text-cyan-400 transition hover:text-cyan-300"
                        >
                          Open
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </ResourceView>
      </div>

      {showCreate && (
        <CreateRunDialog
          onClose={() => setShowCreate(false)}
          onCreated={(runId) => {
            setShowCreate(false);
            runs.reload();
            onOpenRun(runId);
          }}
        />
      )}
    </div>
  );
}

/**
 * Submits a real `RunPlan`. The task rows map directly onto `TaskDef` in
 * domain.rs — id, name, prerequisites, and either an executor key or a
 * simulate spec. No field here is cosmetic.
 */
function CreateRunDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (runId: string) => void }) {
  const [name, setName] = useState("");
  const [concurrency, setConcurrency] = useState(4);
  const [tasksText, setTasksText] = useState(
    JSON.stringify(
      [
        { id: "t1", name: "smoke", executor: "playwright", params: {} },
        { id: "t2", name: "api-contract", prerequisites: ["t1"], executor: "schemathesis", params: {} },
      ],
      null,
      2,
    ),
  );
  const [parseError, setParseError] = useState<string | null>(null);

  const create = useApiAction((plan: RunPlan) => createRun(plan));

  const submit = async () => {
    setParseError(null);
    let tasks: TaskDef[];
    try {
      tasks = JSON.parse(tasksText);
      if (!Array.isArray(tasks)) throw new Error("tasks must be a JSON array");
    } catch (err) {
      setParseError(err instanceof Error ? err.message : "Invalid JSON");
      return;
    }

    const result = await create.run({ name: name.trim() || "unnamed run", tasks, concurrency });
    if (result) onCreated(result.run_id);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
      <div className="w-full max-w-2xl rounded-xl border border-slate-700 bg-[#0a0f1a] shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-800 px-4 py-3">
          <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-100">
            <Play size={14} className="text-cyan-400" /> Create run
          </h2>
          <button onClick={onClose} className="text-slate-500 transition hover:text-slate-300">
            <X size={16} />
          </button>
        </div>

        <div className="space-y-3 p-4">
          <div className="grid grid-cols-3 gap-3">
            <label className="col-span-2 block">
              <span className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-500">Run name</span>
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="regression-2026-08-14"
                className="w-full rounded border border-slate-700 bg-slate-900/60 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-cyan-600"
              />
            </label>
            <label className="block">
              <span className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-500">
                Concurrency
              </span>
              <input
                type="number"
                min={1}
                value={concurrency}
                onChange={(e) => setConcurrency(Math.max(1, Number(e.target.value) || 1))}
                className="w-full rounded border border-slate-700 bg-slate-900/60 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-cyan-600"
              />
            </label>
          </div>

          <label className="block">
            <span className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-500">
              Tasks (TaskDef[] — see domain.rs)
            </span>
            <textarea
              value={tasksText}
              onChange={(e) => setTasksText(e.target.value)}
              rows={12}
              spellCheck={false}
              className="w-full rounded border border-slate-700 bg-slate-900/60 px-2.5 py-2 font-mono text-[11px] leading-relaxed text-slate-300 outline-none focus:border-cyan-600"
            />
            <span className="mt-1 block text-[10px] text-slate-600">
              executor: playwright | newman | pytest | maestro | schemathesis. Omit it and supply{" "}
              <code className="font-mono">simulate</code> to use the built-in SimulatedExecutor.
            </span>
          </label>

          {parseError && (
            <div className="rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-[11px] text-rose-300">
              {parseError}
            </div>
          )}

          {create.error && (
            <div className="flex items-start gap-2 rounded border border-rose-800/60 bg-rose-950/30 px-2.5 py-1.5 text-[11px] text-rose-300">
              <ShieldAlert size={13} className="mt-px shrink-0" />
              <span>{create.error.detail || create.error.message}</span>
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-slate-800 px-4 py-3">
          <button
            onClick={onClose}
            className="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-400 transition hover:bg-slate-800"
          >
            Cancel
          </button>
          <button
            onClick={submit}
            disabled={create.isRunning}
            className="rounded border border-cyan-700/60 bg-cyan-950/40 px-3 py-1.5 text-xs font-medium text-cyan-300 transition hover:bg-cyan-900/40 disabled:opacity-40"
          >
            {create.isRunning ? "Submitting…" : "Submit plan"}
          </button>
        </div>
      </div>
    </div>
  );
}
