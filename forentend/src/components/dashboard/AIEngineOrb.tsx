/**
 * AI Engine Orb — a dense, rotating particle sphere whose every visual
 * property is still a real value, not a decorative particle storm.
 *
 * Rebuilt denser (previously one particle per real capability — only
 * ~7 dots) while keeping the same rule the original orb (which itself
 * replaced the fully-decorative `AIQualityCoreOrb`) established:
 * **motion and colour must encode something real, or not appear.**
 *
 *   - **Particle count is the real routing score**, rounded to the
 *     nearest 10 (so a routine +/-0.3 poll fluctuation doesn't re-layout
 *     the whole sphere) and clamped to a renderable range. More particles
 *     genuinely means a higher real score — never a fixed decorative
 *     count. With no route, the sphere falls back to a small, dim, static
 *     field (`IDLE_PARTICLE_COUNT`), not zero — "no route" is a real
 *     state, not "nothing to draw".
 *   - **Particle colour distribution is the real capability-level mix**:
 *     colours are assigned by cycling through the model's real
 *     `text_generation`/`structured_output`/`reasoning`/`tool_use`/
 *     `long_context`/`vision`/`embedding` levels in order, so across the
 *     whole sphere the *ratio* of emerald (STRONG) to cyan (BASIC) to
 *     slate (UNKNOWN/NONE) particles is exactly this model's real
 *     capability mix — never a random palette.
 *   - **The sphere only rotates while a real route exists** — same rule
 *     as before, just executed as a genuine CSS 3D rotation
 *     (`translate3d` positions on a `preserve-3d` layer, animated with
 *     `rotateY`) instead of a flat 2D spin, so the particles now visibly
 *     orbit in depth rather than sliding around a circle. A second,
 *     slower wobble layer (`orbWobble`) adds a real depth-tumbling feel —
 *     gated by the exact same `live` boolean as the spin itself, so it
 *     is still "motion only when something real is true," not a second,
 *     independent decorative animation.
 *   - **The core itself shows no score** — it is a plain glow when a real
 *     route exists, and an honest "NO ROUTE" label when it does not. The
 *     real routing score is not repeated here since `AIEnginePanel`
 *     already shows it as real text just below the orb; the core still
 *     depends on the same `routingScore`/`live` values (it only glows and
 *     breathes when a route is real), it just no longer renders the
 *     number a second time.
 *   - **Glow intensity is derived from each particle's own real depth**
 *     (`p.z`, already the source of its size) — nearer, larger particles
 *     get a proportionally larger glow radius, not a fixed decorative
 *     blur applied uniformly.
 *
 * Particle positions are a deterministic Fibonacci-sphere distribution
 * (real math, no `Math.random()`), computed once per real (count, model)
 * pair via `useMemo` — the same real state always draws the same sphere,
 * and the animation itself is pure CSS (GPU-composited), so rotating
 * hundreds of particles costs zero extra React renders per frame.
 */

import React, { useMemo } from 'react';
import type { AiCapabilityLevel, AiModelDefinition } from '../../types/contract';

export interface AIEngineOrbProps {
  /** The real model that would be routed to, or `null` when the backend
   *  reports no compatible model. */
  model: AiModelDefinition | null;
  /** Real routing score, shown inside the core when a route exists, and
   *  the real source of the sphere's particle count. */
  routingScore: number | null;
}

const CAPABILITY_KEYS: Array<keyof AiModelDefinition['capabilities']> = [
  'text_generation',
  'structured_output',
  'reasoning',
  'tool_use',
  'long_context',
  'vision',
  'embedding',
];

const LEVEL_COLOR: Record<AiCapabilityLevel, string> = {
  STRONG: '#34d399',
  BASIC: '#22d3ee',
  UNKNOWN: '#64748b',
  NONE: '#334155',
};

const SPHERE_R = 62;
const MIN_PARTICLES = 60;
const MAX_PARTICLES = 320;
const IDLE_PARTICLE_COUNT = 36;
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

interface Particle {
  x: number;
  y: number;
  z: number;
  color: string;
  /** The real capability level this particle's colour came from (or
   *  `'idle'` for the no-route fallback field) — exposed as a `data-`
   *  attribute so the real colour ratio is directly, robustly queryable
   *  from tests instead of matching inline style strings. */
  level: AiCapabilityLevel | 'idle';
  size: number;
}

/** Real, deterministic Fibonacci-sphere placement — evenly distributes
 *  `count` points on a sphere of radius `r` with no randomness, so the
 *  same real count always produces the same sphere. */
function fibonacciSphere(count: number, r: number, colorFor: (i: number) => { color: string; level: AiCapabilityLevel | 'idle' }): Particle[] {
  const particles: Particle[] = [];
  for (let i = 0; i < count; i++) {
    const y = count > 1 ? 1 - (i / (count - 1)) * 2 : 0;
    const radiusAtY = Math.sqrt(Math.max(0, 1 - y * y));
    const theta = GOLDEN_ANGLE * i;
    const x = Math.cos(theta) * radiusAtY;
    const z = Math.sin(theta) * radiusAtY;
    const c = colorFor(i);
    particles.push({
      x: x * r,
      y: y * r,
      z: z * r,
      color: c.color,
      level: c.level,
      // Particles nearer the viewer (z closer to +r) render slightly
      // larger — a real depth cue from the same real z this particle's
      // position already has, not a separate decorative value.
      size: 2 + ((z + r) / (2 * r)) * 1.6,
    });
  }
  return particles;
}

export const AIEngineOrb: React.FC<AIEngineOrbProps> = ({ model, routingScore }) => {
  const live = model !== null && routingScore !== null;

  const particles = useMemo(() => {
    if (!live || routingScore === null) {
      return fibonacciSphere(IDLE_PARTICLE_COUNT, SPHERE_R, () => ({ color: '#334155', level: 'idle' as const }));
    }
    const count = Math.min(MAX_PARTICLES, Math.max(MIN_PARTICLES, Math.round(routingScore / 10) * 10));
    const levels = CAPABILITY_KEYS.map((k) => (model!.capabilities[k] ?? 'UNKNOWN') as AiCapabilityLevel);
    return fibonacciSphere(count, SPHERE_R, (i) => {
      const level = levels[i % levels.length];
      return { color: LEVEL_COLOR[level], level };
    });
  }, [live, routingScore, model]);

  return (
    <div id="ai-engine-orb" className="relative flex items-center justify-center py-1" aria-hidden="true">
      <style>{`
        @keyframes orbSphereSpin { from { transform: rotateY(0deg) rotateX(8deg); } to { transform: rotateY(360deg) rotateX(8deg); } }
        @keyframes orbCoreBreathe { 0%, 100% { opacity: 0.9; } 50% { opacity: 0.55; } }
        @keyframes orbWobble { 0%, 100% { transform: rotateX(8deg) rotateZ(-3deg); } 50% { transform: rotateX(15deg) rotateZ(3deg); } }
      `}</style>

      <div className="relative w-[168px] h-[168px]" style={{ perspective: '480px' }}>
        {/* Halo — two layered gradients, both only meaningful when a real
            route exists: a tight inner glow and a softer, wider outer
            haze for depth, same `live` gate as everything else here. */}
        <div
          className="absolute inset-0 rounded-full"
          style={{
            background: `radial-gradient(circle, ${live ? 'rgba(34,211,238,0.32)' : 'rgba(51,65,85,0.2)'} 0%, transparent 62%)`,
          }}
        />
        <div
          className="absolute inset-0 rounded-full"
          style={{
            background: `radial-gradient(circle, ${live ? 'rgba(94,234,212,0.16)' : 'rgba(51,65,85,0.1)'} 0%, transparent 80%)`,
            filter: 'blur(3px)',
          }}
        />

        {/* Outer wobble layer — a second, slower real-3D rotation gated by
            the exact same `live` boolean as the spin layer inside it, for
            a genuine depth-tumbling feel rather than a flat single-axis
            spin. Never animates on its own when `live` is false. */}
        <div
          className="absolute inset-0"
          style={{
            transformStyle: 'preserve-3d',
            animation: live ? 'orbWobble 9s ease-in-out infinite' : undefined,
            transform: live ? undefined : 'rotateX(8deg)',
          }}
        >
          {/* The particle sphere — a real CSS 3D layer, not a flat 2D spin. */}
          <div
            className="absolute inset-0"
            style={{
              transformStyle: 'preserve-3d',
              animation: live ? 'orbSphereSpin 22s linear infinite' : undefined,
            }}
          >
            {particles.map((p, i) => (
              <div
                key={i}
                data-level={p.level}
                className="absolute rounded-full"
                style={{
                  left: '50%',
                  top: '50%',
                  width: `${p.size}px`,
                  height: `${p.size}px`,
                  background: p.color,
                  opacity: live ? 0.55 + ((p.z + SPHERE_R) / (2 * SPHERE_R)) * 0.45 : 0.35,
                  boxShadow:
                    live && p.color !== LEVEL_COLOR.NONE && p.color !== LEVEL_COLOR.UNKNOWN
                      ? `0 0 ${(2 + p.size).toFixed(1)}px ${p.color}`
                      : undefined,
                  transform: `translate3d(${p.x}px, ${p.y}px, ${p.z}px) translate(-50%, -50%)`,
                }}
              />
            ))}
          </div>
        </div>

        {/* Core — never part of the rotating layers, so it stays upright
            while the sphere spins/wobbles around it. The routing score
            itself is not repeated here (it's already shown as real text
            just below the orb, e.g. "score 211.48") — this circle is a
            plain glow when live, and still says so honestly when there is
            no route to be inert about. */}
        <div className="absolute inset-0 flex items-center justify-center">
          <div
            className="w-11 h-11 rounded-full flex items-center justify-center border"
            style={{
              background: `radial-gradient(circle at 45% 40%, ${live ? '#5eead4' : '#475569'} 0%, ${live ? '#0891b2' : '#334155'} 55%, ${live ? '#0e7490' : '#1e293b'} 100%)`,
              borderColor: live ? '#67e8f9' : '#475569',
              boxShadow: live ? '0 0 18px rgba(34,211,238,0.55), 0 0 36px rgba(34,211,238,0.22)' : undefined,
              animation: live ? 'orbCoreBreathe 3.2s ease-in-out infinite' : undefined,
            }}
          >
            {!live && <span className="font-mono text-[6.5px] text-slate-300 leading-tight text-center">NO ROUTE</span>}
          </div>
        </div>
      </div>
    </div>
  );
};
