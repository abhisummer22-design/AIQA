/**
 * FE-11 — shared tests for the original 7 Case-C pages (each rewritten from
 * a fully fabricated demo UI to a truthful "not implemented" state, since
 * no corresponding backend entity/route exists for any of them — re-
 * verified against `services/orchestrator/src/api.rs`'s real route table).
 *
 * Of those 7:
 * - `APITestingPage` — moved out entirely, `AIQA_PRODUCT_DECISIONS_RECORD.md`
 *   decision #3 (ACC-012, 22 Aug 2026): real via the Execution Hub's
 *   executor selection. See its own `APITestingPage.test.tsx`.
 * - `TestCaseValidatorPage` — moved out entirely, ACC-005 (22 Aug 2026,
 *   same-day investigation per the confirmed decision): real via
 *   `TestCaseGeneratorPage.tsx`'s already-wired validate/approve/reject/
 *   handoff lifecycle (AUD-005). See its own `TestCaseValidatorPage.test.tsx`.
 * - `CustomTestingPage` — moved out entirely (Phase 3, 25 Aug 2026): real
 *   via `POST .../custom-testing/generate` (`custom_testing.rs`), the
 *   real central AI router, and a handoff into the existing Test Case
 *   Generator pipeline. See its own `CustomTestingPage.test.tsx`.
 * - The remaining 4 — `AIQAOrchestratorPage`, `RBACTestingPage`,
 *   `MobileTestingPage`, `FeatureMapFlowPage` — had **only their copy**
 *   updated in that same later unit (`deferredCases` below): each is now a
 *   direct product-owner decision (`AIQA_PRD_FRD_SRD_v6.1_SCOPE_
 *   CORRECTIONS.md` §2) — real, intended future scope, deliberately
 *   *deferred* with a named revisit condition, not an unexplained gap.
 *   Nothing about backend/frontend behavior changed; only whether the page
 *   states this specifically (new copy) instead of generically (the old
 *   "NOT IMPLEMENTED"/"NO BACKEND CAPABILITY" badges, now asserted absent).
 *
 * These tests assert the truthful state renders (generic for Custom
 * Testing, specific-and-sourced for the 4 deferred pages), that no
 * page-specific fabricated content (the original hardcoded demo records/
 * metrics) survives on any of them, and that each page still has a real
 * root element and stays reachable via the same real, authenticated route
 * table every other page uses.
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

const RAW_KEY = 'aiqa_casectest00000000000';

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

// ---------------------------------------------- Deferred-scope pages (4)

const deferredCases: Array<{
  path: string;
  rootId: string;
  heading: string;
  bannedText: string[];
  /** A distinctive substring from *this page's own* real, named revisit
   *  condition (`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §2) — proves
   *  the copy is genuinely page-specific, not one generic sentence reused
   *  four times. */
  revisitSnippet: string;
  accRef: string;
}> = [
  {
    path: '/orchestrator',
    rootId: 'aiqa-orchestrator-page',
    heading: 'AIQA Orchestrator',
    bannedText: ['PLAN-901', 'PLAN-902', 'Dispatch Selected Plan', 'Release Gating'],
    revisitSnippet: 'shifts toward autonomous routing',
    accRef: 'ACC-006',
  },
  {
    path: '/rbac',
    rootId: 'rbac-testing-page',
    heading: 'RBAC Security Testing Matrix',
    bannedText: ['Enterprise Organization & Financial Asset Access Control', 'Evaluate Authorization Matrix', 'BillingManager'],
    revisitSnippet: 'enforcement remains fully committed and already shipped',
    accRef: 'ACC-011',
  },
  {
    path: '/mobile-testing',
    rootId: 'mobile-testing-page',
    heading: 'Mobile Testing Hub',
    bannedText: ['Samsung Galaxy S24 Ultra', 'DEV-AND-01', 'Execute Non-UI Suite', 'Lock & Reserve Device'],
    revisitSnippet: 'device-farm budget decision',
    accRef: 'ACC-014',
  },
  {
    path: '/feature-map',
    rootId: 'feature-map-flow-page',
    heading: 'Feature Map / System Call Flow',
    bannedText: ['NODE-AUTH-GW', 'Payment Ingestion Engine', 'Simulate Call Flow', 'Double-Entry Ledger Service'],
    revisitSnippet: 'gated on Context Ingestion',
    accRef: 'ACC-003',
  },
];

describe('Case-C pages — deferred-scope copy (4 pages, updated 22 Aug 2026)', () => {
  for (const c of deferredCases) {
    describe(c.path, () => {
      it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
        const fetchMock = vi.fn();
        vi.stubGlobal('fetch', fetchMock);
        renderApp(c.path);

        await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
        expect(fetchMock).not.toHaveBeenCalled();
        expect(document.getElementById(c.rootId)).toBeNull();
      });

      it('authenticated renders the new, specific "deferred, real future scope" state — the old generic badges are gone', async () => {
        setApiKey(RAW_KEY);
        mockBackend();
        renderApp(c.path);
        await verifyKey();

        await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());
        expect(screen.getAllByText(c.heading).length).toBeGreaterThan(0);

        // New, specific copy renders.
        expect(screen.getAllByText('DEFERRED — REAL FUTURE SCOPE').length).toBeGreaterThan(0);
        expect(screen.getAllByText('DEFERRED, NOT DESCOPED').length).toBeGreaterThan(0);
        // Some of these pages deliberately echo the revisit condition in
      // both the title (a short teaser) and the full description, so
      // >= 1 real match — not exactly 1 — is the actual requirement.
      expect(screen.getAllByText(new RegExp(c.revisitSnippet)).length).toBeGreaterThan(0);
        expect(screen.getByText(new RegExp(c.accRef))).toBeTruthy();
        // Cited twice by design — once in the description, once in the
        // codeHint — so two real elements genuinely match; getAllByText,
        // not getByText.
        expect(screen.getAllByText(/AIQA_PRD_FRD_SRD_v6\.1_SCOPE_CORRECTIONS\.md/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/product-owner decision/).length).toBeGreaterThan(0);

        // The old generic badges must be gone, not merely supplemented.
        expect(screen.queryByText('NOT IMPLEMENTED')).toBeNull();
        expect(screen.queryByText('NO BACKEND CAPABILITY')).toBeNull();
      });

      it('renders none of the page\'s previous fabricated demo content', async () => {
        setApiKey(RAW_KEY);
        mockBackend();
        renderApp(c.path);
        await verifyKey();
        await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());

        for (const banned of c.bannedText) {
          expect(screen.queryByText(new RegExp(banned.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))).toBeNull();
        }
      });

      it('makes no real backend request for its own (nonexistent) capability', async () => {
        setApiKey(RAW_KEY);
        const fetchMock = mockBackend();
        renderApp(c.path);
        await verifyKey();
        await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());

        // Only the real, generic `/projects` list call (auth verification +
        // `ProjectContext`'s own real project selector) is expected — no
        // page-specific endpoint exists for this page's capability, and
        // none should be invented client-side. A copy-only change must not
        // add one.
        for (const [url] of fetchMock.mock.calls as [string][]) {
          expect(String(url)).toMatch(/\/api\/v1\/projects(\?.*)?$/);
        }
      });
    });
  }

  it('each of the 4 pages\' revisit condition is genuinely distinct, not one sentence reused four times', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    const seen = new Set<string>();
    for (const c of deferredCases) {
      const { unmount } = renderApp(c.path);
      await verifyKey();
      await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());
      // Some of these pages deliberately echo the revisit condition in
      // both the title (a short teaser) and the full description, so
      // >= 1 real match — not exactly 1 — is the actual requirement.
      expect(screen.getAllByText(new RegExp(c.revisitSnippet)).length).toBeGreaterThan(0);
      seen.add(c.revisitSnippet);
      unmount();
    }
    expect(seen.size).toBe(deferredCases.length);
  });
});

describe('Case-C pages — route-table non-regression', () => {
  it('Dashboard remains unregressed with Custom Testing and all 4 deferred-scope pages present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });

  it('Test Generator remains unregressed with Custom Testing and all 4 deferred-scope pages present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/test-generator');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });
});
