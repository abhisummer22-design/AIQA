# AIQA — PRD/FRD/SRD Scope Corrections, v6.1

**Date:** 22 August 2026
**Base document:** `AIQA_Consolidated_PRD_FRD_SRD_v6.0_Hybrid_Multi_Agent (1).pdf` — unedited, remains the
historical v6.0 record. This addendum is the authoritative modification layer on top of it, not a
replacement of it; where the two disagree, this document governs for the items named below, and v6.0
governs everything else unchanged.
**Why a markdown addendum instead of editing the PDF:** the base document is a formal PDF; editing it in
place would destroy the historical v6.0 record this project's own documentation discipline otherwise
preserves everywhere else (see `AIQA_DOCUMENT_INDEX.md`'s superseded-vs-historical distinction). A
dated, versioned addendum achieves the same governing effect without that loss.
**Authority:** every correction below traces to a specific, dated decision in
`AIQA_PRODUCT_DECISIONS_RECORD.md` or `AIQA_SCOPE_RECOMMENDATIONS_REPORT.md` — nothing here is invented
independently of an explicit product-owner decision.

---

## 1. Formally descoped — removed from committed product scope

These three capabilities, as originally described in PRD/FRD/SRD v6.0, are **no longer committed product
scope**, effective this version. They are not "not yet built" — they are decided against, for the
reasons stated, until a future version of this document says otherwise.

### 1.1 API Testing — dedicated rich authoring UI

**v6.0 described:** a dedicated API Testing module with request-building, collections, environments,
and request history (mapped to ACC-012 in the Acceptance Matrix).

**v6.1 correction:** descoped. Real API testing is a committed, shipped capability — it runs through the
Execution Hub's executor picker (`newman`/`pytest`/`schemathesis`), verified end-to-end and live-GO'd
this week. What is descoped is a *separate, dedicated authoring UI* on top of that — a Postman-like
experience. **Why:** the real capability already exists; a bespoke authoring UI is a large net-new
investment for a UI-polish gain, not a capability gain, when a cheaper path (importing real Postman/
OpenAPI artifacts) was never explored and the fixed executor set already covers the real, tested need.

### 1.2 Custom Testing — generic execution-DSL/plugin reading only

**Correction to this section, made 22 August 2026, same day as the original v6.1 publication:** the
descope below was written against the broad "Custom Testing" characterization used informally by the
Frontend Capability Audit and other narrative audits (a generic, user-authored test-suite/step-authoring
framework). It was never checked against `AIQA_ACCEPTANCE_MATRIX_v1.0.md`'s own **ACC-015** Acceptance
Criteria before being written, which describe something narrower and different:

> User can select: project, module, feature, focused testing objective. Generated tests must remain
> scoped to the selected objective.

That narrower reading — scoping test generation to a selected project/module/feature/objective — **was
never put to the product owner and is not descoped by this document.** It remains real, undecided,
committed-scope-pending-investigation, exactly like every item in §2 below. Only the broader reading is
addressed by this correction.

**v6.0 described (broad reading only):** a Custom Testing module for fully user-authored test suites/
steps beyond the fixed executor set, informally associated with ACC-015.

**v6.1 correction (broad reading only):** descoped. **Why:** a generic execution DSL or plugin system
raises real, new sandboxing-safety questions on top of the process-containment work just shipped (see
`AIQA_SECURITY_REMEDIATION_FOUR_DEFERRED_FINDINGS_REPORT.md`) — user-authored arbitrary logic is a
materially different threat model than five specific, well-understood tool adapters. This project's
actual trajectory has been building specific, well-integrated adapters, never a generic authoring
framework.

**ACC-015 itself (narrow reading, project/module/feature/objective scoping):** status unchanged by this
document — not investigated, not decided, not descoped. Whether the existing Test Case Generator already
substantially satisfies this narrower criteria (it already takes a `focus` input) is an open question for
a future session, not answered here.

**Follow-up resolution (22 August 2026, a later session the same day):** the open question above has now
been investigated directly, not deferred again. Answer: **no** — the Test Case Generator's "Requirement
Focus & Generation Notes" field (`GenerationConfigSection.tsx`, state `requirementNotes`) does not
substantially satisfy ACC-015's narrower reading. It is a free-text notes field copied verbatim into one
metadata string (`sourceRequirement`) identically across every generated case — it does not filter which
test categories are produced, does not change any case's real title/description/steps/priority/severity
(those are fixed per-category templates), has no accompanying "module" or "feature" selector anywhere on
the page, and — the largest gap — is never sent to the real, tested AI-generation backend at all: no
request to `POST .../test-cases/generate-ai` exists anywhere in this frontend today (confirmed by direct
search, not assumed), so even the backend's own real, provable prompt-scoping mechanism (skill selection,
proven in `ai_generation.rs`'s tests) is unreachable from this input. See
`AIQA_ACCEPTANCE_MATRIX_v1.0.md` §5A's ACC-015 row for the full evidence trail. **ACC-015 stays
`NOT_STARTED`** — this was a negative finding, not a decision to build; per the investigating unit's own
explicit instruction, no new scoping logic was written on the strength of this investigation.

### 1.3 Generic API-test dispatch entity

**v6.0 described:** a generic HTTP/GraphQL/gRPC/WebSocket dispatch entity, distinct from and beyond the
specific executor adapters (part of ACC-012/ACC-013's original scope).

**v6.1 correction:** descoped. **Why:** redundant with the real, working, tested `NewmanExecutor`/
`PytestExecutor`/`SchemathesisExecutor` adapters. A generic dispatch layer would duplicate capability
that already exists and already ships. (Note: this is narrower than §1.1 — surfacing the *existing*
Prometheus `/metrics` telemetry in a real UI is explicitly **not** descoped; see the Scope
Recommendations Report's build order.)

---

## 2. Formally deferred — real future scope, no committed timeline

These six capabilities remain **real, intended product scope** — nothing about them is rejected — but
none is committed work right now. Each has a named revisit condition rather than an open-ended "someday."

| Capability | ACC ref | Revisit condition |
|---|---|---|
| AIQA Orchestrator (AI-assisted cross-agent routing/prioritization/coordination) | ACC-006 | No fixed trigger — revisit if product direction shifts toward autonomous routing; today's DAG-scheduler orchestrator is the committed model |
| Mobile Testing (real device/emulator dispatch) | ACC-014 | A real device-farm budget decision, separate from this document — matches the PRD's own existing "post-MVP" precedent for adjacent mobile tooling (Appium/Strix) |
| Feature Map / Flow (visual quality-graph/traceability UI) | ACC-003 (visual half) | Practically gated on Context Ingestion (below) — nothing real to visualize without it |
| RBAC Testing scenario-matrix (systematic role×permission×action evaluation tool) | ACC-011 | No fixed trigger — real RBAC *enforcement* remains fully committed and already shipped; only the dedicated evaluation tool is deferred |
| Context Ingestion (PRD/FRD/SRD/Figma/API/DB/repo upload processing) | ACC-002 | No fixed trigger — the single largest, most foundational remaining item; revisit as its own dedicated planning exercise, not folded into an incremental unit |
| Quality-graph queries + requirement traceability backend | ACC-003 (query half) | Gated on Context Ingestion — there is nothing real to query or trace until ingestion exists |

**On the dormant schema:** `migration 0002_quality_graph_and_intelligence.sql`'s `quality_nodes`/
`change_impacts`/`quality_relations` tables remain unused (previously flagged as dead-code risk R11 in
`AIQA_CONSOLIDATED_DOCUMENTATION_REVIEW.md`). Because Quality Graph is *deferred*, not descoped, these
tables are **not** being recommended for removal — they are the real schema this deferred work would use
once committed. If Quality Graph is ever formally descoped in a future version of this document, that
schema should be removed at the same time, not left dormant indefinitely.

---

## 3. Unchanged — remains committed, real, and shipped

Every other capability in PRD/FRD/SRD v6.0 not named in §1 or §2 is unaffected by this addendum,
including the four items decided **to build** this session (Test Case Validator page, API telemetry
surfacing, Report export, Model Gateway task-type expansion) — those were never descoped or deferred;
see `AIQA_SCOPE_RECOMMENDATIONS_REPORT.md` for their build order.

---

## 4. Version history

- **v6.0** — original consolidated PRD/FRD/SRD (PDF), scope as originally envisioned.
- **v6.1** (this document) — 3 capabilities formally descoped, 6 formally deferred with named revisit
  conditions, per direct product-owner decisions recorded 22 August 2026.
