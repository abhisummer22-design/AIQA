/**
 * Neural Graph node detail panel — shown when a node is clicked (spec
 * §6). Renders exactly the real rows/recent items
 * `useNeuralGraphData.ts` already computed for this node; adds no new
 * data of its own.
 */
import React from 'react';
import { X } from 'lucide-react';
import { STATUS_COLOR, STATUS_GLYPH } from './neuralGraph.config';
import type { NeuralNodeData } from './neuralGraph.types';

interface NeuralNodeDetailPanelProps {
  node: NeuralNodeData;
  onClose: () => void;
}

export const NeuralNodeDetailPanel: React.FC<NeuralNodeDetailPanelProps> = ({ node, onClose }) => (
  <div id="neural-node-detail" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2.5">
    <div className="flex items-start justify-between gap-2">
      <div>
        <h4 className="text-xs font-bold text-slate-100 font-mono">{node.label}</h4>
        <div className="flex items-center gap-1.5 mt-0.5 text-[10px] font-mono" style={{ color: STATUS_COLOR[node.status] }}>
          <span>{STATUS_GLYPH[node.status]}</span>
          <span className="uppercase tracking-wide">{node.status}</span>
        </div>
      </div>
      <button type="button" id="btn-close-neural-node-detail" onClick={onClose} aria-label="Close node detail" className="p-1 text-slate-500 hover:text-slate-300 cursor-pointer">
        <X className="w-3.5 h-3.5" />
      </button>
    </div>

    <dl className="grid grid-cols-2 gap-x-2 gap-y-1.5 text-[11px] font-mono">
      {node.detail.rows.map((row) => (
        <React.Fragment key={row.label}>
          <dt className="text-slate-500 truncate">{row.label}</dt>
          <dd className="text-slate-200 text-right truncate">{row.value}</dd>
        </React.Fragment>
      ))}
    </dl>

    {node.detail.recent.length > 0 && (
      <div className="pt-2 border-t border-slate-800/80 space-y-1">
        <p className="text-[9px] font-mono uppercase tracking-wide text-slate-500">Recent</p>
        {node.detail.recent.map((item, i) => (
          <p key={i} className="text-[10px] font-mono text-slate-400 truncate">
            {item}
          </p>
        ))}
      </div>
    )}
  </div>
);
