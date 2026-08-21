# AIQA — Phase 4 — FE-15 Product & Contract Decision Resolution Audit Report

## 1. Status

**PASS.**

## 2. Audit Scope

Read-only investigation of four remaining product/contract decisions blocking further implementation: GAP-02 (Test Generator simulation vs. real Model Gateway), GAP-03 (`INFRA_ERROR` vs. `INFRASTRUCTURE_ERROR` naming), GAP-04 (Acceptance Matrix staleness/ownership), GAP-05 (realtime event vocabulary mismatch). No implementation. No document edits. Only this report was created.

## 3. Read-Only Compliance

| | Result |
|---|---|
| Frontend code changes | **None** |
| Backend code changes | **None** |
| Database changes | **None** |
| Document changes (contract, Acceptance Matrix, PRD/Business Logic/Flow Diagram) | **None** |

Confirmed via `git status --short` (39 lines, identical composition to FE-14's own end-state: the same 31 modified files and 8 untracked entries, `+1` for this report) and `git diff --stat -- services/orchestrator/ forentend/src/` producing no output beyond the pre-existing FE-1…FE-14 diff. No file was created other than this report.

## 4. FE-14 Baseline Verification

Re-confirmed by fresh inspection, not assumed:
- `grep -R "aiqaApi.projects.create" forentend/src` → exactly 3 matches: `ProjectSelector.tsx` (the UI consumer), `ProjectContext.tsx` (the one method that calls it), `aiqaApi.test.ts` (tests). **No duplicate transport.**
- `ProjectContext.createProject` still appends the real backend response directly to `projects` state and selects it — unchanged since FE-14.
- `git diff --stat -- services/orchestrator/` → empty. **No backend modification.**

**GAP-01 remains closed.**

## 5. Executive Summary

All four gaps are real and confirmed by fresh evidence this unit, not carried over from memory. None can be safely implemented without a product-owner decision:

- **GAP-02 (Test Generator)** is the most consequential. Fresh inspection of `TestCaseGeneratorPage.tsx` shows the generator is 100% local (three chained `setTimeout` calls, zero `aiqaApi` import), presents hardcoded real commercial model IDs (`gemini-2.0-flash`, `gemini-1.5-pro`, `claude-3-5-sonnet`) as selectable, and literally writes `"Synthesized via {model} ({depth} depth)"` into each generated case's traceability metadata — a false claim, since nothing is synthesized. `handleExportToExecution` creates a **fully fabricated** local `ExecutionRun` (fake IDs, fake counts, fake test names unrelated to what was generated) that the real Execution Hub never reads. The PRD/Business Logic/Acceptance Matrix are unambiguous and repeated: *"All agents shall use an AI Model Gateway rather than hard-coding a provider"* and *"never fabricated output"*. This is a direct, documented conflict, not a matter of interpretation.
- **GAP-03 (`INFRA_ERROR`)** is real but low-stakes: the backend value is real, tested, and already persisted in the database; the contract's `INFRASTRUCTURE_ERROR` is a documentation preference with zero runtime backing. Updating the contract document is essentially risk-free; renaming the backend is a real breaking change touching 15+ call sites and requiring a data-migration story for already-persisted rows.
- **GAP-04 (Acceptance Matrix)** — the entire 22-row Master Feature Matrix (§5) and the entire Final Verification Record (§34) are still their original 100%-`NOT_STARTED`/`NOT_VERIFIED` template, confirmed by reading the full 1093-line document this unit. Re-evaluating each item against current code/tests (not the historical FE-13 report) surfaces one genuinely new finding: ACC-007's execution layer is more real than previously documented — five real subprocess-based executors (Playwright, Newman, Pytest, Maestro, Schemathesis) exist, are registered, and are UI-selectable, but their real-tool invocation was not independently re-exercised this session, and Maestro's own code comment discloses mobile execution has never been genuinely verified in this environment (no device/emulator).
- **GAP-05 (realtime vocabulary)** is unchanged from FE-13's own finding: the contract's vocabulary is explicitly written with "SHOULD"/"at minimum...should support" (soft) language, while the real backend vocabulary is a smaller, different, already-tested 11-value set. This is genuinely a product decision, not a defect.

No option was chosen. No document was edited. Four decision packages are ready in §13.

## 6. Source Inventory

**Tier 1 (read in full this unit):**
- `AIQA_Consolidated_PRD_FRD_SRD_v6.0_Hybrid_Multi_Agent (1).pdf` (4 pages)
- `AIQA_Business_Logic_Architecture_v2.0_Hybrid_Multi_Agent.pdf` (4 pages)
- `AIQA_Complete_End_to_End_Flow_Diagram_v6.0.pdf` (6 pages)

**Tier 2 (read in full this unit):**
- `AIQA_API_CONTRACT_v1.0.md` (targeted sections: §13 Canonical Execution Status, §14 Realtime Event Contract, §32 Current Implementation Status — full text reproduced in §8/§10 below)
- `AIQA_ACCEPTANCE_MATRIX_v1.0.md` (all 1093 lines read in full)

**Tier 3 (consulted, not re-derived from):**
- `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md`
- `AIQA_POST_FE13_FE14_READINESS_CAPABILITY_GAP_AUDIT_REPORT.md`
- `AIQA_PHASE4_FE13_DOCUMENTATION_CONTRACT_DRIFT_DEAD_CODE_CLEANUP_IMPLEMENTATION_REPORT.md`

**Tier 4 (read/grepped fresh this unit):**
- `forentend/src/components/pages/TestCaseGeneratorPage.tsx` (full file)
- `forentend/src/context/AIQAContext.tsx` (`triggerNewRun`, `ExecutionRun` fabrication)
- `forentend/src/types.ts` (`GeneratedTestCase` shape)
- `forentend/src/types/contract.ts`, `forentend/src/context/RealtimeContext.tsx` (full doc comment), `forentend/src/utils/statusLabel.ts`/`.test.ts`, `forentend/src/components/pages/ExecutionHubPage.tsx`
- `services/orchestrator/src/domain.rs`, `engine.rs`, `executor.rs` (full `executor.rs` real-adapter section), `intelligence.rs`, `policy.rs`, `api.rs`, `main.rs`, `engine_tests.rs` (recovery test inventory)
- `executors/{playwright,newman,pytest,maestro,schemathesis}/` (directory contents)
- `npx tsc --noEmit`, `npx vitest run` (272/272 passing)

## 7. GAP-02 — Test Generator Decision

### 7.1 Current Runtime Reality

`TestCaseGeneratorPage.tsx::handleTriggerGenerate` is entirely local:
```
setGenerationState('VALIDATING') → setTimeout 1000ms
  → setGenerationState('PROCESSING') → setTimeout 1400ms
    → setGenerationState('GENERATING') → setTimeout 1800ms
      → hardcoded per-category template text assembled into GeneratedTestCase[]
```
No `aiqaApi` import exists anywhere in this file. The model selector (`'gemini-2.0-flash' | 'gemini-1.5-pro' | 'claude-3-5-sonnet'`) is a plain local `useState` — selecting a value has no effect beyond being echoed into the output. The header banner reads **"GEMINI 2.0 FLASH REASONING"**. Each generated case's `traceability.aiAnalysis` field is set to `` `Synthesized via ${model} (${depth} depth)` `` — a literal, specific, false claim of AI synthesis. The actual test content (title/objective/steps/assertions) is a fixed `switch (cat)` template per category — the uploaded documents' real content (`docLabels`, `targetUrl`) is interpolated cosmetically into strings but never actually analyzed.

`handleExportToExecution` calls `AIQAContext.triggerNewRun`, which fabricates a complete local `ExecutionRun`: a random `run-####` id, hardcoded `totalTests: 10`, hardcoded counts (`passed: 1, running: 3, queued: 6`), and two fake named tests (`"Smoke: Healthcheck ping to ingress router"`, `"Security: TLS 1.3 Cipher Suite negotiation check"`) — unrelated to the categories actually generated. This object is stored in `AIQAContext.runs`, a state store the real `ExecutionHubPage` (which reads only `aiqaApi.executions.*`) never consumes. The user is then `navigate('/execution')`'d to the real Execution Hub, which shows nothing related to what was just "exported."

Approve/reject actions (`handleApproveTestCase`/`handleRejectTestCase`) are local `setGeneratedCases` mutations only — no backend persistence call exists for generated test cases at all.

### 7.2 Backend Capability

`grep -riE "openai|anthropic|gpt-|claude-|gemini|llm|model.gateway|inference|ollama|prompt" services/orchestrator/src` matches exactly 3 files (`api.rs`, `intelligence.rs`, `policy.rs`), and every match is an explicit **disclaimer**, not integration:
- `intelligence.rs`: *"Deliberately not LLM-backed: the task this session was scoped under explicitly leaves Ollama/LLM integration out... root-cause hypotheses here come from a small rule-based classifier over the failure text, not a model call."*
- `policy.rs`: *"There is no AI judgment here (Ollama/LLM integration is out of scope this session)."*
- `api.rs`: *"...no model gateway) [in this schema]. They are intentionally omitted rather than faked."*

**Zero real LLM/Model Gateway backend capability exists.** No generation endpoint, no generated-test persistence table, no provider abstraction of any kind.

### 7.3 Product Requirement

Quoted directly from the Tier-1 documents (read in full this unit):
- PRD §5 "Model Gateway": *"All agents shall use an AI Model Gateway rather than hard-coding a provider. The gateway supports local inference plus API fallback and runtime telemetry."*
- SRD-MDL-01: *"Agents call the Model Gateway, not providers directly."*
- SRD-MDL-05: *"Model/provider failure results in controlled fallback or review state; never fabricated output."*
- Business Logic §12: *"All agents use one AI Model Gateway; no provider is hard-coded... Provider/model failure triggers controlled fallback or NEEDS_REVIEW, never fabricated output."*
- Flow Diagram: an explicit **"AI MODEL / MODEL ROUTER"** node terminates the top-level product flow; a separate **"AI MODEL GATEWAY"** node is shown wired to Dashboard via `reports`/`results` edges.
- Acceptance Matrix ACC-018: *"No testing intelligence may hard-code a provider as its application-level dependency... Never fabricated output."* E2E-006 "Model Failure": *"Never fabricated output."* §32 Fake/Stub/Placeholder Removal Gate explicitly lists **"fabricated AI results"** and **"simulated successful execution"** as unacceptable in production paths.

### 7.4 Exact Drift

Two distinct, independently-documented rules are both currently violated by the same page:
1. **"No hard-coded provider"** — the UI's own model dropdown *is* a hard-coded provider selector.
2. **"Never fabricated output"** — `aiAnalysis: "Synthesized via gemini-2.0-flash..."` is a specific, false claim of real synthesis, and the export path fabricates a complete fake execution run.

This is Type **C — Frontend implementation drift**, matching FE-13's own DRIFT-013 classification, now reconfirmed against the actual PRD/Business Logic text rather than inferred.

### 7.5 Option A — Truthful Simulation Disclosure

Keep local generation; add explicit, visible disclosure that no real model is called (e.g., replace "GEMINI 2.0 FLASH REASONING" banner and the `aiAnalysis` claim with honest language, remove the model-selector's implication of real routing, or replace it with a "generation profile" label).

- **Code impact**: small — confined to `TestCaseGeneratorPage.tsx` and possibly `types.ts`'s `traceability.aiAnalysis` field semantics. No backend change.
- **UX impact**: the page becomes visibly less "AI-powered" in appearance; may be seen as a product regression in polish even though it removes a real trust problem.
- **Product-requirement satisfaction**: **PARTIAL.** Resolves the "never fabricated output" honesty violation, but does **not** satisfy "all agents shall use an AI Model Gateway" or "generate from project context" (FRD §7) — those requirements remain permanently unmet for this module under Option A.

### 7.6 Option B — Build Real Model Gateway

**Existing evidence for what this would require** (backend, none of it built): a provider abstraction layer, a model router (task type/context/latency/availability/cost — SRD-MDL-03), API key management for at least one real provider, prompt contracts (Agent Contract, SRD §12: *"Large raw artifacts shall not be sent wholesale through the LLM; targeted context shall be used"*), a real generation API endpoint, persistence for generated tests (no such table exists today), a `GeneratedTestCase` schema the backend actually owns, security controls around prompt/response handling, error/retry/fallback behavior (SRD-MDL-05), rate limiting, cost controls, and — critically — a `GeneratedTestCase → TaskDef` mapping for the Execution handoff (see §17 below).

**This is necessary future architecture inference, not existing evidence.** No part of this list exists in the current backend. Its scope (a new AI layer, a new persistence domain, a new security surface) is comparable in size to the entire existing `services/orchestrator` control plane and would require its own dedicated architecture/implementation phase(s) — not something to design speculatively inside this audit.

### 7.7 Dependencies

Building Option B would gate: the Test Generator → Execution real handoff (§17), and transitively the AIQA Orchestrator and Test Case Validator modules (both currently `unsupported-not-implemented`, and both documented as depending on real agent/Model-Gateway output to have anything real to route/validate).

### 7.8 Technical Recommendation

Option A is the lower-risk, immediately-actionable step (small, frontend-only, closes the "fabricated output" violation this unit found). It does not, however, close the underlying capability gap the PRD documents — that remains Option B's territory, and Option B is out of proportion for a single FE unit. **This is a technical observation, not a decision** — see §9.9 for GAP-11's already-established out-of-scope classification of the Orchestrator/Validator that would depend on this.

### 7.9 Product Decision Required

**PRODUCT DECISION REQUIRED.** See Decision 1, §13.

## 8. GAP-03 — Error Vocabulary Decision

### 8.1 Actual Implementation

`domain.rs::TaskStatus::InfraError` serializes as `"INFRA_ERROR"` (`as_db_str`, line 48) and is produced by: `executor.rs` (`SimulatedExecutor`, all 5 real executors' failure path via `outcome_from_output`), `engine.rs` (recovery/finalization logic, comment: *"marked INFRA_ERROR rather than left QUEUED forever"*), and read back by `store.rs`. The corresponding event, `EventType::TaskInfraError`, serializes as `"TASK_INFRA_ERROR"`. Both values are exercised by real, passing tests (`executor.rs`'s own unit test, `engine_tests.rs` lines 143/784/798/1335).

### 8.2 Contract Requirement

`AIQA_API_CONTRACT_v1.0.md` §13 "Canonical Execution Status" lists 9 values including **`INFRASTRUCTURE_ERROR`** (not `INFRA_ERROR`), followed by: *"Frontend MUST NOT create alternate spellings for these statuses."* — MUST-level language. §32 "Current Implementation Status" separately states the contract *"does NOT claim that every endpoint above already exists"* — a general disclaimer about endpoint existence, not a specific exemption for §13's naming rule.

### 8.3 Complete Reference Inventory

| Layer | File | Reference |
|---|---|---|
| Backend producer | `domain.rs:48,62` | `TaskStatus::InfraError` ↔ `"INFRA_ERROR"` |
| Backend producer | `domain.rs:246,262,277` | `EventType::TaskInfraError` ↔ `"TASK_INFRA_ERROR"` |
| Backend producer | `engine.rs` (5 sites), `executor.rs` (5 sites), `store.rs` | all produce/consume the same real string |
| Backend tests | `engine_tests.rs` (4 sites), `executor.rs` (1 site) | assert on `TaskStatus::InfraError` |
| Frontend type | `types/contract.ts:30-43` | `TaskStatus` union includes `'INFRA_ERROR'`, doc comment explicitly notes the naming gap |
| Frontend type | `types.ts:31` | same value in the pre-FE-1 vocabulary |
| Frontend consumer | `utils/statusLabel.ts`/`.test.ts` | formats `'INFRA_ERROR'` → `"INFRA ERROR"` |
| Frontend consumer | `ExecutionHubPage.tsx:31,34,192` | styling map keyed on `INFRA_ERROR`, doc comment cites the same gap |
| Contract | `AIQA_API_CONTRACT_v1.0.md:523` | lists `INFRASTRUCTURE_ERROR` |
| Legacy reference app | `apps/web/` (5 files) | a separate, non-active package (`aiqa-web`, no workspace linkage to `forentend`) — not in scope for this decision, noted only for completeness |

Zero alternate spellings exist inside `forentend/` — frontend and backend already agree perfectly with each other; only the contract document disagrees with both.

### 8.4 Option A — Rename Implementation

Rename `TaskStatus::InfraError`'s wire value to `INFRASTRUCTURE_ERROR` (and, for consistency, `EventType::TaskInfraError` to something matching). This is a **breaking change**: every file in §8.3's backend/frontend rows would need updating, all four `engine_tests.rs` assertions and the `executor.rs` test would need updating, and — critically — **already-persisted rows in `orchestrator.sqlite3`'s `tasks`/`run_events` tables contain the literal string `"INFRA_ERROR"`**. Without a migration or a backward-compatible deserialize alias, historical records become silently unparseable or mismatched against a renamed enum.

### 8.5 Option B — Update Contract

Edit `AIQA_API_CONTRACT_v1.0.md` §13 to list `INFRA_ERROR` instead of `INFRASTRUCTURE_ERROR`. Zero runtime impact, zero code touched, zero test touched, zero migration. A markdown edit only.

### 8.6 Risk Comparison

| | Option A (rename backend) | Option B (update contract) |
|---|---|---|
| Files touched | 15+ across backend/frontend/tests | 1 (markdown) |
| Runtime risk | Real — breaking change to a shipped, tested value | None |
| Data migration needed | Yes (persisted historical rows) | No |
| Test changes needed | Yes (5+ assertions) | No |

### 8.7 Technical Recommendation

Option B is essentially risk-free; Option A is a real breaking change with a data-migration obligation for no functional gain. **This is a technical observation, not a decision** — the contract document is explicitly out of this unit's edit authority.

### 8.8 Decision Required

**DOCUMENTATION APPROVAL REQUIRED**, not a product decision in the same sense as GAP-02 — but still requires a document owner to accept the change (this audit cannot edit `AIQA_API_CONTRACT_v1.0.md` itself). See Decision 2, §13.

## 9. GAP-04 — Acceptance Matrix Audit

### 9.1 Current Matrix State

Confirmed by reading all 1093 lines: §5's Master Feature Matrix (22 rows × 6 columns = 132 cells) is **100% `NOT_STARTED`**. §34 "Final Verification Record" (11 fields) is **100% `NOT_VERIFIED`**. §35 "Final Sign-Off" (Backend/Frontend/Final Acceptance) is **100% `NOT_VERIFIED`**. This is the document's original, never-touched template — confirmed stale, not merely suspected.

### 9.2 Complete Acceptance Item Matrix

| ID | Requirement (from matrix text) | Matrix Status | Actual Current Status | Evidence Source | Recommended Status | Confidence |
|---|---|---|---|---|---|---|
| ACC-001 | Create/select project, project-scoped isolation, backend-enforced | NOT_STARTED | Creation + selection + isolation all real | FE-14 report (11 tests, live 403/zero-project verification), FE-5 tests (6 passing), `require_project_access` backend | **VERIFIED PASS** | HIGH |
| ACC-002 | Context Ingestion (PRD/FRD/SRD/Figma/API/DB/repo upload+processing) | NOT_STARTED | No backend route, no processing pipeline | Zero `/context`/`/ingest` routes in `api.rs`'s 38-route table | **NOT IMPLEMENTED** | HIGH |
| ACC-003 | Quality Graph (requirement→feature→module→...→report traceability) | NOT_STARTED | No backend entity/route | Same route-table check | **NOT IMPLEMENTED** | HIGH |
| ACC-004 | Test Case Generator — generate from real context, quality/traceability | NOT_STARTED | Local simulation only, fixed templates, no real context analysis | §7.1 this report | **VERIFIED PARTIAL** (simulated only) | HIGH |
| ACC-005 | Test Case Validator — quality gates before execution | NOT_STARTED | No backend/frontend implementation; correctly disclosed "not implemented" page | `routeConfig.tsx` `designStatus: 'unsupported-not-implemented'` | **NOT IMPLEMENTED** | HIGH |
| ACC-006 | AIQA Orchestrator — routing/prioritization/agent coordination | NOT_STARTED | No backend/frontend implementation; correctly disclosed | Same | **NOT IMPLEMENTED** | HIGH |
| ACC-007 | Test Execution Hub — DAG, queues, workers, Playwright/API/DB/Security/Mobile workers | NOT_STARTED | DAG/queue/durable-state REAL and tested; 5 real subprocess executors (Playwright/Newman/Pytest/Maestro/Schemathesis) exist, registered, UI-selectable — but no dedicated DB or Security-named executor exists, and Maestro's own code comment discloses mobile execution was never genuinely verified (no device/emulator) | `executor.rs` full read, `main.rs` registration, `executors/*` real project dirs, `ExecutionHubPage.tsx` `EXECUTOR_OPTIONS` | **VERIFIED PARTIAL** (upgraded detail vs. FE-13's blanket "core verified") | MEDIUM — real-tool invocation not re-exercised live this session |
| ACC-008 | Realtime Events — correlation, ordering, reconnect reconciliation | NOT_STARTED | Real WebSocket, real dedup/sequence, reconnect via full refetch (not `?since=` replay) | `RealtimeContext.tsx` full doc comment (fresh read), `ExecutionHubPage.tsx` reconnect effect | **VERIFIED PARTIAL** (real, but vocabulary narrower than documented — see §10) | HIGH |
| ACC-009 | Evidence — persisted, traceable, retrievable | NOT_STARTED | Real, live-verified in FE-14 manual verification | FE-14 report §15 | **VERIFIED PASS** | HIGH |
| ACC-010 | Failure/RCA Intelligence — correlation, AI-hypothesis distinguishability | NOT_STARTED | Real, rule-based (never LLM), `NEEDS_REVIEW` for uncertain findings | `intelligence.rs` full read this unit | **VERIFIED PASS** | HIGH |
| ACC-011 | RBAC Testing — role×permission×action coverage tool | NOT_STARTED | Enforcement real (server-side RBAC exists); dedicated RBAC *testing* module not implemented | `routeConfig.tsx` | **VERIFIED PARTIAL** | HIGH |
| ACC-012 | API Testing module | NOT_STARTED | Not implemented as a dedicated module (though `NewmanExecutor`/`SchemathesisExecutor` provide real API-testing *execution* capability at the Execution Hub level) | §9.2 ACC-007 evidence | **NOT IMPLEMENTED** (as a module); **AMBIGUOUS** whether Execution Hub's executor options satisfy this | MEDIUM |
| ACC-013 | API Latency/Telemetry | NOT_STARTED | Only a generic unused Prometheus `/metrics` endpoint; no UI-level telemetry | Prior FE-13 finding, unchanged | **NOT IMPLEMENTED** | HIGH |
| ACC-014 | Mobile Testing module | NOT_STARTED | Not implemented as a module; `MaestroExecutor` exists but self-discloses no genuine mobile flow was ever verified | `executor.rs:333-337` | **NOT IMPLEMENTED** | HIGH |
| ACC-015 | Custom Testing | NOT_STARTED | Not implemented | `routeConfig.tsx` | **NOT IMPLEMENTED** | HIGH |
| ACC-016 | Reports — Bug/Test/Final Audit, PDF/Excel export | NOT_STARTED | Findings/FailureCluster JSON export only | FE-13 §16 (re-confirmed unchanged) | **VERIFIED PARTIAL** | HIGH |
| ACC-017 | Dashboard/KPIs | NOT_STARTED | 2/11 documented KPI concepts real | FE-13 finding, unchanged | **VERIFIED PARTIAL** | HIGH |
| ACC-018 | AI Model Gateway | NOT_STARTED | Not implemented — see GAP-02 | §7.2 | **NOT IMPLEMENTED** | HIGH |
| ACC-019 | Settings/Configuration | NOT_STARTED | Real API-key panel; rest disclosed local-only | FE-13 finding, unchanged | **VERIFIED PARTIAL** | HIGH |
| ACC-020 | Audit/Governance | NOT_STARTED | Real, live-verified in FE-14 (including this unit's own `project.create` actions appearing in the real audit log) | FE-14 report §15 | **VERIFIED PASS** | HIGH |
| ACC-021 | Security/Project Isolation | NOT_STARTED | Real, server-side RBAC and project isolation confirmed live (FE-14's scoped-key testing) | FE-14 report §15 | **VERIFIED PASS** | HIGH |
| ACC-022 | Persistence/Recovery/Fault Tolerance | NOT_STARTED | 12 real automated backend tests cover worker-restart/orphaned-task/concurrent-recovery scenarios (`engine_tests.rs`); no live full-process-kill manual verification performed | `engine_tests.rs` fresh grep, this unit | **VERIFIED PARTIAL** (upgraded from FE-13's "AMBIGUOUS" — automated coverage is real, but live-process verification is genuinely missing) | MEDIUM |

### 9.3 Independently Verified Items

ACC-001, ACC-009, ACC-010, ACC-020, ACC-021 — VERIFIED PASS with fresh, current-session or immediately-prior-session (FE-14) evidence.

### 9.4 Items Requiring Live Verification

- ACC-007: real Playwright/Newman/Pytest/Maestro/Schemathesis execution has not been live-exercised (only code/config existence confirmed).
- ACC-022: a genuine kill-the-real-process-and-restart scenario has not been performed (only automated in-process recovery tests).
- ACC-012: whether the Execution Hub's real API-testing executors satisfy the *documented* "API Testing module" requirement is a scope-interpretation question, not a pure verification one.

### 9.5 Ambiguous Items

ACC-012 (does Execution Hub's executor selection satisfy the documented "API Testing" module, or does the module still need its own dedicated UI/dashboard as Business Logic §2 implies?) — **AMBIGUOUS, PRODUCT DECISION REQUIRED** to resolve, not a verification gap.

### 9.6 Recommended Status Corrections

The table in §9.2 is the recommended correction set. It can be **mechanically** applied for ACC-001/009/010/020/021 (clear evidence, no interpretation needed) and ACC-002/003/005/006/012(partially)/013/014/015/018 (clear absence, no interpretation needed). ACC-007, ACC-011, ACC-016, ACC-017, ACC-019, ACC-022 require a document owner to accept a **partial/nuanced** status (not mechanical) since "partial" requires judgment about how much counts. ACC-012 requires actual product interpretation before any status can be assigned at all.

### 9.7 Document Ownership / Approval Required

Per the matrix's own §35 "Final Sign-Off": Backend Owner (Claude), Frontend Owner (Google AI Studio), Final Acceptance (Project QA/Product Owner). This audit is not any of those roles — it can only recommend, not sign off.

## 10. GAP-05 — Realtime Vocabulary Decision

### 10.1 Actual Backend Vocabulary

`domain.rs::EventType` (11 real values, confirmed via fresh grep this unit): `RUN_SCHEDULED`, `TASK_STARTED`, `TASK_PASSED`, `TASK_FAILED`, `TASK_BLOCKED`, `TASK_SKIPPED`, `TASK_CANCELLED`, `TASK_TIMEOUT`, `TASK_INFRA_ERROR`, `TASK_NEEDS_REVIEW`, `RUN_COMPLETED`. Per `RealtimeContext.tsx`'s own doc comment (read in full this unit): `RUN_SCHEDULED` fires once at true execution start; `RUN_COMPLETED` fires once, terminal, for *every* end reason (completed/partial/failed/cancelled/rejected) and **never carries the run's final state** — a consumer must re-fetch to know which. There is no dedicated approval/rejection/cancellation event type.

### 10.2 Actual Frontend Consumers

`RealtimeContext.tsx` deduplicates by `event_id`, tracks `sequence`/`project_sequence`. Reconnect reconciliation happens **one layer up**, in each consumer (e.g. `ExecutionHubPage.tsx`'s own effect calls a full `fetchRuns`/`fetchDetail` refetch on reconnect) — a full resync, not a partial `?since=` replay, even though `aiqaApi.executions.events(...,since)` exists real and typed but has zero call sites.

### 10.3 Contract Vocabulary

`AIQA_API_CONTRACT_v1.0.md` §14 "Realtime Event Contract" (read in full this unit) uses explicitly **soft** language: *"WebSocket or equivalent event transport **SHOULD** expose..."* and *"At minimum, the system **should support** normalized events for:"* — followed by 15 named types: `RUN_CREATED`, `RUN_STATUS_CHANGED`, `TASK_CREATED`, `TASK_STATUS_CHANGED`, `TASK_PROGRESS`, `EVIDENCE_CREATED`, `FINDING_CREATED`, `MODEL_REQUEST`, `MODEL_RESPONSE`, `MODEL_FALLBACK`, `WORKER_STATUS_CHANGED`, `API_TELEMETRY`, `RBAC_ACCESS_PATH`, `REPORT_READY`, `ERROR`.

### 10.4 Complete Event Mapping Matrix

| Actual backend event | Current frontend handling | API Contract equivalent | Match? | Runtime status |
|---|---|---|---|---|
| `RUN_SCHEDULED` | Consumed, triggers run-list resync | `RUN_CREATED` / `RUN_STATUS_CHANGED` | No | Real, working |
| `TASK_STARTED` | Consumed | `TASK_STATUS_CHANGED` | Partial (contract collapses into one type) | Real, working |
| `TASK_PASSED`/`FAILED`/`BLOCKED`/`SKIPPED`/`CANCELLED`/`TIMEOUT`/`INFRA_ERROR`/`NEEDS_REVIEW` | Consumed (8 distinct types) | `TASK_STATUS_CHANGED` (1 type) | No (granularity mismatch) | Real, working |
| `RUN_COMPLETED` | Consumed, triggers resync; consumer must re-fetch for final state | `RUN_STATUS_CHANGED` | No | Real, working, documented limitation |
| — (none) | — | `TASK_PROGRESS` | N/A | Not implemented — no incremental progress signal exists |
| — (none) | — | `EVIDENCE_CREATED` | N/A | Not implemented as a distinct event (Evidence is fetched via REST, not pushed) |
| — (none) | — | `FINDING_CREATED` | N/A | Not implemented as a distinct event |
| — (none) | — | `MODEL_REQUEST`/`MODEL_RESPONSE`/`MODEL_FALLBACK` | N/A | Not implemented — no Model Gateway exists (GAP-02) |
| — (none) | — | `WORKER_STATUS_CHANGED` | N/A | Not implemented |
| — (none) | — | `API_TELEMETRY` | N/A | Not implemented (GAP: ACC-013) |
| — (none) | — | `RBAC_ACCESS_PATH` | N/A | Not implemented |
| — (none) | — | `REPORT_READY` | N/A | Not implemented (no async report generation exists) |
| — (none) | — | `ERROR` | N/A | Not implemented as a distinct event type |

### 10.5 Option A — Remap Backend

Rename/restructure `EventType` to match the contract's 15-type vocabulary. **Breaking change**: touches `domain.rs`, every `record.emit(...)` call site in `engine.rs`, `store.rs`, `ws_tests.rs`, `engine_tests.rs`, the frontend's `EventType` union in `contract.ts`, and `RealtimeContext.tsx`'s consumption logic. Client compatibility risk: any already-deployed frontend build expecting the old vocabulary would silently stop receiving recognized events. A real migration strategy (dual-emit during a transition window, or a version flag) would be needed — none exists today.

### 10.6 Option B — Update Contract

Rewrite §14 to describe the real 11-value vocabulary and the real `RUN_COMPLETED`-never-carries-final-state behavior. Zero runtime changes required. The contract's own "SHOULD"/"should support" phrasing already signals this section was aspirational, making a correction lower-friction than §13's MUST-level language (GAP-03).

### 10.7 Risk Comparison

Structurally identical to GAP-03: Option A is a real, multi-file breaking change with client-compatibility risk; Option B is a documentation-only correction with zero runtime risk.

### 10.8 Technical Recommendation

Option B, for the same reasons as GAP-03 — but this is a **technical observation, not a decision**, and unlike GAP-03 this vocabulary also has real product-value questions attached (should `TASK_PROGRESS`/`API_TELEMETRY`/`RBAC_ACCESS_PATH` ever be built for real, independent of naming?) that a pure "update the doc" resolution doesn't answer.

### 10.9 Product Decision Required

**PRODUCT DECISION REQUIRED** (naming) **+ separately, PRODUCT DECISION REQUIRED** (whether the contract's additional event types — progress, telemetry, RBAC access path, report-ready — represent real future scope or should be removed from the contract as never-intended). See Decision 3, §13.

## 11. Cross-Gap Dependency Graph

```
GAP-02 (Test Generator: disclose vs. build Model Gateway)
        ↓ blocks
GAP-02-B (if "build Model Gateway" chosen) → Generated Test Case Contract (does not exist)
        ↓ blocks
GeneratedTestCase → TaskDef Mapping (does not exist — GeneratedTestCase has zero fields
                                       resembling TaskDef's id/prerequisites/simulate/executor;
                                       confirmed by direct type comparison this unit)
        ↓ blocks
Test Generator → Execution real handoff (currently fabricates a fake ExecutionRun instead)
        ↓ blocks (transitively, per prior FE-13/audit classification)
AIQA Orchestrator, Test Case Validator (both currently unsupported-not-implemented,
                                          both documented as needing real agent output to route/validate)

GAP-03 (INFRA_ERROR naming) — independent, no blockers, blocks nothing else.
        If Option A (rename backend) chosen → requires a persisted-data migration story
        (inferred necessity, not existing evidence — no migration tooling exists today).

GAP-05 (realtime vocabulary) — independent, no blockers.
        If Option A (remap backend) chosen → requires a client-compatibility strategy
        (inferred necessity — no versioning/dual-emit mechanism exists today).

GAP-04 (Acceptance Matrix) — depends on → this report's own §9.2 being accepted as a source
        by a document owner; does not block any code work, but blocks having an
        accurate FINAL VERIFIED record (§33 of the matrix itself).

ACC-007's newly-found real executors (Playwright/Newman/Pytest/Maestro/Schemathesis) —
        depends on → live verification (not yet performed) before their status can move
        from VERIFIED PARTIAL to VERIFIED PASS.
```

No relationship above is invented; each is either a direct type/code-level finding from this unit or an explicitly-labeled inference about necessary future work.

## 12. Decision Matrix

| Gap | Actual implementation | Product/contract requirement | Exact conflict | Valid options | Lowest-risk option | Requires product approval? |
|---|---|---|---|---|---|---|
| GAP-02 | 100% local simulation; hardcoded provider names; fabricated export | "All agents shall use an AI Model Gateway... never fabricated output" (PRD §5, SRD-MDL-05, ACC-018) | Direct violation of two documented rules | A: disclose simulation. B: build real Model Gateway | A (small, frontend-only) — but does not fully satisfy the documented requirement | **YES** |
| GAP-03 | `INFRA_ERROR`, real, tested, persisted | Contract §13: `INFRASTRUCTURE_ERROR`, MUST-level | Naming mismatch only; both sides internally consistent | A: rename backend. B: update contract | B (zero runtime risk) | **YES** (document approval) |
| GAP-04 | 22-item matrix, 100% template | Matrix's own completion rule requires real, evidenced status | Document has never reflected 13 FE + 8 BE units of real work | Mechanical update for clear items; interpretation needed for partial/ambiguous items | Mechanical update where possible; live-verify ACC-007/ACC-022 remainder | **YES** (document ownership/sign-off) |
| GAP-05 | 11-value real vocabulary, `RUN_COMPLETED` state-loss | Contract §14: 15-value vocabulary, SHOULD-level | Naming + granularity mismatch; both sides internally consistent | A: remap backend. B: update contract | B (zero runtime risk) | **YES** |

## 13. Decisions Required From Product Owner

### Decision 1 — Test Generator: Disclose or Build Real

**Question:** Should the Test Case Generator disclose that it is a local simulation, or should a real AI Model Gateway be built to make it genuinely functional?

**Option A:** Add clear in-app disclosure that generation is simulated/local and no live AI model is called; remove or reframe the hardcoded model-name selector and the false "Synthesized via..." claim.

**Option B:** Approve and scope a real Model Gateway build — provider abstraction, model router, generation API, persistence, `GeneratedTestCase → TaskDef` mapping, security/rate/cost controls. This is large, multi-phase, backend-and-frontend work.

**Technical recommendation:** Option A closes the immediate honesty violation cheaply; it does not close the documented capability gap. Option B is the only path to genuinely satisfying the PRD, but its scope should not be assumed or speculatively designed further without explicit approval.

**What happens if deferred:** The page continues presenting fabricated AI-generation claims to users, and the Test Generator → Execution handoff remains fake (creates an orphaned fabricated `ExecutionRun` on every "export").

### Decision 2 — `INFRA_ERROR` vs. `INFRASTRUCTURE_ERROR`

**Question:** Should the shipped, tested, persisted backend value be renamed to match the contract, or should the contract be corrected to match the shipped value?

**Option A:** Rename backend enum value — breaking change, needs a migration story for already-persisted rows.

**Option B:** Update `AIQA_API_CONTRACT_v1.0.md` §13 — a markdown edit, zero runtime impact.

**Technical recommendation:** Option B.

**What happens if deferred:** No functional harm — both sides are internally consistent and tested. The only cost of deferral is that the contract document remains provably wrong on this point.

### Decision 3 — Realtime Event Vocabulary

**Question:** Should the backend's real 11-event vocabulary be remapped to the contract's 15-event vocabulary, or should the contract be corrected — and separately, are the contract's extra event types (`TASK_PROGRESS`, `API_TELEMETRY`, `RBAC_ACCESS_PATH`, `REPORT_READY`, `MODEL_*`) real future product scope or should they be removed from the contract?

**Option A:** Remap backend — breaking change, needs a client-compatibility strategy.

**Option B:** Update contract to match real vocabulary, and separately decide the fate of the extra event types (keep as documented-future, or remove as never-intended).

**Technical recommendation:** Option B for naming; the extra-event-types question is a genuine product-scope question this audit cannot answer on its own.

**What happens if deferred:** No functional harm today — same as GAP-03. Deferral risk grows only if more code is built against the real vocabulary while the contract still claims a different one.

### Decision 4 — Acceptance Matrix Ownership

**Question:** Who accepts this report's §9.2 status corrections into the actual `AIQA_ACCEPTANCE_MATRIX_v1.0.md`, and does the "partial"/"ambiguous" set (ACC-007, ACC-011, ACC-012, ACC-016, ACC-017, ACC-019, ACC-022) need additional live verification before being finalized, or is a documented-partial status acceptable as-is?

**Option A:** Accept this report's mechanical corrections now (ACC-001/002/003/005/006/009/010/013/014/015/018/020/021) and schedule live verification for the remainder (ACC-007 real-executor invocation, ACC-022 live process-kill).

**Option B:** Defer all matrix updates until every item — including the currently-clear ones — has fresh live verification performed in one pass.

**Technical recommendation:** Option A — no reason to re-verify already-unambiguous items; the mechanical corrections carry no interpretation risk.

**What happens if deferred:** The matrix continues to misrepresent 13+13 FE/BE units of real, tested work as `NOT_STARTED`, undermining its stated purpose as the "final completion and verification matrix."

## 14. Recommended Next Sequence

**Documentation-only work** (no code, low risk, can proceed once a document owner is assigned):
- Update `AIQA_API_CONTRACT_v1.0.md` §13 (`INFRA_ERROR`) and §14 (real event vocabulary) per Decisions 2/3's Option B.
- Update `AIQA_ACCEPTANCE_MATRIX_v1.0.md` §5 with this report's §9.2 mechanical corrections, per Decision 4.

**Frontend-only work** (gated on Decision 1):
- If Option A chosen: add Test Generator disclosure, remove the fabricated-export path or replace it with an honest "not connected to real execution" state.

**Backend / full-stack work** (gated on Decision 1 Option B, or on separately-approved future scope):
- Real Model Gateway (large, not scoped here).
- Any of the still-unsupported modules (Orchestrator, Validator, RBAC/API/Mobile/Custom Testing modules, report-generation family) — all remain out of scope per the Post-FE-13 audit's own findings, unaffected by this unit.

**Future / out-of-scope work** (not for the next phase):
- Realtime vocabulary remap (Option A of Decision 3), if ever chosen — large, breaking, needs its own migration-strategy unit.
- Live verification of the 5 real executors and a genuine process-kill recovery test — worthwhile, low-risk, but not implementation; could be its own short verification-only unit.

No part of the above was implemented in this unit.

## 15. Verification Commands and Results

| Command | Result |
|---|---|
| `git status --short` | 39 lines; identical composition to FE-14's end-state |
| `git diff --stat -- services/orchestrator/ forentend/src/` | No output beyond the pre-existing FE-1…FE-14 diff |
| `git status --short \| grep "^??"` | 8 untracked entries, all pre-existing report/test files plus this report |
| `npx tsc --noEmit` (forentend) | Clean, exit 0 |
| `npx vitest run` (forentend) | **272/272 passing, 14 files**, exit 0 |
| `grep -R "aiqaApi.projects.create" forentend/src` | 3 matches (real consumer + method + tests), no duplicate |
| `grep -riE "openai\|anthropic\|gpt-\|claude-\|gemini\|llm\|model.gateway\|inference\|ollama\|prompt" services/orchestrator/src` | 3 files, all explicit disclaimers, zero real integration |
| `grep -rn "INFRA_ERROR\|INFRASTRUCTURE_ERROR"` (repo-wide) | 48 files; backend+frontend agree on `INFRA_ERROR`; only the contract doc and one legacy `apps/web` package use `INFRASTRUCTURE_ERROR`/reference it |
| `grep -n "enum EventType\|EventType::"` (`domain.rs`) | 11 real variants confirmed, matches `engine.rs`/`store.rs` producers |
| `grep -n "fn.*recover\|fn.*restart\|fn.*crash\|fn.*orphan"` (`engine_tests.rs`) | 12 real recovery/persistence tests found |
| `ls executors/{playwright,newman,pytest,maestro,schemathesis}` | All 5 contain real config/dependencies (`node_modules`, `playwright.config.ts`, `collection.json`, `test_health.py`, `flow.yaml`, `openapi.yaml`) |

No file was modified to make any of the above pass.

## 16. Test / Typecheck Baseline

- `npx tsc --noEmit`: clean.
- `npx vitest run`: 272/272 passing, 14 files — unchanged from FE-14's own end-state (261 baseline + 11 FE-14 tests), confirming zero regression and zero new test activity this unit (expected, since this is a read-only unit).

## 17. Files Created / Modified

- **Created:** `AIQA_PHASE4_FE15_PRODUCT_CONTRACT_DECISION_RESOLUTION_AUDIT_REPORT.md` (this report) — the only file.
- **Frontend changes:** None.
- **Backend changes:** None.
- **Database changes:** None.
- **Contract/product document edits:** None.

## 18. Risks

- **GAP-02 deferral risk grows over time**: every day the Test Generator keeps claiming "Synthesized via gemini-2.0-flash" without disclosure is a live, user-facing honesty gap — the longer it's deferred, the more it looks like an intentional dark pattern rather than an inherited placeholder.
- **GAP-03/GAP-05 deferral risk is asymmetric with new code**: each new backend consumer built against the real (undocumented) vocabulary widens the eventual remap cost if Option A is ever chosen later; deferring the *documentation* fix costs nothing, but deferring the *decision* indefinitely means the contract keeps drifting further from an increasingly-entrenched reality.
- **GAP-04 risk**: as long as the Acceptance Matrix stays at its NOT_STARTED template, anyone who consults it directly (rather than this report or the FE-13/FE-14 reports) is actively misled about the system's real state.
- **Newly-found ACC-007 nuance risk**: the real Playwright/Newman/Pytest/Maestro/Schemathesis executors being wired-but-unverified-live is itself a risk if anyone assumes "the code exists" means "it works end-to-end" — Maestro's own comment already flags this for mobile.
- **Scope risk for any future unit**: Decision 1's Option B (real Model Gateway) is large enough that a future unit attempting it without a dedicated architecture phase first would likely repeat this audit's own "necessary future architecture inference" list speculatively — that list is not a design, only an evidence-based inventory of missing pieces.

## 19. Remaining Capability Gaps

State updated **only** where this unit found new evidence; nothing is closed merely because it was analyzed:

- **GAP-01**: remains closed (re-confirmed, §4).
- **GAP-02**: remains open. New evidence this unit: the fabricated-export mechanism (`triggerNewRun`) is more concretely damaging than previously documented — it creates and stores a fully fake `ExecutionRun` object, not merely "a disconnected local session."
- **GAP-03**: remains open, unchanged.
- **GAP-04**: remains open. New evidence this unit: ACC-007 and ACC-022 both have *more* real backing than FE-13's blanket classifications suggested (real executors; real recovery tests) — both upgraded from their FE-13 status to VERIFIED PARTIAL with specific, narrower remaining verification needs.
- **GAP-05**: remains open, unchanged.
- **No gap is closed by this unit.** This unit produced decision packages, not resolutions.

## 20. Final Recommendation

### Can implementation continue immediately?

- **GAP-02 (Test Generator):** **NO — product decision required** (Decision 1).
- **GAP-03 (`INFRA_ERROR`):** **NO — documentation approval required** (Decision 2). Once approved, the actual edit is trivial and could be done same-day by whoever owns the contract document.
- **GAP-04 (Acceptance Matrix):** **NO — documentation approval required** for the update itself (Decision 4); the mechanical corrections in §9.2 are implementation-ready evidence the moment an owner accepts them.
- **GAP-05 (Realtime vocabulary):** **NO — product decision required** (Decision 3), both for naming and for the extra event types' fate.
- **Newly-identified ACC-007/ACC-022 live-verification gap:** **YES — implementation-ready** in the narrow sense that it needs no decision, only effort: a short, dedicated verification-only unit (kill the real orchestrator process mid-run; submit a real Playwright/Pytest task and confirm real tool invocation) could close this without any product input.

## 21. Final Result

FE-15 investigated all four assigned gaps using fresh, current-session evidence — full reads of all three Tier-1 PDFs, the complete 1093-line Acceptance Matrix, the complete `TestCaseGeneratorPage.tsx` and `AIQAContext.tsx` fabrication path, the complete real-executor infrastructure in `executor.rs`/`main.rs`/`executors/`, and targeted greps across both the backend and frontend for every relevant vocabulary/keyword — rather than relying on FE-13's or the Post-FE-13 audit's prior conclusions unverified. It produced four fully-specified decision packages (§13), a complete 22-item re-evaluated Acceptance Matrix (§9.2) including two items whose real status turned out better-documented than previously known (ACC-007, ACC-022), and a cross-gap dependency graph distinguishing existing evidence from necessary future inference. It deliberately did **not**: implement any of the four decisions, edit any document, silently choose an option, invent Model Gateway architecture as approved scope, or close any gap. The only artifact produced is this report.
