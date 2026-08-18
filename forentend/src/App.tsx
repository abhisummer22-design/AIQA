/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React from 'react';
import { BrowserRouter } from 'react-router-dom';
import { AIQAProvider } from './context/AIQAContext';
import { AuthProvider } from './context/AuthContext';
import { ProjectProvider } from './context/ProjectContext';
import { Header } from './components/layout/Header';
import { Sidebar } from './components/layout/Sidebar';
import { CommandPalette } from './components/layout/CommandPalette';
import { RunTriggerModal } from './components/layout/RunTriggerModal';
import { AppRoutes } from './routes/AppRoutes';

// FE-4: the 12 primary pages are no longer switched on in-memory state
// here — `routes/routeConfig.tsx` is now the single place that lists
// them, and `routes/AppRoutes.tsx` renders the real `<Routes>` table.
const MainContentArea: React.FC = () => (
  <main id="main-content-area" className="flex-1 overflow-y-auto bg-[#020617] p-3 sm:p-5 lg:p-6 min-h-0 text-slate-100">
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
  <AuthProvider>
    {/* FE-5: project scope depends on real auth state (`useAuth()` inside
        `ProjectProvider`), so it must sit inside `AuthProvider` and outside
        everything that wants to read it — `AIQAProvider` included, even
        though `AIQAProvider`'s own project data remains its separate,
        pre-existing fabricated set (see `ProjectContext.tsx`'s own
        top-level comment). */}
    <ProjectProvider>
      <AIQAProvider>
        <div className="flex flex-col h-screen w-screen overflow-hidden bg-[#020617] text-slate-100 font-sans">
          <Header />
          <div className="flex flex-1 overflow-hidden relative">
            <Sidebar />
            <MainContentArea />
          </div>
          <CommandPalette />
          <RunTriggerModal />
        </div>
      </AIQAProvider>
    </ProjectProvider>
  </AuthProvider>
);

export default function App() {
  return (
    <BrowserRouter>
      <AppShell />
    </BrowserRouter>
  );
}
