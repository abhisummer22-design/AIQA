export type EnvironmentType = "local" | "development" | "staging" | "production";

export type ViewType =
  | "dashboard"
  | "ai-brain"
  | "projects"
  | "test-runs"
  | "test-cases"
  | "test-case-generator"
  | "api-testing"
  | "mobile-testing"
  | "rbac-testing"
  | "workspace"
  | "bugs"
  | "risk-flakiness"
  | "policies"
  | "audit-logs"
  | "reports"
  | "settings";

export type SystemStatusLevel = "HEALTHY" | "DEGRADED" | "CRITICAL";

export interface SystemHealthState {
  backend: "online" | "degraded" | "offline";
  frontend: "online" | "degraded" | "offline";
  database: "connected" | "degraded" | "disconnected";
  aiModel: "ready" | "degraded" | "offline";
  ollama: "connected" | "degraded" | "offline";
  apiConnectivity: number;
  testExecutor: "running" | "idle" | "stopped";
  queue: { status: "healthy" | "congested" | "failed"; pending: number };
  worker: { active: number; total: number; status: "healthy" | "warning" };
  voiceMic: "ready" | "disabled" | "error";
  lastChecked: string;
}

export interface KpiMetrics {
  environment: string;
  totalTests: {
    total: number;
    executedToday: number;
    passPercentage: number;
    trend: string;
  };
  testRuns: {
    running: number;
    completed: number;
    failed: number;
  };
  qualityScore: {
    score: number;
    trend: string;
    historical: { date: string; score: number }[];
  };
  riskScore: {
    level: "Low" | "Medium" | "High" | "Critical";
    value: number;
    criticalCount: number;
    highCount: number;
    mediumCount: number;
    lowCount: number;
  };
  flakiness: {
    percentage: number;
    trend: string;
    flakyTestCount: number;
  };
  openBugs: {
    critical: number;
    high: number;
    medium: number;
    low: number;
    total: number;
  };
}

export interface LiveExecution {
  id: string;
  testName: string;
  project: string;
  environment: string;
  executor: string;
  status: "running" | "passed" | "failed" | "cancelled";
  durationSec: number;
  passed: number;
  failed: number;
  total: number;
  progress: number;
  startTime: string;
  logs: string[];
}

export interface FailureItem {
  id: string;
  testName: string;
  failureType:
    | "Assertion mismatch"
    | "API error"
    | "Timeout"
    | "Authentication failure"
    | "Schema violation"
    | "Dependency failure"
    | "Infrastructure failure"
    | "Unknown";
  httpStatus?: number;
  errorMessage: string;
  environment: string;
  timestamp: string;
  riskLevel: "Critical" | "High" | "Medium" | "Low";
  suggestedAction: string;
  service: string;
  relatedTest: string;
  payloadPreview?: string;
}

export interface BugCandidate {
  id: string;
  title: string;
  severity: "Critical" | "High" | "Medium" | "Low";
  confidence: number;
  relatedTest: string;
  relatedService: string;
  evidenceCount: number;
  suggestedPriority: "P0 - Urgent" | "P1 - Blocker" | "P2 - Normal" | "P3 - Minor";
  status: "pending_review" | "created" | "ignored";
  description: string;
  createdAt: string;
}

export interface ModelRouterState {
  primaryModel: {
    name: string;
    runtime: string;
    status: "online" | "offline" | "degraded";
    healthPercent: number;
    memoryUsageMb: number;
    cpuPercent: number;
    gpuPercent: number;
    modelLoadTimeMs: number;
    inferenceLatencyMs: number;
    requestsPerMin: number;
  };
  fallbackModel: {
    name: string;
    runtime: string;
    status: "available" | "active" | "offline";
    healthPercent: number;
    inferenceLatencyMs: number;
  };
  currentActiveModel: string;
  currentLatencyMs: number;
  requestsPerMinute: number;
  averageLatencyMs: number;
  successRate: number;
  errorRate: number;
  fallbackCountToday: number;
  routingStrategy: string;
}

export interface FlakyTestItem {
  id: string;
  testName: string;
  flakinessPercent: number;
  failureFrequency: string;
  lastFailure: string;
  retries: number;
  quarantined: boolean;
  service: string;
  trend: string;
}

export interface TimelineEvent {
  time: string;
  event: string;
  category: "execution" | "network" | "failure" | "evidence" | "risk" | "ai";
  detail: string;
  level: "info" | "warn" | "error";
}

export interface AuditLogItem {
  id: string;
  timestamp: string;
  actor: string;
  action: string;
  resource: string;
  result: "Success" | "Failed" | "Blocked";
  ip: string;
  tenant: string;
}

export interface GovernancePolicy {
  id: string;
  name: string;
  status: "Active" | "Draft" | "Disabled";
  violations: number;
  blockedExecutions: number;
  severity: "Critical" | "High" | "Medium" | "Low";
  description: string;
}

export interface EvidenceItem {
  id: string;
  title: string;
  type: "screenshot" | "trace" | "payload" | "log" | "har";
  testName: string;
  timestamp: string;
  environment: string;
  source: string;
  relatedFailure: string;
  size: string;
  content: string;
  imageUrl?: string;
}

export interface QualityNode {
  id: string;
  label: string;
  type: "service" | "api" | "test" | "bug" | "risk" | "evidence";
  status?: string;
  risk?: string;
  latency?: string;
  flakiness?: string;
  severity?: string;
  count?: number;
  parent?: string;
}

export interface QualityEdge {
  from: string;
  to: string;
  relationship: string;
}

export interface QualityGraphData {
  nodes: QualityNode[];
  edges: QualityEdge[];
}

export interface ProjectItem {
  id: string;
  name: string;
  key: string;
  tier: string;
  testCount: number;
  passRate: number;
  risk: "Low" | "Medium" | "High" | "Critical";
  lastRun: string;
  owner: string;
  description: string;
}

export interface AlertItem {
  id: string;
  level: "CRITICAL" | "DEGRADED" | "INFO";
  title: string;
  message: string;
  timestamp: string;
  component: string;
  actionLabel?: string;
  resolved?: boolean;
}

// --- RBAC Domain Types ---
export type DefaultRoleId =
  | "super_admin"
  | "org_admin"
  | "project_admin"
  | "qa_engineer"
  | "developer"
  | "viewer";

export interface RbacPermission {
  id: string; // e.g. "tests.execute"
  resource: string; // e.g. "tests"
  action: string; // e.g. "execute"
  group: string; // e.g. "Test Management"
  description: string;
}

export interface RbacRole {
  id: string;
  name: string;
  description: string;
  type: "system" | "custom";
  permissions: string[];
  userCount?: number;
  isProtected?: boolean;
}

export interface RbacUser {
  id: string;
  email: string;
  name: string;
  tenantId: string;
  tenantName: string;
  roleId: string;
  roleName: string;
  projectIds: string[]; // Assigned projects
  status: "active" | "disabled";
  lastActive: string;
  createdAt: string;
}

export interface RbacTenant {
  id: string;
  name: string;
  slug: string;
  tier: "Enterprise" | "Business" | "Starter";
  status: "active" | "suspended";
}

export interface RbacAuditEvent {
  id: string;
  timestamp: string;
  actorId: string;
  actorEmail: string;
  tenantId: string;
  action: string;
  resourceType: string;
  resourceId: string;
  oldValue?: string;
  newValue?: string;
  ip: string;
  result: "SUCCESS" | "FAILED" | "BLOCKED" | "DENIED";
  details?: string;
}

export interface AuthorizationProbeResult {
  authorized: boolean;
  httpStatus: number;
  decision: "ALLOWED" | "DENIED";
  reason: string;
  caller: {
    userId: string;
    email: string;
    role: string;
    tenantId: string;
    projects: string[];
    permissionsCount: number;
  };
  endpoint: {
    method: string;
    path: string;
    requiredPermission: string;
    resourceTenantId?: string;
    resourceProjectId?: string;
  };
  jwtPayload: Record<string, any>;
  evaluatedAt: string;
}

export interface RbacSecuritySuiteTest {
  id: string;
  title: string;
  category: "Authentication" | "Authorization" | "Tenant Isolation" | "Project Isolation" | "Privilege Escalation" | "Role Mutation" | "Disabled User" | "API Bypass";
  description: string;
  expectedStatus: number;
  actualStatus: number;
  verdict: "PASSED" | "FAILED";
  latencyMs: number;
  details: string;
}

export interface RbacFullState {
  users: RbacUser[];
  roles: RbacRole[];
  permissions: RbacPermission[];
  tenants: RbacTenant[];
  currentSessionUser: RbacUser;
  auditEvents: RbacAuditEvent[];
}

export interface DashboardState {
  systemHealth: SystemHealthState;
  kpiMetrics: KpiMetrics;
  liveExecutions: LiveExecution[];
  qualityGraph: QualityGraphData;
  recentFailures: FailureItem[];
  bugCandidates: BugCandidate[];
  evidence: EvidenceItem[];
  modelRouter: ModelRouterState;
  flakyTests: FlakyTestItem[];
  timeline: TimelineEvent[];
  governance: GovernancePolicy[];
  auditLogs: AuditLogItem[];
  alerts: AlertItem[];
  rbac?: RbacFullState;
}

// ==========================================
// --- AI BRAIN & ENGINE CONFIGURATION TYPES ---
// ==========================================

export type BrainEngineSource = "ollama" | "gemini" | "openai" | "anthropic" | "deepseek" | "groq" | "mistral" | "custom_openai";

export type BrainRoutingMode = "smart_auto" | "strict_local" | "cloud_priority" | "task_matrix";

export interface LocalOllamaModel {
  id: string;
  name: string;
  tag: string;
  sizeGb: number;
  quantization: string;
  parameters: string;
  family: string;
  isPrimary: boolean;
  status: "loaded" | "available" | "downloading" | "offline";
  vramUsageMb: number;
  contextLength: number;
  specialization: string;
  lastUsed?: string;
  downloadProgress?: number;
}

export interface CloudApiProvider {
  id: BrainEngineSource;
  name: string;
  provider: string;
  badge: string;
  iconType: string;
  description: string;
  isConfigured: boolean;
  hasKey: boolean;
  maskedKey?: string;
  customBaseUrl?: string;
  selectedModel: string;
  availableModels: string[];
  status: "ready" | "connected" | "invalid_key" | "missing_key" | "rate_limited" | "testing";
  latencyMs?: number;
  tokensPerSec?: number;
  contextWindow: string;
  costPer1kTokens: string;
  features: string[];
}

export interface BrainTaskRouting {
  taskKey: string;
  taskTitle: string;
  description: string;
  assignedEngine: BrainEngineSource;
  assignedModel: string;
  fallbackEngine: BrainEngineSource;
  fallbackModel: string;
  temperature: number;
  maxTokens: number;
  reasoningEffort?: "low" | "medium" | "high";
}

export interface BrainTelemetry {
  todayTotalInferences: number;
  localInferencesCount: number;
  cloudInferencesCount: number;
  localRatioPercent: number;
  avgLatencyMs: number;
  tokensGeneratedToday: number;
  costSavedDollars: number;
  fallbackIncidentsToday: number;
  vramAllocatedMb: number;
  vramTotalMb: number;
  gpuUtilization: number;
  cpuUtilization: number;
  airgapComplianceScore: number;
}

export interface BrainBenchmarkRun {
  id: string;
  engine: BrainEngineSource;
  model: string;
  prompt: string;
  response: string;
  latencyMs: number;
  timeToFirstTokenMs: number;
  tokensPerSec: number;
  promptTokens: number;
  completionTokens: number;
  costDollars: number;
  timestamp: string;
  status: "success" | "error";
  errorMessage?: string;
  endpointUrl?: string;
}

export interface ModelEndpointItem {
  id: string;
  name: string;
  engine: BrainEngineSource;
  modelTag: string;
  endpointUrl: string;
  instanceType: "local_ollama" | "remote_api" | "custom_endpoint";
  instanceLabel: string;
  parameters: string;
  quantization: string;
  contextWindow: string;
  latencyMs: number;
  tokensPerSec: number;
  costPer1k: string;
  status: "ready" | "loaded" | "available" | "connected" | "testing" | "offline";
  isPrimary: boolean;
  specialization: string;
  providerName: string;
}

export interface AbBenchmarkComparison {
  id: string;
  timestamp: string;
  prompt: string;
  scenarioTitle?: string;
  modelA: BrainBenchmarkRun & { endpointName: string; instanceType: string };
  modelB: BrainBenchmarkRun & { endpointName: string; instanceType: string };
  delta: {
    latencyDiffMs: number;
    latencyFasterPct: number;
    speedDiffTps: number;
    speedFasterPct: number;
    costSavingsDollars: number;
    winnerLatency: "A" | "B" | "TIE";
    winnerSpeed: "A" | "B" | "TIE";
    winnerCost: "A" | "B" | "TIE";
    overallRecommendation: string;
  };
}

export interface EndpointFailureAlertConfig {
  enabled: boolean;
  failureThreshold: number;
  soundEnabled: boolean;
  monitoredEndpoints: string[]; // ["all"] or specific IDs
  autoFallbackOnAlert: boolean;
  browserPermission?: "default" | "granted" | "denied";
}

export interface EndpointFailureIncident {
  id: string;
  endpointId: string;
  endpointName: string;
  endpointType: "local_ollama" | "remote_api" | "custom_ingress";
  consecutiveFailures: number;
  timestamp: string;
  lastErrorMessage: string;
  notificationSent: boolean;
}

// ----------------------------------------------------
// Test Case Execution Hub — Test Section Enhancement
// ----------------------------------------------------

export type TestType =
  | "FUNCTIONAL"
  | "NON_FUNCTIONAL"
  | "UI"
  | "PERFORMANCE"
  | "SECURITY"
  | "DATABASE"
  | "EDGE_CASE"
  | "SCENARIO_BASED"
  | "BUSINESS_CASE"
  | "UX";

export type TestCaseStatus =
  | "Not Run"
  | "Queued"
  | "Running"
  | "Passed"
  | "Failed"
  | "Blocked"
  | "Skipped";

export type TestCasePriority = "Critical" | "High" | "Medium" | "Low";

export type TestCaseSeverity = "Blocker" | "Critical" | "Major" | "Minor" | "Trivial";

export interface TestCaseExecutionRecord {
  start_time: string;
  end_time: string;
  duration_ms: number;
  environment: string;
  executor: string;
  status: "Passed" | "Failed" | "Blocked" | "Skipped";
  actual_result?: string;
  failure_reason?: string;
  error_stack?: string;
  logs: string[];
  evidence?: {
    type: "screenshot" | "network_trace" | "dom_snapshot" | "payload" | "diff";
    title: string;
    data: string;
  }[];
  metrics?: {
    p95_latency_ms?: number;
    throughput_rps?: number;
    cpu_percent?: number;
    memory_mb?: number;
  };
}

export interface TestCase {
  id: string;
  name: string;
  description: string;
  test_type: TestType;
  priority: TestCasePriority;
  severity: TestCaseSeverity;
  status: TestCaseStatus;
  project_id: string;
  project_name?: string;
  environment: "Local" | "Development" | "Staging" | "Production";
  owner_id: string;
  owner_name: string;
  preconditions?: string[];
  steps: string[];
  expected_result: string;
  actual_result?: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  last_execution?: TestCaseExecutionRecord;
}

export interface TestCoverageItem {
  test_type: TestType;
  displayName: string;
  percentage: number;
  totalTests: number;
  passedTests: number;
  failedTests: number;
  targetPercentage: number;
}

export interface TestExecutionAnalytics {
  totalTests: number;
  passed: number;
  failed: number;
  blocked: number;
  skipped: number;
  notRun: number;
  running: number;
  passRate: number;
  failureRate: number;
  avgExecutionTimeMs: number;
  byType: Record<TestType, { total: number; passed: number; failed: number; coverage: number }>;
}

export interface TestRecommendation {
  id: string;
  test_type: TestType;
  targetFeature: string;
  reason: string;
  suggestedCount: number;
  priority: "High" | "Medium" | "Low";
}



