/**
 * AIQA Context State Management
 * Persistent shell context, environment switching, execution telemetry, and pipeline states.
 */

import React, { createContext, useContext, useState, useEffect } from 'react';
import {
  NavPage,
  AIQAStatus,
  Environment,
  BackendConnectionState,
  ExecutionRun,
  TestExecutionItem,
  GenerationSession,
  GeneratedTestCase,
  RBACScenario,
  RBACMatrixRule,
  APIEndpointTest,
  CustomTestSuite,
  SystemSettings,
  ActivityEvent,
  QualityIntelligence,
  AIQAProject,
  ProjectStatus,
} from '../types';

interface AIQAContextType {
  activePage: NavPage;
  setActivePage: (page: NavPage) => void;
  isSidebarCollapsed: boolean;
  setIsSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
  currentEnvironment: Environment;
  setCurrentEnvironment: (env: Environment) => void;
  backendState: BackendConnectionState;
  setBackendState: React.Dispatch<React.SetStateAction<BackendConnectionState>>;

  // Project Management
  projects: AIQAProject[];
  selectedProjectId: string;
  setSelectedProjectId: (id: string) => void;
  selectedProject: AIQAProject;
  
  // Execution Runs
  runs: ExecutionRun[];
  activeRunId: string | null;
  setActiveRunId: (id: string | null) => void;
  currentRun: ExecutionRun | null;
  triggerNewRun: (config: {
    title: string;
    environment: Environment;
    executor: ExecutionRun['executor'];
    concurrency: number;
  }) => ExecutionRun;
  cancelRun: (runId: string) => void;

  // Generator
  activeGenerationSession: GenerationSession | null;
  setActiveGenerationSession: React.Dispatch<React.SetStateAction<GenerationSession | null>>;
  startGenerationWorkflow: (params: {
    requirementText: string;
    specType: GenerationSession['specType'];
    targetAppUrl?: string;
    model: GenerationSession['model'];
    depth: GenerationSession['depth'];
  }) => void;
  updateGenerationStep: (step: GenerationSession['currentStep']) => void;
  approveTestCase: (testId: string) => void;
  rejectTestCase: (testId: string) => void;

  // RBAC
  rbacScenarios: RBACScenario[];
  activeRBACScenarioId: string | null;
  setActiveRBACScenarioId: (id: string | null) => void;
  addRBACRule: (scenarioId: string, rule: Omit<RBACMatrixRule, 'id'>) => void;
  evaluateRBACMatrix: (scenarioId: string) => void;

  // API Testing
  apiTests: APIEndpointTest[];
  activeApiTestId: string | null;
  setActiveApiTestId: (id: string | null) => void;
  addApiTest: (test: APIEndpointTest) => void;
  executeApiTest: (testId: string) => Promise<void>;

  // Custom Testing
  customSuites: CustomTestSuite[];
  addCustomSuite: (suite: CustomTestSuite) => void;
  runCustomSuite: (suiteId: string) => void;

  // Activity Stream & Intelligence
  activities: ActivityEvent[];
  addActivity: (event: Omit<ActivityEvent, 'id' | 'timestamp'>) => void;
  qualityIntelligence: QualityIntelligence | null;

  // Settings
  settings: SystemSettings;
  updateSettings: (newSettings: Partial<SystemSettings>) => void;

  // UI Modals
  isCommandPaletteOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;
  isNewRunModalOpen: boolean;
  setNewRunModalOpen: (open: boolean) => void;
  notificationCount: number;
  clearNotifications: () => void;
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
  const [activePage, setActivePage] = useState<NavPage>('dashboard');
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState<boolean>(false);
  const [currentEnvironment, setCurrentEnvironment] = useState<Environment>('staging');
  const [isCommandPaletteOpen, setCommandPaletteOpen] = useState<boolean>(false);
  const [isNewRunModalOpen, setNewRunModalOpen] = useState<boolean>(false);
  const [notificationCount, setNotificationCount] = useState<number>(3);

  // Backend Connection State
  const [backendState, setBackendState] = useState<BackendConnectionState>({
    status: 'connected',
    endpoint: 'https://controlplane.aiqa.internal',
    daemonVersion: 'v2.4.2-enterprise',
    latencyMs: 18,
    lastHeartbeat: new Date().toISOString(),
  });

  // Settings
  const [settings, setSettings] = useState<SystemSettings>(defaultSettings);

  // Authoritative Multi-Project State
  const [projects, setProjects] = useState<AIQAProject[]>([
    {
      id: 'prj-aiqa',
      name: 'AIQA Platform',
      slug: 'aiqa-platform',
      status: 'Running',
      environment: 'Development',
      lastActivity: '2 minutes ago (Run #run-8902 in progress)',
      metrics: {
        totalTestCases: 1248,
        activeRunsCount: 1,
        passed: 1180,
        failed: 3,
        blocked: 1,
        needsReview: 2,
        skipped: 22,
        running: 3,
        queued: 37,
      },
      activeRun: {
        id: 'run-8902',
        title: 'Regression Suite — Checkout & Auth Service Release v4.18',
        status: 'RUNNING',
        startedAt: '12m ago',
        durationEstimate: '15m total',
        testsExecuted: 34,
        totalTests: 48,
        progressPct: 71,
      },
      timelineData: [
        { timestamp: '08:00', timeLabel: '08:00', testsExecuted: 140, passed: 138, failed: 0, blocked: 2 },
        { timestamp: '08:15', timeLabel: '08:15', testsExecuted: 220, passed: 216, failed: 1, blocked: 3 },
        { timestamp: '08:30', timeLabel: '08:30', testsExecuted: 380, passed: 372, failed: 2, blocked: 6 },
        { timestamp: '08:45', timeLabel: '08:45', testsExecuted: 590, passed: 580, failed: 2, blocked: 8 },
        { timestamp: '09:00', timeLabel: '09:00', testsExecuted: 840, passed: 825, failed: 3, blocked: 12 },
        { timestamp: '09:15', timeLabel: '09:15', testsExecuted: 1060, passed: 1045, failed: 3, blocked: 12 },
        { timestamp: '09:30', timeLabel: '09:30', testsExecuted: 1248, passed: 1180, failed: 3, blocked: 1 },
      ],
    },
    {
      id: 'prj-fintech',
      name: 'FinTech Ledger & Payments',
      slug: 'fintech-ledger',
      status: 'Warning',
      environment: 'Staging',
      lastActivity: '18 minutes ago (Security audit rule breach)',
      metrics: {
        totalTestCases: 840,
        activeRunsCount: 0,
        passed: 792,
        failed: 18,
        blocked: 12,
        needsReview: 18,
        skipped: 0,
        running: 0,
        queued: 0,
      },
      timelineData: [
        { timestamp: '06:00', timeLabel: '06:00', testsExecuted: 100, passed: 98, failed: 1, blocked: 1 },
        { timestamp: '07:00', timeLabel: '07:00', testsExecuted: 320, passed: 310, failed: 6, blocked: 4 },
        { timestamp: '08:00', timeLabel: '08:00', testsExecuted: 560, passed: 535, failed: 14, blocked: 11 },
        { timestamp: '09:00', timeLabel: '09:00', testsExecuted: 840, passed: 792, failed: 18, blocked: 12 },
      ],
    },
    {
      id: 'prj-iam',
      name: 'Cloud Gateway & Zero-Trust IAM',
      slug: 'gateway-iam',
      status: 'Healthy',
      environment: 'Production',
      lastActivity: '1 hour ago (OAuth PKCE validation passed)',
      metrics: {
        totalTestCases: 620,
        activeRunsCount: 0,
        passed: 614,
        failed: 0,
        blocked: 2,
        needsReview: 4,
        skipped: 0,
        running: 0,
        queued: 0,
      },
      timelineData: [
        { timestamp: '05:00', timeLabel: '05:00', testsExecuted: 150, passed: 150, failed: 0, blocked: 0 },
        { timestamp: '06:30', timeLabel: '06:30', testsExecuted: 340, passed: 338, failed: 0, blocked: 2 },
        { timestamp: '08:00', timeLabel: '08:00', testsExecuted: 620, passed: 614, failed: 0, blocked: 2 },
      ],
    },
    {
      id: 'prj-mobile',
      name: 'Mobile Client v4.8 Beta',
      slug: 'mobile-client',
      status: 'Healthy',
      environment: 'UAT',
      lastActivity: '3 hours ago (Biometric token tests)',
      metrics: {
        totalTestCases: 430,
        activeRunsCount: 0,
        passed: 422,
        failed: 2,
        blocked: 4,
        needsReview: 2,
        skipped: 0,
        running: 0,
        queued: 0,
      },
      timelineData: [
        { timestamp: '04:00', timeLabel: '04:00', testsExecuted: 120, passed: 118, failed: 1, blocked: 1 },
        { timestamp: '06:00', timeLabel: '06:00', testsExecuted: 280, passed: 276, failed: 2, blocked: 2 },
        { timestamp: '08:00', timeLabel: '08:00', testsExecuted: 430, passed: 422, failed: 2, blocked: 4 },
      ],
    },
    {
      id: 'prj-inventory',
      name: 'Inventory Microservice',
      slug: 'inventory-microservice',
      status: 'Offline',
      environment: 'Development',
      lastActivity: 'No execution history recorded',
      metrics: {
        totalTestCases: 0,
        activeRunsCount: 0,
        passed: 0,
        failed: 0,
        blocked: 0,
        needsReview: 0,
        skipped: 0,
        running: 0,
        queued: 0,
      },
      timelineData: [],
    },
  ]);

  const [selectedProjectId, setSelectedProjectId] = useState<string>('prj-aiqa');
  const selectedProject = projects.find((p) => p.id === selectedProjectId) || projects[0];

  // Initial Authoritative Runs State
  const [runs, setRuns] = useState<ExecutionRun[]>([
    {
      id: 'run-8902',
      title: 'Regression Suite — Checkout & Auth Service Release v4.18',
      environment: 'staging',
      status: 'RUNNING',
      startedAt: new Date(Date.now() - 1000 * 60 * 12).toISOString(),
      executor: 'k8s-runner-pool-us',
      concurrency: 12,
      policyApprovalState: 'APPROVED',
      totalTests: 48,
      counts: {
        passed: 34,
        failed: 3,
        blocked: 1,
        skipped: 2,
        needsReview: 2,
        infraErrors: 1,
        running: 3,
        queued: 2,
      },
      productFailuresCount: 2,
      infraFailuresCount: 1,
      tests: [
        {
          id: 'test-101',
          runId: 'run-8902',
          name: 'E2E Checkout: Credit Card 3D Secure Verification flow',
          suite: 'Payments & Settlement',
          type: 'E2E',
          status: 'PASSED',
          durationMs: 4200,
          executor: 'k8s-runner-01',
          retries: 0,
          evidenceFiles: [{ name: 'trace-101.zip', type: 'trace', size: '1.4 MB' }],
        },
        {
          id: 'test-102',
          runId: 'run-8902',
          name: 'API: Auth Token Refresh with revoked session cookie',
          suite: 'Identity & Access',
          type: 'API',
          status: 'FAILED',
          durationMs: 340,
          failureClassification: 'PRODUCT_FAILURE',
          failureMessage: 'Assertion Failed: Expected HTTP 401 Unauthorized, received HTTP 200 with expired payload',
          stackTrace: 'at AuthAssert.verifyRevocation (auth.spec.ts:88:14)\n    at Runner.executeStep (runner.ts:241:9)',
          executor: 'k8s-runner-02',
          retries: 1,
          evidenceFiles: [
            { name: 'response_payload.json', type: 'har', size: '4.2 KB' },
            { name: 'console_err.log', type: 'log', size: '18 KB' },
          ],
        },
        {
          id: 'test-103',
          runId: 'run-8902',
          name: 'RBAC: Tenant Admin privilege escalation boundary check',
          suite: 'RBAC Security Matrix',
          type: 'RBAC',
          status: 'FAILED',
          durationMs: 512,
          failureClassification: 'PRODUCT_FAILURE',
          failureMessage: 'Security Policy Breach: Tenant Admin was permitted to mutate global system flags',
          stackTrace: 'at RBACValidator.evaluateRule (rbac_engine.ts:112:5)',
          executor: 'k8s-runner-03',
          retries: 0,
        },
        {
          id: 'test-104',
          runId: 'run-8902',
          name: 'E2E Cart: ElasticSearch Inventory cluster sync',
          suite: 'Inventory Catalog',
          type: 'E2E',
          status: 'INFRA_ERROR',
          durationMs: 15200,
          failureClassification: 'INFRASTRUCTURE_FAILURE',
          failureMessage: 'Infrastructure Timeout: Target Redis connection timed out after 15000ms. DNS lookup failed for redis-cluster.staging.svc',
          stackTrace: 'ConnectionError: ETIMEDOUT redis-cluster:6379\n    at Socket.<anonymous> (net.js:842:19)',
          executor: 'k8s-runner-04',
          retries: 2,
          evidenceFiles: [{ name: 'infra_telemetry.log', type: 'log', size: '42 KB' }],
        },
        {
          id: 'test-105',
          runId: 'run-8902',
          name: 'API: GraphQL Federation Gateway Schema Introspection',
          suite: 'API Gateway Tier',
          type: 'API',
          status: 'NEEDS_REVIEW',
          durationMs: 890,
          failureClassification: 'UNCLASSIFIED',
          failureMessage: 'Anomaly Detected: Schema response hash changed without declared deprecation metadata.',
          executor: 'k8s-runner-01',
          retries: 0,
        },
        {
          id: 'test-106',
          runId: 'run-8902',
          name: 'E2E Webhook Dispatcher: Retry on Exponential Backoff',
          suite: 'Event Bus',
          type: 'INTEGRATION',
          status: 'BLOCKED',
          durationMs: 0,
          failureMessage: 'Precondition blocked by failed upstream test-102 (Auth Token Revocation)',
          executor: 'k8s-runner-02',
          retries: 0,
        },
        {
          id: 'test-107',
          runId: 'run-8902',
          name: 'Load: Peak Concurrent Session Handshake',
          suite: 'Performance & Scalability',
          type: 'CUSTOM',
          status: 'RUNNING',
          executor: 'k8s-runner-05',
          retries: 0,
        },
      ],
    },
    {
      id: 'run-8901',
      title: 'Nightly Core Matrix & Security Smoke Tier',
      environment: 'staging',
      status: 'PASSED',
      startedAt: new Date(Date.now() - 1000 * 60 * 180).toISOString(),
      completedAt: new Date(Date.now() - 1000 * 60 * 155).toISOString(),
      executor: 'cloud-playwright-grid',
      concurrency: 16,
      policyApprovalState: 'AUTO_APPROVED',
      totalTests: 120,
      counts: {
        passed: 118,
        failed: 0,
        blocked: 0,
        skipped: 2,
        needsReview: 0,
        infraErrors: 0,
        running: 0,
        queued: 0,
      },
      productFailuresCount: 0,
      infraFailuresCount: 0,
      tests: [],
    },
  ]);

  const [activeRunId, setActiveRunId] = useState<string | null>('run-8902');

  const currentRun = runs.find((r) => r.id === activeRunId) || runs[0] || null;

  // Generator Session
  const [activeGenerationSession, setActiveGenerationSession] = useState<GenerationSession | null>({
    id: 'gen-4401',
    requirementText: 'User initiates OAuth 2.0 PKCE login with multi-factor authentication, then attempts profile update without SMS verification challenge step.',
    specType: 'NATURAL_LANGUAGE',
    targetAppUrl: 'https://staging.enterprise.com/auth/login',
    currentStep: 'GENERATED_CASES',
    model: 'gemini-2.0-flash',
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

  // RBAC Scenarios
  const [rbacScenarios, setRbacScenarios] = useState<RBACScenario[]>([
    {
      id: 'rbac-sc-1',
      name: 'Enterprise Organization & Financial Asset Access Control',
      description: 'Audit role privilege boundaries across Billing, User Management, API Keys, and Audit Logs',
      targetEnvironment: 'staging',
      rolesUnderAudit: ['SuperAdmin', 'BillingManager', 'Developer', 'Auditor', 'ReadOnlyMember'],
      rules: [
        {
          id: 'rule-1',
          role: 'SuperAdmin',
          resource: 'Billing:Invoices',
          action: 'ADMIN',
          expectedAccess: 'ALLOW',
          actualAccess: 'ALLOW',
          status: 'PASSED',
          evidenceNote: 'Granted full administrative mutation rights on invoice ledger',
          evaluatedAt: new Date(Date.now() - 1000 * 60 * 30).toISOString(),
        },
        {
          id: 'rule-2',
          role: 'Developer',
          resource: 'Billing:PaymentMethods',
          action: 'DELETE',
          expectedAccess: 'DENY',
          actualAccess: 'DENY',
          status: 'PASSED',
          evidenceNote: 'Policy strictly rejected delete action (HTTP 403)',
          evaluatedAt: new Date(Date.now() - 1000 * 60 * 30).toISOString(),
        },
        {
          id: 'rule-3',
          role: 'Developer',
          resource: 'Security:APIKeys',
          action: 'WRITE',
          expectedAccess: 'ALLOW',
          actualAccess: 'ALLOW',
          status: 'PASSED',
          evidenceNote: 'Scoped developer token creation allowed in development workspace',
          evaluatedAt: new Date(Date.now() - 1000 * 60 * 30).toISOString(),
        },
        {
          id: 'rule-4',
          role: 'Auditor',
          resource: 'Security:AuditLogs',
          action: 'DELETE',
          expectedAccess: 'DENY',
          actualAccess: 'ALLOW', // Violation
          status: 'FAILED',
          evidenceNote: 'POLICY VIOLATION: Auditor role had unrevoked DELETE permission on S3 cold storage audit trail',
          evaluatedAt: new Date(Date.now() - 1000 * 60 * 30).toISOString(),
        },
        {
          id: 'rule-5',
          role: 'ReadOnlyMember',
          resource: 'Production:Secrets',
          action: 'READ',
          expectedAccess: 'DENY',
          actualAccess: 'DENY',
          status: 'PASSED',
          evidenceNote: 'Access blocked by Vault KMS policy',
          evaluatedAt: new Date(Date.now() - 1000 * 60 * 30).toISOString(),
        },
      ],
    },
  ]);
  const [activeRBACScenarioId, setActiveRBACScenarioId] = useState<string | null>('rbac-sc-1');

  // API Tests
  const [apiTests, setApiTests] = useState<APIEndpointTest[]>([
    {
      id: 'api-t-1',
      name: 'Get Tenant Authorization Context & Active Scopes',
      adapter: 'REST_HTTP',
      method: 'GET',
      endpointUrl: 'https://staging.enterprise.com/api/v2/auth/context',
      headers: {
        'Accept': 'application/json',
        'X-Client-Version': 'v4.18.0',
      },
      queryParams: {
        'include_entitlements': 'true',
      },
      authType: 'BEARER_TOKEN',
      bodyType: 'NONE',
      assertions: [
        { id: 'as-1', property: 'STATUS_CODE', operator: 'EQUALS', targetValue: '200', passed: true, actualValue: '200' },
        { id: 'as-2', property: 'RESPONSE_TIME', operator: 'LESS_THAN', targetValue: '250', passed: true, actualValue: '48ms' },
        { id: 'as-3', property: 'BODY_JSON_PATH', operator: 'EXISTS', targetValue: '$.tenant.id', passed: true, actualValue: 'tenant_99812' },
      ],
      lastExecution: {
        timestamp: new Date(Date.now() - 1000 * 60 * 15).toISOString(),
        status: 'PASSED',
        statusCode: 200,
        responseTimeMs: 48,
        responseHeaders: {
          'content-type': 'application/json; charset=utf-8',
          'x-ratelimit-remaining': '498',
          'x-request-id': 'req-988319-ca',
        },
        responseBody: JSON.stringify({
          tenant: { id: 'tenant_99812', name: 'Acme Corp', tier: 'ENTERPRISE' },
          scopes: ['read:billing', 'write:pipelines', 'admin:rbac'],
          sessionExpiresIn: 3600
        }, null, 2),
      },
    },
    {
      id: 'api-t-2',
      name: 'Dispatch Asynchronous Batch Evaluation Job',
      adapter: 'REST_HTTP',
      method: 'POST',
      endpointUrl: 'https://staging.enterprise.com/api/v2/jobs/evaluate',
      headers: {
        'Content-Type': 'application/json',
      },
      queryParams: {},
      authType: 'BEARER_TOKEN',
      bodyType: 'JSON',
      bodyPayload: JSON.stringify({
        jobType: 'QUALITY_REGRESSION',
        priority: 'HIGH',
        suites: ['auth_suite', 'checkout_suite'],
        concurrency: 8
      }, null, 2),
      assertions: [
        { id: 'as-4', property: 'STATUS_CODE', operator: 'EQUALS', targetValue: '202', passed: true, actualValue: '202' },
        { id: 'as-5', property: 'BODY_JSON_PATH', operator: 'EXISTS', targetValue: '$.jobId', passed: true, actualValue: 'job_48911' },
      ],
      lastExecution: {
        timestamp: new Date(Date.now() - 1000 * 60 * 45).toISOString(),
        status: 'PASSED',
        statusCode: 202,
        responseTimeMs: 92,
        responseHeaders: { 'content-type': 'application/json' },
        responseBody: JSON.stringify({ jobId: 'job_48911', status: 'QUEUED', etaSeconds: 120 }, null, 2),
      },
    },
  ]);
  const [activeApiTestId, setActiveApiTestId] = useState<string | null>('api-t-1');

  // Custom Suites
  const [customSuites, setCustomSuites] = useState<CustomTestSuite[]>([
    {
      id: 'cst-1',
      name: 'Multi-Tenant Visual Regression & Dark Mode Parity',
      category: 'VISUAL_REGRESSION',
      description: 'Diff pixel baseline across 12 critical SaaS dashboards against Figma design tokens at 1440x900 and 390x844 viewports.',
      configuration: { threshold: 0.02, antiAliasing: true, ignoreDynamicTimestamps: true },
      targetUrl: 'https://staging.enterprise.com/dashboard',
      concurrencyLevel: 4,
      timeoutSeconds: 180,
      status: 'PASSED',
      lastRunTime: new Date(Date.now() - 1000 * 60 * 120).toISOString(),
    },
    {
      id: 'cst-2',
      name: 'Resilience: Kubernetes Pod Chaos & Gateway 502 Recovery',
      category: 'CHAOS_RESILIENCE',
      description: 'Inject 30% packet loss and kill ingress pod replicas during active test runs to verify failover circuit breakers.',
      configuration: { faultType: 'POD_TERMINATION', blastRadiusPercentage: 30 },
      targetUrl: 'https://staging.enterprise.com/api',
      concurrencyLevel: 8,
      timeoutSeconds: 240,
      status: 'NEEDS_REVIEW',
      lastRunTime: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
    },
  ]);

  // Activity Timeline Events
  const [activities, setActivities] = useState<ActivityEvent[]>([
    {
      id: 'act-1',
      type: 'INFRA_ALERT',
      timestamp: new Date(Date.now() - 1000 * 60 * 2).toISOString(),
      title: 'Infrastructure Anomaly Detected',
      description: 'DNS lookup failure on redis-cluster.staging.svc in test-104. Classified as INFRASTRUCTURE_FAILURE.',
      runId: 'run-8902',
      testId: 'test-104',
      status: 'INFRA_ERROR',
      classification: 'INFRASTRUCTURE_FAILURE',
    },
    {
      id: 'act-2',
      type: 'FAILURE_DETECTED',
      timestamp: new Date(Date.now() - 1000 * 60 * 5).toISOString(),
      title: 'Product Failure: Auth Token Revocation',
      description: 'Expected HTTP 401 Unauthorized, received HTTP 200 with expired payload in test-102.',
      runId: 'run-8902',
      testId: 'test-102',
      status: 'FAILED',
      classification: 'PRODUCT_FAILURE',
    },
    {
      id: 'act-3',
      type: 'EVIDENCE_CAPTURED',
      timestamp: new Date(Date.now() - 1000 * 60 * 8).toISOString(),
      title: 'Network HAR & Playwright Trace Captured',
      description: 'Generated 1.4 MB diagnostic bundle for test-101 (Credit Card 3D Secure).',
      runId: 'run-8902',
      testId: 'test-101',
      status: 'PASSED',
    },
    {
      id: 'act-4',
      type: 'RUN_STARTED',
      timestamp: new Date(Date.now() - 1000 * 60 * 12).toISOString(),
      title: 'Regression Suite Run #8902 Initiated',
      description: 'Triggered by CI/CD pipeline on commit b98c21a. 12 concurrency slots allocated on k8s-runner-pool-us.',
      runId: 'run-8902',
      status: 'RUNNING',
      actor: 'GitHub Actions (workflow #482)',
    },
  ]);

  // Quality Intelligence
  const [qualityIntelligence] = useState<QualityIntelligence>({
    riskScore: 28,
    riskLevel: 'MODERATE',
    changeImpact: {
      impactedModules: ['Identity & Access', 'Payments & Settlement', 'RBAC Matrix Engine'],
      riskVector: 'High-risk delta in OAuth token expiration and permission validation tier.',
      recommendedRegressionDepth: 'Full E2E + Strict RBAC Security Matrix verification',
    },
    flakinessIndex: 2.1,
    testValueScore: 94,
    failureBreakdown: {
      productDefects: 2,
      infraErrors: 1,
      needsReviewCount: 2,
    },
    qualityGraphNodes: [
      { id: 'node-auth', name: 'Auth Gateway', health: 74, type: 'GATEWAY' },
      { id: 'node-payments', name: 'Payment Core', health: 98, type: 'SERVICE' },
      { id: 'node-rbac', name: 'RBAC Policy Engine', health: 80, type: 'SERVICE' },
      { id: 'node-db', name: 'Primary Spanner DB', health: 100, type: 'DATABASE' },
      { id: 'node-ui', name: 'Web Client Frontend', health: 96, type: 'UI' },
    ],
  });

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

  const triggerNewRun = (config: {
    title: string;
    environment: Environment;
    executor: ExecutionRun['executor'];
    concurrency: number;
  }): ExecutionRun => {
    const newRunId = `run-${Math.floor(1000 + Math.random() * 9000)}`;
    const newRun: ExecutionRun = {
      id: newRunId,
      title: config.title,
      environment: config.environment,
      status: 'RUNNING',
      startedAt: new Date().toISOString(),
      executor: config.executor,
      concurrency: config.concurrency,
      policyApprovalState: 'APPROVED',
      totalTests: 10,
      counts: {
        passed: 1,
        failed: 0,
        blocked: 0,
        skipped: 0,
        needsReview: 0,
        infraErrors: 0,
        running: 3,
        queued: 6,
      },
      productFailuresCount: 0,
      infraFailuresCount: 0,
      tests: [
        {
          id: `t-${Date.now()}-1`,
          runId: newRunId,
          name: 'Smoke: Healthcheck ping to ingress router',
          suite: 'System Health',
          type: 'INTEGRATION',
          status: 'PASSED',
          durationMs: 120,
          executor: config.executor,
          retries: 0,
        },
        {
          id: `t-${Date.now()}-2`,
          runId: newRunId,
          name: 'Security: TLS 1.3 Cipher Suite negotiation check',
          suite: 'Security Baseline',
          type: 'API',
          status: 'RUNNING',
          executor: config.executor,
          retries: 0,
        },
      ],
    };

    setRuns((prev) => [newRun, ...prev]);
    setActiveRunId(newRunId);
    addActivity({
      type: 'RUN_STARTED',
      title: `Run ${newRunId} Triggered`,
      description: `Execution started on ${config.environment} via ${config.executor}`,
      runId: newRunId,
      status: 'RUNNING',
      actor: 'Control Plane Operator',
    });
    return newRun;
  };

  const cancelRun = (runId: string) => {
    setRuns((prev) =>
      prev.map((r) => (r.id === runId ? { ...r, status: 'CANCELLED', completedAt: new Date().toISOString() } : r))
    );
    addActivity({
      type: 'RUN_COMPLETED',
      title: `Run ${runId} Cancelled`,
      description: 'Run terminated by user command.',
      runId,
      status: 'CANCELLED',
    });
  };

  const startGenerationWorkflow = (params: {
    requirementText: string;
    specType: GenerationSession['specType'];
    targetAppUrl?: string;
    model: GenerationSession['model'];
    depth: GenerationSession['depth'];
  }) => {
    const session: GenerationSession = {
      id: `gen-${Math.floor(1000 + Math.random() * 9000)}`,
      requirementText: params.requirementText,
      specType: params.specType,
      targetAppUrl: params.targetAppUrl,
      currentStep: 'APPLICATION_UNDERSTANDING',
      model: params.model,
      depth: params.depth,
      createdAt: new Date().toISOString(),
      generatedCases: [],
    };
    setActiveGenerationSession(session);
    addActivity({
      type: 'RUN_STARTED',
      title: 'AI Test Generation Session Started',
      description: `Analyzing requirements using ${params.model} (${params.depth} depth)`,
    });

    // Simulate AI pipeline progression stages
    setTimeout(() => {
      setActiveGenerationSession((prev) => (prev ? { ...prev, currentStep: 'AI_PLANNING' } : null));
    }, 1200);

    setTimeout(() => {
      const generated: GeneratedTestCase[] = [
        {
          id: `tc-${Date.now()}-1`,
          title: 'Input Boundary Validation & SQL Injection Payload Sanitization',
          description: 'Verify target input fields strictly sanitize SQL syntax patterns and escape single quotes.',
          category: 'Security',
          priority: 'CRITICAL',
          severity: 'CRITICAL',
          riskFactor: 'HIGH',
          status: 'PENDING_REVIEW',
          suggestedTags: ['sqli', 'sanitization', 'security'],
          preconditions: ['Target input field loaded in DOM'],
          steps: [
            { stepNumber: 1, action: "Input \"' OR '1'='1\" into search filter query", expectedResult: 'HTTP 400 or escaped literal query' },
            { stepNumber: 2, action: 'Submit search request', expectedResult: 'No database syntax error returned' },
          ],
          assertions: ['Status is 200/400 without DB stack trace', 'DOM outputs escaped query string'],
        },
        {
          id: `tc-${Date.now()}-2`,
          title: 'Concurrent Idempotency Key Duplicate Submission',
          description: 'Verify 2 identical requests with the same Idempotency-Key header only charge account once.',
          category: 'Functional',
          priority: 'HIGH',
          severity: 'MAJOR',
          riskFactor: 'MEDIUM',
          status: 'PENDING_REVIEW',
          suggestedTags: ['idempotency', 'api', 'p0'],
          preconditions: ['Valid idempotency key generated (UUID v4)'],
          steps: [
            { stepNumber: 1, action: 'Dispatch POST /charge with Idempotency-Key: abc-123', expectedResult: 'HTTP 200 Charge Created' },
            { stepNumber: 2, action: 'Simultaneously dispatch identical request', expectedResult: 'HTTP 200 with cached idempotent response' },
          ],
          assertions: ['Account charged exactly 1 time', 'Second response contains x-cache: idempotent-hit'],
        },
      ];

      setActiveGenerationSession((prev) =>
        prev
          ? {
              ...prev,
              currentStep: 'GENERATED_CASES',
              generatedCases: generated,
            }
          : null
      );
    }, 2800);
  };

  const updateGenerationStep = (step: GenerationSession['currentStep']) => {
    setActiveGenerationSession((prev) => (prev ? { ...prev, currentStep: step } : null));
  };

  const approveTestCase = (testId: string) => {
    setActiveGenerationSession((prev) => {
      if (!prev) return null;
      return {
        ...prev,
        generatedCases: prev.generatedCases.map((tc) =>
          tc.id === testId ? { ...tc, status: 'APPROVED' } : tc
        ),
      };
    });
  };

  const rejectTestCase = (testId: string) => {
    setActiveGenerationSession((prev) => {
      if (!prev) return null;
      return {
        ...prev,
        generatedCases: prev.generatedCases.map((tc) =>
          tc.id === testId ? { ...tc, status: 'REJECTED' } : tc
        ),
      };
    });
  };

  const addRBACRule = (scenarioId: string, rule: Omit<RBACMatrixRule, 'id'>) => {
    const newRule: RBACMatrixRule = {
      ...rule,
      id: `rule-${Date.now().toString().slice(-4)}`,
      evaluatedAt: new Date().toISOString(),
    };
    setRbacScenarios((prev) =>
      prev.map((sc) => (sc.id === scenarioId ? { ...sc, rules: [...sc.rules, newRule] } : sc))
    );
  };

  const evaluateRBACMatrix = (scenarioId: string) => {
    setRbacScenarios((prev) =>
      prev.map((sc) => {
        if (sc.id !== scenarioId) return sc;
        return {
          ...sc,
          rules: sc.rules.map((r) => ({
            ...r,
            actualAccess: r.expectedAccess,
            status: 'PASSED',
            evaluatedAt: new Date().toISOString(),
          })),
        };
      })
    );
    addActivity({
      type: 'TEST_COMPLETED',
      title: 'RBAC Policy Matrix Evaluated',
      description: 'Audit verified role access control rules against current authorization policies.',
      status: 'PASSED',
    });
  };

  const addApiTest = (test: APIEndpointTest) => {
    setApiTests((prev) => [test, ...prev]);
    setActiveApiTestId(test.id);
  };

  const executeApiTest = async (testId: string) => {
    const target = apiTests.find((t) => t.id === testId);
    if (!target) return;

    // Transition test to running state
    setApiTests((prev) =>
      prev.map((t) =>
        t.id === testId
          ? {
              ...t,
              lastExecution: {
                timestamp: new Date().toISOString(),
                status: 'RUNNING',
              },
            }
          : t
      )
    );

    // Realistic API execution delay
    await new Promise((res) => setTimeout(res, 600));

    setApiTests((prev) =>
      prev.map((t) =>
        t.id === testId
          ? {
              ...t,
              lastExecution: {
                timestamp: new Date().toISOString(),
                status: 'PASSED',
                statusCode: 200,
                responseTimeMs: Math.floor(35 + Math.random() * 40),
                responseHeaders: {
                  'content-type': 'application/json; charset=utf-8',
                  'x-ratelimit-remaining': '994',
                  'x-response-time': '42ms',
                },
                responseBody: JSON.stringify(
                  {
                    status: 'success',
                    data: {
                      endpoint: t.endpointUrl,
                      method: t.method,
                      authenticated: t.authType !== 'NONE',
                      timestamp: new Date().toISOString(),
                    },
                  },
                  null,
                  2
                ),
              },
            }
          : t
      )
    );

    addActivity({
      type: 'TEST_COMPLETED',
      title: `API Endpoint Executed: ${target.name}`,
      description: `${target.method} ${target.endpointUrl} (HTTP 200 OK)`,
      status: 'PASSED',
    });
  };

  const addCustomSuite = (suite: CustomTestSuite) => {
    setCustomSuites((prev) => [suite, ...prev]);
  };

  const runCustomSuite = (suiteId: string) => {
    setCustomSuites((prev) =>
      prev.map((s) =>
        s.id === suiteId
          ? { ...s, status: 'RUNNING', lastRunTime: new Date().toISOString() }
          : s
      )
    );
    setTimeout(() => {
      setCustomSuites((prev) =>
        prev.map((s) => (s.id === suiteId ? { ...s, status: 'PASSED' } : s))
      );
      addActivity({
        type: 'TEST_COMPLETED',
        title: 'Custom Testing Suite Completed',
        description: 'Visual regression & resilience suite finished with 0 critical diffs.',
        status: 'PASSED',
      });
    }, 1500);
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

  const clearNotifications = () => setNotificationCount(0);

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
        activePage,
        setActivePage,
        isSidebarCollapsed,
        setIsSidebarCollapsed,
        toggleSidebar,
        currentEnvironment,
        setCurrentEnvironment,
        backendState,
        setBackendState,
        projects,
        selectedProjectId,
        setSelectedProjectId,
        selectedProject,
        runs,
        activeRunId,
        setActiveRunId,
        currentRun,
        triggerNewRun,
        cancelRun,
        activeGenerationSession,
        setActiveGenerationSession,
        startGenerationWorkflow,
        updateGenerationStep,
        approveTestCase,
        rejectTestCase,
        rbacScenarios,
        activeRBACScenarioId,
        setActiveRBACScenarioId,
        addRBACRule,
        evaluateRBACMatrix,
        apiTests,
        activeApiTestId,
        setActiveApiTestId,
        addApiTest,
        executeApiTest,
        customSuites,
        addCustomSuite,
        runCustomSuite,
        activities,
        addActivity,
        qualityIntelligence,
        settings,
        updateSettings,
        isCommandPaletteOpen,
        setCommandPaletteOpen,
        isNewRunModalOpen,
        setNewRunModalOpen,
        notificationCount,
        clearNotifications,
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
