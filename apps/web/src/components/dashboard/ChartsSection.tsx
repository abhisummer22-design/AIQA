import React from "react";
import {
  AreaChart,
  Area,
  LineChart,
  Line,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend
} from "recharts";
import { TrendingUp, Activity, BarChart2 } from "lucide-react";
import { KpiMetrics } from "../../types";

interface ChartsSectionProps {
  metrics: KpiMetrics;
}

const trendData = [
  { time: "06:00", passed: 240, failed: 8, latency: 280 },
  { time: "08:00", passed: 410, failed: 12, latency: 310 },
  { time: "10:00", passed: 580, failed: 15, latency: 390 },
  { time: "12:00", passed: 920, failed: 22, latency: 420 },
  { time: "14:00", passed: 1150, failed: 25, latency: 385 },
  { time: "16:00", passed: 1320, failed: 29, latency: 340 },
  { time: "18:00", passed: 1420, failed: 31, latency: 360 }
];

const latencyDistribution = [
  { bucket: "<100ms", count: 420, fill: "#6366f1" },
  { bucket: "100-300ms", count: 680, fill: "#818cf8" },
  { bucket: "300-500ms", count: 240, fill: "#4f46e5" },
  { bucket: "500-1000ms", count: 65, fill: "#f59e0b" },
  { bucket: ">1000ms", count: 15, fill: "#f43f5e" }
];

export const ChartsSection: React.FC<ChartsSectionProps> = ({ metrics }) => {
  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
      {/* Chart 1: Pass vs Fail Execution Volume & Quality Curve */}
      <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 space-y-2.5">
        <div className="flex items-center justify-between pb-2 border-b border-slate-800">
          <div className="flex items-center gap-2">
            <TrendingUp className="w-3.5 h-3.5 text-indigo-400" />
            <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
              Execution Volume & Pass Rate
            </h3>
          </div>
          <span className="text-[10px] font-mono text-emerald-400 font-semibold">96.8% Pass SLA</span>
        </div>

        <div className="h-48 w-full text-[10px] font-mono">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={trendData} margin={{ top: 5, right: 5, left: -25, bottom: 0 }}>
              <defs>
                <linearGradient id="passedGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#6366f1" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#6366f1" stopOpacity={0.0} />
                </linearGradient>
                <linearGradient id="failedGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#f43f5e" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#f43f5e" stopOpacity={0.0} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="2 2" stroke="#1e293b" />
              <XAxis dataKey="time" stroke="#64748b" tick={{ fontSize: 10 }} />
              <YAxis stroke="#64748b" tick={{ fontSize: 10 }} />
              <Tooltip
                contentStyle={{
                  backgroundColor: "#0D0D0E",
                  borderColor: "#334155",
                  borderRadius: "4px",
                  color: "#f8fafc",
                  fontSize: "11px",
                  fontFamily: "monospace"
                }}
              />
              <Area
                type="monotone"
                dataKey="passed"
                name="Passed Tests"
                stroke="#6366f1"
                strokeWidth={1.5}
                fillOpacity={1}
                fill="url(#passedGradient)"
              />
              <Area
                type="monotone"
                dataKey="failed"
                name="Failed Tests"
                stroke="#f43f5e"
                strokeWidth={1.5}
                fillOpacity={1}
                fill="url(#failedGradient)"
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Chart 2: AI & API Latency SLA Distribution */}
      <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 space-y-2.5">
        <div className="flex items-center justify-between pb-2 border-b border-slate-800">
          <div className="flex items-center gap-2">
            <Activity className="w-3.5 h-3.5 text-indigo-400" />
            <h3 className="text-xs font-bold font-mono uppercase text-white tracking-wider">
              Response Latency Distribution
            </h3>
          </div>
          <span className="text-[10px] font-mono text-indigo-400 font-semibold">P95: 412ms</span>
        </div>

        <div className="h-48 w-full text-[10px] font-mono">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={latencyDistribution} margin={{ top: 5, right: 5, left: -25, bottom: 0 }}>
              <CartesianGrid strokeDasharray="2 2" stroke="#1e293b" />
              <XAxis dataKey="bucket" stroke="#64748b" tick={{ fontSize: 10 }} />
              <YAxis stroke="#64748b" tick={{ fontSize: 10 }} />
              <Tooltip
                contentStyle={{
                  backgroundColor: "#0D0D0E",
                  borderColor: "#334155",
                  borderRadius: "4px",
                  color: "#f8fafc",
                  fontSize: "11px",
                  fontFamily: "monospace"
                }}
              />
              <Bar dataKey="count" name="Requests / Tests" fill="#6366f1" radius={[2, 2, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  );
};

