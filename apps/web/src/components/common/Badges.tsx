/**
 * Status badges for the orchestrator's real state enums.
 *
 * These render the backend's actual `RunState` / `TaskStatus` values
 * verbatim (domain.rs). In particular FAILED and INFRA_ERROR are visually
 * distinct, because FRD §2 and SRD §9 require product failure and
 * infrastructure failure to remain separate classifications — collapsing
 * them into one red "failed" chip would misreport what happened.
 */

import React from "react";
import type { RunState, TaskStatus } from "../../types/orchestrator";

const BASE = "inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[9px] font-medium uppercase tracking-wider";

const TASK_STATUS_CLASS: Record<TaskStatus, string> = {
  QUEUED: "bg-slate-700/40 text-slate-400",
  RUNNING: "bg-sky-500/15 text-sky-300",
  PASSED: "bg-emerald-500/15 text-emerald-300",
  // Product defect.
  FAILED: "bg-rose-500/15 text-rose-300",
  BLOCKED: "bg-amber-500/15 text-amber-300",
  SKIPPED: "bg-slate-700/40 text-slate-500",
  CANCELLED: "bg-slate-700/40 text-slate-500",
  TIMEOUT: "bg-orange-500/15 text-orange-300",
  // Infrastructure/environment fault — deliberately not the same colour as FAILED.
  INFRA_ERROR: "bg-fuchsia-500/15 text-fuchsia-300",
  NEEDS_REVIEW: "bg-violet-500/15 text-violet-300",
};

const RUN_STATE_CLASS: Record<RunState, string> = {
  CREATED: "bg-slate-700/40 text-slate-400",
  PENDING_APPROVAL: "bg-amber-500/15 text-amber-300",
  REJECTED: "bg-rose-500/15 text-rose-300",
  SCHEDULED: "bg-slate-600/40 text-slate-300",
  RUNNING: "bg-sky-500/15 text-sky-300",
  PARTIAL: "bg-orange-500/15 text-orange-300",
  COMPLETED: "bg-emerald-500/15 text-emerald-300",
  CANCELLED: "bg-slate-700/40 text-slate-500",
  FAILED: "bg-rose-500/15 text-rose-300",
};

export function TaskStatusBadge({ status }: { status: TaskStatus }) {
  return <span className={`${BASE} ${TASK_STATUS_CLASS[status] ?? TASK_STATUS_CLASS.QUEUED}`}>{status}</span>;
}

export function RunStateBadge({ state }: { state: RunState }) {
  return <span className={`${BASE} ${RUN_STATE_CLASS[state] ?? RUN_STATE_CLASS.CREATED}`}>{state}</span>;
}

/** Review state on bug drafts / failure clusters — free-form string from the
 *  DB rather than a Rust enum, so unknown values fall through to neutral. */
export function ReviewStateBadge({ state }: { state: string }) {
  const cls =
    state === "CONFIRMED"
      ? "bg-rose-500/15 text-rose-300"
      : state === "DISMISSED"
        ? "bg-slate-700/40 text-slate-500"
        : "bg-violet-500/15 text-violet-300";
  return <span className={`${BASE} ${cls}`}>{state}</span>;
}

/**
 * Confidence shown as the raw backend value, never rounded up into a claim.
 * `null` renders as "not scored" rather than as a number, per PRD §15 — an
 * AI conclusion without a confidence must not look like a confident one.
 */
export function ConfidenceLabel({ value }: { value: number | null }) {
  if (value === null || value === undefined) {
    return <span className="font-mono text-[10px] text-slate-600">not scored</span>;
  }
  return <span className="font-mono text-[10px] text-slate-400">{(value * 100).toFixed(0)}% confidence</span>;
}

/** Consistent timestamp rendering; invalid/absent shows an em dash. */
export function Timestamp({ value }: { value: string | null }) {
  if (!value) return <span className="font-mono text-[10px] text-slate-600">—</span>;
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return <span className="font-mono text-[10px] text-slate-600">—</span>;
  return (
    <span className="font-mono text-[10px] text-slate-500" title={d.toISOString()}>
      {d.toLocaleString()}
    </span>
  );
}
