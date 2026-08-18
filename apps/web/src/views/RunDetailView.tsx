/**
 * Run detail — tasks, dependency graph, real timing, live events, evidence.
 *
 * The only genuinely real-time screen, driven by the orchestrator's own
 * WebSocket (`WS /runs/:id/ws?key=`), not a timer. Reconnect follows
 * SRD §18: on close, re-fetch `GET /runs/:id/events?since=<last sequence>`
 * and merge by sequence, so nothing is missed or double-counted.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, GitBranch, ListTree, Paperclip, Radio, RefreshCw, Timer, RotateCcw } from "lucide-react";
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
  { id: "timeline", label: "Timeline Waterfall", icon: <Timer size={12} /> },
  { id: "graph", label: "Dependencies DAG", icon: <GitBranch size={12} /> },
  { id: "events", label: "Live Event Stream", icon: <Radio size={12} /> },
  { id: "evidence", label: "Artifacts & Evidence", icon: <Paperclip size={12} /> },
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
      {/* Header */}
      <header className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-slate-800">
        <div className="flex items-center gap-3">
          <button
            onClick={onBack}
            className="p-1.5 rounded-md border border-slate-700 bg-slate-900/60 hover:bg-slate-800 text-slate-400 hover:text-slate-200 transition-colors cursor-pointer"
            title="Back to All Runs"
          >
            <ArrowLeft size={14} />
          </button>
          <div>
            <div className="flex items-center gap-2 flex-wrap">
              <h1 className="text-lg font-bold tracking-tight text-slate-100 font-sans">
                {run.data?.name ?? "Execution Run"}
              </h1>
              {run.data && <RunStateBadge state={run.data.state} />}
            </div>
            <div className="font-mono text-[10px] text-slate-500">{runId}</div>
          </div>
        </div>
        <div className="flex items-center gap-3">
          <SocketIndicator state={socketState} />
          <button
            onClick={() => {
              run.reload();
              evidence.reload();
            }}
            className="p-1.5 rounded border border-slate-800 text-slate-400 hover:text-slate-200 transition-colors cursor-pointer"
            title="Reload Run State"
          >
            <RotateCcw size={12} />
          </button>
        </div>
      </header>

      <ResourceView state={run} loadingLabel="Loading execution details…">
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
              {/* Summary Section */}
              <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] p-4 space-y-3.5 shadow-lg">
                <div className="flex flex-wrap items-center gap-x-6 gap-y-2 border-b border-slate-800/80 pb-3">
                  <Field label="Duration">{formatSpan(detail.started_at, detail.finished_at)}</Field>
                  <Field label="Started At">
                    <Timestamp value={detail.started_at} />
                  </Field>
                  <Field label="Finished At">
                    {detail.finished_at ? <Timestamp value={detail.finished_at} /> : "still active running"}
                  </Field>
                  <Field label="Requested By">{detail.requested_by ?? "system / operator"}</Field>
                  <Field label="Project Scope">
                    <span className="font-mono text-[10px] text-cyan-300">
                      {detail.project_id ?? "global"}
                    </span>
                  </Field>
                </div>

                {detail.policy_notes && (
                  <div className="rounded-lg border border-amber-800/60 bg-amber-950/30 px-3 py-2 text-xs text-amber-300">
                    <span className="font-semibold">Policy gate assessment: </span>
                    {detail.policy_notes}
                    {detail.approved_by && (
                      <span className="ml-2 text-amber-400/80">
                        approved by {detail.approved_by} <Timestamp value={detail.approved_at} />
                      </span>
                    )}
                  </div>
                )}

                <div>
                  <div className="mb-1.5 flex items-center justify-between font-mono text-[9px] uppercase tracking-wider text-slate-500">
                    <span>Task Outcome Composition</span>
                    <span>{detail.total} Total Tasks</span>
                  </div>
                  <RunOutcomeBar
                    counters={counters.map((c) => ({ status: c.status, value: c.value }))}
                    total={detail.total}
                  />
                </div>

                <StatusCounters counters={counters} />
              </section>

              {/* Tabs */}
              <div className="flex gap-1 border-b border-slate-800 bg-[#060a12] px-2 rounded-t-lg">
                {TABS.map((t) => {
                  const count =
                    t.id === "events" ? events.length : t.id === "evidence" ? (evidence.data?.length ?? null) : null;
                  return (
                    <button
                      key={t.id}
                      onClick={() => setTab(t.id)}
                      className={`flex items-center gap-1.5 border-b-2 px-3 py-2.5 text-xs font-medium transition-colors cursor-pointer ${
                        tab === t.id
                          ? "border-cyan-400 text-cyan-300 font-semibold"
                          : "border-transparent text-slate-500 hover:text-slate-300"
                      }`}
                    >
                      {t.icon}
                      {t.label}
                      {count !== null && (
                        <span className="font-mono text-[10px] px-1.5 py-0.2 rounded bg-slate-900 border border-slate-800 text-slate-400">
                          {count}
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>

              {tab === "tasks" && <TasksTable detail={detail} evidenceByTask={evidenceByTask} />}

              {tab === "timeline" && (
                <Card icon={<Timer size={14} />} title="Task Execution Timeline Waterfall" bodyClassName="p-4">
                  <TaskWaterfall
                    tasks={detail.tasks}
                    runStartedAt={detail.started_at}
                    runFinishedAt={detail.finished_at}
                  />
                </Card>
              )}

              {tab === "graph" && (
                <Card icon={<GitBranch size={14} />} title="Task Dependency DAG Topology" bodyClassName="p-4">
                  <DependencyGraph tasks={detail.tasks} />
                </Card>
              )}

              {tab === "events" && <EventsPanel events={events} socketState={socketState} />}

              {tab === "evidence" && (
                <Card icon={<Paperclip size={14} />} title="Captured Artifacts & Trace Evidence">
                  <ResourceView
                    state={evidence}
                    loadingLabel="Loading evidence artifacts…"
                    emptyTitle="No evidence captured"
                    emptyMessage="No artifacts were recorded. Real executors (Playwright, Newman, Schemathesis, Maestro) attach logs and trace files here."
                  >
                    {(rows) => (
                      <div className="space-y-3">
                        {[...evidenceByTask.entries()].map(([taskId, items]) => (
                          <div key={taskId} className="border border-slate-800/80 rounded-lg p-3 bg-slate-950/40">
                            <div className="mb-2 font-mono text-[10px] uppercase tracking-wider text-cyan-400 font-semibold flex items-center gap-1.5">
                              <Paperclip size={11} />
                              <span>Task ID: {taskId}</span>
                            </div>
                            <ul className="space-y-1">
                              {items.map((row) => (
                                <li key={row.id}>
                                  <button
                                    onClick={() => onOpenEvidence(row)}
                                    className="flex w-full items-center justify-between rounded px-2.5 py-2 text-left transition hover:bg-slate-900/80 border border-slate-800/60 bg-slate-900/40 cursor-pointer"
                                  >
                                    <div className="min-w-0">
                                      <div className="truncate text-xs text-slate-200 font-mono font-medium">
                                        {row.object_key.split("/").pop()}
                                      </div>
                                      <div className="font-mono text-[10px] text-slate-500 mt-0.5">
                                        {row.kind} · {row.content_type ?? "text/plain"} · {formatBytes(row.size_bytes)} ·{" "}
                                        {relativeTime(row.created_at)}
                                      </div>
                                    </div>
                                    <span className="ml-2 shrink-0 text-xs text-cyan-400 font-mono font-semibold">
                                      View Content →
                                    </span>
                                  </button>
                                </li>
                              ))}
                            </ul>
                          </div>
                        ))}
                        <p className="pt-1 font-mono text-[9px] text-slate-500">
                          {rows.length} artifact(s), {formatBytes(rows.reduce((a, r) => a + r.size_bytes, 0))} total stored.
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
        <p className="py-8 text-center text-xs text-slate-500 font-mono">This execution run has no tasks defined.</p>
      </Card>
    );
  }

  return (
    <Card icon={<ListTree size={14} />} title={`Dispatched Tasks (${detail.tasks.length})`}>
      <div className="overflow-x-auto">
        <table className="w-full min-w-[760px] text-left text-xs">
          <thead>
            <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800/80">
              <th className="pb-2 font-normal">Task Name & ID</th>
              <th className="pb-2 font-normal">Status</th>
              <th className="pb-2 text-right font-normal">Duration</th>
              <th className="pb-2 font-normal">Prerequisites</th>
              <th className="pb-2 font-normal">Reason / Detail</th>
              <th className="pb-2 text-right font-normal">Evidence</th>
            </tr>
          </thead>
          <tbody>
            {detail.tasks.map((task) => {
              const artifacts = evidenceByTask.get(task.id)?.length ?? 0;
              return (
                <tr key={task.id} className="border-t border-slate-800/60 align-top hover:bg-slate-900/30 transition-colors">
                  <td className="py-2.5 pr-3">
                    <div
                      className="border-l-2 pl-2 font-medium text-slate-200"
                      style={{ borderColor: TASK_STATUS_COLOR[task.status] }}
                    >
                      {task.name}
                    </div>
                    <div className="pl-2 font-mono text-[10px] text-slate-500">{task.id}</div>
                  </td>
                  <td className="py-2.5 pr-3">
                    <TaskStatusBadge status={task.status} />
                  </td>
                  <td className="py-2.5 pr-3 text-right font-mono text-[11px] text-slate-400">
                    {formatSpan(task.started_at, task.finished_at)}
                  </td>
                  <td className="py-2.5 pr-3 font-mono text-[10px] text-slate-400">
                    {task.prerequisites.length ? task.prerequisites.join(", ") : "—"}
                  </td>
                  <td className="max-w-xs py-2.5 pr-3 text-[11px] text-slate-400">{task.reason ?? "—"}</td>
                  <td className="py-2.5 text-right font-mono text-[10px] text-slate-400 font-semibold">{artifacts || "—"}</td>
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
    <Card icon={<Radio size={14} />} title={`Realtime Event Stream (${events.length} frames)`}>
      <div className="max-h-[460px] overflow-y-auto">
        {events.length === 0 ? (
          <p className="py-8 text-center text-xs text-slate-500 font-mono">
            {socketState === "unauthenticated"
              ? "No API key configured — cannot open the event WebSocket."
              : "No events streamed yet."}
          </p>
        ) : (
          <ul className="space-y-1">
            {events.map((ev) => (
              <li
                key={ev.event_id}
                className="flex items-start gap-2 rounded px-2 py-1.5 font-mono text-[10px] leading-relaxed hover:bg-slate-900/60 border border-transparent hover:border-slate-800"
              >
                <span className="w-8 shrink-0 text-right text-slate-500 font-bold">#{ev.sequence}</span>
                <span className="w-16 shrink-0 text-slate-500">{new Date(ev.timestamp).toLocaleTimeString()}</span>
                <span
                  className="w-36 shrink-0 font-semibold"
                  style={{ color: ev.status ? TASK_STATUS_COLOR[ev.status] : "#22d3ee" }}
                >
                  {ev.type}
                </span>
                <span className="w-16 shrink-0 text-slate-400 truncate">{ev.task_id ?? "—"}</span>
                <span className="min-w-0 flex-1 text-slate-400">{ev.summary ?? ""}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <p className="mt-2 border-t border-slate-800 pt-2 font-mono text-[9px] text-slate-500">
        Connected to WS /runs/:id/ws. Frames merged by sequence number to prevent double-counting on reconnection (SRD §5).
      </p>
    </Card>
  );
}

function SocketIndicator({ state }: { state: SocketState }) {
  const copy: Record<SocketState, { text: string; dot: string; fg: string }> = {
    connecting: { text: "connecting", dot: "bg-slate-500", fg: "text-slate-500" },
    open: { text: "live stream", dot: "bg-emerald-500", fg: "text-emerald-400" },
    closed: { text: "disconnected (reconnecting)", dot: "bg-amber-500", fg: "text-amber-400" },
    unauthenticated: { text: "no api key", dot: "bg-rose-500", fg: "text-rose-400" },
  };
  const c = copy[state];
  return (
    <div className="flex shrink-0 items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider px-2.5 py-1 rounded bg-slate-900 border border-slate-800">
      <span className={`h-1.5 w-1.5 rounded-full ${c.dot}`} />
      <span className={c.fg}>{c.text}</span>
      {state === "closed" && <RefreshCw size={9} className="animate-spin text-slate-600 ml-1" />}
    </div>
  );
}
