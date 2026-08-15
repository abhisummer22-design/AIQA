/**
 * Two views of a run that read fields the backend has always returned but
 * nothing has ever displayed:
 *
 *   DependencyGraph — the real DAG from `TaskState.prerequisites`. This is
 *     the structure the scheduler actually executes, and it is what makes a
 *     BLOCKED task legible: you can see which prerequisite failed.
 *
 *   TaskWaterfall — real wall-clock timing from `started_at`/`finished_at`.
 *     Bars are positioned against the run's own start and end, so the
 *     picture shows genuine concurrency and genuine duration. The previous
 *     UI displayed a progress percentage advanced by a random timer.
 *
 * Both degrade honestly: a task the backend never started has no bar, and a
 * run with no timing information says so rather than drawing a flat line.
 */

import React from "react";
import type { TaskState, TaskStatus } from "../../types/orchestrator";
import { TASK_STATUS_COLOR } from "./Charts";
import { durationMs, formatDuration } from "../../lib/format";

// --------------------------------------------------------- dependency DAG

interface Layer {
  depth: number;
  tasks: TaskState[];
}

/**
 * Longest-path layering. Depth = 1 + max(depth of prerequisites), which puts
 * every task strictly to the right of everything it waits on. Unknown
 * prerequisite ids are ignored rather than assumed — the orchestrator
 * rejects those at plan validation, so they should not occur.
 */
function layerTasks(tasks: TaskState[]): Layer[] {
  const byId = new Map(tasks.map((t) => [t.id, t]));
  const depth = new Map<string, number>();

  const resolve = (id: string, seen: Set<string>): number => {
    if (depth.has(id)) return depth.get(id)!;
    if (seen.has(id)) return 0; // defensive: cycles are rejected server-side
    seen.add(id);
    const task = byId.get(id);
    const prereqs = (task?.prerequisites ?? []).filter((p) => byId.has(p));
    const d = prereqs.length === 0 ? 0 : 1 + Math.max(...prereqs.map((p) => resolve(p, seen)));
    depth.set(id, d);
    return d;
  };

  for (const t of tasks) resolve(t.id, new Set());

  const grouped = new Map<number, TaskState[]>();
  for (const t of tasks) {
    const d = depth.get(t.id) ?? 0;
    if (!grouped.has(d)) grouped.set(d, []);
    grouped.get(d)!.push(t);
  }

  return [...grouped.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([d, ts]) => ({ depth: d, tasks: ts.sort((a, b) => a.id.localeCompare(b.id)) }));
}

export function DependencyGraph({ tasks }: { tasks: TaskState[] }) {
  const layers = React.useMemo(() => layerTasks(tasks), [tasks]);
  const hasEdges = tasks.some((t) => t.prerequisites.length > 0);

  if (tasks.length === 0) {
    return <p className="py-6 text-center font-mono text-[10px] text-slate-600">No tasks in this run.</p>;
  }

  return (
    <div className="space-y-2">
      {!hasEdges && (
        <p className="font-mono text-[10px] text-slate-600">
          All {tasks.length} tasks are independent — no prerequisites declared, so every task was eligible immediately.
        </p>
      )}

      <div className="flex gap-3 overflow-x-auto pb-1">
        {layers.map((layer, idx) => (
          <div key={layer.depth} className="flex min-w-[150px] flex-col gap-1.5">
            <div className="font-mono text-[9px] uppercase tracking-wider text-slate-600">
              {idx === 0 ? "no prerequisites" : `depth ${layer.depth}`}
            </div>
            {layer.tasks.map((t) => (
              <div
                key={t.id}
                className="rounded-md border border-slate-800 bg-slate-900/50 px-2 py-1.5"
                style={{ borderLeftWidth: 3, borderLeftColor: TASK_STATUS_COLOR[t.status] }}
                title={t.reason ?? t.status}
              >
                <div className="truncate text-[11px] font-medium text-slate-200">{t.name}</div>
                <div className="flex items-center justify-between gap-2">
                  <span className="font-mono text-[9px] text-slate-600">{t.id}</span>
                  <span className="font-mono text-[9px]" style={{ color: TASK_STATUS_COLOR[t.status] }}>
                    {t.status}
                  </span>
                </div>
                {t.prerequisites.length > 0 && (
                  <div className="mt-0.5 truncate font-mono text-[9px] text-slate-600">
                    waits on {t.prerequisites.join(", ")}
                  </div>
                )}
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

// ------------------------------------------------------------- waterfall

export function TaskWaterfall({
  tasks,
  runStartedAt,
  runFinishedAt,
}: {
  tasks: TaskState[];
  runStartedAt: string;
  runFinishedAt: string | null;
}) {
  const timed = tasks.filter((t) => t.started_at);

  if (timed.length === 0) {
    return (
      <p className="py-6 text-center font-mono text-[10px] text-slate-600">
        No task has started yet, so there is no timing to show.
      </p>
    );
  }

  // Window the bars against real boundaries: earliest observed start to
  // latest observed finish (falling back to now for a still-running run).
  const startTimes = timed.map((t) => new Date(t.started_at!).getTime());
  const endTimes = timed.map((t) => (t.finished_at ? new Date(t.finished_at).getTime() : Date.now()));

  const t0 = Math.min(new Date(runStartedAt).getTime(), ...startTimes);
  const t1 = Math.max(runFinishedAt ? new Date(runFinishedAt).getTime() : 0, ...endTimes);
  const span = Math.max(1, t1 - t0);

  const ordered = [...timed].sort(
    (a, b) => new Date(a.started_at!).getTime() - new Date(b.started_at!).getTime(),
  );

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between font-mono text-[9px] text-slate-600">
        <span>0ms</span>
        <span>total {formatDuration(span)}</span>
      </div>

      <div className="space-y-1">
        {ordered.map((t) => {
          const s = new Date(t.started_at!).getTime();
          const e = t.finished_at ? new Date(t.finished_at).getTime() : Date.now();
          const leftPct = ((s - t0) / span) * 100;
          // Floor the width so a sub-millisecond task is still visible.
          const widthPct = Math.max(1.2, ((e - s) / span) * 100);
          const d = durationMs(t.started_at, t.finished_at);

          return (
            <div key={t.id} className="flex items-center gap-2">
              <div className="w-36 shrink-0 truncate text-[10px] text-slate-400" title={t.name}>
                {t.name}
              </div>
              <div className="relative h-3.5 flex-1 rounded bg-slate-900/60">
                <div
                  className="absolute top-0 h-full rounded"
                  style={{
                    left: `${leftPct}%`,
                    width: `${widthPct}%`,
                    background: TASK_STATUS_COLOR[t.status],
                    opacity: t.finished_at ? 0.85 : 0.5,
                  }}
                  title={`${t.status} · ${formatDuration(d)}${t.finished_at ? "" : " (still running)"}`}
                />
              </div>
              <div className="w-14 shrink-0 text-right font-mono text-[9px] text-slate-500">
                {formatDuration(d)}
              </div>
            </div>
          );
        })}
      </div>

      {timed.length < tasks.length && (
        <p className="font-mono text-[9px] text-slate-600">
          {tasks.length - timed.length} task(s) never started (blocked or skipped) and have no bar.
        </p>
      )}
    </div>
  );
}

/** Ten-status counter strip — the full FRD §2 model, not an abridged six. */
export function StatusCounters({
  counters,
}: {
  counters: Array<{ label: string; status: TaskStatus; value: number }>;
}) {
  return (
    <div className="grid grid-cols-3 gap-1.5 sm:grid-cols-5">
      {counters.map((c) => (
        <div
          key={c.status}
          className={`rounded-lg border px-2 py-1.5 ${
            c.value > 0 ? "border-slate-700 bg-slate-900/60" : "border-slate-800/60 bg-slate-900/20"
          }`}
        >
          <div className="truncate font-mono text-[8.5px] uppercase tracking-wider text-slate-500">{c.label}</div>
          <div
            className="font-mono text-base font-bold"
            style={{ color: c.value > 0 ? TASK_STATUS_COLOR[c.status] : "#334155" }}
          >
            {c.value}
          </div>
        </div>
      ))}
    </div>
  );
}
