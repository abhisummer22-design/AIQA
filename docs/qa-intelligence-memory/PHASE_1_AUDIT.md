# Phase 1 — QA Intelligence Memory: Audit & Data Mapping

**Date:** 2026-10-07
**Phase:** 1 of 6
**Status:** complete — this document is the Phase 1 deliverable
**Baseline for all later phases:** backend `1716 passed / 0 failed / 2 ignored` (`cargo test`, exit 0)

---

## 0. What this phase did

An architecture and data audit only. **No production code was changed in Phase 1.**
Everything below is either (a) read out of the source/schema, or (b) measured against
the live database at `infra/data/orchestrator.sqlite3` with 27 applied migrations and
40 tables.

Nothing in this document is inferred from documentation. Where a claim is measured, the
measurement is stated. Where something is *designed* (sections 8–10), it is labelled as a
recommendation and not as existing behaviour.

---

## 1. Current architecture map

### 1.1 The chain the master plan asks for, mapped against reality

The plan's target chain:

```
Project → Release/Build → Environment → Requirement → Module
        → Test Case → Execution → Result → Failure → Bug → Evidence
```

Measured status of each link:

| Link | Exists? | Where | Notes |
|---|---|---|---|
| Project | **yes** | `projects` (id, name, description, created_at, updated_at) | 2 rows |
| → Release/Build | **NO** | — | zero tables, zero columns, zero source references |
| → Environment | **nominal only** | `execution_telemetry.environment TEXT NOT NULL DEFAULT 'test'` | 1 distinct value in 19 rows |
| → Requirement | **yes** | `requirements` (id, project_id, document_id, req_key, ordinal, section, kind, text) | FK → `requirement_documents` |
| → Module | **NO** | — | no `module` node type either; see §5.3 |
| → Test Case | **yes** | `test_cases` (26 columns) | FK → `test_suites` |
| → Execution | **yes** | `runs` → `tasks` → `run_events` | `tasks` has FK → `runs` |
| → Result | **yes** | `tasks.status` / `reason`; `execution_telemetry.status` | |
| → Failure | **yes** | `execution_telemetry.failure_fingerprint`, `failure_category`; `failure_clusters` | 6 of 19 rows carry a fingerprint |
| → Bug | **yes** | `bug_drafts` (FK → `failure_clusters`) | 0 rows |
| → Evidence | **yes** | `evidence` (FK → `runs`, `quality_nodes`) | 2 rows |

**Result: three links are genuinely absent (Release/Build, Module, real Environment) and
one is present in schema but never populated (`execution_telemetry.test_case_id`).**

### 1.2 Existing traceability chain (product's own Phase 8)

`operations.rs:9` already declares and implements a traceability chain:

```
Test → Task → Failure → Cluster → Evidence → Healing Proposal → AST Diff
     → Approval → Execution → Verification → Quality Graph
```

This is a **different, already-built** chain that overlaps the master plan's. It is the
single most important thing to not duplicate. See §7.

### 1.3 Module inventory

81 Rust files, 4,624,282 bytes in `services/orchestrator/src`. The modules that matter for
memory/intelligence:

| Module | Bytes | Role for memory |
|---|---|---|
| `store.rs` | 200,227 | all persistence |
| `intelligence.rs` | 72,289 | risk/value/flakiness upserts, failure recording, signature normalization |
| `adaptive_intelligence.rs` | 33,273 | fingerprinting, categorization, flakiness, trend, strategy scoring, planning feedback |
| `operations.rs` | 39,276 | investigations, recurrence state, environment matrix, retention |
| `governed_healing.rs` | 78,155 | healing proposals + strategy calibration |
| `coverage.rs` | 42,176 | requirement × technique coverage matrix (in-memory, no history) |
| `agents/planning.rs` | — | `TestPlanningAgent` — already consumes memory feedback |

---

## 2. Existing persistence map

40 tables. Grouped by what they hold.

### 2.1 Historical (append/retain) — the memory backbone

| Table | Rows | Role |
|---|---|---|
| `execution_telemetry` | 19 | **the backbone.** one row per task execution; 5 indexes |
| `audit_log` | 1036 | every mutating action, with actor/role/project/outcome |
| `run_events` | 8 | per-task event stream |
| `runs` / `tasks` | 2 / 2 | execution header + unit of work |

`execution_telemetry` indexes (from `0016_adaptive_intelligence.sql`):

```
idx_telemetry_project_task   (project_id, task_name)
idx_telemetry_fingerprint    (project_id, failure_fingerprint)
idx_telemetry_created_at     (project_id, created_at)
idx_telemetry_run_id         (run_id)
idx_telemetry_env            (project_id, environment)
```

Every index is project-scoped first. **Tenant isolation is already built into the physical
layout**, not just the query.

### 2.2 Derived / aggregated

| Table | Rows | Keyed on |
|---|---|---|
| `risk_assessments` | 2 | `node_id` → `quality_nodes` |
| `test_value_assessments` | 2 | `node_id` |
| `flakiness_metrics` | 2 | `node_id` |
| `healing_strategy_metrics` | 0 | `project_id`, `strategy`, `failure_category` |
| `failure_clusters` | 0 | `project_id`, `signature` (unique per project) |

### 2.3 Graph

`quality_nodes` (`project_id`, `node_type`, `external_ref`, `name`) + `quality_relations`
(`from_node_id`, `to_node_id`, `relation_type`).

Relation types actually used in source:
- `validates` — `intelligence.rs:675`, `agent_execution.rs:332` (case → requirement)
- `fails_with` — `adaptive_intelligence.rs:831` (case → failure cluster)

**This is the natural home for a `module` node type — no new table needed.** See §8.

### 2.4 Retention

`store.rs:4166 prune_execution_telemetry` — **opt-in, operator-driven only**, via
`POST /intelligence/retention/dry-run` then `/prune`. It shields tasks of active runs and of
unexpired `PENDING_APPROVAL`/`APPROVED` healing proposals.

So history is *not* silently destroyed. But any memory derived **on demand** from telemetry
is destroyed when an operator prunes. That is a real constraint for Phase 2 design (§8.3).

---

## 3. Existing historical data (measured)

Field fill rates over the 19 `execution_telemetry` rows:

| Field | Filled | Distinct |
|---|---|---|
| `test_case_id` | **0 / 19** | 0 |
| `execution_id` | **0 / 19** | 0 |
| `environment` | 19 / 19 | **1** (`'test'`) |
| `failure_category` | 6 / 19 | 2 |
| `failure_fingerprint` | 6 / 19 | 6 |
| `error_message` | 6 / 19 | 6 |
| `evidence_refs` | 19 / 19 | 1 (always `[]`) |
| `assertion_result` | **0 / 19** | 0 |
| `http_status` | **0 / 19** | 0 |
| `selector` | **0 / 19** | 0 |
| `verification_outcome` | **0 / 19** | 0 |
| `retry_count` / `healing_attempt_count` | 19 / 19 | 1 each |

Statuses seen: `PASSED` 13, `FAILED` 6.

Empty history tables: `requirements`, `failure_clusters`, `bug_drafts`,
`quality_relations`, `change_impacts`, `healing_proposals`, `healing_strategy_metrics`,
`regression_runs`, `regression_schedules`, `openapi_runs`, `performance_runs`,
`autopilot_runs`, `documents`.

### 3.1 The single most important finding

**`execution_telemetry.test_case_id` is NULL in 19 of 19 rows.**

The plan's central chain is `… → Test Case → Execution → …`. The column that would carry
it exists, is indexed by nothing, and is never written. So:

- Test stability history is keyed on a **free-text `task_name` string**, not on a stable id
- `execution_telemetry_from_row` cannot answer "how did *this test case* behave over time"
- `tasks` has **no `test_case_id` column at all**; the case id exists only inside the
  `runs.plan` JSON blob

Every "per test case" intelligence the plan asks for in Phase 3 is currently derived from a
string. `task_name` is not a stable identity — two different cases can share a name, and one
case can be renamed.

---

## 4. Missing historical data

| Missing | Why it matters | Blocked by |
|---|---|---|
| Execution → Test Case link | per-case stability/recurrence over time | no durable column (§3.1) |
| Release / build identity | Phase 5 is *entirely* about release risk | entity does not exist |
| Module identity | Phase 3 "Module Risk History" | entity does not exist |
| Real environment per run | cross-environment correlation is nominal | `environment` is a constant default |
| Per-requirement execution history | Phase 3 "Requirement History" | only derivable via `quality_relations`, which has 0 rows and no run/time dimension |
| Bug recurrence history | Phase 3 "Bug History" | `bug_drafts` holds one mutable `review_state`, no event history |
| Assertion-level outcomes | "why did it fail" beyond a fingerprint | `assertion_result` never written |
| Evidence linkage from telemetry | evidence per execution | `evidence_refs` always `[]` though `evidence` rows exist |

---

## 5. Existing IDs

### 5.1 Stable ids that exist and should be reused

| Entity | Id | Notes |
|---|---|---|
| project | `projects.id` TEXT | UUID |
| test case | `test_cases.id` TEXT | UUID |
| requirement | `requirements.id` TEXT | UUID; `req_key` is the human key |
| run | `runs.id` TEXT | UUID |
| task | `tasks.id` TEXT | UUID |
| telemetry | `execution_telemetry.id` TEXT | UUID |
| fingerprint | `execution_telemetry.failure_fingerprint` TEXT | 64-hex, `sha`-style (`adaptive_intelligence.rs:294 compute_telemetry_fingerprint`) |
| cluster | `failure_clusters.id` | UUID; uniqueness is `(project_id, signature)` at the store level |
| evidence | `evidence.id` | UUID |
| quality node | `quality_nodes.id` | UUID; identity is `(project_id, node_type, external_ref)` (`store.rs:2679`) |

**Reuse all of these. None needs replacement.**

### 5.2 Existing conventions to follow

- Timestamps are **TEXT in RFC3339 with offset**, parsed by `parse_ts` / `now_rfc3339`.
- Enums are stored SCREAMING_SNAKE_CASE strings, with `as_str()` / `from_str()` pairs.
- Migrations are `NNNN_snake_case.sql`, applied by `sqlx::migrate!`, next free number
  **0028**.
- `services/orchestrator/migrations` (27 files) is the live set. `infra/migrations` (20
  files) is a stale partial copy — not used by the running binary.
- Test harness: `store_tests.rs:21 temp_store()` → `tempfile::tempdir()` +
  `Store::connect(&path)`. Every store test gets a real isolated SQLite file.

### 5.3 Module — deliberately not a table

No `module` table exists, and no `node_type = 'module'` is ever written. But
`quality_nodes` is a generic typed graph already used for `test`, requirement and cluster
nodes. A module is exactly the same kind of vertex.

**Recommendation: model module as `quality_nodes.node_type = 'module'` reached by
`quality_relations` from tests.** This avoids a 5th parallel identity system.

---

## 6. Missing relationships

| Relationship | Present as | Problem |
|---|---|---|
| test case → execution | `execution_telemetry.test_case_id` (column) | never written |
| task → test case | inside `runs.plan` JSON | not queryable, not indexed |
| execution → release | — | does not exist |
| execution → module | — | does not exist |
| execution → real environment | `execution_telemetry.environment` | always the literal `'test'` |
| requirement → executions over time | `quality_relations('validates')` | no run or time dimension on the relation; 0 rows |
| bug → recurrence events | `bug_drafts.review_state` | single mutable column, no history |
| telemetry → evidence | `evidence_refs` | always `[]` |

---

## 7. Duplication risks — what must NOT be built again

These already exist and Phase 2–5 must **extend** them:

1. **A "memory" table.** `execution_telemetry` already is the append-only, indexed,
   project-scoped history. A new memory table would be a second copy of the same facts.
2. **A second failure-intelligence system.** `failure_clusters` + `normalize_signature`
   (`intelligence.rs:141`) + `compute_telemetry_fingerprint` +
   `categorize_failure` + `RecurrenceState` (`operations.rs:25`) already cover
   clustering, recurrence and categories.
3. **A second planning engine.** `agents/planning.rs` already consumes memory:
   `api.rs:8409` wires `healing_service.get_feedback_for_planning(...)` and
   `api.rs:8415` wires `generate_planning_feedback(...)`. Phase 4 adds *signals*, not an agent.
4. **Parallel risk/flakiness stores.** `risk_assessments`, `flakiness_metrics`,
   `test_value_assessments` + `/intelligence/risk`, `/intelligence/flakiness`,
   `/intelligence/trends` already serve Phase 3's risk and stability surfaces.
5. **A third traceability chain.** `operations.rs:9` already owns
   Test→Task→Failure→Cluster→Evidence→Healing→Verification→Graph.
6. **Overlapping read endpoints.** `/intelligence/failures` and `/findings` both expose
   clustered failures; `/intelligence/investigations` and
   `/intelligence/failures/:id/investigation` overlap. Phase 3 should extend one, not add a
   fourth.

---

## 8. Recommended minimal architecture

Ordered by value per unit of risk. Each item is additive.

### 8.1 Make the existing test-case link durable (highest value, lowest risk)

Add `tasks.test_case_id` (nullable TEXT) in migration `0028`, plus an index
`(project_id, run_id)`-compatible path. Write it at task creation from the same place
`runs.plan` already knows the case id. Backfill `execution_telemetry.test_case_id` from it
at telemetry-write time.

Why this first: it is the one defect that makes *every* Phase 3 requirement unsound today.
It adds a column; it cannot destroy anything; it makes an existing column start working.

### 8.2 Release as a first-class, optional entity

One table, `qa_releases` (`id`, `project_id`, `name`, `version`, `created_at`, …) and a
nullable `runs.release_id`. Nullable and never fabricated: a run with no release simply has
none, and release intelligence then answers `INSUFFICIENT_DATA` honestly, which §7 of the
master plan explicitly prefers.

This is the only genuinely new table, because Phase 5 cannot exist without it and nothing
today can be extended into it.

### 8.3 Module as a graph node type

`quality_nodes` with `node_type='module'` + `quality_relations`
(`relates_to` / `covered_by`). Reuses §5.3. No new table, no new identity system.

### 8.4 Memory derived, not duplicated

Retrieval reads `execution_telemetry` (+ clusters, + graph) on demand, exactly as
`operations.rs` and `adaptive_intelligence.rs` already do. Given §2.4's retention is
opt-in and destructive-when-run, anything that must survive pruning gets its own small
**append-only** summary row; anything derivable does not.

### 8.5 Explicitly not proposed

- no chain-of-thought storage — §1.5 of the plan; the existing
  `QaIntelligenceFeedback` (counts + labels + confidence) is the correct shape
- no credential material in memory — `mask_dynamic_tokens` (`adaptive_intelligence.rs:280`)
  and `sanitize_telemetry_text` already exist and must be applied to anything new
- no separate tenant table — `projects` + `require_scoped_project` is the boundary

---

## 9. Migration requirements

| Migration | Change | Destructive? |
|---|---|---|
| `0028_qa_memory_linkage.sql` | `ALTER TABLE tasks ADD COLUMN test_case_id TEXT`; index `idx_tasks_case` | **No** — nullable column |
| `0029_qa_releases.sql` | `CREATE TABLE qa_releases`; `ALTER TABLE runs ADD COLUMN release_id TEXT` | **No** — new table + nullable column |

Both are additive. `ALTER TABLE … ADD COLUMN` on SQLite preserves existing rows; no
backfill is required for correctness, and any backfill is optional and idempotent.

**No migration in this plan drops, rewrites, or moves existing data.** Therefore master-plan
§10 stop conditions 1, 2, 4 and 8 are not triggered.

Migrations must be written so they apply to:
(a) a **fresh** database — `Store::connect` on an empty file, and
(b) the **existing** live database — verified against a copy of
`infra/data/orchestrator.sqlite3`, never the live file itself.

---

## 10. Phase 2 implementation plan

**Objective:** establish the persistent foundation, reusing everything in §8.

Steps, in order:

1. `0028` — `tasks.test_case_id`, and make the writer set it.
2. Populate `execution_telemetry.test_case_id` when the task carries one.
3. `0029` — `qa_releases` + `runs.release_id` (nullable).
4. Module as `quality_nodes.node_type='module'` + relations.
5. Backfill: derive `test_case_id` for historical telemetry from `runs.plan` where it is
   unambiguous. **Idempotent, and skipped where ambiguous rather than guessed** — guessing
   would violate §1.4.

### Phase 2 tests to write

| Area | Cases |
|---|---|
| Fresh migration | `0028`/`0029` apply to an empty DB; expected columns/tables exist |
| Existing migration | apply to a DB carrying the pre-0028 schema; existing rows survive |
| Idempotence | re-running the backfill changes nothing |
| Write/read round trip | task created with a case id → telemetry row carries it |
| Unambiguous backfill | historical row whose plan names exactly one case gets it |
| Ambiguous backfill | historical row naming zero or several cases is left NULL, not guessed |
| Project isolation | project A's case id never appears on project B's telemetry |
| Invalid reference | a `test_case_id` naming a case of another project is rejected or reported |
| Partial metadata | release-less run, module-less case, NULL everything → no error |
| No duplicates | two executions of one case produce two telemetry rows, one case |

### Phase 2 PASS gate (self-check)

- [ ] migrations apply to fresh **and** existing databases
- [ ] existing rows preserved (row counts before/after equal)
- [ ] `execution_telemetry.test_case_id` is written on new executions
- [ ] backfill is idempotent and refuses to guess
- [ ] project isolation holds on every new read/write path
- [ ] no new table duplicates `execution_telemetry`
- [ ] backend suite ≥ 1716 passing, 0 failing
- [ ] clippy clean, changed files rustfmt-clean
- [ ] `docs/qa-intelligence-memory/PHASE_2_REPORT.md` written

---

## 11. Open items carried forward (not blockers)

1. **Live history is nearly empty.** 19 telemetry rows, 0 clusters, 0 requirements. Phase 3
   and 5 intelligence must answer `INSUFFICIENT_DATA` on this data — which is correct
   behaviour, and their tests must create their own fixtures rather than rely on the live DB.
2. **`/intelligence/failures` vs `/findings` overlap.** Not resolved in Phase 1; Phase 3
   should pick one to extend.
3. **`infra/migrations` is a stale 20-file copy** of the 27-file live set. Harmless, but a
   trap for anyone reading the wrong directory.
4. **`environment` defaults to `'test'`.** Until a run can carry a real environment,
   `/intelligence/failures/:id/environments` can only ever report one environment. Phase 2
   should make the value settable; Phase 3 can then correlate for real.

---

## 12. Phase 1 PASS gate

| Criterion | Status |
|---|---|
| Architecture mapped | done — §1, §2 |
| Persistence mapped | done — §2 |
| Existing memory capabilities identified | done — §1.2, §1.3, §7 |
| Missing capabilities identified | done — §4 |
| Stable IDs identified | done — §5.1 |
| Relationships identified | done — §3.1, §6 |
| Minimal implementation design documented | done — §8 |
| Migration requirements documented | done — §9 |
| Phase 2 plan documented | done — §10 |
| No code changed in this phase | yes |
| Existing suite still green | yes — 1716 / 0 / 2, exit 0 |

**Phase 1: PASS.**