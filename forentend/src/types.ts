/**
 * AIQA - Quality Engineering Control Plane
 * Core Domain Types & Data Models
 */

export type NavPage =
  | 'dashboard'
  | 'generator'
  | 'orchestrator'
  | 'validator'
  | 'execution'
  | 'rbac'
  | 'api-testing'
  | 'mobile-testing'
  | 'custom'
  | 'feature-map'
  | 'reports'
  | 'audit'
  | 'evidence'
  | 'settings';

export type AIQAStatus =
  | 'QUEUED'
  | 'RUNNING'
  | 'PASSED'
  | 'FAILED'
  | 'BLOCKED'
  | 'SKIPPED'
  | 'CANCELLED'
  | 'TIMEOUT'
  | 'INFRA_ERROR'
  | 'NEEDS_REVIEW';

export type FailureClassification =
  | 'PRODUCT_FAILURE'
  | 'INFRASTRUCTURE_FAILURE'
  | 'UNCLASSIFIED';

export type Environment = 'production' | 'staging' | 'uat' | 'ephemeral-pr';

export type ProjectStatus = 'Healthy' | 'Running' | 'Warning' | 'Degraded' | 'Critical' | 'Offline';

export interface ProjectMetrics {
  totalTestCases: number;
  activeRunsCount: number;
  passed: number;
  failed: number;
  blocked: number;
  needsReview: number;
  skipped: number;
  running: number;
  queued: number;
}

export interface ProjectExecutionPoint {
  timestamp: string;
  timeLabel: string;
  testsExecuted: number;
  passed: number;
  failed: number;
  blocked: number;
}

export interface AIQAProject {
  id: string;
  name: string;
  slug: string;
  status: ProjectStatus;
  environment: string;
  lastActivity: string;
  metrics: ProjectMetrics;
  activeRun?: {
    id: string;
    title: string;
    status: AIQAStatus;
    startedAt: string;
    durationEstimate: string;
    testsExecuted: number;
    totalTests: number;
    progressPct: number;
  };
  timelineData: ProjectExecutionPoint[];
}

export interface BackendConnectionState {
  status: 'connected' | 'disconnected' | 'connecting' | 'degraded';
  endpoint: string;
  daemonVersion?: string;
  latencyMs?: number;
  lastHeartbeat?: string;
}

export interface ActivityEvent {
  id: string;
  type:
    | 'RUN_STARTED'
    | 'TEST_COMPLETED'
    | 'FAILURE_DETECTED'
    | 'EVIDENCE_CAPTURED'
    | 'REVIEW_REQUIRED'
    | 'RUN_COMPLETED'
    | 'INFRA_ALERT';
  timestamp: string;
  title: string;
  description: string;
  runId?: string;
  testId?: string;
  status?: AIQAStatus;
  classification?: FailureClassification;
  actor?: string;
}

export interface TestExecutionItem {
  id: string;
  runId: string;
  name: string;
  suite: string;
  type: 'E2E' | 'API' | 'RBAC' | 'INTEGRATION' | 'UNIT' | 'CUSTOM';
  status: AIQAStatus;
  durationMs?: number;
  failureClassification?: FailureClassification;
  failureMessage?: string;
  stackTrace?: string;
  executor: string;
  retries: number;
  evidenceFiles?: Array<{ name: string; type: 'screenshot' | 'trace' | 'har' | 'log'; size: string }>;
  logs?: string[];
}

export interface ExecutionRun {
  id: string;
  title: string;
  environment: Environment;
  status: AIQAStatus;
  startedAt: string;
  completedAt?: string;
  executor: 'k8s-runner-pool-us' | 'docker-local-agent' | 'cloud-playwright-grid' | 'ephemeral-runner';
  concurrency: number;
  policyApprovalState: 'APPROVED' | 'AUTO_APPROVED' | 'PENDING_APPROVAL' | 'REJECTED';
  totalTests: number;
  counts: {
    passed: number;
    failed: number;
    blocked: number;
    skipped: number;
    needsReview: number;
    infraErrors: number;
    running: number;
    queued: number;
  };
  productFailuresCount: number;
  infraFailuresCount: number;
  tests: TestExecutionItem[];
}

// Test Case Generator Models
export type GeneratorPipelineStep =
  | 'REQUIREMENT_INPUT'
  | 'APPLICATION_UNDERSTANDING'
  | 'AI_PLANNING'
  | 'GENERATED_CASES'
  | 'REVIEW'
  | 'APPROVE'
  | 'READY_FOR_EXECUTION';

export type RequirementDocType = 'PRD' | 'FRD' | 'SRD' | 'MRD' | 'ADDITIONAL';

export interface UploadedRequirementDoc {
  slotId: RequirementDocType;
  label: string;
  fullName: string;
  fileName: string;
  fileSize: number;
  fileType: string;
  uploadedAt: string;
  status: 'READY' | 'UPLOADING' | 'ERROR';
  errorMessage?: string;
}

export type AppSourceType = 'STAGING' | 'DEMO' | 'LIVE' | 'FIGMA';

export type AppConnectionStatus =
  | 'NOT_CONNECTED'
  | 'CONNECTED'
  | 'VALIDATING'
  | 'INVALID_URL'
  | 'CONNECTION_FAILED';

export interface AppSourceConnection {
  type: AppSourceType;
  label: string;
  url: string;
  status: AppConnectionStatus;
  lastValidatedAt?: string;
  details?: {
    title?: string;
    lastUpdated?: string;
    httpStatus?: number;
    sslValid?: boolean;
    pingMs?: number;
    environmentRole?: string;
  };
  errorMessage?: string;
}

export type TestGenerationCategory =
  | 'Functional'
  | 'Non-Functional'
  | 'Positive'
  | 'Negative'
  | 'UI/UX'
  | 'Spike'
  | 'Load'
  | 'Edge Case'
  | 'Senior Base'
  | 'Business Logic'
  | 'API'
  | 'Performance'
  | 'Security'
  | 'Penetration';

export type TestCasePriority = 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW';
export type TestCaseSeverity = 'CRITICAL' | 'MAJOR' | 'MINOR' | 'TRIVIAL';

export type GenerationExecutionState =
  | 'IDLE'
  | 'VALIDATING'
  | 'PROCESSING'
  | 'GENERATING'
  | 'COMPLETED'
  | 'FAILED'
  | 'PARTIAL';

export interface TraceabilityChain {
  sourceDoc: string;
  requirementRef: string;
  requirementText: string;
  aiAnalysis: string;
  testCaseId: string;
  testCategory: string;
  executionTarget: string;
  evidencePolicy: string;
  reportMapping: string;
}

export interface GeneratedTestCase {
  id: string;
  title: string;
  objective?: string;
  description: string;
  category: TestGenerationCategory | string;
  priority: TestCasePriority;
  severity: TestCaseSeverity;
  sourceDocument?: string;
  requirementReference?: string;
  sourceRequirement?: string;
  traceability?: TraceabilityChain;
  preconditions: string[];
  testData?: string;
  steps: Array<{ stepNumber: number; action: string; expectedResult: string }>;
  expectedResult?: string;
  assertions: string[];
  riskFactor: 'LOW' | 'MEDIUM' | 'HIGH';
  status: 'PENDING_REVIEW' | 'APPROVED' | 'REJECTED' | 'EXPORTED_TO_EXECUTION';
  reviewNotes?: string;
  suggestedTags: string[];
  generatedAt?: string;
}

export interface GenerationSession {
  id: string;
  requirementText: string;
  specType: 'NATURAL_LANGUAGE' | 'OPENAPI_SPEC' | 'USER_STORY' | 'FIGMA_SPEC';
  specInput?: string;
  targetAppUrl?: string;
  currentStep: GeneratorPipelineStep;
  generationState?: GenerationExecutionState;
  selectedCategories?: TestGenerationCategory[];
  documents?: UploadedRequirementDoc[];
  appSources?: Record<AppSourceType, AppSourceConnection>;
  model: 'gemini-1.5-pro' | 'gemini-2.0-flash' | 'claude-3-5-sonnet';
  depth: 'SMOKE' | 'STANDARD' | 'EXHAUSTIVE' | 'CHAOS_EXPLORATORY';
  generatedCases: GeneratedTestCase[];
  createdAt: string;
}

// RBAC Testing Models
export interface RBACMatrixRule {
  id: string;
  role: string;
  resource: string;
  action: 'READ' | 'WRITE' | 'DELETE' | 'EXECUTE' | 'ADMIN' | 'IMPERSONATE';
  expectedAccess: 'ALLOW' | 'DENY' | 'CONDITIONAL';
  actualAccess?: 'ALLOW' | 'DENY' | 'CONDITIONAL' | 'ERROR';
  conditionCriteria?: string;
  status?: AIQAStatus;
  evidenceNote?: string;
  evaluatedAt?: string;
}

export interface RBACScenario {
  id: string;
  name: string;
  description: string;
  targetEnvironment: Environment;
  rolesUnderAudit: string[];
  rules: RBACMatrixRule[];
}

// API Testing Models
export type APIAdapterType = 'REST_HTTP' | 'GRAPHQL' | 'GRPC' | 'WEBSOCKET';

export interface APIAssertion {
  id: string;
  property: 'STATUS_CODE' | 'RESPONSE_TIME' | 'JSON_SCHEMA' | 'HEADER_VALUE' | 'BODY_JSON_PATH';
  operator: 'EQUALS' | 'NOT_EQUALS' | 'CONTAINS' | 'MATCHES_REGEX' | 'LESS_THAN' | 'EXISTS';
  targetValue: string;
  passed?: boolean;
  actualValue?: string;
  errorMessage?: string;
}

export interface APIEndpointTest {
  id: string;
  name: string;
  adapter: APIAdapterType;
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  endpointUrl: string;
  headers: Record<string, string>;
  queryParams: Record<string, string>;
  authType: 'NONE' | 'BEARER_TOKEN' | 'API_KEY' | 'OAUTH2' | 'BASIC_AUTH';
  authConfig?: Record<string, string>;
  bodyType: 'NONE' | 'JSON' | 'FORM_DATA' | 'RAW';
  bodyPayload?: string;
  assertions: APIAssertion[];
  lastExecution?: {
    timestamp: string;
    status: AIQAStatus;
    statusCode?: number;
    responseTimeMs?: number;
    responseHeaders?: Record<string, string>;
    responseBody?: string;
    failureClassification?: FailureClassification;
  };
}

// Custom Testing Models
export interface CustomTestSuite {
  id: string;
  name: string;
  category: 'VISUAL_REGRESSION' | 'LOAD_AND_STRESS' | 'CHAOS_RESILIENCE' | 'SYNTHETIC_E2E';
  description: string;
  configuration: Record<string, unknown>;
  targetUrl: string;
  concurrencyLevel: number;
  timeoutSeconds: number;
  status: AIQAStatus;
  lastRunTime?: string;
}

// Reports & Quality Intelligence Models
export interface QualityIntelligence {
  riskScore: number; // 0 - 100
  riskLevel: 'LOW' | 'MODERATE' | 'HIGH' | 'CRITICAL';
  changeImpact: {
    impactedModules: string[];
    riskVector: string;
    recommendedRegressionDepth: string;
  };
  flakinessIndex: number; // %
  testValueScore: number; // 0 - 100
  failureBreakdown: {
    productDefects: number;
    infraErrors: number;
    needsReviewCount: number;
  };
  qualityGraphNodes: Array<{ id: string; name: string; health: number; type: 'SERVICE' | 'UI' | 'DATABASE' | 'GATEWAY' }>;
}

export interface DefectDraft {
  id: string;
  title: string;
  severity: 'CRITICAL' | 'MAJOR' | 'MINOR';
  classification: FailureClassification;
  relatedTestId: string;
  reproductionSteps: string[];
  suggestedFixContext: string;
  jiraExportStatus: 'DRAFT' | 'EXPORTED';
}

// System Settings Models
export interface SystemSettings {
  general: {
    organizationName: string;
    projectKey: string;
    timezone: string;
    dataRetentionDays: number;
  };
  authentication: {
    ssoEnabled: boolean;
    provider: 'OKTA' | 'GOOGLE_WORKSPACE' | 'AZURE_AD' | 'LOCAL';
    apiTokenStatus: 'ACTIVE' | 'EXPIRED' | 'NOT_CONFIGURED';
  };
  execution: {
    defaultExecutor: string;
    maxConcurrency: number;
    globalTimeoutSeconds: number;
    autoRetryCount: number;
    isolateInfraFailures: boolean;
  };
  environments: Array<{
    id: Environment;
    name: string;
    baseUrl: string;
    healthEndpoint: string;
    isActive: boolean;
  }>;
  policies: {
    blockOnProductFailures: boolean;
    blockOnInfraFailures: boolean;
    requireManualApprovalForProd: boolean;
    minimumPassPercentage: number;
  };
  aiModels: {
    defaultModel: string;
    temperature: number;
    reasoningEffort: 'LOW' | 'MEDIUM' | 'HIGH';
    ragGroundingEnabled: boolean;
  };
  notifications: {
    slackWebhook: string;
    emailAlertsEnabled: boolean;
    pagerDutyKeyConfigured: boolean;
    notifyOnInfraErrorOnly: boolean;
  };
}

// Orchestrator Domain Models
export interface OrchestratorPlan {
  id: string;
  name: string;
  scope: string;
  riskScore: number;
  changeDeltaImpact: 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW';
  testValueScore: number;
  status: AIQAStatus;
  specializedAgents: Array<{
    capability: 'SECURITY_AUDIT' | 'API_CONTRACT' | 'LOAD_CHAOS' | 'MOBILE_DEVICE' | 'RBAC_MATRIX' | 'E2E_SYNTHETIC';
    status: 'ACTIVE' | 'IDLE' | 'COMPLETED' | 'STANDBY';
    assignedWorkers: number;
    testsHandled: number;
  }>;
  dagNodes: Array<{
    id: string;
    label: string;
    type: 'STAGE' | 'SUITE' | 'GATE';
    status: AIQAStatus;
    dependencies: string[];
    durationEst: string;
  }>;
  createdAt: string;
}

// Test Case Validator Domain Models
export interface ValidationRuleCheck {
  id: string;
  testCaseId: string;
  testCaseTitle: string;
  traceabilitySource: 'PRD-PAY-001' | 'FRD-AUTH-09' | 'FIGMA-FRAME-402' | 'SRD-API-SPEC';
  traceabilityStatus: 'LINKED' | 'ORPHAN' | 'PARTIAL';
  qualityScore: number; // 0 - 100
  duplicateSimilarityPct: number; // 0 - 100
  duplicateWithId?: string;
  riskCoverage: 'CRITICAL_PATH' | 'EDGE_CASE' | 'SECURITY_BOUNDARY' | 'SUPERFLUOUS';
  approvalState: 'APPROVED' | 'REQUIRES_REMEDIATION' | 'AUTO_BLOCKED';
  remediationSuggestions?: string[];
}

// Mobile Testing Domain Models
export interface MobileDeviceTarget {
  id: string;
  name: string;
  platform: 'ANDROID' | 'IOS';
  osVersion: string;
  manufacturer: 'Samsung' | 'Google' | 'Apple' | 'OnePlus' | 'Motorola';
  screenDensity: string;
  status: 'AVAILABLE' | 'LOCKED_RESERVED' | 'EXECUTING' | 'OFFLINE';
  batteryLevel: number;
  thermalStatus: 'NOMINAL' | 'WARM' | 'THROTTLED';
  lockedBy?: string;
}

export interface MobileTestExecution {
  id: string;
  title: string;
  deviceId: string;
  coverageType:
    | 'DEEP_LINKING'
    | 'OFFLINE_CACHE_SYNC'
    | 'BIOMETRIC_TOKEN_AUTH'
    | 'PUSH_NOTIFICATION_PIPELINE'
    | 'BACKGROUND_WORKER_RESILIENCE'
    | 'NETWORK_THROTTLING_DEGRADATION'
    | 'MEMORY_LEAK_PROFILE';
  status: AIQAStatus;
  durationMs: number;
  deviceMetrics: {
    cpuPeakPct: number;
    ramUsedMb: number;
    networkPackets: number;
    batteryDrainPct: number;
  };
  logSummary: string;
}

// Feature Map & Flow Domain Models
export interface FeatureMapNode {
  id: string;
  name: string;
  module: 'AUTH' | 'PAYMENTS' | 'LEDGER' | 'NOTIFICATIONS' | 'WORKFLOWS' | 'GATEWAY';
  type: 'FRONTEND_VIEW' | 'API_GATEWAY' | 'MICROSERVICE' | 'SQL_DATABASE' | 'ASYNC_QUEUE';
  healthScore: number;
  testCoveragePct: number;
  dependencies: string[];
  activeAlertsCount: number;
}

export interface SystemCallFlow {
  id: string;
  title: string;
  entryTrigger: string;
  steps: Array<{
    stepNumber: number;
    sourceNode: string;
    targetNode: string;
    protocol: 'REST_HTTP' | 'GRPC' | 'EVENT_BUS' | 'DB_QUERY' | 'WS';
    actionName: string;
    latencyMs: number;
    status: 'HEALTHY' | 'DEGRADED' | 'ERROR';
  }>;
  totalLatencyMs: number;
  lastTracedAt: string;
}
