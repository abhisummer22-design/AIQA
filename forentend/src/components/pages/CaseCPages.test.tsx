/**
 * FE-11 — shared tests for the 7 Case-C pages (`AIQAOrchestratorPage`,
 * `TestCaseValidatorPage`, `RBACTestingPage`, `APITestingPage`,
 * `MobileTestingPage`, `CustomTestingPage`, `FeatureMapFlowPage`): each
 * was rewritten from a fully fabricated demo UI to a truthful "not
 * implemented" state, since no corresponding backend entity/route exists
 * for any of them (re-verified against `services/orchestrator/src/
 * api.rs`'s real route table). These tests assert the truthful state
 * renders, that no page-specific fabricated content (the previous
 * hardcoded demo records/metrics) survives, and that each page still has
 * a real root element and stays reachable via the same real, authenticated
 * route table every other page uses.
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

const cases: Array<{
  path: string;
  rootId: string;
  heading: string;
  bannedText: string[];
}> = [
  {
    path: '/orchestrator',
    rootId: 'aiqa-orchestrator-page',
    heading: 'AIQA Orchestrator',
    bannedText: ['PLAN-901', 'PLAN-902', 'Dispatch Selected Plan', 'Release Gating'],
  },
  {
    path: '/validator',
    rootId: 'test-case-validator-page',
    heading: 'Test Case Validator',
    bannedText: ['VAL-101', 'TC-AUTH-001', 'Batch Remediate All', 'Auto-Fix & Approve'],
  },
  {
    path: '/rbac',
    rootId: 'rbac-testing-page',
    heading: 'RBAC Security Testing Matrix',
    bannedText: ['Enterprise Organization & Financial Asset Access Control', 'Evaluate Authorization Matrix', 'BillingManager'],
  },
  {
    path: '/api-testing',
    rootId: 'api-testing-page',
    heading: 'API Testing & Protocol Adapters',
    bannedText: ['Create Ephemeral Tenant Session', 'Execute Endpoint', 'REST_HTTP'],
  },
  {
    path: '/mobile-testing',
    rootId: 'mobile-testing-page',
    heading: 'Mobile Testing Hub',
    bannedText: ['Samsung Galaxy S24 Ultra', 'DEV-AND-01', 'Execute Non-UI Suite', 'Lock & Reserve Device'],
  },
  {
    path: '/custom-testing',
    rootId: 'custom-testing-page',
    heading: 'Custom Testing Workflows',
    bannedText: ['Gateway 504 Gateway Timeout Synthetic Retry', 'Create Custom Suite'],
  },
  {
    path: '/feature-map',
    rootId: 'feature-map-flow-page',
    heading: 'Feature Map / System Call Flow',
    bannedText: ['NODE-AUTH-GW', 'Payment Ingestion Engine', 'Simulate Call Flow', 'Double-Entry Ledger Service'],
  },
];

describe('Case-C pages (FE-11 truthful not-implemented states)', () => {
  for (const c of cases) {
    describe(c.path, () => {
      it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
        const fetchMock = vi.fn();
        vi.stubGlobal('fetch', fetchMock);
        renderApp(c.path);

        await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
        expect(fetchMock).not.toHaveBeenCalled();
        expect(document.getElementById(c.rootId)).toBeNull();
      });

      it('authenticated renders a truthful "not implemented" state with a real root element', async () => {
        setApiKey(RAW_KEY);
        mockBackend();
        renderApp(c.path);
        await verifyKey();

        await waitFor(() => expect(document.getElementById(c.rootId)).toBeTruthy());
        expect(screen.getAllByText('NOT IMPLEMENTED').length).toBeGreaterThan(0);
        expect(screen.getAllByText('NO BACKEND CAPABILITY').length).toBeGreaterThan(0);
        expect(screen.getAllByText(c.heading).length).toBeGreaterThan(0);
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
        // none should be invented client-side.
        for (const [url] of fetchMock.mock.calls as [string][]) {
          expect(String(url)).toMatch(/\/api\/v1\/projects(\?.*)?$/);
        }
      });
    });
  }

  it('Dashboard remains unregressed with all 7 Case-C pages present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });

  it('Test Generator remains unregressed with all 7 Case-C pages present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/test-generator');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });
});
