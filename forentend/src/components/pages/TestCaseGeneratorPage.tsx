/**
 * AIQA Test Case Generator Page — Main Content Area
 * Futuristic AI Quality Engineering Test Generation Control Plane
 * Consumes:
 * - 5 Document Slots (PRD, FRD, SRD, MRD, Additional Document)
 * - 4 Application Sources (Staging, Demo, Live, Figma)
 * - 14 Exact Test Categories
 * - Dual-branch navigation (Source & Generation vs Generated Test Cases)
 * - Traceability Chain Inspection & Test Case Approval
 */

import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Sparkles,
  Layers,
  Globe,
  Sliders,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  RefreshCw,
  Play,
  ArrowRight,
  ShieldCheck,
  Zap,
  Clock,
  Radio,
  FileSpreadsheet,
  Cpu,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { useProject } from '../../context/ProjectContext';
import { DocumentUploadSection } from '../test-generator/DocumentUploadSection';
import { ApplicationSourceSection } from '../test-generator/ApplicationSourceSection';
import { GenerationConfigSection, TEST_CATEGORIES } from '../test-generator/GenerationConfigSection';
import { GeneratedTestCasesView } from '../test-generator/GeneratedTestCasesView';
import {
  UploadedRequirementDoc,
  AppSourceType,
  AppSourceConnection,
  TestGenerationCategory,
  GenerationExecutionState,
  GeneratedTestCase,
} from '../../types';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { CreateTestCaseRequest, TestCaseHandoffRequest, TestCaseRow } from '../../types/contract';

/** AUD-005 — maps a real, persisted `TestCaseRow` (backend) onto the
 *  existing `GeneratedTestCase` shape `GeneratedTestCasesView` already
 *  renders, so that component's real, product-owner-designed layout is
 *  reused rather than duplicated. The real backend `id`/`status` are used
 *  as-is (never a fabricated local id) — this is a field-name/shape
 *  translation only, not a second source of truth. */
function testCaseRowToDisplay(row: TestCaseRow): GeneratedTestCase {
  const statusMap: Record<TestCaseRow['status'], GeneratedTestCase['status']> = {
    GENERATED: 'PENDING_REVIEW',
    VALIDATED: 'VALIDATED',
    APPROVED: 'APPROVED',
    REJECTED: 'REJECTED',
    EXECUTED: 'EXECUTED',
  };
  return {
    id: row.id,
    title: row.title,
    description: row.description,
    category: row.category as TestGenerationCategory,
    priority: row.priority as GeneratedTestCase['priority'],
    severity: row.severity as GeneratedTestCase['severity'],
    sourceDocument: row.source_document ?? undefined,
    requirementReference: row.requirement_reference ?? undefined,
    sourceRequirement: row.source_requirement ?? undefined,
    traceability: (row.traceability as GeneratedTestCase['traceability']) ?? undefined,
    preconditions: row.preconditions,
    testData: row.test_data ?? undefined,
    steps: row.steps.map((s) => ({ stepNumber: s.step_number, action: s.action, expectedResult: s.expected_result })),
    expectedResult: row.expected_result,
    assertions: row.assertions,
    riskFactor: (row.risk_factor as GeneratedTestCase['riskFactor']) ?? 'MEDIUM',
    status: statusMap[row.status],
    suggestedTags: row.tags,
    generatedAt: row.created_at,
  };
}

function generatedCaseToCreateRequest(tc: GeneratedTestCase): CreateTestCaseRequest {
  return {
    title: tc.title,
    description: tc.description,
    category: tc.category,
    priority: tc.priority,
    severity: tc.severity,
    preconditions: tc.preconditions,
    steps: tc.steps.map((s) => ({ step_number: s.stepNumber, action: s.action, expected_result: s.expectedResult })),
    expected_result: tc.expectedResult,
    assertions: tc.assertions,
    test_data: tc.testData,
    source_document: tc.sourceDocument,
    requirement_reference: tc.requirementReference,
    source_requirement: tc.sourceRequirement,
    traceability: tc.traceability,
    risk_factor: tc.riskFactor,
    tags: tc.suggestedTags,
  };
}

// FE-4: the one, minimal, non-visual routing change made to this
// product-owner-designed page — see `handleOpenExecutionHub` below.
//
// FE-5: every generation call in this page is entirely local/simulated
// (`handleTriggerGenerate`'s own `setTimeout` pipeline below) — there is no
// real `aiqaApi` call anywhere in this file, so brief §12's "project-
// dependent actions must use the selected project when the real backend
// contract requires it" currently has nothing to wire (no such real,
// project-scoped action exists yet on this page). `realProjectId` is
// exposed for exactly that future wiring and, in the meantime, used as a
// remount key so an in-progress local generation session is not silently
// carried over to a different real project scope.
export const TestCaseGeneratorPage: React.FC = () => {
  const {
    activeGenerationSession,
    setActiveGenerationSession,
    addActivity,
  } = useAIQA();
  const { selectedProjectId: realProjectId } = useProject();
  const navigate = useNavigate();

  // Navigation Branch Tab ('SOURCES_AND_GENERATION' | 'GENERATED_CASES')
  const [activeBranch, setActiveBranch] = useState<'SOURCES_AND_GENERATION' | 'GENERATED_CASES'>(
    activeGenerationSession?.generatedCases && activeGenerationSession.generatedCases.length > 0
      ? 'GENERATED_CASES'
      : 'SOURCES_AND_GENERATION'
  );

  // 1. Uploaded Documents State (Starts empty according to No Fake Data rule)
  const [documents, setDocuments] = useState<UploadedRequirementDoc[]>(
    activeGenerationSession?.documents || []
  );

  // 2. Application Sources State
  const [appSources, setAppSources] = useState<Record<AppSourceType, AppSourceConnection>>(
    activeGenerationSession?.appSources || {
      STAGING: { type: 'STAGING', label: 'Staging', url: '', status: 'NOT_CONNECTED' },
      DEMO: { type: 'DEMO', label: 'Demo', url: '', status: 'NOT_CONNECTED' },
      LIVE: { type: 'LIVE', label: 'Live', url: '', status: 'NOT_CONNECTED' },
      FIGMA: { type: 'FIGMA', label: 'Figma', url: '', status: 'NOT_CONNECTED' },
    }
  );

  // 3. Selected Test Categories (Defaults to all 14 or customizable)
  const [selectedCategories, setSelectedCategories] = useState<TestGenerationCategory[]>(
    activeGenerationSession?.selectedCategories || [
      'Functional',
      'Non-Functional',
      'Positive',
      'Negative',
      'UI/UX',
      'Edge Case',
      'Business Logic',
      'API',
      'Security',
    ]
  );

  // 4. Requirement Prompt / Notes
  const [requirementNotes, setRequirementNotes] = useState<string>(
    activeGenerationSession?.requirementText || ''
  );

  // 5. Generation depth (local template-selection preference — FE-16: the
  // former 'model' provider selector was removed here; it never had any
  // real effect on generation, see the FE-16 report).
  const [depth, setDepth] = useState<'SMOKE' | 'STANDARD' | 'EXHAUSTIVE' | 'CHAOS_EXPLORATORY'>(
    activeGenerationSession?.depth || 'STANDARD'
  );

  // 6. Generation State Machine: IDLE | VALIDATING | PROCESSING | GENERATING | COMPLETED | FAILED | PARTIAL
  const [generationState, setGenerationState] = useState<GenerationExecutionState>(
    activeGenerationSession?.generationState || 'IDLE'
  );

  // 7. Generated Test Cases list (local, unsaved draft — unchanged since
  // FE-16: a fixed-template preview a user can locally triage before
  // deciding whether to persist it for real).
  const [generatedCases, setGeneratedCases] = useState<GeneratedTestCase[]>(
    activeGenerationSession?.generatedCases || []
  );

  // 8. AUD-005 — the real, backend-persisted test cases for the selected
  // project. Once any exist, they are what's actually shown/actioned in
  // the "Generated Test Cases" branch (real id, real lifecycle,
  // survives refresh/project-switch) — the local draft above is only
  // shown before anything has been saved yet.
  const [persistedCases, setPersistedCases] = useState<TestCaseRow[]>([]);
  const [casesLoading, setCasesLoading] = useState(false);
  const [casesError, setCasesError] = useState<ApiError | null>(null);
  const [saveState, setSaveState] = useState<'IDLE' | 'SAVING' | 'ERROR'>('IDLE');
  const [saveErrorMessage, setSaveErrorMessage] = useState<string | null>(null);
  const [handoffError, setHandoffError] = useState<string | null>(null);

  // Real fetch of this project's persisted test cases — same abort-on-
  // switch pattern every other real-data page in this app already uses
  // (`DashboardPage`/`ReportsPage`/`EvidencePage`), so a project switch
  // never applies a stale response to the new project's view.
  useEffect(() => {
    if (!realProjectId) {
      setPersistedCases([]);
      setCasesError(null);
      return;
    }
    const controller = new AbortController();
    setCasesLoading(true);
    setCasesError(null);
    aiqaApi.testCases
      .list(realProjectId, {}, controller.signal)
      .then((rows) => {
        if (controller.signal.aborted) return;
        setPersistedCases(rows);
      })
      .catch((err) => {
        if (controller.signal.aborted) return;
        setCasesError(err instanceof ApiError ? err : new ApiError('offline', 'Failed to load test cases.'));
      })
      .finally(() => {
        if (!controller.signal.aborted) setCasesLoading(false);
      });
    return () => controller.abort();
  }, [realProjectId]);

  const refetchPersistedCases = async () => {
    if (!realProjectId) return;
    try {
      const rows = await aiqaApi.testCases.list(realProjectId);
      setPersistedCases(rows);
    } catch {
      // A failed background refresh is surfaced the next time the effect
      // above re-runs (project switch) rather than silently retried here;
      // the just-completed action's own real result already reflects
      // reality even if this particular re-list call fails.
    }
  };

  /** Persists every currently-generated local draft as a real test case.
   *  Honest partial-failure reporting: if N of M succeed, that is exactly
   *  what is reported — never a blanket false "saved" claim. */
  const handleSaveToProject = async () => {
    if (!realProjectId) {
      setSaveState('ERROR');
      setSaveErrorMessage('Select a project before saving test cases.');
      return;
    }
    setSaveState('SAVING');
    setSaveErrorMessage(null);
    let succeeded = 0;
    const failures: string[] = [];
    for (const tc of generatedCases) {
      try {
        await aiqaApi.testCases.create(realProjectId, generatedCaseToCreateRequest(tc));
        succeeded += 1;
      } catch (err) {
        failures.push(err instanceof ApiError ? `${tc.title}: ${err.message}` : `${tc.title}: failed to save`);
      }
    }
    await refetchPersistedCases();
    if (failures.length === 0) {
      setSaveState('IDLE');
      setGeneratedCases([]);
      addActivity({
        type: 'RUN_STARTED',
        title: `${succeeded} Test Case${succeeded === 1 ? '' : 's'} Saved to Project`,
        description: `${succeeded} generated test case${succeeded === 1 ? '' : 's'} persisted to the backend and are now real, project-scoped records.`,
      });
    } else {
      setSaveState('ERROR');
      setSaveErrorMessage(`Saved ${succeeded} of ${generatedCases.length} cases. Failures: ${failures.join('; ')}`);
    }
  };

  const handleValidateReal = async (id: string) => {
    if (!realProjectId) return;
    try {
      await aiqaApi.testCases.validate(realProjectId, id);
      await refetchPersistedCases();
    } catch (err) {
      setSaveErrorMessage(err instanceof ApiError ? err.message : 'Validation failed.');
    }
  };

  const handleApproveReal = async (id: string) => {
    if (!realProjectId) return;
    try {
      await aiqaApi.testCases.approve(realProjectId, id);
      await refetchPersistedCases();
    } catch (err) {
      setSaveErrorMessage(err instanceof ApiError ? err.message : 'Approval failed.');
    }
  };

  const handleRejectReal = async (id: string) => {
    if (!realProjectId) return;
    try {
      await aiqaApi.testCases.reject(realProjectId, id);
      await refetchPersistedCases();
    } catch (err) {
      setSaveErrorMessage(err instanceof ApiError ? err.message : 'Reject failed.');
    }
  };

  /** The real bridge to the real Execution Hub (FRD-EXE-11). `outcome` is a
   *  required, explicit, user-visible choice — see
   *  `TestCaseHandoffRequest`'s own doc comment for why there is no
   *  default that would silently fabricate a "passed" result. */
  const handleHandoffReal = async (id: string, outcome: 'pass' | 'fail' | 'timeout' | 'infra_error') => {
    if (!realProjectId) return;
    setHandoffError(null);
    const body: TestCaseHandoffRequest = { simulate: { outcome, delay_ms: 150 } };
    try {
      const created = await aiqaApi.testCases.handoff(realProjectId, id, body);
      await refetchPersistedCases();
      navigate(`/execution?run=${created.run_id}`);
    } catch (err) {
      setHandoffError(err instanceof ApiError ? err.message : 'Handoff failed.');
    }
  };

  // Helper for Header Status Indicator
  const getHeaderStatusIndicator = () => {
    switch (generationState) {
      case 'VALIDATING':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-cyan-950/80 border border-cyan-700/80 text-cyan-300">
            <RefreshCw className="w-3.5 h-3.5 text-cyan-400 animate-spin" />
            Validating Sources...
          </span>
        );
      case 'PROCESSING':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-indigo-950/80 border border-indigo-700/80 text-indigo-300">
            <Cpu className="w-3.5 h-3.5 text-indigo-400 animate-pulse" />
            Preparing Test Templates...
          </span>
        );
      case 'GENERATING':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-purple-950/80 border border-purple-700/80 text-purple-300">
            <Sparkles className="w-3.5 h-3.5 text-purple-400 animate-bounce" />
            Generating Test Cases...
          </span>
        );
      case 'COMPLETED':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-emerald-950/80 border border-emerald-700/80 text-emerald-300">
            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
            Test Cases Generated
          </span>
        );
      case 'FAILED':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-rose-950/80 border border-rose-700/80 text-rose-300">
            <XCircle className="w-3.5 h-3.5 text-rose-400" />
            Generation Failed
          </span>
        );
      case 'PARTIAL':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-amber-950/80 border border-amber-700/80 text-amber-300">
            <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
            Generation Completed with Warnings
          </span>
        );
      case 'IDLE':
      default:
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-semibold bg-slate-900 border border-slate-700 text-cyan-400">
            <span className="w-2 h-2 rounded-full bg-cyan-400 animate-pulse" />
            Ready to Generate
          </span>
        );
    }
  };

  // Toggle Category
  const handleToggleCategory = (cat: TestGenerationCategory) => {
    setSelectedCategories((prev) =>
      prev.includes(cat) ? prev.filter((c) => c !== cat) : [...prev, cat]
    );
  };

  const handleSelectAllCategories = () => {
    setSelectedCategories(TEST_CATEGORIES.map((c) => c.id));
  };

  const handleDeselectAllCategories = () => {
    setSelectedCategories([]);
  };

  // Test Case Generation Pipeline
  const handleTriggerGenerate = () => {
    setGenerationState('VALIDATING');

    // Stage 1: Validate Sources
    setTimeout(() => {
      setGenerationState('PROCESSING');

      // Stage 2: AI Document & Application Analysis
      setTimeout(() => {
        setGenerationState('GENERATING');

        // Stage 3: Generate Authentic Traceable Test Cases across selected categories
        setTimeout(() => {
          const docLabels = documents.map((d) => d.slotId).join(' + ') || 'Product Requirements Spec';
          const targetUrl =
            appSources.STAGING?.url ||
            appSources.DEMO?.url ||
            appSources.LIVE?.url ||
            'https://app.enterprise.internal';

          // Construct traceable test cases for each selected category
          const newGenerated: GeneratedTestCase[] = selectedCategories.map((cat, idx) => {
            const num = (idx + 1).toString().padStart(3, '0');
            const tcId = `TC-${cat.toUpperCase().slice(0, 3)}-${num}`;

            let title = '';
            let objective = '';
            let description = '';
            let priority: 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW' = 'HIGH';
            let severity: 'CRITICAL' | 'MAJOR' | 'MINOR' | 'TRIVIAL' = 'MAJOR';
            let steps: Array<{ stepNumber: number; action: string; expectedResult: string }> = [];
            let assertions: string[] = [];

            switch (cat) {
              case 'Functional':
                title = 'User Authentication & Session Lifecycle Verification';
                objective = 'Validate complete login flow and credential challenge matching.';
                description = 'Verify that valid user credentials grant active bearer session token and route to dashboard.';
                priority = 'CRITICAL';
                severity = 'CRITICAL';
                steps = [
                  { stepNumber: 1, action: `Navigate to ${targetUrl}/auth/login`, expectedResult: 'Login form rendered with CSRF token' },
                  { stepNumber: 2, action: 'Submit valid enterprise user credentials', expectedResult: 'HTTP 200 OK and redirection to /dashboard' },
                ];
                assertions = ['HTTP status is 200', 'Auth token stored in secure cookie with HttpOnly flag'];
                break;

              case 'Non-Functional':
                title = 'High-Availability Service Resilience on Redis Ingress Drop';
                objective = 'Verify graceful service fallback when cache nodes fail.';
                description = 'Simulate transient cache failure and verify read fallback to database replica.';
                priority = 'HIGH';
                severity = 'MAJOR';
                steps = [
                  { stepNumber: 1, action: 'Trigger 100 concurrent requests with simulated cache disconnect', expectedResult: 'Requests routed directly to PostgreSQL replica' },
                  { stepNumber: 2, action: 'Observe P99 latency SLA', expectedResult: 'P99 latency remains under 450ms SLA' },
                ];
                assertions = ['Zero 500 Internal Server Errors returned', 'Circuit breaker transitions to HALF_OPEN'];
                break;

              case 'Positive':
                title = 'Standard Checkout & Payment Authorization Workflow';
                objective = 'Ensure standard checkout with valid payment method succeeds.';
                description = 'Execute full happy-path payment flow using verified test credit card.';
                priority = 'CRITICAL';
                severity = 'CRITICAL';
                steps = [
                  { stepNumber: 1, action: 'Add subscription item to cart and initiate checkout', expectedResult: 'Order summary calculated with accurate tax' },
                  { stepNumber: 2, action: 'Authorize payment with 3D Secure verification', expectedResult: 'Payment confirmed and receipt generated' },
                ];
                assertions = ['Order status set to PAID', 'Webhook event order.created dispatched'];
                break;

              case 'Negative':
                title = 'Malformed Payload Rejection & Schema Validation';
                objective = 'Verify API strictly rejects malformed JSON payloads.';
                description = 'Submit invalid schema attributes and verify 400 Bad Request response with structured error messages.';
                priority = 'HIGH';
                severity = 'MAJOR';
                steps = [
                  { stepNumber: 1, action: 'Dispatch POST /api/v2/mutate with negative integer for quantity', expectedResult: 'HTTP 422 Unprocessable Entity' },
                ];
                assertions = ['HTTP status 422 returned', 'Error details specify invalid field parameter'];
                break;

              case 'UI/UX':
                title = 'Responsive Design & WCAG AA Contrast Compliance';
                objective = 'Validate responsive layout across mobile and desktop viewports.';
                description = 'Inspect visual hierarchy, focus rings, and contrast ratios on dark mode canvas.';
                priority = 'MEDIUM';
                severity = 'MINOR';
                steps = [
                  { stepNumber: 1, action: 'Resize browser viewport to 375x812 (Mobile Viewport)', expectedResult: 'Collapsible navigation drawer rendered without horizontal scroll' },
                  { stepNumber: 2, action: 'Execute axe-core accessibility audit', expectedResult: 'Zero WCAG AA contrast violations detected' },
                ];
                assertions = ['No horizontal scrollbar rendered', 'Axe-core violation count is 0'];
                break;

              case 'Spike':
                title = 'Sudden Traffic Surge & Horizontal Pod Autoscaler Trigger';
                objective = 'Validate cluster behavior during instantaneous 10x traffic spike.';
                description = 'Inject 5,000 req/sec instantaneous load to verify HPA pod autoscaling.';
                priority = 'HIGH';
                severity = 'MAJOR';
                steps = [
                  { stepNumber: 1, action: 'Ramp traffic from 100 to 5,000 RPS in 5 seconds', expectedResult: 'Ingress controller queues excess connections' },
                  { stepNumber: 2, action: 'Observe Kubernetes HPA replica count', expectedResult: 'Replicas scale from 2 to 12 within 45 seconds' },
                ];
                assertions = ['Overall error rate remains < 0.5%', 'CPU utilization stabilized below 80%'];
                break;

              case 'Load':
                title = 'Sustained Concurrency & Database Connection Pool Saturation';
                objective = 'Ensure connection pool handles sustained 2,000 concurrent active users.';
                description = 'Hold steady load for 10 minutes and measure connection pool queue latency.';
                priority = 'HIGH';
                severity = 'MAJOR';
                steps = [
                  { stepNumber: 1, action: 'Sustain 2,000 virtual users querying search API for 10 minutes', expectedResult: 'Database pool size remains bounded without leaks' },
                ];
                assertions = ['Zero database connection timeout errors', 'Average response time < 120ms'];
                break;

              case 'Edge Case':
                title = 'Leap Year & Timezone Transition Boundary Handling';
                objective = 'Verify recurring billing calculation across daylight saving boundary.';
                description = 'Simulate billing renewal execution at 23:59:59 on February 29th across UTC+14 timezone.';
                priority = 'MEDIUM';
                severity = 'MINOR';
                steps = [
                  { stepNumber: 1, action: 'Schedule invoice calculation for Feb 29 UTC+14', expectedResult: 'Invoice period calculated as exactly 29 days' },
                ];
                assertions = ['No off-by-one day calculation errors in billing cycle'];
                break;

              case 'Senior Base':
                title = 'Distributed Transaction Two-Phase Commit Rollback Isolation';
                objective = 'Verify multi-service saga pattern cleanly rolls back on step 3 failure.';
                description = 'Induce fault in inventory service after payment reservation to verify compensating saga transactions.';
                priority = 'CRITICAL';
                severity = 'CRITICAL';
                steps = [
                  { stepNumber: 1, action: 'Initiate multi-item order checkout', expectedResult: 'Payment token reserved' },
                  { stepNumber: 2, action: 'Inject synthetic 500 error in Inventory Reservation service', expectedResult: 'Compensating payment refund triggered immediately' },
                ];
                assertions = ['Ledger balance perfectly restored to original value', 'Saga state transitioned to COMPENSATED'];
                break;

              case 'Business Logic':
                title = 'Tiered Discount Rule & Volume Tier Boundary Verification';
                objective = 'Ensure enterprise tier discounts apply strictly when license count >= 50.';
                description = 'Verify 20% volume discount at 50 licenses and standard pricing at 49 licenses.';
                priority = 'HIGH';
                severity = 'MAJOR';
                steps = [
                  { stepNumber: 1, action: 'Set seat count = 49 in pricing calculator', expectedResult: 'Discount = 0%, Total = $4,900' },
                  { stepNumber: 2, action: 'Set seat count = 50 in pricing calculator', expectedResult: 'Discount = 20%, Total = $4,000' },
                ];
                assertions = ['Correct tier logic applied at exact boundary integers'];
                break;

              case 'API':
                title = 'REST API Contract Validation & RFC 7807 Problem Details';
                objective = 'Verify OpenAPI v3 schema compliance and response headers.';
                description = 'Ensure GET /api/v2/resources conforms to OpenAPI 3.1 specification schema.';
                priority = 'HIGH';
                severity = 'MAJOR';
                steps = [
                  { stepNumber: 1, action: 'Dispatch GET /api/v2/resources with Accept: application/json', expectedResult: 'HTTP 200 with matching schema types' },
                ];
                assertions = ['Response JSON matches OpenAPI definition', 'Header content-type is application/json'];
                break;

              case 'Performance':
                title = 'DOM First Contentful Paint & Core Web Vitals Audit';
                objective = 'Verify FCP < 1.2s and LCP < 2.5s under 4G throttling.';
                description = 'Measure Core Web Vitals on cold cache browser session with simulated 4G connection.';
                priority = 'MEDIUM';
                severity = 'MINOR';
                steps = [
                  { stepNumber: 1, action: 'Load landing page with simulated Fast 4G network throttle', expectedResult: 'FCP measured at < 1.1s, LCP < 2.2s' },
                ];
                assertions = ['Lighthouse Performance score > 90', 'CLS score < 0.05'];
                break;

              case 'Security':
                title = 'OAuth 2.0 PKCE Code Verifier Hash Validation';
                objective = 'Ensure authorization server rejects mismatched code_verifier.';
                description = 'Attempt auth code token exchange with malformed SHA-256 code_verifier string.';
                priority = 'CRITICAL';
                severity = 'CRITICAL';
                steps = [
                  { stepNumber: 1, action: 'Initiate OAuth authorize with S256 code_challenge', expectedResult: 'HTTP 302 with auth code' },
                  { stepNumber: 2, action: 'Submit token exchange with mismatched code_verifier', expectedResult: 'HTTP 400 invalid_grant' },
                ];
                assertions = ['HTTP 400 returned', 'Security alert logged with IP address'];
                break;

              case 'Penetration':
                title = 'Server-Side Request Forgery (SSRF) Filter Bypass in Webhooks';
                objective = 'Verify webhook endpoint blocks loopback and cloud metadata endpoints.';
                description = 'Attempt webhook registration targeting 169.254.169.254 and localhost:8080.';
                priority = 'CRITICAL';
                severity = 'CRITICAL';
                steps = [
                  { stepNumber: 1, action: 'POST /api/v2/webhooks with url: http://169.254.169.254/latest/meta-data/', expectedResult: 'HTTP 400 with SSRF_RESTRICTED_IP' },
                  { stepNumber: 2, action: 'POST /api/v2/webhooks with url: http://localhost:8080', expectedResult: 'HTTP 400 with SSRF_RESTRICTED_IP' },
                ];
                assertions = ['Requests to private and cloud metadata IPs blocked', 'Audit event emitted'];
                break;
            }

            return {
              id: tcId,
              title,
              objective,
              description,
              category: cat,
              priority,
              severity,
              sourceDocument: docLabels,
              requirementReference: `${documents[0]?.slotId || 'PRD'} §${idx + 1}.${idx + 2} Acceptance Criteria`,
              sourceRequirement:
                requirementNotes.trim() ||
                `System must strictly conform to ${cat} quality requirements outlined in ${docLabels}.`,
              traceability: {
                sourceDoc: documents[0]?.fileName || `${docLabels} Specification`,
                requirementRef: `${docLabels} §${idx + 1}.${idx + 2}`,
                requirementText: title,
                // FE-16: was `Synthesized via ${model} (${depth} depth)` — a
                // false claim that a named AI model performed synthesis.
                // This is a fixed local template, and it must say so.
                aiAnalysis: 'Locally assembled from a fixed category template — no AI model was called.',
                testCaseId: tcId,
                testCategory: cat,
                executionTarget: 'k8s-runner-pool-us',
                evidencePolicy: 'HAR + Playwright Trace + Audit Log',
                reportMapping: 'SOC2 / ISO 27001 Compliance Matrix',
              },
              preconditions: [
                'Target application accessible and healthy at specified endpoint',
                'Runner agent authenticated with scoped credentials',
              ],
              testData: JSON.stringify({ category: cat, target: targetUrl, depth }, null, 2),
              steps,
              expectedResult: assertions.join('; '),
              assertions,
              riskFactor: priority === 'CRITICAL' ? 'HIGH' : priority === 'HIGH' ? 'MEDIUM' : 'LOW',
              status: 'PENDING_REVIEW',
              suggestedTags: [cat.toLowerCase().replace(/\s+/g, '-'), 'template-generated', 'traceable'],
              generatedAt: new Date().toISOString(),
            };
          });

          setGeneratedCases(newGenerated);
          setGenerationState('COMPLETED');
          setActiveBranch('GENERATED_CASES');

          // Update Context
          if (setActiveGenerationSession) {
            setActiveGenerationSession({
              id: `gen-${Math.floor(1000 + Math.random() * 9000)}`,
              requirementText: requirementNotes,
              specType: 'NATURAL_LANGUAGE',
              targetAppUrl: targetUrl,
              currentStep: 'GENERATED_CASES',
              generationState: 'COMPLETED',
              selectedCategories,
              documents,
              appSources,
              depth,
              generatedCases: newGenerated,
              createdAt: new Date().toISOString(),
            });
          }

          addActivity({
            type: 'RUN_STARTED',
            title: `Test Case Generation Completed (${newGenerated.length} Cases)`,
            description: `Locally assembled ${newGenerated.length} test cases across ${selectedCategories.length} categories from fixed category templates for ${docLabels}.`,
          });
        }, 1800);
      }, 1400);
    }, 1000);
  };

  // AUD-005 — once any real, persisted cases exist for this project, the
  // Generated Test Cases branch displays and actions those, not the local
  // draft. Approve/Reject on a real case calls the real backend lifecycle
  // endpoint; on a still-local draft (before "Save to Project") it stays
  // exactly the lightweight local triage it always was (FE-16, unchanged).
  const hasPersisted = persistedCases.length > 0;
  const displayedCases: GeneratedTestCase[] = hasPersisted ? persistedCases.map(testCaseRowToDisplay) : generatedCases;

  const handleApproveTestCase = (id: string) => {
    if (hasPersisted) {
      void handleApproveReal(id);
      return;
    }
    setGeneratedCases((prev) => prev.map((tc) => (tc.id === id ? { ...tc, status: 'APPROVED' } : tc)));
  };

  const handleRejectTestCase = (id: string) => {
    if (hasPersisted) {
      void handleRejectReal(id);
      return;
    }
    setGeneratedCases((prev) => prev.map((tc) => (tc.id === id ? { ...tc, status: 'REJECTED' } : tc)));
  };

  const handleApproveAll = () => {
    if (hasPersisted) {
      // Real approval requires each case to already be VALIDATED
      // (FRD-EXE-11/FRD-AGT-04) — this runs the real validate-then-approve
      // sequence for every still-GENERATED case, one real API call pair
      // per case, never a fabricated bulk-approve that skips the gate.
      void (async () => {
        for (const row of persistedCases) {
          if (row.status !== 'GENERATED') continue;
          try {
            await aiqaApi.testCases.validate(realProjectId!, row.id);
            await aiqaApi.testCases.approve(realProjectId!, row.id);
          } catch (err) {
            setSaveErrorMessage(err instanceof ApiError ? err.message : `Failed to approve ${row.title}.`);
          }
        }
        await refetchPersistedCases();
      })();
      return;
    }
    setGeneratedCases((prev) =>
      prev.map((tc) => (tc.status === 'PENDING_REVIEW' ? { ...tc, status: 'APPROVED' } : tc))
    );
  };

  // FE-16: this used to call `AIQAContext.triggerNewRun`, which fabricated a
  // complete local `ExecutionRun` (a random run id, hardcoded fake
  // pass/running/queued counts, and two fake test names unrelated to the
  // categories actually generated) — an object the real Execution Hub
  // (which reads only `aiqaApi.executions.*`) never consumed. No real
  // `GeneratedTestCase -> TaskDef` mapping exists (FE-15 GAP-02), so there is
  // no real handoff to perform. This now does exactly what it truthfully
  // can: navigate to the real Execution Hub, with no claim that the
  // generated cases were transferred, exported, or turned into a real
  // execution — see `GeneratedTestCasesView.tsx`'s "Open Execution Hub"
  // button, which no longer implies dispatch/export.
  const handleOpenExecutionHub = () => {
    navigate('/execution');
  };

  return (
    <div
      id="aiqa-test-case-generator-page"
      key={realProjectId ?? 'no-real-project-selected'}
      className="space-y-4 animate-in fade-in duration-200 text-slate-200"
    >
      {/* 1. Page Header */}
      <header
        id="test-generator-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="p-1.5 rounded bg-indigo-950/80 border border-indigo-700/60 text-indigo-400">
              <Sparkles className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">
              Test Case Generator
            </h1>
            <span className="px-2 py-0.5 rounded bg-cyan-950/60 border border-cyan-800/60 text-[10px] font-mono text-cyan-300 uppercase font-semibold">
              LOCAL TEMPLATE GENERATION
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            Generate intelligent, traceable test cases from your product requirements and application.
          </p>
        </div>

        {/* Small Status Indicator */}
        <div className="flex items-center gap-3 self-start md:self-center">
          {getHeaderStatusIndicator()}
        </div>
      </header>

      {/* Dual-Branch Navigation Ribbon (Source & Generation vs Generated Test Cases) */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-1">
        <div className="flex items-center gap-2">
          {/* Branch 1: Source & Generation */}
          <button
            type="button"
            onClick={() => setActiveBranch('SOURCES_AND_GENERATION')}
            className={`flex items-center gap-2 px-3.5 py-2 rounded-t-lg text-xs font-mono font-bold transition-colors cursor-pointer border-b-2 ${
              activeBranch === 'SOURCES_AND_GENERATION'
                ? 'border-indigo-500 text-indigo-300 bg-[#0B1120]/80'
                : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-900/40'
            }`}
          >
            <Layers className="w-3.5 h-3.5" />
            <span>Source & Generation</span>
            {documents.length > 0 && (
              <span className="text-[10px] px-1.5 py-0.2 rounded bg-cyan-950 text-cyan-300 border border-cyan-800">
                {documents.length} Docs
              </span>
            )}
          </button>

          {/* Branch 2: Generated Test Cases */}
          <button
            type="button"
            onClick={() => setActiveBranch('GENERATED_CASES')}
            className={`flex items-center gap-2 px-3.5 py-2 rounded-t-lg text-xs font-mono font-bold transition-colors cursor-pointer border-b-2 ${
              activeBranch === 'GENERATED_CASES'
                ? 'border-indigo-500 text-indigo-300 bg-[#0B1120]/80'
                : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-900/40'
            }`}
          >
            <Sparkles className="w-3.5 h-3.5" />
            <span>Generated Test Cases</span>
            {generatedCases.length > 0 && (
              <span className="text-[10px] px-1.5 py-0.2 rounded bg-emerald-950 text-emerald-300 border border-emerald-800 font-bold">
                {generatedCases.length}
              </span>
            )}
          </button>
        </div>

        {/* Quick Shortcut to other branch */}
        {activeBranch === 'SOURCES_AND_GENERATION' && generatedCases.length > 0 && (
          <button
            type="button"
            onClick={() => setActiveBranch('GENERATED_CASES')}
            className="flex items-center gap-1 text-xs font-mono text-cyan-400 hover:text-cyan-300 font-semibold cursor-pointer pb-1"
          >
            <span>View {generatedCases.length} Generated Cases</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        )}

        {activeBranch === 'GENERATED_CASES' && (
          <button
            type="button"
            onClick={() => setActiveBranch('SOURCES_AND_GENERATION')}
            className="flex items-center gap-1 text-xs font-mono text-indigo-400 hover:text-indigo-300 font-semibold cursor-pointer pb-1"
          >
            <span>Edit Sources & Configuration</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {/* Main Content Body */}
      {activeBranch === 'SOURCES_AND_GENERATION' ? (
        /* Branch 1: Source & Generation Workspace */
        <div id="project-sources-workspace" className="space-y-6">
          {/* Section Banner: Project Sources */}
          <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-1">
            <div className="flex items-center gap-2">
              <span className="text-[10px] font-mono text-cyan-400 uppercase font-bold bg-cyan-950/60 px-1.5 py-0.5 rounded border border-cyan-800/60">
                SOURCE WORKSPACE
              </span>
              <h2 className="text-sm font-bold text-slate-100 font-sans">
                Project Sources
              </h2>
            </div>
            <p className="text-xs text-slate-400 font-sans">
              Upload product documentation and connect application sources before generating test cases.
            </p>
          </div>

          {/* 1. Document Upload Section (5 slots) */}
          <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
            <DocumentUploadSection
              documents={documents}
              onDocumentsChange={setDocuments}
              disabled={['VALIDATING', 'PROCESSING', 'GENERATING'].includes(generationState)}
            />
          </div>

          {/* 2. Application Source Section (4 channels) */}
          <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
            <ApplicationSourceSection
              appSources={appSources}
              onAppSourcesChange={setAppSources}
              disabled={['VALIDATING', 'PROCESSING', 'GENERATING'].includes(generationState)}
            />
          </div>

          {/* 3. Test Generation Configuration (14 Categories & CTA) */}
          <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
            <GenerationConfigSection
              selectedCategories={selectedCategories}
              onToggleCategory={handleToggleCategory}
              onSelectAllCategories={handleSelectAllCategories}
              onDeselectAllCategories={handleDeselectAllCategories}
              requirementNotes={requirementNotes}
              onRequirementNotesChange={setRequirementNotes}
              depth={depth}
              onDepthChange={setDepth}
              generationState={generationState}
              onTriggerGenerate={handleTriggerGenerate}
              documents={documents}
              appSources={appSources}
            />
          </div>
        </div>
      ) : (
        /* Branch 2: Generated Test Cases Workspace */
        <GeneratedTestCasesView
          testCases={displayedCases}
          onApproveTestCase={handleApproveTestCase}
          onRejectTestCase={handleRejectTestCase}
          onApproveAll={handleApproveAll}
          onOpenExecutionHub={handleOpenExecutionHub}
          onNavigateToSources={() => setActiveBranch('SOURCES_AND_GENERATION')}
          hasPersisted={hasPersisted}
          canSave={!hasPersisted && generatedCases.length > 0}
          saveState={saveState}
          saveErrorMessage={saveErrorMessage}
          onSaveToProject={handleSaveToProject}
          casesLoading={casesLoading}
          casesError={casesError?.message ?? null}
          onValidateTestCase={handleValidateReal}
          onHandoffTestCase={handleHandoffReal}
          handoffError={handoffError}
        />
      )}
    </div>
  );
};
