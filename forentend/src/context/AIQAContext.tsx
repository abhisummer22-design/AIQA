/**
 * AIQA Context State Management
 * Persistent shell context, environment switching, execution telemetry, and pipeline states.
 */

import React, { createContext, useContext, useState, useEffect } from 'react';
import {
  Environment,
  GenerationSession,
  SystemSettings,
  ActivityEvent,
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

  // Activity Stream (write-only — see this field's own useState comment)
  addActivity: (event: Omit<ActivityEvent, 'id' | 'timestamp'>) => void;

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
    { id: 'production', name: 'Production Cloud (US-East)', baseUrl: 'https://app.enterprise.com', healthEndpoint: '/healthz', isActive: true },
    { id: 'staging', name: 'Staging Cluster (US-West)', baseUrl: 'https://staging.enterprise.com', healthEndpoint: '/healthz', isActive: true },
    { id: 'uat', name: 'UAT Acceptance Tier', baseUrl: 'https://uat.enterprise.com', healthEndpoint: '/healthz', isActive: true },
    { id: 'ephemeral-pr', name: 'PR-1482 Ephemeral Sandbox', baseUrl: 'https://pr-1482.preview.enterprise.internal', healthEndpoint: '/health', isActive: true },
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

const AIQAContext = createContext<AIQAContextType | undefined>(undefined);

export const AIQAProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState<boolean>(false);
  const [currentEnvironment, setCurrentEnvironment] = useState<Environment>('staging');
  const [isCommandPaletteOpen, setCommandPaletteOpen] = useState<boolean>(false);

  // Settings
  const [settings, setSettings] = useState<SystemSettings>(defaultSettings);

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
  const [activeGenerationSession, setActiveGenerationSession] = useState<GenerationSession | null>({
    id: 'gen-4401',
    requirementText: 'User initiates OAuth 2.0 PKCE login with multi-factor authentication, then attempts profile update without SMS verification challenge step.',
    specType: 'NATURAL_LANGUAGE',
    targetAppUrl: 'https://staging.enterprise.com/auth/login',
    currentStep: 'GENERATED_CASES',
    depth: 'STANDARD',
    createdAt: new Date(Date.now() - 1000 * 60 * 20).toISOString(),
    generatedCases: [
      {
        id: 'tc-gen-1',
        title: 'OAuth PKCE Challenge Validation on Malformed Code Verifier',
        description: 'Verify authorization server rejects auth code exchange when code_verifier SHA-256 does not match code_challenge.',
        category: 'Security',
        priority: 'CRITICAL',
        severity: 'CRITICAL',
        riskFactor: 'HIGH',
        status: 'PENDING_REVIEW',
        suggestedTags: ['security', 'oauth2', 'pkce', 'p0'],
        preconditions: ['User registered with valid credentials', 'Auth client registered with PKCE strict mode enabled'],
        steps: [
          { stepNumber: 1, action: 'Initiate GET /oauth/authorize with S256 code_challenge', expectedResult: 'HTTP 302 with authorization code' },
          { stepNumber: 2, action: 'POST /oauth/token with mismatched code_verifier', expectedResult: 'HTTP 400 invalid_grant error code' },
        ],
        assertions: [
          'Response status is 400 Bad Request',
          'JSON error field equals "invalid_grant"',
          'Audit log records failed PKCE validation event',
        ],
      },
      {
        id: 'tc-gen-2',
        title: 'MFA Bypass Attempt on Sensitive Profile Attribute Mutation',
        description: 'Ensure profile phone number change endpoint enforces MFA step-up authentication even if bearer session token is active.',
        category: 'Security',
        priority: 'HIGH',
        severity: 'MAJOR',
        riskFactor: 'HIGH',
        status: 'APPROVED',
        suggestedTags: ['mfa', 'security', 'step-up'],
        preconditions: ['User has active session token without step-up MFA challenge token'],
        steps: [
          { stepNumber: 1, action: 'Send PATCH /api/v1/user/profile with new phone number', expectedResult: 'HTTP 403 with step_up_required header' },
          { stepNumber: 2, action: 'Submit valid SMS OTP challenge token', expectedResult: 'HTTP 200 OK profile updated' },
        ],
        assertions: [
          'HTTP 403 received without MFA step-up header',
          'Database profile record remains unmutated until OTP challenge verified',
        ],
      },
      {
        id: 'tc-gen-3',
        title: 'Expired Authorization Code Replay Resistance',
        description: 'Verify one-time authorization code cannot be re-used in token exchange endpoint within timeout window.',
        category: 'Functional',
        priority: 'HIGH',
        severity: 'MAJOR',
        riskFactor: 'MEDIUM',
        status: 'PENDING_REVIEW',
        suggestedTags: ['oauth2', 'replay-attack', 'rfc6749'],
        preconditions: ['Valid auth code previously exchanged for access token'],
        steps: [
          { stepNumber: 1, action: 'Replay POST /oauth/token using already-redeemed code', expectedResult: 'HTTP 400 invalid_grant and revoke all issued tokens for code' },
        ],
        assertions: ['HTTP 400 returned', 'Both new and previously issued tokens immediately revoked'],
      },
    ],
  });

  // Activity Timeline Events
  // FE-11: `activities` has no real or fabricated consumer left anywhere
  // in the app (its render side was already dead before this sprint; the
  // fake demo seed that used to populate it is removed here too, not just
  // its display). `addActivity` (Test Generator's own protected write
  // path — see `TestCaseGeneratorPage.tsx`) still needs somewhere to
  // accumulate events it creates, so the state itself is kept, just empty
  // and no longer exposed through the context's public type/value.
  const [activities, setActivities] = useState<ActivityEvent[]>([]);

  // Actions
  const toggleSidebar = () => setIsSidebarCollapsed((prev) => !prev);

  const addActivity = (event: Omit<ActivityEvent, 'id' | 'timestamp'>) => {
    const newEvent: ActivityEvent = {
      ...event,
      id: `act-${Date.now().toString().slice(-6)}`,
      timestamp: new Date().toISOString(),
    };
    setActivities((prev) => [newEvent, ...prev]);
  };

  const updateSettings = (newSettings: Partial<SystemSettings>) => {
    setSettings((prev) => ({
      ...prev,
      ...newSettings,
      general: { ...prev.general, ...newSettings.general },
      authentication: { ...prev.authentication, ...newSettings.authentication },
      execution: { ...prev.execution, ...newSettings.execution },
      policies: { ...prev.policies, ...newSettings.policies },
      aiModels: { ...prev.aiModels, ...newSettings.aiModels },
      notifications: { ...prev.notifications, ...newSettings.notifications },
    }));
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
        addActivity,
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
