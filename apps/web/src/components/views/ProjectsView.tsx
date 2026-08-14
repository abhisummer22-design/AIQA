import React from "react";
import { FolderGit2, Plus, ArrowUpRight, CheckCircle2, AlertTriangle, ShieldCheck, Cpu } from "lucide-react";
import { ViewType } from "../../types";

interface ProjectsViewProps {
  onNavigate: (view: ViewType) => void;
}

const projects = [
  {
    id: "p1",
    name: "Apex AI Agent",
    repo: "github.com/enterprise/apex-agent",
    tests: 1842,
    passRate: 97.4,
    status: "Healthy",
    lastRun: "2 mins ago",
    env: "Staging / Prod",
    model: "Gemma 3 12B / Gemini"
  },
  {
    id: "p2",
    name: "Payment Core API",
    repo: "github.com/enterprise/payment-service",
    tests: 1240,
    passRate: 91.2,
    status: "Attention",
    lastRun: "12 mins ago",
    env: "Staging",
    model: "Contract / Schemathesis"
  },
  {
    id: "p3",
    name: "Search RAG Pipeline",
    repo: "github.com/enterprise/rag-search-engine",
    tests: 980,
    passRate: 98.8,
    status: "Healthy",
    lastRun: "1 hour ago",
    env: "Prod",
    model: "DeepEval / LLM Judge"
  },
  {
    id: "p4",
    name: "Omni LLM Gateway",
    repo: "github.com/enterprise/omni-gateway",
    tests: 830,
    passRate: 99.1,
    status: "Healthy",
    lastRun: "3 hours ago",
    env: "Prod",
    model: "Multi-Model Router"
  }
];

export const ProjectsView: React.FC<ProjectsViewProps> = ({ onNavigate }) => {
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white">Project Workspaces</h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            Manage test suites, CI/CD pipeline integration, and quality gates by repository
          </p>
        </div>
        <button className="flex items-center gap-2 px-4 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-mono font-semibold shadow-sm">
          <Plus className="w-4 h-4" />
          <span>Register New Project</span>
        </button>
      </div>

      {/* Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {projects.map((proj) => (
          <div
            key={proj.id}
            className="p-5 rounded-xl bg-[#0e1422] border border-slate-800/90 hover:border-cyan-500/40 transition-all shadow-sm space-y-4"
          >
            <div className="flex items-start justify-between">
              <div>
                <div className="flex items-center gap-2">
                  <FolderGit2 className="w-4 h-4 text-cyan-400" />
                  <h3 className="font-bold text-sm text-slate-100">{proj.name}</h3>
                </div>
                <div className="text-xs font-mono text-slate-500 mt-1">{proj.repo}</div>
              </div>
              <span
                className={`text-[10px] font-mono px-2 py-0.5 rounded font-semibold ${
                  proj.status === "Healthy"
                    ? "bg-emerald-950 text-emerald-300 border border-emerald-800"
                    : "bg-amber-950 text-amber-300 border border-amber-800"
                }`}
              >
                {proj.status}
              </span>
            </div>

            <div className="grid grid-cols-3 gap-2 text-center text-xs font-mono py-2 border-y border-slate-800/80">
              <div>
                <div className="text-[10px] text-slate-500">Test Cases</div>
                <div className="font-bold text-slate-200 mt-0.5">{proj.tests}</div>
              </div>
              <div>
                <div className="text-[10px] text-slate-500">Pass Rate</div>
                <div className="font-bold text-emerald-400 mt-0.5">{proj.passRate}%</div>
              </div>
              <div>
                <div className="text-[10px] text-slate-500">Last Run</div>
                <div className="font-bold text-slate-400 mt-0.5">{proj.lastRun}</div>
              </div>
            </div>

            <div className="flex items-center justify-between text-xs font-mono">
              <span className="text-slate-400 text-[11px]">Primary: {proj.model}</span>
              <button
                onClick={() => onNavigate("test-runs")}
                className="text-cyan-400 hover:underline flex items-center gap-1"
              >
                <span>View Runs</span>
                <ArrowUpRight className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
