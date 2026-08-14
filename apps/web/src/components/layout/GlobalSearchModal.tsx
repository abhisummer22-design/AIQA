import React, { useState, useEffect } from "react";
import { Search, X, Layers, Play, CheckSquare, Bug, FileCheck2, Cpu, ArrowRight } from "lucide-react";
import { ViewType } from "../../types";

interface GlobalSearchModalProps {
  isOpen: boolean;
  onClose: () => void;
  onNavigate: (view: ViewType) => void;
}

const searchableItems = [
  { id: "s1", title: "Auth & RBAC Security Fuzzing", category: "Execution Hub", view: "rbac-testing" as ViewType, sub: "RBAC Matrix & Privilege Boundary" },
  { id: "s2", title: "Payment Gateway Idempotency & Webhooks", category: "Execution Hub", view: "test-runs" as ViewType, sub: "Playwright / PyTest" },
  { id: "s3", title: "AI Test Case Generator & Gherkin Synthesizer", category: "Generator", view: "test-case-generator" as ViewType, sub: "OpenAPI, PRD & User Story to Playwright / PyTest" },
  { id: "s4", title: "Mobile Device Farm & Appium Automation", category: "Mobile", view: "mobile-testing" as ViewType, sub: "iOS & Android Simulators" },
  { id: "s5", title: "RBAC Privilege Escalation & BOLA / IDOR", category: "Security", view: "rbac-testing" as ViewType, sub: "Authorization Vulnerability Scan" },
  { id: "s6", title: "Gmail QA Alerts & Defect Reports", category: "Workspace", view: "workspace" as ViewType, sub: "Gmail API Integration" },
  { id: "s7", title: "Google Sheets Test Matrices & Execution Export", category: "Workspace", view: "workspace" as ViewType, sub: "Google Sheets & Drive Sync" },
  { id: "s8", title: "Google Calendar QA Release Gates & Schedules", category: "Workspace", view: "workspace" as ViewType, sub: "Google Calendar API Integration" },
  { id: "s9", title: "POST /v1/checkout/session — Concurrent race condition", category: "Failure", view: "test-runs" as ViewType, sub: "HTTP 500 Deadlock" },
  { id: "s10", title: "Payment API returns 500 during concurrent requests", category: "Bug Candidate", view: "bugs" as ViewType, sub: "AI Drafted P1" },
  { id: "s11", title: "Production Destructive API Protection", category: "Policy", view: "policies" as ViewType, sub: "Active Gate" }
];

export const GlobalSearchModal: React.FC<GlobalSearchModalProps> = ({
  isOpen,
  onClose,
  onNavigate
}) => {
  const [query, setQuery] = useState("");

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        if (isOpen) onClose();
      }
      if (e.key === "Escape" && isOpen) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const results = searchableItems.filter(
    (item) =>
      item.title.toLowerCase().includes(query.toLowerCase()) ||
      item.category.toLowerCase().includes(query.toLowerCase()) ||
      item.sub.toLowerCase().includes(query.toLowerCase())
  );

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-20 p-4 bg-black/75 backdrop-blur-sm">
      <div className="bg-[#0e1422] border border-slate-700/90 rounded-xl w-full max-w-xl overflow-hidden shadow-2xl animate-in zoom-in-95 duration-150">
        {/* Search Input */}
        <div className="p-4 border-b border-slate-800 flex items-center gap-3 bg-[#0b0f19]">
          <Search className="w-5 h-5 text-cyan-400 shrink-0" />
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search test runs, APIs, bugs, evidence, models, policies..."
            autoFocus
            className="bg-transparent text-sm text-slate-100 placeholder-slate-500 focus:outline-none w-full font-mono"
          />
          <kbd className="text-[10px] px-2 py-0.5 rounded bg-slate-800 text-slate-400 font-mono border border-slate-700">
            ESC
          </kbd>
        </div>

        {/* Results List */}
        <div className="max-h-80 overflow-y-auto p-2 divide-y divide-slate-800/60 font-mono">
          {results.length === 0 ? (
            <div className="p-6 text-center text-slate-500 text-xs">
              No results found for &ldquo;{query}&rdquo;
            </div>
          ) : (
            results.map((res) => (
              <button
                key={res.id}
                onClick={() => {
                  onClose();
                  onNavigate(res.view);
                }}
                className="w-full p-2.5 rounded-lg hover:bg-slate-800/70 text-left transition-colors flex items-center justify-between group"
              >
                <div className="min-w-0 flex-1 pr-3">
                  <div className="text-xs font-semibold text-slate-200 group-hover:text-cyan-300 transition-colors truncate">
                    {res.title}
                  </div>
                  <div className="text-[11px] text-slate-500 flex items-center gap-2 mt-0.5">
                    <span className="text-cyan-400 font-medium">{res.category}</span>
                    <span>&bull;</span>
                    <span className="truncate">{res.sub}</span>
                  </div>
                </div>
                <ArrowRight className="w-3.5 h-3.5 text-slate-600 group-hover:text-cyan-400 transition-colors shrink-0" />
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
