/**
 * Dedicated tests for `TestCaseValidatorPage`.
 *
 * History: this file previously asserted the ACC-005 *redirect* copy — the
 * page pointed at the Test Generator instead of doing anything itself. The
 * page now hosts the real validate/approve/reject lifecycle directly
 * (`GET .../test-cases`, `POST .../test-cases/:id/validate|approve|reject`),
 * so those copy assertions were replaced with assertions about the real
 * behavior. What is deliberately unchanged: the page must still make no
 * fabricated claim, must still disclose that validation is structural only,
 * and must still not render any of the original fabricated demo content.
 *
 * The Test Generator's own validate/approve/reject controls are a separate
 * surface onto the same real endpoints and are covered by
 * `TestCaseGeneratorPage.test.tsx` — untouched here.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { Project, TestCaseRow } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_validator_test' } };
}

const RAW_KEY = 'aiqa_validatortest0000000';
const PROJECT_A: Project = {
  id: 'proj-val',
  name: 'Validator Project',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

function testCaseRow(overrides: Partial<TestCaseRow> = {}): TestCaseRow {
  return {
    id: 'case-1',
    project_id: PROJECT_A.id,
    title: 'Reject an expired session token',
    description: 'A real, persisted case.',
    category: 'Security',
    priority: 'HIGH',
    severity: 'MAJOR',
    preconditions: [],
    steps: [{ step_number: 1, action: 'Submit expired token', expected_result: '401 returned' }],
    expected_result: '401 Unauthorized',
    assertions: [],
    test_data: null,
    source_document: null,
    requirement_reference: null,
    source_requirement: null,
    traceability: null,
    risk_factor: null,
    tags: [],
    source: 'generator',
    status: 'GENERATED',
    validation_notes: null,
    executed_run_id: null,
    executed_task_id: null,
    created_by: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

/** Real-shaped transport mock. `onAction` lets a test control what the real
 *  `validate|approve|reject` endpoint actually returns for that call. */
function mockBackend(opts: {
  cases?: TestCaseRow[];
  onAction?: (action: string, url: string) => Response;
} = {}) {
  const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (init?.method === 'POST' && /\/test-cases\/[^/]+\/(validate|approve|reject)$/.test(u)) {
      const action = u.split('/').pop()!;
      return opts.onAction ? opts.onAction(action, u) : jsonResponse(200, testCaseRow({ status: 'VALIDATED' }));
    }
    if (u.includes('/test-cases')) return jsonResponse(200, opts.cases ?? []);
    if (u.includes('/logs')) return jsonResponse(200, []);
    return jsonResponse(200, [PROJECT_A]);
  });
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

async function selectProject(): Promise<void> {
  await waitFor(() => expect(document.getElementById('project-selector-button')).toBeTruthy());
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: PROJECT_A.name }).click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('TestCaseValidatorPage — real validation lifecycle', () => {
  it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp('/validator');

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('test-case-validator-page')).toBeNull();
  });

  it('with no project selected it asks for one and fetches no test cases', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend();
    renderApp('/validator');
    await verifyKey();

    // `test-case-validator-page` shares its id across the auth-checking
    // card and the real content — waiting on it alone races the auth
    // transition on a slower CI runner. Wait for the real, authenticated-
    // only empty state instead.
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    const urls = (fetchMock.mock.calls as [string][]).map(([u]) => String(u));
    expect(urls.some((u) => u.includes('/test-cases'))).toBe(false);
  });

  it('loads and renders the REAL persisted test cases for the selected project', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      cases: [
        testCaseRow({ id: 'c1', title: 'Real generated case', status: 'GENERATED' }),
        testCaseRow({ id: 'c2', title: 'Real validated case', status: 'VALIDATED', validation_notes: 'structural validation passed' }),
      ],
    });
    renderApp('/validator');
    await verifyKey();
    await selectProject();

    await waitFor(() => expect(screen.getByText('Real generated case')).toBeTruthy());
    expect(screen.getByText('Real validated case')).toBeTruthy();
    // The backend's own note is rendered verbatim, not restated client-side.
    expect(screen.getByText('structural validation passed')).toBeTruthy();
  });

  it('Validate calls the real endpoint and applies the real returned status — no optimistic guess', async () => {
    setApiKey(RAW_KEY);
    let posted: string | null = null;
    const fetchMock = mockBackend({
      cases: [testCaseRow({ id: 'c1', title: 'Needs validation', status: 'GENERATED' })],
      onAction: (action, url) => {
        posted = url;
        expect(action).toBe('validate');
        return jsonResponse(
          200,
          testCaseRow({ id: 'c1', title: 'Needs validation', status: 'VALIDATED', validation_notes: 'structural validation passed: title, description, steps and expected result are present' }),
        );
      },
    });
    renderApp('/validator');
    await verifyKey();
    await selectProject();
    await waitFor(() => expect(document.getElementById('btn-validate-c1')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-validate-c1')!.click();
    });

    await waitFor(() => expect(screen.getByText('VALIDATED')).toBeTruthy());
    expect(String(posted)).toMatch(/\/api\/v1\/projects\/proj-val\/test-cases\/c1\/validate$/);
    expect(screen.getByText(/structural validation passed: title, description/)).toBeTruthy();
    expect(fetchMock).toHaveBeenCalled();
  });

  it('a real 409 structural failure is shown verbatim on that row — never a fabricated success', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      cases: [testCaseRow({ id: 'c1', title: 'Broken case', status: 'GENERATED' })],
      onAction: () => jsonResponse(409, envelope('CONFLICT', 'structural validation failed: no steps defined')),
    });
    renderApp('/validator');
    await verifyKey();
    await selectProject();
    await waitFor(() => expect(document.getElementById('btn-validate-c1')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-validate-c1')!.click();
    });

    await waitFor(() => expect(screen.getByText(/structural validation failed: no steps defined/)).toBeTruthy());
    // Status must NOT have advanced.
    expect(screen.getByText('GENERATED')).toBeTruthy();
    expect(screen.queryByText('VALIDATED')).toBeNull();
  });

  it('only offers actions the real backend state machine actually accepts', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      cases: [
        testCaseRow({ id: 'gen', title: 'Generated one', status: 'GENERATED' }),
        testCaseRow({ id: 'val', title: 'Validated one', status: 'VALIDATED' }),
        testCaseRow({ id: 'app', title: 'Approved one', status: 'APPROVED' }),
      ],
    });
    renderApp('/validator');
    await verifyKey();
    await selectProject();
    await waitFor(() => expect(screen.getByText('Generated one')).toBeTruthy());

    // GENERATED: validate + reject, never approve.
    expect(document.getElementById('btn-validate-gen')).toBeTruthy();
    expect(document.getElementById('btn-reject-gen')).toBeTruthy();
    expect(document.getElementById('btn-approve-gen')).toBeNull();

    // VALIDATED: approve + reject, never re-validate.
    expect(document.getElementById('btn-approve-val')).toBeTruthy();
    expect(document.getElementById('btn-reject-val')).toBeTruthy();
    expect(document.getElementById('btn-validate-val')).toBeNull();

    // APPROVED: terminal for this page — no control the backend would 409.
    expect(document.getElementById('btn-validate-app')).toBeNull();
    expect(document.getElementById('btn-approve-app')).toBeNull();
    expect(document.getElementById('btn-reject-app')).toBeNull();
  });

  it('honestly discloses that validation is structural only', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/validator');
    await verifyKey();

    // Same shared-id race as above — wait for real, authenticated-only
    // content rather than the root, which also renders during auth-checking.
    await waitFor(() => expect(screen.getByText(/Validation is structural only/)).toBeTruthy());
    expect(screen.getByText(/Duplicate detection, quality\s+scoring/)).toBeTruthy();
  });

  it('renders the real live orchestrator log panel, not a fabricated activity feed', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/validator');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-orchestrator-logs-panel')).toBeTruthy());
    // The panel loads its real history before it can say anything about
    // content, so wait for that to settle rather than racing it.
    await waitFor(() => expect(screen.getByText('No orchestrator activity yet')).toBeTruthy());
  });

  it('renders none of the page\'s original fabricated demo content', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ cases: [testCaseRow()] });
    renderApp('/validator');
    await verifyKey();
    await selectProject();
    // Wait for the real persisted case to actually render — proof we are
    // past the auth-checking card and looking at real content, not just the
    // shared root id (which both states render).
    await waitFor(() => expect(screen.getByText(testCaseRow().title)).toBeTruthy());

    for (const banned of ['VAL-101', 'TC-AUTH-001', 'Batch Remediate All', 'Auto-Fix & Approve']) {
      expect(screen.queryByText(new RegExp(banned.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))).toBeNull();
    }
  });

  it('a real load failure replaces the queue outright — no stale or invented rows', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        const u = String(url);
        if (u.includes('/test-cases')) return jsonResponse(500, envelope('INTERNAL', 'boom'));
        if (u.includes('/logs')) return jsonResponse(200, []);
        return jsonResponse(200, [PROJECT_A]);
      }),
    );
    renderApp('/validator');
    await verifyKey();
    await selectProject();

    await waitFor(() => expect(screen.getByText("Couldn't load test cases")).toBeTruthy());
    expect(screen.queryByText('Reject an expired session token')).toBeNull();
  });

  it('Dashboard remains unregressed with TestCaseValidatorPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });
});
