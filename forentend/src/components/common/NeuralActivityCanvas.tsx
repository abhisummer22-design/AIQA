/**
 * React wrapper for `utils/neuralActivity.ts`'s `NeuralActivity` canvas
 * engine — a general-purpose visual primitive (`components/common/`,
 * matching `SectionHeader`/`EmptyState`), not tied to any one consumer.
 *
 * <NeuralActivityCanvas level={0.6} inputs={[...]} pulseKey={n} accent="#22d3ee" />
 *
 * - level    : 0..1 overall activity
 * - inputs   : optional array of 0..1 values for the first layer (realtime)
 * - pulseKey : change this value (e.g. increment a counter on a real event)
 *              to fire a pulse burst
 * - accent   : hex colour
 * - nodeColors/nodeLabels : real, optional per-node overrides — see
 *   `utils/neuralActivity.ts`'s own doc comments on `NodeColorGrid`/
 *   `NodeLabelGrid`. The caller must memoize these (e.g. `useMemo`) so a
 *   fresh-but-equal array/object each render doesn't look like new data.
 * - layerLabels : real, optional one-per-layer column labels
 * - activeNode  : real, optional `{l, i}` of the one node genuinely on the
 *   real "winning" path — only edges/signals touching it ever animate;
 *   every other real structural connection stays flat and static, so
 *   motion never implies data is flowing somewhere it truthfully is not.
 *
 * Every prop must be driven by a real value from the caller — this
 * component only draws what it is told; it invents nothing itself.
 */
import React, { useEffect, useRef } from 'react';
import { NeuralActivity, type NodeColorGrid, type NodeLabelGrid } from '../../utils/neuralActivity';

export interface NeuralActivityCanvasProps {
  level: number;
  inputs?: number[] | null;
  pulseKey?: number;
  accent?: string;
  /** Changing this (by reference — memoize it) rebuilds the node grid and
   *  restarts the animation; a real, infrequent event (e.g. the real
   *  discovered-provider count changing), not something to pass a fresh
   *  array literal for on every render. */
  layers?: number[];
  nodeColors?: NodeColorGrid | null;
  nodeLabels?: NodeLabelGrid | null;
  layerLabels?: (string | null | undefined)[] | null;
  activeNode?: { l: number; i: number } | null;
  background?: string | null;
  className?: string;
  style?: React.CSSProperties;
}

export const NeuralActivityCanvas: React.FC<NeuralActivityCanvasProps> = ({
  level,
  inputs = null,
  pulseKey,
  accent = '#22d3ee',
  layers,
  nodeColors = null,
  nodeLabels = null,
  layerLabels = null,
  activeNode = null,
  background = null,
  className,
  style,
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const nnRef = useRef<NeuralActivity | null>(null);

  useEffect(() => {
    if (!canvasRef.current) return;
    nnRef.current = new NeuralActivity(canvasRef.current, { accent, layers, background: background ?? null });
    return () => nnRef.current?.destroy();
    // `layers` genuinely rebuilds the node grid on change (see this
    // component's own doc comment on that prop); initial `accent`/
    // `background` are intentionally excluded — their own updates are
    // applied in place by the effects below, not by tearing down the
    // canvas/animation loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layers]);

  useEffect(() => {
    nnRef.current?.setLevel(level);
  }, [level]);
  useEffect(() => {
    nnRef.current?.setInputs(inputs);
  }, [inputs]);
  useEffect(() => {
    nnRef.current?.setAccent(accent);
  }, [accent]);
  useEffect(() => {
    nnRef.current?.setNodeColors(nodeColors);
  }, [nodeColors]);
  useEffect(() => {
    nnRef.current?.setNodeLabels(nodeLabels);
  }, [nodeLabels]);
  useEffect(() => {
    nnRef.current?.setLayerLabels(layerLabels);
  }, [layerLabels]);
  useEffect(() => {
    nnRef.current?.setActiveNode(activeNode);
  }, [activeNode]);
  useEffect(() => {
    if (pulseKey !== undefined) nnRef.current?.pulse();
  }, [pulseKey]);

  return (
    <canvas
      ref={canvasRef}
      className={className}
      style={{ width: '100%', height: '100%', display: 'block', ...style }}
    />
  );
};
