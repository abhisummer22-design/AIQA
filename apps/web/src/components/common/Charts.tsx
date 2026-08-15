/**
 * Charts over real orchestrator data.
 *
 * Every series here is a direct rendering of counts the backend returned —
 * `aiqa_runs_total{state}` and `aiqa_tasks_total{status}` from `/metrics`,
 * or the per-run counters on `RunSummary`. There is no interpolation, no
 * generated trend line, and no time series: the orchestrator does not store
 * historical snapshots, so a "quality score over the last 7 days" chart
 * cannot be drawn honestly and is not drawn at all.
 *
 * A chart with no data renders an explicit empty note rather than an empty
 * axis, so "nothing has run yet" never looks like "everything is at zero".
 */

import React from "react";
import { Bar, BarChart, Cell, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { RunState, TaskStatus } from "../../types/orchestrator";

/** Status colours match the badge palette so a chart segment and a table
 *  chip for the same status always read as the same thing. */
export const TASK_STATUS_COLOR: Record<TaskStatus, string> = {
  QUEUED: "#64748b",
  RUNNING: "#38bdf8",
  PASSED: "#34d399",
  FAILED: "#fb7185",
  BLOCKED: "#fbbf24",
  SKIPPED: "#475569",
  CANCELLED: "#475569",
  TIMEOUT: "#fb923c",
  INFRA_ERROR: "#e879f9",
  NEEDS_REVIEW: "#a78bfa",
};

export const RUN_STATE_COLOR: Record<RunState, string> = {
  CREATED: "#64748b",
  PENDING_APPROVAL: "#fbbf24",
  REJECTED: "#fb7185",
  SCHEDULED: "#94a3b8",
  RUNNING: "#38bdf8",
  PARTIAL: "#fb923c",
  COMPLETED: "#34d399",
  CANCELLED: "#475569",
  FAILED: "#fb7185",
};

const AXIS = { fontSize: 9, fill: "#64748b", fontFamily: "ui-monospace, monospace" };

function TooltipBox({ active, payload, label }: any) {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-[10px] text-slate-200 shadow-lg">
      <div className="text-slate-400">{label ?? payload[0]?.name}</div>
      <div className="font-semibold">{payload[0]?.value}</div>
    </div>
  );
}

function EmptyNote({ message }: { message: string }) {
  return (
    <div className="flex h-full min-h-[120px] items-center justify-center text-center font-mono text-[10px] text-slate-600">
      {message}
    </div>
  );
}

/**
 * Horizontal breakdown of task statuses. Horizontal because the status names
 * are long enough that vertical labels would be unreadable at this size.
 */
export function TaskStatusChart({ data }: { data: Record<string, number> }) {
  const rows = Object.entries(data)
    .filter(([, v]) => v > 0)
    .map(([status, count]) => ({ status, count }))
    .sort((a, b) => b.count - a.count);

  if (rows.length === 0) return <EmptyNote message="No tasks recorded yet." />;

  return (
    <ResponsiveContainer width="100%" height={Math.max(120, rows.length * 26 + 20)}>
      <BarChart data={rows} layout="vertical" margin={{ top: 4, right: 12, left: 4, bottom: 4 }}>
        <XAxis type="number" tick={AXIS} axisLine={false} tickLine={false} allowDecimals={false} />
        <YAxis type="category" dataKey="status" tick={AXIS} axisLine={false} tickLine={false} width={92} />
        <Tooltip content={<TooltipBox />} cursor={{ fill: "rgba(148,163,184,0.08)" }} />
        <Bar dataKey="count" radius={[0, 3, 3, 0]} barSize={13}>
          {rows.map((r) => (
            <Cell key={r.status} fill={TASK_STATUS_COLOR[r.status as TaskStatus] ?? "#64748b"} />
          ))}
        </Bar>
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Run states as a donut — a small, fixed set of mutually exclusive values. */
export function RunStateChart({ data }: { data: Record<string, number> }) {
  const rows = Object.entries(data)
    .filter(([, v]) => v > 0)
    .map(([state, count]) => ({ name: state, value: count }));

  if (rows.length === 0) return <EmptyNote message="No runs recorded yet." />;

  return (
    <div className="flex items-center gap-3">
      <ResponsiveContainer width="55%" height={150}>
        <PieChart>
          <Pie data={rows} dataKey="value" nameKey="name" innerRadius={34} outerRadius={58} paddingAngle={2} stroke="none">
            {rows.map((r) => (
              <Cell key={r.name} fill={RUN_STATE_COLOR[r.name as RunState] ?? "#64748b"} />
            ))}
          </Pie>
          <Tooltip content={<TooltipBox />} />
        </PieChart>
      </ResponsiveContainer>
      <ul className="flex-1 space-y-1">
        {rows.map((r) => (
          <li key={r.name} className="flex items-center gap-1.5 font-mono text-[10px]">
            <span
              className="h-2 w-2 shrink-0 rounded-sm"
              style={{ background: RUN_STATE_COLOR[r.name as RunState] ?? "#64748b" }}
            />
            <span className="flex-1 truncate text-slate-400">{r.name}</span>
            <span className="font-semibold text-slate-200">{r.value}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Single-run outcome bar built from the ten `RunSummary` counters. The
 * previous UI showed a "progress" percentage that was incremented by a
 * random timer; this is the real terminal-status composition.
 */
export function RunOutcomeBar({
  counters,
  total,
}: {
  counters: Array<{ status: TaskStatus; value: number }>;
  total: number;
}) {
  const present = counters.filter((c) => c.value > 0);
  if (total === 0 || present.length === 0) {
    return <div className="h-2 w-full rounded-full bg-slate-800" title="No tasks" />;
  }

  return (
    <div className="flex h-2 w-full overflow-hidden rounded-full bg-slate-800" role="img" aria-label="Task outcome composition">
      {present.map((c) => (
        <div
          key={c.status}
          title={`${c.status}: ${c.value}`}
          style={{ width: `${(c.value / total) * 100}%`, background: TASK_STATUS_COLOR[c.status] }}
        />
      ))}
    </div>
  );
}
