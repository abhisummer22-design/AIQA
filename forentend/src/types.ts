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
  | 'performance-testing'
  | 'mobile-testing'
  | 'custom'
  | 'feature-map'
  | 'reports'
  | 'audit'
  | 'evidence'
  | 'telemetry'
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

export type Environment = 'demo' | 'uat' | 'live';

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

// Phase A cleanup: `ActivityEvent` removed here together with the dead
// `activities`/`addActivity` write-only state it typed in `AIQAContext` —
// zero consumers remained anywhere in the app.

// FE-16: `TestExecutionItem`/`ExecutionRun` (a fabricated local execution-run
// model, fake IDs/counts/test names) removed here — their only producer was
// `AIQAContext.triggerNewRun`, called only from the Test Generator's former
// fake export path, and their only consumer was `AIQAContext`'s own `runs`
// state, which nothing ever rendered. See
// AIQA_PHASE4_FE16_TEST_GENERATOR_TRUTHFULNESS_FAKE_EXECUTION_HANDOFF_REMOVAL_IMPLEMENTATION_REPORT.md.

// Test Case Generator Models
export type GeneratorPipelineStep =
  | 'REQUIREMENT_INPUT'
  | 'APPLICATION_UNDERSTANDING'
  | 'AI_PLANNING'
  | 'GENERATED_CASES'
  | 'REVIEW'
  | 'APPROVE'
  | 'READY_FOR_EXECUTION';

export type RequirementDocType = 'PRD' | 'FRD' | 'SRD' | 'ADDITIONAL';

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

/** Truthfulness Cleanup Step 1 — every value here is a real, distinct
 *  outcome of an actual backend verification attempt (or, for `IDLE`, the
 *  honest absence of one yet). `VALIDATING`/`CONNECTION_FAILED` (the old,
 *  pre-fix names) are gone: the prior implementation collapsed every real
 *  failure mode into one generic `CONNECTION_FAILED`, and fabricated
 *  `CONNECTED` unconditionally after a fixed delay regardless of whether
 *  any real check occurred. See `app_source.rs::AppSourceStatus` (backend)
 *  — these values are a 1:1 mirror of the real API contract, not a second,
 *  independently-invented frontend vocabulary. */
export type AppConnectionStatus =
  | 'IDLE'
  | 'CHECKING'
  | 'CONNECTED'
  | 'UNREACHABLE'
  | 'REJECTED'
  | 'INVALID_URL'
  | 'ERROR';

export interface AppSourceConnection {
  type: AppSourceType;
  label: string;
  url: string;
  status: AppConnectionStatus;
  lastValidatedAt?: string;
  /** Only ever populated from a real backend `VerifyApplicationSourceResponse`
   *  — never fabricated client-side. The old `sslValid`/`title`/
   *  `environmentRole`/`pingMs` fields are gone: no real signal, from either
   *  the client or the backend, ever backed them (`sslValid` in particular
   *  claimed certificate-validity detail `reqwest`'s default API cannot
   *  actually provide — removed rather than fabricated, per the
   *  Truthfulness Cleanup Step 1 report). */
  details?: {
    httpStatus?: number;
    latencyMs?: number;
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
  /** AUD-005 — 'PENDING_REVIEW'/'APPROVED'/'REJECTED' also describe an
   *  unsaved, local-only draft's own lightweight pre-save triage state
   *  (unchanged since FE-16). 'VALIDATED' and 'EXECUTED' only ever appear
   *  once a case has been persisted for real — they mirror
   *  `contract.ts::TestCaseStatus` exactly (`GENERATED` displays as
   *  'PENDING_REVIEW' for this local vocabulary's own continuity).
   *  'EXPORTED_TO_EXECUTION' is kept only for any remaining local-draft
   *  code path; real persisted cases use 'EXECUTED' instead. */
  status: 'PENDING_REVIEW' | 'VALIDATED' | 'APPROVED' | 'REJECTED' | 'EXPORTED_TO_EXECUTION' | 'EXECUTED';
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
  // FE-16: `model` (a hardcoded fake provider id — 'gemini-2.0-flash' etc.)
  // removed — selecting it never had any real generation effect. See the
  // FE-16 report.
  depth: 'SMOKE' | 'STANDARD' | 'EXHAUSTIVE' | 'CHAOS_EXPLORATORY';
  /** Skills & MCP Phase 1 — real, session-only selected skill ids (see
   *  `hooks/useSkillSelection.ts`), carried across a branch switch the same
   *  way every other generation-config field on this session already is.
   *  Not sent to any backend; this local template pipeline does not yet
   *  use these to vary generated content (see `TestCaseGeneratorPage.tsx`'s
   *  own doc comment on `handleTriggerGenerate`). */
  selectedSkillIds?: string[];
  generatedCases: GeneratedTestCase[];
  createdAt: string;
}

// Cleanup: the fabricated `SystemSettings` model (general org info, fake
// SSO/execution/environment/policy/notification config, all local-only
// with no backend behind any of it) is removed — `SettingsPage.tsx` was
// its only consumer, and that page's mockup UI for it was removed at the
// explicit instruction to keep only what's real.

