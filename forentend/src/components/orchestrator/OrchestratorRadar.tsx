/**
 * Orchestrator Radar — a real-time scanner view of this page's own real
 * executors, replacing `OrchestratorFlowRail` in the same rail slot.
 *
 * The visual language (sweeping radar, glowing blips on concentric rings)
 * is a style choice; every blip is a real executor from `orchState.executors`
 * — not an invented device list. A blip's colour is the real policy gate
 * that executor evaluates before scheduling (`AUTO`/`REQUIRES_APPROVAL`/
 * `BLOCKED`, the exact same field and colour convention the "Executor
 * Capability Routing" panel beside it uses — `gateStyles` in
 * `AIQAOrchestratorPage.tsx`), never a fabricated signal-strength
 * percentage: this backend has no such number for an executor, so none is
 * shown. The centre readout is the real count of runs this process is
 * currently driving (`orchState.active_runs.length`).
 *
 * The sweep itself rotates continuously — an honest claim only that this
 * panel is live-monitoring, the same fact the pulsing "Orchestrator
 * Online" indicator elsewhere in this app already makes — and glows
 * brighter only while a run is genuinely active, so its intensity still
 * carries a real signal rather than being motion for its own sake.
 */
import React from 'react';
import { Radar as RadarIcon } from 'lucide-react';
import type { OrchestratorStateResponse } from '../../types/contract';

export interface OrchestratorRadarProps {
  orchState: OrchestratorStateResponse;
}

const gateDot: Record<string, string> = {
  AUTO: '#34d399',
  REQUIRES_APPROVAL: '#fbbf24',
  BLOCKED: '#fb7185',
};

const gateLabel: Record<string, string> = {
  AUTO: 'text-emerald-400',
  REQUIRES_APPROVAL: 'text-amber-400',
  BLOCKED: 'text-rose-400',
};

const CENTER = 100;
const RING_RADIUS = 70;

export const OrchestratorRadar: React.FC<OrchestratorRadarProps> = ({ orchState }) => {
  const executors = orchState.executors;
  const activeRunCount = orchState.active_runs.length;
  const isLive = activeRunCount > 0;

  const blips = executors.map((executor, i) => {
    const angle = (2 * Math.PI * i) / Math.max(executors.length, 1) - Math.PI / 2;
    const x = CENTER + RING_RADIUS * Math.cos(angle);
    const y = CENTER + RING_RADIUS * Math.sin(angle);
    return { ...executor, x, y };
  });

  return (
    <div id="orchestrator-radar" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 h-full flex flex-col items-center">
      <h3 className="text-[10px] font-bold text-slate-100 font-mono uppercase tracking-wider mb-2 self-stretch flex items-center gap-1.5">
        <RadarIcon className="w-3.5 h-3.5 text-yellow-400" />
        Executor Radar
      </h3>

      <svg viewBox="0 0 200 200" className="w-full max-w-[210px]">
        <defs>
          <radialGradient id="radarGlow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="#facc15" stopOpacity={isLive ? '0.35' : '0.15'} />
            <stop offset="100%" stopColor="#facc15" stopOpacity="0" />
          </radialGradient>
        </defs>

        <circle cx={CENTER} cy={CENTER} r="90" fill="url(#radarGlow)" />
        {[20, 40, 60, 80].map((r) => (
          <circle key={r} cx={CENTER} cy={CENTER} r={r} fill="none" stroke="#854d0e" strokeWidth="1" opacity="0.6" />
        ))}
        <line x1={CENTER} y1="10" x2={CENTER} y2="190" stroke="#854d0e" strokeWidth="1" opacity="0.5" />
        <line x1="10" y1={CENTER} x2="190" y2={CENTER} stroke="#854d0e" strokeWidth="1" opacity="0.5" />

        {/* The sweep — continuous rotation claims only "this panel is live-
            monitoring", never a specific transient event; its opacity is
            the real signal (brighter while a run is genuinely active). */}
        <g opacity={isLive ? '0.9' : '0.45'}>
          <path d={`M ${CENTER} ${CENTER} L ${CENTER} 10 A 90 90 0 0 1 ${CENTER + 90 * Math.sin((40 * Math.PI) / 180)} ${CENTER - 90 * Math.cos((40 * Math.PI) / 180)} Z`} fill="#facc15" fillOpacity="0.25" />
          <line x1={CENTER} y1={CENTER} x2={CENTER} y2="10" stroke="#5eead4" strokeWidth="1.5" />
          <animateTransform attributeName="transform" type="rotate" from={`0 ${CENTER} ${CENTER}`} to={`360 ${CENTER} ${CENTER}`} dur={isLive ? '2.5s' : '6s'} repeatCount="indefinite" />
        </g>

        {blips.map((b) => (
          <g key={b.kind}>
            <circle cx={b.x} cy={b.y} r="4" fill={gateDot[b.gate] ?? '#94a3b8'}>
              {b.gate === 'AUTO' && <animate attributeName="opacity" values="1;0.4;1" dur="2s" repeatCount="indefinite" />}
            </circle>
            <circle cx={b.x} cy={b.y} r="8" fill={gateDot[b.gate] ?? '#94a3b8'} fillOpacity="0.18" />
          </g>
        ))}

        <circle cx={CENTER} cy={CENTER} r="3" fill="#e2e8f0" />
        <text x={CENTER} y={CENTER + 22} textAnchor="middle" className="fill-slate-300" style={{ font: '9px monospace' }}>
          {activeRunCount} active
        </text>
      </svg>

      <ul className="w-full mt-2 space-y-1">
        {executors.map((executor) => (
          <li key={executor.kind} className="flex items-center gap-1.5 text-[10px] font-mono">
            <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: gateDot[executor.gate] ?? '#94a3b8' }} />
            {/* A single combined text node, not a separate bare executor
                name and a separate bare gate label — RTL's `getByText`
                matches an element's own direct text only (nested elements'
                text is excluded from that computation), so two nested
                spans reading "maestro" / "(requires approval)" would still
                each exactly duplicate a leaf span the detailed panel beside
                this rail already renders for the same executor; one plain
                string in one node avoids that regardless of nesting. */}
            <span className={`min-w-0 truncate ${gateLabel[executor.gate] ?? 'text-slate-400'}`}>
              {executor.kind} ({executor.gate.replace(/_/g, ' ').toLowerCase()})
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
};
