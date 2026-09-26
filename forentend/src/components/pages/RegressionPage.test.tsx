/**
 * Phase 8C tests for the Scheduled Regression page — the cases a project has run once, run again on a schedule and compared with the last
 * time. Exercised through the real `AppShell` tree; `fetch` is mocked only at the transport boundary, so what is asserted is what the
 * page really sends and shows.
 */
import React from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation, useNavigationType } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import { OTHER_RUN_ID, OTHER_SCHEDULE_ID, RUN_ID, SCHEDULE_ID, accountFor, reportFor, scheduleFor, scopeFor, summary } from '../regression/regressionTestData';
import { backend, failure, requests, textResponse, type Handler } from '../autopilot/autopilotTestBackend';
import type { Project, RegressionReport, RegressionRunAccount, RegressionSchedule } from '../../types/contract';

const RAW_KEY = 'aiqa_regressionpagetest0';
const PROJECT_A: Project = { id: 'proj-a', name: 'Project Alpha', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-b', name: 'Project Beta', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };

interface Setup {
  schedules?: RegressionSchedule[];
  runs?: RegressionRunAccount[];
  reports?: Record<string, RegressionReport | Handler>;
  routes?: Record<string, Handler>;
  projects?: Project[];
}

function mockBackend({ schedules = [], runs = [accountFor()], reports = {}, routes = {}, projects = [PROJECT_A] }: Setup = {}) {
  const perRun: Record<string, Handler> = {};
  for (const [id, value] of Object.entries(reports)) perRun[`GET /regression/runs/${id}$`] = typeof value === 'function' ? value : () => value;
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
    'GET /regression/schedules$': () => ({ schedules }),
    'GET /regression/scope': () => scopeFor(),
    'GET /test-suites$': () => [{ id: 'suite-1', name: 'Smoke' }],
    [`GET /regression/schedules/${SCHEDULE_ID}/runs$`]: () => ({ runs }),
    [`GET /regression/schedules/${OTHER_SCHEDULE_ID}/runs$`]: () => ({ runs: [] }),
    ...perRun,
    ...routes,
  });
}

const LocationProbe: React.FC = () => {
  const location = useLocation();
  const action = useNavigationType();
  return <div data-testid="location-probe" data-pathname={location.pathname} data-search={location.search} data-action={action} />;
};

function renderApp(initialEntries: string[] = ['/regression']) {
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
  await waitFor(() => expect(el('aiqa-regression-page')).toBeTruthy());
}

const fillName = (name: string) => act(async () => void fireEvent.change(el('regression-name')!, { target: { value: name } }));

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  setApiKey(RAW_KEY);
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe('RegressionPage — reaching it', () => {
  it('a browser with no API key never reaches the backend and shows the sign-in state, not the page', async () => {
    localStorage.clear();
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(el('aiqa-regression-page')).toBeNull();
  });

  it('has its own entry in the navigation, which leads to /regression', async () => {
    mockBackend();
    renderApp(['/dashboard']);
    await verifyKey();
    const nav = el('nav-item-regression');
    expect(nav).toBeTruthy();
    expect(nav!.textContent).toContain('Scheduled Regression');
    await act(async () => {
      nav!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    expect(currentPath()).toBe('/regression');
    await waitFor(() => expect(el('aiqa-regression-page')).toBeTruthy());
  });

  it('with no project selected it asks for one and asks the service nothing', async () => {
    const fetchMock = mockBackend();
    renderApp();
    await verifyKey();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(requests(fetchMock, 'GET /regression')).toHaveLength(0);
    expect(el('regression-form')).toBeNull();
  });

  it('names the project in its header and says what it does', async () => {
    mockBackend();
    await openPage();
    expect(text('regression-header')).toContain('Scheduled Regression');
    expect(text('regression-header')).toContain('The cases of Project Alpha that have been run once, run again on a schedule');
    expect(text('regression-header')).toContain('no AI, nothing compiled again');
  });
});

describe('RegressionPage — the schedules of the project', () => {
  it('starts with the form to make one, and says there is none, when there is none', async () => {
    const fetchMock = mockBackend();
    await openPage();
    await waitFor(() => expect(text('regression-schedules')).toContain('No schedule yet.'));
    expect(el('regression-form')).toBeTruthy();
    expect(button('regression-new').getAttribute('aria-pressed')).toBe('true');
    expect(requests(fetchMock, 'GET /regression/schedules$')[0].path).toBe('/api/v1/projects/proj-a/regression/schedules');
  });

  it('says they are loading until they arrive', async () => {
    let release: (r: unknown) => void = () => {};
    mockBackend({ routes: { 'GET /regression/schedules$': () => new Promise((resolve) => (release = resolve)) } });
    await openPage();
    expect(text('regression-schedules')).toContain('Loading…');
    expect(text('regression-schedules')).not.toContain('No schedule yet.');
    await act(async () => release({ schedules: [] }));
    await waitFor(() => expect(text('regression-schedules')).toContain('No schedule yet.'));
  });

  it('says they could not be read when they could not be, and not that there are none', async () => {
    mockBackend({ routes: { 'GET /regression/schedules$': () => failure(500, 'INTERNAL_ERROR', 'busy') } });
    await openPage();
    await waitFor(() => expect(el('regression-schedules-error')).toBeTruthy());
    expect(text('regression-schedules-error')).toContain('The schedules could not be read: ');
    expect(text('regression-schedules-error')).toContain('busy');
    expect(text('regression-schedules')).not.toContain('No schedule yet.');
    expect(el('regression-form'), 'a new one can still be made').toBeTruthy();
  });

  it('lists each with how often it runs, when next, and how its last run came out', async () => {
    mockBackend({
      schedules: [
        scheduleFor(),
        scheduleFor({ id: 'p', name: 'Paused one', enabled: false, last_run: null }),
        scheduleFor({ id: 'r', name: 'Going now', every_minutes: 60, last_run: accountFor({ verdict: 'RUNNING', final: false }) }),
        scheduleFor({ id: 'g', name: 'Green', last_run: accountFor({ verdict: 'PASSING' }) }),
      ],
    });
    await openPage();
    await waitFor(() => expect(el(`regression-schedule-${SCHEDULE_ID}`)).toBeTruthy());
    expect(text(`regression-schedule-${SCHEDULE_ID}`)).toContain('Nightly');
    expect(text(`regression-schedule-${SCHEDULE_ID}`)).toMatch(/every day, next \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/);
    expect(text('regression-schedule-p')).toContain('paused: runs only when asked');
    expect(text('regression-schedule-r')).toContain('every hour');
    const dot = (id: string) => el(`regression-schedule-${id}`)!.querySelector('span[aria-hidden]')!.className;
    expect(dot(SCHEDULE_ID)).toContain('bg-rose-400');
    expect(dot('p')).toContain('bg-slate-500');
    expect(dot('r')).toContain('animate-pulse');
    expect(dot('g')).toContain('bg-emerald-400');
  });
});

describe('RegressionPage — making a schedule', () => {
  it('says what it would repeat, and asks again when the suite is chosen', async () => {
    const fetchMock = mockBackend();
    await openPage();
    await waitFor(() => expect(text('regression-scope-text')).toBe('3 cases will be repeated; 1 case cannot.'));
    expect(requests(fetchMock, 'GET /regression/scope')[0].query.suite_id).toBeUndefined();
    await waitFor(() => expect(Array.from((el('regression-suite') as HTMLSelectElement).options).map((o) => o.textContent)).toContain('Suite: Smoke'));
    await act(async () => void fireEvent.change(el('regression-suite')!, { target: { value: 'suite-1' } }));
    await waitFor(() => expect(requests(fetchMock, 'GET /regression/scope').at(-1)?.query.suite_id).toBe('suite-1'));
  });

  it('says why nothing could be looked at when the service could not', async () => {
    mockBackend({ routes: { 'GET /regression/scope': () => failure(500, 'INTERNAL_ERROR', 'scope trouble') } });
    await openPage();
    await waitFor(() => expect(text('regression-scope')).toContain('The cases could not be looked at'));
    expect(text('regression-scope')).toContain('scope trouble');
  });

  it('lists what is wrong, and asks nothing until it is right', async () => {
    const fetchMock = mockBackend();
    await openPage();
    expect(text('regression-problems')).toContain('Give the schedule a name.');
    await act(async () => void fireEvent.submit(el('regression-form')!));
    expect(requests(fetchMock, 'POST /regression/schedules$')).toHaveLength(0);
  });

  it('makes it, opens it, and shows it in the list', async () => {
    let made: RegressionSchedule[] = [];
    const fetchMock = mockBackend({
      routes: {
        'GET /regression/schedules$': () => ({ schedules: made }),
        'POST /regression/schedules$': (req) => {
          const s = scheduleFor({ name: req.body.name, every_minutes: req.body.every_minutes, last_run: null });
          made = [s];
          return s;
        },
      },
    });
    await openPage();
    await fillName('  Nightly  ');
    await act(async () => void fireEvent.change(el('regression-every')!, { target: { value: '60' } }));
    await act(async () => void fireEvent.change(el('regression-concurrency')!, { target: { value: '3' } }));
    await act(async () => void fireEvent.change(el('regression-suite')!, { target: { value: 'suite-1' } }));
    await act(async () => button('regression-create').click());
    await waitFor(() => expect(currentSearch()).toBe(`?schedule=${SCHEDULE_ID}`));
    const sent = requests(fetchMock, 'POST /regression/schedules$');
    expect(sent).toHaveLength(1);
    expect(sent[0].path).toBe('/api/v1/projects/proj-a/regression/schedules');
    expect(sent[0].body).toEqual({ name: 'Nightly', every_minutes: 60, concurrency: 3, enabled: true, suite_id: 'suite-1' });
    await waitFor(() => expect(el('regression-detail')).toBeTruthy());
    expect(text('regression-detail')).toContain('Nightly');
    expect(el('regression-form')).toBeNull();
    await waitFor(() => expect(el(`regression-schedule-${SCHEDULE_ID}`)).toBeTruthy());
  });

  it('says why it could not be made, and stays on the form as it was', async () => {
    mockBackend({ routes: { 'POST /regression/schedules$': () => failure(409, 'CONFLICT', 'a project may have at most 20 schedules') } });
    await openPage();
    await fillName('One too many');
    await act(async () => button('regression-create').click());
    await waitFor(() => expect(el('regression-create-error')).toBeTruthy());
    expect(text('regression-create-error')).toContain('at most 20 schedules');
    expect((el('regression-name') as HTMLInputElement).value).toBe('One too many');
    expect(currentSearch()).toBe('');
    expect(button('regression-create').disabled).toBe(false);
  });
});

describe('RegressionPage — a schedule', () => {
  const open = (over: Partial<RegressionSchedule> = {}, setup: Setup = {}) => {
    const fetchMock = mockBackend({ schedules: [scheduleFor(over)], ...setup });
    return fetchMock;
  };

  it('says how often it runs, what it repeats, and when it next runs, with the runs it has made', async () => {
    open({ last_note: '1 case(s) could not be repeated' });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(el('regression-detail')).toBeTruthy());
    expect(text('regression-facts')).toMatch(/^every day · every case of the project that has been run once · 2 at a time · next run \d{4}-\d{2}-\d{2}/);
    expect(text('regression-note')).toBe('1 case(s) could not be repeated');
    await waitFor(() => expect(el('regression-history')).toBeTruthy());
    expect(text(`regression-run-${RUN_ID}`)).toContain('schedule');
    expect(text(`regression-run-${RUN_ID}`)).toContain('Regressions found');
    expect(text(`regression-run-${RUN_ID}`)).toContain('2 passed · 2 failed · 1 regressed');
    expect(button('regression-new').getAttribute('aria-pressed')).toBe('false');
    expect(button(`regression-schedule-${SCHEDULE_ID}`).getAttribute('aria-current')).toBe('true');
  });

  it('says a schedule of one suite repeats that suite, and one that is paused runs only when asked', async () => {
    open({ suite_id: 'suite-1', enabled: false });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(el('regression-detail')).toBeTruthy());
    expect(text('regression-facts')).toContain('the cases of one suite');
    expect(text('regression-facts')).toContain('runs only when asked');
    expect(text('regression-detail')).toContain('Paused');
    expect(text('regression-toggle')).toContain('Resume');
  });

  it('says there is no run yet, and that the runs could not be read', async () => {
    open({}, { runs: [] });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(text('regression-history-empty')).toBe('No run yet.'));
    cleanup();
    mockBackend({ schedules: [scheduleFor()], routes: { [`GET /regression/schedules/${SCHEDULE_ID}/runs$`]: () => failure(500, 'INTERNAL_ERROR', 'no history today') } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(el('regression-history-error')).toBeTruthy());
    expect(text('regression-history-error')).toContain('no history today');
  });

  it('says so when the schedule of the address is not in the project (any more)', async () => {
    mockBackend({ schedules: [scheduleFor()] });
    await openPage(PROJECT_A, [`/regression?schedule=${OTHER_SCHEDULE_ID}`]);
    await waitFor(() => expect(text('regression-missing')).toBe('This schedule is not in this project (any more).'));
    expect(el('regression-detail')).toBeNull();
  });

  it('runs at once when asked, and opens the report of the run', async () => {
    const fetchMock = open({}, { reports: { [OTHER_RUN_ID]: () => reportFor({ id: OTHER_RUN_ID, trigger: 'MANUAL' }) }, routes: { [`POST /regression/schedules/${SCHEDULE_ID}/run$`]: () => ({ id: OTHER_RUN_ID, run_id: 'x', cases: 3, skipped: 1 }) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(button('regression-run-now').disabled).toBe(false));
    await act(async () => button('regression-run-now').click());
    await waitFor(() => expect(currentSearch()).toBe(`?schedule=${SCHEDULE_ID}&run=${OTHER_RUN_ID}`));
    expect(requests(fetchMock, `POST /regression/schedules/${SCHEDULE_ID}/run$`)[0].path).toBe(`/api/v1/projects/proj-a/regression/schedules/${SCHEDULE_ID}/run`);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    expect(text('regression-report-time')).toContain('run by hand');
  });

  it('says why a run could not be made, and stays where it is', async () => {
    open({}, { routes: { [`POST /regression/schedules/${SCHEDULE_ID}/run$`]: () => failure(409, 'CONFLICT', 'the run before this one has not ended yet') } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(button('regression-run-now').disabled).toBe(false));
    await act(async () => button('regression-run-now').click());
    await waitFor(() => expect(el('regression-action-error')).toBeTruthy());
    expect(text('regression-action-error')).toContain('has not ended yet');
    expect(currentSearch()).toBe(`?schedule=${SCHEDULE_ID}`);
    expect(button('regression-run-now').disabled).toBe(false);
  });

  it('cannot be asked to run while one of its runs is going, and says so', async () => {
    open({}, { runs: [accountFor({ verdict: 'RUNNING', final: false })] });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(button('regression-run-now').disabled).toBe(true));
    expect(button('regression-run-now').getAttribute('title')).toBe('The run before has not ended');
    expect(text('regression-detail')).toContain('A run is going');
  });

  it('is paused, and started again, by one button', async () => {
    const fetchMock = open({}, { routes: { [`PATCH /regression/schedules/${SCHEDULE_ID}$`]: (req) => scheduleFor({ enabled: req.body.enabled }) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(text('regression-toggle')).toContain('Pause'));
    await act(async () => button('regression-toggle').click());
    await waitFor(() => expect(requests(fetchMock, `PATCH /regression/schedules/${SCHEDULE_ID}$`)).toHaveLength(1));
    expect(requests(fetchMock, `PATCH /regression/schedules/${SCHEDULE_ID}$`)[0].body).toEqual({ enabled: false });
    await waitFor(() => expect(requests(fetchMock, 'GET /regression/schedules$').length).toBeGreaterThan(1));
  });

  it('is removed only after the person says so, and then the page goes back to making one', async () => {
    const fetchMock = open({}, { routes: { [`DELETE /regression/schedules/${SCHEDULE_ID}$`]: () => new Response(null, { status: 204 }) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(el('regression-delete')).toBeTruthy());
    await act(async () => button('regression-delete').click());
    expect(text('regression-delete-confirm')).toContain('Remove it and its history?');
    await act(async () => button('regression-delete-no').click());
    expect(el('regression-delete-confirm')).toBeNull();
    expect(requests(fetchMock, `DELETE /regression/schedules/${SCHEDULE_ID}$`)).toHaveLength(0);
    await act(async () => button('regression-delete').click());
    await act(async () => button('regression-delete-yes').click());
    await waitFor(() => expect(requests(fetchMock, `DELETE /regression/schedules/${SCHEDULE_ID}$`)).toHaveLength(1));
    await waitFor(() => expect(el('regression-form')).toBeTruthy());
    expect(currentSearch()).toBe('');
  });

  it('says why a change or a removal could not be made', async () => {
    open({}, { routes: { [`PATCH /regression/schedules/${SCHEDULE_ID}$`]: () => failure(403, 'FORBIDDEN', 'a viewer cannot change a schedule') } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(button('regression-toggle')).toBeTruthy());
    await act(async () => button('regression-toggle').click());
    await waitFor(() => expect(el('regression-action-error')).toBeTruthy());
    expect(text('regression-action-error')).toContain('a viewer cannot change a schedule');
  });
});

describe('RegressionPage — the report of a run', () => {
  const withReport = (over: Partial<Setup> = {}) => mockBackend({ schedules: [scheduleFor()], reports: { [RUN_ID]: () => reportFor() }, ...over });

  it('opens on the run in the address, reads it and shows it beside its schedule', async () => {
    const fetchMock = withReport();
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    expect(requests(fetchMock, `GET /regression/runs/${RUN_ID}$`)[0].path).toBe(`/api/v1/projects/proj-a/regression/runs/${RUN_ID}`);
    expect(text('regression-verdict')).toBe('Regressions found');
    expect(el('regression-detail')).toBeTruthy();
    expect(button(`regression-run-${RUN_ID}`).getAttribute('aria-current')).toBe('true');
  });

  it('opens on a run alone, when the address names no schedule', async () => {
    withReport();
    await openPage(PROJECT_A, [`/regression?run=${RUN_ID}`]);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    expect(el('regression-detail')).toBeNull();
    expect(el('regression-form')).toBeNull();
  });

  it('is opened by choosing a run of the history, and left when another schedule is chosen', async () => {
    withReport({ schedules: [scheduleFor(), scheduleFor({ id: OTHER_SCHEDULE_ID, name: 'Weekly' })] });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}`]);
    await waitFor(() => expect(el(`regression-run-${RUN_ID}`)).toBeTruthy());
    await act(async () => button(`regression-run-${RUN_ID}`).click());
    await waitFor(() => expect(currentSearch()).toBe(`?schedule=${SCHEDULE_ID}&run=${RUN_ID}`));
    expect(probe().getAttribute('data-action')).toBe('PUSH');
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    await act(async () => button(`regression-schedule-${OTHER_SCHEDULE_ID}`).click());
    await waitFor(() => expect(currentSearch()).toBe(`?schedule=${OTHER_SCHEDULE_ID}`));
    expect(el('regression-report'), 'another schedule, another history: the open report is not its').toBeNull();
    await act(async () => button('regression-new').click());
    await waitFor(() => expect(el('regression-form')).toBeTruthy());
    expect(currentSearch()).toBe('');
  });

  it('says it is loading until it arrives', async () => {
    let release: (r: unknown) => void = () => {};
    withReport({ reports: { [RUN_ID]: () => new Promise((resolve) => (release = resolve)) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(screen.getByText('Loading the report')).toBeTruthy());
    await act(async () => release(reportFor()));
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
  });

  it('is read again while the run is going and then no more', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let reads = 0;
    withReport({ reports: { [RUN_ID]: () => reportFor({ final: ++reads >= 2, verdict: reads >= 2 ? 'PASSING' : 'RUNNING' }) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(text('regression-verdict')).toBe('Running'));
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    await waitFor(() => expect(text('regression-verdict')).toBe('All passing'));
    const after = reads;
    await act(async () => void (await vi.advanceTimersByTimeAsync(30_000)));
    expect(reads).toBe(after);
  });

  it('says the key’s scope does not let it see the run, and offers to try again after another failure', async () => {
    withReport({ reports: { [RUN_ID]: () => failure(403, 'FORBIDDEN', 'not in scope') } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(screen.getByText('Not permitted to view this run')).toBeTruthy());
    cleanup();
    let reads = 0;
    withReport({ reports: { [RUN_ID]: () => (++reads === 1 ? failure(500, 'INTERNAL_ERROR', 'busy') : reportFor()) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(screen.getByText("Couldn't load the report")).toBeTruthy());
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /retry/i })));
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
  });

  it('copies the report as Markdown, or shows it to copy by hand, or says it could not be read', async () => {
    const markdown = '# Regression run: Nightly\n';
    withReport({ reports: { [RUN_ID]: (req) => (req.query.format === 'markdown' ? textResponse(200, markdown) : reportFor()) } });
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: write }, configurable: true });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    await act(async () => button('regression-copy').click());
    await waitFor(() => expect(text('regression-copy')).toBe('Copied'));
    expect(write).toHaveBeenCalledWith(markdown);
    cleanup();
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) }, configurable: true });
    withReport({ reports: { [RUN_ID]: (req) => (req.query.format === 'markdown' ? textResponse(200, markdown) : reportFor()) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    await act(async () => button('regression-copy').click());
    await waitFor(() => expect(el('regression-markdown')).toBeTruthy());
    expect((el('regression-markdown') as HTMLTextAreaElement).value).toBe(markdown);
    cleanup();
    withReport({ reports: { [RUN_ID]: (req) => (req.query.format === 'markdown' ? failure(500, 'INTERNAL_ERROR', 'no text today') : reportFor()) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    await act(async () => button('regression-copy').click());
    await waitFor(() => expect(el('regression-copy-error')).toBeTruthy());
    expect(text('regression-copy-error')).toContain('no text today');
    expect(el('regression-report')).toBeTruthy();
  });

  it('keeps the report it has when a later look fails, and says so', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let reads = 0;
    withReport({ reports: { [RUN_ID]: () => (++reads === 2 ? failure(500, 'INTERNAL_ERROR', 'busy') : reportFor({ final: false, verdict: 'RUNNING', summary: summary({ pending: 1 }) })) } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    await waitFor(() => expect(el('regression-look-error')).toBeTruthy());
    expect(text('regression-look-error')).toContain('busy');
    expect(el('regression-report')).toBeTruthy();
  });
});

describe('RegressionPage — another project', () => {
  it('leaves the open schedule and run behind when another project is chosen, and not when the first one is', async () => {
    mockBackend({ projects: [PROJECT_A, PROJECT_B], schedules: [scheduleFor()], reports: { [RUN_ID]: () => reportFor() } });
    await openPage(PROJECT_A, [`/regression?schedule=${SCHEDULE_ID}&run=${RUN_ID}`]);
    await waitFor(() => expect(el('regression-report')).toBeTruthy());
    expect(currentSearch(), 'the first choice of a project keeps the address').toBe(`?schedule=${SCHEDULE_ID}&run=${RUN_ID}`);
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(el('regression-form')).toBeTruthy());
    expect(currentSearch()).toBe('');
    expect(probe().getAttribute('data-action')).toBe('REPLACE');
    expect(el('regression-report')).toBeNull();
  });
});
