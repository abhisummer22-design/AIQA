/**
 * AIQA Dashboard Page — Main Content Experience
 * Futuristic AI Quality Engineering Control Plane
 * Consumes authoritative multi-project state, real-time KPIs, AI Quality Core, and live execution telemetry.
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Layers,
  Play,
  CheckCircle2,
  XCircle,
  Ban,
  HelpCircle,
  Activity,
  Server,
  Clock,
  ChevronDown,
  ArrowUpRight,
  ShieldCheck,
  AlertTriangle,
  Radio,
  ExternalLink,
  Cpu,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { useProject } from '../../context/ProjectContext';
import { AIQualityCoreOrb } from '../dashboard/AIQualityCoreOrb';
import { ProjectStatusDonut } from '../dashboard/ProjectStatusDonut';
import { ProjectExecutionGraph } from '../dashboard/ProjectExecutionGraph';
import { ProjectStatus } from '../../types';

// FE-4: this is the one, minimal, non-visual routing change made to this
// product-owner-designed page — the "Execution Hub" quick-action below now
// navigates via the real router instead of the old in-memory `activePage`
// setter. No markup, styling, or interaction was altered.
//
// FE-5: this page's own header selector, KPI cards, and every field read
// off `selectedProject` below (`.metrics`, `.activeRun`, `.environment`,
// `.slug`, `.timelineData`, ...) are `AIQAContext`'s pre-existing fabricated
// `AIQAProject` demo data — none of those fields exist on the real backend
// `Project` type (`{id, name, description, created_at, updated_at}`), and
// this sprint's brief explicitly forbids both inventing them and
// prematurely wiring real per-project KPI endpoints here (brief §11/§12).
// So this page's own selector/data is left exactly as FE-4 left it. The
// one real, non-visual integration point is `realProjectId` below, used
// only as a remount key: if the real global project scope changes, this
// page's local component state (branch/tab selections, in-progress local
// UI state) resets rather than silently continuing to display against a
// project it no longer reflects — this is what "know the current project /
// don't retain stale project-specific info after switching" (brief §11)
// means for a page whose visible data is still 100% fabricated.
export const DashboardPage: React.FC = () => {
  const {
    projects,
    selectedProjectId,
    setSelectedProjectId,
    selectedProject,
    setNewRunModalOpen,
    backendState,
  } = useAIQA();
  const { selectedProjectId: realProjectId } = useProject();
  const navigate = useNavigate();

  // Color helper for project status
  const getStatusBadge = (status: ProjectStatus) => {
    switch (status) {
      case 'Healthy':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-semibold bg-emerald-950/80 border border-emerald-700/80 text-emerald-300">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            Healthy
          </span>
        );
      case 'Running':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-semibold bg-cyan-950/80 border border-cyan-700/80 text-cyan-300">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-ping" />
            Running
          </span>
        );
      case 'Warning':
      case 'Degraded':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-semibold bg-amber-950/80 border border-amber-700/80 text-amber-300">
            <AlertTriangle className="w-3 h-3 text-amber-400" />
            {status}
          </span>
        );
      case 'Critical':
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-semibold bg-rose-950/80 border border-rose-700/80 text-rose-300">
            <XCircle className="w-3 h-3 text-rose-400" />
            Critical
          </span>
        );
      case 'Offline':
      default:
        return (
          <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-mono font-semibold bg-slate-900 border border-slate-700 text-slate-400">
            <span className="w-1.5 h-1.5 rounded-full bg-slate-500" />
            Offline
          </span>
        );
    }
  };

  const { metrics, activeRun } = selectedProject;

  return (
    <div
      id="aiqa-dashboard-main-content"
      key={realProjectId ?? 'no-real-project-selected'}
      className="space-y-4 animate-in fade-in duration-200 text-slate-200"
    >
      {/* 1. Header with Project Selector */}
      <header
        id="dashboard-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-indigo-950/80 border border-indigo-700/60 text-indigo-400">
              <Activity className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">
              Dashboard
            </h1>
            <span className="px-2 py-0.5 rounded bg-indigo-950/60 border border-indigo-800/60 text-[10px] font-mono text-indigo-300 uppercase">
              CONTROL PLANE
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            Real-time quality intelligence for your project
          </p>
        </div>

        {/* Enterprise Project Selector */}
        <div className="flex items-center gap-3">
          <div className="flex flex-col items-start md:items-end">
            <span className="text-[9px] uppercase font-mono tracking-wider text-slate-400">
              Active Project Context
            </span>
            <div className="relative mt-0.5">
              <select
                id="project-selector-dropdown"
                value={selectedProjectId}
                onChange={(e) => setSelectedProjectId(e.target.value)}
                className="appearance-none pl-3 pr-8 py-1.5 rounded-md bg-[#020617] hover:bg-slate-900 border border-indigo-700/60 hover:border-indigo-500 text-xs font-semibold text-slate-100 focus:outline-none focus:ring-1 focus:ring-indigo-500 cursor-pointer font-sans shadow-sm"
              >
                {projects.map((p) => (
                  <option key={p.id} value={p.id} className="bg-[#020617] text-slate-200 py-1">
                    Project [ {p.name} ] — {p.environment}
                  </option>
                ))}
              </select>
              <ChevronDown className="w-3.5 h-3.5 text-indigo-400 absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
            </div>
          </div>

          {/* New Run Action */}
          <button
            id="btn-launch-run"
            onClick={() => setNewRunModalOpen(true)}
            className="flex items-center gap-1.5 px-3 py-2 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer transition-colors shrink-0"
          >
            <Play className="w-3 h-3 fill-current" />
            <span className="hidden sm:inline">Launch Run</span>
          </button>
        </div>
      </header>

      {/* 2. KPI Cards Row */}
      <section id="dashboard-kpi-row" aria-label="Key Performance Indicators">
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2.5">
          {/* Total Test Cases */}
          <div className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 hover:border-slate-700 transition-all shadow-sm flex flex-col justify-between">
            <div className="flex items-center justify-between text-slate-400">
              <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                Total Test Cases
              </span>
              <Layers className="w-3.5 h-3.5 text-slate-400" />
            </div>
            <div className="mt-2">
              <div className="text-xl font-bold font-mono text-slate-100">
                {metrics.totalTestCases.toLocaleString()}
              </div>
              <div className="text-[10px] text-slate-400 font-sans truncate mt-0.5">
                {selectedProject.name}
              </div>
            </div>
          </div>

          {/* Active Runs */}
          <div
            className={`p-3 rounded-lg bg-[#0B1120] border ${
              metrics.activeRunsCount > 0
                ? 'border-cyan-800/80 bg-cyan-950/20'
                : 'border-slate-800'
            } hover:border-slate-700 transition-all shadow-sm flex flex-col justify-between`}
          >
            <div className="flex items-center justify-between text-slate-400">
              <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                Active Runs
              </span>
              <Play
                className={`w-3.5 h-3.5 ${
                  metrics.activeRunsCount > 0 ? 'text-cyan-400 animate-pulse' : 'text-slate-500'
                }`}
              />
            </div>
            <div className="mt-2">
              <div
                className={`text-xl font-bold font-mono ${
                  metrics.activeRunsCount > 0 ? 'text-cyan-300' : 'text-slate-200'
                }`}
              >
                {metrics.activeRunsCount}
              </div>
              <div className="text-[10px] text-slate-400 font-sans truncate mt-0.5">
                {metrics.activeRunsCount > 0 ? 'Executing queue' : 'Idle runner pool'}
              </div>
            </div>
          </div>

          {/* Passed */}
          <div className="p-3 rounded-lg bg-[#0B1120] border border-emerald-900/60 bg-emerald-950/10 hover:border-emerald-700/60 transition-all shadow-sm flex flex-col justify-between">
            <div className="flex items-center justify-between text-emerald-400">
              <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                Passed
              </span>
              <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
            </div>
            <div className="mt-2">
              <div className="text-xl font-bold font-mono text-emerald-300">
                {metrics.passed.toLocaleString()}
              </div>
              <div className="text-[10px] text-emerald-400/80 font-sans truncate mt-0.5">
                {metrics.totalTestCases > 0
                  ? `${((metrics.passed / metrics.totalTestCases) * 100).toFixed(1)}% pass rate`
                  : 'No runs recorded'}
              </div>
            </div>
          </div>

          {/* Failed */}
          <div
            className={`p-3 rounded-lg bg-[#0B1120] border ${
              metrics.failed > 0
                ? 'border-rose-900/60 bg-rose-950/20'
                : 'border-slate-800'
            } hover:border-rose-700/60 transition-all shadow-sm flex flex-col justify-between`}
          >
            <div className="flex items-center justify-between text-rose-400">
              <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                Failed
              </span>
              <XCircle className="w-3.5 h-3.5 text-rose-400" />
            </div>
            <div className="mt-2">
              <div
                className={`text-xl font-bold font-mono ${
                  metrics.failed > 0 ? 'text-rose-300' : 'text-slate-400'
                }`}
              >
                {metrics.failed.toLocaleString()}
              </div>
              <div className="text-[10px] text-rose-400/80 font-sans truncate mt-0.5">
                {metrics.failed > 0 ? 'Defect triage needed' : 'Zero failures'}
              </div>
            </div>
          </div>

          {/* Blocked */}
          <div className="p-3 rounded-lg bg-[#0B1120] border border-purple-900/50 bg-purple-950/10 hover:border-purple-700/60 transition-all shadow-sm flex flex-col justify-between">
            <div className="flex items-center justify-between text-purple-400">
              <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                Blocked
              </span>
              <Ban className="w-3.5 h-3.5 text-purple-400" />
            </div>
            <div className="mt-2">
              <div className="text-xl font-bold font-mono text-purple-300">
                {metrics.blocked.toLocaleString()}
              </div>
              <div className="text-[10px] text-purple-400/80 font-sans truncate mt-0.5">
                Dependency blocked
              </div>
            </div>
          </div>

          {/* Needs Review */}
          <div
            className={`p-3 rounded-lg bg-[#0B1120] border ${
              metrics.needsReview > 0
                ? 'border-amber-900/60 bg-amber-950/20'
                : 'border-slate-800'
            } hover:border-amber-700/60 transition-all shadow-sm flex flex-col justify-between`}
          >
            <div className="flex items-center justify-between text-amber-400">
              <span className="text-[10px] uppercase font-mono font-medium tracking-wider">
                Needs Review
              </span>
              <HelpCircle className="w-3.5 h-3.5 text-amber-400" />
            </div>
            <div className="mt-2">
              <div
                className={`text-xl font-bold font-mono ${
                  metrics.needsReview > 0 ? 'text-amber-300' : 'text-slate-400'
                }`}
              >
                {metrics.needsReview.toLocaleString()}
              </div>
              <div className="text-[10px] text-amber-400/80 font-sans truncate mt-0.5">
                AI anomaly check
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* 3. Middle Section: Current Project Details + Futuristic AI Orb */}
      <section
        id="dashboard-middle-section"
        className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-stretch"
      >
        {/* Left: Current Project Detail (7 Cols on desktop) */}
        <div
          id="current-project-detail-panel"
          className="lg:col-span-7 p-4 rounded-lg bg-[#0B1120] border border-slate-800 flex flex-col justify-between space-y-4"
        >
          {/* Project Title & Metadata Header */}
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-slate-800/80">
            <div className="space-y-1">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-[10px] font-mono text-indigo-400 uppercase font-bold bg-indigo-950/60 px-1.5 py-0.5 rounded border border-indigo-800/60">
                  PROJECT SPEC
                </span>
                <h2 className="text-sm font-bold text-slate-100 font-sans">
                  {selectedProject.name}
                </h2>
                {getStatusBadge(selectedProject.status)}
              </div>
              <div className="flex items-center gap-3 text-[11px] font-mono text-slate-400">
                <span className="flex items-center gap-1">
                  <Server className="w-3 h-3 text-slate-500" />
                  Env: <strong className="text-slate-200">{selectedProject.environment}</strong>
                </span>
                <span>•</span>
                <span className="flex items-center gap-1">
                  <Clock className="w-3 h-3 text-slate-500" />
                  Slug: <strong className="text-slate-300">{selectedProject.slug}</strong>
                </span>
              </div>
            </div>

            {/* Quick Action Button to Execution Hub */}
            <button
              onClick={() => navigate('/execution')}
              className="flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300 font-medium font-mono cursor-pointer self-start sm:self-center"
            >
              <span>Execution Hub</span>
              <ArrowUpRight className="w-3.5 h-3.5" />
            </button>
          </div>

          {/* Last Activity Record */}
          <div className="p-2.5 rounded bg-[#020617] border border-slate-800/90 text-xs flex items-center justify-between">
            <div className="flex items-center gap-2 truncate">
              <Radio className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span className="font-mono text-slate-400 text-[11px]">Last Activity:</span>
              <span className="font-medium text-slate-200 truncate">
                {selectedProject.lastActivity}
              </span>
            </div>
            <span className="text-[10px] font-mono text-slate-500 shrink-0 ml-2">TELEMETRY SYNCED</span>
          </div>

          {/* Current Execution Run Status */}
          <div className="space-y-2">
            <div className="flex items-center justify-between text-[11px] font-mono">
              <span className="font-bold text-slate-300 uppercase tracking-wider">
                Current Execution Run
              </span>
              {activeRun ? (
                <span className="text-cyan-400 font-semibold">{activeRun.id}</span>
              ) : (
                <span className="text-slate-500">IDLE</span>
              )}
            </div>

            {activeRun ? (
              <div className="p-3 rounded-lg bg-gradient-to-r from-cyan-950/20 via-[#020617] to-indigo-950/20 border border-cyan-800/60 space-y-2.5">
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-1.5">
                  <div className="font-bold text-xs text-slate-100 font-sans">
                    {activeRun.title}
                  </div>
                  <div className="flex items-center gap-2 text-[10px] font-mono text-cyan-400 shrink-0">
                    <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-ping" />
                    <span>{activeRun.status}</span>
                  </div>
                </div>

                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-[10px] font-mono text-slate-400">
                  <div className="p-1.5 rounded bg-[#0B1120] border border-slate-800">
                    <span className="text-slate-500">Started:</span> {activeRun.startedAt}
                  </div>
                  <div className="p-1.5 rounded bg-[#0B1120] border border-slate-800">
                    <span className="text-slate-500">Est. Duration:</span> {activeRun.durationEstimate}
                  </div>
                  <div className="p-1.5 rounded bg-[#0B1120] border border-slate-800">
                    <span className="text-slate-500">Executed:</span> {activeRun.testsExecuted} / {activeRun.totalTests}
                  </div>
                  <div className="p-1.5 rounded bg-[#0B1120] border border-slate-800">
                    <span className="text-slate-500">Progress:</span> {activeRun.progressPct}%
                  </div>
                </div>

                {/* Animated Progress Bar */}
                <div className="space-y-1">
                  <div className="flex justify-between text-[10px] font-mono text-slate-400">
                    <span>Execution Throughput</span>
                    <span className="text-cyan-300 font-bold">{activeRun.progressPct}%</span>
                  </div>
                  <div className="h-1.5 w-full bg-slate-800 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-gradient-to-r from-cyan-400 to-indigo-500 rounded-full transition-all duration-500"
                      style={{ width: `${activeRun.progressPct}%` }}
                    />
                  </div>
                </div>
              </div>
            ) : (
              <div className="p-4 rounded-lg bg-[#020617] border border-slate-800/80 text-center space-y-2">
                <div className="text-xs font-semibold text-slate-400 font-mono">
                  No Active Execution Running
                </div>
                <p className="text-[10px] text-slate-500 max-w-sm mx-auto">
                  All test suites are currently in standby. Launch an orchestration run to stream live diagnostics.
                </p>
                <button
                  onClick={() => setNewRunModalOpen(true)}
                  className="px-3 py-1 rounded bg-indigo-950 hover:bg-indigo-900 border border-indigo-700/60 text-indigo-300 text-xs font-mono font-medium cursor-pointer transition-colors"
                >
                  Start Execution Run
                </button>
              </div>
            )}
          </div>
        </div>

        {/* Right: Futuristic AI Orb Centerpiece (5 Cols on desktop) */}
        <div id="ai-quality-core-orb-column" className="lg:col-span-5 flex flex-col">
          <AIQualityCoreOrb
            status={selectedProject.status}
            hasActiveRun={!!activeRun}
            projectName={selectedProject.name}
          />
        </div>
      </section>

      {/* 4. Real-Time Project Status Section */}
      <section id="dashboard-project-status-section" className="space-y-2">
        <div className="flex items-center justify-between px-1">
          <div>
            <h2 className="text-xs font-bold text-slate-100 font-mono uppercase tracking-wider flex items-center gap-1.5">
              <Activity className="w-3.5 h-3.5 text-cyan-400" />
              <span>Project Status</span>
            </h2>
            <p className="text-[10px] text-slate-400 font-sans">
              Live execution and quality state
            </p>
          </div>
          <span className="text-[9px] font-mono text-emerald-400 bg-emerald-950/60 px-2 py-0.5 rounded border border-emerald-800/60 uppercase">
            LIVE TELEMETRY
          </span>
        </div>

        {/* 2-Column Grid: Donut Chart + Execution Activity Graph */}
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-stretch">
          {/* Donut Chart (5 Cols) */}
          <div className="lg:col-span-5">
            <ProjectStatusDonut
              metrics={metrics}
              projectName={selectedProject.name}
            />
          </div>

          {/* Execution Activity Graph (7 Cols) */}
          <div className="lg:col-span-7">
            <ProjectExecutionGraph
              timelineData={selectedProject.timelineData}
              projectName={selectedProject.name}
            />
          </div>
        </div>
      </section>
    </div>
  );
};
