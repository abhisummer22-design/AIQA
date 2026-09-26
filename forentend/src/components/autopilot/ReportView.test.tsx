/**
 * Phase 7 — the report of a run as a person reads it: how the requirements came out, what failed, what could not be answered, what
 * it cost, and the Markdown to paste. `fetch` is mocked at the transport boundary only.
 */
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../../services/aiqaApi';
import { ReportView } from './ReportView';
import { PROJECT_ID, RUN_ID, report } from './autopilotTestData';
import { backend, failure, requests, textResponse } from './autopilotTestBackend';
import type { AutopilotReport } from '../../types/contract';

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_reporttest000000000');
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

const REPORT_ROUTE = 'GET /report';
const text = (id: string) => document.getElementById(id)!.textContent ?? '';
const button = (id: string) => document.getElementById(id) as HTMLButtonElement;
const rowKeys = () => Array.from(document.querySelectorAll('#report-coverage-rows tr')).map((tr) => tr.id.replace('report-req-', ''));

async function showReport(r: AutopilotReport = report(), autoLoad = true, extra: Parameters<typeof backend>[0] = {}) {
  const fetchMock = backend({ [REPORT_ROUTE]: (req) => (req.query.format === 'markdown' ? textResponse(200, '# Autopilot report\n\n- 1 failed') : r), ...extra });
  const view = render(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad={autoLoad} version="v1" />);
  if (autoLoad) await waitFor(() => expect(document.getElementById('report-headline')).toBeTruthy());
  return { fetchMock, view };
}

describe('ReportView — loading', () => {
  it('reads the report of a run that has ended by itself, as JSON', async () => {
    const { fetchMock } = await showReport();
    const [call] = requests(fetchMock, REPORT_ROUTE);
    expect(call.path).toBe(`/api/v1/projects/${PROJECT_ID}/autopilot/runs/${RUN_ID}/report`);
    expect(call.query).toEqual({});
  });

  it('waits to be asked for the report of a run that is still going, and then shows it as not final', async () => {
    const { fetchMock } = await showReport(report({ final: false }), false);
    expect(requests(fetchMock, REPORT_ROUTE)).toHaveLength(0);
    expect(document.getElementById('report-headline')).toBeNull();
    await act(async () => {
      button('report-preview').click();
    });
    await waitFor(() => expect(document.getElementById('report-headline')).toBeTruthy());
    expect(text('report-not-final')).toContain('not final');
  });

  it('shows no “not final” mark on a final report', async () => {
    await showReport();
    expect(document.getElementById('report-not-final')).toBeNull();
  });

  it('says why the report could not be read', async () => {
    backend({ [REPORT_ROUTE]: () => failure(404, 'RESOURCE_NOT_FOUND', 'autopilot run not found in this project') });
    render(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad version="v1" />);
    await waitFor(() => expect(document.querySelector('[role="alert"]')).toBeTruthy());
    expect(document.querySelector('[role="alert"]')!.textContent).toBe('autopilot run not found in this project');
  });

  it('reads the report again when the run’s version changes, and not otherwise', async () => {
    const fetchMock = backend({ [REPORT_ROUTE]: () => report() });
    const { rerender } = render(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad version="v1" />);
    await waitFor(() => expect(document.getElementById('report-headline')).toBeTruthy());
    rerender(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad version="v1" />);
    expect(requests(fetchMock, REPORT_ROUTE)).toHaveLength(1);
    rerender(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad version="v2" />);
    await waitFor(() => expect(requests(fetchMock, REPORT_ROUTE)).toHaveLength(2));
  });
});

describe('ReportView — what it says', () => {
  it('opens with how the requirements came out, worst first, and how long the run took', async () => {
    await showReport();
    expect(text('report-headline')).toContain('Of 3 requirements: 1 failed, 1 without a case, 1 passed.');
    expect(text('report-headline')).toContain('took 6 min');
    expect(text('report-headline')).toContain('completed');
    expect(document.getElementById('report-headline')!.className).toContain('rose'); // something failed
  });

  it('reads amber when nothing failed but some requirement could not be answered, and green when all passed', async () => {
    const { view } = await showReport(report({ coverage_summary: { PASSED: 2, NOT_EXECUTED: 1 } }));
    expect(document.getElementById('report-headline')!.className).toContain('amber');
    view.unmount();
    await showReport(report({ coverage_summary: { PASSED: 3 } }));
    expect(document.getElementById('report-headline')!.className).toContain('emerald');
  });

  it('shows the counts of requirements, cases written, cases run, passed and failed', async () => {
    await showReport();
    expect(text('report-tile-requirements')).toContain('3');
    expect(text('report-tile-cases')).toContain('5');
    expect(text('report-tile-run')).toContain('4 of 4');
    expect(text('report-tile-passed')).toContain('3');
    expect(text('report-tile-failed')).toContain('1');
  });

  it('counts the cases that were really run, not those that only reached an outcome because the compiler refused them', async () => {
    await showReport(report({ execution: { ...report().execution, cases: 11, finished: 11, ran: 10 } }));
    expect(text('report-tile-run')).toContain('10 of 11');
  });

  it('falls back to the cases that finished when the report does not say how many ran', async () => {
    const { ran: _ran, ...noRan } = report().execution;
    await showReport(report({ execution: { ...noRan, cases: 11, finished: 9 } }));
    expect(text('report-tile-run')).toContain('9 of 11');
  });

  it('says the cases were not run when the run was not asked to run them', async () => {
    await showReport(report({ execution: { ...report().execution, enabled: false, finished: null, cases: null } }));
    expect(text('report-tile-run')).toContain('not asked for');
  });

  it('lists the cases that failed, with the requirement, the outcome and the reason', async () => {
    await showReport();
    const findings = text('report-findings');
    expect(findings).toContain('Cases that failed (1)');
    expect(findings).toContain('Login rejects a wrong password');
    expect(findings).toContain('FR-001');
    expect(findings).toContain('Failed');
    expect(findings).toContain('The error message did not appear');
  });

  it('lists the cases that could not give an answer, in the step’s own words for why', async () => {
    await showReport();
    const notRun = text('report-not-run');
    expect(notRun).toContain('Cases that could not give an answer (1)');
    expect(notRun).toContain('Export a report as CSV');
    expect(notRun).toContain('Cannot be run: not enough to compile');
    expect(notRun).toContain('the page for it was not crawled');
  });

  it('shows every attempt of a case that was run more than once', async () => {
    const twice = report().findings[0];
    await showReport(
      report({
        findings: [
          {
            ...twice,
            attempts: [
              { run_id: 'a', outcome: 'INFRA_ERROR', reason: 'the browser crashed' },
              { run_id: 'b', outcome: 'FAILED', reason: 'The error message did not appear' },
            ],
          },
        ],
      }),
    );
    expect(text('report-findings')).toContain('Infrastructure error: the browser crashed');
  });

  it('shows neither list when there is nothing in it', async () => {
    await showReport(report({ findings: [], not_run: [] }));
    expect(document.getElementById('report-findings')).toBeNull();
    expect(document.getElementById('report-not-run')).toBeNull();
  });

  it('shows the notes to read before trusting the numbers, and no list when there are none', async () => {
    const first = await showReport();
    expect(text('report-notes')).toContain('One requirement has no case: nothing tests it.');
    first.view.unmount();
    await showReport(report({ notes: [] }));
    expect(document.getElementById('report-notes')).toBeNull();
  });
});

describe('ReportView — a case with little to say, and a report on its way', () => {
  const bare = { case_id: 'k1', title: 'A bare case', requirement: null, outcome: 'FAILED', reason: null, run_id: null, attempts: [] };

  it('shows nothing extra for a case that names no requirement, gives no reason and was run once', async () => {
    await showReport(report({ findings: [{ ...bare, attempts: [{ run_id: 'a', outcome: 'FAILED', reason: null }] }] }));
    const item = document.querySelector('#report-findings li')!;
    expect(item.querySelectorAll('span.font-mono')).toHaveLength(1); // the outcome, and no requirement
    expect(item.querySelectorAll('p')).toHaveLength(0);
    expect(item.querySelectorAll('ul')).toHaveLength(0); // one attempt is the case itself
  });

  it('shows the attempts of a case that was run twice, each with its outcome and, when it has one, its reason', async () => {
    await showReport(
      report({
        findings: [
          {
            ...bare,
            attempts: [
              { run_id: 'a', outcome: 'INFRA_ERROR', reason: null },
              { run_id: 'b', outcome: 'FAILED', reason: null },
            ],
          },
        ],
      }),
    );
    expect(Array.from(document.querySelectorAll('#report-findings li li')).map((li) => li.textContent)).toEqual(['Infrastructure error', 'Failed']);
  });

  it('dims the report while a newer one is awaited', async () => {
    let release: (r: unknown) => void = () => {};
    let answers = 0;
    backend({ [REPORT_ROUTE]: () => (answers++ === 0 ? report() : new Promise((resolve) => (release = resolve))) });
    const { rerender } = render(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad version="v1" />);
    await waitFor(() => expect(document.getElementById('report-headline')).toBeTruthy());
    const section = () => document.getElementById('autopilot-report')!;
    expect(section().getAttribute('aria-busy')).toBe('false');
    rerender(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad version="v2" />);
    await waitFor(() => expect(section().getAttribute('aria-busy')).toBe('true'));
    expect(document.getElementById('report-headline')).toBeTruthy(); // the last one stays until the next is there
    await act(async () => release(report()));
    await waitFor(() => expect(section().getAttribute('aria-busy')).toBe('false'));
  });
});

describe('ReportView — the requirements table', () => {
  it('lists the requirements worst first, whatever order the backend sent them in', async () => {
    await showReport();
    expect(rowKeys()).toEqual(['FR-001', 'FR-003', 'FR-002']); // failed, no case, passed
  });

  it('shows each requirement’s counts and its verdict', async () => {
    await showReport();
    const row = document.getElementById('report-req-FR-001')!;
    expect(Array.from(row.querySelectorAll('td')).map((td) => td.textContent)).toEqual(['FR-001 functionalUsers can log in.', '2', '2', '1', '1', 'Failed']);
    expect(document.getElementById('report-req-FR-003')!.textContent).toContain('No case');
  });

  it('filters by verdict, with the number for each, and offers only the verdicts there are', async () => {
    await showReport();
    expect(button('report-filter-ALL').textContent).toBe('All 3');
    expect(button('report-filter-FAILED').textContent).toBe('Failed 1');
    expect(button('report-filter-INCONCLUSIVE')).toBeNull();
    fireEvent.click(button('report-filter-PASSED'));
    expect(rowKeys()).toEqual(['FR-002']);
    expect(button('report-filter-PASSED').getAttribute('aria-pressed')).toBe('true');
    // The filter that is on looks it, and the others do not.
    expect(button('report-filter-PASSED').className).toContain('bg-violet-950');
    expect(button('report-filter-ALL').className).not.toContain('bg-violet-950');
    expect(button('report-filter-FAILED').className).not.toContain('bg-violet-950');
    fireEvent.click(button('report-filter-ALL'));
    expect(rowKeys()).toHaveLength(3);
  });

  it('says so when the filter leaves nothing', async () => {
    await showReport(report({ coverage: [], coverage_summary: {} }));
    expect(document.getElementById('report-coverage')!.textContent).toContain('No requirement to show.');
    expect(text('report-headline')).toContain('No requirement was looked at.');
  });
});

describe('ReportView — cost and steps', () => {
  it('sets what was estimated beside what was measured', async () => {
    await showReport();
    const cost = text('report-cost');
    expect(cost).toContain('Estimated: 1–9 AI calls, 4 min–16 min, $0 (free providers).');
    expect(cost).toContain('Measured: 4200 tokens in, 1800 out, 2 min of AI time, 3 min running the cases.');
  });

  it('says who wrote the cases, and when a fallback took over', async () => {
    const { view } = await showReport();
    expect(text('report-providers')).toBe('Written by ollama/qwen3:14b (1 group).');
    view.unmount();
    await showReport(report({ generation: { ...report().generation, providers: { 'ollama/qwen3:14b': 2, 'groq/llama': 1 }, fallback_used: true } }));
    expect(text('report-providers')).toBe('Written by ollama/qwen3:14b (2 groups), groq/llama (1 group) — a fallback provider took over for some groups.');
  });

  it('shows a custom provider by the name it was given, in the list of who wrote the cases and in the notes', async () => {
    const id = 'custom-ai-0f1b4add-810d-44da-a369-4a33eda509d4';
    const named = report({
      generation: { ...report().generation, providers: { [`${id}/mock-autopilot-1`]: 2 }, fallback_used: true },
      notes: [`The AI router used a provider other than its first choice for some of the generation (${id}/mock-autopilot-1).`],
    });
    backend({ [REPORT_ROUTE]: () => named });
    render(<ReportView projectId={PROJECT_ID} runId={RUN_ID} autoLoad version="v1" providerName={(pid) => (pid === id ? 'Mock Autopilot' : pid)} />);
    await waitFor(() => expect(document.getElementById('report-headline')).toBeTruthy());
    expect(text('report-providers')).toBe('Written by Mock Autopilot/mock-autopilot-1 (2 groups) — a fallback provider took over for some groups.');
    expect(text('report-notes')).toContain('(Mock Autopilot/mock-autopilot-1)');
    expect(text('report-notes')).not.toContain('custom-ai-');
  });

  it('does not say who wrote the cases when the report does not say', async () => {
    const first = await showReport(report({ generation: { ...report().generation, providers: null, fallback_used: null } }));
    expect(document.getElementById('report-providers')).toBeNull();
    first.view.unmount();
    await showReport(report({ generation: { ...report().generation, providers: {} } }));
    expect(document.getElementById('report-providers')).toBeNull();
  });

  it('says no estimate was kept when there is none', async () => {
    await showReport(report({ cost: { ...report().cost, estimated: null } }));
    expect(text('report-cost')).toContain('No estimate was kept with this run.');
  });

  it('lists the steps with their state, attempts and time', async () => {
    await showReport();
    const rows = Array.from(document.querySelectorAll('#report-steps tbody tr')).map((tr) => Array.from(tr.querySelectorAll('td')).map((td) => td.textContent));
    expect(rows).toEqual([
      ['Generate', 'done', '', '2 min'],
      ['Execute', 'done', '2 attempts', '3 min'],
    ]);
  });
});

describe('ReportView — Markdown', () => {
  it('copies the report as Markdown to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const { fetchMock } = await showReport();
    await act(async () => {
      button('report-copy').click();
    });
    await waitFor(() => expect(button('report-copy').textContent).toContain('Copied'));
    expect(writeText).toHaveBeenCalledWith('# Autopilot report\n\n- 1 failed');
    expect(requests(fetchMock, REPORT_ROUTE).some((r) => r.query.format === 'markdown')).toBe(true);
    expect(document.getElementById('report-markdown')).toBeNull();
  });

  it('shows the Markdown to copy by hand when the browser will not let the page use the clipboard', async () => {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) }, configurable: true });
    await showReport();
    await act(async () => {
      button('report-copy').click();
    });
    await waitFor(() => expect(document.getElementById('report-markdown')).toBeTruthy());
    expect((document.getElementById('report-markdown') as HTMLTextAreaElement).value).toBe('# Autopilot report\n\n- 1 failed');
    expect(button('report-copy').textContent).toContain('Copy as Markdown');
  });

  it('says why the Markdown could not be read', async () => {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn() }, configurable: true });
    await showReport(report(), true, { 'GET /report': (req) => (req.query.format === 'markdown' ? failure(500, 'INTERNAL_ERROR', 'the report could not be written') : report()) });
    await act(async () => {
      button('report-copy').click();
    });
    await waitFor(() => expect(document.querySelector('[role="alert"]')).toBeTruthy());
    expect(document.querySelector('[role="alert"]')!.textContent).toBe('the report could not be written');
  });
});
