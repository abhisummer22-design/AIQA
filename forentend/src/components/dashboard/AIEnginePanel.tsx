/**
 * AI Engine — the real neural/model engine behind this project, live.
 *
 * Replaces `AIQualityCoreOrb`, which was decorative: its "AIQA QUALITY
 * CORE", "Autonomous Verification Engine", "ACTIVE INFERENCE" and "CORE
 * STABLE" labels were derived entirely from `ProjectStatus` (healthy /
 * running), not from any model, inference, or engine. Nothing was actually
 * running inference when it said so, and no such "verification engine"
 * exists in this codebase — so every word of it was a claim with no backing
 * fact.
 *
 * What genuinely exists is the Central AI Provider & Model Intelligence
 * Layer (`services/orchestrator/src/ai_router.rs`), and this panel shows it
 * as it really is, from `GET /api/v1/ai/status`:
 *
 *   - real discovered providers, with real `reachable`/`status` health
 *   - the real model that would be routed to right now, with the real
 *     heuristic score and the real stated reason
 *   - that model's real capability profile, at the real evidenced level
 *     (`STRONG` / `BASIC` / `UNKNOWN` / `NONE`) — `UNKNOWN` is rendered as
 *     unknown, never upgraded to a guess
 *
 * On embeddings specifically: the capability is part of the real routing
 * model (`AiTask::Embedding`, `RequiredCapability::Embedding` are real code
 * paths), but there is no embedding endpoint and no vector store in this
 * backend — `ai_router.rs` says so itself ("embeddings are a future
 * endpoint"), and the discovered model reports `embedding: NONE`. This
 * panel therefore shows embedding capability truthfully as unavailable
 * rather than presenting a vector-search engine that does not exist.
 */

import React from 'react';
import { AlertTriangle, BrainCircuit, CircleSlash, Info, Loader2 } from 'lucide-react';
import { AIEngineOrb } from './AIEngineOrb';
import type { AiCapabilityLevel, AiModelDefinition, AiStatusResponse } from '../../types/contract';

export interface AIEnginePanelProps {
  status: AiStatusResponse | null;
  loading: boolean;
  /** Real load failure message, if the last fetch failed. */
  error: string | null;
  onRetry: () => void;
}

const levelStyles: Record<AiCapabilityLevel, string> = {
  STRONG: 'text-emerald-300 bg-emerald-950/70 border-emerald-800',
  BASIC: 'text-cyan-300 bg-cyan-950/70 border-cyan-800',
  UNKNOWN: 'text-slate-400 bg-slate-800/70 border-slate-700',
  NONE: 'text-slate-500 bg-slate-900 border-slate-800',
};

const providerStatusStyles: Record<string, string> = {
  HEALTHY: 'text-emerald-300 bg-emerald-950/70 border-emerald-800',
  AVAILABLE: 'text-emerald-300 bg-emerald-950/70 border-emerald-800',
  DEGRADED: 'text-amber-300 bg-amber-950/70 border-amber-800',
  NOT_CONFIGURED: 'text-slate-400 bg-slate-800/70 border-slate-700',
  UNAVAILABLE: 'text-rose-300 bg-rose-950/70 border-rose-800',
  ERROR: 'text-rose-300 bg-rose-950/70 border-rose-800',
};

/** Capability keys in a fixed, real order — every one of these is a real
 *  field on `ai_router.rs::ModelCapabilities`, so nothing here is a label
 *  without a backing value. */
const CAPABILITY_ORDER: Array<{ key: keyof AiModelDefinition['capabilities']; label: string }> = [
  { key: 'text_generation', label: 'text gen' },
  { key: 'structured_output', label: 'structured' },
  { key: 'reasoning', label: 'reasoning' },
  { key: 'tool_use', label: 'tool use' },
  { key: 'long_context', label: 'long ctx' },
  { key: 'vision', label: 'vision' },
  { key: 'embedding', label: 'embedding' },
];

const Pill: React.FC<{ label: string; className?: string }> = ({ label, className }) => (
  <span
    className={`inline-flex items-center px-1.5 py-0.5 rounded border text-[9px] font-mono font-bold uppercase tracking-wide shrink-0 ${
      className ?? 'text-slate-400 bg-slate-800/60 border-slate-700'
    }`}
  >
    {label}
  </span>
);

export const AIEnginePanel: React.FC<AIEnginePanelProps> = ({ status, loading, error, onRetry }) => {
  const routing = status?.routing_preview ?? null;
  // The real model object behind the routing decision, so its real
  // capability profile can be shown rather than restated from the id.
  const activeModel: AiModelDefinition | null =
    routing && status
      ? status.providers
          .find((p) => p.id === routing.provider_id)
          ?.models.find((m) => m.id === routing.model_id) ?? null
      : null;

  return (
    <div
      id="ai-engine-panel"
      className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 h-full flex flex-col space-y-3"
    >
      <div className="flex items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2 min-w-0">
          <BrainCircuit className="w-4 h-4 text-cyan-400 shrink-0" />
          <div className="min-w-0">
            <h3 className="text-xs font-bold text-slate-100 font-mono uppercase tracking-wider">AI Engine</h3>
            <p className="text-[10px] text-slate-500">Real model routing and capabilities</p>
          </div>
        </div>
        {loading && <Loader2 className="w-3.5 h-3.5 animate-spin text-indigo-400 shrink-0" />}
      </div>

      {error ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center gap-2 py-4">
          <AlertTriangle className="w-5 h-5 text-rose-400" />
          <p className="text-[11px] text-slate-400 max-w-[240px]">{error}</p>
          <button
            type="button"
            id="btn-ai-engine-retry"
            onClick={onRetry}
            className="px-2.5 py-1 rounded bg-[#020617] border border-slate-700 hover:border-indigo-600 text-slate-300 hover:text-indigo-300 font-mono text-[11px] cursor-pointer"
          >
            Retry
          </button>
        </div>
      ) : !status ? (
        <div className="flex-1 flex items-center justify-center gap-2 text-xs font-mono text-slate-400">
          <Loader2 className="w-4 h-4 animate-spin text-indigo-400" /> Loading real engine state…
        </div>
      ) : (
        <>
          {/* Particle orb — a visual readout of the same real values shown
              below it (capability levels, routing score, whether a route
              exists at all). It adds no claim of its own: it spins only
              while a model is genuinely routable, and a `NONE` capability
              stays visible as a hollow particle rather than disappearing. */}
          <AIEngineOrb
            model={activeModel}
            routingScore={routing?.routing_score ?? null}
            providers={status.providers}
            activeProviderId={routing?.provider_id ?? null}
          />

          {/* Active route — the real decision, or an honest absence. */}
          {routing ? (
            <div className="rounded-md bg-[#020617] border border-indigo-800/70 p-3 space-y-1.5">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <span className="text-xs font-mono font-bold text-indigo-300 truncate">
                  {routing.provider_id} / {routing.model_id}
                </span>
                <Pill label={`${status.settings.mode} mode`} className="text-indigo-300 bg-indigo-950/60 border-indigo-800" />
              </div>
              <p className="text-[10px] text-slate-400 leading-snug">{routing.reason}</p>
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-[10px] font-mono text-slate-500">score {routing.routing_score.toFixed(2)}</span>
                {activeModel?.parameter_size && (
                  <span className="text-[10px] font-mono text-slate-500">{activeModel.parameter_size}</span>
                )}
                <Pill
                  label={status.settings.fallback_enabled ? 'fallback on' : 'fallback off'}
                  className={
                    status.settings.fallback_enabled
                      ? 'text-emerald-300 bg-emerald-950/60 border-emerald-800'
                      : 'text-slate-400 bg-slate-800/60 border-slate-700'
                  }
                />
              </div>
            </div>
          ) : (
            <div className="rounded-md bg-[#020617] border border-slate-800 p-3 flex items-start gap-2">
              <CircleSlash className="w-3.5 h-3.5 text-slate-500 shrink-0 mt-0.5" />
              <p className="text-[11px] text-slate-400">
                No compatible model is available for test-case generation right now. Nothing is shown in its
                place.
              </p>
            </div>
          )}

          {/* Real capability profile of the model that would actually run. */}
          {activeModel && (
            <div className="space-y-1.5">
              <p className="text-[10px] font-mono uppercase tracking-wider text-slate-500">
                Real capability profile
              </p>
              <div className="grid grid-cols-2 gap-1">
                {CAPABILITY_ORDER.map(({ key, label }) => {
                  const level = activeModel.capabilities[key];
                  return (
                    <div
                      key={key}
                      className="flex items-center justify-between gap-1.5 px-2 py-1 rounded bg-[#020617] border border-slate-800"
                    >
                      <span className="text-[10px] font-mono text-slate-400 truncate">{label}</span>
                      <Pill
                        label={level ?? 'UNKNOWN'}
                        className={levelStyles[(level ?? 'UNKNOWN') as AiCapabilityLevel]}
                      />
                    </div>
                  );
                })}
              </div>
              <p className="text-[10px] font-mono text-slate-500 flex items-start gap-1.5">
                <Info className="w-3 h-3 shrink-0 mt-0.5" />
                <span>
                  Levels come from real discovery. <span className="font-mono">UNKNOWN</span> means the source
                  gave no signal — it is never upgraded to a guess. Embeddings/vector search have no endpoint in
                  this backend yet.
                </span>
              </p>
            </div>
          )}

          {/* Real provider health. */}
          <div className="space-y-1 mt-auto">
            <p className="text-[10px] font-mono uppercase tracking-wider text-slate-500">Providers</p>
            {status.providers.map((p) => (
              <div
                key={p.id}
                className="flex items-center justify-between gap-2 px-2 py-1 rounded bg-[#020617] border border-slate-800"
              >
                <span className="text-[10px] font-mono text-slate-300 truncate">
                  {p.name}
                  <span className="text-slate-600"> · {p.models.length} model{p.models.length === 1 ? '' : 's'}</span>
                </span>
                <Pill label={p.status} className={providerStatusStyles[p.status]} />
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
};
