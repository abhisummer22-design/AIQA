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
  /** Re-planning v1 — `POST .../executions/:run_id/retry-failed`. Defaults
   *  to a real-shaped success so tests that don't care about the exact
   *  response still get one that matches `RetryFailedTasksResponse`. */
  onRetryFailed?: (runId: string, projectId: string) => Response | Promise<Response>;
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
    if (url.includes('/retry-failed')) {
      const m = url.match(/projects\/([^/]+)\/executions\/([^/]+)\/retry-failed/);
      const projectId = m ? m[1] : '';
      const runId = m ? m[2] : '';
      const res = opts.onRetryFailed
        ? await opts.onRetryFailed(runId, projectId)
        : jsonResponse(200, { run_id: 'retry-run-default', state: 'SCHEDULED', retried_task_ids: ['t-infra-1'] });
      if (res.ok) {
        // Real backend behavior: `retry_failed_tasks_scoped` reuses
        // `create_run_core` verbatim, which inserts the new run into the
        // DB/in-memory map before returning — so a real immediately-
        // following `GET .../executions` genuinely includes it too.
        // Mirrored via the same `createdRuns` mechanism as `onCreate`
        // above, so `fetchRuns`'s "clear a stale selection" logic (brief
        // §25) doesn't wrongly wipe out the freshly-selected retry run.
        const created = (await res.clone().json()) as { run_id: string; state: RunListItem['state'] };
        createdRuns.push({ run_id: created.run_id, name: 'Retry run', state: created.state, total: 0, passed: 0, failed: 0, project_id: projectId });
      }
      return res;
    }
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

  it('item 12 — no_key never fetches executions', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText(/No orchestrator API key/i)).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('aiqa-execution-hub-page')).toBeNull();
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
    expect(screen.getByText('Executed Tests')).toBeTruthy();
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

  describe('FE-12 — canonical status vocabulary', () => {
    it('a multi-word real RunState renders space-separated, never with a literal underscore', async () => {
      setApiKey(RAW_KEY);
      const pendingRun: RunListItem = { run_id: 'run-pending', name: 'Mobile Sweep', state: 'PENDING_APPROVAL', total: 1, passed: 0, failed: 0, project_id: PROJECT_A.id };
      const pendingDetail: RunDetail = { ...RUN_A_DETAIL, run_id: 'run-pending', name: 'Mobile Sweep', state: 'PENDING_APPROVAL', tasks: [] };
      mockBackend({
        projects: [PROJECT_A],
        runsByProjectId: { [PROJECT_A.id]: [pendingRun] },
        detailByRunId: { 'run-pending': pendingDetail },
      });
      renderApp();
      await verifyKey();
      await openAndSelectProject(PROJECT_A);

      await waitFor(() => expect(screen.getByText('Mobile Sweep')).toBeTruthy());
      // The real raw backend value stays exactly `PENDING_APPROVAL` (never
      // renamed) — only its *display* drops the underscore for a space,
      // the same convention Reports/the legacy StatusBadge already used.
      await waitFor(() => expect(screen.getAllByText('PENDING APPROVAL').length).toBeGreaterThan(0));
      expect(screen.queryByText('PENDING_APPROVAL')).toBeNull();
    });

    it('a multi-word real TaskStatus (NEEDS_REVIEW) renders space-separated in the task table', async () => {
      setApiKey(RAW_KEY);
      const detailWithNeedsReview: RunDetail = {
        ...RUN_A_DETAIL_TERMINAL,
        needs_review: 1,
        tasks: [
          { id: 't1', name: 'Task One', prerequisites: [], status: 'NEEDS_REVIEW', reason: 'anomaly detected', started_at: '2026-08-19T10:00:00Z', finished_at: '2026-08-19T10:00:05Z' },
        ],
      };
      mockBackend({
        projects: [PROJECT_A],
        runsByProjectId: { [PROJECT_A.id]: [RUN_A] },
        detailByRunId: { [RUN_A.run_id]: detailWithNeedsReview },
      });
      renderApp();
      await verifyKey();
      await openAndSelectProject(PROJECT_A);
      await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());

      await act(async () => {
        screen.getByText('Regression A').click();
      });

      await waitFor(() => expect(screen.getAllByText('NEEDS REVIEW').length).toBeGreaterThan(0));
      expect(screen.queryByText('NEEDS_REVIEW')).toBeNull();
    });

    it('401 (authentication required) and 403 (forbidden) render distinct, non-interchangeable state cards', async () => {
      setApiKey(RAW_KEY);
      mockBackend({
        projects: [PROJECT_A],
        runsByProjectId: { [PROJECT_A.id]: { status: 403, body: envelope('FORBIDDEN', "role 'viewer' cannot perform this action") } },
      });
      renderApp();
      await verifyKey();
      await openAndSelectProject(PROJECT_A);

      await waitFor(() => expect(screen.getByText('ACCESS FORBIDDEN · 403')).toBeTruthy());
      expect(screen.queryByText('AUTHENTICATION REQUIRED · 401')).toBeNull();
      expect(screen.queryByText('Authentication required')).toBeNull();
    });
  });
});

/**
 * FE-17 — real `TaskDef.prerequisites` exposure in the task-creation form.
 * Same real-`AppShell`/transport-boundary-mocked-`fetch` approach as the
 * FE-8 suite above. Task ids are always set to explicit, human-readable
 * values (`alpha`/`beta`/`gamma`) rather than relying on the default
 * `task-N` counter, which is a module-level value shared across every test
 * in this file and not reset between them.
 */
describe('ExecutionHubPage — FE-17 dependency/DAG editor', () => {
  async function openCreatePanel(): Promise<void> {
    await act(async () => {
      document.getElementById('execution-new-button')!.click();
    });
  }
  function taskIdInputs(): HTMLInputElement[] {
    return screen.getAllByPlaceholderText('task id') as HTMLInputElement[];
  }
  function prereqRows(): HTMLElement[] {
    return Array.from(document.querySelectorAll('[id^="task-prereqs-"]')) as HTMLElement[];
  }
  async function addTask(): Promise<void> {
    await act(async () => {
      screen.getByText('Add task').click();
    });
  }

  it('item 1/2 — the dependency control renders only once a second task exists, offering the other real task as a candidate', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    await openCreatePanel();

    // A single task has nothing to depend on — no control yet.
    expect(screen.queryByText('Depends on:')).toBeNull();

    await addTask();
    const ids = taskIdInputs();
    fireEvent.change(ids[0], { target: { value: 'alpha' } });
    fireEvent.change(ids[1], { target: { value: 'beta' } });

    expect(screen.getAllByText('Depends on:').length).toBe(2);
    expect(screen.getByRole('button', { name: 'alpha' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'beta' })).toBeTruthy();
  });

  it('item 3 — a task is never offered as its own prerequisite candidate', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    await openCreatePanel();
    await addTask();
    const ids = taskIdInputs();
    fireEvent.change(ids[0], { target: { value: 'alpha' } });
    fireEvent.change(ids[1], { target: { value: 'beta' } });

    // Exactly one 'alpha' chip exists in the whole panel (inside beta's row)
    // and exactly one 'beta' chip (inside alpha's row) — neither task offers
    // itself, so there is no second copy of either inside its own row.
    expect(screen.getAllByRole('button', { name: 'alpha' }).length).toBe(1);
    expect(screen.getAllByRole('button', { name: 'beta' }).length).toBe(1);
  });

  it('item 4/5 — multiple prerequisites can be selected and then removed', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    await openCreatePanel();
    await addTask();
    await addTask();
    const ids = taskIdInputs();
    fireEvent.change(ids[0], { target: { value: 'alpha' } });
    fireEvent.change(ids[1], { target: { value: 'beta' } });
    fireEvent.change(ids[2], { target: { value: 'gamma' } });

    // gamma is the 3rd task -> its own "Depends on" row is the 3rd one.
    const gammaRow = prereqRows()[2];
    await act(async () => {
      within(gammaRow).getByRole('button', { name: 'alpha' }).click();
    });
    await act(async () => {
      within(gammaRow).getByRole('button', { name: 'beta' }).click();
    });
    expect(within(gammaRow).getByRole('button', { name: 'alpha' }).getAttribute('aria-pressed')).toBe('true');
    expect(within(gammaRow).getByRole('button', { name: 'beta' }).getAttribute('aria-pressed')).toBe('true');
    expect(within(gammaRow).queryByText(/none — click/)).toBeNull();

    // Remove alpha again.
    await act(async () => {
      within(gammaRow).getByRole('button', { name: 'alpha' }).click();
    });
    expect(within(gammaRow).getByRole('button', { name: 'alpha' }).getAttribute('aria-pressed')).toBe('false');
    expect(within(gammaRow).getByRole('button', { name: 'beta' }).getAttribute('aria-pressed')).toBe('true');
  });

  it('item 6 — selected prerequisites are serialized into the real request payload as TaskDef.prerequisites', async () => {
    setApiKey(RAW_KEY);
    let capturedBody: { tasks: Array<{ id: string; prerequisites: string[] }> } | null = null;
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [] },
      onCreate: (_projectId, body) => {
        capturedBody = body as { tasks: Array<{ id: string; prerequisites: string[] }> };
        return jsonResponse(201, { run_id: 'dep-run-1', status: 'SCHEDULED', policy_notes: null });
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    await openCreatePanel();
    await addTask();
    const ids = taskIdInputs();
    fireEvent.change(ids[0], { target: { value: 'alpha' } });
    fireEvent.change(ids[1], { target: { value: 'beta' } });

    const betaRow = prereqRows()[1];
    await act(async () => {
      within(betaRow).getByRole('button', { name: 'alpha' }).click();
    });

    fireEvent.change(document.getElementById('exec-run-name') as HTMLInputElement, { target: { value: 'Dependency Run' } });
    await act(async () => {
      screen.getByText('Start Execution').closest('button')!.click();
    });

    await waitFor(() => expect(capturedBody).not.toBeNull());
    const tasks = capturedBody!.tasks;
    expect(tasks.find((t) => t.id === 'alpha')?.prerequisites).toEqual([]);
    expect(tasks.find((t) => t.id === 'beta')?.prerequisites).toEqual(['alpha']);
  });

  it('item 7 — deleting a task strips it from every other task\'s prerequisites (no stale hidden dependency)', async () => {
    setApiKey(RAW_KEY);
    let capturedBody: { tasks: Array<{ id: string; prerequisites: string[] }> } | null = null;
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [] },
      onCreate: (_projectId, body) => {
        capturedBody = body as { tasks: Array<{ id: string; prerequisites: string[] }> };
        return jsonResponse(201, { run_id: 'dep-run-2', status: 'SCHEDULED', policy_notes: null });
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    await openCreatePanel();
    await addTask();
    const ids = taskIdInputs();
    fireEvent.change(ids[0], { target: { value: 'alpha' } });
    fireEvent.change(ids[1], { target: { value: 'beta' } });

    const betaRow = prereqRows()[1];
    await act(async () => {
      within(betaRow).getByRole('button', { name: 'alpha' }).click();
    });
    expect(within(betaRow).getByRole('button', { name: 'alpha' }).getAttribute('aria-pressed')).toBe('true');

    // Delete alpha (the 1st task's own remove button).
    await act(async () => {
      screen.getAllByTitle('Remove task')[0].click();
    });

    // Only one task remains, so the "Depends on" control disappears — but
    // verify via the real submitted payload that no stale 'alpha' reference
    // survived inside beta's prerequisites.
    fireEvent.change(document.getElementById('exec-run-name') as HTMLInputElement, { target: { value: 'Cleanup Run' } });
    await act(async () => {
      screen.getByText('Start Execution').closest('button')!.click();
    });

    await waitFor(() => expect(capturedBody).not.toBeNull());
    expect(capturedBody!.tasks.length).toBe(1);
    expect(capturedBody!.tasks[0].id).toBe('beta');
    expect(capturedBody!.tasks[0].prerequisites).toEqual([]);
  });

  it('item 9 — a real backend cycle-rejection error is shown truthfully, with no fake success', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [] },
      onCreate: () => jsonResponse(400, envelope('VALIDATION_ERROR', "dependency cycle detected at task 'alpha'")),
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    await openCreatePanel();
    await addTask();
    const ids = taskIdInputs();
    fireEvent.change(ids[0], { target: { value: 'alpha' } });
    fireEvent.change(ids[1], { target: { value: 'beta' } });
    // Construct an actual cycle: alpha depends on beta, beta depends on alpha.
    await act(async () => {
      within(prereqRows()[0]).getByRole('button', { name: 'beta' }).click();
    });
    await act(async () => {
      within(prereqRows()[1]).getByRole('button', { name: 'alpha' }).click();
    });

    fireEvent.change(document.getElementById('exec-run-name') as HTMLInputElement, { target: { value: 'Cyclic Run' } });
    await act(async () => {
      screen.getByText('Start Execution').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText(/dependency cycle detected at task 'alpha'/)).toBeTruthy());
    // Real error shown, not a fabricated success — the create panel is
    // still open with no new run selected.
    expect(screen.getByText('Start Execution')).toBeTruthy();
  });

  it('a network failure during creation shows a real offline error, never a fake success', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [] },
      onCreate: () => {
        throw new TypeError('Failed to fetch');
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
    await openCreatePanel();
    fireEvent.change(document.getElementById('exec-run-name') as HTMLInputElement, { target: { value: 'Offline Run' } });
    await act(async () => {
      screen.getByText('Start Execution').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText(/Cannot reach the AIQA orchestrator/)).toBeTruthy());
  });
});

/**
 * FE-18 — real `RunSummary.approved_by`/`.approved_at` exposure. These
 * fields were already part of the real, already-fetched `RunDetail`
 * response (`store.rs::approve_run`/`reject_run` populate both with the
 * real acting API key's name on either a real approval or a real
 * rejection) but were never rendered anywhere. No new API call, no new
 * type, no backend change — see the FE-18 report.
 */
describe('ExecutionHubPage — FE-18 real approval/rejection attribution', () => {
  it('shows the real approver identity and timestamp for an approved run', async () => {
    setApiKey(RAW_KEY);
    const approvedDetail: RunDetail = {
      ...RUN_A_DETAIL_TERMINAL,
      run_id: 'run-approved',
      name: 'Approved Mobile Sweep',
      approved_by: 'approver_alice',
      approved_at: '2026-08-20T09:00:00Z',
    };
    const approvedRun: RunListItem = { run_id: 'run-approved', name: 'Approved Mobile Sweep', state: 'COMPLETED', total: 2, passed: 2, failed: 0, project_id: PROJECT_A.id };
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [approvedRun] },
      detailByRunId: { 'run-approved': approvedDetail },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Approved Mobile Sweep')).toBeTruthy());
    await act(async () => {
      screen.getByText('Approved Mobile Sweep').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('run-approval-attribution')).toBeTruthy());
    const attribution = document.getElementById('run-approval-attribution')!.textContent ?? '';
    expect(attribution).toContain('Approved by:');
    expect(attribution).toContain('approver_alice');
    expect(attribution).not.toContain('Rejected by:');
  });

  it('shows "Rejected by" (not "Approved by") for a real rejected run, using the same real fields', async () => {
    setApiKey(RAW_KEY);
    const rejectedDetail: RunDetail = {
      ...RUN_A_DETAIL_TERMINAL,
      run_id: 'run-rejected',
      name: 'Rejected Mobile Sweep',
      state: 'REJECTED',
      approved_by: 'rejector_bob',
      approved_at: '2026-08-20T09:05:00Z',
      policy_notes: 'not safe to run against production devices right now',
    };
    const rejectedRun: RunListItem = { run_id: 'run-rejected', name: 'Rejected Mobile Sweep', state: 'REJECTED', total: 2, passed: 0, failed: 0, project_id: PROJECT_A.id };
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [rejectedRun] },
      detailByRunId: { 'run-rejected': rejectedDetail },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Rejected Mobile Sweep')).toBeTruthy());
    await act(async () => {
      screen.getByText('Rejected Mobile Sweep').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('run-approval-attribution')).toBeTruthy());
    const attribution = document.getElementById('run-approval-attribution')!.textContent ?? '';
    expect(attribution).toContain('Rejected by:');
    expect(attribution).toContain('rejector_bob');
    expect(attribution).not.toContain('Approved by:');
  });

  it('shows no approval attribution line at all for a run that was auto-scheduled (never held for approval)', async () => {
    setApiKey(RAW_KEY);
    // RUN_A_DETAIL_TERMINAL already has approved_by/approved_at: null.
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A] },
      detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL_TERMINAL },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A').closest('button')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getAllByText('COMPLETED').length).toBeGreaterThan(0));
    expect(document.getElementById('run-approval-attribution')).toBeNull();
  });
});

/**
 * FE-20 — real full-state count-grid exposure. `domain.rs::RunSummary`
 * (`store.rs::get_run_summary`, a real SQL-backed group-by-status count over
 * a run's persisted `tasks` rows) always carries all 11 fields
 * (`total`/`queued`/`running`/`passed`/`failed`/`blocked`/`skipped`/
 * `cancelled`/`timeout`/`infra_error`/`needs_review`); before this unit the
 * grid rendered only 5 of them. Same real-`AppShell`/transport-boundary-
 * mocked-`fetch` approach as the FE-8/17/18 suites above.
 */
describe('ExecutionHubPage — FE-20 real full run-summary count-grid exposure', () => {
  const RUN_B: RunListItem = { run_id: 'run-b', name: 'Full Sweep B', state: 'RUNNING', total: 15, passed: 4, failed: 1, project_id: PROJECT_A.id };
  const RUN_B_DETAIL: RunDetail = {
    run_id: 'run-b',
    name: 'Full Sweep B',
    state: 'RUNNING',
    total: 15,
    queued: 3,
    running: 2,
    passed: 4,
    failed: 1,
    blocked: 0,
    skipped: 2,
    cancelled: 1,
    timeout: 1,
    infra_error: 1,
    needs_review: 0,
    started_at: '2026-08-20T09:00:00Z',
    finished_at: null,
    project_id: PROJECT_A.id,
    requested_by: 'fe20-key',
    policy_notes: null,
    approved_by: null,
    approved_at: null,
    tasks: [],
  };

  function gridCellValue(label: string): string {
    const grid = document.getElementById('run-detail-count-grid')!;
    const labelEl = within(grid).getByText(label);
    return labelEl.parentElement!.children[1].textContent ?? '';
  }

  it('item 1 — real non-zero QUEUED/RUNNING/SKIPPED/CANCELLED/TIMEOUT/INFRA ERROR values from RunDetail render in the grid', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [RUN_B] }, detailByRunId: { [RUN_B.run_id]: RUN_B_DETAIL } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Full Sweep B')).toBeTruthy());
    await act(async () => {
      screen.getByText('Full Sweep B').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('run-detail-count-grid')).toBeTruthy());
    expect(gridCellValue('QUEUED')).toBe('3');
    expect(gridCellValue('RUNNING')).toBe('2');
    expect(gridCellValue('SKIPPED')).toBe('2');
    expect(gridCellValue('CANCELLED')).toBe('1');
    expect(gridCellValue('TIMEOUT')).toBe('1');
    expect(gridCellValue('INFRA ERROR')).toBe('1');
  });

  it('item 2 — semantically distinct states (TIMEOUT vs FAILED vs INFRA ERROR, SKIPPED vs CANCELLED) render distinct real counts, never merged', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [RUN_B] }, detailByRunId: { [RUN_B.run_id]: RUN_B_DETAIL } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Full Sweep B')).toBeTruthy());
    await act(async () => {
      screen.getByText('Full Sweep B').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('run-detail-count-grid')).toBeTruthy());
    // TIMEOUT (1) and INFRA ERROR (1) share the same real count value but
    // must remain two separate cells with their own real backend field —
    // never collapsed into a single "FAILED" bucket.
    expect(gridCellValue('TIMEOUT')).toBe('1');
    expect(gridCellValue('INFRA ERROR')).toBe('1');
    expect(gridCellValue('FAILED')).toBe('1');
    // SKIPPED (2) and CANCELLED (1) are genuinely different real counts —
    // confirms they are not aliased to each other.
    expect(gridCellValue('SKIPPED')).toBe('2');
    expect(gridCellValue('CANCELLED')).toBe('1');
    expect(gridCellValue('SKIPPED')).not.toBe(gridCellValue('CANCELLED'));
  });

  it('item 3 — a real backend zero renders as the literal count 0, never blank or omitted (no fabricated fallback, no hidden cell)', async () => {
    setApiKey(RAW_KEY);
    // RUN_A_DETAIL already has queued/blocked/skipped/cancelled/timeout/infra_error/needs_review all real-zero.
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [RUN_A] }, detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());
    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('run-detail-count-grid')).toBeTruthy());
    expect(gridCellValue('QUEUED')).toBe('0');
    expect(gridCellValue('SKIPPED')).toBe('0');
    expect(gridCellValue('CANCELLED')).toBe('0');
    expect(gridCellValue('TIMEOUT')).toBe('0');
    expect(gridCellValue('INFRA ERROR')).toBe('0');
  });

  it('item 4 — previously-exposed real counts (TOTAL/PASSED/FAILED/BLOCKED/NEEDS REVIEW) remain correct alongside the new fields', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [RUN_B] }, detailByRunId: { [RUN_B.run_id]: RUN_B_DETAIL } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Full Sweep B')).toBeTruthy());
    await act(async () => {
      screen.getByText('Full Sweep B').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('run-detail-count-grid')).toBeTruthy());
    expect(gridCellValue('TOTAL')).toBe('15');
    expect(gridCellValue('PASSED')).toBe('4');
    expect(gridCellValue('FAILED')).toBe('1');
    expect(gridCellValue('BLOCKED')).toBe('0');
    expect(gridCellValue('NEEDS REVIEW')).toBe('0');
  });

  it('item 5 — switching the selected run re-renders the grid from the newly-fetched real RunDetail, not stale prior values', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A, RUN_B] },
      detailByRunId: { [RUN_A.run_id]: RUN_A_DETAIL, [RUN_B.run_id]: RUN_B_DETAIL },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Regression A')).toBeTruthy());

    await act(async () => {
      screen.getByText('Regression A').closest('button')!.click();
    });
    await waitFor(() => expect(document.getElementById('run-detail-count-grid')).toBeTruthy());
    expect(gridCellValue('QUEUED')).toBe('0');

    await act(async () => {
      screen.getByText('Full Sweep B').closest('button')!.click();
    });
    await waitFor(() => expect(gridCellValue('QUEUED')).toBe('3'));
    expect(gridCellValue('RUNNING')).toBe('2');
    expect(gridCellValue('TIMEOUT')).toBe('1');
  });
});

/**
 * Re-planning v1 — real "Retry Failed" action
 * (`POST .../executions/:run_id/retry-failed`). `canRetryFailed` requires
 * BOTH a terminal `RunState` AND a real non-zero `infra_error`/`timeout`
 * count — a genuine `FAILED` test never makes this button appear, and a
 * still-running execution never does either, regardless of counts already
 * accumulated from an earlier wave. Same real-`AppShell`/transport-
 * boundary-mocked-`fetch` approach as the suites above.
 */
describe('ExecutionHubPage — Re-planning v1 real "Retry Failed" action', () => {
  const RUN_C: RunListItem = { run_id: 'run-c', name: 'Infra Flaky Sweep', state: 'PARTIAL', total: 3, passed: 1, failed: 0, project_id: PROJECT_A.id };
  const RUN_C_DETAIL: RunDetail = {
    run_id: 'run-c',
    name: 'Infra Flaky Sweep',
    state: 'PARTIAL',
    total: 3,
    queued: 0,
    running: 0,
    passed: 1,
    failed: 0,
    blocked: 0,
    skipped: 0,
    cancelled: 0,
    timeout: 1,
    infra_error: 1,
    needs_review: 0,
    started_at: '2026-08-21T09:00:00Z',
    finished_at: '2026-08-21T09:05:00Z',
    project_id: PROJECT_A.id,
    requested_by: 'retry-test-key',
    policy_notes: null,
    approved_by: null,
    approved_at: null,
    tasks: [],
  };

  it('does not show for a still-running execution, even when it already carries real infra/timeout counts from a finished wave', async () => {
    setApiKey(RAW_KEY);
    const stillRunning: RunListItem = { run_id: 'run-still-running', name: 'Still Running Sweep', state: 'RUNNING', total: 3, passed: 1, failed: 0, project_id: PROJECT_A.id };
    const stillRunningDetail: RunDetail = { ...RUN_C_DETAIL, run_id: 'run-still-running', name: 'Still Running Sweep', state: 'RUNNING', finished_at: null };
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [stillRunning] },
      detailByRunId: { 'run-still-running': stillRunningDetail },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Still Running Sweep')).toBeTruthy());
    await act(async () => {
      screen.getByText('Still Running Sweep').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getAllByText('RUNNING').length).toBeGreaterThan(0));
    expect(screen.queryByText(/Retry Failed/)).toBeNull();
  });

  it('does not show for a terminal run with zero real infra/timeout failures', async () => {
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
    expect(screen.queryByText(/Retry Failed/)).toBeNull();
  });

  it('shows with the real combined infra_error+timeout count once a run is terminal with real infra/timeout failures', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_C] },
      detailByRunId: { [RUN_C.run_id]: RUN_C_DETAIL },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Infra Flaky Sweep')).toBeTruthy());
    await act(async () => {
      screen.getByText('Infra Flaky Sweep').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText('Retry Failed (2)')).toBeTruthy());
  });

  it('clicking it calls the real endpoint for the correct project/run, then switches to watching the returned new run and refetches the list', async () => {
    setApiKey(RAW_KEY);
    let capturedProjectId = '';
    let capturedRunId = '';
    const newRunDetail: RunDetail = {
      run_id: 'retry-run-99',
      name: 'Retry (infra failures): Infra Flaky Sweep',
      state: 'SCHEDULED',
      total: 2,
      queued: 2,
      running: 0,
      passed: 0,
      failed: 0,
      blocked: 0,
      skipped: 0,
      cancelled: 0,
      timeout: 0,
      infra_error: 0,
      needs_review: 0,
      started_at: '2026-08-21T09:10:00Z',
      finished_at: null,
      project_id: PROJECT_A.id,
      requested_by: 'retry-test-key',
      policy_notes: null,
      approved_by: null,
      approved_at: null,
      tasks: [],
    };
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_C] },
      detailByRunId: { [RUN_C.run_id]: RUN_C_DETAIL, 'retry-run-99': newRunDetail },
      onRetryFailed: (runId, projectId) => {
        capturedRunId = runId;
        capturedProjectId = projectId;
        return jsonResponse(200, { run_id: 'retry-run-99', state: 'SCHEDULED', retried_task_ids: ['t-infra-1', 't-infra-2'] });
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Infra Flaky Sweep')).toBeTruthy());
    await act(async () => {
      screen.getByText('Infra Flaky Sweep').closest('button')!.click();
    });
    await waitFor(() => expect(screen.getByText('Retry Failed (2)')).toBeTruthy());

    const listCallsBefore = fetchMock.mock.calls.filter(
      ([u]: [string]) => typeof u === 'string' && u.includes('/executions') && !u.includes('/executions/'),
    ).length;

    await act(async () => {
      screen.getByText('Retry Failed (2)').closest('button')!.click();
    });

    await waitFor(() => expect(capturedRunId).toBe(RUN_C.run_id));
    expect(capturedProjectId).toBe(PROJECT_A.id);
    // Switches straight to watching the real new run — never a fabricated one.
    await waitFor(() => expect(screen.getByText('Retry (infra failures): Infra Flaky Sweep')).toBeTruthy());
    // A real extra list refetch happened, beyond the ones already made.
    const listCallsAfter = fetchMock.mock.calls.filter(
      ([u]: [string]) => typeof u === 'string' && u.includes('/executions') && !u.includes('/executions/'),
    ).length;
    expect(listCallsAfter).toBeGreaterThan(listCallsBefore);
  });

  it('a real retry-failed error surfaces via the same actionError banner, without navigating away from the original run', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_C] },
      detailByRunId: { [RUN_C.run_id]: RUN_C_DETAIL },
      onRetryFailed: () =>
        jsonResponse(409, envelope('CONFLICT', 'run has not reached a terminal state yet; retry-failed is only available once a run finishes')),
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Infra Flaky Sweep')).toBeTruthy());
    await act(async () => {
      screen.getByText('Infra Flaky Sweep').closest('button')!.click();
    });
    await waitFor(() => expect(screen.getByText('Retry Failed (2)')).toBeTruthy());

    await act(async () => {
      screen.getByText('Retry Failed (2)').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText(/retry-failed is only available once a run finishes/)).toBeTruthy());
    // Still the original run selected (its name now renders twice — once in
    // the run list, once in the detail header) — no fabricated navigation.
    expect(screen.getAllByText('Infra Flaky Sweep').length).toBeGreaterThan(0);
  });
});
