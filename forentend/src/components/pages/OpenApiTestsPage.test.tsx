/**
 * Phase 8B tests for the OpenAPI Tests page — contract tests made from an API's own OpenAPI document. Exercised through the real
 * `AppShell` tree, like `AutopilotPage.test.tsx`; `fetch` is mocked only at the transport boundary, so what is asserted is what the page
 * really sends and shows.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation, useNavigationType } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import { OTHER_RUN_ID, RUN_ID, planFor, reportFor, runRow } from '../openapi/openApiTestData';
import { backend, failure, requests, textResponse, type Handler } from '../autopilot/autopilotTestBackend';
import type { OpenApiReport, OpenApiRunListRow, Project } from '../../types/contract';

const RAW_KEY = 'aiqa_openapipagetest0000';
const PROJECT_A: Project = { id: 'proj-a', name: 'Project Alpha', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-b', name: 'Project Beta', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };
const SPEC = 'openapi: 3.0.3\ninfo:\n  title: Fixture Pets\n  version: "1.0"\npaths: {}\n';

interface Setup {
  runs?: OpenApiRunListRow[];
  /** The report of a run by its id: the report itself, or what answers the read (`?format=markdown` comes to the same route). */
  reports?: Record<string, OpenApiReport | Handler>;
  routes?: Record<string, Handler>;
  projects?: Project[];
}

function mockBackend({ runs = [], reports = {}, routes = {}, projects = [PROJECT_A] }: Setup = {}) {
  const perRun: Record<string, Handler> = {};
  for (const [id, value] of Object.entries(reports)) perRun[`GET /openapi/runs/${id}$`] = typeof value === 'function' ? value : () => value;
  return backend({
    'GET /projects$': () => projects,
    'GET /dashboard': (req) => ({
      project_id: req.path.split('/')[4],
      project_name: 'x',
      execution_counts: {},
      task_counts: {},
      pass_fail: { passed: 0, failed: 0 },
      test_count: 0,
      generated_at: '2026-08-19T00:00:00Z',
    }),
    'GET /kpis': (req) => ({ project_id: req.path.split('/')[4], execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' }),
    'GET /openapi/runs$': () => ({ runs }),
    'POST /openapi/plan': () => planFor(),
    'POST /openapi/runs$': () => ({ id: RUN_ID, run_id: 'x', cases: 12, cases_run: 8, not_run: 4, state: 'SCHEDULED', notes: [] }),
    ...perRun,
    ...routes,
  });
}

const LocationProbe: React.FC = () => {
  const location = useLocation();
  const action = useNavigationType();
  return <div data-testid="location-probe" data-pathname={location.pathname} data-search={location.search} data-action={action} />;
};

function renderApp(initialEntries: string[] = ['/openapi-tests']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <LocationProbe />
      <AppShell />
    </MemoryRouter>,
  );
}

const probe = () => screen.getByTestId('location-probe');
const currentPath = () => probe().getAttribute('data-pathname') ?? '';
const currentSearch = () => probe().getAttribute('data-search') ?? '';
const el = (id: string) => document.getElementById(id);
const text = (id: string) => el(id)?.textContent ?? '';
const button = (id: string) => el(id) as HTMLButtonElement;

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

/** The page, with the key verified and the project chosen. */
async function openPage(project: Project = PROJECT_A, entries?: string[]) {
  renderApp(entries);
  await verifyKey();
  await openAndSelectProject(project);
  await waitFor(() => expect(el('aiqa-openapi-page')).toBeTruthy());
}

async function pasteSpec(spec = SPEC) {
  await act(async () => void fireEvent.change(el('openapi-spec')!, { target: { value: spec } }));
}

async function showCases() {
  await act(async () => button('openapi-show-cases').click());
  await waitFor(() => expect(el('openapi-preview')).toBeTruthy());
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  setApiKey(RAW_KEY);
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe('OpenApiTestsPage — reaching it', () => {
  it('a browser with no API key never reaches the backend and shows the sign-in state, not the page', async () => {
    localStorage.clear();
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(el('aiqa-openapi-page')).toBeNull();
  });

  it('has its own entry in the navigation, which leads to /openapi-tests', async () => {
    mockBackend();
    renderApp(['/dashboard']);
    await verifyKey();
    const nav = el('nav-item-openapi-tests');
    expect(nav).toBeTruthy();
    expect(nav!.textContent).toContain('OpenAPI Tests');
    await act(async () => {
      nav!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    expect(currentPath()).toBe('/openapi-tests');
    await waitFor(() => expect(el('aiqa-openapi-page')).toBeTruthy());
  });

  it('with no project selected it asks for one and asks the service nothing', async () => {
    const fetchMock = mockBackend();
    renderApp();
    await verifyKey();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(requests(fetchMock, 'GET /openapi')).toHaveLength(0);
    expect(requests(fetchMock, 'POST /openapi')).toHaveLength(0);
    expect(el('openapi-form')).toBeNull();
  });

  it('names the project in its header and says what it does', async () => {
    mockBackend();
    await openPage();
    expect(text('openapi-header')).toContain('OpenAPI Tests');
    expect(text('openapi-header')).toContain('Contract tests for the API of Project Alpha');
    expect(text('openapi-header')).toContain('by rule, with no AI');
  });
});

describe('OpenApiTestsPage — a test that is new', () => {
  it('starts with the form, the runs of the project beside it and nothing shown of what is to come', async () => {
    const fetchMock = mockBackend();
    await openPage();
    expect(el('openapi-form')).toBeTruthy();
    expect(el('openapi-preview')).toBeNull();
    expect(el('openapi-report')).toBeNull();
    await waitFor(() => expect(text('openapi-runs')).toContain('No run yet.'));
    expect(button('openapi-new').getAttribute('aria-pressed')).toBe('true');
    expect(requests(fetchMock, 'GET /openapi/runs$')[0].path).toBe('/api/v1/projects/proj-a/openapi/runs');
    expect(button('openapi-show-cases').disabled).toBe(true);
  });

  it('asks the service to show the cases for what was pasted, and shows them', async () => {
    const fetchMock = mockBackend();
    await openPage();
    await pasteSpec();
    expect(button('openapi-show-cases').disabled).toBe(false);
    await showCases();
    const asked = requests(fetchMock, 'POST /openapi/plan');
    expect(asked).toHaveLength(1);
    expect(asked[0].path).toBe('/api/v1/projects/proj-a/openapi/plan');
    expect(asked[0].body).toEqual({
      spec: SPEC,
      kinds: ['HAPPY', 'NO_AUTH', 'MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND'],
      include_unsafe: false,
      max_cases_per_operation: 8,
    });
    expect(text('openapi-tile-cases')).toContain('12');
    expect(text('openapi-tile-run')).toContain('8');
    expect(text('openapi-run')).toBe('Run 8 cases');
    // Reading it started nothing.
    expect(requests(fetchMock, 'POST /openapi/runs$')).toHaveLength(0);
  });

  it('sends the address of the document, where the API is, the credentials as references and what was chosen', async () => {
    const fetchMock = mockBackend();
    await openPage();
    await act(async () => button('openapi-source-address').click());
    await act(async () => void fireEvent.change(el('openapi-spec-url')!, { target: { value: 'https://api.example.com/openapi.json' } }));
    await act(async () => void fireEvent.change(el('openapi-base-url')!, { target: { value: 'https://api.example.com/v1' } }));
    await act(async () => button('openapi-add-credential').click());
    const variable = document.querySelector('[id^="openapi-credential-variable-"]') as HTMLInputElement;
    await act(async () => void fireEvent.change(variable, { target: { value: 'AIQA_SECRET_API_TOKEN' } }));
    await act(async () => button('openapi-kind-NOT_FOUND').click());
    await act(async () => button('openapi-unsafe').click());
    await act(async () => void fireEvent.change(el('openapi-max-cases')!, { target: { value: '5' } }));
    await showCases();
    expect(requests(fetchMock, 'POST /openapi/plan')[0].body).toEqual({
      spec_url: 'https://api.example.com/openapi.json',
      base_url: 'https://api.example.com/v1',
      auth_headers: { Authorization: 'env:AIQA_SECRET_API_TOKEN' },
      kinds: ['HAPPY', 'NO_AUTH', 'MISSING_REQUIRED', 'WRONG_TYPE'],
      include_unsafe: true,
      max_cases_per_operation: 5,
    });
  });

  it('says what is wrong with the form, and asks nothing until it is right', async () => {
    const fetchMock = mockBackend();
    await openPage();
    expect(text('openapi-problems')).toContain('Paste the OpenAPI document');
    await act(async () => void fireEvent.submit(el('openapi-form')!));
    expect(requests(fetchMock, 'POST /openapi/plan')).toHaveLength(0);
  });

  it('says why when the service cannot read the document, and keeps the form as it is', async () => {
    mockBackend({ routes: { 'POST /openapi/plan': () => failure(400, 'VALIDATION_ERROR', 'this is not an OpenAPI or Swagger document (it has neither an `openapi` nor a `swagger` field)') } });
    await openPage();
    await pasteSpec('hello: world');
    await act(async () => button('openapi-show-cases').click());
    await waitFor(() => expect(el('openapi-plan-error')).toBeTruthy());
    expect(text('openapi-plan-error')).toContain('neither an `openapi` nor a `swagger`');
    expect(el('openapi-preview')).toBeNull();
    expect((el('openapi-spec') as HTMLTextAreaElement).value).toBe('hello: world');
    expect(button('openapi-show-cases').disabled).toBe(false);
  });

  it('shows the cases as they were made only for the form it was made from: changing the form takes them away', async () => {
    mockBackend();
    await openPage();
    await pasteSpec();
    await showCases();
    await act(async () => void fireEvent.change(el('openapi-base-url')!, { target: { value: 'https://api.example.com/v2' } }));
    expect(el('openapi-preview')).toBeNull();
    expect(button('openapi-show-cases').disabled).toBe(false);
  });

  it('shows a plan that is still being read as reading, and does not show the one before', async () => {
    let release: (r: unknown) => void = () => {};
    mockBackend({ routes: { 'POST /openapi/plan': () => new Promise((resolve) => (release = resolve)) } });
    await openPage();
    await pasteSpec();
    await act(async () => button('openapi-show-cases').click());
    expect(button('openapi-show-cases').textContent).toBe('Reading the document…');
    expect(button('openapi-show-cases').disabled).toBe(true);
    expect(el('openapi-preview')).toBeNull();
    await act(async () => release(planFor()));
    await waitFor(() => expect(el('openapi-preview')).toBeTruthy());
    expect(button('openapi-show-cases').textContent).toBe('Show the cases');
  });

  it('ignores the answer to a plan the form has changed since', async () => {
    let release: (r: unknown) => void = () => {};
    mockBackend({ routes: { 'POST /openapi/plan': () => new Promise((resolve) => (release = resolve)) } });
    await openPage();
    await pasteSpec();
    await act(async () => button('openapi-show-cases').click());
    await act(async () => void fireEvent.change(el('openapi-base-url')!, { target: { value: 'https://elsewhere.example.com' } }));
    await act(async () => release(planFor()));
    expect(el('openapi-preview')).toBeNull();
  });
});

describe('OpenApiTestsPage — running the cases', () => {
  it('runs the cases of the operations chosen, all of them by leaving the choice out, and opens the report of the run', async () => {
    const fetchMock = mockBackend({ reports: { [RUN_ID]: () => reportFor() } });
    await openPage();
    await pasteSpec();
    await showCases();
    await act(async () => button('openapi-run').click());
    await waitFor(() => expect(currentSearch()).toBe(`?run=${RUN_ID}`));
    const started = requests(fetchMock, 'POST /openapi/runs$');
    expect(started).toHaveLength(1);
    expect(started[0].path).toBe('/api/v1/projects/proj-a/openapi/runs');
    expect(started[0].body.operations).toBeUndefined();
    expect(started[0].body.spec).toBe(SPEC);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    expect(text('openapi-tile-ran')).toContain('8 of 12');
    // The list of runs is read again, so the new one is in it.
    await waitFor(() => expect(requests(fetchMock, 'GET /openapi/runs$').length).toBeGreaterThan(1));
  });

  it('narrows the run to the operations chosen', async () => {
    const fetchMock = mockBackend({ reports: { [RUN_ID]: () => reportFor() } });
    await openPage();
    await pasteSpec();
    await showCases();
    expect(text('openapi-preview')).toContain('4 of 4 operations chosen');
    await act(async () => void fireEvent.click(el('openapi-op-GET /secure')!.querySelector('input[type=checkbox]')!));
    await act(async () => void fireEvent.click(el('openapi-op-POST /pets')!.querySelector('input[type=checkbox]')!));
    expect(text('openapi-preview')).toContain('2 of 4 operations chosen');
    expect(text('openapi-run')).toBe('Run 6 cases');
    await act(async () => button('openapi-run').click());
    await waitFor(() => expect(requests(fetchMock, 'POST /openapi/runs$')).toHaveLength(1));
    expect(requests(fetchMock, 'POST /openapi/runs$')[0].body.operations).toEqual(['GET /pets', 'GET /pets/{petId}']);
  });

  it('can choose all the operations, or none, and cannot run with none', async () => {
    mockBackend();
    await openPage();
    await pasteSpec();
    await showCases();
    await act(async () => button('openapi-select-none').click());
    expect(text('openapi-preview')).toContain('0 of 4 operations chosen');
    expect(button('openapi-run').disabled).toBe(true);
    await act(async () => button('openapi-select-all').click());
    expect(text('openapi-preview')).toContain('4 of 4 operations chosen');
    expect(button('openapi-run').disabled).toBe(false);
  });

  it('does not count an operation with no case among those to choose', async () => {
    const plan = planFor({
      operations: [...planFor().operations, { id: 'GET /old', method: 'GET', path: '/old', summary: null, tags: [], deprecated: true, requires_auth: false }],
      skipped: [{ operation: 'GET /old', reason: 'the operation is marked deprecated' }],
    });
    const fetchMock = mockBackend({ routes: { 'POST /openapi/plan': () => plan }, reports: { [RUN_ID]: () => reportFor() } });
    await openPage();
    await pasteSpec();
    await showCases();
    expect(text('openapi-preview')).toContain('4 of 5 operations chosen');
    await act(async () => button('openapi-select-none').click());
    expect(text('openapi-preview')).toContain('0 of 5 operations chosen');
    await act(async () => button('openapi-select-all').click());
    expect(text('openapi-preview'), 'all is all that have a case to run').toContain('4 of 5 operations chosen');
    await act(async () => button('openapi-run').click());
    await waitFor(() => expect(requests(fetchMock, 'POST /openapi/runs$')).toHaveLength(1));
    expect(requests(fetchMock, 'POST /openapi/runs$')[0].body.operations, 'all that can be run are chosen: none is named').toBeUndefined();
  });

  it('says why a run could not be started, and stays where it is', async () => {
    mockBackend({ routes: { 'POST /openapi/runs$': () => failure(400, 'VALIDATION_ERROR', 'TARGET_NOT_ALLOWED: rejected target 10.0.0.5:80: address is in a disallowed private range') } });
    await openPage();
    await pasteSpec();
    await showCases();
    await act(async () => button('openapi-run').click());
    await waitFor(() => expect(el('openapi-start-error')).toBeTruthy());
    expect(text('openapi-start-error')).toContain('TARGET_NOT_ALLOWED');
    expect(currentSearch()).toBe('');
    expect(el('openapi-preview')).toBeTruthy();
    expect(button('openapi-run').disabled).toBe(false);
  });

  it('shows the run as starting, and cannot be started twice', async () => {
    let release: (r: unknown) => void = () => {};
    const fetchMock = mockBackend({ routes: { 'POST /openapi/runs$': () => new Promise((resolve) => (release = resolve)) }, reports: { [RUN_ID]: () => reportFor() } });
    await openPage();
    await pasteSpec();
    await showCases();
    await act(async () => button('openapi-run').click());
    expect(button('openapi-run').textContent).toBe('Starting…');
    expect(button('openapi-run').disabled).toBe(true);
    await act(async () => button('openapi-run').click());
    expect(requests(fetchMock, 'POST /openapi/runs$')).toHaveLength(1);
    await act(async () => release({ id: RUN_ID, run_id: 'x', cases: 12, cases_run: 8, not_run: 4, state: 'SCHEDULED', notes: [] }));
    await waitFor(() => expect(currentSearch()).toBe(`?run=${RUN_ID}`));
  });

  it('cannot run while a variable the credentials name is not set', async () => {
    mockBackend({ routes: { 'POST /openapi/plan': () => planFor({ missing_secrets: ['AIQA_SECRET_API_TOKEN'] }) } });
    await openPage();
    await pasteSpec();
    await showCases();
    expect(text('openapi-missing-secrets')).toContain('AIQA_SECRET_API_TOKEN');
    expect(button('openapi-run').disabled).toBe(true);
  });
});

describe('OpenApiTestsPage — the report of a run', () => {
  it('opens on the run in the address, reads it and shows it', async () => {
    const fetchMock = mockBackend({ runs: [runRow()], reports: { [RUN_ID]: () => reportFor() } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    expect(requests(fetchMock, `GET /openapi/runs/${RUN_ID}$`)[0].path).toBe(`/api/v1/projects/proj-a/openapi/runs/${RUN_ID}`);
    expect(document.querySelector('#openapi-report h2')?.textContent).toBe('Fixture Pets 1.0');
    expect(text('openapi-coverage')).toContain('3 of 4 operations');
    expect(el('openapi-form')).toBeNull();
    expect(button('openapi-new').getAttribute('aria-pressed')).toBe('false');
  });

  it('says it is loading the report until it arrives', async () => {
    let release: (r: unknown) => void = () => {};
    mockBackend({ reports: { [RUN_ID]: () => new Promise((resolve) => (release = resolve)) } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(screen.getByText('Loading the report')).toBeTruthy());
    expect(el('openapi-report')).toBeNull();
    await act(async () => release(reportFor()));
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    expect(screen.queryByText('Loading the report')).toBeNull();
  });

  it('is read again while the run is going and then no more', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let reads = 0;
    mockBackend({
      reports: { [RUN_ID]: () => reportFor({ final: ++reads >= 2, run_state: reads >= 2 ? 'COMPLETED' : 'RUNNING' }) },
    });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report-not-final')).toBeTruthy());
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    await waitFor(() => expect(el('openapi-report-final')).toBeTruthy());
    expect(el('openapi-report-not-final')).toBeNull();
    const after = reads;
    await act(async () => void (await vi.advanceTimersByTimeAsync(20_000)));
    expect(reads).toBe(after);
  });

  it('says the key’s scope does not let it see the run', async () => {
    mockBackend({ reports: { [RUN_ID]: () => failure(403, 'FORBIDDEN', 'not in scope') } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(screen.getByText('Not permitted to view this run')).toBeTruthy());
    expect(screen.getByText(/does not permit viewing this project's API test runs/)).toBeTruthy();
  });

  it('offers to try again after another failure', async () => {
    let reads = 0;
    const fetchMock = mockBackend({ reports: { [RUN_ID]: () => (++reads === 1 ? failure(500, 'INTERNAL_ERROR', 'busy') : reportFor()) } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(screen.getByText("Couldn't load the report")).toBeTruthy());
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /retry|try again/i })));
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    expect(requests(fetchMock, `GET /openapi/runs/${RUN_ID}$`)).toHaveLength(2);
  });

  it('keeps the report it has when a later look fails, and says so', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let reads = 0;
    mockBackend({
      reports: { [RUN_ID]: () => (++reads === 2 ? failure(500, 'INTERNAL_ERROR', 'busy') : reportFor({ final: false, run_state: 'RUNNING' })) },
    });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    await waitFor(() => expect(el('openapi-look-error')).toBeTruthy());
    expect(text('openapi-look-error')).toContain('busy');
    expect(el('openapi-report')).toBeTruthy();
  });

  it('copies the report as Markdown, and says it did', async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: write }, configurable: true });
    const markdown = '# API tests from Fixture Pets 1.0\n';
    const fetchMock = mockBackend({ reports: { [RUN_ID]: (req) => (req.query.format === 'markdown' ? textResponse(200, markdown) : reportFor()) } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    expect(text('openapi-copy')).toBe('Copy as Markdown');
    await act(async () => button('openapi-copy').click());
    await waitFor(() => expect(text('openapi-copy')).toBe('Copied'));
    expect(write).toHaveBeenCalledWith(markdown);
    expect(requests(fetchMock, `GET /openapi/runs/${RUN_ID}$`).filter((r) => r.query.format === 'markdown')).toHaveLength(1);
    expect(el('openapi-markdown')).toBeNull();
  });

  it('shows the Markdown to copy by hand when the browser will not copy it', async () => {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) }, configurable: true });
    const markdown = '# API tests\n';
    mockBackend({ reports: { [RUN_ID]: (req) => (req.query.format === 'markdown' ? textResponse(200, markdown) : reportFor()) } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    await act(async () => button('openapi-copy').click());
    await waitFor(() => expect(el('openapi-markdown')).toBeTruthy());
    expect((el('openapi-markdown') as HTMLTextAreaElement).value).toBe(markdown);
    expect(text('openapi-copy')).toBe('Copy as Markdown');
  });

  it('says so when the Markdown could not be read, and keeps the report', async () => {
    mockBackend({ reports: { [RUN_ID]: (req) => (req.query.format === 'markdown' ? failure(500, 'INTERNAL_ERROR', 'no text today') : reportFor()) } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    await act(async () => button('openapi-copy').click());
    await waitFor(() => expect(el('openapi-copy-error')).toBeTruthy());
    expect(text('openapi-copy-error')).toContain('no text today');
    expect(el('openapi-report')).toBeTruthy();
    expect(el('openapi-markdown')).toBeNull();
  });

  it('clears what the copy said when another run is chosen', async () => {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockResolvedValue(undefined) }, configurable: true });
    const same = (req: { query: Record<string, string> }) => (req.query.format === 'markdown' ? textResponse(200, '# x') : reportFor());
    mockBackend({
      runs: [runRow(), runRow({ id: OTHER_RUN_ID, title: 'Older API' })],
      reports: { [RUN_ID]: same, [OTHER_RUN_ID]: (req) => (req.query.format === 'markdown' ? textResponse(200, '# y') : reportFor({ id: OTHER_RUN_ID })) },
    });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    await act(async () => button('openapi-copy').click());
    await waitFor(() => expect(text('openapi-copy')).toBe('Copied'));
    await act(async () => button(`openapi-run-${OTHER_RUN_ID}`).click());
    await waitFor(() => expect(currentSearch()).toBe(`?run=${OTHER_RUN_ID}`));
    await waitFor(() => expect(text('openapi-copy')).toBe('Copy as Markdown'));
  });
});

describe('OpenApiTestsPage — the runs of the project', () => {
  const RUNS = [runRow(), runRow({ id: OTHER_RUN_ID, title: 'Older API', version: '0.9', cases: 5, cases_run: 5, run_state: 'FAILED', created_at: '2026-09-25T08:00:00.000Z' })];

  it('are listed, newest first as the service gives them, with their cases and the time on the viewer’s clock', async () => {
    mockBackend({ runs: RUNS });
    await openPage();
    await waitFor(() => expect(el(`openapi-run-${RUN_ID}`)).toBeTruthy());
    expect(text(`openapi-run-${RUN_ID}`)).toContain('Fixture Pets 1.0');
    expect(text(`openapi-run-${RUN_ID}`)).toContain('8 of 12 cases');
    expect(text(`openapi-run-${OTHER_RUN_ID}`)).toContain('Older API 0.9');
    expect(text(`openapi-run-${OTHER_RUN_ID}`)).toContain('5 of 5 cases');
    expect(text(`openapi-run-${RUN_ID}`)).toMatch(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/);
    const ids = Array.from(document.querySelectorAll('#openapi-runs li button')).map((b) => b.id);
    expect(ids).toEqual([`openapi-run-${RUN_ID}`, `openapi-run-${OTHER_RUN_ID}`]);
  });

  it('are opened by choosing them, which puts the run in the address, and “New test” goes back to the form', async () => {
    mockBackend({ runs: RUNS, reports: { [RUN_ID]: () => reportFor(), [OTHER_RUN_ID]: () => reportFor({ id: OTHER_RUN_ID, spec: { title: 'Older API', version: '0.9', base_url: 'http://old.test' } }) } });
    await openPage();
    await waitFor(() => expect(el(`openapi-run-${OTHER_RUN_ID}`)).toBeTruthy());
    await act(async () => button(`openapi-run-${OTHER_RUN_ID}`).click());
    await waitFor(() => expect(document.querySelector('#openapi-report h2')?.textContent).toBe('Older API 0.9'));
    expect(currentSearch()).toBe(`?run=${OTHER_RUN_ID}`);
    expect(probe().getAttribute('data-action')).toBe('PUSH');
    expect(button(`openapi-run-${OTHER_RUN_ID}`).getAttribute('aria-current')).toBe('true');
    expect(button(`openapi-run-${RUN_ID}`).getAttribute('aria-current')).toBeNull();
    expect(button(`openapi-run-${OTHER_RUN_ID}`).className).toContain('bg-[#020617] border-violet-700');
    expect(button(`openapi-run-${RUN_ID}`).className).toContain('border-slate-800 hover:bg-slate-900');
    expect(button('openapi-new').className).toContain('border-slate-700 text-slate-200 hover:bg-slate-800');
    await act(async () => button('openapi-new').click());
    await waitFor(() => expect(el('openapi-form')).toBeTruthy());
    expect(currentSearch()).toBe('');
    expect(el('openapi-report')).toBeNull();
    expect(button('openapi-new').className).toContain('bg-violet-950/60 border-violet-600 text-violet-200');
  });

  it('are said to be loading until they arrive, and to be none when there is none', async () => {
    let release: (r: unknown) => void = () => {};
    mockBackend({ routes: { 'GET /openapi/runs$': () => new Promise((resolve) => (release = resolve)) } });
    await openPage();
    expect(text('openapi-runs')).toContain('Loading…');
    expect(text('openapi-runs')).not.toContain('No run yet.');
    await act(async () => release({ runs: [] }));
    await waitFor(() => expect(text('openapi-runs')).toContain('No run yet.'));
    expect(text('openapi-runs')).not.toContain('Loading…');
  });

  it('are said not to have been read when they could not be, and not to be none', async () => {
    mockBackend({ routes: { 'GET /openapi/runs$': () => failure(500, 'INTERNAL_ERROR', 'busy') } });
    await openPage();
    await waitFor(() => expect(el('openapi-runs-error')).toBeTruthy());
    expect(text('openapi-runs-error')).toContain('The runs could not be read: ');
    expect(text('openapi-runs-error')).toContain('busy');
    expect(text('openapi-runs')).not.toContain('No run yet.');
    expect(text('openapi-runs')).not.toContain('Loading…');
    // The form is still there: not being able to list the past does not stop a new test.
    expect(el('openapi-form')).toBeTruthy();
  });

  it('are marked by how each ended', async () => {
    mockBackend({
      runs: [runRow({ run_state: 'COMPLETED' }), runRow({ id: 'r2', run_state: 'FAILED' }), runRow({ id: 'r3', run_state: 'RUNNING' }), runRow({ id: 'r4', run_state: null })],
    });
    await openPage();
    await waitFor(() => expect(el('openapi-run-r2')).toBeTruthy());
    const dot = (id: string) => el(`openapi-run-${id}`)!.querySelector('span[aria-hidden]')!.className;
    expect(dot(RUN_ID)).toContain('bg-emerald-400');
    expect(dot('r2')).toContain('bg-rose-400');
    expect(dot('r3')).toContain('animate-pulse');
    expect(dot('r4')).toContain('bg-slate-500');
  });

  it('are left behind when another project is chosen: its address does not name a run of the other', async () => {
    mockBackend({ projects: [PROJECT_A, PROJECT_B], runs: RUNS, reports: { [RUN_ID]: () => reportFor() } });
    await openPage(PROJECT_A, [`/openapi-tests?run=${RUN_ID}`]);
    await waitFor(() => expect(el('openapi-report')).toBeTruthy());
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(el('openapi-form')).toBeTruthy());
    expect(currentSearch()).toBe('');
    expect(probe().getAttribute('data-action')).toBe('REPLACE');
    expect(screen.queryByText(/Couldn't load the report/)).toBeNull();
  });

  it('take away the cases shown for the other project when another is chosen', async () => {
    mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    await openPage();
    await pasteSpec();
    await showCases();
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(el('aiqa-openapi-page')).toBeTruthy());
    expect(el('openapi-preview')).toBeNull();
  });
});
