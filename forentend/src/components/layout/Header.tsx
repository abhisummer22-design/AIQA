/**
 * Top Application Header Component
 * Provides global context, environment switcher, daemon health, and quick actions
 *
 * FE-24 — the notification bell used to open `AIQAContext`'s fabricated
 * `notificationCount` (`useState<number>(3)`, no real entity behind it) and
 * two hardcoded fake alert cards ("Infrastructure DNS alert on Redis" /
 * "Auth Token Revocation Assertion") labeled "Live Stream" — none of it real
 * (FE-23 report §Fake/Stub Audit). It now shows real, recent `RunEvent`s
 * received through the existing `RealtimeContext` (`useRealtime().subscribe`)
 * — the same real, already project-isolated, already-deduplicated stream
 * every other real page (`AuditPage`/`EvidencePage`/`ExecutionHubPage`/
 * `DashboardPage`/`ReportsPage`) already consumes. No new WebSocket, no new
 * backend route, no new project-scoping mechanism: this is simply another
 * subscriber. `RealtimeContext` itself only *stops delivering* an old
 * project's events on switch — it does not clear a consumer's own retained
 * list — so this component owns that reset itself (`useEffect` keyed on
 * `project.selectedProjectId`), the same responsibility every other
 * project-scoped page already takes for its own local state.
 */

import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Menu,
  Search,
  Plus,
  Bell,
  Layers,
  ChevronDown,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { useAuth } from '../../context/AuthContext';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import type { RealtimeConnectionState } from '../../context/RealtimeContext';
import { Environment } from '../../types';
import type { RunEvent } from '../../types/contract';
import { formatStatusLabel } from '../../utils/statusLabel';
import { ProjectSelector } from '../project/ProjectSelector';

/** Bounded so a long-lived session doesn't grow this forever — matches
 *  `RealtimeContext.tsx`'s own `DEDUP_CACHE_SIZE` reasoning (a small,
 *  fixed window is enough for a recent-activity glance, not a history). */
const MAX_RECENT_ACTIVITY = 10;

/** Same label convention `ExecutionHubPage.tsx` already established for
 *  this exact real `RealtimeConnectionState` — duplicated locally per this
 *  codebase's established per-file-formatter convention rather than
 *  extracted into a new shared module. */
const CONNECTION_LABEL: Record<RealtimeConnectionState, string> = {
  disconnected: 'Realtime Off',
  connecting: 'Connecting…',
  connected: 'Live',
  reconnecting: 'Reconnecting…',
  error: 'Realtime Error',
};

function formatEventTimestamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { month: 'short', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

export const Header: React.FC = () => {
  const {
    toggleSidebar,
    currentEnvironment,
    setCurrentEnvironment,
    setCommandPaletteOpen,
  } = useAIQA();
  const project = useProject();
  const realtime = useRealtime();
  const [recentActivity, setRecentActivity] = useState<RunEvent[]>([]);

  // Real events only — `realtime.subscribe` already delivers exclusively
  // this project's own, already-deduplicated events (server-side per
  // `ws_tests.rs::be14_subscribers_receive_only_their_own_projects_
  // real_events`, plus `RealtimeContext`'s own client-side `project_id`
  // check) — no second filter/dedup is invented here.
  useEffect(() => {
    return realtime.subscribe((event) => {
      setRecentActivity((prev) => [event, ...prev].slice(0, MAX_RECENT_ACTIVITY));
    });
  }, [realtime.subscribe]);

  // A project switch stops *new* foreign-project events from arriving, but
  // does not retroactively clear whatever this component already retained
  // from the previously-selected project — that reset is this component's
  // own responsibility (brief §6/§18: "Project A events must not remain
  // visible as active notifications for Project B").
  useEffect(() => {
    setRecentActivity([]);
  }, [project.selectedProjectId]);
  // FE-11 — this header's own daemon-health indicator used to be
  // `AIQAContext`'s fabricated `backendState` (a static "Daemon Online,
  // 18ms" that never actually reflected the real backend — live-caught
  // during FE-10 manual verification: it kept reading "Daemon Online"
  // seconds after the real orchestrator process had been killed). It now
  // reads the real, already-computed `AuthContext.status` instead — no new
  // connectivity check, no second polling loop, just the same real signal
  // every protected route already gates on.
  const auth = useAuth();
  const navigate = useNavigate();

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
        {/* Real backend connectivity — mirrors `AuthContext.status`, the
            same real signal every protected route already gates on. Only
            'authenticated' is genuinely "online"; every other status is an
            honest "not confirmed" rather than a fabricated version/latency. */}
        <div className="relative">
          <button
            id="backend-health-button"
            onClick={() => setIsHealthDetailOpen(!isHealthDetailOpen)}
            className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-[#020617] hover:bg-slate-900 border border-slate-800 text-xs text-slate-300 transition-colors cursor-pointer"
            title="Real orchestrator connection status"
          >
            <span className="relative flex h-2 w-2">
              <span
                className={`animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 ${
                  auth.status === 'authenticated' ? 'bg-emerald-400' : 'bg-rose-400'
                }`}
              />
              <span
                className={`relative inline-flex rounded-full h-2 w-2 ${
                  auth.status === 'authenticated' ? 'bg-emerald-500' : 'bg-rose-500'
                }`}
              />
            </span>
            <span className="hidden xl:inline text-slate-400 font-mono text-[11px]">
              {auth.status === 'authenticated' ? 'Orchestrator Online' : 'Not Confirmed Online'}
            </span>
          </button>

          {isHealthDetailOpen && (
            <div
              className="absolute right-0 mt-1.5 w-72 rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl p-3 z-50 text-xs"
              onMouseLeave={() => setIsHealthDetailOpen(false)}
            >
              <div className="flex items-center justify-between pb-2 border-b border-slate-800 mb-2">
                <span className="font-semibold text-slate-200">Real Connection Status</span>
                <span
                  className={`text-[10px] font-mono px-2 py-0.5 rounded font-medium ${
                    auth.status === 'authenticated'
                      ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                      : 'bg-rose-950 text-rose-300 border border-rose-800'
                  }`}
                >
                  {auth.status.toUpperCase()}
                </span>
              </div>
              <div className="space-y-1 text-slate-400 text-[11px] font-mono">
                <div className="flex justify-between">
                  <span>API key:</span>
                  <span className="text-slate-200">{auth.maskedKey ?? 'not configured'}</span>
                </div>
                <div className="flex justify-between">
                  <span>Status:</span>
                  <span className="text-slate-200">
                    {auth.status === 'authenticated'
                      ? 'A real protected request succeeded this session.'
                      : 'Not yet confirmed against the real backend.'}
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Trigger Run Button — FE-11: navigates to the real Execution Hub
            instead of opening `AIQAContext`'s fabricated `RunTriggerModal`
            (deleted this sprint — see the FE-11 report §7/§24). There is
            only one real execution-creation surface in this app now. */}
        <button
          id="launch-run-header-button"
          onClick={() => navigate('/execution')}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs transition-all shadow-sm shadow-indigo-950 cursor-pointer"
        >
          <Plus className="w-3.5 h-3.5 stroke-[2.5]" />
          <span className="hidden sm:inline font-sans">Launch Run</span>
        </button>

        {/* Notifications — FE-24: real recent execution activity, not a
            fabricated alert stream. See this file's own module doc comment. */}
        <div className="relative">
          <button
            id="notifications-button"
            onClick={() => setIsNotificationsOpen(!isNotificationsOpen)}
            className="p-1.5 rounded-md text-slate-400 hover:text-slate-200 hover:bg-slate-800/80 transition-colors relative cursor-pointer"
            title="Recent Activity"
          >
            <Bell className="w-4 h-4" />
            {recentActivity.length > 0 && (
              <span id="notification-badge-dot" className="absolute top-1 right-1 w-2 h-2 rounded-full bg-indigo-400 ring-2 ring-[#0B1120]" />
            )}
          </button>

          {isNotificationsOpen && (
            <div
              id="recent-activity-panel"
              className="absolute right-0 mt-1.5 w-80 rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl p-3 z-50 text-xs"
              onMouseLeave={() => setIsNotificationsOpen(false)}
            >
              <div className="flex items-center justify-between pb-2 border-b border-slate-800 mb-2">
                <span className="font-semibold text-slate-200">Recent Activity</span>
                <span className="text-[10px] text-slate-400 font-mono">{CONNECTION_LABEL[realtime.connectionState]}</span>
              </div>
              {recentActivity.length === 0 ? (
                <div className="py-4 text-center">
                  <p className="text-slate-300 font-medium">No recent activity</p>
                  <p className="text-[10px] text-slate-500 mt-1">
                    Real execution activity will appear here when events are received.
                  </p>
                </div>
              ) : (
                <div className="space-y-1.5 max-h-64 overflow-y-auto pr-1">
                  {recentActivity.map((event) => (
                    <div key={event.event_id} className="p-2 rounded-md bg-[#020617] border border-slate-800">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono font-medium text-[10px] uppercase text-indigo-300">
                          {formatStatusLabel(event.type)}
                        </span>
                        <span className="text-[9px] text-slate-500 font-mono shrink-0">{formatEventTimestamp(event.timestamp)}</span>
                      </div>
                      <p className="text-[10px] text-slate-400 mt-0.5">{event.summary ?? '—'}</p>
                      <p className="text-[9px] text-slate-600 font-mono mt-0.5 truncate">
                        Run {event.run_id.slice(0, 8)}
                        {event.task_id ? ` · Task ${event.task_id}` : ''}
                      </p>
                    </div>
                  ))}
                </div>
              )}
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
