# AIQA — Phase 4 — FE-21 Implementation Report

## 1. Status

**PASS (audit-only).** No implementation was performed. This unit's own evidence rules (§10 of the prompt) explicitly make an audit-only PASS valid, and the fresh audit below concludes no candidate satisfies FE-21's frontend-only, no-backend-modification boundary — one strong candidate exists, but implementing it correctly requires a backend change, which this unit is not permitted to make.

## 2. Objective

Perform a fresh reconciliation of documented product requirements, real backend capabilities, existing frontend contracts, and actual runtime exposure — across every page and every `aiqaApi.ts` function — to determine whether a genuine, currently-hidden, product-required frontend capability remains, without reusing FE-19/FE-20's conclusions as proof.

## 3. Fresh Capability Inventory

| Area | Fresh finding | Category | Action |
|---|---|---|---|
| **`/intelligence/tests` → `TestIntelRow`** (risk/value/flakiness scoring per test) | Real, live-computed data (`intelligence.rs::ingest_run`, called from `engine.rs` after every run completes) — genuinely populated, not dead code. Zero frontend usage anywhere (`grep` confirmed). Only reachable via the legacy, non-project-scoped route; `store.rs::list_test_intelligence` takes `project_filter: Option<&str>` internally but the route itself has no `:project_id` path param — it silently uses the calling key's own bound `ctx.project_id`, defaulting to the literal tenant `"default"` for any unscoped/multi-project key (the exact key pattern this whole engagement's manual verification has used throughout, including a real "AIQA Core Platform" + "Payments Gateway QA" multi-project admin key). Wiring this into the frontend's project-switcher UI as-is would silently show wrong/empty data whenever a multi-project key has a project other than "default" selected. | **D** (product-required per `intelligence.rs`'s own module doc: *"this is most of the PRD's actual differentiators"*; real backend capability exists; but safe, correct frontend exposure requires a new project-scoped route — a backend change this unit cannot make) | Document only (§9, §25, §28) |
| `/intelligence/failure-clusters` (legacy, unscoped, all clusters) | Superseded by the already-exposed, already-project-scoped `findings.analysis()` (`{finding, failure_cluster}`), which `ReportsPage.tsx` already calls per-finding. | E — redundant | None |
| `/bug-drafts`, `/bug-drafts/:id/review` (legacy) | Fully superseded by the scoped `/findings`/`/findings/:id/review` pair FE-19 already extended. | E — redundant | None |
| `/admin/api-keys*`, `/admin/audit-log` | Real, `Role::Admin`-gated key CRUD and global audit log. No PRD/FRD/SRD citation found for a UI-based key-management screen (fresh search, none found); `/admin/audit-log` is superseded by the already-exposed scoped `/audit`. Implementing key management would also be a materially new admin panel (list+create+revoke UI), not a small exposure. | C (api-keys) / E (audit-log) | None |
| `/metrics` | Prometheus text-format scrape endpoint (`Content-Type: text/plain; version=0.0.4`), not JSON — structurally not a page-renderable capability. | C — ops-only, no product UI requirement | None |
| **Projects** (`list`/`get`/`create`/`update`) | Freshly re-read `Project`/`CreateProjectRequest`/`UpdateProjectRequest`: `id`, `name`, `description`, `created_at`, `updated_at` — all already consumed by `list`/`get`/`create` call sites. `update` re-checked against the PRD/FRD/SRD/Business-Logic/Flow documents (see §4) — no "edit project" requirement citation found, same conclusion as FE-19/FE-20, independently re-derived. | B (list/get/create) / C (update) | None |
| **Executions** — `RunPlan`/`TaskDef` | Freshly re-read: `TaskDef.prerequisites` (FE-17), `.simulate` (pre-existing, exposed in create form's outcome picker), `.executor`/`.params` (pre-existing, exposed), `RunPlan.concurrency` (grepped: `exec-concurrency` input at `ExecutionHubPage.tsx:969` — already exposed). No unexposed `TaskDef`/`RunPlan` field found. | B | None |
| **Executions** — `RunSummary`/`RunDetail` | Freshly re-read all 19 fields: 11 counts (FE-20, re-confirmed rendered via `#run-detail-count-grid`), `requested_by`/`policy_notes` (grepped: both rendered, lines 708/739), `approved_by`/`approved_at` (FE-18, re-confirmed rendered via `#run-approval-attribution`), `started_at`/`finished_at`/`state`/`name`/`run_id` (all rendered). No gap. | B | None |
| **Evidence** (`EvidenceRow`) | Freshly re-read all 7 fields: `id`, `task_id` (grepped: rendered, line 534-536), `kind` (via `content_type`-derived icon), `object_key` (deliberately never rendered — file's own doc comment: "a storage-layer implementation detail... never rendered here", a documented security-conscious non-exposure, not a gap), `content_type`, `size_bytes`, `created_at` — all genuinely accounted for. `.get()` still deliberately unused (documented redundancy with `.list()`, re-confirmed). | B | None |
| **Findings** (`Finding`) | Freshly re-read all 10 fields: `id`, `failure_cluster_id`, `title`, `description`, `severity`, `confidence`, `review_state`, `created_at`, `promoted_at`/`promoted_by` (FE-19, re-confirmed rendered via `#finding-review-attribution-{id}`). No `run_id`/`task_id`/`occurrences` field exists directly on `Finding` — those live on the separately-fetched `FailureCluster` (`occurrence_count`, `first_seen_at`, `last_seen_at`), already rendered in the analysis panel. No gap. | B | None |
| **Audit** (`AuditRow`) | Freshly re-read all 8 fields — all rendered, re-confirmed unchanged since FE-19/FE-20. | B | None |
| **Dashboard** | Freshly re-read `DashboardPage.tsx`'s own doc comment: 9 of 11 documented KPI concepts remain genuinely backend-missing, unchanged since FE-19/FE-20's own fresh checks — no new backend field has appeared to close any of them. | D | None (do not redesign, do not invent) |
| **Test Generator** | Freshly re-confirmed the `GeneratedTestCase → TaskDef` boundary is untouched and still deliberately unmapped (`TestCaseGeneratorPage.tsx` unchanged since FE-16). | B (truthfully local-only, boundary preserved) | None |
| **Settings** | Freshly re-read `SettingsPage.tsx` in full: the Authentication tab's real `OrchestratorAuthPanel` (real key entry/verification) is the only real backend-backed surface. Every other tab/field (General, SSO toggle, "Control Plane Daemon API Token", Execution runner pools, Environments, Policies, AI Models, Notifications) is pre-existing, explicitly-preserved fabricated content per this file's own doc comment ("do not remove existing fake data" — a standing instruction from an earlier sprint, not this unit's to override). | Fabricated-by-design, explicitly out of scope | None |

## 4. Requirement Evidence

- `services/orchestrator/src/intelligence.rs` (module doc, lines 1-18, quoted verbatim): *"Quality Graph / Risk / Failure / Flakiness intelligence — the prior session's build report listed this as 'None built — this is most of the PRD's actual differentiators; pure AI/data-science work, zero started.'"* — direct, specific evidence tying `list_test_intelligence`'s risk/value/flakiness scores to the PRD's own differentiating feature set.
- Cross-checked against `forentend/src/components/pages/TestCaseValidatorPage.tsx`'s own doc comment (FE-11 era), which currently states: *"there is no requirement-traceability entity, no test-case quality-scoring model, no duplicate/semantic-similarity detection... anywhere in the backend."* This is now **materially stale** — a real, populated, per-test risk/value/flakiness scoring capability (`TestIntelRow`) does exist. This is not a claim that FE-11's audit was wrong at the time it was written; `intelligence.rs`'s own doc comment frames itself as later work ("a direct answer to the critique finding") that plausibly landed after that page's disclosure text was authored. Flagged here as a genuine, evidenced finding — not corrected in code this unit, since Category D findings are documented, not silently patched (§25), and rewriting another page's disclosure text without exposing real data would not itself close the gap FE-21 is scoped to look for.
- No requirement evidence was found for `projects.update`, `/admin/api-keys` UI, or any Dashboard KPI beyond the 2 already computed — consistent with, and independently re-derived from, FE-19/FE-20's own prior (also independently-derived) conclusions.

## 5. Backend Evidence

For the one substantive Category-D finding (test intelligence):
- **Domain/persistence**: `store.rs::list_test_intelligence` (lines 978-1012) — real SQL join across `quality_nodes`, `risk_assessments`, `test_value_assessments`, `flakiness_metrics`, filtered by `node_type = 'test'` and a tenant key.
- **Computation**: `intelligence.rs` — a real, deterministic, rule-based (not LLM) classifier that runs after every run completes (`engine.rs` calls `ingest_run`) — live production code, not inert scaffolding.
- **Route**: `GET /intelligence/tests`, mounted in the explicitly-documented "Legacy, flat, unscoped routes" group (`api.rs` lines 74-91) — the same comment block explicitly says these routes were deliberately excluded from "Actual BE-9"'s project-scoped routing overhaul, with the full reasoning pointed at `AIQA_PHASE2_ACTUAL_BE9_PROJECT_SCOPED_ROUTING_REPORT.md §2` (a backend report, consistent with this being a backend architectural decision, not a frontend oversight).
- **Authorization/scope**: any authenticated role can call it; scoping is implicit via `ctx.project_id` (the calling key's own bound project), not an explicit `:project_id` path parameter — architecturally different from every other route this frontend uses, and incompatible with this frontend's multi-project-per-key switcher model without a backend change.

## 6. API Contract Verification

No frontend contract exists for `TestIntelRow`/`FailureClusterRow` (legacy) — `forentend/src/types/contract.ts` and `forentend/src/services/aiqaApi.ts` were both re-read in full this session and contain zero references to `/intelligence/*`, `/bug-drafts`, `/admin/*`, or `/metrics`. This is consistent with the frontend having been built entirely against the `/api/v1/projects/...` scoped surface — confirming the gap is real (no dead/unused contract code masking it) and confirming no accidental partial wiring exists.

## 7. Candidate Analysis

Every candidate investigated this session is listed in §3. Only one (test intelligence) has both real product-requirement evidence and a real, live backend capability. Every other investigated item is either already fully exposed (Category B), has no requirement evidence (Category C), is redundant with an already-exposed path (Category E), or is a previously-documented backend-missing gap unchanged since FE-19/FE-20 (Category D, Dashboard KPIs).

## 8. Selected Candidate

**No valid Category-A candidate remains after this fresh audit.** The one candidate with genuine requirement + backend evidence (test intelligence) fails the Category-A bar specifically on *"implementation requires no backend modification"* — the existing route's scoping mechanism (key-bound `ctx.project_id`, no `:project_id` path param) is structurally incompatible with this frontend's multi-project switcher, and correcting that requires a new backend route. Per this unit's explicit, non-negotiable "no backend modification" rule, it is classified Category D, not A, and is not implemented.

## 9. Candidates Not Selected

- **Test intelligence exposure** — closest to viable, documented in full above; not selected because safe implementation requires a backend change (see §8, §25, §28).
- **`/admin/api-keys` UI** — no requirement evidence found; would also be a materially larger surface than a "smallest truthful exposure" (full CRUD admin panel), not attempted.
- **`projects.update` ("Edit Project")** — no requirement evidence found (third independent fresh check across FE-19/FE-20/FE-21, same conclusion each time).
- **Dashboard KPIs** — backend-missing, explicitly out of scope for a frontend-only unit (Category D, unchanged).

## 10. Implementation

No implementation performed because none was justified. Every investigated Category-A-shaped candidate either turned out to already be fully exposed (Category B) or required a change this unit is not permitted to make (Category D).

## 11. Real Data Verification

Not applicable — no code changed. No live or automated verification was performed or claimed.

## 12. Error / Empty / Auth Handling

Not applicable — no code changed. Existing behavior for loading/error/empty/401/403/offline states across every page was not touched and is not claimed to have been re-verified beyond the passing regression suite (§16).

## 13. Project Scope

Not applicable — no code changed. Project-scoping was specifically the deciding factor in rejecting the one substantive candidate (§8) rather than something this unit altered.

## 14. Realtime

Not applicable — no code changed, no new realtime mechanism considered or needed.

## 15. Fabrication Search

```
grep -Rni "useAIQA" forentend/src            → 12 matches
grep -Rni "AIQAContext" forentend/src        → 36 matches
grep -RniE "fake|mock|demo|sample|fabricated|placeholder|hardcoded" forentend/src → 232 matches
grep -RniE "Math\.random|setTimeout" forentend/src → 31 matches
grep -Rni "intelligence/tests|intelligence/failure-clusters|TestIntelRow|list_test_intelligence" forentend/src → 0 matches
grep -Rni "admin/api-keys|admin/audit-log" forentend/src → 0 matches
```
Classification: the `useAIQA`/`AIQAContext` matches are pre-existing legitimate context usage plus FE-11/FE-16 removal doc-comments, unchanged and untouched this session. The 232 `fake|mock|demo|...` matches are overwhelmingly `SettingsPage.tsx`'s explicitly-preserved fabricated tabs (a standing, documented exception — §3) plus the same accumulated doc-comments from FE-7/FE-11/FE-16/FE-18/FE-19/FE-20 documenting prior real removals, none of them runtime fabrication introduced by this or any recent unit. The 31 `Math.random|setTimeout` matches are pre-existing legitimate debounce/resync timers and the already-scoped FE-16 local-generation pipeline. The two capability-specific searches (`intelligence/*`, `admin/*`) returned zero matches, positively confirming the Category-D finding in §3/§9 is a genuine, currently-unexposed gap rather than a partially-wired feature.

## 16. Tests

No implementation occurred, so no new tests were added (correctly, per §12 of the prompt: *"do not add meaningless tests"*). Existing full regression suite was run to confirm the baseline is unchanged:
```
npx vitest run
```
Result: **299 tests passed, 15 test files** — identical to the count at the end of FE-20 (294 at end of FE-19 → 299 at end of FE-20 → 299, unchanged, at end of FE-21).

## 17. Manual Verification

None performed. No implementation occurred, so there is nothing to manually verify; no scenario is claimed as live-tested.

## 18. Typecheck

```
npx tsc --noEmit
```
Clean, no output, exit 0.

## 19. Build

```
npx vite build
```
Succeeded: `2711 modules transformed`, `dist/assets/index-H7xF6vUN.js` (1,020.67 kB, gzip 290.67 kB) — byte-identical output hash to FE-20's build, confirming zero frontend source changes occurred this unit.

## 20. Backend Boundary

```
git diff --stat -- services/orchestrator/
```
Empty, both before and after this unit's audit work. No backend file was read for the purpose of finding a workaround; the one candidate that would have required a backend change (§8) was documented, not implemented around.

## 21. FE-16 Through FE-20 Regression

- **FE-16** (Test Generator truthfulness): confirmed unchanged; `TestCaseGeneratorPage.tsx` not touched; its 9 tests pass.
- **FE-17** (dependency/DAG editor): confirmed unchanged; the create-panel dependency-chip UI not touched; its tests pass.
- **FE-18** (approval/rejection attribution): confirmed unchanged; `#run-approval-attribution` re-verified still rendered (§3); its tests pass.
- **FE-19** (finding review attribution): confirmed unchanged; `#finding-review-attribution-{id}` re-verified still rendered (§3); its tests pass.
- **FE-20** (full run-summary count grid): confirmed unchanged; `#run-detail-count-grid` re-verified still rendering all 11 fields (§3); its tests pass.
- **FE-12** (status vocabulary migration): not started, not touched. `formatStatusLabel` was read (not modified) purely to confirm it remains a display-only underscore-to-space transformation, unchanged since FE-20.
- Full suite (299/299, §16) confirms all of the above collectively, not just by inspection.

## 22. Files Changed

**None.**

## 23. Files Explicitly Not Changed

`services/orchestrator/**` (all files), every file under `forentend/src/components/pages/`, `forentend/src/types/contract.ts`, `forentend/src/services/aiqaApi.ts`, `forentend/src/context/**`, `forentend/src/routes/**`.

## 24. Remaining Limitations

The test-intelligence finding (§3, §9) is well-evidenced but cannot be closed within a frontend-only, no-backend-modification unit. `TestCaseValidatorPage.tsx`'s disclosure text is now slightly stale (claims "no test-case quality-scoring model" when one demonstrably exists, just unscoped and unexposed) — flagged here rather than corrected, since a text-only correction would not itself satisfy FE-21's "expose a capability" objective and risks appearing to close the gap without actually doing so.

## 25. Open Candidates

- **Category D — Test intelligence** (`/intelligence/tests` → `TestIntelRow`: risk/value/flakiness scoring per test): real, populated, PRD-cited backend capability; needs a new project-scoped route (e.g. `GET /api/v1/projects/:project_id/intelligence/tests`, mirroring the existing `findings`/`evidence` route pattern) before safe frontend exposure. Strongest candidate for a future unit that is explicitly authorized to make a small, justified backend change.
- **Category D — Dashboard KPIs** (9 of 11 documented concepts): unchanged since FE-19/FE-20, still genuinely backend-missing.
- **Category C — `projects.update`**: no requirement evidence across three independent fresh audits (FE-19, FE-20, FE-21); not recommended for future work absent a new requirement citation.
- **Category C — `/admin/api-keys` UI**: no requirement evidence found; would be a materially larger feature than typical FE-unit scope even if a requirement were found.
- **Category E — `/intelligence/failure-clusters`, `/bug-drafts*`, `/admin/audit-log`**: all redundant with already-exposed scoped paths; correctly left unduplicated.

## 26. Truthfulness Statement

No fabricated runtime behavior, endpoint, field, workflow, or execution state was introduced. No code was changed. **Expected: None. Actual: None.**

## 27. Evidence Chain Summary

Requirement (`intelligence.rs`'s own "PRD's actual differentiators" comment, the only candidate with genuine requirement evidence found this session) → Real backend capability (`list_test_intelligence`, live-computed via `ingest_run` after every run) → Real API contract exists, but only via a legacy, key-bound-scoped route with no `:project_id` path parameter → Concrete frontend gap (zero frontend usage, confirmed by search) → **Justified no-op**: correct exposure requires a new project-scoped backend route, which this unit's rules forbid adding → Verification: full regression suite (299/299), clean typecheck, clean build, byte-identical output, backend diff empty before and after.

## 28. Recommendation for Next Unit

**C — backend work is required before the next meaningful frontend-exposure unit.** The fresh evidence in §3-§9 does not support FE-12 (no new evidence changed the status-vocabulary case since FE-19/FE-20 either concluded it out of scope) and does not support inventing another frontend-only unit merely to keep the FE-2x sequence moving (explicitly forbidden by this unit's own Critical Rule #1). The evidence-based recommendation is a small, explicitly-scoped backend addition — a project-scoped route mirroring the existing `findings`/`evidence` pattern for the real, PRD-cited test-intelligence data (§25) — after which a follow-up frontend unit could truthfully expose it using the same "smallest truthful implementation" discipline used in FE-17 through FE-20.

## 29. Final Exit Criterion

| Criterion | Result |
|---|---|
| Fresh audit performed | Yes — independently re-derived, not copied from FE-19/FE-20 |
| Requirement verification performed | Yes (§4) |
| Backend verification performed | Yes (§5) |
| Candidate classification complete | Yes (§3, all areas A-H covered) |
| No invented API | Confirmed |
| No backend modification | Confirmed — `git diff --stat -- services/orchestrator/` empty before and after |
| No fake fallback | N/A — no implementation |
| No new WebSocket | Confirmed — none considered |
| Tests/regression status | 299/299 passed, 15 files — unchanged from FE-20 |
| Typecheck | Clean |
| Build | Succeeds, output unchanged |
| FE-16 through FE-20 preserved | Confirmed (§21) |
| **Overall** | **PASS (audit-only)** |
