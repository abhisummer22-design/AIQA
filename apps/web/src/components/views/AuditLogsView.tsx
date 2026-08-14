import React, { useState } from "react";
import {
  History,
  Search,
  Download,
  ShieldCheck,
  ShieldAlert,
  UserCheck,
  Activity,
  Layers,
  Sparkles,
  Lock
} from "lucide-react";
import { AuditLogItem } from "../../types";
import { RbacActivityLog } from "./RbacActivityLog";

interface AuditLogsViewProps {
  auditLogs: AuditLogItem[];
}

export const AuditLogsView: React.FC<AuditLogsViewProps> = ({ auditLogs }) => {
  const [activeTab, setActiveTab] = useState<"rbac" | "system">("rbac");
  const [search, setSearch] = useState("");

  const filteredSystemLogs = auditLogs.filter(
    (l) =>
      l.action.toLowerCase().includes(search.toLowerCase()) ||
      l.actor.toLowerCase().includes(search.toLowerCase()) ||
      l.resource.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="space-y-6" id="audit-logs-view">
      {/* Header with Title & Navigation Tabs */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white flex items-center gap-2.5">
            <History className="w-5 h-5 text-cyan-400" />
            Immutable Audit Trail &amp; Governance Center
          </h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            Tamper-proof compliance logs of role assignments, privilege escalations, permission modifications, and test suite executions
          </p>
        </div>

        {/* Tab Switcher */}
        <div className="flex items-center bg-[#0e1422] p-1 rounded-xl border border-slate-800 self-start md:self-auto shadow-sm">
          <button
            id="tab-audit-rbac"
            onClick={() => setActiveTab("rbac")}
            className={`px-3.5 py-1.5 rounded-lg text-xs font-mono font-medium flex items-center gap-2 transition-all ${
              activeTab === "rbac"
                ? "bg-cyan-500/15 text-cyan-300 border border-cyan-500/40 shadow-sm"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            <ShieldAlert className="w-3.5 h-3.5 text-cyan-400" />
            <span>RBAC &amp; Access Log</span>
            <span className="text-[10px] px-1.5 py-0.2 rounded-full bg-cyan-950 text-cyan-400 border border-cyan-800/80">
              Enterprise
            </span>
          </button>

          <button
            id="tab-audit-system"
            onClick={() => setActiveTab("system")}
            className={`px-3.5 py-1.5 rounded-lg text-xs font-mono font-medium flex items-center gap-2 transition-all ${
              activeTab === "system"
                ? "bg-cyan-500/15 text-cyan-300 border border-cyan-500/40 shadow-sm"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            <Layers className="w-3.5 h-3.5 text-slate-400" />
            <span>System &amp; Execution Trail</span>
            <span className="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-900 text-slate-400 border border-slate-800">
              {auditLogs.length}
            </span>
          </button>
        </div>
      </div>

      {/* View Content */}
      {activeTab === "rbac" ? (
        <RbacActivityLog />
      ) : (
        <div className="space-y-4">
          <div className="p-3.5 rounded-xl bg-[#0e1422] border border-slate-800/90 flex items-center justify-between gap-3 text-xs font-mono">
            <div className="flex items-center gap-2.5 flex-1 max-w-md bg-slate-900 px-3 py-1.5 rounded-lg border border-slate-800">
              <Search className="w-4 h-4 text-slate-500" />
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search system audit trail by actor, action, or resource..."
                className="bg-transparent text-slate-200 placeholder-slate-500 focus:outline-none w-full"
              />
            </div>
            <span className="text-slate-400">{filteredSystemLogs.length} Events Logged</span>
          </div>

          <div className="bg-[#0e1422] border border-slate-800/90 rounded-xl overflow-hidden shadow-sm">
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs font-mono">
                <thead>
                  <tr className="text-slate-500 border-b border-slate-800/80 text-[10px] uppercase bg-slate-900/50">
                    <th className="p-3 font-medium">Timestamp</th>
                    <th className="p-3 font-medium">Actor</th>
                    <th className="p-3 font-medium">Action</th>
                    <th className="p-3 font-medium">Target Resource</th>
                    <th className="p-3 font-medium text-right">Result</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/60 text-slate-300">
                  {filteredSystemLogs.map((log) => (
                    <tr key={log.id} className="hover:bg-slate-900/30">
                      <td className="p-3 text-slate-400 text-[11px] whitespace-nowrap">{log.timestamp}</td>
                      <td className="p-3 font-semibold text-slate-200">{log.actor}</td>
                      <td className="p-3 text-cyan-400 font-medium">{log.action}</td>
                      <td className="p-3 text-slate-300">{log.resource}</td>
                      <td className="p-3 text-right">
                        <span
                          className={`text-[10px] px-2 py-0.5 rounded font-semibold ${
                            log.result === "Success"
                              ? "bg-emerald-950 text-emerald-300 border border-emerald-800"
                              : "bg-rose-950 text-rose-300 border border-rose-800"
                          }`}
                        >
                          {log.result}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

