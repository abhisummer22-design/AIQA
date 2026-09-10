/**
 * FE-10 targeted tests for the real Dashboard — brief §28 items 1-32
 * (item 33, "FE-1 through FE-9 tests remain passing," is satisfied by the
 * full suite run itself, not a dedicated test here). Exercised through the
 * real `AppShell` tree, exactly like FE-6/7/8's own page test files.
 * `fetch` is mocked at the transport boundary; `WebSocket` is mocked with
 * a controllable stand-in (same technique as `RealtimeContext.test.tsx`)
 * since Dashboard now subscribes to the real centralized realtime layer.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { DashboardResponse, KpisResponse, Project, RunListItem, RunEvent } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_dash_test' } };
}

const RAW_KEY = 'aiqa_dashtest00000000000';
const PROJECT_A: Project = { id: 'proj-a', name: 'Project Alpha', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-b', name: 'Project Beta', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };

function dashboardFor(project: Project, overrides: Partial<DashboardResponse> = {}): DashboardResponse {
  return {
    project_id: project.id,
    project_name: project.name,
    execution_counts: {},
    task_counts: {},
    pass_fail: { passed: 0, failed: 0 },
    test_count: 0,
    generated_at: '2026-08-19T10:00:00Z',
    ...overrides,
  };
}
function kpisFor(project: Project, overrides: Partial<KpisResponse> = {}): KpisResponse {
  return {
    project_id: project.id,
    execution_reliability: null,
    execution_efficiency_ms: null,
    execution_avg_run_duration_ms: null,
    generated_at: '2026-08-19T10:00:00Z',
    ...overrides,
  };
}

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  readyState = 0;
  onopen: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onclose: ((ev: { code: number; reason: string }) => void) | null = null;
  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }
  close() {
    this.readyState = 3;
    this.onclose?.({ code: 1000, reason: '' });
  }
  simulateOpen() {
    this.readyState = 1;
    this.onopen?.({});
  }
  simulateMessage(data: unknown) {
    this.onmessage?.({ data: typeof data === 'string' ? data : JSON.stringify(data) });
  }
}

/** The Dashboard legitimately consumes two *different* real streams, not two
 *  copies of one: `RealtimeContext`'s run-event socket
 *  (`/projects/:id/events`, the single centralized connection every page
 *  shares) and the Orchestrator log stream (`/logs/ws`, owned by
 *  `OrchestratorLogsPanel`). These helpers keep the original
 *  "no second connection" invariant assertable per stream — the rule being
 *  protected is "exactly one run-event socket", which is unchanged. */
function eventSockets(): MockWebSocket[] {
  // Only sockets that are still open. A project switch legitimately closes
  // the previous one and opens a new one, so the raw `instances` array
  // accumulates closed sockets — counting those would assert a leak that
  // is not happening.
  return MockWebSocket.instances.filter((ws) => ws.url.includes('/events') && ws.readyState !== 3);
}

function openLogSockets(): MockWebSocket[] {
  return MockWebSocket.instances.filter((ws) => ws.url.includes('/logs/ws') && ws.readyState !== 3);
}

function realEvent(overrides: Partial<RunEvent> = {}): RunEvent {
  return {
    event_id: `evt-${Math.random().toString(36).slice(2)}`,
    run_id: 'run-x',
    task_id: null,
    sequence: 1,
    timestamp: '2026-08-19T10:00:00Z',
    schema_version: 1,
    type: 'TASK_STARTED',
    status: 'RUNNING',
    summary: null,
    project_id: PROJECT_A.id,
    project_sequence: 1,
    ...overrides,
  };
}

type Entry<T> = T | { status: number; body: unknown } | 'offline';

function mockBackend(opts: {
  projects: Project[];
  dashboardByProjectId?: Record<string, Entry<DashboardResponse>>;
  kpisByProjectId?: Record<string, Entry<KpisResponse>>;
  runsByProjectId?: Record<string, RunListItem[]>;
}) {
  const fetchMock = vi.fn().mockImplementation(async (url: string) => {
    if (url.includes('/dashboard')) {
      const m = url.match(/projects\/([^/]+)\/dashboard/);
      const pid = m ? m[1] : '';
      const entry = opts.dashboardByProjectId?.[pid];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && typeof entry === 'object' && 'status' in entry) return jsonResponse(entry.status, entry.body);
      const proj = opts.projects.find((p) => p.id === pid);
      return jsonResponse(200, entry ?? (proj ? dashboardFor(proj) : dashboardFor(PROJECT_A)));
    }
    if (url.includes('/kpis')) {
      const m = url.match(/projects\/([^/]+)\/kpis/);
      const pid = m ? m[1] : '';
      const entry = opts.kpisByProjectId?.[pid];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && typeof entry === 'object' && 'status' in entry) return jsonResponse(entry.status, entry.body);
      const proj = opts.projects.find((p) => p.id === pid);
      return jsonResponse(200, entry ?? (proj ? kpisFor(proj) : kpisFor(PROJECT_A)));
    }
    if (url.includes('/executions')) {
      const m = url.match(/projects\/([^/]+)\/executions/);
      const pid = m ? m[1] : '';
      return jsonResponse(200, opts.runsByProjectId?.[pid] ?? []);
    }
    return jsonResponse(200, opts.projects);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}


/** A real-shaped `GET /api/v1/ai/status` body — field-for-field
 *  `AiStatusResponse`, mirroring what the live backend actually returns
 *  (observed from `ai_router.rs`). Used only as mocked transport output. */
const AI_STATUS_BODY = {
  providers: [
    {
      id: 'ollama',
      name: 'Ollama',
      provider_type: 'LOCAL',
      endpoint: 'http://localhost:11434',
      configured: true,
      reachable: true,
      status: 'HEALTHY',
      models: [
        {
          id: 'qwen3:14b',
          provider_id: 'ollama',
          display_name: 'qwen3:14b',
          family: 'qwen3',
          parameter_size: '14.8B',
          context_window: null,
          status: 'AVAILABLE',
          capabilities: {
            text_generation: 'STRONG',
            structured_output: 'BASIC',
            reasoning: 'BASIC',
            vision: 'NONE',
            embedding: 'NONE',
            tool_use: 'STRONG',
            long_context: 'UNKNOWN',
          },
        },
      ],
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

function mockWithAiStatus(body: unknown = AI_STATUS_BODY) {
  const fetchMock = vi.fn().mockImplementation(async (url: string) => {
    const u = String(url);
    if (u.includes('/ai/status')) return jsonResponse(200, body);
    if (u.includes('/dashboard')) return jsonResponse(200, dashboardFor(PROJECT_A));
    if (u.includes('/kpis')) return jsonResponse(200, kpisFor(PROJECT_A));
    if (u.includes('/executions')) return jsonResponse(200, []);
    if (u.includes('/logs')) return jsonResponse(200, []);
    return jsonResponse(200, [PROJECT_A]);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(initialEntries: string[] = ['/dashboard']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
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

async function openAndSelectProject(project: Project): Promise<void> {
  await waitFor(() => expect(document.getElementById('project-selector-button')).toBeTruthy());
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

function dashboardRoot(): HTMLElement {
  return document.getElementById('aiqa-dashboard-main-content')!;
}

/** Scoped to the KPI cards specifically — the donut chart's legend grid
 *  re-renders the same real numbers as plain text too, so an unscoped
 *  `getByText('3')`-style query against the whole page is ambiguous. */
function kpiRow(): HTMLElement {
  return document.getElementById('dashboard-kpi-row')!;
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  MockWebSocket.instances = [];
  vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
});

afterEach(() => {
  localStorage.clear();
});

describe('DashboardPage (FE-10 §28 items 1-32)', () => {
  // ------------------------------------------------------------- project scope

  it('item 1 — no project selected never fetches project-specific dashboard data', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/dashboard'))).toBe(false);
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/kpis'))).toBe(false);
  });

  it('item 2 — selecting a project loads its real dashboard/kpis data', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 3, FAILED: 1 } }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    expect(within(kpiRow()).getByText('3')).toBeTruthy(); // Passed KPI card
  });

  it('item 3 — switching project invalidates old data before the new project loads', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      dashboardByProjectId: {
        [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 9 } }),
        [PROJECT_B.id]: dashboardFor(PROJECT_B, { task_counts: { PASSED: 2 } }),
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(kpiRow()).getByText('9')).toBeTruthy());

    await openAndSelectProject(PROJECT_B);
    // The old project's KPI value must not remain visible while the new
    // one loads or after it resolves.
    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_B.name).length).toBeGreaterThan(0));
    expect(within(kpiRow()).queryByText('9')).toBeNull();
    expect(within(kpiRow()).getByText('2')).toBeTruthy();
  });

  it('item 4 — Project A metrics never appear under Project B', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      dashboardByProjectId: {
        [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { NEEDS_REVIEW: 42 } }),
        [PROJECT_B.id]: dashboardFor(PROJECT_B, { task_counts: {} }),
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_B);

    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_B.name).length).toBeGreaterThan(0));
    expect(within(kpiRow()).queryByText('42')).toBeNull();
  });

  // ------------------------------------------------------------------- API

  it('item 5/6 — uses the real dashboard/kpis/executions endpoints and parses their real response shape', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 5, FAILED: 2, BLOCKED: 8, NEEDS_REVIEW: 3 }, execution_counts: { RUNNING: 6 } }) },
      kpisByProjectId: { [PROJECT_A.id]: kpisFor(PROJECT_A, { execution_reliability: 0.625, execution_efficiency_ms: 1500 }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(within(kpiRow()).getByText('5')).toBeTruthy()); // Passed
    expect(within(kpiRow()).getByText('2')).toBeTruthy(); // Failed
    expect(within(kpiRow()).getByText('6')).toBeTruthy(); // Active Runs (execution_counts.RUNNING)
    expect(within(kpiRow()).getByText(/62\.5% reliability/)).toBeTruthy(); // execution_reliability, real KPI
    expect(within(dashboardRoot()).getByText('1.5s')).toBeTruthy(); // execution_efficiency_ms formatted, rendered below the KPI row
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes(`/projects/${PROJECT_A.id}/dashboard`))).toBe(true);
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes(`/projects/${PROJECT_A.id}/kpis`))).toBe(true);
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes(`/projects/${PROJECT_A.id}/executions`))).toBe(true);
  });

  it('item 7 — a real 401 on the dashboard endpoint renders the unauthorized state, not empty/fake data', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: { status: 401, body: envelope('UNAUTHENTICATED', 'revoked') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Authentication required')).toBeTruthy());
    expect(screen.queryByText('Executed Tests')).toBeNull();
  });

  it('item 8 — a real 403 on the dashboard endpoint renders the forbidden state, never fake metric values', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: { status: 403, body: envelope('FORBIDDEN', "role 'viewer' cannot perform this action") } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Not permitted to view this dashboard')).toBeTruthy());
    expect(screen.queryByText('Executed Tests')).toBeNull();
  });

  it('item 9 — offline (network failure) on the dashboard endpoint renders the unreachable state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], dashboardByProjectId: { [PROJECT_A.id]: 'offline' } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
  });

  it('item 10 — a generic API failure on the dashboard endpoint renders a real, distinct error state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: { status: 500, body: envelope('INTERNAL', 'boom') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText("Couldn't load the dashboard")).toBeTruthy());
  });

  it('item 11 — the API key never appears in any dashboard-related request URL', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));

    for (const call of fetchMock.mock.calls) {
      expect(String(call[0])).not.toContain(RAW_KEY);
    }
  });

  // --------------------------------------------------------- KPI mapping

  it('item 12/13 — each KPI card renders the exact real backend value, calculated per its documented meaning', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: {
        [PROJECT_A.id]: dashboardFor(PROJECT_A, {
          test_count: 7,
          execution_counts: { RUNNING: 2 },
          task_counts: { PASSED: 10, FAILED: 4, BLOCKED: 3, NEEDS_REVIEW: 6, SKIPPED: 1 },
        }),
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    const root = kpiRow();
    await waitFor(() => expect(within(root).getByText('7')).toBeTruthy()); // test_count → Executed Tests
    expect(within(root).getByText('2')).toBeTruthy(); // execution_counts.RUNNING → Active Runs
    expect(within(root).getByText('10')).toBeTruthy(); // task_counts.PASSED
    expect(within(root).getByText('4')).toBeTruthy(); // task_counts.FAILED
    expect(within(root).getByText('3')).toBeTruthy(); // task_counts.BLOCKED
    expect(within(root).getByText('6')).toBeTruthy(); // task_counts.NEEDS_REVIEW
  });

  it('item 14 — a real, successful, all-zero dashboard response renders honest zeros, never an error', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    expect(screen.queryByText("Couldn't load the dashboard")).toBeNull();
    expect(within(kpiRow()).getByText('Zero failures')).toBeTruthy();
  });

  it('item 15 — an API failure never falls back to a previously-loaded (now stale) KPI value', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 8 } }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(kpiRow()).getByText('8')).toBeTruthy());

    // Now the same project's dashboard call starts failing — triggered via
    // a real realtime event (re-selecting the *same* already-selected
    // project would not itself change React state or re-fetch anything).
    // The stale "8" must not remain displayed next to/instead of an
    // honest error.
    await waitFor(() => expect(eventSockets().length).toBe(1));
    act(() => eventSockets()[0].simulateOpen());
    fetchMock.mockImplementation(async (url: string) => {
      if (String(url).includes('/dashboard')) throw new TypeError('Failed to fetch');
      if (String(url).includes('/kpis')) return jsonResponse(200, kpisFor(PROJECT_A));
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    await act(async () => {
      eventSockets()[0].simulateMessage(realEvent({ project_id: PROJECT_A.id }));
      await new Promise((r) => setTimeout(r, 500));
    });
    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
    // The error state replaces the KPI row entirely — confirms the stale
    // value isn't rendered anywhere at all, not just outside that region.
    expect(document.getElementById('dashboard-kpi-row')).toBeNull();
    expect(screen.queryByText('8')).toBeNull();
  });

  it('item 16 — the always-unsupported "Est. Duration"/health-score concepts never display a fabricated value', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    // No invented health states (Warning/Degraded/Critical/Offline) exist.
    expect(screen.queryByText('Critical')).toBeNull();
    expect(screen.queryByText('Warning')).toBeNull();
    expect(screen.queryByText('Degraded')).toBeNull();
  });

  // ---------------------------------------------------------------- charts

  it('item 17/18/19 — the execution-activity timeline chart always renders its own real, honest empty state (no fabricated time series exists)', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 50 } }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('No execution timeline available')).toBeTruthy());
    // The panel must state the capability is missing, not promise data that
    // can never arrive — `timelineData` is permanently empty because no
    // time-series history endpoint exists.
    expect(screen.getByText(/no time-series execution-history endpoint/)).toBeTruthy();
    expect(screen.queryByText(/will populate this real-time timeline graph/)).toBeNull();
    // And no chart chrome (legend/axis captions) above an empty chart.
    expect(screen.queryByText('Total Executed')).toBeNull();
    expect(screen.queryByText('X-Axis: Time Window')).toBeNull();
  });

  it('item 20 — the donut chart\'s real task-status distribution updates after a project switch', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      dashboardByProjectId: {
        [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 12 } }),
        [PROJECT_B.id]: dashboardFor(PROJECT_B, { task_counts: {} }),
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(kpiRow()).getByText('12')).toBeTruthy());
    expect(screen.getByText('Executed Tasks')).toBeTruthy(); // donut has real data

    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(screen.getByText('No test execution data available')).toBeTruthy());
  });

  // -------------------------------------------------------------- realtime

  it('item 21/22 — Dashboard consumes the one centralized RealtimeContext connection; no second WebSocket is created', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(eventSockets().length).toBe(1));
    expect(eventSockets()[0].url).toContain(`/projects/${PROJECT_A.id}/events`);
  });

  it('the Orchestrator log stream is one separate connection — never a second run-event socket, and never two open at once', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    // Exactly one run-event socket, and the log stream is a genuinely
    // different endpoint — not a duplicate of it.
    await waitFor(() => expect(eventSockets().length).toBe(1));
    await waitFor(() => expect(openLogSockets().length).toBe(1));
    expect(openLogSockets()[0].url).toContain('/logs/ws');

    // Switching project re-subscribes the log stream, closing the old
    // socket first — a switch must never leave two open simultaneously.
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(eventSockets().length).toBe(1));
    expect(openLogSockets().length).toBeLessThanOrEqual(1);
  });

  it('a server-side close reconnects the log stream — a clean 1000 from the backend is not treated as an intentional client close', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      setApiKey(RAW_KEY);
      mockBackend({ projects: [PROJECT_A] });
      renderApp();
      await verifyKey();
      await openAndSelectProject(PROJECT_A);
      await waitFor(() => expect(openLogSockets().length).toBe(1));

      const first = openLogSockets()[0];
      act(() => first.simulateOpen());

      // The backend restarts / a proxy drops the stream: the browser reports
      // a perfectly clean 1000. That is a real, recoverable drop — the panel
      // must retry it, not sit on OFFLINE forever.
      act(() => first.close());
      expect(openLogSockets().length).toBe(0);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(2000);
      });
      await waitFor(() => expect(openLogSockets().length).toBe(1));
      expect(openLogSockets()[0]).not.toBe(first);
    } finally {
      vi.useRealTimers();
    }
  });

  it('item 23 — a real event for the selected project triggers a debounced dashboard resync', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 1 } }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(kpiRow()).getByText('1')).toBeTruthy());
    await waitFor(() => expect(eventSockets().length).toBe(1));
    act(() => eventSockets()[0].simulateOpen());

    // The real backend's own count changes; the event is the only signal.
    fetchMock.mockImplementation(async (url: string) => {
      if (String(url).includes('/dashboard')) return jsonResponse(200, dashboardFor(PROJECT_A, { task_counts: { PASSED: 9 } }));
      if (String(url).includes('/kpis')) return jsonResponse(200, kpisFor(PROJECT_A));
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    await act(async () => {
      eventSockets()[0].simulateMessage(realEvent({ project_id: PROJECT_A.id }));
      await new Promise((r) => setTimeout(r, 500));
    });
    await waitFor(() => expect(within(kpiRow()).getByText('9')).toBeTruthy());
  });

  it('item 24 — a foreign-project event does not trigger a Dashboard resync (RealtimeContext-level isolation)', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 1 } }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(kpiRow()).getByText('1')).toBeTruthy());
    await waitFor(() => expect(eventSockets().length).toBe(1));
    act(() => eventSockets()[0].simulateOpen());

    const before = fetchMock.mock.calls.length;
    await act(async () => {
      eventSockets()[0].simulateMessage(realEvent({ project_id: 'some-other-project' }));
      await new Promise((r) => setTimeout(r, 500));
    });
    expect(fetchMock.mock.calls.length).toBe(before);
  });

  it('item 25 — two events close together collapse into one resync (debounced, not one request per event)', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    await waitFor(() => expect(eventSockets().length).toBe(1));
    act(() => eventSockets()[0].simulateOpen());

    const before = fetchMock.mock.calls.filter((c) => String(c[0]).includes('/dashboard')).length;
    await act(async () => {
      eventSockets()[0].simulateMessage(realEvent({ event_id: 'evt-1', project_id: PROJECT_A.id }));
      eventSockets()[0].simulateMessage(realEvent({ event_id: 'evt-2', project_id: PROJECT_A.id }));
      eventSockets()[0].simulateMessage(realEvent({ event_id: 'evt-3', project_id: PROJECT_A.id }));
      await new Promise((r) => setTimeout(r, 600));
    });
    const after = fetchMock.mock.calls.filter((c) => String(c[0]).includes('/dashboard')).length;
    expect(after - before).toBe(1);
  });

  // --------------------------------------------------------------- refresh

  it('item 26/27 — the error-state Retry control performs one real request, not a fabricated fallback or duplicate storm', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: 'offline' },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());

    fetchMock.mockImplementation(async (url: string) => {
      if (String(url).includes('/dashboard')) return jsonResponse(200, dashboardFor(PROJECT_A, { task_counts: { PASSED: 4 } }));
      if (String(url).includes('/kpis')) return jsonResponse(200, kpisFor(PROJECT_A));
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    const before = fetchMock.mock.calls.length;
    await act(async () => {
      screen.getByText('Retry').closest('button')!.click();
    });
    await waitFor(() => expect(within(kpiRow()).getByText('4')).toBeTruthy());
    // Exactly the real calls a fresh load makes — no duplicate storm.
    // Three for the Dashboard itself (dashboard/kpis/executions); the
    // Neural Intelligence Graph's own extra three (test-cases/executions/
    // audit) are gone along with that now-removed, confirmed-redundant
    // feature.
    expect(fetchMock.mock.calls.length - before).toBe(3);
  });

  // ------------------------------------------------------------ regression

  it('item 28 — Dashboard\'s approved layout/shell regions all render intact', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(within(dashboardRoot()).getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    expect(document.getElementById('dashboard-header')).toBeTruthy();
    expect(document.getElementById('dashboard-kpi-row')).toBeTruthy();
    expect(document.getElementById('current-project-detail-panel')).toBeTruthy();
    // The decorative orb column was replaced by the real AI engine panel —
    // same layout slot, real data instead of ProjectStatus-derived theatre.
    expect(document.getElementById('ai-engine-column')).toBeTruthy();
    expect(document.getElementById('ai-engine-panel')).toBeTruthy();
    // And none of the orb's fabricated engine claims survive.
    expect(screen.queryByText(/Autonomous Verification Engine/)).toBeNull();
    expect(screen.queryByText('CORE STABLE')).toBeNull();
    expect(screen.queryByText('ACTIVE INFERENCE')).toBeNull();
    expect(document.getElementById('dashboard-project-status-section')).toBeTruthy();
    expect(document.getElementById('project-status-donut-panel')).toBeTruthy();
    expect(document.getElementById('project-execution-graph-panel')).toBeTruthy();
  });

  it('item 29 — Test Generator remains unregressed with the real Dashboard present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/test-generator']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });

  it('item 30 — Execution Hub remains unregressed with the real Dashboard present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/execution']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-execution-hub-page')).toBeTruthy());
  });

  it('item 31 — Audit remains unregressed with the real Dashboard present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/audit']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-audit-page')).toBeTruthy());
  });

  it('item 32 — Evidence remains unregressed with the real Dashboard present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/evidence']);
    await verifyKey();

    await waitFor(() => expect(screen.getAllByText('Evidence Viewer').length).toBeGreaterThan(0));
  });
});

/**
 * FE-26 — real Average Run Duration KPI (`execution_avg_run_duration_ms`,
 * BE-12b). No new request, no new formatter, no new realtime mechanism:
 * this reuses the exact same `aiqaApi.dashboard.kpis` call, the exact same
 * `formatMs` helper already proven for `execution_efficiency_ms`, and the
 * exact same debounced-any-event resync `execution_efficiency_ms` already
 * rides — this describe block only proves the new field rides along
 * correctly, not that any of those mechanisms work (already covered above).
 */
describe('DashboardPage — FE-26 real Average Run Duration KPI', () => {
  it('item 1 — a real execution_avg_run_duration_ms value renders using the existing formatMs convention', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) },
      kpisByProjectId: { [PROJECT_A.id]: kpisFor(PROJECT_A, { execution_avg_run_duration_ms: 20_000 }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    // Same formatMs(ms >= 1000) => `${(ms/1000).toFixed(1)}s` behavior
    // already proven for execution_efficiency_ms (item 5/6 above) — not a
    // new formatting convention invented for this one field.
    await waitFor(() => expect(within(dashboardRoot()).getByText('20.0s')).toBeTruthy());
    expect(within(dashboardRoot()).getByText(/Average run duration/)).toBeTruthy();
  });

  it('item 2 — null renders the existing honest "—" convention, never a fabricated 0', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) },
      kpisByProjectId: { [PROJECT_A.id]: kpisFor(PROJECT_A, { execution_avg_run_duration_ms: null }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(within(dashboardRoot()).getByText(/Average run duration/)).toBeTruthy());
    const row = within(dashboardRoot()).getByText(/Average run duration/).closest('span')!;
    expect(row.textContent).toContain('—');
    expect(row.textContent).not.toMatch(/0ms|0\.0s|\b0\b/);
  });

  it('item 3 — existing Execution Reliability and Execution Efficiency KPIs are unaffected by the new field', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A, { task_counts: { PASSED: 5, FAILED: 3 } }) },
      kpisByProjectId: {
        [PROJECT_A.id]: kpisFor(PROJECT_A, { execution_reliability: 0.625, execution_efficiency_ms: 1500, execution_avg_run_duration_ms: 20_000 }),
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(within(kpiRow()).getByText(/62\.5% reliability/)).toBeTruthy());
    expect(within(dashboardRoot()).getByText('1.5s')).toBeTruthy(); // execution_efficiency_ms, unchanged
    expect(within(dashboardRoot()).getByText('20.0s')).toBeTruthy(); // execution_avg_run_duration_ms, new
  });

  it('item 4 — project switching isolation: Project A\'s duration never leaks into Project B\'s display', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A), [PROJECT_B.id]: dashboardFor(PROJECT_B) },
      kpisByProjectId: {
        [PROJECT_A.id]: kpisFor(PROJECT_A, { execution_avg_run_duration_ms: 5_000 }),
        [PROJECT_B.id]: kpisFor(PROJECT_B, { execution_avg_run_duration_ms: 60_000 }),
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(dashboardRoot()).getByText('5.0s')).toBeTruthy());

    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(within(dashboardRoot()).getByText('60.0s')).toBeTruthy());
    expect(screen.queryByText('5.0s')).toBeNull();
  });

  it('item 5 — the existing debounced realtime resync updates the new KPI, with no new subscription/timer/WebSocket', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) },
      kpisByProjectId: { [PROJECT_A.id]: kpisFor(PROJECT_A, { execution_avg_run_duration_ms: 5_000 }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(dashboardRoot()).getByText('5.0s')).toBeTruthy());
    await waitFor(() => expect(eventSockets().length).toBe(1)); // exactly one real connection, unchanged
    act(() => eventSockets()[0].simulateOpen());

    fetchMock.mockImplementation(async (url: string) => {
      if (String(url).includes('/dashboard')) return jsonResponse(200, dashboardFor(PROJECT_A));
      if (String(url).includes('/kpis')) return jsonResponse(200, kpisFor(PROJECT_A, { execution_avg_run_duration_ms: 45_000 }));
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    await act(async () => {
      eventSockets()[0].simulateMessage(realEvent({ type: 'RUN_COMPLETED', project_id: PROJECT_A.id }));
      await new Promise((r) => setTimeout(r, 500));
    });
    await waitFor(() => expect(within(dashboardRoot()).getByText('45.0s')).toBeTruthy());
    expect(screen.queryByText('5.0s')).toBeNull();
  });

  it('item 6 — the new field causes no additional /kpis request beyond the existing single fetch per load', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      dashboardByProjectId: { [PROJECT_A.id]: dashboardFor(PROJECT_A) },
      kpisByProjectId: { [PROJECT_A.id]: kpisFor(PROJECT_A, { execution_avg_run_duration_ms: 5_000 }) },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(within(dashboardRoot()).getByText('5.0s')).toBeTruthy());

    const kpisCalls = fetchMock.mock.calls.filter((c) => String(c[0]).includes('/kpis'));
    expect(kpisCalls.length).toBe(1);
  });
});


// ------------------------------ AI Engine panel (replaces the fake orb)

describe('DashboardPage — real AI Engine panel', () => {
  it("renders the REAL routed model, its real score and the backend's own stated reason", async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus();
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('ollama / qwen3:14b')).toBeTruthy());
    expect(screen.getByText(/best available model for TestCaseGeneration by routing score/)).toBeTruthy();
    expect(screen.getByText(/score 211\.48/)).toBeTruthy();
    expect(screen.getByText('AUTO mode')).toBeTruthy();
  });

  it('reports embedding capability truthfully as NONE — no vector engine is claimed', async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus();
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('embedding')).toBeTruthy());
    const panel = document.getElementById('ai-engine-panel')!;
    expect(within(panel).getAllByText('NONE').length).toBeGreaterThan(0);
    expect(within(panel).getByText(/Embeddings\/vector search have no endpoint/)).toBeTruthy();
  });

  it('renders UNKNOWN capability as unknown — never upgraded to a guess', async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus();
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    const panel = await waitFor(() => document.getElementById('ai-engine-panel')!);
    expect(within(panel).getByText('long ctx')).toBeTruthy();
    expect(within(panel).getAllByText('UNKNOWN').length).toBeGreaterThan(0);
  });

  it('shows real provider health, including an unconfigured provider', async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus();
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    const panel = await waitFor(() => document.getElementById('ai-engine-panel')!);
    expect(within(panel).getByText(/Ollama/)).toBeTruthy();
    expect(within(panel).getByText('HEALTHY')).toBeTruthy();
    expect(within(panel).getByText('NOT_CONFIGURED')).toBeTruthy();
  });

  it('the neural-activity canvas is driven by one real value per capability field, at its real evidenced level', async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus();
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    const orb = await waitFor(() => document.getElementById('ai-engine-orb')!);
    expect(orb.getAttribute('data-live')).toBe('true');
    // The real per-field mix (text_generation/tool_use STRONG,
    // structured_output/reasoning BASIC, long_context UNKNOWN,
    // vision/embedding NONE) — the same 7 real fields the previous SVG
    // diagram's input column showed, now fed to the canvas engine instead.
    expect(orb.getAttribute('data-cap-text_generation')).toBe('STRONG');
    expect(orb.getAttribute('data-cap-tool_use')).toBe('STRONG');
    expect(orb.getAttribute('data-cap-structured_output')).toBe('BASIC');
    expect(orb.getAttribute('data-cap-reasoning')).toBe('BASIC');
    expect(orb.getAttribute('data-cap-long_context')).toBe('UNKNOWN');
    expect(orb.getAttribute('data-cap-vision')).toBe('NONE');
    expect(orb.getAttribute('data-cap-embedding')).toBe('NONE');
    // The real routing decision is rendered as real, visible text below
    // the canvas (a canvas has no text nodes of its own to read it from).
    expect(orb.getAttribute('data-active-provider')).toBe('ollama');
    // The raw, real, unrounded score (the fixture's own 211.48) on the
    // data attribute; the visible label below the canvas rounds it for
    // display, same as the previous SVG diagram's own `.toFixed(0)`.
    expect(orb.getAttribute('data-routing-score')).toBe('211.48');
    expect(orb.textContent).toContain('211');
  });

  it('the canvas activity level is a real, computed function of how many discovered providers are actually healthy', async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus(); // 2 real providers: ollama HEALTHY, mj-gateway NOT_CONFIGURED
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    const orb = await waitFor(() => document.getElementById('ai-engine-orb')!);
    // level = 0.35 + healthyFraction * 0.45 (AIEngineOrb's own real,
    // disclosed formula) = 0.35 + (1/2)*0.45 = 0.575 — real math on the
    // real provider list, not an arbitrary constant.
    expect(orb.getAttribute('data-level')).toBe('0.575');
    // The real per-provider detail this single scalar cannot carry on its
    // own (which provider, which exact status) remains fully shown,
    // unchanged, in the "Providers" list this same real GET /api/v1/ai/
    // status response drives — see the "shows real provider health"
    // test above for that assertion.
  });

  it('the canvas goes inert with no route — no flow, no score, an honest idle field, and it says so', async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus({ ...AI_STATUS_BODY, routing_preview: null });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    const orb = await waitFor(() => document.getElementById('ai-engine-orb')!);
    expect(orb.getAttribute('data-live')).toBe('false');
    expect(orb.getAttribute('data-active-provider')).toBe('');
    expect(orb.textContent).toContain('NO ROUTE');
    expect(orb.textContent).not.toContain('211');
    // No route -> every capability input falls back to the same honest
    // idle marker, never a stale real level left over from a previous
    // render.
    for (const cap of ['text_generation', 'structured_output', 'reasoning', 'tool_use', 'long_context', 'vision', 'embedding']) {
      expect(orb.getAttribute(`data-cap-${cap}`)).toBe('idle');
    }
  });

  it('shows no route at all when the backend reports none — nothing fabricated in its place', async () => {
    setApiKey(RAW_KEY);
    mockWithAiStatus({ ...AI_STATUS_BODY, routing_preview: null });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(/No compatible model is available/)).toBeTruthy());
    expect(screen.queryByText('ollama / qwen3:14b')).toBeNull();
  });

  it('an unexpected AI status payload is a real error, and never crashes the Dashboard', async () => {
    setApiKey(RAW_KEY);
    // An array is not an `AiStatusResponse`; the old orb-replacement code
    // crashed on `status.providers.map` and unmounted the whole Dashboard.
    mockWithAiStatus([]);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(/unexpected AI status payload/)).toBeTruthy());
    // The rest of the Dashboard is still standing.
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
    expect(document.getElementById('dashboard-kpi-row')).toBeTruthy();
  });
});
