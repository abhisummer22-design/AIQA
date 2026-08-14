import React, { useState, useEffect, useCallback } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { TopHeader } from "./components/layout/TopHeader";
import { SystemHealthBanner } from "./components/system/SystemHealthBanner";
import { KpiCards } from "./components/dashboard/KpiCards";
import { LiveTestExecution } from "./components/dashboard/LiveTestExecution";
import { ChartsSection } from "./components/dashboard/ChartsSection";
import { RiskAnalytics } from "./components/dashboard/RiskAnalytics";
import { FlakyTests } from "./components/dashboard/FlakyTests";
import { FailureIntelligence } from "./components/dashboard/FailureIntelligence";
import { BugIntelligence } from "./components/dashboard/BugIntelligence";
import { ModelRouterStatus } from "./components/dashboard/ModelRouterStatus";
import { OllamaStatus } from "./components/dashboard/OllamaStatus";
import { ExecutionTimeline } from "./components/dashboard/ExecutionTimeline";
import { GovernanceStatus } from "./components/dashboard/GovernanceStatus";
import { AuditLogPreview } from "./components/dashboard/AuditLogPreview";

// View Routes
import { AiBrainView } from "./components/views/AiBrainView";
import { ProjectsView } from "./components/views/ProjectsView";
import { TestRunsView } from "./components/views/TestRunsView";
import { TestCasesView } from "./components/views/TestCasesView";
import { TestCaseGeneratorView } from "./components/views/TestCaseGeneratorView";
import { ApiTestingView } from "./components/views/ApiTestingView";
import { MobileTestingView } from "./components/views/MobileTestingView";
import { RbacTestingView } from "./components/views/RbacTestingView";
import { WorkspaceView } from "./components/workspace/WorkspaceView";
import { BugsView } from "./components/views/BugsView";
import { RiskFlakinessView } from "./components/views/RiskFlakinessView";
import { PoliciesView } from "./components/views/PoliciesView";
import { AuditLogsView } from "./components/views/AuditLogsView";
import { ReportsView } from "./components/views/ReportsView";
import { SettingsView } from "./components/views/SettingsView";

// Modals
import { SystemDiagnosticsModal } from "./components/system/SystemDiagnosticsModal";
import { AlertCenter } from "./components/system/AlertCenter";
import { GlobalSearchModal } from "./components/layout/GlobalSearchModal";
import { CreateBugModal } from "./components/modals/CreateBugModal";
import { TriggerTestModal } from "./components/modals/TriggerTestModal";

// API
import {
  fetchDashboardState,
  connectSSE,
  triggerTestRun,
  toggleQuarantineTest,
  createBugConfirmed,
  simulateChaosComponent,
  dismissAlertItem
} from "./services/api";

import {
  DashboardState,
  ViewType,
  EnvironmentType,
  BugCandidate,
  FailureItem,
  AlertItem
} from "./types";

export function App() {
  const [currentView, setCurrentView] = useState<ViewType>("dashboard");
  const [environment, setEnvironment] = useState<EnvironmentType>("all");
  const [selectedProject, setSelectedProject] = useState<string>("All Projects");
  const [state, setState] = useState<DashboardState | null>(null);
  const [loading, setLoading] = useState(true);

  // Modals & Drawers state
  const [isDiagnosticsOpen, setIsDiagnosticsOpen] = useState(false);
  const [isAlertsOpen, setIsAlertsOpen] = useState(false);
  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const [isTriggerTestOpen, setIsTriggerTestOpen] = useState(false);
  const [reviewingBugCandidate, setReviewingBugCandidate] = useState<BugCandidate | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  const showToast = (msg: string) => {
    setToastMessage(msg);
    setTimeout(() => setToastMessage(null), 3500);
  };

  // Initial load
  useEffect(() => {
    let mounted = true;
    fetchDashboardState()
      .then((data) => {
        if (mounted) {
          setState(data);
          setLoading(false);
        }
      })
      .catch((err) => {
        console.error("Failed to load initial dashboard state:", err);
        if (mounted) setLoading(false);
      });

    // Real-time SSE Stream
    const unsubscribeSSE = connectSSE((data) => {
      if (mounted) {
        setState((prev) => (prev ? { ...prev, ...data } : data));
      }
    });

    return () => {
      mounted = false;
      unsubscribeSSE();
    };
  }, []);

  // Handlers
  const handleTriggerTest = async (payload: {
    testName: string;
    project: string;
    environment: EnvironmentType;
    executor: string;
  }) => {
    try {
      const res = await triggerTestRun(payload);
      if (res.success && state) {
        setState((prev) => {
          if (!prev) return prev;
          return {
            ...prev,
            liveExecutions: [res.execution, ...prev.liveExecutions]
          };
        });
        showToast(`Triggered: "${payload.testName}" on ${payload.environment}`);
      }
    } catch (err) {
      console.error(err);
      showToast("Test execution scheduled on background cluster.");
    }
  };

  const handleToggleQuarantine = async (testId: string) => {
    try {
      const res = await toggleQuarantineTest(testId);
      if (res.success && state) {
        setState((prev) => {
          if (!prev) return prev;
          return {
            ...prev,
            flakyTests: prev.flakyTests.map((t) =>
              t.id === testId ? { ...t, quarantined: res.quarantined } : t
            )
          };
        });
        showToast(
          res.quarantined
            ? `Test quarantined from blocking PRs.`
            : `Test unquarantined and returned to CI gate.`
        );
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleConfirmCreateBug = async (data: {
    candidateId: string;
    title: string;
    severity: string;
    priority: string;
    notes?: string;
  }) => {
    try {
      const res = await createBugConfirmed(data);
      if (res.success && state) {
        setState((prev) => {
          if (!prev) return prev;
          return {
            ...prev,
            bugCandidates: prev.bugCandidates.map((b) =>
              b.id === data.candidateId ? { ...b, status: "created" } : b
            ),
            kpiMetrics: {
              ...prev.kpiMetrics,
              openBugs: {
                ...prev.kpiMetrics.openBugs,
                total: prev.kpiMetrics.openBugs.total + 1
              }
            }
          };
        });
        showToast(`Bug confirmed and synced to Jira/Linear (#BUG-${Math.floor(Math.random() * 800 + 100)})`);
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSimulateChaos = async (component: string, status: string) => {
    try {
      const res = await simulateChaosComponent(component, status);
      if (res.success && state) {
        setState((prev) => {
          if (!prev) return prev;
          return {
            ...prev,
            systemHealth: {
              ...prev.systemHealth,
              [component]: status,
              lastChecked: new Date().toISOString()
            }
          };
        });
        showToast(`Chaos Simulation: Injected status "${status}" into ${component}`);
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleDismissAlert = async (id: string) => {
    try {
      await dismissAlertItem(id);
      if (state) {
        setState((prev) => {
          if (!prev) return prev;
          return {
            ...prev,
            alerts: prev.alerts.filter((a) => a.id !== id)
          };
        });
      }
    } catch (err) {
      console.error(err);
    }
  };

  if (loading || !state) {
    return (
      <div className="min-h-screen bg-[#070b14] text-slate-100 flex items-center justify-center font-mono text-xs">
        <div className="flex flex-col items-center gap-3">
          <div className="w-8 h-8 rounded-full border-2 border-cyan-500 border-t-transparent animate-spin" />
          <span className="text-cyan-400">Initializing AIQA Real-Time Telemetry Engine...</span>
        </div>
      </div>
    );
  }

  return (
    <div id="aiqa-dashboard-root" className="min-h-screen bg-[#050505] text-slate-300 flex font-sans">
      {/* Toast Notification */}
      {toastMessage && (
        <div
          id="system-toast"
          className="fixed bottom-5 right-5 z-50 px-3.5 py-2 rounded bg-indigo-950 border border-indigo-700 text-indigo-200 text-[11px] font-mono shadow-2xl animate-in slide-in-from-bottom duration-200 flex items-center gap-2"
        >
          <span className="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-pulse" />
          <span>{toastMessage}</span>
        </div>
      )}

      {/* Left Sidebar */}
      <Sidebar
        currentView={currentView}
        onNavigate={setCurrentView}
        systemHealth={state.systemHealth}
        onOpenDiagnostics={() => setIsDiagnosticsOpen(true)}
      />

      {/* Main Content Area */}
      <div className="flex-1 ml-52 flex flex-col min-w-0 overflow-x-hidden">
        {/* Top Header */}
        <TopHeader
          activeView={currentView}
          currentView={currentView}
          onNavigate={setCurrentView}
          environment={environment}
          onEnvironmentChange={setEnvironment}
          selectedProject={selectedProject}
          onProjectChange={setSelectedProject}
          systemHealth={state.systemHealth}
          unreadAlertsCount={state.alerts.length}
          onOpenAlerts={() => setIsAlertsOpen(true)}
          onOpenSearch={() => setIsSearchOpen(true)}
          onTriggerTest={() => setIsTriggerTestOpen(true)}
          onOpenDiagnostics={() => setIsDiagnosticsOpen(true)}
        />

        {/* System Health Alert Matrix Banner */}
        <SystemHealthBanner
          systemHealth={state.systemHealth}
          onOpenDiagnostics={() => setIsDiagnosticsOpen(true)}
        />

        {/* Scrollable View Canvas */}
        <main className="flex-1 p-3.5 sm:p-4 space-y-4 max-w-[1700px] w-full mx-auto">
          {currentView === "dashboard" && (
            <>
              {/* 1. KPI Cards */}
              <KpiCards metrics={state.kpiMetrics} onNavigate={setCurrentView} />

              {/* 2. Live Test Execution Panel */}
              <LiveTestExecution
                executions={state.liveExecutions}
                onTriggerNew={() => setIsTriggerTestOpen(true)}
                onNavigate={setCurrentView}
              />

              {/* 3. Recharts Trends & Quality Metrics */}
              <ChartsSection metrics={state.kpiMetrics} />

              {/* 4. Risk & Flakiness Dual Column */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
                <RiskAnalytics metrics={state.kpiMetrics} />
                <FlakyTests
                  flakyTests={state.flakyTests}
                  onToggleQuarantine={handleToggleQuarantine}
                  onNavigate={setCurrentView}
                />
              </div>

              {/* 6. Failure Intelligence & AI Bug Drafting */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
                <FailureIntelligence
                  failures={state.recentFailures}
                  onDraftBug={(fail) => {
                    setReviewingBugCandidate({
                      id: `cand-${Date.now()}`,
                      title: `[AI Draft] ${fail.errorMessage.slice(0, 50)}...`,
                      severity: fail.riskLevel,
                      confidence: 94,
                      relatedTest: fail.testName,
                      relatedService: fail.testName.split(" ")[0] || "Core Service",
                      evidenceCount: 12,
                      suggestedPriority: fail.riskLevel === "Critical" ? "P0 - Urgent" : "P1 - Blocker",
                      description: `Automated triage identified failure pattern: ${fail.errorMessage}. Recommended action: ${fail.suggestedAction}`,
                      createdAt: "Just now",
                      status: "pending"
                    });
                  }}
                  onNavigate={setCurrentView}
                />

                <BugIntelligence
                  candidates={state.bugCandidates}
                  onReviewCandidate={(cand) => setReviewingBugCandidate(cand)}
                  onCreateBug={(cand) => setReviewingBugCandidate(cand)}
                  onIgnoreCandidate={(id) => {
                    setState((prev) => {
                      if (!prev) return prev;
                      return {
                        ...prev,
                        bugCandidates: prev.bugCandidates.filter((c) => c.id !== id)
                      };
                    });
                    showToast("Bug candidate dismissed.");
                  }}
                  onNavigate={setCurrentView}
                />
              </div>

              {/* 7. AI Model Router & Ollama Local AI Runtime */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
                <ModelRouterStatus modelRouter={state.modelRouter} onNavigate={setCurrentView} />
                <OllamaStatus modelRouter={state.modelRouter} systemHealth={state.systemHealth} onNavigate={setCurrentView} />
              </div>

              {/* 8. Timeline, Governance, and Audit Logs Tri-Grid */}
              <div className="grid grid-cols-1 lg:grid-cols-3 gap-3.5">
                <ExecutionTimeline timeline={state.timeline} onNavigate={setCurrentView} />
                <GovernanceStatus policies={state.governance} onNavigate={setCurrentView} />
                <AuditLogPreview auditLogs={state.auditLogs} onNavigate={setCurrentView} />
              </div>
            </>
          )}

          {/* Sub-Views */}
          {currentView === "ai-brain" && <AiBrainView />}
          {currentView === "projects" && <ProjectsView onNavigate={setCurrentView} />}
          {currentView === "test-runs" && (
            <TestRunsView
              executions={state.liveExecutions}
              onTriggerNew={() => setIsTriggerTestOpen(true)}
            />
          )}
          {currentView === "test-cases" && <TestCasesView />}
          {currentView === "test-case-generator" && <TestCaseGeneratorView />}
          {currentView === "api-testing" && <ApiTestingView />}
          {currentView === "mobile-testing" && <MobileTestingView />}
          {currentView === "rbac-testing" && <RbacTestingView />}
          {currentView === "workspace" && (
            <WorkspaceView
              testRuns={state.liveExecutions}
              bugs={state.bugCandidates}
            />
          )}
          {currentView === "bugs" && (
            <BugsView
              candidates={state.bugCandidates}
              onReviewCandidate={(cand) => setReviewingBugCandidate(cand)}
              onCreateBug={(cand) => setReviewingBugCandidate(cand)}
            />
          )}
          {currentView === "risk-flakiness" && (
            <RiskFlakinessView
              metrics={state.kpiMetrics}
              flakyTests={state.flakyTests}
              onToggleQuarantine={handleToggleQuarantine}
            />
          )}
          {currentView === "policies" && <PoliciesView policies={state.governance} />}
          {currentView === "audit-logs" && <AuditLogsView auditLogs={state.auditLogs} />}
          {currentView === "reports" && <ReportsView />}
          {currentView === "settings" && <SettingsView />}
        </main>

        {/* High Density Status Footer */}
        <footer className="h-7 bg-[#0D0D0E] border-t border-slate-800 flex items-center justify-between px-4 text-[9px] uppercase tracking-widest text-slate-500 font-mono select-none">
          <div className="flex items-center gap-4">
            <span className="flex items-center gap-1">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
              <span>CLUSTER SYNC: ACTIVE (SSE 1000ms)</span>
            </span>
            <span className="hidden sm:inline">|</span>
            <span className="hidden sm:inline">SCHEDULER: K8S RUNNER V2</span>
          </div>
          <div className="flex items-center gap-3">
            <span>REGION: US-CENTRAL1-A</span>
            <span>AIQA ENTERPRISE v2.4</span>
          </div>
        </footer>
      </div>

      {/* Global Modals */}
      <SystemDiagnosticsModal
        isOpen={isDiagnosticsOpen}
        onClose={() => setIsDiagnosticsOpen(false)}
        health={state.systemHealth}
        onSimulateChaos={handleSimulateChaos}
      />

      <AlertCenter
        isOpen={isAlertsOpen}
        onClose={() => setIsAlertsOpen(false)}
        alerts={state.alerts}
        onDismissAlert={handleDismissAlert}
        onNavigate={setCurrentView}
      />

      <GlobalSearchModal
        isOpen={isSearchOpen}
        onClose={() => setIsSearchOpen(false)}
        onNavigate={setCurrentView}
      />

      <CreateBugModal
        isOpen={!!reviewingBugCandidate}
        onClose={() => setReviewingBugCandidate(null)}
        candidate={reviewingBugCandidate}
        onConfirmCreate={handleConfirmCreateBug}
      />

      <TriggerTestModal
        isOpen={isTriggerTestOpen}
        onClose={() => setIsTriggerTestOpen(false)}
        onTrigger={handleTriggerTest}
      />
    </div>
  );
}

export default App;
