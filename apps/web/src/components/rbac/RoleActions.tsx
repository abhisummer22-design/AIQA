import React from "react";
import { RoleConfig, RoleActionItem } from "./types";
import { Lock, Play, ArrowRight, Zap } from "lucide-react";

interface RoleActionsProps {
  role: RoleConfig;
  onExecuteAction: (role: RoleConfig, action: RoleActionItem) => void;
  activeActionId?: string | null;
}

export const RoleActions: React.FC<RoleActionsProps> = ({
  role,
  onExecuteAction,
  activeActionId
}) => {
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-[11px] font-mono font-medium text-zinc-400 px-0.5">
        <span className="flex items-center gap-1.5">
          <Zap className="w-3 h-3 text-sky-400" />
          TEST ACTION CONTROLS
        </span>
        <span className="text-[10px] text-zinc-500">Click to probe engine</span>
      </div>

      <div className="grid grid-cols-2 gap-1.5 font-mono text-[11px]">
        {role.actions.map((action) => {
          const isActive = activeActionId === `${role.id}:${action.id}`;
          return (
            <button
              key={action.id}
              onClick={(e) => {
                e.stopPropagation();
                onExecuteAction(role, action);
              }}
              disabled={isActive}
              className={`group relative flex items-center justify-between px-2.5 py-1.5 rounded text-left transition-all border ${
                isActive
                  ? action.isAllowed
                    ? "bg-sky-500/20 border-sky-400 text-sky-200 shadow-[0_0_12px_rgba(56,189,248,0.4)]"
                    : "bg-orange-500/20 border-orange-400 text-orange-200 shadow-[0_0_12px_rgba(249,115,22,0.4)]"
                  : action.isAllowed
                  ? "bg-zinc-900/90 hover:bg-zinc-800/90 text-zinc-200 border-zinc-700/70 hover:border-sky-500/60 hover:text-white"
                  : "bg-zinc-950/70 hover:bg-zinc-900/80 text-zinc-400 border-zinc-800/60 hover:border-orange-500/50 hover:text-orange-300"
              }`}
              title={`${action.label} (${action.permission}) — ${
                action.isAllowed ? "Permitted" : "Forbidden (Tests 403 Response)"
              }`}
            >
              <div className="min-w-0 pr-1">
                <div className="flex items-center gap-1 truncate font-medium">
                  {!action.isAllowed && (
                    <Lock className="w-3 h-3 text-orange-400 shrink-0 inline" />
                  )}
                  <span className="truncate">{action.label}</span>
                </div>
                <div className="text-[9px] text-zinc-500 group-hover:text-zinc-400 truncate mt-0.5">
                  {action.permission}
                </div>
              </div>

              <div className="shrink-0 opacity-60 group-hover:opacity-100 transition-opacity">
                {action.isAllowed ? (
                  <Play className="w-3 h-3 text-sky-400" />
                ) : (
                  <span className="text-[9px] font-bold text-orange-400 bg-orange-500/10 px-1 py-0.2 rounded border border-orange-500/30">
                    403
                  </span>
                )}
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
};
