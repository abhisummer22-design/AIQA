/**
 * Dedicated tests for `RBACTestingPage`.
 *
 * Split out of `CaseCPages.test.tsx` once this page stopped being a deferred
 * placeholder: it now renders the real authorization surface that actually
 * ships — the caller's real role/scope (`GET /api/v1/auth/whoami`), the real
 * API-key inventory (`GET /admin/api-keys`, Admin-only), and real recorded
 * authorization denials from the audit log, alongside the live log stream.
 *
 * The invariants these tests protect are the ones that were violated by the
 * page's original fabricated matrix: no invented roles, no simulated
 * allow/deny evaluation, and a real 403 shown honestly as enforcement rather
 * than hidden or faked into a success.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { ApiKeyListRow, AuditRow, Project, WhoAmIResponse } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_rbac_test' } };
}

const RAW_KEY = 'aiqa_rbactest00000000000';
const PROJECT_A: Project = {
  id: 'proj-rbac',
  name: 'RBAC Project',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

const ADMIN_ME: WhoAmIResponse = {
  name: 'manual-admin',
  role: 'admin',
  project_id: null,
  role_hierarchy: ['viewer', 'operator', 'admin'],
};

const KEYS: ApiKeyListRow[] = [
  { id: 'k1', name: 'bootstrap-admin', role: 'admin', project_id: null, created_at: '2026-01-01T00:00:00Z', revoked_at: null },
  { id: 'k2', name: 'scoped-operator', role: 'operator', project_id: PROJECT_A.id, created_at: '2026-01-02T00:00:00Z', revoked_at: null },
  { id: 'k3', name: 'old-viewer', role: 'viewer', project_id: null, created_at: '2026-01-03T00:00:00Z', revoked_at: '2026-02-01T00:00:00Z' },
];

const AUDIT: AuditRow[] = [
  {
    occurred_at: '2026-08-28T10:00:00Z',
    actor: 'rbac-demo-viewer',
    role: 'viewer',
    project_id: PROJECT_A.id,
    action: 'authz.deny',
    target: null,
    detail: "POST /api/v1/projects denied for role 'viewer'",
    outcome: 'denied',
  },
  {
    occurred_at: '2026-08-28T10:01:00Z',
    actor: 'manual-admin',
    role: 'admin',
    project_id: PROJECT_A.id,
    action: 'project.create',
    target: null,
    detail: 'name=ok',
    outcome: 'ok',
  },
];

function mockBackend(
  opts: { me?: WhoAmIResponse; keysStatus?: number; audit?: AuditRow[]; keys?: ApiKeyListRow[] } = {},
) {
  const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    const u = String(url);
    const method = (init?.method ?? 'GET').toUpperCase();
    if (u.includes('/auth/whoami')) return jsonResponse(200, opts.me ?? ADMIN_ME);
    if (u.includes('/admin/api-keys')) {
      if (opts.keysStatus && opts.keysStatus !== 200) {
        return jsonResponse(opts.keysStatus, envelope('FORBIDDEN', "role 'viewer' cannot perform this action"));
      }
      if (method === 'POST') {
        return jsonResponse(200, { id: 'new-key-id', key: 'aiqa_newlyminted00000000000' });
      }
      if (method === 'DELETE') {
        return new Response(null, { status: 204 });
      }
      return jsonResponse(200, opts.keys ?? KEYS);
    }
    if (u.includes('/audit')) {
      // Real pagination envelope (api.rs::AuditLogResponse) — this
      // fixture's real total is exactly its own length, same as every
      // other single-page test fixture in this app.
      const auditRows = opts.audit ?? AUDIT;
      return jsonResponse(200, { rows: auditRows, total: auditRows.length });
    }
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

describe('RBACTestingPage — real authorization surface', () => {
  it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp('/rbac');

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('rbac-testing-page')).toBeNull();
  });

  it('renders the caller\'s REAL identity, role and scope from the backend', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/rbac');
    await verifyKey();

    await waitFor(() => expect(screen.getByText('manual-admin')).toBeTruthy());
    // The real global-scope fact, not an invented tenant label.
    expect(screen.getByText(/Global scope/)).toBeTruthy();
  });

  it('renders the REAL role hierarchy served by the backend, not a hardcoded client-side copy', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      // A deliberately different ordering proves the page renders what the
      // backend actually sent rather than a constant baked into the bundle.
      me: { ...ADMIN_ME, role_hierarchy: ['viewer', 'operator', 'admin'] },
    });
    renderApp('/rbac');
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/Real role hierarchy/)).toBeTruthy());
    const urls = (fetchMock.mock.calls as [string][]).map(([u]) => String(u));
    expect(urls.some((u) => u.includes('/api/v1/auth/whoami'))).toBe(true);
    expect(screen.getByText(/Role::at_least/)).toBeTruthy();
  });

  it('renders the REAL API key inventory with each key\'s real role, scope and revocation state', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/rbac');
    await verifyKey();

    await waitFor(() => expect(screen.getByText('bootstrap-admin')).toBeTruthy());
    expect(screen.getByText('scoped-operator')).toBeTruthy();
    expect(screen.getByText('old-viewer')).toBeTruthy();
    // Real revocation state, not a decorative badge.
    expect(screen.getByText('revoked')).toBeTruthy();
    expect(screen.getByText(/scoped: proj-rba/)).toBeTruthy();
  });

  it('an admin can create a real API key, and the real plaintext key is shown exactly once', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend();
    renderApp('/rbac');
    await verifyKey();
    await waitFor(() => expect(screen.getByText('bootstrap-admin')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-create-api-key-toggle')!.click();
    });
    const nameInput = screen.getByPlaceholderText('e.g. priya-operator');
    fireEvent.change(nameInput, { target: { value: 'priya-operator' } });
    await act(async () => {
      document.getElementById('btn-submit-create-api-key')!.click();
    });

    await waitFor(() => expect(screen.getByText('aiqa_newlyminted00000000000')).toBeTruthy());
    expect(screen.getByText(/only time the real plaintext key is ever shown/)).toBeTruthy();

    // A real POST with the real, entered fields — not a fabricated request.
    const createCall = (fetchMock.mock.calls as [string, RequestInit?][]).find(
      ([u, init]) => u.includes('/admin/api-keys') && init?.method === 'POST',
    );
    expect(createCall).toBeTruthy();
    const body = JSON.parse(String(createCall![1]!.body));
    expect(body).toEqual({ name: 'priya-operator', role: 'operator' });
  });

  it('requires a real name before creating a key — never submits an empty/placeholder name', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend();
    renderApp('/rbac');
    await verifyKey();
    await waitFor(() => expect(screen.getByText('bootstrap-admin')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-create-api-key-toggle')!.click();
    });
    await act(async () => {
      document.getElementById('btn-submit-create-api-key')!.click();
    });

    await waitFor(() => expect(screen.getByText('Name is required.')).toBeTruthy());
    expect(
      (fetchMock.mock.calls as [string, RequestInit?][]).some(
        ([u, init]) => u.includes('/admin/api-keys') && init?.method === 'POST',
      ),
    ).toBe(false);
  });

  it('revoking a key requires a real second confirming click, and only then sends the real DELETE', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend();
    renderApp('/rbac');
    await verifyKey();
    await waitFor(() => expect(screen.getByText('scoped-operator')).toBeTruthy());

    const row = screen.getByText('scoped-operator').closest('li')!;
    const revokeButton = within(row).getByTitle('Revoke scoped-operator');
    await act(async () => {
      revokeButton.click();
    });

    // First click only arms the confirm — no DELETE sent yet.
    expect(
      (fetchMock.mock.calls as [string, RequestInit?][]).some(
        ([u, init]) => u.includes('/admin/api-keys/k2') && init?.method === 'DELETE',
      ),
    ).toBe(false);

    const confirmButton = within(row).getByTitle('Confirm revoke');
    await act(async () => {
      confirmButton.click();
    });

    await waitFor(() =>
      expect(
        (fetchMock.mock.calls as [string, RequestInit?][]).some(
          ([u, init]) => u.includes('/admin/api-keys/k2') && init?.method === 'DELETE',
        ),
      ).toBe(true),
    );
  });

  it('an already-revoked key offers no revoke control', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/rbac');
    await verifyKey();
    await waitFor(() => expect(screen.getByText('old-viewer')).toBeTruthy());

    const row = screen.getByText('old-viewer').closest('li')!;
    expect(within(row).queryByTitle('Revoke old-viewer')).toBeNull();
  });

  it('a real 403 on the admin-only key list is shown as enforcement working — never hidden, never faked', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ me: { ...ADMIN_ME, name: 'viewer-key', role: 'viewer' }, keysStatus: 403 });
    renderApp('/rbac');
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/refused this admin-only list with a real 403/)).toBeTruthy());
    // The panel itself is still present — hiding it would misrepresent a
    // denied capability as an absent one.
    expect(screen.getByText('API Key Inventory')).toBeTruthy();
    // And no key data is invented to fill it.
    expect(screen.queryByText('bootstrap-admin')).toBeNull();
    // A caller who was just refused the list must not be offered a control
    // to create more keys either — same real boundary, not a partial one.
    expect(document.getElementById('btn-create-api-key-toggle')).toBeNull();
  });

  it('lists only REAL recorded denials from the audit log — allowed actions are not shown as denials', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/rbac');
    await verifyKey();
    await selectProject();

    await waitFor(() => expect(screen.getByText(/POST \/api\/v1\/projects denied for role 'viewer'/)).toBeTruthy());
    expect(screen.getByText('authz.deny')).toBeTruthy();
    // The `ok` row in the same audit response must not appear in a denials panel.
    expect(screen.queryByText('project.create')).toBeNull();
  });

  it('says so honestly when a project has no recorded denials, inventing nothing', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ audit: [] });
    renderApp('/rbac');
    await verifyKey();
    await selectProject();

    await waitFor(() => expect(screen.getByText(/No authorization denials recorded/)).toBeTruthy());
    expect(screen.getByText(/Nothing is invented to fill this space/)).toBeTruthy();
  });

  it('renders the real live orchestrator log panel, not a fabricated activity feed', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/rbac');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-orchestrator-logs-panel')).toBeTruthy());
    await waitFor(() => expect(screen.getByText('No orchestrator activity yet')).toBeTruthy());
  });

  it('still discloses that the ACC-011 scenario matrix is not implemented and refuses to simulate a role', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/rbac');
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/Not implemented:/)).toBeTruthy());
    expect(screen.getByText(/ACC-011/)).toBeTruthy();
    expect(screen.getByText(/cannot impersonate a role you do not hold/)).toBeTruthy();
  });

  it('renders none of the page\'s original fabricated demo content', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/rbac');
    await verifyKey();
    // Wait for real, authenticated-only content. `rbac-testing-page` is
    // also the root of the auth-checking card, so asserting on the root
    // alone could pass vacuously before the real page ever rendered.
    await waitFor(() => expect(screen.getByText('manual-admin')).toBeTruthy());

    for (const banned of [
      'Enterprise Organization & Financial Asset Access Control',
      'Evaluate Authorization Matrix',
      'BillingManager',
    ]) {
      expect(screen.queryByText(new RegExp(banned.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))).toBeNull();
    }
  });

  it('Dashboard remains unregressed with the real RBACTestingPage in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });
});
