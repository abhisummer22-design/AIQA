/**
 * Phase 3 — Custom Testing: real requirement -> AI strategy -> test cases.
 *
 * Replaces the previous truthful "not implemented" placeholder tests (moved
 * out of `CaseCPages.test.tsx`, see that file's own doc comment). Renders
 * the real `AppShell` (real providers, real routed pages) inside a
 * `MemoryRouter`, exactly like `TestCaseGeneratorPage.test.tsx` — `fetch` is
 * mocked only at the transport boundary.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { CustomTestingGenerateResponse, Project } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const PROJECT_A: Project = { id: 'proj-custom-testing', name: 'Custom Testing Project', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };

function customTestingResponse(overrides: Partial<CustomTestingGenerateResponse> = {}): CustomTestingGenerateResponse {
  return {
    generation_source: 'REAL_AI',
    analysis: {
      objective: 'Verify login lockout after repeated invalid attempts',
      normalized_scope: 'Authentication',
      recommended_skills: ['Security Testing', 'Edge Case Analysis'],
      recommended_testing_types: ['Negative', 'Security'],
      risk_areas: ['Account takeover via brute force'],
      assumptions: ['Lockout threshold is a fixed number of attempts'],
      prerequisites: ['A valid test account'],
      limitations: ['Exact lockout threshold not stated in the requirement'],
      generation_approach: 'Deterministic negative-path scenarios around repeated invalid attempts',
      diagnostics: [],
    },
    test_cases: [
      {
        title: 'Lockout after 5 invalid attempts',
        category: 'Security',
        skill_id: 'skill-security-testing',
        preconditions: [],
        steps: [{ action: 'Submit 5 invalid passwords in a row', expected_result: 'Account is locked' }],
        expected_result: 'Account is locked',
        priority: 'HIGH',
        severity: 'MAJOR',
        evidence: 'evidenced',
        source_reference: 'repeated invalid password attempts',
      },
    ],
    provider: 'ollama',
    model: 'llama3',
    failure_reason: null,
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

const RAW_KEY = 'aiqa_customtesting0000000';

function renderApp() {
  return render(
    <MemoryRouter initialEntries={['/custom-testing']}>
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

function page(): HTMLElement | null {
  return document.getElementById('custom-testing-page');
}

async function enterRequirement(text: string, scope?: string): Promise<void> {
  fireEvent.change(document.getElementById('custom-testing-requirement')!, { target: { value: text } });
  if (scope !== undefined) {
    fireEvent.change(document.getElementById('custom-testing-scope')!, { target: { value: scope } });
  }
}

async function clickAnalyzeAndGenerate(): Promise<void> {
  await act(async () => {
    document.getElementById('btn-analyze-and-generate')!.click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('CustomTestingPage — project context and requirement validation', () => {
  it('with no active project, shows the real truthful state and sends no generate request', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, []));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();

    await waitFor(() => expect(page()).toBeTruthy());
    expect(screen.getByText('Select a project before creating a custom testing request')).toBeTruthy();
    expect(document.getElementById('custom-testing-requirement')).toBeNull();
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).not.toContain('/custom-testing/generate');
    }
  });

  it('an empty requirement is rejected client-side — no request is sent', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await clickAnalyzeAndGenerate();
    await waitFor(() => expect(screen.getByText('Enter a custom testing requirement before analyzing.')).toBeTruthy());
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).not.toContain('/custom-testing/generate');
    }
  });
});

describe('CustomTestingPage — real generation', () => {
  it('sends the real requirement text and scope, and reflects the real in-flight request as the loading state', async () => {
    let capturedBody: any = null;
    let resolveFetch: (r: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/custom-testing/generate')) {
        capturedBody = JSON.parse(String(init?.body ?? '{}'));
        return pending;
      }
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await enterRequirement('Test login lockout after repeated invalid password attempts.', 'Authentication');
    await act(async () => {
      document.getElementById('btn-analyze-and-generate')!.click();
    });

    await waitFor(() => expect(screen.getByText('Analyzing Requirement')).toBeTruthy());
    expect((document.getElementById('btn-analyze-and-generate') as HTMLButtonElement).disabled).toBe(true);

    await act(async () => {
      resolveFetch(jsonResponse(200, customTestingResponse()));
      await pending;
    });

    expect(capturedBody.requirement_text).toBe('Test login lockout after repeated invalid password attempts.');
    expect(capturedBody.scope).toBe('Authentication');
    await waitFor(() => expect(screen.getByText('Generated')).toBeTruthy());
  });

  it('a REAL_AI response renders the real strategy and the real generated test cases — never fabricated', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/custom-testing/generate')) return jsonResponse(200, customTestingResponse());
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await enterRequirement('Test login lockout after repeated invalid password attempts.', 'Authentication');
    await clickAnalyzeAndGenerate();

    await waitFor(() => expect(document.getElementById('custom-testing-strategy')).toBeTruthy());
    expect(screen.getByText('Verify login lockout after repeated invalid attempts')).toBeTruthy();
    expect(screen.getByText('Authentication', { selector: 'p' })).toBeTruthy();
    expect(screen.getByText('Account takeover via brute force')).toBeTruthy();

    await waitFor(() => expect(document.getElementById('custom-testing-generated-cases')).toBeTruthy());
    expect(screen.getByText('Lockout after 5 invalid attempts')).toBeTruthy();
  });

  it('AI-recommended skills are shown distinctly from the user\'s own selection and can be added explicitly', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/custom-testing/generate')) return jsonResponse(200, customTestingResponse());
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await enterRequirement('Test login lockout after repeated invalid password attempts.', 'Authentication');
    await clickAnalyzeAndGenerate();
    await waitFor(() => expect(document.getElementById('custom-testing-strategy')).toBeTruthy());

    // "Edge Case Analysis" is one of the two default-selected skills
    // (`DEFAULT_SELECTED_SKILL_IDS`), so it must render as already-selected
    // (✓) even though the AI also recommended it — real state, not a
    // recomputed guess.
    expect(screen.getByText('✓ Edge Case Analysis')).toBeTruthy();
    // "Security Testing" is recommended but NOT among the defaults, so it
    // must render as an addable suggestion (+), distinct from the user's
    // own selection — clicking it genuinely adds it, proven by the badge
    // flipping to ✓ afterward.
    expect(screen.getByText('+ Security Testing')).toBeTruthy();
    fireEvent.click(screen.getByText('+ Security Testing'));
    await waitFor(() => expect(screen.getByText('✓ Security Testing')).toBeTruthy());
  });

  it('AI_UNAVAILABLE (no real provider reachable) shows a truthful error, never a fabricated strategy or cases', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/custom-testing/generate')) {
        return jsonResponse(200, {
          generation_source: 'UNAVAILABLE',
          analysis: null,
          test_cases: [],
          provider: null,
          model: null,
          failure_reason: 'NoCompatibleModel { task: RequirementAnalysis }',
        });
      }
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await enterRequirement('Test login lockout.');
    await clickAnalyzeAndGenerate();

    await waitFor(() => expect(screen.getByText('AI Unavailable')).toBeTruthy());
    expect(screen.getByText('NoCompatibleModel { task: RequirementAnalysis }')).toBeTruthy();
    expect(document.getElementById('custom-testing-strategy')).toBeNull();
    expect(document.getElementById('custom-testing-generated-cases')).toBeNull();
  });

  it('a real analysis that is produced but whose case generation then fails is shown honestly, with no cases section', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/custom-testing/generate')) {
        return jsonResponse(200, customTestingResponse({ generation_source: 'UNAVAILABLE', test_cases: [], provider: null, model: null, failure_reason: 'invalid structured output: EOF' }));
      }
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await enterRequirement('Test login lockout.');
    await clickAnalyzeAndGenerate();

    await waitFor(() => expect(screen.getByText('Generation Failed')).toBeTruthy());
    expect(screen.getByText('invalid structured output: EOF')).toBeTruthy();
    // The real, successfully-produced strategy is still shown — never
    // discarded just because case generation subsequently failed.
    expect(document.getElementById('custom-testing-strategy')).toBeTruthy();
    expect(document.getElementById('custom-testing-generated-cases')).toBeNull();
  });

  it('a project-access-denied response is shown truthfully, distinct from AI unavailability', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/custom-testing/generate')) {
        return jsonResponse(403, { error: { code: 'FORBIDDEN', message: 'you do not have access to this project', details: {}, request_id: 'req_1' } });
      }
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await enterRequirement('Test login lockout.');
    await clickAnalyzeAndGenerate();

    await waitFor(() => expect(screen.getByText('Project Access Denied')).toBeTruthy());
  });
});

describe('CustomTestingPage — continuing into the existing Test Case Generator pipeline', () => {
  it('Continue to Test Case Generator navigates and the real generated cases appear on the existing Generated Cases branch', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/custom-testing/generate')) return jsonResponse(200, customTestingResponse());
      if (url.includes('/test-cases')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await enterRequirement('Test login lockout after repeated invalid password attempts.', 'Authentication');
    await clickAnalyzeAndGenerate();
    await waitFor(() => expect(document.getElementById('custom-testing-generated-cases')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-continue-to-test-case-generator')!.click();
    });

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getAllByText('Lockout after 5 invalid attempts').length).toBeGreaterThan(0);
  });
});
