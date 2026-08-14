import React from "react";
import { CheckCircle2, XCircle, Clock, AlertOctagon, HelpCircle, Activity, Gauge, TrendingUp } from "lucide-react";
import { TestExecutionAnalytics, TestType } from "../../types";
import { TEST_TYPE_LABELS } from "./TestCategoryNav";

interface TestExecutionAnalyticsBarProps {
  analytics: TestExecutionAnalytics | null;
  activeCategory: TestType | "ALL";
}

export const TestExecutionAnalyticsBar: React.FC<TestExecutionAnalyticsBarProps> = ({
  analytics,
  activeCategory
}) => {
  if (!analytics) {
    return (
      <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2 animate-pulse">
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} className="h-16 rounded-xl bg-slate-900/60 border border-slate-800" />
        ))}
      </div>
    );
  }

  const categoryName = activeCategory === "ALL" ? "All Categories" : TEST_TYPE_LABELS[activeCategory] || activeCategory;

  return (
    <div className="space-y-2">
      {/* Category Indicator sub-pill */}
      <div className="flex items-center justify-between text-[11px] font-mono text-slate-400 px-1">
        <div className="flex items-center gap-1.5">
          <Activity className="w-3.5 h-3.5 text-cyan-400" />
          <span>Execution Telemetry Scope:</span>
          <span className="font-bold text-slate-200 uppercase bg-slate-800/80 px-2 py-0.5 rounded border border-slate-700">
            {categoryName}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span>Pass Rate Target:</span>
          <span className="font-bold text-emerald-400">95.0% SLA</span>
        </div>
      </div>

      {/* 8 Metric KPI Cards */}
      <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2 text-xs font-mono">
        {/* 1. Total */}
        <div className="p-3 rounded-xl bg-[#0e1422] border border-slate-800/90 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-slate-500 uppercase font-semibold">Total Tests</div>
          <div className="text-xl font-bold text-white font-mono mt-0.5">{analytics.totalTests}</div>
          <div className="text-[10px] text-slate-400 flex items-center gap-1 mt-1">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
            <span>In repository</span>
          </div>
        </div>

        {/* 2. Passed */}
        <div className="p-3 rounded-xl bg-[#081519] border border-emerald-900/60 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-emerald-400 uppercase font-semibold flex items-center justify-between">
            <span>Passed</span>
            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-xl font-bold text-emerald-300 font-mono mt-0.5">{analytics.passed}</div>
          <div className="text-[10px] text-emerald-500/80 mt-1 font-semibold">
            {analytics.totalTests > 0 ? Math.round((analytics.passed / analytics.totalTests) * 100) : 0}% of suite
          </div>
        </div>

        {/* 3. Failed */}
        <div className="p-3 rounded-xl bg-[#1c0c11] border border-rose-900/60 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-rose-400 uppercase font-semibold flex items-center justify-between">
            <span>Failed</span>
            <XCircle className="w-3.5 h-3.5 text-rose-400" />
          </div>
          <div className="text-xl font-bold text-rose-300 font-mono mt-0.5">{analytics.failed}</div>
          <div className="text-[10px] text-rose-400/80 mt-1 font-semibold">
            {analytics.failed > 0 ? `${analytics.failed} critical` : "0 failures"}
          </div>
        </div>

        {/* 4. Blocked / Skipped */}
        <div className="p-3 rounded-xl bg-[#141209] border border-amber-900/50 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-amber-400 uppercase font-semibold flex items-center justify-between">
            <span>Blocked/Skip</span>
            <AlertOctagon className="w-3.5 h-3.5 text-amber-400" />
          </div>
          <div className="text-xl font-bold text-amber-300 font-mono mt-0.5">{analytics.blocked + analytics.skipped}</div>
          <div className="text-[10px] text-amber-500/80 mt-1">Dependency gate</div>
        </div>

        {/* 5. Not Run */}
        <div className="p-3 rounded-xl bg-[#0e1422] border border-slate-800/90 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-slate-400 uppercase font-semibold flex items-center justify-between">
            <span>Not Run</span>
            <HelpCircle className="w-3.5 h-3.5 text-slate-500" />
          </div>
          <div className="text-xl font-bold text-slate-300 font-mono mt-0.5">{analytics.notRun}</div>
          <div className="text-[10px] text-slate-500 mt-1">Pending run</div>
        </div>

        {/* 6. Pass Rate */}
        <div className="p-3 rounded-xl bg-[#0e1422] border border-slate-800/90 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-slate-400 uppercase font-semibold flex items-center justify-between">
            <span>Pass Rate</span>
            <TrendingUp className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-xl font-bold text-emerald-400 font-mono mt-0.5">{analytics.passRate}%</div>
          <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden mt-1">
            <div
              className={`h-full rounded-full ${analytics.passRate >= 90 ? "bg-emerald-500" : analytics.passRate >= 75 ? "bg-amber-500" : "bg-rose-500"}`}
              style={{ width: `${Math.min(100, analytics.passRate)}%` }}
            />
          </div>
        </div>

        {/* 7. Failure Rate */}
        <div className="p-3 rounded-xl bg-[#0e1422] border border-slate-800/90 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-slate-400 uppercase font-semibold flex items-center justify-between">
            <span>Failure Rate</span>
            <Gauge className="w-3.5 h-3.5 text-rose-400" />
          </div>
          <div className="text-xl font-bold text-rose-400 font-mono mt-0.5">{analytics.failureRate}%</div>
          <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden mt-1">
            <div
              className="h-full bg-rose-500 rounded-full"
              style={{ width: `${Math.min(100, analytics.failureRate * 2)}%` }}
            />
          </div>
        </div>

        {/* 8. Avg Execution Time */}
        <div className="p-3 rounded-xl bg-[#0e1422] border border-slate-800/90 shadow-sm flex flex-col justify-between">
          <div className="text-[10px] text-slate-400 uppercase font-semibold flex items-center justify-between">
            <span>Avg Duration</span>
            <Clock className="w-3.5 h-3.5 text-cyan-400" />
          </div>
          <div className="text-xl font-bold text-cyan-300 font-mono mt-0.5">
            {analytics.avgExecutionTimeMs < 1000
              ? `${analytics.avgExecutionTimeMs}ms`
              : `${(analytics.avgExecutionTimeMs / 1000).toFixed(1)}s`}
          </div>
          <div className="text-[10px] text-cyan-500/80 mt-1">Per test case</div>
        </div>
      </div>
    </div>
  );
};
