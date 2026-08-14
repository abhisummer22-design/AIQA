import React from "react";
import { RoleConfig } from "./types";
import { Check, X } from "lucide-react";

interface PermissionSummaryProps {
  role: RoleConfig;
  compact?: boolean;
}

export const PermissionSummary: React.FC<PermissionSummaryProps> = ({ role, compact = false }) => {
  const summaryItems = [
    { key: "users", label: "Users", allowed: role.permissionSummary.users },
    { key: "projects", label: "Projects", allowed: role.permissionSummary.projects },
    { key: "tests", label: "Tests", allowed: role.permissionSummary.tests },
    { key: "evidence", label: "Evidence", allowed: role.permissionSummary.evidence },
    { key: "bugs", label: "Bugs", allowed: role.permissionSummary.bugs },
    { key: "reports", label: "Reports", allowed: role.permissionSummary.reports },
    { key: "policies", label: "Policies", allowed: role.permissionSummary.policies },
    { key: "auditLogs", label: "Audit Logs", allowed: role.permissionSummary.auditLogs },
    { key: "aiModels", label: "AI Models", allowed: role.permissionSummary.aiModels },
    { key: "system", label: "System Controls", allowed: role.permissionSummary.system },
  ];

  if (compact) {
    return (
      <div className="grid grid-cols-2 sm:grid-cols-5 gap-1.5 p-2.5 bg-zinc-950/60 rounded-md border border-zinc-800/80 font-mono text-[10px]">
        {summaryItems.map((item) => (
          <div
            key={item.key}
            className={`flex items-center gap-1 px-1.5 py-0.5 rounded ${
              item.allowed
                ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20"
                : "bg-zinc-900 text-zinc-500 border border-zinc-800/60 line-through opacity-75"
            }`}
          >
            {item.allowed ? (
              <Check className="w-2.5 h-2.5 shrink-0 text-emerald-400" />
            ) : (
              <X className="w-2.5 h-2.5 shrink-0 text-zinc-500" />
            )}
            <span className="truncate">{item.label}</span>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-[11px] font-mono font-medium text-zinc-400 px-0.5">
        <span>PERMISSION MATRIX SUMMARY</span>
        <span className="text-[10px] text-zinc-500">
          {summaryItems.filter((i) => i.allowed).length}/{summaryItems.length} GRANTED
        </span>
      </div>
      <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5 p-2 bg-zinc-950/70 rounded-md border border-zinc-800/80 font-mono text-[11px]">
        {summaryItems.map((item) => (
          <div
            key={item.key}
            className={`flex items-center justify-between px-2 py-1 rounded transition-colors ${
              item.allowed
                ? "bg-sky-500/10 text-sky-300 border border-sky-500/20"
                : "bg-zinc-900/80 text-zinc-500 border border-zinc-800/50"
            }`}
          >
            <span className="truncate">{item.label}</span>
            {item.allowed ? (
              <span className="text-emerald-400 font-bold text-xs flex items-center gap-0.5">
                <Check className="w-3 h-3 stroke-[3]" />
              </span>
            ) : (
              <span className="text-zinc-500 text-xs flex items-center gap-0.5">
                <X className="w-3 h-3" />
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};
