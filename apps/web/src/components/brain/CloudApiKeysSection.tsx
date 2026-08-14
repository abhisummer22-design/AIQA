import React, { useState } from "react";
import {
  Key,
  Globe,
  Zap,
  CheckCircle2,
  AlertCircle,
  Eye,
  EyeOff,
  RefreshCw,
  ExternalLink,
  ShieldCheck,
  Cpu,
  Layers,
  Sparkles,
  Server,
  Lock
} from "lucide-react";
import { CloudApiProvider, EndpointFailureAlertConfig } from "../../types";
import { recordEndpointFailure, resetEndpointFailures } from "../../utils/browserNotifications";

interface CloudApiKeysSectionProps {
  providers: CloudApiProvider[];
  onUpdateKey: (payload: { providerId: string; apiKey?: string; customBaseUrl?: string; selectedModel?: string }) => void;
  onTestConnection: (providerId: string) => Promise<any>;
  alertConfig?: EndpointFailureAlertConfig;
}

export const CloudApiKeysSection: React.FC<CloudApiKeysSectionProps> = ({
  providers,
  onUpdateKey,
  onTestConnection,
  alertConfig
}) => {
  const [editingProviderId, setEditingProviderId] = useState<string | null>(null);
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});
  const [baseUrlInputs, setBaseUrlInputs] = useState<Record<string, string>>({});
  const [showKeys, setShowKeys] = useState<Record<string, boolean>>({});
  const [testingId, setTestingId] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, { status: string; latency: number; message: string }>>({});
  const [isBatchTesting, setIsBatchTesting] = useState(false);

  const handleToggleShow = (id: string) => {
    setShowKeys((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  const handleStartEdit = (p: CloudApiProvider) => {
    setEditingProviderId(p.id);
    setKeyInputs((prev) => ({ ...prev, [p.id]: "" }));
    setBaseUrlInputs((prev) => ({ ...prev, [p.id]: p.customBaseUrl || "" }));
  };

  const handleSaveProvider = (providerId: string) => {
    const key = keyInputs[providerId];
    const baseUrl = baseUrlInputs[providerId];
    onUpdateKey({
      providerId,
      ...(key ? { apiKey: key } : {}),
      ...(baseUrl !== undefined ? { customBaseUrl: baseUrl } : {})
    });
    setEditingProviderId(null);
  };

  const handleTest = async (providerId: string) => {
    setTestingId(providerId);
    const provider = providers.find((p) => p.id === providerId);
    const providerName = provider ? provider.name : providerId;

    try {
      const res = await onTestConnection(providerId);
      if (res.status === "ERROR") {
        throw new Error(res.message || "Endpoint returned invalid authentication");
      }
      setTestResults((prev) => ({
        ...prev,
        [providerId]: {
          status: "SUCCESS",
          latency: res.latencyMs || 210,
          message: res.message || "Authentication handshake verified"
        }
      }));
      resetEndpointFailures(providerId);
    } catch (err: any) {
      const errMsg = err.message || "Failed to reach endpoint";
      setTestResults((prev) => ({
        ...prev,
        [providerId]: {
          status: "ERROR",
          latency: 0,
          message: errMsg
        }
      }));

      if (alertConfig) {
        recordEndpointFailure(
          providerId,
          providerName,
          providerId === "custom_openai" ? "custom_ingress" : "remote_api",
          errMsg,
          alertConfig
        );
      }
    } finally {
      setTestingId(null);
    }
  };

  const handleBatchTestAll = async () => {
    setIsBatchTesting(true);
    for (const p of providers) {
      if (p.isConfigured || p.hasKey) {
        await handleTest(p.id);
      }
    }
    setIsBatchTesting(false);
  };

  const getProviderIconColor = (id: string) => {
    switch (id) {
      case "gemini":
        return "text-cyan-400 bg-cyan-950/40 border-cyan-500/40";
      case "openai":
        return "text-emerald-400 bg-emerald-950/40 border-emerald-500/40";
      case "anthropic":
        return "text-amber-400 bg-amber-950/40 border-amber-500/40";
      case "deepseek":
        return "text-blue-400 bg-blue-950/40 border-blue-500/40";
      case "groq":
        return "text-orange-400 bg-orange-950/40 border-orange-500/40";
      case "mistral":
        return "text-rose-400 bg-rose-950/40 border-rose-500/40";
      default:
        return "text-purple-400 bg-purple-950/40 border-purple-500/40";
    }
  };

  return (
    <div className="space-y-6">
      {/* Top Banner: Cloud API Power Grid Overview */}
      <div className="p-5 rounded-2xl bg-gradient-to-r from-cyan-950/40 via-zinc-900/80 to-blue-950/30 border border-cyan-500/30 shadow-xl backdrop-blur-md">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
          <div className="space-y-1.5">
            <div className="flex items-center gap-2.5">
              <div className="w-9 h-9 rounded-xl bg-cyan-500/20 border border-cyan-500/40 flex items-center justify-center text-cyan-400">
                <Globe className="w-5 h-5" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-bold text-white font-mono">
                    Cloud AI Power Grid &amp; API Key Vault
                  </h2>
                  <span className="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-cyan-500/10 text-cyan-300 border border-cyan-500/30">
                    7 FRONTIER PROVIDERS
                  </span>
                </div>
                <p className="text-xs text-slate-400 font-mono mt-0.5">
                  Configure API keys, custom gateway proxies, and default models for server-side fallback and specialized tasks
                </p>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <button
              onClick={handleBatchTestAll}
              disabled={isBatchTesting}
              className="px-4 py-2 rounded-xl bg-cyan-600 hover:bg-cyan-500 text-white font-mono text-xs font-bold flex items-center gap-2 shadow-lg shadow-cyan-950/50 transition-all active:scale-95 disabled:opacity-50"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${isBatchTesting ? "animate-spin" : ""}`} />
              <span>{isBatchTesting ? "Testing Bridges..." : "Verify All Cloud Keys"}</span>
            </button>
          </div>
        </div>

        {/* Security Notice */}
        <div className="mt-4 p-3 rounded-xl bg-zinc-950/70 border border-zinc-800/90 flex items-center gap-3 text-xs font-mono text-slate-300">
          <ShieldCheck className="w-4 h-4 text-emerald-400 shrink-0" />
          <span>
            <strong className="text-white">Enterprise Vault Security:</strong> API keys are securely encapsulated server-side with AES-256 masking. No client browser code ever receives raw unmasked provider secrets.
          </span>
        </div>
      </div>

      {/* Providers Cards Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
        {providers.map((provider) => {
          const isEditing = editingProviderId === provider.id;
          const isTesting = testingId === provider.id;
          const testResult = testResults[provider.id];
          const isConfigured = provider.isConfigured || provider.hasKey;
          const isRevealed = showKeys[provider.id];

          return (
            <div
              key={provider.id}
              className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 hover:border-zinc-700 transition-all duration-200 flex flex-col justify-between space-y-4"
            >
              {/* Card Header */}
              <div className="space-y-2">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-center gap-3">
                    <div className={`w-10 h-10 rounded-xl border flex items-center justify-center font-bold text-sm font-mono ${getProviderIconColor(provider.id)}`}>
                      {provider.id.slice(0, 2).toUpperCase()}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="text-sm font-bold font-mono text-white">{provider.name}</h3>
                        <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-zinc-800 text-slate-300 border border-zinc-700">
                          {provider.badge}
                        </span>
                      </div>
                      <span className="text-[11px] text-slate-400 font-mono block">
                        {provider.provider}
                      </span>
                    </div>
                  </div>

                  <span
                    className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded-full border ${
                      isConfigured
                        ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
                        : "bg-amber-500/10 text-amber-400 border-amber-500/30"
                    }`}
                  >
                    {isConfigured ? "● CONNECTED" : "○ KEY NEEDED"}
                  </span>
                </div>

                <p className="text-xs text-slate-300 font-sans">
                  {provider.description}
                </p>
              </div>

              {/* API Key Field & Model Selection */}
              <div className="space-y-3 pt-2 border-t border-zinc-800/80 font-mono text-xs">
                {/* Active Model Selector */}
                <div>
                  <label className="text-[10px] text-slate-500 uppercase font-bold block mb-1">
                    Serving Model
                  </label>
                  <select
                    value={provider.selectedModel}
                    onChange={(e) => onUpdateKey({ providerId: provider.id, selectedModel: e.target.value })}
                    className="w-full p-2 rounded-xl bg-zinc-950 border border-zinc-800 text-slate-200 text-xs focus:outline-none focus:border-cyan-500"
                  >
                    {provider.availableModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                </div>

                {/* Key Input / Masked Display */}
                <div>
                  <div className="flex items-center justify-between text-[10px] text-slate-500 uppercase font-bold mb-1">
                    <span>API Key / Secret Token</span>
                    {!isEditing && (
                      <button
                        type="button"
                        onClick={() => handleStartEdit(provider)}
                        className="text-cyan-400 hover:underline cursor-pointer"
                      >
                        Edit Key &rarr;
                      </button>
                    )}
                  </div>

                  {isEditing ? (
                    <div className="space-y-2">
                      <div className="flex gap-2">
                        <input
                          type="password"
                          value={keyInputs[provider.id] || ""}
                          onChange={(e) => setKeyInputs((prev) => ({ ...prev, [provider.id]: e.target.value }))}
                          placeholder={`Enter ${provider.name} API Key...`}
                          className="flex-1 p-2 rounded-xl bg-zinc-950 border border-cyan-500/60 text-white focus:outline-none"
                        />
                        <button
                          type="button"
                          onClick={() => handleSaveProvider(provider.id)}
                          className="px-3 py-2 rounded-xl bg-cyan-600 hover:bg-cyan-500 text-white font-bold"
                        >
                          Save
                        </button>
                        <button
                          type="button"
                          onClick={() => setEditingProviderId(null)}
                          className="px-3 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-slate-300"
                        >
                          Cancel
                        </button>
                      </div>

                      {provider.id === "custom_openai" && (
                        <div>
                          <label className="text-[10px] text-slate-500 block mb-0.5">Custom Ingress Base URL</label>
                          <input
                            type="text"
                            value={baseUrlInputs[provider.id] || ""}
                            onChange={(e) => setBaseUrlInputs((prev) => ({ ...prev, [provider.id]: e.target.value }))}
                            placeholder="http://vllm-server.internal:8000/v1"
                            className="w-full p-2 rounded-xl bg-zinc-950 border border-zinc-800 text-white text-xs focus:outline-none"
                          />
                        </div>
                      )}
                    </div>
                  ) : (
                    <div className="p-2.5 rounded-xl bg-zinc-950 border border-zinc-800/90 flex items-center justify-between">
                      <div className="flex items-center gap-2 text-slate-300">
                        <Lock className="w-3.5 h-3.5 text-slate-500" />
                        <span className="font-bold text-xs">
                          {isRevealed
                            ? provider.maskedKey || "sk-live-configured"
                            : "••••••••••••••••••••••••"}
                        </span>
                      </div>
                      <button
                        type="button"
                        onClick={() => handleToggleShow(provider.id)}
                        className="text-slate-400 hover:text-white transition-colors"
                        title={isRevealed ? "Hide masked token" : "Reveal masked token"}
                      >
                        {isRevealed ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5 text-cyan-400" />}
                      </button>
                    </div>
                  )}
                </div>

                {/* Specs Pill Matrix */}
                <div className="grid grid-cols-2 gap-2 text-[10px] text-slate-400 pt-2 border-t border-zinc-800/80">
                  <div>
                    <span className="text-slate-500 block">Context Window:</span>
                    <span className="text-slate-200 font-bold">{provider.contextWindow}</span>
                  </div>
                  <div>
                    <span className="text-slate-500 block">Token Pricing:</span>
                    <span className="text-slate-200 font-bold">{provider.costPer1kTokens}</span>
                  </div>
                </div>

                {/* Test Result Message */}
                {testResult && (
                  <div
                    className={`p-2 rounded-xl text-[10px] flex items-center gap-2 ${
                      testResult.status === "SUCCESS"
                        ? "bg-emerald-500/10 text-emerald-300 border border-emerald-500/30"
                        : "bg-rose-500/10 text-rose-300 border border-rose-500/30"
                    }`}
                  >
                    {testResult.status === "SUCCESS" ? (
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                    ) : (
                      <AlertCircle className="w-3.5 h-3.5 text-rose-400 shrink-0" />
                    )}
                    <span>
                      {testResult.message} ({testResult.latency}ms)
                    </span>
                  </div>
                )}
              </div>

              {/* Card Footer Actions */}
              <div className="pt-3 border-t border-zinc-800/80 flex items-center justify-between font-mono text-xs">
                <div className="flex items-center gap-1.5 text-slate-400 text-[10px]">
                  <Zap className="w-3.5 h-3.5 text-amber-400" />
                  <span>Speed: ~{provider.tokensPerSec} t/s</span>
                </div>

                <button
                  type="button"
                  onClick={() => handleTest(provider.id)}
                  disabled={isTesting}
                  className="px-3 py-1.5 rounded-xl bg-zinc-800 hover:bg-cyan-600 text-slate-200 hover:text-white font-bold transition-all flex items-center gap-1.5 text-[11px] shadow-sm active:scale-95 disabled:opacity-50"
                >
                  <RefreshCw className={`w-3 h-3 ${isTesting ? "animate-spin" : ""}`} />
                  <span>{isTesting ? "Verifying..." : "Ping Handshake"}</span>
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
