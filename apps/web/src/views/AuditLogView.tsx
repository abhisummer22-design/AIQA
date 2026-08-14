/**
 * Audit log — `GET /admin/audit-log` (admin role required).
 *
 * Every row is a real entry the orchestrator wrote via `store.insert_audit`:
 * run creation, approvals, rejections, bug-draft reviews, API-key
 * create/revoke, and failed authentication attempts. Nothing is synthesized.
 *
 * A non-admin key gets a 403, which surfaces as the "Not permitted" state
 * rather than an empty table — the distinction matters, because an empty
 * table would imply nothing has happened.
 */

import React, { useState } from "react";
import { ScrollText } from "lucide-react";
import { useApiResource } from "../hooks/useApi";
import { getAuditLog } from "../services/orchestrator";
import type { AuditRow } from "../types/orchestrator";
import { ResourceView } from "../components/common/StateViews";
import { Timestamp } from "../components/common/Badges";

const LIMITS = [50, 100, 250, 500];

export function AuditLogView() {
  const [limit, setLimit] = useState(100);
  const [filter, setFilter] = useState("");

  const audit = useApiResource<AuditRow[]>((signal) => getAuditLog(limit, signal), [limit]);

  return (
    <div className="space-y-4">
      <header className="flex items-end justify-between">
        <div>
          <h1 className="text-lg font-bold tracking-tight text-slate-100">Audit Log</h1>
          <p className="mt-0.5 text-xs text-slate-500">
            Server-recorded actions from <code className="font-mono text-slate-400">GET /admin/audit-log</code>.
            Requires an admin key.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter rows…"
            className="w-44 rounded border border-slate-700 bg-slate-900/60 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-cyan-600"
          />
          <select
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="rounded border border-slate-700 bg-slate-900/60 px-2 py-1.5 text-xs text-slate-300 outline-none focus:border-cyan-600"
          >
            {LIMITS.map((n) => (
              <option key={n} value={n}>
                {n} rows
              </option>
            ))}
          </select>
        </div>
      </header>

      <section className="rounded-xl border border-slate-800 bg-[#0a0f1a]">
        <h2 className="flex items-center gap-2 border-b border-slate-800 px-4 py-2.5 text-sm font-semibold text-slate-200">
          <ScrollText size={14} className="text-cyan-400" />
          Entries
        </h2>
        <div className="overflow-x-auto p-3">
          <ResourceView
            state={audit}
            loadingLabel="Loading audit log…"
            emptyTitle="No audit entries"
            emptyMessage="The orchestrator has recorded no auditable actions yet."
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
                  <p className="py-6 text-center text-xs text-slate-600">
                    No entries match “{filter}”. {rows.length} total loaded.
                  </p>
                );
              }

              return (
                <table className="w-full min-w-[860px] text-left text-xs">
                  <thead>
                    <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-600">
                      <th className="pb-2 font-normal">When</th>
                      <th className="pb-2 font-normal">Actor</th>
                      <th className="pb-2 font-normal">Role</th>
                      <th className="pb-2 font-normal">Action</th>
                      <th className="pb-2 font-normal">Target</th>
                      <th className="pb-2 font-normal">Detail</th>
                      <th className="pb-2 font-normal">Outcome</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visible.map((row, i) => (
                      <tr key={`${row.occurred_at}-${i}`} className="border-t border-slate-800/70 align-top">
                        <td className="whitespace-nowrap py-1.5 pr-3">
                          <Timestamp value={row.occurred_at} />
                        </td>
                        <td className="py-1.5 pr-3 text-slate-300">{row.actor ?? "—"}</td>
                        <td className="py-1.5 pr-3 font-mono text-[10px] text-slate-500">{row.role ?? "—"}</td>
                        <td className="py-1.5 pr-3 font-mono text-[11px] text-cyan-400">{row.action}</td>
                        <td className="py-1.5 pr-3 font-mono text-[10px] text-slate-500">
                          {row.target ? row.target.slice(0, 12) : "—"}
                        </td>
                        <td className="max-w-sm py-1.5 pr-3 text-[11px] text-slate-400">{row.detail ?? "—"}</td>
                        <td className="py-1.5">
                          <span
                            className={`rounded px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-wider ${
                              row.outcome === "ok"
                                ? "bg-emerald-500/15 text-emerald-300"
                                : "bg-rose-500/15 text-rose-300"
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
