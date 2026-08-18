/**
 * AIQA API Contract v1.0 — Frontend Type Definitions
 *
 * Derived verbatim from AIQA_API_CONTRACT_v1.0.md.
 * Represents the target schema for all project-scoped endpoints, AI test generation,
 * validation gates, quality graph topology, RBAC, API/Mobile testing, reports, and model gateway.
 */

// ----------------------------------------------------------------- §4 Auth APIs
export interface AuthUser {
  id: string;
  email: string;
  name: string;
  role: "viewer" | "operator" | "admin";
  created_at: string;
}

export interface AuthSession {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  token_type: "Bearer";
  user: AuthUser;
}

// -------------------------------------------------------------- §5 Project APIs
export type ProjectEnvironment = "development" | "staging" | "uat" | "production" | "ephemeral";

export interface Project {
  id: string;
  name: string;
  key: string;
  description: string | null;
  repository_url: string | null;
  default_environment: ProjectEnvironment;
  created_at: string;
  updated_at: string;
}

export interface CreateProjectRequest {
  name: string;
  key: string;
  description?: string;
  repository_url?: string;
  default_environment?: ProjectEnvironment;
}

export interface UpdateProjectRequest {
  name?: string;
  description?: string;
  repository_url?: string;
  default_environment?: ProjectEnvironment;
}

// ---------------------------------------------------- §6 Context Ingestion APIs
export type ContextType = "PRD" | "FRD" | "SRD" | "OPENAPI" | "GRAPHQL_SCHEMA" | "DATABASE_DDL" | "FIGMA" | "EXISTING_TESTS";
export type ContextProcessingStatus = "QUEUED" | "PROCESSING" | "READY" | "FAILED" | "NEEDS_REVIEW";

export interface ContextItem {
  id: string;
  project_id: string;
  name: string;
  context_type: ContextType;
  status: ContextProcessingStatus;
  source_uri: string | null;
  size_bytes: number;
  extracted_nodes_count: number;
  created_at: string;
  updated_at: string;
}

export interface IngestContextRequest {
  name: string;
  context_type: ContextType;
  content?: string;
  source_uri?: string;
}

// -------------------------------------------------- §7 Quality Graph / Req APIs
export type QualityNodeType = "REQUIREMENT" | "FEATURE" | "MODULE" | "API_ENDPOINT" | "UI_COMPONENT" | "DATA_MODEL" | "TEST_CASE" | "FINDING";

export interface QualityNode {
  id: string;
  project_id: string;
  node_type: QualityNodeType;
  identifier: string;
  name: string;
  description: string | null;
  metadata: Record<string, unknown>;
  coverage_status: "UNCOVERED" | "PARTIAL" | "COVERED" | "FAILED";
}

export interface QualityEdge {
  id: string;
  source_node_id: string;
  target_node_id: string;
  relationship: "IMPLEMENTS" | "COVERS" | "DEPENDS_ON" | "PRODUCES" | "VERIFIES";
}

export interface QualityGraphTopology {
  nodes: QualityNode[];
  edges: QualityEdge[];
  coverage_ratio: number;
  uncovered_nodes_count: number;
}

export interface Requirement {
  id: string;
  project_id: string;
  identifier: string;
  title: string;
  description: string;
  priority: "CRITICAL" | "HIGH" | "MEDIUM" | "LOW";
  category: "FUNCTIONAL" | "SECURITY" | "PERFORMANCE" | "RBAC" | "COMPLIANCE";
  created_at: string;
}

// --------------------------------------------- §8 Test Case Generator APIs
export type TestCategory =
  | "FUNCTIONAL"
  | "NON_FUNCTIONAL"
  | "UI_UX"
  | "EDGE_CASE"
  | "SCENARIO_BASED"
  | "BUSINESS_LOGIC"
  | "SECURITY"
  | "PENETRATION"
  | "RBAC"
  | "REGRESSION"
  | "DATABASE"
  | "API"
  | "PERFORMANCE"
  | "MOBILE";

export interface GenerateTestsRequest {
  scope: {
    module_ids?: string[];
    requirement_ids?: string[];
    endpoint_ids?: string[];
  };
  categories: TestCategory[];
  include_positive: boolean;
  include_negative: boolean;
  constraints?: {
    max_steps?: number;
    require_traceability?: boolean;
    timeout_sec?: number;
  };
}

export interface GeneratedTestCase {
  id: string;
  project_id: string;
  name: string;
  category: TestCategory;
  priority: "CRITICAL" | "HIGH" | "MEDIUM" | "LOW";
  requirement_id: string | null;
  module_id: string | null;
  preconditions: string[];
  steps: Array<{ step_number: number; action: string; expected: string }>;
  expected_outcome: string;
  validation_status: "PENDING" | "PASSED" | "FLAGGED" | "REJECTED";
  created_at: string;
}

export interface TestGenerationJob {
  job_id: string;
  project_id: string;
  status: "QUEUED" | "RUNNING" | "COMPLETED" | "FAILED";
  total_generated: number;
  created_at: string;
  finished_at: string | null;
}

// --------------------------------------------- §9 Test Case Validator APIs
export interface ValidateTestsRequest {
  test_case_ids: string[];
}

export interface TestValidationScore {
  traceability_score: number;
  semantic_precision_score: number;
  duplicate_risk_score: number;
  flakiness_risk_score: number;
}

export interface TestValidationResult {
  validation_id: string;
  test_case_id: string;
  status: "APPROVED" | "NEEDS_REVIEW" | "REJECTED";
  scores: TestValidationScore;
  rejection_reasons: string[];
  validated_at: string;
}

// ----------------------------------------- §10 Test Suite & Plan APIs
export interface TestSuite {
  id: string;
  project_id: string;
  name: string;
  description: string | null;
  test_case_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface CreateTestSuiteRequest {
  name: string;
  description?: string;
  test_case_ids: string[];
}

export interface TestPlanDefinition {
  id: string;
  project_id: string;
  name: string;
  suite_ids: string[];
  environment: ProjectEnvironment;
  execution_policy: "FAIL_FAST" | "CONTINUE_ON_FAILURE" | "HOLD_FOR_APPROVAL";
  concurrency: number;
  created_at: string;
}

export interface CreateTestPlanRequest {
  name: string;
  suite_ids: string[];
  environment: ProjectEnvironment;
  execution_policy?: "FAIL_FAST" | "CONTINUE_ON_FAILURE" | "HOLD_FOR_APPROVAL";
  concurrency?: number;
}

// --------------------------------------- §11 AIQA Orchestrator Planner APIs
export interface OrchestratorPlanRequest {
  objective: string;
  scope: {
    module_ids?: string[];
    suite_ids?: string[];
  };
  constraints?: {
    max_parallelism?: number;
    timeout_sec?: number;
  };
}

export interface OrchestratorPlannedTask {
  task_id: string;
  name: string;
  capability_executor: "playwright" | "schemathesis" | "newman" | "pytest" | "maestro";
  prerequisites: string[];
  estimated_cost_ms: number;
  risk_weight: number;
}

export interface OrchestratorPlan {
  plan_id: string;
  project_id: string;
  objective: string;
  tasks: OrchestratorPlannedTask[];
  resolved_parallelism: number;
  policy_check: "AUTO_APPROVED" | "PENDING_APPROVAL" | "DENIED";
  created_at: string;
}

// ---------------------------------------------------- §17 RBAC Testing APIs
export interface RBACMatrixRule {
  id: string;
  role: string;
  resource: string;
  action: "READ" | "WRITE" | "DELETE" | "ADMIN" | "EXECUTE";
  expected_decision: "ALLOW" | "DENY";
}

export interface RBACMatrix {
  project_id: string;
  roles: string[];
  resources: string[];
  rules: RBACMatrixRule[];
}

export interface GenerateRBACTestsRequest {
  roles: string[];
  resources?: string[];
}

// ----------------------------------------------------- §18 API Testing APIs
export interface ApiSpecification {
  id: string;
  project_id: string;
  name: string;
  format: "OPENAPI_3_0" | "GRAPHQL" | "POSTMAN_COLLECTION";
  endpoint_count: number;
  base_url: string;
  created_at: string;
}

export interface ApiTelemetrySummary {
  project_id: string;
  total_calls: number;
  p50_latency_ms: number;
  p95_latency_ms: number;
  p99_latency_ms: number;
  error_rate_percentage: number;
  schema_drift_detected: boolean;
}

// -------------------------------------------------- §19 Mobile Testing APIs
export type MobilePlatform = "ANDROID" | "IOS";
export type DeviceStatus = "AVAILABLE" | "BUSY" | "OFFLINE" | "MAINTENANCE";

export interface MobileDevice {
  id: string;
  name: string;
  platform: MobilePlatform;
  os_version: string;
  device_model: string;
  is_emulator: boolean;
  status: DeviceStatus;
  locked_by_execution_id: string | null;
}

export interface DeviceLockRequest {
  lease_duration_sec: number;
  flow_id: string;
}

export interface MaestroFlow {
  id: string;
  name: string;
  platform: MobilePlatform;
  target_package: string;
  flow_yaml_path: string;
}

// -------------------------------------------------- §20 Custom Testing APIs
export interface CustomTestObjective {
  id: string;
  project_id: string;
  title: string;
  objective_prompt: string;
  module_id: string | null;
  created_at: string;
}

export interface CreateCustomTestRequest {
  title: string;
  objective_prompt: string;
  module_id?: string;
  constraints?: Record<string, unknown>;
}

// --------------------------------------------------------- §21 Reports APIs
export type ReportType = "BUG_REPORT" | "TEST_EXECUTION_REPORT" | "FINAL_AUDIT_REPORT" | "COMPLIANCE_SUMMARY";
export type ReportFormat = "PDF" | "EXCEL" | "JSON";

export interface QualityReportSummary {
  id: string;
  project_id: string;
  title: string;
  report_type: ReportType;
  format: ReportFormat;
  execution_run_id: string | null;
  total_tests: number;
  passed_count: number;
  failed_count: number;
  defects_count: number;
  download_url: string | null;
  generated_at: string;
}

export interface CreateReportRequest {
  title: string;
  report_type: ReportType;
  format: ReportFormat;
  execution_run_id?: string;
  include_evidence?: boolean;
}

// --------------------------------------------------- §22 Model Gateway APIs
export interface ModelProviderHealth {
  provider_id: string;
  provider_name: string;
  model_id: string;
  status: "ONLINE" | "STANDBY" | "DEGRADED" | "OFFLINE";
  p95_latency_ms: number;
  fallback_available: boolean;
}

export interface ModelTelemetry {
  provider_id: string;
  model_id: string;
  calls_last_hour: number;
  total_tokens_consumed: number;
  estimated_cost_usd: number;
  fallback_events_count: number;
}
