import React from 'react';
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts';
import { ProjectExecutionPoint } from '../../types';
import { Activity, Clock, CheckCircle2, XCircle, Ban } from 'lucide-react';

interface ProjectExecutionGraphProps {
  timelineData: ProjectExecutionPoint[];
  projectName: string;
}

export const ProjectExecutionGraph: React.FC<ProjectExecutionGraphProps> = ({
  timelineData,
  projectName,
}) => {
  const hasData = timelineData && timelineData.length > 0;

  return (
    <div
      id="project-execution-graph-panel"
      className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 flex flex-col justify-between h-full min-h-[310px]"
    >
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2">
          <Activity className="w-3.5 h-3.5 text-cyan-400" />
          <h3 className="text-xs font-bold text-slate-100 font-mono uppercase tracking-wider">
            Execution Activity
          </h3>
        </div>

        {/* Legend Indicators */}
        <div className="flex items-center gap-3 text-[10px] font-mono">
          <div className="flex items-center gap-1 text-cyan-400">
            <span className="w-2 h-0.5 bg-cyan-400" />
            <span>Total Executed</span>
          </div>
          <div className="flex items-center gap-1 text-emerald-400">
            <span className="w-2 h-0.5 bg-emerald-400" />
            <span>Passed</span>
          </div>
          <div className="flex items-center gap-1 text-rose-400">
            <span className="w-2 h-0.5 bg-rose-400" />
            <span>Failed</span>
          </div>
          <div className="flex items-center gap-1 text-purple-400">
            <span className="w-2 h-0.5 bg-purple-400" />
            <span>Blocked</span>
          </div>
        </div>
      </div>

      {/* Main Chart Area */}
      <div className="relative flex-1 flex items-center justify-center my-2 min-h-[190px]">
        {hasData ? (
          <div className="w-full h-48">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={timelineData} margin={{ top: 10, right: 10, left: -20, bottom: 0 }}>
                <defs>
                  <linearGradient id="colorExecuted" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#06b6d4" stopOpacity={0.25} />
                    <stop offset="95%" stopColor="#06b6d4" stopOpacity={0.0} />
                  </linearGradient>
                  <linearGradient id="colorPassed" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#10b981" stopOpacity={0.2} />
                    <stop offset="95%" stopColor="#10b981" stopOpacity={0.0} />
                  </linearGradient>
                  <linearGradient id="colorFailed" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#f43f5e" stopOpacity={0.3} />
                    <stop offset="95%" stopColor="#f43f5e" stopOpacity={0.0} />
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 3" stroke="#1e293b" vertical={false} />
                <XAxis
                  dataKey="timeLabel"
                  stroke="#64748b"
                  fontSize={10}
                  tickLine={false}
                  fontFamily="monospace"
                />
                <YAxis
                  stroke="#64748b"
                  fontSize={10}
                  tickLine={false}
                  axisLine={false}
                  fontFamily="monospace"
                />
                <Tooltip
                  content={({ active, payload, label }) => {
                    if (active && payload && payload.length) {
                      const data = payload[0].payload as ProjectExecutionPoint;
                      return (
                        <div className="p-2.5 rounded bg-[#020617] border border-slate-700 shadow-2xl text-xs font-mono space-y-1">
                          <div className="text-[10px] text-slate-400 pb-1 border-b border-slate-800">
                            Time Window: <strong className="text-slate-200">{label}</strong>
                          </div>
                          <div className="flex justify-between gap-4 text-cyan-300">
                            <span>Executed:</span>
                            <span className="font-bold">{data.testsExecuted}</span>
                          </div>
                          <div className="flex justify-between gap-4 text-emerald-400">
                            <span>Passed:</span>
                            <span className="font-bold">{data.passed}</span>
                          </div>
                          <div className="flex justify-between gap-4 text-rose-400">
                            <span>Failed:</span>
                            <span className="font-bold">{data.failed}</span>
                          </div>
                          <div className="flex justify-between gap-4 text-purple-400">
                            <span>Blocked:</span>
                            <span className="font-bold">{data.blocked}</span>
                          </div>
                        </div>
                      );
                    }
                    return null;
                  }}
                />
                <Area
                  type="monotone"
                  dataKey="testsExecuted"
                  stroke="#06b6d4"
                  strokeWidth={2}
                  fillOpacity={1}
                  fill="url(#colorExecuted)"
                />
                <Area
                  type="monotone"
                  dataKey="passed"
                  stroke="#10b981"
                  strokeWidth={1.5}
                  fillOpacity={1}
                  fill="url(#colorPassed)"
                />
                <Area
                  type="monotone"
                  dataKey="failed"
                  stroke="#f43f5e"
                  strokeWidth={1.5}
                  fillOpacity={1}
                  fill="url(#colorFailed)"
                />
                <Area
                  type="monotone"
                  dataKey="blocked"
                  stroke="#a855f7"
                  strokeWidth={1.5}
                  fillOpacity={0}
                />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center text-center p-6 space-y-2">
            <div className="w-16 h-16 rounded-full border-2 border-dashed border-slate-800 flex items-center justify-center text-slate-600">
              <Clock className="w-6 h-6 text-slate-600" />
            </div>
            <div className="text-xs font-bold text-slate-400 font-mono">
              No Execution Activity Recorded
            </div>
            <p className="text-[10px] text-slate-500 max-w-[240px]">
              Execution runs on {projectName} will populate this real-time timeline graph.
            </p>
          </div>
        )}
      </div>

      {/* Footer Info */}
      <div className="flex items-center justify-between pt-2 border-t border-slate-800/80 text-[10px] font-mono text-slate-500">
        <span>X-Axis: Time Window</span>
        <span>Y-Axis: Test Throughput</span>
      </div>
    </div>
  );
};
