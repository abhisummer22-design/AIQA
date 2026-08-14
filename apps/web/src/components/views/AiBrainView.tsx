import React, { useState, useEffect } from "react";
import {
  Brain,
  Cpu,
  Key,
  Globe,
  Server,
  Zap,
  Shield,
  Gauge,
  Sliders,
  CheckCircle2,
  AlertTriangle,
  RefreshCw,
  Sparkles,
  GitFork,
  BarChart3,
  Layers,
  Lock,
  Activity,
  Bell,
  BellRing,
  BellOff,
  Volume2,
  VolumeX,
  ChevronDown,
  ChevronUp
} from "lucide-react";
import {
  BrainEngineSource,
  BrainRoutingMode,
  LocalOllamaModel,
  CloudApiProvider,
  BrainTaskRouting,
  BrainTelemetry,
  BrainBenchmarkRun,
  EndpointFailureAlertConfig,
  EndpointFailureIncident
} from "../../types";
import {
  fetchBrainState,
  updateBrainRoutingMode,
  updateOllamaDaemonConfig,
  setPrimaryOllamaModel,
  pullOllamaModel,
  updateProviderApiKey,
  testProviderConnection,
  updateTaskRoutingMatrix,
  runBrainBenchmark
} from "../../services/api";
import { OllamaLocalSection } from "../brain/OllamaLocalSection";
import { CloudApiKeysSection } from "../brain/CloudApiKeysSection";
import { TaskRoutingMatrixSection } from "../brain/TaskRoutingMatrixSection";
import { BrainPlaygroundSection } from "../brain/BrainPlaygroundSection";
import { BrainTelemetrySection } from "../brain/BrainTelemetrySection";
import { ModelEndpointSearchDropdown } from "../brain/ModelEndpointSearchDropdown";
import { EndpointAlertsConfigPanel } from "../brain/EndpointAlertsConfigPanel";
import {
  getNotificationPermission,
  requestNotificationPermission,
  recordEndpointFailure,
  resetEndpointFailures
} from "../../utils/browserNotifications";
import { ModelEndpointItem } from "../../types";

export const AiBrainView: React.FC = () => {
  const [activeTab, setActiveTab] = useState<
    "local_ollama" | "cloud_keys" | "task_matrix" | "bench_playground" | "telemetry"
  >("local_ollama");

  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  // Browser-native Outage & Failure Alert Config
  const [alertConfig, setAlertConfig] = useState<EndpointFailureAlertConfig>(() => {
    const saved = typeof window !== "undefined" ? localStorage.getItem("ai_brain_endpoint_alerts") : null;
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        // ignore fallback
      }
    }
    return {
      enabled: false,
      failureThreshold: 3,
      soundEnabled: true,
      monitoredEndpoints: ["all"],
      autoFallbackOnAlert: true,
      browserPermission: getNotificationPermission()
    };
  });

  const [showAlertsConfig, setShowAlertsConfig] = useState<boolean>(false);
  const [failureIncidents, setFailureIncidents] = useState<EndpointFailureIncident[]>([]);

  const handleUpdateAlertConfig = (newConfig: EndpointFailureAlertConfig) => {
    setAlertConfig(newConfig);
    try {
      localStorage.setItem("ai_brain_endpoint_alerts", JSON.stringify(newConfig));
    } catch (e) {
      console.warn("Failed to persist alertConfig to localStorage", e);
    }
  };

  const handleQuickToggleAlerts = async () => {
    if (!alertConfig.enabled && getNotificationPermission() !== "granted") {
      const perm = await requestNotificationPermission();
      handleUpdateAlertConfig({
        ...alertConfig,
        browserPermission: perm,
        enabled: perm === "granted"
      });
      if (perm !== "granted") {
        setShowAlertsConfig(true);
      }
    } else {
      handleUpdateAlertConfig({
        ...alertConfig,
        enabled: !alertConfig.enabled
      });
    }
  };

  const [routingMode, setRoutingMode] = useState<BrainRoutingMode>("smart_auto");
  const [ollamaConfig, setOllamaConfig] = useState<any>({
    baseUrl: "http://localhost:11434",
    status: "connected",
    version: "v0.5.11 (Ollama CUDA Engine)",
    latencyMs: 14,
    lastPing: new Date().toISOString(),
    hardware: {
      gpuName: "NVIDIA RTX 4090 (24GB VRAM)",
      vramUsedMb: 8192,
      vramTotalMb: 24576,
      gpuTempC: 54,
      gpuUtilPercent: 46,
      cpuUtilPercent: 24,
      numaEnabled: true,
      flashAttention2: true,
      contextLength: 32768,
      temperature: 0.2,
      topP: 0.9,
      threads: 16
    }
  });

  const [localModels, setLocalModels] = useState<LocalOllamaModel[]>([]);
  const [cloudProviders, setCloudProviders] = useState<CloudApiProvider[]>([]);
  const [taskRoutings, setTaskRoutings] = useState<BrainTaskRouting[]>([]);
  const [telemetry, setTelemetry] = useState<BrainTelemetry>({
    todayTotalInferences: 2184,
    localInferencesCount: 1680,
    cloudInferencesCount: 504,
    localRatioPercent: 76.9,
    avgLatencyMs: 285,
    tokensGeneratedToday: 894200,
    costSavedDollars: 58.40,
    fallbackIncidentsToday: 3,
    vramAllocatedMb: 8192,
    vramTotalMb: 24576,
    gpuUtilization: 46,
    cpuUtilization: 24,
    airgapComplianceScore: 98.6
  });
  const [benchmarkHistory, setBenchmarkHistory] = useState<BrainBenchmarkRun[]>([]);
  const [isUpdatingMode, setIsUpdatingMode] = useState<boolean>(false);

  const loadState = async () => {
    try {
      setLoading(true);
      const data = await fetchBrainState();
      setRoutingMode(data.routingMode);
      setOllamaConfig(data.ollama);
      setLocalModels(data.localModels);
      setCloudProviders(data.cloudProviders);
      setTaskRoutings(data.taskRoutings);
      setTelemetry(data.telemetry);
      setBenchmarkHistory(data.benchmarkHistory);
      setError(null);
    } catch (err: any) {
      console.error("Failed to load Brain State:", err);
      setError("Unable to connect to Brain Engine API");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadState();
  }, []);

  const handleModeSelect = async (newMode: BrainRoutingMode) => {
    setIsUpdatingMode(true);
    setRoutingMode(newMode);
    try {
      await updateBrainRoutingMode(newMode);
    } catch (err) {
      console.error("Failed to update routing mode:", err);
    } finally {
      setIsUpdatingMode(false);
    }
  };

  const handleSetPrimaryModel = async (modelId: string) => {
    try {
      const res = await setPrimaryOllamaModel(modelId);
      if (res.localModels) {
        setLocalModels(res.localModels);
      }
    } catch (err) {
      console.error("Failed to set primary model:", err);
    }
  };

  const handlePullModel = async (modelName: string) => {
    try {
      const res = await pullOllamaModel(modelName);
      if (res.localModels) {
        setLocalModels(res.localModels);
      }
    } catch (err) {
      console.error("Failed to pull model:", err);
    }
  };

  const handleUpdateOllamaConfig = async (payload: { baseUrl?: string; hardware?: Record<string, any> }) => {
    try {
      const res = await updateOllamaDaemonConfig(payload);
      if (res.ollama) {
        setOllamaConfig(res.ollama);
      }
    } catch (err) {
      console.error("Failed to update Ollama config:", err);
    }
  };

  const handleUpdateProviderKey = async (payload: {
    providerId: string;
    apiKey?: string;
    customBaseUrl?: string;
    selectedModel?: string;
  }) => {
    try {
      const res = await updateProviderApiKey(payload);
      if (res.provider) {
        setCloudProviders((prev) =>
          prev.map((p) => (p.id === res.provider.id ? res.provider : p))
        );
      }
    } catch (err) {
      console.error("Failed to update provider key:", err);
    }
  };

  const handleTestProvider = async (providerId: string) => {
    return testProviderConnection(providerId);
  };

  const handleSaveRoutings = async (routings: BrainTaskRouting[]) => {
    try {
      const res = await updateTaskRoutingMatrix(routings);
      if (res.taskRoutings) {
        setTaskRoutings(res.taskRoutings);
      }
    } catch (err) {
      console.error("Failed to save routings:", err);
    }
  };

  const handleRunBenchmark = async (payload: { engine: string; model: string; prompt: string }) => {
    try {
      const res = await runBrainBenchmark(payload);
      if (res.result) {
        setBenchmarkHistory((prev) => [res.result, ...prev]);
        if (res.result.status === "error") {
          recordEndpointFailure(
            payload.engine,
            `${payload.engine.toUpperCase()}: ${payload.model}`,
            payload.engine === "ollama" ? "local_ollama" : "remote_api",
            res.result.errorMessage || "Inference benchmark returned failure",
            alertConfig,
            (inc) => setFailureIncidents((prev) => [inc, ...prev])
          );
        } else {
          resetEndpointFailures(payload.engine);
        }
      }
      if (res.telemetry) {
        setTelemetry(res.telemetry);
      }
      return res;
    } catch (err: any) {
      recordEndpointFailure(
        payload.engine,
        `${payload.engine.toUpperCase()}: ${payload.model}`,
        payload.engine === "ollama" ? "local_ollama" : "remote_api",
        err.message || "Endpoint connection failed",
        alertConfig,
        (inc) => setFailureIncidents((prev) => [inc, ...prev])
      );
      throw err;
    }
  };

  const loadedModel = localModels.find((m) => m.isPrimary) || localModels[0];

  return (
    <div className="space-y-6 pb-12">
      {/* Top Header & Master Power Ribbon */}
      <div className="p-6 rounded-2xl bg-zinc-900/90 border border-zinc-800 shadow-2xl relative overflow-hidden backdrop-blur-lg">
        {/* Subtle Ambient Background Gradient */}
        <div className="absolute -right-20 -top-20 w-80 h-80 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none" />
        <div className="absolute -left-20 -bottom-20 w-80 h-80 bg-purple-500/10 rounded-full blur-3xl pointer-events-none" />

        <div className="relative z-10 flex flex-col lg:flex-row lg:items-center justify-between gap-6">
          <div className="space-y-2">
            <div className="flex items-center gap-3">
              <div className="w-12 h-12 rounded-2xl bg-gradient-to-tr from-indigo-600 to-purple-600 flex items-center justify-center text-white shadow-lg shadow-indigo-600/30">
                <Brain className="w-6 h-6" />
              </div>
              <div>
                <div className="flex items-center gap-3">
                  <h1 className="text-xl font-bold font-mono text-white tracking-tight">
                    Main Brain &amp; Neural Inference Power Grid
                  </h1>
                  <span className="text-[11px] font-mono font-bold px-2.5 py-0.5 rounded-full bg-indigo-500/20 text-indigo-300 border border-indigo-500/40 flex items-center gap-1.5">
                    <span className="w-2 h-2 rounded-full bg-emerald-400 animate-ping" />
                    <span>ENGINE ONLINE</span>
                  </span>
                </div>
                <p className="text-xs text-slate-400 font-mono mt-0.5">
                  Centralized AI command center: manage local Ollama CUDA models, frontier API keys, and zero-trust task routing
                </p>
              </div>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-3">
            {/* Quick Searchable Model & Endpoint Switcher Dropdown */}
            <div className="w-60">
              <ModelEndpointSearchDropdown
                localModels={localModels}
                cloudProviders={cloudProviders}
                ollamaBaseUrl={ollamaConfig?.baseUrl}
                selectedModelTag={loadedModel?.id}
                selectedEngine="ollama"
                onSelect={(item: ModelEndpointItem) => {
                  if (item.instanceType === "local_ollama") {
                    handleSetPrimaryModel(item.modelTag);
                  } else {
                    setActiveTab("bench_playground");
                  }
                }}
                variant="compact"
                placeholder="Switch model or endpoint..."
              />
            </div>

            {/* Outage & Failure Alert Toggle Pill with Configuration Trigger */}
            <div className="flex items-center gap-1 p-1 bg-zinc-950/80 rounded-xl border border-zinc-800">
              <button
                type="button"
                onClick={handleQuickToggleAlerts}
                className={`px-3 py-1.5 rounded-lg text-xs font-mono font-bold flex items-center gap-2 transition-all ${
                  alertConfig.enabled
                    ? "bg-amber-500/20 text-amber-300 border border-amber-500/40 shadow-sm shadow-amber-500/10"
                    : "text-slate-400 hover:text-slate-200 hover:bg-zinc-800/80"
                }`}
                title={alertConfig.enabled ? "Disable repeated failure notifications" : "Enable browser-native failure notifications"}
              >
                {alertConfig.enabled ? (
                  <BellRing className="w-3.5 h-3.5 text-amber-400 animate-pulse" />
                ) : (
                  <BellOff className="w-3.5 h-3.5 text-slate-500" />
                )}
                <span>{alertConfig.enabled ? `Alerts: ON (${alertConfig.failureThreshold}x)` : "Alerts: OFF"}</span>
              </button>

              <button
                type="button"
                onClick={() => setShowAlertsConfig(!showAlertsConfig)}
                className={`px-2 py-1.5 rounded-lg text-xs font-mono transition-colors flex items-center gap-1 ${
                  showAlertsConfig
                    ? "bg-indigo-600/30 text-indigo-300 border border-indigo-500/40"
                    : "text-slate-400 hover:text-white hover:bg-zinc-800/80"
                }`}
                title="Configure Failure Notification Rules"
              >
                <Sliders className="w-3 h-3" />
                <span className="hidden sm:inline text-[10px]">Rules</span>
                {showAlertsConfig ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
              </button>
            </div>

            <div className="hidden 2xl:block px-4 py-2 rounded-xl bg-zinc-950/80 border border-zinc-800 text-left">
              <span className="text-[10px] text-slate-500 font-mono uppercase font-bold block">
                Cloud Fallback
              </span>
              <span className="text-xs font-mono font-bold text-cyan-300 flex items-center gap-1.5 mt-0.5">
                <Globe className="w-3.5 h-3.5 text-cyan-400" />
                Gemini 3.7 Flash
              </span>
            </div>

            <button
              type="button"
              onClick={loadState}
              className="p-2.5 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-slate-300 hover:text-white transition-colors"
              title="Refresh Engine State"
            >
              <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
            </button>
          </div>
        </div>

        {/* 4-Mode Orchestration Selector */}
        <div className="mt-6 pt-5 border-t border-zinc-800/80">
          <div className="flex items-center justify-between mb-3">
            <span className="text-xs font-mono font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
              <Sliders className="w-3.5 h-3.5 text-indigo-400" />
              <span>Orchestration &amp; Routing Mode</span>
            </span>
            <span className="text-[11px] font-mono text-slate-500">
              Active: <strong className="text-white uppercase">{routingMode.replace("_", " ")}</strong>
            </span>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
            {[
              {
                id: "smart_auto" as BrainRoutingMode,
                title: "Smart Local-First Auto",
                desc: "Prefers local Ollama CUDA, auto-failover to Gemini/Cloud on high load",
                tag: "RECOMMENDED",
                color: "indigo"
              },
              {
                id: "strict_local" as BrainRoutingMode,
                title: "Strict Air-Gapped Local",
                desc: "100% Ollama on-premise execution with zero internet token egress",
                tag: "ZERO EGRESS",
                color: "emerald"
              },
              {
                id: "cloud_priority" as BrainRoutingMode,
                title: "Cloud Frontier Priority",
                desc: "Routes all QA reasoning to Gemini 3.7, GPT-4o, and Claude 3.7",
                tag: "MAX REASONING",
                color: "cyan"
              },
              {
                id: "task_matrix" as BrainRoutingMode,
                title: "Custom Task Matrix",
                desc: "Per-pipeline model assignment defined in Dispatch Matrix tab",
                tag: "GRANULAR",
                color: "purple"
              }
            ].map((mode) => {
              const isSelected = routingMode === mode.id;
              return (
                <button
                  key={mode.id}
                  type="button"
                  onClick={() => handleModeSelect(mode.id)}
                  className={`p-3.5 rounded-xl border text-left transition-all duration-200 relative ${
                    isSelected
                      ? "bg-indigo-950/40 border-indigo-500 shadow-md ring-1 ring-indigo-500/30"
                      : "bg-zinc-950/60 border-zinc-800 hover:border-zinc-700"
                  }`}
                >
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-xs font-bold font-mono text-white">{mode.title}</span>
                    <span
                      className={`text-[9px] font-mono font-bold px-1.5 py-0.2 rounded border ${
                        isSelected
                          ? "bg-indigo-500/20 text-indigo-300 border-indigo-500/40"
                          : "bg-zinc-900 text-slate-500 border-zinc-800"
                      }`}
                    >
                      {mode.tag}
                    </span>
                  </div>
                  <p className="text-[11px] text-slate-400 font-sans line-clamp-2">{mode.desc}</p>
                </button>
              );
            })}
          </div>
        </div>

        {/* Outage Alerts Configuration Drawer Panel */}
        {showAlertsConfig && (
          <div className="mt-5 pt-5 border-t border-zinc-800">
            <EndpointAlertsConfigPanel
              config={alertConfig}
              onUpdateConfig={handleUpdateAlertConfig}
              localModels={localModels}
              cloudProviders={cloudProviders}
              incidents={failureIncidents}
              onClearIncidents={() => setFailureIncidents([])}
            />
          </div>
        )}
      </div>

      {/* Primary Navigation Tabs */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-800 pb-2">
        <div className="flex flex-wrap gap-2">
          {[
            {
              id: "local_ollama" as const,
              label: "Local Models & Ollama Core",
              icon: Server,
              badge: `${localModels.length} Local`
            },
            {
              id: "cloud_keys" as const,
              label: "Cloud Providers & API Keys",
              icon: Key,
              badge: "7 Providers"
            },
            {
              id: "task_matrix" as const,
              label: "Neural Task Dispatch Matrix",
              icon: GitFork,
              badge: "6 Pipelines"
            },
            {
              id: "bench_playground" as const,
              label: "Live Test Bench & Playground",
              icon: Zap,
              badge: "Benchmark"
            },
            {
              id: "telemetry" as const,
              label: "Air-Gap & Token Telemetry",
              icon: BarChart3,
              badge: `${telemetry.airgapComplianceScore}% SOC2`
            }
          ].map((tab) => {
            const isActive = activeTab === tab.id;
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-2 px-4 py-2.5 rounded-xl font-mono text-xs font-bold transition-all ${
                  isActive
                    ? "bg-indigo-600 text-white shadow-lg shadow-indigo-600/30"
                    : "bg-zinc-900/60 hover:bg-zinc-800 text-slate-400 hover:text-slate-200 border border-zinc-800/80"
                }`}
              >
                <Icon className="w-4 h-4" />
                <span>{tab.label}</span>
                <span
                  className={`text-[9px] px-1.5 py-0.2 rounded font-mono ${
                    isActive ? "bg-white/20 text-white" : "bg-zinc-800 text-slate-500"
                  }`}
                >
                  {tab.badge}
                </span>
              </button>
            );
          })}
        </div>

        {/* Quick Outage Alert Config Trigger Tab Pill */}
        <button
          type="button"
          onClick={() => setShowAlertsConfig(!showAlertsConfig)}
          className={`flex items-center gap-2 px-3 py-2 rounded-xl font-mono text-xs font-bold transition-all border ${
            showAlertsConfig
              ? "bg-amber-500/20 text-amber-300 border-amber-500/50 shadow-md shadow-amber-500/10"
              : alertConfig.enabled
              ? "bg-zinc-900/80 text-amber-400 border-amber-500/30 hover:bg-zinc-800"
              : "bg-zinc-900/60 text-slate-400 hover:text-slate-200 border-zinc-800"
          }`}
        >
          {alertConfig.enabled ? (
            <BellRing className="w-3.5 h-3.5 text-amber-400 animate-bounce" />
          ) : (
            <BellOff className="w-3.5 h-3.5 text-slate-500" />
          )}
          <span>Outage Alerts</span>
          <span
            className={`text-[9px] px-1.5 py-0.2 rounded font-mono ${
              alertConfig.enabled ? "bg-amber-500/30 text-amber-200" : "bg-zinc-800 text-slate-500"
            }`}
          >
            {alertConfig.enabled ? `${alertConfig.failureThreshold}x Threshold` : "Off"}
          </span>
        </button>
      </div>

      {/* Main Tab Content Render */}
      {activeTab === "local_ollama" && (
        <OllamaLocalSection
          ollama={ollamaConfig}
          models={localModels}
          onSetPrimary={handleSetPrimaryModel}
          onPullModel={handlePullModel}
          onUpdateConfig={handleUpdateOllamaConfig}
        />
      )}

      {activeTab === "cloud_keys" && (
        <CloudApiKeysSection
          providers={cloudProviders}
          onUpdateKey={handleUpdateProviderKey}
          onTestConnection={handleTestProvider}
          alertConfig={alertConfig}
        />
      )}

      {activeTab === "task_matrix" && (
        <TaskRoutingMatrixSection
          taskRoutings={taskRoutings}
          localModels={localModels}
          cloudProviders={cloudProviders}
          onSaveRoutings={handleSaveRoutings}
        />
      )}

      {activeTab === "bench_playground" && (
        <BrainPlaygroundSection
          localModels={localModels}
          cloudProviders={cloudProviders}
          history={benchmarkHistory}
          ollamaBaseUrl={ollamaConfig?.baseUrl}
          onRunBenchmark={handleRunBenchmark}
        />
      )}

      {activeTab === "telemetry" && (
        <BrainTelemetrySection telemetry={telemetry} history={benchmarkHistory} />
      )}
    </div>
  );
};
