/**
 * Audit log — `GET /admin/audit-log` (admin role required).
 *
 * Immutable, server-side recorded actions with full actor and outcome trace.
 */

import React, { useState } from "react";
import { ScrollText, Filter, Search, RotateCcw } from "lucide-react";
import { useApiResource } from "../hooks/useApi";
import { getAuditLog } from "../services/orchestrator";
import type { AuditRow } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { Timestamp } from "../components/common/Badges";
import { SectionHeader } from "../components/common/SectionHeader";

const LIMITS = [50, 100, 250, 500];

export function AuditLogView() {
  const [limit, setLimit] = useState(100);
  const [filter, setFilter] = useState("");

  const audit = useApiResource<AuditRow[]>((signal) => getAuditLog(limit, signal), [limit]);

  return (
    <div className="space-y-4">
      <SectionHeader
        title="Immutable Audit & Governance Log"
        subtitle="Cryptographically tracked administrative and execution operations recorded by the control plane (admin role required)."
        badge="REALTIME BACKEND"
        badgeTone="live"
        action={
          <button
            onClick={() => audit.reload()}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 bg-slate-900/60 hover:bg-slate-800 text-xs text-slate-300 transition-colors cursor-pointer"
          >
            <RotateCcw size={12} />
            <span>Refresh Audit Log</span>
          </button>
        }
      />

      {/* Filter and Limit Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 p-2.5 rounded-xl border border-slate-800 bg-[#0a0f1a]">
        <div className="flex items-center gap-2 flex-1 max-w-sm">
          <div className="relative w-full">
            <Search size={13} className="absolute left-2.5 top-2.5 text-slate-500" />
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Search actors, actions, targets…"
              className="w-full pl-8 pr-3 py-1.5 rounded-md border border-slate-700 bg-slate-900/80 text-xs text-slate-200 outline-none focus:border-cyan-500 font-sans"
            />
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="font-mono text-[10px] text-slate-500 uppercase">Limit:</span>
          <select
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="rounded border border-slate-700 bg-slate-900/80 px-2.5 py-1.5 text-xs text-slate-300 outline-none focus:border-cyan-500 font-mono"
          >
            {LIMITS.map((n) => (
              <option key={n} value={n}>
                {n} entries
              </option>
            ))}
          </select>
        </div>
      </div>

      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a] shadow-lg">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-3 text-sm font-semibold text-slate-200">
          <ScrollText size={14} className="text-cyan-400" />
          <span>Audit Entries</span>
        </h2>
        <div className="overflow-x-auto p-3">
          <ResourceView
            state={audit}
            loadingLabel="Loading audit log entries…"
            emptyTitle="No audit entries recorded"
            emptyMessage="The orchestrator has recorded no auditable administrative actions yet."
          >
            {(rows) => {
              const q = filter.trim().toLowerCase();
              const visible = q
                ? rows.filter((r) =>
                    [r.actor, r.role, r.action, r.target, r.detail, r.outcome, r.project_id]
                      .filter(Boolean)
                      .some((v) => String(v).toLowerCase().includes(q)),
                  )
                : rows;

              if (visible.length === 0) {
                return (
                  <div className="py-8 text-center text-xs text-slate-500 font-mono">
                    No entries match “{filter}”. ({rows.length} total loaded).
                  </div>
                );
              }

              return (
                <table className="w-full min-w-[860px] text-left text-xs">
                  <thead>
                    <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800/80">
                      <th className="pb-2.5 font-normal">Timestamp</th>
                      <th className="pb-2.5 font-normal">Actor</th>
                      <th className="pb-2.5 font-normal">Role</th>
                      <th className="pb-2.5 font-normal">Action Performed</th>
                      <th className="pb-2.5 font-normal">Target</th>
                      <th className="pb-2.5 font-normal">Context Details</th>
                      <th className="pb-2.5 font-normal">Outcome</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visible.map((row, i) => (
                      <tr key={`${row.occurred_at}-${i}`} className="border-t border-slate-800/60 align-top hover:bg-slate-900/30 transition-colors">
                        <td className="whitespace-nowrap py-2.5 pr-3">
                          <Timestamp value={row.occurred_at} />
                        </td>
                        <td className="py-2.5 pr-3 text-slate-200 font-medium">{row.actor ?? "—"}</td>
                        <td className="py-2.5 pr-3 font-mono text-[10px] text-cyan-400">{row.role ?? "—"}</td>
                        <td className="py-2.5 pr-3 font-mono text-xs font-semibold text-slate-100">{row.action}</td>
                        <td className="py-2.5 pr-3 font-mono text-[10px] text-slate-400">
                          {row.target ? row.target.slice(0, 16) : "—"}
                        </td>
                        <td className="max-w-sm py-2.5 pr-3 text-xs text-slate-400">{row.detail ?? "—"}</td>
                        <td className="py-2.5">
                          <span
                            className={`rounded px-2 py-0.5 font-mono text-[9px] uppercase font-semibold tracking-wider ${
                              row.outcome === "ok"
                                ? "bg-emerald-950/80 text-emerald-300 border border-emerald-800/80"
                                : "bg-rose-950/80 text-rose-300 border border-rose-800/80"
                            }`}
                          >
                            {row.outcome}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              );
            }}
          </ResourceView>
        </div>
      </section>
    </div>
  );
}
