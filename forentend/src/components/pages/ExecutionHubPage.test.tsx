/**
 * FE-8 targeted tests for the real Execution Hub — brief §34 items 11-33,
 * exercised through the real `AppShell` tree, exactly like FE-6/FE-7's own
 * page test files. `fetch` is mocked only at the transport boundary.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { Project, RunDetail, RunListItem } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_exec_test' } };
}

const RAW_KEY = 'aiqa_exectest0000000000000';
const PROJECT_A: Project = {
  id: 'proj-a',
  name: 'Project Alpha',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};
const PROJECT_B: Project = {
  id: 'proj-b',
  name: 'Project Beta',
  description: null,
  created_at: '2026-01-02T00:00:00Z',
  updated_at: '2026-01-02T00:00:00Z',
};
const RUN_A: RunListItem = { run_id: 'run-a', name: 'Regression A', state: 'RUNNING', total: 2, passed: 1, failed: 0, project_id: PROJECT_A.id };
const RUN_A_DETAIL: RunDetail = {
  run_id: 'run-a',
  name: 'Regression A',
  state: 'RUNNING',
  total: 2,
  queued: 0,
  running: 1,
  passed: 1,
  failed: 0,
  blocked: 0,
  skipped: 0,
  cancelled: 0,
  timeout: 0,
  infra_error: 0,
  needs_review: 0,
  started_at: '2026-08-19T10:00:00Z',
  finished_at: null,
  project_id: PROJECT_A.id,
  requested_by: 'fe8-key',
  policy_notes: null,
  approved_by: null,
  approved_at: null,
  tasks: [
    { id: 't1', name: 'Task One', prerequisites: [], status: 'PASSED', reason: null, started_at: '2026-08-19T10:00:00Z', finished_at: '2026-08-19T10:00:05Z' },
    { id: 't2', name: 'Task Two', prerequisites: [], status: 'RUNNING', reason: null, started_at: '2026-08-19T10:00:05Z', finished_at: null },
  ],
};
const RUN_A_DETAIL_TERMINAL: RunDetail = { ...RUN_A_DETAIL, state: 'COMPLETED', running: 0, passed: 2, finished_at: '2026-08-19T10:00:10Z', tasks: [
  { id: 't1', name: 'Task One', prerequisites: [], status: 'PASSED', reason: null, started_at: '2026-08-19T10:00:00Z', finished_at: '2026-08-19T10:00:05Z' },
  { id: 't2', name: 'Task Two', prerequisites: [], status: 'PASSED', reason: null, started_at: '2026-08-19T10:00:05Z', finished_at: '2026-08-19T10:00:10Z' },
] };

type Entry<T> = T | { status: number; body: unknown } | 'offline';

function mockBackend(opts: {
  projects: Project[];
  runsByProjectId?: Record<string, Entry<RunListItem[]>>;
  detailByRunId?: Record<string, Entry<RunDetail>>;
  evidenceByRunId?: Record<string, unknown[]>;
  onCreate?: (projectId: string, body: unknown) => Response | Promise<Response>;
}) {
  const isErrorEntry = (e: unknown): e is { status: number; body: unknown } =>
    typeof e === 'object' && e !== null && 'status' in e && !('run_id' in e);

  /** The real backend always returns a full `RunDetail` (`tasks` included,
   *  even if empty) for any run it actually created — never a bare `{}`.
   *  Used only when a test creates a run without separately registering
   *  its own `detailByRunId` entry (i.e. tests that don't care about the
   *  new run's detail content, only that creation/selection itself
   *  behaved correctly). */
  function fallbackDetail(runId: string, projectId: string): RunDetail {
    return {
      run_id: runId,
      name: 'Untitled execution',
      state: 'SCHEDULED',
      total: 0,
      queued: 0,
      running: 0,
      passed: 0,
      failed: 0,
      blocked: 0,
      skipped: 0,
      cancelled: 0,
      timeout: 0,
      infra_error: 0,
      needs_review: 0,
      started_at: '2026-08-19T00:00:00Z',
      finished_at: null,
      project_id: projectId,
      requested_by: 'test',
      policy_notes: null,
      approved_by: null,
      approved_at: null,
      tasks: [],
    };
  }

  // The real backend inserts a newly-created run into its own list before
  // `POST .../executions` even returns (`create_run_core` writes to both
  // the DB and its in-memory map first) — so a real, immediately-following
  // `GET .../executions` genuinely includes it. Mirrored here so this mock
  // doesn't understate what the real backend does.
  const createdRuns: RunListItem[] = [];

  const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    if (url.includes('/evidence')) {
      const m = url.match(/executions\/([^/]+)\/evidence/);
      const runId = m ? m[1] : '';
      return jsonResponse(200, opts.evidenceByRunId?.[runId] ?? []);
    }
    if (url.includes('/cancel')) return jsonResponse(200, { run_id: url.split('/')[5], status: 'CANCELLED' });
    if (url.includes('/approve')) return jsonResponse(200, { run_id: url.split('/')[5], state: 'SCHEDULED' });
    if (url.includes('/reject')) return jsonResponse(200, { run_id: url.split('/')[5], state: 'REJECTED' });
    const execMatch = url.match(/projects\/([^/]+)\/executions\/([^/?]+)/);
    if (execMatch) {
      const [, projectId, runId] = execMatch;
      const entry = opts.detailByRunId?.[runId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (isErrorEntry(entry)) return jsonResponse(entry.status, entry.body);
      return jsonResponse(200, entry ?? fallbackDetail(runId, projectId));
    }
    if (url.includes('/executions')) {
      if (init?.method === 'POST') {
        const projMatch = url.match(/projects\/([^/]+)\/executions/);
        const projectId = projMatch ? projMatch[1] : '';
        const body = JSON.parse(String(init.body)) as { name: string };
        const res = opts.onCreate
          ? await opts.onCreate(projectId, body)
          : jsonResponse(201, { run_id: 'new-run', status: 'SCHEDULED', policy_notes: null });
        if (res.ok) {
          const created = (await res.clone().json()) as { run_id: string; status: RunListItem['state'] };
          createdRuns.push({ run_id: created.run_id, name: body.name, state: created.status, total: 0, passed: 0, failed: 0, project_id: projectId });
        }
        return res;
      }
      const projMatch = url.match(/projects\/([^/]+)\/executions/);
      const projectId = projMatch ? projMatch[1] : '';
      const entry = opts.runsByProjectId?.[projectId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && !Array.isArray(entry) && 'status' in entry) return jsonResponse(entry.status, entry.body);
      const baseList = Array.isArray(entry) ? entry : [];
      return jsonResponse(200, [...baseList, ...createdRuns.filter((r) => r.project_id === projectId)]);
    }
    // FE-10: real Dashboard endpoints — zero-valued but real-shaped so any
    // test that happens to render `/dashboard` (e.g. the cross-page
    // regression checks) sees an honest, empty-but-real response rather
    // than falling through to the `opts.projects` catch-all below (which
    // is an array, not a `DashboardResponse`/`KpisResponse` shape).
    if (url.includes('/dashboard')) {
      const m = url.match(/projects\/([^/]+)\/dashboard/);
      const pid = m ? m[1] : '';
      const proj = opts.projects.find((p) => p.id === pid);
      return jsonResponse(200, {
        project_id: pid,
        project_name: proj?.name ?? 'Unknown',
        execution_counts: {},
        task_counts: {},
        pass_fail: { passed: 0, failed: 0 },
        test_count: 0,
        generated_at: '2026-08-19T00:00:00Z',
      });
    }
    if (url.includes('/kpis')) {
      const m = url.match(/projects\/([^/]+)\/kpis/);
      const pid = m ? m[1] : '';
      return jsonResponse(200, { project_id: pid, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
    }
    return jsonResponse(200, opts.projects);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(initialEntries: string[] = ['/execution']) {
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
  // Scoped to the header's own real listbox — FE-10's Dashboard now has
  // its own real, separate `<select>` of the same real projects, whose
  // native `<option>` elements share the implicit ARIA "option" role and
  // would otherwise collide with an unscoped query when Dashboard is
  // mounted at the same time.
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('ExecutionHubPage (FE-8 §34 items 11-33)', () => {
  it('item 11 — unauthenticated (no key) never fetches executions; shows the real auth-state card', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('aiqa-execution-hub-page')).toBeNull();
  });

  it('item 12 — key_present (unverified) never fetches executions', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText('Key saved, not yet verified')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('item 13/23/24 — authenticated project loads the real execution list and a running execution\'s real detail', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [RUN_A] }, detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText('Task One')).toBeTruthy());
    expect(screen.getAllByText('RUNNING').length).toBeGreaterThan(0);
  });

  it('item 14 — no project prevents creating an execution', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    // The button stays visible (consistent with every other page's header
    // actions) but real-disabled — a native `disabled` attribute, not just
    // a click handler that silently no-ops.
    expect((document.getElementById('execution-new-button') as HTMLButtonElement).disabled).toBe(true);
  });

  it('item 15/16 — switching project clears the stale selected execution and aborts its in-flight request', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A], [PROJECT_B.id]: [] },
      detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });
    await waitFor(() => expect(screen.getByText('Task One')).toBeTruthy());

    await openAndSelectProject(PROJECT_B);

    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    expect(screen.queryByText('Task One')).toBeNull();
    expect(screen.getByText('No execution selected')).toBeTruthy();
  });

  it('item 17 — a real, successful, zero-row response renders the honest empty state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
  });

  it('item 18 — a real loading state renders while the execution list request is in flight', async () => {
    setApiKey(RAW_KEY);
    let resolveFetch: (r: Response) => void;
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/executions')) return pending;
      return jsonResponse(200, [PROJECT_A]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    expect(screen.getByText('Loading executions')).toBeTruthy();

    await act(async () => {
      resolveFetch!(jsonResponse(200, []));
      await pending;
    });
  });

  it('item 19/20 — clicking Start Execution enters a creating state and the button is disabled against double-submission', async () => {
    setApiKey(RAW_KEY);
    let resolveCreate: (r: Response) => void;
    const pendingCreate = new Promise<Response>((resolve) => {
      resolveCreate = resolve;
    });
    let createCalls = 0;
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [] },
      onCreate: () => {
        createCalls += 1;
        return pendingCreate;
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());

    await act(async () => {
      document.getElementById('execution-new-button')!.click();
    });
    fireEvent.change(document.getElementById('exec-run-name') as HTMLInputElement, { target: { value: 'My Real Run' } });

    await act(async () => {
      screen.getByText('Start Execution').closest('button')!.click();
    });
    // A second click while creating must not fire a second request.
    await act(async () => {
      const btn = screen.queryByText('Creating…')?.closest('button');
      btn?.click();
    });

    expect(createCalls).toBe(1);

    await act(async () => {
      resolveCreate!(jsonResponse(201, { run_id: 'new-run-1', status: 'SCHEDULED', policy_notes: null }));
      await pendingCreate;
    });
  });

  it('item 21 — a successful execution creation selects the real returned run id', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [] },
      detailByRunId: { 'new-run-2': { ...RUN_A_DETAIL, run_id: 'new-run-2', name: 'My Real Run' } },
      onCreate: () => jsonResponse(201, { run_id: 'new-run-2', status: 'SCHEDULED', policy_notes: null }),
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());

    await act(async () => {
      document.getElementById('execution-new-button')!.click();
    });
    fireEvent.change(document.getElementById('exec-run-name') as HTMLInputElement, { target: { value: 'My Real Run' } });
    await act(async () => {
      screen.getByText('Start Execution').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText('Task One')).toBeTruthy());
  });

  it('item 22 — a real creation failure renders a real error message, not a fabricated success', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [] },
      onCreate: () => jsonResponse(403, envelope('FORBIDDEN', "role 'viewer' cannot perform this action")),
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());

    await act(async () => {
      document.getElementById('execution-new-button')!.click();
    });
    fireEvent.change(document.getElementById('exec-run-name') as HTMLInputElement, { target: { value: 'My Real Run' } });
    await act(async () => {
      screen.getByText('Start Execution').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText(/role 'viewer' cannot perform this action/)).toBeTruthy());
  });

  it('item 25 — a terminal execution stops polling and shows the real terminal status', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A] },
      detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL_TERMINAL },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getAllByText('COMPLETED').length).toBeGreaterThan(0));
    // Terminal → no Cancel/Approve/Reject actions.
    expect(screen.queryByText('Cancel')).toBeNull();
  });

  it('item 26 — Refresh performs a real call and clears a selection that no longer exists', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A] },
      detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });
    await waitFor(() => expect(screen.getByText('Task One')).toBeTruthy());

    // Backend now reports zero runs (e.g. it was deleted/expired) — a real
    // refresh must clear the stale selection, not keep showing it.
    fetchMock.mockImplementation(async (url: string) => {
      if (url.includes('/executions') && !url.includes('/executions/')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    await act(async () => {
      document.getElementById('execution-refresh-button')!.click();
    });

    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
  });

  it('item 27 — no fabricated execution/run data ever renders', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [RUN_A] }, detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    // Known AIQAContext fake run/executor/project strings must never
    // appear *on this page* — scoped to `#aiqa-execution-hub-page`
    // specifically, since `k8s-runner-pool-us` is real, pre-existing,
    // unrelated Sidebar chrome (AIQAContext's own fake runner-pool widget,
    // rendered on every route) that this sprint did not touch and is not
    // asserting anything about.
    const pageHtml = document.getElementById('aiqa-execution-hub-page')!.innerHTML;
    expect(pageHtml).not.toContain('k8s-runner-pool-us');
    expect(pageHtml).not.toContain('AIQA Platform');
  });

  it('item 29 — the View Evidence link only appears when the run genuinely has evidence', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A] },
      detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL },
      evidenceByRunId: { [RUN_A.run_id]: [] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });
    await waitFor(() => expect(screen.getByText('Task One')).toBeTruthy());

    expect(screen.queryByText('View Evidence')).toBeNull();
  });

  it('item 29b — View Evidence appears once the real evidence endpoint confirms at least one row', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A] },
      detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL },
      evidenceByRunId: { [RUN_A.run_id]: [{ id: 'ev-1' }] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText('View Evidence')).toBeTruthy());
  });

  it('item 30 — the Audit link never claims a specific number of audit events', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [RUN_A] }, detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText('View Audit Log')).toBeTruthy());
    expect(screen.queryByText(/\d+ (audit )?events?/i)).toBeNull();
  });

  it('item 31 — Dashboard remains unregressed with ExecutionHubPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/dashboard']);
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    // FE-10: Dashboard now renders real project-scoped data (no more
    // fabricated "AIQA Platform" seed project) — proves the page's own
    // real content, not just the route, survived intact.
    await waitFor(() => expect(screen.getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    expect(screen.getByText('Total Test Cases')).toBeTruthy();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
  });

  it('item 32 — Test Generator remains unregressed with ExecutionHubPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/test-generator']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getAllByText('Test Case Generator').length).toBeGreaterThan(0);
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });
});
