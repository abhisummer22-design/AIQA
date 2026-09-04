/**
 * AIQA Context State Management
 * Persistent shell context, environment switching, execution telemetry, and pipeline states.
 */

import React, { createContext, useContext, useState, useEffect } from 'react';
import {
  Environment,
  GenerationSession,
} from '../types';

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

  // Generator
  activeGenerationSession: GenerationSession | null;
  setActiveGenerationSession: React.Dispatch<React.SetStateAction<GenerationSession | null>>;

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

  // Generator Session
  // Phase A cleanup: the fabricated pre-seeded session (3 hardcoded demo
  // cases, `tc-gen-1..3`, that made a fresh page load look like a real AI
  // run had happened) is removed entirely — a fresh load starts genuinely
  // empty. The state itself stays: CustomTestingPage writes real AI
  // analysis results into it and TestCaseGeneratorPage reads it back.
  const [activeGenerationSession, setActiveGenerationSession] =
    useState<GenerationSession | null>(null);

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
