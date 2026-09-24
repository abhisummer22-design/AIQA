import React from 'react';
import { render, screen, within } from '@testing-library/react';
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

  it('marks a call a fallback served, and says so honestly when usage was not reported', () => {
    render(
      <GenerationWaveResults
        waves={[wave({ fallback_used: true, prompt_tokens: null, completion_tokens: null, latency_ms: null })]}
      />,
    );

    const row = screen.getByText('Functional Testing').closest('li')!;
    expect(within(row).getByText('fallback')).toBeTruthy();
    expect(within(row).getByText('Tokens not reported')).toBeTruthy();
    expect(within(row).getByText('Time not reported')).toBeTruthy();
  });
});
