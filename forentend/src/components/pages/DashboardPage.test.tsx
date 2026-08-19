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
  return { project_id: project.id, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T10:00:00Z', ...overrides };
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

function renderApp(initialEntries: string[] = ['/dashboard']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <AppShell />
    </MemoryRouter>,
  );
}

async function verifyKey(): Promise<void> {
  await act(async () => {
    screen.getByText('Verify Connection').closest('button')!.click();
  });
}

async function openAndSelectProject(project: Project): Promise<void> {
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
    expect(screen.queryByText('Total Test Cases')).toBeNull();
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
    expect(screen.queryByText('Total Test Cases')).toBeNull();
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
    await waitFor(() => expect(within(root).getByText('7')).toBeTruthy()); // test_count → Total Test Cases
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
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    act(() => MockWebSocket.instances[0].simulateOpen());
    fetchMock.mockImplementation(async (url: string) => {
      if (String(url).includes('/dashboard')) throw new TypeError('Failed to fetch');
      if (String(url).includes('/kpis')) return jsonResponse(200, kpisFor(PROJECT_A));
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    await act(async () => {
      MockWebSocket.instances[0].simulateMessage(realEvent({ project_id: PROJECT_A.id }));
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

    await waitFor(() => expect(screen.getByText('No Execution Activity Recorded')).toBeTruthy());
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
    expect(screen.getByText('Total Tests')).toBeTruthy(); // donut has real data

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
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    expect(MockWebSocket.instances[0].url).toContain(`/projects/${PROJECT_A.id}/events`);
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
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    act(() => MockWebSocket.instances[0].simulateOpen());

    // The real backend's own count changes; the event is the only signal.
    fetchMock.mockImplementation(async (url: string) => {
      if (String(url).includes('/dashboard')) return jsonResponse(200, dashboardFor(PROJECT_A, { task_counts: { PASSED: 9 } }));
      if (String(url).includes('/kpis')) return jsonResponse(200, kpisFor(PROJECT_A));
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    await act(async () => {
      MockWebSocket.instances[0].simulateMessage(realEvent({ project_id: PROJECT_A.id }));
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
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    act(() => MockWebSocket.instances[0].simulateOpen());

    const before = fetchMock.mock.calls.length;
    await act(async () => {
      MockWebSocket.instances[0].simulateMessage(realEvent({ project_id: 'some-other-project' }));
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
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    act(() => MockWebSocket.instances[0].simulateOpen());

    const before = fetchMock.mock.calls.filter((c) => String(c[0]).includes('/dashboard')).length;
    await act(async () => {
      MockWebSocket.instances[0].simulateMessage(realEvent({ event_id: 'evt-1', project_id: PROJECT_A.id }));
      MockWebSocket.instances[0].simulateMessage(realEvent({ event_id: 'evt-2', project_id: PROJECT_A.id }));
      MockWebSocket.instances[0].simulateMessage(realEvent({ event_id: 'evt-3', project_id: PROJECT_A.id }));
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
    // Exactly the three real calls a fresh load makes (dashboard/kpis/executions) — no duplicate storm.
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
    expect(document.getElementById('ai-quality-core-orb-column')).toBeTruthy();
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
