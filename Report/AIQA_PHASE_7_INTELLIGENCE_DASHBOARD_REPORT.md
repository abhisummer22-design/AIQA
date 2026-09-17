# AIQA Phase 7 — Intelligence Dashboard & Observability UI Report

## 1. Executive Summary

AIQA Phase 7 establishes the production-grade **QA Intelligence Dashboard & Observability UI** within the frontend application (`forentend`), consuming the real Adaptive QA Intelligence and Production Observability backend implemented in Phase 6.

All intelligence data, failure clusters, flakiness scores, strategy metrics, directional trends, and quality graph entities are read directly from authoritative orchestrator REST endpoints (`/api/v1/projects/:project_id/intelligence/*`). The user interface is strictly **read-only**, **project-isolated**, and enforces explicit safety boundaries: autonomous healing actions require AST diff review, plan validation, policy checks, RBAC authorization, and human operator sign-off before execution.

---

## 2. Architecture & Implementation Details

### A. Contract Types (`forentend/src/types/contract.ts`)
Mirrored 1:1 with real Rust SQLite entities and handler responses:
* `ExecutionTelemetryRow`: Task ID, run ID, duration, status, retry count, healing attempt count, failure category, fingerprint, sanitized error message, selector, verification outcome.
* `FailureClusterRow`: Fingerprint signature, occurrence count, root-cause hypothesis, confidence, reproducibility, review state (`NEEDS_REVIEW`, `CONFIRMED`, `DISMISSED`).
* `FlakinessMetricsRow`: Node ID, flakiness score (0.0 to 1.0), window runs, window failures, environment correlation.
* `HealingStrategyMetricsRow`: Strategy name, failure category, attempts count, verified count, not verified count, regression count, calibrated confidence.
* `TrendAnalysis` & `TrendDirection`: Direction (`IMPROVING`, `STABLE`, `DEGRADING`, `INSUFFICIENT_DATA`), current value, previous value, sample count, notes.
* `QualityNodeRow`, `QualityRelationRow`, `QualityGraphResponse`: Real graph vertices and directed edges representing tests, runs, clusters, and strategies.

### B. API Client Layer (`forentend/src/services/aiqaApi.ts`)
Added typed, project-scoped endpoints under `aiqaApi.intelligence`:
* `telemetry(projectId, signal)`: Fetches execution telemetry records.
* `failures(projectId, signal)`: Fetches normalized failure clusters.
* `flakiness(projectId, signal)`: Fetches statistical flakiness metrics.
* `strategies(projectId, signal)`: Fetches healing strategy reliability records.
* `trends(projectId, signal)`: Fetches directional quality trend analysis.
* `qualityGraph(projectId, signal)`: Fetches quality graph topology.
* Preserved existing `tests(projectId, signal)` endpoint used by Reports page.

### C. Intelligence Subcomponents (`forentend/src/components/intelligence/`)
1. **`ExecutionHealthSection.tsx`**:
   * Top-level 8-KPI execution health grid: Total Executions, Pass Rate, Failure Rate, Flaky Tests, Average Duration, Healing Attempts, Verified Heals, and Regressions Detected.
   * Handles sparse/zero data gracefully with `"Insufficient data"` and `"0 recorded"`.
2. **`TelemetryTableSection.tsx`**:
   * Real-time execution telemetry table with search filter (task name, task ID, run ID, failure category).
   * Status filter (`ALL`, `PASSED`, `FAILED`, `TIMEOUT`, `INFRA_ERROR`) and environment filter.
   * Client-side pagination (10 rows/page) and direct drilldown into modal.
3. **`TelemetryDetailModal.tsx`**:
   * Detailed inspection drawer displaying complete execution metadata, task IDs, HTTP status, and duration.
   * Automated secret redaction for API keys (`aiqa_*`) and auth tokens (`bearer *`, `token=*`) in error traces.
4. **`FailureIntelligenceSection.tsx`**:
   * Normalized failure fingerprints clustered by recurring signature.
   * Frequency progress bars, root-cause hypotheses, calibrated confidence percentages, reproducibility scores, and review state badges.
5. **`FlakinessSection.tsx` & `FlakyTestDetailModal.tsx`**:
   * Flakiness scores categorized by stability threshold: Stable (<15%), Flaky (15–39%), Critical (≥40%).
   * Drill-down modal presenting recent PASS/FAIL outcome sequence timeline and environment correlation breakdown.
6. **`HealingIntelligenceSection.tsx`**:
   * Empirical strategy reliability table with attempt counts, verification counts, regression counts, and calibrated confidence.
   * **Governed Self-Healing Pipeline Banner**: Visual 7-step governance chain:
     `01 Intelligence → 02 Recommendation → 03 Plan Validation → 04 Policy Gate → 05 RBAC Check → 06 Approval → 07 Execution`.
     Enforces that UI is strictly read-only and no unverified autonomous code alterations are permitted.
7. **`RiskIntelligenceSection.tsx`**:
   * Multi-factor risk breakdown: Failure Rate Factor (40% weight), Flakiness Factor (30% weight), Regression Factor (30% weight), and Security/SSRF Boundary Gate.
   * Composite risk badge (`CRITICAL`, `HIGH`, `MEDIUM`, `LOW`, `INSUFFICIENT_DATA`).
8. **`TrendsSection.tsx`**:
   * Trajectory cards with directional badges (`IMPROVING`, `STABLE`, `DEGRADING`, `INSUFFICIENT_DATA`), current vs previous comparative values, and sample counters.
9. **`QualityGraphSection.tsx`**:
   * Interactive node-link graph explorer for vertices (`test`, `task`, `failure_cluster`, `healing_strategy`, `component`).
   * Type filtering, node search, and interactive Node Inspector Drawer displaying vertex properties and incoming/outgoing relational edges.
10. **`PlanningInsightsSection.tsx`**:
    * TestPlanningAgent feedback surface highlighting fragile tests, broken DOM selectors, and recommended test priorities.
    * **Strict Safety Constraint**: Zero LLM chain-of-thought or raw internal model reasoning tokens are exposed.

### D. Main Dashboard Page & Routing
* **`QaIntelligenceDashboardPage.tsx`**:
  * Project-isolated via `useProject()`. If no project is selected, presents clean `EmptyState`.
  * Parallel data fetching across all 6 intelligence endpoints with `AbortController` cancellation on project switch.
  * Robust error states using `renderResourceErrorState` for 401 Unauthorized, 403 Forbidden, 500 Server Error, and Offline network failure.
  * 5 tab views: `Overview`, `Telemetry`, `Flakiness & Risk`, `Healing Governance`, `Quality Graph`.
  * Top bar with active project badge and `Refresh` button with spinner.
* **`routeConfig.tsx`**:
  * Registered `/intelligence` route with `Brain` icon and `protectedRoute: true`.

---

## 3. Verification & Test Results

### A. Frontend Test Suite (`forentend`)
* **565 passed across 33 test files** (`npm test`).
* **`QaIntelligenceDashboardPage.test.tsx` (19 passed, 0 failed)**:
  * Select project empty state verification.
  * Error state handling (401, 403, 500, offline).
  * Execution health KPI rendering and insufficient data fallback.
  * Trend direction badges and sample count reporting.
  * Failure clusters rendering, hypothesis display, and empty states.
  * Planning insights feedback and verification that NO chain-of-thought tokens are leaked.
  * Telemetry table searching, status filtering, and pagination.
  * Telemetry modal inspection with sensitive secret/token redaction.
  * Flakiness metrics table and outcome sequence modal drilldown.
  * Multi-factor risk assessment and composite risk badge.
  * 7-step healing safety governance banner and strategy reliability table.
  * Quality graph topology rendering, node type filtering, and inspector drawer.
  * Refresh button refetching.

### B. Backend Verification (`services/orchestrator`)
* **698 tests passed**, 0 failed, 2 ignored (`cargo test`).
* **`cargo check`**: Clean (0 errors).
* **`cargo fmt --check`**: Clean (0 formatting discrepancies).

---

## 4. Security, Isolation & Safety Boundaries

1. **Multi-Tenant & Project Isolation**: All data queries and component states strictly require `selectedProject.id`. Cross-project data leakage is structurally impossible.
2. **Credential Redaction**: Error strings and telemetry traces are sanitized before presentation; API keys and tokens are masked with `••••••••`.
3. **Zero Autonomous Backdoors**: The dashboard is strictly read-only. No buttons or shortcuts bypass plan validation, AST diffing, SSRF checks, RBAC gates, or human approval.
4. **Governed Self-Healing Visibility**: The explicit 7-step governance chain is visibly displayed, maintaining complete auditability and governance compliance.
