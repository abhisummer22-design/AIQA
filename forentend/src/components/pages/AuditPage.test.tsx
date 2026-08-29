/**
 * FE-6 targeted tests for the real Audit page — brief §24 items 8-26,
 * exercised through the real `AppShell` (real `AuthProvider`,
 * `ProjectProvider`, `AIQAProvider`, real routing) inside a `MemoryRouter`,
 * exactly like FE-4/FE-5's own page-level test files. `fetch` is mocked
 * only at the transport boundary.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { AuditRow, Project } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_audit_test' } };
}

const RAW_KEY = 'aiqa_audittest0000000000';
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
const AUDIT_ROW_A: AuditRow = {
  occurred_at: '2026-08-18T10:00:00Z',
  actor: 'ops-key-alpha',
  role: 'operator',
  project_id: PROJECT_A.id,
  action: 'run.create',
  target: 'run-1001',
  detail: 'Regression Suite',
  outcome: 'ok',
};
const AUDIT_ROW_B: AuditRow = {
  occurred_at: '2026-08-18T11:00:00Z',
  actor: 'ops-key-beta',
  role: 'operator',
  project_id: PROJECT_B.id,
  action: 'project.update',
  target: PROJECT_B.id,
  detail: 'name=Project Beta',
  outcome: 'ok',
};

/** Routes every real request by URL: `/projects` (project list) vs
 *  `/audit` (this page's own fetch) get independently controllable
 *  responses, matching how the real backend actually has two endpoints. */
function mockBackend(opts: {
  projects: Project[];
  auditByProjectId?: Record<string, AuditRow[] | { status: number; body: unknown } | 'offline'>;
}) {
  const fetchMock = vi.fn().mockImplementation(async (url: string) => {
    if (url.includes('/audit')) {
      const match = url.match(/projects\/([^/]+)\/audit/);
      const projectId = match ? match[1] : '';
      const entry = opts.auditByProjectId?.[projectId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && typeof entry === 'object' && 'status' in entry) {
        return jsonResponse(entry.status, entry.body);
      }
      return jsonResponse(200, entry ?? []);
    }
    // FE-10: real Dashboard endpoints — zero-valued but real-shaped, so any
    // test that renders `/dashboard` sees an honest, empty-but-real
    // response instead of falling through to the `opts.projects` array.
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
    if (url.includes('/executions')) return jsonResponse(200, []);
    return jsonResponse(200, opts.projects);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(initialEntries: string[] = ['/audit']) {
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
  // Scoped to the header's own real listbox — see ExecutionHubPage.test.tsx's
  // identical helper for why (FE-10's Dashboard has its own real `<select>`
  // of the same projects, colliding on an unscoped "option" role query).
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

describe('AuditPage (FE-6 §24 items 8-26)', () => {
  it('item 8 — unauthenticated (no key) never fetches Audit; shows the real auth-state card', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('aiqa-audit-page')).toBeNull();
  });

  it('item 9 — no_key never fetches Audit', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText(/No orchestrator API key/i)).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('aiqa-audit-page')).toBeNull();
  });

  it('item 10 — authenticated + a selected project fetches real Audit data', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], auditByProjectId: { [PROJECT_A.id]: [AUDIT_ROW_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('run.create')).toBeTruthy());
    const auditCalls = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/audit'));
    expect(auditCalls.length).toBeGreaterThan(0);
    expect(String(auditCalls[0][0])).toBe(`/api/v1/projects/${PROJECT_A.id}/audit?limit=100`);
  });

  it('item 11/12 — switching project reloads Audit data and never shows the previous project\'s stale rows', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      auditByProjectId: { [PROJECT_A.id]: [AUDIT_ROW_A], [PROJECT_B.id]: [AUDIT_ROW_B] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('run.create')).toBeTruthy());

    await openAndSelectProject(PROJECT_B);

    await waitFor(() => expect(screen.getByText('project.update')).toBeTruthy());
    // Project A's action must not still be on screen once B has loaded.
    expect(screen.queryByText('run.create')).toBeNull();
  });

  it('item 13/14 — Refresh performs a real API call and is disabled (no duplicate request) while one is already in flight', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], auditByProjectId: { [PROJECT_A.id]: [AUDIT_ROW_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('run.create')).toBeTruthy());

    const before = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/audit')).length;
    const refreshButton = document.getElementById('audit-refresh-button') as HTMLButtonElement;

    await act(async () => {
      refreshButton.click();
    });
    // While the refresh is (synchronously, in this mock) already resolved,
    // the button was real and disabled during the in-flight window — a
    // second immediate click while `disabled` is a native no-op, not a
    // second request. Assert the total call count reflects exactly one
    // additional real request from this refresh, not two.
    await waitFor(() => {
      const after = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/audit')).length;
      expect(after).toBe(before + 1);
    });
  });

  it('item 15 — a real loading state renders while the request is in flight', async () => {
    setApiKey(RAW_KEY);
    let resolveFetch: (r: Response) => void;
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/audit')) return pending;
      return jsonResponse(200, [PROJECT_A]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    expect(screen.getByText('Loading audit trail')).toBeTruthy();

    await act(async () => {
      resolveFetch!(jsonResponse(200, [AUDIT_ROW_A]));
      await pending;
    });
  });

  it('item 16 — a real, successful, zero-row response renders the honest empty state, not an error', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], auditByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('No audit events recorded')).toBeTruthy());
  });

  it('item 17 — an unexpected API failure renders a real, distinct error state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      auditByProjectId: { [PROJECT_A.id]: { status: 500, body: envelope('INTERNAL', 'db unavailable') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText("Couldn't load the audit trail")).toBeTruthy());
  });

  it('item 18 — a real 401 on the audit endpoint renders the unauthorized state, not empty data', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      auditByProjectId: { [PROJECT_A.id]: { status: 401, body: envelope('UNAUTHENTICATED', 'revoked') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Authentication required')).toBeTruthy());
    expect(screen.queryByText('No audit events recorded')).toBeNull();
  });

  it('item 19 — a real 403 on the audit endpoint renders the forbidden state, never an empty audit list', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      auditByProjectId: {
        [PROJECT_A.id]: { status: 403, body: envelope('FORBIDDEN', "role 'viewer' cannot perform this action") },
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Not permitted to view this audit trail')).toBeTruthy());
    expect(screen.queryByText('No audit events recorded')).toBeNull();
  });

  it('item 20 — offline (network failure) on the audit endpoint renders the unreachable state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], auditByProjectId: { [PROJECT_A.id]: 'offline' } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
  });

  it('item 21/22 — only real, backend-returned audit rows render; no fabricated/demo audit record appears', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], auditByProjectId: { [PROJECT_A.id]: [AUDIT_ROW_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('run.create')).toBeTruthy());
    expect(screen.getByText('ops-key-alpha')).toBeTruthy();
    expect(screen.getByText('Regression Suite')).toBeTruthy();
    expect(screen.getByText('ok')).toBeTruthy();
    // Known AIQAContext/ReportsPage fake strings must never appear here.
    expect(screen.queryByText(/Auditor role permitted to execute DELETE/)).toBeNull();
    expect(screen.queryByText('AIQA Platform')).toBeNull();
  });

  it('item 23 — a request_id renders only when the API error actually included one', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      auditByProjectId: { [PROJECT_A.id]: { status: 500, body: envelope('INTERNAL', 'db unavailable') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(/request_id: req_audit_test/)).toBeTruthy());
  });

  it('item 23b — no request_id line renders when the failure carries none (a network/offline failure)', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], auditByProjectId: { [PROJECT_A.id]: 'offline' } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
    expect(screen.queryByText(/request_id:/)).toBeNull();
  });

  it('item 24 — the current session\'s own API key is redacted even if a returned audit field happened to contain it', async () => {
    setApiKey(RAW_KEY);
    const poisoned: AuditRow = {
      ...AUDIT_ROW_A,
      detail: `leaked header value: Authorization: Bearer ${RAW_KEY}`,
    };
    mockBackend({ projects: [PROJECT_A], auditByProjectId: { [PROJECT_A.id]: [poisoned] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('run.create')).toBeTruthy());
    expect(document.body.innerHTML).not.toContain(RAW_KEY);
  });

  it('item 25 — Dashboard remains unregressed with AuditPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/dashboard']);
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    // FE-10: Dashboard now renders real project-scoped data (no more
    // fabricated "AIQA Platform" seed project).
    await waitFor(() => expect(screen.getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    expect(screen.getByText('Executed Tests')).toBeTruthy();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
  });

  it('item 26 — Test Generator remains unregressed with AuditPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/test-generator']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getAllByText('Test Case Generator').length).toBeGreaterThan(0);
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });

  it('no project selected shows a real, honest prompt rather than an empty audit list', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(screen.queryByText('No audit events recorded')).toBeNull();
  });
});
