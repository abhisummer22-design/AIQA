/**
 * Hover tooltip for one Neural Graph node — real label/status/metric only,
 * positioned by the caller (`NeuralGraph.tsx` already knows the pixel
 * position of the hovered node within its own container).
 */
import React from 'react';
import { STATUS_COLOR, STATUS_GLYPH } from './neuralGraph.config';
import type { NeuralNodeData } from './neuralGraph.types';

interface NeuralTooltipProps {
  node: NeuralNodeData;
  /** Percent-of-container position (e.g. `"42%"`) — the graph's SVG
   *  scales with its container, so a percentage tracks a node's real
   *  position regardless of rendered pixel size, with no
   *  `getBoundingClientRect` bookkeeping needed. */
  x: string;
  y: string;
}

export const NeuralTooltip: React.FC<NeuralTooltipProps> = ({ node, x, y }) => (
  <div
    id="neural-tooltip"
    className="absolute z-10 pointer-events-none px-2.5 py-1.5 rounded-md bg-[#020617]/95 border border-slate-700 shadow-xl text-[10px] font-mono whitespace-nowrap"
    style={{ left: x, top: y, transform: 'translate(-50%, -115%)' }}
  >
    <div className="font-bold text-slate-100">{node.label}</div>
    <div className="flex items-center gap-1.5 mt-0.5" style={{ color: STATUS_COLOR[node.status] }}>
      <span>{STATUS_GLYPH[node.status]}</span>
      <span className="uppercase tracking-wide">{node.status}</span>
    </div>
    <div className="text-slate-400 mt-0.5">
      {node.metricLabel}: <span className="text-slate-200">{node.metricValue}</span>
    </div>
  </div>
);
