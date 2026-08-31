/**
 * One Neural Graph node — a real pipeline stage, a real test-case
 * category, or the center hub. Purely presentational: every prop already
 * carries a real, pre-computed value from `useNeuralGraphData.ts`.
 *
 * Memoized (spec §12) — with up to 21 nodes re-rendering on every hover-
 * driven parent state change would be real, measurable jank; a node only
 * needs to re-render when its own data or interaction state changes.
 */
import React from 'react';
import { CATEGORY_ICON, CORE_AND_STAGE_ICON, STATUS_COLOR, STATUS_GLYPH, TIER_RADIUS } from './neuralGraph.config';
import type { NeuralNodeData } from './neuralGraph.types';

export interface NeuralNodePosition {
  x: number;
  y: number;
}

interface NeuralNodeProps {
  node: NeuralNodeData;
  position: NeuralNodePosition;
  dimmed: boolean;
  highlighted: boolean;
  onHover: (id: string | null) => void;
  onSelect: (id: string) => void;
}

const NeuralNodeImpl: React.FC<NeuralNodeProps> = ({ node, position, dimmed, highlighted, onHover, onSelect }) => {
  const r = TIER_RADIUS[node.tier];
  const color = STATUS_COLOR[node.status];
  const isProcessing = node.status === 'processing' || node.status === 'active';
  const fontSize = node.tier === 'core' ? 13 : node.tier === 'major' ? 11 : 9;
  // Real QA-category glyph — which real pipeline stage/test category this
  // node stands for, at a glance. Looked up by the node's own real id, not
  // invented per node — `CATEGORY_ICON` for the 14 real satellite
  // categories (stripping the `cat-` id prefix), `CORE_AND_STAGE_ICON` for
  // the hub and the six real pipeline stages.
  const Icon = node.tier === 'satellite' ? CATEGORY_ICON[node.id.replace(/^cat-/, '')] : CORE_AND_STAGE_ICON[node.id];
  const iconSize = node.tier === 'core' ? 22 : node.tier === 'major' ? 16 : 10;
  const iconY = node.tier === 'core' ? -15 : node.tier === 'major' ? -10 : -6;
  const glyphY = node.tier === 'core' ? 19 : node.tier === 'major' ? 13 : 7;
  const glyphFontSize = node.tier === 'core' ? 15 : node.tier === 'major' ? 11 : 8;

  return (
    <g
      id={`neural-node-${node.id}`}
      transform={`translate(${position.x}, ${position.y}) scale(${highlighted ? 1.15 : 1})`}
      style={{ transition: 'transform 0.2s ease-out, opacity 0.2s ease-out' }}
      opacity={dimmed ? 0.28 : 1}
      className="cursor-pointer"
      onMouseEnter={() => onHover(node.id)}
      onMouseLeave={() => onHover(null)}
      onClick={() => onSelect(node.id)}
      role="button"
      aria-label={`${node.label} — ${node.status}, ${node.metricLabel} ${node.metricValue}`}
    >
      {highlighted && <circle r={r + 7} fill="none" stroke={color} strokeWidth="1.5" opacity="0.6" />}
      <circle r={r} fill="#0B1120" fillOpacity="0.9" stroke={color} strokeWidth={node.tier === 'core' ? 2.5 : 1.75} />
      <circle r={r} fill={color} fillOpacity={node.tier === 'satellite' ? '0.12' : '0.16'} />
      {isProcessing && <circle r={r} fill="none" stroke={color} strokeWidth="1.5" opacity="0.7" className="animate-ping" style={{ transformOrigin: 'center' }} />}

      {Icon && <Icon x={-iconSize / 2} y={iconY - iconSize / 2} width={iconSize} height={iconSize} color={color} strokeWidth={2} />}
      <text y={glyphY} textAnchor="middle" fill={color} style={{ fontSize: glyphFontSize, fontFamily: 'monospace', fontWeight: 700 }}>
        {STATUS_GLYPH[node.status]}
      </text>
      {node.tier !== 'satellite' && (
        <text y={r + 16} textAnchor="middle" fill="#cbd5e1" style={{ fontSize, fontFamily: 'monospace', fontWeight: 700 }}>
          {node.shortLabel}
        </text>
      )}
      {node.tier !== 'satellite' && (
        <text y={r + 29} textAnchor="middle" fill={color} style={{ font: '9px monospace' }}>
          {node.metricValue}
        </text>
      )}
    </g>
  );
};

export const NeuralNode = React.memo(NeuralNodeImpl);
