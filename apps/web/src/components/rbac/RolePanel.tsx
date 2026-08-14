import React from "react";
import { RoleConfig, RoleActionItem, LiveRbacEvent } from "./types";
import { RoleHeader } from "./RoleHeader";
import { PermissionSummary } from "./PermissionSummary";
import { RoleActions } from "./RoleActions";
import { CheckCircle2, XCircle, Clock, ShieldAlert, Sparkles } from "lucide-react";

interface RolePanelProps {
  role: RoleConfig;
  onZoom: (roleId: RoleConfig["id"]) => void;
  onExecuteAction: (role: RoleConfig, action: RoleActionItem) => void;
  activeActionId?: string | null;
  activeEvent?: LiveRbacEvent | null;
  isEvaluating?: boolean;
  evaluationResult?: "AUTHORIZED" | "FORBIDDEN" | null;
}

export const RolePanel: React.FC<RolePanelProps> = ({
  role,
  onZoom,
  onExecuteAction,
  activeActionId,
  activeEvent,
  isEvaluating,
  evaluationResult
}) => {
  // Determine active glow styling
  const getGlowClasses = () => {
    if (!isEvaluating && !evaluationResult) {
      return "border-zinc-800 hover:border-zinc-700 bg-zinc-900/60 shadow-lg";
    }
    if (evaluationResult === "AUTHORIZED") {
      return "border-sky-400 bg-zinc-900/90 shadow-[0_0_24px_rgba(56,189,248,0.35)] ring-1 ring-sky-400/40";
    }
    if (evaluationResult === "FORBIDDEN") {
      return "border-orange-500 bg-zinc-900/90 shadow-[0_0_24px_rgba(249,115,22,0.4)] ring-1 ring-orange-500/40";
    }
    return "border-sky-500/60 bg-zinc-900/80 shadow-[0_0_15px_rgba(56,189,248,0.2)]";
  };

  return (
    <div
      onClick={() => onZoom(role.id)}
      className={`group relative flex flex-col rounded-xl overflow-hidden border transition-all duration-300 cursor-pointer ${getGlowClasses()}`}
    >
      {/* Top Header */}
      <RoleHeader role={role} onZoom={() => onZoom(role.id)} />

      {/* Main Content Area */}
      <div className="p-3.5 space-y-3 flex-1 flex flex-col justify-between">
        {/* Compact Permission Matrix Summary */}
        <PermissionSummary role={role} compact={false} />

        {/* Action Controls */}
        <RoleActions
          role={role}
          onExecuteAction={onExecuteAction}
          activeActionId={activeActionId}
        />

        {/* Live Event Feedback Bar */}
        {activeEvent && (
          <div
            className={`p-2 rounded-lg border font-mono text-[11px] flex items-center justify-between transition-all ${
              activeEvent.result === "AUTHORIZED"
                ? "bg-sky-950/40 border-sky-500/40 text-sky-200"
                : "bg-orange-950/40 border-orange-500/40 text-orange-200"
            }`}
          >
            <div className="flex items-center gap-1.5 truncate">
              {activeEvent.result === "AUTHORIZED" ? (
                <CheckCircle2 className="w-3.5 h-3.5 text-sky-400 shrink-0" />
              ) : (
                <XCircle className="w-3.5 h-3.5 text-orange-400 shrink-0" />
              )}
              <span className="truncate font-semibold">{activeEvent.action}</span>
              <span className="text-[10px] opacity-75">({activeEvent.permission})</span>
            </div>

            <div className="shrink-0 flex items-center gap-1 font-bold text-[10px]">
              <span
                className={`px-1.5 py-0.5 rounded border ${
                  activeEvent.result === "AUTHORIZED"
                    ? "bg-sky-500/20 text-sky-300 border-sky-400/30"
                    : "bg-orange-500/20 text-orange-300 border-orange-400/30"
                }`}
              >
                {activeEvent.result === "AUTHORIZED" ? "✓ 200 OK" : "✕ 403 DENIED"}
              </span>
            </div>
          </div>
        )}
      </div>

      {/* Subtle Bottom Bar with Session Meta */}
      <div className="px-3.5 py-1.5 bg-zinc-950/90 border-t border-zinc-800/80 flex items-center justify-between text-[10px] font-mono text-zinc-500">
        <span className="truncate">Tenant: {role.tenant}</span>
        <span className="group-hover:text-sky-400 transition-colors flex items-center gap-1">
          Click panel to Zoom <Sparkles className="w-2.5 h-2.5" />
        </span>
      </div>
    </div>
  );
};
