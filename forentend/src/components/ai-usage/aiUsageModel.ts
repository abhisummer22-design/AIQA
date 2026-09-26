/**
 * Phase 8A — what the AI usage page shows and how it says it: money, counts and times in words, the names of tasks, and the
 * figures behind the daily bars. The numbers themselves are the backend's (`ai_usage.rs`); nothing here adds them up again.
 */
import type { AiUsageTally } from '../../types/contract';
import { formatSeconds, formatUsd, plural } from '../autopilot/autopilotModel';

/** The lengths of time the page offers, in days back from today. */
export const DAY_CHOICES: readonly number[] = [7, 30, 90];
export const DEFAULT_DAYS = 30;

/** How far the viewer's clock is ahead of UTC, in minutes (India: 330), so that the backend cuts a day at the viewer's own midnight. */
export const tzOffsetMinutes = (now: Date = new Date()): number => -now.getTimezoneOffset() || 0;

/** Dollars for a total. Nothing is `$0.00`; a sum too small for four decimals is `<$0.0001`, not a `$0.00` that says nothing was spent. */
export function formatMoney(usd: number): string {
  if (!(usd > 0)) return '$0.00';
  if (usd < 0.0001) return '<$0.0001';
  return formatUsd(usd);
}

/** A count in few characters: `950`, `1.5k`, `12k`, `3.2M`. */
export function formatCount(n: number): string {
  if (n < 1000) return String(n);
  const short = (value: number, unit: string) => `${value.toFixed(value >= 10 ? 0 : 1)}${unit}`.replace(/\.0(?=[kM]$)/, '');
  return n < 999_500 ? short(n / 1000, 'k') : short(n / 1_000_000, 'M');
}

/** The whole number, for a tooltip: `1,234,567`. */
export const exactCount = (n: number): string => n.toLocaleString('en-US');

/** A time a call took: `640 ms`, `5 s`, `2 min`. */
export const formatMs = (ms: number): string => (ms < 1000 ? `${Math.round(ms)} ms` : formatSeconds(ms / 1000));

const TASK_LABEL: Record<string, string> = {
  TEST_CASE_GENERATION: 'Test case generation',
  TEST_CASE_COMPILATION: 'Test case compilation',
  REQUIREMENT_ANALYSIS: 'Requirement analysis',
  DOCUMENT_ANALYSIS: 'Document analysis',
  BUG_ANALYSIS: 'Bug analysis',
  TRACEABILITY: 'Requirement gap check',
  SECURITY_ANALYSIS: 'Security analysis',
  PERFORMANCE_ANALYSIS: 'Performance analysis',
  UI_ANALYSIS: 'UI analysis',
  EMBEDDING: 'Embeddings',
  GENERAL_AI: 'General (login suggestion)',
};

/** A task as a person names it; one this page has no words for is its own name in lower case with a capital. */
export function taskLabel(task: string): string {
  const known = TASK_LABEL[task];
  if (known) return known;
  const words = task.toLowerCase().replace(/_/g, ' ').trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : task;
}

/** What a cost rests on, in a line: how many of the calls that answered were priced, and why the others were not. */
export function costBasis(tally: Pick<AiUsageTally, 'answered' | 'priced_calls' | 'no_price_calls' | 'no_tokens_calls'>): string {
  if (tally.answered === 0) return 'nothing has answered yet';
  const parts: string[] = [];
  if (tally.priced_calls > 0) parts.push(`${tally.priced_calls} priced ${plural(tally.priced_calls, 'call', 'calls')}`);
  if (tally.no_price_calls > 0) parts.push(`${tally.no_price_calls} with no price`);
  if (tally.no_tokens_calls > 0) parts.push(`${tally.no_tokens_calls} with no token counts`);
  return parts.join(' · ');
}

/** What the daily bars can show. */
export type BarMetric = 'calls' | 'tokens' | 'cost';
export const BAR_METRICS: ReadonlyArray<{ id: BarMetric; label: string }> = [
  { id: 'calls', label: 'Calls' },
  { id: 'tokens', label: 'Tokens' },
  { id: 'cost', label: 'Cost' },
];

export function metricValue(tally: Pick<AiUsageTally, 'calls' | 'prompt_tokens' | 'completion_tokens' | 'cost_usd'>, metric: BarMetric): number {
  switch (metric) {
    case 'calls':
      return tally.calls;
    case 'tokens':
      return tally.prompt_tokens + tally.completion_tokens;
    case 'cost':
      return tally.cost_usd;
  }
}

/** How tall each day's bar is, from 0 to 1: the busiest day of the period is 1, and a period with nothing in it has no bar at all. */
export function barFractions(days: ReadonlyArray<Parameters<typeof metricValue>[0]>, metric: BarMetric): number[] {
  const values = days.map((d) => metricValue(d, metric));
  const most = Math.max(0, ...values);
  return values.map((v) => (most > 0 ? v / most : 0));
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** `2026-09-26` as `26 Sep`; a text that is not a date is shown as it is. The date is the report's own (already on the viewer's clock),
 *  and the month is written from a fixed list: a locale's own short name for it differs from one browser to the next (`Sep`, `Sept`). */
export function formatDay(day: string): string {
  const match = /^\d{4}-(\d{2})-(\d{2})$/.exec(day);
  if (!match) return day;
  const month = Number(match[1]);
  const date = Number(match[2]);
  if (month < 1 || month > 12 || date < 1 || date > 31) return day;
  return `${date} ${MONTHS[month - 1]}`;
}
