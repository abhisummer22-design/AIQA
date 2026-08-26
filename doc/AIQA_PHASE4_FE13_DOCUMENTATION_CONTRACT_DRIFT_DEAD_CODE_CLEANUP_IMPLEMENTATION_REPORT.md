# AIQA — Phase 4 — FE-13 Documentation Contract Drift & Dead-Code Cleanup Implementation Report

## 1. Status

**PASS.**

## 2. Exit Criterion

The brief's own 16-item completion criterion (§22) is satisfied:

1. All available product/contract documents inventoried — §3.
2. Documentation authority/version relationships documented — §4.
3. Major backend/frontend/documentation domains compared — §5/§9.
4. Every discovered meaningful drift has a classification — §5 (24 entries, all classified A–H).
5. `INFRA_ERROR` vs `INFRASTRUCTURE_ERROR` explicitly resolved or formally documented as unresolved — §7 (formally documented as unresolved; already correctly not-silently-renamed since FE-1/FE-12).
6. Acceptance Matrix claims checked against actual implementation — §8.
7. Unsupported features compared against product requirements — §12.
8. Every deleted code artifact has proof of safe removal — §14.
9. No legitimate future/test/public code was deleted blindly — §15 (several candidates explicitly kept, with reasons).
10. FE-11 fabrication boundaries remain intact — §17.
11. FE-12 status semantics remain intact — §18.
12. No backend code/database/schema was modified — §25 (`git status --porcelain services/orchestrator/` empty throughout).
13. No fake data was introduced — §16 (none; one long-standing, previously-known honesty gap in Test Generator is documented, not newly created).
14. No global UI redesign occurred — every touched file changed only dead code, a stale comment, or duplicate-removal; zero layout/visual changes.
15. All tests pass, typecheck passes, production build passes — §19/§22.
16. Every remaining unresolved drift is explicitly listed — §26.

## 3. Document Inventory

**Product/contract documents (the ones this unit's brief names explicitly):**

| Document | Filename | Version | Purpose | Authority | Intent |
|---|---|---|---|---|---|
| PRD+FRD+SRD | `AIQA_Consolidated_PRD_FRD_SRD_v6.0_Hybrid_Multi_Agent (1).pdf` | v6.0 | Product/functional/system requirements | Highest (per its own §33 and the Acceptance Matrix's own hierarchy) | Describes intended product, including capabilities not yet built |
| Business Logic & Architecture | `AIQA_Business_Logic_Architecture_v2.0_Hybrid_Multi_Agent.pdf` | v2.0 | Business rules/architecture | Highest, alongside PRD/FRD/SRD | Same — intended behavior, not a implementation log |
| Flow Diagram | `AIQA_Complete_End_to_End_Flow_Diagram_v6.0.pdf` | v6.0 | Visual product flow | Same tier | Intended flow (includes `PROJECT CREATE/SELECT` as page 1's first box — not yet built, see DRIFT-022) |
| API Contract | `AIQA_API_CONTRACT_v1.0.md` | v1.0 | Frontend↔backend interface spec | Third tier (its own §Purpose: "derived from" the three docs above) | **Aspirational** — its own §32 states explicitly: "This document defines the target contract. It does NOT claim that every endpoint above already exists." File timestamp (Aug 17 09:38) *predates* the three PDFs above (Aug 18 17:51-52) despite claiming to derive from them — evidence it was written against draft/earlier versions of the product docs, not the ones now in the repo (noted, not resolvable without the document owner) |
| Acceptance Matrix | `AIQA_ACCEPTANCE_MATRIX_v1.0.md` | v1.0 | Completion/verification tracking | Fourth tier | A **template**, never filled in — every one of its 22 `ACC-*` rows and its own §34 "Final Verification Record" still read `NOT_STARTED`/`NOT_VERIFIED`/`TBD` verbatim, despite substantial real implementation having happened since (see §8) |
| Documentation Compliance Gap Analysis | `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md` | n/a | Cross-references FE-4–FE-7 against the three PDFs once they arrived | Analysis only, not a source of new requirements | Already-resolved/deferred findings, re-verified still accurate this unit (§11) |

**Historical/process documents** (inventoried, not deep-read — they are records of *past implementation work*, not current authoritative requirements, and none is named in this unit's own mandatory list): `AIQA_ARCHITECTURE_AUDIT_REPORT.{md,pdf}`, `AIQA_BACKEND_AUDIT_REPORT.{md,pdf}`, `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md`, `AIQA_DEEP_AUDIT_REPORT.{md,pdf}`, `AIQA_FRONTEND_AUDIT_REPORT.md`, `AIQA_FRONTEND_CONSOLIDATION_REPORT.md`, `AIQA_FRONTEND_CONTRACT_READINESS_MATRIX.md`, `AIQA_FRONTEND_PHASE2_INTEGRATION_READINESS_REPORT.md`, `AIQA_FRONTEND_PRE_IMPLEMENTATION_AUDIT.md`, `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md`, `AIQA_D1`–`D5_FIX_REPORT.md`, `AIQA_PHASE1_*.md` (3 files), `AIQA_PHASE2_BE8`–`BE15`+`ACTUAL_BE9`_*.md (9 files), and every `AIQA_PHASE4_FE1`–`FE12_*_IMPLEMENTATION_REPORT.md` (12 files, including this unit's own FE-11/FE-12 predecessors). These are evidence of *what was done and why*, cited throughout this report where directly relevant, but never treated as a source of new product requirements.

**Superseding relationships:** the three v6.0/v2.0 PDFs supersede nothing in this repo (nothing else claims their scope); the API Contract explicitly says it does not replace them; the Acceptance Matrix explicitly says it does not replace the API Contract or the PDFs; the Gap Analysis explicitly says it "does not replace" and exists only as follow-up analysis. No genuine version conflict was found *within* the product-doc tier — v6.0/v2.0 are each internally the latest and only version present.

## 4. Source-of-Truth Hierarchy

Used, per this unit's own explicit instruction (not "backend always wins"):

1. **Product intent** — PRD+FRD+SRD v6.0, Business Logic v2.0, Flow Diagram v6.0 (co-equal; no conflict found between them).
2. **Contract intent** — API Contract v1.0, Acceptance Matrix v1.0 (subordinate to #1 by their own stated hierarchy).
3. **Actual implementation** — real backend (`services/orchestrator`, read extensively, never modified), real frontend typed contract (`types/contract.ts`), actual runtime behavior (confirmed live, §20).

Distinguishing **intended** from **current**: layer 3 is authoritative for *what currently runs*; it is never treated as proof that a documented-but-unbuilt capability in layer 1/2 is wrong. Every drift below states explicitly which side of that line it falls on.

## 5. Complete Drift Matrix

| ID | Domain | Product Docs | API/Acceptance Contract | Backend | Frontend | Runtime | Classification | Recommended Action | FE-13 Action |
|---|---|---|---|---|---|---|---|---|---|
| DRIFT-001 | TaskStatus naming | "infrastructure/environment error" (prose, not a literal token) | `INFRASTRUCTURE_ERROR` (API Contract §13, explicit canonical name) | `INFRA_ERROR` (`domain.rs`, real enum) | `INFRA_ERROR` everywhere (`types/contract.ts`, matches backend) | `INFRA_ERROR` | **D — API contract drift** | Contract owner decides: rename the contract doc's canonical name to match the shipped backend, or file a backend rename request for a future backend unit | **Documented only** — no code changed (see §7) |
| DRIFT-002 | AI Model Gateway / LLM intelligence | Central premise of "v6.0 Hybrid Multi-Agent": Model Gateway, Requirement/Test-Design/Security/API/Mobile "Intelligence" agents | ACC-018 full section; API Contract §22 | **Zero implementation** — no `/ai/models*` routes, no LLM/provider API call anywhere in `services/orchestrator` (confirmed: `grep -i "openai\|anthropic\|gpt-\|gemini\|llm"` across all backend source returns only `intelligence.rs`/`policy.rs` comments *explicitly disclaiming* LLM use) | No Model Gateway UI; `ReportsPage.tsx` correctly labels its real root-cause feature "Heuristic root-cause hypothesis (rule-based, not LLM-derived)" | Matches frontend | **E — Future/planned capability** | Needs a product decision: scope this out of the near-term roadmap explicitly, or schedule a real backend Model Gateway unit | Documented; frontend already correctly avoids overclaiming this (verified, not fixed — nothing to fix) |
| DRIFT-003 | AIQA Orchestrator (central planning/routing entity) | PRD §2, Business Logic §2/§4, ACC-006 | API Contract §11 | Zero `/orchestrator/plans*` routes | FE-11 Case C — truthful "not implemented" | N/A | **E** | Product decision: build, or formally scope out | None (already correctly Case C) |
| DRIFT-004 | Test Case Validator | PRD §2, ACC-005 | API Contract §9 | Zero `/test-validation*` routes | FE-11 Case C | N/A | **E** | Same | None |
| DRIFT-005 | RBAC Testing module | PRD §2, Business Logic §7, ACC-011 | API Contract §17 | Zero `/rbac/*` routes (the *enforcement* mechanism, `auth::require_role`, is real and matches docs — only the *testing/matrix UI* capability is absent) | FE-11 Case C | N/A | **E** (module) / matches (enforcement) | Product decision on the module; no action needed on enforcement | None |
| DRIFT-006 | API Testing module | PRD §2, ACC-012 | API Contract §18 | Zero `/api/specifications`, `/api/test-generation`, `/api/executions`, `/api/telemetry` routes | FE-11 Case C | N/A | **E** | Product decision | None |
| DRIFT-007 | Mobile Testing module | PRD §10 ("v5.0 baseline... remains"), ACC-014 | API Contract §19 | Zero `/mobile/devices*` routes — only a `maestro` *executor string* exists in `TaskDef` (policy-gated, no device entity behind it) | FE-11 Case C | N/A | **E** (with a caveat — the docs claim this "remains" from a v5.0 baseline; no evidence was found anywhere in this repo, including the historical Phase1/Phase2 backend reports, that a real device-inventory backend was ever built at any version) | Product decision — including whether the "v5.0 baseline retained" claim itself needs correcting | None |
| DRIFT-008 | Custom Testing module | PRD §2, ACC-015 | API Contract §20 | Zero `/custom-tests` routes | FE-11 Case C | N/A | **E** | Product decision | None |
| DRIFT-009 | Feature Map / Flow | Business Logic §2, ACC (not separately numbered — folded under module list) | Not in API Contract (no §) | Zero routes | FE-11 Case C | N/A | **E** | Product decision | None |
| DRIFT-010 | Context Ingestion | PRD §2/§3, ACC-002 | API Contract §5.5/§6 | Zero `/context*` routes | No UI | N/A | **E** | Product decision — this blocks DRIFT-002/003 (Requirement Intelligence needs ingested context) | None |
| DRIFT-011 | Quality Graph | Business Logic §3, ACC-003 | API Contract §7 | Zero `/quality-graph`, `/requirements`, `/features` routes | No UI | N/A | **E** | Product decision | None |
| DRIFT-012 | Reports (full scope) | PRD §2, Business Logic §11, ACC-016: "Bug Report, Test Report, Final Audit Report... PDF/Excel export" | API Contract §21 | Real: `Finding`/`FailureCluster` (`bug_drafts`) — a genuine Bug-Report-equivalent. **Not real**: no `/reports*` routes, no Test Report/Final Audit Report entity, no PDF/Excel export anywhere | FE-11 rebuilt Reports onto the real Findings subset; its own module doc comment explicitly discloses Test Report/Final Audit Report/PDF/Excel as out of scope | Matches frontend | **E** for the missing 3/4 of ACC-016; the Findings subset is real and correctly scoped (no classification needed there) | Product decision on the missing report types/exports | None — already correctly, explicitly disclosed; this report formalizes that FE-11's Reports page satisfies only part of ACC-016, not all of it (brief §6.I's own explicit warning against overclaiming) |
| DRIFT-013 | Test Case Generator — end-user honesty | PRD §2 ("ingest project evidence and generate... tests"); Business Logic §2 | API Contract §8 (real generation endpoint expected, async) | Zero `/test-generation*` routes | 100% local `setTimeout`-based simulation; presents specific real AI model names (`gemini-2.0-flash`, `claude-3-5-sonnet`) in its own UI copy ("GEMINI 2.0 FLASH REASONING" banner, model dropdown) with **no in-app disclosure** that no real model call ever happens — disclosure exists only in source comments and prior FE unit reports, never in rendered UI text | Confirmed live this unit (§20) — banner and model name render exactly as fake-real | **C — Frontend implementation drift** (the backend gap is Type E, but the frontend's specific choice to name real products as if invoked, with zero in-UI disclosure, is a frontend-side honesty gap layered on top) | A future unit should either build the real Model Gateway + generation endpoint, or add a real, user-visible "Simulated Preview — no AI model was actually called" disclosure | **Documented only** — this is a long-standing, previously-known characteristic (Test Generator has been called "protected/disclosed-fake" since FE-5/FE-8/FE-11/FE-12), not a new discovery; brief §6.K/§16 forbid redesigning it in this unit. See §16 |
| DRIFT-014 | `RealtimeContext.tsx`'s own module doc comment | n/a (code comment, not a product document) | n/a | n/a | Comment claimed catch-up went through `aiqaApi.executions.events` via a `handleReconnect` function "below" — **neither exists in the file** (confirmed: `handleReconnect` has zero matches in the file; `.events()` has zero call sites anywhere in `forentend`) | The real mechanism is each consumer's own full-resync-on-reconnect (e.g. `ExecutionHubPage.tsx`'s `prevConnectionStateRef` effect) | **A — Documentation stale** (at the source-comment level) | Fix the comment | **Fixed this unit** — see §9 |
| DRIFT-015 | Realtime event-type vocabulary | Flow Diagram (generic "REAL-TIME EVENTS" box, no specific names) | API Contract §14: `RUN_CREATED`, `RUN_STATUS_CHANGED`, `TASK_CREATED`, `TASK_STATUS_CHANGED`, `TASK_PROGRESS`, `EVIDENCE_CREATED`, `FINDING_CREATED`, `MODEL_REQUEST/RESPONSE/FALLBACK`, `WORKER_STATUS_CHANGED`, `API_TELEMETRY`, `RBAC_ACCESS_PATH`, `REPORT_READY`, `ERROR` | Real: `RUN_SCHEDULED`, 9×`TASK_*` (one per specific status transition, not a generic `STATUS_CHANGED`), `RUN_COMPLETED` — a completely different naming/granularity scheme, with no `MODEL_*`/`WORKER_*`/`API_TELEMETRY`/`RBAC_ACCESS_PATH`/`REPORT_READY` events at all (none of those subsystems exist — see DRIFT-002/005/006/012) | `types/contract.ts`'s `EventType` mirrors the real backend exactly, with its own comment already citing this exact drift and deferring it: *"Deliberately not remapped to the contract's... Integration Plan §11.2 item 2 calls that remapping 'a §31 decision, not an implementation choice'"* | Matches backend | **G — Ambiguous / needs product decision** (already correctly identified and deferred by FE-1/FE-9, re-confirmed here) | Contract owner decides whether to update the API Contract's event vocabulary to match reality, or schedule a backend remap | **Documented only** — already correctly deferred; no new drift, re-verified accurate |
| DRIFT-016 | Acceptance Matrix currency | n/a | The Matrix itself (§5 Master Feature Matrix: all 22 rows `NOT_STARTED` across every column; §34: all `NOT_VERIFIED`/`TBD`) | Substantial real backend exists (BE-8 through BE-15) | Substantial real frontend exists (FE-1 through FE-12) | Substantial real end-to-end behavior confirmed live across many FE units' own manual verification | **A — Documentation stale** | Document owner should fill in the Matrix from real status, or explicitly mark it superseded by the FE-unit reports | **Documented only** (§8 gives this unit's own independent real-status assessment; the source file itself was not edited, per §18's policy) |
| DRIFT-017 | Dashboard KPI coverage | ACC-017: 11 KPI concepts listed | API Contract §23.2: same 11 | Real: `execution_reliability`, `execution_efficiency_ms` only (2/11) | Renders exactly those 2, honestly, no fabricated values for the other 9 (FE-10's own disclosed decision) | Matches | **E** for the 9 uncomputed KPIs; **F** for the 2 real ones (intentional, correct, disclosed) | Product decision on the remaining 9 | None — already correct |
| DRIFT-018 | Settings/Configuration | ACC-019 | API Contract §24 | Zero `/settings*` routes | Local-only, disclosed-fake (all sections except the real API-key panel) | Matches | **E** (backend); **F** (frontend handling — correct, disclosed since FE-3) | Product decision | None |
| DRIFT-019 | API Latency/Telemetry | ACC-013 | API Contract §18.4/§22.3 | No `/api/telemetry` or `/ai/models/telemetry`; only a generic Prometheus `/metrics` text endpoint exists, and it has zero frontend consumer | No UI | N/A | **E** | Product decision | None |
| DRIFT-020 | Audit/Evidence navigation placement | PRD/Business Logic/Flow Diagram module tables have no standalone "Audit"/"Evidence" page | Not addressed | Real, working, project-scoped endpoints for both | Standalone `/audit` and `/evidence` pages/nav entries (FE-6/FE-7) | Matches frontend | **F — Intentional implementation difference** (already fully reasoned through in the Gap Analysis §3.1: moving them now would mean redoing real integration work twice) | None — already a resolved, documented decision | **Re-verified accurate**, no new action |
| DRIFT-021 | `NEEDS_REVIEW` as a documented state | SRD §13, FRD-FI-02: explicitly documented | Same | Real (`TaskStatus::NeedsReview`, `ReviewState::NeedsReview`) | Real, wired (FE-11 Reports, FE-12 label formatting) | Matches | **A, already resolved** (Gap Analysis §3.2 already corrected the earlier FE-6/FE-7 reports' now-outdated "no documentation defines this" reasoning) | None | **Re-verified accurate** |
| DRIFT-022 | Project creation UI | Flow Diagram page 1: `PROJECT CREATE/SELECT` is the first step; ACC-001: "User can create a project" | API Contract §5.2 (real endpoint spec) | Real, working `POST /api/v1/projects` (confirmed in `api.rs`) | Zero UI anywhere calls `aiqaApi.projects.create` — only a project *switcher* exists (Header's real `ProjectSelector`, FE-5) | No creation UI | **C — Frontend implementation drift** (already identified in the Gap Analysis §3.3 as "a gap, not a wrong turn") | A future unit should build a real project-creation screen using the already-real, already-tested-shape `aiqaApi.projects.create` | **Re-verified accurate, still open** — `aiqaApi.projects.create`/`.get`/`.update` deliberately kept, not deleted (§13/§15) |
| DRIFT-023 | Execution DAG/task-dependency exposure | FRD-EXE-12, ACC-007: "DAG dependencies" | API Contract §12.1 (`test_case_ids`/plan concept, no explicit dependency field shown) | Real: `TaskDef.prerequisites`, real DAG scheduling (`scheduler.rs`) | `ExecutionHubPage.tsx`'s minimal creation form does not expose a prerequisites editor (its own comment: avoiding "an elaborate execution wizard") | Backend DAG logic is real and exercised by other tests, just not from this form | **C — minor, disclosed** (FE-8's own considered scope decision) | Could be extended in a future unit if a real dependency-editing UI is wanted | None — already correct/disclosed |
| DRIFT-024 | Legacy/admin backend routes | n/a | n/a | Real, working: `/runs*`, `/intelligence/*`, `/bug-drafts*`, `/admin/*`, `/metrics` — explicitly kept "for compatibility," per `api.rs`'s own comment | Zero frontend caller for any of them (`aiqaApi.ts`'s own comment: "no legacy flat path... is called here, per this sprint's explicit instruction") | N/A | **F — Intentional implementation difference**, explicitly documented on both sides | None needed | None — consistent, both sides agree and say why |

## 6. Product Intent vs Current Implementation

**Intended** (per the v6.0/v2.0 product docs): a Hybrid Multi-Agent platform where a central AI Orchestrator routes work through specialized LLM-backed intelligence agents (Requirement, Test Design, Security, API/Performance, Mobile), a Test Case Validator gates execution, RBAC/API/Mobile/Custom testing are dedicated real modules with live device/role/API introspection, Reports produce Bug/Test/Final-Audit documents exportable as PDF/Excel, and an AI Model Gateway abstracts local/API model routing with measured accuracy and fallback telemetry.

**Currently implemented**: a real, working execution/evidence/audit/findings platform — project-scoped executions with a real DAG scheduler, real approval-gated policy engine, real WebSocket realtime events, real evidence capture/retrieval, real project-scoped audit trail, and a real (rule-based, not LLM) failure-clustering/finding system with a real Confirm/Dismiss review workflow. None of the AI-agent, orchestration-planning, RBAC/API/Mobile/Custom-testing, context-ingestion, quality-graph, report-generation, or model-gateway capabilities exist in the backend. The frontend's FE-11 response to this gap (7 truthful "not implemented" pages, a Findings-based Reports rewrite, honest Dashboard/KPI subsets) is the correct, proportionate, non-fabricated reflection of that real gap — confirmed consistent with the product docs' own distinction between "what AIQA is meant to become" and "what is running today."

## 7. Status Contract Audit

All six FE-12-established domains re-verified intact this unit (fresh greps, not memory): `RunState`, `TaskStatus`, `ReviewState`, `AuthStatus`, `RealtimeConnectionState`, `ApiError.kind`, plus `audit.outcome` (`"ok"`/`"denied"`, lowercase, real) and the frontend-derived `ProjectStatus` (`'Healthy'`/`'Running'`).

**`INFRA_ERROR` vs `INFRASTRUCTURE_ERROR`**: **formally documented as unresolved, requiring a product/contract-owner decision — not resolved by this unit, and not silently aliased.** The real backend (`domain.rs::TaskStatus::InfraError`, serialized `INFRA_ERROR`) is authoritative for what currently runs; the API Contract's §13 canonical list names `INFRASTRUCTURE_ERROR` and explicitly forbids "alternate spellings," which the real backend already violates (a pre-existing condition, not introduced by any FE unit). Per this unit's own explicit rule ("do not blindly rename INFRA_ERROR to INFRASTRUCTURE_ERROR," "do not modify backend code"), no rename occurred anywhere — not in `domain.rs`, not in `types/contract.ts`, not in any display label. `formatStatusLabel` (FE-12) only ever inserts a space for readability (`INFRA_ERROR` → "INFRA ERROR" as *displayed* text); it never changes the identifier itself. The required next action is a product/contract-owner decision: update the API Contract's §13 to say `INFRA_ERROR` (the simpler fix, since the backend is real/shipped/tested and a backend rename risks breaking the legacy `/runs`-family clients too), or schedule a real backend unit to rename the enum (a breaking API change per the Contract's own §31 policy, requiring the full change-control process that section describes).

## 8. Acceptance Matrix Audit

The Matrix document itself (§16 above) was never updated past its initial `NOT_STARTED` template. This unit's own independent assessment, from real inspection (not from the stale document):

| ACC | Area | This unit's real-status assessment | Evidence |
|---|---|---|---|
| ACC-001 | Project Management | **PARTIALLY IMPLEMENTED** — select/scope/isolate: real, verified (FE-5, extensively re-tested by every subsequent FE unit's real 401/403 checks); create: real backend, zero frontend (DRIFT-022) | Live-verified this unit and prior units |
| ACC-002 | Context Ingestion | **DOCUMENTED BUT NOT IMPLEMENTED** | Zero backend routes |
| ACC-003 | Quality Graph | **DOCUMENTED BUT NOT IMPLEMENTED** | Zero backend routes |
| ACC-004 | Test Case Generator | **PARTIALLY IMPLEMENTED** — local UI simulation exists and functions (verified live this unit), zero real backend generation | DRIFT-013 |
| ACC-005 | Test Case Validator | **DOCUMENTED BUT NOT IMPLEMENTED** (truthfully disclosed, FE-11 Case C) | — |
| ACC-006 | AIQA Orchestrator | **DOCUMENTED BUT NOT IMPLEMENTED** (truthfully disclosed) | — |
| ACC-007 | Test Execution Hub | **IMPLEMENTED AND VERIFIED** (core flow); dependency-editor UI **PARTIALLY IMPLEMENTED** (DRIFT-023) | Live-verified this unit (real execution created/approved/transitioned) |
| ACC-008 | Realtime Events | **IMPLEMENTED AND VERIFIED** | Live-verified this unit (real WS transition observed) |
| ACC-009 | Evidence | **IMPLEMENTED AND VERIFIED** | Live-verified (FE-7, re-checked this unit) |
| ACC-010 | Failure/RCA Intelligence | **IMPLEMENTED AND VERIFIED** (rule-based, correctly never described as LLM) | Live-verified this unit |
| ACC-011 | RBAC Testing | **PARTIALLY IMPLEMENTED** — enforcement real/verified; testing-UI module not implemented (truthfully disclosed) | — |
| ACC-012 | API Testing | **DOCUMENTED BUT NOT IMPLEMENTED** | — |
| ACC-013 | API Latency/Telemetry | **DOCUMENTED BUT NOT IMPLEMENTED** | — |
| ACC-014 | Mobile Testing | **DOCUMENTED BUT NOT IMPLEMENTED** | — |
| ACC-015 | Custom Testing | **DOCUMENTED BUT NOT IMPLEMENTED** | — |
| ACC-016 | Reports | **PARTIALLY IMPLEMENTED** — Findings subset real/verified; Test Report/Final Audit Report/PDF/Excel not implemented | DRIFT-012 |
| ACC-017 | Dashboard/KPIs | **PARTIALLY IMPLEMENTED** — 2/11 KPIs real; core dashboard verified | DRIFT-017 |
| ACC-018 | AI Model Gateway | **DOCUMENTED BUT NOT IMPLEMENTED** | DRIFT-002 |
| ACC-019 | Settings/Configuration | **PARTIALLY IMPLEMENTED** — real API-key auth panel verified; rest disclosed-local-only | — |
| ACC-020 | Audit/Governance | **IMPLEMENTED AND VERIFIED** | Live-verified this unit |
| ACC-021 | Security/Isolation | **IMPLEMENTED AND VERIFIED** — server-side RBAC and project isolation confirmed via real 401/403 across many units, including this one | Live-verified this unit (real 403 not re-triggered this specific unit, but confirmed via FE-11/FE-12's own live 403 tests plus this unit's re-read of `auth::require_role` call sites) |
| ACC-022 | Recovery/Fault Tolerance | **AMBIGUOUS — not independently re-verified this session.** Real durable SQLite persistence and the BE-13 report's own claimed worker-isolation behavior were not re-tested live by this frontend-focused unit | Code inspection only (`domain.rs`/`engine.rs` durability comments); no new worker-crash test performed |

No item was marked implemented merely because a page/route/placeholder exists — every "IMPLEMENTED AND VERIFIED" above was independently either live-verified this unit or is backed by a specific prior unit's own live verification cited in this repo's own reports.

## 9. Frontend Contract Drift

- **DRIFT-014, fixed this unit**: `RealtimeContext.tsx`'s module doc comment claimed a `handleReconnect` function and an `aiqaApi.executions.events()` catch-up call that do not exist in the file. Corrected to accurately describe the real mechanism (per-consumer full resync on reconnect) — zero runtime behavior changed, comment-only fix.
- **DRIFT-013**: documented, not fixed (out of scope — see §12/§16).
- **DRIFT-022/023**: re-confirmed accurate, not fixed (require new feature work, out of this unit's cleanup-only scope).

## 10. Backend Contract Drift

Per this unit's own instruction ("document only unless a frontend-safe compatibility issue is proven"), no backend drift below required or received a frontend-side workaround:

- DRIFT-001 (`INFRA_ERROR`/`INFRASTRUCTURE_ERROR`) — documented, §7.
- DRIFT-002/003/004/005/006/007/008/009/010/011/018/019 — the full list of documented-but-unbuilt backend capabilities — documented, §5.
- DRIFT-015 (event-type vocabulary) — already deferred by a prior unit, re-confirmed, documented.
- DRIFT-024 (legacy routes) — intentional, both sides already agree, documented.

## 11. Documentation Drift

- DRIFT-016 (Acceptance Matrix never filled in) — the single largest documentation-currency gap found. Not silently corrected (per §18's policy); this report's §8 is the substitute real-status record.
- The API Contract's file-timestamp-predates-its-own-cited-sources oddity (§3) — noted, not resolvable without the document owner (it may simply mean the PDFs existed in draft form before being formally added to this repo; no evidence either way).
- Everything else in §5 that reads "Documented but not implemented" is, from a pure documentation-quality standpoint, **not** stale or wrong — the PRD/Business Logic/Flow Diagram accurately describe the *intended* v6.0 product; they are not miscategorized as "dead documentation" (Type H) because there is no evidence the product vision itself has been abandoned, only that most of it is unbuilt. None of the 24 drift entries were classified Type H.

## 12. Unsupported Feature Audit

| Page | Documented requirement? | Backend support? | Current UI state appropriate? | Remaining action |
|---|---|---|---|---|
| AIQA Orchestrator | YES (PRD §2, Business Logic §2/§4) | NO | YES — truthful, correctly explains the specific missing capability, links to the real Execution Hub | Product decision only |
| Test Case Validator | YES (PRD §2, ACC-005) | NO | YES | Product decision only |
| RBAC Testing | YES (Business Logic §7, ACC-011) | NO (module); real enforcement exists separately | YES — correctly distinguishes the two | Product decision only |
| API Testing | YES (PRD §2, ACC-012) | NO | YES | Product decision only |
| Mobile Testing | YES, with a caveat (§5 DRIFT-007) | NO | YES | Product decision only, plus resolving the "v5.0 baseline retained" documentation claim |
| Custom Testing | YES (PRD §2, ACC-015) | NO | YES | Product decision only |
| Feature Map / Flow | YES (Business Logic §2) | NO | YES | Product decision only |

All 7 remain exactly as FE-11 left them (verified live this unit, §20) — none was turned back into a fake implementation, and none needed a wording fix (their "not implemented"/"no backend capability" copy was independently re-checked against the real route table and remains accurate).

## 13. Dead-Code Inventory

Investigated via an independent, whole-repository grep sweep (a background research pass, cross-verified by this unit's own direct greps for every consequential claim before acting):

| Candidate | Location | Reason suspected dead | Consumer search result | Classification | Final action |
|---|---|---|---|---|---|
| `MetricCard` component | `components/common/MetricCard.tsx` | Generic KPI-tile atom, part of the original pre-FE-1 baseline | Zero imports anywhere; no test file | **SAFE TO DELETE** — Dashboard already implements the same concept with its own inline markup | **Deleted** |
| `StatusBadge`/`FailureClassificationBadge` | `components/common/StatusBadge.tsx` | FE-12 already found this orphaned and documented it in a comment, but left the file in place | Zero imports; the codebase's own comment (`utils/statusLabel.ts`) already calls it "since-orphaned" | **SAFE TO DELETE** — its prop types (`AIQAStatus`/`FailureClassification`) remain used elsewhere, unaffected | **Deleted** |
| `approveTestCase`, `rejectTestCase`, `startGenerationWorkflow`, `updateGenerationStep` | `context/AIQAContext.tsx` | Duplicate of logic `TestCaseGeneratorPage.tsx` already implements itself, directly | Zero call sites anywhere outside their own declaration — confirmed by two independent greps, including of `TestCaseGeneratorPage.tsx` itself, which uses its own local `handleApproveTestCase`/`handleRejectTestCase`/`setActiveGenerationSession` instead | **SAFE TO DELETE** — genuinely superseded duplicate, not the protected simulation itself (that protected boundary is `triggerNewRun`/`addActivity`/the `runs` array, all kept — see §15) | **Deleted**, live-verified afterward (§20: Approve Test Case still works, via the real, surviving local handler) |
| `Role` type | `types/contract.ts` | Zero import sites found anywhere | Confirmed — every other "Role" match in the repo is either DOM a11y `getByRole`, a table-column label, or backend-Rust-enum prose in a comment | **KEEP — real, correct, zero-cost mirror of the backend's real `Role` enum; directly useful for a plausible near-term real RBAC-status display feature.** Zero imports alone was judged insufficient proof of safe deletion for a 1-line type with no maintenance cost, per this unit's own explicit caution | Not deleted |
| `aiqaApi.projects.get`/`.create`/`.update` | `services/aiqaApi.ts` | Zero call sites, including in tests | Confirmed | **KEEP — public transport API for a real, working, documented backend capability (DRIFT-022's own project-creation gap)** | Not deleted |
| `aiqaApi.executions.task` (singular) | `services/aiqaApi.ts` | Zero call sites, including in tests | Confirmed | **KEEP — real, correct, supports a plausible future task-detail drill-down view** | Not deleted |
| `aiqaApi.executions.events` | `services/aiqaApi.ts` | Zero call sites; its own doc comment over-claimed it was used (DRIFT-014) | Confirmed | **KEEP — real, correct, legitimate extension point for a future finer-grained reconnect-catch-up implementation**; only its doc comment was misleading, now fixed | Not deleted (comment fixed) |
| `aiqaApi.evidence.get` | `services/aiqaApi.ts` | Zero real-page call sites (only test-covered) | Confirmed — `EvidencePage.tsx` reuses the list's own row instead | **KEEP — real, tested, correct; the list-reuse optimization documented in `EvidencePage.tsx`'s own comment doesn't make the alternate detail-fetch path dead, just currently unneeded** | Not deleted |
| `AIQAContext`'s `runs`/`triggerNewRun`/`ExecutionRun`/`TestExecutionItem` seed data | `context/AIQAContext.tsx`, `types.ts` | Read-side is provably dead (nothing renders `runs`) | Confirmed — `triggerNewRun` *is* called (`TestCaseGeneratorPage.tsx`), so this is write-only, not fully orphaned | **KEEP — PROTECTED BOUNDARY.** This is Test Generator's own explicitly-protected local-simulation handoff, documented as such since FE-8 ("this AI-generated test case has no real mapping to this backend's actual `TaskDef`... without inventing that mapping"). Trimming its internals means editing Test Generator's own mechanism, out of bounds per brief §6.K/§16 ("audit documentation alignment only... do not redesign") | Not deleted |
| `activities`/`addActivity`/`ActivityEvent` | `context/AIQAContext.tsx`, `types.ts` | Read-side already known dead (FE-11's own comment says so) | Confirmed — `addActivity` *is* called (`TestCaseGeneratorPage.tsx`), same write-only pattern as above | **KEEP — same protected-boundary reasoning**; FE-11 already made and documented this exact call | Not deleted |
| `CreateProjectRequest`/`UpdateProjectRequest` types | `types/contract.ts` | Only used as parameter types for the unused `.create`/`.update` functions above | Confirmed — real, correct, tied to real capability | **KEEP** (follows the functions' own classification) | Not deleted |

## 14. Deleted Code

| File/symbol | Proof of safe removal |
|---|---|
| `forentend/src/components/common/MetricCard.tsx` (whole file) | Step 1: zero grep matches outside itself. Step 2: `tsc --noEmit` clean after removal. Step 3: no test referenced it (none existed). Step 4: not exported/re-exported anywhere as a public surface. Step 5: Dashboard's real KPI display already exists via inline markup — no capability lost. |
| `forentend/src/components/common/StatusBadge.tsx` (whole file, both exports) | Same 5 steps; additionally the file's own sibling code already called it "orphaned" before this unit touched it. |
| `AIQAContext.tsx`: `startGenerationWorkflow`, `updateGenerationStep`, `approveTestCase`, `rejectTestCase` (type declarations, implementations, and provider-value exposure); the now-dead `GeneratedTestCase` import | Step 1: zero call sites anywhere, confirmed twice independently. Step 2: `tsc --noEmit` clean. Step 3: no test referenced any of the four by name. Step 4: `TestCaseGeneratorPage.tsx` — the one page that owns this exact domain — already has its own working, separately-verified equivalent (`handleApproveTestCase`/`handleRejectTestCase`/direct `setActiveGenerationSession` calls), so nothing was "reserved" for it. Step 5: live-verified this unit (§20) that Approve/Reject still works end to end through the surviving real implementation — no capability was lost. |

No other file or exported symbol was deleted.

## 15. Preserved Code

Every "KEEP" row from §13, with its reason, is the full list; summarized:

- `Role` type — real, zero-cost, plausible near-term use.
- `aiqaApi.projects.get/create/update`, `.executions.task`, `.executions.events`, `.evidence.get`, and their supporting request types — real transport for documented, not-yet-UI-wired backend capabilities (DRIFT-022 project creation being the most concrete near-term one).
- `AIQAContext`'s `runs`/`triggerNewRun`/`ExecutionRun`/`TestExecutionItem` and `activities`/`addActivity`/`ActivityEvent` — Test Generator's protected local-simulation write path, explicitly out of this unit's redesign scope even though their read side is dead.

## 16. New Fabrication Discoveries

**None that meet this unit's own bar for reopening FE-11** (a *newly discovered*, *currently rendered* fake value *caused by* documentation/implementation drift). DRIFT-013 (Test Case Generator's zero in-UI disclosure that its AI-model banner/dropdown never calls a real model) was found and is documented in full (§5, §12), but it is **not new** — it has been the page's exact, unchanged behavior since FE-5, and has been explicitly, repeatedly disclosed as a protected, deliberate scope boundary in the FE-5, FE-8, FE-11, and FE-12 reports (all four independently reasoned through the same "no real backend mapping exists, do not invent one" conclusion). Reopening it would directly violate this unit's own explicit instruction ("do not reopen FE-11... Do not redesign Test Generator"). It is recorded here as a real, honestly-classified drift finding (Type C) with a recommended next action for a future, explicitly-scoped unit — not acted on in FE-13.

**This does not block FE-13.** No other rendered fake value was found anywhere in the app during this unit's full-repository sweep.

## 17. FE-11 Preservation

Confirmed intact: all 7 Case-C pages verified live this unit (§20) still render their exact FE-11 truthful copy; `AIQAContext`'s fabricated project seed, backend-connection state, RBAC/API-testing/custom-suite arrays, and quality-intelligence state remain removed (none were reintroduced — this unit only removed *additional*, already-dead surface, never added fake data back). The one new finding (DRIFT-013) is documented, not fabricated further and not silently expanded.

## 18. FE-12 Preservation

Confirmed intact, re-verified with fresh searches this unit: `INFRA_ERROR`/`PENDING_APPROVAL`/`NEEDS_REVIEW` are never spelled `INFRASTRUCTURE_ERROR`/literal-underscore/anything else anywhere in `forentend/src` (zero matches for `INFRASTRUCTURE_ERROR` as a real value, zero for `CANCELED`); `formatStatusLabel` and `renderResourceErrorState` (both introduced in FE-12) are unmodified by this unit; 401 and 403 remain visibly and semantically distinct everywhere (re-confirmed live this unit, §20, via the audit trail showing exactly the calls this unit itself made); the six status domains (§7) remain separate.

## 19. Tests

**261/261 pre-existing tests: unchanged, all still passing.** No new tests were added.

Reasoning (per brief §19's own "add tests only where actual code changes require them," and §19's explicit "do not add meaningless tests just to increase the test count"): every code change this unit made was a zero-observable-behavior-change deletion (two never-rendered files; four never-called context functions) or a doc-comment-only fix (`RealtimeContext.tsx`). None of the five test-writing triggers in the brief's own §19 actually applies:

- "Drift-related frontend fixes" — the one frontend fix (DRIFT-014) changed a comment, not code; there is no "previous incorrect behavior" to cover, since nothing behaved differently before or after.
- "Dead-code removal: affected route/page still renders, no replacement fake implementation introduced" — satisfied by the existing, unmodified 261-test suite passing unchanged (the strongest possible evidence these deletions were truly inert) plus this unit's own live manual re-verification (§20), rather than new synthetic tests for a no-op change.

## 20. Manual Verification

Performed live against the real backend this unit (not simulated). Restarted the real orchestrator and frontend dev server, minted a fresh real `fe13-manual-verify-operator` key.

- **Manually verified, live, this unit:**
  - Dashboard — real KPI values, real donut chart, for the real "AIQA Core Platform" project. No regression.
  - Test Case Generator — full page renders (all sections); clicked the real "Approve Test Case" button on `tc-gen-1`, confirmed it transitioned Pending Review → Approved with the counts and badge updating correctly — direct proof the deleted `AIQAContext.approveTestCase` was truly dead and its removal caused zero behavior change, since the page's own separate handler did the work.
  - Execution Hub — real execution list rendered, including the real `PENDING_APPROVAL`→`RUNNING`→`FAILED` run created during FE-12's own manual verification (now resolved to `FAILED`, a real outcome from the unimplemented `maestro` adapter).
  - Reports (Findings) — real findings list rendered correctly (`NEEDS REVIEW`/`DISMISSED`, space-formatted, FE-12's fix intact).
  - Audit Trail — real audit rows rendered, including this unit's own real actions from earlier FE-11/12 sessions, confirming persistence and no regression.
  - Evidence Viewer — real execution picker rendered with real status labels.
  - Mobile Testing (one Case-C page, spot-checked) — exact FE-11 "not implemented" copy, unchanged.
  - Browser console: zero errors across every page visited.
- **Automated test only (not re-triggered live this unit)**: the other 6 Case-C pages (covered by `CaseCPages.test.tsx`, all 30 tests passing unchanged); ExecutionHubPage's/ReportsPage's/StateViews' FE-12 vocabulary-formatting tests (all passing unchanged).
- **Code inspection only**: ACC-022 (recovery/fault-tolerance) — not re-tested live this unit; relies on the BE-13 report's own prior claims and a direct read of `domain.rs`/`engine.rs`'s durability logic (see §8's own "AMBIGUOUS" mark for this item, stated honestly rather than claimed verified).
- No claim above describes anything that did not actually happen this session.

## 21. Search Verification

- **Contract-drift candidates**: `INFRA_ERROR` (found only in real code + accurate comments), `INFRASTRUCTURE_ERROR` (found only in comments describing the known gap, zero real-value matches), `NEEDS_REVIEW`/`NEEDS REVIEW` (consistent, FE-12 formatting intact), `PENDING_APPROVAL`/`PENDING APPROVAL` (consistent), `CANCELLED`/`CANCELED` (zero matches for the single-L spelling anywhere).
- **Dead-code candidates**: full-repository consumer search for every `.tsx`/`.ts` file and every exported symbol in `types.ts`/`types/contract.ts`/`aiqaApi.ts` (§13/§15) — two files and four functions proven genuinely dead and removed; everything else classified KEEP with a specific reason, never deleted on "zero imports" alone.
- **Legacy remnants**: `grep -Rni "deprecated\|legacy\|obsolete\|TODO\|FIXME\|temporary\|compatibility\|fallback\|old\|unused"` across `forentend/src` — zero `TODO`/`FIXME`/`deprecated`/`obsolete`/`unused` matches; every `legacy`/`compatibility`/`fallback` match individually inspected and confirmed to be either accurate doc-comment prose about the backend's own real legacy routes, or legitimate honest-error-state code (never a real defect).

## 22. Typecheck / Build

- `npx tsc --noEmit` — clean after every edit in this unit.
- `npx vitest run` — 261/261 passing, 14 files, unchanged from before this unit's changes.
- `npx vite build` — succeeds; JS bundle 1,021.65 kB → 1,019.29 kB, CSS 93.59 kB → 88.95 kB (both shrank, consistent with removing two unused component files and their Tailwind classes — concrete evidence the deletions had a real, measurable, safe effect).

## 23. Regression

Zero. All 261 pre-existing tests (FE-1 through FE-12 coverage) pass unmodified. Live manual verification (§20) confirmed Dashboard, Test Generator (including its approve/reject flow specifically), Execution Hub, Audit, Evidence, Reports, and a Case-C page all function correctly end to end against the real backend, with zero console errors.

## 24. Documentation Changes

**None** — drift was documented in this report, not silently rewritten, per §18's policy. The one source-code *comment* fix (`RealtimeContext.tsx`, DRIFT-014) is not a "document" in the PRD/FRD/SRD/API-Contract/Acceptance-Matrix sense this policy governs — it is a code comment describing code, corrected to match the code it sits beside, with zero product/contract implications.

## 25. Backend Changes

**None.** `git status --porcelain services/orchestrator/` was empty for this unit's entire duration. `domain.rs`, `api.rs`, `engine.rs`, `policy.rs`, `intelligence.rs`, and `scheduler.rs` were read extensively for drift analysis, never modified.

## 26. Remaining Unresolved Drift

| Item | Affected layers | Why FE-13 did not resolve it | Required next decision/action |
|---|---|---|---|
| DRIFT-001: `INFRA_ERROR` vs `INFRASTRUCTURE_ERROR` | API Contract vs. real backend/frontend | Renaming the backend enum is a backend change (forbidden this unit and every prior FE unit); renaming the Contract doc requires the document owner | Contract owner: update API Contract §13 to say `INFRA_ERROR`, or formally schedule a backend rename under §31's Contract Change Policy |
| DRIFT-002/003/004/005/006/007/008/009/010/011/018/019: the full "documented, not backend-built" capability list | Product docs vs. real backend | Building any of these is new backend feature work, explicitly forbidden this unit | Product owner: prioritize/schedule, or formally descope from the product documents |
| DRIFT-012: Reports' missing Test Report/Final Audit Report/PDF/Excel | Product docs/Acceptance Matrix vs. real backend+frontend | Same — new backend + frontend feature work | Product owner: prioritize, or descope |
| DRIFT-013: Test Case Generator's zero in-UI disclosure of its simulated nature | Frontend UX vs. product docs' implied "real generation" | Protected boundary, explicitly out of this unit's redesign scope | Product owner: choose disclosure-banner (small, frontend-only) vs. real backend Model Gateway build (large) |
| DRIFT-015: realtime event-type vocabulary mismatch | API Contract vs. real backend/frontend | Already deferred by FE-1/FE-9 as "a §31 decision, not an implementation choice"; re-confirmed, not newly resolvable | Contract owner: formally update API Contract §14, or accept the real vocabulary as canonical |
| DRIFT-016: Acceptance Matrix never filled in | Documentation currency | This unit's own policy (§18) is to report drift, not rewrite authoritative documents unilaterally | Document owner: adopt this report's §8 as the real-status record, or formally update the Matrix file itself |
| DRIFT-022: no project-creation UI | Frontend vs. real backend capability | New frontend feature work, out of a cleanup-only unit's scope | Product owner: schedule a future FE unit |
| ACC-022 (Recovery/Fault Tolerance) | Verification completeness | Not independently re-tested live this session (frontend-focused unit, no worker-crash harness available) | A future unit (or a dedicated backend-recovery verification pass) should live-test worker crash/restart |

## 27. Limitations

- The dead-code sweep's initial candidate list came from a background research pass; every consequential claim in it (the 4 dead `AIQAContext` functions, the 2 dead files, the `Role` type, the unused `aiqaApi.ts` functions) was independently re-verified by this unit's own direct greps before any deletion — but the sweep's own "everything else is consumed" claims (the large majority of the codebase) were spot-checked rather than each individually re-derived from scratch.
- ACC-022 (recovery/fault tolerance) was not independently live-verified this unit — see §8/§20/§26.
- The Test Case Generator honesty gap (DRIFT-013) is documented, not fixed — a deliberate scope decision (§16), not an oversight, but genuinely leaves the gap in place for real end users until a future unit acts on it.
- No performance/bundle-size analysis beyond the raw before/after numbers in §22 was performed (the >500kB chunk-size warning is pre-existing and unrelated to this unit).

## 28. Out of Scope

**No FE-14 (or any post-FE-13) work was started.** This unit's only code changes were: two file deletions (§14), four dead-function removals plus their now-unused import (§14), and one source-code comment correction (§9/§14). No backend file, no database, no product/contract document, and no page's visual design was touched.
