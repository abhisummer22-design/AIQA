/**
 * Neural Graph summary bar (spec §11) — every value is read straight from
 * `NeuralGraphSummary`, already real (see `useNeuralGraphData.ts`).
 * `coveragePercent`/`routingScore` render "N/A" for their real `null`
 * case (no test cases to compute a ratio over; no compatible model
 * routed) — the one place N/A is honest here, since every other field is
 * always a real, already-computed number (including a real zero).
 */
import React from 'react';
import { AlertTriangle, CheckCircle2, FlaskConical, Gauge, PlayCircle, ShieldAlert, Sparkles } from 'lucide-react';
import type { NeuralGraphSummary } from './neuralGraph.types';

const Stat: React.FC<{ icon: React.ComponentType<{ className?: string }>; label: string; value: string; accent?: string }> = ({ icon: Icon, label, value, accent }) => (
  <div className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800">
    <Icon className={`w-3.5 h-3.5 shrink-0 ${accent ?? 'text-slate-500'}`} />
    <div className="min-w-0">
      <div className="text-[9px] font-mono text-slate-500 uppercase tracking-wide truncate">{label}</div>
      <div className="text-xs font-mono font-bold text-slate-200">{value}</div>
    </div>
  </div>
);

export const NeuralGraphStats: React.FC<{ summary: NeuralGraphSummary }> = ({ summary }) => (
  <div id="neural-graph-stats" className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-7 gap-2">
    <Stat icon={Sparkles} label="Nodes Active" value={`${summary.nodesActive}/${summary.nodesTotal}`} accent="text-cyan-400" />
    <Stat icon={FlaskConical} label="Tests Generated" value={String(summary.testsGenerated)} accent="text-indigo-400" />
    <Stat icon={PlayCircle} label="Tests Executed" value={String(summary.testsExecuted)} accent="text-emerald-400" />
    <Stat icon={AlertTriangle} label="Risk Flagged" value={summary.risksDetected === null ? 'N/A' : String(summary.risksDetected)} accent="text-amber-400" />
    <Stat icon={ShieldAlert} label="Critical Issues" value={summary.criticalIssues === null ? 'N/A' : String(summary.criticalIssues)} accent="text-rose-400" />
    <Stat icon={CheckCircle2} label="Coverage" value={summary.coveragePercent === null ? 'N/A' : `${summary.coveragePercent}%`} accent="text-emerald-400" />
    <Stat icon={Gauge} label="Routing Score" value={summary.routingScore === null ? 'N/A' : summary.routingScore.toFixed(1)} accent="text-cyan-400" />
  </div>
);
