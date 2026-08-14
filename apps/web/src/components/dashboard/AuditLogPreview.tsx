import React from "react";
import { History, CheckCircle2, XCircle, ShieldAlert, ArrowRight, User } from "lucide-react";
import { AuditLogItem, ViewType } from "../../types";

interface AuditLogPreviewProps {
  auditLogs: AuditLogItem[];
  onNavigate: (view: ViewType) => void;
}

export const AuditLogPreview: React.FC<AuditLogPreviewProps> = ({ auditLogs, onNavigate }) => {
  return (
    <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 space-y-2.5">
      {/* Header */}
      <div className="flex items-center justify-between pb-2 border-b border-slate-800">
        <div className="flex items-center gap-2">
          <History className="w-3.5 h-3.5 text-indigo-400" />
          <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
            Immutable Audit Trail
          </h3>
        </div>
        <button
          onClick={() => onNavigate("audit-logs")}
          className="text-[10px] font-mono text-indigo-400 hover:underline"
        >
          Full Log &rarr;
        </button>
      </div>

      {/* Table */}
      <div className="overflow-x-auto">
        <table className="w-full text-left text-[11px] font-mono">
          <thead>
            <tr className="text-slate-500 border-b border-slate-800 text-[10px] uppercase">
              <th className="pb-1.5 font-medium">Time</th>
              <th className="pb-1.5 font-medium">Actor</th>
              <th className="pb-1.5 font-medium">Action</th>
              <th className="pb-1.5 font-medium">Resource</th>
              <th className="pb-1.5 font-medium text-right">Result</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60 text-slate-300">
            {auditLogs.slice(0, 4).map((log) => (
              <tr key={log.id} className="hover:bg-slate-900/30">
                <td className="py-1.5 text-slate-400 text-[10px]">{log.timestamp}</td>
                <td className="py-1.5 text-slate-200 font-semibold truncate max-w-[110px]">
                  {log.actor}
                </td>
                <td className="py-1.5 text-indigo-400">{log.action}</td>
                <td className="py-1.5 text-slate-400 truncate max-w-[120px]">{log.resource}</td>
                <td className="py-1.5 text-right">
                  <span
                    className={`text-[9px] px-1.5 py-0.2 rounded font-semibold ${
                      log.result === "Success"
                        ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                        : "bg-rose-500/10 text-rose-400 border border-rose-500/30"
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
  );
};

