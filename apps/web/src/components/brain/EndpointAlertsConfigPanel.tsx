import React, { useState, useEffect } from "react";
import {
  Bell,
  BellRing,
  BellOff,
  AlertTriangle,
  Volume2,
  VolumeX,
  Sliders,
  CheckCircle2,
  ShieldAlert,
  Sparkles,
  Zap,
  Info,
  RefreshCw,
  Play,
  RotateCcw,
  Check,
  X,
  Settings,
  ChevronDown,
  Cpu,
  Globe,
  Server
} from "lucide-react";
import {
  EndpointFailureAlertConfig,
  EndpointFailureIncident,
  LocalOllamaModel,
  CloudApiProvider
} from "../../types";
import {
  checkNotificationSupport,
  getNotificationPermission,
  requestNotificationPermission,
  sendBrowserNotification,
  playAlertChime,
  recordEndpointFailure,
  resetEndpointFailures,
  getEndpointFailureCounts
} from "../../utils/browserNotifications";

interface EndpointAlertsConfigPanelProps {
  config: EndpointFailureAlertConfig;
  onUpdateConfig: (newConfig: EndpointFailureAlertConfig) => void;
  localModels: LocalOllamaModel[];
  cloudProviders: CloudApiProvider[];
  incidents: EndpointFailureIncident[];
  onClearIncidents: () => void;
  className?: string;
}

export const EndpointAlertsConfigPanel: React.FC<EndpointAlertsConfigPanelProps> = ({
  config,
  onUpdateConfig,
  localModels,
  cloudProviders,
  incidents,
  onClearIncidents,
  className = ""
}) => {
  const [permission, setPermission] = useState<NotificationPermission>("default");
  const [isSupported, setIsSupported] = useState(true);
  const [testSent, setTestSent] = useState(false);
  const [simulationActive, setSimulationActive] = useState(false);
  const [activeFailureCounts, setActiveFailureCounts] = useState<
    Record<string, { count: number; lastError: string; lastTime: string; name: string }>
  >(getEndpointFailureCounts());

  useEffect(() => {
    setIsSupported(checkNotificationSupport());
    setPermission(getNotificationPermission());
  }, []);

  const refreshCounters = () => {
    setActiveFailureCounts(getEndpointFailureCounts());
  };

  const handleRequestPermission = async () => {
    const perm = await requestNotificationPermission();
    setPermission(perm);
    onUpdateConfig({
      ...config,
      browserPermission: perm,
      enabled: perm === "granted" ? true : config.enabled
    });
  };

  const handleToggleEnable = async () => {
    if (!config.enabled && permission !== "granted") {
      const perm = await requestNotificationPermission();
      setPermission(perm);
      if (perm === "granted") {
        onUpdateConfig({ ...config, enabled: true, browserPermission: perm });
      } else {
        // Still toggle preference, but warn user
        onUpdateConfig({ ...config, enabled: true, browserPermission: perm });
      }
    } else {
      onUpdateConfig({ ...config, enabled: !config.enabled });
    }
  };

  const handleTestNotification = () => {
    if (permission !== "granted") {
      handleRequestPermission();
      return;
    }

    sendBrowserNotification("⚠️ Test Alert: AI Model Endpoint Outage", {
      body: `This is a test notification from AI Brain. Consecutive failure threshold is set to ${config.failureThreshold} attempts.`,
      playSound: config.soundEnabled
    });

    setTestSent(true);
    setTimeout(() => setTestSent(false), 3000);
  };

  const handleSimulateFailure = () => {
    setSimulationActive(true);
    const targetEndpoint = "simulated-ollama-node";
    const targetName = "Local Ollama Node (11434)";

    // Simulate 3 failures consecutively
    for (let i = 0; i < config.failureThreshold; i++) {
      recordEndpointFailure(
        targetEndpoint,
        targetName,
        "local_ollama",
        "CUDA kernel execution timeout: Out of VRAM on layer allocation [simulated]",
        config
      );
    }

    refreshCounters();
    setTimeout(() => setSimulationActive(false), 2000);
  };

  const handleClearFailures = (endpointId?: string) => {
    if (endpointId) {
      resetEndpointFailures(endpointId);
    } else {
      Object.keys(activeFailureCounts).forEach((id) => resetEndpointFailures(id));
    }
    refreshCounters();
    onClearIncidents();
  };

  const availableEndpoints = [
    { id: "all", name: "All Models & Providers (Global)", type: "global" },
    ...localModels.map((m) => ({ id: `ollama-${m.id}`, name: `Local Ollama: ${m.name}`, type: "local" })),
    ...cloudProviders.map((p) => ({ id: p.id, name: `Cloud API: ${p.name}`, type: "cloud" }))
  ];

  return (
    <div
      className={`p-5 rounded-2xl bg-zinc-900/90 border border-zinc-800 shadow-xl backdrop-blur-md font-mono ${className}`}
    >
      {/* Header & Main Toggle Switch */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-4 border-b border-zinc-800">
        <div className="flex items-start gap-3">
          <div
            className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 border ${
              config.enabled
                ? "bg-amber-500/20 text-amber-300 border-amber-500/40 shadow-lg shadow-amber-500/20"
                : "bg-zinc-800 text-slate-400 border-zinc-700"
            }`}
          >
            {config.enabled ? <BellRing className="w-5 h-5 animate-bounce" /> : <BellOff className="w-5 h-5" />}
          </div>

          <div>
            <div className="flex items-center gap-2.5 flex-wrap">
              <h3 className="text-sm font-bold text-white tracking-tight">
                Repeated Model / API Endpoint Failure Alerts
              </h3>
              <span
                className={`text-[10px] font-bold px-2 py-0.5 rounded-full border ${
                  config.enabled
                    ? "bg-emerald-500/20 text-emerald-300 border-emerald-500/40"
                    : "bg-zinc-800 text-slate-400 border-zinc-700"
                }`}
              >
                {config.enabled ? "ALERTS ACTIVE" : "DISABLED"}
              </span>

              {permission === "granted" ? (
                <span className="text-[10px] px-2 py-0.5 rounded-full bg-indigo-500/20 text-indigo-300 border border-indigo-500/40 flex items-center gap-1">
                  <CheckCircle2 className="w-3 h-3 text-indigo-400" />
                  <span>Browser Permission Granted</span>
                </span>
              ) : permission === "denied" ? (
                <span className="text-[10px] px-2 py-0.5 rounded-full bg-rose-500/20 text-rose-300 border border-rose-500/40 flex items-center gap-1">
                  <AlertTriangle className="w-3 h-3 text-rose-400" />
                  <span>Browser Permission Denied</span>
                </span>
              ) : (
                <button
                  type="button"
                  onClick={handleRequestPermission}
                  className="text-[10px] px-2 py-0.5 rounded-full bg-amber-500/20 hover:bg-amber-500/30 text-amber-300 border border-amber-500/40 flex items-center gap-1 transition-colors"
                >
                  <Bell className="w-3 h-3" />
                  <span>Grant Permission &rarr;</span>
                </button>
              )}
            </div>

            <p className="text-xs text-slate-400 mt-1 leading-relaxed">
              Triggers system-level OS browser notifications when any monitored local CUDA model or cloud API provider fails consecutively.
            </p>
          </div>
        </div>

        {/* Master Configuration Toggle Switch Button */}
        <div className="flex items-center gap-3 self-start md:self-auto shrink-0">
          <button
            type="button"
            onClick={handleToggleEnable}
            className={`px-4 py-2.5 rounded-xl font-bold text-xs flex items-center gap-2 transition-all shadow-md active:scale-95 ${
              config.enabled
                ? "bg-amber-600 hover:bg-amber-500 text-white shadow-amber-950/60"
                : "bg-zinc-800 hover:bg-zinc-700 text-slate-300 border border-zinc-700"
            }`}
          >
            {config.enabled ? (
              <>
                <BellRing className="w-4 h-4" />
                <span>Notifications Enabled</span>
              </>
            ) : (
              <>
                <BellOff className="w-4 h-4" />
                <span>Enable Notifications</span>
              </>
            )}
          </button>
        </div>
      </div>

      {/* Configuration Controls Matrix */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-4">
        {/* 1. Failure Threshold Stepper */}
        <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/80 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider flex items-center gap-1.5">
              <Sliders className="w-3.5 h-3.5 text-indigo-400" />
              <span>Consecutive Failure Threshold</span>
            </span>
            <span className="text-xs font-bold text-indigo-300">{config.failureThreshold} Fails</span>
          </div>
          <div className="flex items-center gap-1.5 pt-1">
            {[2, 3, 4, 5].map((val) => (
              <button
                key={val}
                type="button"
                onClick={() => onUpdateConfig({ ...config, failureThreshold: val })}
                className={`flex-1 py-1.5 rounded-lg text-xs font-bold transition-colors ${
                  config.failureThreshold === val
                    ? "bg-indigo-600 text-white shadow-sm"
                    : "bg-zinc-900 text-slate-400 hover:bg-zinc-800 hover:text-white border border-zinc-800"
                }`}
              >
                {val}x
              </button>
            ))}
          </div>
          <span className="text-[10px] text-slate-500 block">
            Alert fires after {config.failureThreshold} repeated consecutive errors without recovery.
          </span>
        </div>

        {/* 2. Sound & Tone Alert Chime Toggle */}
        <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/80 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider flex items-center gap-1.5">
              {config.soundEnabled ? (
                <Volume2 className="w-3.5 h-3.5 text-amber-400" />
              ) : (
                <VolumeX className="w-3.5 h-3.5 text-slate-500" />
              )}
              <span>Auditory Warning Tone</span>
            </span>
            <span
              className={`text-[10px] font-bold px-1.5 py-0.2 rounded ${
                config.soundEnabled ? "bg-amber-500/20 text-amber-300" : "bg-zinc-900 text-slate-500"
              }`}
            >
              {config.soundEnabled ? "AUDIO ON" : "MUTED"}
            </span>
          </div>

          <div className="flex items-center gap-2 pt-1">
            <button
              type="button"
              onClick={() => onUpdateConfig({ ...config, soundEnabled: !config.soundEnabled })}
              className={`flex-1 py-1.5 rounded-lg text-xs font-bold transition-colors flex items-center justify-center gap-1.5 ${
                config.soundEnabled
                  ? "bg-amber-600/30 text-amber-300 border border-amber-500/40"
                  : "bg-zinc-900 text-slate-400 hover:text-white border border-zinc-800"
              }`}
            >
              {config.soundEnabled ? "Disable Chime" : "Enable Chime"}
            </button>

            <button
              type="button"
              onClick={playAlertChime}
              className="px-2.5 py-1.5 rounded-lg bg-zinc-900 hover:bg-zinc-800 border border-zinc-800 text-slate-300 hover:text-white text-xs"
              title="Preview Audio Chime"
            >
              Test Tone
            </button>
          </div>

          <span className="text-[10px] text-slate-500 block">
            Plays a synthesized dual-frequency sound on repeated outage.
          </span>
        </div>

        {/* 3. Test & Simulation Sandbox Actions */}
        <div className="p-3.5 rounded-xl bg-zinc-950 border border-zinc-800/80 space-y-2">
          <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider flex items-center gap-1.5">
            <Sparkles className="w-3.5 h-3.5 text-cyan-400" />
            <span>Diagnostics &amp; Verification</span>
          </span>

          <div className="flex items-center gap-2 pt-1">
            <button
              type="button"
              onClick={handleTestNotification}
              className="flex-1 py-1.5 rounded-lg bg-indigo-600/20 hover:bg-indigo-600/30 text-indigo-300 border border-indigo-500/40 text-xs font-bold transition-colors flex items-center justify-center gap-1"
            >
              {testSent ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Play className="w-3.5 h-3.5" />}
              <span>{testSent ? "Sent!" : "Send Test Alert"}</span>
            </button>

            <button
              type="button"
              onClick={handleSimulateFailure}
              disabled={simulationActive}
              className="flex-1 py-1.5 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/40 text-xs font-bold transition-colors flex items-center justify-center gap-1"
            >
              <AlertTriangle className="w-3.5 h-3.5" />
              <span>{simulationActive ? "Simulating..." : `Simulate ${config.failureThreshold}x Fail`}</span>
            </button>
          </div>

          <span className="text-[10px] text-slate-500 block">
            Immediately test browser notification permissions or simulate a multi-failure trigger.
          </span>
        </div>
      </div>

      {/* Active Failure Count Monitors & Incidents Tray */}
      {Object.keys(activeFailureCounts).length > 0 && (
        <div className="mt-4 p-3.5 rounded-xl bg-rose-950/20 border border-rose-500/30 space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-xs font-bold text-rose-300">
              <ShieldAlert className="w-4 h-4 text-rose-400" />
              <span>Current Endpoint Failure Status &amp; Consecutive Counts</span>
            </div>

            <button
              type="button"
              onClick={() => handleClearFailures()}
              className="px-2.5 py-1 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/30 text-[10px] font-bold flex items-center gap-1"
            >
              <RotateCcw className="w-3 h-3" />
              <span>Clear All Counters</span>
            </button>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2.5">
            {Object.entries(activeFailureCounts).map(([id, info]: [string, { count: number; lastError: string; lastTime: string; name: string }]) => (
              <div
                key={id}
                className="p-2.5 rounded-lg bg-zinc-950 border border-rose-500/40 flex items-center justify-between text-xs"
              >
                <div className="min-w-0 pr-2">
                  <div className="font-bold text-white truncate">{info.name}</div>
                  <div className="text-[10px] text-rose-400 mt-0.5 truncate">{info.lastError}</div>
                  <div className="text-[9px] text-slate-500 mt-0.5">
                    Last: {new Date(info.lastTime).toLocaleTimeString()}
                  </div>
                </div>

                <div className="flex items-center gap-2 shrink-0">
                  <span
                    className={`px-2 py-0.5 rounded text-xs font-bold ${
                      info.count >= config.failureThreshold
                        ? "bg-rose-500 text-white animate-pulse"
                        : "bg-amber-500/20 text-amber-300"
                    }`}
                  >
                    {info.count} / {config.failureThreshold} fails
                  </span>
                  <button
                    type="button"
                    onClick={() => handleClearFailures(id)}
                    className="p-1 rounded text-slate-400 hover:text-white bg-zinc-900"
                    title="Reset counter"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
