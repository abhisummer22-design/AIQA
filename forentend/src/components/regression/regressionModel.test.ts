/**
 * Phase 8C — what the scheduled-regression page asks and says, worked out from what the service says.
 */
import { describe, expect, it } from 'vitest';
import {
  CHANGE_LABEL,
  CUSTOM_EVERY,
  EVERY_CHOICES,
  buildBody,
  changeLine,
  ciSnippet,
  emptyForm,
  everyMinutes,
  everyText,
  formProblems,
  scopeText,
  verdictTone,
  type ScheduleFormState,
} from './regressionModel';
import { CASES, scopeFor } from './regressionTestData';

const valid = (over: Partial<ScheduleFormState> = {}): ScheduleFormState => ({ ...emptyForm(), name: 'Nightly', ...over });

describe('the form as it starts', () => {
  it('repeats every case, every day, at two cases at a time, and starts by itself', () => {
    expect(emptyForm()).toEqual({ name: '', suiteId: '', every: '1440', customMinutes: 60, startAt: '', concurrency: 2, enabled: true });
  });

  it('offers the intervals people ask for by name, each in minutes', () => {
    expect(EVERY_CHOICES.map((c) => c.minutes)).toEqual([15, 60, 360, 1440, 10_080]);
    expect(everyMinutes(valid({ every: '360' }))).toBe(360);
    expect(everyMinutes(valid({ every: CUSTOM_EVERY, customMinutes: 90 }))).toBe(90);
  });
});

describe('formProblems', () => {
  it('has none for a form the service can be asked', () => {
    expect(formProblems(valid())).toEqual([]);
    expect(formProblems(valid({ startAt: '2026-10-01T02:00' }))).toEqual([]);
  });

  it('wants a name, of some length', () => {
    expect(formProblems(emptyForm())).toEqual(['Give the schedule a name.']);
    expect(formProblems(valid({ name: '   ' }))).toEqual(['Give the schedule a name.']);
    expect(formProblems(valid({ name: 'x'.repeat(100) }))).toEqual([]);
    expect(formProblems(valid({ name: 'x'.repeat(101) }))).toEqual(['The name must be at most 100 characters.']);
    expect(formProblems(valid({ name: 'é'.repeat(100) })), 'characters, not bytes').toEqual([]);
  });

  it('wants an interval from five minutes to a month, in whole minutes', () => {
    const bad = 'How often must be from 5 to 44,640 minutes.';
    for (const minutes of [4, 0, -5, 44_641, 2.5, Number.NaN]) {
      expect(formProblems(valid({ every: CUSTOM_EVERY, customMinutes: minutes })), String(minutes)).toEqual([bad]);
    }
    for (const minutes of [5, 90, 44_640]) expect(formProblems(valid({ every: CUSTOM_EVERY, customMinutes: minutes })), String(minutes)).toEqual([]);
  });

  it('wants a first run that is a date and a time, or none', () => {
    expect(formProblems(valid({ startAt: 'soon' }))).toEqual(['The first run must be a date and a time.']);
  });

  it('wants one to eight cases at a time', () => {
    for (const n of [0, 9, 2.5, Number.NaN]) expect(formProblems(valid({ concurrency: n })), String(n)).toEqual(['Cases at a time must be from 1 to 8.']);
    for (const n of [1, 8]) expect(formProblems(valid({ concurrency: n })), String(n)).toEqual([]);
  });

  it('says every problem at once', () => {
    expect(formProblems(valid({ name: '', every: CUSTOM_EVERY, customMinutes: 1, concurrency: 0, startAt: 'x' }))).toHaveLength(4);
  });
});

describe('buildBody', () => {
  it('sends the name trimmed, the interval in minutes, the concurrency and whether it starts by itself', () => {
    expect(buildBody(valid({ name: '  Nightly  ', every: '60', concurrency: 3, enabled: false }))).toEqual({ name: 'Nightly', every_minutes: 60, concurrency: 3, enabled: false });
  });

  it('names the suite only when one is chosen, and the first run only when one was given (as the viewer’s clock, in UTC)', () => {
    const body = buildBody(valid({ suiteId: 'suite-1', startAt: '2026-10-01T02:00' }));
    expect(body.suite_id).toBe('suite-1');
    expect(body.start_at).toBe(new Date('2026-10-01T02:00').toISOString());
    const plain = buildBody(valid());
    expect('suite_id' in plain).toBe(false);
    expect('start_at' in plain).toBe(false);
  });
});

describe('everyText', () => {
  it('says the longest whole unit', () => {
    const table: Array<[number, string]> = [
      [5, 'every 5 minutes'],
      [15, 'every 15 minutes'],
      [60, 'every hour'],
      [90, 'every 90 minutes'],
      [120, 'every 2 hours'],
      [360, 'every 6 hours'],
      [1440, 'every day'],
      [2880, 'every 2 days'],
      [10_080, 'every week'],
      [20_160, 'every 2 weeks'],
      [44_640, 'every 31 days'],
    ];
    for (const [minutes, text] of table) expect(everyText(minutes), String(minutes)).toBe(text);
  });
});

describe('changeLine', () => {
  const change = (i: number) => CASES[i];
  it('sets what it came to before beside what it comes to now', () => {
    expect(changeLine(change(0))).toBe('passed last time, fails now');
    expect(changeLine(change(1))).toBe('failed last time, fails now');
    expect(changeLine(change(2))).toBe('failed in the run it was copied from, passes now');
    expect(changeLine(change(3))).toBe('passed last time, passes now');
  });

  it('says so when there is nothing to compare with, and when it could not run', () => {
    expect(changeLine({ ...CASES[0], before: null, before_from: null, change: 'NEW', outcome: 'PASSED' })).toBe('no earlier result to compare with; it passes');
    expect(changeLine({ ...CASES[0], before: 'PASSED', outcome: 'ERROR' })).toBe('passed last time, could not run now');
    expect(changeLine({ ...CASES[0], before: 'ERROR', outcome: 'PASSED' })).toBe('could not run last time, passes now');
    expect(changeLine({ ...CASES[0], before: 'PASSED', outcome: 'PENDING' })).toBe('passed last time, is waiting to run now');
  });
});

describe('ciSnippet', () => {
  it('gives the lines that run the schedule from a CI job, with the project and the schedule filled in and the rest left to its owner', () => {
    const lines = ciSnippet('proj-a', '11111111-1111-4111-8111-111111111111').split('\n');
    expect(lines).toHaveLength(5);
    expect(lines[2]).toBe('export AIQA_PROJECT="proj-a"');
    expect(lines[4]).toBe('bash scripts/ci/aiqa-ci.sh regression 11111111-1111-4111-8111-111111111111');
    expect(lines[0]).toMatch(/^export AIQA_URL="<where AIQA can be reached/);
    expect(lines[1]).toMatch(/^export AIQA_KEY="<an API key with the operator role/);
    expect(lines[3]).toMatch(/^export AIQA_FAIL_ON=regressed/);
  });

  it('never has a key in it: it is a placeholder for a secret', () => {
    expect(ciSnippet('p', 's')).not.toMatch(/aiqa_[A-Za-z0-9]{8,}/);
  });
});

describe('scopeText and the labels', () => {
  it('counts what would be repeated and what could not', () => {
    expect(scopeText(scopeFor())).toBe('3 cases will be repeated; 1 case cannot.');
    expect(scopeText(scopeFor({ skipped: [] }))).toBe('3 cases will be repeated.');
    expect(scopeText(scopeFor({ repeatable: [scopeFor().repeatable[0]], skipped: [] }))).toBe('1 case will be repeated.');
    expect(scopeText(scopeFor({ repeatable: [] }))).toBe('No case can be repeated yet; 1 case cannot.');
    expect(scopeText(scopeFor({ repeatable: [], skipped: [] }))).toBe('No case can be repeated yet.');
  });

  it('gives a change and a verdict their own word and colour', () => {
    expect(Object.values(CHANGE_LABEL)).toEqual(['Regressed', 'Fixed', 'Still failing', 'Still passing', 'New', 'Not compared']);
    expect(verdictTone('PASSING')).toBe('good');
    expect(verdictTone('REGRESSED')).toBe('bad');
    expect(verdictTone('FAILING')).toBe('bad');
    expect(verdictTone('RUNNING')).toBe('warn');
    expect(verdictTone('NOTHING_RAN')).toBe('plain');
  });
});
