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

import React, { useEffect, useState } from 'react';
import { Settings as SettingsIcon, Key, Sparkles, Save, Loader2, RefreshCw, AlertTriangle, CircleHelp, Plus, Trash2 } from 'lucide-react';
import { useAuth } from '../../context/AuthContext';
import { renderAuthState } from '../state/StateViews';
import { SectionHeader } from '../common/SectionHeader';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { AiProviderInfo, AiSettings, AiStatusResponse, CustomAiProviderView } from '../../types/contract';

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
      <div className="flex items-center gap-1.5 text-slate-200 font-semibold text-xs">
        <Key className="w-3.5 h-3.5 text-indigo-400" />
        AIQA Orchestrator API Key
      </div>
      <div className="text-slate-400 text-[11px] -mt-2">
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
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void handleSave();
              }
            }}
            placeholder="aiqa_..."
            className="flex-1 px-2.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
          />
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={!inputValue.trim() || saving}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {saving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Key className="w-3.5 h-3.5" />}
            {hasKey ? 'Replace Key' : 'Save Key'}
          </button>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setShowInput(true)}
          className="text-[11px] font-medium text-indigo-400 hover:text-indigo-300 cursor-pointer"
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
    AVAILABLE: { label: 'Available', cls: 'bg-cyan-950/80 border-cyan-700 text-cyan-300' },
    DISCOVERING: { label: 'Discovering…', cls: 'bg-indigo-950/80 border-indigo-700 text-indigo-300' },
    DEGRADED: { label: 'Degraded', cls: 'bg-amber-950/80 border-amber-700 text-amber-300' },
    UNAVAILABLE: { label: 'Unavailable', cls: 'bg-slate-900 border-slate-700 text-slate-400' },
    ERROR: { label: 'Error', cls: 'bg-rose-950/80 border-rose-700 text-rose-300' },
    NOT_CONFIGURED: { label: 'Not Configured', cls: 'bg-slate-900 border-slate-700 text-slate-500' },
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

  // Real, user-added providers (Settings UI -> `custom_ai_providers` DB
  // table) — a separate list from `status.providers` (which already shows
  // these generically too, once added; this list exists so each one has
  // its own real Delete control, which built-in env-var providers don't).
  const [customProviders, setCustomProviders] = useState<CustomAiProviderView[]>([]);
  const [newProviderName, setNewProviderName] = useState('');
  const [newProviderBaseUrl, setNewProviderBaseUrl] = useState('');
  const [newProviderApiKey, setNewProviderApiKey] = useState('');
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

  const handleAddCustomProvider = async () => {
    setAddingProvider(true);
    setAddProviderError(null);
    try {
      await aiqaApi.ai.customProviders.create({
        name: newProviderName.trim(),
        base_url: newProviderBaseUrl.trim(),
        api_key: newProviderApiKey.trim(),
      });
      // The raw key never lingers in component state once saved — same
      // real-secret discipline `OrchestratorAuthPanel` above already
      // holds itself to.
      setNewProviderName('');
      setNewProviderBaseUrl('');
      setNewProviderApiKey('');
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
      });
      setStatus((prev) => (prev ? { ...prev, settings: saved } : prev));
    } catch (err) {
      setSaveError(err instanceof ApiError ? err.message : 'Failed to save AI settings.');
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center gap-2 text-slate-400 text-xs">
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

  return (
    <div id="ai-intelligence-panel" className="space-y-3">
      <div className="pb-2 border-b border-slate-800">
        <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">AI Intelligence</h3>
        <p className="text-slate-400 text-xs mt-0.5">Provider discovery, model routing, and generation health — all real, live-checked state.</p>
      </div>

      {/* Overall status */}
      <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2">
          <span className={`w-2 h-2 rounded-full ${overallCopy.dot}`} />
          <span className={`font-mono font-bold text-xs ${overallCopy.text}`}>{overallCopy.label}</span>
        </div>
        <div className="flex items-center gap-2 text-[11px] font-mono text-slate-400">
          <span>{status.providers.length} provider(s) checked</span>
          <button
            type="button"
            id="btn-refresh-ai-providers"
            onClick={() => void handleRefresh()}
            disabled={refreshing}
            className="flex items-center gap-1 px-2 py-1 rounded bg-[#0B1120] border border-slate-700 hover:border-indigo-600 text-slate-300 hover:text-indigo-300 cursor-pointer disabled:opacity-50"
          >
            <RefreshCw className={`w-3 h-3 ${refreshing ? 'animate-spin' : ''}`} />
            <span>Scan Providers</span>
          </button>
        </div>
      </div>

      {/* Active routing preview (real, from AiInfrastructure::route_among) */}
      {status.routing_preview ? (
        <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-1 text-xs font-mono">
          <div className="flex justify-between">
            <span className="text-slate-500">Active Provider</span>
            <span className="text-slate-200 font-semibold">{status.routing_preview.provider_id}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-slate-500">Active Model</span>
            <span className="text-slate-200 font-semibold">{status.routing_preview.model_id}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-slate-500">Reason</span>
            <span className="text-slate-300">{status.routing_preview.reason}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-slate-500">Routing Score</span>
            <span className="text-slate-300" title="A documented routing heuristic, not a benchmarked accuracy score.">
              {status.routing_preview.routing_score.toFixed(1)}
            </span>
          </div>
        </div>
      ) : (
        <div className="p-3 rounded-lg bg-slate-900/60 border border-slate-800 text-[11px] font-mono text-slate-400 flex items-start gap-2">
          <CircleHelp className="w-3.5 h-3.5 shrink-0 mt-0.5" />
          <span>No AI model currently available for Test Case Generation. Test generation will use the local template pipeline.</span>
        </div>
      )}

      {/* Discovered providers */}
      <div className="space-y-1.5">
        <span className="text-[10px] font-mono uppercase tracking-wider text-slate-500 font-semibold">Discovered Providers</span>
        {status.providers.map((p) => (
          <div key={p.id} className="p-2.5 rounded-md bg-[#020617] border border-slate-800 flex items-center justify-between flex-wrap gap-1.5">
            <div className="flex items-center gap-2">
              <span className="font-mono text-xs font-semibold text-slate-200">{p.name}</span>
              <span className="text-[10px] font-mono text-slate-500">{p.provider_type}</span>
              {p.models.length > 0 && <span className="text-[10px] font-mono text-slate-500">{p.models.length} model(s)</span>}
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
      <div id="custom-ai-providers-section" className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5">
        <span className="text-[10px] font-mono uppercase tracking-wider text-slate-500 font-semibold">Your Custom Providers</span>

        {customProviders.length > 0 && (
          <div className="space-y-1.5">
            {customProviders.map((p) => (
              <div key={p.id} className="p-2 rounded-md bg-[#0B1120] border border-slate-800 flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="font-mono text-xs font-semibold text-slate-200 truncate">{p.name}</div>
                  <div className="font-mono text-[10px] text-slate-500 truncate">{p.base_url}</div>
                </div>
                <button
                  type="button"
                  id={`btn-delete-custom-provider-${p.id}`}
                  onClick={() => void handleDeleteCustomProvider(p.id)}
                  disabled={deletingProviderId === p.id}
                  title="Remove this custom provider"
                  className="p-1.5 rounded text-slate-500 hover:text-rose-300 hover:bg-rose-950/40 cursor-pointer disabled:opacity-50 shrink-0"
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
            value={newProviderName}
            onChange={(e) => setNewProviderName(e.target.value)}
            placeholder="Name (e.g. My Provider)"
            className="px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 placeholder:text-slate-600 focus:outline-none focus:border-indigo-500"
          />
          <input
            type="text"
            id="new-custom-provider-base-url"
            value={newProviderBaseUrl}
            onChange={(e) => setNewProviderBaseUrl(e.target.value)}
            placeholder="Base URL (https://...)"
            className="px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 placeholder:text-slate-600 focus:outline-none focus:border-indigo-500"
          />
          <input
            type="password"
            id="new-custom-provider-api-key"
            value={newProviderApiKey}
            onChange={(e) => setNewProviderApiKey(e.target.value)}
            placeholder="API key"
            className="px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 placeholder:text-slate-600 focus:outline-none focus:border-indigo-500"
          />
        </div>
        <p className="text-[10px] font-mono text-slate-500">
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
          disabled={addingProvider || !newProviderName.trim() || !newProviderBaseUrl.trim() || !newProviderApiKey.trim()}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-mono text-xs font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {addingProvider ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Plus className="w-3.5 h-3.5" />}
          <span>Add Custom Provider</span>
        </button>
      </div>

      {/* Mode */}
      <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5">
        <div>
          <label className="block text-xs font-bold font-mono text-slate-300 mb-1">AI Mode</label>
          <select
            value={draftMode}
            onChange={(e) => setDraftMode(e.target.value as AiSettings['mode'])}
            className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 focus:outline-none focus:border-indigo-500 cursor-pointer"
          >
            <option value="AUTO">AUTO — best available model is selected automatically</option>
            <option value="MANUAL">MANUAL — choose a specific provider/model</option>
          </select>
        </div>

        {draftMode === 'MANUAL' && (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
            <div>
              <label className="block text-[10px] font-mono text-slate-400 mb-1">Provider</label>
              <select
                value={draftProvider}
                onChange={(e) => {
                  setDraftProvider(e.target.value);
                  setDraftModel('');
                }}
                className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 focus:outline-none focus:border-indigo-500 cursor-pointer"
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
              <label className="block text-[10px] font-mono text-slate-400 mb-1">Model</label>
              <select
                value={draftModel}
                onChange={(e) => setDraftModel(e.target.value)}
                disabled={!selectedProvider}
                className="w-full px-2.5 py-1.5 rounded bg-[#0B1120] border border-slate-700 text-xs font-mono text-slate-200 focus:outline-none focus:border-indigo-500 cursor-pointer disabled:opacity-50"
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

        <label className="flex items-center gap-2 text-[11px] font-mono text-slate-300 cursor-pointer">
          <input type="checkbox" checked={draftFallback} onChange={(e) => setDraftFallback(e.target.checked)} className="accent-indigo-500" />
          <span>Fall back to another available model if the selected one fails</span>
        </label>

        {saveError && <p className="text-[11px] font-mono text-rose-300">{saveError}</p>}

        <button
          type="button"
          id="btn-save-ai-settings"
          onClick={() => void handleSaveSettings()}
          disabled={saving || (draftMode === 'MANUAL' && (!draftProvider || !draftModel))}
          className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-mono text-xs font-bold cursor-pointer"
        >
          {saving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Save className="w-3.5 h-3.5" />}
          <span>Save AI Settings</span>
        </button>
      </div>

      <p className="text-[10px] font-mono text-slate-500">
        Settings are saved to the real orchestrator backend and take effect on the next AI generation request.
      </p>
    </div>
  );
};

type SettingsTab = 'authentication' | 'aiModels';

export const SettingsPage: React.FC = () => {
  const [activeTab, setActiveTab] = useState<SettingsTab>('authentication');

  const tabs: Array<{ id: SettingsTab; label: string; icon: React.ComponentType<{ className?: string }> }> = [
    { id: 'authentication', label: 'Authentication & SSO', icon: Key },
    { id: 'aiModels', label: 'AI Models & Reasoning', icon: Sparkles },
  ];

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
        <div className="md:col-span-4 rounded-lg border border-slate-800 bg-[#0B1120] p-2.5 space-y-1">
          <div className="text-[10px] font-mono uppercase tracking-wider text-slate-500 px-2.5 py-1.5 font-bold">
            Configuration Sections
          </div>
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;

            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`w-full flex items-center gap-2.5 px-2.5 py-2 rounded-md text-xs font-medium transition-all text-left cursor-pointer ${
                  isActive
                    ? 'bg-indigo-950/60 border border-indigo-800/80 text-indigo-300 font-semibold shadow-sm'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/40 border border-transparent'
                }`}
              >
                <Icon className={`w-3.5 h-3.5 shrink-0 ${isActive ? 'text-indigo-400' : 'text-slate-500'}`} />
                <span className="truncate">{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Settings Body (8 cols) */}
        <div className="md:col-span-8 rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-4 text-xs">
          {activeTab === 'authentication' && (
            <div className="space-y-3">
              <div className="pb-2 border-b border-slate-800">
                <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Authentication</h3>
                <p className="text-slate-400 text-xs mt-0.5">The real orchestrator API key this frontend authenticates with</p>
              </div>
              <OrchestratorAuthPanel />
            </div>
          )}

          {activeTab === 'aiModels' && <AiIntelligencePanel />}
        </div>
      </div>
    </div>
  );
};
