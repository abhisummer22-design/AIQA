import React from "react";
import {
  Layers,
  CheckCircle2,
  Cpu,
  Shield,
  Database,
  AlertTriangle,
  GitBranch,
  Briefcase,
  Eye,
  Sliders,
  Sparkles
} from "lucide-react";
import { TestType } from "../../types";

export interface CategoryNavTab {
  id: TestType | "ALL";
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  colorClass: string;
  activeBorderClass: string;
  activeBgClass: string;
  badgeClass: string;
}

export const CATEGORY_TABS: CategoryNavTab[] = [
  {
    id: "ALL",
    label: "ALL",
    icon: Layers,
    colorClass: "text-slate-400 hover:text-slate-200",
    activeBorderClass: "border-cyan-500",
    activeBgClass: "bg-cyan-950/40 text-cyan-300",
    badgeClass: "bg-slate-800 text-slate-300 border-slate-700"
  },
  {
    id: "FUNCTIONAL",
    label: "FUNCTIONAL",
    icon: CheckCircle2,
    colorClass: "text-sky-400 hover:text-sky-300",
    activeBorderClass: "border-sky-500",
    activeBgClass: "bg-sky-950/50 text-sky-300",
    badgeClass: "bg-sky-950/70 text-sky-300 border-sky-800"
  },
  {
    id: "NON_FUNCTIONAL",
    label: "NON-FUNCTIONAL",
    icon: Cpu,
    colorClass: "text-indigo-400 hover:text-indigo-300",
    activeBorderClass: "border-indigo-500",
    activeBgClass: "bg-indigo-950/50 text-indigo-300",
    badgeClass: "bg-indigo-950/70 text-indigo-300 border-indigo-800"
  },
  {
    id: "UI",
    label: "UI",
    icon: Eye,
    colorClass: "text-purple-400 hover:text-purple-300",
    activeBorderClass: "border-purple-500",
    activeBgClass: "bg-purple-950/50 text-purple-300",
    badgeClass: "bg-purple-950/70 text-purple-300 border-purple-800"
  },
  {
    id: "PERFORMANCE",
    label: "PERFORMANCE",
    icon: Sliders,
    colorClass: "text-amber-400 hover:text-amber-300",
    activeBorderClass: "border-amber-500",
    activeBgClass: "bg-amber-950/50 text-amber-300",
    badgeClass: "bg-amber-950/70 text-amber-300 border-amber-800"
  },
  {
    id: "SECURITY",
    label: "SECURITY",
    icon: Shield,
    colorClass: "text-rose-400 hover:text-rose-300",
    activeBorderClass: "border-rose-500",
    activeBgClass: "bg-rose-950/50 text-rose-300",
    badgeClass: "bg-rose-950/70 text-rose-300 border-rose-800"
  },
  {
    id: "DATABASE",
    label: "DATABASE",
    icon: Database,
    colorClass: "text-emerald-400 hover:text-emerald-300",
    activeBorderClass: "border-emerald-500",
    activeBgClass: "bg-emerald-950/50 text-emerald-300",
    badgeClass: "bg-emerald-950/70 text-emerald-300 border-emerald-800"
  },
  {
    id: "EDGE_CASE",
    label: "EDGE CASE",
    icon: AlertTriangle,
    colorClass: "text-fuchsia-400 hover:text-fuchsia-300",
    activeBorderClass: "border-fuchsia-500",
    activeBgClass: "bg-fuchsia-950/50 text-fuchsia-300",
    badgeClass: "bg-fuchsia-950/70 text-fuchsia-300 border-fuchsia-800"
  },
  {
    id: "SCENARIO_BASED",
    label: "SCENARIO-BASED",
    icon: GitBranch,
    colorClass: "text-yellow-400 hover:text-yellow-300",
    activeBorderClass: "border-yellow-500",
    activeBgClass: "bg-yellow-950/50 text-yellow-300",
    badgeClass: "bg-yellow-950/70 text-yellow-300 border-yellow-800"
  },
  {
    id: "BUSINESS_CASE",
    label: "BUSINESS CASE",
    icon: Briefcase,
    colorClass: "text-lime-400 hover:text-lime-300",
    activeBorderClass: "border-lime-500",
    activeBgClass: "bg-lime-950/50 text-lime-300",
    badgeClass: "bg-lime-950/70 text-lime-300 border-lime-800"
  },
  {
    id: "UX",
    label: "UX",
    icon: Sparkles,
    colorClass: "text-teal-400 hover:text-teal-300",
    activeBorderClass: "border-teal-500",
    activeBgClass: "bg-teal-950/50 text-teal-300",
    badgeClass: "bg-teal-950/70 text-teal-300 border-teal-800"
  }
];

export const TEST_TYPE_LABELS: Record<TestType, string> = {
  FUNCTIONAL: "Functional Testing",
  NON_FUNCTIONAL: "Non-Functional Testing",
  UI: "UI Testing",
  PERFORMANCE: "Performance Testing",
  SECURITY: "Security Testing",
  DATABASE: "Database Testing",
  EDGE_CASE: "Edge Case Testing",
  SCENARIO_BASED: "Scenario-Based Testing",
  BUSINESS_CASE: "Business Case Testing",
  UX: "UX Testing"
};

interface TestCategoryNavProps {
  activeCategory: TestType | "ALL";
  onSelectCategory: (category: TestType | "ALL") => void;
  counts: Record<string, number>;
}

export const TestCategoryNav: React.FC<TestCategoryNavProps> = ({
  activeCategory,
  onSelectCategory,
  counts
}) => {
  return (
    <div className="w-full bg-[#080d16] border border-slate-800/90 rounded-xl p-1.5 shadow-sm">
      <div className="flex items-center gap-1.5 overflow-x-auto no-scrollbar scroll-smooth py-0.5 px-0.5">
        {CATEGORY_TABS.map((tab) => {
          const Icon = tab.icon;
          const isActive = activeCategory === tab.id;
          const count = tab.id === "ALL" ? counts.ALL ?? 0 : counts[tab.id] ?? 0;

          return (
            <button
              key={tab.id}
              id={`nav-tab-${tab.id.toLowerCase()}`}
              onClick={() => onSelectCategory(tab.id)}
              className={`flex items-center gap-2 px-3 py-2 rounded-lg text-xs font-mono font-bold whitespace-nowrap transition-all duration-150 shrink-0 border ${
                isActive
                  ? `${tab.activeBgClass} ${tab.activeBorderClass} shadow-sm ring-1 ring-white/10`
                  : "bg-slate-900/60 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-800/60"
              }`}
            >
              <Icon className={`w-3.5 h-3.5 ${isActive ? "text-current" : "text-slate-500"}`} />
              <span className="tracking-wide uppercase text-[11px]">{tab.label}</span>
              <span
                className={`text-[10px] px-1.5 py-0.2 rounded-full font-mono font-semibold border ${
                  isActive
                    ? "bg-black/30 border-white/20 text-white"
                    : "bg-slate-800/80 border-slate-700/60 text-slate-400"
                }`}
              >
                {count}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
};
