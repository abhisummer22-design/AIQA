/**
 * AIQA Context State Management
 * Persistent shell context, environment switching, execution telemetry, and pipeline states.
 */

import React, { createContext, useContext, useState, useEffect } from 'react';
import {
  Environment,
  GenerationSession,
  SystemSettings,
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

  // Settings
  settings: SystemSettings;
  updateSettings: (newSettings: Partial<SystemSettings>) => void;

  // UI Modals
  isCommandPaletteOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;
}

const defaultSettings: SystemSettings = {
  general: {
    organizationName: 'Acme Enterprise Engineering',
    projectKey: 'AIQA-CORE',
    timezone: 'UTC-07:00 (Pacific Time)',
    dataRetentionDays: 90,
  },
  authentication: {
    ssoEnabled: true,
    provider: 'OKTA',
    apiTokenStatus: 'ACTIVE',
  },
  execution: {
    defaultExecutor: 'k8s-runner-pool-us',
    maxConcurrency: 16,
    globalTimeoutSeconds: 300,
    autoRetryCount: 2,
    isolateInfraFailures: true,
  },
  environments: [
    { id: 'demo', name: 'DEMO Sandbox', baseUrl: 'https://demo.enterprise.com', healthEndpoint: '/healthz', isActive: true },
    { id: 'uat', name: 'UAT Acceptance Tier', baseUrl: 'https://uat.enterprise.com', healthEndpoint: '/healthz', isActive: true },
    { id: 'live', name: 'LIVE Production (US-East)', baseUrl: 'https://app.enterprise.com', healthEndpoint: '/healthz', isActive: true },
  ],
  policies: {
    blockOnProductFailures: true,
    blockOnInfraFailures: false,
    requireManualApprovalForProd: true,
    minimumPassPercentage: 98,
  },
  aiModels: {
    defaultModel: 'gemini-2.0-flash',
    temperature: 0.2,
    reasoningEffort: 'HIGH',
    ragGroundingEnabled: true,
  },
  notifications: {
    slackWebhook: 'https://hooks.slack.com/services/T00/B00/XXXXX',
    emailAlertsEnabled: true,
    pagerDutyKeyConfigured: true,
    notifyOnInfraErrorOnly: false,
  },
};

const VALID_ENVIRONMENTS: Environment[] = ['demo', 'uat', 'live'];

const AIQAContext = createContext<AIQAContextType | undefined>(undefined);

const SETTINGS_STORAGE_KEY = 'aiqa.settings';

function loadStoredSettings(): SystemSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return defaultSettings;
    const parsed = JSON.parse(raw);
    const storedEnvs = Array.isArray(parsed.environments)
      ? parsed.environments.filter((e: any) => VALID_ENVIRONMENTS.includes(e?.id))
      : [];
    return {
      ...defaultSettings,
      ...parsed,
      environments: storedEnvs.length > 0 ? storedEnvs : defaultSettings.environments,
      general: { ...defaultSettings.general, ...parsed.general },
      authentication: { ...defaultSettings.authentication, ...parsed.authentication },
      execution: { ...defaultSettings.execution, ...parsed.execution },
      policies: { ...defaultSettings.policies, ...parsed.policies },
      aiModels: { ...defaultSettings.aiModels, ...parsed.aiModels },
      notifications: { ...defaultSettings.notifications, ...parsed.notifications },
    };
  } catch {
    return defaultSettings;
  }
}

export const AIQAProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState<boolean>(false);
  const [currentEnvironment, setCurrentEnvironment] = useState<Environment>('demo');
  const [isCommandPaletteOpen, setCommandPaletteOpen] = useState<boolean>(false);

  // Settings — persisted to localStorage so a refresh doesn't silently
  // revert the workspace back to defaultSettings (this context has no
  // backend-backed store for these fields, unlike AI Models & Reasoning).
  const [settings, setSettings] = useState<SystemSettings>(loadStoredSettings);

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

  const updateSettings = (newSettings: Partial<SystemSettings>) => {
    setSettings((prev) => {
      const next = {
        ...prev,
        ...newSettings,
        general: { ...prev.general, ...newSettings.general },
        authentication: { ...prev.authentication, ...newSettings.authentication },
        execution: { ...prev.execution, ...newSettings.execution },
        policies: { ...prev.policies, ...newSettings.policies },
        aiModels: { ...prev.aiModels, ...newSettings.aiModels },
        notifications: { ...prev.notifications, ...newSettings.notifications },
      };
      localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  };

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
        settings,
        updateSettings,
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
