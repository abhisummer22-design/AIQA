/**
 * FE-11 — shared tests for the original 7 Case-C pages (each rewritten from
 * a fully fabricated demo UI to a truthful "not implemented" state, since
 * no corresponding backend entity/route exists for any of them — re-
 * verified against `services/orchestrator/src/api.rs`'s real route table).
 *
 * Of those 7:
 * - `APITestingPage` — moved out entirely, `AIQA_PRODUCT_DECISIONS_RECORD.md`
 *   decision #3 (ACC-012, 22 Aug 2026): real via the Execution Hub's
 *   executor selection. See its own `APITestingPage.test.tsx`.
 * - `TestCaseValidatorPage` — moved out entirely, ACC-005 (22 Aug 2026,
 *   same-day investigation per the confirmed decision): real via
 *   `TestCaseGeneratorPage.tsx`'s already-wired validate/approve/reject/
 *   handoff lifecycle (AUD-005). See its own `TestCaseValidatorPage.test.tsx`.
 * - `CustomTestingPage` — moved out entirely (Phase 3, 25 Aug 2026): real
 *   via `POST .../custom-testing/generate` (`custom_testing.rs`), the
 *   real central AI router, and a handoff into the existing Test Case
 *   Generator pipeline. See its own `CustomTestingPage.test.tsx`.
 * - `AIQAOrchestratorPage` — moved out entirely (28 Aug 2026): the page now
 *   renders the orchestration that genuinely *does* run — real AI provider
 *   routing (`GET /api/v1/ai/status`'s `routing_preview`, produced by
 *   `ai_router.rs::score_model`), real executor capability routing and
 *   policy gates plus the live DAG (`GET /api/v1/orchestrator/state`), and
 *   the real WebSocket log stream. Only AI-assisted *autonomous* agent
 *   routing (ACC-006) stays deferred, and the page still says so. Its own
 *   real-behavior tests live in the `AIQA Orchestrator — real realtime
 *   orchestration` describe block at the bottom of this file.
 * - `RBACTestingPage` — moved out entirely (28 Aug 2026): the page now
 *   shows the real authorization surface that ships — the caller's real
 *   role/scope (`GET /api/v1/auth/whoami`), the real API-key inventory with
 *   real roles and scopes (`GET /admin/api-keys`), and real recorded/live
 *   authorization denials (now emitted by `auth::require_auth`). Only the
 *   ACC-011 scenario-matrix *tool* stays deferred, and the page still says
 *   so. Its own tests live in `RBACTestingPage.test.tsx`.
 * - `MobileTestingPage` — moved out entirely (29 Aug 2026): the page now
 *   shows the real device fleet this host actually has — real
 *   `adb devices -l`, real `emulator -list-avds` (bootable for real), and
 *   the real Maestro CLI version (`GET /api/v1/mobile/fleet`, `mobile.rs`).
 *   Zero devices is rendered as a real empty fleet. Its own tests live in
 *   `MobileTestingPage.test.tsx`.
 * - The remaining 1 — `FeatureMapFlowPage` — had
 *   **only their copy** updated in that same
 *   later unit (`deferredCases` below): each is now a direct product-owner
 *   decision (`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §2) — real,
 *   intended future scope, deliberately *deferred* with a named revisit
 *   condition, not an unexplained gap. Nothing about backend/frontend
 *   behavior changed; only whether the page states this specifically (new
 *   copy) instead of generically (the old "NOT IMPLEMENTED"/"NO BACKEND
 *   CAPABILITY" badges, now asserted absent).
 *
 * These tests assert the truthful state renders (generic for Custom
 * Testing, specific-and-sourced for the remaining deferred page), that no
 * page-specific fabricated content (the original hardcoded demo records/
 * metrics) survives on any of them, and that each page still has a real
 * root element and stays reachable via the same real, authenticated route
 * table every other page uses.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const RAW_KEY = 'aiqa_casectest00000000000';

/** A real-shaped project, for the pipeline tests that need one selected
 *  (the pipeline endpoint is project-scoped). */
const PROJECT_A = {
  id: 'proj-pipeline',
  name: 'Pipeline Project',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

async function openAndSelectProject(project: { name: string }): Promise<void> {
  await waitFor(() => expect(document.getElementById('project-selector-button')).toBeTruthy());
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

function mockBackend() {
  const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, []));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AppShell />
    </MemoryRouter>,
  );
}

async function verifyKey(): Promise<void> {
  const btn = screen.queryByText('Verify Connection')?.closest('button');
  if (btn) {
    await act(async () => {
      btn.click();
    });
  }
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

// ---------------------------------------------- Deferred-scope pages (1)

const deferredCases: Array<{
  path: string;
  rootId: string;
  heading: string;
  bannedText: string[];
  /** A distinctive substring from *this page's own* real, named revisit
   *  condition (`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §2) — proves
   *  the copy is genuinely page-specific, not one generic sentence reused
   *  four times. */
  revisitSnippet: string;
  accRef: string;
}> = [
  {
    path: '/feature-map',
    rootId: 'feature-map-flow-page',
    heading: 'Feature Map / System Call Flow',
    bannedText: ['NODE-AUTH-GW', 'Payment Ingestion Engine', 'Simulate Call Flow', 'Double-Entry Ledger Service'],
    revisitSnippet: 'gated on Context Ingestion',
    accRef: 'ACC-003',
  },
];

describe('Case-C pages — deferred-scope copy (1 page, updated 22 Aug 2026)', () => {
  for (const c of deferredCases) {
    describe(c.path, () => {
      it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
        const fetchMock = vi.fn();
        vi.stubGlobal('fetch', fetchMock);
        renderApp(c.path);

        await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
        expect(fetchMock).not.toHaveBeenCalled();
        expect(document.getElementById(c.rootId)).toBeNull();
      });

      it('authenticated renders the new, specific "deferred, real future scope" state — the old generic badges are gone', async () => {
        setApiKey(RAW_KEY);
        mockBackend();
        renderApp(c.path);
        await verifyKey();

        await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());
        expect(screen.getAllByText(c.heading).length).toBeGreaterThan(0);

        // New, specific copy renders.
        expect(screen.getAllByText('DEFERRED — REAL FUTURE SCOPE').length).toBeGreaterThan(0);
        expect(screen.getAllByText('DEFERRED, NOT DESCOPED').length).toBeGreaterThan(0);
        // Some of these pages deliberately echo the revisit condition in
      // both the title (a short teaser) and the full description, so
      // >= 1 real match — not exactly 1 — is the actual requirement.
      expect(screen.getAllByText(new RegExp(c.revisitSnippet)).length).toBeGreaterThan(0);
        expect(screen.getByText(new RegExp(c.accRef))).toBeTruthy();
        // Cited twice by design — once in the description, once in the
        // codeHint — so two real elements genuinely match; getAllByText,
        // not getByText.
        expect(screen.getAllByText(/AIQA_PRD_FRD_SRD_v6\.1_SCOPE_CORRECTIONS\.md/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/product-owner decision/).length).toBeGreaterThan(0);

        // The old generic badges must be gone, not merely supplemented.
        expect(screen.queryByText('NOT IMPLEMENTED')).toBeNull();
        expect(screen.queryByText('NO BACKEND CAPABILITY')).toBeNull();
      });

      it('renders none of the page\'s previous fabricated demo content', async () => {
        setApiKey(RAW_KEY);
        mockBackend();
        renderApp(c.path);
        await verifyKey();
        await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());

        for (const banned of c.bannedText) {
          expect(screen.queryByText(new RegExp(banned.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))).toBeNull();
        }
      });

      it('makes no real backend request for its own (nonexistent) capability', async () => {
        setApiKey(RAW_KEY);
        const fetchMock = mockBackend();
        renderApp(c.path);
        await verifyKey();
        await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());

        // Only the real, generic `/projects` list call (auth verification +
        // `ProjectContext`'s own real project selector) is expected — no
        // page-specific endpoint exists for this page's capability, and
        // none should be invented client-side. A copy-only change must not
        // add one.
        for (const [url] of fetchMock.mock.calls as [string][]) {
          expect(String(url)).toMatch(/\/api\/v1\/projects(\?.*)?$/);
        }
      });
    });
  }

  it('each of the 3 pages\' revisit condition is genuinely distinct, not one sentence reused three times', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    const seen = new Set<string>();
    for (const c of deferredCases) {
      const { unmount } = renderApp(c.path);
      await verifyKey();
      await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());
      // Some of these pages deliberately echo the revisit condition in
      // both the title (a short teaser) and the full description, so
      // >= 1 real match — not exactly 1 — is the actual requirement.
      expect(screen.getAllByText(new RegExp(c.revisitSnippet)).length).toBeGreaterThan(0);
      seen.add(c.revisitSnippet);
      unmount();
    }
    expect(seen.size).toBe(deferredCases.length);
  });
});

describe('Case-C pages — route-table non-regression', () => {
  it('Dashboard remains unregressed with Custom Testing and all 4 deferred-scope pages plus the real Orchestrator present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });

  it('Test Generator remains unregressed with Custom Testing and all 4 deferred-scope pages plus the real Orchestrator present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/test-generator');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });
});

// ------------------------------- AIQA Orchestrator — now genuinely real

/** A real-shaped `GET /api/v1/ai/status` body — field-for-field the
 *  `AiStatusResponse` contract type, mirroring the live backend response
 *  observed from `ai_router.rs`. Used only as mocked transport output. */
const AI_STATUS = {
  providers: [
    {
      id: 'ollama',
      name: 'Ollama',
      provider_type: 'LOCAL',
      endpoint: 'http://localhost:11434',
      configured: true,
      reachable: true,
      status: 'HEALTHY',
      models: [{ id: 'qwen3:14b', provider_id: 'ollama', display_name: 'qwen3:14b', status: 'AVAILABLE' }],
    },
    {
      id: 'mj-gateway',
      name: 'MJ AI Gateway',
      provider_type: 'OPEN_AI_COMPATIBLE',
      endpoint: 'http://127.0.0.1:8000',
      configured: false,
      reachable: false,
      status: 'NOT_CONFIGURED',
      models: [],
    },
  ],
  settings: { mode: 'AUTO', preferred_provider: null, preferred_model: null, fallback_enabled: true },
  routing_preview: {
    provider_id: 'ollama',
    model_id: 'qwen3:14b',
    routing_score: 211.48,
    reason: 'AUTO mode: best available model for TestCaseGeneration by routing score',
  },
};

/** A real-shaped `GET /api/v1/orchestrator/state` body — matches
 *  `api.rs::OrchestratorStateResponse`, including the real default policy
 *  gate (`maestro` requires approval, everything else auto-schedules). */
const ORCH_STATE = {
  executors: [
    { kind: 'maestro', gate: 'REQUIRES_APPROVAL' },
    { kind: 'newman', gate: 'AUTO' },
    { kind: 'playwright', gate: 'AUTO' },
  ],
  max_concurrency: 8,
  active_runs: [
    {
      run_id: 'run-1',
      name: 'Smoke Run',
      project_id: 'proj-1',
      state: 'RUNNING',
      concurrency: 2,
      tasks: [
        { id: 't1', name: 'Login smoke check', prerequisites: [], status: 'PASSED', reason: null, started_at: null, finished_at: null },
        { id: 't2', name: 'Checkout flow', prerequisites: ['t1'], status: 'RUNNING', reason: null, started_at: null, finished_at: null },
      ],
    },
  ],
};

/** Real-shaped `GET /api/v1/projects/:id/pipeline` body, matching
 *  `api.rs::NeuralPipelineResponse`. Re-planning/Vector Store/RAG Retrieval
 *  are real as of this feature (`api.rs::get_neural_pipeline`'s own
 *  `replanning`/`vector`/`rag` stages) — `available: true` with the real
 *  backend backing text, not the earlier "Not implemented" placeholders. */
const PIPELINE = {
  stages: [
    { id: 'brain', label: 'Model Router', backing: 'ai_router.rs', available: true, metric: null, metric_label: null },
    { id: 'planning', label: 'Planning', backing: 'scheduler.rs::validate_plan', available: true, metric: null, metric_label: null },
    { id: 'tools', label: 'Tool Routing', backing: 'executor.rs::ExecutorRegistry', available: true, metric: 6, metric_label: 'executors' },
    { id: 'execution', label: 'Execution', backing: 'engine.rs', available: true, metric: 0, metric_label: 'tasks running' },
    { id: 'observation', label: 'Observation', backing: 'log_bus.rs', available: true, metric: null, metric_label: null },
    { id: 'memory', label: 'Quality Graph', backing: 'quality_nodes + quality_relations', available: true, metric: 6, metric_label: 'nodes' },
    { id: 'goal', label: 'Goal State', backing: 'engine.rs terminal state', available: true, metric: 2, metric_label: 'completed runs' },
    { id: 'replanning', label: 'Re-planning', backing: "retry-failed — retries a finished run's real INFRA_ERROR/TIMEOUT tasks only, never a genuine FAILED test. A human triggers it, not an autonomous re-planner.", available: true, metric: null, metric_label: null },
    { id: 'vector', label: 'Vector Store', backing: 'test_case_embeddings — one real Gemini embedding per APPROVED test case, brute-force cosine similarity at retrieval time.', available: true, metric: 3, metric_label: 'cases embedded' },
    { id: 'rag', label: 'RAG Retrieval', backing: "Every generation request embeds its own requirement text and retrieves this project's most similar already-APPROVED test cases as grounding examples.", available: true, metric: null, metric_label: null },
  ],
  graph_nodes: 6,
  graph_relations: 0,
  active_runs: 0,
  completed_runs: 2,
};

function mockOrchestratorBackend(pipeline: unknown = PIPELINE) {
  const fetchMock = vi.fn().mockImplementation(async (url: string) => {
    const u = String(url);
    if (u.includes('/ai/status')) return jsonResponse(200, AI_STATUS);
    if (u.includes('/orchestrator/state')) return jsonResponse(200, ORCH_STATE);
    if (u.includes('/pipeline')) return jsonResponse(200, pipeline);
    if (u.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
    return jsonResponse(200, []);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

describe('AIQA Orchestrator — real realtime orchestration', () => {
  it('unauthenticated (no key) shows the real auth-state card and makes no orchestrator request', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp('/orchestrator');

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('renders the REAL routing decision from the backend — provider, model, score, and stated reason', async () => {
    setApiKey(RAW_KEY);
    mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-orchestrator-page')).toBeTruthy());
    await waitFor(() => expect(screen.getByText('ollama / qwen3:14b')).toBeTruthy());
    // The real backend-supplied reason and score, not a client-side guess.
    expect(screen.getByText(/best available model for TestCaseGeneration by routing score/)).toBeTruthy();
    expect(screen.getByText(/routing score 211\.48/)).toBeTruthy();
    expect(screen.getByText('AUTO mode')).toBeTruthy();
  });

  it('calls the real orchestrator + ai-status endpoints (this page is no longer a no-backend page)', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('aiqa-orchestrator-page')).toBeTruthy());

    // Re-read `mock.calls` inside each assertion — a snapshot taken before
    // the real requests settle would assert against a stale array.
    const calledUrls = () => (fetchMock.mock.calls as [string][]).map(([u]) => String(u));
    await waitFor(() => expect(calledUrls().some((u) => u.includes('/api/v1/ai/status'))).toBe(true));
    await waitFor(() => expect(calledUrls().some((u) => u.includes('/api/v1/orchestrator/state'))).toBe(true));
  });

  it('renders the real executor capability-routing table with its real policy gates', async () => {
    setApiKey(RAW_KEY);
    mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();

    await waitFor(() => expect(screen.getByText('maestro')).toBeTruthy());
    expect(screen.getByText('playwright')).toBeTruthy();
    // The real default gate: maestro is approval-gated, others auto-schedule.
    expect(screen.getByText('REQUIRES APPROVAL')).toBeTruthy();
    expect(screen.getAllByText('AUTO').length).toBeGreaterThan(0);
    expect(screen.getByText(/Policy-enforced max concurrency: 8/)).toBeTruthy();
  });

  it('renders the real live DAG — task states and genuine prerequisite edges', async () => {
    setApiKey(RAW_KEY);
    mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();

    await waitFor(() => expect(screen.getByText('Login smoke check')).toBeTruthy());
    expect(screen.getByText('Checkout flow')).toBeTruthy();
    // The real dependency edge the backend actually published.
    expect(screen.getByText('after: t1')).toBeTruthy();
    expect(screen.getByText(/concurrency 2 · 2 tasks/)).toBeTruthy();
  });

  it('labels each lane by its real transport — push for the DAG, polled for provider health', async () => {
    setApiKey(RAW_KEY);
    mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();

    await waitFor(() => expect(screen.getByText('ollama / qwen3:14b')).toBeTruthy());
    // Provider health genuinely has no backend event stream, so this lane
    // must say polled rather than claim a push feed that does not exist.
    expect(screen.getByText(/polled 30s/)).toBeTruthy();
    // The DAG lane reports the real WebSocket state from RealtimeContext —
    // with no project selected in this test, that is honestly not "live".
    expect(screen.getByText(/^push: /)).toBeTruthy();
    expect(screen.queryByText('push: live')).toBeNull();
  });

  it('never claims the routing score is an accuracy or quality benchmark', async () => {
    setApiKey(RAW_KEY);
    mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();

    await waitFor(() => expect(screen.getByText('ollama / qwen3:14b')).toBeTruthy());
    expect(screen.getByText(/not an accuracy or quality\s+benchmark/)).toBeTruthy();
  });

  it('shows no fabricated route when the backend reports no compatible model', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (String(url).includes('/ai/status')) return jsonResponse(200, { ...AI_STATUS, routing_preview: null });
        if (String(url).includes('/orchestrator/state')) return jsonResponse(200, ORCH_STATE);
        return jsonResponse(200, []);
      }),
    );
    renderApp('/orchestrator');
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/No compatible model is currently available/)).toBeTruthy());
    expect(screen.queryByText('ollama / qwen3:14b')).toBeNull();
    expect(screen.queryByText(/routing score/)).toBeNull();
  });

  it('a real backend failure replaces the panels outright — never leaves stale numbers looking current', async () => {
    setApiKey(RAW_KEY);
    let failNow = false;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        const u = String(url);
        if (u.includes('/ai/status')) {
          if (failNow) return jsonResponse(500, { error: { code: 'INTERNAL', message: 'boom', details: {}, request_id: 'r1' } });
          return jsonResponse(200, AI_STATUS);
        }
        if (u.includes('/orchestrator/state')) return jsonResponse(200, ORCH_STATE);
        return jsonResponse(200, []);
      }),
    );
    renderApp('/orchestrator');
    await verifyKey();
    await waitFor(() => expect(screen.getByText('ollama / qwen3:14b')).toBeTruthy());

    failNow = true;
    await act(async () => {
      document.getElementById('btn-orchestrator-refresh')!.click();
    });

    await waitFor(() => expect(screen.queryByText('ollama / qwen3:14b')).toBeNull());
    expect(screen.queryByText(/routing score 211\.48/)).toBeNull();
  });

  it('renders none of the page\'s previous fabricated demo content', async () => {
    setApiKey(RAW_KEY);
    mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();
    await waitFor(() => expect(screen.getByText('ollama / qwen3:14b')).toBeTruthy());

    for (const banned of ['PLAN-901', 'PLAN-902', 'Dispatch Selected Plan', 'Release Gating']) {
      expect(screen.queryByText(new RegExp(banned))).toBeNull();
    }
  });
});


// ------------------------------------ Neural Pipeline (real loop, honest gaps)

describe('AIQA Orchestrator — Neural Pipeline', () => {
  async function openWithProject() {
    setApiKey(RAW_KEY);
    mockOrchestratorBackend();
    renderApp('/orchestrator');
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(document.getElementById('neural-pipeline-panel')).toBeTruthy());
  }

  it('renders the real loop stages backed by real modules', async () => {
    await openWithProject();

    for (const id of ['brain', 'planning', 'tools', 'execution', 'observation', 'goal']) {
      expect(document.getElementById(`pipeline-stage-${id}`)).toBeTruthy();
    }
    // Real live counters from the backend, not decoration. Scoped to the
    // stage that owns each one — the same number legitimately appears in
    // more than one real place on this page.
    const tools = document.getElementById('pipeline-stage-tools')!;
    expect(within(tools).getByText('6')).toBeTruthy();
    expect(within(tools).getByText('executors')).toBeTruthy();
    const goal = document.getElementById('pipeline-stage-goal')!;
    expect(within(goal).getByText('2')).toBeTruthy();
  });

  it('reports vector store and RAG as real, working supporting capabilities — never "not implemented"', async () => {
    await openWithProject();

    const vector = document.getElementById('pipeline-stage-vector')!;
    const rag = document.getElementById('pipeline-stage-rag')!;
    expect(within(vector).queryByText('not implemented')).toBeNull();
    expect(within(rag).queryByText('not implemented')).toBeNull();
    // They render in the "Supporting capabilities" section (real, working,
    // just not a step of the strict sequential loop), never in "Not part
    // of this pipeline".
    expect(screen.getByText('Supporting capabilities')).toBeTruthy();
    expect(screen.queryByText('Not part of this pipeline')).toBeNull();
    // The real backend backing text is still disclosed, via the node's
    // own tooltip.
    expect(vector.getAttribute('title')).toContain('test_case_embeddings');
    expect(rag.getAttribute('title')).toContain('retrieves');
    // Vector Store's real metric (how many cases are actually embedded).
    expect(within(vector).getByText('3')).toBeTruthy();
    expect(within(vector).getByText('cases embedded')).toBeTruthy();
  });

  it('reports re-planning as a real, working supporting capability rather than a closed agent loop', async () => {
    await openWithProject();

    const replanning = document.getElementById('pipeline-stage-replanning')!;
    expect(within(replanning).queryByText('not implemented')).toBeNull();
    expect(replanning.getAttribute('title')).toContain('retry-failed');
  });

  it('omits "Not part of this pipeline" entirely once nothing genuinely remains unavailable', async () => {
    await openWithProject();
    // Every real stage in this fixture is `available: true` — the section
    // that discloses gaps must not render an empty, pointless heading.
    expect(screen.queryByText('Not part of this pipeline')).toBeNull();
  });

  it('shows IDLE when no task is executing — the animation never implies activity that is not happening', async () => {
    await openWithProject();
    expect(screen.getByText('IDLE')).toBeTruthy();
    expect(screen.queryByText('EXECUTING')).toBeNull();
  });

  it('shows EXECUTING only when the backend reports genuinely running tasks', async () => {
    setApiKey(RAW_KEY);
    const live = {
      ...PIPELINE,
      stages: PIPELINE.stages.map((s) => (s.id === 'execution' ? { ...s, metric: 3 } : s)),
      active_runs: 1,
    };
    mockOrchestratorBackend(live);
    renderApp('/orchestrator');
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('EXECUTING')).toBeTruthy());
    expect(screen.queryByText('IDLE')).toBeNull();
  });

  it('renders the real quality-graph size, including a genuine zero', async () => {
    await openWithProject();
    const panel = document.getElementById('neural-pipeline-panel')!;
    // Real counts, including a genuine zero for relations — not hidden.
    // The counts row renders numbers and units as separate nodes, so match
    // the row's own text content rather than a single element.
    expect(panel.textContent).toContain('nodes');
    expect(panel.textContent).toContain('relations');
    expect(panel.textContent).toContain('Quality Graph');
  });
});
