/**
 * Top Application Header Component
 * Provides global context, environment switcher, daemon health, and quick actions
 */

import React, { useState } from 'react';
import {
  Menu,
  Search,
  Plus,
  Bell,
  CheckCircle2,
  AlertCircle,
  Radio,
  Server,
  Layers,
  ChevronDown,
  Sparkles,
  ExternalLink
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { Environment } from '../../types';
import { ProjectSelector } from '../project/ProjectSelector';

export const Header: React.FC = () => {
  const {
    toggleSidebar,
    currentEnvironment,
    setCurrentEnvironment,
    backendState,
    setBackendState,
    setCommandPaletteOpen,
    setNewRunModalOpen,
    notificationCount,
    clearNotifications,
    settings,
  } = useAIQA();

  const [isEnvDropdownOpen, setIsEnvDropdownOpen] = useState(false);
  const [isHealthDetailOpen, setIsHealthDetailOpen] = useState(false);
  const [isNotificationsOpen, setIsNotificationsOpen] = useState(false);

  const envOptions: Array<{ id: Environment; label: string; badge: string; color: string }> = [
    { id: 'staging', label: 'Staging Cluster', badge: 'US-West', color: 'text-amber-400 border-amber-800/60 bg-amber-950/40' },
    { id: 'production', label: 'Production Tier', badge: 'US-East', color: 'text-rose-400 border-rose-800/60 bg-rose-950/40' },
    { id: 'uat', label: 'UAT Acceptance', badge: 'v4.18', color: 'text-cyan-400 border-cyan-800/60 bg-cyan-950/40' },
    { id: 'ephemeral-pr', label: 'Ephemeral PR-1482', badge: 'Auto-Tear', color: 'text-purple-400 border-purple-800/60 bg-purple-950/40' },
  ];

  const currentEnvConfig = envOptions.find((e) => e.id === currentEnvironment) || envOptions[0];

  return (
    <header className="h-13 border-b border-slate-800/90 bg-[#0B1120] px-3 sm:px-4 flex items-center justify-between gap-3 sticky top-0 z-30 select-none">
      {/* Left: Hamburger & Brand */}
      <div className="flex items-center gap-2.5">
        <button
          id="sidebar-toggle-button"
          onClick={toggleSidebar}
          className="p-1.5 rounded-md text-slate-400 hover:text-slate-200 hover:bg-slate-800/80 transition-colors cursor-pointer"
          title="Toggle Navigation Sidebar (Ctrl+B)"
        >
          <Menu className="w-4 h-4" />
        </button>

        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-md bg-gradient-to-br from-indigo-500 to-cyan-500 p-[1px] shadow-sm shadow-indigo-950">
            <div className="w-full h-full bg-[#020617] rounded-[5px] flex items-center justify-center">
              <span className="font-mono font-black text-xs text-indigo-400 tracking-tighter">AI</span>
            </div>
          </div>
          <div className="flex flex-col">
            <div className="flex items-center gap-1.5">
              <span className="font-bold text-xs tracking-tight text-slate-100 font-sans">
                AIQA
              </span>
              <span className="text-[9px] font-mono uppercase px-1.5 py-0.2 rounded bg-indigo-950/80 text-indigo-300 border border-indigo-800/60 font-semibold tracking-wider">
                Control Plane
              </span>
            </div>
          </div>
        </div>

        {/* Vertical divider */}
        <div className="hidden sm:block h-4 w-[1px] bg-slate-800 mx-1" />

        {/* Environment Selector Dropdown */}
        <div className="relative">
          <button
            id="env-selector-button"
            onClick={() => setIsEnvDropdownOpen(!isEnvDropdownOpen)}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-[#020617] hover:bg-slate-900 border border-slate-800 text-xs text-slate-200 transition-colors cursor-pointer"
          >
            <Layers className="w-3.5 h-3.5 text-slate-400" />
            <span className="font-medium">{currentEnvConfig.label}</span>
            <span className={`text-[10px] font-mono px-1.5 py-0.2 rounded border ${currentEnvConfig.color}`}>
              {currentEnvConfig.badge}
            </span>
            <ChevronDown className="w-3 h-3 text-slate-500 ml-0.5" />
          </button>

          {isEnvDropdownOpen && (
            <div
              className="absolute left-0 mt-1.5 w-60 rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl p-1.5 z-50 text-xs"
              onMouseLeave={() => setIsEnvDropdownOpen(false)}
            >
              <div className="px-2.5 py-1 text-[10px] font-mono text-slate-400 uppercase tracking-wider font-semibold">
                Target Environments
              </div>
              {envOptions.map((env) => (
                <button
                  key={env.id}
                  onClick={() => {
                    setCurrentEnvironment(env.id);
                    setIsEnvDropdownOpen(false);
                  }}
                  className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded-md text-left transition-colors cursor-pointer ${
                    currentEnvironment === env.id
                      ? 'bg-slate-800/90 text-slate-100 font-semibold'
                      : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <div
                      className={`w-1.5 h-1.5 rounded-full ${
                        currentEnvironment === env.id ? 'bg-indigo-400' : 'bg-slate-600'
                      }`}
                    />
                    <span>{env.label}</span>
                  </div>
                  <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded border ${env.color}`}>
                    {env.badge}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>

        {/* FE-5: real, global project scope — see `ProjectSelector.tsx`.
            Placed here (global top header, next to the existing environment
            selector it visually mirrors) per this sprint's brief §10:
            "prefer integrating into an existing header/sidebar area already
            intended for contextual controls" — least invasive, no layout
            redesign. */}
        <div className="hidden sm:block h-4 w-[1px] bg-slate-800 mx-1" />
        <ProjectSelector />
      </div>

      {/* Center: Command Palette Trigger */}
      <div className="hidden md:flex items-center flex-1 max-w-md mx-2">
        <button
          id="global-search-button"
          onClick={() => setCommandPaletteOpen(true)}
          className="w-full flex items-center justify-between px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-900 border border-slate-800/90 text-xs text-slate-400 transition-all group cursor-pointer"
        >
          <div className="flex items-center gap-2">
            <Search className="w-3.5 h-3.5 text-slate-500 group-hover:text-slate-300" />
            <span className="group-hover:text-slate-300 text-xs">Quick search runs, tests, RBAC rules...</span>
          </div>
          <div className="flex items-center gap-1 font-mono text-[10px] text-slate-400 bg-slate-800/90 px-1.5 py-0.5 rounded border border-slate-700">
            <span>⌘</span>
            <span>K</span>
          </div>
        </button>
      </div>

      {/* Right: Health, Run Trigger, Notifications, User */}
      <div className="flex items-center gap-2">
        {/* Backend Daemon Health Indicator */}
        <div className="relative">
          <button
            id="backend-health-button"
            onClick={() => setIsHealthDetailOpen(!isHealthDetailOpen)}
            className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-[#020617] hover:bg-slate-900 border border-slate-800 text-xs text-slate-300 transition-colors cursor-pointer"
            title="Backend Control Plane Status"
          >
            <span className="relative flex h-2 w-2">
              <span
                className={`animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 ${
                  backendState.status === 'connected' ? 'bg-emerald-400' : 'bg-rose-400'
                }`}
              />
              <span
                className={`relative inline-flex rounded-full h-2 w-2 ${
                  backendState.status === 'connected' ? 'bg-emerald-500' : 'bg-rose-500'
                }`}
              />
            </span>
            <span className="hidden xl:inline text-slate-400 font-mono text-[11px]">
              {backendState.status === 'connected' ? 'Daemon Online' : 'Daemon Offline'}
            </span>
            {backendState.latencyMs && (
              <span className="hidden sm:inline text-[10px] font-mono text-emerald-400 bg-emerald-950/60 px-1.5 py-0.2 rounded border border-emerald-800/50">
                {backendState.latencyMs}ms
              </span>
            )}
          </button>

          {isHealthDetailOpen && (
            <div
              className="absolute right-0 mt-1.5 w-72 rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl p-3 z-50 text-xs"
              onMouseLeave={() => setIsHealthDetailOpen(false)}
            >
              <div className="flex items-center justify-between pb-2 border-b border-slate-800 mb-2">
                <span className="font-semibold text-slate-200">Control Plane Telemetry</span>
                <span
                  className={`text-[10px] font-mono px-2 py-0.5 rounded font-medium ${
                    backendState.status === 'connected'
                      ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                      : 'bg-rose-950 text-rose-300 border border-rose-800'
                  }`}
                >
                  {backendState.status.toUpperCase()}
                </span>
              </div>
              <div className="space-y-1 text-slate-400 text-[11px] font-mono">
                <div className="flex justify-between">
                  <span>Endpoint:</span>
                  <span className="text-slate-200 truncate max-w-[140px]" title={backendState.endpoint}>
                    {backendState.endpoint}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>Daemon Engine:</span>
                  <span className="text-slate-200">{backendState.daemonVersion || 'v2.4.2'}</span>
                </div>
                <div className="flex justify-between">
                  <span>Heartbeat:</span>
                  <span className="text-slate-200">Active (18ms)</span>
                </div>
              </div>
              <div className="mt-2.5 pt-2 border-t border-slate-800 flex justify-between gap-2">
                <button
                  onClick={() => {
                    setBackendState((prev) => ({
                      ...prev,
                      status: prev.status === 'connected' ? 'disconnected' : 'connected',
                    }));
                  }}
                  className="w-full py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 font-medium text-[11px] border border-slate-700 cursor-pointer"
                >
                  {backendState.status === 'connected' ? 'Simulate Disconnect' : 'Reconnect Daemon'}
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Trigger Run Button */}
        <button
          id="launch-run-header-button"
          onClick={() => setNewRunModalOpen(true)}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs transition-all shadow-sm shadow-indigo-950 cursor-pointer"
        >
          <Plus className="w-3.5 h-3.5 stroke-[2.5]" />
          <span className="hidden sm:inline font-sans">Launch Run</span>
        </button>

        {/* Notifications */}
        <div className="relative">
          <button
            id="notifications-button"
            onClick={() => {
              setIsNotificationsOpen(!isNotificationsOpen);
              if (notificationCount > 0) clearNotifications();
            }}
            className="p-1.5 rounded-md text-slate-400 hover:text-slate-200 hover:bg-slate-800/80 transition-colors relative cursor-pointer"
            title="Activity Notifications"
          >
            <Bell className="w-4 h-4" />
            {notificationCount > 0 && (
              <span className="absolute top-1 right-1 w-2 h-2 rounded-full bg-indigo-400 ring-2 ring-[#0B1120]" />
            )}
          </button>

          {isNotificationsOpen && (
            <div
              className="absolute right-0 mt-1.5 w-80 rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl p-3 z-50 text-xs"
              onMouseLeave={() => setIsNotificationsOpen(false)}
            >
              <div className="flex items-center justify-between pb-2 border-b border-slate-800 mb-2">
                <span className="font-semibold text-slate-200">Recent Quality Alerts</span>
                <span className="text-[10px] text-slate-400 font-mono">Live Stream</span>
              </div>
              <div className="space-y-1.5 max-h-64 overflow-y-auto pr-1">
                <div className="p-2 rounded-md bg-[#020617] border border-amber-900/40 text-amber-300">
                  <div className="font-medium text-[11px] flex items-center gap-1.5">
                    <AlertCircle className="w-3 h-3 text-amber-400" />
                    <span>Infrastructure DNS alert on Redis</span>
                  </div>
                  <p className="text-[10px] text-slate-400 mt-0.5">
                    test-104 failed due to DNS timeout (Infra Error, not product defect).
                  </p>
                </div>
                <div className="p-2 rounded-md bg-[#020617] border border-rose-900/40 text-rose-300">
                  <div className="font-medium text-[11px] flex items-center gap-1.5">
                    <AlertCircle className="w-3 h-3 text-rose-400" />
                    <span>Auth Token Revocation Assertion</span>
                  </div>
                  <p className="text-[10px] text-slate-400 mt-0.5">
                    Expected HTTP 401, received HTTP 200 on revoked cookie.
                  </p>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* User Identity Context */}
        <div className="flex items-center gap-1.5 pl-1 border-l border-slate-800">
          <div className="w-6 h-6 rounded-md bg-slate-800 border border-slate-700 flex items-center justify-center text-[10px] font-semibold text-indigo-300 font-mono">
            QA
          </div>
        </div>
      </div>
    </header>
  );
};
