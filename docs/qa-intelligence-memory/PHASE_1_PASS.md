# Phase 1 — PASS

**Phase number:** 1 of 6
**Completion date:** 2026-10-07
**Status:** PASS

## Summary

Architecture and data audit of QA Intelligence Memory. No production code was changed.
The audit establishes that the repository already contains a substantial, indexed,
project-scoped historical-intelligence backbone, and that the master's plan is therefore
mostly an *extension* job rather than a build job.

Three findings shaped the whole plan:

1. `execution_telemetry` is a real append-only history with five project-scoped indexes —
   it is the memory backbone, and a new "memory" table would duplicate it.
2. `execution_telemetry.test_case_id` is **NULL in 19 of 19 rows**, and `tasks` has no
   `test_case_id` column at all. The case→execution identity the master plan's central chain
   depends on does not exist in practice; per-case history is currently keyed on a free-text
   `task_name`.
3. **Release/Build, Module and a real Environment do not exist** — no tables, no columns, no
   source references. `environment` is a constant default (`'test'`, 1 distinct value).

## Files changed

| File | Kind |
|---|---|
| `docs/qa-intelligence-memory/PHASE_1_AUDIT.md` | added (the phase deliverable) |
| `docs/qa-intelligence-memory/PHASE_1_PASS.md` | added (this file) |

No source, migration, or test file was touched.

## Tests run

```
cd services/orchestrator && cargo test
```

Baseline taken before and after the audit:

```
test result: ok. 1716 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
exit code: 0
```

## Tests passed

1716 passed, 0 failed, 2 ignored (the 2 pre-existing ignores are untouched).

## Bugs found

None — Phase 1 changed no code, so it introduced none.

## Bugs fixed

Not applicable to this phase.

## Data defects found (reported, not fixed here)

| Defect | Evidence |
|---|---|
| `execution_telemetry.test_case_id` never written | 0 of 19 rows populated |
| `tasks` has no `test_case_id` column | schema of `0001_core_execution.sql` / live DB |
| `environment` is a constant default | 1 distinct value (`'test'`) over 19 rows |
| `assertion_result`, `http_status`, `selector`, `verification_outcome`, `execution_id` never written | 0 of 19 rows each |
| `evidence_refs` always `[]` although `evidence` rows exist | 19/19 identical |
| No Release/Build entity | zero schema/source occurrences |
| No Module entity and no `module` node type | zero schema/source occurrences |
| `infra/migrations` is a stale 20-file copy of the 27-file live set | directory listing |

These are inputs to Phase 2, which addresses them with additive, non-destructive migrations.

## Regression result

Unchanged and green: 1716 passed / 0 failed / 2 ignored, exit 0.

## Known limitations

- The audit reads the live database for measurements. The live DB holds very little history
  (19 telemetry rows, 0 clusters, 0 requirements), so no Phase 3/5 claim can be
  demonstrated against it. That is correct behaviour — those phases must answer
  `INSUFFICIENT_DATA` — and their tests must build their own fixtures rather than depend
  on live rows.
- `/intelligence/failures` and `/findings` expose overlapping clustered-failure views. Not
  resolved in Phase 1.
- No performance measurement was taken; Phase 1 is a static audit.

## Stop conditions

Master-plan §10 was checked against every item. None is triggered:

1. migration destroying data — no migration proposed yet; both planned ones are additive
2. architecture conflict — the design extends `quality_nodes` and `execution_telemetry`
3. entity cannot be linked — Release is being created, not linked; nullable by design
4. tenant isolation break — every new read/write is project-scoped and indexed that way
5. tests contradict the design — baseline green, nothing contradicted
6. undocumented architectural dependency — none found beyond those documented
7. PASS needs weakened tests — no
8. major subsystem rewrite — no
9. history cannot support a claim — handled by `INSUFFICIENT_DATA`, not by inventing data
10. replace-or-extend ambiguity — resolved: extend

## Next phase prerequisites

Before Phase 2 begins, read:

1. `docs/qa-intelligence-memory/PHASE_1_AUDIT.md` — especially §7 (duplication risks),
   §8 (minimal architecture) and §9 (migration requirements)
2. `docs/qa-intelligence-memory/PHASE_1_PASS.md` — this file
3. `services/orchestrator/migrations/0016_adaptive_intelligence.sql` — the
   `execution_telemetry` shape and its indexes
4. `services/orchestrator/migrations/0001_core_execution.sql` — the `runs`/`tasks` shape
5. `services/orchestrator/src/store.rs` — `insert_execution_telemetry` (L3875),
   `insert_task`, `runs.plan` construction, and the `(project_id, node_type, external_ref)`
   identity at L2679
6. `services/orchestrator/src/store_tests.rs:21` — the `temp_store()` harness every new
   store test must use
7. `services/orchestrator/src/adaptive_intelligence.rs:378` —
   `ExecutionTelemetry::from_task_state`, where `test_case_id` must now be passed

Phase 2 must implement exactly §8 and §10 of the audit and nothing beyond them.