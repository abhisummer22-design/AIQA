/**
 * Dedicated tests for `APITestingPage` — split out of `CaseCPages.test.tsx`
 * (FE-11's original shared "not implemented" suite) once this page's copy
 * was corrected per `AIQA_PRODUCT_DECISIONS_RECORD.md` decision #3
 * (ACC-012, 22 Aug 2026): the page no longer claims "not implemented" — it
 * honestly redirects to the Execution Hub, where the real capability
 * (choosing the pytest/Newman/Schemathesis executor for a task) actually
 * lives. These tests assert the honest copy renders, the old misleading
 * copy is gone, no old fabricated demo content survives, no page-specific
 * backend request is invented, and the "Go to Execution Hub" action
 * performs a real navigation to the real Execution Hub page.
 */
import React from 'react';
import { act, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const RAW_KEY = 'aiqa_apitesttest0000000000';

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
  await act(async () => {
    screen.getByText('Verify Connection').closest('button')!.click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('APITestingPage (ACC-012 honest redirect)', () => {
  it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp('/api-testing');

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('api-testing-page')).toBeNull();
  });

  it('authenticated renders the honest redirect copy, not the old "not implemented" claim', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/api-testing');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('api-testing-page')).toBeTruthy());
    expect(screen.getAllByText('API Testing & Protocol Adapters').length).toBeGreaterThan(0);
    expect(screen.getByText('API testing runs through the Execution Hub')).toBeTruthy();
    expect(screen.getByText(/set a task's executor to pytest, Newman, or Schemathesis/)).toBeTruthy();
    expect(screen.getByText('VIA EXECUTION HUB')).toBeTruthy();
    expect(screen.getByText('IMPLEMENTED ELSEWHERE')).toBeTruthy();

    // The old, misleading "capability doesn't exist" claim must be gone.
    expect(screen.queryByText('NOT IMPLEMENTED')).toBeNull();
    expect(screen.queryByText('NO BACKEND CAPABILITY')).toBeNull();
    expect(screen.queryByText(/API test harness is not implemented/)).toBeNull();
  });

  it('renders none of the page\'s original fabricated demo content', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/api-testing');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('api-testing-page')).toBeTruthy());

    for (const banned of ['Create Ephemeral Tenant Session', 'Execute Endpoint', 'REST_HTTP']) {
      expect(screen.queryByText(new RegExp(banned.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))).toBeNull();
    }
  });

  it('makes no real backend request for its own (still nonexistent) request-builder capability', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend();
    renderApp('/api-testing');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('api-testing-page')).toBeTruthy());

    // Only the real, generic `/projects` list call (auth verification +
    // `ProjectContext`'s own real project selector) is expected — this page
    // still has no dedicated backend endpoint of its own, and none should be
    // invented client-side just because the messaging changed.
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).toMatch(/\/api\/v1\/projects(\?.*)?$/);
    }
  });

  it('"Go to Execution Hub" performs a real navigation to the real Execution Hub page', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/api-testing');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('api-testing-page')).toBeTruthy());

    await act(async () => {
      screen.getByText('Go to Execution Hub').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('aiqa-execution-hub-page')).toBeTruthy());
    expect(document.getElementById('api-testing-page')).toBeNull();
  });

  it('Dashboard remains unregressed with APITestingPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });
});
