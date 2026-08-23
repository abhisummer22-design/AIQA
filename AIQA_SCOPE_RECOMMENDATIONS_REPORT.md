# AIQA — ACC-012 Fix, and Scope Recommendations for the Seven Honest Stubs

**Date:** 22 August 2026
**Scope:** Part A — verify and close the ACC-012 page-copy defect. Part B — recommend build-or-descope
for every capability with zero backend implementation, take that decision directly from the product
owner (one question per item), reissue PRD/FRD/SRD scope where descoped, and propose a build order for
what's being built. **No implementation happens in this report beyond Part A's single, already-decided
fix** — Part B is recommendation, decision-recording, and ordering only, per this session's own
instruction.

---

## Part A — ACC-012 page-copy defect: fixed and verified

**Status at the start of this session:** a separate background session had already made the fix
(`forentend/src/components/pages/APITestingPage.tsx` rewritten from a misleading "not implemented" claim
to an honest redirect naming the Execution Hub, plus a new `APITestingPage.test.tsx` with 6 tests) and
was then deleted before this session began. Per this project's own "verify fresh, don't trust blindly"
discipline, the fix was not assumed correct — it was re-verified from scratch this session.

**Verification performed:**
```
cd forentend && npx vitest run       → 341 passed, 0 failed, 18 files (was 339/17 before this fix)
cd forentend && npx tsc --noEmit     → clean
cd forentend && npx vite build       → clean, 2716 modules
```
The 6 new `APITestingPage.test.tsx` tests cover: unauthenticated state gating, the honest redirect copy
rendering (and the old "not implemented" claim being genuinely gone), no fabricated demo content
surviving, no invented backend request for the page's own still-nonexistent capability, a real
`navigate('/execution')` action landing on the real Execution Hub, and Dashboard non-regression.
`CaseCPages.test.tsx` (the shared "7 honest stubs" test suite) was correctly updated to 6, with
`APITestingPage` removed and a comment explaining why.

**File changed:** `forentend/src/components/pages/APITestingPage.tsx` (copy + navigation action),
`forentend/src/components/pages/APITestingPage.test.tsx` (new), `forentend/src/components/pages/
CaseCPages.test.tsx` (updated for the 6 remaining stubs).

**Acceptance Matrix updated:** `AIQA_ACCEPTANCE_MATRIX_v1.0.md` ACC-012 moved from `TESTED` to
`VERIFIED` in both §5 (grid) and §5A (evidence), citing the file above. Tally corrected to 9 VERIFIED.

---

## Part B — Scope recommendations for the seven honest stubs and their backend domains

### Method

Every item below maps to a specific ACC-### row in `AIQA_ACCEPTANCE_MATRIX_v1.0.md` §5A, which already
established what's real and what isn't. Frontend stubs are paired with their corresponding backend domain
where one doesn't make sense without the other (e.g. a Mobile Testing page with no device dispatch behind
it isn't a meaningful separate decision) — 7 frontend-visible items plus 5 backend-only items, 12
decision points in total. For each, this report assessed real build cost, descope cost, and what this
project's actual 8-day trajectory suggests, then took a direct decision from the product owner —
individually, not bundled, matching the pattern already used successfully for the six decisions in
`AIQA_PRODUCT_DECISIONS_RECORD.md`.

### The twelve decisions

| # | Item | ACC ref | Decision | Rationale given |
|---|---|---|---|---|
| 1 | AIQA Orchestrator (visualizer + AI-assisted routing/coordination) | ACC-006 | **DEFER** | Foundational architecture project on its own; zero movement in 8 days of real work |
| 2 | API Testing dedicated rich UI | ACC-012 (extension) | **DESCOPE** | Real capability already ships via Execution Hub; bespoke authoring UI is a large investment for a polish gain |
| 3 | Custom Testing (generic authoring) | ACC-015 | **DESCOPE** | New sandboxing-safety surface on top of just-shipped containment work; no trajectory toward a generic framework |
| 4 | Mobile Testing (real device dispatch) | ACC-014 | **DEFER** | Real, recurring $ cost (device farm) is a budget decision separate from engineering — matches the PRD's own existing post-MVP precedent for adjacent mobile tooling |
| 5 | Feature Map / Flow (visual quality graph) | ACC-003 (visual) | **DEFER** | Practically gated on Context Ingestion (#8) |
| 6 | RBAC Testing scenario-matrix | ACC-011 | **DEFER** | Recommended BUILD (cheap — real primitives already exist); product owner chose to defer anyway |
| 7 | Test Case Validator dedicated page | ACC-005 | **BUILD** (investigate first) | Backend 100% real and tested (AUD-005); likely another cheap redirect/copy fix, same shape as ACC-012 |
| 8 | Context Ingestion | ACC-002 | **DEFER** | Single most expensive, most foundational item on this entire list |
| 9 | Quality-graph queries + requirement traceability | ACC-003 (query) | **DEFER** | Gated on Context Ingestion (#8) — nothing real to query without it |
| 10a | Generic API-test dispatch entity | ACC-012/013 (dispatch half) | **DESCOPE** | Redundant with real, working executor adapters |
| 10b | API telemetry UI (surface existing `/metrics`) | ACC-013 (telemetry half) | **BUILD** | Data already real and collecting; only a UI surface is missing |
| 11 | Report generation/export (PDF/Excel) | ACC-016 | **BUILD** | Findings/FailureCluster data already real and tested; this is an export/formatting layer only |
| 12 | Model Gateway — remaining 8 `AiTask` types | ACC-018 (extension) | **BUILD** (incrementally) | Foundational infrastructure already real and live-GO'd this week; adding a task type is well-understood, low-risk incremental work — the strongest build case on the whole list |

**Net result:** 3 descoped, 6 deferred, 4 to build (item 10 split into its descope/build halves,
`#10a`/`#10b`, since they had genuinely different costs and got genuinely different decisions).

**Governing document for the 3 descoped + 6 deferred items:**
`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md`, published alongside this report, reissues the PRD/FRD/SRD's
stated scope to actually match what was just decided — the original v6.0 PDF remains the unedited
historical record; v6.1 is the governing addendum on top of it.

### Note on item 6 — RBAC scenario-matrix

Flagged here explicitly because the decision went against this report's own recommendation, and that's
worth being honest about rather than quietly averaging it into the summary above: the underlying RBAC
primitives (`Role::Admin/Operator/Viewer`, `require_role`, `require_project_access`) already exist,
tested, live-verified multiple times including this week — this was assessed as one of the cheaper,
lower-risk items on the whole list. The product owner deferred it anyway. No further action follows from
that disagreement; the decision stands as given, recorded accurately, not re-litigated.

---

## Build order — for the 4 items decided to build

Ordered by lowest effort/risk first, then highest-value-with-real-dependents next, per the governing
instruction. **Nothing below is implemented by this report** — this is the proposal only; each item
starts as its own independently reportable unit in a future session.

### 1. Test Case Validator dedicated page — investigate first, likely cheapest

**Why first:** the backend lifecycle (`POST .../test-cases/:id/validate`, approve/reject/handoff) is
already 100% real and tested (AUD-005, 19 passing tests). The very first step of this unit should be
checking whether the Test Case Generator page already exposes validate/approve/reject inline — if so,
this may turn out to be another "the capability exists, the page's copy is just wrong" fix, the same
shape ACC-012 just was, not a new build. Lowest realistic effort of the four, real dependents already
proven.

### 2. API telemetry UI — surface existing `/metrics` data

**Why second:** the data is already real and already being collected (`api.rs::get_metrics`, Prometheus
format). This is a UI-only unit in the best case (parse the existing text-format response and chart it)
or UI + a small JSON-formatted metrics adapter in the more likely case (Prometheus text format isn't
naturally chart-friendly client-side). No new backend data source, no new executor, no new safety
surface — low risk, moderate-low effort.

### 3. Report generation/export (PDF/Excel)

**Why third:** real data exists (Findings/FailureCluster, already JSON-exportable), but this unit adds a
genuinely new dependency (a PDF/Excel generation library, backend or frontend side) and real formatting/
layout work — moderate effort, still low risk since no new data source or safety surface is involved.

### 4. Model Gateway — incremental task-type expansion

**Why last, and why "incremental" rather than one unit:** the foundational work (discovery, routing,
scoring, fallback, settings) is done; this isn't a single build, it's an ongoing pattern of adding one
`AiTask` variant at a time, each its own small, well-understood unit following the exact template
`ai_generation.rs` already established for `TestCaseGeneration`. Starting it after the three quick wins
above keeps early momentum on fast, visible, low-risk units first.

**Recommended first task type: `BugAnalysis`.** Of the 8 remaining unused `AiTask` variants
(`RequirementAnalysis`, `DocumentAnalysis`, `BugAnalysis`, `Traceability`, `SecurityAnalysis`,
`PerformanceAnalysis`, `UiAnalysis`, `Embedding`), `BugAnalysis` has the clearest real, already-existing
dependent: `intelligence.rs`'s failure-clustering/bug-drafting system is real, tested, and explicitly
rule-based today (this project's own `README.md`: *"Heuristic quality-graph/risk/flakiness/failure-
cluster/bug-drafting intelligence… rule-based (not LLM-backed)"*) — an AI-assisted root-cause hypothesis
layer on top of that already-real clustering is a genuine capability upgrade with a named, real consumer
waiting for it. By contrast: `RequirementAnalysis`/`DocumentAnalysis` depend on Context Ingestion (just
deferred, #8 above); `UiAnalysis` needs a vision-capable model, and the one live-verified provider today
(`gemma3:4b`) has `vision: NONE`; `Embedding` has the same gap (`embedding: NONE`). `BugAnalysis` is the
one remaining type with both a real dependent and no capability blocker on today's actual provider.

Order after that (not committed, a starting proposal only): whichever of `SecurityAnalysis`/
`Traceability`/`GeneralAi` gets a named real consumer next, decided at that point rather than guessed now.

---

## What this report deliberately did not do

- Did not implement any of the 4 build-decided items — build order only, per the governing instruction.
- Did not re-litigate the RBAC scenario-matrix decision after it went against this report's own
  recommendation (see note above).
- Did not touch the original PRD/FRD/SRD PDF directly — the v6.1 addendum governs alongside it instead,
  preserving the historical v6.0 record.
- Did not remove the dormant Quality Graph database schema — Quality Graph was deferred, not descoped;
  removing real (if unused) schema on a deferred item would be premature, not a cleanup.
