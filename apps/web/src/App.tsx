/**
 * AIQA web shell.
 *
 * Every view reachable from here either renders data the Rust orchestrator
 * actually returned, or states plainly that the backend capability does not
 * exist. There is no fallback dataset anywhere in this app.
 *
 * Navigation is split accordingly:
 *   "Live"          — backed by real orchestrator endpoints
 *   "Not available" — the UI concept exists in the product design, but no
 *                     backend capability does; these render NotImplemented
 *                     rather than sample content.
 */

import React, { useState } from "react";
import {
  Activity,
  Ban,
  Bug,
  Brain,
  FileSearch,
  Gauge,
  KeyRound,
  ListChecks,
  ScrollText,
  Settings as SettingsIcon,
  ShieldCheck,
  Smartphone,
  TestTube2,
  Users,
  Workflow,
} from "lucide-react";

import { DashboardView } from "./views/DashboardView";
import { RunsView } from "./views/RunsView";
import { RunDetailView } from "./views/RunDetailView";
import { IntelligenceView } from "./views/IntelligenceView";
import { BugDraftsView } from "./views/BugDraftsView";
import { AuditLogView } from "./views/AuditLogView";
import { SettingsView } from "./views/SettingsView";
import { NotImplementedState } from "./components/common/StateViews";
import { ConnectionBanner } from "./components/common/ConnectionBanner";

export type ViewId =
  | "dashboard"
  | "runs"
  | "intelligence"
  | "bug-drafts"
  | "audit-log"
  | "settings"
  | "test-cases"
  | "test-generator"
  | "api-testing"
  | "mobile-testing"
  | "rbac"
  | "ai-brain"
  | "quality-graph"
  | "policies";

interface NavItem {
  id: ViewId;
  label: string;
  icon: React.ReactNode;
}

/** Views with a real backend endpoint behind them. */
const LIVE_NAV: NavItem[] = [
  { id: "dashboard", label: "Dashboard", icon: <Gauge size={15} /> },
  { id: "runs", label: "Runs", icon: <Workflow size={15} /> },
  { id: "intelligence", label: "Intelligence", icon: <Activity size={15} /> },
  { id: "bug-drafts", label: "Bug Drafts", icon: <Bug size={15} /> },
  { id: "audit-log", label: "Audit Log", icon: <ScrollText size={15} /> },
  { id: "settings", label: "Settings", icon: <SettingsIcon size={15} /> },
];

/**
 * Views with no backend capability. Kept in the navigation deliberately —
 * hiding them would make the product's gaps invisible; each one states what
 * would have to exist for it to work.
 */
const UNAVAILABLE_NAV: NavItem[] = [
  { id: "test-cases", label: "Test Cases", icon: <ListChecks size={15} /> },
  { id: "test-generator", label: "Test Generator", icon: <TestTube2 size={15} /> },
  { id: "api-testing", label: "API Testing", icon: <FileSearch size={15} /> },
  { id: "mobile-testing", label: "Mobile Testing", icon: <Smartphone size={15} /> },
  { id: "rbac", label: "RBAC", icon: <Users size={15} /> },
  { id: "ai-brain", label: "AI Brain", icon: <Brain size={15} /> },
  { id: "quality-graph", label: "Quality Graph", icon: <Workflow size={15} /> },
  { id: "policies", label: "Policies", icon: <ShieldCheck size={15} /> },
];

/** What each unavailable view would require from the backend. Written to be
 *  actionable: these are the real gaps, not marketing copy. */
const MISSING_CAPABILITY: Record<string, { feature: string; requires: string }> = {
  "test-cases": {
    feature: "Test case management",
    requires: "a test_cases table and CRUD endpoints on the orchestrator — the run plan is currently submitted whole via POST /runs, and there is no persisted test-case catalogue",
  },
  "test-generator": {
    feature: "AI test generation",
    requires: "an LLM-backed generation endpoint; the orchestrator has no model integration of any kind",
  },
  "api-testing": {
    feature: "API test authoring",
    requires: "endpoints to manage Newman collections and Schemathesis specs; the executors exist but are only reachable by submitting a run plan",
  },
  "mobile-testing": {
    feature: "Mobile test authoring",
    requires: "device-pool and Maestro flow management endpoints; the Maestro executor exists but has no management API",
  },
  rbac: {
    feature: "User and role management",
    requires: "a users table and role-assignment endpoints. The orchestrator authenticates API keys with three fixed roles (viewer/operator/admin) — it has no concept of users, and keys are managed under Settings",
  },
  "ai-brain": {
    feature: "Model routing and AI telemetry",
    requires: "an inference service plus routing configuration endpoints; no LLM or model router exists in the backend",
  },
  "quality-graph": {
    feature: "Quality graph",
    requires: "a graph query endpoint. The 0002 migration defines quality-graph tables, but no HTTP route exposes them",
  },
  policies: {
    feature: "Policy administration",
    requires: "CRUD endpoints for policy rules. policy.rs evaluates rules from environment configuration at run-creation time, but exposes no route to read or edit them",
  },
};

export function App() {
  const [view, setView] = useState<ViewId>("dashboard");
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);

  const openRun = (runId: string) => {
    setSelectedRunId(runId);
    setView("runs");
  };

  const renderView = () => {
    if (view === "runs" && selectedRunId) {
      return <RunDetailView runId={selectedRunId} onBack={() => setSelectedRunId(null)} />;
    }

    switch (view) {
      case "dashboard":
        return <DashboardView onOpenRun={openRun} onNavigate={setView} />;
      case "runs":
        return <RunsView onOpenRun={openRun} />;
      case "intelligence":
        return <IntelligenceView />;
      case "bug-drafts":
        return <BugDraftsView />;
      case "audit-log":
        return <AuditLogView />;
      case "settings":
        return <SettingsView />;
      default: {
        const gap = MISSING_CAPABILITY[view];
        return (
          <div className="mx-auto max-w-3xl pt-10">
            <NotImplementedState feature={gap.feature} requires={gap.requires} />
          </div>
        );
      }
    }
  };

  return (
    <div className="flex min-h-screen bg-[#070b14] font-sans text-slate-300">
      {/* Sidebar */}
      <aside className="fixed inset-y-0 left-0 flex w-52 flex-col border-r border-slate-800 bg-[#0a0f1a]">
        <div className="flex h-14 items-center gap-2 border-b border-slate-800 px-4">
          <div className="flex h-7 w-7 items-center justify-center rounded bg-cyan-500/15 text-cyan-400">
            <ShieldCheck size={16} />
          </div>
          <div className="leading-tight">
            <div className="text-sm font-bold tracking-tight text-slate-100">AIQA</div>
            <div className="font-mono text-[9px] uppercase tracking-widest text-slate-500">Orchestrator UI</div>
          </div>
        </div>

        <nav className="flex-1 overflow-y-auto px-2 py-3">
          <SectionLabel>Live</SectionLabel>
          {LIVE_NAV.map((item) => (
            <NavButton
              key={item.id}
              item={item}
              active={view === item.id}
              onClick={() => {
                setSelectedRunId(null);
                setView(item.id);
              }}
            />
          ))}

          <SectionLabel className="mt-4">
            No backend capability
          </SectionLabel>
          {UNAVAILABLE_NAV.map((item) => (
            <NavButton
              key={item.id}
              item={item}
              active={view === item.id}
              muted
              onClick={() => {
                setSelectedRunId(null);
                setView(item.id);
              }}
            />
          ))}
        </nav>

        <div className="border-t border-slate-800 px-3 py-2 font-mono text-[9px] leading-relaxed text-slate-600">
          Data source: Rust orchestrator only.
          <br />
          No sample or generated values.
        </div>
      </aside>

      {/* Main */}
      <div className="ml-52 flex min-w-0 flex-1 flex-col">
        <ConnectionBanner onOpenSettings={() => setView("settings")} />
        <main className="mx-auto w-full max-w-[1500px] flex-1 p-4">{renderView()}</main>
      </div>
    </div>
  );
}

function SectionLabel({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return (
    <div className={`px-2 pb-1.5 font-mono text-[9px] uppercase tracking-widest text-slate-600 ${className}`}>
      {children}
    </div>
  );
}

function NavButton({
  item,
  active,
  muted = false,
  onClick,
}: {
  item: NavItem;
  active: boolean;
  muted?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`mb-0.5 flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition ${
        active
          ? "bg-cyan-500/10 text-cyan-300"
          : muted
            ? "text-slate-600 hover:bg-slate-800/40 hover:text-slate-400"
            : "text-slate-400 hover:bg-slate-800/40 hover:text-slate-200"
      }`}
    >
      <span className="shrink-0 opacity-80">{item.icon}</span>
      <span className="truncate">{item.label}</span>
      {muted && <Ban size={11} className="ml-auto shrink-0 opacity-50" />}
    </button>
  );
}

export default App;
