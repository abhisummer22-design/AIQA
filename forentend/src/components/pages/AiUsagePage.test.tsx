/**
 * Phase 8A tests for the AI usage page — what the AI calls came to. Exercised through the real `AppShell` tree, like
 * `AutopilotPage.test.tsx`; `fetch` is mocked only at the transport boundary, so what is asserted is what the page really sends and
 * shows.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import { everywhereReport, tally, usageReport } from '../ai-usage/aiUsageTestData';
import { backend, failure, requests, type Handler } from '../autopilot/autopilotTestBackend';
import type { AiUsageReport, Project } from '../../types/contract';

const RAW_KEY = 'aiqa_aiusagetest000000000';
const PROJECT_A: Project = { id: 'proj-a', name: 'Project Alpha', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-b', name: 'Project Beta', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };
const CUSTOM = 'custom-ai-11111111-1111-4111-8111-111111111111';

interface Setup {
  routes?: Record<string, Handler>;
  projects?: Project[];
}

function mockBackend({ routes = {}, projects = [PROJECT_A] }: Setup = {}) {
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
    'GET /ai-usage': () => usageReport(),
    'GET /ai/usage': () => everywhereReport(),
    'GET /ai/status': () => ({ providers: [{ id: CUSTOM, name: 'Home GPU' }], settings: {}, routing_preview: null }),
    ...routes,
  });
}

const LocationProbe: React.FC = () => {
  const location = useLocation();
  return <div data-testid="location-probe" data-pathname={location.pathname} />;
};

function renderApp(initialEntries: string[] = ['/ai-usage']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <LocationProbe />
      <AppShell />
    </MemoryRouter>,
  );
}

const currentPath = () => screen.getByTestId('location-probe').getAttribute('data-pathname') ?? '';
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

/** The page, with the key verified, the project chosen and its usage on screen. */
async function openPage(project: Project = PROJECT_A) {
  renderApp();
  await verifyKey();
  await openAndSelectProject(project);
  await waitFor(() => expect(el('aiqa-ai-usage-page')).toBeTruthy());
}

const usageShown = () => waitFor(() => expect(el('ai-usage-report')).toBeTruthy());

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  setApiKey(RAW_KEY);
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe('AiUsagePage — reaching it', () => {
  it('a browser with no API key never reaches the backend and shows the sign-in state, not the page', async () => {
    localStorage.clear();
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(el('aiqa-ai-usage-page')).toBeNull();
  });

  it('has its own entry in the navigation, which leads to /ai-usage', async () => {
    mockBackend();
    renderApp(['/dashboard']);
    await verifyKey();
    const nav = el('nav-item-ai-usage');
    expect(nav).toBeTruthy();
    expect(nav!.textContent).toContain('AI Usage & Cost');
    await act(async () => {
      nav!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    expect(currentPath()).toBe('/ai-usage');
    await waitFor(() => expect(el('aiqa-ai-usage-page')).toBeTruthy());
  });

  it('with no project selected it asks for one and reads nothing', async () => {
    const fetchMock = mockBackend();
    renderApp();
    await verifyKey();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(requests(fetchMock, 'GET /ai-usage')).toHaveLength(0);
    expect(requests(fetchMock, 'GET /ai/usage')).toHaveLength(0);
    expect(button('ai-usage-refresh').disabled).toBe(true);
  });

  it('can still show every project’s usage with no project selected', async () => {
    const fetchMock = mockBackend();
    renderApp();
    await verifyKey();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    await act(async () => button('ai-usage-scope-all').click());
    await usageShown();
    expect(requests(fetchMock, 'GET /ai/usage')).toHaveLength(1);
    expect(el('usage-by-project')).toBeTruthy();
  });
});

describe('AiUsagePage — a project’s usage', () => {
  it('reads the chosen project’s usage for a month on the viewer’s own clock, and shows it', async () => {
    vi.spyOn(Date.prototype, 'getTimezoneOffset').mockReturnValue(-330);
    const fetchMock = mockBackend();
    await openPage();
    await usageShown();
    const asked = requests(fetchMock, 'GET /ai-usage');
    expect(asked).toHaveLength(1);
    expect(asked[0].path).toBe('/api/v1/projects/proj-a/ai-usage');
    expect(asked[0].query).toEqual({ days: '30', tz_offset_minutes: '330' });
    expect(requests(fetchMock, 'GET /ai/usage')).toHaveLength(0);
    expect(text('usage-calls')).toContain('12');
    expect(text('usage-cost')).toContain('$0.001');
    expect(el('usage-by-provider')).toBeTruthy();
    expect(el('usage-by-task')).toBeTruthy();
    expect(el('usage-by-project')).toBeNull();
  });

  it('names the project in the header, and says which usage is shown', async () => {
    mockBackend();
    await openPage();
    await usageShown();
    expect(text('ai-usage-header')).toContain('AI Usage & Cost');
    expect(text('ai-usage-header')).toContain('What the AI calls made for Project Alpha came to');
    expect(button('ai-usage-scope-project').getAttribute('aria-pressed')).toBe('true');
    expect(button('ai-usage-scope-all').getAttribute('aria-pressed')).toBe('false');
  });

  it('offers a week, a month and a quarter, the month chosen, and reads again for another', async () => {
    const fetchMock = mockBackend();
    await openPage();
    await usageShown();
    expect(['ai-usage-days-7', 'ai-usage-days-30', 'ai-usage-days-90'].map((id) => button(id).getAttribute('aria-pressed'))).toEqual(['false', 'true', 'false']);
    await act(async () => button('ai-usage-days-7').click());
    await waitFor(() => expect(requests(fetchMock, 'GET /ai-usage')).toHaveLength(2));
    expect(requests(fetchMock, 'GET /ai-usage')[1].query.days).toBe('7');
    expect(['ai-usage-days-7', 'ai-usage-days-30', 'ai-usage-days-90'].map((id) => button(id).getAttribute('aria-pressed'))).toEqual(['true', 'false', 'false']);
    await act(async () => button('ai-usage-days-90').click());
    await waitFor(() => expect(requests(fetchMock, 'GET /ai-usage')).toHaveLength(3));
    expect(requests(fetchMock, 'GET /ai-usage')[2].query.days).toBe('90');
    await usageShown();
  });

  it('shows a loading state while it reads, not the figures of another period', async () => {
    let release: (r: unknown) => void = () => {};
    mockBackend({
      routes: { 'GET /ai-usage': (req) => (req.query.days === '7' ? new Promise((resolve) => (release = resolve)) : usageReport()) },
    });
    await openPage();
    await usageShown();
    await act(async () => button('ai-usage-days-7').click());
    await waitFor(() => expect(screen.getByText('Loading AI usage')).toBeTruthy());
    expect(el('ai-usage-report')).toBeNull();
    await act(async () => release(usageReport({ range: { days: 7, from: '2026-09-20', to: '2026-09-26', tz_offset_minutes: 0 } })));
    await usageShown();
    expect(screen.queryByText('Loading AI usage')).toBeNull();
  });

  it('reads another project’s usage when another project is chosen', async () => {
    const fetchMock = mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    await openPage();
    await usageShown();
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(requests(fetchMock, 'GET /ai-usage').map((r) => r.path)).toEqual(['/api/v1/projects/proj-a/ai-usage', '/api/v1/projects/proj-b/ai-usage']));
    await waitFor(() => expect(text('ai-usage-header')).toContain('Project Beta'));
    await usageShown();
  });

  it('names a provider added in Settings by the name it was given', async () => {
    mockBackend({
      routes: { 'GET /ai-usage': () => usageReport({ by_provider: [{ provider: CUSTOM, model: 'gpt-x', ...tally({ calls: 2, answered: 2 }) }] }) },
    });
    await openPage();
    await usageShown();
    await waitFor(() => expect(text('usage-by-provider')).toContain('Home GPU/gpt-x'));
    expect(text('usage-by-provider')).not.toContain(CUSTOM);
  });

  it('says so, and explains what is counted, when there were no AI calls in the period', async () => {
    mockBackend({ routes: { 'GET /ai-usage': () => usageReport({ totals: tally(), by_provider: [], by_task: [], notes: [] }) } });
    await openPage();
    await waitFor(() => expect(screen.getByText('No AI calls counted in the last 3 days')).toBeTruthy());
    expect(el('ai-usage-report')).toBeNull();
    expect(screen.getByText(/Every call to an AI provider is counted from the moment this version started/)).toBeTruthy();
  });

  it('says “day” for a period of one day', async () => {
    mockBackend({ routes: { 'GET /ai-usage': () => usageReport({ range: { days: 1, from: '2026-09-26', to: '2026-09-26', tz_offset_minutes: 0 }, totals: tally() }) } });
    await openPage();
    await waitFor(() => expect(screen.getByText('No AI calls counted in the last day')).toBeTruthy());
  });
});

describe('AiUsagePage — every project at once', () => {
  it('reads the platform-wide route and shows the split by project, by the projects’ names', async () => {
    const fetchMock = mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    await openPage();
    await usageShown();
    await act(async () => button('ai-usage-scope-all').click());
    await waitFor(() => expect(el('usage-by-project')).toBeTruthy());
    const asked = requests(fetchMock, 'GET /ai/usage');
    expect(asked).toHaveLength(1);
    expect(asked[0].path).toBe('/api/v1/ai/usage');
    expect(asked[0].query.days).toBe('30');
    expect(button('ai-usage-scope-all').getAttribute('aria-pressed')).toBe('true');
    expect(text('ai-usage-header')).toContain('What the AI calls of every project came to');
    const rows = Array.from(el('usage-by-project')!.querySelectorAll('tbody tr')).map((tr) => tr.querySelector('td')?.textContent);
    expect(rows).toEqual(['Project Alpha', 'Project Beta', 'Not made for a project']);
  });

  it('names a project that is no longer there by the start of its id', async () => {
    mockBackend({ projects: [PROJECT_A] });
    await openPage();
    await usageShown();
    await act(async () => button('ai-usage-scope-all').click());
    await waitFor(() => expect(el('usage-by-project')).toBeTruthy());
    const names = Array.from(el('usage-by-project')!.querySelectorAll('tbody tr')).map((tr) => tr.querySelector('td')?.textContent);
    expect(names).toEqual(['Project Alpha', 'A removed project (proj-b)', 'Not made for a project']);
  });

  it('explains that it needs an admin key not tied to one project when the key is not one, and goes back to the project’s own', async () => {
    mockBackend({ routes: { 'GET /ai/usage': () => failure(403, 'FORBIDDEN', 'the usage of every project is for a key that is not tied to one project') } });
    await openPage();
    await usageShown();
    await act(async () => button('ai-usage-scope-all').click());
    await waitFor(() => expect(screen.getByText('Not permitted to view every project')).toBeTruthy());
    expect(screen.getByText(/needs an admin key that is not tied to one project/)).toBeTruthy();
    await act(async () => button('ai-usage-scope-project').click());
    await usageShown();
    expect(screen.queryByText('Not permitted to view every project')).toBeNull();
  });
});

describe('AiUsagePage — when reading fails', () => {
  it('says the key’s scope does not let it see the project’s usage', async () => {
    mockBackend({ routes: { 'GET /ai-usage': () => failure(403, 'FORBIDDEN', 'resource belongs to a different project') } });
    await openPage();
    await waitFor(() => expect(screen.getByText('Not permitted to view this project')).toBeTruthy());
    expect(screen.getByText(/does not permit viewing this project's AI usage/)).toBeTruthy();
  });

  it('offers to try again after any other failure, and shows the usage when the second try works', async () => {
    let reads = 0;
    const fetchMock = mockBackend({
      routes: { 'GET /ai-usage': () => (++reads === 1 ? failure(500, 'INTERNAL_ERROR', 'the ledger is busy') : usageReport()) },
    });
    await openPage();
    await waitFor(() => expect(screen.getByText("Couldn't load the AI usage")).toBeTruthy());
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /retry|try again/i })));
    await usageShown();
    expect(requests(fetchMock, 'GET /ai-usage')).toHaveLength(2);
  });
});

describe('AiUsagePage — reading again', () => {
  it('has a Refresh button that reads again, keeps the figures on screen meanwhile, and says when it was read', async () => {
    let release: (r: unknown) => void = () => {};
    let reads = 0;
    const fetchMock = mockBackend({
      routes: { 'GET /ai-usage': () => (++reads === 1 ? usageReport() : new Promise((resolve) => (release = resolve))) },
    });
    await openPage();
    await usageShown();
    await waitFor(() => expect(text('ai-usage-updated')).toMatch(/^read \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/));
    expect(button('ai-usage-refresh').disabled).toBe(false);
    await act(async () => button('ai-usage-refresh').click());
    await waitFor(() => expect(requests(fetchMock, 'GET /ai-usage')).toHaveLength(2));
    // While it reads: the figures it has stay, and the button cannot be pressed again.
    expect(el('ai-usage-report')).toBeTruthy();
    expect(button('ai-usage-refresh').disabled).toBe(true);
    expect(button('ai-usage-refresh').querySelector('svg')?.getAttribute('class')).toContain('animate-spin');
    await act(async () => release(usageReport({ totals: tally({ calls: 99, answered: 99 }) })));
    await waitFor(() => expect(text('usage-calls')).toContain('99'));
    expect(button('ai-usage-refresh').disabled).toBe(false);
  });

  it('keeps the figures it has when reading again fails, and says so', async () => {
    let reads = 0;
    mockBackend({
      routes: { 'GET /ai-usage': () => (++reads === 1 ? usageReport() : failure(500, 'INTERNAL_ERROR', 'the ledger is busy')) },
    });
    await openPage();
    await usageShown();
    expect(el('ai-usage-error')).toBeNull();
    await act(async () => button('ai-usage-refresh').click());
    await waitFor(() => expect(el('ai-usage-error')).toBeTruthy());
    expect(text('ai-usage-error')).toContain('Reading the usage again failed');
    expect(text('ai-usage-error')).toContain('the ledger is busy');
    expect(el('ai-usage-report')).toBeTruthy();
    expect(text('usage-calls')).toContain('12');
  });
});

describe('AiUsagePage — what it shows', () => {
  it('lays out the tiles, the days and the three tables of the report', async () => {
    mockBackend();
    await openPage();
    await usageShown();
    for (const id of ['usage-calls', 'usage-tokens', 'usage-cost', 'usage-time', 'usage-notes', 'usage-days', 'usage-by-provider', 'usage-by-task', 'usage-legend']) {
      expect(el(id), id).toBeTruthy();
    }
    const report: AiUsageReport = usageReport();
    expect(el('usage-days')!.querySelectorAll('li[data-day]')).toHaveLength(report.by_day.length);
  });
});
