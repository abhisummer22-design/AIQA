import React from "react";
import {
  Play,
  CheckCircle2,
  XCircle,
  Clock,
  AlertOctagon,
  HelpCircle,
  ArrowRight,
  Shield,
  Tag,
  User,
  Activity,
  Layers,
  Terminal
} from "lucide-react";
import { TestCase, TestType } from "../../types";

interface TestCaseCardProps {
  testCase: TestCase;
  isSelected: boolean;
  onToggleSelect: (id: string) => void;
  onRunSingle: (testCase: TestCase) => void;
  onViewDetails: (testCase: TestCase) => void;
  isRunning?: boolean;
}

export const TestCaseCard: React.FC<TestCaseCardProps> = ({
  testCase,
  isSelected,
  onToggleSelect,
  onRunSingle,
  onViewDetails,
  isRunning = false
}) => {
  // Category Badge Styles
  const getCategoryBadge = (type: TestType) => {
    switch (type) {
      case "FUNCTIONAL":
        return "bg-sky-950/70 text-sky-300 border-sky-800";
      case "NON_FUNCTIONAL":
        return "bg-indigo-950/70 text-indigo-300 border-indigo-800";
      case "UI":
        return "bg-purple-950/70 text-purple-300 border-purple-800";
      case "PERFORMANCE":
        return "bg-amber-950/70 text-amber-300 border-amber-800";
      case "SECURITY":
        return "bg-rose-950/70 text-rose-300 border-rose-800";
      case "DATABASE":
        return "bg-emerald-950/70 text-emerald-300 border-emerald-800";
      case "EDGE_CASE":
        return "bg-fuchsia-950/70 text-fuchsia-300 border-fuchsia-800";
      case "SCENARIO_BASED":
        return "bg-yellow-950/70 text-yellow-300 border-yellow-800";
      case "BUSINESS_CASE":
        return "bg-lime-950/70 text-lime-300 border-lime-800";
      case "UX":
      default:
        return "bg-teal-950/70 text-teal-300 border-teal-800";
    }
  };

  // Status Badge Styles
  const getStatusBadge = () => {
    switch (testCase.status) {
      case "Passed":
        return {
          icon: <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />,
          style: "bg-emerald-950/80 text-emerald-300 border-emerald-800",
          text: "PASSED"
        };
      case "Failed":
        return {
          icon: <XCircle className="w-3.5 h-3.5 text-rose-400" />,
          style: "bg-rose-950/80 text-rose-300 border-rose-800",
          text: "FAILED"
        };
      case "Running":
      case "Queued":
        return {
          icon: <Activity className="w-3.5 h-3.5 text-cyan-400 animate-spin" />,
          style: "bg-cyan-950/80 text-cyan-300 border-cyan-800",
          text: "RUNNING"
        };
      case "Blocked":
        return {
          icon: <AlertOctagon className="w-3.5 h-3.5 text-amber-400" />,
          style: "bg-amber-950/80 text-amber-300 border-amber-800",
          text: "BLOCKED"
        };
      case "Skipped":
        return {
          icon: <HelpCircle className="w-3.5 h-3.5 text-slate-400" />,
          style: "bg-slate-900 text-slate-400 border-slate-700",
          text: "SKIPPED"
        };
      case "Not Run":
      default:
        return {
          icon: <HelpCircle className="w-3.5 h-3.5 text-slate-500" />,
          style: "bg-slate-900/60 text-slate-400 border-slate-800",
          text: "NOT RUN"
        };
    }
  };

  // Priority Styles
  const getPriorityStyle = (priority: string) => {
    switch (priority) {
      case "Critical":
        return "bg-rose-950/60 text-rose-400 border-rose-800";
      case "High":
        return "bg-amber-950/60 text-amber-400 border-amber-800";
      case "Medium":
        return "bg-blue-950/60 text-blue-400 border-blue-800";
      case "Low":
      default:
        return "bg-slate-900 text-slate-400 border-slate-800";
    }
  };

  const statusInfo = getStatusBadge();

  return (
    <div
      id={`test-case-card-${testCase.id.toLowerCase()}`}
      className={`p-4 rounded-xl border transition-all duration-150 font-mono space-y-3 ${
        isSelected
          ? "bg-[#0f172a] border-cyan-500 shadow-md ring-1 ring-cyan-500/50"
          : "bg-[#0e1422] border-slate-800/90 hover:border-slate-700 hover:bg-[#11192a]"
      }`}
    >
      {/* Top Header Row */}
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3 min-w-0">
          {/* Checkbox */}
          <input
            type="checkbox"
            checked={isSelected}
            onChange={() => onToggleSelect(testCase.id)}
            className="mt-1 w-4 h-4 rounded border-slate-700 bg-slate-900 text-cyan-600 focus:ring-0 focus:ring-offset-0 cursor-pointer"
          />

          {/* Test ID & Title */}
          <div className="space-y-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="font-bold text-xs text-cyan-400 bg-cyan-950/60 px-1.5 py-0.5 rounded border border-cyan-800/80">
                {testCase.id}
              </span>

              {/* Category Badge */}
              <span
                className={`text-[10px] font-bold px-2 py-0.5 rounded uppercase border ${getCategoryBadge(
                  testCase.test_type
                )}`}
              >
                [{testCase.test_type.replace("_", " ")}]
              </span>

              {/* Priority */}
              <span
                className={`text-[10px] font-semibold px-1.5 py-0.2 rounded border ${getPriorityStyle(
                  testCase.priority
                )}`}
              >
                {testCase.priority}
              </span>

              {/* Severity */}
              {testCase.severity && (
                <span className="text-[10px] px-1.5 py-0.2 rounded bg-slate-900 border border-slate-800 text-slate-400">
                  {testCase.severity}
                </span>
              )}
            </div>

            {/* Title */}
            <h3
              onClick={() => onViewDetails(testCase)}
              className="text-sm font-bold text-slate-100 hover:text-cyan-300 cursor-pointer leading-snug transition-colors line-clamp-2"
            >
              {testCase.name}
            </h3>
          </div>
        </div>

        {/* Status Badge & Actions */}
        <div className="flex items-center gap-2 shrink-0">
          <div
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-bold border shadow-sm ${statusInfo.style}`}
          >
            {statusInfo.icon}
            <span>{statusInfo.text}</span>
          </div>

          {/* Quick Run Button */}
          <button
            onClick={() => onRunSingle(testCase)}
            disabled={isRunning}
            title="Execute this test case now"
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white text-xs font-bold shadow-sm transition-all"
          >
            <Play className="w-3.5 h-3.5 fill-current" />
            <span className="hidden sm:inline">Run</span>
          </button>

          {/* View Drawer Button */}
          <button
            onClick={() => onViewDetails(testCase)}
            title="Open full execution diagnostics drawer"
            className="p-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
          >
            <ArrowRight className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Description Snippet */}
      <p className="text-xs text-slate-400 leading-relaxed line-clamp-2">
        {testCase.description}
      </p>

      {/* Failure Diagnostic Alert if Failed */}
      {testCase.status === "Failed" && testCase.last_execution?.failure_reason && (
        <div className="p-2.5 rounded-lg bg-rose-950/40 border border-rose-900/60 text-xs text-rose-300 space-y-1">
          <div className="flex items-center gap-1.5 font-bold text-rose-400 text-[11px] uppercase">
            <XCircle className="w-3.5 h-3.5" />
            <span>Failure Diagnostics:</span>
          </div>
          <p className="text-[11px] text-rose-300 leading-relaxed font-mono">
            {testCase.last_execution.failure_reason}
          </p>
        </div>
      )}

      {/* Performance Latency Badge if Available */}
      {testCase.test_type === "PERFORMANCE" && testCase.last_execution?.metrics && (
        <div className="p-2 rounded-lg bg-amber-950/30 border border-amber-900/50 text-[11px] text-amber-300 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span>P95 Latency:</span>
            <strong className="text-amber-200">
              {testCase.last_execution.metrics.p95_latency_ms}ms
            </strong>
          </div>
          <div className="flex items-center gap-2">
            <span>Throughput:</span>
            <strong className="text-amber-200">
              {testCase.last_execution.metrics.throughput_rps} rps
            </strong>
          </div>
        </div>
      )}

      {/* Bottom Metadata Bar */}
      <div className="pt-2 border-t border-slate-800/80 flex flex-wrap items-center justify-between gap-2 text-xs text-slate-400">
        <div className="flex items-center gap-3 flex-wrap">
          {/* Environment */}
          <span className="flex items-center gap-1 text-[11px] px-1.5 py-0.2 rounded bg-slate-900 border border-slate-800 text-slate-300">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
            {testCase.environment}
          </span>

          {/* Project */}
          {testCase.project_name && (
            <span className="flex items-center gap-1 text-[11px] text-slate-300">
              <Layers className="w-3.5 h-3.5 text-slate-500" />
              {testCase.project_name}
            </span>
          )}

          {/* Owner */}
          <span className="flex items-center gap-1 text-[11px] text-slate-400">
            <User className="w-3.5 h-3.5 text-slate-500" />
            {testCase.owner_name}
          </span>

          {/* Execution Time */}
          {testCase.last_execution && (
            <span className="flex items-center gap-1 text-[11px] text-slate-400">
              <Clock className="w-3.5 h-3.5 text-slate-500" />
              <span>
                {testCase.last_execution.duration_ms}ms duration
              </span>
            </span>
          )}
        </div>

        {/* Tags */}
        <div className="flex items-center gap-1 flex-wrap">
          {testCase.tags.slice(0, 3).map((tag, idx) => (
            <span
              key={idx}
              className="text-[10px] px-1.5 py-0.2 rounded bg-slate-900/80 text-slate-400 border border-slate-800"
            >
              #{tag}
            </span>
          ))}
          {testCase.tags.length > 3 && (
            <span className="text-[10px] text-slate-500">+{testCase.tags.length - 3}</span>
          )}
        </div>
      </div>
    </div>
  );
};
