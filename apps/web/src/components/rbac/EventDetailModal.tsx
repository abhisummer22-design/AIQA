import React from "react";
import { X, CheckCircle2, XCircle, Shield, KeyRound, Terminal, Database, Server } from "lucide-react";
import { LiveRbacEvent } from "./types";

interface EventDetailModalProps {
  event: LiveRbacEvent | null;
  onClose: () => void;
}

export const EventDetailModal: React.FC<EventDetailModalProps> = ({ event, onClose }) => {
  if (!event) return null;

  const isAuth = event.result === "AUTHORIZED";

  return (
    <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="bg-zinc-950 border border-zinc-700/80 rounded-2xl w-full max-w-2xl max-h-[90vh] flex flex-col shadow-2xl overflow-hidden font-mono text-xs animate-in fade-in zoom-in-95 duration-200">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 bg-zinc-900 border-b border-zinc-800">
          <div className="flex items-center gap-2.5">
            <div
              className={`p-2 rounded-lg border ${
                isAuth
                  ? "bg-sky-500/10 text-sky-400 border-sky-500/20"
                  : "bg-orange-500/10 text-orange-400 border-orange-500/20"
              }`}
            >
              {isAuth ? <CheckCircle2 className="w-5 h-5" /> : <XCircle className="w-5 h-5" />}
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h3 className="text-sm font-bold text-zinc-100 uppercase tracking-wider">
                  AUTHORIZATION AUDIT INSPECTOR
                </h3>
                <span
                  className={`px-2 py-0.2 rounded text-[10px] font-bold border ${
                    isAuth
                      ? "bg-sky-500/20 text-sky-300 border-sky-400/30"
                      : "bg-orange-500/20 text-orange-300 border-orange-400/30"
                  }`}
                >
                  {isAuth ? "✓ 200 AUTHORIZED" : "✕ 403 FORBIDDEN"}
                </span>
              </div>
              <p className="text-[11px] text-zinc-400">Event ID: {event.id}</p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content Body */}
        <div className="p-5 space-y-4 overflow-y-auto">
          {/* Key Summary Grid */}
          <div className="grid grid-cols-2 gap-2.5 bg-zinc-900/60 p-3 rounded-xl border border-zinc-800">
            <div>
              <span className="text-zinc-500 text-[10px] block">CALLER SESSION:</span>
              <span className="text-zinc-200 font-semibold">{event.actorEmail}</span>
            </div>
            <div>
              <span className="text-zinc-500 text-[10px] block">ROLE ASSIGNMENT:</span>
              <span className="text-sky-300 font-semibold">{event.roleName} ({event.roleId})</span>
            </div>
            <div>
              <span className="text-zinc-500 text-[10px] block">ACTION SOUGHT:</span>
              <span className="text-amber-300 font-semibold">{event.action}</span>
            </div>
            <div>
              <span className="text-zinc-500 text-[10px] block">REQUIRED PERMISSION:</span>
              <span className="text-purple-300 font-semibold">{event.permission}</span>
            </div>
            <div>
              <span className="text-zinc-500 text-[10px] block">TARGET RESOURCE:</span>
              <span className="text-zinc-300">{event.resource}</span>
            </div>
            <div>
              <span className="text-zinc-500 text-[10px] block">PIPELINE LATENCY:</span>
              <span className="text-zinc-300">{event.latencyMs} ms</span>
            </div>
          </div>

          {/* Pipeline Checks Breakdown */}
          <div className="space-y-1.5">
            <h4 className="text-zinc-300 font-bold text-[11px] uppercase tracking-wider flex items-center gap-1.5">
              <Shield className="w-3.5 h-3.5 text-sky-400" />
              RBAC Evaluation Steps
            </h4>
            <div className="space-y-1 bg-zinc-950 p-2.5 rounded-xl border border-zinc-800">
              <div className="flex items-center justify-between p-1.5 rounded bg-zinc-900/50">
                <span className="text-zinc-400">1. Authentication Signature</span>
                <span className="text-emerald-400 font-bold">✓ PASS (Valid JWT)</span>
              </div>
              <div className="flex items-center justify-between p-1.5 rounded bg-zinc-900/50">
                <span className="text-zinc-400">2. Granular Scope ({event.permission})</span>
                <span className={event.checks.permissionCheck ? "text-emerald-400 font-bold" : "text-orange-400 font-bold"}>
                  {event.checks.permissionCheck ? "✓ GRANTED" : "✕ MISSING REQUIRED SCOPE"}
                </span>
              </div>
              <div className="flex items-center justify-between p-1.5 rounded bg-zinc-900/50">
                <span className="text-zinc-400">3. Tenant Isolation Guard ({event.tenantId})</span>
                <span className="text-emerald-400 font-bold">✓ MATCHED (Single-Tenant)</span>
              </div>
              <div className="flex items-center justify-between p-1.5 rounded bg-zinc-900/50">
                <span className="text-zinc-400">4. Project Access Membership</span>
                <span className="text-emerald-400 font-bold">✓ AUTHORIZED BOUNDARY</span>
              </div>
              <div className="flex items-center justify-between p-1.5 rounded bg-zinc-900/50">
                <span className="text-zinc-400">5. Governance Security Policy</span>
                <span className={event.checks.policyCheck ? "text-emerald-400 font-bold" : "text-orange-400 font-bold"}>
                  {event.checks.policyCheck ? "✓ COMPLIANT" : "✕ RESTRICTED BY GUARDRAIL"}
                </span>
              </div>
            </div>
          </div>

          {/* Raw JSON payload representation */}
          <div className="space-y-1.5">
            <h4 className="text-zinc-300 font-bold text-[11px] uppercase tracking-wider flex items-center gap-1.5">
              <Terminal className="w-3.5 h-3.5 text-indigo-400" />
              Cryptographic Audit Payload
            </h4>
            <pre className="p-3 bg-zinc-950 rounded-xl border border-zinc-800 text-[11px] text-zinc-300 overflow-x-auto">
              {JSON.stringify(
                {
                  eventId: event.id,
                  timestamp: event.timestamp,
                  caller: {
                    email: event.actorEmail,
                    role: event.roleId,
                    tenant: event.tenantId
                  },
                  request: {
                    action: event.action,
                    permission: event.permission,
                    resource: event.resource
                  },
                  evaluation: {
                    result: event.result,
                    httpStatusCode: event.statusCode,
                    latencyMs: event.latencyMs,
                    reason: event.reason
                  }
                },
                null,
                2
              )}
            </pre>
          </div>
        </div>

        {/* Footer */}
        <div className="px-5 py-3 bg-zinc-900 border-t border-zinc-800 flex items-center justify-end">
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 hover:text-white transition-colors text-xs font-semibold"
          >
            Close Inspector
          </button>
        </div>
      </div>
    </div>
  );
};
