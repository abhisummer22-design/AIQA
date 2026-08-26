# AIQA — Canonical ID Registry

**Date:** 22 August 2026
**Purpose:** this corpus grew several independent, similarly-shaped numbering sequences over eight days
of dense work, and at least two of them collide visually (`B12` vs `BE-12`) while meaning completely
unrelated things. This registry is the single place that says, for any short ID appearing anywhere in
this corpus, which sequence it belongs to and what it actually means — so nobody has to re-derive it from
context again. Companion to `AIQA_DOCUMENT_INDEX.md`.

---

## The core disambiguation: five unrelated sequences, two of which look nearly identical

| Prefix | What it actually is | Owning document | Purpose |
|---|---|---|---|
| `C#`, `H#`, `M#`, `L#` | Severity-coded **findings** (Critical/High/Medium/Low) from one specific audit | `AIQA_BACKEND_AUDIT_REPORT.md` | Defect tracking for that one audit only |
| **`B#`** (`B1`…`B15`) | The Backend Implementation Plan's own **defect/task** numbering — also reused as literal Rust test-name prefixes (`b1_`, `b5_`, `b14_`…) for traceability back to this list | `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md` | A fix/task checklist, not a feature-shipping unit |
| **`BE-#`** (`BE-8`…`BE-15`, `BE-11`, `BE-12B`) | Backend **implementation unit** numbers — one per feature-building session | `AIQA_PHASE2_BE{8,9,10,12,13,14,15}_*`, `BE-11_*`, `BE-12B_*` | A shipped, tested unit of backend work |
| `FE-#` (`FE-1`…`FE-26`) | Frontend **implementation unit** numbers | `AIQA_PHASE4_FE{1..21}_*`, `AIQA_FE22_*`, `FE-23_*`…`FE-26_*` | A shipped, tested unit of frontend work |
| `ACC-###` / `E2E-###` | Acceptance Matrix criteria / end-to-end scenario IDs | `AIQA_ACCEPTANCE_MATRIX_v1.0.md` | What "done" means for one capability |
| `GAP-##` / `DRIFT-###` | FE-13's and FE-15's own internal finding numbering | `AIQA_PHASE4_FE13_*`, `AIQA_PHASE4_FE15_*` | Documentation/contract drift tracking, local to those two reports |
| `D1`…`D5` | Independent-verification defect numbers | `AIQA_D{1..5}_FIX_REPORT.md`, `AIQA_PHASE1_FINAL_INDEPENDENT_VERIFICATION_REPORT.md` | Defects a reversion-tested independent audit found in Phase 1 |

**The one genuinely dangerous collision: `B#` vs `BE-#`.** They read almost identically, come from
different documents, and can name completely unrelated things at the same number:

> **`B12`** = *"`CorsLayer::new().allow_origin(Any)`"* (a CORS finding, `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md`) — closed 22 August 2026, see `AIQA_SECURITY_REMEDIATION_FOUR_DEFERRED_FINDINGS_REPORT.md`.
> **`BE-12`** = *"Dashboard/KPI JSON endpoints"* (`AIQA_PHASE2_BE12_DASHBOARD_KPI_IMPLEMENTATION_REPORT.md`) — a real, shipped, unrelated backend unit.

They share a number by coincidence, not by relationship. Every other `B#`/`BE-#` pair in this corpus is
the same kind of coincidence — always check the prefix (`B` alone vs `BE-`) before assuming two documents
are about the same thing.

---

## The two `BE-10` reports

Two files both call themselves "BE-10," and — unusually for this corpus — the later one already flags
this itself, in its own opening line:

| File | Date | What it actually is |
|---|---|---|
| `AIQA_PHASE2_BE10_IMPLEMENTATION_REPORT.md` | 17 Aug 2026 | The *first* "BE-10": **"Remaining Core Backend Contract Work"** — a broad, catch-all completion pass over whatever core backend-contract items were still open at that point in Phase 2. |
| `AIQA_BE10_IMPLEMENTATION_REPORT.md` | (undated; titled "Phase 4 — BE-10") | The *second*, unrelated "BE-10": exposes Test Intelligence via a real project-scoped route (`GET /api/v1/projects/:project_id/intelligence/tests`), triggered by an FE-21 finding, feeding FE-22. Its own opening line: *"This 'BE-10' is a distinct unit from the pre-existing Phase 2 'BE-10'… the naming collision is coincidental across two different numbering sequences."* |

**Rule going forward:** when citing "BE-10," name the file, not just the number — `AIQA_PHASE2_BE10_...`
or `AIQA_BE10_...` (the "Phase 4" one) — the number alone is ambiguous in this corpus specifically.

---

## The two `BE-9` reports (a one-step label drift, not a coincidence)

A second, related collision, one number lower than BE-10, and slightly different in kind — this one is a
**correction**, not a coincidence:

| File | What it actually is |
|---|---|
| `AIQA_PHASE2_BE9_IMPLEMENTATION_REPORT.md` | Converts every REST handler error to the contract's structured JSON envelope (§2.5). Still real, still valid — this work is untouched and still current. |
| `AIQA_PHASE2_ACTUAL_BE9_PROJECT_SCOPED_ROUTING_REPORT.md` | Implements `/api/v1` project-scoped routing for executions/evidence. Titled *"Actual"* BE-9 because, per the Backend Implementation Plan's own numbering table, project-scoped routing was the real "BE-9" — the structured-error-envelope work above was mislabeled BE-9 when it was really closer to BE-10-adjacent scope. |

**Both reports' content is real and current** — this is not a case of one superseding the other's
*work*, only a numbering label getting corrected one unit later. Cite by filename, same rule as BE-10.

---

## FE-13 and FE-15: the two units that redefine what "FE-#" means

Every other `FE-#` report in this corpus is a **feature-shipping** unit — it builds and ships real,
tested frontend capability (FE-8 = Execution Hub, FE-24 = notification bell, etc.). FE-13 and FE-15 break
that pattern on purpose, and a reader skimming only the numbering could easily misread them as "just
another feature unit":

- **FE-13** (`AIQA_PHASE4_FE13_DOCUMENTATION_CONTRACT_DRIFT_DEAD_CODE_CLEANUP_IMPLEMENTATION_REPORT.md`)
  is an **audit and cleanup** unit — it inventories documentation/contract drift (its own `DRIFT-###`
  numbering), dead code, and produces §8's Acceptance Matrix status re-assessment. It ships no new
  product-facing feature.
- **FE-15** (`AIQA_PHASE4_FE15_PRODUCT_CONTRACT_DECISION_RESOLUTION_AUDIT_REPORT.md`) is a **decision
  audit** unit — it exists to identify open product/contract decisions (its own `GAP-##` numbering) and
  recommend resolutions, explicitly stating it cannot itself sign off on any of them (§9.7: *"This audit
  is not any of those roles — it can only recommend, not sign off"*). It ships no new product-facing
  feature either.

Both are real, both are rigorous, and both are load-bearing for later work (this session's
`AIQA_PRODUCT_DECISIONS_RECORD.md` directly resolves six decisions FE-15 first raised, and
`AIQA_ACCEPTANCE_MATRIX_v1.0.md`'s reconciliation is built directly on FE-13 §8 and FE-15 §9.2). They are
just not the same *kind* of unit as their neighbors in the sequence.

---

## Quick lookup — every `BE-#`/`FE-#` unit and what it shipped

| ID | Ships | File |
|---|---|---|
| BE-8 | Project CRUD contract alignment | `AIQA_PHASE2_BE8_IMPLEMENTATION_REPORT.md` |
| BE-9 | Structured error envelope, RBAC/auth error shapes | `AIQA_PHASE2_BE9_IMPLEMENTATION_REPORT.md` |
| BE-10 *(Phase 2)* | Remaining core backend contract work | `AIQA_PHASE2_BE10_IMPLEMENTATION_REPORT.md` |
| BE-10 *(Phase 4)* | Project-scoped Test Intelligence route | `AIQA_BE10_IMPLEMENTATION_REPORT.md` |
| BE-11 | Dashboard KPI capability audit | `BE-11_DASHBOARD_KPI_CAPABILITY_AUDIT.md` |
| BE-12 | Dashboard/KPI JSON endpoints | `AIQA_PHASE2_BE12_DASHBOARD_KPI_IMPLEMENTATION_REPORT.md` |
| BE-12B | Average Run Duration KPI (backend) | `BE-12B_AVERAGE_RUN_DURATION_IMPLEMENTATION_REPORT.md` |
| BE-13 | Cancel + execution alignment | `AIQA_PHASE2_BE13_EXECUTION_ALIGNMENT_IMPLEMENTATION_REPORT.md` |
| BE-14 | Realtime event stream | `AIQA_PHASE2_BE14_REALTIME_EVENT_STREAM_IMPLEMENTATION_REPORT.md` |
| BE-15 | Evidence/findings/audit APIs | `AIQA_PHASE2_BE15_EVIDENCE_FINDINGS_AUDIT_IMPLEMENTATION_REPORT.md` |
| FE-1…FE-21 | Sequential frontend feature units (integration layer through DAG editor) | `AIQA_PHASE4_FE{1..21}_*_IMPLEMENTATION_REPORT.md` |
| FE-22 | Consumes BE-10 (Phase 4)'s Test Intelligence route | `AIQA_FE22_IMPLEMENTATION_REPORT.md` |
| FE-23 | Frontend capability audit | `FE-23_FRONTEND_CAPABILITY_AUDIT.md` |
| FE-24 | Real recent-activity notification bell | `FE-24_IMPLEMENTATION_REPORT.md` |
| FE-25 | Test Intelligence auto-refresh on real `RUN_COMPLETED` events | `FE-25_IMPLEMENTATION_REPORT.md` |
| FE-26 | Average Run Duration KPI (frontend) | `FE-26_AVERAGE_RUN_DURATION_KPI_IMPLEMENTATION_REPORT.md` |

**Naming-convention drift, for the record (not a defect to fix, just a fact worth knowing):** the prefix
pattern itself changed three times across the corpus's eight days — `AIQA_PHASE4_FE1_...` (phase-prefixed,
no hyphen) → `AIQA_FE22_...` (no phase prefix, no hyphen) → `FE-23_...` (no `AIQA_` prefix, hyphenated).
The unit numbers themselves are still one continuous sequence throughout (1 through 26) — only the
filename convention drifted, not the numbering.

---

*See `AIQA_DOCUMENT_INDEX.md` for the status (current/superseded/historical) of every document named
above.*
