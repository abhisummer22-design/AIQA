/**
 * Dedicated tests for `TestCaseValidatorPage` — split out of
 * `CaseCPages.test.tsx` (FE-11's original shared "not implemented" suite)
 * once this page's copy was corrected per ACC-005 (22 Aug 2026, following
 * the same investigate-before-building rule that closed ACC-012): the page
 * no longer claims "not implemented" — it honestly redirects to the Test
 * Generator page, where the real capability (validate/approve/reject a
 * saved test case) actually lives. These tests assert the honest copy
 * renders, the old misleading copy is gone, no old fabricated demo content
 * survives, no page-specific backend request is invented, and the "Go to
 * Test Generator" action performs a real navigation.
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

const RAW_KEY = 'aiqa_validatortest0000000';

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

describe('TestCaseValidatorPage (ACC-005 honest redirect)', () => {
  it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp('/validator');

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('test-case-validator-page')).toBeNull();
  });

  it('authenticated renders the honest redirect copy, not the old "not implemented" claim', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/validator');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('test-case-validator-page')).toBeTruthy());
    expect(screen.getAllByText('Test Case Validator').length).toBeGreaterThan(0);
    expect(screen.getByText('Test case validation runs on the Test Generator page')).toBeTruthy();
    expect(screen.getByText(/use Validate \/ Approve \/ Reject/)).toBeTruthy();
    expect(screen.getByText('VIA TEST GENERATOR')).toBeTruthy();
    expect(screen.getByText('IMPLEMENTED ELSEWHERE')).toBeTruthy();

    // The old, misleading "capability doesn't exist" claim must be gone.
    expect(screen.queryByText('NOT IMPLEMENTED')).toBeNull();
    expect(screen.queryByText('NO BACKEND CAPABILITY')).toBeNull();
    expect(screen.queryByText(/test case validation is not implemented/i)).toBeNull();
  });

  it('honestly discloses what real validation does not yet check', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/validator');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('test-case-validator-page')).toBeTruthy());

    // The redirect must not overclaim that the real, structural-only
    // validator satisfies ACC-005's full semantic criteria.
    expect(screen.getByText(/duplicate\/overlap detection, quality-benchmark scoring/)).toBeTruthy();
  });

  it('renders none of the page\'s original fabricated demo content', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/validator');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('test-case-validator-page')).toBeTruthy());

    for (const banned of ['VAL-101', 'TC-AUTH-001', 'Batch Remediate All', 'Auto-Fix & Approve']) {
      expect(screen.queryByText(new RegExp(banned.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))).toBeNull();
    }
  });

  it('makes no real backend request for its own (still nonexistent) dedicated validator endpoint', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend();
    renderApp('/validator');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('test-case-validator-page')).toBeTruthy());

    // Only the real, generic `/projects` list call (auth verification +
    // `ProjectContext`'s own real project selector) is expected — this page
    // still has no dedicated backend endpoint of its own.
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).toMatch(/\/api\/v1\/projects(\?.*)?$/);
    }
  });

  it('"Go to Test Generator" performs a real navigation to the real Test Generator page', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/validator');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('test-case-validator-page')).toBeTruthy());

    await act(async () => {
      screen.getByText('Go to Test Generator').closest('button')!.click();
    });

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(document.getElementById('test-case-validator-page')).toBeNull();
  });

  it('Dashboard remains unregressed with TestCaseValidatorPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });
});
