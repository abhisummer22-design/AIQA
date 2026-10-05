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

import React, { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Menu,
  Search,
  Plus,
  Bell,
  Layers,
  ChevronDown,
  Moon,
  Sun,
} from 'lucide-react';
import { matchesMedia } from './Sidebar';
import { useAIQA } from '../../context/AIQAContext';
import { useAuth } from '../../context/AuthContext';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import { useTheme } from '../../context/ThemeContext';
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
  const { theme, toggleTheme } = useTheme();

  const [isEnvDropdownOpen, setIsEnvDropdownOpen] = useState(false);
  const [isHealthDetailOpen, setIsHealthDetailOpen] = useState(false);
  const [isNotificationsOpen, setIsNotificationsOpen] = useState(false);

  const envOptions: Array<{ id: Environment; label: string; badge: string; color: string }> = [
    { id: 'demo', label: 'DEMO', badge: 'Active', color: 'text-amber-400 border-amber-800/60 bg-amber-950/40' },
    { id: 'uat', label: 'UAT', badge: 'v4.18', color: 'text-yellow-400 border-yellow-800/60 bg-yellow-950/40' },
    { id: 'live', label: 'LIVE', badge: 'US-East', color: 'text-emerald-400 border-emerald-800/60 bg-emerald-950/40' },
  ];

  const currentEnvConfig = envOptions.find((e) => e.id === currentEnvironment) || envOptions[0];

  /**
   * All three header popovers closed on `onMouseLeave` and nothing else.
   *
   * That is not a keyboard path at all: `mouseleave` never fires for someone who
   * never used a mouse, so opening the environment selector, the backend-health
   * detail or the notifications list with Enter left it stuck open with no way
   * to shut it -- no Escape, no outside click. WCAG 2.1.1 Keyboard (A).
   *
   * One listener for all three rather than three, because a single
   * `keydown`/`pointerdown` pair at the document is the only version of this
   * that cannot drift out of sync with the next dropdown added.
   *
   * `pointerdown` rather than `click`, so a press that starts outside and ends
   * inside still counts as outside, and so it runs before the click handler that
   * would otherwise re-open the popover it is closing.
   */
  const anyPopoverOpen = isEnvDropdownOpen || isHealthDetailOpen || isNotificationsOpen;
  const closeAllPopovers = useCallback(() => {
    setIsEnvDropdownOpen(false);
    setIsHealthDetailOpen(false);
    setIsNotificationsOpen(false);
  }, []);

  useEffect(() => {
    if (!anyPopoverOpen) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeAllPopovers();
    };
    const onPointerDown = (event: Event) => {
      const target = event.target;
      // A press inside a popover's own positioned wrapper is not an outside
      // click. `closest` rather than a single ref, because each popover wraps its
      // own trigger separately.
      if (target instanceof Element && target.closest('[data-header-popover-root]')) return;
      closeAllPopovers();
    };

    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('pointerdown', onPointerDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('pointerdown', onPointerDown);
    };
  }, [anyPopoverOpen, closeAllPopovers]);

  /**
   * No `overflow-x` on this header, deliberately -- it was `overflow-x-auto`
   * before, and that is worse than it looks.
   *
   * `overflow-x: auto` on a `position: sticky` element reserves a horizontal
   * scrollbar gutter whether or not anything overflows, so the header renders
   * with its content visibly shifted and a second copy of the right-hand
   * controls peeking in at the edge. That is what the earlier fix produced: it
   * made an overflow *reachable* at the cost of looking wrong at every width.
   *
   * Reachability does not need it here. The header's contents already respond
   * to width -- the environment selector and project selector are `hidden`
   * below `md`, the search field is `hidden md:flex`, and the Launch Run label
   * collapses to its icon below `sm`. So at narrow widths the header holds
   * fewer controls rather than wider ones, and `min-w-0` on the flexible group
   * lets the search field shrink instead of pushing the rest past the edge.
   *
   * The app shell clips with `overflow-hidden` either way (`index.css`
   * `body { overflow-x: hidden }`), so a regression that somehow reintroduced a
   * too-wide element would clip rather than silently scroll -- and
   * `headerMustNotScrollHorizontally` below fails if `overflow-x` ever comes
   * back, which is the part this comment cannot enforce on its own.
   */
  return (
    <header
      data-testid="app-header"
      className="h-13 border-b border-border-default/90 bg-surface-card px-3 sm:px-4 flex items-center justify-between gap-3 sticky top-0 z-30 select-none"
    >
      {/* Left: Hamburger & Brand */}
      <div className="flex items-center gap-2.5">
        <button
          id="sidebar-toggle-button"
          // Dispatches an event rather than toggling directly: below 768px the
          // sidebar is an overlay drawer and this button has to *open* it,
          // whereas above it the same button collapses the column. The sidebar
          // listens and decides, so the breakpoint lives in exactly one place.
          onClick={() => {
            // `matchesMedia` rather than `window.matchMedia` directly: the guarded
            // helper, because jsdom has no matchMedia and an unguarded call here
            // takes down every test that renders the header.
            if (matchesMedia('(max-width: 767px)')) {
              window.dispatchEvent(new Event('aiqa:open-sidebar'));
            } else {
              toggleSidebar();
            }
          }}
          className="p-1.5 rounded-md text-content-secondary hover:text-content-primary hover:bg-slate-800/80 transition-colors cursor-pointer md:hidden"
          aria-label="Open navigation menu"
          aria-controls="main-sidebar"
          title="Open Navigation Menu"
        >
          <Menu className="w-4 h-4" />
        </button>

        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-md bg-gradient-to-br from-violet-500 to-yellow-500 p-[1px] shadow-sm shadow-violet-950">
            <div className="w-full h-full bg-surface-page rounded-[5px] flex items-center justify-center">
              <span className="font-mono font-black text-xs text-violet-400 tracking-tighter">AI</span>
            </div>
          </div>
          <div className="flex flex-col">
            <div className="flex items-center gap-1.5">
              <span className="font-bold text-xs tracking-tight text-content-primary font-sans">
                AIQA
              </span>
              <span className="text-[9px] font-mono uppercase px-1.5 py-0.2 rounded bg-violet-950/80 text-violet-300 border border-violet-800/60 font-semibold tracking-wider">
                Control Plane
              </span>
            </div>
          </div>
        </div>

        {/* Vertical divider */}
        <div className="hidden sm:block h-4 w-[1px] bg-border-default mx-1" />

        {/* Environment Selector Dropdown */}
        <div className="relative hidden md:block">
          <button
            id="env-selector-button"
            aria-expanded={isEnvDropdownOpen}
            aria-haspopup="menu"
            onClick={() => setIsEnvDropdownOpen(!isEnvDropdownOpen)}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-surface-page hover:bg-slate-900 border border-border-default text-xs text-content-primary transition-colors cursor-pointer"
          >
            <Layers className="w-3.5 h-3.5 text-content-secondary" />
            <span className="font-medium">{currentEnvConfig.label}</span>
            <span className={`text-[10px] font-mono px-1.5 py-0.2 rounded border ${currentEnvConfig.color}`}>
              {currentEnvConfig.badge}
            </span>
            <ChevronDown className="w-3 h-3 text-content-muted ml-0.5" />
          </button>

          {isEnvDropdownOpen && (
            <div
              className="absolute left-0 mt-1.5 w-60 rounded-lg bg-surface-card border border-slate-700/80 shadow-2xl p-1.5 z-50 text-xs"
              data-header-popover-root={""}
              onMouseLeave={() => setIsEnvDropdownOpen(false)}
            >
              <div className="px-2.5 py-1 text-[10px] font-mono text-content-secondary uppercase tracking-wider font-semibold">
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
                      ? 'bg-slate-800/90 text-content-primary font-semibold'
                      : 'text-content-secondary hover:text-content-primary hover:bg-slate-800/50'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <div
                      className={`w-1.5 h-1.5 rounded-full ${
                        currentEnvironment === env.id ? 'bg-violet-400' : 'bg-slate-600'
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
        {/* One selector per breakpoint, never two. The mobile copy has to be
            hidden from `md` up and the desktop copy hidden below it; rendering
            the mobile copy unwrapped showed both side by side on a desktop
            window, so the same project appeared in two identical dropdowns. */}
        <div className="md:hidden">
          <ProjectSelector />
        </div>
        <div className="hidden md:block"><ProjectSelector /></div>
      </div>

      {/* Center: Command Palette Trigger.
          `min-w-0`: a flex item's default `min-width: auto` stops it shrinking
          below its content, so the search button's placeholder text would hold
          the whole row open and push the right-hand controls past the edge
          instead of the search field giving way. */}
      <div className="hidden md:flex items-center flex-1 max-w-md mx-2 min-w-0">
        <button
          id="global-search-button"
          onClick={() => setCommandPaletteOpen(true)}
          className="w-full flex items-center justify-between px-3 py-1.5 rounded-md bg-surface-page hover:bg-slate-900 border border-border-default/90 text-xs text-content-secondary transition-all group cursor-pointer"
        >
          <div className="flex items-center gap-2 min-w-0">
            <Search className="w-3.5 h-3.5 shrink-0 text-content-muted group-hover:text-content-secondary" />
            {/* `truncate` so the placeholder gives way before the row can be
                pushed wider than the header. Without it this one span sets the
                header's minimum width at every viewport size. */}
            <span className="truncate group-hover:text-content-secondary text-xs">Quick search runs, tests, RBAC rules...</span>
          </div>
          <div className="flex items-center gap-1 font-mono text-[10px] text-content-secondary bg-slate-800/90 px-1.5 py-0.5 rounded border border-slate-700">
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
                aria-expanded={isHealthDetailOpen}
                aria-haspopup="true"
            onClick={() => setIsHealthDetailOpen(!isHealthDetailOpen)}
            className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-surface-page hover:bg-slate-900 border border-border-default text-xs text-content-secondary transition-colors cursor-pointer"
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
            <span className="hidden xl:inline text-content-secondary font-mono text-[11px]">
              {auth.status === 'authenticated' ? 'Orchestrator Online' : 'Not Confirmed Online'}
            </span>
          </button>

          {isHealthDetailOpen && (
            <div
              className="absolute right-0 mt-1.5 w-72 rounded-lg bg-surface-card border border-slate-700/80 shadow-2xl p-3 z-50 text-xs"
              data-header-popover-root={""}
              onMouseLeave={() => setIsHealthDetailOpen(false)}
            >
              <div className="flex items-center justify-between pb-2 border-b border-border-default mb-2">
                <span className="font-semibold text-content-primary">Real Connection Status</span>
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
              <div className="space-y-1 text-content-secondary text-[11px] font-mono">
                <div className="flex justify-between">
                  <span>API key:</span>
                  <span className="text-content-primary">{auth.maskedKey ?? 'not configured'}</span>
                </div>
                <div className="flex justify-between">
                  <span>Status:</span>
                  <span className="text-content-primary">
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
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-semibold text-xs transition-all shadow-sm shadow-violet-950 cursor-pointer"
        >
          <Plus className="w-3.5 h-3.5 stroke-[2.5]" />
          <span className="hidden sm:inline font-sans">Launch Run</span>
        </button>

        {/* Notifications — FE-24: real recent execution activity, not a
            fabricated alert stream. See this file's own module doc comment. */}
        <div className="relative">
          <button
            id="notifications-button"
                aria-expanded={isNotificationsOpen}
                aria-haspopup="true"
            onClick={() => setIsNotificationsOpen(!isNotificationsOpen)}
            className="p-1.5 rounded-md text-content-secondary hover:text-content-primary hover:bg-slate-800/80 transition-colors relative cursor-pointer"
            title="Recent Activity"
          >
            <Bell className="w-4 h-4" />
            {recentActivity.length > 0 && (
              <span id="notification-badge-dot" className="absolute top-1 right-1 w-2 h-2 rounded-full bg-violet-400 ring-2 ring-surface-card" />
            )}
          </button>

          {isNotificationsOpen && (
            <div
              id="recent-activity-panel"
              className="absolute right-0 mt-1.5 w-80 rounded-lg bg-surface-card border border-slate-700/80 shadow-2xl p-3 z-50 text-xs"
              data-header-popover-root={""}
              onMouseLeave={() => setIsNotificationsOpen(false)}
            >
              <div className="flex items-center justify-between pb-2 border-b border-border-default mb-2">
                <span className="font-semibold text-content-primary">Recent Activity</span>
                <span className="text-[10px] text-content-secondary font-mono">{CONNECTION_LABEL[realtime.connectionState]}</span>
              </div>
              {recentActivity.length === 0 ? (
                <div className="py-4 text-center">
                  <p className="text-content-secondary font-medium">No recent activity</p>
                  <p className="text-[10px] text-content-muted mt-1">
                    Real execution activity will appear here when events are received.
                  </p>
                </div>
              ) : (
                <div className="space-y-1.5 max-h-64 overflow-y-auto pr-1">
                  {recentActivity.map((event) => (
                    <div key={event.event_id} className="p-2 rounded-md bg-surface-page border border-border-default">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono font-medium text-[10px] uppercase text-violet-300">
                          {formatStatusLabel(event.type)}
                        </span>
                        <span className="text-[9px] text-content-muted font-mono shrink-0">{formatEventTimestamp(event.timestamp)}</span>
                      </div>
                      <p className="text-[10px] text-content-secondary mt-0.5">{event.summary ?? '—'}</p>
                      <p className="text-[9px] text-content-muted font-mono mt-0.5 truncate">
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

        {/* Phase 6 gap-closing — real light/dark toggle. Icon shows the
            theme a click would switch *to* (sun while dark, moon while
            light), the same convention most theme toggles use. */}
        <button
          id="theme-toggle-button"
          onClick={toggleTheme}
          className="p-1.5 rounded-md text-content-secondary hover:text-content-primary hover:bg-slate-800/80 transition-colors cursor-pointer"
          title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {theme === 'dark' ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
        </button>

        {/* User Identity Context */}
        <div className="flex items-center gap-1.5 pl-1 border-l border-border-default">
          <div className="w-6 h-6 rounded-md bg-slate-800 border border-slate-700 flex items-center justify-center text-[10px] font-semibold text-violet-300 font-mono">
            QA
          </div>
        </div>
      </div>
    </header>
  );
};
