# Phase 2 — PASS

**Phase number:** 2 of 6
**Completion date:** 2026-10-07
**Status:** PASS

## Summary

The persistent foundation for QA Intelligence Memory, implemented exactly as
`PHASE_1_AUDIT.md` §8 specified and nothing beyond it.

The central problem was that `execution_telemetry.test_case_id` was NULL in **19 of 19**
rows the system had ever written: the column existed, but the forward link from an
execution to its test case had nowhere to live, so per-case history could only be keyed
on a free-text `task_name` — which is not an identity. That link is now durable, and
`ExecutionPlan.test_case_id`, which already existed and was being discarded, is finally
carried through.

Alongside it, the two entities Phase 1 found genuinely absent: **releases** (the only new
table in the whole plan) and **modules** — and modules were deliberately *not* given a
table, becoming a vertex on the quality graph that already exists.

## Files changed

| File | Kind |
|---|---|
| `services/orchestrator/migrations/0028_qa_memory_linkage.sql` | new — `tasks.test_case_id` + 2 indexes |
| `services/orchestrator/migrations/0029_qa_releases.sql` | new — `qa_releases` + `runs.release_id` + 3 indexes |
| `services/orchestrator/src/qa_memory_tests.rs` | new — 25 tests |
| `services/orchestrator/src/qa_memory_migration_tests.rs` | new — 1 test (existing-DB migration) |
| `services/orchestrator/src/store.rs` | modified — releases, backfill, module, `insert_run_for_release`, column plumbing |
| `services/orchestrator/src/domain.rs` | modified — `TaskDef.test_case_id`, `TaskState.test_case_id` |
| `services/orchestrator/src/scheduler.rs` | modified — carries the field |
| `services/orchestrator/src/intelligence.rs` | modified — real case id into telemetry |
| `services/orchestrator/src/api.rs` | modified — handoff links; crawl explicitly does not |
| `services/orchestrator/src/execution_plan.rs` | modified — forwards `plan.test_case_id` |
| `services/orchestrator/src/plan_validator.rs` | modified — simulated/hand-declared stay `None` |
| `services/orchestrator/src/lib.rs` | modified — registers the test modules |

## Tests run

```
cd services/orchestrator
cargo test                             # full suite
cargo test --lib qa_memory             # the phase suite
cargo test --lib qa_memory_migration   # existing-DB migration, 3 ways
cargo clippy --all-targets
cargo fmt --check
```

## Tests passed

| | |
|---|---|
| Full backend suite | **1742 passed / 0 failed / 2 ignored** (exit 0) |
| Baseline before this phase | 1716 passed / 0 failed / 2 ignored |
| New tests | 26 |
| clippy | 0 errors; no warning in any file this phase created |
| rustfmt | clean for every file created or edited here |

## Bugs found

7 — all in code this phase wrote, none in pre-existing code.

The one worth remembering: **B1**, where a failing test turned out to be the *test's*
fault, not the code's. It asserted that a plan whose tasks each name a different test
case was "ambiguous" — but that plan is fully determined, and treating it as ambiguous
would have discarded real recoverable history. Corrected to assert the mapping is
recovered, and two genuinely unresolvable cases (no case id at all; a non-UUID case id)
were added in its place. Net effect: more coverage than before.

**B2** was the same class of mistake: the migration test compared `_sqlx_migrations`,
which legitimately grows by one row per migration, so it failed on a fresh copy and
passed on an already-migrated one — an order-dependent false failure, now excluded with
a comment explaining why.

## Bugs fixed

All 7, within the phase. No pre-existing test was weakened, skipped or disabled.

## Regression result

Green. `1716 → 1742 passed`, 0 failed throughout. Explicitly re-verified as unaffected:
run creation, engine scheduling, execution, regression, adaptive intelligence, coverage,
failure intelligence, healing, RBAC probing, retention, audit logging.

## Data integrity

Verified against a **copy** of the live database (27 migrations, 40 tables, 1036 audit
rows, 19 telemetry rows): after migration every table's row count is unchanged, the new
columns exist, and `execution_telemetry.test_case_id` is still NULL in every historical
row — the migration invents no links. The live file was never migrated in place.

## Known limitations

1. **No endpoint exposes releases yet** — store capability only; Phase 3/5 wires the API.
2. **Pre-Phase-2 telemetry can never gain a case link**, because those plans never
   carried one. Phase 3 must not claim per-case history for those runs.
3. `environment` is still the literal `'test'` (audit §11.4 → Phase 3).
4. `evidence_refs` still always `[]`; `assertion_result`, `http_status`, `selector` and
   `verification_outcome` still never written (audit §4; deliberately out of Phase 2 scope).
5. `list_releases` has no `LIST_*_MAX` cap, unlike the run and relation listings.
6. Two pre-existing `store.rs` rustfmt diffs (lines 84, 1012+) were verified against
   `HEAD` and left alone.

## Next phase prerequisites

Phase 3 (QA Intelligence Retrieval) must read, before changing code:

1. `docs/qa-intelligence-memory/PHASE_1_AUDIT.md` — §7 duplication risks, §4 missing data
2. `docs/qa-intelligence-memory/PHASE_2_REPORT.md` — this phase's limits, especially §11.2
3. `docs/qa-intelligence-memory/PHASE_2_PASS.md` — this file
4. `services/orchestrator/migrations/0028_qa_memory_linkage.sql`, `0029_qa_releases.sql`
5. `services/orchestrator/src/store.rs` — `backfill_task_test_case_links`,
   `list_releases`, `runs_for_release`, `list_execution_telemetry`, `upsert_quality_node`
6. `services/orchestrator/src/operations.rs` — `RecurrenceState`,
   `get_investigation`, `get_environment_matrix`, `get_investigation_timeline`
7. `services/orchestrator/src/adaptive_intelligence.rs` — `categorize_failure`,
   `evaluate_flakiness`, `calculate_trend`
8. The 13 existing `/intelligence/*` routes and `/findings` in `api.rs` — Phase 3 extends
   them and **must not** add a fourth overlapping failure view

Phase 3 rules that bind it: results must be project-scoped, deterministic, evidence-backed,
time-aware, explainable and bounded; `INSUFFICIENT_DATA` is the correct answer on empty
history; and no historical claim may outrun the stored evidence.