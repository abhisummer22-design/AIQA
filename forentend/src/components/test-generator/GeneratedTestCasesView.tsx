import React, { useState, useMemo } from 'react';
import {
  Search,
  CheckCircle2,
  XCircle,
  Clock,
  Sparkles,
  Play,
  ArrowRight,
  Layers,
  Check,
  X,
  FileText,
  CheckSquare,
  Terminal,
  Cpu,
  Bookmark,
  Shield,
  ShieldCheck,
  RefreshCw,
  FileCode2,
  Camera,
  Activity,
  Server,
  Network,
  Info,
  ExternalLink,
  HelpCircle,
  Pencil,
} from 'lucide-react';
import {
  GeneratedTestCase,
  TestGenerationCategory,
  TestCasePriority,
  TestCaseSeverity,
} from '../../types';
import { TraceabilityGraph } from './TraceabilityGraph';
import { TEST_CATEGORIES } from './GenerationConfigSection';
import { skillStem } from '../../services/skillRegistry';
import { EXECUTOR_OPTIONS, SIMULATE_OUTCOMES } from '../pages/ExecutionHubPage';
import type { HandoffChoice } from '../pages/TestCaseGeneratorPage';
import { EditTestCaseModal, type EditTestCaseUpdates } from './EditTestCaseModal';
import type { CompilationOutcome } from '../../types/contract';

/** Phase 2.1 — the one non-real-executor addition to the handoff dropdown:
 *  compiles this specific case's own actual content into a structured plan
 *  instead of running a fixed reference fixture. Kept out of
 *  `EXECUTOR_OPTIONS` itself (that list is shared with the Execution Hub's
 *  manual, no-test-case task creation, which has nothing to compile). */
const COMPILE_OPTION_VALUE = '__compile__';
const HANDOFF_OPTIONS = [
  { value: COMPILE_OPTION_VALUE, label: 'Compile & Execute (real, from this case’s own content)' },
  ...EXECUTOR_OPTIONS,
];

/** Every canonical category id, keyed by its own stem, for `resolveCategoryId`. */
const CATEGORY_ID_BY_STEM = new Map(TEST_CATEGORIES.map((c) => [c.id.toLowerCase(), c.id]));

/** A generated test case's real `category` field is freeform AI output
 *  (`ai_generation.rs::GeneratedTestCaseDraft.category: String` — the
 *  model is never constrained to the 14 canonical category ids), and it
 *  demonstrably sometimes writes the originating skill's own display name
 *  into it (e.g. "Functional Testing") instead of the bare category id
 *  ("Functional") — so a strict `===` match against `TEST_CATEGORIES`
 *  silently drops those real cases from every per-category filter/count
 *  while they still count under "All Categories" (the visible symptom:
 *  most category chips read 0 even though the cases genuinely exist).
 *
 *  Reuses the exact same testing-verb-suffix stemming
 *  `categoryMatchedSkillIds` (`skillRegistry.ts`) already applies the
 *  other direction (category -> matching skill), so "Functional Testing"
 *  and "Functional" resolve to the same real id here too. When the raw
 *  text does not stem-match any canonical id (e.g. "RBAC" — a real skill
 *  with no corresponding category), the real text is returned unchanged
 *  rather than forced into a category it does not belong to. */
function resolveCategoryId(rawCategory: string): string {
  return CATEGORY_ID_BY_STEM.get(skillStem(rawCategory)) ?? rawCategory;
}

interface GeneratedTestCasesViewProps {
  testCases: GeneratedTestCase[];
  onApproveTestCase: (id: string) => void;
  onRejectTestCase: (id: string) => void;
  onApproveAll: () => void;
  /** FE-16: was `onExportToExecution` — renamed because there is no real
   *  export/dispatch: this only navigates to the real Execution Hub. See
   *  the FE-16 report §10. */
  onOpenExecutionHub: () => void;
  onNavigateToSources: () => void;
  /** AUD-005 — real, persisted-backend fields below. `hasPersisted`: true
   *  once at least one real test case exists for this project (`testCases`
   *  above is then the real, backend-sourced list, not a local draft). */
  hasPersisted?: boolean;
  canSave?: boolean;
  saveState?: 'IDLE' | 'SAVING' | 'ERROR';
  saveErrorMessage?: string | null;
  onSaveToProject?: () => void;
  casesLoading?: boolean;
  casesError?: string | null;
  onValidateTestCase?: (id: string) => void;
  /** Real save — a backend PATCH for an already-persisted case, or a local
   *  draft mutation for one not yet saved (the caller decides which,
   *  exactly like every other action here already branches on
   *  `hasPersisted`). Rejects with a real, specific error on failure; the
   *  modal surfaces it and does not close. */
  onEditTestCase?: (id: string, updates: EditTestCaseUpdates) => Promise<void>;
  /** Phase 2 — `choice` is either a real executor (`{ mode: 'executor',
   *  executor }`, dispatched via `TestCaseHandoffRequest.executor` with no
   *  params, exactly like the Execution Hub's own manually-created tasks —
   *  no per-case params UI exists anywhere in this app, and none is
   *  invented here) or an explicit simulated outcome (`{ mode: 'simulate',
   *  outcome }`, unchanged from before this phase). See
   *  `TestCaseGeneratorPage.tsx::handleHandoffReal`. */
  onHandoffTestCase?: (id: string, choice: HandoffChoice) => void;
  handoffError?: string | null;
  /** Phase 2.1 — read-only compile preview (`POST .../compile`, never
   *  mutates anything). `compilePreviewForId` disambiguates which test
   *  case `compilePreview`/`compilePreviewLoading`/`compilePreviewError`
   *  actually belong to, since the parent owns this as single shared
   *  state, not per-row state. */
  onCompilePreview?: (id: string) => void;
  compilePreview?: CompilationOutcome | null;
  compilePreviewLoading?: boolean;
  compilePreviewForId?: string | null;
  compilePreviewError?: string | null;
}

export const GeneratedTestCasesView: React.FC<GeneratedTestCasesViewProps> = ({
  testCases,
  onApproveTestCase,
  onRejectTestCase,
  onApproveAll,
  onOpenExecutionHub,
  onNavigateToSources,
  hasPersisted = false,
  canSave = false,
  saveState = 'IDLE',
  saveErrorMessage = null,
  onSaveToProject,
  casesLoading = false,
  casesError = null,
  onValidateTestCase,
  onEditTestCase,
  onHandoffTestCase,
  handoffError = null,
  onCompilePreview,
  compilePreview = null,
  compilePreviewLoading = false,
  compilePreviewForId = null,
  compilePreviewError = null,
}) => {
  const [handoffOutcome, setHandoffOutcome] = useState<'pass' | 'fail' | 'timeout' | 'infra_error'>('pass');
  // Phase 2 — '' means Simulate (matches `EXECUTOR_OPTIONS`'s own '' entry),
  // any other value is a real, registered executor name.
  const [handoffExecutor, setHandoffExecutor] = useState<string>('');
  const [selectedCategory, setSelectedCategory] = useState<string>('ALL');
  // Self-contained modal-open state, same convention `SkillManagerModal`'s
  // own caller (`SkillsAndMcpPanel`) already uses — the real save logic
  // itself lives in `onEditTestCase`, owned by the page.
  const [editingCase, setEditingCase] = useState<GeneratedTestCase | null>(null);
  const [selectedTestCaseId, setSelectedTestCaseId] = useState<string | null>(
    testCases.length > 0 ? testCases[0].id : null
  );
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('ALL');
  const [priorityFilter, setPriorityFilter] = useState<string>('ALL');
  const [showTestData, setShowTestData] = useState<boolean>(false);
  const [showSourceExcerpt, setShowSourceExcerpt] = useState<boolean>(false);

  // If selected test case is no longer available or was cleared, keep sync
  const activeTestCase =
    testCases.find((tc) => tc.id === selectedTestCaseId) || testCases[0] || null;

  // Human-readable "TC-01" style label shown to the user in place of the
  // real backend UUID (`tc.id`) — the UUID itself is untouched and still
  // used for every functional purpose (React key, selection, API calls).
  // Prefers a label already baked into `traceability.testCaseId` (the
  // AI-generation path already sets one, e.g. "AI-001", at draft time) so
  // AI-generated cases keep their existing label; any case without one
  // (e.g. a manually-authored case) gets a stable ordinal assigned by
  // real creation order (`generatedAt` ascending — the oldest case is
  // always TC-01, regardless of the current sort/filter), so the label
  // never shifts around when the list is filtered or reordered.
  const displayIdById = useMemo(() => {
    const byAge = [...testCases].sort((a, b) => {
      const ta = a.generatedAt ? Date.parse(a.generatedAt) : 0;
      const tb = b.generatedAt ? Date.parse(b.generatedAt) : 0;
      return ta - tb;
    });
    const map = new Map<string, string>();
    byAge.forEach((tc, idx) => {
      const existing = tc.traceability?.testCaseId?.trim();
      map.set(tc.id, existing || `TC-${String(idx + 1).padStart(2, '0')}`);
    });
    return map;
  }, [testCases]);
  const getDisplayId = (tc: GeneratedTestCase): string => displayIdById.get(tc.id) ?? tc.id;

  // Filtered cases
  const filteredCases = testCases.filter((tc) => {
    if (selectedCategory !== 'ALL' && resolveCategoryId(tc.category) !== selectedCategory) return false;
    if (statusFilter !== 'ALL' && tc.status !== statusFilter) return false;
    if (priorityFilter !== 'ALL' && tc.priority !== priorityFilter) return false;
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      return (
        tc.id.toLowerCase().includes(q) ||
        getDisplayId(tc).toLowerCase().includes(q) ||
        tc.title.toLowerCase().includes(q) ||
        tc.description?.toLowerCase().includes(q) ||
        tc.objective?.toLowerCase().includes(q) ||
        tc.requirementReference?.toLowerCase().includes(q)
      );
    }
    return true;
  });

  const approvedCount = testCases.filter((tc) => tc.status === 'APPROVED').length;
  const pendingCount = testCases.filter((tc) => tc.status === 'PENDING_REVIEW').length;
  const rejectedCount = testCases.filter((tc) => tc.status === 'REJECTED').length;

  // Compute Technical Target accurately from steps without conflating with Category
  const technicalTargetInfo = useMemo(() => {
    if (!activeTestCase) {
      return { layer: 'Web UI / Application', endpoint: null, description: 'User interface & front-end components' };
    }

    const stepsText = activeTestCase.steps.map((s) => s.action).join(' ');
    
    // Check for HTTP methods / API endpoints in steps
    const httpMatch = stepsText.match(/\b(GET|POST|PUT|DELETE|PATCH)\s+([/\w\-.:]+)/i);
    const hasUrl = stepsText.match(/https?:\/\/[^\s]+/i) || stepsText.includes('/api/') || stepsText.includes('/oauth/');

    if (httpMatch) {
      return {
        layer: 'REST API / HTTP Endpoint',
        endpoint: `${httpMatch[1].toUpperCase()} ${httpMatch[2]}`,
        description: `Targeted HTTP operation (${httpMatch[1].toUpperCase()} ${httpMatch[2]}) executing within the ${activeTestCase.category} test scenario.`,
      };
    } else if (hasUrl) {
      return {
        layer: 'HTTP / Web Service',
        endpoint: 'HTTP Service Request',
        description: `Web service integration executed as part of the ${activeTestCase.category} test specification.`,
      };
    } else if (stepsText.match(/\b(click|select|navigate|input|button|field|modal|dropdown|page|screen|form)\b/i)) {
      return {
        layer: 'Web UI / Browser DOM',
        endpoint: null,
        description: `Browser DOM interactions, form elements, and view components.`,
      };
    } else if (stepsText.match(/\b(database|sql|query|table|record|db)\b/i)) {
      return {
        layer: 'Database / Storage Layer',
        endpoint: null,
        description: `Data store query and persistence assertions.`,
      };
    }

    return {
      layer: 'Application Interface',
      endpoint: null,
      description: `Target system runtime components.`,
    };
  }, [activeTestCase]);

  const getPriorityBadge = (priority: TestCasePriority) => {
    switch (priority) {
      case 'CRITICAL':
        return (
          <span className="px-2.5 py-1 rounded text-[11px] font-mono font-bold bg-rose-950/90 border border-rose-600/80 text-rose-300 shadow-sm">
            P0 • CRITICAL
          </span>
        );
      case 'HIGH':
        return (
          <span className="px-2.5 py-1 rounded text-[11px] font-mono font-bold bg-orange-950/90 border border-orange-600/80 text-orange-300">
            P1 • HIGH
          </span>
        );
      case 'MEDIUM':
        return (
          <span className="px-2.5 py-1 rounded text-[11px] font-mono font-semibold bg-amber-950/90 border border-amber-600/80 text-amber-300">
            P2 • MEDIUM
          </span>
        );
      case 'LOW':
      default:
        return (
          <span className="px-2.5 py-1 rounded text-[11px] font-mono font-medium bg-slate-900 border border-slate-700 text-slate-400">
            P3 • LOW
          </span>
        );
    }
  };

  const getSeverityBadge = (sev?: TestCaseSeverity) => {
    if (!sev) return null;
    return (
      <span className="px-2.5 py-1 rounded text-[11px] font-mono font-bold bg-slate-900/90 border border-slate-700 text-slate-300 uppercase tracking-wider">
        {sev} SEVERITY
      </span>
    );
  };

  const getStatusBadge = (status: GeneratedTestCase['status']) => {
    switch (status) {
      case 'APPROVED':
        return (
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono font-bold bg-emerald-950/90 border border-emerald-600 text-emerald-300 shadow-sm">
            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
            <span>Approved</span>
          </span>
        );
      case 'REJECTED':
        return (
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono font-bold bg-rose-950/90 border border-rose-600 text-rose-300 shadow-sm">
            <XCircle className="w-3.5 h-3.5 text-rose-400" />
            <span>Rejected</span>
          </span>
        );
      case 'EXPORTED_TO_EXECUTION':
        return (
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono font-bold bg-indigo-950/90 border border-indigo-600 text-indigo-300 shadow-sm">
            <Play className="w-3.5 h-3.5 text-indigo-400 fill-current" />
            <span>Dispatched to Runner</span>
          </span>
        );
      case 'EXECUTED':
        return (
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono font-bold bg-indigo-950/90 border border-indigo-600 text-indigo-300 shadow-sm">
            <Play className="w-3.5 h-3.5 text-indigo-400 fill-current" />
            <span>Executed</span>
          </span>
        );
      case 'VALIDATED':
        return (
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono font-bold bg-cyan-950/90 border border-cyan-600 text-cyan-300 shadow-sm">
            <CheckSquare className="w-3.5 h-3.5 text-cyan-400" />
            <span>Validated</span>
          </span>
        );
      case 'PENDING_REVIEW':
      default:
        return (
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-mono font-semibold bg-amber-950/80 border border-amber-600 text-amber-300">
            <Clock className="w-3.5 h-3.5 text-amber-400" />
            <span>Pending Review</span>
          </span>
        );
    }
  };

  // AUD-005 — real backend fetch states, checked before the empty-draft
  // state below (a real, in-flight/erroring fetch is a different state
  // than "nothing has ever been generated").
  if (casesLoading) {
    return (
      <div id="generated-test-cases-loading" className="p-12 rounded-lg bg-[#0B1120] border border-slate-800 text-center flex flex-col items-center justify-center space-y-3 min-h-[440px]">
        <RefreshCw className="w-8 h-8 text-indigo-400 animate-spin" />
        <p className="text-xs font-mono text-slate-400">Loading saved test cases…</p>
      </div>
    );
  }
  if (casesError) {
    return (
      <div id="generated-test-cases-error" className="p-12 rounded-lg bg-[#0B1120] border border-rose-800/60 text-center flex flex-col items-center justify-center space-y-3 min-h-[440px]">
        <XCircle className="w-8 h-8 text-rose-400" />
        <p className="text-xs font-mono text-rose-300 max-w-md">{casesError}</p>
      </div>
    );
  }

  // If no test cases exist yet
  if (testCases.length === 0) {
    return (
      <div
        id="generated-test-cases-empty-workspace"
        className="p-12 rounded-lg bg-[#0B1120] border border-slate-800 text-center flex flex-col items-center justify-center space-y-4 min-h-[440px]"
      >
        <div className="w-16 h-16 rounded-full bg-indigo-950/60 border border-indigo-700/60 flex items-center justify-center text-indigo-400 shadow-lg">
          <Sparkles className="w-8 h-8" />
        </div>
        <div className="space-y-1.5 max-w-md">
          <h3 className="text-base font-bold font-mono text-slate-100 tracking-wider">
            NO TEST CASES GENERATED YET
          </h3>
          <p className="text-xs text-slate-400 font-sans leading-relaxed">
            Upload product documentation (PRD, FRD, SRD), connect your target application, and select testing strategies to run autonomous AI test case generation.
          </p>
        </div>
        <button
          type="button"
          onClick={onNavigateToSources}
          className="flex items-center gap-2 px-5 py-2.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-xs font-mono shadow-md cursor-pointer transition-all mt-2"
        >
          <span>Configure Sources & Strategies</span>
          <ArrowRight className="w-4 h-4" />
        </button>
      </div>
    );
  }

  return (
    <div id="generated-test-cases-workspace" className="space-y-4 w-full">
      {/* 1. Top Suite Toolbar & Summary Ribbon */}
      <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 flex flex-col md:flex-row md:items-center justify-between gap-4 shadow-sm">
        <div className="flex items-center gap-3.5 flex-wrap">
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded bg-indigo-950/80 border border-indigo-700/60 text-indigo-400">
              <Sparkles className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-sm font-bold text-slate-100 font-sans tracking-wide">
                Generated Test Suite ({testCases.length} Cases)
              </h2>
              <p className="text-[11px] text-slate-400 font-sans">
                Review requirements, verify execution steps, and locally approve or reject each generated case.
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2 text-xs font-mono">
            <span className="px-2.5 py-1 rounded bg-emerald-950/80 border border-emerald-700 text-emerald-300 font-semibold">
              {approvedCount} Approved
            </span>
            <span className="px-2.5 py-1 rounded bg-amber-950/80 border border-amber-700 text-amber-300 font-semibold">
              {pendingCount} Pending Review
            </span>
            {rejectedCount > 0 && (
              <span className="px-2.5 py-1 rounded bg-rose-950/80 border border-rose-700 text-rose-300 font-semibold">
                {rejectedCount} Rejected
              </span>
            )}
          </div>
        </div>

        {/* Global Suite Actions */}
        <div className="flex items-center gap-2.5 flex-wrap">
          {canSave && (
            <button
              type="button"
              id="btn-save-to-project"
              onClick={onSaveToProject}
              disabled={saveState === 'SAVING'}
              className="flex items-center gap-1.5 px-3.5 py-2 rounded-md bg-cyan-600 hover:bg-cyan-500 disabled:opacity-60 disabled:cursor-not-allowed text-white font-mono text-xs font-bold cursor-pointer transition-colors shadow-sm"
            >
              {saveState === 'SAVING' ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <CheckSquare className="w-3.5 h-3.5" />}
              <span>{saveState === 'SAVING' ? 'Saving…' : 'Save to Project'}</span>
            </button>
          )}

          {pendingCount > 0 && (
            <button
              type="button"
              onClick={onApproveAll}
              className="flex items-center gap-1.5 px-3.5 py-2 rounded-md bg-[#020617] hover:bg-slate-900 border border-emerald-600/80 text-emerald-300 font-mono text-xs font-semibold cursor-pointer transition-colors shadow-sm"
            >
              <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
              <span>{hasPersisted ? `Validate + Approve All Pending (${pendingCount})` : `Approve All Pending (${pendingCount})`}</span>
            </button>
          )}

          <button
            type="button"
            id="btn-open-execution-hub"
            onClick={onOpenExecutionHub}
            title="Opens the real Execution Hub."
            className="flex items-center gap-2 px-4 py-2 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-mono text-xs font-bold cursor-pointer transition-all shadow-md"
          >
            <Play className="w-3.5 h-3.5 fill-current" />
            <span>Open Execution Hub</span>
          </button>
        </div>
      </div>

      {saveErrorMessage && (
        <p id="save-error-message" className="px-3 py-2 rounded bg-rose-950/60 border border-rose-800 text-[11px] font-mono text-rose-300">
          {saveErrorMessage}
        </p>
      )}

      {/* AUD-005: once real, persisted cases exist, this page is showing
          and actioning them for real — the FE-16 local-only disclosure
          only applies to a still-unsaved draft. */}
      <p className="px-1 text-[11px] font-mono text-slate-500">
        {hasPersisted
          ? 'These test cases are saved to the project. Validate and approve a case, then hand it off to the real Execution Hub below.'
          : 'Generated cases stay local to this page — opening the Execution Hub does not send them to the backend or create a run.'}
      </p>

      {/* 2. Horizontal Category Filter Navigation Bar */}
      <div className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-2">
        <div className="flex items-center justify-between text-xs font-mono font-bold text-slate-300 px-1">
          <div className="flex items-center gap-2">
            <Layers className="w-3.5 h-3.5 text-cyan-400" />
            <span>Filter by Test Category</span>
          </div>
          <span className="text-[11px] text-slate-500 font-normal">
            Showing {filteredCases.length} of {testCases.length} cases
          </span>
        </div>

        {/* Category Filter Chips / Pills */}
        <div className="flex items-center gap-1.5 overflow-x-auto pb-1 pt-0.5 scrollbar-thin scrollbar-thumb-slate-700">
          {/* ALL option */}
          <button
            type="button"
            onClick={() => setSelectedCategory('ALL')}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-mono whitespace-nowrap transition-all cursor-pointer shrink-0 ${
              selectedCategory === 'ALL'
                ? 'bg-indigo-600 text-white font-bold shadow-sm'
                : 'bg-[#020617] hover:bg-slate-900 border border-slate-800 text-slate-300'
            }`}
          >
            <span>All Categories</span>
            <span
              className={`text-[10px] px-1.5 py-0.2 rounded font-bold ${
                selectedCategory === 'ALL' ? 'bg-indigo-900 text-indigo-100' : 'bg-slate-800 text-slate-300'
              }`}
            >
              {testCases.length}
            </span>
          </button>

          {/* 14 Individual Categories */}
          {TEST_CATEGORIES.map((cat) => {
            const count = testCases.filter((tc) => resolveCategoryId(tc.category) === cat.id).length;
            const isSelected = selectedCategory === cat.id;
            const Icon = cat.icon;

            return (
              <button
                key={cat.id}
                type="button"
                onClick={() => setSelectedCategory(cat.id)}
                className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-mono whitespace-nowrap transition-all cursor-pointer shrink-0 ${
                  isSelected
                    ? 'bg-indigo-600 text-white font-bold shadow-sm'
                    : 'bg-[#020617] hover:bg-slate-900 border border-slate-800/80 text-slate-400 hover:text-slate-200'
                }`}
              >
                <Icon className={`w-3.5 h-3.5 ${isSelected ? 'text-white' : cat.accentColor} shrink-0`} />
                <span>{cat.title}</span>
                <span
                  className={`text-[10px] px-1.5 py-0.2 rounded font-mono font-bold ${
                    isSelected
                      ? 'bg-indigo-900 text-indigo-100'
                      : count > 0
                      ? 'bg-indigo-950/80 text-indigo-300 border border-indigo-800/80'
                      : 'bg-slate-900 text-slate-600'
                  }`}
                >
                  {count}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* 3. Main Workspace: CSS Grid Layout Expanding Width (Compact List + Expansive Full-Width Detail Section) */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start w-full">
        {/* Left Column: Compact Search, Filters & Test Case List (3.5 cols on xl, 4 cols on lg) */}
        <div className="lg:col-span-4 xl:col-span-3 space-y-3 p-3.5 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm">
          {/* Search Bar & Dropdown Filters */}
          <div className="space-y-2.5 pb-2.5 border-b border-slate-800">
            <div className="relative">
              <Search className="w-4 h-4 text-slate-500 absolute left-3 top-1/2 -translate-y-1/2" />
              <input
                type="text"
                placeholder="Search by ID, title, objective..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="w-full pl-9 pr-3 py-2 rounded-md bg-[#020617] border border-slate-700/80 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500 transition-colors"
              />
            </div>

            <div className="grid grid-cols-2 gap-2">
              <div>
                <label className="block text-[10px] font-mono text-slate-400 mb-1">Status Filter</label>
                <select
                  value={statusFilter}
                  onChange={(e) => setStatusFilter(e.target.value)}
                  className="w-full px-2 py-1.5 rounded bg-[#020617] border border-slate-700/80 text-xs font-mono text-slate-200 focus:outline-none focus:border-indigo-500 cursor-pointer"
                >
                  <option value="ALL">All Statuses</option>
                  <option value="PENDING_REVIEW">Pending Review</option>
                  <option value="APPROVED">Approved</option>
                  <option value="REJECTED">Rejected</option>
                </select>
              </div>

              <div>
                <label className="block text-[10px] font-mono text-slate-400 mb-1">Priority Filter</label>
                <select
                  value={priorityFilter}
                  onChange={(e) => setPriorityFilter(e.target.value)}
                  className="w-full px-2 py-1.5 rounded bg-[#020617] border border-slate-700/80 text-xs font-mono text-slate-200 focus:outline-none focus:border-indigo-500 cursor-pointer"
                >
                  <option value="ALL">All Priorities</option>
                  <option value="CRITICAL">P0 • Critical</option>
                  <option value="HIGH">P1 • High</option>
                  <option value="MEDIUM">P2 • Medium</option>
                  <option value="LOW">P3 • Low</option>
                </select>
              </div>
            </div>
          </div>

          {/* Test Case Cards List */}
          <div className="space-y-2 max-h-[700px] overflow-y-auto pr-1">
            {filteredCases.length === 0 ? (
              <div className="p-8 text-center text-xs font-mono text-slate-500 bg-[#020617] rounded border border-slate-800/80">
                No matching test cases found.
              </div>
            ) : (
              filteredCases.map((tc) => {
                const isSelected = activeTestCase?.id === tc.id;
                return (
                  <div
                    key={tc.id}
                    id={`test-case-item-${tc.id.toLowerCase()}`}
                    onClick={() => setSelectedTestCaseId(tc.id)}
                    className={`p-3.5 rounded-lg border text-left cursor-pointer transition-all ${
                      isSelected
                        ? 'bg-gradient-to-r from-indigo-950/70 to-[#020617] border-indigo-500 shadow-md ring-1 ring-indigo-500/30'
                        : 'bg-[#020617] border-slate-800/90 hover:border-slate-700 hover:bg-[#03091e]'
                    }`}
                  >
                    <div className="flex items-center justify-between gap-1.5 mb-1.5">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-mono font-bold text-cyan-400 tracking-wide">
                          {getDisplayId(tc)}
                        </span>
                        <span className="text-[10px] font-mono text-indigo-300 bg-indigo-950/60 px-1.5 py-0.2 rounded border border-indigo-800/60 truncate max-w-[110px]">
                          {tc.category}
                        </span>
                      </div>
                      <div className="flex items-center gap-1.5">
                        {getPriorityBadge(tc.priority)}
                      </div>
                    </div>

                    <h4 className="text-xs font-bold font-sans text-slate-100 line-clamp-2 leading-snug">
                      {tc.title}
                    </h4>

                    {tc.objective && (
                      <p className="text-[11px] text-slate-400 font-sans line-clamp-1 mt-1">
                        {tc.objective}
                      </p>
                    )}

                    <div className="flex items-center justify-between text-[10px] font-mono text-slate-400 mt-2.5 pt-2 border-t border-slate-800/60">
                      <span className="text-slate-400 truncate max-w-[140px]">
                        {tc.requirementReference || '§3.1 Spec'}
                      </span>
                      <div>{getStatusBadge(tc.status)}</div>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* Right Column: Refactored CSS Grid Layout for Test Case Detail Section (8 cols on lg, 9 on xl) */}
        <div className="lg:col-span-8 xl:col-span-9 p-6 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm w-full space-y-6">
          {activeTestCase ? (
            <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 w-full items-start">
              
              {/* ======================================================== */}
              {/* 1. TOP HEADER & METADATA SPAN (Full Width 12 cols) */}
              {/* ======================================================== */}
              <div id="test-case-detail-header" className="xl:col-span-12 space-y-4 pb-5 border-b border-slate-800/90">
                {/* Top Row: First-Class Test Case ID & Status Badges */}
                <div className="flex items-center justify-between gap-3 flex-wrap">
                  {/* First-Class Test Case ID Banner */}
                  <div className="flex items-center gap-3">
                    <div className="px-3.5 py-1.5 rounded-lg bg-indigo-950/90 border border-indigo-500/80 shadow-md">
                      <span className="text-[9px] font-mono uppercase tracking-widest text-indigo-300 block font-semibold">
                        Test Case ID
                      </span>
                      <span className="text-sm font-mono font-bold text-cyan-300 tracking-wide">
                        {getDisplayId(activeTestCase)}
                      </span>
                    </div>

                    {/* Category Badge strictly from Metadata */}
                    <div className="px-3.5 py-1.5 rounded-lg bg-slate-900 border border-slate-700">
                      <span className="text-[9px] font-mono uppercase tracking-widest text-slate-400 block font-semibold">
                        Category
                      </span>
                      <span className="text-xs font-mono font-bold text-slate-100">
                        {activeTestCase.category}
                      </span>
                    </div>
                  </div>

                  {/* Priority, Severity & Review Status */}
                  <div className="flex items-center gap-2 flex-wrap">
                    {getPriorityBadge(activeTestCase.priority)}
                    {getSeverityBadge(activeTestCase.severity)}
                    {getStatusBadge(activeTestCase.status)}
                    {onEditTestCase && (
                      <button
                        type="button"
                        id="btn-edit-test-case"
                        onClick={() => setEditingCase(activeTestCase)}
                        className="flex items-center gap-1 px-2.5 py-1 rounded border border-slate-700 hover:border-indigo-600 text-slate-400 hover:text-indigo-300 font-mono text-[10px] font-semibold cursor-pointer transition-colors"
                      >
                        <Pencil className="w-3 h-3" />
                        <span>Edit</span>
                      </button>
                    )}
                  </div>
                </div>

                {/* Title */}
                <div>
                  <h3 className="text-lg md:text-xl font-bold font-sans text-slate-100 leading-tight">
                    {activeTestCase.title}
                  </h3>
                </div>

                {/* Objective */}
                {activeTestCase.objective && (
                  <div className="p-3.5 rounded-lg bg-[#020617] border border-slate-800/90 text-xs font-sans text-slate-300">
                    <span className="font-bold text-indigo-300 uppercase font-mono text-[10px] tracking-wider block mb-1">
                      Test Objective
                    </span>
                    {activeTestCase.objective}
                  </div>
                )}
              </div>

              {/* ======================================================== */}
              {/* 2. SOURCE & TRACEABILITY SPAN (Full Width 12 cols) */}
              {/* ======================================================== */}
              <div id="test-case-source-traceability" className="xl:col-span-12 p-4 rounded-lg bg-[#040817] border border-slate-800 space-y-3">
                <div className="flex items-center gap-2 text-xs font-mono font-bold text-slate-200">
                  <Bookmark className="w-4 h-4 text-cyan-400" />
                  <span className="tracking-wide">SOURCE SPECIFICATION & ACCEPTANCE CRITERIA</span>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5 text-xs font-mono">
                  <div className="p-3 rounded bg-[#020617] border border-slate-800/90">
                    <span className="text-[10px] text-slate-500 uppercase tracking-wider block mb-0.5 font-semibold">
                      Source Document
                    </span>
                    <span className="text-indigo-300 font-semibold text-xs">
                      {activeTestCase.sourceDocument || 'PRD / FRD / Specification'}
                    </span>
                  </div>

                  <div className="p-3 rounded bg-[#020617] border border-slate-800/90">
                    <span className="text-[10px] text-slate-500 uppercase tracking-wider block mb-0.5 font-semibold">
                      Requirement Reference
                    </span>
                    <span className="text-cyan-300 font-semibold text-xs">
                      {activeTestCase.requirementReference || '§3.1 Spec'}
                    </span>
                  </div>
                </div>

                {activeTestCase.sourceRequirement && (
                  <div className="rounded bg-[#020617] border border-slate-800/80">
                    <button
                      type="button"
                      id="btn-toggle-source-excerpt"
                      onClick={() => setShowSourceExcerpt(!showSourceExcerpt)}
                      className="w-full flex items-center justify-between gap-2 p-3 text-left cursor-pointer"
                    >
                      <span className="text-[10px] font-mono text-slate-400 font-semibold uppercase tracking-wider">
                        Document Excerpt
                      </span>
                      <span className="text-[11px] font-mono text-indigo-400 hover:text-indigo-300 shrink-0">
                        {showSourceExcerpt ? 'Collapse' : 'Expand'}
                      </span>
                    </button>

                    {showSourceExcerpt && (
                      <div className="text-xs text-slate-300 px-3 pb-3 italic font-sans">
                        "{activeTestCase.sourceRequirement}"
                      </div>
                    )}
                  </div>
                )}
              </div>

              {/* ======================================================== */}
              {/* 3. SECTION: PRECONDITIONS (Explicit Header & Clear Separators) */}
              {/* ======================================================== */}
              <div id="test-case-preconditions-section" className="xl:col-span-12 space-y-3 pt-2">
                {/* Clear Section Header with Spacing Separator */}
                <div className="flex items-center gap-3 pb-2 border-b border-slate-800">
                  <div className="p-1.5 rounded bg-amber-950/60 border border-amber-700/60 text-amber-400">
                    <Shield className="w-4 h-4" />
                  </div>
                  <div className="flex-1 flex items-center justify-between">
                    <div>
                      <h4 className="text-xs font-mono font-bold text-slate-100 uppercase tracking-wider">
                        Preconditions
                      </h4>
                      <p className="text-[11px] text-slate-400 font-sans">
                        Environmental setup, state requisites, and active user context
                      </p>
                    </div>
                    <span className="text-[10px] font-mono text-slate-400 bg-[#020617] px-2.5 py-1 rounded border border-slate-800">
                      {activeTestCase.preconditions?.length || 0} Defined
                    </span>
                  </div>
                </div>

                {/* Preconditions Content Card */}
                <div className="p-4 rounded-lg bg-[#020617] border border-slate-800/90 shadow-inner">
                  {activeTestCase.preconditions && activeTestCase.preconditions.length > 0 ? (
                    <ul className="space-y-2 text-slate-300 text-xs font-sans">
                      {activeTestCase.preconditions.map((p, idx) => (
                        <li key={idx} className="flex items-start gap-2.5">
                          <CheckSquare className="w-4 h-4 text-cyan-400 shrink-0 mt-0.5" />
                          <span className="leading-relaxed">{p}</span>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="text-xs text-slate-500 font-sans italic">
                      No special preconditions required for this test scenario.
                    </p>
                  )}
                </div>
              </div>

              {/* ======================================================== */}
              {/* 4. SECTION: EXECUTION STEPS (Explicit Header & Clear Separators) */}
              {/* ======================================================== */}
              <div id="test-case-execution-steps-section" className="xl:col-span-12 space-y-3 pt-2">
                {/* Clear Section Header with Spacing Separator */}
                <div className="flex items-center gap-3 pb-2 border-b border-slate-800">
                  <div className="p-1.5 rounded bg-indigo-950/60 border border-indigo-700/60 text-indigo-400">
                    <FileCode2 className="w-4 h-4" />
                  </div>
                  <div className="flex-1 flex items-center justify-between">
                    <div>
                      <h4 className="text-xs font-mono font-bold text-slate-100 uppercase tracking-wider">
                        Execution Steps
                      </h4>
                      <p className="text-[11px] text-slate-400 font-sans">
                        Step-by-step procedural actions and expected assertions
                      </p>
                    </div>
                    <span className="text-[10px] font-mono text-indigo-300 bg-indigo-950/80 px-2.5 py-1 rounded border border-indigo-800">
                      {activeTestCase.steps.length} Steps
                    </span>
                  </div>
                </div>

                {/* Steps CSS Grid Layout */}
                <div className="space-y-3">
                  {activeTestCase.steps.map((step) => (
                    <div
                      key={step.stepNumber}
                      className="p-4 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5 hover:border-slate-700 transition-colors shadow-sm"
                    >
                      <div className="flex items-center justify-between">
                        <span className="px-2.5 py-0.5 rounded bg-indigo-950 border border-indigo-700 text-xs font-mono font-bold text-indigo-300">
                          Step #{step.stepNumber}
                        </span>
                        <span className="text-[10px] font-mono text-slate-500">
                          USER ACTION & ASSERTION
                        </span>
                      </div>

                      <div className="text-xs font-sans text-slate-200 leading-relaxed pl-1">
                        <strong className="text-slate-400 font-mono text-[11px] block mb-0.5">
                          Action:
                        </strong>
                        {step.action}
                      </div>

                      <div className="text-xs font-sans text-emerald-300 pt-2 border-t border-slate-800/80 bg-emerald-950/20 p-2.5 rounded border border-emerald-900/40">
                        <strong className="text-emerald-400 font-mono text-[11px] block mb-0.5">
                          Expected Result:
                        </strong>
                        {step.expectedResult}
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              {/* ======================================================== */}
              {/* 5. DUAL COLUMN SUB-GRID: Technical Target & Execution State */}
              {/* ======================================================== */}
              <div className="xl:col-span-6 p-4 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5">
                <div className="flex items-center justify-between pb-1.5 border-b border-slate-800/80">
                  <div className="flex items-center gap-2">
                    <Cpu className="w-3.5 h-3.5 text-cyan-400" />
                    <span className="text-xs font-mono font-bold text-slate-200 uppercase tracking-wider">
                      Technical Target Interface
                    </span>
                  </div>
                  <span className="text-[10px] font-mono text-slate-500">
                    Surface
                  </span>
                </div>

                <div className="flex items-center gap-2 flex-wrap pt-0.5">
                  <span className="px-2.5 py-1 rounded bg-[#040817] border border-slate-700 text-xs font-mono font-semibold text-cyan-300">
                    Layer: {technicalTargetInfo.layer}
                  </span>

                  {technicalTargetInfo.endpoint && (
                    <span className="px-2.5 py-1 rounded bg-slate-900 border border-slate-700 text-xs font-mono text-amber-300">
                      Endpoint: <code className="font-bold">{technicalTargetInfo.endpoint}</code>
                    </span>
                  )}
                </div>

                <div className="p-2.5 rounded bg-[#040817] border border-slate-800/80 text-[11px] font-sans text-slate-400 flex items-start gap-2">
                  <Info className="w-3.5 h-3.5 text-indigo-400 shrink-0 mt-0.5" />
                  <span>
                    Classification: <strong className="text-indigo-300">{activeTestCase.category}</strong>. Endpoint represents the technical surface.
                  </span>
                </div>
              </div>

              <div className="xl:col-span-6 p-4 rounded-lg bg-[#020617] border border-slate-800 space-y-2.5">
                <div className="flex items-center justify-between pb-1.5 border-b border-slate-800/80">
                  <div className="flex items-center gap-2">
                    <Activity className="w-3.5 h-3.5 text-indigo-400" />
                    <span className="text-xs font-mono font-bold text-slate-200 uppercase tracking-wider">
                      Execution State
                    </span>
                  </div>
                  <div>
                    {activeTestCase.status === 'EXPORTED_TO_EXECUTION' ? (
                      <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-indigo-950 text-indigo-300 border border-indigo-700">
                        DISPATCHED / QUEUED
                      </span>
                    ) : activeTestCase.status === 'APPROVED' ? (
                      <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-cyan-950 text-cyan-300 border border-cyan-700">
                        READY FOR DISPATCH
                      </span>
                    ) : (
                      <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-slate-900 text-slate-400 border border-slate-700">
                        NOT STARTED
                      </span>
                    )}
                  </div>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs font-mono">
                  <div className="p-2.5 rounded bg-[#040817] border border-slate-800">
                    <span className="text-[10px] text-slate-500 block mb-0.5">Status</span>
                    <span className="text-slate-200 font-semibold">
                      {activeTestCase.status === 'EXPORTED_TO_EXECUTION'
                        ? 'Dispatched to Runner'
                        : activeTestCase.status === 'APPROVED'
                        ? 'Ready for Dispatch'
                        : 'Awaiting Review'}
                    </span>
                  </div>

                  <div className="p-2.5 rounded bg-[#040817] border border-slate-800">
                    <span className="text-[10px] text-slate-500 block mb-0.5">Runner Pool</span>
                    <span className="text-slate-300 font-semibold truncate block">
                      {activeTestCase.traceability?.executionTarget || 'k8s-runner-pool-us'}
                    </span>
                  </div>
                </div>
              </div>

              {/* ======================================================== */}
              {/* 6. SECTION: EVIDENCE (Explicit Header & Clear Separators) */}
              {/* ======================================================== */}
              <div id="test-case-evidence-section" className="xl:col-span-12 space-y-3 pt-2">
                {/* Clear Section Header with Spacing Separator */}
                <div className="flex items-center gap-3 pb-2 border-b border-slate-800">
                  <div className="p-1.5 rounded bg-teal-950/60 border border-teal-700/60 text-teal-400">
                    <Camera className="w-4 h-4" />
                  </div>
                  <div className="flex-1 flex items-center justify-between">
                    <div>
                      <h4 className="text-xs font-mono font-bold text-slate-100 uppercase tracking-wider">
                        Evidence
                      </h4>
                      <p className="text-[11px] text-slate-400 font-sans">
                        Captured test execution artifacts, traces, logs, and network recordings
                      </p>
                    </div>
                    <span className="text-[10px] font-mono text-slate-400 bg-[#020617] px-2.5 py-1 rounded border border-slate-800">
                      0 Artifacts
                    </span>
                  </div>
                </div>

                {/* Honest Evidence Empty State */}
                <div className="p-5 rounded-lg bg-[#040817] border border-slate-800/80 text-center space-y-2">
                  <div className="w-9 h-9 rounded-full bg-slate-900 border border-slate-800 text-slate-500 flex items-center justify-center mx-auto">
                    <Camera className="w-4 h-4" />
                  </div>
                  <div className="text-xs font-mono font-bold text-slate-300">
                    No evidence captured yet
                  </div>
                  <p className="text-[11px] text-slate-500 font-sans max-w-md mx-auto leading-relaxed">
                    Execution artifacts (HAR network recordings, Playwright DOM traces, console logs, and step failure screenshots) are captured automatically upon test execution.
                  </p>
                </div>
              </div>

              {/* ======================================================== */}
              {/* 7. TRACEABILITY CHAIN (Full Width 12 cols) */}
              {/* ======================================================== */}
              <div className="xl:col-span-12 pt-2">
                <TraceabilityGraph
                  traceability={activeTestCase.traceability}
                  testCaseId={getDisplayId(activeTestCase)}
                  category={activeTestCase.category}
                  sourceDoc={activeTestCase.sourceDocument}
                  requirementRef={activeTestCase.requirementReference}
                  testCaseStatus={activeTestCase.status}
                  hasEvidence={false}
                />
              </div>

              {/* Optional Test Data Payload Preview (Collapsible) */}
              {activeTestCase.testData && (
                <div className="xl:col-span-12 p-3.5 rounded-lg bg-[#020617] border border-slate-800 space-y-2">
                  <button
                    type="button"
                    onClick={() => setShowTestData(!showTestData)}
                    className="w-full flex items-center justify-between text-xs font-mono font-bold text-slate-300 cursor-pointer"
                  >
                    <div className="flex items-center gap-1.5">
                      <Terminal className="w-3.5 h-3.5 text-cyan-400" />
                      <span>Test Execution Payload & Context Parameters</span>
                    </div>
                    <span className="text-[11px] text-indigo-400 hover:text-indigo-300">
                      {showTestData ? 'Hide JSON' : 'View JSON'}
                    </span>
                  </button>

                  {showTestData && (
                    <pre className="p-3 rounded bg-[#000000] border border-slate-800 text-[11px] font-mono text-cyan-300 overflow-x-auto">
                      {activeTestCase.testData}
                    </pre>
                  )}
                </div>
              )}

              {/* ======================================================== */}
              {/* 8. APPROVAL ACTIONS: Sticky Review Footer (Full Width 12 cols) */}
              {/* ======================================================== */}
              <div id="test-case-approval-actions" className="xl:col-span-12 pt-4 border-t border-slate-800 flex items-center justify-between gap-3 flex-wrap">
                <div className="text-xs font-mono text-slate-400 flex items-center gap-1.5">
                  <span>Current Review Status:</span>
                  {getStatusBadge(activeTestCase.status)}
                </div>

                <div className="flex items-center gap-2.5 flex-wrap">
                  {!hasPersisted ? (
                    <>
                      <button
                        type="button"
                        onClick={() => onRejectTestCase(activeTestCase.id)}
                        className="flex items-center gap-1.5 px-4 py-2 rounded-md bg-rose-950/60 hover:bg-rose-900/80 border border-rose-700 text-rose-300 font-mono text-xs font-bold cursor-pointer transition-colors shadow-sm"
                      >
                        <X className="w-4 h-4" />
                        <span>Reject</span>
                      </button>

                      <button
                        type="button"
                        onClick={() => onApproveTestCase(activeTestCase.id)}
                        className="flex items-center gap-1.5 px-5 py-2 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white font-mono text-xs font-bold cursor-pointer transition-colors shadow-md"
                      >
                        <Check className="w-4 h-4" />
                        <span>Approve Test Case</span>
                      </button>
                    </>
                  ) : (
                    <>
                      {(activeTestCase.status === 'PENDING_REVIEW' || activeTestCase.status === 'VALIDATED') && (
                        <button
                          type="button"
                          onClick={() => onRejectTestCase(activeTestCase.id)}
                          className="flex items-center gap-1.5 px-4 py-2 rounded-md bg-rose-950/60 hover:bg-rose-900/80 border border-rose-700 text-rose-300 font-mono text-xs font-bold cursor-pointer transition-colors shadow-sm"
                        >
                          <X className="w-4 h-4" />
                          <span>Reject</span>
                        </button>
                      )}

                      {activeTestCase.status === 'PENDING_REVIEW' && (
                        <button
                          type="button"
                          id="btn-validate-test-case"
                          onClick={() => onValidateTestCase?.(activeTestCase.id)}
                          className="flex items-center gap-1.5 px-5 py-2 rounded-md bg-cyan-600 hover:bg-cyan-500 text-white font-mono text-xs font-bold cursor-pointer transition-colors shadow-md"
                        >
                          <ShieldCheck className="w-4 h-4" />
                          <span>Validate Test Case</span>
                        </button>
                      )}

                      {activeTestCase.status === 'VALIDATED' && (
                        <button
                          type="button"
                          onClick={() => onApproveTestCase(activeTestCase.id)}
                          className="flex items-center gap-1.5 px-5 py-2 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white font-mono text-xs font-bold cursor-pointer transition-colors shadow-md"
                        >
                          <Check className="w-4 h-4" />
                          <span>Approve Test Case</span>
                        </button>
                      )}

                      {activeTestCase.status === 'APPROVED' && (
                        <div className="flex flex-col items-end gap-2">
                          <div className="flex items-center gap-2">
                            <label className="text-[10px] font-mono text-slate-400" htmlFor="handoff-executor-select">
                              Executor
                            </label>
                            <select
                              id="handoff-executor-select"
                              value={handoffExecutor}
                              onChange={(e) => setHandoffExecutor(e.target.value)}
                              className="px-2 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-mono text-slate-200 cursor-pointer"
                            >
                              {HANDOFF_OPTIONS.map((o) => (
                                <option key={o.value} value={o.value}>
                                  {o.label}
                                </option>
                              ))}
                            </select>
                            {handoffExecutor === COMPILE_OPTION_VALUE && (
                              <button
                                type="button"
                                id="btn-preview-compilation"
                                onClick={() => onCompilePreview?.(activeTestCase.id)}
                                disabled={compilePreviewLoading}
                                className="px-3 py-1.5 rounded bg-slate-800 hover:bg-slate-700 border border-slate-600 text-slate-200 font-mono text-[11px] font-bold cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                              >
                                {compilePreviewLoading && compilePreviewForId === activeTestCase.id ? 'Compiling…' : 'Preview Compilation'}
                              </button>
                            )}
                            {handoffExecutor === '' && (
                              <>
                                <label className="text-[10px] font-mono text-slate-400" htmlFor="handoff-outcome-select">
                                  Simulated outcome
                                </label>
                                <select
                                  id="handoff-outcome-select"
                                  value={handoffOutcome}
                                  onChange={(e) => setHandoffOutcome(e.target.value as typeof handoffOutcome)}
                                  className="px-2 py-1.5 rounded bg-[#020617] border border-slate-700 text-xs font-mono text-slate-200 cursor-pointer"
                                >
                                  {SIMULATE_OUTCOMES.map((o) => (
                                    <option key={o.value} value={o.value}>
                                      {o.label}
                                    </option>
                                  ))}
                                </select>
                              </>
                            )}
                            {(() => {
                              const isCompileMode = handoffExecutor === COMPILE_OPTION_VALUE;
                              const previewIsForThisCase = compilePreviewForId === activeTestCase.id;
                              const isReady = isCompileMode && previewIsForThisCase && compilePreview?.status === 'READY';
                              const compileBlocked = isCompileMode && !isReady;
                              return (
                                <button
                                  type="button"
                                  id="btn-handoff-test-case"
                                  disabled={compileBlocked}
                                  onClick={() =>
                                    onHandoffTestCase?.(
                                      activeTestCase.id,
                                      handoffExecutor === ''
                                        ? { mode: 'simulate', outcome: handoffOutcome }
                                        : isCompileMode
                                          ? { mode: 'compile' }
                                          : { mode: 'executor', executor: handoffExecutor },
                                    )
                                  }
                                  title={
                                    handoffExecutor === ''
                                      ? 'Dispatches through the real Execution Hub with the explicit simulated outcome selected above — no real automation runs.'
                                      : isCompileMode
                                        ? isReady
                                          ? 'Runs the real compiled plan (this case’s own actual content) through a compatible real executor.'
                                          : 'Run "Preview Compilation" first — this case must compile to READY before it can actually execute.'
                                        : `Dispatches a real ${handoffExecutor} run through the real Execution Hub — its own default reference check, not a bespoke script for this case's specific steps.`
                                  }
                                  className="flex items-center gap-1.5 px-5 py-2 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-mono text-xs font-bold cursor-pointer transition-colors shadow-md disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-indigo-600"
                                >
                                  <Play className="w-4 h-4 fill-current" />
                                  <span>Hand Off to Execution</span>
                                </button>
                              );
                            })()}
                          </div>
                          {handoffExecutor === COMPILE_OPTION_VALUE && compilePreviewForId === activeTestCase.id && (
                            <div id="compile-preview-panel" className="max-w-md text-right">
                              {compilePreviewLoading ? (
                                <p className="text-[10px] font-mono text-slate-400">Compiling this case's own content into a real execution plan…</p>
                              ) : compilePreviewError ? (
                                <p className="text-[10px] font-mono text-rose-300/90">Compilation preview failed: {compilePreviewError}</p>
                              ) : compilePreview?.status === 'READY' ? (
                                <div className="text-[10px] font-mono text-emerald-300/90 space-y-0.5">
                                  <p>
                                    READY — {compilePreview.execution_type} plan, {compilePreview.actions.length} action{compilePreview.actions.length === 1 ? '' : 's'}
                                    {compilePreview.target ? `, target ${compilePreview.target}` : ''}.
                                  </p>
                                  <p className="text-slate-400">
                                    Compiled {compilePreview.compiler === 'AI' ? `by AI (${compilePreview.provider ?? 'unknown provider'}/${compilePreview.model ?? 'unknown model'})` : 'deterministically'} from this case's own steps/expected result.
                                  </p>
                                </div>
                              ) : compilePreview?.status === 'NOT_EXECUTABLE' ? (
                                <div className="text-[10px] font-mono text-amber-300/90">
                                  <p>NOT_EXECUTABLE — this case's own content does not state enough to compile a real plan:</p>
                                  <ul className="list-disc list-inside">
                                    {compilePreview.reasons.map((r, i) => (
                                      <li key={i}>{r}</li>
                                    ))}
                                  </ul>
                                </div>
                              ) : compilePreview?.status === 'COMPILATION_FAILED' ? (
                                <div className="text-[10px] font-mono text-rose-300/90">
                                  <p>COMPILATION_FAILED:</p>
                                  <ul className="list-disc list-inside">
                                    {compilePreview.reasons.map((r, i) => (
                                      <li key={i}>{r}</li>
                                    ))}
                                  </ul>
                                </div>
                              ) : compilePreview?.status === 'COMPILATION_UNAVAILABLE' ? (
                                <p className="text-[10px] font-mono text-amber-300/90">COMPILATION_UNAVAILABLE: {compilePreview.reason}</p>
                              ) : null}
                            </div>
                          )}
                          {handoffExecutor !== '' && handoffExecutor !== COMPILE_OPTION_VALUE && (
                            <p className="text-[10px] font-mono text-amber-300/90 max-w-md text-right">
                              Runs {handoffExecutor}'s own default reference check (not this case's specific steps translated into a script)
                              {handoffExecutor === 'maestro' ? ' — requires operator approval before it runs.' : '.'}
                            </p>
                          )}
                        </div>
                      )}
                    </>
                  )}
                </div>
              </div>

              {hasPersisted && (activeTestCase.status === 'APPROVED' || activeTestCase.status === 'EXECUTED') && (
                <p className="xl:col-span-12 text-[11px] font-mono text-slate-500 -mt-2">
                  Real-executor handoffs run that tool's own default reference check, not a bespoke script for this case's specific steps; "Compile &amp; Execute" instead compiles this case's own actual steps/expected result into a real plan and runs that; simulated handoffs report an explicit, disclosed placeholder result. Either way, the real outcome is shown on the Execution Hub page you're taken to next.
                </p>
              )}

              {handoffError && (
                <p className="xl:col-span-12 text-[11px] font-mono text-rose-300 -mt-2">{handoffError}</p>
              )}

            </div>
          ) : (
            <div className="p-12 text-center text-xs font-mono text-slate-500">
              Select a test case from the list on the left to inspect its requirement traceability and execution steps.
            </div>
          )}
        </div>
      </div>

      {editingCase && onEditTestCase && (
        <EditTestCaseModal
          testCase={editingCase}
          onClose={() => setEditingCase(null)}
          onSave={(updates) => onEditTestCase(editingCase.id, updates)}
        />
      )}
    </div>
  );
};
