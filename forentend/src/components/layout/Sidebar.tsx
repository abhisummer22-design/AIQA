/**
 * AIQA Sidebar Navigation Component
 * Contains strictly the 8 primary navigation items with expanded/collapsed modes
 *
 * FE-4: the nav item list is no longer hardcoded here — it's read from
 * `routes/routeConfig.tsx`, the single source every navigation surface
 * shares. Active state and click-navigation are driven by the real URL
 * (`useLocation`/`useNavigate`), not the old in-memory `activePage`.
 */

import React from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { ChevronLeft, ChevronRight, Activity, Cpu } from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { routeDefs } from '../../routes/routeConfig';

export const Sidebar: React.FC = () => {
  const { isSidebarCollapsed, toggleSidebar, runs, settings } = useAIQA();
  const navigate = useNavigate();
  const location = useLocation();

  // Find running count for live indicator
  const runningCount = runs.filter((r) => r.status === 'RUNNING').length;

  return (
    <aside
      id="main-sidebar"
      className={`relative flex flex-col border-r border-slate-800/90 bg-[#0B1120] transition-all duration-200 z-20 select-none ${
        isSidebarCollapsed ? 'w-16' : 'w-64'
      }`}
    >
      {/* Top Workspace Identity Section */}
      <div className="p-3 border-b border-slate-800/80 flex items-center justify-between">
        {!isSidebarCollapsed ? (
          <div className="flex items-center gap-2.5 overflow-hidden">
            <div className="w-6 h-6 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-indigo-400 font-mono text-[11px] font-bold">
              QE
            </div>
            <div className="truncate">
              <div className="text-xs font-semibold text-slate-200 truncate">
                {settings.general.organizationName}
              </div>
              <div className="text-[10px] font-mono text-slate-500 truncate">
                Project: {settings.general.projectKey}
              </div>
            </div>
          </div>
        ) : (
          <div className="w-full flex justify-center">
            <div className="w-7 h-7 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-indigo-400 font-mono text-xs font-bold">
              QE
            </div>
          </div>
        )}

        <button
          onClick={toggleSidebar}
          className="hidden md:flex p-1 rounded-md text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors cursor-pointer"
          title={isSidebarCollapsed ? 'Expand Sidebar (Ctrl+B)' : 'Collapse Sidebar (Ctrl+B)'}
        >
          {isSidebarCollapsed ? (
            <ChevronRight className="w-4 h-4" />
          ) : (
            <ChevronLeft className="w-4 h-4" />
          )}
        </button>
      </div>

      {/* Navigation Links (Strictly the 8 required items) */}
      <nav className="flex-1 py-2.5 px-2 space-y-0.5 overflow-y-auto">
        {!isSidebarCollapsed && (
          <div className="px-2 py-1 text-[10px] font-mono uppercase tracking-widest text-slate-500 font-bold">
            Navigation Plane
          </div>
        )}
        {routeDefs.map((item) => {
          const Icon = item.icon;
          const isActive = location.pathname === item.path;

          return (
            <button
              key={item.id}
              id={`nav-item-${item.id}`}
              onClick={() => navigate(item.path)}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-xs font-medium transition-all group relative cursor-pointer ${
                isActive
                  ? 'bg-indigo-600 text-white font-semibold shadow-sm shadow-indigo-950/60'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/60 border border-transparent'
              }`}
              title={isSidebarCollapsed ? item.label : undefined}
            >
              {/* Active neon accent pill */}
              {isActive && (
                <div className="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-indigo-300 shadow-[0_0_8px_rgba(165,180,252,0.8)]" />
              )}

              <Icon
                className={`w-4 h-4 shrink-0 transition-colors ${
                  isActive
                    ? 'text-white'
                    : 'text-slate-400 group-hover:text-slate-200'
                }`}
              />

              {!isSidebarCollapsed && (
                <span className="flex-1 text-left truncate font-sans tracking-tight">
                  {item.label}
                </span>
              )}

              {!isSidebarCollapsed && item.badge && (
                <span
                  className={`text-[10px] font-mono px-1.5 py-0.2 rounded font-semibold uppercase tracking-wider shrink-0 ${
                    isActive
                      ? 'bg-indigo-700/80 text-white'
                      : item.badgeType === 'live' && runningCount > 0
                      ? 'bg-indigo-950 text-indigo-300 border border-indigo-800 animate-pulse'
                      : 'bg-slate-800 text-slate-400 border border-slate-700'
                  }`}
                >
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </nav>

      {/* Runner Pool / Telemetry Bottom Box */}
      <div className="p-2.5 border-t border-slate-800/80 bg-[#020617]">
        {!isSidebarCollapsed ? (
          <div className="p-2 rounded-md bg-[#0B1120] border border-slate-800 space-y-1.5">
            <div className="flex items-center justify-between text-[11px]">
              <div className="flex items-center gap-1.5 text-slate-300 font-medium">
                <Cpu className="w-3.5 h-3.5 text-indigo-400" />
                <span>k8s-runner-pool-us</span>
              </div>
              <span className="text-[10px] font-mono text-emerald-400">12 / 16 slots</span>
            </div>
            <div className="w-full bg-slate-800 rounded-full h-1.5 overflow-hidden">
              <div
                className="bg-gradient-to-r from-indigo-500 to-cyan-400 h-1.5 rounded-full"
                style={{ width: '75%' }}
              />
            </div>
          </div>
        ) : (
          <div
            className="flex justify-center p-1.5 text-indigo-400 cursor-pointer"
            title="Cluster Capacity: 12/16 active runners"
          >
            <Activity className="w-4 h-4" />
          </div>
        )}
      </div>
    </aside>
  );
};
