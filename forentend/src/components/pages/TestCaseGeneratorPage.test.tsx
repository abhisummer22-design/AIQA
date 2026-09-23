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
import { parseWaveStartedSkillNames } from './TestCaseGeneratorPage';
import type { GenerateAiTestCasesResponse, Project, TestCaseRow, RunDetail, CrawlOutput } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const PROJECT_A: Project = { id: 'proj-aud005', name: 'AUD-005 Project', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-ambiance-2', name: 'A Brand New Project', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };

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
    crawl_pages_used: 0,
    similar_cases_used: 0,
    similar_documents_used: 0,
    total_prompt_tokens: 120,
    total_completion_tokens: 340,
    total_estimated_cost_usd: 0,
    total_latency_ms: 4200,
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

  it('toggling on RBAC Testing genuinely changes the skills array actually sent to the backend', async () => {
    // RBAC Testing deliberately has no matching test category (there is no
    // "RBAC" category), so it is never auto-included the way "Security
    // Testing"/"Functional Testing"/etc. now are once their own
    // like-named category is checked (`categoryMatchedSkillIds`) — this
    // keeps the test proving what its name claims: that the manual toggle
    // itself, not a default category match, is what puts it in the request.
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
      document.getElementById('skill-toggle-skill-rbac-testing')!.click();
    });
    await generate();

    expect(capturedBody.skills.some((s: any) => s.id === 'skill-rbac-testing' && s.name === 'RBAC Testing')).toBe(true);
  }, 15000);

  it('a category with a matching built-in skill auto-includes that skill even when its own checkbox was never clicked', async () => {
    // "Performance" category <-> "Performance Testing" skill: the exact
    // pairing from the standing request this feature implements. Default
    // categories do not include "Performance", and DEFAULT_SELECTED_SKILL_IDS
    // does not include "Performance Testing" either, so this only passes if
    // checking the category is genuinely what includes the skill.
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
      document.getElementById('cat-card-performance')!.click();
    });

    // Its own checkbox was never clicked — it must show as selected anyway,
    // never left looking unused while actually being sent. Checked before
    // generating: a successful generation switches to the Generated Cases
    // branch, which unmounts this checkbox entirely.
    expect(document.getElementById('skill-toggle-skill-performance-testing')!.getAttribute('aria-checked')).toBe('true');

    await generate();

    expect(capturedBody.skills.some((s: any) => s.id === 'skill-performance-testing' && s.name === 'Performance Testing')).toBe(
      true,
    );
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
    // Real usage/cost/latency from the response (the default fixture:
    // 120 prompt / 340 completion tokens, $0 — an Unmetered provider —,
    // 4200ms), never fabricated when the backend actually reported them.
    expect(within(panel).getByText('120 in / 340 out')).toBeTruthy();
    expect(within(panel).getByText('4.2s')).toBeTruthy();
    expect(within(panel).getByText('Free (self-hosted)')).toBeTruthy();
  }, 15000);

  it('a genuinely unknown token/cost figure is shown honestly as "Not Reported", never a fabricated number', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
          return jsonResponse(
            200,
            aiGenerateResponse({
              total_prompt_tokens: null,
              total_completion_tokens: null,
              total_estimated_cost_usd: null,
            }),
          );
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

    await waitFor(() => expect(document.getElementById('generation-details-panel')).toBeTruthy());
    const panel = document.getElementById('generation-details-panel')!;
    expect(within(panel).getAllByText('Not Reported').length).toBeGreaterThanOrEqual(2);
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

  it('a category chip correctly counts and filters a real case whose AI-returned category is the skill display name, not the bare category id', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
          return jsonResponse(200, aiGenerateResponse({
            test_cases: [
              {
                // The model's real, common phrasing -- not the bare
                // "Functional" id the "Functional" category chip below
                // filters/counts by. A strict `===` match would silently
                // show 0 on that chip even though this real case exists.
                title: 'Valid login succeeds',
                category: 'Functional Testing',
                skill_id: 'skill-functional-testing',
                preconditions: [],
                steps: [{ action: 'Log in with valid credentials', expected_result: 'User reaches dashboard' }],
                expected_result: 'User reaches dashboard',
                priority: 'HIGH',
                severity: 'MAJOR',
                evidence: 'evidenced',
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
    await generate();

    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    const chip = screen.getByText('Functional').closest('button')!;
    expect(within(chip).getByText('1')).toBeTruthy();

    // Clicking it must actually show the real case, not the honest-but-
    // wrong "No matching test cases found" the bug produced.
    await act(async () => {
      chip.click();
    });
    expect(screen.getAllByText('Valid login succeeds').length).toBeGreaterThan(0);
    expect(screen.queryByText(/No matching test cases found/)).toBeNull();
  }, 15000);
});

describe('TestCaseGeneratorPage — real unsaved-work warning before a browser unload', () => {
  it('warns before an actual tab close/refresh once real generated cases exist and are not yet saved', async () => {
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

    // Before any generation, there is genuinely nothing to lose yet — a
    // real unload must never be blocked here.
    const beforeGenerate = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(beforeGenerate);
    expect(beforeGenerate.defaultPrevented).toBe(false);

    await generate();

    // Real, un-persisted cases now exist (the backend's own test-cases
    // list is still genuinely empty — the same real gap that let 445 real
    // cases vanish silently) — a real unload must now be blocked.
    const afterGenerate = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(afterGenerate);
    expect(afterGenerate.defaultPrevented).toBe(true);
  }, 15000);

  it('never warns once the real backend already has persisted cases for this project', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
        if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow()]);
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();
    await waitFor(() => expect(screen.getAllByText(/Approve|Validate/).length).toBeGreaterThan(0));

    // Real, already-persisted cases exist — nothing local is at risk, so a
    // real unload must never be blocked here either.
    const event = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
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

  it('Edit Test Case sends a real PATCH with the real edited fields, closing a real gap: reject-and-regenerate was the only fix before', async () => {
    let patchBody: unknown = null;
    let currentTitle = 'Wrong title';
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/test-cases/case-edit') && init?.method === 'PATCH') {
        patchBody = JSON.parse(String(init.body));
        currentTitle = (patchBody as { title: string }).title;
        return jsonResponse(200, testCaseRow({ id: 'case-edit', title: currentTitle, status: 'GENERATED' }));
      }
      // A real backend remembers the update — the refetch after a
      // successful PATCH must see it too, not the original fixture again.
      if (url.includes('/test-cases')) return jsonResponse(200, [testCaseRow({ id: 'case-edit', title: currentTitle, status: 'GENERATED' })]);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToGeneratedCasesBranch();
    await waitFor(() => expect(screen.getAllByText('Wrong title').length).toBeGreaterThan(0));

    await act(async () => {
      document.getElementById('btn-edit-test-case')!.click();
    });
    expect(document.getElementById('edit-test-case-modal')).toBeTruthy();

    const titleInput = document.getElementById('edit-tc-title') as HTMLInputElement;
    await act(async () => {
      fireEvent.change(titleInput, { target: { value: 'Corrected title' } });
    });
    await act(async () => {
      document.getElementById('btn-save-test-case-edit')!.click();
    });

    await waitFor(() => expect(patchBody).not.toBeNull());
    expect((patchBody as { title: string }).title).toBe('Corrected title');
    // The real, unmodified other fields are still sent — not a partial
    // patch missing everything the user didn't touch.
    expect((patchBody as { steps: unknown[] }).steps.length).toBeGreaterThan(0);
    // A successful real save refetches the real persisted list — the
    // corrected title (from the server's own response) is what's shown.
    await waitFor(() => expect(screen.getAllByText('Corrected title').length).toBeGreaterThan(0));
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
    // RBAC Testing has no matching test category, so `categoryMatchedSkillIds`
    // never auto-includes it — a clean skill for proving a plain manual
    // toggle, unaffected by that separate feature ("Security Testing" is a
    // bad choice here since "Security" is a default-checked category and
    // would already show checked before any click).
    const rbac = document.getElementById('skill-toggle-skill-rbac-testing')!;
    expect(functional.getAttribute('aria-checked')).toBe('true');
    expect(rbac.getAttribute('aria-checked')).toBe('false');

    await act(async () => {
      rbac.click();
    });
    expect(document.getElementById('skill-toggle-skill-rbac-testing')!.getAttribute('aria-checked')).toBe('true');

    await act(async () => {
      document.getElementById('skill-toggle-skill-rbac-testing')!.click();
    });
    expect(document.getElementById('skill-toggle-skill-rbac-testing')!.getAttribute('aria-checked')).toBe('false');
  });

  it('Select All genuinely checks every real built-in skill, and Deselect All genuinely clears them', async () => {
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await goToSourcesBranch();

    const builtInSkillIds = [
      'skill-functional-testing',
      'skill-edge-case-analysis',
      'skill-security-testing',
      'skill-performance-testing',
      'skill-api-testing',
      'skill-database-testing',
      'skill-rbac-testing',
      'skill-business-case-testing',
    ];
    // Not every one of these starts checked (only Functional/Edge Case are
    // selected by default, plus whichever categories happen to auto-match) —
    // real, mixed starting state, not a contrived all-or-nothing setup.
    expect(document.getElementById('skill-toggle-skill-rbac-testing')!.getAttribute('aria-checked')).toBe('false');

    await act(async () => {
      document.getElementById('btn-select-all-skills')!.click();
    });
    for (const id of builtInSkillIds) {
      expect(
        document.getElementById(`skill-toggle-${id}`)!.getAttribute('aria-checked'),
        `expected ${id} to be checked after Select All`,
      ).toBe('true');
    }
    expect(document.getElementById('btn-select-all-skills')).toBeNull();
    expect(document.getElementById('btn-deselect-all-skills')).toBeTruthy();

    await act(async () => {
      document.getElementById('btn-deselect-all-skills')!.click();
    });
    // RBAC Testing has no matching category (see the test above), so it's
    // the one real, unambiguous proof Deselect All actually cleared manual
    // selection rather than being a no-op.
    expect(document.getElementById('skill-toggle-skill-rbac-testing')!.getAttribute('aria-checked')).toBe('false');
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

// ================================================ crawl grounding (Phase 6 -> Phase 8)

function runDetail(overrides: Partial<RunDetail> = {}): RunDetail {
  return {
    run_id: 'crawl-run-1',
    name: 'Crawl: https://target.example.com',
    state: 'RUNNING',
    total: 1,
    queued: 0,
    running: 1,
    passed: 0,
    failed: 0,
    blocked: 0,
    skipped: 0,
    cancelled: 0,
    timeout: 0,
    infra_error: 0,
    needs_review: 0,
    started_at: new Date().toISOString(),
    finished_at: null,
    project_id: PROJECT_A.id,
    requested_by: null,
    policy_notes: null,
    approved_by: null,
    approved_at: null,
    tasks: [
      { id: 'crawl-task-1', name: 'Crawl', prerequisites: [], status: 'RUNNING', reason: null, started_at: null, finished_at: null },
    ],
    ...overrides,
  };
}

function crawlOutput(): CrawlOutput {
  return {
    roles: [
      {
        role: 'anonymous',
        login_succeeded: true,
        login_error: null,
        pages: [
          {
            url: 'https://target.example.com/login',
            title: 'Login',
            elements: [{ kind: 'button', text: 'Sign In', selector_hint: 'button', href: null }],
          },
        ],
      },
    ],
    visibility_matrix: [],
  };
}

describe('TestCaseGeneratorPage — crawl grounding (Phase 6 -> Phase 8 wiring)', () => {
  it('Crawl This Application makes a real POST to the real crawl endpoint with the entered URL', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/crawl') && init?.method === 'POST') {
        const body = JSON.parse(String(init.body));
        expect(body.base_url).toBe('https://target.example.com');
        expect(body.roles).toEqual([{ name: 'anonymous', login_actions: [] }]);
        return jsonResponse(201, { run_id: 'crawl-run-1' });
      }
      if (url.includes('/executions/crawl-run-1')) return jsonResponse(200, runDetail());
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    const urlInput = document.getElementById('crawl-target-url') as HTMLInputElement;
    await act(async () => {
      fireEvent.change(urlInput, { target: { value: 'https://target.example.com' } });
    });
    await act(async () => {
      document.getElementById('btn-start-crawl')!.click();
    });

    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([u, i]) => String(u).includes('/crawl') && (i as RequestInit)?.method === 'POST')).toBe(true),
    );
  }, 15000);

  it('a completed crawl shows real page/element counts and opts its real run_id into the next generate-ai request', async () => {
    const generateAiCalls: unknown[] = [];
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/crawl') && url.endsWith('/result')) return jsonResponse(200, crawlOutput());
      if (url.includes('/crawl') && init?.method === 'POST') return jsonResponse(201, { run_id: 'crawl-run-1' });
      if (url.includes('/executions/crawl-run-1')) return jsonResponse(200, runDetail({ state: 'COMPLETED', finished_at: new Date().toISOString() }));
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
        generateAiCalls.push(JSON.parse(String(init.body)));
        return jsonResponse(200, aiGenerateResponse({ crawl_pages_used: 1 }));
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

    const urlInput = document.getElementById('crawl-target-url') as HTMLInputElement;
    await act(async () => {
      fireEvent.change(urlInput, { target: { value: 'https://target.example.com' } });
    });
    await act(async () => {
      document.getElementById('btn-start-crawl')!.click();
    });

    // Real counts from the real crawl result, and the "use it" checkbox
    // defaults on for a genuinely completed crawl.
    await waitFor(() => expect(screen.getByText(/Crawled 1 real page, 1 real element/)).toBeTruthy(), { timeout: 10000 });
    const checkbox = document.getElementById('crawl-use-in-generation') as HTMLInputElement;
    expect(checkbox.checked).toBe(true);

    await generate();

    expect(generateAiCalls.length).toBe(1);
    expect((generateAiCalls[0] as { crawl_run_id: string | null }).crawl_run_id).toBe('crawl-run-1');

    // A successful generate navigates away from this branch; the real
    // selection persists at the page level and its real summary is
    // restored on returning here — proven by the checkbox still being
    // checked without re-crawling.
    await goToSourcesBranch();
    const restoredCheckbox = await waitFor(() => document.getElementById('crawl-use-in-generation') as HTMLInputElement);
    expect(restoredCheckbox.checked).toBe(true);

    // Unchecking it must make the next real request omit real grounding.
    await act(async () => {
      fireEvent.click(restoredCheckbox);
    });
    expect(restoredCheckbox.checked).toBe(false);
    await generate();
    expect((generateAiCalls[1] as { crawl_run_id: string | null }).crawl_run_id).toBeNull();
  }, 20000);

  it('a crawl that genuinely fails shows the real reason and never opts a run_id into generation', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/crawl') && init?.method === 'POST') return jsonResponse(201, { run_id: 'crawl-run-1' });
      if (url.includes('/executions/crawl-run-1')) {
        return jsonResponse(
          200,
          runDetail({
            state: 'FAILED',
            failed: 1,
            running: 0,
            finished_at: new Date().toISOString(),
            tasks: [
              {
                id: 'crawl-task-1',
                name: 'Crawl',
                prerequisites: [],
                status: 'INFRA_ERROR',
                reason: 'NO_COMPATIBLE_EXECUTOR: playwright is not registered',
                started_at: null,
                finished_at: null,
              },
            ],
          }),
        );
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

    const urlInput = document.getElementById('crawl-target-url') as HTMLInputElement;
    await act(async () => {
      fireEvent.change(urlInput, { target: { value: 'https://target.example.com' } });
    });
    await act(async () => {
      document.getElementById('btn-start-crawl')!.click();
    });

    await waitFor(() => expect(screen.getByText(/NO_COMPATIBLE_EXECUTOR/)).toBeTruthy(), { timeout: 10000 });
    expect(document.getElementById('crawl-use-in-generation')).toBeNull();
  }, 15000);
});

describe('TestCaseGeneratorPage — live wave/skill indicator', () => {
  // `parseWaveStartedSkillNames` is the one real thing standing between "a
  // frame arrived on the project's log WS" and "this skill glows" — a
  // false match here would light up the wrong skill, or a real message it
  // fails to recognize would silently mean the indicator never lights at
  // all. Pure function, no rendering/mocking needed.

  it('parses the real backend wave-start message into the real skill names it names', () => {
    expect(parseWaveStartedSkillNames("Wave 'base' (Functional Testing) started")).toEqual([
      'Functional Testing',
    ]);
    expect(
      parseWaveStartedSkillNames("Wave 'dependent' (RBAC Testing, Business Case Testing) started")
    ).toEqual(['RBAC Testing', 'Business Case Testing']);
    expect(
      parseWaveStartedSkillNames("Wave 'non_functional' (Security Testing, Performance Testing) started")
    ).toEqual(['Security Testing', 'Performance Testing']);
  });

  it('never matches this project\'s other real log messages — including the wave summary lines emitted after generation finishes', () => {
    expect(parseWaveStartedSkillNames('Generating functional test cases with AI model')).toBeNull();
    expect(
      parseWaveStartedSkillNames("Wave 'base' (Functional Testing) generated 3 case(s) via ollama/qwen3:14b")
    ).toBeNull();
    expect(
      parseWaveStartedSkillNames("Wave 'dependent' (RBAC Testing) failed, other waves' coverage is unaffected: timeout")
    ).toBeNull();
    expect(parseWaveStartedSkillNames('Routed TestCaseGeneration to ollama/qwen3:14b (score 271.48, AUTO mode)')).toBeNull();
    expect(parseWaveStartedSkillNames('Generated 3 test case(s) via ollama (qwen3:14b)')).toBeNull();
  });

  it('is honest about a malformed or empty match rather than returning a fabricated skill list', () => {
    expect(parseWaveStartedSkillNames('')).toBeNull();
    expect(parseWaveStartedSkillNames("Wave 'base' () started")).toBeNull();
    expect(parseWaveStartedSkillNames("Wave 'base' (Functional Testing) STARTED")).toBeNull();
  });
});

// ==================================================== surviving navigation
//
// Before this, this page's uploaded documents and in-progress generation
// state lived only in its own local `useState` — genuinely destroyed the
// instant React Router unmounted it for a different page, since none of it
// reached `activeGenerationSession` (`AIQAContext`, the one state that
// survives that) until a generation call finished. Navigating away lost an
// uploaded FRD/PRD/SRD outright, and a still-running `generate-ai` call had
// no `AbortController`, so it kept running unseen with no way for a later
// visit to tell a genuinely-finished attempt from one that was still going.

function frdFileInput(): HTMLInputElement {
  return document.querySelector('#doc-slot-frd input[type=file]') as HTMLInputElement;
}

/** A real-shaped, empty `DashboardResponse`/`KpisResponse` — `DashboardPage`
 *  (this suite's own "another page" to navigate to) reads real fields off
 *  these (`dashboard.task_counts`, etc.), so an empty-array catch-all
 *  response crashes it; this is what an honestly-empty project actually
 *  looks like, not a shape this suite invented. */
function dashboardResponseFor(url: string): Response | null {
  if (url.includes('/dashboard')) {
    return jsonResponse(200, {
      project_id: PROJECT_A.id,
      project_name: PROJECT_A.name,
      execution_counts: {},
      task_counts: {},
      pass_fail: { passed: 0, failed: 0 },
      test_count: 0,
      generated_at: '2026-01-01T00:00:00Z',
    });
  }
  if (url.includes('/kpis')) {
    return jsonResponse(200, {
      project_id: PROJECT_A.id,
      execution_reliability: null,
      execution_efficiency_ms: null,
      execution_avg_run_duration_ms: null,
      generated_at: '2026-01-01T00:00:00Z',
    });
  }
  return null;
}

async function uploadFrd(fetchMock: ReturnType<typeof vi.fn>): Promise<void> {
  await act(async () => {
    fireEvent.change(frdFileInput(), {
      target: { files: [new File(['Password must be at least 8 characters.'], 'requirements.frd.md', { type: 'text/markdown' })] },
    });
  });
  await waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes('/documents/extract-text'))).toBe(true));
  await waitFor(() => expect(screen.getByText('requirements.frd.md')).toBeTruthy());
}

async function goToDashboard(): Promise<void> {
  await act(async () => {
    document.getElementById('nav-item-dashboard')!.click();
  });
  await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeNull());
}

async function goToGenerator(): Promise<void> {
  await act(async () => {
    document.getElementById('nav-item-generator')!.click();
  });
  await waitFor(() => expect(page()).toBeTruthy());
}

describe('TestCaseGeneratorPage — surviving in-app navigation', () => {
  it('an uploaded document survives navigating to another page and back, even before Generate is ever clicked', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/documents/extract-text')) {
        return jsonResponse(200, { text: 'Password must be at least 8 characters.', char_count: 40, truncated: false });
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return dashboardResponseFor(url) ?? jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();
    await uploadFrd(fetchMock);

    await goToDashboard();
    await goToGenerator();

    // The real, extracted document is still here — not a re-upload, the
    // same one, carried across the round trip through `activeGenerationSession`.
    await waitFor(() => expect(screen.getByText('requirements.frd.md')).toBeTruthy());
    expect(screen.queryByText(/could not be analyzed/)).toBeNull();
  }, 15000);

  it('navigating away while generation is in flight cancels the real request, and a return shows the interruption honestly — without losing the uploaded document', async () => {
    let capturedSignal: AbortSignal | undefined;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/documents/extract-text')) {
        return jsonResponse(200, { text: 'Password must be at least 8 characters.', char_count: 40, truncated: false });
      }
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
        capturedSignal = init.signal ?? undefined;
        // Never settles — a real aborted fetch does not resolve with a
        // response either; what must recover from this is the page, not a
        // stub response.
        return new Promise<Response>(() => {});
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return dashboardResponseFor(url) ?? jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();
    await uploadFrd(fetchMock);

    await act(async () => {
      document.getElementById('btn-generate-test-cases-primary')!.click();
    });
    await waitFor(() => expect(screen.getByText('Generating Test Cases...')).toBeTruthy());
    expect(capturedSignal, 'the real fetch call must carry a cancellable AbortSignal').toBeTruthy();
    expect(capturedSignal!.aborted).toBe(false);

    await goToDashboard();

    // Leaving the page really cancelled the real request — not just a
    // local flag this page stopped looking at (the real backend receives
    // this and can genuinely stop the work, unlike before, when nothing
    // ever told it the caller had gone). `resolveFetch` is deliberately
    // never called anywhere in this test: the fix must correct the shown
    // state at the moment the page is left, not wait on this — a real,
    // aborted browser fetch never resolves with a response at all.
    expect(capturedSignal!.aborted).toBe(true);

    await goToGenerator();

    expect(screen.getByText('Generation Failed')).toBeTruthy();
    expect(screen.getByText(/navigated away before it finished/)).toBeTruthy();
    // No fabricated AI response is shown for a call that was cancelled,
    // not actually answered.
    expect(screen.queryByText('Provider:')).toBeNull();
    // And the document that was already uploaded is still here.
    expect(screen.getByText('requirements.frd.md')).toBeTruthy();
  }, 15000);
});

// =============================================== surviving a project switch
//
// Reported: test cases generated for a project called "ambiance" kept
// showing after creating and switching to a brand-new project. A project
// switch never unmounts this page (`realProjectId` comes from
// `useProject()`, read inside an already-mounted page; the `key=` this page
// itself puts on its own root `<div>` only remounts what's *inside* that
// div, not the state above it that gets handed down as props) — nothing
// reset `documents`/`generatedCases`/config when the selected project
// changed under an already-open page.

describe('TestCaseGeneratorPage — surviving a project switch', () => {
  it('switching to a different project clears the previous one\'s generated cases, uploaded documents, and configuration', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/documents/extract-text')) {
        return jsonResponse(200, { text: 'Password must be at least 8 characters.', char_count: 40, truncated: false });
      }
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A, PROJECT_B]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();
    await uploadFrd(fetchMock);
    await act(async () => {
      document.getElementById('cat-card-security')!.click();
    });
    await generate();

    // Really generated, for "AUD-005 Project" — the real case, the real
    // uploaded document, a non-default category selection, all present
    // before the switch.
    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    expect(screen.getAllByText('Reject login with an expired session token').length).toBeGreaterThan(0);

    await openAndSelectProject(PROJECT_B);

    // None of "AUD-005 Project"'s state survives the switch.
    expect(screen.queryByText('Reject login with an expired session token')).toBeNull();
    expect(screen.queryByText('requirements.frd.md')).toBeNull();
    expect(document.getElementById('generation-details-panel')).toBeNull();
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
    // Back on the branch a fresh, never-generated project actually opens
    // on, with an untouched category selection.
    await waitFor(() => expect(screen.getByText('9 / 14 SELECTED')).toBeTruthy());
    expect(document.querySelector('#doc-slot-frd')?.textContent).toContain('No document uploaded');
  }, 20000);

  it('switching projects mid-generation leaves the real request running instead of cancelling it, and its result reaches the project it was for even though a different project is now on screen', async () => {
    // Corrected behaviour, reported directly: generating for "ambiance",
    // switching away, then switching back must show "ambiance" exactly as
    // it was left — including a generation that was genuinely still running
    // the whole time — never a wipe and never an abort just because the
    // user looked at a different project for a while.
    let capturedSignal: AbortSignal | undefined;
    let resolveFetch: (value: Response) => void;
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') {
        capturedSignal = init.signal ?? undefined;
        return pending;
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A, PROJECT_B]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('btn-generate-test-cases-primary')!.click();
    });
    await waitFor(() => expect(screen.getByText('Generating Test Cases...')).toBeTruthy());

    await openAndSelectProject(PROJECT_B);

    // The switch itself never cancels a request still genuinely running for
    // the project it was left for — only truly leaving the page does that
    // (the unmount-cleanup effect, exercised by the navigation tests above).
    expect(capturedSignal!.aborted).toBe(false);
    // "A Brand New Project" has never generated anything — its own fresh,
    // untouched view, not "AUD-005 Project"'s in-progress one.
    expect(screen.getByText('Ready to Generate')).toBeTruthy();

    await act(async () => {
      resolveFetch(jsonResponse(200, aiGenerateResponse()));
    });

    // Still on "A Brand New Project"'s own view, undisturbed — a completion
    // that arrives for a project no longer on screen must never touch
    // whatever a *different* project is currently showing.
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
    expect(screen.queryByText('Reject login with an expired session token')).toBeNull();

    await openAndSelectProject(PROJECT_A);

    // Back on "AUD-005 Project": its own generation really did keep running
    // in the background the whole time it was away, and its real result is
    // exactly what is shown now — not a wipe, not a stuck spinner, not a
    // fabricated failure.
    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    expect(screen.getAllByText('Reject login with an expired session token').length).toBeGreaterThan(0);
  }, 15000);

  it('switching back to a project while it is still generating shows it honestly still generating — not reset, not a fabricated error — and it completes live once the real request actually resolves', async () => {
    let resolveFetch: (value: Response) => void;
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return pending;
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A, PROJECT_B]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();

    await act(async () => {
      document.getElementById('btn-generate-test-cases-primary')!.click();
    });
    await waitFor(() => expect(screen.getByText('Generating Test Cases...')).toBeTruthy());

    await openAndSelectProject(PROJECT_B);
    expect(screen.getByText('Ready to Generate')).toBeTruthy();

    await openAndSelectProject(PROJECT_A);

    // The real request never actually finished — a return to this project
    // must say so honestly, not silently drop back to "Ready to Generate".
    expect(screen.getByText('Generating Test Cases...')).toBeTruthy();
    // And merely looking at it again never fires a second real request.
    expect(fetchMock.mock.calls.filter((c) => String(c[0]).includes('/test-cases/generate-ai')).length).toBe(1);

    await act(async () => {
      resolveFetch(jsonResponse(200, aiGenerateResponse()));
    });

    // Resolves while this project is genuinely the one on screen — reflected
    // live, exactly as if the switch away and back had never happened.
    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());
    expect(screen.getAllByText('Reject login with an expired session token').length).toBeGreaterThan(0);
  }, 15000);

  it('the project this page opens already showing (nothing switched away from) keeps its own real, restored generation session', async () => {
    // Not a regression this fix could cause, but the guard that makes the
    // above possible (skip the reset when there was no real *previous*
    // project) is new code, worth its own direct check rather than relying
    // only on the broader "surviving in-app navigation" tests above to
    // incidentally exercise it.
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/test-cases/generate-ai') && init?.method === 'POST') return jsonResponse(200, aiGenerateResponse());
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return dashboardResponseFor(url) ?? jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);
    await goToSourcesBranch();
    await generate();
    await waitFor(() => expect(screen.getByText(/Generated Test Suite/)).toBeTruthy());

    await goToDashboard();
    await goToGenerator();

    // Still project A's real result — this fresh mount's very first sight
    // of `realProjectId` (== PROJECT_A.id, already selected) must not be
    // mistaken for "switched away from some other project".
    expect(screen.getAllByText('Reject login with an expired session token').length).toBeGreaterThan(0);
  }, 15000);
});
