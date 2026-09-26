/**
 * Phase 8C — the form that makes a schedule and the report of a regression run, each on its own.
 */
import React, { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { ScheduleForm, type ScopeState } from './ScheduleForm';
import { RegressionReportView, type CopyState } from './RegressionReportView';
import { CUSTOM_EVERY, emptyForm, formProblems, type ScheduleFormState } from './regressionModel';
import { reportFor, scopeFor, summary } from './regressionTestData';
import type { RegressionReport } from '../../types/contract';

afterEach(cleanup);

const el = (id: string) => document.getElementById(id) as HTMLElement;
const text = (id: string) => el(id)?.textContent ?? '';

// ------------------------------------------------------------------------------------------------------------------- the form

function harness(start: Partial<ScheduleFormState> = {}, scope: ScopeState = { status: 'ready', scope: scopeFor() }, props: { submitting?: boolean } = {}) {
  const submitted = vi.fn();
  const latest: { form: ScheduleFormState } = { form: { ...emptyForm(), ...start } };
  const Harness: React.FC = () => {
    const [form, setForm] = useState<ScheduleFormState>({ ...emptyForm(), ...start });
    latest.form = form;
    return (
      <ScheduleForm
        form={form}
        onChange={setForm}
        problems={formProblems(form)}
        suites={[{ id: 'suite-1', name: 'Smoke' }, { id: 'suite-2', name: 'Checkout' }]}
        scope={scope}
        onSubmit={submitted}
        submitting={props.submitting ?? false}
      />
    );
  };
  render(<Harness />);
  return { submitted, form: () => latest.form };
}

describe('ScheduleForm', () => {
  it('starts on every case, every day, two at a time, ticked to start by itself', () => {
    harness();
    expect((el('regression-suite') as HTMLSelectElement).value).toBe('');
    expect((el('regression-every') as HTMLSelectElement).value).toBe('1440');
    expect((el('regression-concurrency') as HTMLInputElement).value).toBe('2');
    expect((el('regression-enabled') as HTMLInputElement).checked).toBe(true);
    expect(el('regression-custom')).toBeNull();
  });

  it('offers the suites of the project beside every case', () => {
    const { form } = harness();
    const options = Array.from((el('regression-suite') as HTMLSelectElement).options).map((o) => o.textContent);
    expect(options).toEqual(['Every case of the project that has been run once', 'Suite: Smoke', 'Suite: Checkout']);
    fireEvent.change(el('regression-suite'), { target: { value: 'suite-2' } });
    expect(form().suiteId).toBe('suite-2');
  });

  it('holds what is typed and chosen: the name, the interval (named or in minutes), the first run and the cases at a time', () => {
    const { form } = harness();
    fireEvent.change(el('regression-name'), { target: { value: 'Nightly' } });
    expect(form().name).toBe('Nightly');
    fireEvent.change(el('regression-every'), { target: { value: '60' } });
    expect(form().every).toBe('60');
    fireEvent.change(el('regression-every'), { target: { value: CUSTOM_EVERY } });
    expect(el('regression-custom')).toBeTruthy();
    fireEvent.change(el('regression-custom'), { target: { value: '90' } });
    expect(form().customMinutes).toBe(90);
    fireEvent.change(el('regression-start'), { target: { value: '2026-10-01T02:00' } });
    expect(form().startAt).toBe('2026-10-01T02:00');
    fireEvent.change(el('regression-concurrency'), { target: { value: '4' } });
    expect(form().concurrency).toBe(4);
    fireEvent.click(el('regression-enabled'));
    expect(form().enabled).toBe(false);
  });

  it('says what would be repeated, and why some could not, when the service has looked', () => {
    harness();
    expect(text('regression-scope-text')).toBe('3 cases will be repeated; 1 case cannot.');
    expect(text('regression-scope-skipped')).toContain('Never run');
    expect(text('regression-scope-skipped')).toContain('approved, but not run yet');
  });

  it('says it is looking, and that it could not', () => {
    harness({}, { status: 'loading' });
    expect(text('regression-scope')).toContain('Looking at the cases…');
    cleanup();
    harness({}, { status: 'failed', message: 'busy' });
    expect(text('regression-scope')).toContain('The cases could not be looked at: busy');
  });

  it('has no list of reasons when everything can be repeated', () => {
    harness({}, { status: 'ready', scope: scopeFor({ skipped: [] }) });
    expect(el('regression-scope-skipped')).toBeNull();
  });

  it('lists what is wrong and does not let the button be pressed', () => {
    const { submitted } = harness();
    expect(text('regression-problems')).toContain('Give the schedule a name.');
    expect((el('regression-create') as HTMLButtonElement).disabled).toBe(true);
    fireEvent.submit(el('regression-form'));
    expect(submitted).not.toHaveBeenCalled();
  });

  it('asks once the form has nothing wrong, by the button or by submitting', () => {
    const { submitted } = harness({ name: 'Nightly' });
    expect(el('regression-problems')).toBeNull();
    const button = el('regression-create') as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    expect(button.textContent).toBe('Make the schedule');
    fireEvent.click(button);
    expect(submitted).toHaveBeenCalledTimes(1);
    fireEvent.submit(el('regression-form'));
    expect(submitted).toHaveBeenCalledTimes(2);
  });

  it('says it is making the schedule while it does, and does not ask again meanwhile', () => {
    const { submitted } = harness({ name: 'Nightly' }, undefined, { submitting: true });
    const button = el('regression-create') as HTMLButtonElement;
    expect(button.textContent).toBe('Making the schedule…');
    expect(button.disabled).toBe(true);
    fireEvent.submit(el('regression-form'));
    expect(submitted).not.toHaveBeenCalled();
  });
});

// ------------------------------------------------------------------------------------------------------------ the report

function show(report: RegressionReport = reportFor(), over: Partial<React.ComponentProps<typeof RegressionReportView>> = {}) {
  const onCopy = vi.fn();
  render(<RegressionReportView report={report} onCopyMarkdown={onCopy} copyState={'idle' as CopyState} markdown="" {...over} />);
  return onCopy;
}

const caseRow = (id: string) => Array.from(document.querySelectorAll('[data-case]')).find((e) => e.getAttribute('data-case') === id) as HTMLElement;

describe('the head of the report', () => {
  it('names the schedule, when the run was made and by what, and how it came out', () => {
    show();
    expect(document.querySelector('h2')?.textContent).toBe('Nightly');
    expect(text('regression-report-time')).toMatch(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} · by the schedule/);
    expect(el('regression-verdict').getAttribute('data-verdict')).toBe('REGRESSED');
    expect(text('regression-verdict')).toBe('Regressions found');
    expect(el('regression-verdict').className).toContain('text-rose-300');
    cleanup();
    show(reportFor({ trigger: 'MANUAL', verdict: 'PASSING' }));
    expect(text('regression-report-time')).toContain('run by hand');
    expect(text('regression-verdict')).toBe('All passing');
    expect(el('regression-verdict').className).toContain('text-emerald-300');
    cleanup();
    show(reportFor({ verdict: 'RUNNING', final: false }));
    expect(text('regression-verdict')).toBe('Running');
    expect(el('regression-verdict').querySelector('svg')?.getAttribute('class')).toContain('animate-spin');
  });

  it('can be copied as Markdown, says when it was, and shows the text when the browser would not copy it', () => {
    const onCopy = show();
    expect(text('regression-copy')).toBe('Copy as Markdown');
    fireEvent.click(el('regression-copy'));
    expect(onCopy).toHaveBeenCalledTimes(1);
    cleanup();
    show(reportFor(), { copyState: 'copied' });
    expect(text('regression-copy')).toBe('Copied');
    expect(el('regression-markdown')).toBeNull();
    cleanup();
    show(reportFor(), { copyState: 'shown', markdown: '# Regression run: Nightly' });
    expect((el('regression-markdown') as HTMLTextAreaElement).value).toBe('# Regression run: Nightly');
    expect(el('regression-markdown').getAttribute('rows')).toBe('8');
  });
});

describe('the JUnit download', () => {
  it('is there when it is given something to do, and does it when it is pressed', () => {
    const onDownload = vi.fn();
    show(reportFor(), { onDownloadJunit: onDownload });
    expect(text('regression-junit')).toBe('Download JUnit');
    fireEvent.click(el('regression-junit'));
    expect(onDownload).toHaveBeenCalledTimes(1);
  });

  it('is not there when there is nothing to do', () => {
    show();
    expect(document.getElementById('regression-junit')).toBeNull();
  });
});

describe('the tiles', () => {
  it('count the cases run, passed, failed (errors with them), regressed, fixed and still failing', () => {
    show();
    expect(text('regression-tile-cases')).toContain('4 of 4');
    expect(text('regression-tile-passed')).toContain('2');
    expect(text('regression-tile-failed')).toContain('2');
    expect(text('regression-tile-regressed')).toContain('1');
    expect(text('regression-tile-fixed')).toContain('1');
    expect(text('regression-tile-still-failing')).toContain('1');
    cleanup();
    show(reportFor({ summary: summary({ failed: 1, error: 2 }) }));
    expect(text('regression-tile-failed')).toContain('3');
  });

  it('are rose for what is wrong, emerald for what is good, and plain when there is nothing to say', () => {
    show();
    expect(el('regression-tile-failed').className).toContain('bg-rose-950/30 border-rose-800/60');
    expect(el('regression-tile-regressed').className).toContain('bg-rose-950/30 border-rose-800/60');
    expect(el('regression-tile-still-failing').className).toContain('bg-rose-950/30');
    expect(el('regression-tile-fixed').className).toContain('bg-emerald-950/20 border-emerald-800/50');
    expect(el('regression-tile-passed').className).not.toContain('emerald');
    expect(el('regression-tile-cases').className).toContain('bg-[#020617] border-slate-800');
    cleanup();
    show(reportFor({ summary: summary({ passed: 4, failed: 0, error: 0, regressed: 0, fixed: 0, still_failing: 0, still_passing: 4 }) }));
    expect(el('regression-tile-passed').className).toContain('bg-emerald-950/20');
    for (const id of ['failed', 'regressed', 'fixed', 'still-failing']) expect(el(`regression-tile-${id}`).className, id).toContain('bg-[#020617]');
    cleanup();
    show(reportFor({ summary: summary({ ran: 0, passed: 0, failed: 0, regressed: 0, fixed: 0, still_failing: 0, still_passing: 0 }) }));
    expect(el('regression-tile-passed').className, 'nothing ran: nothing passed').not.toContain('emerald');
  });

  it('say how many cases are still waiting, and only then', () => {
    show(reportFor({ final: false, summary: summary({ ran: 2, pending: 2 }) }));
    expect(el('regression-tile-cases').querySelector('.mt-0\\.5')?.textContent).toBe('2 still waiting');
    expect(el('regression-tile-regressed').querySelector('.mt-0\\.5')?.textContent).toBe('passed last time, not now');
    cleanup();
    show();
    expect(el('regression-tile-cases').querySelector('.mt-0\\.5')).toBeNull();
    expect(el('regression-tile-passed').querySelector('.mt-0\\.5')).toBeNull();
  });
});

describe('the cases', () => {
  it('are listed in the order the service gives, regressions first, each with what it came to and what it came to before', () => {
    show();
    const order = Array.from(document.querySelectorAll('[data-case]')).map((e) => e.getAttribute('data-change'));
    expect(order).toEqual(['REGRESSED', 'STILL_FAILING', 'FIXED', 'STILL_PASSING']);
    const login = caseRow('case-login');
    expect(login.textContent).toContain('Regressed');
    expect(login.textContent).toContain('Login rejects a wrong password');
    expect(login.textContent).toContain('Failed');
    expect(login.textContent).toContain('passed last time, fails now');
    expect(login.getAttribute('data-outcome')).toBe('FAILED');
  });

  it('give the reason a case failed in rose, and none for a case that passed', () => {
    show();
    const reason = caseRow('case-login').querySelector('[data-reason]') as HTMLElement;
    expect(reason.textContent).toBe('expected an HTTP status among 200, got real status 500');
    expect(reason.className).toContain('text-rose-300');
    expect(caseRow('case-search').querySelector('[data-reason]')).toBeNull();
  });

  it('are coloured by what became of them', () => {
    show();
    const badge = (id: string) => caseRow(id).querySelector('span') as HTMLElement;
    expect(badge('case-login').className).toContain('text-rose-300');
    expect(badge('case-checkout').className).toContain('text-rose-300');
    expect(badge('case-search').className).toContain('text-emerald-300');
    expect(badge('case-profile').className).toContain('text-slate-400');
    cleanup();
    const fresh = reportFor().cases.slice(0, 2).map((c, i) => ({ ...c, before: null, before_from: null, change: 'NEW' as const, outcome: i === 0 ? ('FAILED' as const) : ('PASSED' as const) }));
    show(reportFor({ cases: fresh }));
    expect((caseRow('case-login').querySelector('span') as HTMLElement).className, 'a new case that fails is a warning').toContain('text-amber-300');
    expect((caseRow('case-checkout').querySelector('span') as HTMLElement).className, 'one that passes is nothing to say').toContain('text-slate-400');
  });

  it('say so when none was repeated', () => {
    show(reportFor({ cases: [], skipped: [] }));
    expect(text('regression-no-cases')).toBe('No case was repeated in this run.');
    expect(el('regression-cases')).toBeNull();
    expect(el('regression-skipped')).toBeNull();
  });
});

describe('what could not be repeated, and the notes', () => {
  it('lists each case that was not repeated with why, shut until it is wanted', () => {
    show();
    expect((el('regression-skipped') as HTMLDetailsElement).open).toBe(false);
    expect(el('regression-skipped').querySelector('summary')?.textContent).toBe('Not repeated (1)');
    expect(text('regression-skipped')).toContain('Never run: approved, but not run yet');
  });

  it('shows the notes, and none when there are none', () => {
    show();
    expect(text('regression-notes')).toContain('compared with the run they were copied from');
    cleanup();
    show(reportFor({ notes: [] }));
    expect(el('regression-notes')).toBeNull();
  });
});
