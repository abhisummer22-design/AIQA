import React from "react";
import { RoleConfig, RoleActionItem, LiveRbacEvent } from "./types";
import { RoleHeader } from "./RoleHeader";
import { PermissionSummary } from "./PermissionSummary";
import { RoleActions } from "./RoleActions";
import { SessionStatus } from "./SessionStatus";
import {
  X,
  Shield,
  Zap,
  CheckCircle2,
  XCircle,
  KeyRound,
  Terminal,
  Activity,
  Server,
  Layers,
  ArrowRight
} from "lucide-react";

interface RoleZoomViewProps {
  zoomedRole: RoleConfig;
  allRoles: RoleConfig[];
  onClose: () => void;
  onSelectRole: (roleId: RoleConfig["id"]) => void;
  onExecuteAction: (role: RoleConfig, action: RoleActionItem) => void;
  activeActionId?: string | null;
  events: LiveRbacEvent[];
  onInspectEvent: (event: LiveRbacEvent) => void;
}

export const RoleZoomView: React.FC<RoleZoomViewProps> = ({
  zoomedRole,
  allRoles,
  onClose,
  onSelectRole,
  onExecuteAction,
  activeActionId,
  events,
  onInspectEvent
}) => {
  const otherRoles = allRoles.filter((r) => r.id !== zoomedRole.id);
  const roleEvents = events.filter((e) => e.roleId === zoomedRole.id);

  return (
    <div className="relative w-full rounded-2xl bg-zinc-950 border border-zinc-700/80 shadow-[0_0_50px_rgba(0,0,0,0.8)] overflow-hidden flex flex-col transition-all duration-300">
      {/* Top Bar with Prominent Close Button */}
      <div className="flex items-center justify-between px-4 py-3 bg-zinc-900/90 border-b border-zinc-800">
        <div className="flex items-center gap-3">
          <div
            className={`w-9 h-9 rounded-lg flex items-center justify-center font-bold border ${zoomedRole.color.border} ${zoomedRole.color.bg} ${zoomedRole.color.text}`}
          >
            <Shield className="w-5 h-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-base font-bold font-mono text-zinc-100 uppercase tracking-wider">
                {zoomedRole.name} SESSION FOCUS
              </h2>
              <SessionStatus role={zoomedRole} />
              <span className="px-2 py-0.5 rounded-full text-[10px] font-mono font-bold bg-sky-500/10 text-sky-400 border border-sky-500/30">
                ZOOMED VIEW
              </span>
            </div>
            <p className="text-xs text-zinc-400 font-mono">
              Session User: <span className="text-sky-300 font-medium">{zoomedRole.email}</span> • Tenant:{" "}
              <span className="text-zinc-300">{zoomedRole.tenant}</span>
            </p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          {/* Peripheral Quick Role Switchers */}
          <div className="hidden md:flex items-center gap-1.5 bg-zinc-950 px-2 py-1 rounded-lg border border-zinc-800 text-xs font-mono">
            <span className="text-zinc-500 text-[10px]">SWITCH ROLE:</span>
            {otherRoles.map((r) => (
              <button
                key={r.id}
                onClick={() => onSelectRole(r.id)}
                className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors hover:bg-zinc-800 ${r.color.text}`}
              >
                {r.name}
              </button>
            ))}
          </div>

          {/* Prominent Close Button */}
          <button
            onClick={onClose}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 hover:text-white border border-zinc-600 transition-all text-xs font-mono font-bold shadow-sm"
          >
            <X className="w-4 h-4" />
            <span>CLOSE ZOOM</span>
          </button>
        </div>
      </div>

      {/* Main Zoom Workspace Grid */}
      <div className="p-4 grid grid-cols-1 lg:grid-cols-3 gap-4 flex-1">
        {/* Left Column: Role Details, Permissions, Token */}
        <div className="space-y-4">
          {/* Permission Summary */}
          <div className="bg-zinc-900/70 p-3.5 rounded-xl border border-zinc-800">
            <h4 className="text-xs font-mono font-bold text-zinc-200 uppercase tracking-wider mb-2.5 flex items-center gap-1.5">
              <KeyRound className="w-4 h-4 text-sky-400" />
              Role Permission Governance
            </h4>
            <PermissionSummary role={zoomedRole} compact={false} />
          </div>

          {/* Session Token & Context */}
          <div className="bg-zinc-900/70 p-3.5 rounded-xl border border-zinc-800 font-mono text-[11px] space-y-2">
            <h4 className="text-xs font-bold text-zinc-200 uppercase tracking-wider flex items-center gap-1.5">
              <Terminal className="w-4 h-4 text-indigo-400" />
              Active Session Metadata
            </h4>
            <div className="bg-zinc-950 p-2.5 rounded-lg border border-zinc-800/80 space-y-1 text-zinc-400">
              <div>
                <span className="text-zinc-500">Subject ID:</span> {zoomedRole.id}
              </div>
              <div>
                <span className="text-zinc-500">JWT Scopes:</span> {zoomedRole.allowedPermissions.length} active
              </div>
              <div>
                <span className="text-zinc-500">Bearer Token:</span>{" "}
                <span className="text-sky-400 truncate">{zoomedRole.sessionToken.slice(0, 24)}...</span>
              </div>
              <div>
                <span className="text-zinc-500">Isolation Boundary:</span> Single-Tenant Strict Mode
              </div>
            </div>
          </div>
        </div>

        {/* Center Column: Live Action Probes & Controls */}
        <div className="space-y-4">
          <div className="bg-zinc-900/70 p-3.5 rounded-xl border border-zinc-800">
            <h4 className="text-xs font-mono font-bold text-zinc-200 uppercase tracking-wider mb-2.5 flex items-center gap-1.5">
              <Zap className="w-4 h-4 text-amber-400" />
              Direct Role Action Sandbox
            </h4>
            <p className="text-[11px] text-zinc-400 font-mono mb-3">
              Trigger any allowed or prohibited action to observe real-time evaluation steps and decision latency.
            </p>
            <RoleActions
              role={zoomedRole}
              onExecuteAction={onExecuteAction}
              activeActionId={activeActionId}
            />
          </div>
        </div>

        {/* Right Column: Role Specific Real-Time Event Audit */}
        <div className="bg-zinc-900/70 p-3.5 rounded-xl border border-zinc-800 flex flex-col">
          <div className="flex items-center justify-between mb-2.5">
            <h4 className="text-xs font-mono font-bold text-zinc-200 uppercase tracking-wider flex items-center gap-1.5">
              <Activity className="w-4 h-4 text-emerald-400" />
              Role Activity Stream
            </h4>
            <span className="text-[10px] font-mono text-zinc-500">
              {roleEvents.length} recorded
            </span>
          </div>

          <div className="flex-1 overflow-y-auto max-h-80 divide-y divide-zinc-800 font-mono text-[11px]">
            {roleEvents.length === 0 ? (
              <div className="p-6 text-center text-zinc-500 text-xs">
                No events recorded yet for this session. Click an action control to fire a request.
              </div>
            ) : (
              roleEvents.map((evt) => (
                <div
                  key={evt.id}
                  onClick={() => onInspectEvent(evt)}
                  className="p-2 hover:bg-zinc-800/80 transition-colors rounded-lg cursor-pointer space-y-1"
                >
                  <div className="flex items-center justify-between">
                    <span className="text-zinc-400 font-semibold truncate">{evt.action}</span>
                    <span
                      className={`text-[9px] px-1.5 py-0.2 rounded font-bold border ${
                        evt.result === "AUTHORIZED"
                          ? "bg-sky-500/20 text-sky-300 border-sky-400/30"
                          : "bg-orange-500/20 text-orange-300 border-orange-400/30"
                      }`}
                    >
                      {evt.result === "AUTHORIZED" ? "✓ 200" : "✕ 403"}
                    </span>
                  </div>
                  <div className="text-[10px] text-zinc-500 flex items-center justify-between">
                    <span>{evt.permission}</span>
                    <span>{evt.timeFormatted}</span>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
