# AIQA Phase 10 — QA Copilot: Architecture Audit and Implementation Plan

**Date:** 2026-10-11  
**Status:** Architecture Audit & Planning (Phase 10 Not Started; Planning Only)  
**Authoritative Reference:** `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §"PHASE 10 — AI QA COPILOT" (L369–431)  
**Predecessor Checkpoint:** `docs/next-intelligence/PHASE_9_PASS.md` (Phase 9 closed with PASS)  

---

## 1. Executive Summary & Repository Checkpoint

Phase 9 (Release Readiness Intelligence & Test Artifact Lifecycle Remediation) was formally completed and closed at **PASS**, with zero remaining test-artifact leaks across all 13 batches and production blockers.

As established by the roadmap discipline:
- **Phase 10 has not started.**
- No source code, tests, configuration, or pre-existing reports are modified during this audit.
- No new implementation files, database tables, or routes are added during this task.
- This document establishes the authoritative architecture audit, capability gap analysis, and phased implementation plan for Phase 10.

### Repository Baseline Checkpoint
- **Repository Root:** `H:\AIQA`
- **Branch:** `main`
- **HEAD Commit:** `7930507785d203f3ff9cb3e575413bd95f04aabb` (0 ahead / 0 behind `origin/main`)
- **Tracked Modifications:** 44 files (unchanged; preserved intact)
- **Untracked Files:** 46 files (unchanged; preserved intact prior to this planning document)
- **Commits / Pushes:** Zero commits created; zero pushes attempted.

---

## 2. Authoritative Roadmap Specifications (Phase 10)

Derived directly from `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` L369–431:

### 2.1 Core Objective
> *"Provide a conversational interface over existing QA evidence and intelligence, without creating another memory database."*

The foundational constraint of Phase 10 is **evidence reuse without memory duplication**. The Copilot is an intelligent, grounded conversational layer over the database, intelligence engines, and artifacts already produced by Phases 1 through 9. It does **not** create a parallel database, a separate vector lake, or an ungrounded reasoning store.

### 2.2 Canonical Question Taxonomy
The roadmap defines eight canonical queries that Phase 10 must resolve:
1. **"Why did this release fail?"** (Release readiness, run status, blocking defects)
2. **"What should I test next?"** (Change impact, risk prioritisation, unexecuted regression)
3. **"Which changes affect my regression suite?"** (Change declarations, graph impacts, impacted test cases)
4. **"Why is this test high risk?"** (Measured risk factors, historical telemetry, flakiness, failure rates)
5. **"Which failures are recurring?"** (Failure clusters, canonical fingerprints, defect patterns)
6. **"Which requirements lack test coverage?"** (Requirement nodes, test case linkages, coverage gaps)
7. **"Why was this finding classified as insufficient evidence?"** (Integrity verifications, test quality verdicts, unmeasured factors)
8. **"What is blocking release readiness?"** (Release readiness deterministic gates, mandatory test floors)

### 2.3 Architecture Requirements
1. **Intent Parsing:** Parse natural language into a typed, known query intent.
2. **Evidence Retrieval:** Retrieve evidence using existing project-scoped APIs and readers.
3. **Grounded Synthesis:** Produce a concise answer strictly grounded in returned records.
4. **Precise Citations:** Explicitly cite entity IDs, run IDs, report links, timestamps, and evidence references.
5. **Fact vs Inference Separation:** Structurally distinguish confirmed stored facts from model-derived inferences.
6. **Uncertainty & Incompleteness:** Return `INSUFFICIENT_DATA` when evidence is missing or unmeasured; never invent conclusions.
7. **Clarification Handling:** Prompt the user for clarification when the entity, project, or observation window is ambiguous.
8. **Governance Enforcement:** Enforce caller role (`Viewer`, `Operator`, `Admin`) and project boundary (`project_id`).

### 2.4 Safety and Privacy Mandates
- **No Chain-of-Thought Storage:** Do not persist internal prompt chains or reasoning traces to the database.
- **No Secret Exposure:** Redact API tokens, credentials, and sensitive values from inputs and outputs.
- **No Arbitrary SQL or Shell Execution:** The Copilot cannot execute arbitrary SQL strings or shell commands. All database interactions must use existing parameterized `Store` methods.
- **No Authorization Bypasses:** The Copilot cannot bypass RBAC or read cross-project records.
- **Zero Hallucination Policy:** Never invent bugs, metrics, historical runs, or release outcomes.
- **No Autonomous Mutation Authority:** The Copilot is strictly read-only; it cannot mutate tests, approve runs, or alter system state without human approval.

### 2.5 Required Verification Scenarios (11 Test Categories)
1. Question answered from actual evidence.
2. No evidence available (`INSUFFICIENT_DATA`).
3. Ambiguous project (clarification prompted).
4. Unauthorized project access (RBAC denied).
5. Prompt injection contained in stored requirement or defect text.
6. Sensitive values redacted.
7. Contradictory sources handled transparently.
8. Stale records flagged with freshness notes.
9. Facts versus inferences clearly separated in output.
10. Query limits and timeouts enforced.
11. No arbitrary SQL or shell execution verified.

---

## 3. Current-State Architectural Audit

An inspection of `services/orchestrator/src/` was conducted to classify existing components against Phase 10 requirements:

| Capability / Requirement | Current State in Repository | Implementation Location | Gap / Architectural Need |
|---|---|---|---|
| **Release Readiness Assessment** | **Already Supported** | `release_readiness.rs` (`evaluate_release_readiness`) | Assessment logic exists, but has no API endpoint. Copilot can call the Rust function directly. |
| **Change Impact & Regression Selection** | **Already Supported** | `change_impact.rs` (`ChangeImpactReader`, `resolve`) | Supported for `requirement`, `test_case`, `failure_cluster`. Missing source targets return `INSUFFICIENT_DATA`. |
| **Risk Factors & Calibration** | **Already Supported** | `qa_memory_risk.rs` (`calibrate_partial`, `Factor`), `adaptive_intelligence.rs` | Explicit measured vs unmeasured factors. D3 `failure_severity` divergence remains open. |
| **Failure Clustering & Fingerprints** | **Already Supported** | `intelligence.rs` (`FailureClusterRow`, `classify_failure`), `store.rs` | Deterministic clustering based on canonical fingerprints. Optional AI hypothesis exists. |
| **Requirement Traceability** | **Already Supported** | `intelligence.rs` (`link_test_case_to_requirement`), `store.rs` | Quality graph edges (`quality_relations`) link test cases to requirement nodes. |
| **Test Quality Assessments** | **Already Supported** | `test_quality.rs` (`assess_case`, `QualityVerdict`) | Per-dimension evaluation; `InsufficientData` and `NotAssessed` are first-class verdicts. |
| **Data Integrity & Consistency** | **Already Supported** | `integrity.rs` (`IntegrityVerifier`, `verify_run`) | Cross-layer observation comparison; explicit `InsufficientData` and `Inconsistent`. |
| **AI Infrastructure & Model Routing** | **Already Supported** | `ai_router.rs` (`AiInfrastructure::generate`, `AiTask`) | Multi-provider discovery, scoring, fallback. Reused by `ai_generation.rs` and `intelligence.rs`. |
| **Authentication & RBAC** | **Already Supported** | `auth.rs` (`Role::Viewer`, `Operator`, `Admin`, `AuthContext`) | Project scoping via URL / task-local storage. Store enforces project isolation. |
| **Copilot Query Intent Parser** | **Not Implemented** | *None* (0 occurrences of copilot in source) | Needs deterministic pattern-matching and structured classification into canonical query intents. |
| **Copilot Evidence Retrieval Coordinator** | **Not Implemented** | *None* | Needs a coordinator service that routes intent to the appropriate existing reader and bounds results. |
| **Copilot Response Synthesizer & Citation Engine** | **Not Implemented** | *None* | Needs structured response formatting separating facts, inferences, citations, and status. |
| **Secret Redaction for Copilot** | **Partially Implemented** | Destination policy regexes in `destinations.rs` | Needs general-purpose credential redaction for prompt inputs and responses. |
| **Copilot API Endpoint** | **Not Implemented** | `api.rs` has no copilot routes | Needs `POST /api/v1/projects/:project_id/copilot/ask` with rate-limiting and timeouts. |

### Architectural Limitations & Open Decisions Disclosed
1. **Decision D3 (`failure_severity` Divergence):** Two implementations share one factor name. As in Phase 8 and Phase 9, the Copilot must **not** attempt to reconcile or guess this value; it must report risk factors with their exact provenance.
2. **Project vs Release Scoping:** Five tables (`bug_drafts`, `failure_clusters`, `exploration_findings`, `change_impacts`, user journeys) lack a `release_id`. The Copilot must strictly label their evidence scope as `Project` rather than falsely attributing them to a specific release.
3. **No Conversational Memory Table:** The roadmap explicitly prohibits adding a memory database. The Copilot must be stateless across turns or accept a bounded, caller-provided short conversation context.

---

## 4. Phased Implementation Plan

Phase 10 is partitioned into **five reviewable batches**, ordered strictly by dependency constraints:

```
[Batch 1: Intent Taxonomy & Parser]
                 │
                 ▼
[Batch 2: Evidence Retrieval Engine]
                 │
                 ▼
[Batch 3: Grounded Synthesis & Citations]
                 │
                 ▼
[Batch 4: API & Governance Integration]
                 │
                 ▼
[Batch 5: Verification & PASS Rollup]
```

---

### Batch 1: Intent Classification & Query Dispatch Architecture

- **1. Objective & Outcome:** Establish the `CopilotIntent` taxonomy, query parameter extraction, and deterministic parsing pipeline. Natural language inputs map to structured, typed intents without calling external databases.
- **2. Components to Reuse:** `ai_router::AiInfrastructure` (for fallback classification if needed), `domain.rs`.
- **3. Exact Files Likely to Change:** `services/orchestrator/src/lib.rs` (module declaration).
- **4. New Files:**
  - `services/orchestrator/src/copilot/intent.rs`
  - `services/orchestrator/src/copilot/intent_tests.rs`
- **5. API / Data Model / Storage:**
  - Enum `CopilotIntent`:
    - `ReleaseFailureReason { release_id: Option<String> }`
    - `NextTestRecommendation`
    - `RegressionImpact { change_ref: Option<String> }`
    - `TestRiskExplanation { test_case_id: String }`
    - `RecurringFailures`
    - `RequirementCoverageGaps`
    - `InsufficientEvidenceReason { finding_id: Option<String>, run_id: Option<String> }`
    - `ReleaseReadinessBlockers { release_id: Option<String> }`
    - `ClarificationRequired { prompt: String }`
    - `UnsupportedQuery { message: String }`
  - Pure in-memory parser; 0 database migrations, 0 schema changes.
- **6. Tests:**
  - Unit tests mapping all 8 canonical question forms to their corresponding intent.
  - Negative tests for ambiguous questions returning `ClarificationRequired`.
  - Negative tests for off-topic/adversarial questions returning `UnsupportedQuery`.
  - Fast, deterministic tests (0 tempdirs, 0 SQLite connections).
- **7. Security & Boundaries:** Input length bounds (max 1,000 characters). Denial-of-service prevention on regex/parsing.
- **8. Acceptance Criteria:** 100% of canonical question variants parse to the expected intent; zero database interactions.
- **9. Dependencies & Risks:** None. Fully independent first step.
- **10. Deliverable:** Working intent parser module with complete test suite.

---

### Batch 2: Project-Scoped Evidence Retrieval Engine

- **1. Objective & Outcome:** Build a unified evidence retrieval coordinator that executes project-scoped reads over existing engines based on the parsed `CopilotIntent`.
- **2. Components to Reuse:**
  - `release_readiness::evaluate_release_readiness`
  - `change_impact::ChangeImpactReader`
  - `qa_memory_risk::Factor`, `calibrate_partial`
  - `intelligence::Intelligence` (`list_failure_clusters`, `list_requirements`)
  - `test_quality::assess_case`
  - `integrity::IntegrityVerifier`
  - `store::Store` (strictly parameterized existing readers)
- **3. Exact Files Likely to Change:** `services/orchestrator/src/lib.rs`.
- **4. New Files:**
  - `services/orchestrator/src/copilot/retrieval.rs`
  - `services/orchestrator/src/copilot/retrieval_tests.rs`
- **5. API / Data Model / Storage:**
  - Struct `RetrievedEvidence`: encapsulates typed evidence structs returned by the respective engine.
  - Strict enforcement of `project_id`. No cross-project reading.
  - Bounded queries: hard limits on rows retrieved (e.g., max 50 clusters, max 100 tests).
  - Explicit `InsufficientData` when reader returns 0 records.
  - 0 database migrations; strictly read-only queries.
- **6. Tests:**
  - Evidence retrieval for each intent against seeded test data.
  - Empty database test asserting `InsufficientData` returned without error.
  - Multi-tenant isolation test: ensuring project A cannot retrieve project B evidence even with matching entity IDs.
  - Fixture lifecycle discipline: explicit awaited pool closure on all temporary test databases (delta = 0).
- **7. Security & Boundaries:** Read-only by construction. No arbitrary SQL paths exist. Tenant boundary enforced at the query parameter level.
- **8. Acceptance Criteria:** Evidence retrieval cleanly returns grounded facts or `InsufficientData` for all 8 intents; project boundary isolation 100% verified.
- **9. Dependencies & Risks:** Depends on Batch 1 intent types.

---

### Batch 3: Grounded Answer Synthesis, Fact/Inference Separation & Citation Engine

- **1. Objective & Outcome:** Synthesize natural language answers grounded strictly in retrieved evidence, with structural separation of facts from inferences, explicit entity citations, and credential redaction.
- **2. Components to Reuse:**
  - `ai_router::AiInfrastructure` (for natural language phrasing, using `AiTask::GeneralAi`).
  - Existing credential-detection regexes.
- **3. Exact Files Likely to Change:** `services/orchestrator/src/ai_router.rs` (if adding a dedicated `AiTask::QaCopilot`).
- **4. New Files:**
  - `services/orchestrator/src/copilot/synthesizer.rs`
  - `services/orchestrator/src/copilot/redaction.rs`
  - `services/orchestrator/src/copilot/synthesizer_tests.rs`
- **5. API / Data Model / Storage:**
  - Struct `CopilotResponse`:
    - `status: CopilotResponseStatus` (`Answered`, `InsufficientData`, `ClarificationNeeded`)
    - `summary: String` (concise natural language answer)
    - `facts: Vec<FactStatement>` (statements backed 1:1 by retrieved database rows)
    - `inferences: Vec<InferenceStatement>` (model-derived conclusions or recommendations)
    - `citations: Vec<EvidenceCitation>` (`entity_id`, `entity_type`, `run_id`, `timestamp`, `uri`)
    - `freshness_note: Option<String>`
  - Redaction engine: strips API keys, bearer tokens, passwords, and connection strings from inputs and outputs.
  - Prompt injection shield: wraps untrusted database text (e.g. bug descriptions, requirement prose) in boundary markers, instructing the model to treat them purely as data.
  - 0 database storage: zero storage of prompt traces or chain-of-thought.
- **6. Tests:**
  - Groundedness verification: asserts that facts contain only values present in `RetrievedEvidence`.
  - Empty evidence test: asserts `status == InsufficientData` without hallucinating metrics.
  - Prompt injection containment test: simulated adversarial text in a requirement cannot hijack the copilot instructions.
  - Redaction tests: API tokens and credentials in test text are masked (`[REDACTED]`).
  - Structural validation test: facts and inferences serialize to distinct JSON fields.
- **7. Security & Boundaries:** Strict prompt boundaries; complete credential sanitization; no autonomous mutation tools offered to the model.
- **8. Acceptance Criteria:** Facts and inferences strictly separated; 100% of citations resolve to valid IDs; zero ungrounded assertions produced.
- **9. Dependencies & Risks:** Depends on Batch 1 and Batch 2.

---

### Batch 4: API Route, RBAC Governance & End-to-End Test Suite

- **1. Objective & Outcome:** Expose the copilot via a public REST endpoint under full authentication, role-based authorization, request timeout, and rate-limiting governance.
- **2. Components to Reuse:**
  - `auth::AuthContext`, `auth::Role`, `auth::require_role`
  - `api::AppState`
  - Axum routing and error models.
- **3. Exact Files Likely to Change:**
  - `services/orchestrator/src/api.rs` (register route, handler)
  - `services/orchestrator/src/lib.rs` (public exports)
- **4. New Files:**
  - `services/orchestrator/src/copilot/api.rs`
  - `services/orchestrator/src/copilot/e2e_tests.rs`
- **5. API / Data Model / Storage:**
  - Endpoint: `POST /api/v1/projects/:project_id/copilot/ask`
  - Payload: `{ "question": "Why did this release fail?", "context_window": null }`
  - Response: `CopilotResponse` JSON.
  - Auth: Bearer token required. Minimum role: `Role::Viewer`.
  - Scoping: `project_id` must match `auth.project_id` (unless Admin).
  - Execution timeout: 30-second bounded cancellation.
- **6. Tests:**
  - End-to-end HTTP tests via `axum::test` / `TestClient`.
  - Authentication tests: 401 on missing token, 403 on project mismatch.
  - Timeout tests: slow model responses fail cleanly without hanging.
  - Comprehensive suite exercising all 11 required roadmap scenarios.
  - Strict tempdir lifecycle verification: delta = 0.
- **7. Security & Boundaries:** Full RBAC matrix validation; project scope enforcement; rate limiting.
- **8. Acceptance Criteria:** API endpoint live, authenticated, project-isolated; all 11 roadmap verification scenarios pass.
- **9. Dependencies & Risks:** Depends on Batches 1, 2, and 3.

---

### Batch 5: Documentation Rollup & Phase 10 PASS Gate

- **1. Objective & Outcome:** Formally verify and document the completion of Phase 10.
- **2. Components to Reuse:** All Phase 10 artifacts, reports, and tests.
- **3. Exact Files Likely to Change:** None (documentation only).
- **4. New Files:**
  - `docs/next-intelligence/PHASE_10_QA_COPILOT_REPORT.md`
  - `docs/next-intelligence/PHASE_10_PASS.md`
- **5. Verification:**
  - Full backend test suite run (`cargo test`).
  - Formatting verification (`cargo fmt --check`).
  - Clippy verification (`cargo clippy --all-targets`).
  - Multi-run artifact leak check (delta = 0).
- **6. Acceptance Criteria:** Full suite green; all 11 required tests verified; zero test-artifact leaks; PASS gate criteria met.
- **7. Dependencies & Risks:** Depends on Batches 1–4.

---

## 5. Recommended First Implementation Batch

### Recommended: **Batch 1 (Intent Classification & Query Dispatch Architecture)**

#### Rationale:
1. **Zero Downstream Blockers:** Batch 1 has no prerequisites. It establishes the typed semantic taxonomy that Batches 2, 3, and 4 depend upon.
2. **Pure & Deterministic:** Intent classification and question normalization can be developed as pure Rust functions without database side effects, connection locks, or external network calls.
3. **Low Risk & Fast Verification:** It can be exhaustively unit-tested across all 8 canonical questions, prompt injections, and ambiguous edge cases with instantaneous test execution and zero artifact leakage.
4. **Firm Architectural Boundary:** Defining the typed `CopilotIntent` first guarantees that the subsequent evidence retrieval engine (Batch 2) is purpose-built to fetch only what the intent requires, preventing wasteful over-fetching or ungrounded queries.

---

## 6. Verification & Safety Guardrails

During future implementation, the following invariants must be maintained:
1. **Zero Test-Artifact Leakage:** Every integration test allocating a SQLite temporary directory must explicitly await `store.pool.close().await` before test completion, adhering to the standard established in Phase 9.2.
2. **No Arbitrary SQL:** The Copilot must strictly query the database through existing, typed `Store` and `QaMemoryReader` functions. No raw SQL strings or user-interpolated queries may be introduced.
3. **Stateless Governance:** No user conversations or intermediate reasoning traces may be persisted to the database.
4. **Preservation of Predecessors:** Phase 1 through Phase 9 implementations and test files must remain untouched.

---
*End of Planning Document.*
