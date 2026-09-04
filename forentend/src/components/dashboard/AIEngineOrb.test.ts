/**
 * Unit tests for `AIEngineOrb.tsx`'s pure, exported helpers — the parts
 * that don't need a canvas (jsdom has none; see `neuralActivity.ts`'s own
 * doc comment on degrading gracefully without one) and don't need
 * `AIEngineOrb` itself rendered (`DashboardPage.test.tsx` already covers
 * the full render + real data attributes end to end).
 */
import { describe, expect, it } from 'vitest';
import { activeProviderNodeIndex, capabilityNodeColors, capabilityNodeLabels } from './AIEngineOrb';
import { CAPABILITY_LEVEL_COLOR, CAPABILITY_ORDER } from './aiCapabilities';
import type { AiModelDefinition, AiProviderInfo } from '../../types/contract';

const MODEL: AiModelDefinition = {
  id: 'gemma3:12b',
  provider_id: 'ollama',
  display_name: 'gemma3:12b',
  family: 'gemma3',
  parameter_size: '12.2B',
  context_window: null,
  capabilities: {
    text_generation: 'STRONG',
    structured_output: 'BASIC',
    reasoning: 'BASIC',
    tool_use: 'STRONG',
    long_context: 'UNKNOWN',
    vision: 'NONE',
    embedding: 'NONE',
  },
  status: 'AVAILABLE',
};

function provider(id: string, status: AiProviderInfo['status']): AiProviderInfo {
  return {
    id,
    name: id,
    provider_type: 'LOCAL',
    endpoint: null,
    configured: true,
    reachable: true,
    status,
    models: [],
    pricing: { kind: 'UNMETERED' },
  };
}

describe('capabilityNodeColors', () => {
  it('colours each real field by its own real evidenced level, in the real fixed CAPABILITY_ORDER', () => {
    const colors = capabilityNodeColors(MODEL);
    expect(colors).toEqual(
      CAPABILITY_ORDER.map(({ key }) => CAPABILITY_LEVEL_COLOR[MODEL.capabilities[key]]),
    );
    // Spot-check the real, distinct colours actually differ — this must
    // be genuinely colourful, not one hue repeated 7 times.
    expect(new Set(colors).size).toBeGreaterThan(1);
  });

  it('falls back to the honest idle colour for every field when there is no real model at all', () => {
    const colors = capabilityNodeColors(null);
    expect(colors).toEqual(CAPABILITY_ORDER.map(() => CAPABILITY_LEVEL_COLOR.idle));
  });
});

describe('capabilityNodeLabels', () => {
  it('is the real, fixed capability wording — the exact same order/text AIEnginePanel shows', () => {
    expect(capabilityNodeLabels()).toEqual(CAPABILITY_ORDER.map(({ label }) => label));
  });
});

describe('activeProviderNodeIndex — the one node whose edges are allowed to animate', () => {
  it('finds the real winning provider among several real discovered providers', () => {
    const providers = [provider('ollama', 'HEALTHY'), provider('mj-gateway', 'NOT_CONFIGURED')];
    expect(activeProviderNodeIndex(providers, 'ollama')).toBe(0);
    expect(activeProviderNodeIndex(providers, 'mj-gateway')).toBe(1);
  });

  it('is null when there is no real active provider id at all (no route)', () => {
    const providers = [provider('ollama', 'HEALTHY'), provider('mj-gateway', 'NOT_CONFIGURED')];
    expect(activeProviderNodeIndex(providers, null)).toBeNull();
  });

  it('is null when the real active id genuinely matches no discovered provider, rather than a stale/wrong index', () => {
    const providers = [provider('ollama', 'HEALTHY')];
    expect(activeProviderNodeIndex(providers, 'a-provider-that-was-removed')).toBeNull();
  });

  it('a real discovered-but-unconfigured provider is never mistaken for the active one', () => {
    // The exact scenario this fix closes: mj-gateway is genuinely
    // discovered and shown, but it must never be treated as "on the real
    // path" just because ollama (the real winner) is also in the list.
    const providers = [provider('ollama', 'HEALTHY'), provider('mj-gateway', 'NOT_CONFIGURED')];
    const activeIndex = activeProviderNodeIndex(providers, 'ollama');
    const mjGatewayIndex = providers.findIndex((p) => p.id === 'mj-gateway');
    expect(activeIndex).not.toBe(mjGatewayIndex);
  });
});
