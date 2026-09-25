/**
 * Phase 7 tests for the Autopilot page — from the uploaded requirements to a report, in one run. Exercised through the real
 * `AppShell` tree, like `RequirementCoveragePage.test.tsx`; `fetch` is mocked only at the transport boundary, so what is asserted
 * is what the page really sends and shows.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import { DEFAULT_SELECTED_SKILL_IDS } from '../../services/skillRegistry';
import { RUN_ID, detail, estimate, report, run, steps, testCase } from '../autopilot/autopilotTestData';
import { backend, failure, requests, type Handler } from '../autopilot/autopilotTestBackend';
import type { AutopilotRunDetail, Project, RequirementDocumentRecord } from '../../types/contract';

const RAW_KEY = 'aiqa_autopilottest00000000';
const PROJECT_A: Project = { id: 'proj-a', name: 'Project Alpha', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-b', name: 'Project Beta', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };
const OTHER_RUN = '44444444-4444-4444-8444-444444444444';

const DOCS: RequirementDocumentRecord[] = [
  {
    id: 'doc-1',
    file_name: 'PRD.pdf',
    doc_type: 'PRD',
    char_count: 5000,
    requirements_count: 6,
    sections_count: 3,
    skipped_sections: [],
    furniture_lines: 0,
    created_at: '2026-09-01T00:00:00Z',
    updated_at: '2026-09-01T00:00:00Z',
  },
];

interface Setup {
  runs?: ReturnType<typeof run>[];
  details?: Record<string, AutopilotRunDetail | (() => AutopilotRunDetail | Response)>;
  routes?: Record<string, Handler>;
  projects?: Project[];
}

function mockBackend({ runs = [], details = {}, routes = {}, projects = [PROJECT_A] }: Setup = {}) {
  const perRun: Record<string, Handler> = {};
  for (const [id, value] of Object.entries(details)) perRun[`GET /autopilot/runs/${id}$`] = () => (typeof value === 'function' ? value() : value);
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
    'GET /autopilot/runs$': () => ({ runs }),
    'GET /requirements': () => ({ documents: DOCS, requirements: [] }),
    'GET /executions': () => [],
    'POST /autopilot/estimate': () => estimate(),
    'GET /report': () => report(),
    'GET /test-cases': () => [],
    ...perRun,
    ...routes,
  });
}

const LocationProbe: React.FC = () => {
  const location = useLocation();
  return <div data-testid="location-probe" data-pathname={location.pathname} data-search={location.search} />;
};

function renderApp(initialEntries: string[] = ['/autopilot']) {
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
  await waitFor(() => expect(el('aiqa-autopilot-page')).toBeTruthy());
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

describe('AutopilotPage — reaching it', () => {
  it('a browser with no API key never reaches the backend and shows the sign-in state, not the page', async () => {
    localStorage.clear();
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(el('aiqa-autopilot-page')).toBeNull();
  });

  it('has its own entry in the navigation, which leads to /autopilot', async () => {
    mockBackend();
    renderApp(['/dashboard']);
    await verifyKey();
    const nav = el('nav-item-autopilot');
    expect(nav).toBeTruthy();
    expect(nav!.textContent).toContain('Autopilot');
    await act(async () => {
      nav!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    expect(currentPath()).toBe('/autopilot');
    await waitFor(() => expect(el('aiqa-autopilot-page')).toBeTruthy());
  });

  it('with no project selected it asks for one and calls nothing of the autopilot’s', async () => {
    const fetchMock = mockBackend();
    renderApp();
    await verifyKey();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(requests(fetchMock, 'GET /autopilot')).toHaveLength(0);
  });

  it('says so when the key’s scope does not let it see the project’s runs', async () => {
    mockBackend({ routes: { 'GET /autopilot/runs$': () => failure(403, 'FORBIDDEN', 'project not in scope') } });
    await openPage();
    await waitFor(() => expect(screen.getByText('Not permitted to view this project')).toBeTruthy());
  });
});

describe('AutopilotPage — a project with no run yet', () => {
  it('shows the form to make one, and says there is no run', async () => {
    mockBackend();
    await openPage();
    await waitFor(() => expect(el('autopilot-wizard')).toBeTruthy());
    expect(text('autopilot-runs')).toContain('No run yet.');
    expect(button('autopilot-new').getAttribute('aria-pressed')).toBe('true');
    expect(currentSearch()).toBe('');
  });

  it('sends the person to the Test Case Generator to upload a document when the project has none', async () => {
    mockBackend({ routes: { 'GET /requirements': () => ({ documents: [], requirements: [] }) } });
    await openPage();
    await waitFor(() => expect(el('wizard-upload')).toBeTruthy());
    await act(async () => {
      button('wizard-upload').click();
    });
    expect(currentPath()).toBe('/test-generator');
  });

  it('makes a run from the form, opens it with its estimate, and starts it only after the estimate is accepted', async () => {
    const made = detail('AWAITING_APPROVAL');
    const started = detail('RUNNING', { REQUIREMENTS: 'RUNNING' }, { run: { current_step: 'REQUIREMENTS' } });
    let currentList: ReturnType<typeof run>[] = [];
    const fetchMock = mockBackend({
      routes: {
        'GET /autopilot/runs$': () => ({ runs: currentList }),
        'POST /autopilot/runs$': () => {
          currentList = [made.run];
          return made;
        },
        [`GET /autopilot/runs/${RUN_ID}$`]: () => (requests(fetchMock, `POST /autopilot/runs/${RUN_ID}/start`).length > 0 ? started : made),
        [`POST /autopilot/runs/${RUN_ID}/start`]: () => started,
      },
    });
    await openPage();
    await waitFor(() => expect(el('wizard-doc-doc-1')).toBeTruthy());
    fireEvent.change(el('wizard-base-url')!, { target: { value: 'https://shop.example.test' } });
    await act(async () => {
      button('wizard-create').click();
    });

    // The run is chosen (its id is in the address), shown with its estimate, and nothing has started.
    await waitFor(() => expect(el('run-estimate')).toBeTruthy());
    expect(currentSearch()).toBe(`?run=${RUN_ID}`);
    expect(requests(fetchMock, `POST /autopilot/runs/${RUN_ID}/start`)).toHaveLength(0);
    await waitFor(() => expect(text('autopilot-runs')).toContain('Autopilot: https://shop.example.test'));
    expect(text('autopilot-runs')).toContain('Awaiting cost approval');
    expect(el('autopilot-wizard')).toBeNull();

    const body = requests(fetchMock, 'POST /autopilot/runs$')[0].body;
    expect(body.base_url).toBe('https://shop.example.test');
    expect(body.roles).toEqual([{ name: 'anonymous', login_actions: [] }]);
    expect(body.skills.map((s: { id: string }) => s.id).sort()).toEqual([...DEFAULT_SELECTED_SKILL_IDS].sort());

    fireEvent.click(el('accept-estimate')!);
    await act(async () => {
      button('run-start').click();
    });
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    expect(requests(fetchMock, `POST /autopilot/runs/${RUN_ID}/start`)[0].body).toEqual({ accept_estimate: true });
    expect(text('run-state')).toBe('Running');
  });

  it('shows the backend’s refusal in the form when the run is refused, and stays on the form', async () => {
    mockBackend({ routes: { 'POST /autopilot/runs$': () => failure(409, 'CONFLICT', 'this project has another autopilot run that has not ended') } });
    await openPage();
    await waitFor(() => expect(el('wizard-doc-doc-1')).toBeTruthy());
    fireEvent.change(el('wizard-base-url')!, { target: { value: 'https://shop.example.test' } });
    await act(async () => {
      button('wizard-create').click();
    });
    await waitFor(() => expect(el('wizard-create-error')).toBeTruthy());
    expect(text('wizard-create-error')).toContain('another autopilot run that has not ended');
    expect(currentSearch()).toBe('');
  });
});

describe('AutopilotPage — coming back to runs', () => {
  it('lands on a run that has not ended, so a run that is going is never lost from sight', async () => {
    mockBackend({ runs: [run('RUNNING')], details: { [RUN_ID]: detail('RUNNING', { CRAWL: 'RUNNING' }, { run: { current_step: 'CRAWL' } }) } });
    await openPage();
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    expect(currentSearch()).toBe(`?run=${RUN_ID}`);
    expect(el('autopilot-wizard')).toBeNull();
    expect(text('run-running')).toContain('Working on: Crawl');
  });

  it('lands on the form, not on an old run, when every run has ended', async () => {
    mockBackend({ runs: [run('COMPLETED')], details: { [RUN_ID]: detail('COMPLETED') } });
    await openPage();
    await waitFor(() => expect(el('autopilot-wizard')).toBeTruthy());
    expect(currentSearch()).toBe('');
    expect(text('autopilot-runs')).toContain('Completed');
  });

  it('opens a run named in the address, whatever its state', async () => {
    mockBackend({ runs: [run('COMPLETED')], details: { [RUN_ID]: detail('COMPLETED') } });
    await openPage(PROJECT_A, [`/autopilot?run=${RUN_ID}`]);
    await waitFor(() => expect(el('run-state')).toBeTruthy());
    expect(text('run-state')).toBe('Completed');
    await waitFor(() => expect(el('report-headline')).toBeTruthy());
  });

  it('opens a run picked from the list, and goes back to the form on New run', async () => {
    mockBackend({
      runs: [run('COMPLETED', { id: OTHER_RUN, name: 'Older run' }), run('COMPLETED')],
      details: { [RUN_ID]: detail('COMPLETED'), [OTHER_RUN]: detail('COMPLETED', {}, { run: { id: OTHER_RUN, name: 'Older run' } }) },
    });
    await openPage();
    await waitFor(() => expect(el('autopilot-wizard')).toBeTruthy());
    await act(async () => {
      button(`autopilot-run-${OTHER_RUN}`).click();
    });
    await waitFor(() => expect(text('run-header')).toContain('Older run'));
    expect(currentSearch()).toBe(`?run=${OTHER_RUN}`);
    expect(button(`autopilot-run-${OTHER_RUN}`).getAttribute('aria-current')).toBe('true');
    expect(button('autopilot-new').getAttribute('aria-pressed')).toBe('false');

    await act(async () => {
      button('autopilot-new').click();
    });
    await waitFor(() => expect(el('autopilot-wizard')).toBeTruthy());
    expect(currentSearch()).toBe('');
  });

  it('does not send the person back to the run that is going when they press New run, but says it stands in the way', async () => {
    mockBackend({ runs: [run('RUNNING')], details: { [RUN_ID]: detail('RUNNING') } });
    await openPage();
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    await act(async () => {
      button('autopilot-new').click();
    });
    await waitFor(() => expect(el('autopilot-wizard')).toBeTruthy());
    expect(currentSearch()).toBe('');
    await new Promise((r) => setTimeout(r, 100));
    expect(el('autopilot-wizard')).toBeTruthy(); // still the form
    expect(text('wizard-blocked')).toContain('has a run that has not ended');
    expect(button('wizard-create').disabled).toBe(true);
    await act(async () => {
      button('wizard-open-blocking').click();
    });
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    expect(currentSearch()).toBe(`?run=${RUN_ID}`);
  });

  it('says a run that does not exist was not found, and can look again', async () => {
    let found = false;
    mockBackend({
      details: { [RUN_ID]: () => (found ? detail('COMPLETED') : failure(404, 'RESOURCE_NOT_FOUND', 'autopilot run not found in this project')) },
    });
    await openPage(PROJECT_A, [`/autopilot?run=${RUN_ID}`]);
    await waitFor(() => expect(screen.getByText(/Couldn't load the run/)).toBeTruthy());
    found = true;
    await act(async () => {
      screen.getByText('Retry').click();
    });
    await waitFor(() => expect(el('run-state')).toBeTruthy());
  });

  it('leaves a run behind when another project is chosen: its id in the address would be a 404 there', async () => {
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      routes: {
        'GET /autopilot/runs$': (req) => ({ runs: req.path.includes('proj-a') ? [run('COMPLETED')] : [] }),
        [`GET /autopilot/runs/${RUN_ID}$`]: (req) => (req.path.includes('proj-a') ? detail('COMPLETED') : failure(404, 'RESOURCE_NOT_FOUND', 'autopilot run not found in this project')),
      },
    });
    await openPage(PROJECT_A, [`/autopilot?run=${RUN_ID}`]);
    await waitFor(() => expect(text('run-state')).toBe('Completed'));
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(el('autopilot-wizard')).toBeTruthy());
    expect(currentSearch()).toBe('');
    expect(screen.queryByText(/Couldn't load the run/)).toBeNull();
  });

  it('shows nothing of one project’s run under another project', async () => {
    mockBackend({
      projects: [PROJECT_A, PROJECT_B],
      routes: { 'GET /autopilot/runs$': (req) => ({ runs: req.path.includes('proj-a') ? [run('COMPLETED', { name: 'Alpha run' })] : [] }) },
    });
    await openPage(PROJECT_A);
    await waitFor(() => expect(text('autopilot-runs')).toContain('Alpha run'));
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(text('autopilot-runs')).toContain('No run yet.'));
    expect(text('autopilot-runs')).not.toContain('Alpha run');
  });
});

describe('AutopilotPage — a run as it changes', () => {
  it('follows a running run to its end: the steps, the list and the report update by themselves', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let looks = 0;
    const running = detail('RUNNING', { REQUIREMENTS: 'DONE', CRAWL: 'RUNNING' }, { run: { current_step: 'CRAWL' } });
    const done = detail('COMPLETED', { REQUIREMENTS: 'DONE', CRAWL: 'DONE', REPORT: 'DONE' });
    mockBackend({
      runs: [run('RUNNING')],
      routes: {
        [`GET /autopilot/runs/${RUN_ID}$`]: () => (++looks < 3 ? running : done),
        'GET /autopilot/runs$': () => ({ runs: [looks < 3 ? run('RUNNING') : run('COMPLETED')] }),
      },
    });
    await openPage();
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    expect(el('step-CRAWL')!.getAttribute('data-state')).toBe('RUNNING');

    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    await waitFor(() => expect(text('run-state')).toBe('Completed'));
    expect(el('run-running')).toBeNull();
    expect(el('step-CRAWL')!.getAttribute('data-state')).toBe('DONE');
    await waitFor(() => expect(el('report-headline')).toBeTruthy());
    await waitFor(() => expect(text(`autopilot-run-${RUN_ID}`)).toContain('Completed'));
  });

  it('says a look that failed, keeps showing the run, and looks again', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let looks = 0;
    mockBackend({
      runs: [run('RUNNING')],
      routes: {
        [`GET /autopilot/runs/${RUN_ID}$`]: () => (++looks === 2 ? failure(500, 'INTERNAL_ERROR', 'the service is busy') : detail('RUNNING')),
      },
    });
    await openPage();
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
    await waitFor(() => expect(el('autopilot-look-error')).toBeTruthy());
    expect(text('autopilot-look-error')).toContain('the service is busy');
    expect(el('run-running')).toBeTruthy(); // what was shown stays
    await act(async () => void (await vi.advanceTimersByTimeAsync(10_000)));
    await waitFor(() => expect(el('autopilot-look-error')).toBeNull());
  });

  it('takes a run at the approval gate through approving its cases and letting it go on', async () => {
    const gate = detail('WAITING', { REQUIREMENTS: 'DONE', GENERATE: { state: 'DONE', detail: { suite_id: 's1', cases_created: 1, groups: [] } }, APPROVE: 'WAITING' }, { run: { current_step: 'APPROVE' } });
    const going = detail('RUNNING', { APPROVE: 'DONE', EXECUTE: 'RUNNING' }, { run: { current_step: 'EXECUTE' } });
    let cases = [testCase('c1', 'Login with a valid user', 'VALIDATED' as const)];
    let released = false;
    const fetchMock = mockBackend({
      runs: [gate.run],
      routes: {
        [`GET /autopilot/runs/${RUN_ID}$`]: () => (released ? going : gate),
        'GET /test-cases': () => cases,
        'POST /approve-cases': () => {
          cases = [testCase('c1', 'Login with a valid user', 'APPROVED' as const)];
          return { suite_id: 's1', approved: ['c1'], refused: [] };
        },
        'POST /continue': () => {
          released = true;
          return going;
        },
      },
    });
    await openPage();
    await waitFor(() => expect(el('autopilot-gate')).toBeTruthy());
    expect(requests(fetchMock, 'GET /test-cases')[0].query).toEqual({ suite_id: 's1' });
    await act(async () => {
      button('gate-approve-all').click();
    });
    await waitFor(() => expect(text('gate-tally')).toContain('1 approved'));
    await act(async () => {
      button('gate-continue').click();
    });
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    expect(el('autopilot-gate')).toBeNull();
    expect(text('run-running')).toContain('Working on: Execute');
  });

  it('starts a failed run again where it stopped, and follows it', async () => {
    const failed = detail('FAILED', { REQUIREMENTS: 'DONE', CRAWL: { state: 'FAILED', error: 'no page could be reached' } }, { run: { current_step: 'CRAWL', error: 'no page could be reached' } });
    const again = detail('RUNNING', { REQUIREMENTS: 'DONE', CRAWL: 'RUNNING' }, { run: { current_step: 'CRAWL' } });
    let resumed = false;
    const fetchMock = mockBackend({
      runs: [failed.run],
      routes: {
        [`GET /autopilot/runs/${RUN_ID}$`]: () => (resumed ? again : failed),
        'POST /resume': () => {
          resumed = true;
          return again;
        },
      },
    });
    await openPage(PROJECT_A, [`/autopilot?run=${RUN_ID}`]);
    await waitFor(() => expect(el('run-failed')).toBeTruthy());
    expect(text('run-failed')).toContain('no page could be reached');
    await act(async () => {
      button('run-resume').click();
    });
    await waitFor(() => expect(el('run-running')).toBeTruthy());
    expect(el('run-failed')).toBeNull();
    expect(requests(fetchMock, 'POST /resume')).toHaveLength(1);
  });

  it('shows the steps of a fresh run as waiting, before anything has started', async () => {
    mockBackend({ runs: [run('AWAITING_APPROVAL')], details: { [RUN_ID]: { ...detail('AWAITING_APPROVAL'), steps: steps() } } });
    await openPage();
    await waitFor(() => expect(el('run-estimate')).toBeTruthy());
    expect(Array.from(document.querySelectorAll('#autopilot-steps > li')).every((li) => li.getAttribute('data-state') === 'PENDING')).toBe(true);
  });
});
