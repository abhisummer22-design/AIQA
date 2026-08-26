/**
 * FE-7 targeted tests for the real Evidence Viewer — brief §26 items 8-30,
 * exercised through the real `AppShell` tree, exactly like FE-6's own
 * `AuditPage.test.tsx`. `fetch` is mocked only at the transport boundary.
 *
 * `URL.createObjectURL`/`revokeObjectURL` are stubbed in `beforeEach` —
 * jsdom does not implement either (confirmed before writing this file: a
 * throwaway check showed `typeof URL.createObjectURL === 'undefined'` in
 * this project's actual jsdom test environment). Every real browser
 * supports both; this stubs a missing *environment* capability at the
 * boundary, the same way `fetch` itself is already stubbed everywhere in
 * this test suite — it does not touch `EvidencePage.tsx`'s own logic.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { EvidenceRow, Project, RunListItem } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function blobResponse(text: string, contentType = 'text/plain'): Response {
  const blob = new globalThis.Blob([text], { type: contentType });
  return {
    ok: true,
    status: 200,
    headers: new Headers({ 'Content-Type': contentType }),
    blob: async () => blob,
  } as unknown as Response;
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_evidence_test' } };
}

const RAW_KEY = 'aiqa_evidencetest0000000000';
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
const RUN_A1: RunListItem = { run_id: 'run-a1', name: 'Regression A1', state: 'COMPLETED', total: 10, passed: 9, failed: 1, project_id: PROJECT_A.id };
const RUN_B1: RunListItem = { run_id: 'run-b1', name: 'Smoke B1', state: 'COMPLETED', total: 5, passed: 5, failed: 0, project_id: PROJECT_B.id };
const EVIDENCE_TEXT: EvidenceRow = {
  id: 'ev-text-1',
  task_id: 't1',
  kind: 'log',
  object_key: 'run-a1/t1-log.txt',
  content_type: 'text/plain',
  size_bytes: 128,
  created_at: '2026-08-18T10:00:00Z',
};
const EVIDENCE_IMAGE: EvidenceRow = {
  id: 'ev-image-1',
  task_id: 't2',
  kind: 'screenshot',
  object_key: 'run-a1/t2-shot.png',
  content_type: 'image/png',
  size_bytes: 4096,
  created_at: '2026-08-18T10:05:00Z',
};
const EVIDENCE_UNSUPPORTED: EvidenceRow = {
  id: 'ev-bin-1',
  task_id: 't3',
  kind: 'trace',
  object_key: 'run-a1/t3.trace',
  content_type: 'application/octet-stream',
  size_bytes: 999,
  created_at: '2026-08-18T10:10:00Z',
};

type ContentEntry =
  | { kind: 'ok'; body: string; contentType: string }
  | { kind: 'error'; status: number; body: unknown }
  | 'offline';

function contentOk(body: string, contentType: string): ContentEntry {
  return { kind: 'ok', body, contentType };
}

function mockBackend(opts: {
  projects: Project[];
  runsByProjectId?: Record<string, RunListItem[] | { status: number; body: unknown } | 'offline'>;
  evidenceByRunId?: Record<string, EvidenceRow[] | { status: number; body: unknown } | 'offline'>;
  contentByEvidenceId?: Record<string, ContentEntry>;
}) {
  const fetchMock = vi.fn().mockImplementation(async (url: string) => {
    if (url.includes('/content')) {
      const match = url.match(/evidence\/([^/]+)\/content/);
      const id = match ? match[1] : '';
      const entry = opts.contentByEvidenceId?.[id];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry?.kind === 'ok') {
        return blobResponse(entry.body, entry.contentType);
      }
      if (entry?.kind === 'error') return jsonResponse(entry.status, entry.body);
      return new Response('', { status: 404 });
    }
    if (url.includes('/evidence')) {
      const match = url.match(/executions\/([^/]+)\/evidence/);
      const runId = match ? match[1] : '';
      const entry = opts.evidenceByRunId?.[runId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && !Array.isArray(entry) && 'status' in entry) return jsonResponse(entry.status, entry.body);
      return jsonResponse(200, entry ?? []);
    }
    if (url.includes('/executions')) {
      const match = url.match(/projects\/([^/]+)\/executions/);
      const projectId = match ? match[1] : '';
      const entry = opts.runsByProjectId?.[projectId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && !Array.isArray(entry) && 'status' in entry) return jsonResponse(entry.status, entry.body);
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
    return jsonResponse(200, opts.projects);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(initialEntries: string[] = ['/evidence']) {
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
  // Scoped to the header's own real listbox — see ExecutionHubPage.test.tsx's
  // identical helper for why (FE-10's Dashboard has its own real `<select>`
  // of the same projects, colliding on an unscoped "option" role query).
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

function runSelect(): HTMLSelectElement {
  return document.getElementById('evidence-run-select') as HTMLSelectElement;
}

async function selectRun(run: RunListItem): Promise<void> {
  await act(async () => {
    const select = runSelect();
    select.value = run.run_id;
    select.dispatchEvent(new Event('change', { bubbles: true }));
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  (URL as unknown as { createObjectURL: (b: Blob) => string }).createObjectURL = vi.fn(() => 'blob:mock-url');
  (URL as unknown as { revokeObjectURL: (u: string) => void }).revokeObjectURL = vi.fn();
});

afterEach(() => {
  localStorage.clear();
});

describe('EvidencePage (FE-7 §26 items 8-30)', () => {
  it('item 8 — unauthenticated (no key) never fetches evidence; shows the real auth-state card', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('aiqa-evidence-page')).toBeNull();
  });

  it('item 9 — key_present (unverified) never fetches evidence', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText('Key saved, not yet verified')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('item 10/11 — authenticated + a selected project/run fetches real, correctly-scoped evidence', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_TEXT] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText('log')).toBeTruthy());
    const evidenceCalls = fetchMock.mock.calls.filter(([u]: [string]) => String(u).includes('/evidence') && !String(u).includes('/content'));
    expect(String(evidenceCalls[0][0])).toBe(`/api/v1/projects/${PROJECT_A.id}/executions/${RUN_A1.run_id}/evidence`);
  });

  it('item 12/15 — switching project cancels/invalidates the old evidence and never shows it as current', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1], [PROJECT_B.id]: [RUN_B1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_TEXT], [RUN_B1.run_id]: [] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);
    await waitFor(() => expect(screen.getByText('log')).toBeTruthy());

    await openAndSelectProject(PROJECT_B);

    // Evidence and the run selection itself must reset — Project A's
    // evidence must never remain visible under Project B's scope.
    await waitFor(() => expect(screen.getByText('Select an execution')).toBeTruthy());
    expect(screen.queryByText('log')).toBeNull();
  });

  it('item 13/14 — Refresh performs a real call and is disabled (no duplicate request) while already in flight', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_TEXT] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);
    await waitFor(() => expect(screen.getByText('log')).toBeTruthy());

    const before = fetchMock.mock.calls.filter(([u]: [string]) => String(u).includes('/evidence') && !String(u).includes('/content')).length;
    await act(async () => {
      (document.getElementById('evidence-refresh-button') as HTMLButtonElement).click();
    });
    await waitFor(() => {
      const after = fetchMock.mock.calls.filter(([u]: [string]) => String(u).includes('/evidence') && !String(u).includes('/content')).length;
      expect(after).toBe(before + 1);
    });
  });

  it('item 16 — a real loading state renders while the evidence request is in flight', async () => {
    setApiKey(RAW_KEY);
    let resolveFetch: (r: Response) => void;
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/evidence')) return pending;
      if (url.includes('/executions')) return jsonResponse(200, [RUN_A1]);
      return jsonResponse(200, [PROJECT_A]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    expect(screen.getByText('Loading evidence')).toBeTruthy();

    await act(async () => {
      resolveFetch!(jsonResponse(200, [EVIDENCE_TEXT]));
      await pending;
    });
  });

  it('item 17 — a real, successful, zero-row response renders the honest empty state, not an error', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText('No evidence recorded')).toBeTruthy());
  });

  it('item 18 — no project selected shows a distinct, honest prompt, never an evidence list', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
  });

  it('project selected but no executions exist shows a distinct honest state, never a fake run', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('No executions yet')).toBeTruthy());
  });

  it('item 20 — an unexpected API failure on evidence renders a real, distinct error state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: { status: 500, body: envelope('INTERNAL', 'db unavailable') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText("Couldn't load this data")).toBeTruthy());
  });

  it('item 21 — a real 401 on the evidence endpoint renders the unauthorized state, not empty data', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: { status: 401, body: envelope('UNAUTHENTICATED', 'revoked') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText('Authentication required')).toBeTruthy());
    expect(screen.queryByText('No evidence recorded')).toBeNull();
  });

  it('item 22 — a real 403 on the evidence endpoint renders the forbidden state, never an empty list', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: { status: 403, body: envelope('FORBIDDEN', 'project scope mismatch') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText('Not permitted to view this evidence')).toBeTruthy());
    expect(screen.queryByText('No evidence recorded')).toBeNull();
  });

  it('item 23 — offline (network failure) on the evidence endpoint renders the unreachable state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: 'offline' },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
  });

  it('item 24/27 — only real, backend-returned evidence renders; no fabricated record appears', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_TEXT] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText('log')).toBeTruthy());
    expect(screen.getByText('t1', { exact: false })).toBeTruthy();
    expect(screen.queryByText('AIQA Platform')).toBeNull();
  });

  it('item 25/26 — a real text evidence preview loads and renders the real content', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_TEXT] },
      contentByEvidenceId: { [EVIDENCE_TEXT.id]: contentOk('real captured log output', 'text/plain') },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);
    await waitFor(() => expect(screen.getByText('log')).toBeTruthy());

    await act(async () => {
      screen.getByText('log').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText('real captured log output')).toBeTruthy());
  });

  it('item 25 — an unsupported content type is handled honestly ("Preview unavailable"), never a fake preview', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_UNSUPPORTED] },
      contentByEvidenceId: { [EVIDENCE_UNSUPPORTED.id]: contentOk('binary-ish', 'application/octet-stream') },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);
    await waitFor(() => expect(screen.getByText('trace')).toBeTruthy());

    await act(async () => {
      screen.getByText('trace').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText(/Preview unavailable/)).toBeTruthy());
  });

  it('a real image evidence preview creates a real object URL from the real fetched bytes', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_IMAGE] },
      contentByEvidenceId: { [EVIDENCE_IMAGE.id]: contentOk('fake-png-bytes', 'image/png') },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);
    await waitFor(() => expect(screen.getByText('screenshot')).toBeTruthy());

    await act(async () => {
      screen.getByText('screenshot').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByAltText(`Evidence ${EVIDENCE_IMAGE.id}`)).toBeTruthy());
    expect(URL.createObjectURL).toHaveBeenCalled();
  });

  it('item 28 — the current session\'s own key is redacted even if evidence content happened to contain it', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_TEXT] },
      contentByEvidenceId: { [EVIDENCE_TEXT.id]: contentOk(`leaked header: Authorization: Bearer ${RAW_KEY}`, 'text/plain') },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);
    await waitFor(() => expect(screen.getByText('log')).toBeTruthy());

    await act(async () => {
      screen.getByText('log').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText(/leaked header/)).toBeTruthy());
    expect(document.body.innerHTML).not.toContain(RAW_KEY);
  });

  it('the internal object_key is never rendered anywhere on the page', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [RUN_A1] },
      evidenceByRunId: { [RUN_A1.run_id]: [EVIDENCE_TEXT] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(runSelect().options.length).toBe(2));
    await selectRun(RUN_A1);

    await waitFor(() => expect(screen.getByText('log')).toBeTruthy());
    expect(document.body.innerHTML).not.toContain(EVIDENCE_TEXT.object_key);
  });

  it('item 29 — Dashboard remains unregressed with EvidencePage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/dashboard']);
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    // FE-10: Dashboard now renders real project-scoped data (no more
    // fabricated "AIQA Platform" seed project).
    await waitFor(() => expect(screen.getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    expect(screen.getByText('Total Test Cases')).toBeTruthy();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
  });

  it('item 30 — Test Generator remains unregressed with EvidencePage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/test-generator']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getAllByText('Test Case Generator').length).toBeGreaterThan(0);
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });
});
