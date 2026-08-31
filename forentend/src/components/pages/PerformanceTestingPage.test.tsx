/**
 * Phase 6 — Performance Testing. Renders the real `AppShell` (real
 * providers, real routed pages) inside a `MemoryRouter`, exactly like
 * `APITestingPage.test.tsx`/`CustomTestingPage.test.tsx` — `fetch` is
 * mocked only at the transport boundary. Live polling uses fake timers
 * (same pattern as `RealtimeContext.test.tsx`) so tests never depend on
 * real wall-clock time.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { PerformanceRunStatusResponse, PerformanceTestStartResponse, Project } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const PROJECT_A: Project = { id: 'proj-perf', name: 'Performance Project', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };

function runningStatus(overrides: Partial<PerformanceRunStatusResponse> = {}): PerformanceRunStatusResponse {
  return {
    run_id: 'perf-run-1',
    name: 'LOAD test: GET https://example.com/health',
    test_type: 'LOAD',
    status: 'RUNNING',
    verdict: null,
    verdict_reasons: [],
    error_reason: null,
    elapsed_ms: 1200,
    current_concurrency: 5,
    sample_count: 12,
    metrics: { total_requests: 12, successful: 12, failed: 0, errored: 0, error_rate_percent: 0, success_rate_percent: 100, min_ms: 10, avg_ms: 20, max_ms: 35, p50_ms: 19, p95_ms: 33, p99_ms: 35, throughput_rps: 10, actual_duration_ms: 1200 },
    samples: [{ sequence: 0, offset_ms: 100, duration_ms: 20, outcome: 'SUCCESS', http_status: 200, error_reason: null, concurrency_at_start: 5 }],
    config: { request: { method: 'GET', url: 'https://example.com/health' }, expected_status: 200, schedule: { test_type: 'LOAD', concurrency: 5, duration_secs: 15, max_requests: null } },
    started_at: '2026-01-01T00:00:00.000Z',
    finished_at: null,
    evidence_id: null,
    live: true,
    ...overrides,
  };
}

function completedStatus(overrides: Partial<PerformanceRunStatusResponse> = {}): PerformanceRunStatusResponse {
  return {
    ...runningStatus(),
    status: 'COMPLETED',
    verdict: 'PASSED',
    verdict_reasons: ['all configured thresholds were met'],
    elapsed_ms: 15000,
    sample_count: 150,
    finished_at: '2026-01-01T00:00:15.000Z',
    metrics: { total_requests: 150, successful: 150, failed: 0, errored: 0, error_rate_percent: 0, success_rate_percent: 100, min_ms: 8, avg_ms: 18, max_ms: 40, p50_ms: 17, p95_ms: 30, p99_ms: 38, throughput_rps: 10, actual_duration_ms: 15000 },
    live: true,
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

const RAW_KEY = 'aiqa_perftesting0000000000';

function renderApp() {
  return render(
    <MemoryRouter initialEntries={['/performance-testing']}>
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
  return document.getElementById('performance-testing-page');
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
  vi.useRealTimers();
});

describe('PerformanceTestingPage — project context and validation', () => {
  it('with no active project, shows the real truthful state and sends no start request', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, []));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();

    await waitFor(() => expect(page()).toBeTruthy());
    expect(screen.getByText('Select a project before running a performance test')).toBeTruthy();
    expect(document.getElementById('perf-url')).toBeNull();
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).not.toContain('/performance-testing/start');
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
      document.getElementById('btn-start-performance-test')!.click();
    });
    await waitFor(() => expect(screen.getByText(/INVALID_URL/)).toBeTruthy());
    for (const [url] of fetchMock.mock.calls as [string][]) {
      expect(String(url)).not.toContain('/performance-testing/start');
    }
  });
});

describe('PerformanceTestingPage — real Load workload: start, live poll, terminal state', () => {
  it('sends the real LOAD schedule, shows real live metrics while RUNNING, then stops polling once terminal', async () => {
    let capturedStartBody: any = null;
    let statusCallCount = 0;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/performance-testing/start')) {
        capturedStartBody = JSON.parse(String(init?.body ?? '{}'));
        return jsonResponse(200, { run_id: 'perf-run-1', status: 'RUNNING' } satisfies PerformanceTestStartResponse);
      }
      if (url.includes('/performance-testing/perf-run-1/status')) {
        statusCallCount += 1;
        return jsonResponse(200, statusCallCount === 1 ? runningStatus() : completedStatus());
      }
      if (url.endsWith('/performance-testing')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('perf-url')!, { target: { value: 'https://example.com/health' } });
    // Only switch to fake timers now, right before the polling interval
    // matters — login/project-selection above used real timers so their
    // own async chains (unrelated to this page's polling) are never stuck
    // waiting on a fake clock that nothing advances.
    vi.useFakeTimers();
    // Fake timers active: `waitFor`'s own internal polling never gets a
    // chance to re-check the DOM without an explicit timer advance (same
    // reasoning as `RealtimeContext.test.tsx`'s fake-timer tests) — flush
    // the Start click's full async chain (start fetch -> setRunId -> first
    // poll -> status fetch -> setLive) with an explicit 0ms advance instead
    // of `waitFor`, then assert directly.
    await act(async () => {
      document.getElementById('btn-start-performance-test')!.click();
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(capturedStartBody).toBeTruthy();
    expect(capturedStartBody.request).toEqual({ method: 'GET', url: 'https://example.com/health' });
    expect(capturedStartBody.expected_status).toBe(200);
    expect(capturedStartBody.schedule).toEqual({ test_type: 'LOAD', concurrency: 5, duration_secs: 15, max_requests: undefined });

    // First poll landed as part of Start — real, live RUNNING metrics from
    // the real first response, not a fabricated progress bar.
    expect(screen.getByText('Running (real, live)')).toBeTruthy();
    expect(screen.getByText('12')).toBeTruthy(); // sample_count

    // Advance exactly one real poll interval — the second, terminal
    // response must be reflected, and no further polling should occur.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByText('Passed')).toBeTruthy();
    expect(statusCallCount).toBe(2);

    const countAfterTerminal = statusCallCount;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(statusCallCount).toBe(countAfterTerminal); // polling genuinely stopped, not still ticking
  });

  it('switching to STRESS sends the real, distinct stress schedule fields', async () => {
    let capturedStartBody: any = null;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/performance-testing/start')) {
        capturedStartBody = JSON.parse(String(init?.body ?? '{}'));
        return jsonResponse(200, { run_id: 'perf-run-2', status: 'RUNNING' } satisfies PerformanceTestStartResponse);
      }
      if (url.includes('/performance-testing/perf-run-2/status')) return jsonResponse(200, completedStatus({ run_id: 'perf-run-2' }));
      if (url.endsWith('/performance-testing')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('perf-url')!, { target: { value: 'https://example.com/health' } });
    await act(async () => {
      document.getElementById('perf-type-stress')!.click();
    });
    await act(async () => {
      document.getElementById('btn-start-performance-test')!.click();
    });

    await waitFor(() => expect(capturedStartBody).toBeTruthy());
    expect(capturedStartBody.schedule).toEqual({ test_type: 'STRESS', start_concurrency: 2, max_concurrency: 20, step_size: 2, step_interval_secs: 5, max_duration_secs: 30 });
  });
});

describe('PerformanceTestingPage — real cancellation', () => {
  it('Cancel calls the real cancel endpoint and reflects the real result', async () => {
    let cancelCalls = 0;
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/performance-testing/start')) return jsonResponse(200, { run_id: 'perf-run-3', status: 'RUNNING' } satisfies PerformanceTestStartResponse);
      if (url.includes('/performance-testing/perf-run-3/cancel')) {
        cancelCalls += 1;
        return jsonResponse(200, { run_id: 'perf-run-3', cancelled: true, reason: null });
      }
      if (url.includes('/performance-testing/perf-run-3/status')) {
        return jsonResponse(200, cancelCalls > 0 ? { ...runningStatus({ run_id: 'perf-run-3' }), status: 'CANCELLED', verdict: 'CANCELLED', verdict_reasons: ['cancelled by user request before the configured schedule completed'] } : runningStatus({ run_id: 'perf-run-3' }));
      }
      if (url.endsWith('/performance-testing')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('perf-url')!, { target: { value: 'https://example.com/health' } });
    await act(async () => {
      document.getElementById('btn-start-performance-test')!.click();
    });
    await waitFor(() => expect(document.getElementById('btn-cancel-performance-test')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-cancel-performance-test')!.click();
    });

    await waitFor(() => expect(cancelCalls).toBe(1));
    await waitFor(() => expect(screen.getByText('Cancelled')).toBeTruthy());
  });
});

describe('PerformanceTestingPage — genuine run-level ERROR display', () => {
  it('ERROR is shown as a distinct, truthful "Execution Error" state — never as a threshold FAIL or a successful completion', async () => {
    const errorStatus: PerformanceRunStatusResponse = {
      ...runningStatus({ run_id: 'perf-run-err' }),
      status: 'ERROR',
      verdict: 'ERROR',
      verdict_reasons: ['performance execution terminated unexpectedly: deterministic test failure'],
      error_reason: 'performance execution terminated unexpectedly: deterministic test failure',
      metrics: { total_requests: 7, successful: 7, failed: 0, errored: 0, error_rate_percent: 0, success_rate_percent: 100, min_ms: 5, avg_ms: 8, max_ms: 12, p50_ms: 8, p95_ms: 12, p99_ms: 12, throughput_rps: 3.5, actual_duration_ms: 2000 },
      sample_count: 7,
    };
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/performance-testing/start')) return jsonResponse(200, { run_id: 'perf-run-err', status: 'RUNNING' } satisfies PerformanceTestStartResponse);
      if (url.includes('/performance-testing/perf-run-err/status')) return jsonResponse(200, errorStatus);
      if (url.endsWith('/performance-testing')) return jsonResponse(200, []);
      if (url.endsWith('/projects')) return jsonResponse(200, [PROJECT_A]);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    renderApp();
    await verifyKey();
    await waitFor(() => expect(page()).toBeTruthy());
    await openAndSelectProject(PROJECT_A);

    fireEvent.change(document.getElementById('perf-url')!, { target: { value: 'https://example.com/health' } });
    await act(async () => {
      document.getElementById('btn-start-performance-test')!.click();
    });

    // Distinct label from both a threshold FAIL and a normal completion —
    // never rendered as "Failed" or "Passed" or "Completed".
    await waitFor(() => expect(screen.getByText('Error')).toBeTruthy());
    expect(screen.queryByText('Failed')).toBeNull();
    expect(screen.queryByText('Passed')).toBeNull();
    expect(screen.queryByText('Completed')).toBeNull();

    // The real, safe error reason is rendered in the dedicated Execution
    // Error panel, not silently dropped.
    expect(document.getElementById('perf-execution-error')).toBeTruthy();
    expect(screen.getByText('performance execution terminated unexpectedly: deterministic test failure')).toBeTruthy();

    // The generic Verdict Reasons panel does not also duplicate the same
    // text a second time under a different, less specific heading.
    expect(document.getElementById('perf-verdict-reasons')).toBeNull();

    // Partial real metrics (7 real samples) are shown, not discarded and
    // not padded to look like a complete run.
    expect(screen.getByText('7')).toBeTruthy();
  });
});

describe('PerformanceTestingPage — regression', () => {
  it('Dashboard remains unregressed with PerformanceTestingPage present in the route table', async () => {
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
