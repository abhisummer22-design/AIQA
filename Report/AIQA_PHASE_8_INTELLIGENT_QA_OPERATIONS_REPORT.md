# AIQA Phase 8 — Intelligent QA Operations & Cross-Environment Correlation Report

## 1. Executive Summary

AIQA Phase 8 delivers **Intelligent QA Operations & Cross-Environment Correlation**, providing engineering and QA teams with deep forensic investigation, cross-environment failure correlation, operational timeline reconstruction, and safe, bounded telemetry retention.

Phase 8 adheres strictly to foundational governance and safety principles:
* **Strictly Observational & Analytical**: Phase 8 observes, analyzes, correlates, and recommends. It NEVER bypasses RBAC, plan validation, policy gates, or human approval.
* **Strict Architectural Boundaries**: Zero OpenJarvis, zero Python runtimes, zero PyO3, zero second AI Router, zero second Quality Graph, zero second execution engine. All implementations leverage existing Rust orchestrator subsystems and React frontend architecture.
* **Strict Project & Tenant Isolation**: All queries, investigation workflows, and retention actions are strictly scoped to the active `project_id`. Telemetry, clusters, and timeline events never leak across project boundaries.
* **Deterministic & Conservative Analysis**: Cross-environment correlations utilize deterministic SHA-256 signatures, selectors, tasks, and components. Regressions are categorized conservatively (`"No related regression observed"`, `"Possible related regression"`, `"Confirmed regression"`). Change correlations are explicitly marked non-causal (*"Failure frequency increased after this change; direct causality not confirmed"*).
* **Safe, Bounded Telemetry Retention**: Project-scoped retention with dry-run simulation and safety exclusion gates (active runs, pending/approved healing proposals, audit logs, and Quality Graph nodes are permanently preserved).

---

## 2. Architecture & Domain Models

### A. Operations Service (`services/orchestrator/src/operations.rs`)
Centralizes operational intelligence, cross-environment matrix generation, timeline event reconstruction, and safe retention management.

Key Domain Entities:
* `RecurrenceState`: State machine tracking failure lifecycle:
  * `NEW`: Single occurrence, newly detected.
  * `RECURRING`: Multiple occurrences across runs/tasks.
  * `PERSISTENT`: 3+ occurrences across multiple environments or extended timeframes.
  * `RESOLVED`: No recent occurrences following verified healing.
* `InvestigationTimelineEvent`: Chronological operational events:
  * `FAILURE_OBSERVED`: Raw execution failure recorded.
  * `HEALING_PROPOSED`: Autonomous self-healing generated proposal.
  * `HEALING_APPROVED`: Human operator or policy gate authorized change.
  * `HEALING_EXECUTED`: AST modification applied.
  * `HEALING_VERIFIED`: Test rerun confirmed resolution.
  * `REGRESSION_DETECTED`: Failure recurred or secondary failure introduced.
* `EnvironmentOccurrence`: Environment-specific failure counts, total runs, failure rate (%), first seen, and last seen timestamps.
* `EnvironmentCorrelationMatrix`: Aggregated matrix across environments (`dev`, `staging`, `production`, etc.) with cross-environment divergence flag (`divergent_across_environments`).
* `HealingHistoryCorrelation`: Correlates past healing proposals with AST diffs, approval state, verification outcome, and regression assessment.
* `FailureInvestigation`: Comprehensive aggregate containing root cause hypothesis, confidence, recurrence state, environment matrix, chronological timeline, healing correlations, change-time observations, and recommended QA actions.
* `RetentionPolicy` & `RetentionReport`: Configures retention cutoff (days), dry-run flag, candidate count, deleted count, preserved count, and safety exclusions list.

---

## 3. Core Capabilities

### A. Full Traceability Chain
Reconstructs the complete operational lifecycle from test failure to quality graph:
```
Test -> Task -> Failure -> Cluster -> Evidence -> Healing Proposal -> AST Diff -> Approval -> Execution -> Verification -> Quality Graph
```
Each timeline event captures timestamp, task ID, run ID, environment, details, and linked evidence artifacts (screenshots, logs, DOM snapshots).

### B. Cross-Environment Correlation
* Analyzes execution telemetry across all project environments.
* Detects **Cross-Environment Divergence** when failure patterns vary significantly between environments (e.g., failure occurs in `staging` or `production` but passes or is absent in `dev`), surfacing environmental drift or configuration discrepancies.

### C. Conservative Regression & Non-Causal Change Correlation
* **Regression Assessment**: Analyzes post-healing telemetry to verify whether failures recurred or new failures surfaced. Categorizes strictly as:
  * `"No related regression observed"`
  * `"Possible related regression"`
  * `"Confirmed regression"`
* **Non-Causal Change Correlation**: Flags code or configuration updates preceding failure bursts with explicit non-causal phrasing:
  * `"Failure frequency increased after commit <sha>; correlation only, direct causality not confirmed."`

### D. Safe, Bounded Telemetry Retention
* **Dry-Run Simulation**: Computes candidate count and lists safety invariants without deleting any rows.
* **Safety Invariant Gates**:
  1. Active runs (`PENDING`, `RUNNING`) are strictly excluded.
  2. Tasks linked to healing proposals in `PENDING_APPROVAL` or `APPROVED` states are strictly excluded.
  3. Audit logs are strictly immutable and never pruned.
  4. Quality Graph vertices and edges are permanent and never deleted by telemetry retention.
* **Admin-Only RBAC**: Pruning endpoints strictly require Admin role or `ADMIN_SECRET_KEY`.

---

## 4. REST Endpoints & RBAC

Implemented in `services/orchestrator/src/api.rs`:

| Method | Path | RBAC Role | Description |
|---|---|---|---|
| `GET` | `/api/v1/projects/:project_id/intelligence/investigations` | Reader+ | Lists all failure investigations for project |
| `GET` | `/api/v1/projects/:project_id/intelligence/failures/:cluster_id/investigation` | Reader+ | Complete forensic investigation for failure cluster |
| `GET` | `/api/v1/projects/:project_id/intelligence/failures/:cluster_id/environments` | Reader+ | Cross-environment correlation matrix |
| `GET` | `/api/v1/projects/:project_id/intelligence/failures/:cluster_id/timeline` | Reader+ | Chronological timeline of operational events |
| `POST` | `/api/v1/projects/:project_id/intelligence/retention/dry-run` | **Admin Only** | Simulates telemetry pruning with safety report |
| `POST` | `/api/v1/projects/:project_id/intelligence/retention/prune` | **Admin Only** | Executes bounded telemetry pruning |

---

## 5. Prometheus Observability Metrics

Exported via `services/orchestrator/src/metrics.rs` under `/metrics`:

* `aiqa_investigations_total`: Total number of forensic investigations generated.
* `aiqa_correlated_failures_total`: Total number of failures successfully correlated.
* `aiqa_cross_environment_correlations_total`: Total number of cross-environment correlations analyzed.
* `aiqa_telemetry_pruned_total`: Cumulative counter of telemetry rows safely deleted.
* `aiqa_retention_runs_total`: Total count of retention jobs executed (dry-run or live).

---

## 6. Frontend UI Components (`forentend`)

### A. Failure Investigation Workspace Modal (`FailureInvestigationWorkspaceModal.tsx`)
* Accessible via "Investigate" button on each failure cluster card in `FailureIntelligenceSection.tsx`.
* **Traceability Breadcrumb Header**: Visual flow indicating `Test → Task → Failure → Cluster → Evidence → Proposal → AST Diff → Approval → Execution → Verification → Quality Graph`.
* **Executive Summary Card**: Signature, failure category, recurrence state (`NEW`, `RECURRING`, `PERSISTENT`, `RESOLVED`), divergence badge, root cause hypothesis, and confidence bar.
* **Tab 1: Cross-Environment Matrix**: Tabular comparison of failure counts, total runs, failure rates, and first/last seen across environments.
* **Tab 2: Operational Timeline**: Chronological event feed with event-type badges, timestamps, task/run IDs, details, and clickable evidence badges.
* **Tab 3: Healing History & AST Diff**: Proposal status, failure fingerprint, verification status, regression assessment, and interactive AST diff toggle showing unified patch (`- wait(5000)`, `+ wait(10000)`).
* **Tab 4: Recommendations & Change Analysis**: Non-causal temporal change observations and prioritized QA actions.

### B. Telemetry Retention Modal (`TelemetryRetentionModal.tsx`)
* Accessible via "Retention" button in `QaIntelligenceDashboardPage.tsx` header.
* Prominently displays safety invariants and governance policies.
* Configurable retention period (default 30 days).
* Separate **Simulate Retention (Dry-Run)** and **Execute Prune (Live)** triggers.
* Displays detailed summary reports showing candidates evaluated, deleted count, preserved count, duration (ms), and active safety exclusions.

---

## 7. Verification & Test Results

### A. Backend Verification (`services/orchestrator`)
* **716 tests passed**, 0 failed, 2 ignored (`cargo test --workspace`).
* **14 Unit & Integration Tests** in `services/orchestrator/src/operations_tests.rs`:
  * Recurrence state transitions (`NEW`, `RECURRING`, `PERSISTENT`).
  * Cross-environment occurrence aggregation and divergence detection.
  * Timeline event reconstruction and sorting.
  * Regression assessment (`No related regression observed`, `Possible related regression`).
  * Safety exclusion of active runs (`PENDING`, `RUNNING`).
  * Safety exclusion of tasks with pending healing proposals (`PENDING_APPROVAL`).
  * Retention dry-run non-destructiveness.
  * Live retention bounded pruning and report accuracy.
  * Strict multi-tenant/cross-project isolation.
* **2 API Integration Tests** in `services/orchestrator/src/api_tests.rs`:
  * `phase8_failure_investigation_and_environment_matrix_endpoints`: Verifies HTTP 200, schema correctness, and project scoping.
  * `phase8_retention_dry_run_and_prune_rbac`: Verifies RBAC 403 rejection without admin authorization, and 200 OK with admin authorization.
* **`cargo check`**: Clean (0 errors).
* **`cargo fmt --check`**: **NOT clean** — it reports formatting differences in ~19 files of this crate (verified this session). An earlier revision of this document claimed 0 discrepancies; that claim was wrong.
* **582 tests passed across 35 test files** was the frontend figure when written; the suite is now 1,501 across 71 files.

### B. Frontend Verification (`forentend`)
* **1,501 tests passed across 71 test files** (`npm test -- --run`) as of this session; the figure previously stated here was 582 across 35 files.
* **15 Dedicated Unit Tests**:
  * `FailureInvestigationWorkspaceModal.test.tsx` (8 tests):
    * Modal rendering, header badges, and hypothesis.
    * Environment matrix statistics and divergence warnings.
    * Chronological timeline rendering and evidence links.
    * Healing history tab and AST diff toggling.
    * Recommendations and non-causal change correlations.
    * Graceful API error handling.
    * Modal close callback.
  * `TelemetryRetentionModal.test.tsx` (7 tests):
    * Initial modal state and safety policy rendering.
    * Retention days input validation.
    * Dry-run simulation execution and candidate display.
    * Live pruning execution and deletion report.
    * Graceful API error handling.
    * Modal close callback.
* **Production Build**: `npm run build` completed cleanly (0 errors, 2596 modules transformed).

---

## 8. Security, Isolation & Safety Boundaries

1. **Multi-Tenant & Project Isolation**: Cluster lookups strictly verify `project_id` (`get_failure_cluster_by_id_scoped`). Retention operations delete only rows belonging to `project_id`.
2. **Safety Exclusions by Design**: Active workloads, pending human approvals, audit trails, and quality graph topology are structurally exempt from deletion.
3. **Admin-Only RBAC Enforcement**: Telemetry retention requires explicit administrative credentials, preventing unauthorized operators from modifying historical telemetry.
4. **Governed Automation**: Phase 8 delivers intelligence and recommendations only; automated code modifications remain subject to the full 7-step governance chain (AST diff review, plan validation, policy gate, RBAC, human approval).
