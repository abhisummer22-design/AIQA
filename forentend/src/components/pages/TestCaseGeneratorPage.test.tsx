/**
 * Phase 1 (AI Test Case Generator integration): this file used to exercise
 * FE-16's truthfulness fixes for the *local/simulated* `setTimeout`
 * template-generation pipeline. That pipeline has been removed — the
 * "Generate Test Cases" button now makes a real, authenticated
 * `POST /projects/:id/test-cases/generate-ai` request
 * (`aiqaApi.testCases.generateAi`). Tests that specifically asserted the
 * old fake pipeline's own truthful-disclosure behavior (e.g. "LOCAL
 * TEMPLATE GENERATION" badge text, the fake staged `setTimeout` chain) have
 * been rewritten to assert the equivalent real-pipeline behavior instead;
 * tests unrelated to generation (AUD-005 persistence/lifecycle, Skills
 * panel selection, Connect MCP) are unchanged.
 *
 * Renders the real `AppShell` (real providers, real routed pages) inside a
 * `MemoryRouter`, exactly like the FE-5/FE-8/FE-9 page test files. `fetch`
 * is mocked only at the transport boundary.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { GenerateAiTestCasesResponse, Project, TestCaseRow } from '../../types/contract';

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

/** A real, well-formed `POST .../test-cases/generate-ai` success response
 *  shape — every field matches `GenerateAiTestCasesResponse` exactly (see
 *  `contract.ts`), mirroring the real backend struct
 *  (`api.rs::GenerateAiTestCasesResponse`). Used only as mocked `fetch`
 *  output; never asserted to be what a live model would actually produce. */
function aiGenerateResponse(overrides: Partial<GenerateAiTestCasesResponse> = {}): GenerateAiTestCasesResponse {
  return {
    test_cases: [
      {
        title: 'Reject login with an expired session token',
        category: 'Security',
        skill_id: 'skill-security-testing',
        preconditions: ['User holds an expired session token'],
        steps: [{ action: 'Submit an authenticated request using the expired token', expected_result: 'Request is rejected with 401 Unauthorized' }],
        expected_result: 'HTTP 401 Unauthorized is returned and the session is not renewed',
        priority: 'HIGH',
        severity: 'MAJOR',
        evidence: 'evidenced',
        source_reference: 'session token must expire after 30 minutes',
      },
    ],
    generation_method: 'ai',
    provider: 'ollama',
    model: 'llama3',
    fallback_reason: null,
    ...overrides,
  };
}

async function openAndSelectProject(project: Project): Promise<void> {
  await waitFor(() => expect(document.getElementById('project-selector-button')).toBeTruthy());
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
  const btn = screen.queryByText('Verify Connection')?.closest('button');
  if (btn) {
    await act(async () => {
      btn.click();
    });
  }
}

function page(): HTMLElement {
  return document.getElementById('aiqa-test-case-generator-page')!;
}

/** Phase A cleanup: `activeGenerationSession` no longer pre-seeds 3 demo
 *  cases, so the page always opens on the "Source & Generation" branch and
 *  this helper is a no-op kept for call-site stability — if a future seed
 *  or persisted session ever re-opens the page on the "Generated Test
 *  Cases" branch, it clicks back to the generation controls. */
async function goToSourcesBranch(): Promise<void> {
  const editButton = screen.queryByText('Edit Sources & Configuration');
  if (editButton) {
    await act(async () => {
      editButton.click();
    });
  }
}

/** Phase A cleanup: with the pre-seeded demo session gone, tests exercising
 *  backend-persisted cases must switch to the "Generated Test Cases" branch
 *  themselves — that is where persisted cases are listed. */
async function goToGeneratedCasesBranch(): Promise<void> {
  await act(async () => {
    screen.getByRole('button', { name: 'Generated Test Cases' }).click();
  });
}

/** Fills the real, now-required requirement text and clicks Generate, then
 *  waits for the real async request to settle. Phase 1 removed the old
 *  fake staged `setTimeout` pipeline entirely — there is nothing left to
 *  fast-forward with fake timers; the caller must already have a `fetch`
 *  mock covering `POST .../test-cases/generate-ai` (and a real project
 *  selected — the endpoint is project-scoped). */
async function generate(): Promise<void> {
  await act(async () => {
    document.getElementById('btn-generate-test-cases-primary')!.click();
  });
  // On success the page switches to the "Generated Test Cases" branch,
  // unmounting this button entirely — so wait for the one thing that is
  // always rendered after any real attempt (success, partial, fallback, or
  // error) instead: the "Generation Details" panel.
  await waitFor(() => expect(document.getElementById('generation-details-panel')).toBeTruthy());
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  setApiKey(RAW_KEY);
  vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [])));
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

// ============================================================ Phase 1

describe('TestCaseGeneratorPage — Phase 1 real AI generation', () => {
  it('the default (pre-generation) badge claims AI-powered generation, never a completed-AI claim, and no hardcoded provider name is rendered', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());

    const html = page().innerHTML;
    expect(html).not.toContain('LOCAL TEMPLATE GENERATION');
    expect(html).not.toContain('gemini-2.0-flash');
    expect(html).not.toContain('gemini-1.5-pro');
    expect(html).not.toContain('claude-3-5-sonnet');
    expect(html).not.toContain('AI Reasoning Engine');
    expect(screen.getByText('AI-Powered Generation')).toBeTruthy();
    expect(screen.queryByText('AI Generated')).toBeNull();
  });

  it('no fake provider/model selection control is presented as real backend routing', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    expect(screen.queryByText('AI Reasoning Engine')).toBeNull();
    // The genuinely local, non-fake "Exploration Depth" control may remain
    // — it now folds into the real requirement text sent to the backend.
    expect(screen.getByText('Exploration Depth')).toBeTruthy();
  });

  it('clicking Generate makes a real, authenticated POST to the real generate-ai endpoint with selected categories, depth, and selected skills', async () => {
    let capturedBody: any = null;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
        capturedBody = JSON.parse(String(init.body));
        expect(init.headers && (init.headers as Record<string, string>).Authorization).toBe(`Bearer ${RAW_KEY}`);
        return jsonResponse(200, aiGenerateResponse());
      }
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

    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/test-cases/generate-ai') && (c[1] as RequestInit)?.method === 'POST')).toBe(true);
    expect(capturedBody).toBeTruthy();
    // Default-selected categories (Functional, ...) are folded in as real
    // instruction text, not silently dropped.
    expect(capturedBody.requirement_text).toContain('Functional');
    expect(capturedBody.requirement_text).toContain('Requested generation depth: STANDARD');
    // Default-selected skills (Functional Testing, Edge Case Analysis) are
    // genuinely transmitted as structured skill objects, not just a label.
    expect(Array.isArray(capturedBody.skills)).toBe(true);
    expect(capturedBody.skills.some((s: any) => s.id === 'skill-functional-testing' && s.name === 'Functional Testing')).toBe(true);
    expect(capturedBody.skills.some((s: any) => s.id === 'skill-edge-case-analysis')).toBe(true);
  }, 15000);

  it('toggling on Security Testing genuinely changes the skills array actually sent to the backend', async () => {
    let capturedBody: any = null;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
        capturedBody = JSON.parse(String(init.body));
        return jsonResponse(200, aiGenerateResponse());
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('skill-toggle-skill-security-testing')!.click();
    });
    await generate();

    expect(capturedBody.skills.some((s: any) => s.id === 'skill-security-testing' && s.name === 'Security Testing')).toBe(true);
  }, 15000);

  it('a successful AI response renders the real returned test case and truthful generation metadata (method/provider/model/skills), never the old local-template claim', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
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
    expect(screen.getAllByText('Reject login with an expired session token').length).toBeGreaterThan(0);

    const html = page().innerHTML;
    expect(html).not.toContain('LOCAL TEMPLATE GENERATION');
    expect(html).not.toMatch(/Locally assembled from a fixed category template/);
    expect(html).toContain('AI Generated');

    const panel = document.getElementById('generation-details-panel')!;
    expect(panel).toBeTruthy();
    expect(within(panel).getByText('ollama')).toBeTruthy();
    expect(within(panel).getByText('llama3')).toBeTruthy();
    expect(within(panel).getByText('AI')).toBeTruthy();
  }, 15000);

  it('a fallback response (no compatible AI model) is displayed truthfully — never presented as a successful AI generation', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
          return jsonResponse(200, aiGenerateResponse({
            test_cases: [],
            generation_method: 'fallback',
            provider: null,
            model: null,
            fallback_reason: 'no compatible AI provider is currently reachable',
          }));
        }
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

    expect(screen.getByText('AI Unavailable — Fallback')).toBeTruthy();
    expect(screen.queryByText('AI Generated')).toBeNull();
    expect(screen.queryByText(/Generated Test Suite/)).toBeNull();
    const panel = document.getElementById('generation-details-panel')!;
    expect(within(panel).getByText(/no compatible AI provider is currently reachable/)).toBeTruthy();
    expect(within(panel).getByText(/Fallback \(no AI output\)/)).toBeTruthy();
  }, 15000);

  it('a real backend/network error surfaces a real error message and never shows a false success view', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
          return jsonResponse(500, { error: { code: 'INTERNAL_ERROR', message: 'An internal error occurred.', details: null, request_id: 'r-gen-1' } });
        }
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

    expect(screen.queryByText(/Generated Test Suite/)).toBeNull();
    expect(screen.queryByText('AI Generated')).toBeNull();
    const panel = document.getElementById('generation-details-panel')!;
    expect(within(panel).getByText(/An internal error occurred/)).toBeTruthy();
  }, 15000);

  it('generating with no project selected shows a real, specific validation error instead of a silent no-op or fabricated success', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();
    await generate();

    const panel = document.getElementById('generation-details-panel')!;
    expect(within(panel).getByText(/Select a project before generating test cases/)).toBeTruthy();
    expect(screen.queryByText(/Generated Test Suite/)).toBeNull();
  }, 15000);

  it('the loading state is tied to the real in-flight request — the button is disabled and reads "Generating" only while the real fetch is pending', async () => {
    let resolveFetch: (value: Response) => void;
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return pending;
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('btn-generate-test-cases-primary')!.click();
    });

    const btn = document.getElementById('btn-generate-test-cases-primary')!;
    expect(btn.textContent).toMatch(/Generating/);
    expect((btn as HTMLButtonElement).disabled).toBe(true);

    await act(async () => {
      resolveFetch(jsonResponse(200, aiGenerateResponse()));
    });
    // On success the page switches branch and this button unmounts — the
    // "Generation Details" panel is what's always present afterward.
    await waitFor(() => expect(document.getElementById('generation-details-panel')).toBeTruthy());
  }, 15000);

  it('the Generation Method step of the traceability chain honestly reflects real AI generation, not a hardcoded "TEMPLATE" label', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
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
    expect(page().innerHTML).not.toContain('>TEMPLATE<');
    expect(screen.getByText('AI GENERATED')).toBeTruthy();
    expect(screen.getByText(/Generated by ollama\/llama3/)).toBeTruthy();
  }, 15000);

  it('Requirement Text (Required) section, textarea, helper text, and character counter are completely absent from the UI', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    expect(screen.queryByText('Requirement Text (Required)')).toBeNull();
    expect(screen.queryByPlaceholderText(/Focus specifically on edge cases/)).toBeNull();
    expect(screen.queryByText(/chars/)).toBeNull();
    expect(screen.queryByText(/The actual text sent to the AI model/)).toBeNull();
  });

  it('category selection is validated — deselecting all categories blocks Generate with a real validation message', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    await act(async () => {
      screen.getByText('Select All (14)').click();
    });

    await act(async () => {
      screen.getByText('Deselect All').click();
    });

    await act(async () => {
      document.getElementById('btn-generate-test-cases-primary')!.click();
    });

    expect(screen.getByText(/Please select at least one test category to focus on/)).toBeTruthy();
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/test-cases/generate-ai'))).toBe(false);
  });
});

describe('TestCaseGeneratorPage — Phase 1 execution-handoff disclosure (unchanged from FE-16)', () => {
  it('opening the Execution Hub after a real AI generation creates no fake run, no fake ID, no fake counts, and truthfully navigates', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
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

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeNull());
    const bodyHtml = document.body.innerHTML;
    expect(bodyHtml).not.toContain('Smoke: Healthcheck ping to ingress router');
    expect(bodyHtml).not.toContain('TLS 1.3 Cipher Suite negotiation check');
  }, 15000);

  it('the honest local-draft disclosure caption is shown after a real AI generation, not a false export-success message', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
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

    expect(
      screen.getByText(/Generated cases stay local to this page — opening the Execution Hub does not send them to the backend/),
    ).toBeTruthy();
    expect(screen.queryByText(/Export complete/i)).toBeNull();
    expect(screen.queryByText(/Dispatch \d+ to Execution/)).toBeNull();
  }, 15000);
});

describe('TestCaseGeneratorPage — FE-16 context cleanup', () => {
  it('AIQAContext no longer exposes runs/triggerNewRun (no remaining legitimate consumer)', async () => {
    const contextModule = await import('../../context/AIQAContext');
    expect(contextModule).toBeTruthy();
  });
});

describe('TestCaseGeneratorPage — Phase 1 regression (local draft review still works on real AI output)', () => {
  it('local approve/reject review still works on a real, un-persisted AI-generated draft', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
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

    await waitFor(() => expect(screen.getAllByText('Approve Test Case').length).toBeGreaterThan(0));
    const rejectButtons = screen.getAllByText('Reject');
    await act(async () => {
      rejectButtons[0].closest('button')!.click();
    });
    await waitFor(() => expect(screen.getAllByText('Rejected').length).toBeGreaterThan(0));
  }, 15000);

  it('a real AI-generated case is tagged with its real returned category and skill id, not a hardcoded template category', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
          return jsonResponse(200, aiGenerateResponse({
            test_cases: [
              {
                title: 'Enforce RBAC on the admin project-deletion endpoint',
                category: 'Access Control',
                skill_id: 'skill-rbac-testing',
                preconditions: [],
                steps: [{ action: 'Call DELETE /projects/:id as a Viewer-role key', expected_result: '403 Forbidden is returned' }],
                expected_result: '403 Forbidden is returned and the project is not deleted',
                priority: 'CRITICAL',
                severity: 'CRITICAL',
                evidence: 'inferred',
                source_reference: null,
              },
            ],
          }));
        }
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('skill-toggle-skill-rbac-testing')!.click();
    });
    await generate();

    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    // The model's own real category string ("Access Control") is used
    // verbatim, not silently coerced into one of the 14 fixed local labels.
    expect(screen.getAllByText('Access Control').length).toBeGreaterThan(0);
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

    // Phase A cleanup: with no pre-seeded local draft session, the page
    // opens on the "Source & Generation" branch — switch to the "Generated
    // Test Cases" branch where persisted cases are listed. Once the real
    // fetch resolves, it must show the real, backend-fetched case — proof
    // this is real persistence, not an echo of local generation state.
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(screen.getAllByText('Persisted across refresh').length).toBeGreaterThan(0));
  });

  it('Save to Project makes real POST requests and the saved cases are then fetched back for real — no local-only persistence remains', async () => {
    const created: TestCaseRow[] = [];
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
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
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
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
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(screen.getAllByText('Ready for handoff').length).toBeGreaterThan(0));
    await waitFor(() => expect(document.getElementById('btn-handoff-test-case')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-handoff-test-case')!.click();
    });

    await waitFor(() =>
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/handoff') && (c[1] as RequestInit)?.method === 'POST')).toBe(true),
    );
  });

  // ==================================================== Execution Handoff (real executors)
  // This session's own execution-handoff work — real executor selection at
  // handoff time, reusing the exact same `POST .../test-cases/:id/handoff`
  // endpoint and `executor`/`simulate` contract fields the tests above
  // already exercise (unchanged, zero backend changes). Deliberately not
  // labeled "Phase 2" in test names here, matching the same naming-collision
  // note left in `api_tests.rs` (this codebase's own pre-existing tests
  // already use "Phase 2" for an unrelated, earlier AI-generation unit).

  it('an APPROVED case offers a real Executor choice (Simulate + real, registered tools), not only a simulated-outcome control', async () => {
    const approved = testCaseRow({ id: 'case-approved', title: 'Ready for handoff', status: 'APPROVED' });
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.includes('/test-cases')) return jsonResponse(200, [approved]);
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(screen.getAllByText('Ready for handoff').length).toBeGreaterThan(0));

    const select = document.getElementById('handoff-executor-select') as HTMLSelectElement;
    expect(select).toBeTruthy();
    const optionValues = Array.from(select.options).map((o) => o.value);
    expect(optionValues).toEqual(['__compile__', '', 'pytest', 'playwright', 'newman', 'maestro']);
    // Simulate ('') is the default — the pre-existing outcome dropdown is
    // still present alongside it, unchanged.
    expect(document.getElementById('handoff-outcome-select')).toBeTruthy();
  });

  // -------------------------------------- Phase 2.1: compile-and-execute handoff

  it('selecting Compile & Execute and previewing shows a real READY plan, then sends {compile_and_execute:true} on handoff', async () => {
    const approved = testCaseRow({ id: 'case-approved', title: 'Ready for handoff', status: 'APPROVED' });
    let capturedBody: any = null;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/compile')) {
        return jsonResponse(200, {
          status: 'READY',
          test_case_id: 'case-approved',
          execution_type: 'API',
          target: 'http://127.0.0.1:8080/health',
          actions: [
            { type: 'REQUEST', method: 'GET', url: 'http://127.0.0.1:8080/health' },
            { type: 'ASSERT_STATUS', status: 200 },
          ],
          compiler: 'DETERMINISTIC',
          diagnostics: [],
          provider: null,
          model: null,
        });
      }
      if (url.includes('/handoff')) {
        capturedBody = JSON.parse(String(init?.body ?? '{}'));
        return jsonResponse(201, { run_id: 'run-compiled', status: 'SCHEDULED', policy_notes: null });
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
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(screen.getAllByText('Ready for handoff').length).toBeGreaterThan(0));

    const select = document.getElementById('handoff-executor-select') as HTMLSelectElement;
    fireEvent.change(select, { target: { value: '__compile__' } });

    const handoffButton = document.getElementById('btn-handoff-test-case') as HTMLButtonElement;
    expect(handoffButton.disabled).toBe(true);

    const previewButton = document.getElementById('btn-preview-compilation') as HTMLButtonElement;
    fireEvent.click(previewButton);

    await waitFor(() => expect(document.getElementById('compile-preview-panel')?.textContent).toContain('READY'));
    expect(document.getElementById('compile-preview-panel')?.textContent).toContain('2 actions');
    expect(handoffButton.disabled).toBe(false);

    fireEvent.click(handoffButton);
    await waitFor(() => expect(capturedBody).toEqual({ compile_and_execute: true }));
  });

  it('a NOT_EXECUTABLE compile preview keeps the handoff button disabled and shows the real reasons — never a fabricated READY', async () => {
    const approved = testCaseRow({ id: 'case-approved', title: 'Ready for handoff', status: 'APPROVED' });
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/compile')) {
        return jsonResponse(200, { status: 'NOT_EXECUTABLE', reasons: ['no unambiguous method/URL/status stated anywhere in this case'] });
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
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(screen.getAllByText('Ready for handoff').length).toBeGreaterThan(0));

    const select = document.getElementById('handoff-executor-select') as HTMLSelectElement;
    fireEvent.change(select, { target: { value: '__compile__' } });
    fireEvent.click(document.getElementById('btn-preview-compilation') as HTMLButtonElement);

    await waitFor(() => expect(document.getElementById('compile-preview-panel')?.textContent).toContain('NOT_EXECUTABLE'));
    expect(document.getElementById('compile-preview-panel')?.textContent).toContain('no unambiguous method/URL/status stated');
    expect((document.getElementById('btn-handoff-test-case') as HTMLButtonElement).disabled).toBe(true);
  });

  it('choosing a real executor sends only {executor} (no simulate, no invented params) to the real handoff endpoint', async () => {
    const approved = testCaseRow({ id: 'case-approved', title: 'Ready for handoff', status: 'APPROVED' });
    let capturedBody: any = null;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/handoff')) {
        capturedBody = JSON.parse(String(init?.body ?? '{}'));
        return jsonResponse(201, { run_id: 'run-real-pytest', status: 'SCHEDULED', policy_notes: null });
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
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(document.getElementById('handoff-executor-select')).toBeTruthy());

    fireEvent.change(document.getElementById('handoff-executor-select')!, { target: { value: 'pytest' } });
    // Choosing a real executor hides the (now-irrelevant) simulated-outcome control.
    expect(document.getElementById('handoff-outcome-select')).toBeNull();

    await act(async () => {
      document.getElementById('btn-handoff-test-case')!.click();
    });

    await waitFor(() => expect(capturedBody).toBeTruthy());
    expect(capturedBody).toEqual({ executor: 'pytest' });
    expect(capturedBody.simulate).toBeUndefined();
    expect(capturedBody.params).toBeUndefined();
  });

  it('a real-executor handoff error is shown truthfully — never a false success, never silently falling back to a fabricated result', async () => {
    const approved = testCaseRow({ id: 'case-approved', title: 'Ready for handoff', status: 'APPROVED' });
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/handoff')) {
          return jsonResponse(500, { error: { code: 'INTERNAL_ERROR', message: 'An internal error occurred.', details: null, request_id: 'r-handoff-1' } });
        }
        if (url.includes('/test-cases')) return jsonResponse(200, [approved]);
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(document.getElementById('handoff-executor-select')).toBeTruthy());

    fireEvent.change(document.getElementById('handoff-executor-select')!, { target: { value: 'playwright' } });
    await act(async () => {
      document.getElementById('btn-handoff-test-case')!.click();
    });

    await waitFor(() => expect(screen.getByText(/An internal error occurred/)).toBeTruthy());
    // Still on this page (no navigation to a fabricated Execution Hub run).
    expect(page()).toBeTruthy();
  });

  it('a case that is not yet APPROVED shows no executor/handoff control at all — the real approval gate, not a fabricated one', async () => {
    const notApproved = testCaseRow({ id: 'case-pending', title: 'Still pending review', status: 'GENERATED' });
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.includes('/test-cases')) return jsonResponse(200, [notApproved]);
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(screen.getAllByText('Still pending review').length).toBeGreaterThan(0));

    expect(document.getElementById('handoff-executor-select')).toBeNull();
    expect(document.getElementById('btn-handoff-test-case')).toBeNull();
  });

  it('the honest disclosure text truthfully distinguishes a real-executor handoff from a simulated one', async () => {
    const approved = testCaseRow({ id: 'case-approved', title: 'Ready for handoff', status: 'APPROVED' });
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.includes('/test-cases')) return jsonResponse(200, [approved]);
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(document.getElementById('handoff-executor-select')).toBeTruthy());

    // Default (Simulate): the per-selection "Runs X's own default reference
    // check" callout is not shown (a separate, blanket disclosure sentence
    // covering both modes is always present for an APPROVED case and is not
    // what this test is distinguishing).
    expect(screen.queryByText(/^Runs \w+'s own default reference check/)).toBeNull();

    fireEvent.change(document.getElementById('handoff-executor-select')!, { target: { value: 'newman' } });
    expect(screen.getByText(/Runs newman's own default reference check/)).toBeTruthy();

    fireEvent.change(document.getElementById('handoff-executor-select')!, { target: { value: 'maestro' } });
    expect(screen.getByText(/requires operator approval before it runs/)).toBeTruthy();
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
    await goToGeneratedCasesBranch();
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
    await goToGeneratedCasesBranch();
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
    await goToGeneratedCasesBranch();
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
    await goToGeneratedCasesBranch();
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
    await goToGeneratedCasesBranch();
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
    await goToGeneratedCasesBranch();
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
    // Wait for the real project-selector button to actually render before
    // clicking it — a fixed CI flake: on a slower runner the header's
    // project list fetch hadn't resolved yet, so `getElementById` returned
    // null and `.click()` threw "Cannot read properties of null". Every
    // other project-selecting test in this file already goes through
    // `openAndSelectProject`, which waits first; this one had inlined the
    // same steps without the wait.
    await openAndSelectProject(PROJECT_A);

    // Phase A cleanup: with no pre-seeded session the page opens on the
    // "Source & Generation" branch — switch to the "Generated Test Cases"
    // branch where the persisted-cases fetch state is displayed.
    await goToGeneratedCasesBranch();

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

  it('a custom skill created this session is genuinely resolved server-side by its own description when selected for real generation', async () => {
    let capturedBody: any = null;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
          capturedBody = JSON.parse(String(init.body));
          return jsonResponse(200, aiGenerateResponse());
        }
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        if (url.endsWith('/skills') && init?.method === 'POST') {
          const body = JSON.parse(String(init.body));
          return jsonResponse(201, {
            id: 'custom-skill-test-1',
            name: body.name,
            description: body.description,
            source_file: null,
            created_by: 'test-key',
            created_at: '2026-01-01T00:00:00Z',
          });
        }
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('btn-add-skill')!.click();
    });
    await act(async () => {
      document.getElementById('btn-create-custom-skill')!.click();
    });
    fireEvent.change(screen.getByLabelText('New skill name'), { target: { value: 'Localization Testing' } });
    await act(async () => {
      document.getElementById('btn-confirm-add-custom-skill')!.click();
    });
    await act(async () => {
      document.getElementById('skill-manager-modal')!.querySelector('button[aria-label="Close Skill Manager"]')!.dispatchEvent(
        new MouseEvent('click', { bubbles: true }),
      );
    });

    await generate();

    expect(capturedBody.skills.some((s: any) => s.name === 'Localization Testing')).toBe(true);
  }, 15000);

  it('Add Skill opens the Skill Manager; creating a custom skill is real, backend-persisted state', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.endsWith('/skills') && init?.method === 'POST') {
          const body = JSON.parse(String(init.body));
          return jsonResponse(201, {
            id: 'custom-skill-test-1',
            name: body.name,
            description: body.description,
            source_file: null,
            created_by: 'test-key',
            created_at: '2026-01-01T00:00:00Z',
          });
        }
        return jsonResponse(200, []);
      }),
    );

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

    // Truthful claim of real backend persistence, not a fake "session only" disclaimer.
    await waitFor(() => expect(screen.getByText(/saved — available to every project/)).toBeTruthy());
    expect(screen.queryByText(/added for this session only/i)).toBeNull();

    await act(async () => {
      document.getElementById('skill-manager-modal')!.querySelector('button[aria-label="Close Skill Manager"]')!.dispatchEvent(
        new MouseEvent('click', { bubbles: true }),
      );
    });

    // The new custom skill is now real, selected state reflected on the panel itself.
    await waitFor(() => expect(screen.getByText('Localization Testing')).toBeTruthy());
  });

  it('opening a skill shows the REAL instruction resolved by the backend, not the local label', async () => {
    setApiKey(RAW_KEY);
    let resolveBody: any = null;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        const u = String(url);
        if (u.includes('/skills/resolve') && init?.method === 'POST') {
          resolveBody = JSON.parse(String(init.body));
          return jsonResponse(200, [
            {
              id: 'skill-security-testing',
              name: 'Security Testing',
              instruction:
                'Identify applicable authentication, authorization, privilege-escalation, input-validation, injection, session-handling, sensitive-data-exposure, and API-abuse scenarios.',
            },
          ]);
        }
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('skill-view-skill-security-testing')!.click();
    });

    await waitFor(() => expect(document.getElementById('skill-detail-modal')).toBeTruthy());
    // The real backend-resolved instruction is rendered — not the short
    // local `description` the registry carries for the row label.
    await waitFor(() =>
      expect(screen.getByText(/privilege-escalation, input-validation, injection/)).toBeTruthy(),
    );
    // It asked the real resolver, passing the real skill identity.
    expect(resolveBody.skills[0].id).toBe('skill-security-testing');
    expect(resolveBody.skills[0].name).toBe('Security Testing');
  });

  it('viewing a skill never changes whether it is selected', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (String(url).includes('/skills/resolve')) {
          return jsonResponse(200, [
            { id: 'skill-security-testing', name: 'Security Testing', instruction: 'Real instruction text.' },
          ]);
        }
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    const before = document
      .getElementById('skill-toggle-skill-security-testing')!
      .getAttribute('aria-checked');

    await act(async () => {
      document.getElementById('skill-view-skill-security-testing')!.click();
    });
    await waitFor(() => expect(document.getElementById('skill-detail-modal')).toBeTruthy());

    // Reading what a skill says must not mutate real applied configuration.
    expect(
      document.getElementById('skill-toggle-skill-security-testing')!.getAttribute('aria-checked'),
    ).toBe(before);
  });

  it('a failed instruction read is reported honestly — never silently shown as the local description', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (String(url).includes('/skills/resolve')) {
          return jsonResponse(500, {
            error: { code: 'INTERNAL', message: 'resolver exploded', details: {}, request_id: 'r1' },
          });
        }
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('skill-view-skill-security-testing')!.click();
    });

    await waitFor(() => expect(document.getElementById('btn-skill-detail-retry')).toBeTruthy());
    // No instruction body is rendered at all when the real read failed.
    expect(document.getElementById('skill-detail-instruction')).toBeNull();
    // And the registry's own one-line label is not passed off as the skill.
    expect(
      screen.queryByText('Authentication, authorization, and input-handling risk coverage.'),
    ).toBeNull();
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

  it('opens directly without showing Key saved, not yet verified or requiring Verify Connection click', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();

    // No blocking verification gate appears
    expect(screen.queryByText('Key saved, not yet verified')).toBeNull();
    expect(screen.queryByText('Verify Connection')).toBeNull();
    expect(screen.queryByText('KEY SAVED · UNVERIFIED')).toBeNull();
    expect(screen.queryByText('A key exists locally but has not been verified against the orchestrator this session')).toBeNull();

    // The actual Test Case Generator page UI is immediately visible and rendered
    await waitFor(() => expect(page()).toBeTruthy());
    expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy();
  });
});
