import React from "react";
import {
  Search,
  Bell,
  Activity,
  Play,
  Layers,
  Globe2,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  SlidersHorizontal,
  ChevronDown
} from "lucide-react";
import { EnvironmentType, SystemHealthState, ViewType } from "../../types";

interface TopHeaderProps {
  activeView?: ViewType;
  currentView?: ViewType;
  onNavigate?: (view: ViewType) => void;
  environment: EnvironmentType;
  onSelectEnvironment?: (env: EnvironmentType) => void;
  onEnvironmentChange?: (env: EnvironmentType) => void;
  selectedProject: string;
  onSelectProject?: (proj: string) => void;
  onProjectChange?: (proj: string) => void;
  systemHealth: SystemHealthState;
  onOpenSearch: () => void;
  onOpenNotifications?: () => void;
  onOpenAlerts?: () => void;
  onOpenDiagnostics: () => void;
  onOpenTriggerModal?: () => void;
  onTriggerTest?: () => void;
  unreadAlertsCount: number;
}

const projects = [
  { id: "all", name: "All Projects (4 Active)" },
  { id: "Apex AI Agent", name: "Apex AI Agent" },
  { id: "Payment Core API", name: "Payment Core API" },
  { id: "Search RAG Pipeline", name: "Search RAG Pipeline" },
  { id: "Omni LLM Service", name: "Omni LLM Service" }
];

export const TopHeader: React.FC<TopHeaderProps> = ({
  activeView,
  currentView,
  onNavigate,
  environment,
  onSelectEnvironment,
  onEnvironmentChange,
  selectedProject,
  onSelectProject,
  onProjectChange,
  systemHealth,
  onOpenSearch,
  onOpenNotifications,
  onOpenAlerts,
  onOpenDiagnostics,
  onOpenTriggerModal,
  onTriggerTest,
  unreadAlertsCount
}) => {
  const handleEnvChange = (env: EnvironmentType) => {
    if (onEnvironmentChange) onEnvironmentChange(env);
    if (onSelectEnvironment) onSelectEnvironment(env);
  };

  const handleProjectChange = (proj: string) => {
    if (onProjectChange) onProjectChange(proj);
    if (onSelectProject) onSelectProject(proj);
  };

  const handleTrigger = () => {
    if (onTriggerTest) onTriggerTest();
    if (onOpenTriggerModal) onOpenTriggerModal();
  };

  const handleAlerts = () => {
    if (onOpenAlerts) onOpenAlerts();
    if (onOpenNotifications) onOpenNotifications();
  };

  let healthLevel: "HEALTHY" | "DEGRADED" | "CRITICAL" = "HEALTHY";
  if (systemHealth.backend === "offline" || systemHealth.database === "disconnected") {
    healthLevel = "CRITICAL";
  } else if (
    systemHealth.backend === "degraded" ||
    systemHealth.database === "degraded" ||
    systemHealth.ollama === "offline" ||
    systemHealth.apiConnectivity < 98
  ) {
    healthLevel = "DEGRADED";
  }

  const effectiveView = activeView || currentView || "dashboard";
  const viewTitle =
    effectiveView === "dashboard"
      ? "AIQA Control Center"
      : effectiveView === "test-runs"
      ? "Test Case Execution Hub"
      : effectiveView
          .split("-")
          .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
          .join(" ");

  return (
    <header
      id="top-header"
      className="sticky top-0 z-20 h-12 bg-[#080809]/95 backdrop-blur-md border-b border-slate-800 px-4 flex items-center justify-between gap-3 select-none"
    >
      {/* Left: Page Title & Subtitle */}
      <div className="flex items-center gap-3 min-w-0">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-xs sm:text-sm font-bold text-white tracking-tight truncate leading-none">
              {viewTitle}
            </h1>
            {effectiveView === "dashboard" && (
              <span className="hidden sm:inline-flex items-center gap-1 text-[9px] font-mono px-1.5 py-0.2 rounded bg-indigo-500/20 text-indigo-400 border border-indigo-500/40 font-semibold">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span>
                LIVE
              </span>
            )}
          </div>
          <p className="text-[9px] uppercase tracking-wider text-slate-500 font-semibold mt-0.5">
            AI Quality Engineering Control Center
          </p>
        </div>
      </div>

      {/* Center / Right: High-Density Controls & Selectors */}
      <div className="flex items-center gap-2 text-xs">
        {/* Project Selector */}
        <div className="relative hidden md:block">
          <div className="flex items-center gap-1 px-2 py-1 rounded bg-slate-900 border border-slate-700/80 text-[10px] text-slate-200">
            <Layers className="w-3 h-3 text-indigo-400 shrink-0" />
            <select
              id="select-project-dropdown"
              value={selectedProject}
              onChange={(e) => handleProjectChange(e.target.value)}
              aria-label="Select Project"
              className="bg-transparent text-slate-200 text-[10px] focus:outline-none cursor-pointer pr-3 font-medium"
            >
              {projects.map((p) => (
                <option key={p.id} value={p.id} className="bg-slate-900 text-slate-200">
                  {p.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* High Density Environment Selector */}
        <div className="flex bg-slate-900 border border-slate-700/80 rounded overflow-hidden text-[10px]">
          {(["staging", "production", "development"] as EnvironmentType[]).map((env, idx) => {
            const isSelected = environment === env;
            return (
              <button
                key={env}
                id={`btn-env-${env}`}
                onClick={() => handleEnvChange(env)}
                className={`px-2 py-0.5 font-medium uppercase tracking-tight transition-colors ${
                  idx > 0 ? "border-l border-slate-700/80" : ""
                } ${
                  isSelected
                    ? "bg-indigo-600 text-white font-semibold shadow-inner"
                    : "text-slate-400 hover:bg-slate-800 hover:text-slate-200"
                }`}
              >
                {env === "production" ? "Prod" : env === "staging" ? "Stg" : "Dev"}
              </button>
            );
          })}
        </div>

        {/* Google Workspace Integration Button */}
        {onNavigate && (
          <button
            id="btn-workspace-header-link"
            onClick={() => onNavigate("workspace")}
            title="Google Workspace (Gmail, Sheets, Calendar)"
            className="flex items-center gap-1.5 px-2 py-1 rounded bg-slate-900 border border-slate-700/80 hover:border-indigo-500/50 hover:bg-slate-800 text-[10px] font-medium text-slate-300 transition-colors"
          >
            <div className="flex items-center -space-x-1">
              <span className="w-2 h-2 rounded-full bg-rose-500" title="Gmail" />
              <span className="w-2 h-2 rounded-full bg-emerald-500" title="Sheets" />
              <span className="w-2 h-2 rounded-full bg-blue-500" title="Calendar" />
            </div>
            <span className="hidden xl:inline">Workspace</span>
          </button>
        )}

        {/* Global Search Bar (Cmd+K) */}
        <button
          id="btn-global-search"
          onClick={onOpenSearch}
          className="hidden lg:flex items-center gap-1.5 px-2 py-1 rounded bg-slate-900/90 border border-slate-700/70 text-[10px] text-slate-400 hover:text-slate-200 hover:border-slate-600 transition-colors"
        >
          <Search className="w-3 h-3 text-slate-400" />
          <span>Search...</span>
          <kbd className="text-[8px] px-1 py-0.2 rounded bg-slate-800 text-slate-400 font-mono border border-slate-700">
            ⌘K
          </kbd>
        </button>

        {/* System Health Indicator Banner Button */}
        <button
          id="btn-system-health-indicator"
          onClick={onOpenDiagnostics}
          title="Click to view system diagnostics"
          className={`flex items-center gap-1.5 px-2.5 py-1 rounded border text-[10px] font-bold uppercase tracking-tight transition-all ${
            healthLevel === "HEALTHY"
              ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-400 hover:bg-emerald-500/20"
              : healthLevel === "DEGRADED"
              ? "bg-amber-500/10 border-amber-500/30 text-amber-400 hover:bg-amber-500/20 animate-pulse"
              : "bg-rose-500/10 border-rose-500/30 text-rose-400 hover:bg-rose-500/20 animate-pulse"
          }`}
        >
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              healthLevel === "HEALTHY"
                ? "bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.6)]"
                : healthLevel === "DEGRADED"
                ? "bg-amber-500 shadow-[0_0_8px_rgba(245,158,11,0.6)]"
                : "bg-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.6)]"
            }`}
          />
          <span className="hidden sm:inline">HEALTH: {healthLevel}</span>
          <span className="sm:hidden">{healthLevel}</span>
        </button>

        {/* Trigger Test Run CTA */}
        <button
          id="btn-trigger-test-run"
          onClick={handleTrigger}
          className="flex items-center gap-1 px-2.5 py-1 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-[10px] font-semibold transition-colors shadow-sm"
        >
          <Play className="w-3 h-3 fill-current" />
          <span className="hidden sm:inline">Trigger Test</span>
        </button>

        {/* Notification Bell */}
        <button
          id="btn-notifications"
          onClick={handleAlerts}
          aria-label="Open notifications"
          className="relative p-1.5 rounded bg-slate-900 border border-slate-700/80 hover:border-slate-600 text-slate-300 hover:text-white transition-colors"
        >
          <Bell className="w-3.5 h-3.5" />
          {unreadAlertsCount > 0 && (
            <span className="absolute -top-1 -right-1 w-3.5 h-3.5 rounded-full bg-rose-500 text-white text-[9px] font-bold flex items-center justify-center border border-[#080809]">
              {unreadAlertsCount}
            </span>
          )}
        </button>
      </div>
    </header>
  );
};

