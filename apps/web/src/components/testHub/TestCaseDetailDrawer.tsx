import React, { useState } from "react";
import {
  X,
  Play,
  CheckCircle2,
  XCircle,
  Clock,
  Layers,
  Cpu,
  User,
  Shield,
  Tag,
  Terminal,
  Bug,
  FileText,
  Activity,
  AlertOctagon,
  Copy,
  Check,
  Share2
} from "lucide-react";
import { TestCase, TestType } from "../../types";
import { TEST_TYPE_LABELS } from "./TestCategoryNav";

interface TestCaseDetailDrawerProps {
  testCase: TestCase | null;
  isOpen: boolean;
  onClose: () => void;
  onRunTest: (testCase: TestCase) => void;
  onCreateBugFromFailure?: (testCase: TestCase) => void;
}

export const TestCaseDetailDrawer: React.FC<TestCaseDetailDrawerProps> = ({
  testCase,
  isOpen,
  onClose,
  onRunTest,
  onCreateBugFromFailure
}) => {
  const [activeTab, setActiveTab] = useState<"info" | "execution" | "logs" | "evidence">("info");
  const [copied, setCopied] = useState<boolean>(false);

  if (!isOpen || !testCase) return null;

  const handleCopyJson = () => {
    navigator.clipboard.writeText(JSON.stringify(testCase, null, 2));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const isFailed = testCase.status === "Failed";
  const exec = testCase.last_execution;

  return (
    <div className="fixed inset-0 z-50 overflow-hidden bg-black/70 backdrop-blur-xs flex justify-end animate-in fade-in duration-150 font-mono">
      <div
        id="test-case-detail-drawer"
        className="w-full max-w-2xl bg-[#0a0f1d] border-l border-slate-800 h-full shadow-2xl flex flex-col min-w-0"
      >
        {/* Drawer Header */}
        <div className="p-4 border-b border-slate-800 bg-[#0d1424] flex items-start justify-between gap-4">
          <div className="space-y-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="font-bold text-xs text-cyan-400 bg-cyan-950 px-2 py-0.5 rounded border border-cyan-800">
                {testCase.id}
              </span>
              <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-slate-800 text-slate-200 uppercase border border-slate-700">
                {TEST_TYPE_LABELS[testCase.test_type] || testCase.test_type}
              </span>
              <span
                className={`text-[10px] font-bold px-2 py-0.5 rounded border ${
                  testCase.status === "Passed"
                    ? "bg-emerald-950 text-emerald-300 border-emerald-800"
                    : testCase.status === "Failed"
                    ? "bg-rose-950 text-rose-300 border-rose-800"
                    : "bg-slate-900 text-slate-400 border-slate-700"
                }`}
              >
                {testCase.status.toUpperCase()}
              </span>
            </div>
            <h2 className="text-base font-bold text-slate-100 leading-snug truncate">
              {testCase.name}
            </h2>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <button
              onClick={handleCopyJson}
              title="Copy JSON representation"
              className="p-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
            >
              {copied ? <Check className="w-4 h-4 text-emerald-400" /> : <Copy className="w-4 h-4" />}
            </button>
            <button
              onClick={onClose}
              className="p-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="flex items-center gap-1 px-4 py-2 border-b border-slate-800/80 bg-[#090d18] text-xs">
          <button
            onClick={() => setActiveTab("info")}
            className={`px-3 py-1.5 rounded-lg font-bold transition-colors ${
              activeTab === "info"
                ? "bg-cyan-950 text-cyan-300 border border-cyan-800"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            Specification & Steps
          </button>
          <button
            onClick={() => setActiveTab("execution")}
            className={`px-3 py-1.5 rounded-lg font-bold transition-colors ${
              activeTab === "execution"
                ? "bg-cyan-950 text-cyan-300 border border-cyan-800"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            Diagnostics & Results
          </button>
          <button
            onClick={() => setActiveTab("logs")}
            className={`px-3 py-1.5 rounded-lg font-bold transition-colors flex items-center gap-1.5 ${
              activeTab === "logs"
                ? "bg-cyan-950 text-cyan-300 border border-cyan-800"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            <Terminal className="w-3.5 h-3.5" />
            <span>Stdout Logs ({exec?.logs?.length || 0})</span>
          </button>
          {exec?.evidence && exec.evidence.length > 0 && (
            <button
              onClick={() => setActiveTab("evidence")}
              className={`px-3 py-1.5 rounded-lg font-bold transition-colors ${
                activeTab === "evidence"
                  ? "bg-cyan-950 text-cyan-300 border border-cyan-800"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              Evidence ({exec.evidence.length})
            </button>
          )}
        </div>

        {/* Scrollable Body Canvas */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4 text-xs">
          {/* TAB 1: SPECIFICATION & STEPS */}
          {activeTab === "info" && (
            <div className="space-y-4">
              {/* Description */}
              <div className="p-3 rounded-lg bg-[#0e1422] border border-slate-800 space-y-1">
                <div className="text-[10px] text-slate-500 uppercase font-bold">Description</div>
                <p className="text-slate-300 leading-relaxed">{testCase.description}</p>
              </div>

              {/* Key Attributes Grid */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                <div className="p-2.5 rounded-lg bg-[#0e1422] border border-slate-800">
                  <div className="text-[10px] text-slate-500 uppercase font-semibold">Priority</div>
                  <div className="font-bold text-slate-200 mt-0.5">{testCase.priority}</div>
                </div>
                <div className="p-2.5 rounded-lg bg-[#0e1422] border border-slate-800">
                  <div className="text-[10px] text-slate-500 uppercase font-semibold">Severity</div>
                  <div className="font-bold text-slate-200 mt-0.5">{testCase.severity || "Major"}</div>
                </div>
                <div className="p-2.5 rounded-lg bg-[#0e1422] border border-slate-800">
                  <div className="text-[10px] text-slate-500 uppercase font-semibold">Environment</div>
                  <div className="font-bold text-slate-200 mt-0.5">{testCase.environment}</div>
                </div>
                <div className="p-2.5 rounded-lg bg-[#0e1422] border border-slate-800">
                  <div className="text-[10px] text-slate-500 uppercase font-semibold">Owner</div>
                  <div className="font-bold text-slate-200 mt-0.5 truncate">{testCase.owner_name}</div>
                </div>
              </div>

              {/* Preconditions */}
              {testCase.preconditions && testCase.preconditions.length > 0 && (
                <div className="p-3 rounded-lg bg-[#0e1422] border border-slate-800 space-y-1.5">
                  <div className="text-[10px] text-slate-500 uppercase font-bold">Preconditions</div>
                  <ul className="list-disc list-inside space-y-1 text-slate-300">
                    {testCase.preconditions.map((prec, i) => (
                      <li key={i}>{prec}</li>
                    ))}
                  </ul>
                </div>
              )}

              {/* Test Steps Table */}
              <div className="space-y-2">
                <div className="text-[10px] text-slate-500 uppercase font-bold flex items-center justify-between">
                  <span>Step-by-Step Test Procedure</span>
                  <span className="text-cyan-400">{testCase.steps.length} Steps</span>
                </div>
                <div className="rounded-lg bg-[#0e1422] border border-slate-800 divide-y divide-slate-800/80 overflow-hidden">
                  {testCase.steps.map((step, idx) => (
                    <div key={idx} className="p-2.5 flex items-start gap-3 text-slate-300">
                      <span className="w-5 h-5 rounded bg-slate-900 border border-slate-700 text-cyan-400 flex items-center justify-center font-bold text-[10px] shrink-0 mt-0.5">
                        {idx + 1}
                      </span>
                      <span className="leading-relaxed flex-1">{step}</span>
                    </div>
                  ))}
                </div>
              </div>

              {/* Expected Result */}
              <div className="p-3 rounded-lg bg-emerald-950/20 border border-emerald-900/40 space-y-1">
                <div className="text-[10px] text-emerald-400 uppercase font-bold">Expected Outcome</div>
                <p className="text-emerald-300 leading-relaxed">{testCase.expected_result}</p>
              </div>

              {/* Tags */}
              <div className="flex items-center gap-1.5 flex-wrap">
                <span className="text-[10px] text-slate-500 font-bold uppercase">Tags:</span>
                {testCase.tags.map((tag, idx) => (
                  <span
                    key={idx}
                    className="text-[10px] px-2 py-0.5 rounded bg-slate-900 border border-slate-800 text-slate-300"
                  >
                    #{tag}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* TAB 2: DIAGNOSTICS & RESULTS */}
          {activeTab === "execution" && (
            <div className="space-y-4">
              {/* Failure Banner if Failed */}
              {isFailed && (
                <div className="p-3.5 rounded-lg bg-rose-950/40 border border-rose-800 space-y-2">
                  <div className="flex items-center justify-between text-rose-300 font-bold">
                    <span className="flex items-center gap-1.5">
                      <XCircle className="w-4 h-4 text-rose-400" />
                      Assertion / Execution Failure Detected
                    </span>
                    <span className="text-[10px] uppercase px-2 py-0.5 rounded bg-rose-950 border border-rose-700">
                      High Priority Defect
                    </span>
                  </div>

                  {exec?.failure_reason && (
                    <div className="text-rose-200 text-xs font-mono leading-relaxed bg-black/40 p-2.5 rounded border border-rose-900">
                      {exec.failure_reason}
                    </div>
                  )}

                  {exec?.error_stack && (
                    <div className="space-y-1">
                      <div className="text-[10px] text-rose-400 uppercase font-bold">Error Stack Trace</div>
                      <pre className="text-[11px] font-mono text-rose-300 bg-black/70 p-2.5 rounded border border-rose-900/60 overflow-x-auto whitespace-pre-wrap">
                        {exec.error_stack}
                      </pre>
                    </div>
                  )}
                </div>
              )}

              {/* Performance Metrics if Available */}
              {exec?.metrics && (
                <div className="p-3 rounded-lg bg-amber-950/20 border border-amber-900/50 space-y-2">
                  <div className="text-[10px] text-amber-400 uppercase font-bold flex items-center gap-1.5">
                    <Activity className="w-3.5 h-3.5" />
                    <span>Performance Benchmark Telemetry</span>
                  </div>
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                    <div className="p-2 rounded bg-black/40 border border-amber-900/40">
                      <div className="text-[10px] text-slate-500">P95 Latency</div>
                      <div className="text-sm font-bold text-amber-300">{exec.metrics.p95_latency_ms}ms</div>
                    </div>
                    <div className="p-2 rounded bg-black/40 border border-amber-900/40">
                      <div className="text-[10px] text-slate-500">Throughput</div>
                      <div className="text-sm font-bold text-amber-300">{exec.metrics.throughput_rps} RPS</div>
                    </div>
                    <div className="p-2 rounded bg-black/40 border border-amber-900/40">
                      <div className="text-[10px] text-slate-500">Peak CPU</div>
                      <div className="text-sm font-bold text-amber-300">{exec.metrics.cpu_percent}%</div>
                    </div>
                    <div className="p-2 rounded bg-black/40 border border-amber-900/40">
                      <div className="text-[10px] text-slate-500">VRAM / Heap</div>
                      <div className="text-sm font-bold text-amber-300">{exec.metrics.memory_mb} MB</div>
                    </div>
                  </div>
                </div>
              )}

              {/* Execution Run Metadata */}
              <div className="p-3 rounded-lg bg-[#0e1422] border border-slate-800 space-y-2">
                <div className="text-[10px] text-slate-500 uppercase font-bold">Execution Run Telemetry</div>
                <div className="grid grid-cols-2 gap-3 text-slate-300">
                  <div>
                    <span className="text-slate-500">Start Time:</span>{" "}
                    <strong>{exec?.start_time ? new Date(exec.start_time).toLocaleTimeString() : "N/A"}</strong>
                  </div>
                  <div>
                    <span className="text-slate-500">Duration:</span>{" "}
                    <strong>{exec?.duration_ms ? `${exec.duration_ms}ms` : "N/A"}</strong>
                  </div>
                  <div>
                    <span className="text-slate-500">Executor Engine:</span>{" "}
                    <strong>{exec?.executor || "AIQA Distributed Runner"}</strong>
                  </div>
                  <div>
                    <span className="text-slate-500">Target Environment:</span>{" "}
                    <strong>{exec?.environment || testCase.environment}</strong>
                  </div>
                </div>
              </div>

              {/* Actual Outcome vs Expected */}
              <div className="space-y-2">
                <div className="p-3 rounded-lg bg-[#0e1422] border border-slate-800 space-y-1">
                  <div className="text-[10px] text-slate-500 uppercase font-bold">Actual Outcome Recorded</div>
                  <p className="text-slate-200 leading-relaxed font-mono">
                    {exec?.actual_result || testCase.actual_result || "Awaiting execution trigger"}
                  </p>
                </div>
              </div>
            </div>
          )}

          {/* TAB 3: STDOUT LOGS */}
          {activeTab === "logs" && (
            <div className="space-y-2">
              <div className="text-[10px] text-slate-500 uppercase font-bold flex items-center justify-between">
                <span>Console Logs Stream</span>
                <span>{exec?.logs?.length || 0} entries</span>
              </div>
              <div className="p-3 rounded-lg bg-black/90 border border-slate-800 text-[11px] font-mono space-y-1.5 overflow-x-auto min-h-[220px]">
                {exec?.logs && exec.logs.length > 0 ? (
                  exec.logs.map((log, idx) => (
                    <div
                      key={idx}
                      className={
                        log.includes("[FAIL]") || log.includes("Error")
                          ? "text-rose-400 font-bold"
                          : log.includes("[PASS]")
                          ? "text-emerald-400 font-semibold"
                          : "text-slate-300"
                      }
                    >
                      {log}
                    </div>
                  ))
                ) : (
                  <div className="text-slate-500 italic">No stdout logs available for this test case.</div>
                )}
              </div>
            </div>
          )}

          {/* TAB 4: EVIDENCE ARTIFACTS */}
          {activeTab === "evidence" && exec?.evidence && (
            <div className="space-y-3">
              <div className="text-[10px] text-slate-500 uppercase font-bold">
                Captured Diagnostics Evidence ({exec.evidence.length})
              </div>
              {exec.evidence.map((ev, i) => (
                <div key={i} className="p-3 rounded-lg bg-[#0e1422] border border-slate-800 space-y-2">
                  <div className="flex items-center justify-between font-bold text-xs text-cyan-300">
                    <span>{ev.title}</span>
                    <span className="text-[9px] uppercase px-1.5 py-0.2 rounded bg-slate-900 border border-slate-700 text-slate-400">
                      {ev.type}
                    </span>
                  </div>
                  <pre className="p-2.5 rounded bg-black/80 border border-slate-800/80 text-[11px] font-mono text-slate-300 overflow-x-auto whitespace-pre-wrap">
                    {ev.data}
                  </pre>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Drawer Action Footer */}
        <div className="p-4 border-t border-slate-800 bg-[#0d1424] flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <button
              onClick={() => onRunTest(testCase)}
              className="flex items-center gap-2 px-4 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white font-bold text-xs shadow-sm transition-all"
            >
              <Play className="w-4 h-4 fill-current" />
              <span>Execute Test Case</span>
            </button>

            {isFailed && onCreateBugFromFailure && (
              <button
                onClick={() => onCreateBugFromFailure(testCase)}
                className="flex items-center gap-2 px-3.5 py-2 rounded-lg bg-rose-600 hover:bg-rose-500 text-white font-bold text-xs shadow-sm transition-all"
              >
                <Bug className="w-4 h-4" />
                <span>Create Bug Candidate</span>
              </button>
            )}
          </div>

          <button
            onClick={onClose}
            className="px-4 py-2 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 font-bold text-xs"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
