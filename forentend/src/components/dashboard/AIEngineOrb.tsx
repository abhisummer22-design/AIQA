/**
 * AI Engine Orb — the real, live routing decision, rendered as an animated
 * neural-activity canvas (`components/common/NeuralActivityCanvas.tsx`,
 * ported from a `NeuralActivity` class the user supplied directly) rather
 * than the earlier static, hand-drawn layered-network SVG diagram.
 *
 * Same non-negotiable rule as before and as everywhere else in this app:
 * **motion and colour must encode something real, or not appear.** Nothing
 * in the canvas is decorative filler — every value driving it is real:
 *
 *   - **Layer shape** — `[7, providers.length, 1]`: 7 real capability
 *     fields, one node per real discovered provider (never a fixed
 *     decorative count), 1 real decision — the same real structure the
 *     previous SVG diagram's three columns had.
 *   - **Input-layer colour + label** — each of the 7 nodes is coloured and
 *     labelled by that real field's real evidenced level
 *     (`STRONG`/`BASIC`/`UNKNOWN`/`NONE`), the exact colours/wording
 *     `AIEnginePanel`'s own capability grid already uses
 *     (`aiCapabilities.ts`, shared so the two can never drift apart).
 *   - **Provider-layer colour + label** — each node is coloured by that
 *     real provider's real health status and labelled with its real name —
 *     restoring the per-provider detail the very first canvas version of
 *     this file could not carry (a plain compressed `level` scalar), now
 *     that the layer's node count is genuinely bound to the real provider
 *     list instead of a demo default.
 *   - **`level`** (overall activity/brightness/speed) — real: a route
 *     existing at all (dominant term) plus the real healthy-provider
 *     fraction, reinforcing the same fact the provider nodes' own colours
 *     already show, not a second, independent claim.
 *   - **`pulseKey`** — increments only when the real routing decision
 *     actually changes (a different `activeProviderId` or `routingScore`
 *     between polls) — a genuine "the engine just decided something new"
 *     event, not a fixed metronome.
 *   - **`accent`** (output node + anything with no per-node colour) — gold
 *     while a real route exists, muted slate when not.
 *   - **Column labels** ("INPUT" / "PROVIDERS" / "OUTPUT") and the real
 *     `provider / model · score` (or honest "NO ROUTE") text below the
 *     canvas — the same orientation the old diagram's own labels gave.
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { NeuralActivityCanvas } from '../common/NeuralActivityCanvas';
import { CAPABILITY_LEVEL_COLOR, CAPABILITY_ORDER, PROVIDER_STATUS_COLOR } from './aiCapabilities';
import type { NodeColorGrid, NodeLabelGrid } from '../../utils/neuralActivity';
import type { AiModelDefinition, AiProviderInfo } from '../../types/contract';

export interface AIEngineOrbProps {
  /** The real model that would be routed to, or `null` when the backend
   *  reports no compatible model. */
  model: AiModelDefinition | null;
  /** Real routing score, shown inside the output node when a route
   *  exists. */
  routingScore: number | null;
  /** Every real discovered provider — one real node per provider. */
  providers: AiProviderInfo[];
  /** The real provider id the routing decision actually chose, or `null`
   *  when there is no route. */
  activeProviderId: string | null;
}

/** Real evidenced level -> a real 0..1 activation. */
const LEVEL_ACTIVATION: Record<keyof typeof CAPABILITY_LEVEL_COLOR, number> = {
  STRONG: 1,
  BASIC: 0.6,
  UNKNOWN: 0.25,
  NONE: 0,
  idle: 0,
};

const LIVE_ACCENT = '#facc15';
const IDLE_ACCENT = '#475569';

/** Canvas space is tightest in the provider column (it sits between two
 *  other columns on both sides) — a real name is truncated for legibility,
 *  never altered in meaning (the full real name is still shown, unchanged,
 *  in `AIEnginePanel`'s own "Providers" list right below this orb). */
function shortProviderLabel(name: string): string {
  return name.length > 11 ? `${name.slice(0, 10)}…` : name;
}

/** Pure so it is directly unit-testable without a canvas — the real
 *  per-input-node colour row. */
export function capabilityNodeColors(model: AiModelDefinition | null): (string | null)[] {
  return CAPABILITY_ORDER.map(({ key }) =>
    model ? CAPABILITY_LEVEL_COLOR[model.capabilities[key] ?? 'UNKNOWN'] : CAPABILITY_LEVEL_COLOR.idle,
  );
}

/** Pure so it is directly unit-testable without a canvas — the real
 *  per-input-node label row. */
export function capabilityNodeLabels(): string[] {
  return CAPABILITY_ORDER.map(({ label }) => label);
}

/** Pure so it is directly unit-testable without a canvas — the real
 *  provider-layer index of the real winning provider, or `null` when
 *  there is none (no route, or the id genuinely matches no discovered
 *  provider). This is the one node `activeNode` marks as genuinely "on
 *  the path" — see `NeuralActivityCanvas`'s own doc comment on that prop
 *  for why every other provider node's edges must NOT animate. */
export function activeProviderNodeIndex(
  providers: AiProviderInfo[],
  activeProviderId: string | null,
): number | null {
  if (!activeProviderId) return null;
  const i = providers.findIndex((p) => p.id === activeProviderId);
  return i >= 0 ? i : null;
}

export const AIEngineOrb: React.FC<AIEngineOrbProps> = ({ model, routingScore, providers, activeProviderId }) => {
  const live = model !== null && routingScore !== null;

  // Real structure: 7 capability fields -> one node per real discovered
  // provider -> 1 real decision. Memoized on the real provider count only,
  // so a routine poll that finds the same providers never restarts the
  // animation (see `NeuralActivityCanvas`'s own doc comment on `layers`).
  const layers = useMemo(() => [CAPABILITY_ORDER.length, providers.length, 1], [providers.length]);

  const inputs = useMemo<number[]>(
    () =>
      CAPABILITY_ORDER.map(({ key }) =>
        live ? LEVEL_ACTIVATION[model!.capabilities[key] ?? 'UNKNOWN'] : 0,
      ),
    [live, model],
  );

  const nodeColors = useMemo<NodeColorGrid>(
    () => [
      live ? capabilityNodeColors(model) : CAPABILITY_ORDER.map(() => CAPABILITY_LEVEL_COLOR.idle),
      providers.map((p) => PROVIDER_STATUS_COLOR[p.status]),
      [], // output node — no override, falls back to the live/idle accent
    ],
    [live, model, providers],
  );

  const decisionLabel =
    live && routingScore !== null ? `${activeProviderId} · ${routingScore.toFixed(0)}` : 'NO ROUTE';

  const nodeLabels = useMemo<NodeLabelGrid>(
    () => [
      capabilityNodeLabels(),
      providers.map((p) => shortProviderLabel(p.name)),
      [decisionLabel], // the one real output node's own real decision
    ],
    [providers, decisionLabel],
  );

  const layerLabels = useMemo(() => ['INPUT', 'PROVIDERS', 'OUTPUT'], []);

  // The one real node genuinely on the winning path — only edges/signals
  // touching it animate (`NeuralActivityCanvas`'s own `activeNode` doc
  // comment); every other real, discovered-but-not-chosen provider (e.g.
  // a genuinely unconfigured one) stays a flat, static connection, never
  // implying data is flowing to it when it truthfully is not.
  const activeNode = useMemo(() => {
    if (!live) return null;
    const i = activeProviderNodeIndex(providers, activeProviderId);
    return i !== null ? { l: 1, i } : null;
  }, [live, activeProviderId, providers]);

  const healthyFraction = useMemo(() => {
    if (providers.length === 0) return 0;
    const healthy = providers.filter((p) => p.status === 'HEALTHY' || p.status === 'AVAILABLE').length;
    return healthy / providers.length;
  }, [providers]);

  // A real, disclosed compression — see this file's own module doc
  // comment. Reinforces, rather than replaces, what the provider nodes'
  // own real colours already show.
  const level = live ? 0.35 + healthyFraction * 0.45 : 0.08;

  // Pulse only on a genuine change in the real decision — never a fixed
  // metronome unrelated to anything actually happening.
  const [pulseKey, setPulseKey] = useState(0);
  const prevDecisionRef = useRef<string | null>(null);
  useEffect(() => {
    const decision = live ? `${activeProviderId ?? ''}:${routingScore ?? ''}` : null;
    if (decision !== null && decision !== prevDecisionRef.current) {
      setPulseKey((k) => k + 1);
    }
    prevDecisionRef.current = decision;
  }, [live, activeProviderId, routingScore]);

  const capDataAttrs = Object.fromEntries(
    CAPABILITY_ORDER.map(({ key }) => [`data-cap-${key}`, live ? model!.capabilities[key] ?? 'UNKNOWN' : 'idle']),
  );

  // The fuller-text HTML version of `decisionLabel` above — spans the
  // canvas's full width below it, so has room to spell out "score" where
  // the on-node label (squeezed beside a single node) does not.
  const label = live && routingScore !== null ? `${activeProviderId} · score ${routingScore.toFixed(0)}` : 'NO ROUTE';

  return (
    <div
      id="ai-engine-orb"
      data-live={live}
      data-active-provider={activeProviderId ?? ''}
      data-routing-score={routingScore ?? ''}
      data-level={level.toFixed(3)}
      {...capDataAttrs}
      className="w-full"
      aria-hidden="true"
    >
      <div className="w-full h-[200px]">
        <NeuralActivityCanvas
          level={level}
          inputs={inputs}
          pulseKey={pulseKey}
          accent={live ? LIVE_ACCENT : IDLE_ACCENT}
          layers={layers}
          nodeColors={nodeColors}
          nodeLabels={nodeLabels}
          layerLabels={layerLabels}
          activeNode={activeNode}
        />
      </div>
      {/* Below the canvas, not laid over it: with four or more providers the last provider node's own label
          sits in the canvas's bottom margin, and a label laid over that margin covered it. */}
      <div
        className={`mt-1 text-center text-[10px] font-mono font-bold tracking-wide truncate px-2 ${
          live ? 'text-yellow-300' : 'text-content-muted'
        }`}
      >
        {label}
      </div>
    </div>
  );
};
