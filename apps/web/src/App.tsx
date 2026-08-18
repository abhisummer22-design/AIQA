/**
 * AIQA Web Shell & Quality Engineering Control Plane
 *
 * Official frontend source of truth for the AIQA platform.
 * Real backend integrations (Runs, Realtime WS, Evidence, Intelligence, Bug Drafts, Audit, Settings)
 * are preserved and connected. Staged specifications feature full UI structure with BACKEND API REQUIRED
 * notifications and zero fabricated data.
 */

import React, { useState, useEffect } from "react";
import { Header } from "./components/layout/Header";
import { Sidebar } from "./components/layout/Sidebar";
import { CommandPalette } from "./components/layout/CommandPalette";
import { RunTriggerModal } from "./components/layout/RunTriggerModal";
import { ConnectionBanner } from "./components/common/ConnectionBanner";

// Real Backend-Connected Views
import { DashboardView } from "./views/DashboardView";
import { RunsView } from "./views/RunsView";
import { RunDetailView } from "./views/RunDetailView";
import { IntelligenceView } from "./views/IntelligenceView";
import { BugDraftsView } from "./views/BugDraftsView";
import { AuditLogView } from "./views/AuditLogView";
import { SettingsView } from "./views/SettingsView";

// Staged Workspace Views (Contract Skeletons)
import { ProjectsView } from "./views/staged/ProjectsView";
import { QualityGraphView } from "./views/staged/QualityGraphView";
import { TestGeneratorView } from "./views/staged/TestGeneratorView";
import { TestValidatorView } from "./views/staged/TestValidatorView";
import { TestSuitesView } from "./views/staged/TestSuitesView";
import { OrchestratorPlanView } from "./views/staged/OrchestratorPlanView";
import { RBACTestingView } from "./views/staged/RBACTestingView";
import { APITestingView } from "./views/staged/APITestingView";
import { MobileTestingView } from "./views/staged/MobileTestingView";
import { CustomTestingView } from "./views/staged/CustomTestingView";
import { ReportsView } from "./views/staged/ReportsView";
import { ModelGatewayView } from "./views/staged/ModelGatewayView";

export type ViewId =
  | "dashboard"
  | "runs"
  | "intelligence"
  | "bug-drafts"
  | "audit-log"
  | "settings"
  | "projects"
  | "quality-graph"
  | "test-generator"
  | "test-validator"
  | "test-suites"
  | "orchestrator-plan"
  | "rbac"
  | "api-testing"
  | "mobile-testing"
  | "custom-testing"
  | "reports"
  | "model-gateway";

export function App() {
  const [view, setView] = useState<ViewId>("dashboard");
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState<boolean>(false);
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState<boolean>(false);
  const [isNewRunModalOpen, setIsNewRunModalOpen] = useState<boolean>(false);

  // Keyboard shortcut listeners (⌘B / Ctrl+B for Sidebar, ⌘K for Command Palette)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setIsSidebarCollapsed((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const openRun = (runId: string) => {
    setSelectedRunId(runId);
    setView("runs");
  };

  const handleRunCreated = (runId: string) => {
    setIsNewRunModalOpen(false);
    openRun(runId);
  };

  const renderView = () => {
    if (view === "runs" && selectedRunId) {
      return <RunDetailView runId={selectedRunId} onBack={() => setSelectedRunId(null)} />;
    }

    switch (view) {
      // Live Connected Views
      case "dashboard":
        return (
          <DashboardView
            onOpenRun={openRun}
            onNavigate={(v) => {
              setSelectedRunId(null);
              setView(v);
            }}
            onOpenNewRunModal={() => setIsNewRunModalOpen(true)}
          />
        );
      case "runs":
        return (
          <RunsView
            onOpenRun={openRun}
            onOpenNewRunModal={() => setIsNewRunModalOpen(true)}
          />
        );
      case "intelligence":
        return <IntelligenceView />;
      case "bug-drafts":
        return <BugDraftsView />;
      case "audit-log":
        return <AuditLogView />;
      case "settings":
        return <SettingsView />;

      // Staged Specification Views
      case "projects":
        return <ProjectsView />;
      case "quality-graph":
        return <QualityGraphView />;
      case "test-generator":
        return <TestGeneratorView />;
      case "test-validator":
        return <TestValidatorView />;
      case "test-suites":
        return <TestSuitesView />;
      case "orchestrator-plan":
        return <OrchestratorPlanView />;
      case "rbac":
        return <RBACTestingView />;
      case "api-testing":
        return <APITestingView />;
      case "mobile-testing":
        return <MobileTestingView />;
      case "custom-testing":
        return <CustomTestingView />;
      case "reports":
        return <ReportsView />;
      case "model-gateway":
        return <ModelGatewayView />;

      default:
        return (
          <DashboardView
            onOpenRun={openRun}
            onNavigate={(v) => {
              setSelectedRunId(null);
              setView(v);
            }}
            onOpenNewRunModal={() => setIsNewRunModalOpen(true)}
          />
        );
    }
  };

  return (
    <div className="flex min-h-screen bg-[#070b14] font-sans text-slate-300">
      {/* Sidebar Navigation */}
      <Sidebar
        currentView={view}
        onSelectView={(v) => {
          setSelectedRunId(null);
          setView(v);
        }}
        isCollapsed={isSidebarCollapsed}
        onToggleCollapse={() => setIsSidebarCollapsed((prev) => !prev)}
      />

      {/* Main Content Area */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Application Top Header */}
        <Header
          onToggleSidebar={() => setIsSidebarCollapsed((prev) => !prev)}
          onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
          onOpenNewRunModal={() => setIsNewRunModalOpen(true)}
          onOpenSettings={() => {
            setSelectedRunId(null);
            setView("settings");
          }}
        />

        {/* Global Connection / Offline Banner */}
        <ConnectionBanner onOpenSettings={() => setView("settings")} />

        {/* Dynamic Workspace Container */}
        <main className="mx-auto w-full max-w-[1540px] flex-1 p-4 sm:p-5">
          {renderView()}
        </main>
      </div>

      {/* Global Command Palette (⌘K) */}
      <CommandPalette
        isOpen={isCommandPaletteOpen}
        onClose={() => setIsCommandPaletteOpen(false)}
        onSelectView={(v) => {
          setSelectedRunId(null);
          setView(v);
        }}
        onOpenNewRunModal={() => setIsNewRunModalOpen(true)}
      />

      {/* Real Run Execution Trigger Modal */}
      <RunTriggerModal
        isOpen={isNewRunModalOpen}
        onClose={() => setIsNewRunModalOpen(false)}
        onRunCreated={handleRunCreated}
      />
    </div>
  );
}

export default App;
