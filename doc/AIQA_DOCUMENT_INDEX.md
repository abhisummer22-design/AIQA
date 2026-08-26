# AIQA — Document Index

**Date:** 22 August 2026
**Coverage:** all 74 markdown documents in the repository root as of this date, including this index
itself, `AIQA_ID_REGISTRY.md`, and `AIQA_PRODUCT_DECISIONS_RECORD.md` — all three published together this
session. Companion to `AIQA_ID_REGISTRY.md` (ID disambiguation) and `AIQA_PRODUCT_DECISIONS_RECORD.md`
(the decisions several "superseded" rows below rest on).

**Status legend**
- **current** — no evidence of being superseded; still the operative source for what it covers.
- **superseded** — a later, named document has replaced this one's specific claims. The later document
  is named in the "Superseded by" column.
- **historical** — a dated, point-in-time record, correct for when it was written, not current guidance,
  and not superseded by any single later document (usually because later work moved past its scope
  entirely rather than correcting it).

---

## Governing / contract documents

| File | Date | Description | Status |
|---|---|---|---|
| `AIQA_API_CONTRACT_v1.0.md` | — | Backend↔frontend API integration contract: endpoints, error envelope, versioning, event rules | current — two corrections pending (`AIQA_PRODUCT_DECISIONS_RECORD.md` #1 status naming, #2 event vocabulary), not yet applied to this file's own text |
| `AIQA_ACCEPTANCE_MATRIX_v1.0.md` | Reconciled 22 Aug 2026 | The completion/verification matrix — 22 ACC + 7 E2E criteria | **current** — rebuilt from real evidence this session (§5/§5A/§34/§35); see below |
| `AIQA_ID_REGISTRY.md` | 22 Aug 2026 | This corpus's ID-disambiguation registry (B# vs BE-#, the two BE-9/BE-10s, FE-13/FE-15) | current |
| `AIQA_DOCUMENT_INDEX.md` | 22 Aug 2026 | This document | current |
| `AIQA_PRODUCT_DECISIONS_RECORD.md` | 22 Aug 2026 | Six product decisions, made directly by the product owner this session | current |

## This week's newest reports (22 August 2026)

| File | Description | Status |
|---|---|---|
| `AIQA_AI_MODEL_AUTO_DISCOVERY_E2E_VERIFICATION_REPORT.md` | Live **GO** verdict — real AI generation through the full stack, real Ollama, real differential output | **current** |
| `AIQA_SECURITY_REMEDIATION_FOUR_DEFERRED_FINDINGS_REPORT.md` | Closes CORS/SSRF/approval-durability fully; executor sandboxing partially, with a dated risk acceptance | **current** |

## Bootstrap-era audits (14–15 August 2026)

| File | Date | Description | Status |
|---|---|---|---|
| `AIQA_ARCHITECTURE_AUDIT_REPORT.md` | 14 Aug 2026 | Forensic audit: monolithic Rust orchestrator, no LLM, unsandboxed executors, simulated dashboard | **historical** — the "CRITICAL — Missing Executor Isolation" finding it raised is addressed (partially, with a dated acceptance) by `AIQA_SECURITY_REMEDIATION_FOUR_DEFERRED_FINDINGS_REPORT.md`; the rest of this document describes an architecture real work has moved past |
| `AIQA_DEEP_AUDIT_REPORT.md` | 14 Aug 2026 | Forensic audit connecting real backend to `apps/web` frontend; deletes fabricated shadow server | **historical** — describes `apps/web` as the target frontend, contradicted by every later frontend unit (see `apps/web` supersession below) |
| `AIQA_BACKEND_AUDIT_REPORT.md` | 15 Aug 2026 | Deep orchestrator audit — C1/C2/H1-4/M1-5/L1-4 severity-coded findings, incl. M2 (SSRF) and L4 (process-local approval) | **historical** for its narrative; its M2/L4 findings are **closed** by `AIQA_SECURITY_REMEDIATION_FOUR_DEFERRED_FINDINGS_REPORT.md` |

## Frontend-selection audits (17 August 2026) — `apps/web` vs `forentend`

| File | Description | Status |
|---|---|---|
| `AIQA_FRONTEND_AUDIT_REPORT.md` | Read-only audit: `forentend` had zero network calls at that point, entirely fabricated/simulated | **historical** |
| `AIQA_FRONTEND_PRE_IMPLEMENTATION_AUDIT.md` | Compares `forentend` vs `apps/web` before choosing an integration target | **historical** |
| `AIQA_FRONTEND_CONSOLIDATION_REPORT.md` | Declares `apps/web` the "Official Production Frontend" | **superseded** — dated note added this session; see below |
| `AIQA_FRONTEND_CONTRACT_READINESS_MATRIX.md` | Contract readiness matrix scoped to `apps/web` | **superseded** — dated note added this session; see below |
| `AIQA_FRONTEND_PHASE2_INTEGRATION_READINESS_REPORT.md` | Phase 2 integration readiness scoped to `apps/web` | **superseded** — dated note added this session; see below |
| `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md` | 17 Aug 2026. Architecture decision: `forentend` as target + backend/API gap-closing plan | **current** — this is the plan that ultimately *did* win in practice (every FE-1…26 unit landed in `forentend`), even though the three files above briefly declared the opposite three days later |

**`apps/web` supersession (Part B of this session):** all three `AIQA_FRONTEND_CONSOLIDATION_REPORT.md`,
`AIQA_FRONTEND_CONTRACT_READINESS_MATRIX.md`, and `AIQA_FRONTEND_PHASE2_INTEGRATION_READINESS_REPORT.md`
now carry a dated superseding note at their top, stating plainly that `apps/web` is not the shipping
frontend — every FE unit, the AI router work, and the Security Remediation work all landed in `forentend`
instead, and `apps/web` has had no real commit since 18 August. `apps/web/` itself has **not** been
archived or deleted; no product decision to do so has been made (see the Decisions Record).

## Backend Implementation Plan & Phase 1 (17 August 2026)

| File | Date | Description | Status |
|---|---|---|---|
| `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md` | 17 Aug 2026 | Remediation plan derived from the audits above (B1-B15 task numbering); planning only, no code | **superseded** — folded into the Phase 1/2 implementation reports below, which executed it |
| `AIQA_PHASE1_IMPLEMENTATION_REPORT.md` | 17 Aug 2026 | Phase 1 backend correctness/persistence work (B1-B6, B14) | current |
| `AIQA_PHASE1_FINAL_VERIFICATION_REPORT.md` | 17 Aug 2026 | Adversarial verification of B1-B6/B14 via direct SQLite inspection | current |
| `AIQA_D1_FIX_REPORT.md` … `AIQA_D5_FIX_REPORT.md` | 17 Aug 2026 (×5) | D1-D5: swallowed persistence failures, orphaned tasks, divergent finalization rules, fault-injection audit | current |
| `AIQA_PHASE1_FINAL_INDEPENDENT_VERIFICATION_REPORT.md` | 17 Aug 2026 | Independent re-verification of D1-D5 via *reversion testing* (temporarily reverting fixes, confirming failures reproduce) | current — the more rigorous of the two "Phase 1 Final Verification" reports; both are genuinely independent passes, not duplicates (see `AIQA_ID_REGISTRY.md`) |

## Phase 2 — Backend (`BE-#`) implementation reports (17-18 August 2026)

| File | Description | Status |
|---|---|---|
| `AIQA_PHASE2_BE8_IMPLEMENTATION_REPORT.md` | Projects API resource | current |
| `AIQA_PHASE2_BE9_IMPLEMENTATION_REPORT.md` | Structured error envelope (§2.5) | current — see BE-9 collision, `AIQA_ID_REGISTRY.md` |
| `AIQA_PHASE2_ACTUAL_BE9_PROJECT_SCOPED_ROUTING_REPORT.md` | The real "BE-9" per the plan's own numbering: project-scoped routing | current — see BE-9 collision |
| `AIQA_PHASE2_ACTUAL_BE9_INDEPENDENT_VERIFICATION_REPORT.md` | Independent verification of the above | current |
| `AIQA_PHASE2_BE10_IMPLEMENTATION_REPORT.md` | Completes the structured-error envelope for extractor rejections | current — see BE-10 collision, `AIQA_ID_REGISTRY.md` |
| `AIQA_BE10_IMPLEMENTATION_REPORT.md` | The *other*, unrelated "BE-10": project-scoped Test Intelligence route | current — see BE-10 collision |
| `AIQA_PHASE2_BE12_DASHBOARD_KPI_IMPLEMENTATION_REPORT.md` | Dashboard/KPI JSON endpoints | current |
| `AIQA_PHASE2_BE13_EXECUTION_ALIGNMENT_IMPLEMENTATION_REPORT.md` | Execution API alignment, real auditable cancel | current |
| `AIQA_PHASE2_BE14_REALTIME_EVENT_STREAM_IMPLEMENTATION_REPORT.md` | Project-scoped realtime WebSocket stream | current |
| `AIQA_PHASE2_BE15_EVIDENCE_FINDINGS_AUDIT_IMPLEMENTATION_REPORT.md` | Evidence/Findings/Audit contract-path routes | current |
| `BE-11_DASHBOARD_KPI_CAPABILITY_AUDIT.md` | Audits missing KPI concepts, selects Average Run Duration | current |
| `BE-12B_AVERAGE_RUN_DURATION_IMPLEMENTATION_REPORT.md` | Implements Average Run Duration KPI (backend) | current |

## Phase 4 — Frontend (`FE-#`) implementation reports (18-21 August 2026)

| File | Description | Status |
|---|---|---|
| `AIQA_PHASE4_FE1_FRONTEND_INTEGRATION_LAYER_IMPLEMENTATION_REPORT.md` | Real `aiqaApi.ts` client, wired to zero pages yet | current |
| `AIQA_PHASE4_FE2_IMPLEMENTATION_REPORT.md` | Auth UI: key entry/storage/masking, 401 vs 403 states | current |
| `AIQA_PHASE4_FE3_IMPLEMENTATION_REPORT.md` | Reusable `StateViews`/`StateGate` component layer | current |
| `AIQA_PHASE4_FE4_ROUTING_NAVIGATION_IMPLEMENTATION_REPORT.md` | Real client-side routing/navigation | current |
| `AIQA_PHASE4_FE5_GLOBAL_PROJECT_SCOPE_IMPLEMENTATION_REPORT.md` | Centralized `ProjectContext` | current |
| `AIQA_PHASE4_FE6_AUDIT_PAGE_IMPLEMENTATION_REPORT.md` | Real Audit page | current |
| `AIQA_PHASE4_FE7_EVIDENCE_VIEWER_IMPLEMENTATION_REPORT.md` | Real Evidence Viewer | current |
| `AIQA_PHASE4_FE8_EXECUTION_HUB_IMPLEMENTATION_REPORT.md` | Execution Hub with polling-based monitoring | **superseded by `AIQA_PHASE4_FE9_REALTIME_IMPLEMENTATION_REPORT.md`** — polling explicitly replaced by WebSocket realtime one unit later |
| `AIQA_PHASE4_FE9_REALTIME_IMPLEMENTATION_REPORT.md` | Replaces FE-8's polling with realtime WebSocket | current |
| `AIQA_PHASE4_FE10_DASHBOARD_REAL_DATA_IMPLEMENTATION_REPORT.md` | Dashboard: fabricated → real backend data | current |
| `AIQA_PHASE4_FE11_FABRICATION_REMOVAL_IMPLEMENTATION_REPORT.md` | Removes remaining fabricated data across pages | current |
| `AIQA_PHASE4_FE12_STATUS_VOCABULARY_MIGRATION_IMPLEMENTATION_REPORT.md` | Status-vocabulary consistency inventory + fix | current |
| `AIQA_PHASE4_FE13_DOCUMENTATION_CONTRACT_DRIFT_DEAD_CODE_CLEANUP_IMPLEMENTATION_REPORT.md` | Doc/contract drift matrix + dead-code cleanup | current — an audit unit, not a feature unit; see `AIQA_ID_REGISTRY.md` |
| `AIQA_PHASE4_FE14_PROJECT_CREATION_UI_IMPLEMENTATION_REPORT.md` | Real Project Creation UI | current |
| `AIQA_PHASE4_FE15_PRODUCT_CONTRACT_DECISION_RESOLUTION_AUDIT_REPORT.md` | Identifies the decisions `AIQA_PRODUCT_DECISIONS_RECORD.md` resolves | current — an audit unit, not a feature unit; see `AIQA_ID_REGISTRY.md` |
| `AIQA_PHASE4_FE16_TEST_GENERATOR_TRUTHFULNESS_FAKE_EXECUTION_HANDOFF_REMOVAL_IMPLEMENTATION_REPORT.md` | Removes fake AI-model branding + fabricated handoff | current |
| `AIQA_PHASE4_FE17_EXECUTION_DEPENDENCY_DAG_EDITOR_IMPLEMENTATION_REPORT.md` | DAG/dependency editor UI | current |
| `AIQA_PHASE4_FE18_IMPLEMENTATION_REPORT.md` | Real approval/rejection attribution display | current |
| `AIQA_PHASE4_FE19_IMPLEMENTATION_REPORT.md` | Finding `promoted_at` timestamp display | current |
| `AIQA_PHASE4_FE20_IMPLEMENTATION_REPORT.md` | Full 11-field task-status count grid | current |
| `AIQA_PHASE4_FE21_IMPLEMENTATION_REPORT.md` | Audit-only: Test Intelligence needs a backend route first | current — this is what `AIQA_BE10_IMPLEMENTATION_REPORT.md` then built |
| `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md` | Flags PRD/FRD/SRD-vs-shipped contradictions | current |
| `AIQA_POST_FE13_FE14_READINESS_CAPABILITY_GAP_AUDIT_REPORT.md` | Selects Project Creation UI as next safe unit | **historical** — a point-in-time sequencing decision, not a standing claim |

## Post-FE-21 units (21 August 2026) — naming convention drifts here

| File | Description | Status |
|---|---|---|
| `AIQA_FE22_IMPLEMENTATION_REPORT.md` | Test Intelligence surfaced on Reports page | current |
| `FE-23_FRONTEND_CAPABILITY_AUDIT.md` | Post-FE-22 capability matrix, FE-1…22/BE-10 | **historical** — a snapshot audit; superseded in currency by `AIQA_FULL_PROJECT_FRESH_AUDIT_REPORT.md`'s later module table, not by name |
| `FE-24_IMPLEMENTATION_REPORT.md` | Real notification bell (recent-activity feed) | current |
| `FE-25_IMPLEMENTATION_REPORT.md` | Test Intelligence realtime auto-refresh | current |
| `FE-26_AVERAGE_RUN_DURATION_KPI_IMPLEMENTATION_REPORT.md` | Displays BE-12b's KPI on the Dashboard | current |
| `AIQA_SKILLS_MCP_PHASE1_IMPLEMENTATION_REPORT.md` | Skills & MCP selection panel (no real MCP backend yet) | current |
| `AIQA_AI_MODEL_AUTO_DISCOVERY_IMPLEMENTATION_REPORT.md` | Central AI router; verdict **PARTIAL** at the time | **superseded by `AIQA_AI_MODEL_AUTO_DISCOVERY_E2E_VERIFICATION_REPORT.md`** — same-day upgrade to a live **GO** |
| `AIQA_AUD005_TEST_CASE_PERSISTENCE_HANDOFF_IMPLEMENTATION_REPORT.md` | Real test-case persistence/lifecycle + execution handoff | current |
| `AIQA_TEST_FLAKINESS_ROOT_CAUSE_AND_FIX_IMPLEMENTATION_REPORT.md` | Fixes two test-harness timing races (test-only) | current |

## Fresh full-project audits (20-21 August 2026) — near-duplicate names, different scope

| File | Description | Status |
|---|---|---|
| `AIQA_FRESH_FULL_PROJECT_AUDIT_REPORT.md` | Full-project audit checkpointed at FE-8/9/10 (git `de7ff43`) | **superseded by `AIQA_FULL_PROJECT_FRESH_AUDIT_REPORT.md`** — an earlier checkpoint of the same kind of audit |
| `AIQA_FULL_PROJECT_FRESH_AUDIT_REPORT.md` | Module-by-module audit through FE-26/BE-12b; "Conditionally Ready" | **current** — this is the one `AIQA_ACCEPTANCE_MATRIX_v1.0.md` §5A cites |
| `AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md` | Meta-audit of the entire corpus (~55 documents), raised R1-R12, named the four dropped security findings and six open decisions | **current** for everything not yet resolved; **closed** for: R2 (security findings — `AIQA_SECURITY_REMEDIATION_FOUR_DEFERRED_FINDINGS_REPORT.md`), the six §9 decisions (`AIQA_PRODUCT_DECISIONS_RECORD.md`), GAP-04/Decision 4 acceptance-matrix ownership (`AIQA_ACCEPTANCE_MATRIX_v1.0.md` §5A/§34/§35), and the `apps/web` supersession (Part B, this session) |

## Backlog / meta

| File | Description | Status |
|---|---|---|
| `AIQA_PROMPT_BACKLOG_AND_STATUS_ASSESSMENT.md` | A status assessment + library of ready-to-paste prompts for continuing the project, prepared by an earlier session | **current** — actively being worked through; this session executed its security-remediation and governance/decisions prompts |
| `README.md` | Project overview, layout, running instructions | current |

---

## What superseded what — quick reference

| Superseded document | Superseded by | Reason |
|---|---|---|
| `AIQA_FRONTEND_CONSOLIDATION_REPORT.md` | (dated note added in place, no single replacement) | Declared `apps/web` the shipping frontend; contradicted by all subsequent work |
| `AIQA_FRONTEND_CONTRACT_READINESS_MATRIX.md` | (dated note added in place) | Same reason |
| `AIQA_FRONTEND_PHASE2_INTEGRATION_READINESS_REPORT.md` | (dated note added in place) | Same reason |
| `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md` | `AIQA_PHASE1_IMPLEMENTATION_REPORT.md` + Phase 2 `BE-#` reports | Was a plan; the plan was executed |
| `AIQA_PHASE4_FE8_EXECUTION_HUB_IMPLEMENTATION_REPORT.md` | `AIQA_PHASE4_FE9_REALTIME_IMPLEMENTATION_REPORT.md` | Polling explicitly replaced by realtime WebSocket |
| `AIQA_FRESH_FULL_PROJECT_AUDIT_REPORT.md` | `AIQA_FULL_PROJECT_FRESH_AUDIT_REPORT.md` | Earlier checkpoint of the same audit type |
| `AIQA_AI_MODEL_AUTO_DISCOVERY_IMPLEMENTATION_REPORT.md` | `AIQA_AI_MODEL_AUTO_DISCOVERY_E2E_VERIFICATION_REPORT.md` | PARTIAL verdict upgraded to live GO, same day |
| `AIQA_ACCEPTANCE_MATRIX_v1.0.md` (its own pre-22-Aug §5/§34/§35 content) | Itself, §5/§5A/§34/§35 as rebuilt this session | Original bootstrap template, never previously touched |

**Everything else is current or historical, not superseded** — "historical" is not a synonym for wrong;
it means the document was accurate for its moment and nothing has formally corrected or replaced its
specific claims, even though later work has moved past its scope.
