/**
 * AIQA System Settings Workspace
 *
 * Cleanup: this page used to have 7 tabs, 5 of which were pure fabricated
 * UI — General & Organization, Execution & Runners, Target Environments,
 * Quality Policies & Gates, and Alerts & Notifications all read/wrote a
 * local-only `SystemSettings` object (`useAIQA().settings`/`updateSettings`,
 * persisted to nothing but `localStorage`) with no backend behind any of
 * it; the Authentication tab mixed one real control (the orchestrator API
 * key panel) in among several more fake ones (SSO toggle/IDP picker, a
 * "Control Plane Daemon API Token" card whose "Rotate Token" button had no
 * handler at all). None of that fake surface had any test coverage —
 * `SettingsPage.test.tsx` only ever exercised the AI Intelligence panel —
 * confirming it was never real to begin with. Removed rather than left as
 * unreachable dead code, at the explicit instruction to keep only what's
 * real. What remains is exactly the two sections backed by a genuine
 * backend call: the orchestrator API key (real Bearer auth,
 * `useAuth()`/`services/aiqaApi.ts`) and AI Models & Reasoning (real
 * provider discovery/routing, `GET/PUT /api/v1/ai/*`, `ai_router.rs`).
 * The fake `SystemSettings` type/context slice this page was the only
 * consumer of is removed too — see `types.ts` and `context/AIQAContext.tsx`.
 */

import React, { useEffect, useRef, useState } from 'react';
import { Settings as SettingsIcon, Key, Sparkles, Save, Loader2, RefreshCw, AlertTriangle, CircleHelp, Plus, Trash2, Video } from 'lucide-react';
import { useAuth } from '../../context/AuthContext';
import { renderAuthState } from '../state/StateViews';
import { SectionHeader } from '../common/SectionHeader';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import { TaskRoutingChains } from '../settings/TaskRoutingChains';
import type { AiProviderInfo, AiSettings, AiStatusResponse, CustomAiProviderView, TaskProviderPreference } from '../../types/contract';

/**
 * FE-2/FE-3 — the real orchestrator credential panel: everything on this
 * tab now, since Authentication & SSO's other, fabricated controls were
 * removed (see this file's top-of-file doc comment).
 *
 * FE-3: the status card (badge/icon/title/description/actions) is no
 * longer drawn here — it's `renderAuthState` from `components/state/
 * StateViews.tsx`, the single state-to-component mapping every consumer
 * shares. This component only owns what is genuinely Settings-page-
 * specific: the key-entry input and its Save/Replace button, wired in via
 * the `onConfigure` callback.
 */
const OrchestratorAuthPanel: React.FC = () => {
  const auth = useAuth();
  const { hasKey, saveKey } = auth;
  const [inputValue, setInputValue] = useState('');
  const [showInput, setShowInput] = useState(false);
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    const trimmed = inputValue.trim();
    if (!trimmed) return;
    setSaving(true);
    try {
      await saveKey(trimmed);
      // The raw value never lingers in component state once saved — only
      // the masked form (read from storage via `maskedKey`) is ever shown
      // again (this sprint's brief §4, §6).
      setInputValue('');
      setShowInput(false);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-1.5 text-content-primary font-semibold text-xs">
        <Key className="w-3.5 h-3.5 text-violet-400" />
        AIQA Orchestrator API Key
      </div>
      <div className="text-content-secondary text-[11px] -mt-2">
        Real backend credential (Bearer token) — <code className="font-mono">services/orchestrator</code>, opaque{' '}
        <code className="font-mono">aiqa_&lt;hex&gt;</code> keys, roles viewer / operator / admin.
      </div>

      {renderAuthState(auth, { onConfigure: () => setShowInput(true) })}

      {(!hasKey || showInput) ? (
        <div className="flex items-center gap-2">
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            aria-label="AIQA Orchestrator API Key"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void handleSave();
              }
            }}
            placeholder="aiqa_..."
            className="flex-1 px-2.5 py-1.5 rounded-md bg-surface-card border border-slate-700 text-content-primary focus:border-violet-500 focus:outline-none font-mono text-xs"
          />
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={!inputValue.trim() || saving}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-semibold text-xs shadow-sm shadow-violet-950/60 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {saving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Key className="w-3.5 h-3.5" />}
            {hasKey ? 'Replace Key' : 'Save Key'}
          </button>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setShowInput(true)}
          className="text-[11px] font-medium text-violet-400 hover:text-violet-300 cursor-pointer"
        >
          Enter a different key…
        </button>
      )}
    </div>
  );
};

/** Truthful status badge helpers (brief §13/§14) — every real value the
 *  backend can produce gets its own distinct label; nothing collapses to
 *  a generic "Connected". */
function providerStatusBadge(status: AiProviderInfo['status']) {
  const map: Record<AiProviderInfo['status'], { label: string; cls: string }> = {
    HEALTHY: { label: 'Healthy', cls: 'bg-emerald-950/80 border-emerald-700 text-emerald-300' },
    AVAILABLE: { label: 'Available', cls: 'bg-yellow-950/80 border-yellow-700 text-yellow-300' },
    DISCOVERING: { label: 'Discovering…', cls: 'bg-violet-950/80 border-violet-700 text-violet-300' },
    DEGRADED: { label: 'Degraded', cls: 'bg-amber-950/80 border-amber-700 text-amber-300' },
    UNAVAILABLE: { label: 'Unavailable', cls: 'bg-slate-900 border-slate-700 text-content-secondary' },
    ERROR: { label: 'Error', cls: 'bg-rose-950/80 border-rose-700 text-rose-300' },
    NOT_CONFIGURED: { label: 'Not Configured', cls: 'bg-slate-900 border-slate-700 text-content-muted' },
  };
  const m = map[status];
  return <span className={`px-2 py-0.5 rounded text-[10px] font-mono font-bold border ${m.cls}`}>{m.label}</span>;
}

/**
 * Central AI Provider & Model Intelligence Layer — real settings panel.
 *
 * Every status/health value shown here is read verbatim from
 * `GET /api/v1/ai/status` (`ai_router.rs`'s real discovery snapshot) —
 * nothing is rendered as "Healthy"/"Connected" unless the backend itself
 * reported that real value.
 */
const AiIntelligencePanel: React.FC = () => {
  const [status, setStatus] = useState<AiStatusResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // Local, uncommitted draft of the mode/provider/model picker — only
  // sent to the backend when the user explicitly saves, so an in-progress
  // selection is never mistaken for the active, persisted configuration.
  const [draftMode, setDraftMode] = useState<AiSettings['mode']>('AUTO');
  const [draftProvider, setDraftProvider] = useState<string>('');
  const [draftModel, setDraftModel] = useState<string>('');
  const [draftFallback, setDraftFallback] = useState<boolean>(true);
  // The per-task overrides (incl. each task's model chain) travel with every save — the backend replaces the
  // whole settings row, so leaving them out would silently erase them.
  const [draftTaskOverrides, setDraftTaskOverrides] = useState<Record<string, TaskProviderPreference>>({});

  // Real, user-added providers (Settings UI -> `custom_ai_providers` DB
  // table) — a separate list from `status.providers` (which already shows
  // these generically too, once added; this list exists so each one has
  // its own real Delete control, which built-in env-var providers don't).
  const [customProviders, setCustomProviders] = useState<CustomAiProviderView[]>([]);
  const [newProviderName, setNewProviderName] = useState('');
  const [newProviderBaseUrl, setNewProviderBaseUrl] = useState('');
  const [newProviderApiKey, setNewProviderApiKey] = useState('');
  const [newProviderInputPrice, setNewProviderInputPrice] = useState('');
  const [newProviderOutputPrice, setNewProviderOutputPrice] = useState('');
  const [addingProvider, setAddingProvider] = useState(false);
  const [addProviderError, setAddProviderError] = useState<string | null>(null);
  const [deletingProviderId, setDeletingProviderId] = useState<string | null>(null);

  const loadStatus = async () => {
    try {
      const s = await aiqaApi.ai.status();
      setStatus(s);
      setDraftMode(s.settings.mode);
      setDraftProvider(s.settings.preferred_provider ?? '');
      setDraftModel(s.settings.preferred_model ?? '');
      setDraftFallback(s.settings.fallback_enabled);
      setDraftTaskOverrides(s.settings.task_overrides ?? {});
      setError(null);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to load AI status.');
    } finally {
      setLoading(false);
    }
  };

  const loadCustomProviders = async () => {
    try {
      setCustomProviders(await aiqaApi.ai.customProviders.list());
    } catch {
      // Non-fatal: the main status panel above still loads/works even if
      // this real-but-secondary list fails to fetch.
    }
  };

  useEffect(() => {
    void loadStatus();
    void loadCustomProviders();
  }, []);

  // Both prices or neither (the backend enforces the same); each zero or more.
  const inputPrice = newProviderInputPrice.trim();
  const outputPrice = newProviderOutputPrice.trim();
  const priceProblem =
    (inputPrice === '') !== (outputPrice === '')
      ? 'Enter both prices, or leave both empty.'
      : [inputPrice, outputPrice].some((p) => p !== '' && !(Number(p) >= 0))
        ? 'Prices must be zero or more.'
        : null;

  const handleAddCustomProvider = async () => {
    setAddingProvider(true);
    setAddProviderError(null);
    try {
      await aiqaApi.ai.customProviders.create({
        name: newProviderName.trim(),
        base_url: newProviderBaseUrl.trim(),
        api_key: newProviderApiKey.trim(),
        ...(inputPrice !== '' ? { input_usd_per_1k: Number(inputPrice), output_usd_per_1k: Number(outputPrice) } : {}),
      });
      // The raw key never lingers in component state once saved — same
      // real-secret discipline `OrchestratorAuthPanel` above already
      // holds itself to.
      setNewProviderName('');
      setNewProviderBaseUrl('');
      setNewProviderApiKey('');
      setNewProviderInputPrice('');
      setNewProviderOutputPrice('');
      await loadCustomProviders();
      // Force a real, immediate discovery pass (same call "Scan Providers"
      // makes) so the new provider shows up in "Discovered Providers"
      // right away, not just after the next TTL-gated refresh.
      await aiqaApi.ai.refresh();
      await loadStatus();
    } catch (err) {
      setAddProviderError(err instanceof ApiError ? err.message : 'Failed to add custom provider.');
    } finally {
      setAddingProvider(false);
    }
  };

  const handleDeleteCustomProvider = async (id: string) => {
    setDeletingProviderId(id);
    try {
      await aiqaApi.ai.customProviders.delete(id);
      await loadCustomProviders();
      await aiqaApi.ai.refresh();
      await loadStatus();
    } catch (err) {
      setAddProviderError(err instanceof ApiError ? err.message : 'Failed to remove custom provider.');
    } finally {
      setDeletingProviderId(null);
    }
  };

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await aiqaApi.ai.refresh();
      await loadStatus();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to refresh providers.');
    } finally {
      setRefreshing(false);
    }
  };

  const handleSaveSettings = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      const saved = await aiqaApi.ai.updateSettings({
        mode: draftMode,
        preferred_provider: draftMode === 'MANUAL' ? draftProvider || null : null,
        preferred_model: draftMode === 'MANUAL' ? draftModel || null : null,
        fallback_enabled: draftFallback,
        ...(Object.keys(draftTaskOverrides).length > 0 ? { task_overrides: draftTaskOverrides } : {}),
      });
      setStatus((prev) => (prev ? { ...prev, settings: saved } : prev));
      // "Active Model" above is worked out by the backend from these settings, so ask it again.
      try {
        const fresh = await aiqaApi.ai.status();
        setStatus((prev) => (prev ? { ...prev, routing_preview: fresh.routing_preview } : prev));
      } catch {
        // The saved settings are already shown; the preview catches up on the next scan.
      }
    } catch (err) {
      setSaveError(err instanceof ApiError ? err.message : 'Failed to save AI settings.');
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center gap-2 text-content-secondary text-xs">
        <Loader2 className="w-4 h-4 animate-spin" />
        <span>Loading AI infrastructure status…</span>
      </div>
    );
  }

  if (error || !status) {
    return (
      <div className="p-4 rounded-lg bg-rose-950/40 border border-rose-800/70 text-rose-300 text-xs flex items-start gap-2">
        <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" />
        <span>{error || 'AI status unavailable.'}</span>
      </div>
    );
  }

  const anyHealthy = status.providers.some((p) => p.status === 'HEALTHY');
  const overallStatus: 'READY' | 'PARTIAL' | 'DEGRADED' = anyHealthy ? 'READY' : status.routing_preview ? 'PARTIAL' : 'DEGRADED';
  const overallCopy = {
    READY: { label: 'Ready', dot: 'bg-emerald-400', text: 'text-emerald-300' },
    PARTIAL: { label: 'Partial', dot: 'bg-amber-400', text: 'text-amber-300' },
    DEGRADED: { label: 'Degraded — No AI model currently available', dot: 'bg-rose-400', text: 'text-rose-300' },
  }[overallStatus];

  const selectedProvider = status.providers.find((p) => p.id === draftProvider);
  // The name a provider was given, not its internal id (a custom one is `custom-ai-<uuid>`).
  const previewProviderId = status.routing_preview?.provider_id;
  const previewProviderName = status.providers.find((p) => p.id === previewProviderId)?.name ?? previewProviderId;

  return (
    <div id="ai-intelligence-panel" className="space-y-3">
      <div className="pb-2 border-b border-border-default">
        <h2 className="text-xs font-bold text-content-primary uppercase font-mono">AI Intelligence</h2>
        <p className="text-content-secondary text-xs mt-0.5">Provider discovery, model routing, and generation health — all real, live-checked state.</p>
      </div>

      {/* Overall status */}
      <div className="p-3 rounded-lg bg-surface-page border border-border-default flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2">
          <span className={`w-2 h-2 rounded-full ${overallCopy.dot}`} />
          <span className={`font-mono font-bold text-xs ${overallCopy.text}`}>{overallCopy.label}</span>
        </div>
        <div className="flex items-center gap-2 text-[11px] font-mono text-content-secondary">
          <span>{status.providers.length} provider(s) checked</span>
          <button
            type="button"
            id="btn-refresh-ai-providers"
            onClick={() => void handleRefresh()}
            disabled={refreshing}
            className="flex items-center gap-1 px-2 py-1 rounded bg-surface-card border border-slate-700 hover:border-violet-600 text-content-secondary hover:text-violet-300 cursor-pointer disabled:opacity-50"
          >
            <RefreshCw className={`w-3 h-3 ${refreshing ? 'animate-spin' : ''}`} />
            <span>Scan Providers</span>
          </button>
        </div>
      </div>

      {/* Active routing preview (real, from AiInfrastructure::route_among) */}
      {status.routing_preview ? (
        <div className="p-3 rounded-lg bg-surface-page border border-border-default space-y-1 text-xs font-mono">
          <div className="flex justify-between">
            <span className="text-content-muted">Active Provider</span>
            <span className="text-content-primary font-semibold">{previewProviderName}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-content-muted">Active Model</span>
            <span className="text-content-primary font-semibold">{status.routing_preview.model_id}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-content-muted">Reason</span>
            <span className="text-content-secondary">{status.routing_preview.reason}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-content-muted">Routing Score</span>
            <span className="text-content-secondary" title="A documented routing heuristic, not a benchmarked accuracy score.">
              {status.routing_preview.routing_score.toFixed(1)}
            </span>
          </div>
        </div>
      ) : (
        <div className="p-3 rounded-lg bg-slate-900/60 border border-border-default text-[11px] font-mono text-content-secondary flex items-start gap-2">
          <CircleHelp className="w-3.5 h-3.5 shrink-0 mt-0.5" />
          <span>No AI model currently available for Test Case Generation. Test generation will use the local template pipeline.</span>
        </div>
      )}

      {/* Discovered providers */}
      <div className="space-y-1.5">
        <span className="text-[10px] font-mono uppercase tracking-wider text-content-muted font-semibold">Discovered Providers</span>
        {status.providers.map((p) => (
          <div key={p.id} className="p-2.5 rounded-md bg-surface-page border border-border-default flex items-center justify-between flex-wrap gap-1.5">
            <div className="flex items-center gap-2">
              <span className="font-mono text-xs font-semibold text-content-primary">{p.name}</span>
              <span className="text-[10px] font-mono text-content-muted">{p.provider_type}</span>
              {p.models.length > 0 && <span className="text-[10px] font-mono text-content-muted">{p.models.length} model(s)</span>}
              <span
                className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-emerald-950/40 border border-emerald-800/60 text-emerald-300"
                title={
                  p.pricing.kind === 'UNMETERED'
                    ? 'Self-hosted — no per-token bill'
                    : `$${p.pricing.input_usd_per_1k}/1K tokens in · $${p.pricing.output_usd_per_1k}/1K tokens out`
                }
              >
                {p.pricing.kind === 'UNMETERED'
                  ? 'Free'
                  : `$${p.pricing.input_usd_per_1k}/$${p.pricing.output_usd_per_1k} per 1K`}
              </span>
            </div>
            {providerStatusBadge(p.status)}
          </div>
        ))}
      </div>

      {/* Custom providers you've added — separate from the read-only
          "Discovered Providers" list above (which already shows these too,
          generically) because only a provider added here can be removed
          again; Ollama/MJ Gateway/Groq/Gemini are env-var-configured and
          have no real "delete" this UI could perform. */}
      <div id="custom-ai-providers-section" className="p-3 rounded-lg bg-surface-page border border-border-default space-y-2.5">
        <span className="text-[10px] font-mono uppercase tracking-wider text-content-muted font-semibold">Your Custom Providers</span>

        {customProviders.length > 0 && (
          <div className="space-y-1.5">
            {customProviders.map((p) => (
              <div key={p.id} className="p-2 rounded-md bg-surface-card border border-border-default flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="font-mono text-xs font-semibold text-content-primary truncate">{p.name}</div>
                  <div className="font-mono text-[10px] text-content-muted truncate">{p.base_url}</div>
                  <div className="font-mono text-[10px] text-content-muted">
                    {p.input_usd_per_1k != null && p.output_usd_per_1k != null
                      ? `$${p.input_usd_per_1k} in / $${p.output_usd_per_1k} out per 1K tokens`
                      : 'No price entered — its cost shows as Free'}
                  </div>
                  {p.api_key_env && (
                    <div className={`font-mono text-[10px] ${p.api_key_env_set === false ? 'text-amber-300' : 'text-content-muted'}`}>
                      Key from env:{p.api_key_env}
                      {p.api_key_env_set === false && ' — not set on the server yet (add it to the server environment and restart)'}
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  id={`btn-delete-custom-provider-${p.id}`}
                  onClick={() => void handleDeleteCustomProvider(p.id)}
                  disabled={deletingProviderId === p.id}
                  title="Remove this custom provider"
                  className="p-1.5 rounded text-content-muted hover:text-rose-300 hover:bg-rose-950/40 cursor-pointer disabled:opacity-50 shrink-0"
                >
                  {deletingProviderId === p.id ? (
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  ) : (
                    <Trash2 className="w-3.5 h-3.5" />
                  )}
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
          <input
            type="text"
            id="new-custom-provider-name"
            aria-label="Provider name"
            value={newProviderName}
            onChange={(e) => setNewProviderName(e.target.value)}
            placeholder="Name (e.g. My Provider)"
            className="px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary placeholder:text-content-muted focus:outline-none focus:border-violet-500"
          />
          <input
            type="text"
            id="new-custom-provider-base-url"
            aria-label="Provider base URL"
            value={newProviderBaseUrl}
            onChange={(e) => setNewProviderBaseUrl(e.target.value)}
            placeholder="Base URL (https://...)"
            className="px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary placeholder:text-content-muted focus:outline-none focus:border-violet-500"
          />
          <input
            type="password"
            id="new-custom-provider-api-key"
            aria-label="Provider API key"
            value={newProviderApiKey}
            onChange={(e) => setNewProviderApiKey(e.target.value)}
            placeholder="API key, or env:NAME"
            className="px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary placeholder:text-content-muted focus:outline-none focus:border-violet-500"
          />
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <input
            type="number"
            min="0"
            step="any"
            id="new-custom-provider-input-price"
            aria-label="Input price, dollars per 1K tokens"
            value={newProviderInputPrice}
            onChange={(e) => setNewProviderInputPrice(e.target.value)}
            placeholder="Input price, $ per 1K tokens (optional)"
            className="px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary placeholder:text-content-muted focus:outline-none focus:border-violet-500"
          />
          <input
            type="number"
            min="0"
            step="any"
            id="new-custom-provider-output-price"
            aria-label="Output price, dollars per 1K tokens"
            value={newProviderOutputPrice}
            onChange={(e) => setNewProviderOutputPrice(e.target.value)}
            placeholder="Output price, $ per 1K tokens (optional)"
            className="px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary placeholder:text-content-muted focus:outline-none focus:border-violet-500"
          />
        </div>
        {priceProblem && <p className="text-[11px] font-mono text-amber-300">{priceProblem}</p>}
        <p className="text-[10px] font-mono text-content-muted">
          Price: copy it from the provider's own price list so a run through it shows a real cost. With no price its
          cost shows as Free — it is not looked up for you. Key: paste it, or write <code>env:NAME</code> (for example{' '}
          <code>env:OPENAI_API_KEY</code>) to read it from an environment variable on the server, so the key itself is
          never stored here.
        </p>
        <p className="text-[10px] font-mono text-content-muted">
          Must be a real, OpenAI-compatible endpoint (<code>{'{base_url}'}/v1/models</code> and{' '}
          <code>{'{base_url}'}/v1/chat/completions</code>) — the same convention Groq and most hosted providers already use.
        </p>
        {addProviderError && (
          <p className="text-[11px] font-mono text-rose-300 flex items-start gap-1.5">
            <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
            <span>{addProviderError}</span>
          </p>
        )}
        <button
          type="button"
          id="btn-add-custom-ai-provider"
          onClick={() => void handleAddCustomProvider()}
          disabled={addingProvider || !newProviderName.trim() || !newProviderBaseUrl.trim() || !newProviderApiKey.trim() || priceProblem !== null}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-mono text-xs font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {addingProvider ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Plus className="w-3.5 h-3.5" />}
          <span>Add Custom Provider</span>
        </button>
      </div>

      {/* Mode */}
      <div className="p-3 rounded-lg bg-surface-page border border-border-default space-y-2.5">
        <div>
          <label htmlFor="settings-ai-mode" className="block text-xs font-bold font-mono text-content-secondary mb-1">AI Mode</label>
          <select
            id="settings-ai-mode"
            value={draftMode}
            onChange={(e) => setDraftMode(e.target.value as AiSettings['mode'])}
            className="w-full px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary focus:outline-none focus:border-violet-500 cursor-pointer"
          >
            <option value="AUTO">AUTO — best available model is selected automatically</option>
            <option value="MANUAL">MANUAL — choose a specific provider/model</option>
          </select>
        </div>

        {draftMode === 'MANUAL' && (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
            <div>
              <label htmlFor="settings-ai-provider" className="block text-[10px] font-mono text-content-secondary mb-1">Provider</label>
              <select
                id="settings-ai-provider"
                value={draftProvider}
                onChange={(e) => {
                  setDraftProvider(e.target.value);
                  setDraftModel('');
                }}
                className="w-full px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary focus:outline-none focus:border-violet-500 cursor-pointer"
              >
                <option value="">Select a provider…</option>
                {status.providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label htmlFor="settings-ai-model" className="block text-[10px] font-mono text-content-secondary mb-1">Model</label>
              <select
                id="settings-ai-model"
                value={draftModel}
                onChange={(e) => setDraftModel(e.target.value)}
                disabled={!selectedProvider}
                className="w-full px-2.5 py-1.5 rounded bg-surface-card border border-slate-700 text-xs font-mono text-content-primary focus:outline-none focus:border-violet-500 cursor-pointer disabled:opacity-50"
              >
                <option value="">Select a model…</option>
                {selectedProvider?.models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.display_name}
                  </option>
                ))}
              </select>
            </div>
          </div>
        )}

        <label className="flex items-center gap-2 text-[11px] font-mono text-content-secondary cursor-pointer">
          <input type="checkbox" checked={draftFallback} onChange={(e) => setDraftFallback(e.target.checked)} className="accent-violet-500" />
          <span>Fall back to another available model if the selected one fails</span>
        </label>

        <TaskRoutingChains providers={status.providers} overrides={draftTaskOverrides} onChange={setDraftTaskOverrides} />

        {saveError && <p className="text-[11px] font-mono text-rose-300">{saveError}</p>}

        <button
          type="button"
          id="btn-save-ai-settings"
          onClick={() => void handleSaveSettings()}
          disabled={saving || (draftMode === 'MANUAL' && (!draftProvider || !draftModel))}
          className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-mono text-xs font-bold cursor-pointer"
        >
          {saving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Save className="w-3.5 h-3.5" />}
          <span>Save AI Settings</span>
        </button>
      </div>

      <p className="text-[10px] font-mono text-content-muted">
        Settings are saved to the real orchestrator backend and take effect on the next AI generation request.
      </p>
    </div>
  );
};

/**
 * Test Execution — real, live-toggleable browser visibility.
 *
 * `GET`/`PUT /api/v1/execution-settings` (`state.rs::AppState::show_browser_window`,
 * `executor.rs::PlaywrightExecutor`): whether a Playwright run's real Chromium browser window is
 * shown on screen while it drives a test, instead of running headless — headless (off) is the
 * default every earlier release always used, and stays the default here unless this is turned
 * on. The very next Playwright run after a change reads the new value; nothing needs a restart.
 */
const TestExecutionPanel: React.FC = () => {
  const [value, setValue] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const s = await aiqaApi.executionSettings.get();
        if (!cancelled) setValue(s.show_browser_window);
      } catch (err) {
        if (!cancelled) setError(err instanceof ApiError ? err.message : 'Failed to load execution settings.');
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const handleToggle = async () => {
    if (value === null || saving) return;
    const next = !value;
    setSaving(true);
    setError(null);
    try {
      const saved = await aiqaApi.executionSettings.update({ show_browser_window: next });
      setValue(saved.show_browser_window);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to save the execution setting.');
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center gap-2 text-content-secondary text-xs">
        <Loader2 className="w-4 h-4 animate-spin" />
        <span>Loading execution settings…</span>
      </div>
    );
  }

  return (
    <div id="test-execution-panel" className="space-y-3">
      <div className="pb-2 border-b border-border-default">
        <h2 className="text-xs font-bold text-content-primary uppercase font-mono">Test Execution</h2>
        <p className="text-content-secondary text-xs mt-0.5">Whether a real browser window is shown while a Playwright test runs</p>
      </div>

      <div className="flex items-center justify-between gap-3 p-3 rounded-lg bg-surface-page border border-border-default">
        <div className="min-w-0 pr-2">
          <p className="text-content-primary font-semibold text-xs">Show the browser window while tests run</p>
          <p className="text-content-secondary text-[11px] mt-1">
            Off by default — tests run headless (invisibly), which is usually a little faster. Turn this on only when
            you want to watch a run live; turn it back off when you don't, since a visible window needs a real screen
            to open on.
          </p>
        </div>
        <button
          type="button"
          id="btn-toggle-show-browser-window"
          role="switch"
          aria-checked={value ?? false}
          aria-label="Show the browser window while tests run"
          onClick={() => void handleToggle()}
          disabled={saving}
          className={`shrink-0 relative w-9 h-5 rounded-full transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed ${
            value ? 'bg-violet-600' : 'bg-slate-700'
          }`}
        >
          <span
            className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
              value ? 'translate-x-4' : 'translate-x-0'
            }`}
          />
        </button>
      </div>

      <p className="text-[10px] font-mono text-content-muted flex items-start gap-1.5">
        <CircleHelp className="w-3 h-3 shrink-0 mt-0.5" />
        <span>Saved to the real orchestrator backend and read by the very next Playwright run — no restart needed.</span>
      </p>

      {saving && (
        <p className="text-[11px] text-violet-300 flex items-center gap-1.5">
          <Loader2 className="w-3 h-3 animate-spin" />
          Saving…
        </p>
      )}
      {error && (
        <p id="test-execution-error" role="alert" className="text-[11px] text-rose-300 flex items-start gap-1.5">
          <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
          <span>{error}</span>
        </p>
      )}
    </div>
  );
};

type SettingsTab = 'authentication' | 'aiModels' | 'testExecution';

export const SettingsPage: React.FC = () => {
  const [activeTab, setActiveTab] = useState<SettingsTab>('authentication');

  const tabs: Array<{ id: SettingsTab; label: string; icon: React.ComponentType<{ className?: string }> }> = [
    { id: 'authentication', label: 'Authentication & SSO', icon: Key },
    { id: 'aiModels', label: 'AI Models & Reasoning', icon: Sparkles },
    { id: 'testExecution', label: 'Test Execution', icon: Video },
  ];

  // WAI-ARIA tab pattern (APG): one tab stop for the whole strip, Arrow/Home/End
  // move selection and focus together, so the relationship between the strip
  // and the panel it swaps is announced rather than implied by styling.
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const handleTabKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const current = tabs.findIndex((t) => t.id === activeTab);
    if (current < 0) return;
    let next = current;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (current + 1) % tabs.length;
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (current - 1 + tabs.length) % tabs.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = tabs.length - 1;
    else return;
    e.preventDefault();
    setActiveTab(tabs[next].id);
    tabRefs.current[tabs[next].id]?.focus();
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="System Settings & Governance"
        subtitle="Real orchestrator credentials and AI provider/model routing — every other section this page used to show was unwired mockup and has been removed"
        badge="GOVERNANCE"
        icon={SettingsIcon}
      />

      {/* Main Settings Grid: Tabs on left, Content on right */}
      <div className="grid grid-cols-1 md:grid-cols-12 gap-4">
        {/* Settings Navigation Tabs (4 cols) */}
        <div
          role="tablist"
          aria-label="Configuration Sections"
          onKeyDown={handleTabKeyDown}
          className="md:col-span-4 rounded-lg border border-border-default bg-surface-card p-2.5 space-y-1"
        >
          <div className="text-[10px] font-mono uppercase tracking-wider text-content-muted px-2.5 py-1.5 font-bold">
            Configuration Sections
          </div>
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;

            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                id={`settings-tab-${tab.id}`}
                aria-selected={isActive}
                aria-controls={`settings-panel-${tab.id}`}
                tabIndex={isActive ? 0 : -1}
                ref={(el) => {
                  tabRefs.current[tab.id] = el;
                }}
                onClick={() => setActiveTab(tab.id)}
                className={`w-full flex items-center gap-2.5 px-2.5 py-2 rounded-md text-xs font-medium transition-all text-left cursor-pointer ${
                  isActive
                    ? 'bg-violet-950/60 border border-violet-800/80 text-violet-300 font-semibold shadow-sm'
                    : 'text-content-secondary hover:text-content-primary hover:bg-slate-800/40 border border-transparent'
                }`}
              >
                <Icon className={`w-3.5 h-3.5 shrink-0 ${isActive ? 'text-violet-400' : 'text-content-muted'}`} />
                <span className="truncate">{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Settings Body (8 cols) */}
        <div className="md:col-span-8 rounded-lg border border-border-default bg-surface-card p-4 space-y-4 text-xs">
          {activeTab === 'authentication' && (
            <div role="tabpanel" id="settings-panel-authentication" aria-labelledby="settings-tab-authentication" className="space-y-3">
              <div className="pb-2 border-b border-border-default">
                <h2 className="text-xs font-bold text-content-primary uppercase font-mono">Authentication</h2>
                <p className="text-content-secondary text-xs mt-0.5">The real orchestrator API key this frontend authenticates with</p>
              </div>
              <OrchestratorAuthPanel />
            </div>
          )}

          {activeTab === 'aiModels' && (
            <div role="tabpanel" id="settings-panel-aiModels" aria-labelledby="settings-tab-aiModels">
              <AiIntelligencePanel />
            </div>
          )}

          {activeTab === 'testExecution' && (
            <div role="tabpanel" id="settings-panel-testExecution" aria-labelledby="settings-tab-testExecution">
              <TestExecutionPanel />
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
