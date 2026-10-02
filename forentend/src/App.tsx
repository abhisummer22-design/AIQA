/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React from 'react';
import { BrowserRouter } from 'react-router-dom';
import { AIQAProvider } from './context/AIQAContext';
import { AuthProvider } from './context/AuthContext';
import { ProjectProvider } from './context/ProjectContext';
import { RealtimeProvider } from './context/RealtimeContext';
import { ThemeProvider } from './context/ThemeContext';
import { Header } from './components/layout/Header';
import { Sidebar } from './components/layout/Sidebar';
import { CommandPalette } from './components/layout/CommandPalette';
import { AppRoutes } from './routes/AppRoutes';

// FE-4: the 12 primary pages are no longer switched on in-memory state
// here — `routes/routeConfig.tsx` is now the single place that lists
// them, and `routes/AppRoutes.tsx` renders the real `<Routes>` table.
const MainContentArea: React.FC = () => (
  <main id="main-content-area" className="flex-1 overflow-y-auto bg-surface-page p-3 sm:p-5 lg:p-6 min-h-0 text-content-primary">
    <div className="max-w-7xl mx-auto w-full">
      <AppRoutes />
    </div>
  </main>
);

/**
 * The real app shell — providers, layout chrome, routed content — with no
 * router of its own. Split out from `App` purely so tests can wrap it in a
 * `MemoryRouter` (deterministic `initialEntries`, no real browser history)
 * instead of the real `BrowserRouter`; production always renders it
 * through `App` below, unchanged.
 */
export const AppShell: React.FC = () => (
  // Phase 6 gap-closing — theme is global, presentation-only state with no
  // dependency on (and no dependent among) auth/project/realtime, so it
  // wraps everything outermost rather than threading into that dependency
  // chain below.
  <ThemeProvider>
    <AuthProvider>
      {/* FE-5: project scope depends on real auth state (`useAuth()` inside
          `ProjectProvider`), so it must sit inside `AuthProvider` and outside
          everything that wants to read it — `AIQAProvider` included. FE-11
          removed `AIQAProvider`'s own fabricated project/backend-connection
          state entirely; `ProjectContext` is now the only "current project"
          concept in the app. */}
      <ProjectProvider>
        {/* FE-9: realtime connection lifecycle is driven by `useAuth()` +
            `useProject()`, so it must sit inside both — same placement
            reasoning as `ProjectProvider` above. Outside `AIQAProvider`
            since nothing here reads or writes its remaining state. */}
        <RealtimeProvider>
          <AIQAProvider>
            <div className="flex flex-col h-screen w-screen overflow-hidden bg-surface-page text-content-primary font-sans">
              {/* WCAG 2.4.1 Bypass Blocks (A). A keyboard user opening this app
                  lands on the header hamburger, and Tab has to walk the entire
                  sidebar -- 12 destinations -- before reaching the page they
                  navigated to, on every single page load. `sr-only` keeps it
                  invisible until focused, and `focus:not-sr-only` brings it
                  back on focus: hiding it entirely would be the same defect in
                  the other direction, leaving nobody able to see it.

                  Placed immediately before `<Header />` because that is the first
                  focusable element in DOM order, so this has to come before it.
                  Target `#main-content-area` already exists below. */}
              <a
                href="#main-content-area"
                className="sr-only focus:not-sr-only focus:absolute focus:z-[100] focus:top-2 focus:left-2 focus:px-3 focus:py-2 focus:rounded-md focus:bg-violet-600 focus:text-white focus:text-sm focus:font-medium"
              >
                Skip to main content
              </a>
              <Header />
              <div className="flex flex-1 overflow-hidden relative">
                <Sidebar />
                <MainContentArea />
              </div>
              <CommandPalette />
            </div>
          </AIQAProvider>
        </RealtimeProvider>
      </ProjectProvider>
    </AuthProvider>
  </ThemeProvider>
);

export default function App() {
  return (
    <BrowserRouter>
      <AppShell />
    </BrowserRouter>
  );
}
