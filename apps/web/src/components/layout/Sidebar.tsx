/**
 * AIQA Sidebar Navigation Component
 * Structured into Connected Control Plane vs Staged Quality Engineering Workspaces.
 */

import React from 'react';
import {
  LayoutDashboard,
  Sparkles,
  Workflow,
  CheckCheck,
  PlayCircle,
  ShieldCheck,
  Network,
  Smartphone,
  Sliders,
  Layers,
  FileText,
  Settings,
  ChevronLeft,
  ChevronRight,
  Activity,
  Cpu,
  Bug,
  ScrollText,
  KeyRound,
  FileSearch,
  ListChecks,
  Brain,
  Radio,
  FolderKanban,
  ShieldAlert,
} from 'lucide-react';
import type { ViewId } from '../../App';

export interface NavItemDef {
  id: ViewId;
  label: string;
  icon: React.ComponentType<{ className?: string; size?: number }>;
  badge?: string;
  badgeType?: 'live' | 'staged' | 'info';
  isLive?: boolean;
}

export const LIVE_NAV_ITEMS: NavItemDef[] = [
  { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard, isLive: true },
  { id: 'runs', label: 'Execution Hub', icon: PlayCircle, badge: 'LIVE', badgeType: 'live', isLive: true },
  { id: 'intelligence', label: 'Failure Intelligence', icon: Activity, isLive: true },
  { id: 'bug-drafts', label: 'Bug Drafts / RCA', icon: Bug, isLive: true },
  { id: 'audit-log', label: 'Audit Log', icon: ScrollText, isLive: true },
  { id: 'settings', label: 'Settings & Auth', icon: Settings, isLive: true },
];

export const STAGED_NAV_ITEMS: NavItemDef[] = [
  { id: 'projects', label: 'Projects & Workspaces', icon: FolderKanban, badge: 'STAGED', badgeType: 'staged' },
  { id: 'quality-graph', label: 'Requirements & Traceability', icon: Layers, badge: 'STAGED', badgeType: 'staged' },
  { id: 'test-generator', label: 'Test Case Generator', icon: Sparkles, badge: 'AI', badgeType: 'staged' },
  { id: 'test-validator', label: 'Test Case Validator', icon: CheckCheck, badge: 'GATE', badgeType: 'staged' },
  { id: 'test-suites', label: 'Test Suites & Plans', icon: ListChecks, badge: 'STAGED', badgeType: 'staged' },
  { id: 'orchestrator-plan', label: 'AIQA Orchestrator Planner', icon: Workflow, badge: 'PLAN', badgeType: 'staged' },
  { id: 'rbac', label: 'RBAC Testing Matrix', icon: ShieldCheck, badge: 'STAGED', badgeType: 'staged' },
  { id: 'api-testing', label: 'API Testing Console', icon: Network, badge: 'STAGED', badgeType: 'staged' },
  { id: 'mobile-testing', label: 'Mobile Testing Hub', icon: Smartphone, badge: 'STAGED', badgeType: 'staged' },
  { id: 'custom-testing', label: 'Custom Testing Suites', icon: Sliders, badge: 'STAGED', badgeType: 'staged' },
  { id: 'reports', label: 'Reports & Exports', icon: FileText, badge: 'STAGED', badgeType: 'staged' },
  { id: 'model-gateway', label: 'Model Gateway & Telemetry', icon: Brain, badge: 'STAGED', badgeType: 'staged' },
];

interface SidebarProps {
  currentView: ViewId;
  onSelectView: (view: ViewId) => void;
  isCollapsed: boolean;
  onToggleCollapse: () => void;
  activeRunCount?: number;
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentView,
  onSelectView,
  isCollapsed,
  onToggleCollapse,
  activeRunCount = 0,
}) => {
  return (
    <aside
      id="main-sidebar"
      className={`relative flex flex-col border-r border-slate-800/90 bg-[#0B1120] transition-all duration-200 z-20 select-none ${
        isCollapsed ? 'w-16' : 'w-64'
      }`}
    >
      {/* Top Workspace Identity Section */}
      <div className="p-3 border-b border-slate-800/80 flex items-center justify-between">
        {!isCollapsed ? (
          <div className="flex items-center gap-2.5 overflow-hidden">
            <div className="w-6 h-6 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-cyan-400 font-mono text-[11px] font-bold">
              QA
            </div>
            <div className="truncate">
              <div className="text-xs font-semibold text-slate-200 truncate">
                AI Quality Orchestrator
              </div>
              <div className="text-[10px] font-mono text-slate-500 truncate">
                Control Plane v2.0
              </div>
            </div>
          </div>
        ) : (
          <div className="w-full flex justify-center">
            <div className="w-7 h-7 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-cyan-400 font-mono text-xs font-bold">
              QA
            </div>
          </div>
        )}

        <button
          onClick={onToggleCollapse}
          className="hidden md:flex p-1 rounded-md text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors cursor-pointer"
          title={isCollapsed ? 'Expand Sidebar (Ctrl+B)' : 'Collapse Sidebar (Ctrl+B)'}
        >
          {isCollapsed ? (
            <ChevronRight className="w-4 h-4" />
          ) : (
            <ChevronLeft className="w-4 h-4" />
          )}
        </button>
      </div>

      {/* Navigation Links */}
      <nav className="flex-1 py-2.5 px-2 space-y-4 overflow-y-auto">
        {/* Section 1: Live Control Plane */}
        <div className="space-y-0.5">
          {!isCollapsed && (
            <div className="px-2 py-1 text-[9px] font-mono uppercase tracking-widest text-cyan-400/80 font-bold flex items-center justify-between">
              <span>Live Control Plane</span>
              <span className="flex h-1.5 w-1.5 rounded-full bg-emerald-400" />
            </div>
          )}
          {LIVE_NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = currentView === item.id;

            return (
              <button
                key={item.id}
                id={`nav-item-${item.id}`}
                onClick={() => onSelectView(item.id)}
                className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-xs font-medium transition-all group relative cursor-pointer ${
                  isActive
                    ? 'bg-cyan-600/90 text-white font-semibold shadow-sm shadow-cyan-950/60'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/60 border border-transparent'
                }`}
                title={isCollapsed ? item.label : undefined}
              >
                {isActive && (
                  <div className="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-cyan-300 shadow-[0_0_8px_rgba(34,211,238,0.8)]" />
                )}

                <Icon
                  className={`w-4 h-4 shrink-0 transition-colors ${
                    isActive ? 'text-white' : 'text-slate-400 group-hover:text-slate-200'
                  }`}
                />

                {!isCollapsed && (
                  <span className="flex-1 text-left truncate font-sans tracking-tight">
                    {item.label}
                  </span>
                )}

                {!isCollapsed && item.badge && (
                  <span
                    className={`text-[9px] font-mono px-1.5 py-0.2 rounded font-semibold uppercase tracking-wider shrink-0 ${
                      isActive
                        ? 'bg-cyan-700 text-white'
                        : item.badgeType === 'live'
                        ? 'bg-emerald-950 text-emerald-300 border border-emerald-800/70 animate-pulse'
                        : 'bg-slate-800 text-slate-400 border border-slate-700'
                    }`}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </div>

        {/* Section 2: Staged Workspaces */}
        <div className="space-y-0.5">
          {!isCollapsed && (
            <div className="px-2 py-1 text-[9px] font-mono uppercase tracking-widest text-slate-500 font-bold">
              Quality Workspaces
            </div>
          )}
          {STAGED_NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = currentView === item.id;

            return (
              <button
                key={item.id}
                id={`nav-item-${item.id}`}
                onClick={() => onSelectView(item.id)}
                className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-xs font-medium transition-all group relative cursor-pointer ${
                  isActive
                    ? 'bg-slate-800 text-cyan-300 font-semibold border border-slate-700'
                    : 'text-slate-400 hover:text-slate-300 hover:bg-slate-800/40 border border-transparent'
                }`}
                title={isCollapsed ? `${item.label} (Staged)` : undefined}
              >
                {isActive && (
                  <div className="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-slate-400" />
                )}

                <Icon
                  className={`w-4 h-4 shrink-0 transition-colors ${
                    isActive ? 'text-cyan-300' : 'text-slate-500 group-hover:text-slate-400'
                  }`}
                />

                {!isCollapsed && (
                  <span className="flex-1 text-left truncate font-sans tracking-tight text-slate-400">
                    {item.label}
                  </span>
                )}

                {!isCollapsed && item.badge && (
                  <span className="text-[9px] font-mono px-1 py-0.2 rounded bg-slate-900 border border-slate-800 text-slate-500 tracking-wider shrink-0">
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </nav>

      {/* Runner Pool / Telemetry Bottom Box */}
      <div className="p-2.5 border-t border-slate-800/80 bg-[#020617]">
        {!isCollapsed ? (
          <div className="p-2 rounded-md bg-[#0B1120] border border-slate-800 space-y-1.5">
            <div className="flex items-center justify-between text-[11px]">
              <div className="flex items-center gap-1.5 text-slate-300 font-medium font-mono text-[10px]">
                <Cpu className="w-3.5 h-3.5 text-cyan-400" />
                <span>Rust Engine :8080</span>
              </div>
              <span className="text-[10px] font-mono text-emerald-400">Connected</span>
            </div>
            <div className="w-full bg-slate-800 rounded-full h-1 overflow-hidden">
              <div
                className="bg-gradient-to-r from-cyan-500 to-indigo-500 h-1 rounded-full"
                style={{ width: '100%' }}
              />
            </div>
          </div>
        ) : (
          <div
            className="flex justify-center p-1.5 text-cyan-400 cursor-pointer"
            title="Backend Engine: Rust Orchestrator :8080"
          >
            <Activity className="w-4 h-4" />
          </div>
        )}
      </div>
    </aside>
  );
};
