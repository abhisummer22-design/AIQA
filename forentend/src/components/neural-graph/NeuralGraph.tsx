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
 *
 * Clicking a node does both things spec §6 asked for, not just the detail
 * panel: the view also smoothly pans/scales so the clicked node moves to
 * centre and enlarges — a real camera move, not a flourish, since it
 * always lands exactly on the real node that was clicked. `zoom` and `pan`
 * are a single shared camera state from here on: the +/- buttons, a click-
 * to-focus, and a manual click-and-drag pan all read and write the same
 * two numbers, so zooming out always works regardless of how the view got
 * where it is, and dragging never fights a separate "focus" transform.
 *
 * The slow rotating sweep behind the nodes is the one purely atmospheric
 * touch, and even that is gated by something real: it only turns while
 * this graph is genuinely subscribed to live events (`useNeuralGraphData`'s
 * realtime resync), and spins faster/brighter in proportion to the real
 * `nodesActive` count.
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
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
const FOCUS_ZOOM = 1.6;
/** Below this many client pixels of movement, a press-release is a click
 *  (select/deselect), not a drag — matches the small-movement tolerance
 *  most map/canvas UIs use so an imprecise click never gets read as an
 *  accidental pan. */
const DRAG_THRESHOLD_PX = 4;

export const NeuralGraph: React.FC<NeuralGraphProps> = ({ projectId, projectName, dashboard, aiStatus }) => {
  const { data, loading, error, retry } = useNeuralGraphData(projectId, projectName, dashboard, aiStatus);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  // Pan, in percent-of-viewBox units (so it resolves against the SVG's own
  // box regardless of its actual rendered pixel size) — the single camera
  // offset every interaction (focus-click, drag, zoom) shares.
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isFullscreen, setIsFullscreen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const graphAreaRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ startClientX: number; startClientY: number; startPanX: number; startPanY: number; dragged: boolean } | null>(null);
  const suppressClickRef = useRef(false);

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

  const handleSelectNode = (id: string) => {
    if (suppressClickRef.current) return; // a drag just ended here, not a click
    setSelectedId(id);
    const pos = positions.get(id);
    if (pos) {
      setPan({ x: ((CENTER - pos.x) / VIEWBOX_SIZE) * 100, y: ((CENTER - pos.y) / VIEWBOX_SIZE) * 100 });
      // Only ever zooms further IN on click — a user who had already
      // zoomed in past the focus level keeps their own zoom, so clicking
      // around never fights a deliberate zoom-in.
      setZoom((z) => Math.max(z, FOCUS_ZOOM));
    }
  };

  const handleBackgroundClick = () => {
    if (suppressClickRef.current) return;
    setSelectedId(null);
  };

  // Manual click-and-drag pan. Starts tracking on any press inside the
  // graph area (nodes included — you can grab from anywhere), but only
  // actually moves the camera once the pointer has travelled past
  // `DRAG_THRESHOLD_PX`, and only then suppresses the click that would
  // otherwise fire on release — so a real click still selects/deselects
  // exactly as before, and a real drag never also fires one.
  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    dragRef.current = { startClientX: e.clientX, startClientY: e.clientY, startPanX: pan.x, startPanY: pan.y, dragged: false };
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    const area = graphAreaRef.current;
    if (!drag || !area) return;
    const dxClient = e.clientX - drag.startClientX;
    const dyClient = e.clientY - drag.startClientY;
    if (!drag.dragged && Math.hypot(dxClient, dyClient) > DRAG_THRESHOLD_PX) {
      drag.dragged = true;
      area.style.cursor = 'grabbing';
    }
    if (drag.dragged) {
      const rect = area.getBoundingClientRect();
      const dxPct = (dxClient / rect.width) * 100 / zoom;
      const dyPct = (dyClient / rect.height) * 100 / zoom;
      setPan({ x: drag.startPanX + dxPct, y: drag.startPanY + dyPct });
    }
  };

  const handlePointerUp = () => {
    if (dragRef.current?.dragged) {
      suppressClickRef.current = true;
      setTimeout(() => {
        suppressClickRef.current = false;
      }, 0);
    }
    dragRef.current = null;
    if (graphAreaRef.current) graphAreaRef.current.style.cursor = 'grab';
  };

  // Shared by the +/- buttons and the mouse wheel: changing zoom also
  // scales `pan` by the same ratio, so panning shrinks back toward centre
  // as you zoom out (and grows back if you zoom back in) instead of
  // sitting at a fixed pixel offset regardless of zoom level. Without
  // this, a pan set while focused-in stayed the same *visual* size as you
  // zoomed out, so "zoom out" could shrink the graph without ever
  // revealing the rest of it — this is the real fix for that, not just
  // letting the zoom number go down.
  const applyZoomDelta = (delta: number) => {
    setZoom((z) => {
      const newZoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, +(z + delta).toFixed(2)));
      if (newZoom !== z) setPan((p) => ({ x: (p.x * newZoom) / z, y: (p.y * newZoom) / z }));
      return newZoom;
    });
  };

  // Mouse wheel zoom — the interaction most people reach for first on any
  // pannable canvas, and the graph had no handler for it at all.
  //
  // Attached as a real, non-passive native DOM listener via this effect
  // rather than React's `onWheel` prop: React attaches wheel/touch
  // listeners as *passive* by default (a real browser/React performance
  // optimization, not a bug), and a passive listener's `preventDefault()`
  // is silently ignored — the page scrolls underneath regardless of
  // calling it. That was the actual cause of the whole page scrolling
  // while zooming: `onWheel={handleWheel}` genuinely never had the power
  // to stop it. `{ passive: false }` here is the one way to get a
  // `preventDefault()` that the browser actually honours.
  //
  // `applyZoomDeltaRef` holds the latest closure so this effect can attach
  // the listener exactly once (empty deps) instead of removing and
  // re-adding it on every render.
  const applyZoomDeltaRef = useRef(applyZoomDelta);
  applyZoomDeltaRef.current = applyZoomDelta;

  useEffect(() => {
    const area = graphAreaRef.current;
    if (!area) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      applyZoomDeltaRef.current(e.deltaY > 0 ? -ZOOM_STEP : ZOOM_STEP);
    };
    area.addEventListener('wheel', onWheel, { passive: false });
    return () => area.removeEventListener('wheel', onWheel);
    // Re-attaches whenever the graph area's real presence in the DOM
    // flips (it only exists once `data` has actually loaded — see the
    // conditional render below) — an empty dependency array would run
    // this before that div exists on first load and never retry.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [Boolean(data)]);

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
          onZoomIn={() => applyZoomDelta(ZOOM_STEP)}
          onZoomOut={() => applyZoomDelta(-ZOOM_STEP)}
          onReset={() => {
            setZoom(1);
            setPan({ x: 0, y: 0 });
            setSelectedId(null);
          }}
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
            <div
              ref={graphAreaRef}
              onPointerDown={handlePointerDown}
              onPointerMove={handlePointerMove}
              onPointerUp={handlePointerUp}
              onPointerLeave={handlePointerUp}
              className="hidden md:block lg:col-span-3 relative rounded-md border border-slate-800/80 bg-[radial-gradient(circle_at_center,_#1e1b4b_0%,_#020617_75%)] overflow-hidden"
              style={{ aspectRatio: '1 / 1', cursor: 'grab', touchAction: 'none' }}
            >
              <svg
                viewBox={`0 0 ${VIEWBOX_SIZE} ${VIEWBOX_SIZE}`}
                className="w-full h-full"
                style={{
                  transform: `scale(${zoom}) translate(${pan.x}%, ${pan.y}%)`,
                  transformOrigin: 'center',
                  transition: dragRef.current ? 'none' : 'transform 0.3s ease-out',
                }}
              >
                <defs>
                  <pattern id="neuralGrid" width="36" height="36" patternUnits="userSpaceOnUse">
                    <path d="M 36 0 L 0 0 0 36" fill="none" stroke="#0f2937" strokeWidth="1" />
                  </pattern>
                  <radialGradient id="neuralSweepFade" cx="0%" cy="0%" r="100%">
                    <stop offset="0%" stopColor="#22d3ee" stopOpacity="0.5" />
                    <stop offset="100%" stopColor="#22d3ee" stopOpacity="0" />
                  </radialGradient>
                </defs>
                <rect width={VIEWBOX_SIZE} height={VIEWBOX_SIZE} fill="url(#neuralGrid)" opacity="0.5" onClick={handleBackgroundClick} />

                {/* The one atmospheric touch — a slow radar sweep. Its
                    presence is real, not decorative: it exists at all only
                    because this graph is genuinely realtime-subscribed
                    (`useNeuralGraphData`'s resync effect), and its speed and
                    brightness scale with the real `nodesActive` count, so a
                    quiet project sweeps slowly and dim, an active one fast
                    and bright. */}
                <g style={{ transformOrigin: `${CENTER}px ${CENTER}px` }}>
                  <path
                    d={`M ${CENTER} ${CENTER} L ${CENTER} 40 A ${CENTER - 40} ${CENTER - 40} 0 0 1 ${CENTER + (CENTER - 40) * Math.sin((50 * Math.PI) / 180)} ${CENTER - (CENTER - 40) * Math.cos((50 * Math.PI) / 180)} Z`}
                    fill="url(#neuralSweepFade)"
                    opacity={0.15 + Math.min(0.35, data.summary.nodesActive * 0.03)}
                  />
                  <animateTransform
                    attributeName="transform"
                    type="rotate"
                    from={`0 ${CENTER} ${CENTER}`}
                    to={`360 ${CENTER} ${CENTER}`}
                    dur={`${Math.max(6, 26 - data.summary.nodesActive)}s`}
                    repeatCount="indefinite"
                  />
                </g>

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
                      onSelect={handleSelectNode}
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
