import React from 'react';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { GenerationWaveResults } from './GenerationWaveResults';
import type { GenerationWaveSummary } from '../../types/contract';

function wave(overrides: Partial<GenerationWaveSummary> = {}): GenerationWaveSummary {
  return {
    wave: 'base',
    skill_names: ['Functional Testing'],
    case_count: 3,
    provider: 'ollama',
    model: 'qwen3:14b',
    routing_score: 270,
    fallback_used: false,
    routing_mode: 'AUTO',
    prompt_tokens: 2000,
    completion_tokens: 900,
    estimated_cost_usd: 0,
    latency_ms: 4200,
    failure_reason: null,
    skipped: false,
    ...overrides,
  };
}

const failed = (skill: string, reason: string): GenerationWaveSummary =>
  wave({
    skill_names: [skill],
    case_count: 0,
    provider: null,
    model: null,
    prompt_tokens: null,
    completion_tokens: null,
    latency_ms: null,
    failure_reason: reason,
  });

describe('GenerationWaveResults', () => {
  it('renders nothing when no skill call was made', () => {
    const { container } = render(<GenerationWaveResults waves={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('shows the provider, model, case count, tokens and time of every successful skill call, and no warning', () => {
    render(<GenerationWaveResults waves={[wave(), wave({ skill_names: ['Edge Case Analysis'], case_count: 1 })]} />);

    expect(screen.getByText('Per-skill results: 2 of 2 succeeded')).toBeTruthy();
    expect(screen.getAllByText('ollama / qwen3:14b')).toHaveLength(2);
    expect(screen.getByText('3 cases')).toBeTruthy();
    expect(screen.getByText('1 case')).toBeTruthy();
    expect(screen.getAllByText('2000 in / 900 out')).toHaveLength(2);
    expect(screen.getAllByText('4.2s')).toHaveLength(2);
    expect(document.getElementById('generation-wave-warning')).toBeNull();
  });

  it('names every failed skill in the warning and shows each failure reason on its own row', () => {
    render(
      <GenerationWaveResults
        waves={[
          wave(),
          failed('Negative Testing', 'HTTP 429: Rate limit reached'),
          failed('Security Testing', 'empty response: the model used its whole 8192-token budget'),
        ]}
      />,
    );

    expect(screen.getByText('Per-skill results: 1 of 3 succeeded')).toBeTruthy();
    const warning = screen.getByText(/2 skill calls failed/);
    expect(warning.textContent).toContain('Negative Testing; Security Testing');
    expect(screen.getByText('HTTP 429: Rate limit reached')).toBeTruthy();
    expect(screen.getByText(/used its whole 8192-token budget/)).toBeTruthy();
    expect(screen.getAllByText('FAILED')).toHaveLength(2);
  });

  it('shows the name a provider was given rather than its id when a name lookup is passed, and the id itself otherwise', () => {
    const custom = wave({ provider: 'custom-ai-3f2a', model: 'gpt-4o' });
    const { rerender } = render(<GenerationWaveResults waves={[custom]} />);
    expect(screen.getByText('custom-ai-3f2a / gpt-4o')).toBeTruthy();

    rerender(<GenerationWaveResults waves={[custom]} providerName={(id) => (id === 'custom-ai-3f2a' ? 'My Paid API' : id)} />);
    expect(screen.getByText('My Paid API / gpt-4o')).toBeTruthy();
    expect(screen.queryByText(/custom-ai-3f2a/)).toBeNull();
  });

  // The label is "provider fallback", deliberately not a bare "fallback": this
  // is the router serving a wave from a provider other than the one it
  // preferred, which is a different fact from the request-level generation
  // method the Generation Details tile reports. Both used to read "fallback",
  // so a run that was pure AI could show "Fallback: No" here while every row
  // claimed a fallback had occurred.
  it('marks a call a provider fallback served (not a bare "fallback"), and says so honestly when usage was not reported', () => {
    render(
      <GenerationWaveResults
        waves={[wave({ fallback_used: true, prompt_tokens: null, completion_tokens: null, latency_ms: null })]}
      />,
    );

    const row = screen.getByText('Functional Testing').closest('li')!;
    expect(within(row).getByText('provider fallback')).toBeTruthy();
    expect(within(row).getByText('Tokens not reported')).toBeTruthy();
    expect(within(row).getByText('Time not reported')).toBeTruthy();
  });

  it('says which batch of requirements a call carried, and how many, only for a call from requirements', () => {
    render(
      <GenerationWaveResults
        waves={[
          wave({ batch: '2/5', requirements: 14 }),
          wave({ skill_names: ['Edge Case Analysis'], batch: '1/1', requirements: 1 }),
          wave({ skill_names: ['Negative Testing'] }),
        ]}
      />,
    );

    expect(within(screen.getByText('Functional Testing').closest('li')!).getByText('batch 2/5 · 14 requirements')).toBeTruthy();
    expect(within(screen.getByText('Edge Case Analysis').closest('li')!).getByText('batch 1/1 · 1 requirement')).toBeTruthy();
    expect(screen.getByText('Negative Testing').closest('li')!.textContent).not.toContain('batch');
  });

  it('names the batch that failed in the warning, so it is clear which requirements got nothing from that skill', () => {
    render(
      <GenerationWaveResults
        waves={[
          wave({ batch: '1/3', requirements: 10 }),
          { ...failed('Functional Testing', 'HTTP 429: Rate limit reached'), batch: '2/3', requirements: 10 },
          failed('Negative Testing', 'HTTP 500'),
        ]}
      />,
    );

    const warning = screen.getByText(/2 skill calls failed/);
    expect(warning.textContent).toContain('Functional Testing (batch 2/3); Negative Testing');
  });

  it('says a top-up call was for a technique, and its place among the calls of its round', () => {
    render(
      <GenerationWaveResults
        waves={[
          wave({ batch: '1/2', requirements: 14 }),
          wave({ wave: 'top_up', skill_names: ['Negative Testing'], technique: 'negative', batch: '1/2', requirements: 40 }),
          wave({ wave: 'top_up', skill_names: ['Edge Case Analysis'], technique: 'edge', batch: '2/2', requirements: 1 }),
        ]}
      />,
    );

    const row = (skill: string) => screen.getByText(skill).closest('li') as HTMLElement;
    expect(within(row('Negative Testing')).getByText('top-up negative · call 1/2 · 40 requirements')).toBeTruthy();
    expect(within(row('Edge Case Analysis')).getByText('top-up edge · call 2/2 · 1 requirement')).toBeTruthy();
    expect(within(row('Functional Testing')).getByText('batch 1/2 · 14 requirements')).toBeTruthy();
  });

  it('names the technique of a failed top-up call in the warning, so it is clear what is still missing', () => {
    render(
      <GenerationWaveResults
        waves={[
          wave(),
          { ...failed('Negative Testing', 'HTTP 400: refused'), wave: 'top_up', technique: 'negative', batch: '1/3', requirements: 9 },
        ]}
      />,
    );

    expect(screen.getByText(/1 skill call failed/).textContent).toContain('Negative Testing (top-up negative, call 1/3)');
    expect(screen.getByText('HTTP 400: refused')).toBeTruthy();
  });

  describe('a run with many calls (one per skill per batch)', () => {
    const many = (n: number, failedAt: number[] = []) =>
      Array.from({ length: n }, (_, i) =>
        failedAt.includes(i)
          ? { ...failed(`Skill ${i + 1}`, `reason ${i + 1}`), batch: `${i + 1}/${n}`, requirements: 5 }
          : wave({ skill_names: [`Skill ${i + 1}`], batch: `${i + 1}/${n}`, requirements: 5 }),
      );

    it('lists every call, with nothing folded away, up to twelve of them', () => {
      render(<GenerationWaveResults waves={many(12)} />);
      expect(within(document.getElementById('generation-wave-results')!).getAllByRole('listitem')).toHaveLength(12);
      expect(document.getElementById('btn-toggle-wave-rows')).toBeNull();
    });

    it('keeps the calls in the order they ran while they all fit, a failed one included', () => {
      render(<GenerationWaveResults waves={many(12, [5])} />);

      const items = within(document.getElementById('generation-wave-results')!).getAllByRole('listitem');
      expect(items[0].textContent).toContain('Skill 1');
      expect(items[5].textContent).toContain('Skill 6');
      expect(items[5].textContent).toContain('FAILED');
    });

    it('folds a longer list to twelve, failed calls first, and says how many there are altogether', () => {
      render(<GenerationWaveResults waves={many(30, [25, 28])} />);

      const items = within(document.getElementById('generation-wave-results')!).getAllByRole('listitem');
      expect(items).toHaveLength(12);
      // The two failures are shown though they are far down the run.
      expect(items[0].textContent).toContain('Skill 26');
      expect(items[1].textContent).toContain('Skill 29');
      expect(items[2].textContent).toContain('Skill 1');
      // The overall figures still count every call.
      expect(screen.getByText('Per-skill results: 28 of 30 succeeded')).toBeTruthy();
      expect(document.getElementById('btn-toggle-wave-rows')!.textContent).toBe('Show all 30 calls');
    });

    it('shows all of them, in the order they ran, when asked, and folds them again', () => {
      render(<GenerationWaveResults waves={many(30, [25, 28])} />);

      fireEvent.click(document.getElementById('btn-toggle-wave-rows')!);
      const items = within(document.getElementById('generation-wave-results')!).getAllByRole('listitem');
      expect(items).toHaveLength(30);
      expect(items[0].textContent).toContain('Skill 1');
      expect(items[25].textContent).toContain('Skill 26');
      expect(document.getElementById('btn-toggle-wave-rows')!.textContent).toBe('Show fewer calls');

      fireEvent.click(document.getElementById('btn-toggle-wave-rows')!);
      expect(within(document.getElementById('generation-wave-results')!).getAllByRole('listitem')).toHaveLength(12);
    });
  });
});
