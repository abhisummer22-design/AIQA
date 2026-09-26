/**
 * Phase 8C — what the scheduled-regression page shows and asks, worked out from what the service says (`regression_api.rs`). Nothing
 * here calls the service.
 */
import type {
  RegressionCase,
  RegressionChange,
  RegressionOutcome,
  RegressionScheduleBody,
  RegressionScope,
  RegressionVerdict,
} from '../../types/contract';

export const MIN_EVERY_MINUTES = 5;
export const MAX_EVERY_MINUTES = 44_640;
export const MAX_NAME_CHARS = 100;
export const MAX_CONCURRENCY = 8;
export const DEFAULT_CONCURRENCY = 2;

/** The intervals that are offered by name; anything else is typed in minutes. */
export const EVERY_CHOICES = [
  { minutes: 15, label: 'Every 15 minutes' },
  { minutes: 60, label: 'Every hour' },
  { minutes: 360, label: 'Every 6 hours' },
  { minutes: 1440, label: 'Every day' },
  { minutes: 10_080, label: 'Every week' },
] as const;

export const CUSTOM_EVERY = 'custom';

export interface ScheduleFormState {
  name: string;
  /** '' = every case of the project. */
  suiteId: string;
  /** The minutes of one of `EVERY_CHOICES` as text, or `CUSTOM_EVERY`. */
  every: string;
  customMinutes: number;
  /** A `datetime-local` value (the viewer's clock), or '' for one interval from now. */
  startAt: string;
  concurrency: number;
  enabled: boolean;
}

export const emptyForm = (): ScheduleFormState => ({
  name: '',
  suiteId: '',
  every: '1440',
  customMinutes: 60,
  startAt: '',
  concurrency: DEFAULT_CONCURRENCY,
  enabled: true,
});

export const everyMinutes = (form: ScheduleFormState): number => (form.every === CUSTOM_EVERY ? form.customMinutes : Number(form.every));

/** What is wrong with the form, in words; empty when the service can be asked. */
export function formProblems(form: ScheduleFormState): string[] {
  const problems: string[] = [];
  const name = form.name.trim();
  if (name === '') problems.push('Give the schedule a name.');
  else if ([...name].length > MAX_NAME_CHARS) problems.push(`The name must be at most ${MAX_NAME_CHARS} characters.`);
  const minutes = everyMinutes(form);
  if (!Number.isInteger(minutes) || minutes < MIN_EVERY_MINUTES || minutes > MAX_EVERY_MINUTES) {
    problems.push(`How often must be from ${MIN_EVERY_MINUTES} to ${MAX_EVERY_MINUTES.toLocaleString('en-US')} minutes.`);
  }
  if (form.startAt !== '' && Number.isNaN(new Date(form.startAt).getTime())) problems.push('The first run must be a date and a time.');
  if (!Number.isInteger(form.concurrency) || form.concurrency < 1 || form.concurrency > MAX_CONCURRENCY) {
    problems.push(`Cases at a time must be from 1 to ${MAX_CONCURRENCY}.`);
  }
  return problems;
}

/** The body for making a schedule. A first run that was not given is left out (the service takes one interval from now). */
export function buildBody(form: ScheduleFormState): RegressionScheduleBody {
  const body: RegressionScheduleBody = {
    name: form.name.trim(),
    every_minutes: everyMinutes(form),
    concurrency: form.concurrency,
    enabled: form.enabled,
  };
  if (form.suiteId !== '') body.suite_id = form.suiteId;
  if (form.startAt !== '') body.start_at = new Date(form.startAt).toISOString();
  return body;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** "every hour", "every 3 days", "every 90 minutes". */
export function everyText(minutes: number): string {
  const each = (unit: number, one: string, many: string) => {
    const n = minutes / unit;
    return n === 1 ? `every ${one}` : `every ${n} ${many}`;
  };
  if (minutes % 10_080 === 0) return each(10_080, 'week', 'weeks');
  if (minutes % 1440 === 0) return each(1440, 'day', 'days');
  if (minutes % 60 === 0) return each(60, 'hour', 'hours');
  return `every ${plural(minutes, 'minute', 'minutes')}`;
}

export const VERDICT_LABEL: Record<RegressionVerdict, string> = {
  REGRESSED: 'Regressions found',
  FAILING: 'Failing',
  PASSING: 'All passing',
  RUNNING: 'Running',
  NOTHING_RAN: 'Nothing ran',
};

export type Tone = 'good' | 'bad' | 'warn' | 'plain';

export const verdictTone = (verdict: RegressionVerdict): Tone =>
  verdict === 'PASSING' ? 'good' : verdict === 'REGRESSED' || verdict === 'FAILING' ? 'bad' : verdict === 'RUNNING' ? 'warn' : 'plain';

export const CHANGE_LABEL: Record<RegressionChange, string> = {
  REGRESSED: 'Regressed',
  FIXED: 'Fixed',
  STILL_FAILING: 'Still failing',
  STILL_PASSING: 'Still passing',
  NEW: 'New',
  UNCOMPARED: 'Not compared',
};

export const OUTCOME_LABEL: Record<RegressionOutcome, string> = {
  PASSED: 'Passed',
  FAILED: 'Failed',
  ERROR: 'Could not run',
  PENDING: 'Waiting to run',
  NOT_RUN: 'Not run',
};

const WAS: Record<string, string> = { PASSED: 'passed', FAILED: 'failed', ERROR: 'could not run' };
const IS: Record<RegressionOutcome, string> = { PASSED: 'passes', FAILED: 'fails', ERROR: 'could not run', PENDING: 'is waiting to run', NOT_RUN: 'was not run' };

/** "passed last time, fails now": what the case came to before, beside what it comes to now. */
export function changeLine(c: RegressionCase): string {
  const now = IS[c.outcome];
  if (c.before === null) return `no earlier result to compare with; it ${now}`;
  const when = c.before_from === 'SOURCE_RUN' ? 'in the run it was copied from' : 'last time';
  return `${WAS[c.before]} ${when}, ${now} now`;
}

/** What a schedule over the chosen cases would do, in a sentence. */
export function scopeText(scope: RegressionScope): string {
  const n = scope.repeatable.length;
  const cannot = scope.skipped.length;
  const first = n === 0 ? 'No case can be repeated yet' : `${plural(n, 'case', 'cases')} will be repeated`;
  return cannot === 0 ? `${first}.` : `${first}; ${plural(cannot, 'case', 'cases')} cannot.`;
}

export const SCHEDULE_POLL_MS = 15_000;
export const RUNS_POLL_MS = 3_000;
export const RUNS_IDLE_POLL_MS = 15_000;
export const REPORT_POLL_MS = 2_000;
export const RETRY_DELAY_MS = 10_000;
