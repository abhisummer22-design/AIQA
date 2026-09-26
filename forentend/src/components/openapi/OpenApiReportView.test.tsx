/**
 * Phase 8B — the report of an OpenAPI test run: what each operation came to, what each case expected and what came back, and how much of
 * the document was tested.
 */
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { OpenApiReportView } from './OpenApiReportView';
import { reportFor } from './openApiTestData';
import type { OpenApiReport } from '../../types/contract';

afterEach(cleanup);

const el = (id: string) => document.getElementById(id) as HTMLElement;
const text = (id: string) => el(id)?.textContent ?? '';

function show(report: OpenApiReport = reportFor(), over: Partial<React.ComponentProps<typeof OpenApiReportView>> = {}) {
  const onCopy = vi.fn();
  render(<OpenApiReportView report={report} onCopyMarkdown={onCopy} copyState="idle" markdown="" {...over} />);
  return onCopy;
}

const caseRow = (id: string) => Array.from(document.querySelectorAll('[data-case]')).find((e) => e.getAttribute('data-case') === id) as HTMLElement;

describe('the head of the report', () => {
  it('names the API by its document and says where it was tested', () => {
    show();
    expect(document.querySelector('h2')?.textContent).toBe('Fixture Pets 1.0');
    expect(text('openapi-report-target')).toBe('http://127.0.0.1:8080/fixture');
  });

  it('says how the run ended when it has, and that it is running when it has not', () => {
    show();
    expect(text('openapi-report-final')).toBe('completed');
    expect(el('openapi-report-not-final')).toBeNull();
    cleanup();
    show(reportFor({ final: false, run_state: 'RUNNING' }));
    expect(text('openapi-report-not-final')).toContain('running');
    expect(el('openapi-report-final')).toBeNull();
    cleanup();
    show(reportFor({ run_state: 'PARTIAL' }));
    expect(text('openapi-report-final')).toBe('partial');
  });

  it('can be copied as Markdown, and says when it was', () => {
    const onCopy = show();
    expect(text('openapi-copy')).toBe('Copy as Markdown');
    fireEvent.click(el('openapi-copy'));
    expect(onCopy).toHaveBeenCalledTimes(1);
    cleanup();
    show(reportFor(), { copyState: 'copied' });
    expect(text('openapi-copy')).toBe('Copied');
    expect(el('openapi-markdown')).toBeNull();
  });

  it('shows the text to copy by hand when the browser would not copy it', () => {
    show(reportFor(), { copyState: 'shown', markdown: '# API tests from Fixture Pets 1.0' });
    expect((el('openapi-markdown') as HTMLTextAreaElement).value).toBe('# API tests from Fixture Pets 1.0');
    expect(el('openapi-markdown').getAttribute('rows')).toBe('8');
    expect(text('openapi-copy')).toBe('Copy as Markdown');
  });
});

describe('the JUnit download', () => {
  it('is there when it is given something to do, and does it when it is pressed', () => {
    const onDownload = vi.fn();
    show(reportFor(), { onDownloadJunit: onDownload });
    expect(text('openapi-junit')).toBe('Download JUnit');
    fireEvent.click(el('openapi-junit'));
    expect(onDownload).toHaveBeenCalledTimes(1);
  });

  it('is not there when there is nothing to do', () => {
    show();
    expect(el('openapi-junit')).toBeNull();
  });
});

describe('the tiles', () => {
  it('give the cases run of all, how many passed, failed, could not run, were not run, and the pass rate', () => {
    show();
    expect(text('openapi-tile-ran')).toContain('8 of 12');
    expect(text('openapi-tile-passed')).toContain('7');
    expect(text('openapi-tile-failed')).toContain('1');
    expect(text('openapi-tile-error')).toContain('0');
    expect(text('openapi-tile-not-run')).toContain('4');
    expect(text('openapi-tile-rate')).toContain('88%');
  });

  it('are red for a failure, amber for a case that could not be run, and green only when all that ran passed', () => {
    show();
    expect(el('openapi-tile-failed').className).toContain('bg-rose-950/30');
    expect(el('openapi-tile-passed').className).not.toContain('emerald');
    expect(el('openapi-tile-error').className).not.toContain('amber');
    cleanup();
    show(reportFor({ summary: { cases: 4, ran: 3, passed: 3, failed: 0, error: 1, pending: 0, not_run: 1, pass_rate: 0.75 } }));
    expect(el('openapi-tile-error').className).toContain('bg-amber-950/30');
    expect(el('openapi-tile-passed').className).not.toContain('emerald');
    expect(el('openapi-tile-failed').className).not.toContain('rose');
    cleanup();
    show(reportFor({ summary: { cases: 4, ran: 4, passed: 4, failed: 0, error: 0, pending: 0, not_run: 0, pass_rate: 1 } }));
    expect(el('openapi-tile-passed').className).toContain('bg-emerald-950/20');
    expect(text('openapi-tile-rate')).toContain('100%');
    cleanup();
    show(reportFor({ summary: { cases: 4, ran: 0, passed: 0, failed: 0, error: 0, pending: 0, not_run: 4, pass_rate: null } }));
    expect(el('openapi-tile-passed').className).not.toContain('emerald');
    expect(text('openapi-tile-rate')).toContain('—');
  });

  it('count the cases still waiting with those not run, and say so', () => {
    show(reportFor({ final: false, summary: { cases: 12, ran: 3, passed: 3, failed: 0, error: 0, pending: 5, not_run: 4, pass_rate: 1 } }));
    expect(text('openapi-tile-not-run')).toContain('9');
    expect(text('openapi-tile-not-run')).toContain('5 still waiting');
    cleanup();
    show();
    expect(text('openapi-tile-not-run')).not.toContain('still waiting');
  });
});

describe('the looks of the tiles', () => {
  it('are rose for what failed, emerald when all that ran passed, amber for what could not run, and plain otherwise', () => {
    show();
    expect(el('openapi-tile-failed').className).toContain('bg-rose-950/30 border-rose-800/60');
    expect(el('openapi-tile-ran').className).toContain('bg-[#020617] border-slate-800');
    cleanup();
    show(reportFor({ summary: { cases: 4, ran: 3, passed: 3, failed: 0, error: 1, pending: 0, not_run: 1, pass_rate: 0.75 } }));
    expect(el('openapi-tile-error').className).toContain('bg-amber-950/30 border-amber-800/60');
    cleanup();
    show(reportFor({ summary: { cases: 4, ran: 4, passed: 4, failed: 0, error: 0, pending: 0, not_run: 0, pass_rate: 1 } }));
    expect(el('openapi-tile-passed').className).toContain('bg-emerald-950/20 border-emerald-800/50');
  });

  it('put a note under the value only when there is one', () => {
    show(reportFor({ final: false, summary: { cases: 12, ran: 3, passed: 3, failed: 0, error: 0, pending: 5, not_run: 4, pass_rate: 1 } }));
    expect(el('openapi-tile-not-run').querySelector('.mt-0\\.5')?.textContent).toBe('5 still waiting');
    expect(el('openapi-tile-ran').querySelector('.mt-0\\.5')).toBeNull();
  });
});

describe('the coverage of the document', () => {
  it('says how many operations were tested, and which were not and why', () => {
    show();
    expect(text('openapi-coverage')).toContain('3 of 4 operations of the document tested.');
    expect(text('openapi-coverage')).toContain('Not tested: POST /pets (none of its cases was run).');
    expect(document.querySelector('[data-not-tested="POST /pets"]')).toBeTruthy();
  });

  it('lists several untested operations one after another, and none when all were tested', () => {
    show(
      reportFor({
        coverage: {
          operations: 5,
          tested: 3,
          not_tested: [
            { operation: 'POST /pets', reason: 'none of its cases was run' },
            { operation: 'GET /old', reason: 'the operation is marked deprecated' },
          ],
        },
      }),
    );
    expect(text('openapi-coverage')).toContain('Not tested: POST /pets (none of its cases was run); GET /old (the operation is marked deprecated).');
    cleanup();
    show(reportFor({ coverage: { operations: 1, tested: 1, not_tested: [] } }));
    expect(text('openapi-coverage')).toBe('1 of 1 operation of the document tested.');
  });
});

describe('the notes', () => {
  it('say what to know before trusting the numbers, and are not there when there are none', () => {
    show();
    expect(text('openapi-report-notes')).toContain('4 case(s) were not run because their request changes data');
    cleanup();
    show(reportFor({ notes: [] }));
    expect(el('openapi-report-notes')).toBeNull();
  });
});

describe('the operations', () => {
  const block = (id: string) => el(`openapi-report-op-${id}`) as HTMLDetailsElement;

  it('show what an operation does only when the document says, and not an empty place for it', () => {
    show();
    expect(block('GET /pets').querySelector('summary span.text-slate-400:not([data-verdict])')?.textContent).toBe('List the pets');
    expect(block('POST /pets').querySelector('summary span.text-slate-400:not([data-verdict])')).toBeNull();
    expect(block('GET /secure').querySelector('summary span.text-slate-400:not([data-verdict])')).toBeNull();
  });

  it('are one block each, in the order of the document, with what each came to', () => {
    show();
    const verdicts = Array.from(document.querySelectorAll('[id^="openapi-report-op-"]')).map((d) => [d.id.replace('openapi-report-op-', ''), d.getAttribute('data-verdict')]);
    expect(verdicts).toEqual([
      ['GET /pets', 'PASSED'],
      ['POST /pets', 'NOT_RUN'],
      ['GET /pets/{petId}', 'FAILED'],
      ['GET /secure', 'PASSED'],
    ]);
    expect(block('GET /pets/{petId}').querySelector('summary')?.textContent).toContain('2 passed · 1 failed');
    expect(block('GET /pets').querySelector('summary')?.textContent).toContain('List the pets');
    expect(block('POST /pets').querySelector('summary')?.textContent).toContain('4 not run');
  });

  it('are open when they failed or could not run, so what went wrong is seen first, and shut when they passed', () => {
    show();
    expect(block('GET /pets/{petId}').open).toBe(true);
    expect(block('GET /pets').open).toBe(false);
    expect(block('POST /pets').open).toBe(false);
    cleanup();
    const report = reportFor();
    report.operations[0] = { ...report.operations[0], verdict: 'ERROR' };
    show(report);
    expect(block('GET /pets').open).toBe(true);
  });

  it('say why an operation has no case, in its heading and under it', () => {
    const report = reportFor();
    report.operations.push({ id: 'GET /old', summary: null, tags: [], verdict: 'SKIPPED', counts: null, why_no_cases: 'the operation is marked deprecated', cases: [] });
    report.operations.push({ id: 'GET /other', summary: null, tags: [], verdict: 'NOT_SELECTED', counts: null, why_no_cases: 'not selected', cases: [] });
    report.operations.push({ id: 'GET /bare', summary: null, tags: [], verdict: 'SKIPPED', counts: null, why_no_cases: null, cases: [] });
    show(report);
    expect(block('GET /old').querySelector('summary')?.textContent).toContain('the operation is marked deprecated');
    expect(block('GET /old').textContent).toContain('No case was made: the operation is marked deprecated.');
    expect(block('GET /old').textContent).toContain('No cases');
    expect(block('GET /other').textContent).toContain('Not selected');
    expect(block('GET /bare').querySelector('summary')?.textContent).toContain('no cases');
    expect(block('GET /bare').textContent).toContain('No case was made: none applies to it.');
  });
});

describe('a case', () => {
  it('that passed says what it expected and what it got, and no reason', () => {
    show();
    const row = caseRow('GET /pets#HAPPY');
    expect(row.getAttribute('data-outcome')).toBe('PASSED');
    expect(row.textContent).toContain('Passed');
    expect(row.textContent).toContain('Valid request');
    expect(row.textContent).toContain('a valid request gets a documented success');
    expect(row.textContent).toContain('GET /fixture/pets?limit=1');
    expect(row.textContent).toContain('expected 200 · body fits its schema');
    expect(row.textContent).not.toContain('got');
  });

  it('that failed says what it expected, what came back and in how long, and why', () => {
    show();
    const row = caseRow('GET /pets/{petId}#WRONG_TYPE:petId');
    expect(row.getAttribute('data-outcome')).toBe('FAILED');
    expect(row.textContent).toContain('Failed');
    expect(row.textContent).toContain('expected 404');
    expect(row.textContent).toContain('got 400 in 12 ms');
    expect(row.textContent).toContain('Status is one of 404: got 400 Bad Request');
    const reason = row.querySelector('[data-reason]') as HTMLElement;
    expect(reason.textContent).toContain('got 400 Bad Request');
    expect(reason.className).toContain('text-rose-300');
    expect(row.textContent).toContain('GET /fixture/pets/not-a-number');
  });

  it('shows the status that came back in rose when the case did not pass, and in grey when it did', () => {
    show();
    const status = (row: HTMLElement, text: string) => Array.from(row.querySelectorAll('span')).find((s) => s.textContent === text) as HTMLElement;
    expect(status(caseRow('GET /pets/{petId}#WRONG_TYPE:petId'), '400').className).toContain('text-rose-300');
    cleanup();
    const report = reportFor();
    report.operations[0].cases[0] = { ...report.operations[0].cases[0], actual: { status: 200, duration_ms: 5, assertions: [] } };
    show(report);
    const passed = status(caseRow('GET /pets#HAPPY'), '200');
    expect(passed.className).toContain('text-slate-300');
    expect(passed.className).not.toContain('rose');
  });

  it('that was not run says why, in plain grey, and does not claim to have expected or got anything', () => {
    show();
    const row = caseRow('POST /pets#HAPPY');
    expect(row.getAttribute('data-outcome')).toBe('NOT_RUN');
    expect(row.textContent).toContain('Not run');
    expect(row.textContent).toContain('POST changes data on the server');
    expect(row.textContent).not.toContain('expected');
    const reason = row.querySelector('[data-reason]') as HTMLElement;
    expect(reason.textContent).toContain('POST changes data');
    expect(reason.className).toContain('text-slate-500');
    expect(reason.className).not.toContain('rose');
  });

  it('that ran but has no evidence read shows what it expected and no answer', () => {
    const report = reportFor();
    report.operations[0].cases[0] = { ...report.operations[0].cases[0], outcome: 'ERROR', reason: 'CONNECTION_FAILED: refused', actual: null };
    show(report);
    const row = caseRow('GET /pets#HAPPY');
    expect(row.textContent).toContain('Could not run');
    expect(row.textContent).toContain('expected 200 · body fits its schema');
    expect(row.textContent).not.toContain('got');
    expect(row.textContent).toContain('CONNECTION_FAILED: refused');
  });

  it('that is still waiting is shown as such', () => {
    const report = reportFor({ final: false });
    report.operations[0].cases[0] = { ...report.operations[0].cases[0], outcome: 'PENDING', reason: null, actual: null };
    show(report);
    expect(caseRow('GET /pets#HAPPY').textContent).toContain('Waiting to run');
    expect(caseRow('GET /pets#HAPPY').textContent).toContain('expected 200');
  });
});

describe('a report that has not been read again yet', () => {
  it('is shown a little dimmed while it is read again', () => {
    show(reportFor(), { refreshing: true });
    expect(el('openapi-tile-ran').parentElement?.className).toContain('opacity-80');
    cleanup();
    show();
    expect(el('openapi-tile-ran').parentElement?.className).not.toContain('opacity-80');
  });
});
