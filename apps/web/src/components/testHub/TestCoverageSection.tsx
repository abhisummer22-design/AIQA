import React, { useState } from "react";
import { ShieldCheck, ChevronDown, ChevronUp, Sparkles, CheckCircle2, AlertCircle } from "lucide-react";
import { TestCoverageItem, TestType } from "../../types";

interface TestCoverageSectionProps {
  coverageList: TestCoverageItem[];
  activeCategory: TestType | "ALL";
  onSelectCategory: (cat: TestType) => void;
  onGenerateCategoryTests: (cat: TestType) => void;
}

export const TestCoverageSection: React.FC<TestCoverageSectionProps> = ({
  coverageList,
  activeCategory,
  onSelectCategory,
  onGenerateCategoryTests
}) => {
  const [isExpanded, setIsExpanded] = useState<boolean>(true);

  const avgCoverage =
    coverageList.length > 0
      ? Math.round(coverageList.reduce((acc, c) => acc + c.percentage, 0) / coverageList.length)
      : 0;

  const belowTargetCount = coverageList.filter((c) => c.percentage < c.targetPercentage).length;

  return (
    <div className="bg-[#0e1422] border border-slate-800/90 rounded-xl p-4 shadow-sm space-y-3 font-mono">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <div className="p-1.5 rounded-lg bg-cyan-950/60 border border-cyan-800 text-cyan-400">
            <ShieldCheck className="w-4 h-4" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-bold text-slate-100">Test Category Coverage Matrix</h2>
              <span className="text-[10px] px-2 py-0.5 rounded font-semibold bg-emerald-950 text-emerald-300 border border-emerald-800">
                {avgCoverage}% Overall Average
              </span>
              {belowTargetCount > 0 && (
                <span className="text-[10px] px-2 py-0.5 rounded font-semibold bg-amber-950 text-amber-300 border border-amber-800 flex items-center gap-1">
                  <AlertCircle className="w-3 h-3" />
                  {belowTargetCount} Below SLA
                </span>
              )}
            </div>
            <p className="text-[11px] text-slate-400 mt-0.5">
              Requirements traceability & coverage analysis across all 10 specialized QA testing methodologies
            </p>
          </div>
        </div>

        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className="flex items-center gap-1 text-xs text-slate-400 hover:text-slate-200 px-2 py-1 rounded bg-slate-900 border border-slate-800"
        >
          <span>{isExpanded ? "Collapse Matrix" : "Expand Matrix"}</span>
          {isExpanded ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
        </button>
      </div>

      {/* Coverage Grid */}
      {isExpanded && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-5 gap-3 pt-2 border-t border-slate-800/70">
          {coverageList.map((item) => {
            const isMet = item.percentage >= item.targetPercentage;
            const isSelected = activeCategory === item.test_type;

            return (
              <div
                key={item.test_type}
                className={`p-3 rounded-lg border transition-all duration-150 relative group ${
                  isSelected
                    ? "bg-slate-900/90 border-cyan-500 ring-1 ring-cyan-500/50"
                    : "bg-[#080d16] border-slate-800/80 hover:border-slate-700"
                }`}
              >
                <div className="flex items-center justify-between text-xs font-semibold">
                  <button
                    onClick={() => onSelectCategory(item.test_type)}
                    className="text-left font-bold text-slate-200 hover:text-cyan-400 truncate max-w-[130px]"
                  >
                    {item.displayName}
                  </button>
                  <span
                    className={`text-[11px] font-bold ${
                      isMet ? "text-emerald-400" : item.percentage >= 50 ? "text-amber-400" : "text-rose-400"
                    }`}
                  >
                    {item.percentage}%
                  </span>
                </div>

                {/* Progress bar */}
                <div className="mt-2 w-full bg-slate-800/90 h-1.5 rounded-full overflow-hidden relative">
                  <div
                    className={`h-full rounded-full transition-all duration-300 ${
                      isMet ? "bg-emerald-500" : item.percentage >= 50 ? "bg-amber-500" : "bg-rose-500"
                    }`}
                    style={{ width: `${Math.min(100, item.percentage)}%` }}
                  />
                  {/* Target SLA marker */}
                  <div
                    className="absolute top-0 bottom-0 w-0.5 bg-white/70"
                    style={{ left: `${item.targetPercentage}%` }}
                    title={`Target SLA: ${item.targetPercentage}%`}
                  />
                </div>

                <div className="flex items-center justify-between text-[10px] text-slate-400 mt-2">
                  <span>
                    {item.totalTests} tests ({item.passedTests} passed)
                  </span>
                  <span className="text-slate-500">Target: {item.targetPercentage}%</span>
                </div>

                {!isMet && (
                  <button
                    onClick={() => onGenerateCategoryTests(item.test_type)}
                    className="mt-2 w-full flex items-center justify-center gap-1.5 py-1 px-2 rounded bg-cyan-950/40 hover:bg-cyan-900/60 border border-cyan-800 text-cyan-300 text-[10px] font-semibold transition-colors"
                  >
                    <Sparkles className="w-3 h-3 text-cyan-400" />
                    <span>Generate AI Tests</span>
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
