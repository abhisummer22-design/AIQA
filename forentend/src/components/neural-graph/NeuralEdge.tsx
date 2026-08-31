/**
 * One Neural Graph connection. `active` is a real claim, not decoration:
 * it is only ever `true` when `useNeuralGraphData.ts` found the edge's
 * destination genuinely has current work (a processing/active status) or
 * a real, non-zero relationship (a category with at least one real test
 * case) — matching the same "motion means something real" rule
 * `AIEngineOrb`/`NeuralPipeline` use.
 *
 * An active edge renders as a thick, glowing "flowing pipe" (a soft blur
 * underlay, a bright core stroke, and a short highlight segment animated
 * along its length) — reserved for genuinely active connections only, so
 * the visual drama of "something is flowing" never appears on a
 * connection that is not, in fact, real and current. An idle edge stays
 * the same plain thin line as before.
 *
 * Each edge's path is a deterministic quadratic curve, not a straight
 * radial line — the bend direction/magnitude is derived from the edge's
 * own real `id` (a stable hash, not `Math.random()`), so the same real
 * edge always bends the same way and the graph never looks laser-straight
 * and mechanical, while still being fully reproducible.
 */
import React from 'react';

interface NeuralEdgeProps {
  id: string;
  from: { x: number; y: number };
  to: { x: number; y: number };
  active: boolean;
  dimmed: boolean;
  highlighted: boolean;
}

/** Small deterministic string hash — same real edge id always bends the
 *  same way, across every render and every reload. */
function curveOffsetFor(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  const magnitude = 14 + (h % 24); // 14..37px bend
  return h % 2 === 0 ? magnitude : -magnitude;
}

const NeuralEdgeImpl: React.FC<NeuralEdgeProps> = ({ id, from, to, active, dimmed, highlighted }) => {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  const nx = -dy / len;
  const ny = dx / len;
  const offset = curveOffsetFor(id);
  const mx = (from.x + to.x) / 2 + nx * offset;
  const my = (from.y + to.y) / 2 + ny * offset;
  const d = `M ${from.x} ${from.y} Q ${mx} ${my} ${to.x} ${to.y}`;

  if (!active) {
    return (
      <path
        d={d}
        fill="none"
        stroke={highlighted ? '#67e8f9' : '#1e3a4f'}
        strokeWidth={highlighted ? 2 : 1}
        strokeLinecap="round"
        opacity={dimmed ? 0.08 : highlighted ? 0.9 : 0.3}
      />
    );
  }

  const color = highlighted ? '#a5f3fc' : '#38bdf8';
  const pathLength = len + Math.abs(offset); // rough real length for a believable flow speed

  return (
    <g opacity={dimmed ? 0.12 : 1}>
      {/* Soft glow underlay — the "pipe" reads as lit from within. */}
      <path d={d} fill="none" stroke={color} strokeWidth={highlighted ? 9 : 7} strokeLinecap="round" opacity={0.18} />
      {/* The pipe itself. */}
      <path d={d} fill="none" stroke={color} strokeWidth={highlighted ? 3.4 : 2.6} strokeLinecap="round" opacity={0.55} />
      {/* A bright segment flowing along the pipe from source to
          destination — the one part of this that's pure animation, gated
          entirely behind the real `active` flag above. */}
      <path
        d={d}
        fill="none"
        stroke="#f0fdff"
        strokeWidth={highlighted ? 3.4 : 2.6}
        strokeLinecap="round"
        opacity={0.9}
        strokeDasharray={`${Math.max(10, pathLength * 0.12)} ${pathLength}`}
      >
        <animate attributeName="stroke-dashoffset" from={pathLength} to="0" dur="1.6s" repeatCount="indefinite" />
      </path>
    </g>
  );
};

export const NeuralEdge = React.memo(NeuralEdgeImpl);
