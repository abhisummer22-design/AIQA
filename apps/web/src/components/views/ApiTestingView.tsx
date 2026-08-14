import React from "react";
import { Terminal, ShieldAlert, CheckCircle2, Play, Cpu, FileCode, Layers } from "lucide-react";

const apiEndpoints = [
  { path: "POST /v1/auth/tokens", method: "POST", schema: "OpenAPI 3.1", status: "Compliant", latency: "140ms", fuzzed: "1,200 mutations" },
  { path: "POST /v1/checkout/session", method: "POST", schema: "OpenAPI 3.1", status: "Failing Fuzz", latency: "420ms", fuzzed: "4,500 mutations" },
  { path: "GET /v1/users/{id}/profile", method: "GET", schema: "OpenAPI 3.1", status: "Compliant", latency: "95ms", fuzzed: "800 mutations" },
  { path: "POST /v1/search/rag-query", method: "POST", schema: "OpenAPI 3.1", status: "Compliant", latency: "380ms", fuzzed: "2,100 mutations" },
  { path: "DELETE /v1/webhooks/{id}", method: "DELETE", schema: "OpenAPI 3.1", status: "Compliant", latency: "110ms", fuzzed: "600 mutations" }
];

export const ApiTestingView: React.FC = () => {
  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold font-mono text-white flex items-center gap-2.5">
            <Terminal className="w-5 h-5 text-cyan-400" />
            API Contract &amp; Autonomous Fuzzing Hub
          </h1>
          <p className="text-xs text-slate-400 font-mono mt-1">
            Property-based contract validation, Schemathesis fuzzing runs, and stateful API invariants
          </p>
        </div>
        <button className="flex items-center gap-2 px-4 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-mono font-semibold shadow-sm">
          <Play className="w-4 h-4 fill-current" />
          <span>Execute Full Fuzz Cycle</span>
        </button>
      </div>

      <div className="bg-[#0e1422] border border-slate-800/90 rounded-xl overflow-hidden shadow-sm">
        <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
          <span className="text-xs font-bold font-mono uppercase text-slate-200">Cataloged API Surface</span>
          <span className="text-xs font-mono text-slate-400">5 Endpoints Active</span>
        </div>

        <div className="divide-y divide-slate-800/60 font-mono text-xs">
          {apiEndpoints.map((ep, idx) => (
            <div key={idx} className="p-4 hover:bg-slate-900/40 transition-colors flex flex-col md:flex-row md:items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span
                    className={`text-[10px] px-2 py-0.5 rounded font-bold ${
                      ep.method === "GET"
                        ? "bg-cyan-950 text-cyan-300 border border-cyan-800"
                        : ep.method === "POST"
                        ? "bg-emerald-950 text-emerald-300 border border-emerald-800"
                        : "bg-rose-950 text-rose-300 border border-rose-800"
                    }`}
                  >
                    {ep.method}
                  </span>
                  <span className="font-bold text-slate-100">{ep.path}</span>
                  <span
                    className={`text-[10px] px-1.5 py-0.2 rounded border font-semibold ${
                      ep.status === "Compliant"
                        ? "bg-emerald-950 text-emerald-300 border-emerald-800"
                        : "bg-rose-950 text-rose-300 border-rose-800"
                    }`}
                  >
                    {ep.status}
                  </span>
                </div>
                <div className="text-slate-400 text-[11px] mt-1 flex items-center gap-3">
                  <span>Schema: {ep.schema}</span>
                  <span>&bull;</span>
                  <span>Fuzz Coverage: {ep.fuzzed}</span>
                </div>
              </div>

              <div className="flex items-center gap-4 text-slate-300">
                <div className="text-right">
                  <div className="text-[10px] text-slate-500">P95 Latency</div>
                  <div className="font-bold text-cyan-400">{ep.latency}</div>
                </div>
                <button className="px-3 py-1.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs">
                  Run Schemathesis
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
