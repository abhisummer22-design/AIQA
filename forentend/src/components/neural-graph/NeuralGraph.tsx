/**
 * AIQA Neural Intelligence Graph — the Dashboard's visual intelligence
 * layer (spec §16). Every number on this graph is real: fetched and
 * aggregated by `useNeuralGraphData.ts` from this project's actual test
 * cases, executions, evidence, and audit log — never a placeholder
 * dataset. Node layout is a fixed, deterministic radial arrangement (like
 * `AIEngineOrb`/`OrchestratorRadar` elsewhere in this app) rather than a
 * physics simulation — the same real state always draws the same graph,
 * which also keeps this inspectable in tests like any other DOM.
 *
 * Desktop/tablet render the real SVG graph; mobile (spec §13) renders the
 * same real node data as a scannable list instead of forcing a 21-node
 * graph into a few hundred pixels.
 */
import React, { useMemo, useRef, useState } from 'react';
import { BrainCircuit, Loader2, AlertTriangle } from 'lucide-react';
import { NeuralNode, type NeuralNodePosition } from './NeuralNode';
import { NeuralEdge } from './NeuralEdge';
import { NeuralTooltip } from './NeuralTooltip';
import { NeuralGraphControls } from './NeuralGraphControls';
import { NeuralGraphStats } from './NeuralGraphStats';
import { NeuralNodeDetailPanel } from './NeuralNodeDetailPanel';
import { useNeuralGraphData } from '../../hooks/useNeuralGraphData';
import {
  CATEGORY_NODES,
  CENTER,
  MAJOR_RING_RADIUS,
  MAJOR_STAGES,
  SATELLITE_RING_RADIUS,
  STATUS_COLOR,
  STATUS_GLYPH,
  VIEWBOX_SIZE,
} from './neuralGraph.config';
import type { AiStatusResponse, DashboardResponse } from '../../types/contract';

export interface NeuralGraphProps {
  projectId: string | null;
  projectName: string | null;
  dashboard: DashboardResponse | null;
  aiStatus: AiStatusResponse | null;
}

const ZOOM_MIN = 0.6;
const ZOOM_MAX = 2.4;
const ZOOM_STEP = 0.2;

export const NeuralGraph: React.FC<NeuralGraphProps> = ({ projectId, projectName, dashboard, aiStatus }) => {
  const { data, loading, error, retry } = useNeuralGraphData(projectId, projectName, dashboard, aiStatus);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Fixed, deterministic layout: the core at the exact center, six major
  // stages on an inner ring, fourteen real category nodes on an outer
  // ring — the same real node set always produces the same positions.
  const positions = useMemo(() => {
    const map = new Map<string, NeuralNodePosition>();
    map.set('core', { x: CENTER, y: CENTER });
    MAJOR_STAGES.forEach((stage, i) => {
      const angle = (2 * Math.PI * i) / MAJOR_STAGES.length - Math.PI / 2;
      map.set(stage.id, { x: CENTER + MAJOR_RING_RADIUS * Math.cos(angle), y: CENTER + MAJOR_RING_RADIUS * Math.sin(angle) });
    });
    CATEGORY_NODES.forEach((cat, i) => {
      const angle = (2 * Math.PI * i) / CATEGORY_NODES.length - Math.PI / 2;
      map.set(`cat-${cat.id}`, { x: CENTER + SATELLITE_RING_RADIUS * Math.cos(angle), y: CENTER + SATELLITE_RING_RADIUS * Math.sin(angle) });
    });
    return map;
  }, []);

  const highlightedIds = useMemo(() => {
    if (!hoveredId || !data) return new Set<string>();
    const set = new Set<string>([hoveredId]);
    for (const edge of data.edges) {
      if (edge.from === hoveredId) set.add(edge.to);
      if (edge.to === hoveredId) set.add(edge.from);
    }
    return set;
  }, [hoveredId, data]);

  const selectedNode = data?.nodes.find((n) => n.id === selectedId) ?? null;
  const hoveredNode = data?.nodes.find((n) => n.id === hoveredId) ?? null;
  const hoveredPos = hoveredId ? positions.get(hoveredId) : null;

  const toggleFullscreen = () => {
    if (!containerRef.current) return;
    if (!document.fullscreenElement) {
      void containerRef.current.requestFullscreen().then(() => setIsFullscreen(true));
    } else {
      void document.exitFullscreen().then(() => setIsFullscreen(false));
    }
  };

  if (!projectId) {
    return (
      <section id="neural-intelligence-graph" className="p-6 rounded-lg bg-[#0B1120] border border-slate-800 text-center">
        <BrainCircuit className="w-6 h-6 text-slate-600 mx-auto mb-2" />
        <p className="text-xs font-mono text-slate-500">Select a project to load its real Neural Intelligence Graph.</p>
      </section>
    );
  }

  return (
    <section id="neural-intelligence-graph" ref={containerRef} className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
      <div className="flex items-center justify-between gap-2 flex-wrap pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2 min-w-0">
          <BrainCircuit className="w-4 h-4 text-cyan-400 shrink-0" />
          <div className="min-w-0">
            <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">AIQA Neural Intelligence Graph</h2>
            <p className="text-[10px] font-mono text-slate-500 truncate">Real requirement, test-case, execution, evidence, risk, and audit intelligence for {projectName ?? projectId}</p>
          </div>
        </div>
        <NeuralGraphControls
          onZoomIn={() => setZoom((z) => Math.min(ZOOM_MAX, +(z + ZOOM_STEP).toFixed(2)))}
          onZoomOut={() => setZoom((z) => Math.max(ZOOM_MIN, +(z - ZOOM_STEP).toFixed(2)))}
          onReset={() => setZoom(1)}
          onToggleFullscreen={toggleFullscreen}
          isFullscreen={isFullscreen}
        />
      </div>

      {error ? (
        <div className="p-6 flex flex-col items-center gap-2 text-center">
          <AlertTriangle className="w-5 h-5 text-rose-400" />
          <p className="text-xs font-mono text-slate-400">{error}</p>
          <button type="button" id="btn-retry-neural-graph" onClick={retry} className="px-3 py-1.5 rounded-md bg-[#020617] border border-slate-700 hover:border-indigo-600 text-slate-300 hover:text-indigo-300 font-mono text-xs cursor-pointer">
            Retry
          </button>
        </div>
      ) : loading || !data ? (
        <div className="p-10 flex items-center justify-center gap-2 text-xs font-mono text-slate-400">
          <Loader2 className="w-4 h-4 animate-spin text-indigo-400" /> Loading real Neural Intelligence Graph…
        </div>
      ) : (
        <>
          <NeuralGraphStats summary={data.summary} />

          <div className="grid grid-cols-1 lg:grid-cols-4 gap-3">
            {/* Desktop/tablet — the real graph. */}
            <div className="hidden md:block lg:col-span-3 relative rounded-md border border-slate-800/80 bg-[radial-gradient(circle_at_center,_#0f1b2e_0%,_#020617_75%)] overflow-hidden" style={{ aspectRatio: '1 / 1' }}>
              <svg
                viewBox={`0 0 ${VIEWBOX_SIZE} ${VIEWBOX_SIZE}`}
                className="w-full h-full"
                style={{ transform: `scale(${zoom})`, transformOrigin: 'center', transition: 'transform 0.15s ease-out' }}
              >
                <defs>
                  <pattern id="neuralGrid" width="36" height="36" patternUnits="userSpaceOnUse">
                    <path d="M 36 0 L 0 0 0 36" fill="none" stroke="#0f2937" strokeWidth="1" />
                  </pattern>
                </defs>
                <rect width={VIEWBOX_SIZE} height={VIEWBOX_SIZE} fill="url(#neuralGrid)" opacity="0.5" />

                {data.edges.map((edge) => {
                  const from = positions.get(edge.from);
                  const to = positions.get(edge.to);
                  if (!from || !to) return null;
                  const dimmed = hoveredId !== null && !highlightedIds.has(edge.from) && !highlightedIds.has(edge.to);
                  const highlighted = hoveredId !== null && highlightedIds.has(edge.from) && highlightedIds.has(edge.to);
                  return <NeuralEdge key={edge.id} id={edge.id} from={from} to={to} active={edge.active} dimmed={dimmed} highlighted={highlighted} />;
                })}

                {data.nodes.map((node) => {
                  const pos = positions.get(node.id);
                  if (!pos) return null;
                  const dimmed = hoveredId !== null && !highlightedIds.has(node.id);
                  return (
                    <NeuralNode
                      key={node.id}
                      node={node}
                      position={pos}
                      dimmed={dimmed}
                      highlighted={hoveredId === node.id || selectedId === node.id}
                      onHover={setHoveredId}
                      onSelect={setSelectedId}
                    />
                  );
                })}
              </svg>

              {hoveredNode && hoveredPos && (
                <NeuralTooltip node={hoveredNode} x={`${(hoveredPos.x / VIEWBOX_SIZE) * 100}%`} y={`${(hoveredPos.y / VIEWBOX_SIZE) * 100}%`} />
              )}
            </div>

            {/* Mobile — same real node data, as a scannable list (spec §13). */}
            <div className="md:hidden lg:col-span-3 space-y-1.5 max-h-[420px] overflow-y-auto">
              {data.nodes.map((node) => (
                <button
                  type="button"
                  key={node.id}
                  onClick={() => setSelectedId(node.id)}
                  className="w-full flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-left cursor-pointer"
                >
                  <span className="flex items-center gap-1.5 min-w-0">
                    <span style={{ color: STATUS_COLOR[node.status] }}>{STATUS_GLYPH[node.status]}</span>
                    <span className="text-xs font-mono text-slate-200 truncate">{node.label}</span>
                  </span>
                  <span className="text-[10px] font-mono text-slate-400 shrink-0">
                    {node.metricLabel}: {node.metricValue}
                  </span>
                </button>
              ))}
            </div>

            <div className="lg:col-span-1">
              {selectedNode ? (
                <NeuralNodeDetailPanel node={selectedNode} onClose={() => setSelectedId(null)} />
              ) : (
                <div className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 border-dashed text-center text-[11px] font-mono text-slate-500 h-full flex items-center justify-center">
                  Click a node to see its real detail.
                </div>
              )}
            </div>
          </div>
        </>
      )}
    </section>
  );
};
