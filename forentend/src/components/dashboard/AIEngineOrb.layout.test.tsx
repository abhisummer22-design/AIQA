/**
 * Where `AIEngineOrb` puts its decision label. With four or more providers the last provider node's own label
 * is drawn in the canvas's bottom margin, so a label laid over that margin covers it. jsdom has no layout, so
 * this pins the structure that rules the overlap out: the label sits after the canvas's own box, outside it,
 * and is not absolutely positioned. (`DashboardPage.test.tsx` covers what the label says.)
 */
import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AIEngineOrb } from './AIEngineOrb';
import type { AiModelDefinition, AiProviderInfo } from '../../types/contract';

const MODEL: AiModelDefinition = {
  id: 'qwen/qwen3.8-27b',
  provider_id: 'groq',
  display_name: 'qwen/qwen3.8-27b',
  family: null,
  parameter_size: '27b',
  context_window: null,
  capabilities: {
    text_generation: 'STRONG',
    structured_output: 'STRONG',
    reasoning: 'STRONG',
    tool_use: 'UNKNOWN',
    long_context: 'STRONG',
    vision: 'NONE',
    embedding: 'NONE',
  },
  status: 'HEALTHY',
};

function provider(id: string): AiProviderInfo {
  return {
    id,
    name: id,
    provider_type: 'API',
    endpoint: null,
    configured: true,
    reachable: true,
    status: 'HEALTHY',
    models: [],
    pricing: { kind: 'UNMETERED' },
  };
}

const FOUR_PROVIDERS = ['ollama', 'mj-gateway', 'groq', 'gemini'].map(provider);

function labelOf(container: HTMLElement): { canvasBox: HTMLElement; label: HTMLElement } {
  const orb = container.querySelector('#ai-engine-orb')!;
  const canvasBox = orb.querySelector('canvas')!.parentElement!;
  return { canvasBox, label: canvasBox.nextElementSibling as HTMLElement };
}

describe('AIEngineOrb — the decision label', () => {
  it('sits below the canvas box instead of over it', () => {
    const { container } = render(
      <AIEngineOrb model={MODEL} routingScore={330} providers={FOUR_PROVIDERS} activeProviderId="groq" />,
    );
    const { canvasBox, label } = labelOf(container);
    expect(label.textContent).toBe('groq · score 330');
    expect(canvasBox.contains(label)).toBe(false);
    expect(label.className).not.toMatch(/\babsolute\b/);
  });

  it('does the same when there is no route', () => {
    const { container } = render(
      <AIEngineOrb model={null} routingScore={null} providers={FOUR_PROVIDERS} activeProviderId={null} />,
    );
    const { canvasBox, label } = labelOf(container);
    expect(label.textContent).toBe('NO ROUTE');
    expect(canvasBox.contains(label)).toBe(false);
    expect(label.className).not.toMatch(/\babsolute\b/);
  });
});
