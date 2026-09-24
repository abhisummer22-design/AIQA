/**
 * AIQA Test Case Generator Page — Main Content Area
 * Futuristic AI Quality Engineering Test Generation Control Plane
 * Consumes:
 * - 5 Document Slots (BRD, PRD, FRD, SRD, Additional Document)
 * - 4 Application Sources (Staging, Demo, Live, Figma)
 * - 14 Exact Test Categories
 * - Dual-branch navigation (Source & Generation vs Generated Test Cases)
 * - Traceability Chain Inspection & Test Case Approval
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
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
import { CrawlGroundingSection } from '../test-generator/CrawlGroundingSection';
import { RequirementCoverageSummary } from '../test-generator/RequirementCoverageSummary';
import { RequirementGenerationSection } from '../test-generator/RequirementGenerationSection';
import { GenerationConfigSection, TEST_CATEGORIES } from '../test-generator/GenerationConfigSection';
import { GeneratedTestCasesView } from '../test-generator/GeneratedTestCasesView';
import { GenerationWaveResults } from '../test-generator/GenerationWaveResults';
import { SkillsAndMcpPanel } from '../test-generator/SkillsAndMcpPanel';
import { TestGenerationFlow } from '../test-generator/TestGenerationFlow';
import { useSkillSelection } from '../../hooks/useSkillSelection';
import { useProviderNames } from '../../hooks/useProviderNames';
import { categoryMatchedSkillIds, DEFAULT_SELECTED_SKILL_IDS } from '../../services/skillRegistry';
import {
  UploadedRequirementDoc,
  AppSourceType,
  AppSourceConnection,
  TestGenerationCategory,
  GenerationExecutionState,
  GeneratedTestCase,
} from '../../types';
import { ApiError, aiqaApi, orchestratorLogsWsUrl } from '../../services/aiqaApi';
import type { EditTestCaseUpdates } from '../test-generator/EditTestCaseModal';
import type {
  CreateTestCaseRequest,
  TestCaseHandoffRequest,
  TestCaseRow,
  GenerateAiSkillInput,
  GenerateAiTestCasesResponse,
  GeneratedTestCaseDraft,
  AiRoutingMode,
  CompilationOutcome,
  OrchestratorLogEntry,
  RequirementRecord,
} from '../../types/contract';

/** The default test-category selection — a fresh generation for a project
 *  starts here, same as it always has; pulled out to a constant only so the
 *  project-switch reset below can put it back exactly, rather than
 *  duplicating the literal a second time. */
const DEFAULT_SELECTED_CATEGORIES: TestGenerationCategory[] = [
  'Functional',
  'Non-Functional',
  'Positive',
  'Negative',
  'UI/UX',
  'Edge Case',
  'Business Logic',
  'API',
  'Security',
];

/** The default (nothing connected) Application Sources state — a function,
 *  not a shared constant object, so every caller gets its own fresh object
 *  no `setAppSources` call could ever end up mutating a copy someone else
 *  is still holding. */
function defaultAppSources(): Record<AppSourceType, AppSourceConnection> {
  return {
    STAGING: { type: 'STAGING', label: 'Staging', url: '', status: 'IDLE' },
    DEMO: { type: 'DEMO', label: 'Demo', url: '', status: 'IDLE' },
    LIVE: { type: 'LIVE', label: 'Live', url: '', status: 'IDLE' },
    FIGMA: { type: 'FIGMA', label: 'Figma', url: '', status: 'IDLE' },
  };
}

/** Parses a real `generate_ai` wave-start log message
 *  (`ai_generation.rs::run_wave`'s own `"Wave '{wave}' ({skills}) started"`
 *  format — the exact same message shape its "generated N case(s)"/"failed"
 *  siblings already use, distinguished only by ending in "started") into the
 *  real skill names that wave is about to run. Returns `null` for anything
 *  that isn't genuinely this one message shape — including this project's
 *  other real log traffic — so an unrelated log entry never lights up a
 *  skill it has nothing to do with. */
export function parseWaveStartedSkillNames(message: string): string[] | null {
  const match = /^Wave '[a-z_]+' \(([^)]*)\) started$/.exec(message);
  if (!match) return null;
  const names = match[1]
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  return names.length > 0 ? names : null;
}

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

/** Phase 2 (Execution Handoff) — the caller's real, explicit handoff
 *  choice. `executor` carries no `params`: no per-case params UI exists
 *  anywhere in this app (the Execution Hub's own manual task creation
 *  doesn't offer one either), so a real-executor handoff always runs that
 *  tool's own default reference fixture (`executor.rs`'s own
 *  `safe_param(task, key, default)` fallback) — never a fabricated
 *  translation of this case's natural-language steps into a script. */
export type HandoffChoice =
  | { mode: 'simulate'; outcome: 'pass' | 'fail' | 'timeout' | 'infra_error' }
  | { mode: 'executor'; executor: string }
  /** Phase 2.1 — sends `compile_and_execute: true` instead of a fixed
   *  `executor`: the real backend compiler (`execution_plan.rs`) reads
   *  this case's own actual steps/expected_result, deterministically or
   *  AI-assistedly compiles a structured plan from them, then runs that
   *  plan through whichever real executor is compatible — never a fixed
   *  reference fixture. See `GeneratedTestCasesView.tsx`'s own compile
   *  preview UI for the truthful READY/NOT_EXECUTABLE/... disclosure
   *  shown before this is sent. */
  | { mode: 'compile' };

// FE-4: the one, minimal, non-visual routing change made to this
// product-owner-designed page — see `handleOpenExecutionHub` below.
//
// Phase 1 (AI Test Case Generator integration): `handleTriggerGenerate`
// below now makes a real, authenticated `aiqaApi.testCases.generateAi`
// call to the real backend AI generation pipeline
// (`ai_generation.rs` -> `ai_router.rs`) — the previous local/simulated
// `setTimeout` template pipeline (FE-5) has been removed, not merely
// disclosed. `realProjectId` is required for this real call (the endpoint
// is project-scoped) and is also used as a remount key so an in-progress
// generation session is not silently carried over to a different project.
export const TestCaseGeneratorPage: React.FC = () => {
  const {
    activeGenerationSession,
    setActiveGenerationSession,
    getGenerationSession,
    setGenerationSessionFor,
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
    activeGenerationSession?.appSources || defaultAppSources()
  );

  // 2b. Real crawl grounding (Phase 6 -> Phase 8 wiring) — the real,
  // completed crawl `run_id` opted into generation, or `null`. Not part of
  // `activeGenerationSession` yet (a session-restore concern out of this
  // increment's scope) — always starts fresh, same as `documents` does.
  const [crawlRunId, setCrawlRunId] = useState<string | null>(null);

  // 3. Selected Test Categories (Defaults to all 14 or customizable)
  const [selectedCategories, setSelectedCategories] = useState<TestGenerationCategory[]>(
    activeGenerationSession?.selectedCategories || DEFAULT_SELECTED_CATEGORIES
  );

  // 4. Generation depth (local template-selection preference — FE-16: the
  // former 'model' provider selector was removed here; it never had any
  // real effect on generation, see the FE-16 report).
  const [depth, setDepth] = useState<'SMOKE' | 'STANDARD' | 'EXHAUSTIVE' | 'CHAOS_EXPLORATORY'>(
    activeGenerationSession?.depth || 'STANDARD'
  );

  // 5. Phase 3 — opt-in: generate from the numbered requirements read out of
  // the uploaded documents (`RequirementGenerationSection`) instead of
  // pasting the documents into every AI call. Off unless the user turned it
  // on, and kept in the project's own session like the rest of the setup.
  const [requirementMode, setRequirementMode] = useState<boolean>(
    activeGenerationSession?.requirementMode ?? false
  );

  // 6. Generation State Machine: IDLE | VALIDATING | PROCESSING | GENERATING | COMPLETED | FAILED | PARTIAL
  const [generationState, setGenerationState] = useState<GenerationExecutionState>(
    activeGenerationSession?.generationState || 'IDLE'
  );

  // 6b. Phase 1 — real, request-backed generation outcome/error state.
  // `lastGenerationResult` is the real, unmodified response from
  // `POST .../test-cases/generate-ai` (never fabricated); `generationError`
  // is a real, human-readable reason whenever generation did not produce
  // reviewable cases (network/auth failure, no compatible AI model, or a
  // real AI response that produced zero cases passing validation).
  // Both live in the project's session too, so a return to it — after a
  // page switch or a project switch, including a generation that finished
  // while it was off screen — still shows the real provider/model, the
  // per-skill results and the failure reason of its last attempt. A failed
  // attempt clears the response (nothing real is left to show for it).
  const [generationError, setGenerationError] = useState<string | null>(
    activeGenerationSession?.generationError ?? null
  );
  const [lastGenerationResult, setLastGenerationResult] = useState<GenerateAiTestCasesResponse | null>(
    activeGenerationSession?.lastGenerationResult ?? null
  );
  const [lastRoutingMode, setLastRoutingMode] = useState<AiRoutingMode | null>(null);
  // A provider added in Settings shows its own name, not its internal id.
  const providerName = useProviderNames(lastGenerationResult !== null);

  // Real, in-flight "which skill is running right now" signal — a
  // project-scoped WS connection listening only for the real wave-start
  // events `ai_generation.rs::run_wave` emits on the same log stream
  // `OrchestratorLogsPanel` already consumes elsewhere (see
  // `parseWaveStartedSkillNames` above for the exact, narrow message shape
  // matched — every other real log entry on this stream is ignored here).
  // Reset to empty whenever a new generation starts or the previous one
  // finishes (`handleTriggerGenerate` below) — the single
  // `POST .../generate-ai` call is synchronous end-to-end, so once it
  // resolves every wave has already really finished, and holding the glow
  // on any longer would claim a wave is still running when it is not.
  const [activeWaveSkillNames, setActiveWaveSkillNames] = useState<string[]>([]);

  useEffect(() => {
    if (!realProjectId) {
      setActiveWaveSkillNames([]);
      return;
    }
    const wsUrl = orchestratorLogsWsUrl(realProjectId);
    if (!wsUrl) return;

    let cancelled = false;
    let ws: WebSocket | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    const connect = () => {
      if (cancelled) return;
      try {
        ws = new WebSocket(wsUrl);
        ws.onmessage = (event) => {
          try {
            const parsed: unknown = JSON.parse(event.data);
            if (typeof parsed !== 'object' || parsed === null) return;
            const message = (parsed as Partial<OrchestratorLogEntry>).message;
            if (typeof message !== 'string') return;
            // Real, narrow match only — an unrelated real log entry (a
            // different source/run) is silently ignored, never guessed at.
            const names = parseWaveStartedSkillNames(message);
            if (names) setActiveWaveSkillNames(names);
          } catch {
            // Malformed frame — dropped, never rendered on faith.
          }
        };
        ws.onclose = () => {
          if (cancelled) return;
          reconnectTimer = setTimeout(connect, 3000);
        };
        ws.onerror = () => {
          ws?.close();
        };
      } catch {
        reconnectTimer = setTimeout(connect, 3000);
      }
    };
    connect();

    return () => {
      cancelled = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      if (ws) {
        ws.onopen = null;
        ws.onmessage = null;
        ws.onerror = null;
        ws.onclose = null;
        ws.close();
      }
    };
  }, [realProjectId]);

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

  // Skills & MCP — Phase 1 (see hooks/useSkillSelection.ts). A standalone
  // hook, not inline state here, so a future generation-engine unit can
  // reuse the exact same selection contract without depending on this
  // page's other internals.
  const {
    skills,
    selectedSkillIds,
    toggleSkill,
    selectAllSkills,
    deselectAllSkills,
    resetSelectedSkills,
    addCustomSkill,
    fileSkillErrors,
    fileSkillsDirectory,
  } = useSkillSelection(
    activeGenerationSession?.selectedSkillIds || DEFAULT_SELECTED_SKILL_IDS,
  );
  // A skill whose name genuinely is a selected category (e.g. category
  // "Performance" <-> skill "Performance Testing") is used for generation
  // automatically, without also requiring its own checkbox — see
  // `categoryMatchedSkillIds`'s doc comment for why the match is exact-stem
  // only, never a fuzzy guess.
  const autoMatchedSkillIds = useMemo(
    () => categoryMatchedSkillIds(selectedCategories, skills),
    [selectedCategories, skills],
  );
  // What actually gets sent to the real generator: every manually-checked
  // skill plus every auto-matched one — the same set `handleTriggerGenerate`
  // sends, so the "Skills:" summary below can never under-report what a
  // real generation call used.
  const effectiveSkillIds = useMemo(
    () => [...new Set([...selectedSkillIds, ...autoMatchedSkillIds.keys()])],
    [selectedSkillIds, autoMatchedSkillIds],
  );
  // The skills exactly as a generation sends them — and what the requirement
  // plan below is worked out for, so the plan can never describe a different
  // set than the request that follows.
  const requestSkills: GenerateAiSkillInput[] = useMemo(
    () =>
      effectiveSkillIds
        .map((id) => skills.find((s) => s.id === id))
        .filter((s): s is (typeof skills)[number] => Boolean(s))
        .map((s) => ({ id: s.id, name: s.name, description: s.description })),
    [effectiveSkillIds, skills],
  );
  // Phase 3 — the uploaded documents that have numbered requirements read
  // from them, and whether this generation is to use those: the user opted in
  // AND there is at least one such document. Otherwise (the default) the
  // documents are pasted into the request as they always were.
  const requirementDocIds = useMemo(
    () => documents.map((d) => d.requirementsDocumentId).filter((id): id is string => Boolean(id)),
    [documents],
  );
  const useRequirements = requirementMode && requirementDocIds.length > 0;
  const isGeneratingNow = ['VALIDATING', 'PROCESSING', 'GENERATING'].includes(generationState);

  // The controller for each project's own `generate-ai` call currently in
  // flight, if any, keyed by that project's id — a real map, not a single
  // slot, because switching projects deliberately does not cancel one (see
  // the reseed effect's own doc comment below for why it is meant to keep
  // running instead), so this page can genuinely have more than one
  // outstanding at once (generate for project A, switch to B before it
  // settles, generate for B too). Set at the start of
  // `handleTriggerGenerate`, keyed by the project that call is for; that
  // entry is deleted once the call settles on its own (success or a real
  // failure), so the unmount cleanup below only ever acts on calls that
  // are genuinely still running.
  const generationAbortControllersRef = useRef<Record<string, AbortController>>({});

  // The project this page is *currently* showing, from whichever render's
  // closure last ran this effect — kept in a ref (not just read off
  // `realProjectId`) so `handleTriggerGenerate`'s completion handler,
  // itself a closure over the render a real generation call actually
  // started on, can tell "is the project I generated for still the one on
  // screen right now" apart from "the user has since switched away from
  // it", however much later that completion actually arrives. Doubles as
  // the previous-vs-current comparison the reseed effect below needs;
  // starts `null`, which only otherwise means "no project selected", so
  // seeing it there is how that effect's very first run tells "just found
  // out which project is active" apart from "the project genuinely
  // changed under us".
  const currentProjectIdRef = useRef<string | null>(null);

  // Reseeds every project-scoped piece of this page's own state from that
  // *other* real project's own real session the moment the selected
  // project itself changes — a project switch never unmounts this page
  // (`realProjectId` comes from `useProject()`, read inside an
  // already-mounted `TestCaseGeneratorPage`; the `key={realProjectId}` a
  // few hundred lines down only remounts this page's own child components,
  // not the state declared here, above them, that it hands down to them as
  // props), so without this, everything below kept right on showing
  // whichever project was selected when it was last set.
  //
  // Reported in two parts: (1) cases just generated for project "ambiance"
  // kept showing after switching to a brand-new project with none of its
  // own yet — `persistedCases` correctly became empty for it (its own
  // real, project-scoped fetch, unaffected by this), but `hasPersisted`
  // being `false` then fell back to `generatedCases`, which was still
  // "ambiance"'s; (2) switching back to "ambiance" afterward must show
  // "ambiance"'s own session exactly as it was left, not a wipe — a
  // generation genuinely in flight for it must keep running in the
  // background the whole time, same as a person doing exactly that would
  // expect. `activeGenerationSession` (`AIQAContext`) is the real, durable,
  // per-project store this reseed reads from — kept correct for whichever
  // project this page *last* showed by the continuous-mirror effect below
  // (while this page was showing it) and by `handleTriggerGenerate`'s own
  // completion handler (even after this page has since moved on, per that
  // handler's own doc comment) — so a project's own entry is never stale
  // by more than "a call for it that hasn't come back yet", which is
  // exactly the "still generating" state a return to it should honestly
  // show.
  //
  // Skipped on this page's very first render for whatever project it
  // mounted already showing (see `currentProjectIdRef`'s own doc comment
  // on why `null` is what that first run sees there; the dependency array
  // already skips a render where `realProjectId` itself did not change).
  useEffect(() => {
    const previousProjectId = currentProjectIdRef.current;
    currentProjectIdRef.current = realProjectId;
    if (previousProjectId === null || previousProjectId === realProjectId) return;

    const session = realProjectId ? getGenerationSession(realProjectId) : null;
    setActiveBranch(session?.generatedCases && session.generatedCases.length > 0 ? 'GENERATED_CASES' : 'SOURCES_AND_GENERATION');
    setDocuments(session?.documents || []);
    setAppSources(session?.appSources || defaultAppSources());
    setCrawlRunId(null); // see this state's own declaration — never restored from a session yet, for any project
    setSelectedCategories(session?.selectedCategories || DEFAULT_SELECTED_CATEGORIES);
    setDepth(session?.depth || 'STANDARD');
    setRequirementMode(session?.requirementMode ?? false);
    setGenerationState(session?.generationState || 'IDLE');
    setGenerationError(session?.generationError ?? null);
    setLastGenerationResult(session?.lastGenerationResult ?? null);
    setLastRoutingMode(null);
    setActiveWaveSkillNames([]);
    setGeneratedCases(session?.generatedCases || []);
    setSaveState('IDLE');
    setSaveErrorMessage(null);
    setHandoffError(null);
    resetSelectedSkills(session?.selectedSkillIds || DEFAULT_SELECTED_SKILL_IDS);
  }, [realProjectId, getGenerationSession, resetSelectedSkills]);

  // Keeps `activeGenerationSession` (`AIQAContext` — the one state React
  // Router does not tear down on an in-app page switch, see that context's
  // own doc comment) mirroring this page's setup and progress the moment
  // any of it changes, not only once a generation call finishes. Before
  // this, an upload, or generation simply starting, never reached that
  // session — only `handleTriggerGenerate`'s own success/failure paths
  // wrote to it, once each, at the very end — so navigating away before a
  // generation finished (or before ever clicking Generate) genuinely lost
  // the uploaded FRD/PRD/SRD and left the session with no sign a
  // generation had even been attempted.
  //
  // This does not replace those two direct writes in `handleTriggerGenerate`
  // below: a real response (or a real failure) can arrive after this page
  // has already been left, and an effect never runs again once unmounted —
  // only a setter belonging to the still-mounted `AIQAProvider`, called
  // directly from that still-running async function regardless of this
  // page's own lifetime, can still reach the session at that point. What
  // this effect adds is everything that changes *while the page is still
  // open* — every document upload, every config change, and the instant
  // generation starts — none of which waits for that async function to
  // finish.
  useEffect(() => {
    setActiveGenerationSession((prev) => ({
      id: prev?.id ?? `gen-${Date.now()}`,
      requirementText: buildRequirementText(),
      specType: prev?.specType ?? 'NATURAL_LANGUAGE',
      targetAppUrl: appSources.STAGING?.url || appSources.DEMO?.url || appSources.LIVE?.url || undefined,
      currentStep: generatedCases.length > 0 ? 'GENERATED_CASES' : isGeneratingNow ? 'AI_PLANNING' : 'REQUIREMENT_INPUT',
      generationState,
      generationError,
      lastGenerationResult,
      selectedCategories,
      documents,
      appSources,
      depth,
      selectedSkillIds,
      requirementMode,
      generatedCases,
      createdAt: prev?.createdAt ?? new Date().toISOString(),
    }));
    // `buildRequirementText` closes over the same fields already listed
    // below; `isGeneratingNow` is derived from `generationState`, already
    // listed. Neither needs its own entry.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [documents, appSources, selectedCategories, depth, generationState, generationError, lastGenerationResult, generatedCases, selectedSkillIds, requirementMode]);

  // The call that started a generation this page is showing as in flight can
  // belong to an *earlier instance* of this page (left and re-entered while
  // it ran): its completion handler only reaches the project's session, and
  // this instance's own state is not subscribed to it. So when the session
  // reports the attempt finished while this page still shows it running, take
  // the real result from the session. Keyed on the session alone, so a new
  // attempt (local state flips to running before the mirror effect above has
  // written that to the session) is never reverted to the previous result.
  useEffect(() => {
    const finished = activeGenerationSession;
    if (!isGeneratingNow || !finished?.generationState) return;
    if (['VALIDATING', 'PROCESSING', 'GENERATING'].includes(finished.generationState)) return;
    setGeneratedCases(finished.generatedCases || []);
    setGenerationState(finished.generationState);
    setGenerationError(finished.generationError ?? null);
    setLastGenerationResult(finished.lastGenerationResult ?? null);
    setActiveWaveSkillNames([]);
    if (finished.generationState === 'COMPLETED') setActiveBranch('GENERATED_CASES');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeGenerationSession]);

  // A generation call still in flight keeps running even if this page
  // itself is left (navigated away from entirely, not just switched to a
  // different project) — same reasoning as the reseed effect above for a
  // project switch: `handleTriggerGenerate`'s own completion handler
  // writes its real result directly to `generatingForProjectId`'s session
  // in `AIQAContext` (still mounted regardless of this page), so a later
  // visit — to this project, from this page or a fresh one — reads that
  // real result back, exactly as if this page had stayed open the whole
  // time. Nothing here needs to cancel it just because this page unmounts.

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

  /** Real save for the Edit Test Case modal — a real backend PATCH for an
   *  already-persisted case (refetching the real row afterward, same
   *  `refetchPersistedCases` every other real lifecycle action above
   *  already uses), or a plain local mutation for a still-local draft.
   *  Deliberately lets a real failure propagate (no local catch here) so
   *  the modal's own error display shows it and the modal does not close
   *  on a failed save. */
  const handleEditTestCase = async (id: string, updates: EditTestCaseUpdates): Promise<void> => {
    if (hasPersisted) {
      if (!realProjectId) throw new Error('No project selected.');
      await aiqaApi.testCases.update(realProjectId, id, {
        title: updates.title,
        description: updates.description,
        priority: updates.priority,
        preconditions: updates.preconditions,
        steps: updates.steps.map((s) => ({
          step_number: s.stepNumber,
          action: s.action,
          expected_result: s.expectedResult,
        })),
        expected_result: updates.expectedResult,
      });
      await refetchPersistedCases();
      return;
    }
    setGeneratedCases((prev) =>
      prev.map((tc) =>
        tc.id === id
          ? {
              ...tc,
              title: updates.title,
              description: updates.description,
              priority: updates.priority,
              preconditions: updates.preconditions,
              steps: updates.steps,
              expectedResult: updates.expectedResult,
            }
          : tc,
      ),
    );
  };

  /** The real bridge to the real Execution Hub (FRD-EXE-11). `choice` is a
   *  required, explicit, user-visible decision — see
   *  `TestCaseHandoffRequest`'s own doc comment for why there is no default
   *  that would silently fabricate a "passed" result. Phase 2: `choice.mode
   *  === 'executor'` sends the real `executor` field (no `params` — see
   *  `HandoffChoice`'s own doc comment), reaching the exact same real,
   *  unmodified backend path (`api.rs::handoff_test_case_scoped` ->
   *  `create_run_core` -> `engine::spawn_run` -> a real `executor.rs`
   *  adapter) the Execution Hub's own manually-created tasks already use —
   *  no new backend code, no duplicate execution path. */
  const handleHandoffReal = async (id: string, choice: HandoffChoice) => {
    if (!realProjectId) return;
    setHandoffError(null);
    const body: TestCaseHandoffRequest =
      choice.mode === 'executor'
        ? { executor: choice.executor }
        : choice.mode === 'compile'
          ? { compile_and_execute: true }
          : { simulate: { outcome: choice.outcome, delay_ms: 150 } };
    try {
      const created = await aiqaApi.testCases.handoff(realProjectId, id, body);
      await refetchPersistedCases();
      navigate(`/execution?run=${created.run_id}`);
    } catch (err) {
      setHandoffError(err instanceof ApiError ? err.message : 'Handoff failed.');
    }
  };

  // Phase 2.1 — read-only compile preview: lets the user see the real
  // compiled plan's READY/NOT_EXECUTABLE/COMPILATION_FAILED/
  // COMPILATION_UNAVAILABLE outcome (`POST .../compile`, never mutates
  // anything) before committing to a real `compile_and_execute` handoff.
  const [compilePreview, setCompilePreview] = useState<CompilationOutcome | null>(null);
  const [compilePreviewLoading, setCompilePreviewLoading] = useState(false);
  const [compilePreviewForId, setCompilePreviewForId] = useState<string | null>(null);
  const [compilePreviewError, setCompilePreviewError] = useState<string | null>(null);

  const handleCompilePreview = async (id: string) => {
    if (!realProjectId) return;
    setCompilePreviewLoading(true);
    setCompilePreviewError(null);
    setCompilePreview(null);
    setCompilePreviewForId(id);
    try {
      const outcome = await aiqaApi.testCases.compile(realProjectId, id);
      setCompilePreview(outcome);
    } catch (err) {
      setCompilePreviewError(err instanceof ApiError ? err.message : 'Compilation preview failed.');
    } finally {
      setCompilePreviewLoading(false);
    }
  };

  // Helper for Header Status Indicator
  const getHeaderStatusIndicator = () => {
    switch (generationState) {
      case 'VALIDATING':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-yellow-950/80 border border-yellow-700/80 text-yellow-300">
            <RefreshCw className="w-3.5 h-3.5 text-yellow-400 animate-spin" />
            Validating Sources...
          </span>
        );
      case 'PROCESSING':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-bold bg-violet-950/80 border border-violet-700/80 text-violet-300">
            <Cpu className="w-3.5 h-3.5 text-violet-400 animate-pulse" />
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
          <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-semibold bg-slate-900 border border-slate-700 text-yellow-400">
            <span className="w-2 h-2 rounded-full bg-yellow-400 animate-pulse" />
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

  // Test Case Generation Pipeline (Phase 1 — real AI backend integration).
  //
  // The real backend contract (`GenerateAiTestCasesRequest`,
  // `contract.ts`) has exactly two fields: `requirement_text` and `skills`.
  // Selected categories, generation depth, attached document names, and
  // connected application-source labels have no dedicated wire field on
  // the real endpoint, so they are folded into `requirement_text` itself
  // as plain, clearly-labeled instruction/context lines — a real way to
  // give them influence over the actual prompt without inventing a backend
  // field that does not exist.
  //
  // Document *content* genuinely does reach the model now
  // (`doc_extract.rs`, wired through `DocumentUploadSection`'s real
  // extraction call): each document below carries its own real, extracted
  // text when extraction succeeded, or an honest, specific reason when it
  // didn't (e.g. a legacy .doc/.pdf this app cannot yet parse) — never a
  // silent "content not analyzed" claim papering over either case.
  const buildRequirementText = (): string => {
    const parts: string[] = [];
    if (selectedCategories.length > 0) {
      parts.push(`Focus particularly on these test categories where applicable: ${selectedCategories.join(', ')}.`);
    }
    parts.push(`Requested generation depth: ${depth}.`);
    for (const doc of documents) {
      // Phase 3 — generating from numbered requirements: a document read
      // into them is sent by its id (never pasted as well, which is what
      // would overflow every call), and one with text but no requirements
      // cannot be sent in this mode at all (`RequirementGenerationSection`
      // names it). Only the notes about content that could not be analyzed
      // stay.
      if (useRequirements && (doc.requirementsDocumentId || doc.extractedText)) continue;
      if (doc.extractedText) {
        const truncatedNote = doc.extractedTruncated ? ' (truncated to the first 20,000 characters)' : '';
        parts.push(`=== ${doc.label} — ${doc.fileName}${truncatedNote} ===\n${doc.extractedText}`);
      } else if (doc.extractionError) {
        parts.push(`${doc.label} (${doc.fileName}) is attached, but its real content could not be analyzed: ${doc.extractionError}`);
      }
    }
    const connectedSources = (Object.values(appSources) as AppSourceConnection[]).filter((a) => a.status === 'CONNECTED');
    if (connectedSources.length > 0) {
      parts.push(
        `Connected application sources (reference only — not fetched by this request): ${connectedSources
          .map((a) => `${a.label} (${a.url})`)
          .join(', ')}.`
      );
    }
    return parts.filter((p) => p.trim().length > 0).join('\n');
  };

  /** Mirrors `testCaseRowToDisplay` above: turns one real, un-persisted
   *  `GeneratedTestCaseDraft` (actual AI output, or the caller's own
   *  resolved skill on a fallback path) into the existing
   *  `GeneratedTestCase` shape this page's draft-review UI already
   *  renders. Every field is either copied verbatim from the real response
   *  or is a disclosed, honestly-labeled derivation of it — never a
   *  reintroduced fabricated value from the old local template. */
  const aiDraftToDisplay = (
    draft: GeneratedTestCaseDraft,
    index: number,
    meta: { method: 'ai' | 'fallback'; provider: string | null; model: string | null },
    /** Requirement-driven generation only: the numbered requirements the
     *  request was generated from, by id (`null` for an id two documents
     *  share, which cannot be told apart). */
    requirements?: Map<string, RequirementRecord | null>,
  ): GeneratedTestCase => {
    const skill = skills.find((s) => s.id === draft.skill_id);
    const tcId = `AI-${(index + 1).toString().padStart(3, '0')}`;
    // The requirement a case was generated for, when the response names one
    // and its text could be read back: that requirement is then the case's
    // source — its own document, its own text — rather than every document
    // and every setting of the request together.
    const linked = draft.requirement_id ? requirements?.get(draft.requirement_id) ?? null : null;
    const linkedSlot = linked ? documents.find((d) => d.requirementsDocumentId === linked.document_id)?.slotId : undefined;
    const docLabels = linkedSlot ?? (documents.map((d) => d.slotId).join(' + ') || undefined);
    const reqText = buildRequirementText();
    const aiAnalysis =
      meta.method === 'ai'
        ? `Generated by ${meta.provider ?? 'an AI provider'}/${meta.model ?? 'an unreported model'}${
            skill ? ` (skill: ${skill.name})` : ''
          } — evidence: ${draft.evidence}${draft.requirement_id ? `, requirement: ${draft.requirement_id}` : ''}${
            draft.source_reference ? `, cites: "${draft.source_reference}"` : ''
          }.`
        : 'AI generation did not produce this case.';

    return {
      id: tcId,
      title: draft.title,
      description: `AI-generated ${draft.category} test case (skill: ${skill?.name ?? draft.skill_id}, evidence: ${draft.evidence}).`,
      category: draft.category,
      priority: draft.priority as GeneratedTestCase['priority'],
      severity: draft.severity as GeneratedTestCase['severity'],
      sourceDocument: docLabels,
      // The requirement's own id (what the quality graph's requirement
      // impact is looked up by) when the case is tied to one; otherwise the
      // quoted phrase, as it always was.
      requirementReference: draft.requirement_id ?? draft.source_reference ?? undefined,
      sourceRequirement: linked?.text ?? (reqText || undefined),
      traceability: {
        sourceDoc: docLabels ?? 'Configuration & Skills (no document attached)',
        requirementRef: draft.requirement_id
          ? `${draft.requirement_id}${draft.source_reference ? ` — cites: "${draft.source_reference}"` : ''}`
          : draft.source_reference ?? `Not cited (${draft.evidence})`,
        requirementText: linked?.text ?? reqText,
        aiAnalysis,
        testCaseId: tcId,
        testCategory: draft.category,
        executionTarget: 'Not yet assigned — assigned at execution handoff',
        evidencePolicy: 'Not yet captured — captured at execution handoff',
        reportMapping: 'Not yet mapped',
      },
      preconditions: draft.preconditions,
      testData: JSON.stringify(
        {
          skill_id: draft.skill_id,
          evidence: draft.evidence,
          source_reference: draft.source_reference,
          ...(draft.requirement_id ? { requirement_id: draft.requirement_id } : {}),
        },
        null,
        2
      ),
      steps: draft.steps.map((s, i) => ({ stepNumber: i + 1, action: s.action, expectedResult: s.expected_result })),
      expectedResult: draft.expected_result,
      assertions: [draft.expected_result],
      riskFactor: draft.priority === 'CRITICAL' ? 'HIGH' : draft.priority === 'HIGH' ? 'MEDIUM' : 'LOW',
      status: 'PENDING_REVIEW',
      suggestedTags: [
        draft.category.toLowerCase().replace(/\s+/g, '-'),
        draft.skill_id,
        draft.evidence,
        meta.method === 'ai' ? 'ai-generated' : 'ai-fallback',
      ],
      generatedAt: new Date().toISOString(),
    };
  };

  const handleTriggerGenerate = async () => {
    if (!realProjectId) {
      setGenerationState('FAILED');
      setGenerationError('Select a project before generating test cases — the real AI generation endpoint is project-scoped.');
      return;
    }
    // Frozen for this one call, in this render's closure — deliberately not
    // read again as `realProjectId` after the `await` below, since the user
    // may by then have switched to a different project (`realProjectId`
    // itself would reflect *that* one instead). This is the project the
    // real backend call below is actually made for, and the one its result
    // belongs to, however much later it actually arrives.
    const generatingForProjectId = realProjectId;
    const reqText = buildRequirementText();
    if (!reqText.trim()) {
      setGenerationState('FAILED');
      setGenerationError('Select at least one test category or source before generating test cases.');
      return;
    }

    setGenerationError(null);
    setGenerationState('GENERATING');
    // Clear any glow left over from a previous run before this one's own
    // real wave-start events (if any) arrive.
    setActiveWaveSkillNames([]);

    // Lets the unmount-cleanup effect above cancel this specific call if
    // the page is left before it finishes — see that effect's own doc
    // comment.
    const controller = new AbortController();
    generationAbortControllersRef.current[generatingForProjectId] = controller;

    // Phase 3 — frozen for this call like `generatingForProjectId` above:
    // the documents whose numbered requirements this request is generated
    // from, or none (the default) for a request that pastes them.
    const requirementDocumentIds = useRequirements ? requirementDocIds : [];

    // Best-effort, non-blocking: real routing mode for the "Generation
    // Details" panel (brief §10). A failure here must never block or fail
    // the real generation call itself — it only means "Mode" renders as
    // "Not Reported" rather than a fabricated guess.
    try {
      const settings = await aiqaApi.ai.getSettings();
      setLastRoutingMode(settings.mode);
    } catch {
      setLastRoutingMode(null);
    }

    try {
      const response = await aiqaApi.testCases.generateAi(
        generatingForProjectId,
        {
          requirement_text: reqText,
          skills: requestSkills,
          crawl_run_id: crawlRunId,
          ...(requirementDocumentIds.length > 0 ? { requirement_document_ids: requirementDocumentIds } : {}),
        },
        controller.signal,
      );
      // The call settled on its own — nothing left for the unmount
      // cleanup above to cancel for this project.
      delete generationAbortControllersRef.current[generatingForProjectId];

      // Phase 3 — a requirement-driven run's cases each name a requirement;
      // reading the requirements back is what lets a case show that
      // requirement's own text and document as its source. Best-effort: if
      // they cannot be read, the cases still carry the requirement's id and
      // simply fall back to the request's own text as their source.
      let requirementsById: Map<string, RequirementRecord | null> | undefined;
      if (response.requirements && requirementDocumentIds.length > 0) {
        try {
          const read = await aiqaApi.requirements.list(generatingForProjectId, { documentIds: requirementDocumentIds });
          requirementsById = new Map();
          for (const r of read.requirements) {
            // An id two documents share cannot be told apart by id alone.
            requirementsById.set(r.key, requirementsById.has(r.key) ? null : r);
          }
        } catch {
          requirementsById = undefined;
        }
      }

      const meta = { method: response.generation_method, provider: response.provider, model: response.model };
      const mapped = response.test_cases.map((draft, i) => aiDraftToDisplay(draft, i, meta, requirementsById));

      let finalState: GenerationExecutionState;
      let finalError: string | null;
      if (response.generation_method === 'ai' && mapped.length > 0) {
        finalState = 'COMPLETED';
        finalError = null;
      } else if (response.generation_method === 'ai') {
        // A real AI call ran, but every candidate case was dropped by the
        // real quality/traceability/dedup gates (`ai_generation.rs`) — a
        // genuine, non-empty outcome, not a network/server error, but also
        // not a set of cases to review.
        finalState = 'PARTIAL';
        finalError =
          'The AI model responded, but no candidate test case passed quality/traceability validation. Try different test categories or skills.';
      } else {
        // generation_method === 'fallback' — the real backend honestly
        // reports that no compatible AI model was available. Never
        // silently substitute local template output here.
        finalState = 'FAILED';
        finalError = response.fallback_reason || 'No AI provider is currently available for test case generation.';
      }

      // The user may have since switched to a different project — this
      // page's own local state (`documents`/`generatedCases`/... below) is
      // whatever *that* project is currently showing, so a completion for a
      // project no longer on screen must not touch any of it: doing so
      // would overwrite a different project's own real, currently-displayed
      // state with this one's. Its real result still needs to reach
      // somewhere durable, though — see the explicit, project-id-targeted
      // write below, which happens either way.
      if (currentProjectIdRef.current === generatingForProjectId) {
        setLastGenerationResult(response);
        // The whole multi-wave call has genuinely finished by now — every
        // wave it ran has really completed, so no wave is "in progress"
        // anymore. Holding the glow on past this point would claim otherwise.
        setActiveWaveSkillNames([]);
        setGeneratedCases(mapped);
        setGenerationState(finalState);
        setGenerationError(finalError);
        if (finalState === 'COMPLETED') setActiveBranch('GENERATED_CASES');
      }

      // Written directly, not only left to the mirror effect above, and
      // explicitly targeting the project this call was actually for (not
      // necessarily "whichever project is selected right now" —
      // `setActiveGenerationSession` above means that, this does not): this
      // completion can genuinely arrive after the page has moved on to a
      // different project, or been left entirely (nothing cancels this call
      // on either of those). An effect keyed on this page's own state never
      // runs again once that state stops changing for the project in
      // question, but this setter belongs to the still-mounted
      // `AIQAProvider`, so it still reaches that project's own session
      // regardless of what this page is currently showing.
      setGenerationSessionFor(generatingForProjectId, (prev) => ({
        id: prev?.id ?? `gen-${Date.now()}`,
        requirementText: reqText,
        specType: prev?.specType ?? 'NATURAL_LANGUAGE',
        targetAppUrl: appSources.STAGING?.url || appSources.DEMO?.url || appSources.LIVE?.url || undefined,
        currentStep: mapped.length > 0 ? 'GENERATED_CASES' : 'REQUIREMENT_INPUT',
        generationState: finalState,
        generationError: finalError,
        lastGenerationResult: response,
        selectedCategories,
        documents,
        appSources,
        depth,
        generatedCases: mapped,
        createdAt: prev?.createdAt ?? new Date().toISOString(),
        selectedSkillIds,
        requirementMode,
      }));
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        // Nothing in this page aborts a generation call any more (leaving
        // the page or switching project lets it finish); a genuine abort
        // from elsewhere must still never be reported as a real failure of
        // the AI/backend.
        return;
      }
      delete generationAbortControllersRef.current[generatingForProjectId];
      const message = err instanceof ApiError ? err.message : 'Test case generation failed.';
      if (currentProjectIdRef.current === generatingForProjectId) {
        setLastGenerationResult(null);
        setGenerationState('FAILED');
        setGenerationError(message);
        setActiveWaveSkillNames([]);
      }
      // Same reasoning and the same explicit-project-id targeting as the
      // success path above. Only the state, the error and the stored
      // response are patched: an attempt that failed produced no new cases,
      // so whatever was already in that project's session (from an earlier
      // successful attempt, if any) is left exactly as it was — except the
      // response, which no longer describes the latest attempt.
      setGenerationSessionFor(generatingForProjectId, (prev) =>
        prev ? { ...prev, generationState: 'FAILED', generationError: message, lastGenerationResult: null } : prev,
      );
    }
  };

  // AUD-005 — once any real, persisted cases exist for this project, the
  // Generated Test Cases branch displays and actions those, not the local
  // draft. Approve/Reject on a real case calls the real backend lifecycle
  // endpoint; on a still-local draft (before "Save to Project") it stays
  // exactly the lightweight local triage it always was (FE-16, unchanged).
  const hasPersisted = persistedCases.length > 0;
  const displayedCases: GeneratedTestCase[] = hasPersisted ? persistedCases.map(testCaseRowToDisplay) : generatedCases;

  // A real, native browser warning before losing genuinely unsaved
  // generated cases — closes a real gap that let real AI output vanish
  // silently on a tab close/refresh with zero warning. Only fires for an
  // actual browser-level unload (tab close, refresh, browser close);
  // in-app navigation to another page never reaches this, since
  // `generatedCases` already survives that via `activeGenerationSession`
  // (unchanged, see its own state declaration above) — this guard is only
  // for the one case that genuinely had none at all.
  useEffect(() => {
    if (hasPersisted || generatedCases.length === 0) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      // A truthy `returnValue` is what makes Chrome/older browsers show
      // their own built-in warning — the actual displayed text is
      // browser-controlled; every modern browser ignores a custom message
      // here, so none is invented.
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [hasPersisted, generatedCases.length]);

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
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <Sparkles className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">
              Test Case Generator
            </h1>
            {/* Phase 1 — truthful, response-driven badge. Never claims "AI
                GENERATED" before a real generation has actually completed. */}
            {lastGenerationResult ? (
              lastGenerationResult.generation_method === 'ai' ? (
                <span className="px-2 py-0.5 rounded bg-emerald-950/60 border border-emerald-800/60 text-[10px] font-mono text-emerald-300 uppercase font-semibold">
                  AI Generated
                </span>
              ) : (
                <span className="px-2 py-0.5 rounded bg-amber-950/60 border border-amber-800/60 text-[10px] font-mono text-amber-300 uppercase font-semibold">
                  AI Unavailable — Fallback
                </span>
              )
            ) : (
              <span className="px-2 py-0.5 rounded bg-yellow-950/60 border border-yellow-800/60 text-[10px] font-mono text-yellow-300 uppercase font-semibold">
                AI-Powered Generation
              </span>
            )}
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

      {/* Generation Details (Phase 1) — real, response-driven metadata.
          Never rendered before a real generation attempt has actually
          happened; every value is either the real response field or the
          honest "Not Reported" placeholder — never fabricated. */}
      {(generationError || lastGenerationResult) && (
        <div
          id="generation-details-panel"
          className={`p-3.5 rounded-lg border text-xs font-mono space-y-2 ${
            lastGenerationResult?.generation_method === 'ai'
              ? 'bg-emerald-950/20 border-emerald-800/60'
              : 'bg-amber-950/20 border-amber-800/60'
          }`}
        >
          <div className="flex items-center justify-between flex-wrap gap-2">
            <span className="font-bold text-slate-200 uppercase tracking-wide text-[11px]">Generation Details</span>
            {/* Suppressed when the reason is already shown by the "Fallback"
                line below, to avoid showing the exact same message twice. */}
            {generationError && lastGenerationResult?.generation_method !== 'fallback' && (
              <span className="text-rose-300">{generationError}</span>
            )}
          </div>
          {lastGenerationResult && (
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-x-4 gap-y-1.5 text-slate-300">
              <div>
                <span className="text-slate-500">Method: </span>
                {lastGenerationResult.generation_method === 'ai' ? 'AI' : 'Fallback (no AI output)'}
              </div>
              <div>
                <span className="text-slate-500">Provider: </span>
                {lastGenerationResult.provider ? providerName(lastGenerationResult.provider) : 'Not Reported'}
              </div>
              <div>
                <span className="text-slate-500">Model: </span>
                {lastGenerationResult.model ?? 'Not Reported'}
              </div>
              <div>
                <span className="text-slate-500">Mode: </span>
                {lastRoutingMode ?? 'Not Reported'}
              </div>
              <div className="col-span-2 sm:col-span-1">
                <span className="text-slate-500">Skills: </span>
                {effectiveSkillIds.length > 0
                  ? effectiveSkillIds.map((id) => skills.find((s) => s.id === id)?.name ?? id).join(', ')
                  : 'None selected'}
              </div>
              <div>
                <span className="text-slate-500">Fallback: </span>
                {lastGenerationResult.generation_method === 'fallback'
                  ? `Yes — ${lastGenerationResult.fallback_reason ?? 'no reason reported'}`
                  : 'No'}
              </div>
              {lastGenerationResult.generation_method === 'ai' && (
                <>
                  <div>
                    <span className="text-slate-500">Tokens: </span>
                    {lastGenerationResult.total_prompt_tokens !== null &&
                    lastGenerationResult.total_completion_tokens !== null
                      ? `${lastGenerationResult.total_prompt_tokens} in / ${lastGenerationResult.total_completion_tokens} out`
                      : 'Not Reported'}
                  </div>
                  <div>
                    <span className="text-slate-500">Latency: </span>
                    {(lastGenerationResult.total_latency_ms / 1000).toFixed(1)}s
                  </div>
                  <div>
                    <span className="text-slate-500">Est. Cost: </span>
                    {lastGenerationResult.total_estimated_cost_usd !== null
                      ? lastGenerationResult.total_estimated_cost_usd === 0
                        ? 'Free (self-hosted)'
                        : `$${lastGenerationResult.total_estimated_cost_usd.toFixed(4)}`
                      : 'Not Reported'}
                  </div>
                </>
              )}
            </div>
          )}
          {lastGenerationResult?.requirements && (
            <RequirementCoverageSummary coverage={lastGenerationResult.requirements} />
          )}
          {lastGenerationResult?.waves && (
            <GenerationWaveResults waves={lastGenerationResult.waves.filter((w) => !w.skipped)} providerName={providerName} />
          )}
        </div>
      )}

      {/* Dual-Branch Navigation Ribbon (Source & Generation vs Generated Test Cases) */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-1">
        <div className="flex items-center gap-2">
          {/* Branch 1: Source & Generation */}
          <button
            type="button"
            onClick={() => setActiveBranch('SOURCES_AND_GENERATION')}
            className={`flex items-center gap-2 px-3.5 py-2 rounded-t-lg text-xs font-mono font-bold transition-colors cursor-pointer border-b-2 ${
              activeBranch === 'SOURCES_AND_GENERATION'
                ? 'border-violet-500 text-violet-300 bg-[#0B1120]/80'
                : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-900/40'
            }`}
          >
            <Layers className="w-3.5 h-3.5" />
            <span>Source & Generation</span>
            {documents.length > 0 && (
              <span className="text-[10px] px-1.5 py-0.2 rounded bg-yellow-950 text-yellow-300 border border-yellow-800">
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
                ? 'border-violet-500 text-violet-300 bg-[#0B1120]/80'
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
            className="flex items-center gap-1 text-xs font-mono text-yellow-400 hover:text-yellow-300 font-semibold cursor-pointer pb-1"
          >
            <span>View {generatedCases.length} Generated Cases</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        )}

        {activeBranch === 'GENERATED_CASES' && (
          <button
            type="button"
            onClick={() => setActiveBranch('SOURCES_AND_GENERATION')}
            className="flex items-center gap-1 text-xs font-mono text-violet-400 hover:text-violet-300 font-semibold cursor-pointer pb-1"
          >
            <span>Edit Sources & Configuration</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {/* Main Content Body */}
      {activeBranch === 'SOURCES_AND_GENERATION' ? (
        /* Branch 1: Source & Generation Workspace.
           Skills & MCP Phase 1: a 70/30 grid (`lg:grid-cols-10`,
           `col-span-7`/`col-span-3` — the same 12/10-column grid idiom
           `GeneratedTestCasesView.tsx`'s own detail layout already uses,
           not a new ratio system) puts the Skills & MCP panel beside this
           existing input area on desktop, and stacks it below on
           tablet/mobile via the default `grid-cols-1`. Nothing inside
           either column below was redesigned — only re-parented.
           Widened to `lg:grid-cols-12` (same two columns keep their
           `col-span-7`/`col-span-3`, just narrower relative to a 12-wide
           row) to make room for a third `col-span-2` column: the AI Engine
           rail, the same real orb `DashboardPage` shows, filling what was
           previously dead margin space rather than adding a decorative
           graphic with no real data behind it. */
        <div id="project-sources-workspace" className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start">
          <div className="lg:col-span-7 space-y-6 min-w-0">
            {/* Section Banner: Project Sources */}
            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-1">
              <div className="flex items-center gap-2">
                <span className="text-[10px] font-mono text-yellow-400 uppercase font-bold bg-yellow-950/60 px-1.5 py-0.5 rounded border border-yellow-800/60">
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
                projectId={realProjectId}
                disabled={isGeneratingNow}
              />
            </div>

            {/* 2. Application Source Section (4 channels) */}
            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
              <ApplicationSourceSection
                appSources={appSources}
                onAppSourcesChange={setAppSources}
                projectId={realProjectId}
                disabled={isGeneratingNow}
              />
            </div>

            {/* 2b. Crawl Grounding Section — Phase 6's real crawler, now
                wired into generation. */}
            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
              <CrawlGroundingSection
                appSources={appSources}
                projectId={realProjectId}
                crawlRunId={crawlRunId}
                onCrawlRunIdChange={setCrawlRunId}
                disabled={isGeneratingNow}
              />
            </div>

            {/* 2c. Requirement-driven generation (Phase 3) — opt-in, off by
                default: generate from the numbered requirements read out of
                the uploaded documents instead of pasting them. */}
            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
              <RequirementGenerationSection
                projectId={realProjectId}
                enabled={requirementMode}
                onEnabledChange={setRequirementMode}
                documents={documents}
                skills={requestSkills}
                disabled={isGeneratingNow}
              />
            </div>

            {/* 3. Test Generation Configuration (14 Categories & CTA).
                Note: the "Generate Test Cases" action stays embedded here
                (not hoisted below both grid columns) — it shares this
                section's own validation logic
                (`GenerationConfigSection::handleGenerateClick`), and
                duplicating that logic at the page level purely to relocate
                one button was judged a worse trade than this one, disclosed,
                minor deviation from the layout mock's literal button
                placement. See the Phase 1 implementation report. */}
            <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
              <GenerationConfigSection
                selectedCategories={selectedCategories}
                onToggleCategory={handleToggleCategory}
                onSelectAllCategories={handleSelectAllCategories}
                onDeselectAllCategories={handleDeselectAllCategories}
                depth={depth}
                onDepthChange={setDepth}
                generationState={generationState}
                onTriggerGenerate={handleTriggerGenerate}
                documents={documents}
                appSources={appSources}
              />
            </div>
          </div>

          <div className="lg:col-span-3 min-w-0">
            <SkillsAndMcpPanel
              skills={skills}
              selectedSkillIds={selectedSkillIds}
              onToggleSkill={toggleSkill}
              onSelectAll={selectAllSkills}
              onDeselectAll={deselectAllSkills}
              onAddCustomSkill={addCustomSkill}
              disabled={isGeneratingNow}
              fileSkillErrors={fileSkillErrors}
              fileSkillsDirectory={fileSkillsDirectory}
              autoMatchedSkills={autoMatchedSkillIds}
              activeSkillNames={activeWaveSkillNames}
            />
          </div>

          <div className="hidden lg:block lg:col-span-2 min-w-0">
            <TestGenerationFlow
              documents={documents}
              appSources={appSources}
              skillCount={effectiveSkillIds.length}
              generationState={generationState}
              lastGenerationResult={lastGenerationResult}
              generatedCasesCount={generatedCases.length}
              providerName={providerName}
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
          onEditTestCase={handleEditTestCase}
          onHandoffTestCase={handleHandoffReal}
          handoffError={handoffError}
          onCompilePreview={handleCompilePreview}
          compilePreview={compilePreview}
          compilePreviewLoading={compilePreviewLoading}
          compilePreviewForId={compilePreviewForId}
          compilePreviewError={compilePreviewError}
        />
      )}
    </div>
  );
};
