import React from "react";
import { Cpu, Check, X, Shield, Activity, Lock, CheckCircle2, AlertOctagon } from "lucide-react";
import { LiveRbacEvent } from "./types";

interface RBACEngineNodeProps {
  activeEvent: LiveRbacEvent | null;
  isEvaluating: boolean;
  totalEvaluated: number;
}

export const RBACEngineNode: React.FC<RBACEngineNodeProps> = ({
  activeEvent,
  isEvaluating,
  totalEvaluated
}) => {
  const isAuthorized = activeEvent?.result === "AUTHORIZED";
  const isForbidden = activeEvent?.result === "FORBIDDEN";

  const getContainerGlow = () => {
    if (!activeEvent && !isEvaluating) {
      return "border-zinc-800 bg-zinc-900/90 shadow-xl";
    }
    if (isAuthorized) {
      return "border-sky-400 bg-zinc-900 shadow-[0_0_30px_rgba(56,189,248,0.45)] ring-1 ring-sky-400/50";
    }
    if (isForbidden) {
      return "border-orange-500 bg-zinc-900 shadow-[0_0_30px_rgba(249,115,22,0.45)] ring-1 ring-orange-500/50";
    }
    return "border-sky-500/70 bg-zinc-900 shadow-[0_0_20px_rgba(56,189,248,0.25)]";
  };

  const steps = [
    {
      id: "auth",
      label: "Authentication",
      active: !!activeEvent,
      passed: activeEvent?.checks.authentication ?? true,
      desc: activeEvent ? `${activeEvent.actorEmail} (Verified)` : "JWT / Session Token"
    },
    {
      id: "permission",
      label: "Permission Check",
      active: !!activeEvent,
      passed: activeEvent?.checks.permissionCheck ?? (isAuthorized || false),
      desc: activeEvent ? `${activeEvent.permission}` : "Granular Scope Match"
    },
    {
      id: "tenant",
      label: "Tenant Check",
      active: !!activeEvent,
      passed: activeEvent?.checks.tenantCheck ?? true,
      desc: activeEvent ? `${activeEvent.tenantId}` : "Org Isolation Boundary"
    },
    {
      id: "project",
      label: "Project Access",
      active: !!activeEvent,
      passed: activeEvent?.checks.projectAccess ?? true,
      desc: "Scope Boundary Valid"
    },
    {
      id: "policy",
      label: "Policy Check",
      active: !!activeEvent,
      passed: activeEvent?.checks.policyCheck ?? (isAuthorized || false),
      desc: "Security Guardrails"
    },
    {
      id: "decision",
      label: "Decision",
      active: !!activeEvent,
      passed: isAuthorized,
      desc: activeEvent
        ? isAuthorized
          ? "200 AUTHORIZED"
          : `403 FORBIDDEN (${activeEvent.reason || "Denied"})`
        : "Evaluation Pipeline"
    }
  ];

  return (
    <div
      className={`relative z-20 w-full max-w-xs sm:max-w-sm rounded-xl border p-3.5 transition-all duration-300 ${getContainerGlow()}`}
    >
      {/* Header */}
      <div className="flex items-center justify-between border-b border-zinc-800 pb-2 mb-2.5">
        <div className="flex items-center gap-2">
          <div
            className={`w-7 h-7 rounded-lg flex items-center justify-center border transition-all ${
              isAuthorized
                ? "bg-sky-500/20 text-sky-300 border-sky-500/40 animate-pulse"
                : isForbidden
                ? "bg-orange-500/20 text-orange-300 border-orange-500/40 animate-pulse"
                : "bg-zinc-800 text-zinc-300 border-zinc-700"
            }`}
          >
            <Cpu className="w-4 h-4" />
          </div>
          <div>
            <h4 className="text-xs font-bold font-mono tracking-wider text-zinc-100 uppercase flex items-center gap-1.5">
              RBAC ENGINE
            </h4>
            <div className="flex items-center gap-1 text-[10px] text-emerald-400 font-mono">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-ping" />
              <span>ONLINE</span>
            </div>
          </div>
        </div>

        <div className="text-right font-mono text-[10px]">
          <div className="text-zinc-500">PROBES</div>
          <div className="text-zinc-300 font-semibold">{totalEvaluated} ops</div>
        </div>
      </div>

      {/* Real-time Checklist Visualization */}
      <div className="space-y-1 font-mono text-[10px]">
        {steps.map((step, idx) => {
          const isPassed = step.passed;
          const isStepActive = step.active;

          return (
            <div
              key={step.id}
              className={`flex items-center justify-between px-2 py-1 rounded transition-colors ${
                isStepActive
                  ? isPassed
                    ? "bg-sky-950/40 text-sky-300 border border-sky-500/30"
                    : "bg-orange-950/40 text-orange-300 border border-orange-500/30 font-semibold"
                  : "bg-zinc-950/50 text-zinc-400 border border-zinc-900"
              }`}
            >
              <div className="flex items-center gap-1.5 truncate">
                <span className="text-zinc-600 text-[9px] w-3">{idx + 1}.</span>
                <span className="truncate font-medium">{step.label}</span>
              </div>

              <div className="flex items-center gap-1 shrink-0">
                {isStepActive ? (
                  isPassed ? (
                    <span className="text-emerald-400 flex items-center gap-0.5 font-bold">
                      <Check className="w-3 h-3 stroke-[2.5]" />
                      <span className="hidden sm:inline text-[9px]">PASS</span>
                    </span>
                  ) : (
                    <span className="text-orange-400 flex items-center gap-0.5 font-bold">
                      <X className="w-3 h-3 stroke-[2.5]" />
                      <span className="text-[9px]">DENIED</span>
                    </span>
                  )
                ) : (
                  <span className="text-zinc-600 text-[9px]">IDLE</span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* Decision Status Banner */}
      {activeEvent && (
        <div
          className={`mt-2.5 p-2 rounded-lg border text-center font-mono text-[11px] font-bold tracking-wide transition-all ${
            isAuthorized
              ? "bg-sky-500/10 border-sky-400/40 text-sky-300 shadow-[0_0_15px_rgba(56,189,248,0.2)]"
              : "bg-orange-500/10 border-orange-400/40 text-orange-300 shadow-[0_0_15px_rgba(249,115,22,0.2)]"
          }`}
        >
          {isAuthorized ? "✓ DECISION: AUTHORIZED (200)" : "✕ DECISION: 403 FORBIDDEN"}
          <div className="text-[9px] font-normal text-zinc-400 truncate mt-0.5">
            {activeEvent.action} → {activeEvent.resource}
          </div>
        </div>
      )}
    </div>
  );
};
