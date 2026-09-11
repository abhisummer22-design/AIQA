/**
 * Shared real capability metadata for the AI Engine widgets
 * (`AIEnginePanel.tsx`, `AIEngineOrb.tsx`) — pulled out to its own module
 * so both can import the same real order/wording instead of one importing
 * the other (which would be a circular import: `AIEnginePanel` already
 * renders `AIEngineOrb`).
 */
import type { AiCapabilityLevel, AiModelDefinition, AiProviderStatus } from '../../types/contract';

/** Capability keys in a fixed, real order — every one of these is a real
 *  field on `ai_router.rs::ModelCapabilities`, so nothing here is a label
 *  without a backing value. */
export const CAPABILITY_ORDER: Array<{ key: keyof AiModelDefinition['capabilities']; label: string }> = [
  { key: 'text_generation', label: 'text gen' },
  { key: 'structured_output', label: 'structured' },
  { key: 'reasoning', label: 'reasoning' },
  { key: 'tool_use', label: 'tool use' },
  { key: 'long_context', label: 'long ctx' },
  { key: 'vision', label: 'vision' },
  { key: 'embedding', label: 'embedding' },
];

/** Real evidenced level -> a real colour, the same tiers `AIEnginePanel`'s
 *  own `levelStyles` Tailwind classes already use. */
export const CAPABILITY_LEVEL_COLOR: Record<AiCapabilityLevel | 'idle', string> = {
  STRONG: '#34d399',
  BASIC: '#facc15',
  UNKNOWN: '#64748b',
  NONE: '#334155',
  idle: '#334155',
};

/** Real provider health -> a real colour, the same vocabulary
 *  `AIEnginePanel`'s own `providerStatusStyles` Tailwind classes use. */
export const PROVIDER_STATUS_COLOR: Record<AiProviderStatus, string> = {
  HEALTHY: '#34d399',
  AVAILABLE: '#34d399',
  DEGRADED: '#fbbf24',
  NOT_CONFIGURED: '#64748b',
  DISCOVERING: '#a78bfa',
  UNAVAILABLE: '#fb7185',
  ERROR: '#fb7185',
};
