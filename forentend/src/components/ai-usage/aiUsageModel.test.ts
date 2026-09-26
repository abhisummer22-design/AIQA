/**
 * Phase 8A — the words and figures of the AI usage page: money, counts and times, task names, the bars of the days.
 */
import { describe, expect, it } from 'vitest';
import {
  BAR_METRICS,
  DAY_CHOICES,
  DEFAULT_DAYS,
  barFractions,
  costBasis,
  exactCount,
  formatCount,
  formatDay,
  formatMoney,
  formatMs,
  metricValue,
  taskLabel,
  tzOffsetMinutes,
} from './aiUsageModel';
import { tally } from './aiUsageTestData';

describe('the lengths of time on offer', () => {
  it('are a week, a month and a quarter, and the month is the one it starts on', () => {
    expect(DAY_CHOICES).toEqual([7, 30, 90]);
    expect(DEFAULT_DAYS).toBe(30);
    expect(DAY_CHOICES).toContain(DEFAULT_DAYS);
  });
});

describe('tzOffsetMinutes', () => {
  const clock = (behindUtc: number) => ({ getTimezoneOffset: () => behindUtc }) as unknown as Date;

  it('is how far the clock is ahead of UTC: the browser says how far it is behind, and the sign is turned', () => {
    expect(tzOffsetMinutes(clock(-330))).toBe(330); // India
    expect(tzOffsetMinutes(clock(300))).toBe(-300); // US Central (summer)
    expect(tzOffsetMinutes(clock(-345))).toBe(345); // Nepal
  });

  it('is a plain zero at UTC, never a negative zero', () => {
    expect(Object.is(tzOffsetMinutes(clock(0)), 0)).toBe(true);
  });

  it('is this machine’s own offset when no clock is given', () => {
    expect(tzOffsetMinutes()).toBe(-new Date().getTimezoneOffset() || 0);
  });
});

describe('formatMoney', () => {
  it.each([
    [0, '$0.00'],
    [-1, '$0.00'],
    [Number.NaN, '$0.00'],
    [0.00001, '<$0.0001'],
    [0.00009, '<$0.0001'],
    [0.0001, '$0.0001'],
    [0.0003, '$0.0003'],
    [0.001, '$0.001'],
    [0.42, '$0.42'],
    [1.75, '$1.75'],
    [7, '$7.00'],
    [12.5, '$12.50'],
    [8.75, '$8.75'],
    [1234.5678, '$1234.5678'],
  ])('%s is %s', (usd, text) => {
    expect(formatMoney(usd)).toBe(text);
  });
});

describe('formatCount', () => {
  it.each([
    [0, '0'],
    [7, '7'],
    [999, '999'],
    [1000, '1k'],
    [1500, '1.5k'],
    [9999, '10k'],
    [10_000, '10k'],
    [12_345, '12k'],
    [999_499, '999k'],
    [999_500, '1M'],
    [1_000_000, '1M'],
    [1_500_000, '1.5M'],
    [12_345_678, '12M'],
  ])('%s is %s', (n, text) => {
    expect(formatCount(n)).toBe(text);
  });
});

describe('exactCount', () => {
  it('groups the thousands', () => {
    expect(exactCount(0)).toBe('0');
    expect(exactCount(1234567)).toBe('1,234,567');
  });
});

describe('formatMs', () => {
  it.each([
    [0, '0 ms'],
    [640, '640 ms'],
    [999, '999 ms'],
    [1000, '1 s'],
    [5363, '5 s'],
    [90_000, '2 min'],
    [3_600_000, '1 h'],
  ])('%s ms is %s', (ms, text) => {
    expect(formatMs(ms)).toBe(text);
  });
});

describe('taskLabel', () => {
  it('names the tasks the router serves in words', () => {
    expect(taskLabel('TEST_CASE_GENERATION')).toBe('Test case generation');
    expect(taskLabel('TEST_CASE_COMPILATION')).toBe('Test case compilation');
    expect(taskLabel('REQUIREMENT_ANALYSIS')).toBe('Requirement analysis');
    expect(taskLabel('DOCUMENT_ANALYSIS')).toBe('Document analysis');
    expect(taskLabel('BUG_ANALYSIS')).toBe('Bug analysis');
    expect(taskLabel('TRACEABILITY')).toBe('Requirement gap check');
    expect(taskLabel('SECURITY_ANALYSIS')).toBe('Security analysis');
    expect(taskLabel('PERFORMANCE_ANALYSIS')).toBe('Performance analysis');
    expect(taskLabel('UI_ANALYSIS')).toBe('UI analysis');
    expect(taskLabel('EMBEDDING')).toBe('Embeddings');
    expect(taskLabel('GENERAL_AI')).toBe('General (login suggestion)');
  });

  it('gives a task it has no words for its own name, in lower case with a capital', () => {
    expect(taskLabel('SOME_NEW_TASK')).toBe('Some new task');
    expect(taskLabel('X')).toBe('X');
  });

  it('shows a name that is only underscores as it is', () => {
    expect(taskLabel('__')).toBe('__');
    expect(taskLabel('')).toBe('');
  });
});

describe('costBasis', () => {
  const basis = (over: Parameters<typeof tally>[0]) => costBasis(tally(over));

  it('says nothing has answered when nothing has', () => {
    expect(basis({ calls: 2, failed: 2 })).toBe('nothing has answered yet');
  });

  it('says how many calls were priced and how many were not, and why', () => {
    expect(basis({ answered: 11, priced_calls: 4, no_price_calls: 6, no_tokens_calls: 1 })).toBe('4 priced calls · 6 with no price · 1 with no token counts');
  });

  it('leaves out what has no calls, and says one call in the singular', () => {
    expect(basis({ answered: 1, priced_calls: 1 })).toBe('1 priced call');
    expect(basis({ answered: 3, no_price_calls: 3 })).toBe('3 with no price');
    expect(basis({ answered: 2, priced_calls: 1, no_tokens_calls: 1 })).toBe('1 priced call · 1 with no token counts');
  });
});

describe('the bars of the days', () => {
  const days = [
    tally({ calls: 1, prompt_tokens: 100, completion_tokens: 50, cost_usd: 0.5 }),
    tally({ calls: 2, prompt_tokens: 300, completion_tokens: 100, cost_usd: 0.25 }),
    tally({ calls: 4, prompt_tokens: 20, completion_tokens: 20, cost_usd: 1 }),
    tally(),
  ];

  it('offers calls, tokens and cost', () => {
    expect(BAR_METRICS.map((m) => m.id)).toEqual(['calls', 'tokens', 'cost']);
    expect(BAR_METRICS.map((m) => m.label)).toEqual(['Calls', 'Tokens', 'Cost']);
  });

  it('takes the value of a day for each of them; tokens are those in and those out together', () => {
    expect(metricValue(days[1], 'calls')).toBe(2);
    expect(metricValue(days[1], 'tokens')).toBe(400);
    expect(metricValue(days[1], 'cost')).toBe(0.25);
  });

  it('makes the busiest day the full height and the others in proportion to it', () => {
    expect(barFractions(days, 'calls')).toEqual([0.25, 0.5, 1, 0]);
    expect(barFractions(days, 'tokens')).toEqual([150 / 400, 1, 40 / 400, 0]);
    expect(barFractions(days, 'cost')).toEqual([0.5, 0.25, 1, 0]);
  });

  it('draws no bar at all for a period with nothing in it', () => {
    expect(barFractions([tally(), tally()], 'calls')).toEqual([0, 0]);
    expect(barFractions([], 'calls')).toEqual([]);
  });
});

describe('formatDay', () => {
  it('writes a date as the day and the month, whatever the clock of this machine', () => {
    expect(formatDay('2026-09-26')).toBe('26 Sep');
    expect(formatDay('2026-01-05')).toBe('5 Jan');
    expect(formatDay('2026-12-31')).toBe('31 Dec');
  });

  it('names every month the same way in every browser', () => {
    const months = Array.from({ length: 12 }, (_, i) => formatDay(`2026-${String(i + 1).padStart(2, '0')}-01`));
    expect(months).toEqual(['1 Jan', '1 Feb', '1 Mar', '1 Apr', '1 May', '1 Jun', '1 Jul', '1 Aug', '1 Sep', '1 Oct', '1 Nov', '1 Dec']);
  });

  it('shows text that is not a date as it is', () => {
    expect(formatDay('yesterday')).toBe('yesterday');
    expect(formatDay('2026-09-26T10:00:00Z')).toBe('2026-09-26T10:00:00Z');
    expect(formatDay('2026-13-01')).toBe('2026-13-01');
    expect(formatDay('2026-00-10')).toBe('2026-00-10');
    expect(formatDay('2026-09-00')).toBe('2026-09-00');
    expect(formatDay('2026-09-32')).toBe('2026-09-32');
  });
});
