/**
 * Tests for the real System Telemetry page (build order item 2, 22 Aug
 * 2026). `fetch` is mocked only at the transport boundary — `TelemetryPage`
 * itself makes real `aiqaApi.metrics.get()` calls and parses whatever real
 * Prometheus text the mock returns, exactly like production.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import { parsePrometheusText } from './TelemetryPage';

function textResponse(status: number, body: string, contentType = 'text/plain; version=0.0.4'): Response {
  return new Response(body, { status, headers: { 'Content-Type': contentType } });
}

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const RAW_KEY = 'aiqa_telemetrytest000000';

const REAL_METRICS_TEXT = [
  '# HELP aiqa_runs_total Number of runs by state',
  '# TYPE aiqa_runs_total gauge',
  'aiqa_runs_total{state="COMPLETED"} 7',
  'aiqa_runs_total{state="FAILED"} 2',
  'aiqa_runs_total{state="RUNNING"} 1',
  '# HELP aiqa_tasks_total Number of tasks by status (across all runs)',
  '# TYPE aiqa_tasks_total gauge',
  'aiqa_tasks_total{status="PASSED"} 40',
  'aiqa_tasks_total{status="FAILED"} 5',
  'aiqa_tasks_total{status="TIMEOUT"} 3',
  '# HELP aiqa_bug_drafts_total Number of bug drafts by review_state',
  '# TYPE aiqa_bug_drafts_total gauge',
  'aiqa_bug_drafts_total{review_state="NEEDS_REVIEW"} 4',
  '# HELP aiqa_failure_clusters_total Number of distinct failure clusters',
  '# TYPE aiqa_failure_clusters_total gauge',
  'aiqa_failure_clusters_total 6',
  '# HELP aiqa_api_keys_active Number of non-revoked API keys',
  '# TYPE aiqa_api_keys_active gauge',
  'aiqa_api_keys_active 3',
  '# HELP aiqa_audit_log_total Number of audit log entries',
  '# TYPE aiqa_audit_log_total counter',
  'aiqa_audit_log_total 128',
  '',
].join('\n');

const EMPTY_METRICS_TEXT = [
  '# HELP aiqa_runs_total Number of runs by state',
  '# TYPE aiqa_runs_total gauge',
  '# HELP aiqa_tasks_total Number of tasks by status (across all runs)',
  '# TYPE aiqa_tasks_total gauge',
  '# HELP aiqa_bug_drafts_total Number of bug drafts by review_state',
  '# TYPE aiqa_bug_drafts_total gauge',
  '# HELP aiqa_failure_clusters_total Number of distinct failure clusters',
  '# TYPE aiqa_failure_clusters_total gauge',
  'aiqa_failure_clusters_total 0',
  '# HELP aiqa_api_keys_active Number of non-revoked API keys',
  '# TYPE aiqa_api_keys_active gauge',
  'aiqa_api_keys_active 0',
  '# HELP aiqa_audit_log_total Number of audit log entries',
  '# TYPE aiqa_audit_log_total counter',
  'aiqa_audit_log_total 0',
  '',
].join('\n');

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

describe('parsePrometheusText (unit)', () => {
  it('parses real labeled and unlabeled Prometheus lines, ignoring comments', () => {
    const samples = parsePrometheusText(REAL_METRICS_TEXT);
    expect(samples).toContainEqual({ name: 'aiqa_runs_total', labels: { state: 'COMPLETED' }, value: 7 });
    expect(samples).toContainEqual({ name: 'aiqa_tasks_total', labels: { status: 'TIMEOUT' }, value: 3 });
    expect(samples).toContainEqual({ name: 'aiqa_failure_clusters_total', labels: {}, value: 6 });
    // No # HELP/# TYPE line was ever parsed as a sample.
    expect(samples.every((s) => !s.name.startsWith('#'))).toBe(true);
  });

  it('never invents a value for text it cannot parse', () => {
    expect(parsePrometheusText('not a metric line at all\n')).toEqual([]);
    expect(parsePrometheusText('')).toEqual([]);
  });
});

describe('TelemetryPage', () => {
  it('unauthenticated (no key) shows the real auth-state card, not page content', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp('/telemetry');

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('telemetry-page')).toBeNull();
  });

  it('renders real backend data: series and scalars, plus the honest "does not show" disclosure', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (String(url).endsWith('/metrics')) return textResponse(200, REAL_METRICS_TEXT);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    renderApp('/telemetry');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('telemetry-page')).toBeTruthy());
    await waitFor(() => expect(screen.getByText('Runs by State')).toBeTruthy());
    const content = within(document.getElementById('telemetry-content')!);

    expect(content.getByText('COMPLETED')).toBeTruthy();
    expect(content.getByText('7')).toBeTruthy();
    // "TIMEOUT" legitimately appears twice in this component: once as the
    // real bar-row label, once inside the honest explanatory note below the
    // Tasks by Status card (see the disclosure test further down).
    expect(content.getAllByText('TIMEOUT').length).toBeGreaterThan(0);
    expect(content.getByText('NEEDS_REVIEW')).toBeTruthy();
    expect(content.getByText('6')).toBeTruthy(); // failure clusters scalar
    expect(content.getByText('128')).toBeTruthy(); // audit log total scalar

    // The real request actually went to /metrics.
    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/metrics'))).toBe(true);

    // The honest disclosure — no fabricated latency/p95/p99 anywhere.
    expect(document.getElementById('telemetry-disclosure')).toBeTruthy();
    expect(screen.getByText(/latency, response time, p95, or p99/)).toBeTruthy();
  });

  it('shows the real empty state for a genuinely empty /metrics snapshot — never a fabricated chart', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (String(url).endsWith('/metrics')) return textResponse(200, EMPTY_METRICS_TEXT);
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    renderApp('/telemetry');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('telemetry-page')).toBeTruthy());
    await waitFor(() => expect(screen.getByText('No telemetry recorded yet')).toBeTruthy());
    expect(screen.queryByText('Runs by State')).toBeNull();
  });

  it('shows a real, honest error state on a failed fetch — never a fabricated chart', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (String(url).endsWith('/metrics')) {
        return jsonResponse(500, { error: { code: 'INTERNAL_ERROR', message: 'boom', details: null, request_id: 'r-metrics-1' } });
      }
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    renderApp('/telemetry');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('telemetry-page')).toBeTruthy());
    await waitFor(() => expect(screen.getByText("Couldn't load telemetry")).toBeTruthy());
    expect(screen.queryByText('Runs by State')).toBeNull();
  });

  it('Refresh makes a real second request to the real /metrics endpoint', async () => {
    let calls = 0;
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (String(url).endsWith('/metrics')) {
        calls += 1;
        return textResponse(200, REAL_METRICS_TEXT);
      }
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);

    renderApp('/telemetry');
    await verifyKey();
    await waitFor(() => expect(document.getElementById('telemetry-page')).toBeTruthy());
    await waitFor(() => expect(screen.getByText('Runs by State')).toBeTruthy());
    const callsAfterMount = calls;

    await act(async () => {
      document.getElementById('telemetry-refresh-button')!.click();
    });

    await waitFor(() => expect(calls).toBeGreaterThan(callsAfterMount));
  });

  it('Dashboard remains unregressed with TelemetryPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [])));
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });
});
