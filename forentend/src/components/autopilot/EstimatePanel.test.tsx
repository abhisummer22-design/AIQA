/**
 * Phase 7 — the estimate as a person reads it: the totals, each step, and where each task's calls would go, with the assumptions
 * behind the figures.
 */
import { cleanup, render, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { EstimatePanel } from './EstimatePanel';
import { estimate } from './autopilotTestData';
import type { AutopilotEstimate } from '../../types/contract';

afterEach(cleanup);

const text = (id: string) => document.getElementById(id)!.textContent ?? '';

describe('EstimatePanel', () => {
  it('shows the cases, calls, time and cost of a free run, each as the range the backend gave', () => {
    render(<EstimatePanel estimate={estimate(false)} />);
    expect(text('estimate-cases')).toContain('6–18');
    expect(text('estimate-cases')).toContain('from 6 requirements');
    expect(text('estimate-calls')).toContain('1–9');
    expect(text('estimate-time')).toContain('4 min–16 min');
    expect(text('estimate-time')).toContain('not counting the wait for your approval');
    expect(text('estimate-cost')).toContain('$0 (free providers)');
    expect(text('estimate-cost')).not.toContain('charges');
  });

  it('says so, and marks the cost, when a provider that charges is used', () => {
    render(<EstimatePanel estimate={estimate(true)} />);
    expect(text('estimate-cost')).toContain('$0.02–$0.05');
    expect(text('estimate-cost')).toContain('a provider that charges is used');
    expect(document.getElementById('estimate-cost')!.className).toContain('amber');
  });

  it('says a cost nobody can state is not known, rather than showing zero', () => {
    const e = estimate(false);
    const unknown: AutopilotEstimate = { ...e, totals: { ...e.totals, cost_usd: null } };
    render(<EstimatePanel estimate={unknown} />);
    expect(text('estimate-cost')).toContain('not known');
    expect(text('estimate-cost')).not.toContain('$0');
  });

  it('lists every step with its calls and time, and a dash where a step asks the AI for nothing', () => {
    render(<EstimatePanel estimate={estimate()} />);
    const rows = Array.from(document.querySelectorAll('#estimate-steps tbody tr')).map((tr) => Array.from(tr.querySelectorAll('td')).map((td) => td.textContent));
    expect(rows[0]).toEqual(['Requirements', '—', '1 s–2 s', '']);
    expect(rows[1]).toEqual(['Crawl', '—', '30 s–2 min', 'as 2 roles']); // 90 s reads as the nearest minute
    expect(rows[2]).toEqual(['Generate', '1–3', '1 min–4 min', '']);
    expect(rows[3]).toEqual(['Execute', '0–6', '2 min–10 min', '']);
  });

  it('shows where a task’s calls would go: the provider, whether it charges, tokens and how the speed is known', () => {
    render(<EstimatePanel estimate={estimate(false)} />);
    const task = document.getElementById('estimate-task-TEST_CASE_GENERATION')!;
    expect(task.textContent).toContain('Writing the test cases');
    expect(task.textContent).toContain('Ollama/qwen3:14b');
    expect(task.textContent).toContain('local');
    expect(task.textContent).toContain('free');
    expect(task.textContent).toContain('12k tokens in');
    expect(task.textContent).toContain('9.0k out');
    expect(task.textContent).toContain('assumed 20 tokens a second (nothing measured yet)');
  });

  it('says a task that no provider can serve has none', () => {
    const e = estimate(false);
    const none: AutopilotEstimate = {
      ...e,
      tasks: [{ ...e.tasks[0], provider: null, cost_usd: null, latency: { basis: 'UNKNOWN' } }],
    };
    render(<EstimatePanel estimate={none} />);
    const task = document.getElementById('estimate-task-TEST_CASE_GENERATION')!;
    expect(within(task).getAllByText('no provider can serve this').length).toBeGreaterThan(0);
  });

  it('says how long a call takes when it has been measured', () => {
    const e = estimate(false);
    const measured: AutopilotEstimate = { ...e, tasks: [{ ...e.tasks[0], latency: { basis: 'MEASURED', calls: 7, avg_seconds: 41 } }] };
    render(<EstimatePanel estimate={measured} />);
    expect(text('estimate-task-TEST_CASE_GENERATION')).toContain('41 s a call, measured over 7 earlier calls');
  });

  it('lists the warnings, and only when there are some', () => {
    const { rerender } = render(<EstimatePanel estimate={estimate()} />);
    expect(document.getElementById('estimate-warnings')).toBeNull();
    rerender(<EstimatePanel estimate={{ ...estimate(), warnings: ['No depth is chosen: the number of cases is only a guess.'] }} />);
    expect(text('estimate-warnings')).toContain('No depth is chosen');
  });

  it('keeps every assumption behind the figures, folded away until asked for', () => {
    render(<EstimatePanel estimate={estimate()} />);
    const details = document.getElementById('estimate-assumptions') as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(details.textContent).toContain('A group of ten requirements is one call.');
  });

  it('says one requirement in the singular', () => {
    render(<EstimatePanel estimate={{ ...estimate(), requirements: 1 }} />);
    expect(text('estimate-cases')).toContain('from 1 requirement');
    expect(text('estimate-cases')).not.toContain('1 requirements');
  });
});
