/**
 * Truthful connection status for the orchestrator.
 *
 * Reports exactly three observable facts, each from a real check:
 *   - is the orchestrator process reachable (`GET /health`, unauthenticated)
 *   - do we hold an API key at all
 *   - does that key work (any authenticated call succeeding)
 *
 * The old UI had a `systemHealth` object with hardcoded "online"/"connected"
 * values for backend, database, AI model, Ollama, queue, workers and a
 * microphone, none of which were ever measured. Only the two things the
 * backend actually exposes are reported here.
 */

import React from "react";
import { CheckCircle2, KeyRound, PlugZap, RefreshCw } from "lucide-react";
import { usePolledResource, useIsAuthenticated } from "../../hooks/useApi";
import { checkHealth } from "../../services/orchestrator";
import { hasApiKey } from "../../services/apiClient";

/** Health is polled because the orchestrator offers no push channel for
 *  process liveness — not to animate anything. */
const HEALTH_POLL_MS = 15000;

export function ConnectionBanner({ onOpenSettings }: { onOpenSettings: () => void }) {
  const authed = useIsAuthenticated();
  const health = usePolledResource(
    (signal) => checkHealth(signal),
    HEALTH_POLL_MS,
    [],
    { isEmpty: () => false },
  );

  const isOnline = health.data === "online";
  const isChecking = health.status === "loading";

  return (
    <div className="flex h-11 items-center justify-between border-b border-slate-800 bg-[#0a0f1a] px-4">
      <div className="flex items-center gap-4">
        {/* Orchestrator reachability */}
        <div className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider">
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              isChecking ? "bg-slate-500" : isOnline ? "bg-emerald-500" : "bg-rose-500"
            }`}
          />
          <span className={isChecking ? "text-slate-500" : isOnline ? "text-emerald-400" : "text-rose-400"}>
            {isChecking ? "Checking orchestrator…" : isOnline ? "Orchestrator online" : "Orchestrator unreachable"}
          </span>
        </div>

        {/* Auth */}
        <div className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider">
          <KeyRound size={10} className={authed ? "text-emerald-400" : "text-amber-400"} />
          <span className={authed ? "text-emerald-400" : "text-amber-400"}>
            {authed ? "API key set" : "No API key"}
          </span>
        </div>
      </div>

      <div className="flex items-center gap-2">
        {!isOnline && !isChecking && (
          <span className="font-mono text-[10px] text-slate-500">
            start with: cargo run -p aiqa-orchestrator
          </span>
        )}
        {!hasApiKey() && (
          <button
            onClick={onOpenSettings}
            className="rounded border border-amber-700/60 bg-amber-950/40 px-2 py-1 text-[10px] font-medium text-amber-300 transition hover:bg-amber-900/40"
          >
            Add API key
          </button>
        )}
        <button
          onClick={health.reload}
          title="Re-check orchestrator health"
          className="rounded border border-slate-700 p-1 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
        >
          <RefreshCw size={11} className={health.isRefreshing ? "animate-spin" : ""} />
        </button>
      </div>
    </div>
  );
}

/** Small inline health pill for use inside cards. */
export function HealthPill({ online }: { online: boolean }) {
  return online ? (
    <span className="inline-flex items-center gap-1 text-[10px] text-emerald-400">
      <CheckCircle2 size={10} /> online
    </span>
  ) : (
    <span className="inline-flex items-center gap-1 text-[10px] text-rose-400">
      <PlugZap size={10} /> unreachable
    </span>
  );
}
