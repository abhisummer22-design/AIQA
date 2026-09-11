/**
 * AIQA Sidebar Navigation Component
 * Contains strictly the 8 primary navigation items with expanded/collapsed modes
 *
 * FE-4: the nav item list is no longer hardcoded here — it's read from
 * `routes/routeConfig.tsx`, the single source every navigation surface
 * shares. Active state and click-navigation are driven by the real URL
 * (`useLocation`/`useNavigate`), not the old in-memory `activePage`.
 *
 * FE-11: removed two fabricated elements — the workspace-identity block
 * used to show `AIQAContext`'s fake `settings.general.organizationName`/
 * `.projectKey` ("Acme Enterprise Engineering" / "AIQA-CORE") regardless of
 * which real project was selected; it now shows the real
 * `useProject().selectedProject`. The bottom "k8s-runner-pool-us, 12/16
 * slots, 75%" box was 100% hardcoded, tied to no state at all — removed
 * rather than wired to a fake number, since no real runner-pool/capacity
 * concept exists in the backend. The Execution Hub nav item's "LIVE" badge
 * no longer pulses off `AIQAContext`'s fake seeded `runs` array (a fake
 * RUNNING run that never actually completes) — it is now a plain static
 * label, consistent with the other nav items' static badges ("AI", "PLAN",
 * "GATE").
 */

import React from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { useProject } from '../../context/ProjectContext';
import { routeDefs } from '../../routes/routeConfig';

export const Sidebar: React.FC = () => {
  const { isSidebarCollapsed, toggleSidebar } = useAIQA();
  const { selectedProject } = useProject();
  const navigate = useNavigate();
  const location = useLocation();

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
            <div className="w-6 h-6 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-violet-400 font-mono text-[11px] font-bold">
              QE
            </div>
            <div className="truncate">
              <div className="text-xs font-semibold text-slate-200 truncate">
                {selectedProject ? selectedProject.name : 'AIQA'}
              </div>
              <div className="text-[10px] font-mono text-slate-500 truncate">
                {selectedProject ? `Project: ${selectedProject.id.slice(0, 8)}` : 'No active project'}
              </div>
            </div>
          </div>
        ) : (
          <div className="w-full flex justify-center">
            <div className="w-7 h-7 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-violet-400 font-mono text-xs font-bold">
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
                  ? 'bg-violet-600 text-white font-semibold shadow-sm shadow-violet-950/60'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/60 border border-transparent'
              }`}
              title={isSidebarCollapsed ? item.label : undefined}
            >
              {/* Active neon accent pill */}
              {isActive && (
                <div className="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-violet-300 shadow-[0_0_8px_rgba(196,181,253,0.8)]" />
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
                      ? 'bg-violet-700/80 text-white'
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

    </aside>
  );
};
