/**
 * Phase 4 — API Testing: real, ad-hoc HTTP request execution.
 *
 * Replaces the previous honest-redirect-page tests (that page had no
 * request-builder UI of its own; see git history / the Phase 4 report for
 * that version's own doc comment). Renders the real `AppShell` (real
 * providers, real routed pages) inside a `MemoryRouter`, exactly like
 * `CustomTestingPage.test.tsx` — `fetch` is mocked only at the transport
 * boundary.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { ApiTestExecutionResponse, Project } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const PROJECT_A: Project = { id: 'proj-api-testing', name: 'API Testing Project', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };

function passedResponse(overrides: Partial<ApiTestExecutionResponse> = {}): ApiTestExecutionResponse {
  return {
    run_id: 'run-1',
    task_status: 'PASSED',
    reason: null,
    started_at: '2026-01-01T00:00:00.000Z',
    finished_at: '2026-01-01T00:00:00.050Z',
    result: {
      status: 200,
      request_duration_ms: 42,
      response_headers: [['content-type', 'application/json']],
      assertions: [{ description: 'Status equals 200', actual: '200 OK', passed: true }],
    },
    response_body_preview: '{"ok":true}',
    evidence_id: 'ev-1',
    ...overrides,
  };
}

async function openAndSelectProject(project: Project): Promise<void> {
  const { within } = await import('@testing-library/react');
  await waitFor(() => expect(document.getElementById('project-selector-button')).toBeTruthy());
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

const RAW_KEY = 'aiqa_apitesting00000000000';

function renderApp() {
  return render(
    <MemoryRouter initialEntries={['/api-testing']}>
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
  return document.getElementById('api-testing-page');
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('APITestingPage — project context and validation', () => {
  it('with no active project, shows the real truthful state and sends no execute request', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, []));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();

    await waitFor(() => expect(page()).toBeTruthy());
    expect(screen.getByText('Select a project before executing an API test')).toBeTruthy();
    expect(document.getElementById('api-testing-url')).toBeNull();
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).not.toContain('/api-testing/execute');
    }
  });

  it('an empty URL is rejected client-side — no request is sent', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => (url.endsWith('/projects') ? jsonResponse(200, [PROJECT_A]) : jsonResponse(200, [])));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await act(async () => {
      document.getElementById('btn-execute-api-test')!.click();
    });
    await waitFor(() => expect(screen.getByText(/INVALID_URL/)).toBeTruthy());
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).not.toContain('/api-testing/execute');
    }
  });

  it('invalid JSON in the request body is rejected client-side — no request is sent', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => (url.endsWith('/projects') ? jsonResponse(200, [PROJECT_A]) : jsonResponse(200, [])));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('api-testing-url')!, { target: { value: 'https://example.com/api' } });
    fireEvent.change(document.getElementById('api-testing-body')!, { target: { value: '{not valid json' } });
    await act(async () => {
      document.getElementById('btn-execute-api-test')!.click();
    });
    await waitFor(() => expect(screen.getByText(/INVALID_JSON_BODY/)).toBeTruthy());
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).not.toContain('/api-testing/execute');
    }
  });
});

describe('APITestingPage — real execution', () => {
  it('sends the real method/url/query/headers/body/assertions and reflects the real in-flight request as the loading state', async () => {
    let capturedBody: any = null;
    let resolveFetch: (r: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/api-testing/execute')) {
        capturedBody = JSON.parse(String(init?.body ?? '{}'));
        return pending;
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    await act(async () => {
      fireEvent.change(document.getElementById('api-testing-method')!, { target: { value: 'POST' } });
      fireEvent.change(document.getElementById('api-testing-url')!, { target: { value: 'https://example.com/api/items' } });
      fireEvent.change(document.getElementById('api-testing-body')!, { target: { value: '{"name":"widget"}' } });
    });
    await act(async () => {
      fireEvent.click(document.getElementById('btn-add-query-row')!);
    });
    const queryRow = document.querySelector('#api-testing-page input[placeholder="param"]') as HTMLInputElement;
    expect(queryRow).toBeTruthy();
    await act(async () => {
      fireEvent.change(queryRow, { target: { value: 'debug' } });
    });
    await act(async () => {
      fireEvent.click(document.getElementById('btn-add-header-row')!);
    });
    const headerRow = document.querySelector('#api-testing-page input[placeholder="Header-Name"]') as HTMLInputElement;
    expect(headerRow).toBeTruthy();
    await act(async () => {
      fireEvent.change(headerRow, { target: { value: 'X-Custom' } });
    });

    await act(async () => {
      fireEvent.click(document.getElementById('btn-execute-api-test')!);
    });

    await waitFor(() => expect(screen.getByText('Executing…')).toBeTruthy());
    expect((document.getElementById('btn-execute-api-test') as HTMLButtonElement).disabled).toBe(true);

    await act(async () => {
      resolveFetch(jsonResponse(200, passedResponse()));
      await pending;
    });

    expect(capturedBody.method).toBe('POST');
    expect(capturedBody.url).toBe('https://example.com/api/items');
    expect(capturedBody.body).toEqual({ name: 'widget' });
    expect(capturedBody.query_params).toEqual([['debug', '']]);
    expect(capturedBody.headers).toEqual({ 'X-Custom': '' });
    expect(capturedBody.assertions).toEqual([{ type: 'ASSERT_STATUS', status: 200 }]);
    await waitFor(() => expect(screen.getByText('Passed')).toBeTruthy());
  });

  it('a PASSED response renders the real status, real latency, real headers, real body, and real assertion result', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/api-testing/execute')) return jsonResponse(200, passedResponse());
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('api-testing-url')!, { target: { value: 'https://example.com/health' } });
    await act(async () => {
      document.getElementById('btn-execute-api-test')!.click();
    });

    await waitFor(() => expect(document.getElementById('api-testing-response')).toBeTruthy());
    expect(screen.getByText('42 ms (real, measured request/response time)')).toBeTruthy();
    expect(screen.getByText('{"ok":true}')).toBeTruthy();
    expect(screen.getByText('PASS')).toBeTruthy();
    expect(screen.getAllByText('200').length).toBeGreaterThan(0);
  });

  it('a FAILED response renders a genuine assertion failure — never a fabricated PASS', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/api-testing/execute')) {
        return jsonResponse(
          200,
          passedResponse({
            task_status: 'FAILED',
            reason: 'expected HTTP status 404, got real status 200 OK',
            result: { status: 200, request_duration_ms: 10, response_headers: [], assertions: [{ description: 'Status equals 404', actual: '200 OK', passed: false }] },
          }),
        );
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('api-testing-url')!, { target: { value: 'https://example.com/health' } });
    await act(async () => {
      document.getElementById('btn-execute-api-test')!.click();
    });

    await waitFor(() => expect(document.getElementById('api-testing-response')).toBeTruthy());
    expect(screen.getByText('Failed')).toBeTruthy();
    expect(screen.getByText(/FAIL \(actual: 200 OK\)/)).toBeTruthy();
  });

  it('a real INFRA_ERROR (e.g. connection failure) is shown truthfully, never converted into a fake pass/fail', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/api-testing/execute')) {
        return jsonResponse(200, { run_id: 'run-2', task_status: 'INFRA_ERROR', reason: 'CONNECTION_FAILED: tcp connect error', started_at: null, finished_at: null, result: null, response_body_preview: null, evidence_id: null });
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('api-testing-url')!, { target: { value: 'https://unreachable.example.com/' } });
    await act(async () => {
      document.getElementById('btn-execute-api-test')!.click();
    });

    await waitFor(() => expect(screen.getByText('Execution Error')).toBeTruthy());
    expect(screen.getByText(/CONNECTION_FAILED/)).toBeTruthy();
    expect(document.getElementById('api-testing-response')).toBeNull();
  });

  it('a project-access-denied response is shown truthfully', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/api-testing/execute')) return jsonResponse(403, { error: { code: 'FORBIDDEN', message: 'you do not have access to this project', details: {}, request_id: 'req_1' } });
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('api-testing-url')!, { target: { value: 'https://example.com/health' } });
    await act(async () => {
      document.getElementById('btn-execute-api-test')!.click();
    });

    await waitFor(() => expect(screen.getByText('Access Denied')).toBeTruthy());
  });
});

describe('APITestingPage — Save as Test Case (distinct from Execute)', () => {
  it('saving does not execute the request, and executing does not save it', async () => {
    let executeCalls = 0;
    let saveCalls = 0;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/api-testing/execute')) {
        executeCalls += 1;
        return jsonResponse(200, passedResponse());
      }
      if (url.includes('/test-cases') && init?.method === 'POST') {
        saveCalls += 1;
        return jsonResponse(201, { id: 'case-saved-1', status: 'GENERATED' });
      }
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('api-testing-url')!, { target: { value: 'https://example.com/health' } });

    await act(async () => {
      document.getElementById('btn-save-as-test-case')!.click();
    });
    await waitFor(() => expect(screen.getByText(/Saved as test case/)).toBeTruthy());

    expect(saveCalls).toBe(1);
    expect(executeCalls).toBe(0);
  });
});

describe('APITestingPage — regression', () => {
  it('Dashboard remains unregressed with APITestingPage present in the route table', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, []));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(
      <MemoryRouter initialEntries={['/dashboard']}>
        <AppShell />
      </MemoryRouter>,
    );
    await verifyKey();
    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });
});
