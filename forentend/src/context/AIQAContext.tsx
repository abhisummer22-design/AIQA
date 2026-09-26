/**
 * AIQA Context State Management
 * Persistent shell context, environment switching, execution telemetry, and pipeline states.
 */

import React, { createContext, useCallback, useContext, useState, useEffect } from 'react';
import { useProject } from './ProjectContext';
import {
  Environment,
  GenerationSession,
} from '../types';

/** `activeGenerationSession`/`setActiveGenerationSession` read/write this
 *  key when no real project is selected — a real project id can never
 *  collide with it (`ProjectContext`'s ids are the backend's own UUIDs). */
const NO_PROJECT_KEY = '__no_project_selected__';

interface AIQAContextType {
  // FE-4: `activePage`/`setActivePage` were removed from here — the real
  // URL (via react-router) is now the single source of truth for "which
  // page is active"; see `routes/AppRoutes.tsx` and `types.ts`'s `NavPage`
  // (still used to key routes, just no longer mirrored into this context).
  isSidebarCollapsed: boolean;
  setIsSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
  currentEnvironment: Environment;
  setCurrentEnvironment: (env: Environment) => void;

  // Generator — one real session per real project (a `TestCaseGeneratorPage`
  // visit for project A must never show project B's uploaded documents or
  // drafted cases, and switching back to A must show exactly what was left
  // there, not a second, different project's leftovers or a wipe). These
  // two always read/write whichever project is *currently selected*
  // (`useProject()`, read inside this provider); `getGenerationSession`/
  // `setGenerationSessionFor` below reach an *explicit* project id instead,
  // for the one caller (`TestCaseGeneratorPage`'s own generation-completion
  // handler) that must keep targeting the project a call was actually made
  // for, even after the user has since switched to a different one.
  activeGenerationSession: GenerationSession | null;
  setActiveGenerationSession: React.Dispatch<React.SetStateAction<GenerationSession | null>>;
  getGenerationSession: (projectId: string) => GenerationSession | null;
  setGenerationSessionFor: (projectId: string, action: React.SetStateAction<GenerationSession | null>) => void;

  // UI Modals
  isCommandPaletteOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;
}

// Cleanup: the fabricated `SystemSettings` model (general/execution/
// environments/policies/notifications — a local-only object with no
// backend behind it, `localStorage`-persisted only) and this context's
// `settings`/`updateSettings` slice that owned it are removed — its one
// consumer, `SettingsPage.tsx`, no longer has the mockup UI that read/
// wrote it (removed at the explicit instruction to keep only what's
// real; see that file's own doc comment). `SystemSettings` itself is
// removed from `types.ts` too, and had no other consumer.

const AIQAContext = createContext<AIQAContextType | undefined>(undefined);

export const AIQAProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState<boolean>(false);
  const [currentEnvironment, setCurrentEnvironment] = useState<Environment>('demo');
  const [isCommandPaletteOpen, setCommandPaletteOpen] = useState<boolean>(false);

  // FE-16: the fabricated `runs`/`triggerNewRun` local execution-run model
  // (fake run IDs, fake pass/running/queued counts, fake test names) was
  // removed here — its only producer was the Test Generator's former fake
  // export path, and nothing ever rendered `runs` (confirmed by a
  // repo-wide search before removal — see the FE-16 report §11).

  // FE-24: `notificationCount`/`clearNotifications` (a hardcoded
  // `useState<number>(3)` with no real backend notification/alert entity
  // behind it — FE-23's audit finding) were removed from this context
  // entirely, not just stopped-being-used. Their only consumer,
  // `Header.tsx`'s notification bell, now derives real recent-activity
  // state directly from `useRealtime()`/`useProject()` — a real backend
  // signal already flowing through every page, not a second UI-shell
  // concept this generic context has no business owning.

  // Generator Session — Phase A cleanup: the fabricated pre-seeded session
  // (3 hardcoded demo cases, `tc-gen-1..3`, that made a fresh page load
  // look like a real AI run had happened) is removed entirely — a fresh
  // load starts genuinely empty. The state itself stays real:
  // `CustomTestingPage` writes real AI analysis results into it and
  // `TestCaseGeneratorPage` reads it back.
  //
  // Keyed by real project id (`NO_PROJECT_KEY` when none is selected) —
  // was a single, unscoped slot, which is exactly what let one project's
  // session silently show while a different one was actually selected:
  // `TestCaseGeneratorPage` never unmounts on a project switch (only on
  // leaving the page entirely), so nothing ever reset or rescoped it. This
  // way, switching *which* project's session is read/written is just
  // switching which key gets looked up — no reset needed, and no leak
  // into a different project's key either.
  const [sessionsByProject, setSessionsByProject] = useState<Record<string, GenerationSession | null>>({});
  const { selectedProjectId } = useProject();
  const activeProjectKey = selectedProjectId ?? NO_PROJECT_KEY;

  const setGenerationSessionFor = useCallback(
    (projectId: string, action: React.SetStateAction<GenerationSession | null>) => {
      setSessionsByProject((prev) => {
        const current = prev[projectId] ?? null;
        const next = typeof action === 'function'
          ? (action as (p: GenerationSession | null) => GenerationSession | null)(current)
          : action;
        return { ...prev, [projectId]: next };
      });
    },
    [],
  );
  const getGenerationSession = useCallback(
    (projectId: string) => sessionsByProject[projectId] ?? null,
    [sessionsByProject],
  );
  const activeGenerationSession = sessionsByProject[activeProjectKey] ?? null;
  const setActiveGenerationSession = useCallback(
    (action: React.SetStateAction<GenerationSession | null>) => setGenerationSessionFor(activeProjectKey, action),
    [activeProjectKey, setGenerationSessionFor],
  );

  // Actions
  const toggleSidebar = () => setIsSidebarCollapsed((prev) => !prev);

  // Global Keyboard Shortcuts (Cmd+K / Ctrl+K for command palette)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setCommandPaletteOpen((prev) => !prev);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
        e.preventDefault();
        toggleSidebar();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <AIQAContext.Provider
      value={{
        isSidebarCollapsed,
        setIsSidebarCollapsed,
        toggleSidebar,
        currentEnvironment,
        setCurrentEnvironment,
        activeGenerationSession,
        setActiveGenerationSession,
        getGenerationSession,
        setGenerationSessionFor,
        isCommandPaletteOpen,
        setCommandPaletteOpen,
      }}
    >
      {children}
    </AIQAContext.Provider>
  );
};

export const useAIQA = (): AIQAContextType => {
  const context = useContext(AIQAContext);
  if (!context) {
    throw new Error('useAIQA must be used within an AIQAProvider');
  }
  return context;
};
