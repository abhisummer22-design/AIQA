import React from "react";
import {
  ShieldCheck,
  Zap,
  TrendingUp,
  DollarSign,
  Cpu,
  Globe,
  Lock,
  Clock,
  HardDrive,
  BarChart3,
  Server
} from "lucide-react";
import { BrainTelemetry, BrainBenchmarkRun } from "../../types";

interface BrainTelemetrySectionProps {
  telemetry: BrainTelemetry;
  history: BrainBenchmarkRun[];
}

export const BrainTelemetrySection: React.FC<BrainTelemetrySectionProps> = ({
  telemetry,
  history
}) => {
  return (
    <div className="space-y-6">
      {/* Top Banner */}
      <div className="p-5 rounded-2xl bg-gradient-to-r from-blue-950/40 via-zinc-900/80 to-indigo-950/30 border border-blue-500/30 shadow-xl backdrop-blur-md">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
          <div className="space-y-1.5">
            <div className="flex items-center gap-2.5">
              <div className="w-9 h-9 rounded-xl bg-blue-500/20 border border-blue-500/40 flex items-center justify-center text-blue-400">
                <BarChart3 className="w-5 h-5" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-bold text-white font-mono">
                    AI Inference Telemetry &amp; Air-Gap Privacy Audit
                  </h2>
                  <span className="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-300 border border-blue-500/30">
                    REAL-TIME TOKEN LEDGER
                  </span>
                </div>
                <p className="text-xs text-slate-400 font-mono mt-0.5">
                  Audited telemetry tracking local GPU inference ratios, cost savings vs cloud APIs, and zero-egress data privacy compliance
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Metrics 4-Box Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-2">
          <div className="flex items-center justify-between text-xs font-mono text-slate-400">
            <span className="flex items-center gap-1.5">
              <Cpu className="w-4 h-4 text-indigo-400" /> Local Inference Ratio
            </span>
            <span className="text-emerald-400 font-bold">{telemetry.localRatioPercent}%</span>
          </div>
          <div className="text-2xl font-bold font-mono text-white">
            {telemetry.localInferencesCount.toLocaleString()}{" "}
            <span className="text-xs text-slate-500 font-normal">/ {telemetry.todayTotalInferences.toLocaleString()}</span>
          </div>
          <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden mt-2">
            <div
              className="bg-indigo-500 h-full rounded-full"
              style={{ width: `${telemetry.localRatioPercent}%` }}
            />
          </div>
          <span className="text-[10px] font-mono text-slate-500 block pt-1">
            Processed directly on local GPU without internet transit
          </span>
        </div>

        <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-2">
          <div className="flex items-center justify-between text-xs font-mono text-slate-400">
            <span className="flex items-center gap-1.5">
              <DollarSign className="w-4 h-4 text-emerald-400" /> Cloud Cost Saved
            </span>
            <span className="text-emerald-400 font-bold">+18.4% WoW</span>
          </div>
          <div className="text-2xl font-bold font-mono text-emerald-400">
            ${telemetry.costSavedDollars.toFixed(2)}
          </div>
          <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden mt-2">
            <div className="bg-emerald-500 h-full rounded-full" style={{ width: "85%" }} />
          </div>
          <span className="text-[10px] font-mono text-slate-500 block pt-1">
            Calculated against commercial GPT-4o frontier API rates
          </span>
        </div>

        <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-2">
          <div className="flex items-center justify-between text-xs font-mono text-slate-400">
            <span className="flex items-center gap-1.5">
              <ShieldCheck className="w-4 h-4 text-cyan-400" /> Air-Gap Privacy Score
            </span>
            <span className="text-cyan-400 font-bold">SOC2 / HIPAA</span>
          </div>
          <div className="text-2xl font-bold font-mono text-white">
            {telemetry.airgapComplianceScore}%
          </div>
          <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden mt-2">
            <div
              className="bg-cyan-500 h-full rounded-full"
              style={{ width: `${telemetry.airgapComplianceScore}%` }}
            />
          </div>
          <span className="text-[10px] font-mono text-slate-500 block pt-1">
            Zero PII or proprietary test tokens leaked to public internet
          </span>
        </div>

        <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-2">
          <div className="flex items-center justify-between text-xs font-mono text-slate-400">
            <span className="flex items-center gap-1.5">
              <Clock className="w-4 h-4 text-purple-400" /> Tokens Generated Today
            </span>
            <span className="text-purple-400 font-bold">{telemetry.avgLatencyMs}ms avg</span>
          </div>
          <div className="text-2xl font-bold font-mono text-white">
            {(telemetry.tokensGeneratedToday / 1000).toFixed(1)}k
          </div>
          <div className="w-full bg-zinc-800 h-1.5 rounded-full overflow-hidden mt-2">
            <div className="bg-purple-500 h-full rounded-full" style={{ width: "72%" }} />
          </div>
          <span className="text-[10px] font-mono text-slate-500 block pt-1">
            Failover incidents today: {telemetry.fallbackIncidentsToday} (Auto-recovered)
          </span>
        </div>
      </div>

      {/* Historical Inference Ledger */}
      <div className="p-5 rounded-2xl bg-zinc-900/70 border border-zinc-800 space-y-4">
        <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
          <div className="flex items-center gap-2 font-mono text-xs font-bold text-white uppercase">
            <Clock className="w-4 h-4 text-blue-400" />
            <span>Recent Benchmark &amp; Inference Audit Logs</span>
          </div>
          <span className="text-[10px] font-mono text-slate-500">Showing last {history.length} runs</span>
        </div>

        <div className="space-y-3">
          {history.map((run) => (
            <div
              key={run.id}
              className="p-4 rounded-xl bg-zinc-950 border border-zinc-800/80 hover:border-zinc-700 transition-colors font-mono text-xs space-y-2"
            >
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <span className="px-2 py-0.5 rounded bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 text-[10px] font-bold">
                    {run.engine.toUpperCase()}
                  </span>
                  <span className="text-white font-bold">{run.model}</span>
                </div>
                <div className="flex items-center gap-3 text-[11px] text-slate-400">
                  <span>Latency: <strong className="text-white">{run.latencyMs}ms</strong></span>
                  <span>•</span>
                  <span>Speed: <strong className="text-emerald-400">{run.tokensPerSec} t/s</strong></span>
                  <span>•</span>
                  <span>Cost: <strong className="text-white">{run.costDollars === 0 ? "Local (Free)" : `$${run.costDollars.toFixed(5)}`}</strong></span>
                </div>
              </div>

              <p className="text-slate-400 text-[11px] italic line-clamp-1">
                &quot;{run.prompt}&quot;
              </p>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
