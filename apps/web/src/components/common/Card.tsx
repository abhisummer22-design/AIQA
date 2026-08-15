/**
 * Shared surface primitives, so every panel in the app has the same frame
 * and the same treatment of a missing value.
 */

import React from "react";

export function Card({
  icon,
  title,
  action,
  children,
  bodyClassName = "p-3",
}: {
  icon?: React.ReactNode;
  title: React.ReactNode;
  action?: React.ReactNode;
  children: React.ReactNode;
  bodyClassName?: string;
}) {
  return (
    <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
      <div className="flex items-center justify-between gap-2 border-b border-slate-800 px-4 py-2.5">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
          {icon && <span className="text-cyan-400">{icon}</span>}
          {title}
        </h2>
        {action}
      </div>
      <div className={bodyClassName}>{children}</div>
    </section>
  );
}

/**
 * A single counter. `null` renders an em dash, never `0` — a metric the
 * orchestrator did not emit is not the same fact as a metric that is zero,
 * and collapsing the two is exactly the kind of small lie this UI avoids.
 */
export function Tile({
  label,
  value,
  sub,
  tone = "neutral",
}: {
  label: string;
  value: number | null;
  sub?: string;
  tone?: "neutral" | "warn" | "danger";
}) {
  const valueClass =
    value === null
      ? "text-slate-600"
      : tone === "warn"
        ? "text-amber-400"
        : tone === "danger"
          ? "text-rose-400"
          : "text-slate-100";

  return (
    <div className="rounded-xl border border-slate-800 bg-[#0a0f1a] px-3.5 py-3">
      <div className="font-mono text-[10px] uppercase tracking-wider text-slate-500">{label}</div>
      <div className={`mt-1 font-mono text-2xl font-bold ${valueClass}`}>{value === null ? "—" : value}</div>
      {sub && <div className="mt-0.5 text-[10px] text-slate-600">{sub}</div>}
    </div>
  );
}

/** Consistent label/value row for detail panels. */
export function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="font-mono text-[9px] uppercase tracking-wider text-slate-600">{label}</div>
      <div className="mt-0.5 text-[11px] text-slate-300">{children}</div>
    </div>
  );
}
