# AIQA — Post FE-13 — FE-14 Readiness & Remaining Capability Gap Audit Report

**Audit type:** Read-only. No implementation performed.
**Scope:** `forentend/src/`, `services/orchestrator/`, all root-level product/contract/report documents.
**Baseline:** FE-1 through FE-13, treated as a completed, non-regressed baseline per repository evidence (see §3, §22).

---

## 1. Overall Status

**PARTIALLY READY.**

A single, well-evidenced, low-risk FE-14 candidate exists (real Project Creation UI — Frontend-only, backend already implemented and tested). Everything else remaining in the system requires either a product/documentation decision (Category D) or is explicitly future/out-of-scope (Category E). FE-14 should proceed with the narrow scope in §19, not a broader one — no evidence supports a larger scope, and the brief this audit answers explicitly forbids "FE-14 = implement everything remaining."

---

## 2. Executive Summary

FE-13 left the system in a verified, non-fabricating, internally-consistent state: 261/261 tests passing, zero LLM/AI-model backend integration (confirmed by exhaustive grep), a fully-inventoried 24-entry drift matrix, and a dead-code cleanup with proof-of-safety for every deletion. This audit re-confirms that baseline (git state unchanged, tests unchanged, backend route count unchanged at 38) and then asks the only question FE-13 did not: **what, if anything, is genuinely ready to build next?**

The answer is narrow. Of FE-13's 24 documented drift entries and 8 unresolved items, exactly **one** clears every bar this audit requires (evidence-supported requirement, clear expected behavior, no invented workflow, known dependencies, clear ownership): **real Project Creation UI**. The backend endpoint (`POST /api/v1/projects`) is real, tested at the API level, and has had a typed, zero-consumer frontend transport function (`aiqaApi.projects.create`) sitting unused since FE-1 specifically for this purpose. No new backend work, no new data model, no ambiguity about behavior.

Everything else falls into one of two buckets:
- **Product/documentation decisions required first** (Category D): the `INFRA_ERROR`/`INFRASTRUCTURE_ERROR` contract mismatch, the Acceptance Matrix's stale template, the Test Case Generator's undisclosed-simulation banner, the realtime event-vocabulary mismatch — all real, all previously identified, none actionable without someone outside this audit choosing a direction.
- **Future/out of scope** (Category E): the seven unsupported modules (AIQA Orchestrator, Test Case Validator, RBAC Testing, API Testing, Mobile Testing, Custom Testing, Feature Map/Flow), PDF/Excel/per-run/audit/test/bug report generation, the Model Gateway/LLM layer, and Dashboard's 9 undocumented-as-real KPI concepts — all require new backend capability that no one has approved building.

No fake-capability shortcuts are recommended anywhere in this report. Where a real capability is missing, the correct missing layer is named instead of a workaround.

---

## 3. FE-13 Baseline

Extracted directly from `AIQA_PHASE4_FE13_DOCUMENTATION_CONTRACT_DRIFT_DEAD_CODE_CLEANUP_IMPLEMENTATION_REPORT.md` (285 lines, read in full this audit, byte-consistent with recollection — no drift):

| Item | FE-13 Result |
|---|---|
| Status | PASS |
| Exit Criterion | 16/16 satisfied |
| Drift Matrix | 24 entries (DRIFT-001 … DRIFT-024), each classified A–H |
| Acceptance Matrix Audit | 22 ACC items independently re-assessed against live inspection |
| Dead-code deleted | `MetricCard.tsx`, `StatusBadge.tsx` (whole files), 4 dead `AIQAContext` functions (`approveTestCase`, `rejectTestCase`, `startGenerationWorkflow`, `updateGenerationStep`) |
| Dead-code preserved | `Role` type, 5 unused `aiqaApi.ts` functions (incl. `projects.create`), Test Generator's protected `runs`/`activities` read-side machinery |
| Tests | 261/261 passing |
| Typecheck / Build | Clean; bundle JS 1,021.65 kB, CSS 88.95 kB |
| Backend changes | None |
| New fabrication discoveries | None meeting the reopening bar |
| Remaining Unresolved Drift | 8 items carried forward (see §7 below, re-mapped to gap IDs) |

This audit treats FE-1 through FE-13 as complete and does not repeat any of that work.

---

## 4. Source Inventory

Re-confirmed fresh this audit via `ls *.md *.pdf` in the repository root — **no new product/contract documents have appeared since FE-13.**

| Document | Version | Purpose | Authority | Intent |
|---|---|---|---|---|
| `AIQA_Consolidated_PRD_FRD_SRD_v6.0_Hybrid_Multi_Agent (1).pdf` | v6.0 | Product/functional/system requirements | Product intent (highest) | Describes full intended product, including unbuilt v6.0 Hybrid Multi-Agent capabilities |
| `AIQA_Business_Logic_Architecture_v2.0_Hybrid_Multi_Agent.pdf` | v2.0 | Business logic / architecture | Product intent (highest) | Same tier as PRD/FRD/SRD |
| `AIQA_Complete_End_to_End_Flow_Diagram_v6.0.pdf` | v6.0 | End-to-end flow | Product intent (highest) | Same tier |
| `AIQA_API_CONTRACT_v1.0.md` | v1.0 | Wire contract | Contract intent (subordinate) | Explicitly aspirational per its own §32 ("does NOT claim every endpoint above already exists") |
| `AIQA_ACCEPTANCE_MATRIX_v1.0.md` | v1.0 | Acceptance criteria | Contract intent (subordinate) | Never updated past its NOT_STARTED template (DRIFT-016) |
| `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md` | — | Prior gap findings | Contract intent | Pre-existing resolved/open findings, cited throughout |
| `AIQA_PHASE4_FE1…FE13_*_IMPLEMENTATION_REPORT.md` (13 files) | — | Implementation history | Actual-implementation record | Ground truth for what was built and why |
| `AIQA_PHASE2_BE8…BE15_*_IMPLEMENTATION_REPORT.md` (8 files) | — | Backend implementation history | Actual-implementation record | Ground truth for real backend behavior |
| ~15 other audit/verification/fix-report `.md` files (D1–D5, Phase1/Phase2 verification, deep audits, etc.) | — | Historical process record | Not a new-requirement source | Inventoried, not treated as new requirements |

No FRD/SRD/PRD/Flow Diagram exists as a standalone `.md` in-repo — only the PDFs above. No document required by this audit's own checklist is missing; nothing is reported as "NOT AVAILABLE — NOT ASSUMED" this round because every category (PRD/FRD/SRD, Business Logic, Flow Diagram, API Contract, Acceptance Matrix, Gap Analysis, FE-13 report) is present and was already located by FE-13.

---

## 5. Current Capability Inventory

Classification scheme: DOCUMENTED+IMPLEMENTED / DOCUMENTED+PARTIALLY IMPLEMENTED / DOCUMENTED+NOT IMPLEMENTED / UNDOCUMENTED+IMPLEMENTED / UNDOCUMENTED+NOT IMPLEMENTED / AMBIGUOUS.

| Domain | Classification | Evidence |
|---|---|---|
| **Authentication** (API key auth, 401/403, role enforcement) | DOCUMENTED+IMPLEMENTED | Real backend key hashing/lookup, real 401/403 paths, real role checks — verified live in FE-2/FE-13 |
| **Projects — selection/scope/isolation** | DOCUMENTED+IMPLEMENTED | Real `ProjectContext`, real cross-project isolation, verified live |
| **Projects — creation** | DOCUMENTED+PARTIALLY IMPLEMENTED | Backend real (`POST /projects`), transport real and typed, **zero UI** (DRIFT-022, re-confirmed this audit: `aiqaApi.projects.create` has 0 call sites in `src`) |
| **Test Generation — local simulation** | DOCUMENTED+PARTIALLY IMPLEMENTED | Fully working local `setTimeout` simulation; presents real model names (e.g. "gemini-2.0-flash") with no in-app disclosure of non-real generation (DRIFT-013) |
| **Test Generation — real backend integration** | DOCUMENTED+NOT IMPLEMENTED | Zero LLM/model-gateway backend route exists anywhere (confirmed by grep, FE-13 and re-affirmed) |
| **Execution — task/plan creation, lifecycle, cancel, approval gates** | DOCUMENTED+IMPLEMENTED | Real `TaskDef`/`RunPlan`, real state machine, verified live (ACC-007) |
| **Execution — DAG/prerequisites editor UI** | DOCUMENTED+PARTIALLY IMPLEMENTED | Backend `TaskDef.prerequisites` real; `ExecutionHubPage.tsx`'s creation form doesn't expose a dependency editor (DRIFT-023, disclosed) |
| **Realtime — event delivery, project scoping, reconnect** | DOCUMENTED+IMPLEMENTED | Verified live (ACC-008) |
| **Realtime — event-type vocabulary vs contract** | AMBIGUOUS | Real vocabulary (`TASK_*`) diverges from contract's aspirational vocabulary (`TASK_STATUS_CHANGED`, `MODEL_*`, …) — explicitly deferred as a product decision since FE-1/FE-9 (DRIFT-015) |
| **Evidence — generation, storage, retrieval, isolation** | DOCUMENTED+IMPLEMENTED | Verified live (ACC-009) |
| **Audit — logging, retrieval, filtering, isolation** | DOCUMENTED+IMPLEMENTED | Verified live (ACC-020) |
| **Findings/Intelligence — generation, clustering, review workflow** | DOCUMENTED+IMPLEMENTED | Real, rule-based (never LLM), verified live (ACC-010) |
| **Dashboard — 2 real KPIs (execution reliability, execution efficiency)** | DOCUMENTED+IMPLEMENTED | Real, computed from persisted state |
| **Dashboard — remaining 9 documented KPI concepts** | DOCUMENTED+NOT IMPLEMENTED | Intentionally omitted rather than fabricated (BE-12 §3) |
| **Reports — Findings/FailureCluster JSON export** | DOCUMENTED+IMPLEMENTED | Real, re-confirmed this audit (`ReportsPage.tsx` `handleExportJSON`) |
| **Reports — PDF export, Excel export, per-run report, audit report, test report, bug report** | DOCUMENTED+NOT IMPLEMENTED | No corresponding backend or frontend code exists anywhere; re-confirmed this audit via grep (only "Export JSON" present) |
| **Settings — API key panel** | DOCUMENTED+IMPLEMENTED | Real, backend-backed |
| **Settings — everything else on the page** | DOCUMENTED+NOT IMPLEMENTED | Disclosed local-only since FE-3 |
| **7 unsupported modules** (Orchestrator, Validator, RBAC Testing, API Testing, Mobile Testing, Custom Testing, Feature Map/Flow) | DOCUMENTED+NOT IMPLEMENTED | Re-confirmed this audit via `routeConfig.tsx`: all 7 carry `designStatus: 'unsupported-not-implemented'`, zero backend routes for any |

---

## 6. End-to-End Core Workflow Matrix

| Transition | Status | Note |
|---|---|---|
| Authenticate | REAL | |
| Select Project | REAL | |
| **Create Project** | **MISSING** | Real backend + transport, zero UI entry point — a fresh admin key with no assigned project has no in-app way to bootstrap one |
| Define/Generate Test Work | PARTIAL | Local simulation only, no real backend generation |
| Create Valid Executable Task/Plan | REAL | |
| Execute | REAL | |
| Realtime Progress | REAL | |
| Results | REAL | |
| Evidence | REAL | |
| Audit | REAL | |
| Findings/Review | REAL | |
| Dashboard/Reporting | PARTIAL | 2/11 KPIs real; Findings-JSON-only reporting |

The one **MISSING** transition (project creation) is the strongest, cleanest signal in this audit — every other transition is at least REAL or a disclosed PARTIAL, but project creation is a hard dead end for a scenario the product must support (onboarding a genuinely new deployment/project).

---

## 7. Complete Remaining-Gap Matrix

Gap Types used: `BACKEND_MISSING`, `FRONTEND_MISSING`, `FULL_STACK_MISSING`, `PARTIAL_IMPLEMENTATION`, `DOCUMENTATION_DRIFT`, `CONTRACT_DRIFT`, `PRODUCT_DECISION_REQUIRED`, `FUTURE_CAPABILITY`, `NO_GAP`.

| Gap ID | Capability | Source | Backend | Frontend | Runtime | Gap Type | Priority | Recommended Action |
|---|---|---|---|---|---|---|---|---|
| GAP-01 | Project creation UI | Flow Diagram p.1, ACC-001 | Real (`POST /projects`) | None | Dead end for new projects | `FRONTEND_MISSING` | **P0** | Build in FE-14 (see §19) |
| GAP-02 | Test Generator real-backend generation | PRD v6.0 Model Gateway | None | Local simulation only | Simulated, undisclosed | `PRODUCT_DECISION_REQUIRED` (backend scope if approved) | P1 | Decide: disclose simulation, or approve building real Model Gateway |
| GAP-03 | `INFRA_ERROR` vs `INFRASTRUCTURE_ERROR` naming | API Contract §13 | Real (`INFRA_ERROR`) | Matches backend | Consistent internally | `CONTRACT_DRIFT` | P2 | Doc owner updates contract text to match shipped value |
| GAP-04 | Acceptance Matrix template never updated | Acceptance Matrix v1.0 | N/A | N/A | N/A | `DOCUMENTATION_DRIFT` | P2 | Doc owner replaces NOT_STARTED template with real status (this report's §5 can seed it) |
| GAP-05 | Realtime event-vocabulary mismatch | API Contract vs `domain.rs::EventType` | Real, own vocabulary | Matches backend | Consistent internally | `PRODUCT_DECISION_REQUIRED` | P1 | Product decides: remap backend to contract vocabulary, or update contract to match backend |
| GAP-06 | Execution dependency/DAG editor UI | Flow Diagram, `TaskDef.prerequisites` | Real | Not exposed in creation form | Works without it | `FRONTEND_MISSING` | P2 | Candidate for a later phase, not FE-14 |
| GAP-07 | Dashboard's 9 remaining documented KPI concepts | PRD v6.0 §KPIs | Not computed | N/A | 2/11 real | `BACKEND_MISSING` | P2 | Requires backend aggregation work; not approved |
| GAP-08 | Reports: PDF export | PRD/FRD reporting reqs | None | None | Unsupported | `FULL_STACK_MISSING` | P2 | Future phase, needs product decision on report templates |
| GAP-09 | Reports: Excel export | PRD/FRD reporting reqs | None | None | Unsupported | `FULL_STACK_MISSING` | P2 | Same as GAP-08 |
| GAP-10 | Reports: per-run / audit / test / bug report generation | PRD/FRD reporting reqs | None | None | Unsupported | `FULL_STACK_MISSING` | P2 | Same as GAP-08 |
| GAP-11 | 7 unsupported modules (Orchestrator, Validator, RBAC Testing, API Testing, Mobile Testing, Custom Testing, Feature Map/Flow) | PRD v6.0 Hybrid Multi-Agent | None for any | Truthful "not implemented" pages | Correctly disclosed | `FUTURE_CAPABILITY` | P3 | Out of scope until product approves building specific backend capability per module |
| GAP-12 | ACC-022 Recovery/Fault Tolerance — not independently live-verified | Acceptance Matrix | Unknown (code-inspection only) | N/A | AMBIGUOUS | `PRODUCT_DECISION_REQUIRED` (verification, not implementation) | P2 | Optional low-cost live verification pass, not implementation |

No duplicate gaps for the same root cause. GAP-02/GAP-05 each roll up multiple FE-13 drift entries (DRIFT-013; DRIFT-015) rather than restating them separately.

---

## 8. P0 Gaps

**GAP-01 — Project Creation UI.** Without this, a documented core AIQA workflow (bootstrapping any project from a fresh state) is incomplete. The only way a genuinely new project currently enters the system is direct database/API manipulation outside the running application — not a legitimate product workflow. Backend is real and already exercised; frontend transport is real, typed, and has sat unused since FE-1 specifically pending this. Zero ambiguity about required behavior (name + optional description, same shape the backend already validates).

---

## 9. P1 Gaps

- **GAP-02 — Test Generator disclosure/real-backend decision.** Real, user-facing (the page shows real model names with no disclosure they're not called), but not implementable without a decision: disclose-as-simulation is a small frontend change; building a real Model Gateway is a large backend undertaking. Do not implement either without product sign-off.
- **GAP-05 — Realtime event-vocabulary decision.** Both sides are internally consistent and tested; the mismatch is only visible against the contract document. A product decision (remap vs. update contract) is needed before any code changes.

---

## 10. P2/P3 Gaps

- GAP-03, GAP-04: documentation-only fixes, no code impact, owned by whoever maintains the contract/matrix documents.
- GAP-06: a real, low-risk, low-value UI refinement — defer to a later phase to keep FE-14 focused.
- GAP-07 through GAP-10: all require new backend capability nobody has approved; genuinely future work.
- GAP-11 (7 unsupported modules): P3, explicitly future per PRD v6.0's own Hybrid Multi-Agent framing — none has been approved for near-term build.
- GAP-12: an optional verification task, not a capability gap; cheap to add to any phase if there's room, not required for FE-14.

---

## 11. Product Decisions Required

| Item | Question | Options |
|---|---|---|
| Test Generator (GAP-02) | Should the UI disclose that generation is simulated, or should a real Model Gateway be built? | (a) Add disclosure banner — small FE change, ships fast. (b) Approve backend Model Gateway work — large, new architecture. |
| Realtime vocabulary (GAP-05) | Should the backend's real event names be remapped to the contract's aspirational names, or should the contract be updated to match the shipped backend? | (a) Remap backend — breaking change to a working, tested system. (b) Update contract — clerical, zero runtime risk. |
| `INFRA_ERROR` naming (GAP-03) | Same shape as above, smaller stakes. | (a) Rename backend enum value — breaking. (b) Update contract text — clerical. |
| Acceptance Matrix (GAP-04) | Who owns updating the NOT_STARTED template to reflect 13 FE units of real status? | This report's §5 and §7 can seed the update; needs a document owner to formally accept it. |
| 7 unsupported modules (GAP-11) | Which, if any, should ever be built, and in what order? | Requires per-module product prioritization; none is assumed here. |

---

## 12. Backend Requirements (list only — not implemented by this audit)

- Real Model Gateway/LLM integration, if GAP-02 is resolved toward "build it real" (large; out of scope for FE-14 regardless).
- Backend aggregation for the 9 remaining Dashboard KPI concepts (GAP-07).
- Report-generation backend (templates, PDF/Excel rendering, per-run/audit/test/bug report entities) (GAP-08–10).
- Any backend work behind the 7 unsupported modules (GAP-11) — none currently scoped or approved.

---

## 13. Frontend Requirements

- **Project Creation UI (GAP-01)** — the one FE-14 candidate. See §19.
- Test Generator disclosure banner, only if GAP-02 is resolved toward "disclose" (not assumed here).
- Execution dependency/DAG editor UI (GAP-06) — deferred, not FE-14.

---

## 14. Full-Stack Requirements

- GAP-08, GAP-09, GAP-10 (report generation family) — all require both new backend entities/rendering and new frontend UI; none is approved or scoped.
- GAP-11 (any of the 7 unsupported modules, if ever approved) — full-stack by definition, no scoping performed here.

---

## 15. Unsupported Module Decision Matrix

| Module | Documented? | Required for current milestone? | Backend-architecture-supported? | New entities/models needed? | Depends on another missing capability? | Decision |
|---|---|---|---|---|---|---|
| AIQA Orchestrator | Yes (PRD v6.0) | No | No | Yes (orchestration/plan entities) | Depends on Model Gateway | Out of scope for FE-14; product decision required |
| Test Case Validator | Yes | No | No | Yes (validation-result entities) | Depends on Test Generator's real-backend decision (GAP-02) | Out of scope for FE-14; product decision required |
| RBAC Testing | Yes | No | Enforcement already real; testing-tool itself is not | Yes (RBAC test-run entities) | No | Out of scope for FE-14; product decision required |
| API Testing | Yes | No | No | Yes | No | Out of scope for FE-14; product decision required |
| Mobile Testing | Yes (with a documented caveat the "v5.0 baseline retained" claim may itself be inaccurate — FE-13 DRIFT-007) | No | No | Yes | No | Out of scope for FE-14; needs its own documentation-accuracy check before any product decision |
| Custom Testing | Yes | No | No | Yes | No | Out of scope for FE-14; product decision required |
| Feature Map / Flow | Yes | No | No | Yes | No | Out of scope for FE-14; product decision required |

None of the seven is recommended for FE-14. Each is truthfully disclosed as "not implemented" today, which is the correct interim state — no fake implementation is recommended for any of them.

---

## 16. Reports Capability Matrix

| Capability | Status |
|---|---|
| Report generation (Findings/FailureCluster) | SUPPORTED |
| Report entities (Finding, FailureCluster) | SUPPORTED |
| JSON export | SUPPORTED |
| PDF export | UNSUPPORTED |
| Excel export | UNSUPPORTED |
| Single-module export | PARTIALLY SUPPORTED (JSON export of the Findings module only; no other module exports) |
| Per-run report | UNSUPPORTED |
| Audit report (formatted report *of* the audit log, distinct from the real Audit page itself) | UNSUPPORTED |
| Test report | UNSUPPORTED |
| Bug report | AMBIGUOUS — Findings/FailureCluster JSON export is the closest real analogue but was never documented as satisfying a "bug report" requirement specifically; needs a product decision on whether it counts |

Findings-based reporting does **not** satisfy the full documented reporting surface. Six of nine sub-capabilities are unsupported or ambiguous. None is recommended for FE-14 without a product decision on scope and format.

---

## 17. Test Generator → Execution Gap

**Does a valid real handoff exist? No.**

The Test Generator's `handleExportToExecution`/`triggerNewRun` path creates a fabricated `AIQAContext` run that has no real `TaskDef` mapping — this is a disclosed, considered decision from earlier FE units (FE-8), not an oversight. There is no real backend generation endpoint at all (confirmed: zero LLM/model-gateway routes anywhere in `services/orchestrator`), so there is nothing for a "real" handoff to originate from.

**What data-model gap exists:** the generated test case shape (`GeneratedTestCase`, local-only) has no defined mapping to the real `TaskDef` the Execution Hub actually consumes (`id`, `name`, `prerequisites`, `simulate`, `executor`, `params`). No one has designed that mapping.

**Priority:** P1 — real, user-visible, but gated behind GAP-02's product decision, not independently actionable.

**What must happen before implementation:** (1) resolve GAP-02 (is generation ever going to be real, or permanently disclosed-simulated); (2) only if "real" is chosen, design the `GeneratedTestCase → TaskDef` mapping as its own scoped unit before any code is written. Do not invent this mapping speculatively.

---

## 18. Dependency Graph

```
GAP-01 (Project Creation UI) — no dependencies, ready now
GAP-02 (Test Generator decision) — blocks → GAP-17-mapping (Test Generator → Execution real handoff)
GAP-02 — blocks → any Model Gateway backend work (GAP-11 Orchestrator/Validator depend transitively on this)
GAP-05 (Realtime vocabulary decision) — independent, no blockers, blocks nothing else currently
GAP-03 (INFRA_ERROR naming) — independent, clerical
GAP-04 (Acceptance Matrix update) — depends on → this report's own §5/§7 being accepted as a source
GAP-08/09/10 (Report family) — depends on → a product decision on report scope/format (not yet raised as its own gap; would need to be opened as GAP-13 if pursued)
GAP-11 (7 unsupported modules) — each depends on → its own backend-architecture approval; Orchestrator and Validator additionally depend on → GAP-02
```

GAP-01 is the only P0/P1 item with zero dependencies — this is the primary reason it is the sole FE-14 recommendation.

---

## 19. Recommended FE-14 Scope

**Capability:** Real Project Creation UI.

**Backend scope:** None. `POST /api/v1/projects` already exists, already validated (`CreateProjectRequest { name, description? }`), already returns a real `Project`.

**Frontend scope:**
- A "Create Project" entry point added to the existing project selector (Header/`ProjectSelector`), not a new page — avoids touching Dashboard or Test Generator's approved designs.
- A form (name required, description optional) calling the already-real, already-typed `aiqaApi.projects.create`.
- On success, the new project becomes selectable/selected via the existing `ProjectContext` machinery (no new state model needed).
- Add test coverage for `aiqaApi.projects.create` (success and error paths) — currently zero coverage exists for `.create`/`.update`/`.get` in `aiqaApi.test.ts`, a genuine gap this unit should close since it's the code being newly exercised.

**Dependencies:** None.

**Acceptance criteria:**
1. A user with a valid key but zero accessible projects can create one entirely through the UI, with no direct database/API access required.
2. The created project immediately becomes the active project.
3. Creation errors (e.g. validation failure) surface a real error state, not a silent failure.
4. No existing page's approved design (Dashboard, Test Generator) is modified.
5. `aiqaApi.projects.create` gains real test coverage.

**Explicit exclusions:** everything in §7 other than GAP-01 — no Test Generator disclosure/backend work, no report-export work, no unsupported-module work, no realtime-vocabulary remapping, no dependency-editor UI, no Dashboard KPI expansion. Including any of these would violate this audit's own "FE-14 must be focused, not everything remaining" constraint.

---

## 20. Recommended Phase After FE-14

State only — not implemented here. The most defensible next phase is a **Product/Documentation Decision Round**, not another code phase: resolving GAP-02 (Test Generator), GAP-05 (realtime vocabulary), GAP-03 (contract naming), and GAP-04 (Acceptance Matrix ownership) before any further backend-touching work is scoped. Only once those decisions land does it become possible to honestly scope a phase that includes backend work (e.g. a real Model Gateway, or report generation) — attempting to scope that now would require inventing product decisions this audit is explicitly forbidden from making.

---

## 21. Risks

- **Scope creep risk on FE-14:** GAP-01's UI naturally invites "while we're in the selector, let's also add project editing/deletion" — resist; `aiqaApi.projects.update` exists but was not audited here as ready (no evidence gathered on its UI requirements), and is not part of this recommendation.
- **Contract-drift risk if GAP-03/GAP-05 stay unresolved indefinitely:** every new backend consumer built against the real vocabulary widens the gap from the documented contract, making a future remap more expensive the longer it's deferred.
- **Test Generator credibility risk (GAP-02):** the undisclosed simulation banner is real user-facing behavior; the longer it stays undecided, the longer a real product surface silently overclaims real AI usage.
- **Acceptance Matrix staleness risk (GAP-04):** as long as it stays a NOT_STARTED template, anyone consulting it as a status source (rather than the FE-13 report or this report) will be misled.

---

## 22. Verification Results

All commands actually run this audit, read-only:

| Command | Result |
|---|---|
| `Read` — full FE-13 report (285 lines) | Byte-consistent with prior knowledge; no drift |
| `git status --short` | 34 lines — identical composition to FE-13's own end-state (28 modified, 3 deleted, untracked new-report files); zero new changes this session |
| `git log --oneline -3` | Unchanged top-3 commits; no new commits |
| `git diff --stat services/orchestrator/` | Empty — zero backend diff |
| `npx tsc --noEmit` (forentend) | Clean, exit 0 |
| `npx vitest run --reporter=dot` (forentend, background) | **261/261 tests passing, 14 files**, exit 0 — unchanged from FE-13 |
| `Grep "\.route\(" services/orchestrator/src/api.rs` (count) | 38 — unchanged from FE-13's documented route count |
| `ls *.md *.pdf` (repo root) | No new documents since FE-13 |
| `Grep` for `projects.create`/`CreateProject`/UI creation entry points | Zero UI call sites — confirms DRIFT-022/GAP-01 still open |
| `Read routeConfig.tsx` | 14 routes, `designStatus` unchanged from FE-13's documented state |
| `Grep` PDF/Excel/export in `ReportsPage.tsx` | Only "Export JSON" present — confirms GAP-08/09/10 still fully unsupported |
| `git status --short \| wc -l` (final check) | 34 — matches the pre-audit count exactly |

No file was created, edited, or deleted in `forentend/`, `services/orchestrator/`, or any database during this audit, other than this report file itself.

---

## 23. Code Changes

**None.** No frontend file, backend file, or database was modified during this audit. The only file written was this report (`AIQA_POST_FE13_FE14_READINESS_CAPABILITY_GAP_AUDIT_REPORT.md`), which is the deliverable this task requested, not a code or product-document change. No audit violation occurred.

---

## 24. Final Recommendation

**A — FE-14 can start immediately, with the exact focused scope in §19: real Project Creation UI (frontend-only, zero backend work, zero new data model, one clear acceptance-tested capability).**

No other candidate in the remaining-gap matrix clears this audit's own readiness bar (evidence-supported requirement, clear expected behavior, no invented workflow, known dependencies, clear frontend/backend ownership) without a prior product or documentation decision. Recommendation B (product decision required) applies to GAP-02/GAP-05/GAP-11 but does not block GAP-01, so it does not override A for FE-14 itself — those items should be tracked as the decision round in §20, run in parallel with or after FE-14, not as a precondition for it.
