/**
 * AIQA System Settings Workspace
 * Enterprise configuration sections:
 * General, Authentication, Execution, Environments, Policies, AI/Models, Notifications.
 */

import React, { useState } from 'react';
import {
  Settings as SettingsIcon,
  Shield,
  Key,
  Cpu,
  Layers,
  FileCheck,
  Sparkles,
  Bell,
  Check,
  Save,
  RotateCcw,
  ExternalLink,
  Loader2,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { useAuth } from '../../context/AuthContext';
import { renderAuthState } from '../state/StateViews';
import { SectionHeader } from '../common/SectionHeader';
import { SystemSettings } from '../../types';

/**
 * FE-2/FE-3 — the real orchestrator credential panel. Everything else on
 * this tab (SSO toggle, "Control Plane Daemon API Token" card below) is
 * pre-existing fabricated content unrelated to this backend's actual
 * authentication model and is left exactly as-is per this sprint's
 * explicit "do not remove existing fake data" instruction; this panel is
 * additive, not a replacement.
 *
 * FE-3: the status card (badge/icon/title/description/actions) is no
 * longer drawn here — it's `renderAuthState` from `components/state/
 * StateViews.tsx`, the single state-to-component mapping every consumer
 * shares. This component now only owns what is genuinely
 * Settings-page-specific: the key-entry input and its Save/Replace button,
 * wired in via the `onConfigure` callback.
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
        // A plain <div>, not <form> — this panel is rendered inside the
        // Settings page's own outer <form> (its unrelated, pre-existing
        // "Save Settings" submit), and nested <form> elements are invalid
        // HTML with undefined submit behavior across browsers.
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

type SettingsTab =
  | 'general'
  | 'authentication'
  | 'execution'
  | 'environments'
  | 'policies'
  | 'aiModels'
  | 'notifications';

export const SettingsPage: React.FC = () => {
  const { settings, updateSettings } = useAIQA();
  const [activeTab, setActiveTab] = useState<SettingsTab>('general');
  const [formState, setFormState] = useState<SystemSettings>(settings);
  const [savedSuccess, setSavedSuccess] = useState(false);

  const handleSave = (e: React.FormEvent) => {
    e.preventDefault();
    updateSettings(formState);
    setSavedSuccess(true);
    setTimeout(() => setSavedSuccess(false), 2500);
  };

  const tabs: Array<{ id: SettingsTab; label: string; icon: React.ComponentType<{ className?: string }> }> = [
    { id: 'general', label: 'General & Organization', icon: SettingsIcon },
    { id: 'authentication', label: 'Authentication & SSO', icon: Key },
    { id: 'execution', label: 'Execution & Runners', icon: Cpu },
    { id: 'environments', label: 'Target Environments', icon: Layers },
    { id: 'policies', label: 'Quality Policies & Gates', icon: FileCheck },
    { id: 'aiModels', label: 'AI Models & Reasoning', icon: Sparkles },
    { id: 'notifications', label: 'Alerts & Notifications', icon: Bell },
  ];

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="System Settings & Governance"
        subtitle="Global control plane configuration, runner pools, quality gates, and AI reasoning parameters"
        badge="GOVERNANCE"
        icon={SettingsIcon}
        actions={
          <div className="flex items-center gap-2">
            {savedSuccess && (
              <span className="text-xs font-mono text-emerald-400 flex items-center gap-1">
                <Check className="w-3.5 h-3.5" />
                <span>Configuration Saved</span>
              </span>
            )}
            <button
              onClick={handleSave}
              className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
            >
              <Save className="w-3.5 h-3.5" />
              <span>Save Changes</span>
            </button>
          </div>
        }
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

        {/* Settings Form Body (8 cols) */}
        <div className="md:col-span-8 rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-4 text-xs">
          <form onSubmit={handleSave} className="space-y-4">
            {/* 1. General */}
            {activeTab === 'general' && (
              <div className="space-y-3">
                <div className="pb-2 border-b border-slate-800">
                  <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">General Workspace Settings</h3>
                  <p className="text-slate-400 text-xs mt-0.5">Control plane organization identifiers and retention</p>
                </div>

                <div>
                  <label className="block text-slate-300 font-medium mb-1">Organization Name</label>
                  <input
                    type="text"
                    value={formState.general.organizationName}
                    onChange={(e) =>
                      setFormState({
                        ...formState,
                        general: { ...formState.general, organizationName: e.target.value },
                      })
                    }
                    className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none text-xs"
                  />
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-slate-300 font-medium mb-1">Project Key</label>
                    <input
                      type="text"
                      value={formState.general.projectKey}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          general: { ...formState.general, projectKey: e.target.value },
                        })
                      }
                      className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                    />
                  </div>

                  <div>
                    <label className="block text-slate-300 font-medium mb-1">Data Retention (Days)</label>
                    <input
                      type="number"
                      value={formState.general.dataRetentionDays}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          general: { ...formState.general, dataRetentionDays: parseInt(e.target.value) },
                        })
                      }
                      className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                    />
                  </div>
                </div>

                <div>
                  <label className="block text-slate-300 font-medium mb-1">Timezone</label>
                  <input
                    type="text"
                    value={formState.general.timezone}
                    onChange={(e) =>
                      setFormState({
                        ...formState,
                        general: { ...formState.general, timezone: e.target.value },
                      })
                    }
                    className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  />
                </div>
              </div>
            )}

            {/* 2. Authentication */}
            {activeTab === 'authentication' && (
              <div className="space-y-3">
                <div className="pb-2 border-b border-slate-800">
                  <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Authentication & Identity Providers</h3>
                  <p className="text-slate-400 text-xs mt-0.5">Enterprise SAML SSO and control plane service tokens</p>
                </div>

                <OrchestratorAuthPanel />

                <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5">
                  <label className="flex items-center justify-between cursor-pointer">
                    <div>
                      <span className="font-semibold text-slate-200 block">Enforce Enterprise SSO</span>
                      <span className="text-slate-400 text-[11px]">Require single sign-on for all QA engineers</span>
                    </div>
                    <input
                      type="checkbox"
                      checked={formState.authentication.ssoEnabled}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          authentication: { ...formState.authentication, ssoEnabled: e.target.checked },
                        })
                      }
                      className="w-4 h-4 accent-indigo-500 rounded cursor-pointer"
                    />
                  </label>

                  <div>
                    <label className="block text-slate-300 font-medium mb-1">SSO Identity Provider</label>
                    <select
                      value={formState.authentication.provider}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          authentication: { ...formState.authentication, provider: e.target.value as any },
                        })
                      }
                      className="w-full px-2.5 py-1.5 rounded-md bg-[#0B1120] border border-slate-700 text-slate-200 focus:border-indigo-500 focus:outline-none text-xs"
                    >
                      <option value="OKTA">Okta SAML 2.0</option>
                      <option value="GOOGLE_WORKSPACE">Google Workspace OIDC</option>
                      <option value="AZURE_AD">Microsoft Entra ID (Azure AD)</option>
                      <option value="LOCAL">Local Vault Credentials</option>
                    </select>
                  </div>
                </div>

                <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 flex items-center justify-between">
                  <div>
                    <div className="font-semibold text-slate-200">Control Plane Daemon API Token</div>
                    <div className="text-slate-400 text-[10px] font-mono">Status: ACTIVE • Expires in 180 days</div>
                  </div>
                  <button
                    type="button"
                    className="px-2.5 py-1 rounded bg-[#0B1120] hover:bg-slate-800 text-slate-200 text-xs font-mono border border-slate-700 cursor-pointer"
                  >
                    Rotate Token
                  </button>
                </div>
              </div>
            )}

            {/* 3. Execution */}
            {activeTab === 'execution' && (
              <div className="space-y-3">
                <div className="pb-2 border-b border-slate-800">
                  <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Test Execution & Runners</h3>
                  <p className="text-slate-400 text-xs mt-0.5">Runner pool allocation, timeout ceilings, and failure isolation</p>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-slate-300 font-medium mb-1">Default Runner Pool</label>
                    <input
                      type="text"
                      value={formState.execution.defaultExecutor}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          execution: { ...formState.execution, defaultExecutor: e.target.value },
                        })
                      }
                      className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                    />
                  </div>

                  <div>
                    <label className="block text-slate-300 font-medium mb-1">Max Concurrency Slots</label>
                    <input
                      type="number"
                      value={formState.execution.maxConcurrency}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          execution: { ...formState.execution, maxConcurrency: parseInt(e.target.value) },
                        })
                      }
                      className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                    />
                  </div>
                </div>

                <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5">
                  <label className="flex items-center justify-between cursor-pointer">
                    <div>
                      <span className="font-semibold text-slate-200 block">Isolate Infrastructure Failures</span>
                      <span className="text-slate-400 text-[11px]">
                        Distinguish runner DNS/container crashes from product application bugs
                      </span>
                    </div>
                    <input
                      type="checkbox"
                      checked={formState.execution.isolateInfraFailures}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          execution: { ...formState.execution, isolateInfraFailures: e.target.checked },
                        })
                      }
                      className="w-4 h-4 accent-indigo-500 rounded cursor-pointer"
                    />
                  </label>
                </div>
              </div>
            )}

            {/* 4. Environments */}
            {activeTab === 'environments' && (
              <div className="space-y-3">
                <div className="pb-2 border-b border-slate-800">
                  <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Target Environments</h3>
                  <p className="text-slate-400 text-xs mt-0.5">Authoritative test target hosts and health endpoints</p>
                </div>

                <div className="space-y-2.5">
                  {formState.environments.map((env) => (
                    <div key={env.id} className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-1.5">
                      <div className="flex items-center justify-between font-mono">
                        <span className="font-bold text-indigo-400">{env.name}</span>
                        <span className="text-[9px] text-emerald-400 bg-emerald-950 px-1.5 py-0.2 rounded border border-emerald-800">
                          {env.id.toUpperCase()}
                        </span>
                      </div>
                      <div className="grid grid-cols-2 gap-2 text-xs font-mono">
                        <div>
                          <span className="text-slate-500 block text-[9px]">Base URL</span>
                          <span className="text-slate-200 truncate text-[11px]">{env.baseUrl}</span>
                        </div>
                        <div>
                          <span className="text-slate-500 block text-[9px]">Health Check</span>
                          <span className="text-slate-200 text-[11px]">{env.healthEndpoint}</span>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* 5. Policies */}
            {activeTab === 'policies' && (
              <div className="space-y-3">
                <div className="pb-2 border-b border-slate-800">
                  <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Quality Gates & Release Policies</h3>
                  <p className="text-slate-400 text-xs mt-0.5">Rules governing automated pipeline gating and deployment approvals</p>
                </div>

                <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5">
                  <label className="flex items-center justify-between cursor-pointer">
                    <div>
                      <span className="font-semibold text-slate-200 block">Block Release on Product Failures</span>
                      <span className="text-slate-400 text-[11px]">Hard fail CI/CD build if any functional/RBAC defect occurs</span>
                    </div>
                    <input
                      type="checkbox"
                      checked={formState.policies.blockOnProductFailures}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          policies: { ...formState.policies, blockOnProductFailures: e.target.checked },
                        })
                      }
                      className="w-4 h-4 accent-indigo-500 rounded cursor-pointer"
                    />
                  </label>

                  <label className="flex items-center justify-between cursor-pointer">
                    <div>
                      <span className="font-semibold text-slate-200 block">Require Manual Sign-off for Production</span>
                      <span className="text-slate-400 text-[11px]">Require QA Lead manual approval before promoting build</span>
                    </div>
                    <input
                      type="checkbox"
                      checked={formState.policies.requireManualApprovalForProd}
                      onChange={(e) =>
                        setFormState({
                          ...formState,
                          policies: { ...formState.policies, requireManualApprovalForProd: e.target.checked },
                        })
                      }
                      className="w-4 h-4 accent-indigo-500 rounded cursor-pointer"
                    />
                  </label>
                </div>
              </div>
            )}

            {/* 6. AI Models */}
            {activeTab === 'aiModels' && (
              <div className="space-y-3">
                <div className="pb-2 border-b border-slate-800">
                  <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">AI Reasoning & Model Hyperparameters</h3>
                  <p className="text-slate-400 text-xs mt-0.5">Configure autonomous test planning reasoning depth</p>
                </div>

                <div>
                  <label className="block text-slate-300 font-medium mb-1">Default Test Generation Model</label>
                  <select
                    value={formState.aiModels.defaultModel}
                    onChange={(e) =>
                      setFormState({
                        ...formState,
                        aiModels: { ...formState.aiModels, defaultModel: e.target.value },
                      })
                    }
                    className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  >
                    <option value="gemini-2.0-flash">Gemini 2.0 Flash (Fast & Low Latency)</option>
                    <option value="gemini-1.5-pro">Gemini 1.5 Pro (Deep Code Reasoning)</option>
                  </select>
                </div>

                <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2">
                  <div className="flex justify-between font-mono text-xs">
                    <span className="text-slate-300">Model Temperature:</span>
                    <span className="text-indigo-400 font-bold">{formState.aiModels.temperature} (Deterministic)</span>
                  </div>
                  <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.05"
                    value={formState.aiModels.temperature}
                    onChange={(e) =>
                      setFormState({
                        ...formState,
                        aiModels: { ...formState.aiModels, temperature: parseFloat(e.target.value) },
                      })
                    }
                    className="w-full accent-indigo-500"
                  />
                </div>
              </div>
            )}

            {/* 7. Notifications */}
            {activeTab === 'notifications' && (
              <div className="space-y-3">
                <div className="pb-2 border-b border-slate-800">
                  <h3 className="text-xs font-bold text-slate-100 uppercase font-mono">Quality Alerts & Integrations</h3>
                  <p className="text-slate-400 text-xs mt-0.5">Slack webhooks, PagerDuty, and email incident routing</p>
                </div>

                <div>
                  <label className="block text-slate-300 font-medium mb-1">Slack Incident Webhook</label>
                  <input
                    type="text"
                    value={formState.notifications.slackWebhook}
                    onChange={(e) =>
                      setFormState({
                        ...formState,
                        notifications: { ...formState.notifications, slackWebhook: e.target.value },
                      })
                    }
                    className="w-full px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-[11px]"
                  />
                </div>
              </div>
            )}

            <div className="pt-3 border-t border-slate-800 flex justify-end">
              <button
                type="submit"
                className="px-4 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
              >
                Save Settings
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
};
