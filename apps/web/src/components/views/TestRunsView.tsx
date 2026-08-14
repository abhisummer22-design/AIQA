import React, { useState, useEffect, useMemo, useCallback } from "react";
import {
  Play,
  Filter,
  Terminal,
  CheckCircle2,
  XCircle,
  Clock,
  Layers,
  Cpu,
  Search,
  Plus,
  Sparkles,
  RefreshCw,
  SlidersHorizontal,
  CheckSquare,
  Square,
  Shield,
  Activity,
  ListFilter,
  Check,
  AlertOctagon
} from "lucide-react";
import {
  LiveExecution,
  TestCase,
  TestType,
  TestCoverageItem,
  TestExecutionAnalytics,
  TestRecommendation
} from "../../types";
import { TestCategoryNav, TEST_TYPE_LABELS } from "../testHub/TestCategoryNav";
import { TestExecutionAnalyticsBar } from "../testHub/TestExecutionAnalyticsBar";
import { TestCoverageSection } from "../testHub/TestCoverageSection";
import { AiIntelligenceRecommendations } from "../testHub/AiIntelligenceRecommendations";
import { LiveExecutionProgressBanner } from "../testHub/LiveExecutionProgressBanner";
import { TestCaseCard } from "../testHub/TestCaseCard";
import { TestCaseDetailDrawer } from "../testHub/TestCaseDetailDrawer";
import { AiTestGeneratorModal } from "../testHub/AiTestGeneratorModal";
import { BatchExecutionConfirmModal } from "../testHub/BatchExecutionConfirmModal";
import {
  fetchTestCases,
  executeTestCasesApi,
  fetchTestCoverage,
  fetchTestAnalytics,
  fetchTestRecommendations
} from "../../services/api";

interface TestRunsViewProps {
  executions: LiveExecution[];
  onTriggerNew: () => void;
  onCreateBugFromCandidate?: (candidate: any) => void;
}

export const TestRunsView: React.FC<TestRunsViewProps> = ({
  executions,
  onTriggerNew,
  onCreateBugFromCandidate
}) => {
  // Navigation & Category state
  const [activeCategory, setActiveCategory] = useState<TestType | "ALL">("ALL");
  const [activeHubView, setActiveHubView] = useState<"cases" | "runs">("cases");

  // Filtering states
  const [filterStatus, setFilterStatus] = useState<string>("all");
  const [filterPriority, setFilterPriority] = useState<string>("all");
  const [filterEnv, setFilterEnv] = useState<string>("all");
  const [searchQuery, setSearchQuery] = useState<string>("");

  // Data states
  const [testCases, setTestCases] = useState<TestCase[]>([]);
  const [coverageList, setCoverageList] = useState<TestCoverageItem[]>([]);
  const [analytics, setAnalytics] = useState<TestExecutionAnalytics | null>(null);
  const [recommendations, setRecommendations] = useState<TestRecommendation[]>([]);
  const [categoryCounts, setCategoryCounts] = useState<Record<string, number>>({});
  const [isLoading, setIsLoading] = useState<boolean>(true);

  // Multi-selection state
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  // Drawer & Modal states
  const [inspectingTestCase, setInspectingTestCase] = useState<TestCase | null>(null);
  const [isAiModalOpen, setIsAiModalOpen] = useState<boolean>(false);
  const [aiModalCategory, setAiModalCategory] = useState<TestType>("FUNCTIONAL");
  const [aiModalFeature, setAiModalFeature] = useState<string>("Payment & Checkout Pipeline");

  // Batch Execution Confirm Modal
  const [isConfirmModalOpen, setIsConfirmModalOpen] = useState<boolean>(false);
  const [confirmScope, setConfirmScope] = useState<TestType | "ALL" | "SELECTED">("ALL");
  const [confirmCount, setConfirmCount] = useState<number>(0);

  // Live execution progress animation state
  const [isLiveRunning, setIsLiveRunning] = useState<boolean>(false);
  const [liveScope, setLiveScope] = useState<string>("");
  const [liveTotal, setLiveTotal] = useState<number>(0);
  const [liveCompleted, setLiveCompleted] = useState<number>(0);
  const [livePassed, setLivePassed] = useState<number>(0);
  const [liveFailed, setLiveFailed] = useState<number>(0);
  const [currentLiveLog, setCurrentLiveLog] = useState<string>("");

  // Load test cases, coverage, analytics, and recommendations
  const loadData = useCallback(async () => {
    try {
      setIsLoading(true);
      const [casesRes, covRes, analyticsRes, recRes] = await Promise.all([
        fetchTestCases({
          test_type: activeCategory === "ALL" ? undefined : activeCategory,
          status: filterStatus === "all" ? undefined : filterStatus,
          priority: filterPriority === "all" ? undefined : filterPriority,
          environment: filterEnv === "all" ? undefined : filterEnv,
          search: searchQuery || undefined
        }),
        fetchTestCoverage(),
        fetchTestAnalytics(activeCategory === "ALL" ? undefined : activeCategory),
        fetchTestRecommendations()
      ]);

      if (casesRes?.testCases) {
        setTestCases(casesRes.testCases);
      }
      if (covRes?.coverage) {
        setCoverageList(covRes.coverage);
        // Compute category counts
        const counts: Record<string, number> = {
          ALL: covRes.coverage.reduce((acc, c) => acc + c.totalTests, 0)
        };
        covRes.coverage.forEach((c) => {
          counts[c.test_type] = c.totalTests;
        });
        setCategoryCounts(counts);
      }
      if (analyticsRes?.analytics) {
        setAnalytics(analyticsRes.analytics);
      }
      if (recRes?.recommendations) {
        setRecommendations(recRes.recommendations);
      }
    } catch (err) {
      console.error("Failed to load test cases data:", err);
    } finally {
      setIsLoading(false);
    }
  }, [activeCategory, filterStatus, filterPriority, filterEnv, searchQuery]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Handle Category Selection
  const handleSelectCategory = (cat: TestType | "ALL") => {
    setActiveCategory(cat);
    setSelectedIds([]);
  };

  // Toggle selection
  const handleToggleSelect = (id: string) => {
    setSelectedIds((prev) =>
      prev.includes(id) ? prev.filter((i) => i !== id) : [...prev, id]
    );
  };

  // Select all / Deselect all
  const handleSelectAll = () => {
    if (selectedIds.length === testCases.length) {
      setSelectedIds([]);
    } else {
      setSelectedIds(testCases.map((c) => c.id));
    }
  };

  // Open AI modal for a specific category
  const handleOpenAiForCategory = (cat: TestType, feature?: string) => {
    setAiModalCategory(cat);
    setAiModalFeature(feature || `${TEST_TYPE_LABELS[cat]} Validation Suite`);
    setIsAiModalOpen(true);
  };

  // Execute single test case
  const handleRunSingleTest = async (testCase: TestCase) => {
    setIsLiveRunning(true);
    setLiveScope(`[${testCase.test_type}] ${testCase.id}`);
    setLiveTotal(1);
    setLiveCompleted(0);
    setLivePassed(0);
    setLiveFailed(0);
    setCurrentLiveLog(`Initializing single execution for ${testCase.id} (${testCase.name})...`);

    try {
      const res = await executeTestCasesApi({
        testIds: [testCase.id],
        environment: testCase.environment
      });

      setLiveCompleted(1);
      setLivePassed(res.passed);
      setLiveFailed(res.failed);
      setCurrentLiveLog(`Execution completed: ${res.passed > 0 ? "PASSED" : "FAILED"}`);

      // Refresh data
      await loadData();
      if (inspectingTestCase?.id === testCase.id) {
        const updated = res.testCases?.find((c) => c.id === testCase.id);
        if (updated) setInspectingTestCase(updated);
      }
    } catch (err) {
      console.error("Failed to run test:", err);
      setCurrentLiveLog("Execution error encountered.");
    } finally {
      setTimeout(() => setIsLiveRunning(false), 2500);
    }
  };

  // Prompt confirmation before executing batch or category
  const promptBatchExecution = (scope: TestType | "ALL" | "SELECTED", count: number) => {
    setConfirmScope(scope);
    setConfirmCount(count);
    setIsConfirmModalOpen(true);
  };

  // Execute confirmed batch
  const handleConfirmedBatchExecution = async () => {
    setIsConfirmModalOpen(false);
    setIsLiveRunning(true);

    const isSelected = confirmScope === "SELECTED";
    const scopeName =
      confirmScope === "ALL"
        ? "All Categories Regression"
        : isSelected
        ? `${selectedIds.length} Selected Test Cases`
        : `${TEST_TYPE_LABELS[confirmScope as TestType]} Suite`;

    setLiveScope(scopeName);
    setLiveTotal(confirmCount);
    setLiveCompleted(0);
    setLivePassed(0);
    setLiveFailed(0);
    setCurrentLiveLog(`Broadcasting cluster dispatch for ${confirmCount} test cases...`);

    try {
      const res = await executeTestCasesApi({
        testIds: isSelected ? selectedIds : undefined,
        testType: !isSelected && confirmScope !== "ALL" ? (confirmScope as TestType) : undefined,
        environment: "Staging"
      });

      setLiveCompleted(res.totalExecuted);
      setLivePassed(res.passed);
      setLiveFailed(res.failed);
      setCurrentLiveLog(
        `Batch execution finished: ${res.passed} Passed, ${res.failed} Failed in ${res.durationMs}ms`
      );

      setSelectedIds([]);
      await loadData();
    } catch (err) {
      console.error("Batch execution failed:", err);
      setCurrentLiveLog("Batch execution interrupted by cluster gateway.");
    } finally {
      setTimeout(() => setIsLiveRunning(false), 3000);
    }
  };

  // Handle newly created AI test cases
  const handleNewTestCasesAdded = (newCases: TestCase[]) => {
    loadData();
    if (newCases.length > 0) {
      setActiveCategory(newCases[0].test_type);
    }
  };

  // Handle draft bug from failure
  const handleCreateBugFromFailure = (testCase: TestCase) => {
    if (onCreateBugFromCandidate) {
      onCreateBugFromCandidate({
        id: `cand-${Date.now().toString().slice(-4)}`,
        title: `[${testCase.test_type}] ${testCase.name} - Assertion Failure`,
        severity: testCase.priority === "Critical" ? "Critical" : "High",
        confidence: 96,
        relatedTest: `${testCase.id}: ${testCase.name}`,
        relatedService: testCase.project_name || "Enterprise Backend",
        evidenceCount: testCase.last_execution?.evidence?.length || 4,
        suggestedPriority: testCase.priority === "Critical" ? "P0 - Blocker" : "P1 - High",
        description: `Automated test execution failed. Diagnostic: ${
          testCase.last_execution?.failure_reason || "Expected assertion did not match response"
        }`,
        createdAt: "Just now",
        status: "pending"
      });
    }
  };

  // Filtered live executions for "runs" tab
  const filteredExecutions = executions.filter((e) => {
    if (filterEnv !== "all" && e.environment.toLowerCase() !== filterEnv.toLowerCase())
      return false;
    if (filterStatus !== "all" && e.status !== filterStatus) return false;
    return true;
  });

  return (
    <div className="space-y-4 font-mono">
      {/* Top Header & Global Actions */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 bg-[#0e1422] p-4 rounded-xl border border-slate-800/90 shadow-sm">
        <div>
          <div className="flex items-center gap-2.5">
            <h1 className="text-lg font-bold text-white tracking-tight">
              Test Case Execution Hub
            </h1>
            <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-cyan-950 text-cyan-300 border border-cyan-800 uppercase">
              10 Specialized Testing Categories
            </span>
          </div>
          <p className="text-xs text-slate-400 mt-1">
            Organize, generate, execute, filter, and inspect enterprise test suites across functional, non-functional, security, and performance vectors
          </p>
        </div>

        {/* Action Controls */}
        <div className="flex items-center gap-2 flex-wrap">
          {/* Refresh button */}
          <button
            onClick={() => loadData()}
            title="Refresh repository state"
            className="p-2 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
          >
            <RefreshCw className={`w-4 h-4 ${isLoading ? "animate-spin text-cyan-400" : ""}`} />
          </button>

          {/* AI Generator Button */}
          <button
            onClick={() =>
              handleOpenAiForCategory(
                activeCategory === "ALL" ? "FUNCTIONAL" : activeCategory
              )
            }
            className="flex items-center gap-2 px-3.5 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-bold shadow-sm transition-all"
          >
            <Sparkles className="w-3.5 h-3.5" />
            <span>Generate with AI</span>
          </button>

          {/* Execute All Tests Button */}
          <button
            onClick={() =>
              promptBatchExecution(
                activeCategory,
                activeCategory === "ALL"
                  ? categoryCounts.ALL || testCases.length
                  : categoryCounts[activeCategory] || testCases.length
              )
            }
            disabled={isLiveRunning}
            className="flex items-center gap-2 px-3.5 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white text-xs font-bold shadow-sm transition-all"
          >
            <Play className="w-3.5 h-3.5 fill-current" />
            <span>
              {activeCategory === "ALL"
                ? "Execute All Tests"
                : `Execute [${activeCategory.replace("_", " ")}] Suite`}
            </span>
          </button>

          {/* Trigger Ad-Hoc Execution */}
          <button
            onClick={onTriggerNew}
            className="flex items-center gap-2 px-3 py-2 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 text-xs font-bold transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Ad-Hoc Run</span>
          </button>
        </div>
      </div>

      {/* 1. HORIZONTAL CATEGORY NAVIGATION BAR (All 10 Sections + ALL) */}
      <TestCategoryNav
        activeCategory={activeCategory}
        onSelectCategory={handleSelectCategory}
        counts={categoryCounts}
      />

      {/* 2. REAL-TIME EXECUTION TELEMETRY & ANALYTICS BAR */}
      <TestExecutionAnalyticsBar analytics={analytics} activeCategory={activeCategory} />

      {/* 3. LIVE EXECUTION PROGRESS BANNER (Displays during cluster execution) */}
      <LiveExecutionProgressBanner
        isRunning={isLiveRunning}
        targetScope={liveScope}
        total={liveTotal}
        completed={liveCompleted}
        passed={livePassed}
        failed={liveFailed}
        currentLog={currentLiveLog}
        onCancel={() => {
          setIsLiveRunning(false);
          setCurrentLiveLog("Execution cancelled by operator.");
        }}
      />

      {/* 4. AI TEST GAP RECOMMENDATIONS (Scans and recommends missing categories) */}
      <AiIntelligenceRecommendations
        recommendations={recommendations}
        onGenerateForRecommendation={(rec) =>
          handleOpenAiForCategory(rec.test_type, rec.targetFeature)
        }
      />

      {/* 5. TEST CATEGORY COVERAGE MATRIX (Traceability & SLAs) */}
      <TestCoverageSection
        coverageList={coverageList}
        activeCategory={activeCategory}
        onSelectCategory={(cat) => handleSelectCategory(cat)}
        onGenerateCategoryTests={(cat) => handleOpenAiForCategory(cat)}
      />

      {/* 6. MAIN HUB VIEW SELECTOR & FILTERS BAR */}
      <div className="p-3.5 rounded-xl bg-[#0e1422] border border-slate-800/90 flex flex-wrap items-center justify-between gap-3 text-xs">
        {/* Left: View Tabs (Test Cases vs Historical Runs) */}
        <div className="flex items-center gap-2">
          <div className="flex items-center bg-slate-900/90 rounded-lg p-0.5 border border-slate-800">
            <button
              onClick={() => setActiveHubView("cases")}
              className={`px-3 py-1.5 rounded-md font-bold text-xs transition-colors flex items-center gap-1.5 ${
                activeHubView === "cases"
                  ? "bg-cyan-950 text-cyan-300 border border-cyan-800"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <ListFilter className="w-3.5 h-3.5" />
              <span>Test Cases ({testCases.length})</span>
            </button>
            <button
              onClick={() => setActiveHubView("runs")}
              className={`px-3 py-1.5 rounded-md font-bold text-xs transition-colors flex items-center gap-1.5 ${
                activeHubView === "runs"
                  ? "bg-cyan-950 text-cyan-300 border border-cyan-800"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <Terminal className="w-3.5 h-3.5" />
              <span>Console Runs ({executions.length})</span>
            </button>
          </div>

          {/* Multi-Selection Count & Batch Action */}
          {activeHubView === "cases" && (
            <div className="flex items-center gap-2 pl-2 border-l border-slate-800">
              <button
                onClick={handleSelectAll}
                className="flex items-center gap-1.5 text-xs text-slate-400 hover:text-slate-200"
              >
                {selectedIds.length === testCases.length && testCases.length > 0 ? (
                  <CheckSquare className="w-4 h-4 text-cyan-400" />
                ) : (
                  <Square className="w-4 h-4 text-slate-500" />
                )}
                <span>Select All</span>
              </button>

              {selectedIds.length > 0 && (
                <button
                  onClick={() => promptBatchExecution("SELECTED", selectedIds.length)}
                  className="flex items-center gap-1.5 px-3 py-1 rounded bg-cyan-600 hover:bg-cyan-500 text-white font-bold text-xs shadow-sm transition-all animate-in fade-in"
                >
                  <Play className="w-3 h-3 fill-current" />
                  <span>Execute Selected ({selectedIds.length})</span>
                </button>
              )}
            </div>
          )}
        </div>

        {/* Right: Search & Dropdown Filters */}
        <div className="flex items-center gap-2 flex-wrap">
          {/* Search Box */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search tests by name, tag, ID..."
              className="pl-8 pr-3 py-1.5 rounded-lg bg-slate-900 border border-slate-800 text-slate-200 text-xs focus:border-cyan-500 focus:outline-none w-48 sm:w-60"
            />
          </div>

          {/* Status Filter */}
          <select
            value={filterStatus}
            onChange={(e) => setFilterStatus(e.target.value)}
            className="p-1.5 rounded-lg bg-slate-900 border border-slate-800 text-slate-200 text-xs focus:outline-none"
          >
            <option value="all">All Statuses</option>
            <option value="Passed">Passed</option>
            <option value="Failed">Failed</option>
            <option value="Running">Running</option>
            <option value="Blocked">Blocked</option>
            <option value="Skipped">Skipped</option>
            <option value="Not Run">Not Run</option>
          </select>

          {/* Priority Filter */}
          {activeHubView === "cases" && (
            <select
              value={filterPriority}
              onChange={(e) => setFilterPriority(e.target.value)}
              className="p-1.5 rounded-lg bg-slate-900 border border-slate-800 text-slate-200 text-xs focus:outline-none"
            >
              <option value="all">All Priorities</option>
              <option value="Critical">Critical</option>
              <option value="High">High</option>
              <option value="Medium">Medium</option>
              <option value="Low">Low</option>
            </select>
          )}

          {/* Environment Filter */}
          <select
            value={filterEnv}
            onChange={(e) => setFilterEnv(e.target.value)}
            className="p-1.5 rounded-lg bg-slate-900 border border-slate-800 text-slate-200 text-xs focus:outline-none"
          >
            <option value="all">All Environments</option>
            <option value="Production">Production</option>
            <option value="Staging">Staging</option>
            <option value="Development">Development</option>
          </select>
        </div>
      </div>

      {/* 7. VIEW CANVAS CONTENT: TEST CASES REPOSITORY */}
      {activeHubView === "cases" && (
        <div className="space-y-3">
          {isLoading && testCases.length === 0 ? (
            <div className="p-8 text-center bg-[#0e1422] rounded-xl border border-slate-800 space-y-2">
              <RefreshCw className="w-6 h-6 animate-spin text-cyan-400 mx-auto" />
              <div className="text-xs text-slate-400">Loading test cases repository...</div>
            </div>
          ) : testCases.length === 0 ? (
            <div className="p-12 text-center bg-[#0e1422] rounded-xl border border-slate-800 space-y-3">
              <AlertOctagon className="w-8 h-8 text-slate-600 mx-auto" />
              <div className="text-sm font-bold text-slate-200">
                No test cases found for current filter criteria
              </div>
              <p className="text-xs text-slate-400 max-w-md mx-auto">
                No test cases match the category [{activeCategory.replace("_", " ")}] with the current status and priority filters.
              </p>
              <div className="flex items-center justify-center gap-2 pt-2">
                <button
                  onClick={() => {
                    setFilterStatus("all");
                    setFilterPriority("all");
                    setFilterEnv("all");
                    setSearchQuery("");
                  }}
                  className="px-3 py-1.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-semibold"
                >
                  Clear Filters
                </button>
                <button
                  onClick={() =>
                    handleOpenAiForCategory(
                      activeCategory === "ALL" ? "FUNCTIONAL" : activeCategory
                    )
                  }
                  className="px-4 py-1.5 rounded bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-bold flex items-center gap-1.5"
                >
                  <Sparkles className="w-3.5 h-3.5" />
                  <span>Generate Test Cases with AI</span>
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-2.5">
              {testCases.map((tc) => (
                <TestCaseCard
                  key={tc.id}
                  testCase={tc}
                  isSelected={selectedIds.includes(tc.id)}
                  onToggleSelect={handleToggleSelect}
                  onRunSingle={handleRunSingleTest}
                  onViewDetails={(c) => setInspectingTestCase(c)}
                  isRunning={isLiveRunning}
                />
              ))}
            </div>
          )}
        </div>
      )}

      {/* 8. VIEW CANVAS CONTENT: HISTORICAL LIVE EXECUTION RUNS */}
      {activeHubView === "runs" && (
        <div className="bg-[#0e1422] border border-slate-800/90 rounded-xl overflow-hidden shadow-sm divide-y divide-slate-800/60">
          {filteredExecutions.length === 0 ? (
            <div className="p-8 text-center text-xs text-slate-500">
              No historical execution runs match the filters.
            </div>
          ) : (
            filteredExecutions.map((run) => (
              <div key={run.id} className="p-4 hover:bg-slate-900/40 transition-colors space-y-3">
                <div className="flex flex-col md:flex-row md:items-center justify-between gap-3">
                  <div>
                    <div className="flex items-center gap-2.5 flex-wrap">
                      <span className="font-bold text-sm text-slate-100">{run.testName}</span>
                      <span
                        className={`text-[10px] font-mono px-2 py-0.5 rounded font-semibold border ${
                          run.status === "running"
                            ? "bg-cyan-950 text-cyan-300 border-cyan-800 animate-pulse"
                            : run.failed > 0
                            ? "bg-rose-950 text-rose-300 border-rose-800"
                            : "bg-emerald-950 text-emerald-300 border-emerald-800"
                        }`}
                      >
                        {run.status.toUpperCase()}
                      </span>
                      <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 border border-slate-700">
                        {run.environment}
                      </span>
                    </div>

                    <div className="mt-1 flex items-center gap-3 text-xs text-slate-400 font-mono">
                      <span className="flex items-center gap-1 text-slate-300">
                        <Layers className="w-3.5 h-3.5 text-slate-500" />
                        {run.project}
                      </span>
                      <span>&bull;</span>
                      <span className="flex items-center gap-1">
                        <Cpu className="w-3.5 h-3.5 text-cyan-500" />
                        {run.executor}
                      </span>
                      <span>&bull;</span>
                      <span>Duration: {run.durationSec}s</span>
                    </div>
                  </div>

                  <div className="flex items-center gap-4 shrink-0 font-mono text-xs">
                    <div className="text-right">
                      <div className="text-emerald-400 font-bold">{run.passed} Passed</div>
                      {run.failed > 0 && (
                        <div className="text-rose-400 font-bold">{run.failed} Failed</div>
                      )}
                    </div>

                    <div className="w-28">
                      <div className="text-[10px] text-slate-500 text-right mb-1">
                        {run.progress}%
                      </div>
                      <div className="w-full bg-slate-800 h-2 rounded-full overflow-hidden">
                        <div
                          className="bg-cyan-500 h-full rounded-full transition-all"
                          style={{ width: `${run.progress}%` }}
                        />
                      </div>
                    </div>
                  </div>
                </div>

                {/* Stored logs snippet */}
                <div className="p-3 rounded-lg bg-[#080d16] border border-slate-800 text-[11px] font-mono text-slate-400 space-y-1">
                  <div className="text-[10px] text-slate-500 uppercase font-semibold">
                    Live Stdout / Telemetry
                  </div>
                  {run.logs.map((log, idx) => (
                    <div
                      key={idx}
                      className={
                        log.includes("[FAIL]")
                          ? "text-rose-400 font-bold"
                          : log.includes("[PASS]")
                          ? "text-emerald-400 font-semibold"
                          : "text-slate-300"
                      }
                    >
                      {log}
                    </div>
                  ))}
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 9. TEST CASE DETAIL & EXECUTION DIAGNOSTICS DRAWER */}
      <TestCaseDetailDrawer
        testCase={inspectingTestCase}
        isOpen={Boolean(inspectingTestCase)}
        onClose={() => setInspectingTestCase(null)}
        onRunTest={handleRunSingleTest}
        onCreateBugFromFailure={handleCreateBugFromFailure}
      />

      {/* 10. AI TEST GENERATOR MODAL */}
      <AiTestGeneratorModal
        isOpen={isAiModalOpen}
        onClose={() => setIsAiModalOpen(false)}
        initialCategory={aiModalCategory}
        initialFeature={aiModalFeature}
        onTestCasesAdded={handleNewTestCasesAdded}
      />

      {/* 11. BATCH EXECUTION CONFIRMATION MODAL */}
      <BatchExecutionConfirmModal
        isOpen={isConfirmModalOpen}
        onClose={() => setIsConfirmModalOpen(false)}
        onConfirm={handleConfirmedBatchExecution}
        testCount={confirmCount}
        testType={confirmScope}
        environment={filterEnv !== "all" ? filterEnv : "Staging"}
        isExecuting={isLiveRunning}
      />
    </div>
  );
};
