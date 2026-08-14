import React, { useState } from "react";
import { CheckSquare, Plus, Search, Filter, Play, ArrowUpRight, CheckCircle2, XCircle } from "lucide-react";

const testCasesList = [
  { id: "TC-101", title: "Verify JWT OAuth2 Token Refresh with Revocation", suite: "Auth & RBAC", type: "Security", status: "Active", passRate: "99.8%", lastDuration: "140ms" },
  { id: "TC-102", title: "Concurrent Checkout Race Condition Idempotency", suite: "Payments", type: "Fuzzing", status: "Failing", passRate: "91.2%", lastDuration: "420ms" },
  { id: "TC-103", title: "RAG Context Needle-in-Haystack Vector Retrieval", suite: "Search & AI", type: "AI Eval", status: "Active", passRate: "98.4%", lastDuration: "520ms" },
  { id: "TC-104", title: "Schemathesis OpenAPI Contract Validation v3", suite: "API Schema", type: "Contract", status: "Active", passRate: "100%", lastDuration: "90ms" },
  { id: "TC-105", title: "Adversarial Prompt Injection Boundary Checks", suite: "AI Guardrails", type: "Red-Teaming", status: "Active", passRate: "99.0%", lastDuration: "280ms" },
  { id: "TC-106", title: "Webhook Retry Exponential Backoff Compliance", suite: "Async Events", type: "Integration", status: "Active", passRate: "97.5%", lastDuration: "310ms" }
];

export const TestCasesView: React.FC = () => {
  const [search, setSearch] = useState("");
  const filtered = testCasesList.filter((tc) =>
    tc.title.toLowerCase().includes(search.toLowerCase()) ||
    tc.suite.toLowerCase().includes(search.toLowerCase()) ||
    tc.id.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white">Automated Test Repository</h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            4,892 test cases cataloged across API contract, AI evaluation, fuzzing, and end-to-end suites
          </p>
        </div>
        <button className="flex items-center gap-2 px-4 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-mono font-semibold shadow-sm">
          <Plus className="w-4 h-4" />
          <span>New Test Case</span>
        </button>
      </div>

      {/* Search & Filter bar */}
      <div className="p-3.5 rounded-xl bg-[#0e1422] border border-slate-800/90 flex items-center justify-between gap-3 text-xs font-mono">
        <div className="flex items-center gap-2.5 flex-1 max-w-md bg-slate-900 px-3 py-1.5 rounded-lg border border-slate-800">
          <Search className="w-4 h-4 text-slate-500" />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search test cases by title, ID, or suite..."
            className="bg-transparent text-slate-200 placeholder-slate-500 focus:outline-none w-full"
          />
        </div>
        <span className="text-slate-400">{filtered.length} of {testCasesList.length} shown</span>
      </div>

      {/* List */}
      <div className="bg-[#0e1422] border border-slate-800/90 rounded-xl overflow-hidden shadow-sm divide-y divide-slate-800/60 font-mono text-xs">
        {filtered.map((tc) => (
          <div key={tc.id} className="p-4 hover:bg-slate-900/40 transition-colors flex flex-col md:flex-row md:items-center justify-between gap-3">
            <div>
              <div className="flex items-center gap-2">
                <span className="text-cyan-400 font-bold">{tc.id}</span>
                <span className="font-bold text-slate-100">{tc.title}</span>
                <span
                  className={`text-[10px] px-1.5 py-0.2 rounded border font-semibold ${
                    tc.status === "Active"
                      ? "bg-emerald-950 text-emerald-300 border-emerald-800"
                      : "bg-rose-950 text-rose-300 border-rose-800"
                  }`}
                >
                  {tc.status}
                </span>
              </div>
              <div className="text-slate-400 text-[11px] mt-1 flex items-center gap-3">
                <span>Suite: {tc.suite}</span>
                <span>&bull;</span>
                <span>Type: {tc.type}</span>
                <span>&bull;</span>
                <span>Duration: {tc.lastDuration}</span>
              </div>
            </div>

            <div className="flex items-center gap-4 shrink-0">
              <div className="text-right">
                <div className="text-[10px] text-slate-500">Historical Pass Rate</div>
                <div className="font-bold text-emerald-400">{tc.passRate}</div>
              </div>
              <button className="p-2 rounded bg-slate-800 hover:bg-slate-700 text-cyan-400 transition-colors">
                <Play className="w-3.5 h-3.5 fill-current" />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
