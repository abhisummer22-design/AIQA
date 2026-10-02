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

import React, { useCallback, useEffect, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { useProject } from '../../context/ProjectContext';
import { routeDefs } from '../../routes/routeConfig';

/** Below this the sidebar cannot be a permanent column: 256px of it leaves 119px
 *  of content at 375px, and even the collapsed 64px permanently consumes 17% of
 *  the viewport. So under it the sidebar becomes an overlay drawer instead. */
const MOBILE_BREAKPOINT = 768;

/**
 * `window.matchMedia` guarded, and defaulting to "no match".
 *
 * jsdom does not implement it at all, and an unguarded call takes down every
 * test that renders this component -- 595 of them, the first time this was
 * written. Returning `false` means an environment that cannot answer the question
 * gets the desktop layout, which is the one that works everywhere.
 */
export function matchesMedia(query: string): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false;
  return window.matchMedia(query).matches;
}

function useIsMobileViewport() {
  const query = `(max-width: ${MOBILE_BREAKPOINT - 1}px)`;
  const [isMobile, setIsMobile] = useState(() => matchesMedia(query));

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
    const list = window.matchMedia(query);
    const onChange = () => setIsMobile(list.matches);
    onChange();
    list.addEventListener('change', onChange);
    return () => list.removeEventListener('change', onChange);
    // `query` is a constant derived from the module-level breakpoint.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return isMobile;
}

export const Sidebar: React.FC = () => {
  const { isSidebarCollapsed, toggleSidebar } = useAIQA();
  const { selectedProject } = useProject();
  const navigate = useNavigate();
  const location = useLocation();
  const isMobile = useIsMobileViewport();
  const [drawerOpen, setDrawerOpen] = useState(false);

  // On mobile the hamburger in the header opens the drawer; on desktop the same
  // button collapses the column. `isSidebarCollapsed` is still the desktop
  // control's state, and the header decides which by reading the same breakpoint.
  const openDrawer = useCallback(() => {
    if (isMobile) setDrawerOpen(true);
    else toggleSidebar();
  }, [isMobile, toggleSidebar]);

  // Leaving the mobile breakpoint must not strand the drawer open over content
  // that is no longer covered by its backdrop.
  useEffect(() => {
    if (!isMobile) setDrawerOpen(false);
  }, [isMobile]);

  // Escape closes the drawer, and selecting a destination closes it too: a
  // keyboard user who navigated somewhere should be looking at it.
  useEffect(() => {
    if (!drawerOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setDrawerOpen(false);
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [drawerOpen]);

  useEffect(() => {
    if (drawerOpen) setDrawerOpen(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.pathname]);

  // The header's hamburger drives the sidebar, so the open state has to live
  // outside this component.
  useEffect(() => {
    const onOpen = () => setDrawerOpen(true);
    window.addEventListener('aiqa:open-sidebar', onOpen);
    return () => window.removeEventListener('aiqa:open-sidebar', onOpen);
  }, []);

  if (isMobile) {
    return (
      <>
        {drawerOpen && (
          <div
            className="fixed inset-0 z-40 bg-slate-950/70 backdrop-blur-sm"
            data-testid="sidebar-backdrop"
            onClick={() => setDrawerOpen(false)}
          />
        )}
        <aside
          id="main-sidebar"
          data-testid="sidebar-drawer"
          data-open={drawerOpen ? 'true' : 'false'}
          className={`fixed top-0 bottom-0 left-0 z-50 w-64 flex flex-col border-r border-border-default/90 \
bg-surface-card transition-transform duration-200 select-none ${
            drawerOpen ? 'translate-x-0' : '-translate-x-full'
          }`}
        >
          <SidebarBody
            isSidebarCollapsed={false}
            selectedProject={selectedProject}
            isActive={(itemPath) => location.pathname === itemPath}
            onNavigate={(to) => {
              navigate(to);
              setDrawerOpen(false);
            }}
            onToggle={openDrawer}
          />
        </aside>
      </>
    );
  }

  return (
    <aside
      id="main-sidebar"
      data-testid="sidebar-column"
      className={`relative flex flex-col border-r border-border-default/90 bg-surface-card transition-all duration-200 z-20 select-none ${
        isSidebarCollapsed ? 'w-16' : 'w-64'
      }`}
    >
      <SidebarBody
        isSidebarCollapsed={isSidebarCollapsed}
        selectedProject={selectedProject}
        isActive={(itemPath) => location.pathname === itemPath}
        onNavigate={navigate}
        onToggle={openDrawer}
      />
    </aside>
  );
};

/** The sidebar's contents, shared by the desktop column and the mobile drawer so
 *  the two cannot drift apart. */
const SidebarBody: React.FC<{
  isSidebarCollapsed: boolean;
  selectedProject: ReturnType<typeof useProject>['selectedProject'];
  isActive: (path: string) => boolean;
  onNavigate: (path: string) => void;
  onToggle: () => void;
}> = ({ isSidebarCollapsed, selectedProject, isActive, onNavigate, onToggle }) => {
  return (
    <div className="contents">
      {/* Top Workspace Identity Section */}
      <div className="p-3 border-b border-border-default/80 flex items-center justify-between">
        {!isSidebarCollapsed ? (
          <div className="flex items-center gap-2.5 overflow-hidden">
            <div className="w-6 h-6 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-violet-400 font-mono text-[11px] font-bold">
              QE
            </div>
            <div className="truncate">
              <div className="text-xs font-semibold text-content-primary truncate">
                {selectedProject ? selectedProject.name : 'AIQA'}
              </div>
              <div className="text-[10px] font-mono text-content-muted truncate">
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
          onClick={onToggle}
          className="hidden md:flex p-1 rounded-md text-content-secondary hover:text-content-primary hover:bg-slate-800 transition-colors cursor-pointer"
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
      {/* `aria-label` names the landmark. Without it a screen reader announces a bare
          "navigation" alongside any other nav on the page, with no way to tell
          them apart. */}
      <nav
        aria-label="Primary"
        className="flex-1 py-2.5 px-2 space-y-0.5 overflow-y-auto"
      >
        {!isSidebarCollapsed && (
          <div className="px-2 py-1 text-[10px] font-mono uppercase tracking-widest text-content-muted font-bold">
            Navigation Plane
          </div>
        )}
        {routeDefs.map((item) => {
          const Icon = item.icon;
          const active = isActive(item.path);

          return (
            <button
              key={item.id}
              id={`nav-item-${item.id}`}
              onClick={() => onNavigate(item.path)}
              // These are page links, but they are rendered as `<button>` so that
              // `aria-current="page"` is the only thing telling assistive tech
              // which one you are on. Without it the active state was conveyed by
              // background colour alone, which a screen reader never announces --
              // WCAG 1.3.1 Info and Relationships (A).
              aria-current={active ? 'page' : undefined}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-xs font-medium transition-all group relative cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-400 ${
                active
                  ? 'bg-violet-600 text-white font-semibold shadow-sm shadow-violet-950/60'
                  : 'text-content-secondary hover:text-content-primary hover:bg-slate-800/60 border border-transparent'
              }`}
              title={isSidebarCollapsed ? item.label : undefined}
            >
              {/* Active neon accent pill */}
              {active && (
                <div className="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-violet-300 shadow-[0_0_8px_rgba(196,181,253,0.8)]" />
              )}

              <Icon
                className={`w-4 h-4 shrink-0 transition-colors ${
                  active
                    ? 'text-white'
                    : 'text-content-secondary group-hover:text-content-primary'
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
                    active
                      ? 'bg-violet-700/80 text-white'
                      : 'bg-slate-800 text-content-secondary border border-slate-700'
                  }`}
                >
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </nav>
    </div>
  );
};
