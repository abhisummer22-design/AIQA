import React, { useState, useEffect, useMemo } from "react";
import {
  Cpu,
  Zap,
  RotateCcw,
  Sparkles,
  CheckCircle2,
  Clock,
  Activity,
  ArrowRight,
  TrendingDown,
  TrendingUp
} from "lucide-react";
import {
  ResponsiveContainer,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ReferenceLine
} from "recharts";
import { ModelRouterState, ViewType } from "../../types";

interface ModelRouterStatusProps {
  modelRouter: ModelRouterState;
  onNavigate: (view: ViewType) => void;
}

interface LatencyPoint {
  timeLabel: string;
  secondOffset: number;
  latency: number;
  model: string;
  isFallback: boolean;
}

export const ModelRouterStatus: React.FC<ModelRouterStatusProps> = ({
  modelRouter,
  onNavigate
}) => {
  const isFallbackActive =
    modelRouter.currentActiveModel.includes("Gemini") ||
    modelRouter.primaryModel.status === "offline";

  // Rolling 60-second history buffer
  const [latencyHistory, setLatencyHistory] = useState<LatencyPoint[]>(() => {
    const baseLatency = modelRouter.currentLatencyMs || 380;
    const initialPoints: LatencyPoint[] = [];
    for (let i = 59; i >= 0; i--) {
      // Deterministic realistic jitter for initial 60 seconds
      const wave = Math.sin(i / 4) * 28 + Math.cos(i / 7) * 18;
      const noise = ((i * 17) % 23) - 11;
      const simulatedLatency = Math.max(80, Math.round(baseLatency + wave + noise));
      initialPoints.push({
        timeLabel: i === 0 ? "Now" : `-${i}s`,
        secondOffset: -i,
        latency: simulatedLatency,
        model: modelRouter.currentActiveModel,
        isFallback: isFallbackActive
      });
    }
    return initialPoints;
  });

  // Rolling update effect every 1 second
  useEffect(() => {
    const interval = setInterval(() => {
      setLatencyHistory((prev) => {
        const baseLatency = modelRouter.currentLatencyMs || 380;
        // Minor natural variance per second
        const randomJitter = Math.floor(Math.random() * 25) - 12;
        const newLatency = Math.max(60, baseLatency + randomJitter);

        const newPoint: LatencyPoint = {
          timeLabel: "Now",
          secondOffset: 0,
          latency: newLatency,
          model: modelRouter.currentActiveModel,
          isFallback: isFallbackActive
        };

        const updated = [...prev.slice(1), newPoint];
        return updated.map((pt, idx) => {
          const sec = 59 - idx;
          return {
            ...pt,
            timeLabel: sec === 0 ? "Now" : `-${sec}s`,
            secondOffset: -sec
          };
        });
      });
    }, 1000);

    return () => clearInterval(interval);
  }, [modelRouter.currentLatencyMs, modelRouter.currentActiveModel, isFallbackActive]);

  // Derived 60s summary stats
  const stats = useMemo(() => {
    if (latencyHistory.length === 0) {
      return { min: 0, max: 0, avg: 0, p95: 0, trend: 0 };
    }
    const latencies = latencyHistory.map((p) => p.latency);
    const min = Math.min(...latencies);
    const max = Math.max(...latencies);
    const sum = latencies.reduce((a, b) => a + b, 0);
    const avg = Math.round(sum / latencies.length);

    const sorted = [...latencies].sort((a, b) => a - b);
    const p95Index = Math.floor(sorted.length * 0.95);
    const p95 = sorted[p95Index] || max;

    // Trend: difference between last 10s avg and first 10s avg
    const recent10 = latencies.slice(-10).reduce((a, b) => a + b, 0) / 10;
    const initial10 = latencies.slice(0, 10).reduce((a, b) => a + b, 0) / 10;
    const trend = Math.round(recent10 - initial10);

    return { min, max, avg, p95, trend };
  }, [latencyHistory]);

  const chartColor = isFallbackActive ? "#f59e0b" : "#818cf8";
  const chartFillGradient = isFallbackActive ? "fallbackGradient" : "localGradient";

  return (
    <div id="model-router-status-panel" className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between pb-2 border-b border-slate-800">
        <div className="flex items-center gap-2">
          <Cpu className="w-3.5 h-3.5 text-indigo-400" />
          <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
            AI Model Router &amp; Multi-Engine
          </h3>
        </div>
        <button
          type="button"
          onClick={() => onNavigate("ai-brain")}
          className="text-[10px] font-mono text-indigo-400 hover:underline flex items-center gap-1 cursor-pointer"
        >
          <span>Main Brain &amp; Keys &rarr;</span>
        </button>
      </div>

      {/* Active Model Dispatch Card */}
      <div className="p-2.5 rounded bg-[#070708] border border-slate-800 flex flex-col sm:flex-row sm:items-center justify-between gap-2.5">
        <div className="space-y-0.5">
          <div className="text-[9px] font-mono uppercase text-slate-500">Current Serving Model</div>
          <div className="flex items-center gap-2">
            <span className="text-xs font-bold font-mono text-white">
              {modelRouter.currentActiveModel}
            </span>
            <span
              className={`text-[9px] font-mono px-1.5 py-0.2 rounded font-semibold ${
                isFallbackActive
                  ? "bg-amber-500/10 text-amber-400 border border-amber-500/30"
                  : "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
              }`}
            >
              {isFallbackActive ? "Cloud Fallback" : "Local Inference"}
            </span>
          </div>
        </div>

        <div className="flex items-center gap-3 text-[11px] font-mono text-slate-300">
          <div>
            <div className="text-[9px] text-slate-500">Active Latency</div>
            <div className="text-indigo-400 font-bold">{modelRouter.currentLatencyMs} ms</div>
          </div>
          <div>
            <div className="text-[9px] text-slate-500">Throughput</div>
            <div className="text-slate-200 font-bold">{modelRouter.requestsPerMinute} req/min</div>
          </div>
        </div>
      </div>

      {/* 60-Second Latency Sparkline Chart using Recharts */}
      <div className="p-2.5 rounded bg-[#070708] border border-slate-800/90 space-y-2">
        <div className="flex items-center justify-between text-xs font-mono">
          <div className="flex items-center gap-1.5">
            <span className="relative flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-indigo-400 opacity-75" />
              <span className="relative inline-flex rounded-full h-2 w-2 bg-indigo-500" />
            </span>
            <span className="text-[10px] font-bold text-slate-300 uppercase tracking-wider">
              Response Latency (Last 60s)
            </span>
          </div>

          <div className="flex items-center gap-2 text-[10px] text-slate-400">
            <span className="flex items-center gap-1">
              <span className="text-slate-500">Min/Max:</span>
              <span className="text-slate-200 font-bold">{stats.min}–{stats.max}ms</span>
            </span>
            <span className="text-slate-600">|</span>
            <span className="flex items-center gap-1">
              <span className="text-slate-500">P95:</span>
              <span className="text-indigo-400 font-bold">{stats.p95}ms</span>
            </span>
            <span className="text-slate-600">|</span>
            <span className="flex items-center gap-0.5">
              {stats.trend <= 0 ? (
                <span className="text-emerald-400 flex items-center font-bold">
                  <TrendingDown className="w-3 h-3" />
                  {Math.abs(stats.trend)}ms
                </span>
              ) : (
                <span className="text-amber-400 flex items-center font-bold">
                  <TrendingUp className="w-3 h-3" />
                  +{stats.trend}ms
                </span>
              )}
            </span>
          </div>
        </div>

        {/* Recharts Sparkline Container */}
        <div className="h-16 w-full pt-1">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart
              data={latencyHistory}
              margin={{ top: 4, right: 2, left: 2, bottom: 0 }}
            >
              <defs>
                <linearGradient id="localGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#818cf8" stopOpacity={0.45} />
                  <stop offset="95%" stopColor="#818cf8" stopOpacity={0.0} />
                </linearGradient>
                <linearGradient id="fallbackGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#f59e0b" stopOpacity={0.45} />
                  <stop offset="95%" stopColor="#f59e0b" stopOpacity={0.0} />
                </linearGradient>
              </defs>

              <XAxis
                dataKey="timeLabel"
                tickLine={false}
                axisLine={{ stroke: "#27272a", strokeWidth: 1 }}
                interval={14}
                tick={{ fill: "#71717a", fontSize: 8, fontFamily: "monospace" }}
              />

              <YAxis
                domain={[(dataMin: number) => Math.max(0, Math.floor(dataMin * 0.8)), (dataMax: number) => Math.ceil(dataMax * 1.15)]}
                hide
              />

              <Tooltip
                content={({ active, payload }) => {
                  if (active && payload && payload.length) {
                    const data = payload[0].payload as LatencyPoint;
                    return (
                      <div className="bg-zinc-950/95 border border-zinc-700 px-2 py-1.5 rounded shadow-xl font-mono text-[10px] space-y-0.5 pointer-events-none z-50">
                        <div className="text-slate-400 font-bold flex items-center justify-between gap-3">
                          <span>{data.timeLabel === "Now" ? "Current Time (0s)" : `${Math.abs(data.secondOffset)}s ago`}</span>
                          <span className={data.isFallback ? "text-amber-400" : "text-emerald-400"}>
                            {data.isFallback ? "Fallback" : "Ollama"}
                          </span>
                        </div>
                        <div className="text-white font-bold flex items-center gap-1.5">
                          <span className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: chartColor }} />
                          <span>{data.latency} ms</span>
                        </div>
                        <div className="text-[9px] text-slate-500 truncate max-w-[140px]">
                          {data.model}
                        </div>
                      </div>
                    );
                  }
                  return null;
                }}
              />

              {modelRouter.averageLatencyMs > 0 && (
                <ReferenceLine
                  y={modelRouter.averageLatencyMs}
                  stroke="#4f46e5"
                  strokeDasharray="2 2"
                  strokeOpacity={0.6}
                />
              )}

              <Area
                type="monotone"
                dataKey="latency"
                stroke={chartColor}
                strokeWidth={1.75}
                fill={`url(#${chartFillGradient})`}
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>

        {/* Sparkline Baseline Indicator Footer */}
        <div className="flex items-center justify-between text-[9px] font-mono text-slate-500 px-0.5">
          <span>60s Rolling Window</span>
          <span className="flex items-center gap-1 text-slate-400">
            <span className="w-2 h-0.5 bg-indigo-500 inline-block" />
            <span>Target Avg: {modelRouter.averageLatencyMs}ms</span>
          </span>
          <span className="text-indigo-400 font-semibold">Live Polling</span>
        </div>
      </div>

      {/* Primary vs Fallback Spec Matrix */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 text-xs font-mono">
        {/* Primary Model (Ollama) */}
        <div
          className={`p-2.5 rounded border ${
            !isFallbackActive
              ? "bg-indigo-500/5 border-indigo-500/30"
              : "bg-slate-900/30 border-slate-800 opacity-70"
          }`}
        >
          <div className="flex items-center justify-between text-slate-400 text-[9px] mb-1">
            <span>PRIMARY INFERENCE NODE</span>
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                modelRouter.primaryModel.status === "online" ? "bg-emerald-400" : "bg-rose-500"
              }`}
            />
          </div>
          <div className="font-bold text-slate-200 text-xs">{modelRouter.primaryModel.name}</div>
          <div className="text-[10px] text-slate-400 mt-0.5">{modelRouter.primaryModel.runtime}</div>
          <div className="mt-2 text-[9px] text-slate-500 flex items-center justify-between border-t border-slate-800/80 pt-1">
            <span>Avg Latency: 420ms</span>
            <span className="text-emerald-400">99.4% Health</span>
          </div>
        </div>

        {/* Fallback Model (Gemini 3.7 Cloud) */}
        <div
          className={`p-2.5 rounded border ${
            isFallbackActive
              ? "bg-amber-500/5 border-amber-500/30"
              : "bg-slate-900/30 border-slate-800"
          }`}
        >
          <div className="flex items-center justify-between text-slate-400 text-[9px] mb-1">
            <span>CLOUD RESILIENCE FALLBACK</span>
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
          </div>
          <div className="font-bold text-slate-200 text-xs">{modelRouter.fallbackModel.name}</div>
          <div className="text-[10px] text-slate-400 mt-0.5">{modelRouter.fallbackModel.runtime}</div>
          <div className="mt-2 text-[9px] text-slate-500 flex items-center justify-between border-t border-slate-800/80 pt-1">
            <span>Avg Latency: 290ms</span>
            <span className="text-indigo-400">SLA 100%</span>
          </div>
        </div>
      </div>

      {/* Router Metrics bar */}
      <div className="pt-2 border-t border-slate-800 grid grid-cols-4 gap-2 text-center text-xs font-mono">
        <div className="p-1.5 rounded bg-[#070708] border border-slate-800">
          <div className="text-[9px] text-slate-500 uppercase">Avg Latency</div>
          <div className="font-bold text-slate-200 text-xs">{modelRouter.averageLatencyMs}ms</div>
        </div>
        <div className="p-1.5 rounded bg-[#070708] border border-slate-800">
          <div className="text-[9px] text-slate-500 uppercase">Success Rate</div>
          <div className="font-bold text-emerald-400 text-xs">{modelRouter.successRate}%</div>
        </div>
        <div className="p-1.5 rounded bg-[#070708] border border-slate-800">
          <div className="text-[9px] text-slate-500 uppercase">Error Rate</div>
          <div className="font-bold text-rose-400 text-xs">{modelRouter.errorRate}%</div>
        </div>
        <div className="p-1.5 rounded bg-[#070708] border border-slate-800">
          <div className="text-[9px] text-slate-500 uppercase">Fallbacks</div>
          <div className="font-bold text-amber-400 text-xs">{modelRouter.fallbackCountToday}</div>
        </div>
      </div>
    </div>
  );
};


