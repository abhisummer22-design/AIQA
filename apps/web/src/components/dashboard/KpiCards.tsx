import React from "react";
import {
  CheckCircle2,
  PlayCircle,
  TrendingUp,
  TrendingDown,
  ShieldAlert,
  Flame,
  Bug,
  ArrowUpRight,
  Sparkles,
  Zap
} from "lucide-react";
import { KpiMetrics, ViewType } from "../../types";

interface KpiCardsProps {
  metrics: KpiMetrics;
  onNavigate: (view: ViewType) => void;
}

export const KpiCards: React.FC<KpiCardsProps> = ({ metrics, onNavigate }) => {
  return (
    <div id="kpi-cards-grid" className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6 gap-2.5">
      {/* 1. Total Tests */}
      <div
        id="kpi-card-total-tests"
        onClick={() => onNavigate("test-cases")}
        role="button"
        tabIndex={0}
        className="group relative bg-[#0D0D0E] hover:bg-[#111113] border border-slate-800 hover:border-indigo-500/40 rounded p-3 transition-all duration-150 cursor-pointer flex flex-col justify-between"
      >
        <div className="flex items-center justify-between text-slate-400 mb-1">
          <span className="text-[10px] opacity-50 uppercase font-bold tracking-wider text-slate-400">Total Tests</span>
          <ArrowUpRight className="w-3 h-3 text-slate-600 group-hover:text-indigo-400 transition-colors" />
        </div>
        <div>
          <div className="text-xl font-bold font-mono text-white tracking-tight">
            {metrics.totalTests.total.toLocaleString()}
          </div>
          <div className="mt-1.5 flex items-center justify-between text-[10px] text-slate-400 border-t border-slate-800/80 pt-1.5 font-mono">
            <span>Today: <strong className="text-slate-200 font-semibold">{metrics.totalTests.executedToday}</strong></span>
            <span className="text-emerald-400 font-semibold">{metrics.totalTests.passPercentage}% Pass</span>
          </div>
        </div>
      </div>

      {/* 2. Test Runs */}
      <div
        id="kpi-card-test-runs"
        onClick={() => onNavigate("test-runs")}
        role="button"
        tabIndex={0}
        className="group relative bg-[#0D0D0E] hover:bg-[#111113] border border-slate-800 hover:border-indigo-500/40 rounded p-3 transition-all duration-150 cursor-pointer flex flex-col justify-between"
      >
        <div className="flex items-center justify-between text-slate-400 mb-1">
          <span className="text-[10px] opacity-50 uppercase font-bold tracking-wider text-slate-400">Test Case Execution Hub</span>
          <ArrowUpRight className="w-3 h-3 text-slate-600 group-hover:text-indigo-400 transition-colors" />
        </div>
        <div>
          <div className="flex items-baseline gap-2">
            <span className="text-xl font-bold font-mono text-white tracking-tight">
              {metrics.testRuns.running + metrics.testRuns.completed + metrics.testRuns.failed}
            </span>
            <span className="inline-flex items-center gap-1 text-[9px] font-mono text-indigo-400 bg-indigo-950/70 border border-indigo-800/60 px-1 py-0.2 rounded font-semibold">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
              {metrics.testRuns.running} Running
            </span>
          </div>
          <div className="mt-1.5 flex items-center justify-between text-[10px] text-slate-400 border-t border-slate-800/80 pt-1.5 font-mono">
            <span className="text-emerald-400">{metrics.testRuns.completed} Passed</span>
            <span className="text-rose-400">{metrics.testRuns.failed} Failed</span>
          </div>
        </div>
      </div>

      {/* 3. Quality Score */}
      <div
        id="kpi-card-quality-score"
        onClick={() => onNavigate("quality-intelligence")}
        role="button"
        tabIndex={0}
        className="group relative bg-[#0D0D0E] hover:bg-[#111113] border border-slate-800 hover:border-indigo-500/40 rounded p-3 transition-all duration-150 cursor-pointer flex flex-col justify-between"
      >
        <div className="flex items-center justify-between text-slate-400 mb-1">
          <span className="text-[10px] opacity-50 uppercase font-bold tracking-wider text-slate-400">Quality Score</span>
          <ArrowUpRight className="w-3 h-3 text-slate-600 group-hover:text-indigo-400 transition-colors" />
        </div>
        <div>
          <div className="flex items-baseline gap-1.5">
            <span className="text-xl font-bold font-mono text-indigo-300 tracking-tight">
              {metrics.qualityScore.score}%
            </span>
            <span className="inline-flex items-center text-[10px] font-mono font-semibold text-emerald-400">
              <TrendingUp className="w-2.5 h-2.5 mr-0.5" />
              {metrics.qualityScore.trend}
            </span>
          </div>
          <div className="mt-1.5 text-[10px] text-slate-400 border-t border-slate-800/80 pt-1.5 flex items-center justify-between font-mono">
            <span>Target: &gt;85.0%</span>
            <span className="text-indigo-400 font-medium">Healthy SLA</span>
          </div>
        </div>
      </div>

      {/* 4. Risk Score */}
      <div
        id="kpi-card-risk-score"
        onClick={() => onNavigate("risk-flakiness")}
        role="button"
        tabIndex={0}
        className="group relative bg-[#0D0D0E] hover:bg-[#111113] border border-slate-800 hover:border-amber-500/40 rounded p-3 transition-all duration-150 cursor-pointer flex flex-col justify-between"
      >
        <div className="flex items-center justify-between text-slate-400 mb-1">
          <span className="text-[10px] opacity-50 uppercase font-bold tracking-wider text-slate-400">Risk Score</span>
          <ArrowUpRight className="w-3 h-3 text-slate-600 group-hover:text-amber-400 transition-colors" />
        </div>
        <div>
          <div className="flex items-center justify-between">
            <span className="text-xl font-bold font-mono text-amber-300 tracking-tight">
              {metrics.riskScore.level}
            </span>
            <span className="text-[10px] font-mono text-slate-400">{metrics.riskScore.value}/100</span>
          </div>
          <div className="mt-1.5 w-full bg-slate-800 h-1 rounded-full overflow-hidden">
            <div
              className="bg-amber-400 h-full rounded-full transition-all duration-500"
              style={{ width: `${metrics.riskScore.value}%` }}
            />
          </div>
          <div className="mt-1.5 flex items-center justify-between text-[9px] text-slate-400 border-t border-slate-800/80 pt-1.5 font-mono">
            <span className="text-rose-400 font-bold">{metrics.riskScore.criticalCount} Crit</span>
            <span className="text-amber-400">{metrics.riskScore.highCount} High</span>
            <span className="text-slate-400">{metrics.riskScore.mediumCount} Med</span>
          </div>
        </div>
      </div>

      {/* 5. Flakiness */}
      <div
        id="kpi-card-flakiness"
        onClick={() => onNavigate("risk-flakiness")}
        role="button"
        tabIndex={0}
        className="group relative bg-[#0D0D0E] hover:bg-[#111113] border border-slate-800 hover:border-indigo-500/40 rounded p-3 transition-all duration-150 cursor-pointer flex flex-col justify-between"
      >
        <div className="flex items-center justify-between text-slate-400 mb-1">
          <span className="text-[10px] opacity-50 uppercase font-bold tracking-wider text-slate-400">Flakiness</span>
          <ArrowUpRight className="w-3 h-3 text-slate-600 group-hover:text-indigo-400 transition-colors" />
        </div>
        <div>
          <div className="flex items-baseline gap-1.5">
            <span className="text-xl font-bold font-mono text-white tracking-tight">
              {metrics.flakiness.percentage}%
            </span>
            <span className="inline-flex items-center text-[10px] font-mono font-semibold text-emerald-400">
              <TrendingDown className="w-2.5 h-2.5 mr-0.5" />
              {metrics.flakiness.trend}
            </span>
          </div>
          <div className="mt-1.5 flex items-center justify-between text-[10px] text-slate-400 border-t border-slate-800/80 pt-1.5 font-mono">
            <span>Flaky: <strong className="text-slate-200 font-semibold">{metrics.flakiness.flakyTestCount}</strong></span>
            <span className="text-amber-400 font-medium">1 Quarantined</span>
          </div>
        </div>
      </div>

      {/* 6. Open Bugs */}
      <div
        id="kpi-card-open-bugs"
        onClick={() => onNavigate("bugs")}
        role="button"
        tabIndex={0}
        className="group relative bg-[#0D0D0E] hover:bg-[#111113] border border-slate-800 hover:border-rose-500/40 rounded p-3 transition-all duration-150 cursor-pointer flex flex-col justify-between"
      >
        <div className="flex items-center justify-between text-slate-400 mb-1">
          <span className="text-[10px] opacity-50 uppercase font-bold tracking-wider text-slate-400">Open Bugs</span>
          <ArrowUpRight className="w-3 h-3 text-slate-600 group-hover:text-rose-400 transition-colors" />
        </div>
        <div>
          <div className="flex items-baseline gap-1.5">
            <span className="text-xl font-bold font-mono text-rose-300 tracking-tight">
              {metrics.openBugs.total}
            </span>
            <span className="text-[10px] text-rose-400 font-mono font-semibold">
              {metrics.openBugs.critical} Critical
            </span>
          </div>
          <div className="mt-1.5 flex items-center justify-between text-[9px] text-slate-400 border-t border-slate-800/80 pt-1.5 font-mono">
            <span className="text-amber-400">{metrics.openBugs.high} High</span>
            <span className="text-slate-400">{metrics.openBugs.medium} Med</span>
            <span className="text-slate-500">{metrics.openBugs.low} Low</span>
          </div>
        </div>
      </div>
    </div>
  );
};

