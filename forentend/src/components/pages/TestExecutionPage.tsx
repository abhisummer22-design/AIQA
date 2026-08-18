/**
 * AIQA Test Execution Workspace
 * Real-time runner orchestration, live logs, diagnostic trace viewer,
 * and failure analysis supporting AIQA's 10 status codes.
 */

import React, { useState } from 'react';
import {
  PlayCircle,
  Play,
  Square,
  RefreshCw,
  Terminal,
  Filter,
  Search,
  FileText,
  ShieldAlert,
  ServerCrash,
  CheckCircle2,
  Clock,
  ChevronRight,
  ExternalLink,
  Layers,
  Cpu,
  AlertTriangle,
  Download
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { StatusBadge, FailureClassificationBadge } from '../common/StatusBadge';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';
import { TestExecutionItem, AIQAStatus } from '../../types';

export const TestExecutionPage: React.FC = () => {
  const {
    runs,
    activeRunId,
    setActiveRunId,
    currentRun,
    cancelRun,
    setNewRunModalOpen,
    currentEnvironment,
  } = useAIQA();

  const [statusFilter, setStatusFilter] = useState<string>('ALL');
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [activeTestDrawer, setActiveTestDrawer] = useState<TestExecutionItem | null>(null);
  const [activeTab, setActiveTab] = useState<'details' | 'logs' | 'evidence'>('details');

  const allStatuses: AIQAStatus[] = [
    'QUEUED',
    'RUNNING',
    'PASSED',
    'FAILED',
    'BLOCKED',
    'SKIPPED',
    'CANCELLED',
    'TIMEOUT',
    'INFRA_ERROR',
    'NEEDS_REVIEW',
  ];

  const tests = currentRun?.tests || [];

  const filteredTests = tests.filter((t) => {
    if (statusFilter !== 'ALL' && t.status !== statusFilter) return false;
    if (searchQuery.trim() && !t.name.toLowerCase().includes(searchQuery.toLowerCase()) && !t.id.toLowerCase().includes(searchQuery.toLowerCase())) {
      return false;
    }
    return true;
  });

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Test Execution Workspace"
        subtitle="Autonomous distributed test orchestration, concurrency management, and runtime telemetry"
        badge={currentRun ? currentRun.executor : 'RUNNER POOL'}
        icon={PlayCircle}
        actions={
          <div className="flex items-center gap-2">
            {currentRun && currentRun.status === 'RUNNING' && (
              <button
                onClick={() => cancelRun(currentRun.id)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-rose-950/80 hover:bg-rose-900 border border-rose-800 text-rose-300 text-xs font-semibold cursor-pointer"
              >
                <Square className="w-3.5 h-3.5 fill-current" />
                <span>Abort Run</span>
              </button>
            )}
            <button
              onClick={() => setNewRunModalOpen(true)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>Launch New Execution</span>
            </button>
          </div>
        }
      />

      {/* Execution Config & Run Header */}
      {currentRun ? (
        <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-3.5 md:p-4 space-y-3">
          <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-3 pb-3 border-b border-slate-800">
            <div className="space-y-1">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-mono text-[11px] font-bold text-indigo-400 bg-indigo-950/60 px-1.5 py-0.2 rounded border border-indigo-800/80 uppercase">
                  {currentRun.id}
                </span>
                <h2 className="text-sm font-bold text-slate-100 font-sans">{currentRun.title}</h2>
                <StatusBadge status={currentRun.status} size="sm" />
              </div>
              <div className="flex items-center gap-3 text-[11px] font-mono text-slate-400 flex-wrap">
                <span>Env: <strong className="text-slate-200">{currentRun.environment}</strong></span>
                <span>Executor: <strong className="text-slate-200">{currentRun.executor}</strong></span>
                <span>Concurrency: <strong className="text-slate-200">{currentRun.concurrency} slots</strong></span>
                <span>Approval: <strong className="text-emerald-400">{currentRun.policyApprovalState}</strong></span>
                <span>Started: <strong className="text-slate-200">{new Date(currentRun.startedAt).toLocaleTimeString()}</strong></span>
              </div>
            </div>

            {/* Switch Run */}
            <div className="flex items-center gap-2">
              <span className="text-xs text-slate-400">Active Run:</span>
              <select
                value={currentRun.id}
                onChange={(e) => setActiveRunId(e.target.value)}
                className="px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-xs text-slate-200 focus:border-indigo-500 focus:outline-none font-mono"
              >
                {runs.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.id} ({r.status}) — {r.environment}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Status Chips Filter Bar */}
          <div className="flex flex-col md:flex-row items-start md:items-center justify-between gap-2.5">
            {/* Search */}
            <div className="relative w-full md:w-64">
              <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-2" />
              <input
                type="text"
                placeholder="Filter tests by name or ID..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="w-full pl-8 pr-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800 text-xs text-slate-200 placeholder-slate-500 focus:border-indigo-500 focus:outline-none"
              />
            </div>

            {/* Status Filters */}
            <div className="flex flex-wrap items-center gap-1 text-xs">
              <button
                onClick={() => setStatusFilter('ALL')}
                className={`px-2 py-0.5 rounded font-mono text-[10px] font-semibold cursor-pointer ${
                  statusFilter === 'ALL'
                    ? 'bg-slate-700 text-slate-100'
                    : 'bg-[#020617] border border-slate-800 text-slate-400 hover:text-slate-200'
                }`}
              >
                ALL ({tests.length})
              </button>
              {allStatuses.map((st) => {
                const count = tests.filter((t) => t.status === st).length;
                if (count === 0 && statusFilter !== st) return null;
                return (
                  <button
                    key={st}
                    onClick={() => setStatusFilter(st)}
                    className={`px-1.5 py-0.5 rounded font-mono text-[10px] font-medium transition-colors cursor-pointer ${
                      statusFilter === st
                        ? 'bg-indigo-950 border border-indigo-700 text-indigo-300'
                        : 'bg-[#020617] border border-slate-800/80 text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    {st} ({count})
                  </button>
                );
              })}
            </div>
          </div>

          {/* Test Status Table */}
          <div className="overflow-x-auto rounded-lg border border-slate-800 bg-[#020617]">
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="border-b border-slate-800 bg-[#0B1120] text-slate-400 font-mono text-[10px] uppercase">
                  <th className="py-2.5 px-3">Identifier & Test Name</th>
                  <th className="py-2.5 px-3">Suite Module</th>
                  <th className="py-2.5 px-3">Type</th>
                  <th className="py-2.5 px-3">Status</th>
                  <th className="py-2.5 px-3">Diagnosis</th>
                  <th className="py-2.5 px-3">Duration</th>
                  <th className="py-2.5 px-3">Executor</th>
                  <th className="py-2.5 px-3 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 font-sans">
                {filteredTests.length === 0 ? (
                  <tr>
                    <td colSpan={8} className="py-6 text-center text-slate-500 font-mono text-xs">
                      No tests match current filter parameters.
                    </td>
                  </tr>
                ) : (
                  filteredTests.map((test) => (
                    <tr
                      key={test.id}
                      onClick={() => setActiveTestDrawer(test)}
                      className="hover:bg-slate-800/30 transition-colors cursor-pointer group"
                    >
                      <td className="py-2.5 px-3">
                        <div className="font-semibold text-slate-200 group-hover:text-indigo-300 text-xs">
                          {test.name}
                        </div>
                        <div className="font-mono text-[10px] text-slate-500">{test.id}</div>
                      </td>
                      <td className="py-2.5 px-3 text-slate-400 font-mono text-[11px]">{test.suite}</td>
                      <td className="py-2.5 px-3">
                        <span className="px-1.5 py-0.2 rounded bg-[#0B1120] border border-slate-800 text-slate-300 font-mono text-[10px]">
                          {test.type}
                        </span>
                      </td>
                      <td className="py-2.5 px-3">
                        <StatusBadge status={test.status} size="sm" />
                      </td>
                      <td className="py-2.5 px-3">
                        {test.failureClassification ? (
                          <FailureClassificationBadge classification={test.failureClassification} />
                        ) : (
                          <span className="text-slate-600 font-mono">—</span>
                        )}
                      </td>
                      <td className="py-2.5 px-3 font-mono text-slate-300 text-xs">
                        {test.durationMs ? `${(test.durationMs / 1000).toFixed(2)}s` : '—'}
                      </td>
                      <td className="py-2.5 px-3 font-mono text-[10px] text-slate-400">{test.executor}</td>
                      <td className="py-2.5 px-3 text-right font-mono">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setActiveTestDrawer(test);
                          }}
                          className="px-2 py-0.5 rounded bg-[#0B1120] hover:bg-slate-800 text-indigo-400 border border-slate-700 text-[10px] transition-colors cursor-pointer"
                        >
                          Inspect
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        <EmptyState
          icon={PlayCircle}
          title="No Active Execution Run"
          description="Select an environment and launch a test execution workload."
          actionLabel="Launch New Run"
          onAction={() => setNewRunModalOpen(true)}
        />
      )}

      {/* Side Inspection Drawer */}
      {activeTestDrawer && (
        <div className="fixed inset-0 z-50 flex items-center justify-end bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-100">
          <div
            className="w-full max-w-xl h-full bg-[#0B1120] border-l border-slate-800 shadow-2xl p-5 flex flex-col justify-between text-xs overflow-y-auto"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="space-y-3.5">
              <div className="flex items-start justify-between pb-3 border-b border-slate-800">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-xs text-slate-400">{activeTestDrawer.id}</span>
                    <StatusBadge status={activeTestDrawer.status} size="sm" />
                  </div>
                  <h3 className="text-sm font-bold text-slate-100 mt-1 font-sans">{activeTestDrawer.name}</h3>
                </div>
                <button
                  onClick={() => setActiveTestDrawer(null)}
                  className="px-2 py-1 rounded bg-[#020617] border border-slate-800 text-slate-300 hover:bg-slate-800 cursor-pointer text-xs"
                >
                  Close
                </button>
              </div>

              {/* Tabs */}
              <div className="flex border-b border-slate-800 gap-4 text-xs font-medium">
                {(['details', 'logs', 'evidence'] as const).map((tab) => (
                  <button
                    key={tab}
                    onClick={() => setActiveTab(tab)}
                    className={`pb-1.5 capitalize font-mono text-[11px] cursor-pointer ${
                      activeTab === tab
                        ? 'text-indigo-400 border-b-2 border-indigo-400 font-bold'
                        : 'text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    {tab}
                  </button>
                ))}
              </div>

              {/* Tab Contents */}
              {activeTab === 'details' && (
                <div className="space-y-3">
                  {activeTestDrawer.failureClassification && (
                    <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2">
                      <div className="text-[10px] font-mono text-slate-400 font-bold uppercase">
                        Failure Diagnosis & Classification
                      </div>
                      <FailureClassificationBadge classification={activeTestDrawer.failureClassification} />
                      {activeTestDrawer.failureMessage && (
                        <div className="p-2 rounded bg-rose-950/40 border border-rose-900 text-rose-300 font-mono text-[11px]">
                          {activeTestDrawer.failureMessage}
                        </div>
                      )}
                      {activeTestDrawer.stackTrace && (
                        <pre className="p-2.5 rounded bg-[#0B1120] border border-slate-800 font-mono text-[10px] text-slate-400 overflow-x-auto max-h-44">
                          {activeTestDrawer.stackTrace}
                        </pre>
                      )}
                    </div>
                  )}

                  <div className="grid grid-cols-2 gap-2 text-slate-300 font-mono text-[11px]">
                    <div className="p-2 rounded bg-[#020617] border border-slate-800">
                      <span className="text-slate-500">Suite:</span> {activeTestDrawer.suite}
                    </div>
                    <div className="p-2 rounded bg-[#020617] border border-slate-800">
                      <span className="text-slate-500">Executor:</span> {activeTestDrawer.executor}
                    </div>
                    <div className="p-2 rounded bg-[#020617] border border-slate-800">
                      <span className="text-slate-500">Duration:</span> {activeTestDrawer.durationMs || 0}ms
                    </div>
                    <div className="p-2 rounded bg-[#020617] border border-slate-800">
                      <span className="text-slate-500">Retries:</span> {activeTestDrawer.retries}
                    </div>
                  </div>
                </div>
              )}

              {activeTab === 'logs' && (
                <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 font-mono text-[10px] text-slate-300 space-y-1 max-h-96 overflow-y-auto">
                  <div className="text-slate-500">// Runner stdout / stderr log stream</div>
                  <div>[2026-08-16T09:12:01.002Z] Starting test container on k8s-runner-02</div>
                  <div>[2026-08-16T09:12:01.418Z] Injecting mock tokens & session headers</div>
                  <div className="text-rose-400">[2026-08-16T09:12:02.100Z] Assertion Failed: Expected status 401, got 200</div>
                  <div className="text-slate-500">[2026-08-16T09:12:02.340Z] Captured HAR network snapshot</div>
                </div>
              )}

              {activeTab === 'evidence' && (
                <div className="space-y-2">
                  <div className="text-xs font-semibold text-slate-300">Recorded Diagnostic Traces</div>
                  {activeTestDrawer.evidenceFiles && activeTestDrawer.evidenceFiles.length > 0 ? (
                    activeTestDrawer.evidenceFiles.map((ev) => (
                      <div
                        key={ev.name}
                        className="p-2.5 rounded-lg bg-[#020617] border border-slate-800 flex items-center justify-between font-mono text-xs"
                      >
                        <div className="flex items-center gap-2">
                          <FileText className="w-4 h-4 text-indigo-400" />
                          <span className="text-slate-200 text-[11px]">{ev.name}</span>
                        </div>
                        <div className="flex items-center gap-2">
                          <span className="text-slate-500 text-[10px]">{ev.size}</span>
                          <button className="text-indigo-400 hover:text-indigo-300 cursor-pointer">
                            <Download className="w-3 h-3" />
                          </button>
                        </div>
                      </div>
                    ))
                  ) : (
                    <div className="p-4 text-center text-slate-500 font-mono text-xs bg-[#020617] rounded-lg border border-slate-800">
                      No trace artifacts recorded for this test item.
                    </div>
                  )}
                </div>
              )}
            </div>

            <div className="pt-3 border-t border-slate-800 flex justify-end">
              <button
                onClick={() => setActiveTestDrawer(null)}
                className="px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 border border-slate-800 text-slate-200 text-xs font-semibold cursor-pointer"
              >
                Close Drawer
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
