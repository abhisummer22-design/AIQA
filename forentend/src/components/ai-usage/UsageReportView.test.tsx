/**
 * Phase 8A — the report of what the AI calls came to, shown: the tiles, the notes, the bar for each day and the three tables.
 */
import React from 'react';
import { cleanup, fireEvent, render, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';
import { UsageReportView, costCell } from './UsageReportView';
import { everywhereReport, tally, usageReport } from './aiUsageTestData';
import type { AiUsageReport } from '../../types/contract';

afterEach(cleanup);

const NAMES: Record<string, string> = { 'custom-ai-11111111-1111-4111-8111-111111111111': 'Home GPU' };
const providerName = (id: string) => NAMES[id] ?? id;
const projectName = (id: string | null) => (id === null ? 'Not made for a project' : id === 'proj-a' ? 'Project Alpha' : `Project ${id}`);

function show(report: AiUsageReport = usageReport()) {
  return render(
    <MemoryRouter>
      <UsageReportView report={report} providerName={providerName} projectName={projectName} />
    </MemoryRouter>,
  );
}

const el = (id: string) => document.getElementById(id) as HTMLElement;
const text = (id: string) => el(id)?.textContent ?? '';
/** The cells of each row of a table, by row id. */
function rowsOf(tableId: string): Record<string, string[]> {
  const rows: Record<string, string[]> = {};
  el(tableId)
    .querySelectorAll('tbody tr')
    .forEach((tr) => {
      rows[tr.getAttribute('data-row')!] = Array.from(tr.querySelectorAll('td')).map((td) => td.textContent ?? '');
    });
  return rows;
}

describe('the tiles', () => {
  it('say how many calls, and how many answered and failed', () => {
    show();
    expect(text('usage-calls')).toContain('AI calls');
    expect(text('usage-calls')).toContain('12');
    expect(text('usage-calls')).toContain('11 answered · 1 failed');
  });

  it('are amber when a call failed, and plain when none did', () => {
    show();
    expect(el('usage-calls').className).toContain('bg-amber-950/30');
    cleanup();
    show(usageReport({ totals: tally({ calls: 3, answered: 3 }) }));
    expect(el('usage-calls').className).not.toContain('amber');
    expect(text('usage-calls')).toContain('3 answered');
    expect(text('usage-calls')).not.toContain('failed');
  });

  it('give the tokens in and out in few characters, and the exact numbers on hover', () => {
    show();
    expect(text('usage-tokens')).toContain('11k in · 3.6k out');
    expect(el('usage-tokens').querySelector('[title]')?.getAttribute('title')).toBe('11,000 in, 3,600 out (14,600 together)');
  });

  it('say how many of the answered calls reported their tokens, only when some did not', () => {
    show();
    expect(text('usage-tokens')).toContain('reported by 9 of 11 answered calls');
    cleanup();
    show(usageReport({ totals: tally({ calls: 4, answered: 4, token_calls: 4, prompt_tokens: 40, completion_tokens: 8 }) }));
    expect(text('usage-tokens')).not.toContain('reported by');
  });

  it('show the estimated cost and what it rests on', () => {
    show();
    expect(text('usage-cost')).toContain('Estimated cost');
    expect(text('usage-cost')).toContain('$0.001');
    expect(text('usage-cost')).toContain('4 priced calls · 6 with no price · 1 with no token counts');
  });

  it('show a cost of nothing as $0.00, and say when nothing was priced', () => {
    show(usageReport({ totals: tally({ calls: 5, answered: 5, no_price_calls: 5 }) }));
    expect(text('usage-cost')).toContain('$0.00');
    expect(text('usage-cost')).toContain('5 with no price');
  });

  it('show the time the answers took, the average call, and the time lost to failed calls', () => {
    show();
    expect(text('usage-time')).toContain('AI time');
    expect(text('usage-time')).toContain('59 s');
    expect(text('usage-time')).toContain('5 s a call on average');
    expect(text('usage-time')).toContain('30 s spent on failed calls');
  });

  it('leave out the average of nothing, and the time lost when none was', () => {
    show(usageReport({ totals: tally({ calls: 2, answered: 0, failed: 2, failed_ms: 4000 }) }));
    expect(text('usage-time')).not.toContain('on average');
    expect(text('usage-time')).toContain('4 s spent on failed calls');
    cleanup();
    show(usageReport({ totals: tally({ calls: 2, answered: 2, answered_ms: 900, avg_answer_ms: 450 }) }));
    expect(text('usage-time')).toContain('900 ms');
    expect(text('usage-time')).toContain('450 ms a call on average');
    expect(text('usage-time')).not.toContain('spent on failed calls');
  });

  it('have no note at all when there is nothing to note', () => {
    show(usageReport({ totals: tally({ calls: 2, answered: 2, answered_ms: 2000 }) }));
    // The label and the value, and no line under them.
    expect(el('usage-time').children).toHaveLength(2);
    cleanup();
    show();
    expect(el('usage-time').children).toHaveLength(3);
  });
});

describe('the notes', () => {
  it('list what the backend says is left out of the cost, and where a price is set', () => {
    show();
    const items = Array.from(el('usage-notes').querySelectorAll('li')).map((li) => li.textContent);
    expect(items.slice(0, 3)).toEqual(usageReport().notes);
    expect(items[3]).toContain('A price is set per provider, in Settings');
    expect(items[3]).toContain('a local model has none');
    expect(el('usage-notes').querySelector('a')?.getAttribute('href')).toBe('/settings');
  });

  it('keep the line about prices when there is nothing else to note', () => {
    show(usageReport({ notes: [] }));
    expect(el('usage-notes').querySelectorAll('li')).toHaveLength(1);
  });
});

describe('the bars of the days', () => {
  const bars = () => Array.from(el('usage-days').querySelectorAll('li[data-day]')) as HTMLElement[];
  const heights = () => bars().map((b) => (b.querySelector('span') as HTMLElement).style.height);

  it('are one for each day of the report, oldest first, with the first and the last day written under them', () => {
    show();
    expect(bars().map((b) => b.getAttribute('data-day'))).toEqual(['2026-09-24', '2026-09-25', '2026-09-26']);
    expect(text('usage-days-axis')).toBe('24 Sep26 Sep');
  });

  it('show the calls by default: the busiest days are as tall as the bar goes and the others in proportion', () => {
    show();
    expect(el('usage-bars-calls').getAttribute('aria-pressed')).toBe('true');
    expect(el('usage-bars-tokens').getAttribute('aria-pressed')).toBe('false');
    expect(bars().map((b) => b.getAttribute('data-value'))).toEqual(['5', '2', '5']);
    expect(heights()).toEqual(['100%', '40%', '100%']);
  });

  it('can show the tokens or the cost instead, and one is pressed at a time', () => {
    show();
    fireEvent.click(el('usage-bars-tokens'));
    expect(el('usage-bars-tokens').getAttribute('aria-pressed')).toBe('true');
    expect(el('usage-bars-calls').getAttribute('aria-pressed')).toBe('false');
    expect(bars().map((b) => b.getAttribute('data-value'))).toEqual(['7500', '1300', '5800']);
    expect(heights()).toEqual(['100%', `${(1300 / 7500) * 100}%`, `${(5800 / 7500) * 100}%`]);
    fireEvent.click(el('usage-bars-cost'));
    expect(el('usage-bars-cost').getAttribute('aria-pressed')).toBe('true');
    expect(bars().map((b) => b.getAttribute('data-value'))).toEqual(['0.0009', '0.0001', '0']);
    expect(heights()).toEqual(['100%', `${(0.0001 / 0.0009) * 100}%`, '0%']);
  });

  it('draw a day with something in it at least a sliver tall, and a day with nothing not at all', () => {
    show(
      usageReport({
        by_day: [
          { day: '2026-09-24', ...tally({ calls: 1000 }) },
          { day: '2026-09-25', ...tally({ calls: 1 }) },
          { day: '2026-09-26', ...tally() },
        ],
      }),
    );
    expect(heights()).toEqual(['100%', '3%', '0%']);
  });

  it('say on hover what each day came to', () => {
    show();
    expect(bars().map((b) => b.getAttribute('title'))).toEqual([
      '24 Sep: 5 calls, 7.5k tokens, $0.0009',
      '25 Sep: 2 calls, 1.3k tokens, $0.0001',
      '26 Sep: 5 calls, 5.8k tokens, $0.00',
    ]);
    cleanup();
    show(usageReport({ by_day: [{ day: '2026-09-26', ...tally({ calls: 1, prompt_tokens: 10, completion_tokens: 5, cost_usd: 1.75 }) }] }));
    expect(bars()[0].getAttribute('title')).toBe('26 Sep: 1 call, 15 tokens, $1.75');
  });

  it('write nothing under a period with no days', () => {
    show(usageReport({ by_day: [] }));
    expect(bars()).toHaveLength(0);
    expect(el('usage-days-axis')).toBeNull();
  });
});

describe('the table by provider and model', () => {
  it('has a row for each, with its calls, failures, tokens, average answer and cost', () => {
    show();
    const rows = rowsOf('usage-by-provider');
    expect(Object.keys(rows)).toEqual(['groq/llama', 'ollama/qwen']);
    expect(rows['groq/llama']).toEqual(['groq/llama', '6', '1', '6.5k', '1.6k', '2 s', '$0.001*']);
    expect(rows['ollama/qwen']).toEqual(['ollama/qwen', '6', '0', '4.5k', '2k', '9 s', '—']);
  });

  it('names a provider added in Settings by the name it was given', () => {
    const custom = 'custom-ai-11111111-1111-4111-8111-111111111111';
    show(usageReport({ by_provider: [{ provider: custom, model: 'gpt-x', ...tally({ calls: 1, answered: 1 }) }] }));
    expect(rowsOf('usage-by-provider')[`${custom}/gpt-x`][0]).toBe('Home GPU/gpt-x');
  });

  it('shows the whole number of tokens on hover, and why a cost is what it is', () => {
    show();
    const cells = el('usage-by-provider').querySelectorAll('tbody tr:first-child td');
    expect(cells[3].getAttribute('title')).toBe('6,500');
    expect(cells[4].getAttribute('title')).toBe('1,600');
    expect(cells[6].getAttribute('title')).toBe('4 priced calls · 1 with no token counts');
  });

  it('marks the failures amber, and an average of nothing with a dash', () => {
    show(usageReport({ by_provider: [{ provider: 'p', model: 'm', ...tally({ calls: 2, failed: 2, failed_ms: 100 }) }, { provider: 'q', model: 'm', ...tally({ calls: 1, answered: 1, avg_answer_ms: 700 }) }] }));
    const [failing, fine] = Array.from(el('usage-by-provider').querySelectorAll('tbody tr')).map((tr) => Array.from(tr.querySelectorAll('td')));
    expect(failing[2].className).toContain('text-amber-300');
    expect(fine[2].className).not.toContain('amber');
    expect(failing[5].textContent).toBe('—');
    expect(fine[5].textContent).toBe('700 ms');
  });
});

describe('the table by task', () => {
  it('names the tasks in words, the most called first as the backend sent them', () => {
    show();
    const rows = rowsOf('usage-by-task');
    expect(Object.keys(rows)).toEqual(['TEST_CASE_GENERATION', 'GENERAL_AI', 'TEST_CASE_COMPILATION']);
    expect(rows.TEST_CASE_GENERATION[0]).toBe('Test case generation');
    expect(rows.GENERAL_AI[0]).toBe('General (login suggestion)');
    expect(rows.GENERAL_AI.slice(1)).toEqual(['5', '0', '4k', '1.8k', '10 s', '—']);
  });
});

describe('the table by project', () => {
  it('is there only for the usage of every project at once', () => {
    show();
    expect(el('usage-by-project')).toBeNull();
  });

  it('has a row for each project, by its name, and one for the calls made for none', () => {
    show(everywhereReport());
    const rows = rowsOf('usage-by-project');
    expect(Object.keys(rows)).toEqual(['proj-a', 'proj-b', 'none']);
    expect(rows['proj-a'].slice(0, 3)).toEqual(['Project Alpha', '6', '1']);
    expect(rows['proj-b'][0]).toBe('Project proj-b');
    expect(rows.none[0]).toBe('Not made for a project');
    expect(within(el('usage-by-project')).getByText('Project')).toBeTruthy();
  });
});

describe('the cost of a row', () => {
  it('is a dash when no call of the row was priced, with the reason on hover', () => {
    expect(costCell(tally({ calls: 3, answered: 3, no_price_calls: 3 }))).toEqual({ text: '—', title: 'No cost counted: 3 with no price' });
    expect(costCell(tally({ calls: 2, failed: 2 }))).toEqual({ text: '—', title: 'No cost counted: nothing has answered yet' });
  });

  it('is the money when every answered call was priced', () => {
    expect(costCell(tally({ calls: 2, answered: 2, priced_calls: 2, cost_usd: 1.75 }))).toEqual({ text: '$1.75', title: '2 priced calls' });
  });

  it('has a star when some of the calls of the row were left out of it, for either reason', () => {
    expect(costCell(tally({ calls: 2, answered: 2, priced_calls: 1, no_price_calls: 1, cost_usd: 0.5 })).text).toBe('$0.50*');
    expect(costCell(tally({ calls: 2, answered: 2, priced_calls: 1, no_tokens_calls: 1, cost_usd: 0.5 })).text).toBe('$0.50*');
  });

  it('is a cost of nothing, not a dash, when the calls were priced and cost nothing', () => {
    expect(costCell(tally({ calls: 1, answered: 1, priced_calls: 1, cost_usd: 0 })).text).toBe('$0.00');
  });
});

describe('the legend', () => {
  it('says what the dash and the star mean', () => {
    show();
    expect(text('usage-legend')).toContain('— no cost counted');
    expect(text('usage-legend')).toContain('* some calls of the row are left out of its cost');
  });
});

describe('the tables', () => {
  it('are each named by their own heading, so a screen reader says it once', () => {
    show(everywhereReport());
    for (const name of ['By provider and model', 'By task', 'By project']) {
      expect(within(document.body).getByRole('table', { name })).toBeTruthy();
    }
    expect(document.querySelectorAll('caption')).toHaveLength(0);
  });
});
