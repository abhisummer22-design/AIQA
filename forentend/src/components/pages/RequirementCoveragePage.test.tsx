/**
 * Phase 6 tests for the Requirement Coverage page — the gap check
 * (`gap_check.rs`, `POST .../gap-check`) shown to a person. Exercised through the
 * real `AppShell` tree, like `EvidencePage.test.tsx`; `fetch` is mocked only at
 * the transport boundary, so what is asserted is what the page really sends and
 * shows.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { GapCheckResponse, GapRequirement, Project, RunListItem } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_gap_test' } };
}

const RAW_KEY = 'aiqa_gaptest000000000000000';
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
// The list is newest first: crawl-2 is the newest.
const CRAWL_NEW: RunListItem = { run_id: 'crawl-2', name: 'Crawl: https://shop.example.test', state: 'COMPLETED', total: 1, passed: 1, failed: 0, project_id: PROJECT_A.id };
const REGRESSION: RunListItem = { run_id: 'run-9', name: 'Regression A1', state: 'COMPLETED', total: 10, passed: 9, failed: 1, project_id: PROJECT_A.id };
const CRAWL_OLD: RunListItem = { run_id: 'crawl-1', name: 'Crawl: https://old.example.test', state: 'COMPLETED', total: 1, passed: 1, failed: 0, project_id: PROJECT_A.id };
const CRAWL_B: RunListItem = { run_id: 'crawl-b', name: 'Crawl: https://beta.example.test', state: 'COMPLETED', total: 1, passed: 1, failed: 0, project_id: PROJECT_B.id };

function requirement(overrides: Partial<GapRequirement> & Pick<GapRequirement, 'key' | 'status'>): GapRequirement {
  return {
    section: '3 Shop',
    kind: 'functional',
    text: `${overrides.key} text`,
    matches: [],
    cases: { total: 0, approved: 0, executed: 0, techniques: {}, missing_techniques: ['positive', 'negative', 'edge'] },
    ...overrides,
  };
}

const FR_LOGIN = requirement({
  key: 'FR-001',
  kind: 'security',
  status: 'FOUND',
  basis: 'text',
  text: 'The user shall be able to log in with a username and password.',
  matches: [
    {
      url: 'https://shop.example.test/',
      title: 'Shop',
      score: 1,
      basis: 'text',
      terms: ['password', 'username', 'login'],
      elements: ['Username', 'Password', 'Login'],
      roles: ['standard'],
    },
  ],
  cases: { total: 2, approved: 1, executed: 1, techniques: { positive: 1, negative: 1 }, missing_techniques: ['edge'] },
});
const FR_WEAK = requirement({
  key: 'FR-004',
  status: 'WEAK',
  basis: 'text',
  text: 'Users can add products to a wishlist.',
  matches: [
    {
      url: 'https://shop.example.test/inventory.html?sort=price',
      title: 'Shop',
      score: 0.545,
      basis: 'text',
      terms: ['add', 'products'],
      elements: ['Add to cart'],
      roles: ['standard'],
    },
  ],
});
const FR_MISSING = requirement({
  key: 'FR-005',
  status: 'MISSING',
  text: 'The system shall export any report as a CSV file.',
  cases: { total: 1, approved: 1, executed: 0, techniques: { positive: 1 }, missing_techniques: ['negative', 'edge'] },
});
const FR_PERF = requirement({ key: 'FR-006', kind: 'performance', status: 'NOT_CHECKABLE', text: 'Pages shall load within 2 seconds.', cases: { total: 0, approved: 0, executed: 0, techniques: {}, missing_techniques: ['performance'] } });

const REPORT: GapCheckResponse = {
  crawl: {
    run_id: 'crawl-2',
    source: 'grounding',
    role: null,
    roles: [
      { role: 'standard', login_succeeded: true, pages: 4 },
      { role: 'admin', login_succeeded: false, pages: 1 },
    ],
  },
  summary: {
    requirements: 4,
    checkable: 3,
    found: 1,
    weak: 1,
    missing: 1,
    not_checkable: 1,
    pages: 4,
    undocumented_pages: 1,
    cases: 3,
    with_cases: 2,
    shown_without_cases: 1,
    tested_without_evidence: 1,
  },
  requirements: [FR_LOGIN, FR_WEAK, FR_MISSING, FR_PERF],
  undocumented_pages: [
    { url: 'https://shop.example.test/legal/terms.html', title: 'Terms of Service', roles: ['standard'], elements: ['Privacy Policy'], element_count: 1 },
  ],
  notes: ['Role "admin" could not log in, so its pages are only what was reachable before that.'],
};

type GapReply = (body: Record<string, unknown>, projectId: string) => Response | Promise<Response>;

function mockBackend(opts: {
  projects: Project[];
  runsByProjectId?: Record<string, RunListItem[] | { status: number; body: unknown } | 'offline'>;
  gap?: GapReply;
}) {
  const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    if (url.includes('/gap-check')) {
      const projectId = (url.match(/projects\/([^/]+)\/gap-check/) ?? [])[1] ?? '';
      const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : {};
      if (opts.gap) return opts.gap(body, projectId);
      return jsonResponse(200, REPORT);
    }
    if (url.includes('/executions')) {
      const projectId = (url.match(/projects\/([^/]+)\/executions/) ?? [])[1] ?? '';
      const entry = opts.runsByProjectId?.[projectId];
      if (entry === 'offline') throw new TypeError('Failed to fetch');
      if (entry && !Array.isArray(entry) && 'status' in entry) return jsonResponse(entry.status, entry.body);
      return jsonResponse(200, entry ?? []);
    }
    if (url.includes('/dashboard')) {
      const pid = (url.match(/projects\/([^/]+)\/dashboard/) ?? [])[1] ?? '';
      return jsonResponse(200, {
        project_id: pid,
        project_name: opts.projects.find((p) => p.id === pid)?.name ?? 'Unknown',
        execution_counts: {},
        task_counts: {},
        pass_fail: { passed: 0, failed: 0 },
        test_count: 0,
        generated_at: '2026-08-19T00:00:00Z',
      });
    }
    if (url.includes('/kpis')) {
      const pid = (url.match(/projects\/([^/]+)\/kpis/) ?? [])[1] ?? '';
      return jsonResponse(200, { project_id: pid, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
    }
    // Only the projects list is the projects: anything else the app asks for (the generator page, when a test
    // navigates to it) is an empty list.
    if (/\/projects(\?|$)/.test(url)) return jsonResponse(200, opts.projects);
    return jsonResponse(200, []);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

/** The bodies of every gap-check request made so far. */
function gapBodies(fetchMock: ReturnType<typeof vi.fn>): Record<string, unknown>[] {
  return fetchMock.mock.calls
    .filter(([u]: [string]) => String(u).includes('/gap-check'))
    .map(([, init]: [string, RequestInit | undefined]) => (init?.body ? JSON.parse(String(init.body)) : {}));
}

const LocationProbe: React.FC = () => {
  const location = useLocation();
  return <div data-testid="location-probe" data-pathname={location.pathname} />;
};

function renderApp(initialEntries: string[] = ['/requirement-coverage']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <LocationProbe />
      <AppShell />
    </MemoryRouter>,
  );
}

function currentPath(): string {
  return screen.getByTestId('location-probe').getAttribute('data-pathname') ?? '';
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

async function openReport(project: Project = PROJECT_A): Promise<void> {
  await verifyKey();
  await openAndSelectProject(project);
  await waitFor(() => expect(document.getElementById('gap-summary')).toBeTruthy());
}

function select(id: string, value: string) {
  const el = document.getElementById(id) as HTMLSelectElement;
  fireEvent.change(el, { target: { value } });
}

function rowIds(): string[] {
  return Array.from(document.querySelectorAll('#gap-requirement-list > li')).map((li) => li.id);
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('RequirementCoveragePage — reaching it', () => {
  it('a browser with no API key never reaches the backend and shows the sign-in state, not the page', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('aiqa-requirement-coverage-page')).toBeNull();
  });

  it('has its own entry in the navigation, which leads to /requirement-coverage', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp(['/dashboard']);
    await verifyKey();
    const nav = document.getElementById('nav-item-requirement-coverage');
    expect(nav).toBeTruthy();
    expect(nav!.textContent).toContain('Requirement Coverage');
    await act(async () => {
      nav!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    expect(currentPath()).toBe('/requirement-coverage');
    await waitFor(() => expect(document.getElementById('aiqa-requirement-coverage-page')).toBeTruthy());
  });

  it('with no project selected it asks for one and calls nothing', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(fetchMock.mock.calls.some(([u]: [string]) => String(u).includes('/gap-check'))).toBe(false);
    expect(fetchMock.mock.calls.some(([u]: [string]) => String(u).includes('/executions'))).toBe(false);
  });

  it('a project that has run no crawl says so and points to where one is made', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [REGRESSION] } });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No crawl yet')).toBeTruthy());
    expect(gapBodies(fetchMock)).toHaveLength(0);
    await act(async () => {
      screen.getByText('Open Test Case Generator').click();
    });
    expect(currentPath()).toBe('/test-generator');
  });
});

describe('RequirementCoveragePage — the free check', () => {
  it('runs the words check by itself, against the newest crawl, sending no option that calls a provider', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW, REGRESSION, CRAWL_OLD] } });
    renderApp();
    await openReport();

    // Only crawls are offered, the newest chosen.
    const crawlSelect = document.getElementById('gap-crawl-select') as HTMLSelectElement;
    expect(Array.from(crawlSelect.options).map((o) => o.value)).toEqual(['crawl-2', 'crawl-1']);
    expect(crawlSelect.value).toBe('crawl-2');
    expect(gapBodies(fetchMock)).toEqual([{ crawl_run_id: 'crawl-2' }]);
    const call = fetchMock.mock.calls.find(([u]: [string]) => String(u).includes('/gap-check'))!;
    expect(String(call[0])).toBe(`/api/v1/projects/${PROJECT_A.id}/gap-check`);
    expect((call[1] as RequestInit).method).toBe('POST');
  });

  it('looks at the newest crawl that finished, not one still running or one that failed, and at the newest when none has', async () => {
    setApiKey(RAW_KEY);
    const running: RunListItem = { ...CRAWL_NEW, run_id: 'crawl-3', state: 'RUNNING' };
    const failed: RunListItem = { ...CRAWL_NEW, run_id: 'crawl-4', state: 'FAILED' };
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [failed, running, CRAWL_NEW, CRAWL_OLD] } });
    renderApp();
    await openReport();
    expect((document.getElementById('gap-crawl-select') as HTMLSelectElement).value).toBe('crawl-2');
    expect(gapBodies(fetchMock)).toEqual([{ crawl_run_id: 'crawl-2' }]);
  });

  it('counts a crawl that ended partly (a role could not log in) as finished', async () => {
    setApiKey(RAW_KEY);
    const running: RunListItem = { ...CRAWL_NEW, run_id: 'crawl-4', state: 'RUNNING' };
    const partial: RunListItem = { ...CRAWL_NEW, run_id: 'crawl-3', state: 'PARTIAL' };
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [running, partial, CRAWL_NEW, CRAWL_OLD] } });
    renderApp();
    await openReport();
    expect((document.getElementById('gap-crawl-select') as HTMLSelectElement).value).toBe('crawl-3');
    expect(gapBodies(fetchMock)).toEqual([{ crawl_run_id: 'crawl-3' }]);
  });

  it('with no crawl finished it looks at the newest, whatever it is', async () => {
    setApiKey(RAW_KEY);
    const running: RunListItem = { ...CRAWL_NEW, run_id: 'crawl-3', state: 'RUNNING' };
    const failed: RunListItem = { ...CRAWL_NEW, run_id: 'crawl-4', state: 'FAILED' };
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [running, failed] } });
    renderApp();
    await openReport();
    expect((document.getElementById('gap-crawl-select') as HTMLSelectElement).value).toBe('crawl-3');
    expect(gapBodies(fetchMock)).toEqual([{ crawl_run_id: 'crawl-3' }]);
  });

  it('shows the counts the backend gave, and says what "found" does and does not mean', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] } });
    renderApp();
    await openReport();
    const tile = (id: string) => document.getElementById(id)!.textContent;
    expect(tile('gap-tile-requirements')).toContain('4');
    expect(tile('gap-tile-found')).toContain('1');
    expect(tile('gap-tile-weak')).toContain('1');
    expect(tile('gap-tile-missing')).toContain('1');
    expect(tile('gap-tile-not-checkable')).toContain('1');
    expect(tile('gap-tile-pages')).toContain('4');
    expect(tile('gap-tile-undocumented')).toContain('1');
    expect(tile('gap-tile-cases')).toContain('3');
    expect(document.getElementById('gap-summary')!.textContent).toMatch(/not proof that it works/);
    expect(document.getElementById('gap-shown-untested')!.textContent).toContain('1 shown by the application, no case tests it');
    expect(document.getElementById('gap-tested-unshown')!.textContent).toContain('1 with cases, but no sign of it in the crawl');
    expect(document.getElementById('gap-notes')!.textContent).toContain('"admin" could not log in');
  });

  it('shows each count under its own name, whatever the counts are', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      // Every figure different, so a tile or a sentence that read another figure would show.
      gap: () =>
        jsonResponse(200, {
          ...REPORT,
          summary: { requirements: 14, checkable: 12, found: 5, weak: 4, missing: 3, not_checkable: 2, pages: 9, undocumented_pages: 6, cases: 8, with_cases: 7, shown_without_cases: 10, tested_without_evidence: 11 },
        }),
    });
    renderApp();
    await openReport();
    const shown = (id: string) => document.getElementById(id)?.textContent ?? '';
    expect(shown('gap-tile-requirements')).toBe('14requirements');
    expect(shown('gap-tile-found')).toBe('5found on a page');
    expect(shown('gap-tile-weak')).toBe('4weak evidence');
    expect(shown('gap-tile-missing')).toBe('3no UI evidence');
    expect(shown('gap-tile-not-checkable')).toBe('2not checkable');
    expect(shown('gap-tile-pages')).toBe('9pages crawled');
    expect(shown('gap-tile-undocumented')).toBe('6pages not in the documents');
    expect(shown('gap-tile-cases')).toBe('8cases tied to a requirement');
    expect(document.getElementById('gap-summary')!.textContent).toContain('7 of 14 requirements have a case');
    expect(shown('gap-shown-untested')).toContain('10 shown by the application, no case tests them');
    expect(shown('gap-tested-unshown')).toContain('11 with cases, but no sign of them in the crawl');
  });

  it('lists every requirement with its status, its best page and how many cases it has', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] } });
    renderApp();
    await openReport();
    expect(rowIds()).toEqual(['gap-requirement-0', 'gap-requirement-1', 'gap-requirement-2', 'gap-requirement-3']);
    const login = document.getElementById('gap-requirement-0')!;
    expect(login.textContent).toContain('FR-001');
    expect(login.textContent).toContain('Found');
    expect(login.textContent).toContain('100%');
    expect(login.textContent).toContain('2 cases');
    const missing = document.getElementById('gap-requirement-2')!;
    expect(missing.textContent).toContain('No UI evidence');
    const weak = document.getElementById('gap-requirement-1')!;
    expect(weak.textContent).toContain('Weak');
    expect(weak.textContent).toContain('/inventory.html?sort=price · 55%');
    expect(weak.textContent).toContain('no cases');
    expect(document.getElementById('gap-requirement-3')!.textContent).toContain('Not checkable');
  });

  it('opens a requirement to show the words and elements behind a match, and the cases and what they lack', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] } });
    renderApp();
    await openReport();
    const row = document.getElementById('gap-requirement-0')!;
    expect(within(row).queryByText('words on the page:')).toBeNull();
    await act(async () => {
      within(row).getAllByRole('button')[0].click();
    });
    expect(within(row).getByText('words on the page:')).toBeTruthy();
    expect(within(row).getByText('password')).toBeTruthy();
    expect(within(row).getByText('carried by:')).toBeTruthy();
    expect(within(row).getByText('Username')).toBeTruthy();
    expect(within(row).getByText(/reached by: standard/)).toBeTruthy();
    expect(within(row).getByText('2 total')).toBeTruthy();
    expect(within(row).getByText('negative × 1')).toBeTruthy();
    expect(within(row).getByText('still without a case:')).toBeTruthy();
    expect(within(row).getByText('edge')).toBeTruthy();
    // And it closes again.
    await act(async () => {
      within(row).getAllByRole('button')[0].click();
    });
    expect(within(row).queryByText('words on the page:')).toBeNull();
  });

  it('a requirement no page carries says why, and one with no case says so', async () => {
    setApiKey(RAW_KEY);
    mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] } });
    renderApp();
    await openReport();
    const row = document.getElementById('gap-requirement-2')!;
    await act(async () => {
      within(row).getAllByRole('button')[0].click();
    });
    expect(within(row).getByText('No page of the crawl carries what this requirement is about.')).toBeTruthy();
    const weak = document.getElementById('gap-requirement-1')!;
    await act(async () => {
      within(weak).getAllByRole('button')[0].click();
    });
    expect(within(weak).getByText('No case is tied to this requirement.')).toBeTruthy();
  });

  it('lists the pages no requirement mentions, and says when there are none', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW], [PROJECT_B.id]: [CRAWL_B] },
      gap: (_body, projectId) =>
        jsonResponse(200, projectId === PROJECT_A.id ? REPORT : { ...REPORT, undocumented_pages: [], summary: { ...REPORT.summary, undocumented_pages: 0 } }),
    });
    renderApp();
    await openReport(PROJECT_A);
    const section = document.getElementById('gap-undocumented-pages')!;
    expect(section.textContent).toContain('/legal/terms.html');
    expect(section.textContent).toContain('Terms of Service');
    expect(section.textContent).toContain('Privacy Policy');
    expect(section.textContent).toContain('1 page that no requirement mentions');

    await openAndSelectProject(PROJECT_B);
    await waitFor(() =>
      expect(document.getElementById('gap-undocumented-pages')!.textContent).toContain('Every crawled page is pointed at by at least one requirement'),
    );
  });
});

describe('RequirementCoveragePage — narrowing the list', () => {
  async function ready() {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] } });
    renderApp();
    await openReport();
    return fetchMock;
  }

  it('filters by status, with the count of each on its button', async () => {
    const fetchMock = await ready();
    expect(document.getElementById('gap-filter-ALL')!.textContent).toBe('All (4)');
    expect(document.getElementById('gap-filter-MISSING')!.textContent).toBe('No UI evidence (1)');
    await act(async () => {
      document.getElementById('gap-filter-MISSING')!.click();
    });
    expect(rowIds()).toEqual(['gap-requirement-2']);
    expect(document.getElementById('gap-filter-MISSING')!.getAttribute('aria-pressed')).toBe('true');
    await act(async () => {
      document.getElementById('gap-filter-FOUND')!.click();
    });
    expect(rowIds()).toEqual(['gap-requirement-0']);
    await act(async () => {
      document.getElementById('gap-filter-ALL')!.click();
    });
    expect(rowIds()).toHaveLength(4);
    // Filtering is on what is already here: it asks the backend for nothing.
    expect(gapBodies(fetchMock)).toHaveLength(1);
  });

  it('shows only the requirements with no case, and can search their key and text', async () => {
    await ready();
    await act(async () => {
      document.getElementById('gap-untested-only')!.click();
    });
    expect(rowIds()).toEqual(['gap-requirement-1', 'gap-requirement-3']);
    await act(async () => {
      document.getElementById('gap-untested-only')!.click();
    });
    fireEvent.change(document.getElementById('gap-search')!, { target: { value: 'csv' } });
    expect(rowIds()).toEqual(['gap-requirement-2']);
    fireEvent.change(document.getElementById('gap-search')!, { target: { value: 'FR-001' } });
    expect(rowIds()).toEqual(['gap-requirement-0']);
    fireEvent.change(document.getElementById('gap-search')!, { target: { value: 'nothing like this' } });
    expect(rowIds()).toEqual([]);
    expect(document.getElementById('gap-no-rows')!.textContent).toContain('No requirement matches this filter');
  });

  it('keeps a row open through filtering by the row it is, not by its position', async () => {
    await ready();
    await act(async () => {
      within(document.getElementById('gap-requirement-2')!).getAllByRole('button')[0].click();
    });
    await act(async () => {
      document.getElementById('gap-filter-MISSING')!.click();
    });
    expect(within(document.getElementById('gap-requirement-2')!).getByText('Cases')).toBeTruthy();
  });
});

describe('RequirementCoveragePage — the crawl and the role', () => {
  it('checks against another crawl when one is chosen, and forgets the role of the last', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW, CRAWL_OLD] } });
    renderApp();
    await openReport();
    select('gap-role-select', 'standard');
    await waitFor(() => expect(gapBodies(fetchMock)).toHaveLength(2));
    select('gap-crawl-select', 'crawl-1');
    await waitFor(() => expect(gapBodies(fetchMock)).toHaveLength(3));
    expect(gapBodies(fetchMock)[2]).toEqual({ crawl_run_id: 'crawl-1' });
    expect((document.getElementById('gap-role-select') as HTMLSelectElement).value).toBe('');
  });

  it('offers the roles of the crawl, and checks one role only when it is chosen, without losing the others', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      // A report limited to one role names only that role.
      gap: (body) =>
        jsonResponse(
          200,
          body.role ? { ...REPORT, crawl: { ...REPORT.crawl, role: 'standard', roles: [REPORT.crawl.roles[0]] } } : REPORT,
        ),
    });
    renderApp();
    await openReport();
    const roles = () => Array.from((document.getElementById('gap-role-select') as HTMLSelectElement).options).map((o) => o.value);
    expect(roles()).toEqual(['', 'standard', 'admin']);
    select('gap-role-select', 'standard');
    await waitFor(() => expect(gapBodies(fetchMock)).toHaveLength(2));
    expect(gapBodies(fetchMock)[1]).toEqual({ crawl_run_id: 'crawl-2', role: 'standard' });
    await waitFor(() => expect(document.getElementById('gap-summary')).toBeTruthy());
    expect(roles()).toEqual(['', 'standard', 'admin']);
  });
});

describe('RequirementCoveragePage — the options that call a provider', () => {
  it('runs them only from the button, with the choices it was given, and never without a choice', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] } });
    renderApp();
    await openReport();
    const run = document.getElementById('gap-run-button') as HTMLButtonElement;
    expect(run.disabled).toBe(true);

    await act(async () => {
      document.getElementById('gap-use-embeddings')!.click();
    });
    expect(run.disabled).toBe(false);
    // Ticking is not running.
    expect(gapBodies(fetchMock)).toHaveLength(1);

    await act(async () => {
      document.getElementById('gap-verify')!.click();
    });
    expect((document.getElementById('gap-max-verifications') as HTMLInputElement).value).toBe('12');
    fireEvent.change(document.getElementById('gap-max-verifications')!, { target: { value: '500' } });
    expect((document.getElementById('gap-max-verifications') as HTMLInputElement).value).toBe('60');
    fireEvent.change(document.getElementById('gap-max-verifications')!, { target: { value: '0' } });
    expect((document.getElementById('gap-max-verifications') as HTMLInputElement).value).toBe('1');
    fireEvent.change(document.getElementById('gap-max-verifications')!, { target: { value: '7' } });
    await act(async () => {
      run.click();
    });
    await waitFor(() => expect(gapBodies(fetchMock)).toHaveLength(2));
    expect(gapBodies(fetchMock)[1]).toEqual({ crawl_run_id: 'crawl-2', use_embeddings: true, verify: true, max_verifications: 7 });
  });

  it('sends only the option that was ticked', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A], runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] } });
    renderApp();
    await openReport();
    await act(async () => {
      document.getElementById('gap-verify')!.click();
    });
    await act(async () => {
      document.getElementById('gap-run-button')!.click();
    });
    await waitFor(() => expect(gapBodies(fetchMock)).toHaveLength(2));
    expect(gapBodies(fetchMock)[1]).toEqual({ crawl_run_id: 'crawl-2', verify: true, max_verifications: 12 });
  });

  it('shows what each step did, and the AI check on the requirement it changed', async () => {
    setApiKey(RAW_KEY);
    const checked = requirement({
      key: 'FR-004',
      status: 'FOUND',
      basis: 'ai',
      text: 'Users can add products to a wishlist.',
      matches: [{ url: 'https://shop.example.test/inventory.html', title: 'Shop', score: 0.54, basis: 'ai', terms: ['add'], elements: [], roles: ['standard'] }],
      verification: {
        verdict: 'CONFIRMED',
        pages: ['https://shop.example.test/inventory.html'],
        reason: 'The product list has a heart button that adds a product to the wishlist.',
        by: 'mock/mock-model',
      },
    });
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      gap: (body) =>
        jsonResponse(
          200,
          body.verify
            ? {
                ...REPORT,
                requirements: [FR_LOGIN, checked, FR_MISSING, FR_PERF],
                method: {
                  embeddings: { requested: false, used: false, model: null, texts: 0, failed: 0, note: null },
                  verification: { requested: true, calls: 1, checked: 2, confirmed: 1, not_found: 0, unclear: 1, failed: 0, by: ['mock/mock-model'], note: null },
                },
              }
            : REPORT,
        ),
    });
    renderApp();
    await openReport();
    await act(async () => {
      document.getElementById('gap-verify')!.click();
    });
    await act(async () => {
      document.getElementById('gap-run-button')!.click();
    });
    await waitFor(() => expect(document.getElementById('gap-method')).toBeTruthy());
    expect(document.getElementById('gap-method')!.textContent).toContain(
      'AI check by mock/mock-model: 2 checked in 1 call — 1 confirmed, 0 not found, 1 unclear.',
    );
    const row = document.getElementById('gap-requirement-1')!;
    expect(row.textContent).toContain('AI-checked');
    await act(async () => {
      within(row).getAllByRole('button')[0].click();
    });
    expect(within(row).getByText('AI check: CONFIRMED')).toBeTruthy();
    expect(within(row).getByText('The product list has a heart button that adds a product to the wishlist.')).toBeTruthy();
    expect(within(row).getByText('mock/mock-model')).toBeTruthy();
  });

  it('says an embedder that could not be used was not, with the reason', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      gap: (body) =>
        jsonResponse(
          200,
          body.use_embeddings
            ? {
                ...REPORT,
                method: {
                  embeddings: { requested: true, used: false, model: null, texts: 0, failed: 0, note: 'GEMINI_API_KEY is not set' },
                  verification: { requested: false, calls: 0, checked: 0, confirmed: 0, not_found: 0, unclear: 0, failed: 0, by: [], note: null },
                },
              }
            : REPORT,
        ),
    });
    renderApp();
    await openReport();
    await act(async () => {
      document.getElementById('gap-use-embeddings')!.click();
    });
    await act(async () => {
      document.getElementById('gap-run-button')!.click();
    });
    await waitFor(() => expect(document.getElementById('gap-method')).toBeTruthy());
    expect(document.getElementById('gap-method')!.textContent).toContain('Meaning was not compared: GEMINI_API_KEY is not set.');
  });

  it('shows a meaning match as such', async () => {
    setApiKey(RAW_KEY);
    const semantic = requirement({
      key: 'FR-009',
      status: 'WEAK',
      basis: 'embedding',
      text: 'Users can purchase gift cards.',
      matches: [{ url: 'https://shop.example.test/cards', title: 'Cards', score: 0, basis: 'embedding', terms: [], elements: [], roles: [], similarity: 0.83 }],
    });
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      gap: () => jsonResponse(200, { ...REPORT, requirements: [semantic] }),
    });
    renderApp();
    await openReport();
    const row = document.getElementById('gap-requirement-0')!;
    expect(row.textContent).toContain('by meaning');
    await act(async () => {
      within(row).getAllByRole('button')[0].click();
    });
    expect(within(row).getByText('similarity 0.83')).toBeTruthy();
  });
});

describe('RequirementCoveragePage — when the check cannot run', () => {
  it('says there are no requirements and points to the generator', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      gap: () => jsonResponse(400, envelope('VALIDATION_ERROR', 'there are no requirements to check: upload a requirements document first (Test Case Generator, Source & Generation)')),
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No requirements to check')).toBeTruthy());
    expect(screen.getByText(/upload a requirements document first/)).toBeTruthy();
    await act(async () => {
      screen.getByText('Open Test Case Generator').click();
    });
    expect(currentPath()).toBe('/test-generator');
  });

  it('says a crawl that left nothing to read has nothing to compare with, and can try again', async () => {
    setApiKey(RAW_KEY);
    let calls = 0;
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      gap: () => {
        calls += 1;
        return calls === 1
          ? jsonResponse(404, envelope('RESOURCE_NOT_FOUND', 'this run recorded no crawl — it is not a crawl, or it is still running or did not finish'))
          : jsonResponse(200, REPORT);
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('No crawl to check against')).toBeTruthy());
    expect(screen.getByText(/recorded no crawl/)).toBeTruthy();
    await act(async () => {
      screen.getByText('Try again').click();
    });
    await waitFor(() => expect(document.getElementById('gap-summary')).toBeTruthy());
    expect(gapBodies(fetchMock)).toHaveLength(2);
    expect(screen.queryByText('No crawl to check against')).toBeNull();
  });

  it('shows a forbidden key as a permission problem, and an unreachable orchestrator as such, each with a retry', async () => {
    setApiKey(RAW_KEY);
    let mode: 'forbidden' | 'offline' | 'ok' = 'forbidden';
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      gap: () => {
        if (mode === 'forbidden') return jsonResponse(403, envelope('FORBIDDEN', 'role viewer cannot perform this action (requires operator)'));
        if (mode === 'offline') throw new TypeError('Failed to fetch');
        return jsonResponse(200, REPORT);
      },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Not permitted to run this check')).toBeTruthy());
    mode = 'offline';
    await act(async () => {
      screen.getByText('Retry').click();
    });
    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
    mode = 'ok';
    await act(async () => {
      screen.getByText('Retry').click();
    });
    await waitFor(() => expect(document.getElementById('gap-summary')).toBeTruthy());
    expect(gapBodies(fetchMock)).toHaveLength(3);
  });

  it('shows the executions listing failing as its own error, without trying the check', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: 'offline' },
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(screen.getByText('Orchestrator unreachable')).toBeTruthy());
    expect(gapBodies(fetchMock)).toHaveLength(0);
  });
});

describe('RequirementCoveragePage — moving between projects', () => {
  it('drops one project\'s report on a switch and never shows a late answer for the old one', async () => {
    setApiKey(RAW_KEY);
    let releaseA: (r: Response) => void = () => {};
    const pendingA = new Promise<Response>((resolve) => {
      releaseA = resolve;
    });
    let releaseB: (r: Response) => void = () => {};
    const pendingB = new Promise<Response>((resolve) => {
      releaseB = resolve;
    });
    const fetchMock = mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW], [PROJECT_B.id]: [CRAWL_B] },
      gap: (_body, projectId) => (projectId === PROJECT_A.id ? pendingA : pendingB),
    });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(gapBodies(fetchMock)).toHaveLength(1));
    // Alpha's report arrives and is shown; then Beta is chosen and its answer is still to come.
    await act(async () => {
      releaseA(jsonResponse(200, REPORT));
    });
    await waitFor(() => expect(screen.getByText('FR-001')).toBeTruthy());
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(gapBodies(fetchMock)).toHaveLength(2));
    expect(screen.queryByText('FR-001'), "Alpha's report is not Beta's").toBeNull();
    expect(document.getElementById('gap-summary')).toBeNull();
    await act(async () => {
      releaseB(
        jsonResponse(200, { ...REPORT, requirements: [requirement({ key: 'B-1', status: 'FOUND', text: 'Beta requirement' })], summary: { ...REPORT.summary, requirements: 1, found: 1 } }),
      );
    });
    await waitFor(() => expect(screen.getByText('B-1')).toBeTruthy());
    // A second, late answer for Alpha must not replace Beta's report either.
    await act(async () => {
      releaseA(jsonResponse(200, REPORT));
    });
    expect(screen.getByText('B-1')).toBeTruthy();
    expect(screen.queryByText('FR-001')).toBeNull();
    // One check for each project — never Alpha's crawl asked of Beta.
    expect(gapBodies(fetchMock)).toEqual([{ crawl_run_id: 'crawl-2' }, { crawl_run_id: 'crawl-b' }]);
  });

  it('shows a requirement\'s text as text', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      projects: [PROJECT_A],
      runsByProjectId: { [PROJECT_A.id]: [CRAWL_NEW] },
      gap: () =>
        jsonResponse(200, {
          ...REPORT,
          requirements: [requirement({ key: 'X-1', status: 'MISSING', text: '<img src=x onerror="window.__pwned = 1"> must work' })],
        }),
    });
    renderApp();
    await openReport();
    expect(document.querySelector('#gap-requirement-list img')).toBeNull();
    expect(screen.getByText(/<img src=x onerror=/)).toBeTruthy();
    expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
  });
});
