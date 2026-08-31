import React from 'react';
import { PieChart, Pie, Cell, ResponsiveContainer, Tooltip } from 'recharts';
import { ProjectMetrics } from '../../types';
import { Layers, AlertCircle } from 'lucide-react';

interface ProjectStatusDonutProps {
  metrics: ProjectMetrics;
  projectName: string;
}

interface ChartDataItem {
  name: string;
  value: number;
  color: string;
}

export const ProjectStatusDonut: React.FC<ProjectStatusDonutProps> = ({ metrics, projectName }) => {
  const total = metrics.totalTestCases;

  const rawData: ChartDataItem[] = [
    { name: 'Passed', value: metrics.passed, color: '#10b981' },
    { name: 'Failed', value: metrics.failed, color: '#f43f5e' },
    { name: 'Running', value: metrics.running, color: '#06b6d4' },
    { name: 'Blocked', value: metrics.blocked, color: '#a855f7' },
    { name: 'Needs Review', value: metrics.needsReview, color: '#f59e0b' },
    { name: 'Skipped', value: metrics.skipped, color: '#64748b' },
  ];

  // Filter out zero-value items for the chart rendering
  const activeData = rawData.filter((item) => item.value > 0);

  const hasData = total > 0 && activeData.length > 0;

  return (
    <div
      id="project-status-donut-panel"
      className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 flex flex-col justify-between h-full min-h-[310px]"
    >
      {/* Header */}
      <div className="flex items-center justify-between pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2">
          <Layers className="w-3.5 h-3.5 text-indigo-400" />
          <h3 className="text-xs font-bold text-slate-100 font-mono uppercase tracking-wider">
            Test Status Distribution
          </h3>
        </div>
        <span className="text-[10px] font-mono text-slate-400">
          {projectName}
        </span>
      </div>

      {/* Main Visualization */}
      <div className="relative flex-1 flex items-center justify-center my-2 min-h-[170px]">
        {hasData ? (
          <div className="w-full h-44 relative">
            <ResponsiveContainer width="100%" height="100%">
              <PieChart>
                <Tooltip
                  content={({ active, payload }) => {
                    if (active && payload && payload.length) {
                      const data = payload[0].payload as ChartDataItem;
                      const pct = ((data.value / total) * 100).toFixed(1);
                      return (
                        <div className="p-2 rounded bg-[#020617] border border-slate-700 shadow-xl text-xs font-mono">
                          <div className="flex items-center gap-2">
                            <span
                              className="w-2 h-2 rounded-full"
                              style={{ backgroundColor: data.color }}
                            />
                            <span className="text-slate-200 font-bold">{data.name}:</span>
                            <span className="text-white font-bold">{data.value.toLocaleString()}</span>
                            <span className="text-slate-400">({pct}%)</span>
                          </div>
                        </div>
                      );
                    }
                    return null;
                  }}
                />
                <Pie
                  data={activeData}
                  cx="50%"
                  cy="50%"
                  innerRadius={52}
                  outerRadius={74}
                  paddingAngle={3}
                  dataKey="value"
                  stroke="#0B1120"
                  strokeWidth={2}
                >
                  {activeData.map((entry, index) => (
                    <Cell key={`cell-${index}`} fill={entry.color} />
                  ))}
                </Pie>
              </PieChart>
            </ResponsiveContainer>

            {/* Centered Total and Project Label */}
            <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none text-center">
              <span className="text-lg font-black font-mono text-slate-100 leading-tight">
                {total.toLocaleString()}
              </span>
              {/* This total is the sum of real task statuses from execution
                  runs (`dashboard.task_counts`), not a count of saved test
                  cases — see the "Executed Tests" KPI card's own comment in
                  `DashboardPage.tsx` for why that distinction matters. */}
              <span className="text-[9px] uppercase font-mono font-semibold text-slate-400 tracking-wider">
                Executed Tasks
              </span>
              <span className="text-[8px] font-sans text-indigo-400 font-medium truncate max-w-[90px]">
                {projectName}
              </span>
            </div>
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center text-center p-6 space-y-2">
            <div className="w-16 h-16 rounded-full border-2 border-dashed border-slate-800 flex items-center justify-center text-slate-600">
              <AlertCircle className="w-6 h-6 text-slate-600" />
            </div>
            <div className="text-xs font-bold text-slate-400 font-mono">
              No test execution data available
            </div>
            <p className="text-[10px] text-slate-500 max-w-[200px]">
              No active test cases or execution telemetry logged for this project yet.
            </p>
          </div>
        )}
      </div>

      {/* Legend Grid */}
      {hasData && (
        <div className="grid grid-cols-3 gap-1.5 pt-2 border-t border-slate-800/80 text-[10px] font-mono">
          {rawData.map((item) => (
            <div
              key={item.name}
              className="flex items-center justify-between p-1 rounded bg-[#020617]/60 border border-slate-800/60"
            >
              <div className="flex items-center gap-1.5 truncate">
                <span
                  className="w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ backgroundColor: item.color }}
                />
                <span className="text-slate-400 truncate">{item.name}</span>
              </div>
              <span className="font-bold text-slate-200 ml-1">
                {item.value.toLocaleString()}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
