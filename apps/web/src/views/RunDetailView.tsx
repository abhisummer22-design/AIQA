/**
 * Run detail — tasks, live events, evidence.
 *
 * This is the only genuinely real-time screen in the app, and it is driven
 * by the orchestrator's actual WebSocket (`WS /runs/:id/ws?key=`), not a
 * timer. The previous UI advanced progress bars with
 * `Math.random() > 0.4` inside a `setInterval`; nothing here moves unless
 * the backend emitted an event.
 *
 * Reconnect follows SRD §18: on socket close the client re-fetches
 * `GET /runs/:id/events?since=<last sequence>` and merges by sequence
 * number, so no event is missed or double-counted.
 */

import React, { useCallback, useEffect, useRef, useState } from "react";
import { ArrowLeft, Paperclip, Radio, RefreshCw } from "lucide-react";
import { useApiResource } from "../hooks/useApi";
import {
  getRun,
  getRunEvents,
  listRunEvidence,
  openRunEventSocket,
} from "../services/orchestrator";
import type { EvidenceRow, RunDetail, RunEvent } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { RunStateBadge, TaskStatusBadge, Timestamp } from "../components/common/Badges";
import { EvidenceViewer } from "../components/common/EvidenceViewer";

type SocketState = "connecting" | "open" | "closed" | "unauthenticated";

export function RunDetailView({ runId, onBack }: { runId: string; onBack: () => void }) {
  const run = useApiResource<RunDetail>((signal) => getRun(runId, signal), [runId], { isEmpty: () => false });
  const evidence = useApiResource<EvidenceRow[]>((signal) => listRunEvidence(runId, signal), [runId]);

  const [events, setEvents] = useState<RunEvent[]>([]);
  const [socketState, setSocketState] = useState<SocketState>("connecting");
  const [selectedEvidence, setSelectedEvidence] = useState<EvidenceRow | null>(null);

  /** Highest sequence number seen — the resume point for reconnects. */
  const lastSequenceRef = useRef(0);

  /** Merge by sequence: idempotent, so a replayed backlog cannot double-count
   *  (SRD §5 requires consumers to be idempotent). */
  const mergeEvents = useCallback((incoming: RunEvent[]) => {
    setEvents((prev) => {
      const bySequence = new Map<number, RunEvent>(prev.map((e) => [e.sequence, e] as const));
      for (const ev of incoming) bySequence.set(ev.sequence, ev);
      const merged = [...bySequence.values()].sort((a, b) => a.sequence - b.sequence);
      lastSequenceRef.current = merged.length ? merged[merged.length - 1].sequence : 0;
      return merged;
    });
  }, []);

  // Live socket, with a catch-up fetch on every close.
  useEffect(() => {
    let disposed = false;
    let socket: WebSocket | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const catchUp = async () => {
      try {
        const missed = await getRunEvents(runId, lastSequenceRef.current + 1);
        if (!disposed && missed.length) mergeEvents(missed);
      } catch {
        // Catch-up failing is itself reported by the socket state; no
        // synthesized events are inserted to fill the gap.
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
          // The run may simply have finished; retry slowly rather than hammering.
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

  // A terminal event means the summary/task table is stale — refetch once.
  useEffect(() => {
    if (events.some((e) => e.type === "RUN_COMPLETED")) {
      run.reload();
      evidence.reload();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [events.length]);

  return (
    <div className="space-y-4">
      <header className="flex items-start justify-between">
        <div className="flex items-start gap-3">
          <button
            onClick={onBack}
            className="mt-0.5 rounded border border-slate-700 p-1 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
          >
            <ArrowLeft size={14} />
          </button>
          <div>
            <h1 className="text-lg font-bold tracking-tight text-slate-100">
              {run.data?.name ?? "Run"}
            </h1>
            <div className="mt-0.5 font-mono text-[10px] text-slate-600">{runId}</div>
          </div>
        </div>
        <SocketIndicator state={socketState} />
      </header>

      {/* Summary */}
      <ResourceView state={run} loadingLabel="Loading run…">
        {(detail) => (
          <>
            <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] p-4">
              <div className="mb-3 flex flex-wrap items-center gap-3">
                <RunStateBadge state={detail.state} />
                <span className="font-mono text-[10px] text-slate-500">
                  started <Timestamp value={detail.started_at} />
                </span>
                {detail.finished_at && (
                  <span className="font-mono text-[10px] text-slate-500">
                    finished <Timestamp value={detail.finished_at} />
                  </span>
                )}
                {detail.requested_by && (
                  <span className="font-mono text-[10px] text-slate-500">by {detail.requested_by}</span>
                )}
              </div>

              {detail.policy_notes && (
                <div className="mb-3 rounded border border-amber-800/60 bg-amber-950/30 px-3 py-2 text-[11px] text-amber-300">
                  <span className="font-semibold">Policy: </span>
                  {detail.policy_notes}
                  {detail.approved_by && (
                    <span className="ml-2 text-amber-400/80">
                      approved by {detail.approved_by} <Timestamp value={detail.approved_at} />
                    </span>
                  )}
                </div>
              )}

              <div className="grid grid-cols-3 gap-2 sm:grid-cols-6">
                <Counter label="Total" value={detail.total} />
                <Counter label="Passed" value={detail.passed} tone="pass" />
                <Counter label="Failed" value={detail.failed} tone="fail" />
                <Counter label="Blocked" value={detail.blocked} tone="warn" />
                <Counter label="Infra error" value={detail.infra_error} tone="infra" />
                <Counter label="Needs review" value={detail.needs_review} tone="review" />
              </div>
            </section>

            {/* Tasks */}
            <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
              <h2 className="border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
                Tasks ({detail.tasks.length})
              </h2>
              <div className="p-3">
                {detail.tasks.length === 0 ? (
                  <p className="px-1 py-4 text-center text-xs text-slate-600">This run has no tasks.</p>
                ) : (
                  <table className="w-full text-left text-xs">
                    <thead>
                      <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-600">
                        <th className="pb-2 font-normal">Task</th>
                        <th className="pb-2 font-normal">Status</th>
                        <th className="pb-2 font-normal">Prerequisites</th>
                        <th className="pb-2 font-normal">Reason</th>
                        <th className="pb-2 font-normal">Finished</th>
                      </tr>
                    </thead>
                    <tbody>
                      {detail.tasks.map((task) => (
                        <tr key={task.id} className="border-t border-slate-800/70 align-top">
                          <td className="py-2 pr-3">
                            <div className="font-medium text-slate-200">{task.name}</div>
                            <div className="font-mono text-[10px] text-slate-600">{task.id}</div>
                          </td>
                          <td className="py-2 pr-3">
                            <TaskStatusBadge status={task.status} />
                          </td>
                          <td className="py-2 pr-3 font-mono text-[10px] text-slate-500">
                            {task.prerequisites.length ? task.prerequisites.join(", ") : "—"}
                          </td>
                          <td className="max-w-xs py-2 pr-3 text-[11px] text-slate-400">{task.reason ?? "—"}</td>
                          <td className="py-2">
                            <Timestamp value={task.finished_at} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            </section>
          </>
        )}
      </ResourceView>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Live events */}
        <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
          <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
            <Radio size={13} className="text-cyan-400" />
            Event stream ({events.length})
          </h2>
          <div className="max-h-80 overflow-y-auto p-3">
            {events.length === 0 ? (
              <p className="py-6 text-center text-xs text-slate-600">
                {socketState === "unauthenticated"
                  ? "No API key — cannot open the event socket."
                  : "No events received yet."}
              </p>
            ) : (
              <ul className="space-y-1">
                {events.map((ev) => (
                  <li key={ev.event_id} className="flex items-start gap-2 font-mono text-[10px] leading-relaxed">
                    <span className="w-8 shrink-0 text-right text-slate-600">#{ev.sequence}</span>
                    <span className="shrink-0 text-slate-500">
                      {new Date(ev.timestamp).toLocaleTimeString()}
                    </span>
                    <span className="shrink-0 text-cyan-400">{ev.type}</span>
                    {ev.task_id && <span className="shrink-0 text-slate-400">{ev.task_id}</span>}
                    {ev.summary && <span className="min-w-0 flex-1 text-slate-500">{ev.summary}</span>}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </section>

        {/* Evidence */}
        <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
          <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
            <Paperclip size={13} className="text-cyan-400" />
            Evidence
          </h2>
          <div className="max-h-80 overflow-y-auto p-3">
            <ResourceView
              state={evidence}
              compact
              loadingLabel="Loading evidence…"
              emptyTitle="No evidence"
              emptyMessage="No artifacts were captured for this run."
            >
              {(rows) => (
                <ul className="space-y-1">
                  {rows.map((row) => (
                    <li key={row.id}>
                      <button
                        onClick={() => setSelectedEvidence(row)}
                        className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left transition hover:bg-slate-800/50"
                      >
                        <div className="min-w-0">
                          <div className="truncate text-[11px] text-slate-300">{row.object_key}</div>
                          <div className="font-mono text-[10px] text-slate-600">
                            {row.kind}
                            {row.task_id ? ` · ${row.task_id}` : ""} · {row.size_bytes} B
                          </div>
                        </div>
                        <span className="ml-2 shrink-0 text-[10px] text-cyan-400">View</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </ResourceView>
          </div>
        </section>
      </div>

      {selectedEvidence && (
        <EvidenceViewer evidence={selectedEvidence} onClose={() => setSelectedEvidence(null)} />
      )}
    </div>
  );
}

function SocketIndicator({ state }: { state: SocketState }) {
  const copy: Record<SocketState, { text: string; cls: string }> = {
    connecting: { text: "connecting", cls: "text-slate-500 bg-slate-700" },
    open: { text: "live", cls: "text-emerald-400 bg-emerald-500" },
    closed: { text: "disconnected", cls: "text-amber-400 bg-amber-500" },
    unauthenticated: { text: "no api key", cls: "text-rose-400 bg-rose-500" },
  };
  const c = copy[state];
  return (
    <div className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider">
      <span className={`h-1.5 w-1.5 rounded-full ${c.cls.split(" ")[1]}`} />
      <span className={c.cls.split(" ")[0]}>{c.text}</span>
      {state === "closed" && <RefreshCw size={9} className="animate-spin text-slate-600" />}
    </div>
  );
}

function Counter({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: number;
  tone?: "neutral" | "pass" | "fail" | "warn" | "infra" | "review";
}) {
  const toneClass = {
    neutral: "text-slate-200",
    pass: "text-emerald-400",
    fail: "text-rose-400",
    warn: "text-amber-400",
    infra: "text-fuchsia-400",
    review: "text-violet-400",
  }[tone];

  return (
    <div className="rounded-lg border border-slate-800 bg-slate-900/40 px-2.5 py-2">
      <div className="font-mono text-[9px] uppercase tracking-wider text-slate-500">{label}</div>
      <div className={`mt-0.5 font-mono text-lg font-bold ${toneClass}`}>{value}</div>
    </div>
  );
}
