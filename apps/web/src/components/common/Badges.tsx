/**
 * Status badges for the orchestrator's real state enums.
 *
 * Re-exports and wraps the canonical StatusBadge for backward-compatibility
 * while adding rich tooltips and classification distinctions.
 */

import React from "react";
import type { RunState, TaskStatus } from "../../types/orchestrator";
import { StatusBadge, FailureClassificationBadge } from "./StatusBadge";

export { StatusBadge, FailureClassificationBadge };

export function TaskStatusBadge({
  status,
  size = "sm",
}: {
  status: TaskStatus;
  size?: "sm" | "md" | "lg";
}) {
  return <StatusBadge status={status} size={size} />;
}

export function RunStateBadge({
  state,
  size = "sm",
}: {
  state: RunState;
  size?: "sm" | "md" | "lg";
}) {
  return <StatusBadge status={state} size={size} />;
}

/** Review state on bug drafts / failure clusters */
export function ReviewStateBadge({ state }: { state: string }) {
  return <StatusBadge status={state} size="sm" />;
}

/**
 * Confidence shown as the raw backend value, never rounded up into a claim.
 * `null` renders as "not scored" rather than as a number, per PRD §15.
 */
export function ConfidenceLabel({ value }: { value: number | null }) {
  if (value === null || value === undefined) {
    return <span className="font-mono text-[10px] text-slate-600">not scored</span>;
  }
  return (
    <span className="inline-flex items-center gap-1 font-mono text-[10px] text-cyan-400 bg-cyan-950/40 border border-cyan-800/40 px-1.5 py-0.5 rounded">
      <span>{(value * 100).toFixed(0)}%</span>
      <span className="text-slate-500">confidence</span>
    </span>
  );
}

/** Consistent timestamp rendering; invalid/absent shows an em dash. */
export function Timestamp({ value }: { value: string | null }) {
  if (!value) return <span className="font-mono text-[10px] text-slate-600">—</span>;
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return <span className="font-mono text-[10px] text-slate-600">—</span>;
  return (
    <span className="font-mono text-[10px] text-slate-400 hover:text-slate-200 transition-colors" title={d.toISOString()}>
      {d.toLocaleString()}
    </span>
  );
}
