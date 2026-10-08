# Phase 0 — Master Feature Audit

**Audit date:** 2026-10-08
**Plan audited:** `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md` (11 phases, 0–10)
**Repository state:** `main` @ `50488f2` ("feat: add QA intelligence memory"), working tree clean
**Phase type:** AUDIT ONLY — no application logic modified, no migration created, no feature code written

---

## 1. Executive summary

Fourteen proposed features were audited against the live repository. The result is
decisively not "greenfield".

| Status | Count | Meaning |
|---|---|---|
| **EXISTS** | 2 | implemented, with tests |
| **PARTIAL** | 5 | some of the capability is real; a required part is missing |
| **MISSING** | 7 | no implementation found after search |
| **NOT_CHECKABLE** | 0 | - |
| | **14** | **total features audited (§3.1–§3.14)** |

**Correction note (added during Phase 1).** This table previously read
`EXISTS 4 / PARTIAL 5 / MISSING 4 / DUPLICATE_RISK 1`. That was wrong twice over, and
it is recorded here rather than quietly fixed:

1. **The counts did not match the sections they summarised.** The per-feature
   classifications in §3.1–§3.14 are `EXISTS` = §3.1, §3.11 (2);
   `PARTIAL` = §3.2, §3.3, §3.10, §3.12, §3.13 (5);
   `MISSING` = §3.4, §3.5, §3.6, §3.7, §3.8, §3.9, §3.14 (7). Those sum to 14. The
   table's `4 / 5 / 4` was reconciled to 14 only by counting `DUPLICATE_RISK` as a
   fourteenth category, which double-counted AI Bug Intelligence — a feature that is
   both `EXISTS` (§3.1) and flagged `DUPLICATE_RISK` (§3.15). The sections were treated
   as authoritative; the table was corrected to match them. **No application logic
   changed for this correction.**
2. **`DUPLICATE_RISK` is an orthogonal flag, not a status.** It is a *risk annotation*
   carried by one of the 14 already-classified features (AI Bug Intelligence, §3.1 →
   §3.15), not a 15th bucket and not an additional feature. It is therefore reported
   separately and deliberately excluded from the 14-way partition, so that
   `EXISTS + PARTIAL + MISSING + NOT_CHECKABLE = 14` holds exactly.

`DUPLICATE_RISK`: **1** feature flagged (§3.1, "AI Bug Intelligence").

**Likely source of the original `EXISTS 4`.** §4 of this audit lists **16** pre-existing
*capabilities* (rows 386–400), most of them `EXISTS`, and they are a **different set**
from the 14 plan features in §3. `QA Intelligence Memory`, `Adaptive Intelligence` and
`Failure Intelligence` are `EXISTS` in §4 and are also plan features — but §3
classified QA Intelligence Memory under §3.11 `EXISTS`, Adaptive Intelligence is not one
of the 14 at all, and Failure Intelligence is not one of the 14 either. Reading an
`EXISTS` count off §4 instead of §3 is the most plausible way the wrong number got
there. The two tables are kept separate on purpose: §3 answers "what does the plan ask
for, and does it exist?", §4 answers "what is already built?".

**The single most important finding:** the plan's own recommended order
(`Phase 1 = AI Bug Intelligence`) is the **wrong starting point**, and repository
evidence contradicts it.

`AI Bug Intelligence` is one of the **best-built** capabilities AIQA already has:
`bug_drafts` is written by the real failure-recording path, AI hypothesis generation and
review endpoints exist, and `api_tests.rs` carries 313 tests. Building "AI Bug
Intelligence" as a new phase would substantially re-implement a working subsystem.

What is actually missing sits **underneath** it: there is no stable identity linking a
requirement to the test that validates it, to the execution, to the failure, and back to
a fix. `quality_relations` has **0 rows** in the live database. `change_impacts` has a
table but **zero writers in the entire codebase**. The graph, the trace and the bug
record all exist as *pieces*; they are not *joined*.

Three further findings shape the whole roadmap:

1. **Historical data cannot support any intelligence claim.** The live database holds 19
   telemetry rows, 0 clusters, 0 requirements, 0 bug drafts, 0 relations. Every
   capability in this audit is built but **unexercised on real data**.
2. **The CI workflow has no E2E or integration job.** Three jobs only: backend
   fmt/clippy/test, frontend lint/test/build, Gitleaks. This is a pre-existing gap and
   it will block phases that claim end-to-end validation.
3. **No capability exists for cross-layer correlation or a QA copilot.** Not partial,
   not a stub — **zero occurrences** in source, schema, or tests.

---

## 2. Current architecture relevant to these features

### 2.1 Measured baseline

| Measure | Value |
|---|---|
| Tables (live DB) | 40 (excluding `_sqlx_migrations`) |
| Migrations applied | 30 |
| HTTP routes in `api.rs` | 123 |
| Rust source files | 81 (`src/`) + 4 agent modules |
| Test modules (`*_tests.rs`) | 22 files, 401 test functions |
| Files with inline `mod tests` | 41 |
| CI jobs | 3 |

### 2.2 The chain that exists

```text
Project ─▶ Requirement (requirements, 0 rows)
             │
             └▶ TestCase (test_cases, 2 rows)
                    │
                    ├─▶ runs ─▶ tasks ─▶ execution_telemetry (19 rows)  ← THE MEMORY BACKBONE
                    │                  │
                    │                  └─▶ failure_clusters (0) ─▶ bug_drafts (0)
                    │
                    ├─▶ evidence (2 rows)
                    └─▶ quality_nodes (2) / quality_relations (0)   ← GRAPH IS EMPTY
```

### 2.3 The memory layer just completed

`qa_memory.rs` (1,426 lines) and `qa_memory_risk.rs` (853 lines) provide deterministic,
project-scoped retrieval over `execution_telemetry`, plus module/release risk on the
existing `calibrate_risk`. 93 tests. This is the foundation every later phase should
build on rather than re-create.

### 2.4 Auth / project scoping — the boundary every feature must respect

| Component | Location |
|---|---|
| `Role` enum | `auth.rs:32` |
| `RequireViewer` / `RequireOperator` / `RequireAdmin` | `auth.rs` |
| `require_project_access` | `auth.rs` |
| `require_scoped_project` / `require_resource_in_project` | `api.rs` |
| RBAC matrix tests | `rbac_matrix_tests.rs` — 15 tests |
| RBAC live probe (measures the server, not a declared matrix) | `rbac_probe.rs` — 621 lines, 17 tests |
| Key-scope tests | `key_scope_tests.rs` — 9 tests |

This is the strongest existing area and must not be duplicated.

---

## 3. Feature-by-feature audit

### 3.1 AI Bug Intelligence — **EXISTS**

| Aspect | Evidence |
|---|---|
| Modules | `intelligence.rs` (`record_failure`, `classify_failure`, `normalize_signature`), `api.rs` findings handlers, `operations.rs` (`FailureInvestigation`), `qa_memory.rs` (`bug_history`) |
| Tables | `bug_drafts` (id, failure_cluster_id, title, description, severity, confidence, review_state, created_at, promoted_at, promoted_by), `failure_clusters` |
| APIs | `GET /findings`, `GET /findings/:id`, `POST /findings/:id/analysis`, `POST /findings/:id/ai-hypothesis`, `POST /findings/:id/review`, `GET /intelligence/failures`, `…/investigation`, `…/timeline`, `…/environments` |
| Tests | `api_tests.rs` findings + review tests; `intelligence.rs` inline; `operations_tests.rs` 15; `qa_memory_tests.rs` defect test |
| Relationships | `bug_drafts.failure_cluster_id → failure_clusters.id`; cluster ↔ telemetry via `failure_fingerprint` (migration 0030) |

**Assessment:** complete, including AI hypothesis generation and a human review gate
that never auto-confirms. **Plan Phase 1 would duplicate this.**

---

### 3.2 Requirement → Test → Execution → Failure → Bug → Fix → Retest traceability — **PARTIAL**

| Aspect | Evidence |
|---|---|
| Modules | `agents/planning.rs`, `intelligence.rs`, `operations.rs`, `execution_plan.rs`, `agent_execution.rs`, `store.rs` |
| Tables | `requirement_documents`, `requirements`, `test_cases` (26 cols incl. `requirement_reference`, `source_requirement`, `executed_run_id`, `executed_task_id`), `test_plans`, `runs`, `tasks`, `execution_telemetry`, `evidence`, `failure_clusters`, `bug_drafts`, `healing_proposals` |
| APIs | `GET /requirements`, `POST /gap-check`, `POST /planning/test-plan`, `POST /test-cases/:id/compile`, `POST /test-cases/:id/handoff`, `GET /executions/:run_id/*`, `GET /evidence/:id`, `POST /healing/proposals/:id/execute` |
| Tests | 1,043 backend tests across api/engine/agent/planning/execution |
| Relationships | **`quality_relations` exists but holds 0 rows.** Three relation types are written by code: `validates` (case → requirement, `agent_execution.rs`, `intelligence.rs`), `fails_with` (test → cluster, `adaptive_intelligence.rs`), `covers` (case → module, `store.rs`) |

**What is missing:** the trace is assembled by *querying each table separately* at read
time, not by a walkable chain. `operations.rs:81` `affected_tests` is a
`Vec<String>` of **task names, not stable ids** — the same identity weakness the
previous audit found and partly fixed. `operations.rs:9` declares a traceability chain in
a doc comment, but the graph rows that would make it walkable are absent from live data.

---

### 3.3 Risk-Based Testing — **PARTIAL**

| Aspect | Evidence |
|---|---|
| Modules | `adaptive_intelligence.rs` (`calibrate_risk`, `RiskFactors`, `RiskCalibration`), `intelligence.rs` (`upsert_risk_assessment`, `upsert_test_value`), `qa_memory_risk.rs` (`module_risk`, `release_risk`, `FACTOR_WEIGHTS`) |
| Tables | `risk_assessments` (2 rows), `test_value_assessments` (2 rows) |
| APIs | `GET /intelligence/risk`, `GET /intelligence/tests`, `GET /intelligence/strategies` |
| Tests | `adaptive_intelligence_tests.rs` 38; `qa_memory_risk_tests.rs` 18 |
| Relationships | both keyed `node_id → quality_nodes` |

**Measured limitation, not speculation.** `calibrate_risk` weights six factors:
35% failure rate, 20% flakiness, 15% severity, 10% dependency, 10% healing regression,
10% security. The **last three have no column anywhere in the schema**, so **30% of the
model is structurally unmeasurable**. A full `RiskCalibration` is therefore never emitted;
`qa_memory_risk.rs:76` records this as `STRUCTURALLY_UNAVAILABLE_WEIGHT = 0.30` and
reports `measured_weight` instead.

**What is missing:** risk is *computed and displayed*, never *used to select or order
tests*. No module reads `risk_assessments` to decide execution order or scope.

---

### 3.4 Change Impact Analysis — **MISSING**

| Aspect | Evidence |
|---|---|
| Table | **`change_impacts` exists** (migration `0002_quality_graph_and_intelligence.sql`) |
| Writers | **`INSERT INTO change_impacts` / `UPDATE change_impacts`: 0 occurrences in all 81 source files** |
| Modules | none |
| APIs | none |
| Tests | none |

The table is a schema stub. The single keyword hit in `store.rs:1596` is a comment
listing tables a project delete cascades — not an implementation. No git-diff ingestion,
no file/function-level mapping to tests.

---

### 3.5 AI Exploratory Testing Agent — **MISSING**

Keyword search returned 33 hits, and **all are false positives**: `ai_generation.rs`
uses "exploratory" as an **evidence-level label** on a generated test case
(`evidence: evidenced | inferred | exploratory`, `ai_generation.rs:980, 1349`). One
further hit is prose in `skill_files.rs:418` ("mirror real user journeys").

| Aspect | Evidence |
|---|---|
| Agent modules | `agents/` contains `healing.rs`, `planning.rs`, `mod.rs`, `tests.rs` — **no exploratory agent** |
| Autonomous exploration loop | none — no heuristic exploration, no adaptive action generation |
| APIs | none |

What exists is *crawl-driven* exploration: `crawler.rs` discovers selectors from a live
site, and `grounding.rs` verifies a selector against crawl evidence. That is
**deterministic discovery**, not an exploratory agent that decides what to try.

---

### 3.6 Production-like User Journey Testing — **MISSING**

| Aspect | Evidence |
|---|---|
| Modules | none (1 prose hit in `skill_files.rs`) |
| Tables | none |
| APIs | none |
| Tests | none |

No journey model, no persona, no cross-step flow composition. Individual test cases can
carry `steps: Vec<TestStep>`, but nothing composes multiple cases into a journey with
shared state.

---

### 3.7 Data Integrity Intelligence — **MISSING**

| Aspect | Evidence |
|---|---|
| Modules | none |
| Tables | none |
| APIs | none |
| Tests | none |

All 9 keyword hits are **comments** about individual data-integrity rules
(`store.rs:289, 1242, 1274` — "a real parse failure is a genuine data-integrity [error]",
"BE-12 data-integrity rule"). These are defensive coding notes, not a product capability.

There is **no engine** that validates a dataset's completeness, referential
consistency, or cross-row invariants, and no such output is stored or surfaced.

---

### 3.8 Cross-Layer Correlation — **MISSING**

| Aspect | Evidence |
|---|---|
| Keywords | `cross_layer`, `crosslayer`, `CrossLayer` → **0 occurrences** in source, schema, or tests |
| Nearest existing | `operations.rs` `EnvironmentCorrelationMatrix` (env × env for one cluster), `qa_memory_risk.rs` trend across releases |

What exists correlates within **one dimension at a time** (environment, or release).
Nothing correlates **across layers** — e.g. a schema change against the tests that touch
that table against the failures that followed against the release that shipped it.

---

### 3.9 Release Readiness Intelligence — **MISSING**

| Aspect | Evidence |
|---|---|
| Keywords | `readiness` → 4 hits, **all false positives**: 3 are `adb readiness` in a mobile-emulator comment (`api.rs:990`), 1 unrelated |
| Tables | `qa_releases` (created in the previous phase, **0 rows**), `runs.release_id` (nullable) |
| Modules | `qa_memory_risk.rs` (`release_risk`, `release_risk_overview`, trend) |
| APIs | **none** |

Risk *for* a release exists at the service layer. There is **no gate, no checklist, no
verdict, and no endpoint**. `qa_releases` is a record; nothing evaluates readiness.

---

### 3.10 Smart Regression Selection — **PARTIAL**

| Aspect | Evidence |
|---|---|
| Modules | `regression.rs` (1,204 lines), `regression_api.rs` (519), `regression_runner.rs` (296) |
| Tables | `regression_schedules`, `regression_runs` (both **0 rows**) |
| APIs | `GET /regression/scope`, `GET /regression/schedules/:id/runs`, `GET /regression/runs/:id`, `POST /regression/schedules`, `POST /regression/schedules/:id/run`, `PATCH /regression/schedules/:id` |
| Tests | `regression.rs` inline; `run_map_retention_tests.rs` 6 |

**Critical distinction:** `GET /regression/scope` is **project-scoping**, not
intelligent selection. Verified: the handler resolves a schedule and returns its run
scope — no `select_tests`, `choose_tests`, `prioritize_tests`, `test_selection` or
`affected_tests`-driven selection function exists anywhere outside
`operations.rs`'s `affected_tests: Vec<String>` (task names, again not ids).

A regression engine runs **what it is told to run**. It never decides *what that should
be*.

---

### 3.11 QA Memory / Historical Intelligence — **EXISTS**

| Aspect | Evidence |
|---|---|
| Modules | `qa_memory.rs` (1,426), `qa_memory_risk.rs` (853), `adaptive_intelligence.rs`, `operations.rs` |
| Tables | `execution_telemetry` (19 rows, 6 indexes incl. `idx_telemetry_test_case`), `qa_releases`, `failure_clusters.failure_fingerprint`, `tasks.test_case_id`, `runs.release_id` |
| APIs | **13 `/intelligence/*` + `/findings` + `/requirement-impact`**, all reading the same tables |
| Tests | **93 new** across 7 modules + full 1,809 suite |
| Relationships | execution ↔ test case (`tasks.test_case_id` → `execution_telemetry.test_case_id`), run ↔ release, cluster ↔ telemetry (fingerprint), case → module (`covers`) |

Complete per its own spec, with `INSUFFICIENT_DATA` as a first-class answer and no
chain-of-thought. **Note:** the retrieval layer has **no HTTP surface** of its own — it
is store/service capability only.

---

### 3.12 Flaky Test Auto-Management — **PARTIAL**

| Aspect | Evidence |
|---|---|
| Modules | `adaptive_intelligence.rs` (`evaluate_flakiness` → `FlakinessAnalysis`, `FlakinessClassification`), `intelligence.rs` (upsert), `qa_memory.rs` (`test_stability.flakiness_score`) |
| Tables | `flakiness_metrics` (2 rows, keyed `node_id`) |
| APIs | `GET /intelligence/flakiness` |
| Tests | `adaptive_intelligence_tests.rs` 38; `qa_memory_retrieval_tests.rs` stability tests |

**What is missing — verified by search:** `quarantine`, `auto_disable`,
`disable_on`, `auto_retry`, `flaky_threshold` → **no auto-management implementation**.
The three keyword hits are unrelated (`engine.rs:965` environment health gate,
`executor.rs:566` Windows job objects, `operations.rs:927` a recommendation string).

Flakiness is **measured, classified and displayed — never acted upon**. There is no
auto-quarantine, no automatic retry policy, no flake-driven suite membership change.

---

### 3.13 AI Test Case Quality Score — **PARTIAL**

| Aspect | Evidence |
|---|---|
| Modules | `intelligence.rs` (`upsert_test_value`), `store.rs:3233` |
| Tables | **`test_value_assessments` (2 rows): `value_score`, `risk_coverage`, `historical_…`** — a genuine composite score |
| APIs | `GET /intelligence/tests` |
| Tests | `intelligence.rs` inline; `store_tests.rs` |

**What is missing:** `quality_score` / `QualityScore` → **0 occurrences**. What exists
is *test value* (is this test worth running?), which is adjacent but not identical to
*test quality* (is this test well-written — assertion strength, selector brittleness,
duplicate coverage, flaky). The plan's Phase 8 lists dimensions (assertion strength,
maintainability, duplicate coverage, flakiness, priority correctness) — **none of those
five are scored anywhere**.

---

### 3.14 AI QA Copilot — **MISSING**

| Aspect | Evidence |
|---|---|
| Keywords | `copilot`, `Copilot` → **0 occurrences** in source, schema, tests, or frontend |
| Conversational/session layer | none — no chat, no session, no assistant state |
| APIs | none |

Note `login_suggestion.rs` (85 KB) is **login-form suggestion**, unrelated to a copilot.

---

### 3.15 Duplicate-architecture risk summary

The plan's own rule forbids creating a second memory database, risk engine, failure
fingerprint engine, regression engine, planning engine, or evidence store. One proposed
feature hits that rule directly:

> **DUPLICATE_RISK — "AI Bug Intelligence" (plan Phase 1).** `bug_drafts`,
> `failure_clusters`, AI hypothesis generation, a human review gate, cluster
> investigation, timelines, environment correlation and bug-history retrieval all exist
> and are tested. Building the feature as a new phase would create a second bug
> intelligence path — exactly what the plan's "No duplicate systems" rule forbids.
>
> **Resolution:** Phase 1 should be re-scoped from *build* to *complete the trace into
> the existing bug record* (see §7).

A second, lower risk: a naive **Smart Regression** phase could build selection logic that
overlaps `qa_memory.rs::test_stability` and `adaptive_intelligence::calibrate_risk`
instead of consuming them.

---

## 4. Existing capabilities inspected (plan §"Existing capabilities")

| Capability | Status | Where |
|---|---|---|
| QA Intelligence Memory | EXISTS | `qa_memory.rs`, `qa_memory_risk.rs`, `execution_telemetry` |
| Adaptive Intelligence | EXISTS | `adaptive_intelligence.rs` (1,311+ lines), `ai_call_stats`, `ai_usage` |
| Failure Intelligence / fingerprints | EXISTS | `intelligence.rs`, `failure_clusters`, `compute_telemetry_fingerprint`, `normalize_signature` |
| Regression engine | EXISTS (not smart) | `regression*.rs` (2,019 lines), `regression_schedules/runs` |
| Coverage / technique planning | PARTIAL | `coverage.rs`, `requirement_generation.rs::RequirementCoverage` — **in-memory, no table, not persisted** |
| Requirement gap checking | EXISTS | `gap_check.rs` (2,394), `gap_verify.rs` (2,330), `POST /gap-check` |
| Test planning | EXISTS | `agents/planning.rs`, `planning/test-plan`, consumes memory feedback |
| Execution / Action AST | EXISTS | `execution_plan.rs` (2,385), `executor.rs`, `grounding.rs` |
| Evidence collection | EXISTS | `evidence` table, `evidence.rs`, per-run evidence endpoints |
| Governed healing | EXISTS | `governed_healing.rs` (1,961), `healing_proposals`, AST diff + verify gate |
| Quality Graph | PARTIAL | tables + 3 relation types **exist and are written**, but **0 rows live** |
| AI usage tracking | EXISTS | `ai_usage` (33 rows), `ai_call_stats` (3), `/ai/usage`, `/ai/status` |
| Auth / project scoping | EXISTS | `auth.rs`, 41 RBAC/scope tests |
| Persistence & migrations | EXISTS | 30 migrations, all additive, verified on a live-DB copy |
| CI / test infrastructure | PARTIAL | 3 CI jobs; **no E2E or integration job** |

---

## 5. Missing capabilities and links

| Missing link | Evidence it is missing |
|---|---|
| **A walkable trace graph** | `quality_relations` = 0 rows; no endpoint traverses requirement → test → execution → failure → bug |
| **Stable-id traceability** | `operations.rs` `affected_tests: Vec<String>` holds task **names**, not ids |
| **Change → impact → tests** | `change_impacts` table with **0 writers**; no git ingestion anywhere |
| **Risk → execution order** | `risk_assessments` written, **never read** for selection |
| **Flake → action** | no quarantine/disable/retry; flakiness is display-only |
| **Persistence of coverage** | no coverage/matrix table; `RequirementCoverage` recomputes per request |
| **Exploratory agent** | no agent module, no exploration loop |
| **User journey model** | no entity, no composition |
| **Data integrity engine** | no module/table/API |
| **Cross-layer correlation** | 0 keyword occurrences |
| **Release readiness gate** | no endpoint, no verdict, no checklist |
| **Test quality scoring** | no `quality_score`; only `value_score` (a different question) |
| **Copilot** | 0 occurrences |
| **E2E CI job** | `ci.yml` has 3 jobs, none E2E/integration |
| **Retrieval HTTP surface** | `qa_memory.rs` has no routes (deliberate in its own phase, but a gap for consumers) |

---

## 6. Reuse / extension opportunities

| Opportunity | Instead of building |
|---|---|
| Walk the existing **quality graph** (`validates` / `fails_with` / `covers` already written by 3 modules) | a new traceability store — extend `quality_relations` with the edges needed |
| Feed `calibrate_risk` real factors; add the 3 missing columns | a new risk engine — model coverage is currently 0.70 |
| Consume `qa_memory::test_stability` + `PlanningSignal` | a new prioritisation model in the copilot or risk phase |
| Extend `bug_drafts` with trace columns (fix id, retest run id) | a new bug-intelligence subsystem |
| Use `execution_plan.rs` Action AST + `grounding.rs` crawl evidence as the explorer's action space | a separate action vocabulary |
| Extend `regression.rs` scheduling with a **selection** step | a second regression engine |
| Persist `RequirementCoverage` into a new table | recomputing per request |
| Use `governed_healing`'s approval/verification gate pattern | inventing governance for copilot/agent writes |

---

## 7. Dependencies (implementation order must respect these)

```text
quality-graph trace identity  ──┐
   (requirement ↔ test ↔ run)   │
                                ├─▶ Change Impact ─▶ Smart Regression Selection
change ingestion (git diff) ────┘         │
                                          ▼
                    Risk-Based Testing ◀──┘  (risk must select, not just display)
                              │
                              ├─▶ Release Readiness (needs risk + regression evidence)
                              ▼
                    AI QA Copilot (needs every signal above)

independent of the above:
  Exploratory Agent  ── depends on: Action AST + grounding (both exist)
  User Journeys      ── depends on: a step/composition model (missing)
  Data Integrity     ── depends on: nothing existing (greenfield, but greenfield ≠ duplicate)
  Cross-Layer        ── depends on: the trace graph existing first
```

**Critical dependency the plan's order misses:** Change Impact (Phase 3) and Smart
Regression (Phase 3) both require a walkable trace graph. That graph does not exist in
live data. Phase 3 as currently sequenced would have nothing to walk.

---

## 8. Recommended implementation order

Deviates from the plan's suggested order, **on repository evidence**, which the plan
explicitly permits ("The order may change ONLY if Phase 0 repository evidence shows a
dependency or existing implementation that justifies it").

| # | Phase | Rationale from evidence |
|---|---|---|
| **1** | **Trace graph completion** *(new)* | 0 relations live; 3 relation types already written. Everything downstream walks this. |
| **2** | **AI Bug Intelligence — completion, not construction** | reuse §3.1; add trace + fix + retest linkage to the existing `bug_drafts` |
| **3** | **Change Impact Analysis** | table exists with 0 writers; needs phase 1 to map change → tests |
| **4** | **Smart Regression Selection** | consume phases 1 + 3; extend `regression.rs`, do not replace |
| **5** | **Risk-Based Testing** | risk exists; make it **select** rather than display; close the 0.30 coverage gap |
| **6** | **Cross-Layer Correlation** | now possible once phase 1 exists |
| **7** | **Release Readiness** | needs risk + regression evidence; gate + verdict |
| **8** | **Data Integrity Intelligence** | genuinely greenfield, no dependency |
| **9** | **User Journeys** | needs a composition model |
| **10** | **Exploratory Agent** | build on Action AST + grounding, under the healing governance pattern |
| **11** | **Test Quality Score** | extend `test_value_assessments` with quality dimensions |
| **12** | **AI QA Copilot** | last — it should read every signal above |
| — | **E2E CI job** *(enabler)* | add early; it validates phases 2–12 and is missing today |

---

## 9. Recommended Phase 1 scope

**Phase 1 = Trace Graph Completion.** Not a new subsystem: populate and extend the
graph that three existing modules already write to.

**In scope**
1. Determine why `quality_relations` is empty in live data (code path vs. data) and fix
   the real cause.
2. Replace `operations.rs` `affected_tests: Vec<String>` (task names) with stable test-case
   ids, reusing the `test_case_id` link migration 0028 added.
3. Add the missing graph edges needed for a walk: `derived_from` (case → requirement,
   reusing `validates`), `retests` (bug → run), `fixed_by` (cluster → healing proposal).
4. Add **one** read endpoint that walks requirement → test → execution → failure → bug →
   healing, project-scoped, deterministic.
5. Persist `RequirementCoverage` so coverage is a record, not a per-request recompute.

**Explicitly out of scope**
- No new bug-intelligence subsystem (it exists — `§3.1`).
- No risk engine, no regression engine, no planning engine.
- No new memory table.
- No destructive migration.

**PASS gate:** trace walk returns real data on a fixture built through production code
paths · 0 cross-project leakage · existing 1,809 suite still green · walk is deterministic
and bounded · no duplicate architecture.

---

## 10. Estimated complexity

| Proposed feature | Complexity | Why |
|---|---|---|
| Trace graph completion | **Medium** | edges + 1 endpoint; schemas exist |
| AI Bug Intelligence (completion) | **Low–Medium** | reuse; add linkage columns |
| Change Impact Analysis | **Medium–High** | git ingestion is new; table exists |
| Smart Regression Selection | **Medium** | selection algorithm; scheduling exists |
| Risk-Based Testing | **Medium** | factors measured; wiring + 3 columns |
| Cross-Layer Correlation | **High** | needs the graph + multi-source joins |
| Release Readiness | **Medium** | gate + verdict over existing evidence |
| Data Integrity Intelligence | **Medium** | greenfield but well-scoped |
| User Journeys | **Medium–High** | new composition model |
| Exploratory Agent | **High** | agent loop + governance; Action AST helps |
| Test Quality Score | **Medium** | scoring dimensions; `value_score` is a start |
| AI QA Copilot | **High** | reads everything above; session layer new |
| E2E CI job | **Low** | infrastructure only |

---

## 11. Open questions and risks

| # | Question / risk | Impact |
|---|---|---|
| Q1 | **Why is `quality_relations` empty?** Code path never taken, or data issue? Must be answered before Phase 1 | Blocks the entire recommended order |
| Q2 | **Why does `change_impacts` have a table and 0 writers?** Dead schema from an earlier design, or an unfinished feature? | Decides whether Phase 3 extends or creates |
| Q3 | 19 telemetry rows cannot demonstrate any intelligence. **Every capability is unexercised on real data.** Fixtures will carry the load | Claims must be scoped to fixtures until real history exists |
| Q4 | No E2E CI job — phases claiming end-to-end validation have no automated gate | Add early |
| Q5 | `tasks.test_case_id` has **no foreign key** (SQLite cannot `ALTER TABLE ADD CONSTRAINT`; needs a destructive table rebuild) | Integrity gap; stop-condition candidate |
| Q6 | `runs_for_release(release_id)` is not project-scoped; safe today via 6 guarded call sites, but `pub` | Latent leak if a future caller bypasses the guard |
| Q7 | Pre-existing: `runs.plan` stores `TaskDef.params` **verbatim, including credentials**. Retrieval never reads it, so QA memory is unaffected | Security item needing a decision |
| Q8 | 30% of the risk model is structurally unmeasurable | Risk scores are partial by construction |
| Q9 | `test_cases` has **17 of 19** orphaned telemetry rows in live data | Pre-existing data condition, unchanged by migrations |
| Q10 | `store.rs` has pre-existing rustfmt/clippy debt; `cargo fmt --check` fails repo-wide | Keep new files clean; don't reformat the repo |
| Q11 | Coverage is in-memory and recomputed — no persisted record | Any coverage claim is per-request only |

---

## 12. Phase 0 PASS gate

| Criterion | Status |
|---|---|
| Current capabilities mapped | yes — §2, §4 |
| Duplicates identified | yes — §3.15 (AI Bug Intelligence) |
| Missing links identified | yes — §5 |
| Dependencies documented | yes — §7 |
| Implementation order justified from repository evidence | yes — §8, with the deviation and its reason stated |
| Every one of the 14 features classified | yes — §3 |
| Every classification backed by search, not assumption | yes — false positives explicitly recorded and dismissed |
| No application logic modified | yes — working tree clean at `50488f2` |
| No migration created | yes |
| No feature code written | yes |
| Nothing committed or pushed | yes |

**PHASE 0: PASS**

---

## 13. Method — how each classification was established

Every status in §3 was reached by searching the repository, then reading the matching
implementation and its tests. Where a keyword hit turned out to be a comment, a prose
string, or an unrelated constant, that is stated inline rather than counted as evidence.
Specific examples of dismissed false positives:

- `explorat` → 33 hits, all an evidence-level **label** on generated test cases
  (`ai_generation.rs:980, 1349`), plus one prose line (`skill_files.rs:418`)
- `readiness` → 4 hits, three are `adb readiness` in a mobile-emulator comment
  (`api.rs:990`)
- `change_impact` → 1 hit, a cascade comment in `store.rs:1596`; the table has 0 writers
- `data_integrity` → 9 hits, all comments about individual defensive rules
- `journey` → 1 hit, prose in `skill_files.rs:418`
- `smart_regression` → 812 hits, dominated by the word "smart" in unrelated identifiers
- `quality_score` → 0; only `value_score` exists, which answers a different question

Baseline validation that the audit modified nothing: `git status --short` returned only
the user-supplied untracked `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`; the working tree is
otherwise clean at `50488f2`.