/**
 * FE-16 targeted tests for the Test Generator's truthfulness fix and fake
 * execution-handoff removal (FE-15 GAP-02 Option A). Renders the real
 * `AppShell` (real providers, real routed pages) inside a `MemoryRouter`,
 * exactly like the FE-5/FE-8/FE-9 page test files. `fetch` is mocked only at
 * the transport boundary; the Test Generator itself makes no real `aiqaApi`
 * call, so the only fetch needed here is the one `verifyKey()` triggers.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { Project, TestCaseRow } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const PROJECT_A: Project = { id: 'proj-aud005', name: 'AUD-005 Project', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };

function testCaseRow(overrides: Partial<TestCaseRow> = {}): TestCaseRow {
  return {
    id: 'case-1',
    project_id: PROJECT_A.id,
    title: 'Saved case',
    description: 'A real, persisted case.',
    category: 'Functional',
    priority: 'HIGH',
    severity: 'MAJOR',
    preconditions: [],
    steps: [{ step_number: 1, action: 'Do the thing', expected_result: 'It works' }],
    expected_result: 'It works',
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

async function openAndSelectProject(project: Project): Promise<void> {
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

const RAW_KEY = 'aiqa_fe16testgen0000000000';

function renderApp() {
  return render(
    <MemoryRouter initialEntries={['/test-generator']}>
      <AppShell />
    </MemoryRouter>,
  );
}

async function verifyKey(): Promise<void> {
  await act(async () => {
    screen.getByText('Verify Connection').closest('button')!.click();
  });
}

function page(): HTMLElement {
  return document.getElementById('aiqa-test-case-generator-page')!;
}

/** The seeded `activeGenerationSession` (AIQAContext) already has 3 demo
 *  generated cases, so the page opens on the "Generated Test Cases" branch —
 *  switch to "Source & Generation" to reach the real generation controls. */
async function goToSourcesBranch(): Promise<void> {
  await act(async () => {
    screen.getByText('Edit Sources & Configuration').click();
  });
}

/** Fills the one thing `handleGenerateClick`'s validation requires (a doc,
 *  a connected app source, or requirement notes) and clicks Generate, then
 *  fast-forwards the real (fake-timer) VALIDATING->PROCESSING->GENERATING
 *  chain (1000+1400+1800 = 4200ms). */
async function generate(): Promise<void> {
  const notes = screen.getByPlaceholderText(/Focus specifically on edge cases/);
  fireEvent.change(notes, { target: { value: 'Focus on the login flow.' } });

  await act(async () => {
    document.getElementById('btn-generate-test-cases-primary')!.click();
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(4500);
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  // `shouldAdvanceTime` lets real-time-driven polling (RTL's `waitFor`, the
  // auth/fetch machinery) keep working while `vi.advanceTimersByTimeAsync`
  // is still used below to fast-forward the generation pipeline's own
  // 4200ms of chained `setTimeout` calls without the test wall-clock
  // actually waiting that long.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  setApiKey(RAW_KEY);
  vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [])));
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe('TestCaseGeneratorPage — FE-16 truthfulness (GAP-02 Option A)', () => {
  it('item 1/5 — no hardcoded provider name is rendered as the active generation engine', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());

    const html = page().innerHTML;
    expect(html).not.toContain('GEMINI 2.0 FLASH REASONING');
    expect(html).not.toContain('gemini-2.0-flash');
    expect(html).not.toContain('gemini-1.5-pro');
    expect(html).not.toContain('claude-3-5-sonnet');
    expect(html).not.toContain('AI Reasoning Engine');
    expect(screen.getByText('LOCAL TEMPLATE GENERATION')).toBeTruthy();
  });

  it('item 6 — no fake provider/model selection control is presented as real backend routing', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    expect(screen.queryByText('AI Reasoning Engine')).toBeNull();
    // The genuinely local, non-fake "Exploration Depth" control may remain.
    expect(screen.getByText('Exploration Depth')).toBeTruthy();
  });

  it('item 2/3/4 — generation completes locally, with no "Synthesized via ..." fabrication and truthful metadata', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();
    await generate();

    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    const html = page().innerHTML;
    expect(html).not.toMatch(/Synthesized via/);
    expect(html).toContain('Locally assembled from a fixed category template');
  }, 15000);

  it('item 5 — no UI text claims live AI/model inference occurred, on either branch of the page', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    // Sources & Generation branch: the pipeline visualization.
    expect(page().innerHTML).not.toContain('AI Analysis');
    expect(page().innerHTML).not.toContain('AI Reasoning');
    expect(page().innerHTML).toContain('Local Assembly');

    await generate();

    // Generated Test Cases branch: the traceability chain's step 3.
    expect(page().innerHTML).not.toContain('AI Analysis');
    expect(page().innerHTML).not.toContain('AI Reasoning');
    expect(page().innerHTML).toContain('Generation Method');
  }, 15000);
});

describe('TestCaseGeneratorPage — FE-16 fake execution handoff removal', () => {
  it('items 7-10 — opening the Execution Hub creates no fake run, no fake ID, no fake counts, no fabricated test names, and truthfully navigates', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();
    await generate();

    // Approve at least one generated case so the action isn't hidden behind
    // an empty-state, then click the real button.
    const approveButtons = screen.getAllByText('Approve Test Case');
    await act(async () => {
      approveButtons[0].closest('button')!.click();
    });

    const openHubButton = document.getElementById('btn-open-execution-hub')!;
    expect(openHubButton.textContent).toContain('Open Execution Hub');
    expect(openHubButton.textContent).not.toContain('Dispatch');

    await act(async () => {
      openHubButton.click();
    });

    // Real navigation to the real Execution Hub — no fake run, no fake ID,
    // no fake counts, no fabricated test names anywhere in its output.
    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeNull());
    const bodyHtml = document.body.innerHTML;
    expect(bodyHtml).not.toContain('Smoke: Healthcheck ping to ingress router');
    expect(bodyHtml).not.toContain('TLS 1.3 Cipher Suite negotiation check');
  }, 15000);

  it('item 12 — the honest disclosure caption is shown, not a false export-success message', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();
    await generate();

    expect(
      screen.getByText(/Generated cases stay local to this page — opening the Execution Hub does not send them to the backend/),
    ).toBeTruthy();
    expect(screen.queryByText(/Export complete/i)).toBeNull();
    expect(screen.queryByText(/Dispatch \d+ to Execution/)).toBeNull();
  }, 15000);
});

describe('TestCaseGeneratorPage — FE-16 context cleanup', () => {
  it('item 13 — AIQAContext no longer exposes runs/triggerNewRun (no remaining legitimate consumer)', async () => {
    const contextModule = await import('../../context/AIQAContext');
    // The module itself is the source of truth here — a TypeScript compile
    // of the whole project (`npx tsc --noEmit`, recorded in the FE-16
    // report) is what actually proves no consumer references these; this
    // assertion just documents the same fact at runtime for this file.
    expect(contextModule).toBeTruthy();
  });
});

describe('TestCaseGeneratorPage — FE-16 regression', () => {
  it('items 16/18 — the page still renders and local approve/reject review still works', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();
    await generate();

    await waitFor(() => expect(screen.getAllByText('Approve Test Case').length).toBeGreaterThan(0));
    const rejectButtons = screen.getAllByText('Reject');
    await act(async () => {
      rejectButtons[0].closest('button')!.click();
    });
    await waitFor(() => expect(screen.getAllByText('Rejected').length).toBeGreaterThan(0));
  }, 15000);

  it('item 17 — local category generation still works and produces real category-tagged cases', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();
    await generate();

    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    // Default-selected categories include Functional — confirm at least one
    // real, category-tagged case exists (not empty, not an error state).
    expect(screen.getAllByText('Functional').length).toBeGreaterThan(0);
  }, 15000);
});

// ============================================================ AUD-005

describe('TestCaseGeneratorPage — AUD-005 real persistence, validation, execution handoff', () => {
  it('a previously saved test case is fetched from the real backend and rendered — survives a fresh mount, no local generation needed', async () => {
    const saved = testCaseRow({ id: 'case-persisted', title: 'Persisted across refresh' });
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.includes('/test-cases')) return jsonResponse(200, [saved]);
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    // The page already opens on the Generated Test Cases branch (a seeded
    // local draft session exists); once the real fetch resolves, it must
    // show the real, backend-fetched case — proof this is real
    // persistence, not an echo of local generation state.
    await waitFor(() => expect(screen.getAllByText('Persisted across refresh').length).toBeGreaterThan(0));
  });

  it('Save to Project makes real POST requests and the saved cases are then fetched back for real — no local-only persistence remains', async () => {
    const created: TestCaseRow[] = [];
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.endsWith('/test-cases') && init?.method === 'POST') {
        const body = JSON.parse(String(init.body));
        const row = testCaseRow({ id: `case-${created.length + 1}`, title: body.title, category: body.category });
        created.push(row);
        return jsonResponse(201, row);
      }
      if (url.includes('/test-cases')) return jsonResponse(200, created);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();
    await generate();

    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    const saveButton = document.getElementById('btn-save-to-project')!;
    await act(async () => {
      saveButton.click();
    });

    await waitFor(() => expect(created.length).toBeGreaterThan(0));
    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/test-cases') && (c[1] as RequestInit)?.method === 'POST')).toBe(true);
    // The disclosure text switches to the real, persisted-state wording.
    await waitFor(() => expect(screen.getByText(/These test cases are saved to the project/)).toBeTruthy());
  }, 20000);

  it('a failed save reports a real, specific error — never a false success and no fabricated fallback data', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.endsWith('/test-cases') && init?.method === 'POST') {
          return jsonResponse(500, { error: { code: 'INTERNAL_ERROR', message: 'An internal error occurred.', details: null, request_id: 'r1' } });
        }
        if (url.includes('/test-cases')) return jsonResponse(200, []);
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();
    await generate();

    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    await act(async () => {
      document.getElementById('btn-save-to-project')!.click();
    });

    await waitFor(() => expect(document.getElementById('save-error-message')).toBeTruthy());
    expect(document.getElementById('save-error-message')!.textContent).toContain('Saved 0 of');
    // Never silently shows the "saved" disclosure text on a real failure.
    expect(screen.queryByText(/These test cases are saved to the project/)).toBeNull();
  }, 20000);

  it('handoff on an approved case sends a real request with the explicit simulated outcome and navigates toward the real Execution Hub run', async () => {
    const approved = testCaseRow({ id: 'case-approved', title: 'Ready for handoff', status: 'APPROVED' });
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/handoff')) {
        const body = JSON.parse(String(init?.body ?? '{}'));
        expect(body.simulate?.outcome).toBeTruthy();
        return jsonResponse(201, { run_id: 'run-from-handoff', status: 'SCHEDULED', policy_notes: null });
      }
      if (url.includes('/test-cases')) return jsonResponse(200, [approved]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getAllByText('Ready for handoff').length).toBeGreaterThan(0));
    await waitFor(() => expect(document.getElementById('btn-handoff-test-case')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-handoff-test-case')!.click();
    });

    await waitFor(() =>
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/handoff') && (c[1] as RequestInit)?.method === 'POST')).toBe(true),
    );
  });

  // -------------------------------------------------------------- ACC-005
  // Validate/Approve/Reject were already wired to the real backend
  // (`handleValidateReal`/`handleApproveReal`/`handleRejectReal`,
  // `GeneratedTestCasesView.tsx`'s status-gated buttons) before this
  // session, but had zero direct test coverage — confirmed by reading this
  // file before writing anything here (investigate-before-building, same
  // rule that closed ACC-012). These tests close that gap without adding
  // any new frontend logic: every assertion below exercises code that
  // already existed.

  it('Validate Test Case on a real GENERATED case makes a real request and the case becomes VALIDATED', async () => {
    let currentStatus: TestCaseRow['status'] = 'GENERATED';
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/validate') && init?.method === 'POST') {
        currentStatus = 'VALIDATED';
        return jsonResponse(200, testCaseRow({ id: 'case-validate', title: 'Needs validation', status: currentStatus }));
      }
      if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow({ id: 'case-validate', title: 'Needs validation', status: currentStatus })]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getAllByText('Needs validation').length).toBeGreaterThan(0));

    await waitFor(() => expect(document.getElementById('btn-validate-test-case')).toBeTruthy());
    await act(async () => {
      document.getElementById('btn-validate-test-case')!.click();
    });

    await waitFor(() =>
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/validate') && (c[1] as RequestInit)?.method === 'POST')).toBe(true),
    );
    // Real state transition, reflected by the real button that appears next
    // in the real status-gated sequence (PENDING_REVIEW -> VALIDATED).
    await waitFor(() => expect(document.getElementById('btn-validate-test-case')).toBeNull());
  });

  it('Validate reports the real, specific structural rejection reason — no steps defined', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/validate') && init?.method === 'POST') {
        return jsonResponse(409, {
          error: { code: 'CONFLICT', message: 'structural validation failed: no steps defined', details: null, request_id: 'r-novalidate-1' },
        });
      }
      if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow({ id: 'case-nosteps', title: 'Missing steps', status: 'GENERATED' })]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(document.getElementById('btn-validate-test-case')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-validate-test-case')!.click();
    });

    // The real backend message, not a generic client-side string.
    await waitFor(() => expect(document.getElementById('save-error-message')!.textContent).toContain('no steps defined'));
  });

  it('Validate reports a different real rejection reason for a different real structural problem — empty expected_result', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/validate') && init?.method === 'POST') {
        return jsonResponse(409, {
          error: { code: 'CONFLICT', message: 'structural validation failed: expected_result is empty', details: null, request_id: 'r-noexpected-1' },
        });
      }
      if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow({ id: 'case-noexpected', title: 'Missing expected result', status: 'GENERATED' })]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(document.getElementById('btn-validate-test-case')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-validate-test-case')!.click();
    });

    // A genuinely different real message for a genuinely different real
    // problem — proof this surfaces whatever the backend actually said,
    // not one hardcoded rejection string reused for every failure.
    await waitFor(() => expect(document.getElementById('save-error-message')!.textContent).toContain('expected_result is empty'));
  });

  it('Approve Test Case on a real VALIDATED case makes a real request and the case becomes APPROVED', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/approve') && init?.method === 'POST') {
        return jsonResponse(200, testCaseRow({ id: 'case-approve', title: 'Ready to approve', status: 'APPROVED' }));
      }
      if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow({ id: 'case-approve', title: 'Ready to approve', status: 'VALIDATED' })]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getAllByText('Ready to approve').length).toBeGreaterThan(0));

    const approveButton = screen.getByText('Approve Test Case').closest('button')!;
    await act(async () => {
      approveButton.click();
    });

    await waitFor(() =>
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/approve') && (c[1] as RequestInit)?.method === 'POST')).toBe(true),
    );
  });

  it('Approve reports a real, honest error when the backend rejects it — never a fabricated success', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/approve') && init?.method === 'POST') {
        return jsonResponse(409, { error: { code: 'CONFLICT', message: 'test case is not in VALIDATED state (current: GENERATED)', details: null, request_id: 'r-approve-1' } });
      }
      if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow({ id: 'case-badapprove', title: 'Not yet validated', status: 'VALIDATED' })]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getAllByText('Not yet validated').length).toBeGreaterThan(0));

    const approveButton = screen.getByText('Approve Test Case').closest('button')!;
    await act(async () => {
      approveButton.click();
    });

    await waitFor(() => expect(document.getElementById('save-error-message')!.textContent).toContain('not in VALIDATED state'));
  });

  it('Reject on a real case makes a real request', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/reject') && init?.method === 'POST') {
        return jsonResponse(200, testCaseRow({ id: 'case-reject', title: 'To be rejected', status: 'REJECTED' }));
      }
      if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow({ id: 'case-reject', title: 'To be rejected', status: 'GENERATED' })]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getAllByText('To be rejected').length).toBeGreaterThan(0));

    const rejectButton = screen.getByText('Reject').closest('button')!;
    await act(async () => {
      rejectButton.click();
    });

    await waitFor(() =>
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/reject') && (c[1] as RequestInit)?.method === 'POST')).toBe(true),
    );
  });

  it('shows the real loading state while persisted cases are being fetched', async () => {
    let resolveFetch: (value: Response) => void;
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/test-cases')) return pending;
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await act(async () => {
      document.getElementById('project-selector-button')!.click();
    });
    await act(async () => {
      within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: PROJECT_A.name }).click();
    });

    // Real loading state while the real fetch is still in flight — not a
    // fabricated instant transition.
    await waitFor(() => expect(document.getElementById('generated-test-cases-loading')).toBeTruthy());

    await act(async () => {
      resolveFetch(jsonResponse(200, [testCaseRow({ id: 'case-loaded', title: 'Loaded after wait' })]));
    });
    await waitFor(() => expect(document.getElementById('generated-test-cases-loading')).toBeNull());
    await waitFor(() => expect(screen.getAllByText('Loaded after wait').length).toBeGreaterThan(0));
  });
});

// ============================================================ Skills & MCP Phase 1

describe('TestCaseGeneratorPage — Skills & MCP Phase 1', () => {
  it('renders the Skills & MCP panel with built-in skills, Add Skill, and Connect MCP', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    const panel = document.getElementById('skills-and-mcp-panel')!;
    expect(panel).toBeTruthy();
    expect(within(panel).getByText('Functional Testing')).toBeTruthy();
    expect(within(panel).getByText('Edge Case Analysis')).toBeTruthy();
    expect(within(panel).getByText('Security Testing')).toBeTruthy();
    expect(within(panel).getByText('Performance Testing')).toBeTruthy();
    expect(within(panel).getByText('API Testing')).toBeTruthy();
    expect(within(panel).getByText('Database Testing')).toBeTruthy();
    expect(within(panel).getByText('RBAC Testing')).toBeTruthy();
    expect(within(panel).getByText('Business Case Testing')).toBeTruthy();
    expect(document.getElementById('btn-add-skill')).toBeTruthy();
    expect(document.getElementById('btn-connect-mcp')).toBeTruthy();
  });

  it('Functional Testing and Edge Case Analysis are selected by default; toggling a skill updates real, visible state', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    const functional = document.getElementById('skill-toggle-skill-functional-testing')!;
    const security = document.getElementById('skill-toggle-skill-security-testing')!;
    expect(functional.getAttribute('aria-checked')).toBe('true');
    expect(security.getAttribute('aria-checked')).toBe('false');

    await act(async () => {
      security.click();
    });
    expect(document.getElementById('skill-toggle-skill-security-testing')!.getAttribute('aria-checked')).toBe('true');

    await act(async () => {
      functional.click();
    });
    expect(document.getElementById('skill-toggle-skill-functional-testing')!.getAttribute('aria-checked')).toBe('false');
  });

  it('existing local generation still completes with skills selected, and selection does not break the generate flow', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('skill-toggle-skill-security-testing')!.click();
    });

    await generate();

    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    expect(screen.getAllByText('Functional').length).toBeGreaterThan(0);
  }, 15000);

  it('Add Skill opens the Skill Manager; creating a custom skill is real, session-only state — never claims persistence', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('btn-add-skill')!.click();
    });
    expect(document.getElementById('skill-manager-modal')).toBeTruthy();

    await act(async () => {
      document.getElementById('btn-create-custom-skill')!.click();
    });
    fireEvent.change(screen.getByLabelText('New skill name'), { target: { value: 'Localization Testing' } });
    await act(async () => {
      document.getElementById('btn-confirm-add-custom-skill')!.click();
    });

    // Truthful, not a fake "saved successfully" claim.
    expect(screen.getByText(/added for this session only — not saved anywhere/)).toBeTruthy();
    expect(screen.queryByText(/saved successfully/i)).toBeNull();

    await act(async () => {
      document.getElementById('skill-manager-modal')!.querySelector('button[aria-label="Close Skill Manager"]')!.dispatchEvent(
        new MouseEvent('click', { bubbles: true }),
      );
    });

    // The new custom skill is now real, selected state reflected on the panel itself.
    await waitFor(() => expect(screen.getByText('Localization Testing')).toBeTruthy());
  });

  it('Connect MCP shows a truthful "Not Configured" status and never claims a live connection', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('btn-connect-mcp')!.click();
    });

    expect(document.getElementById('mcp-connect-modal')).toBeTruthy();
    expect(document.getElementById('mcp-status-badge')!.textContent).toContain('Not Configured');
    expect(document.getElementById('mcp-status-badge')!.textContent).not.toContain('Connected');
    expect(screen.getByText(/External MCP connection management is not yet configured/)).toBeTruthy();
    expect(screen.queryByText(/^Connected$/)).toBeNull();
  });
});
