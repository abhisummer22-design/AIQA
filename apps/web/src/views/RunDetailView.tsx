/**
 * Run detail — tasks, dependency graph, real timing, live events, evidence.
 *
 * The only genuinely real-time screen, driven by the orchestrator's own
 * WebSocket (`WS /runs/:id/ws?key=`), not a timer. Reconnect follows
 * SRD §18: on close, re-fetch `GET /runs/:id/events?since=<last sequence>`
 * and merge by sequence, so nothing is missed or double-counted.
 *
 * Every panel here reads a field the backend already returns:
 *   Tasks        status, reason, prerequisites, started_at, finished_at
 *   Timeline     durations computed from those timestamps
 *   Dependencies the real DAG from prerequisites
 *   Events       the durable event log, replayed then streamed
 *   Evidence     artifacts captured by the executor, grouped by task
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, GitBranch, ListTree, Paperclip, Radio, RefreshCw, Timer } from "lucide-react";
import { useApiResource } from "../hooks/useApi";
import { getRun, getRunEvents, listRunEvidence, openRunEventSocket } from "../services/orchestrator";
import type { EvidenceRow, RunDetail, RunEvent, TaskStatus } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { RunStateBadge, TaskStatusBadge, Timestamp } from "../components/common/Badges";
import { Card, Field } from "../components/common/Card";
import { DependencyGraph, StatusCounters, TaskWaterfall } from "../components/common/RunVisuals";
import { RunOutcomeBar, TASK_STATUS_COLOR } from "../components/common/Charts";
import { EvidenceViewer } from "../components/common/EvidenceViewer";
import { formatBytes, formatSpan, relativeTime } from "../lib/format";

type SocketState = "connecting" | "open" | "closed" | "unauthenticated";
type Tab = "tasks" | "timeline" | "graph" | "events" | "evidence";

const TABS: Array<{ id: Tab; label: string; icon: React.ReactNode }> = [
  { id: "tasks", label: "Tasks", icon: <ListTree size={12} /> },
  { id: "timeline", label: "Timeline", icon: <Timer size={12} /> },
  { id: "graph", label: "Dependencies", icon: <GitBranch size={12} /> },
  { id: "events", label: "Events", icon: <Radio size={12} /> },
  { id: "evidence", label: "Evidence", icon: <Paperclip size={12} /> },
];

export function RunDetailView({ runId, onBack }: { runId: string; onBack: () => void }) {
  const run = useApiResource<RunDetail>((signal) => getRun(runId, signal), [runId], { isEmpty: () => false });
  const evidence = useApiResource<EvidenceRow[]>((signal) => listRunEvidence(runId, signal), [runId]);

  const [selectedEvidence, setSelectedEvidence] = useState<EvidenceRow | null>(null);
  const onOpenEvidence = setSelectedEvidence;
  const [tab, setTab] = useState<Tab>("tasks");
  const [events, setEvents] = useState<RunEvent[]>([]);
  const [socketState, setSocketState] = useState<SocketState>("connecting");

  const lastSequenceRef = useRef(0);

  const mergeEvents = useCallback((incoming: RunEvent[]) => {
    setEvents((prev) => {
      const bySequence = new Map<number, RunEvent>(prev.map((e) => [e.sequence, e] as const));
      for (const ev of incoming) bySequence.set(ev.sequence, ev);
      const merged = [...bySequence.values()].sort((a, b) => a.sequence - b.sequence);
      lastSequenceRef.current = merged.length ? merged[merged.length - 1].sequence : 0;
      return merged;
    });
  }, []);

  useEffect(() => {
    let disposed = false;
    let socket: WebSocket | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const catchUp = async () => {
      try {
        const missed = await getRunEvents(runId, lastSequenceRef.current + 1);
        if (!disposed && missed.length) mergeEvents(missed);
      } catch {
        // Reported by the socket indicator; no synthesized events fill the gap.
      }
    };

    const connect = () => {
      socket = openRunEventSocket(runId, {
        onOpen: () => !disposed && setSocketState("open"),
        onEvent: (ev) => !disposed && mergeEvents([ev]),
        onError: () => !disposed && setSocketState("closed"),
        onClose: () => {
          if (disposed) return;
          setSocketState("closed");
          void catchUp();
          retryTimer = setTimeout(connect, 5000);
        },
      });
      if (!socket) setSocketState("unauthenticated");
    };

    connect();
    return () => {
      disposed = true;
      if (retryTimer) clearTimeout(retryTimer);
      socket?.close();
    };
  }, [runId, mergeEvents]);

  // A terminal event means the summary and evidence list are stale.
  useEffect(() => {
    if (events.some((e) => e.type === "RUN_COMPLETED")) {
      run.reload();
      evidence.reload();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [events.length]);

  const evidenceByTask = useMemo(() => {
    const map = new Map<string, EvidenceRow[]>();
    for (const row of evidence.data ?? []) {
      const key = row.task_id ?? "(run-level)";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(row);
    }
    return map;
  }, [evidence.data]);

  return (
    <div className="space-y-4">
      <header className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <button
            onClick={onBack}
            className="mt-0.5 shrink-0 rounded border border-slate-700 p-1 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
          >
            <ArrowLeft size={14} />
          </button>
          <div className="min-w-0">
            <h1 className="truncate text-lg font-bold tracking-tight text-slate-100">{run.data?.name ?? "Run"}</h1>
            <div className="font-mono text-[10px] text-slate-600">{runId}</div>
          </div>
        </div>
        <SocketIndicator state={socketState} />
      </header>

      <ResourceView state={run} loadingLabel="Loading run…">
        {(detail) => {
          const counters: Array<{ label: string; status: TaskStatus; value: number }> = [
            { label: "Passed", status: "PASSED", value: detail.passed },
            { label: "Failed", status: "FAILED", value: detail.failed },
            { label: "Blocked", status: "BLOCKED", value: detail.blocked },
            { label: "Infra err", status: "INFRA_ERROR", value: detail.infra_error },
            { label: "Review", status: "NEEDS_REVIEW", value: detail.needs_review },
            { label: "Running", status: "RUNNING", value: detail.running },
            { label: "Queued", status: "QUEUED", value: detail.queued },
            { label: "Skipped", status: "SKIPPED", value: detail.skipped },
            { label: "Timeout", status: "TIMEOUT", value: detail.timeout },
            { label: "Cancelled", status: "CANCELLED", value: detail.cancelled },
          ];

          return (
            <>
              {/* Summary */}
              <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] p-4">
                <div className="mb-3 flex flex-wrap items-center gap-x-4 gap-y-2">
                  <RunStateBadge state={detail.state} />
                  <Field label="Duration">{formatSpan(detail.started_at, detail.finished_at)}</Field>
                  <Field label="Started">
                    <Timestamp value={detail.started_at} />
                  </Field>
                  <Field label="Finished">
                    {detail.finished_at ? <Timestamp value={detail.finished_at} /> : "still running"}
                  </Field>
                  <Field label="Requested by">{detail.requested_by ?? "—"}</Field>
                  <Field label="Project">{detail.project_id ?? "global"}</Field>
                </div>

                {detail.policy_notes && (
                  <div className="mb-3 rounded border border-amber-800/60 bg-amber-950/30 px-3 py-2 text-[11px] text-amber-300">
                    <span className="font-semibold">Policy gate: </span>
                    {detail.policy_notes}
                    {detail.approved_by && (
                      <span className="ml-2 text-amber-400/80">
                        approved by {detail.approved_by} <Timestamp value={detail.approved_at} />
                      </span>
                    )}
                  </div>
                )}

                <div className="mb-3">
                  <div className="mb-1 flex items-center justify-between font-mono text-[9px] uppercase tracking-wider text-slate-600">
                    <span>Outcome composition</span>
                    <span>{detail.total} tasks</span>
                  </div>
                  <RunOutcomeBar
                    counters={counters.map((c) => ({ status: c.status, value: c.value }))}
                    total={detail.total}
                  />
                </div>

                <StatusCounters counters={counters} />
              </section>

              {/* Tabs */}
              <div className="flex gap-1 border-b border-slate-800">
                {TABS.map((t) => {
                  const count =
                    t.id === "events" ? events.length : t.id === "evidence" ? (evidence.data?.length ?? null) : null;
                  return (
                    <button
                      key={t.id}
                      onClick={() => setTab(t.id)}
                      className={`flex items-center gap-1.5 border-b-2 px-3 py-2 text-xs transition ${
                        tab === t.id
                          ? "border-cyan-500 text-cyan-300"
                          : "border-transparent text-slate-500 hover:text-slate-300"
                      }`}
                    >
                      {t.icon}
                      {t.label}
                      {count !== null && <span className="font-mono text-[10px] text-slate-600">{count}</span>}
                    </button>
                  );
                })}
              </div>

              {tab === "tasks" && <TasksTable detail={detail} evidenceByTask={evidenceByTask} />}

              {tab === "timeline" && (
                <Card icon={<Timer size={14} />} title="Task timing" bodyClassName="p-4">
                  <TaskWaterfall
                    tasks={detail.tasks}
                    runStartedAt={detail.started_at}
                    runFinishedAt={detail.finished_at}
                  />
                </Card>
              )}

              {tab === "graph" && (
                <Card icon={<GitBranch size={14} />} title="Dependency graph" bodyClassName="p-4">
                  <DependencyGraph tasks={detail.tasks} />
                </Card>
              )}

              {tab === "events" && <EventsPanel events={events} socketState={socketState} />}

              {tab === "evidence" && (
                <Card icon={<Paperclip size={14} />} title="Evidence">
                  <ResourceView
                    state={evidence}
                    loadingLabel="Loading evidence…"
                    emptyTitle="No evidence"
                    emptyMessage="No artifacts were captured. The SimulatedExecutor produces none; real executors write logs."
                  >
                    {(rows) => (
                      <div className="space-y-3">
                        {[...evidenceByTask.entries()].map(([taskId, items]) => (
                          <div key={taskId}>
                            <div className="mb-1 font-mono text-[9px] uppercase tracking-wider text-slate-600">
                              {taskId}
                            </div>
                            <ul className="space-y-1">
                              {items.map((row) => (
                                <li key={row.id}>
                                  <button
                                    onClick={() => onOpenEvidence(row)}
                                    className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left transition hover:bg-slate-800/50"
                                  >
                                    <div className="min-w-0">
                                      <div className="truncate text-[11px] text-slate-300">
                                        {row.object_key.split("/").pop()}
                                      </div>
                                      <div className="font-mono text-[9px] text-slate-600">
                                        {row.kind} · {row.content_type ?? "text/plain"} · {formatBytes(row.size_bytes)} ·{" "}
                                        {relativeTime(row.created_at)}
                                      </div>
                                    </div>
                                    <span className="ml-2 shrink-0 text-[10px] text-cyan-400">View</span>
                                  </button>
                                </li>
                              ))}
                            </ul>
                          </div>
                        ))}
                        <p className="pt-1 font-mono text-[9px] text-slate-600">
                          {rows.length} artifact(s), {formatBytes(rows.reduce((a, r) => a + r.size_bytes, 0))} total.
                        </p>
                      </div>
                    )}
                  </ResourceView>
                </Card>
              )}
            </>
          );
        }}
      </ResourceView>

      {selectedEvidence && (
        <EvidenceViewer evidence={selectedEvidence} onClose={() => setSelectedEvidence(null)} />
      )}
    </div>
  );
}

function TasksTable({
  detail,
  evidenceByTask,
}: {
  detail: RunDetail;
  evidenceByTask: Map<string, EvidenceRow[]>;
}) {
  if (detail.tasks.length === 0) {
    return (
      <Card title="Tasks">
        <p className="py-6 text-center text-xs text-slate-600">This run has no tasks.</p>
      </Card>
    );
  }

  return (
    <Card icon={<ListTree size={14} />} title={`Tasks (${detail.tasks.length})`}>
      <div className="overflow-x-auto">
        <table className="w-full min-w-[760px] text-left text-xs">
          <thead>
            <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-600">
              <th className="pb-2 font-normal">Task</th>
              <th className="pb-2 font-normal">Status</th>
              <th className="pb-2 text-right font-normal">Duration</th>
              <th className="pb-2 font-normal">Prerequisites</th>
              <th className="pb-2 font-normal">Reason</th>
              <th className="pb-2 text-right font-normal">Evidence</th>
            </tr>
          </thead>
          <tbody>
            {detail.tasks.map((task) => {
              const artifacts = evidenceByTask.get(task.id)?.length ?? 0;
              return (
                <tr key={task.id} className="border-t border-slate-800/70 align-top">
                  <td className="py-2 pr-3">
                    <div
                      className="border-l-2 pl-2 font-medium text-slate-200"
                      style={{ borderColor: TASK_STATUS_COLOR[task.status] }}
                    >
                      {task.name}
                    </div>
                    <div className="pl-2 font-mono text-[10px] text-slate-600">{task.id}</div>
                  </td>
                  <td className="py-2 pr-3">
                    <TaskStatusBadge status={task.status} />
                  </td>
                  <td className="py-2 pr-3 text-right font-mono text-[11px] text-slate-400">
                    {formatSpan(task.started_at, task.finished_at)}
                  </td>
                  <td className="py-2 pr-3 font-mono text-[10px] text-slate-500">
                    {task.prerequisites.length ? task.prerequisites.join(", ") : "—"}
                  </td>
                  <td className="max-w-xs py-2 pr-3 text-[11px] text-slate-400">{task.reason ?? "—"}</td>
                  <td className="py-2 text-right font-mono text-[10px] text-slate-500">{artifacts || "—"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

function EventsPanel({ events, socketState }: { events: RunEvent[]; socketState: SocketState }) {
  return (
    <Card icon={<Radio size={14} />} title={`Event stream (${events.length})`}>
      <div className="max-h-[460px] overflow-y-auto">
        {events.length === 0 ? (
          <p className="py-8 text-center text-xs text-slate-600">
            {socketState === "unauthenticated"
              ? "No API key — cannot open the event socket."
              : "No events received yet."}
          </p>
        ) : (
          <ul className="space-y-0.5">
            {events.map((ev) => (
              <li
                key={ev.event_id}
                className="flex items-start gap-2 rounded px-1.5 py-1 font-mono text-[10px] leading-relaxed hover:bg-slate-800/40"
              >
                <span className="w-8 shrink-0 text-right text-slate-600">#{ev.sequence}</span>
                <span className="w-16 shrink-0 text-slate-500">{new Date(ev.timestamp).toLocaleTimeString()}</span>
                <span
                  className="w-36 shrink-0"
                  style={{ color: ev.status ? TASK_STATUS_COLOR[ev.status] : "#22d3ee" }}
                >
                  {ev.type}
                </span>
                <span className="w-12 shrink-0 text-slate-400">{ev.task_id ?? "—"}</span>
                <span className="min-w-0 flex-1 text-slate-500">{ev.summary ?? ""}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <p className="mt-2 border-t border-slate-800 pt-2 font-mono text-[9px] text-slate-600">
        Delivered over WS /runs/:id/ws. Merged by sequence number, so a replayed backlog cannot double-count (SRD §5).
      </p>
    </Card>
  );
}

function SocketIndicator({ state }: { state: SocketState }) {
  const copy: Record<SocketState, { text: string; dot: string; fg: string }> = {
    connecting: { text: "connecting", dot: "bg-slate-500", fg: "text-slate-500" },
    open: { text: "live", dot: "bg-emerald-500", fg: "text-emerald-400" },
    closed: { text: "disconnected", dot: "bg-amber-500", fg: "text-amber-400" },
    unauthenticated: { text: "no api key", dot: "bg-rose-500", fg: "text-rose-400" },
  };
  const c = copy[state];
  return (
    <div className="flex shrink-0 items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider">
      <span className={`h-1.5 w-1.5 rounded-full ${c.dot}`} />
      <span className={c.fg}>{c.text}</span>
      {state === "closed" && <RefreshCw size={9} className="animate-spin text-slate-600" />}
    </div>
  );
}
