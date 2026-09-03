/**
 * AI Engine Orb — the real, live routing decision as an actual layered
 * network diagram: real capability signals (input) -> real discovered
 * providers (hidden) -> the real chosen model (output), fully connected
 * exactly like a classic neural-network architecture diagram, because
 * that fan-out/fan-in *is* what "AUTO mode: best available model by
 * routing score" genuinely does — every capability signal is weighed
 * against every real provider before one wins.
 *
 * Replaces the earlier single-hub "fan into one point" version, which
 * read as a starburst rather than a network. Same non-negotiable rule as
 * before and as everywhere else in this app: **motion and colour must
 * encode something real, or not appear.**
 *
 *   - **Input layer** — one node per real field on `AiModelCapabilities`
 *     (always 7, because the real field count is always 7), coloured by
 *     that field's real evidenced level. Unchanged from the previous
 *     version.
 *   - **Hidden layer** — one node per real discovered provider (from
 *     `GET /api/v1/ai/status`'s own `providers[]`, never a fixed
 *     decorative count), coloured by that provider's real `status`.
 *   - **Output node** — the one real routing decision: the real score
 *     shown verbatim, or an honest "NO ROUTE".
 *   - **Every real edge is drawn** (every input to every provider, every
 *     provider to the output) — the mesh itself is real: every provider
 *     genuinely gets scored against every capability signal. Only the
 *     edges actually touching the real winning provider animate a
 *     flowing pulse; every other edge is the same plain idle line,
 *     because it did not, in fact, carry the winning decision.
 *   - **The winning provider's node gets a highlighted ring** — the one
 *     real fact "this provider is the one currently chosen", shown once
 *     on the node itself, not restated as a separate decorative badge.
 *
 * Layout is fixed and deterministic (the same real inputs always draw the
 * same diagram) rather than force-directed, matching every other
 * real-time diagram in this app (`NeuralGraph`).
 */

import React, { useId, useMemo } from 'react';
import type { AiCapabilityLevel, AiModelDefinition, AiProviderInfo, AiProviderStatus } from '../../types/contract';

export interface AIEngineOrbProps {
  /** The real model that would be routed to, or `null` when the backend
   *  reports no compatible model. */
  model: AiModelDefinition | null;
  /** Real routing score, shown inside the output node when a route
   *  exists. */
  routingScore: number | null;
  /** Every real discovered provider — the diagram's hidden layer, one
   *  node per real provider, never a fixed decorative count. */
  providers: AiProviderInfo[];
  /** The real provider id the routing decision actually chose, or `null`
   *  when there is no route. */
  activeProviderId: string | null;
}

/** Fixed, real order — matches `AIEnginePanel`'s own `CAPABILITY_ORDER` so
 *  the Nth input node from the top is always the same real field as the
 *  Nth row in the capability profile list below it. */
const CAPABILITY_KEYS: Array<keyof AiModelDefinition['capabilities']> = [
  'text_generation',
  'structured_output',
  'reasoning',
  'tool_use',
  'long_context',
  'vision',
  'embedding',
];

const LEVEL_COLOR: Record<AiCapabilityLevel | 'idle', string> = {
  STRONG: '#34d399',
  BASIC: '#22d3ee',
  UNKNOWN: '#64748b',
  NONE: '#334155',
  idle: '#334155',
};

/** Same real health vocabulary `AIEnginePanel`'s own `providerStatusStyles`
 *  uses, as hex values for SVG fills instead of Tailwind classes. */
const PROVIDER_STATUS_COLOR: Record<AiProviderStatus, string> = {
  HEALTHY: '#34d399',
  AVAILABLE: '#34d399',
  DEGRADED: '#fbbf24',
  NOT_CONFIGURED: '#64748b',
  DISCOVERING: '#818cf8',
  UNAVAILABLE: '#fb7185',
  ERROR: '#fb7185',
};

const VB_W = 340;
const VB_H = 200;
const INPUT_X = 34;
const HIDDEN_X = 172;
const OUTPUT_X = 306;
const NODE_R = 8;
const OUTPUT_R = 13;
const COLUMN_TOP = 14;
const COLUMN_BOTTOM = 170;
const LABEL_Y = 192;

/** Centers `count` nodes vertically around the column's real midpoint —
 *  real math, no randomness, so the same real provider/capability count
 *  always draws the same column, regardless of how many nodes a
 *  neighbouring column happens to have. */
function columnY(i: number, count: number, spacing: number): number {
  const mid = (COLUMN_TOP + COLUMN_BOTTOM) / 2;
  if (count <= 1) return mid;
  const span = (count - 1) * spacing;
  return mid - span / 2 + i * spacing;
}

interface InputNode {
  key: keyof AiModelDefinition['capabilities'];
  level: AiCapabilityLevel | 'idle';
  color: string;
  y: number;
}

interface HiddenNode {
  provider: AiProviderInfo;
  color: string;
  isActive: boolean;
  y: number;
}

/** One straight, directional connection with an arrowhead. Reuses the
 *  same "glow underlay + bright core + animated flowing dash" technique
 *  `NeuralEdge` uses elsewhere in this app, as a local straight-line
 *  variant — a dense fully-connected mesh needs straight lines to stay
 *  legible, which `NeuralEdge`'s deliberate curve (right for the radial
 *  Neural Graph) does not suit here, and this mesh's directional
 *  input->output reading wants an arrowhead `NeuralEdge` doesn't draw. */
const Edge: React.FC<{ x1: number; y1: number; x2: number; y2: number; active: boolean; arrowId: string }> = ({
  x1,
  y1,
  x2,
  y2,
  active,
  arrowId,
}) => {
  const len = Math.hypot(x2 - x1, y2 - y1) || 1;
  if (!active) {
    return (
      <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="#334155" strokeWidth={1} opacity={0.4} markerEnd={`url(#${arrowId}-idle)`} />
    );
  }
  return (
    <g>
      <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="#38bdf8" strokeWidth={3.5} strokeLinecap="round" opacity={0.16} />
      <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="#67e8f9" strokeWidth={1.5} opacity={0.85} markerEnd={`url(#${arrowId}-active)`} />
      <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="#f0fdff" strokeWidth={1.5} strokeDasharray={`${Math.max(6, len * 0.16)} ${len}`}>
        <animate attributeName="stroke-dashoffset" from={len} to="0" dur="1.3s" repeatCount="indefinite" />
      </line>
    </g>
  );
};

export const AIEngineOrb: React.FC<AIEngineOrbProps> = ({ model, routingScore, providers, activeProviderId }) => {
  const gradId = useId();
  const live = model !== null && routingScore !== null;

  const inputs = useMemo<InputNode[]>(
    () =>
      CAPABILITY_KEYS.map((key, i) => {
        const level: AiCapabilityLevel | 'idle' = live
          ? ((model!.capabilities[key] ?? 'UNKNOWN') as AiCapabilityLevel)
          : 'idle';
        return { key, level, color: LEVEL_COLOR[level], y: columnY(i, CAPABILITY_KEYS.length, 22) };
      }),
    [live, model],
  );

  const hidden = useMemo<HiddenNode[]>(
    () =>
      providers.map((provider, i) => ({
        provider,
        color: PROVIDER_STATUS_COLOR[provider.status],
        isActive: live && provider.id === activeProviderId,
        y: columnY(i, providers.length, 26),
      })),
    [providers, live, activeProviderId],
  );

  const outputY = columnY(0, 1, 0);

  return (
    <div id="ai-engine-orb" data-live={live} className="relative w-full py-1" aria-hidden="true">
      <svg viewBox={`0 0 ${VB_W} ${VB_H}`} width="100%" height={VB_H} preserveAspectRatio="xMidYMid meet">
        <defs>
          <radialGradient id={`${gradId}-out`} cx="45%" cy="40%">
            <stop offset="0%" stopColor={live ? '#5eead4' : '#475569'} />
            <stop offset="55%" stopColor={live ? '#0891b2' : '#334155'} />
            <stop offset="100%" stopColor={live ? '#0e7490' : '#1e293b'} />
          </radialGradient>
          <marker id={`${gradId}-idle`} viewBox="0 0 10 10" refX="8" refY="5" markerWidth="4.5" markerHeight="4.5" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="#334155" />
          </marker>
          <marker id={`${gradId}-active`} viewBox="0 0 10 10" refX="8" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="#67e8f9" />
          </marker>
        </defs>

        {/* input -> hidden: the real, fully-connected mesh — every
            capability signal genuinely informs every provider's score. */}
        {inputs.map((inp) =>
          hidden.map((h) => (
            <Edge
              key={`${inp.key}-${h.provider.id}`}
              x1={INPUT_X}
              y1={inp.y}
              x2={HIDDEN_X}
              y2={h.y}
              active={h.isActive}
              arrowId={gradId}
            />
          )),
        )}
        {/* hidden -> output: only the real winner's edge flows. */}
        {hidden.map((h) => (
          <Edge
            key={`out-${h.provider.id}`}
            x1={HIDDEN_X}
            y1={h.y}
            x2={OUTPUT_X}
            y2={outputY}
            active={h.isActive}
            arrowId={gradId}
          />
        ))}

        {/* Input layer — seven real capability signals. */}
        {inputs.map((inp) => (
          <g
            key={inp.key}
            data-node="input"
            data-capability={inp.key}
            data-level={inp.level}
            transform={`translate(${INPUT_X}, ${inp.y})`}
          >
            <circle r={NODE_R} fill="#0B1120" stroke={inp.color} strokeWidth={1.75} />
            <circle
              r={NODE_R - 3.5}
              fill={inp.color}
              style={{
                filter: inp.level === 'STRONG' || inp.level === 'BASIC' ? `drop-shadow(0 0 3px ${inp.color})` : undefined,
              }}
            />
          </g>
        ))}

        {/* Hidden layer — real discovered providers. */}
        {hidden.map((h) => (
          <g
            key={h.provider.id}
            data-node="hidden"
            data-provider={h.provider.id}
            data-status={h.provider.status}
            data-active={h.isActive}
            transform={`translate(${HIDDEN_X}, ${h.y})`}
          >
            {h.isActive && <circle r={NODE_R + 6} fill="none" stroke={h.color} strokeWidth={1.5} opacity={0.6} />}
            <circle r={NODE_R} fill="#0B1120" stroke={h.color} strokeWidth={1.75} />
            <circle r={NODE_R - 3.5} fill={h.color} style={{ filter: `drop-shadow(0 0 3px ${h.color})` }} />
          </g>
        ))}

        {/* Output — the real decision, or an honest absence. */}
        <g data-node="output" transform={`translate(${OUTPUT_X}, ${outputY})`}>
          <circle r={OUTPUT_R} fill={`url(#${gradId}-out)`} stroke={live ? '#67e8f9' : '#475569'} strokeWidth={1.75} />
          {live && routingScore !== null ? (
            <text
              textAnchor="middle"
              dominantBaseline="central"
              fill="#ecfeff"
              style={{ fontSize: 11, fontFamily: 'monospace', fontWeight: 700 }}
            >
              {routingScore.toFixed(0)}
            </text>
          ) : (
            <text textAnchor="middle" fill="#cbd5e1" style={{ fontSize: 6, fontFamily: 'monospace', fontWeight: 700 }}>
              <tspan x="0" dy="-1">
                NO
              </tspan>
              {' '}
              <tspan x="0" dy="7">
                ROUTE
              </tspan>
            </text>
          )}
        </g>

        {/* Column labels — the same real terms this panel already uses
            ("Providers" is `AIEnginePanel`'s own section heading below). */}
        <text
          x={INPUT_X}
          y={LABEL_Y}
          textAnchor="middle"
          fill="#64748b"
          style={{ fontSize: 8, fontFamily: 'monospace', fontWeight: 700, letterSpacing: '0.05em' }}
        >
          INPUT
        </text>
        <text
          x={HIDDEN_X}
          y={LABEL_Y}
          textAnchor="middle"
          fill="#64748b"
          style={{ fontSize: 8, fontFamily: 'monospace', fontWeight: 700, letterSpacing: '0.05em' }}
        >
          PROVIDERS
        </text>
        <text
          x={OUTPUT_X}
          y={LABEL_Y}
          textAnchor="middle"
          fill="#64748b"
          style={{ fontSize: 8, fontFamily: 'monospace', fontWeight: 700, letterSpacing: '0.05em' }}
        >
          OUTPUT
        </text>
      </svg>
    </div>
  );
};
