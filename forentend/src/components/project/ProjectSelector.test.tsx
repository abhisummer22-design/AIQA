/**
 * FE-5 targeted tests for the real project selector's UI consumption of
 * `ProjectContext` — items 17-20 of the brief's minimum test list (§20).
 * Renders the real `AppShell` (real providers, real `Header`/`Sidebar`,
 * real routed pages) inside a `MemoryRouter`, exactly like FE-4's own
 * `AppRoutes.test.tsx`. `fetch` is mocked only at the transport boundary.
 *
 * Note: this context deliberately never auto-selects a project, even when
 * there is exactly one (`ProjectContext.tsx`'s own documented judgment
 * call — brief §7 Case B explicitly warns against assuming auto-selection
 * without documentation support). So a freshly authenticated session shows
 * "Select a project", not the project's name, until a real selection is
 * made — these tests open the dropdown rather than assuming auto-select.
 */
import React from 'react';
import { act, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { Project } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const REAL_PROJECT: Project = {
  id: 'proj-real-selector',
  name: 'Real Selector Project',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};
const RAW_KEY = 'aiqa_selectortest00000000';

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

function selectorButton(): HTMLElement {
  return document.getElementById('project-selector-button')!;
}

/** Waits for the real project fetch (triggered by `verifyKey()`) to finish. */
async function waitForProjectsLoaded(): Promise<void> {
  await waitFor(() => expect(selectorButton().textContent).not.toContain('Loading projects'));
}

async function openSelector(): Promise<void> {
  await act(async () => {
    selectorButton().click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('ProjectSelector (FE-5 §20 items 17-20)', () => {
  it('item 17 — the raw API key never appears anywhere in the rendered selector output', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();

    await waitFor(() => expect(screen.getByRole('option', { name: REAL_PROJECT.name })).toBeTruthy());

    expect(document.body.innerHTML).not.toContain(RAW_KEY);
  });

  it('item 18 — selecting a project never puts anything into the URL', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();
    await waitFor(() => expect(screen.getByRole('option', { name: REAL_PROJECT.name })).toBeTruthy());

    await act(async () => {
      screen.getByRole('option', { name: REAL_PROJECT.name }).click();
    });

    // The selection did take effect (button now shows the real name)...
    await waitFor(() => expect(selectorButton().textContent).toContain(REAL_PROJECT.name));
    // ...but context-only scope (this sprint's brief §15: no documented
    // URL/route requirement was found, so selection stays centralized in
    // context, never the URL) — and never a credential either way (§18).
    expect(window.location.href).not.toContain(RAW_KEY);
    expect(window.location.search).toBe('');
    expect(window.location.pathname).not.toContain(REAL_PROJECT.id);
  });

  it('item 19 — Dashboard still renders its existing product-owner-approved content unregressed', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp(['/dashboard']);
    await verifyKey();

    // Dashboard's own pre-existing fabricated content (AIQAContext-driven,
    // untouched by FE-5 — see `DashboardPage.tsx`'s own FE-5 comment).
    await waitFor(() => expect(screen.getAllByText('AIQA Platform').length).toBeGreaterThan(0));
    expect(screen.getByText('Total Test Cases')).toBeTruthy();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
    // The real project selector renders alongside it, unrelated to Dashboard's own fake selector.
    await waitForProjectsLoaded();
    expect(selectorButton()).toBeTruthy();
  });

  it('item 20 — Test Generator still renders its existing product-owner-approved content unregressed', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp(['/test-generator']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    // "Test Case Generator" is legitimately ambiguous (Sidebar label + page
    // heading, pre-existing — see FE-4's own report §"item 18" fix).
    expect(screen.getAllByText('Test Case Generator').length).toBeGreaterThan(0);
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
    await waitForProjectsLoaded();
    expect(selectorButton()).toBeTruthy();
  });

  it('empty accessible-projects state renders a real empty state, not a fabricated project', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [])));
    renderApp();
    await verifyKey();

    await waitFor(() => expect(selectorButton().textContent).toContain('No accessible projects'));

    await openSelector();
    expect(screen.getByText(/has no projects assigned/)).toBeTruthy();
  });

  it('single accessible project is not auto-selected (brief §7 Case B — no documentation supports it)', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp();
    await verifyKey();

    await waitFor(() => expect(selectorButton().textContent).toContain('Select a project'));
  });
});
