/**
 * FE-11 targeted tests for the real Findings page — exercised through the
 * real `AppShell` (real `AuthProvider`, `ProjectProvider`, `AIQAProvider`,
 * real routing) inside a `MemoryRouter`, matching the pattern established
 * by `AuditPage.test.tsx`/`EvidencePage.test.tsx`. `fetch` is mocked only
 * at the transport boundary.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import jsPDF from 'jspdf';
import * as XLSX from 'xlsx';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import { buildBugReportRows, buildFinalAuditRows, buildTestReportRows } from './ReportsPage';
import type { DashboardResponse, Finding, FailureCluster, KpisResponse, Project, TestIntelRow } from '../../types/contract';

// ACC-016 — real `jsPDF` instances assign `save` as an own instance
// property inside their own constructor (confirmed by direct inspection of
// the installed `jspdf` package before writing this — it is not on
// `jsPDF.prototype`, so `vi.spyOn(jsPDF.prototype, 'save')` cannot see it).
// `vi.mock` intercepts the whole module instead: a real subclass still
// runs the real constructor (so `.text()`/`.setFontSize()`/`autoTable()`/
// `.output()` all stay 100% real), and only re-points the one
// browser-download-triggering method afterward — mirroring this file's
// `fetch`-mocked-only-at-the-transport-boundary rule, applied to the
// export boundary. Must live at module top level (not inside a
// `describe`/`beforeEach`) — Vitest hoists `vi.mock` above all imports.
const { capturedPdfDocs, pdfSaveCalls } = vi.hoisted(() => ({
  capturedPdfDocs: [] as unknown[],
  pdfSaveCalls: [] as unknown[][],
}));
vi.mock('jspdf', async (importOriginal) => {
  const actual = await importOriginal<typeof import('jspdf')>();
  class TestJsPDF extends actual.default {
    constructor(...args: ConstructorParameters<typeof actual.default>) {
      super(...args);
      capturedPdfDocs.push(this);
      (this as unknown as { save: (...a: unknown[]) => unknown }).save = (...saveArgs: unknown[]) => {
        pdfSaveCalls.push(saveArgs);
        return this;
      };
    }
  }
  return { ...actual, default: TestJsPDF };
});

// Same reasoning as the `jspdf` mock above, for a different underlying
// cause: `xlsx`'s synthetic ESM namespace defines `writeFile` as a
// non-configurable property (confirmed by `vi.spyOn(XLSX, 'writeFile')`
// throwing "Cannot redefine property: writeFile" when first tried), so it
// cannot be spied on directly. `vi.mock` sidesteps this by building the
// mocked module from scratch: every real export (`utils.book_new`,
// `.aoa_to_sheet`, `.book_append_sheet`, `.sheet_to_csv`, etc.) is spread
// through unchanged from the real module — only `writeFile`, the one
// browser-download-triggering call, is replaced.
const { xlsxWriteFileCalls } = vi.hoisted(() => ({
  xlsxWriteFileCalls: [] as unknown[][],
}));
vi.mock('xlsx', async (importOriginal) => {
  const actual = await importOriginal<typeof import('xlsx')>();
  return {
    ...actual,
    writeFile: (...args: unknown[]) => {
      xlsxWriteFileCalls.push(args);
    },
  };
});

function jsonResponse(status: number, body: unknown): Response {
  return new Response(status === 204 ? null : JSON.stringify(body), {
    status,
    headers: status === 204 ? {} : { 'Content-Type': 'application/json' },
  });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_findings_test' } };
}

const RAW_KEY = 'aiqa_findingstest0000000';
const PROJECT_A: Project = {
  id: 'proj-a',
  name: 'Project Alpha',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};
const PROJECT_B: Project = {
  id: 'proj-b',
  name: 'Project Beta',
  description: null,
  created_at: '2026-01-02T00:00:00Z',
  updated_at: '2026-01-02T00:00:00Z',
};

const FINDING_A: Finding = {
  id: 'finding-aaaa1111',
  failure_cluster_id: 'cluster-aaaa1111',
  title: 'Recurring failure in checkout task (3x)',
  description: 'Task has failed 3 times with a matching failure signature.',
  severity: 'High',
  confidence: 0.55,
  review_state: 'NEEDS_REVIEW',
  created_at: '2026-08-18T10:00:00Z',
  promoted_at: null,
  promoted_by: null,
};
const FINDING_B: Finding = {
  id: 'finding-bbbb2222',
  failure_cluster_id: 'cluster-bbbb2222',
  title: 'Recurring failure in auth task (2x)',
  description: 'Task has failed 2 times with a matching failure signature.',
  severity: 'Medium',
  confidence: 0.4,
  review_state: 'CONFIRMED',
  created_at: '2026-08-17T09:00:00Z',
  promoted_at: '2026-08-17T09:30:00Z',
  promoted_by: 'ops-key-alpha',
};
const CLUSTER_A: FailureCluster = {
  id: 'cluster-aaaa1111',
  signature: 'connection refused',
  first_seen_at: '2026-08-18T08:00:00Z',
  last_seen_at: '2026-08-18T10:00:00Z',
  occurrence_count: 3,
  root_cause_hypothesis: 'Target service was unreachable (connection refused)',
  confidence: 0.55,
  reproducibility: null,
  review_state: 'NEEDS_REVIEW',
};

function mockBackend(opts: {
  projects: Project[];
  findingsByProjectId?: Record<string, Finding[] | { status: number; body: unknown } | 'offline'>;
  analysisByFindingId?: Record<string, { finding: Finding; failure_cluster: FailureCluster } | { status: number; body: unknown }>;
  reviewByFindingId?: Record<string, { status: number; body?: unknown } | 'ok'>;
  testIntelByProjectId?: Record<string, TestIntelRow[] | { status: number; body: unknown } | 'offline'>;
}) {
  const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    if (url.includes('/review')) {
      const match = url.match(/findings\/([^/]+)\/review/);
      const findingId = match ? match[1] : '';
      const entry = opts.reviewByFindingId?.[findingId] ?? 'ok';
      if (entry === 'ok') return jsonResponse(204, null);
      return jsonResponse(entry.status, entry.body ?? {});
    }
    if (url.includes('/analysis')) {
      const match = url.match(/findings\/([^/]+)\/analysis/);
      const findingId = match ? match[1] : '';
      const entry = opts.analysisByFindingId?.[findingId];
      if (!entry) return jsonResponse(404, envelope('RESOURCE_NOT_FOUND', 'finding not found'));
      if ('status' in entry) return jsonResponse(entry.status, entry.body);
      return jsonResponse(200, entry);
    }
    if (url.includes('/findings')) {
      const match = url.match(/projects\/([^/]+)\/findings/);
      const projectId = match ? match[1] : '';
      const entry = opts.findingsByProjectId?.[projectId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && typeof entry === 'object' && !Array.isArray(entry) && 'status' in entry) {
        return jsonResponse(entry.status, entry.body);
      }
      return jsonResponse(200, entry ?? []);
    }
    if (url.includes('/intelligence/tests')) {
      const match = url.match(/projects\/([^/]+)\/intelligence\/tests/);
      const projectId = match ? match[1] : '';
      const entry = opts.testIntelByProjectId?.[projectId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && typeof entry === 'object' && !Array.isArray(entry) && 'status' in entry) {
        return jsonResponse(entry.status, entry.body);
      }
      return jsonResponse(200, entry ?? []);
    }
    if (url.includes('/dashboard')) {
      const m = url.match(/projects\/([^/]+)\/dashboard/);
      const pid = m ? m[1] : '';
      const proj = opts.projects.find((p) => p.id === pid);
      return jsonResponse(200, {
        project_id: pid,
        project_name: proj?.name ?? 'Unknown',
        execution_counts: {},
        task_counts: {},
        pass_fail: { passed: 0, failed: 0 },
        test_count: 0,
        generated_at: '2026-08-19T00:00:00Z',
      });
    }
    if (url.includes('/kpis')) {
      const m = url.match(/projects\/([^/]+)\/kpis/);
      const pid = m ? m[1] : '';
      return jsonResponse(200, { project_id: pid, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
    }
    if (url.includes('/executions')) return jsonResponse(200, []);
    return jsonResponse(200, opts.projects);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(initialEntries: string[] = ['/reports']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
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

async function openAndSelectProject(project: Project): Promise<void> {
  await waitFor(() => expect(document.getElementById('project-selector-button')).toBeTruthy());
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('ReportsPage (FE-11 real Findings)', () => {
  it('unauthenticated (no key) never fetches findings; shows the real auth-state card', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('aiqa-reports-page')).toBeNull();
  });

  it('no project selected shows a real, honest prompt rather than an empty findings list', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(screen.queryByText('No findings recorded')).toBeNull();
  });

  it('authenticated + a selected project fetches and renders real findings data', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [FINDING_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());
    expect(screen.getByText('High')).toBeTruthy();
    expect(screen.getByText('NEEDS REVIEW')).toBeTruthy();
    const findingsCalls = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/findings') && !String(url).includes('/analysis') && !String(url).includes('/review'));
    expect(String(findingsCalls[0][0])).toBe(`/api/v1/projects/${PROJECT_A.id}/findings`);
  });

  it('switching project reloads findings and never shows the previous project\'s stale rows', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      findingsByProjectId: { [PROJECT_A.id]: [FINDING_A], [PROJECT_B.id]: [FINDING_B] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await openAndSelectProject(PROJECT_B);

    await waitFor(() => expect(screen.getByText(FINDING_B.title)).toBeTruthy());
    expect(screen.queryByText(FINDING_A.title)).toBeNull();
  });

  it('a real, successful, zero-row response renders the honest empty state, not an error', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('No findings recorded')).toBeTruthy());
  });

  it('a real 401 on the findings endpoint renders the unauthorized state, not empty data', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: { status: 401, body: envelope('UNAUTHENTICATED', 'revoked') } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Authentication required')).toBeTruthy());
    expect(screen.queryByText('No findings recorded')).toBeNull();
  });

  it('a real 403 on the findings endpoint renders the forbidden state, never an empty findings list', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: { status: 403, body: envelope('FORBIDDEN', "role 'viewer' cannot perform this action") } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Not permitted to view these findings')).toBeTruthy());
  });

  it('offline (network failure) on the findings endpoint renders the unreachable state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: 'offline' } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
  });

  it('selecting a finding fetches its real root-cause analysis via POST', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [FINDING_A] },
      analysisByFindingId: { [FINDING_A.id]: { finding: FINDING_A, failure_cluster: CLUSTER_A } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await act(async () => {
      screen.getByText(FINDING_A.title).click();
    });

    await waitFor(() => expect(screen.getAllByText(/connection refused/).length).toBeGreaterThan(0));
    const analysisCalls = fetchMock.mock.calls.filter(([url, init]: [string, RequestInit]) => String(url).includes('/analysis'));
    expect(analysisCalls.length).toBe(1);
    expect(String(analysisCalls[0][0])).toBe(`/api/v1/projects/${PROJECT_A.id}/findings/${FINDING_A.id}/analysis`);
    expect(analysisCalls[0][1]?.method).toBe('POST');
  });

  it('confirming a finding performs a real POST review request and refetches the real list', async () => {
    setApiKey(RAW_KEY);
    const confirmedFinding: Finding = { ...FINDING_A, review_state: 'CONFIRMED', promoted_by: 'ops-key-alpha' };
    let reviewed = false;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (String(url).includes('/review')) {
        reviewed = true;
        return jsonResponse(204, null);
      }
      if (String(url).includes('/analysis')) {
        return jsonResponse(200, { finding: reviewed ? confirmedFinding : FINDING_A, failure_cluster: CLUSTER_A });
      }
      if (String(url).includes('/findings')) {
        return jsonResponse(200, reviewed ? [confirmedFinding] : [FINDING_A]);
      }
      if (String(url).includes('/dashboard')) return jsonResponse(200, { project_id: PROJECT_A.id, project_name: PROJECT_A.name, execution_counts: {}, task_counts: {}, pass_fail: { passed: 0, failed: 0 }, test_count: 0, generated_at: '2026-08-19T00:00:00Z' });
      if (String(url).includes('/kpis')) return jsonResponse(200, { project_id: PROJECT_A.id, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await act(async () => {
      screen.getByText(FINDING_A.title).click();
    });
    await waitFor(() => expect(screen.getByText('Confirm as Defect')).toBeTruthy());

    await act(async () => {
      screen.getByText('Confirm as Defect').click();
    });

    await waitFor(() => expect(screen.getByText(/confirmed as a real defect/)).toBeTruthy());
    const reviewCalls = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/review'));
    expect(reviewCalls.length).toBe(1);
    expect(reviewCalls[0][1]?.method).toBe('POST');
    expect(JSON.parse(reviewCalls[0][1]?.body as string)).toEqual({ action: 'confirm' });
  });

  it('a real 403 on review renders a real forbidden message, not a fabricated success', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [FINDING_A] },
      reviewByFindingId: { [FINDING_A.id]: { status: 403, body: envelope('FORBIDDEN', "role 'viewer' cannot perform this action") } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await act(async () => {
      screen.getByText(FINDING_A.title).click();
    });
    await waitFor(() => expect(screen.getByText('Confirm as Defect')).toBeTruthy());

    await act(async () => {
      screen.getByText('Confirm as Defect').click();
    });

    await waitFor(() =>
      expect(screen.getByText(/does not permit reviewing findings/)).toBeTruthy(),
    );
    // The finding must NOT be shown as reviewed — the real request failed.
    expect(screen.getByText('Confirm as Defect')).toBeTruthy();
    void fetchMock;
  });

  it('only real, backend-returned findings render; no fabricated demo data appears', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [FINDING_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());
    // Known previous-version fake strings must never appear.
    expect(screen.queryByText('DEF-881')).toBeNull();
    expect(screen.queryByText('DEF-882')).toBeNull();
    expect(screen.queryByText(/Auth Token Refresh accepts expired payload/)).toBeNull();
    expect(screen.queryByText(/Evidence Coverage/)).toBeNull();
  });

  it('Dashboard remains unregressed with ReportsPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/dashboard']);
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getAllByText(PROJECT_A.name).length).toBeGreaterThan(0));
    expect(screen.getByText('Executed Tests')).toBeTruthy();
  });

  it('Test Generator remains unregressed with ReportsPage present in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/test-generator']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
  });
});

describe('ReportsPage — FE-19 real Finding.promoted_at review-attribution exposure', () => {
  it('a finding already reviewed by the real backend shows the real promoted_at timestamp, not just promoted_by', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_B], findingsByProjectId: { [PROJECT_B.id]: [FINDING_B] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_B);

    await waitFor(() => expect(screen.getByText(FINDING_B.title)).toBeTruthy());
    await act(async () => {
      screen.getByText(FINDING_B.title).click();
    });
    await waitFor(() => expect(document.getElementById(`finding-review-attribution-${FINDING_B.id}`)).toBeTruthy());
    const text = document.getElementById(`finding-review-attribution-${FINDING_B.id}`)!.textContent;
    expect(text).toContain('confirmed as a real defect');
    expect(text).toContain(FINDING_B.promoted_by!);
    expect(text).toContain('at');
    // formatTimestamp renders a locale date/time string derived from promoted_at,
    // not the raw ISO string — assert the real year is present rather than an exact format.
    expect(text).toContain('2026');
  });

  it('a not-yet-reviewed finding shows no review-attribution text and no fabricated timestamp', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [FINDING_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());
    await act(async () => {
      screen.getByText(FINDING_A.title).click();
    });
    await waitFor(() => expect(screen.getByText('Confirm as Defect')).toBeTruthy());
    expect(document.getElementById(`finding-review-attribution-${FINDING_A.id}`)).toBeNull();
    expect(screen.queryByText(/Reviewed —/)).toBeNull();
  });

  it('confirming a finding whose real backend response includes promoted_at renders the real timestamp after the real POST review', async () => {
    setApiKey(RAW_KEY);
    const confirmedWithTimestamp: Finding = {
      ...FINDING_A,
      review_state: 'CONFIRMED',
      promoted_by: 'ops-key-alpha',
      promoted_at: '2026-08-19T12:00:00Z',
    };
    let reviewed = false;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (String(url).includes('/review')) {
        reviewed = true;
        return jsonResponse(204, null);
      }
      if (String(url).includes('/analysis')) {
        return jsonResponse(200, { finding: reviewed ? confirmedWithTimestamp : FINDING_A, failure_cluster: CLUSTER_A });
      }
      if (String(url).includes('/findings')) {
        return jsonResponse(200, reviewed ? [confirmedWithTimestamp] : [FINDING_A]);
      }
      if (String(url).includes('/dashboard')) return jsonResponse(200, { project_id: PROJECT_A.id, project_name: PROJECT_A.name, execution_counts: {}, task_counts: {}, pass_fail: { passed: 0, failed: 0 }, test_count: 0, generated_at: '2026-08-19T00:00:00Z' });
      if (String(url).includes('/kpis')) return jsonResponse(200, { project_id: PROJECT_A.id, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await act(async () => {
      screen.getByText(FINDING_A.title).click();
    });
    await waitFor(() => expect(screen.getByText('Confirm as Defect')).toBeTruthy());

    await act(async () => {
      screen.getByText('Confirm as Defect').click();
    });

    await waitFor(() => expect(document.getElementById(`finding-review-attribution-${FINDING_A.id}`)).toBeTruthy());
    const text = document.getElementById(`finding-review-attribution-${FINDING_A.id}`)!.textContent;
    expect(text).toContain('confirmed as a real defect');
    expect(text).toContain('ops-key-alpha');
    expect(text).toContain('2026');
  });
});

describe('ReportsPage — FE-22 real Test Intelligence exposure', () => {
  const INTEL_ROW_A: TestIntelRow = {
    id: 'node-aaaa1111',
    name: 'checkout_flow_test',
    risk_score: 0.82,
    failure_rate: 0.6,
    dependency_count: 3,
    execution_cost_ms: 4500,
    value_score: 0.71,
    flakiness_score: 0.25,
    window_runs: 10,
    window_failures: 6,
  };
  const INTEL_ROW_B: TestIntelRow = {
    id: 'node-cccc3333',
    name: 'auth_login_test',
    risk_score: 0.1,
    failure_rate: 0.0,
    dependency_count: 1,
    execution_cost_ms: 250,
    value_score: 0.4,
    flakiness_score: 0.0,
    window_runs: 5,
    window_failures: 0,
  };
  const INTEL_ROW_NULLS: TestIntelRow = {
    id: 'node-dddd4444',
    name: 'brand_new_test',
    risk_score: null,
    failure_rate: null,
    dependency_count: null,
    execution_cost_ms: null,
    value_score: null,
    flakiness_score: null,
    window_runs: null,
    window_failures: null,
  };

  it('item 1 — calls the real BE-10 project-scoped endpoint with the selected project id, never the legacy unscoped route', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [] }, testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(INTEL_ROW_A.name)).toBeTruthy());
    const intelCalls = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests'));
    expect(intelCalls.length).toBeGreaterThan(0);
    expect(String(intelCalls[0][0])).toBe(`/api/v1/projects/${PROJECT_A.id}/intelligence/tests`);
    // Never the legacy, unscoped route BE-10's report explicitly says this
    // app must not fall back to.
    expect(fetchMock.mock.calls.some(([url]: [string]) => String(url) === '/intelligence/tests')).toBe(false);
  });

  it('item 2 — a real TestIntelRow renders every dimension: name, risk, failure rate, value, flakiness, execution cost, dependencies, window', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [] }, testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_A] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(INTEL_ROW_A.name)).toBeTruthy());
    const row = screen.getByText(INTEL_ROW_A.name).closest('tr')!;
    const cells = within(row).getAllByRole('cell').map((c) => c.textContent);
    expect(cells).toEqual([
      'checkout_flow_test',
      '82.0%', // risk_score
      '60.0%', // failure_rate
      '71.0%', // value_score
      '25.0%', // flakiness_score
      '4.5s',  // execution_cost_ms
      '3',     // dependency_count
      '10 runs, 6 failed', // window_runs / window_failures
    ]);
  });

  it('item 3 — a real, successful, zero-row response renders the honest empty state, not sample rows', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [] }, testIntelByProjectId: { [PROJECT_A.id]: [] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('No test intelligence available for this project yet')).toBeTruthy());
    expect(screen.queryByText(INTEL_ROW_A.name)).toBeNull();
  });

  it('item 4 — a real 403 on the intelligence endpoint renders the existing forbidden state, not fake intelligence', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [] },
      testIntelByProjectId: { [PROJECT_A.id]: { status: 403, body: envelope('FORBIDDEN', "role 'viewer' cannot perform this action") } },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText('Not permitted to view test intelligence')).toBeTruthy());
    expect(screen.queryByText(INTEL_ROW_A.name)).toBeNull();
  });

  it('item 5 — switching project reloads test intelligence and never shows the previous project\'s stale rows', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      findingsByProjectId: { [PROJECT_A.id]: [], [PROJECT_B.id]: [] },
      testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_A], [PROJECT_B.id]: [INTEL_ROW_B] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(INTEL_ROW_A.name)).toBeTruthy());

    await openAndSelectProject(PROJECT_B);

    await waitFor(() => expect(screen.getByText(INTEL_ROW_B.name)).toBeTruthy());
    expect(screen.queryByText(INTEL_ROW_A.name)).toBeNull();

    const intelCalls = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).map(([url]: [string]) => String(url));
    expect(intelCalls).toContain(`/api/v1/projects/${PROJECT_A.id}/intelligence/tests`);
    expect(intelCalls).toContain(`/api/v1/projects/${PROJECT_B.id}/intelligence/tests`);
  });

  it('item 6 — null fields render the honest "—" placeholder, never a fabricated 0%/0ms/NaN, and never crash', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], findingsByProjectId: { [PROJECT_A.id]: [] }, testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_NULLS] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(screen.getByText(INTEL_ROW_NULLS.name)).toBeTruthy());
    const row = screen.getByText(INTEL_ROW_NULLS.name).closest('tr')!;
    const cells = within(row).getAllByRole('cell').map((c) => c.textContent);
    expect(cells).toEqual(['brand_new_test', '—', '—', '—', '—', '—', '—', '—']);
    expect(row.textContent).not.toMatch(/NaN|0%|0ms/);
  });
});

/**
 * FE-25 — real Test Intelligence realtime auto-refresh. Same real-`AppShell`
 * / `MockWebSocket`-for-`RealtimeContext` pattern `RealtimeContext.test.tsx`/
 * `Header.test.tsx` already established, introduced into this file for the
 * first time (Findings' own realtime resync, live since FE-9, had never
 * been directly exercised by a WS-driven test in this file before FE-25 —
 * closing that pre-existing gap is part of item 10 below). Scoped to its
 * own nested `beforeEach`/`afterEach` so the `WebSocket` stub never affects
 * the non-realtime tests above.
 */
describe('ReportsPage — FE-25 Test Intelligence realtime auto-refresh', () => {
  class MockWebSocket {
    static instances: MockWebSocket[] = [];
    url: string;
    readyState = 0;
    onopen: ((ev: unknown) => void) | null = null;
    onmessage: ((ev: { data: string }) => void) | null = null;
    onerror: ((ev: unknown) => void) | null = null;
    onclose: ((ev: { code: number; reason: string }) => void) | null = null;
    constructor(url: string) {
      this.url = url;
      MockWebSocket.instances.push(this);
    }
    close() {
      if (this.readyState === 3) return;
      this.readyState = 3;
      this.onclose?.({ code: 1000, reason: '' });
    }
    simulateOpen() {
      this.readyState = 1;
      this.onopen?.({});
    }
    simulateMessage(data: unknown) {
      this.onmessage?.({ data: typeof data === 'string' ? data : JSON.stringify(data) });
    }
  }

  function realEvent(overrides: Partial<import('../../types/contract').RunEvent> = {}) {
    return {
      event_id: `evt-${Math.random().toString(36).slice(2)}`,
      run_id: 'run-a',
      task_id: null,
      sequence: 1,
      timestamp: '2026-08-20T10:00:00Z',
      schema_version: 1,
      type: 'RUN_COMPLETED' as const,
      status: null,
      summary: 'run finished',
      project_id: PROJECT_A.id,
      project_sequence: 1,
      ...overrides,
    };
  }

  const INTEL_ROW_REALTIME: TestIntelRow = {
    id: 'node-rt1',
    name: 'realtime_seed_test',
    risk_score: 0.2,
    failure_rate: 0.1,
    dependency_count: 0,
    execution_cost_ms: 100,
    value_score: 0.3,
    flakiness_score: 0.0,
    window_runs: 1,
    window_failures: 0,
  };

  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
  });

  async function connectRealtime(): Promise<InstanceType<typeof MockWebSocket>> {
    await waitFor(() => expect(MockWebSocket.instances.length).toBeGreaterThan(0));
    const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    act(() => ws.simulateOpen());
    return ws;
  }

  it('item 2 — a real RUN_COMPLETED event automatically refetches Test Intelligence, no Refresh click needed', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [] },
      testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_REALTIME] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(INTEL_ROW_REALTIME.name)).toBeTruthy());
    const ws = await connectRealtime();

    const intelCallsBefore = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
    act(() => ws.simulateMessage(realEvent({ type: 'RUN_COMPLETED' })));

    await waitFor(() => {
      const after = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
      expect(after).toBe(intelCallsBefore + 1);
    });
  });

  it('item 3a — a non-terminal event (TASK_STARTED) never triggers a Test Intelligence refetch', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [] },
      testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_REALTIME] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(INTEL_ROW_REALTIME.name)).toBeTruthy());
    const ws = await connectRealtime();

    const intelCallsBefore = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
    act(() => ws.simulateMessage(realEvent({ type: 'TASK_STARTED', summary: null })));

    // Give the 500ms debounce window plenty of time to have fired if it
    // were (incorrectly) going to — `TestIntelRow` fields are only ever
    // recomputed by the real backend at run completion, so a mid-run event
    // by itself can never mean fresh intelligence data exists yet.
    await new Promise((resolve) => setTimeout(resolve, 700));
    const intelCallsAfter = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
    expect(intelCallsAfter).toBe(intelCallsBefore);
  });

  it('item 3b — three RUN_COMPLETED events in a burst produce exactly one debounced refetch, not three', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [] },
      testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_REALTIME] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(INTEL_ROW_REALTIME.name)).toBeTruthy());
    const ws = await connectRealtime();

    const intelCallsBefore = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
    act(() => ws.simulateMessage(realEvent({ event_id: 'evt-burst-1' })));
    act(() => ws.simulateMessage(realEvent({ event_id: 'evt-burst-2' })));
    act(() => ws.simulateMessage(realEvent({ event_id: 'evt-burst-3' })));

    await new Promise((resolve) => setTimeout(resolve, 700));
    const intelCallsAfter = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
    expect(intelCallsAfter).toBe(intelCallsBefore + 1);
  });

  it('item 5 — a late Project A response can never overwrite Project B\'s already-displayed intelligence', async () => {
    setApiKey(RAW_KEY);
    let resolveA: ((rows: TestIntelRow[]) => void) | null = null;
    const rowA: TestIntelRow = { ...INTEL_ROW_REALTIME, id: 'node-a-late', name: 'project_a_late_test' };
    const rowB: TestIntelRow = { ...INTEL_ROW_REALTIME, id: 'node-b-fast', name: 'project_b_fast_test' };
    let aCallCount = 0;
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      if (String(url).includes(`/projects/${PROJECT_A.id}/intelligence/tests`)) {
        aCallCount += 1;
        if (aCallCount === 1) {
          // Real, immediate initial load for A.
          return jsonResponse(200, [rowA]);
        }
        // The resync-triggered second call for A never resolves until the
        // test explicitly lets it — simulating a real slow/delayed response
        // arriving *after* the user has already moved on to Project B.
        return new Promise<Response>((resolve) => {
          resolveA = (rows) => resolve(jsonResponse(200, rows));
        });
      }
      if (String(url).includes(`/projects/${PROJECT_B.id}/intelligence/tests`)) {
        return jsonResponse(200, [rowB]);
      }
      if (String(url).includes('/findings')) return jsonResponse(200, []);
      if (String(url).includes('/executions')) return jsonResponse(200, []);
      if (String(url).includes('/dashboard')) return jsonResponse(200, { project_id: '', project_name: '', execution_counts: {}, task_counts: {}, pass_fail: { passed: 0, failed: 0 }, test_count: 0, generated_at: '2026-08-20T00:00:00Z' });
      if (String(url).includes('/kpis')) return jsonResponse(200, { project_id: '', execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-20T00:00:00Z' });
      return jsonResponse(200, [PROJECT_A, PROJECT_B]);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(rowA.name)).toBeTruthy());
    const ws = await connectRealtime();

    // A real RUN_COMPLETED for A starts a second, slow-resolving fetch.
    act(() => ws.simulateMessage(realEvent({ type: 'RUN_COMPLETED', project_id: PROJECT_A.id })));
    await waitFor(() => expect(aCallCount).toBe(2));

    // Before A's slow response arrives, the user switches to Project B —
    // this aborts A's in-flight request and starts B's own (fast) one.
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(screen.getByText(rowB.name)).toBeTruthy());

    // Now A's stale response finally resolves.
    expect(resolveA).toBeTruthy();
    act(() => resolveA!([rowA]));
    await new Promise((resolve) => setTimeout(resolve, 50));

    // B's real data must still be the only thing shown — A's late data
    // must never have overwritten it.
    expect(screen.getByText(rowB.name)).toBeTruthy();
    expect(screen.queryByText(rowA.name)).toBeNull();
  });

  it('item 6 — manual Refresh still triggers a real Test Intelligence refetch, independent of realtime', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [] },
      testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_REALTIME] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(INTEL_ROW_REALTIME.name)).toBeTruthy());

    const intelCallsBefore = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
    await act(async () => {
      screen.getByText('Refresh').closest('button')!.click();
    });

    await waitFor(() => {
      const after = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
      expect(after).toBe(intelCallsBefore + 1);
    });
  });

  it('item 10 — Findings\' own any-event resync remains unchanged, unaffected by Test Intelligence\'s RUN_COMPLETED-only filter', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      findingsByProjectId: { [PROJECT_A.id]: [] },
      testIntelByProjectId: { [PROJECT_A.id]: [INTEL_ROW_REALTIME] },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(INTEL_ROW_REALTIME.name)).toBeTruthy());
    const ws = await connectRealtime();

    const findingsCallsBefore = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/findings') && !String(url).includes('/analysis') && !String(url).includes('/review')).length;
    const intelCallsBefore = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;

    // A real non-terminal event: Findings (unchanged FE-9 behavior) still
    // resyncs on *any* event; Test Intelligence (FE-25's own, tighter
    // filter) correctly does not.
    act(() => ws.simulateMessage(realEvent({ type: 'TASK_FAILED', summary: 'a real task failure' })));

    await waitFor(() => {
      const after = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/findings') && !String(url).includes('/analysis') && !String(url).includes('/review')).length;
      expect(after).toBe(findingsCallsBefore + 1);
    });
    const intelCallsAfter = fetchMock.mock.calls.filter(([url]: [string]) => String(url).includes('/intelligence/tests')).length;
    expect(intelCallsAfter).toBe(intelCallsBefore);
  });
});

/**
 * Build order item 3 (22 Aug 2026) — ACC-016 PDF/Excel export. See
 * `ReportsPage.tsx`'s module doc comment for why "Bug Report" / "Test
 * Report" / "Final Audit Report" resolve to `Finding[]` /
 * `DashboardResponse`+`KpisResponse` / a computed reconciliation of the
 * two, and why no fourth ("module") data source was invented.
 *
 * The pure row-builders are unit-tested directly first (no rendering, no
 * library calls — same convention as `TelemetryPage.test.tsx`'s direct
 * `parsePrometheusText` tests), then a smaller set of component tests
 * verify the real generated PDF/Excel bytes actually contain real data.
 * `jsPDF`/`xlsx` are both real, unmodified libraries up to the one call
 * each would use to trigger a real browser download (`save`/`writeFile`)
 * — only that boundary is intercepted (mirroring this codebase's "mock
 * only at the transport boundary" rule, applied here to the export
 * boundary instead of `fetch`; see the two `vi.mock` calls above), so
 * everything upstream (real fetched data → real row-building → real
 * jsPDF/xlsx document construction) runs for real.
 *
 * jsPDF's default (uncompressed) content stream means the real text placed
 * on the page is genuinely present in `doc.output()` — confirmed directly
 * against the installed `jspdf` package before writing these assertions —
 * but `autoTable` legitimately word-wraps long cell content across
 * multiple lines/draw calls (real, correct rendering, not a bug), which
 * splits one sentence into several separate `Tj`/`TJ` operators. A plain
 * `doc.output().includes(longSentence)` check would then wrongly fail on
 * wrapped content even though every word is genuinely on the page —
 * empirically confirmed against a real generated table before writing
 * `extractPdfText` below, not assumed. `extractPdfText` reconstructs the
 * visible text by joining every text-showing operator's string operand in
 * document order, so assertions check what a reader would actually see,
 * independent of exactly where `autoTable` happened to wrap a line.
 */
function extractPdfText(raw: string): string {
  const runs: string[] = [];
  const unescape = (s: string) => s.replace(/\\([()\\])/g, '$1').replace(/\\n/g, ' ').replace(/\\r/g, '');
  // Single show-text operator: (text) Tj
  for (const m of raw.matchAll(/\(((?:\\.|[^\\()])*)\)\s*Tj/g)) runs.push(unescape(m[1]));
  // Kerned/spaced show-text operator: [ (text) num (text) ... ] TJ
  for (const m of raw.matchAll(/\[((?:\\.|[^\\[\]])*)\]\s*TJ/g)) {
    for (const inner of m[1].matchAll(/\(((?:\\.|[^\\()])*)\)/g)) runs.push(unescape(inner[1]));
  }
  return runs.join(' ');
}
describe('ReportsPage — ACC-016 report-builder pure functions (unit)', () => {
  it('buildBugReportRows maps every real Finding field for an unreviewed and a reviewed finding, inventing none', () => {
    const rows = buildBugReportRows([FINDING_A, FINDING_B]);
    expect(rows).toHaveLength(2);
    const [rowA, rowB] = rows;

    expect(rowA[0]).toBe(FINDING_A.id);
    expect(rowA[1]).toBe(FINDING_A.title);
    expect(rowA[2]).toBe(FINDING_A.severity);
    expect(rowA[3]).toBe(FINDING_A.review_state);
    expect(rowA[4]).toBe('55.0%');
    expect(String(rowA[5])).toContain('2026'); // Created At — locale-formatted, real year
    // FINDING_A was never promoted — honest '—', not a fabricated date/actor.
    expect(rowA[6]).toBe('—');
    expect(rowA[7]).toBe('—');

    expect(rowB[0]).toBe(FINDING_B.id);
    expect(rowB[3]).toBe('CONFIRMED');
    expect(rowB[4]).toBe('40.0%');
    // FINDING_B has a real promoted_at/promoted_by — both must appear, not '—'.
    expect(String(rowB[6])).toContain('2026');
    expect(rowB[7]).toBe(FINDING_B.promoted_by);
  });

  it('buildBugReportRows on a real, empty findings list produces zero rows, not a placeholder row', () => {
    expect(buildBugReportRows([])).toEqual([]);
  });

  it('buildTestReportRows maps real DashboardResponse/KpisResponse fields, formatting a genuinely null KPI honestly', () => {
    const dashboard: DashboardResponse = {
      project_id: 'proj-a',
      project_name: 'Project Alpha',
      execution_counts: { COMPLETED: 4, FAILED: 1 },
      task_counts: { PASSED: 20, FAILED: 3 },
      pass_fail: { passed: 20, failed: 3 },
      test_count: 23,
      generated_at: '2026-08-20T00:00:00Z',
    };
    const kpis: KpisResponse = {
      project_id: 'proj-a',
      execution_reliability: 0.87,
      execution_efficiency_ms: 1500,
      execution_avg_run_duration_ms: null,
      generated_at: '2026-08-20T00:00:00Z',
    };
    const rows = buildTestReportRows(dashboard, kpis);
    expect(rows).toContainEqual(['Executed Tests', 23]);
    expect(rows).toContainEqual(['Passed', 20]);
    expect(rows).toContainEqual(['Failed', 3]);
    expect(rows).toContainEqual(['Execution Reliability', '87.0%']);
    expect(rows).toContainEqual(['Avg. Task Duration', '1.5s']);
    // execution_avg_run_duration_ms is genuinely null — never a fabricated 0/0ms.
    expect(rows).toContainEqual(['Avg. Run Duration', '—']);
  });

  it('buildTestReportRows on a null dashboard (fetch not yet resolved) produces zero rows, not a fabricated zero-count table', () => {
    expect(buildTestReportRows(null, null)).toEqual([]);
  });

  it('buildFinalAuditRows reconciles real totals and states the project-level scope honestly', () => {
    const dashboard: DashboardResponse = {
      project_id: 'proj-a',
      project_name: 'Project Alpha',
      execution_counts: { COMPLETED: 4, FAILED: 1 },
      task_counts: { PASSED: 20, FAILED: 3 },
      pass_fail: { passed: 20, failed: 3 },
      test_count: 23,
      generated_at: '2026-08-20T00:00:00Z',
    };
    const rows = buildFinalAuditRows([FINDING_A, FINDING_B], dashboard, 'Project Alpha');
    expect(rows).toContainEqual(['Total Findings (Bug Report)', 2]);
    expect(rows).toContainEqual(['Total Failed Tasks (Test Report)', 3]);
    expect(rows).toContainEqual(['Total Runs (Test Report)', 5]);
    expect(rows).toContainEqual(['Total Tasks (Test Report)', 23]);
    expect(
      rows.some(
        (r) => String(r[0]) === 'Report Scope' && String(r[1]).includes('Project Alpha') && String(r[1]).includes('no "module" entity'),
      ),
    ).toBe(true);
    expect(rows.some((r) => String(r[1]).includes('2 findings recorded against 3 failed tasks'))).toBe(true);
  });

  it('buildFinalAuditRows on null findings/dashboard reconciles to real zeros, not fabricated placeholders', () => {
    const rows = buildFinalAuditRows(null, null, 'Empty Project');
    expect(rows).toContainEqual(['Total Findings (Bug Report)', 0]);
    expect(rows).toContainEqual(['Total Failed Tasks (Test Report)', 0]);
    expect(rows).toContainEqual(['Total Runs (Test Report)', 0]);
    expect(rows).toContainEqual(['Total Tasks (Test Report)', 0]);
  });
});

describe('ReportsPage — ACC-016 real PDF/Excel report export', () => {
  const DASHBOARD_NONEMPTY = {
    project_id: PROJECT_A.id,
    project_name: PROJECT_A.name,
    execution_counts: { COMPLETED: 4, FAILED: 1 },
    task_counts: { PASSED: 20, FAILED: 3 },
    pass_fail: { passed: 20, failed: 3 },
    test_count: 23,
    generated_at: '2026-08-21T00:00:00Z',
  };
  const KPIS_NONEMPTY = {
    project_id: PROJECT_A.id,
    execution_reliability: 0.87,
    execution_efficiency_ms: 1500,
    execution_avg_run_duration_ms: 42000,
    generated_at: '2026-08-21T00:00:00Z',
  };
  const DASHBOARD_EMPTY = {
    project_id: PROJECT_A.id,
    project_name: PROJECT_A.name,
    execution_counts: {},
    task_counts: {},
    pass_fail: { passed: 0, failed: 0 },
    test_count: 0,
    generated_at: '2026-08-21T00:00:00Z',
  };
  const KPIS_EMPTY = {
    project_id: PROJECT_A.id,
    execution_reliability: null,
    execution_efficiency_ms: null,
    execution_avg_run_duration_ms: null,
    generated_at: '2026-08-21T00:00:00Z',
  };

  function mockExportBackend(opts: {
    findings: Finding[] | { status: number; body: unknown };
    dashboard?: unknown | { status: number; body: unknown };
    kpis?: unknown | { status: number; body: unknown };
  }) {
    const dashboard = opts.dashboard ?? DASHBOARD_NONEMPTY;
    const kpis = opts.kpis ?? KPIS_NONEMPTY;
    const fetchMock = vi.fn().mockImplementation(async (url: string) => {
      const u = String(url);
      if (u.includes('/findings') && !u.includes('/analysis') && !u.includes('/review')) {
        if (Array.isArray(opts.findings)) return jsonResponse(200, opts.findings);
        return jsonResponse(opts.findings.status, opts.findings.body);
      }
      if (u.includes('/intelligence/tests')) return jsonResponse(200, []);
      if (u.includes('/dashboard')) {
        if (dashboard && typeof dashboard === 'object' && 'status' in (dashboard as Record<string, unknown>)) {
          const d = dashboard as { status: number; body: unknown };
          return jsonResponse(d.status, d.body);
        }
        return jsonResponse(200, dashboard);
      }
      if (u.includes('/kpis')) {
        if (kpis && typeof kpis === 'object' && 'status' in (kpis as Record<string, unknown>)) {
          const k = kpis as { status: number; body: unknown };
          return jsonResponse(k.status, k.body);
        }
        return jsonResponse(200, kpis);
      }
      if (u.includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    vi.stubGlobal('fetch', fetchMock);
    return fetchMock;
  }

  beforeEach(() => {
    // `jspdf` and `xlsx` are both mocked at module top level (see this
    // file's own top-of-file doc comments) — only reset the capture
    // arrays here.
    capturedPdfDocs.length = 0;
    pdfSaveCalls.length = 0;
    xlsxWriteFileCalls.length = 0;
  });

  it('Export PDF and Export Excel stay disabled until a project is selected', async () => {
    setApiKey(RAW_KEY);
    mockExportBackend({ findings: [] });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('Export PDF').closest('button')).toBeTruthy());
    expect((screen.getByText('Export PDF').closest('button') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText('Export Excel').closest('button') as HTMLButtonElement).disabled).toBe(true);
  });

  it('Export PDF generates a real PDF containing a real finding title, real execution counts, and the real reconciliation', async () => {
    setApiKey(RAW_KEY);
    mockExportBackend({ findings: [FINDING_A, FINDING_B] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await act(async () => {
      screen.getByText('Export PDF').closest('button')!.click();
    });

    await waitFor(() => expect(pdfSaveCalls).toHaveLength(1));
    expect(pdfSaveCalls[0][0]).toBe(`aiqa_report_${PROJECT_A.id}.pdf`);
    expect(capturedPdfDocs).toHaveLength(1);
    const raw = extractPdfText((capturedPdfDocs[0] as InstanceType<typeof jsPDF>).output());
    // Real Bug Report content — an actual finding's real title.
    expect(raw).toContain(FINDING_A.title);
    // Real Test Report content — the real test_count from the mock backend.
    expect(raw).toContain('23');
    // Real Final Audit Report reconciliation, not a fabricated statement.
    expect(raw).toContain('2 findings recorded against 3 failed tasks');
  });

  it('Export Excel generates a real workbook — Bug Report / Test Report / Final Audit Report sheets, each with real data', async () => {
    setApiKey(RAW_KEY);
    mockExportBackend({ findings: [FINDING_A, FINDING_B] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await act(async () => {
      screen.getByText('Export Excel').closest('button')!.click();
    });

    await waitFor(() => expect(xlsxWriteFileCalls).toHaveLength(1));
    expect(xlsxWriteFileCalls[0][1]).toBe(`aiqa_report_${PROJECT_A.id}.xlsx`);
    const wb = xlsxWriteFileCalls[0][0] as XLSX.WorkBook;
    expect(wb.SheetNames).toEqual(['Bug Report', 'Test Report', 'Final Audit Report']);

    const bugCsv = XLSX.utils.sheet_to_csv(wb.Sheets['Bug Report']);
    expect(bugCsv).toContain(FINDING_A.title);
    expect(bugCsv).toContain(FINDING_B.id);

    const testCsv = XLSX.utils.sheet_to_csv(wb.Sheets['Test Report']);
    expect(testCsv).toContain('23');
    expect(testCsv).toContain('Failed');

    const auditCsv = XLSX.utils.sheet_to_csv(wb.Sheets['Final Audit Report']);
    expect(auditCsv).toContain('Report Scope');
    expect(auditCsv).toContain(PROJECT_A.name);
  });

  it('an empty project (zero findings, zero execution data) produces an honest report in both formats — no fabricated placeholder rows', async () => {
    setApiKey(RAW_KEY);
    mockExportBackend({ findings: [], dashboard: DASHBOARD_EMPTY, kpis: KPIS_EMPTY });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No findings recorded')).toBeTruthy());

    await act(async () => {
      screen.getByText('Export PDF').closest('button')!.click();
    });
    await waitFor(() => expect(pdfSaveCalls).toHaveLength(1));
    const raw = extractPdfText((capturedPdfDocs[capturedPdfDocs.length - 1] as InstanceType<typeof jsPDF>).output());
    // Bug Report: zero findings is a real empty list — the honest
    // "no findings" message, never a fabricated placeholder row.
    expect(raw).toContain('No findings recorded for this project.');
    // Test Report: DASHBOARD_EMPTY is a real, *successful* response whose
    // counts are genuinely zero — this is a different case from "no fetch
    // result" (covered by buildTestReportRows's own null-dashboard unit
    // test above) and correctly renders real zero rows, not a "no data"
    // placeholder — asserting the real zero values, not a fabricated
    // non-zero number, is what actually proves this case is handled
    // honestly.
    expect(raw).toContain('Executed Tests 0');
    expect(raw).toContain('Passed 0');
    expect(raw).toContain('Failed 0');
    // Real zero reconciliation, not a fabricated non-zero placeholder.
    expect(raw).toContain('0 findings recorded against 0 failed tasks');
    // Known previous-version fake strings must never appear here either.
    expect(raw).not.toContain('DEF-881');

    await act(async () => {
      screen.getByText('Export Excel').closest('button')!.click();
    });
    await waitFor(() => expect(xlsxWriteFileCalls).toHaveLength(1));
    const wb = xlsxWriteFileCalls[0][0] as XLSX.WorkBook;
    expect(XLSX.utils.sheet_to_csv(wb.Sheets['Bug Report'])).toContain('No findings recorded for this project.');
    const testCsv = XLSX.utils.sheet_to_csv(wb.Sheets['Test Report']);
    expect(testCsv).toContain('Executed Tests,0');
    expect(testCsv).toContain('Passed,0');
  });

  it('a real failure fetching Test Report data at export time shows a real error, never a partial or fabricated report', async () => {
    setApiKey(RAW_KEY);
    mockExportBackend({
      findings: [FINDING_A],
      dashboard: { status: 500, body: envelope('INTERNAL_ERROR', 'boom') },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText(FINDING_A.title)).toBeTruthy());

    await act(async () => {
      screen.getByText('Export PDF').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText(/Could not build the report/)).toBeTruthy());
    expect(pdfSaveCalls).toHaveLength(0);
  });
});
