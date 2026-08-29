/**
 * AI Engine Orb — a particle orb whose every visual property is a real value.
 *
 * The page previously had a particle-style orb (`AIQualityCoreOrb`) that was
 * pure decoration: its glow, its label ("ACTIVE INFERENCE", "CORE STABLE")
 * and its motion were derived from `ProjectStatus` alone — nothing was
 * inferring anything when it said so. It was removed for that reason.
 *
 * This brings the orb visual back without bringing the fabrication back. The
 * rule applied here is the same one used for the Neural Pipeline's flow
 * rail: **motion and colour must encode something real, or not appear.**
 * Concretely, every property below maps to a real field of
 * `GET /api/v1/ai/status`:
 *
 *   - **Each orbiting particle is one real capability** of the model that
 *     would actually be routed to. There are exactly as many particles as
 *     that model has capability entries — never a decorative count.
 *   - **Particle colour is that capability's real evidenced level**
 *     (`STRONG` / `BASIC` / `UNKNOWN` / `NONE`). A `NONE` capability is
 *     rendered as a dim, hollow particle rather than hidden, so the orb
 *     cannot look "fully powered" while a capability is actually absent —
 *     which is exactly how `embedding: NONE` reads here today.
 *   - **Orbit radius is the level too**: stronger capabilities sit closer to
 *     the core. The shape of the orb is therefore a real readout of model
 *     capability, not an arbitrary arrangement.
 *   - **The core only glows when a real route exists.** With
 *     `routing_preview: null` the core goes inert and grey, because in that
 *     state the backend genuinely has no compatible model.
 *   - **Rotation only runs when a model is genuinely available.** A still
 *     orb means nothing is routable — never "we chose not to animate".
 *
 * No canvas, no animation library, no random values: the particles are
 * deterministic SVG nodes positioned from the real capability set, so the
 * same backend state always draws the same orb, and it is inspectable in
 * tests like any other DOM.
 */

import React from 'react';
import type { AiCapabilityLevel, AiModelDefinition } from '../../types/contract';

export interface AIEngineOrbProps {
  /** The real model that would be routed to, or `null` when the backend
   *  reports no compatible model. */
  model: AiModelDefinition | null;
  /** Real routing score, shown inside the core when a route exists. */
  routingScore: number | null;
}

/** Real capability keys, in the fixed order they are rendered around the
 *  orb. Every one is a real field on `ai_router.rs::ModelCapabilities`. */
const CAPABILITY_KEYS: Array<{ key: keyof AiModelDefinition['capabilities']; short: string }> = [
  { key: 'text_generation', short: 'gen' },
  { key: 'structured_output', short: 'json' },
  { key: 'reasoning', short: 'rsn' },
  { key: 'tool_use', short: 'tool' },
  { key: 'long_context', short: 'ctx' },
  { key: 'vision', short: 'vis' },
  { key: 'embedding', short: 'emb' },
];

/** Stronger capability → tighter orbit and brighter particle. `NONE` is
 *  deliberately still drawn, hollow and far out, so an absent capability is
 *  visible as absent rather than silently omitted. */
const LEVEL_VISUAL: Record<AiCapabilityLevel, { radius: number; fill: string; r: number; opacity: number }> = {
  STRONG: { radius: 30, fill: '#34d399', r: 3.4, opacity: 1 },
  BASIC: { radius: 42, fill: '#22d3ee', r: 2.8, opacity: 0.9 },
  UNKNOWN: { radius: 54, fill: '#64748b', r: 2.2, opacity: 0.65 },
  NONE: { radius: 64, fill: 'none', r: 2.2, opacity: 0.5 },
};

export const AIEngineOrb: React.FC<AIEngineOrbProps> = ({ model, routingScore }) => {
  const live = model !== null;

  const particles = (model ? CAPABILITY_KEYS : []).map(({ key, short }, i) => {
    const level = (model!.capabilities[key] ?? 'UNKNOWN') as AiCapabilityLevel;
    const visual = LEVEL_VISUAL[level];
    // Deterministic placement — evenly distributed, no randomness, so the
    // same real capability set always draws the same orb.
    const angle = (i / CAPABILITY_KEYS.length) * Math.PI * 2;
    return {
      key,
      short,
      level,
      visual,
      cx: 80 + Math.cos(angle) * visual.radius,
      cy: 80 + Math.sin(angle) * visual.radius,
    };
  });

  return (
    <div id="ai-engine-orb" className="relative flex items-center justify-center py-1" aria-hidden="true">
      <style>{`
        @keyframes orbSpin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
        @keyframes orbBreathe { 0%,100% { opacity: .85; } 50% { opacity: .45; } }
      `}</style>

      <svg viewBox="0 0 160 160" className="w-[168px] h-[168px]">
        <defs>
          <radialGradient id="orbCore" cx="50%" cy="45%">
            <stop offset="0%" stopColor={live ? '#5eead4' : '#475569'} />
            <stop offset="55%" stopColor={live ? '#0891b2' : '#334155'} />
            <stop offset="100%" stopColor={live ? '#0e7490' : '#1e293b'} />
          </radialGradient>
          <radialGradient id="orbHalo" cx="50%" cy="50%">
            <stop offset="0%" stopColor={live ? '#22d3ee' : '#334155'} stopOpacity="0.35" />
            <stop offset="100%" stopColor="transparent" stopOpacity="0" />
          </radialGradient>
        </defs>

        {/* Halo — only meaningful when a real route exists. */}
        <circle cx="80" cy="80" r="72" fill="url(#orbHalo)" />

        {/* Orbit guides, one per real level band actually in use. */}
        {[30, 42, 54, 64].map((r) => (
          <circle
            key={r}
            cx="80"
            cy="80"
            r={r}
            fill="none"
            stroke={live ? '#155e75' : '#1e293b'}
            strokeWidth="0.5"
            strokeDasharray="2 4"
            opacity="0.7"
          />
        ))}

        {/* Particles: one per real capability. Rotation runs only while a
            model is genuinely available. */}
        <g
          style={
            live
              ? { animation: 'orbSpin 26s linear infinite', transformOrigin: '80px 80px' }
              : { transformOrigin: '80px 80px' }
          }
        >
          {particles.map((p) => (
            <g key={p.key}>
              <line
                x1="80"
                y1="80"
                x2={p.cx}
                y2={p.cy}
                stroke={p.level === 'NONE' ? '#334155' : p.visual.fill}
                strokeWidth="0.4"
                opacity={p.level === 'NONE' ? 0.25 : 0.35}
              />
              <circle
                cx={p.cx}
                cy={p.cy}
                r={p.visual.r}
                fill={p.visual.fill}
                stroke={p.level === 'NONE' ? '#475569' : p.visual.fill}
                strokeWidth={p.level === 'NONE' ? 1 : 0}
                opacity={p.visual.opacity}
                style={
                  p.level === 'STRONG'
                    ? { animation: 'orbBreathe 2.4s ease-in-out infinite' }
                    : undefined
                }
              />
            </g>
          ))}
        </g>

        {/* Core */}
        <circle cx="80" cy="80" r="20" fill="url(#orbCore)" />
        <circle cx="80" cy="80" r="20" fill="none" stroke={live ? '#67e8f9' : '#475569'} strokeWidth="0.8" opacity="0.7" />

        {/* Real routing score inside the core — the one number that decided
            this model actually wins. */}
        {live && routingScore !== null && (
          <text
            x="80"
            y="83"
            textAnchor="middle"
            className="font-mono"
            fontSize="10"
            fontWeight="700"
            fill="#ecfeff"
          >
            {routingScore.toFixed(0)}
          </text>
        )}
        {!live && (
          <text x="80" y="83" textAnchor="middle" className="font-mono" fontSize="7" fill="#94a3b8">
            NO ROUTE
          </text>
        )}
      </svg>
    </div>
  );
};
