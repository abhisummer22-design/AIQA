# AIQA — Documentation Compliance Gap Analysis

**Date:** 2026-08-18
**Trigger:** Three authoritative product documents were added to the repository root after FE-4 through FE-7 were already implemented, tested, and pushed:

- `AIQA_Consolidated_PRD_FRD_SRD_v6.0_Hybrid_Multi_Agent (1).pdf`
- `AIQA_Business_Logic_Architecture_v2.0_Hybrid_Multi_Agent.pdf`
- `AIQA_Complete_End_to_End_Flow_Diagram_v6.0.pdf`

Every prior FE-4–FE-7 report stated, correctly at the time: *"FRD/SRD/PRD/flow-diagram source files were not present in the repository."* That is no longer true. This document is the promised follow-up each of those reports committed to: **flag contradictions clearly rather than silently resolve them**, once real documentation became available.

**Scope of this document:** analysis only. No code was changed to produce this document. It exists so a future consolidation unit has a single, accurate reference instead of re-deriving this comparison from scratch.

---

## 1. Status

**Documents are authoritative and correct.** The gaps below are between the **already-shipped FE-4–FE-7 implementation** (built before these documents existed, from backend-contract inspection and this engagement's own explicit unit prompts) and what the documents now specify. Nothing here means the PRD/FRD/SRD is wrong — it means several implementation decisions made in good faith, in the absence of documentation, now need reconciling against it.

## 2. What already matches — no action needed

| Area | Document requirement | Implementation | Verdict |
|---|---|---|---|
| Transport | — | FE-1's `aiqaApi.ts`, single `Authorization: Bearer` mechanism, `ApiError` classification | ✅ No conflict |
| RBAC enforcement | SRD §16: "Server-side RBAC remains mandatory"; Business Logic §7: "authorization is enforced server-side" | FE-2/FE-3: "frontend checks are UX only; backend remains authoritative" (`AuthContext.tsx`'s own doc comment) | ✅ Matches exactly |
| Project isolation | Business Logic §15: "Project isolation for every artifact/runtime object"; Flow diagram: "one selected project controls scope across every module" | FE-5's `ProjectContext` — every project-scoped request keyed off `selectedProjectId`, no cross-project leakage | ✅ Matches exactly |
| Core module set | PRD §2 module table: Dashboard, Test Case Generator, AIQA Orchestrator, Test Case Validator, Test Execution Hub, RBAC/API/Mobile/Custom, Reports, Settings/AI Model | FE-4's original 12 routes (`dashboard`, `generator`, `orchestrator`, `validator`, `execution`, `rbac`, `api-testing`, `mobile-testing`, `custom`, `feature-map`, `reports`, `settings`) | ✅ One-to-one match |
| Evidence metadata/content split | SRD §17: "Evidence metadata is queryable without loading large binaries" | FE-7: `EvidenceRow` (metadata) vs `.content` (bytes) are separate endpoints/fetches, list never eagerly loads bytes | ✅ Matches exactly |
| Never-fabricate-on-model-failure | SRD §14/Business Logic §12: "Model/provider failure results in controlled fallback or review state; never fabricated output" | FE-1–FE-7's own repeated "no fake data, ever" rule, applied identically | ✅ Same principle, independently arrived at |

## 3. Flagged contradictions

### 3.1 Audit and Evidence are not documented as standalone top-level pages

**What the documents say:** Across the PRD's module table (§2), the Business Logic module table (§2), and all six pages of the flow diagram, there is no "Audit" or "Evidence Viewer" entry in the primary navigation/module set.

- "Audit" appears only as: (a) a Reports export type — *"Reports: Bug Report, Test Report and **Final Audit Report**; export full project or selected module in PDF/Excel"* (PRD §2, Business Logic §11) — and (b) a general property of the system — *"Approvals, policy decisions, agent actions and side effects are auditable"* (SRD §16), *"Record routing/decision metadata sufficient for audit"* (FRD-AGT-07).
- "Evidence" appears only as a cross-cutting capability attached to other entities — *"Link evidence to run/task/test/finding/report"* (FRD-RT-04), *"preserves evidence"* (PRD §1), *"Capture screenshots, traces, logs, request/response and timing evidence"* (Business Logic §10) — never as its own page a user navigates to independently.

**What was built:** FE-6 built a standalone `/audit` page with its own Sidebar entry ("Audit Trail"); FE-7 built a standalone `/evidence` page with its own Sidebar entry ("Evidence Viewer").

**Why the divergence happened:** Both units were explicitly commissioned by name in this engagement's own sequential prompts (*"FE-6 — Audit Page / Audit Trail"*, *"FE-7 — Evidence Viewer"*), issued before these product documents existed. FE-4's own `routeConfig.tsx` had already flagged `/audit` and `/evidence` as unimplemented illustrative routes; FE-6/FE-7 then implemented them for real once real backend endpoints were confirmed to exist — a reasonable reading of the instructions available at the time.

**What is NOT in question:** the underlying data and functionality are real and correct — the audit log genuinely reflects `services/orchestrator`'s own audit table, and evidence genuinely reflects real run/task artifacts. This is a **navigation placement** question, not a correctness or fabrication question.

**Recommendation:** do not move this now. Reports and Test Execution Hub are still 100% Google-Studio-generated pages with no real data wiring — moving Audit/Evidence into them today would mean rebuilding that integration a second time once Reports/Execution Hub themselves get a real-data FE unit. Fold this into whichever future unit gives Reports and Execution Hub their real implementation, informed by this document, so it happens once.

### 3.2 `NeedsReview` is now a documented state — prior reasoning is outdated

**What the documents say:** the Common Execution Contract (SRD §13) explicitly lists `NEEDS_REVIEW` as one of the normalized outcome states alongside `PASSED`/`FAILED`/`BLOCKED`/etc. FRD-FI-02: *"Uncertain AI findings remain anomaly/needs-review, not confirmed defects."* Business Logic §10 repeats the same normalized outcome list.

**What was built/said:** both the FE-6 (`AuditPage`) and FE-7 (`EvidencePage`) implementation reports state, verbatim: *"no FRD/SRD/PRD and no existing design-system component defines this state's semantics... this is the correct, disclosed choice, not an oversight."* That reasoning was accurate given the information available at the time, but the premise ("no documentation defines it") is now false.

**What is NOT affected:** neither Audit nor Evidence's real backend endpoints (`audit_log`, `evidence` tables) currently emit a `needs_review`-shaped value — `AuditRow.outcome` is a free-text string (`"ok"`/other), and `EvidenceRow` has no status field at all. So there is nothing to wire today; this is a note for whichever future unit adds a `NeedsReview` resource state to the shared `StateViews` vocabulary (most naturally alongside real Finding/RCA work, where `NEEDS_REVIEW` actually originates per FRD-FI-02).

**Recommendation:** no code change now (nothing in the current contract carries this status). Update the reasoning going forward: `NeedsReview` is a documented, real product state — its absence from FE-6/FE-7 is because the specific endpoints they used don't emit it, not because it's undefined.

### 3.3 Documented entry flow starts with an explicit "Project Create/Select" step

**What the documents say:** the flow diagram's very first row (page 1) is literally `PROJECT CREATE/SELECT → PROJECT CONTEXT → DASHBOARD → ...`. Project creation is a named, first-class step before Dashboard, not folded into a persistent header control.

**What was built:** FE-5 implemented `ProjectContext`/`ProjectSelector` as an always-visible **header** control (select from already-existing real projects only). There is no dedicated "Create Project" screen or action anywhere in `forentend` — a real `aiqaApi.projects.create()` function exists (from FE-1) but nothing in the UI calls it.

**Why the divergence happened:** FE-5's brief scoped it as "global project scope," not "project management"; building a creation UI was reasonably read as out of scope for that unit, and no documentation existed yet to say otherwise.

**Recommendation:** this is a gap, not a wrong turn — the header-selector pattern for *switching* between existing projects is a defensible, still-valid interpretation and doesn't need to be undone. What's missing is a project-creation entry point, which was never assigned to any FE unit so far. Flag as scope for a future unit (or an explicit addition to FE-5's own follow-up) rather than retrofitting silently.

### 3.4 Dashboard's own project selector is documented, but is entirely fake

**What the documents say:** PRD §2 lists "project selector" as part of Dashboard's own purpose.

**What was built:** Dashboard already has a project selector — but it is FE-4/pre-existing `AIQAContext` fake data (`prj-aiqa`, `prj-fintech`, etc.), deliberately left untouched by FE-5 per that sprint's own "do not redesign Dashboard" constraint. The *real* project selector lives only in the Header (FE-5).

**Recommendation:** no action now — this is the same, already-documented, deliberate FE-5 decision (see `DashboardPage.tsx`'s own FE-5 comment: real per-project KPI wiring is explicitly deferred to a later unit, and rewiring the fake selector without real KPI data behind it would be more misleading, not less). Noted here only so a future "make Dashboard real" unit has this cross-reference.

## 4. Non-findings worth recording

- The documents' Mobile, RBAC, Security, and Custom Testing sections are now far more concrete (real roles/users fetch, positive/negative RBAC matrices, `p95`/`p99` API latency, resource/device locks) than anything implemented so far — but none of FE-1–FE-7 touched those pages, so there is no contradiction, only a much clearer spec waiting for whichever future unit builds them for real.
- `EvidenceRow`'s real backend shape has no `test_id`/`finding_id` field, so FRD-RT-04's full `run/task/test/finding/report` linkage is only partially realized today (`run` + `task` only) — this is a **backend contract gap**, not something the frontend can fix; noted for whichever unit next touches `services/orchestrator`'s evidence schema.

## 5. Suggested next step

Do not restructure Audit/Evidence navigation or add a project-creation flow reactively. Carry this document forward as input to whichever future unit(s) give Reports, Test Execution Hub, and project management their real implementations — resolve §3.1, §3.3, and §3.4 together, once, at that point, rather than moving already-tested code twice.
