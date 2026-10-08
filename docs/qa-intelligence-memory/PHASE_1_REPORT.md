# Phase 1 — QA Intelligence Memory Audit & Data Mapping: Report

**Implemented:** 2026-10-07 · **Report finalised:** 2026-10-08
**Phase:** 1 of 6
**Status:** PASS
**Baseline before this phase:** 1716 passed / 0 failed / 2 ignored
**After this phase:** 1716 passed / 0 failed / 2 ignored (unchanged — audit only)

The substantive deliverable of this phase is **`PHASE_1_AUDIT.md`** (19,246 bytes).
This report summarises it and records what the audit concluded and why.

---

## 1. What this phase did

An architecture and data audit. **No production code was changed.**

Everything reported is either read out of the source and schema, or measured against
the live database at `infra/data/orchestrator.sqlite3` (27 migrations, 40 tables).
Where something is *designed* rather than existing, it is labelled as a recommendation.

---

## 2. The finding that shaped the whole plan

```text
execution_telemetry.test_case_id   →  0 of 19 rows populated
tasks.test_case_id                 →  column did not exist at all
```

The master plan's central chain is
`… → Test Case → Execution → …`. The column that would carry it existed, was indexed by
nothing, and was never written — because the forward link from an execution to its test
case had nowhere to live. The only place a case id existed was inside the `runs.plan`
JSON blob: unqueryable, unindexed, and lost the moment anything read the plan.

Every "per test case" intelligence the plan asks for in Phase 3 was therefore derived
from a free-text `task_name` string. **A name is not an identity** — two different cases
can share one, and one case can be renamed — so history keyed on it is not history.

The backward half of the link already existed and was untouched:
`test_cases.executed_run_id` / `executed_task_id`.

---

## 3. Second finding — three links genuinely absent

| Link | State |
|---|---|
| Project | exists |
| → **Release/Build** | **absent** — zero tables, zero columns, zero source references |
| → Environment | nominal only — `execution_telemetry.environment` defaults to the literal `'test'`; **1 distinct value in 19 rows** |
| → Requirement | exists |
| → **Module** | **absent** — no table, and no `module` node type either |
| → Test Case | exists, but the execution link does not |
| → Execution, Result, Failure, Bug, Evidence | exist |

Additional sparsity measured: `assertion_result`, `http_status`, `selector`,
`verification_outcome`, `execution_id` are each **0 of 19** populated, and
`evidence_refs` is `[]` in 19 of 19 rows although `evidence` rows exist.

---

## 4. The finding that redirected the work

The plan reads like a build specification. The audit established it was not:

- `execution_telemetry` is a real append-only history with **five project-scoped
  indexes** — it *is* the memory backbone, and a new "memory" table would duplicate it
- `operations.rs` already owns a traceability chain (Phase 8 of the product's own
  history): Test → Task → Failure → Cluster → Evidence → Healing → Approval →
  Execution → Verification → Quality Graph
- `RecurrenceState` (New/Recurring/Persistent/Resolved) already exists
- 13 `/intelligence/*` endpoints plus `/findings` and `/requirement-impact` already serve
  much of what Phase 3 requests
- `TestPlanningAgent` already consumes memory feedback via `api.rs:8409` and `:8415`

So this is an **extension** job. The audit's §7 lists six things that must **not** be
built again, and that list governed every subsequent phase.

---

## 5. The minimal design this phase recommended

| # | Recommendation | Rationale |
|---|---|---|
| 1 | `tasks.test_case_id` + write path + idempotent backfill | highest value, lowest risk; makes an existing column start working |
| 2 | `qa_releases` + nullable `runs.release_id` | the one genuinely new entity — Phase 5 cannot exist without it |
| 3 | Module as `quality_nodes[node_type='module']` | a module is the same kind of vertex as a test or requirement; a table would be a fourth parallel identity system |
| 4 | Retrieval derived on demand, not duplicated | consistent with how `operations.rs` and `adaptive_intelligence.rs` already read telemetry |
| 5 | Explicitly **not** proposed | no chain-of-thought storage; no credential material; no separate tenant table |

Migrations specified in §9, with the safety argument: nullable columns only, no `DROP`,
no `DELETE`, no rewrite, verified against both a fresh and an existing database.

---

## 6. Bugs found

No code was changed, so none were introduced.

**Eight data defects were found and reported** (audit §4), all left for later phases:

| Defect | Evidence |
|---|---|
| `execution_telemetry.test_case_id` never written | 0 of 19 rows |
| `tasks` has no `test_case_id` column | schema and live DB |
| `environment` is a constant default | 1 distinct value over 19 rows |
| `assertion_result`, `http_status`, `selector`, `verification_outcome`, `execution_id` never written | 0 of 19 each |
| `evidence_refs` always `[]` although `evidence` rows exist | 19 of 19 identical |
| No release/build entity | zero occurrences |
| No module entity and no `module` node type | zero occurrences |
| `infra/migrations` is a stale 20-file copy of the 27-file live set | directory listing |

Of these, the first, second and the two absent entities were addressed by Phase 2; the
fingerprint linkage gap that surfaced later is recorded in `PHASE_6_REPORT.md` §3.

---

## 7. Tests

No new tests — the phase changed no code.

```text
cd services/orchestrator && cargo test
  → 1716 passed / 0 failed / 2 ignored   (exit 0, before and after)
```

A baseline was nonetheless taken before and after, so later phases have a reference
point. It became the anchor for the whole plan's regression accounting.

---

## 8. Stop conditions

Master plan §10 was checked against all ten items. **None was triggered** — the design
was additive, extended existing systems, and introduced no new tenant concept. The
reasoning is recorded in `PHASE_1_PASS.md`.

---

## 9. Phase 1 PASS gate

| Criterion | Status |
|---|---|
| Architecture mapped | yes — audit §1, §2 |
| Persistence mapped | yes — audit §2 |
| Existing memory capabilities identified | yes — audit §1.2, §1.3, §7 |
| Missing capabilities identified | yes — audit §4 |
| Stable IDs identified | yes — audit §5.1 |
| Relationships identified | yes — audit §3.1, §6 |
| Minimal implementation design documented | yes — audit §8 |
| Migration requirements documented | yes — audit §9 |
| Phase 2 plan documented | yes — audit §10 |
| No code changed in this phase | yes |
| Existing suite still green | yes — 1716 / 0 / 2, exit 0 |

**Phase 1: PASS.**