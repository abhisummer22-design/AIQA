# Phase 2 — Historical QA Memory Foundation: Report

**Date:** 2026-10-07
**Phase:** 2 of 6
**Status:** PASS
**Baseline before this phase:** 1716 passed / 0 failed / 2 ignored
**After this phase:** 1742 passed / 0 failed / 2 ignored (+26)

---

## 1. Objective

Create the persistent foundation required to retain useful historical QA
intelligence, reusing existing entities wherever possible and creating no second
memory system.

Implemented exactly the three items of `PHASE_1_AUDIT.md` §8, in §8's order:

| §8 item | Delivered |
|---|---|
| 8.1 durable test-case ↔ execution link | `tasks.test_case_id` + write path + backfill |
| 8.2 release as a first-class optional entity | `qa_releases` + nullable `runs.release_id` |
| 8.3 module as a graph node type | `quality_nodes` `node_type='module'` + `covers` edge |
| 8.4 memory derived, not duplicated | no new memory table; reads stay on `execution_telemetry` |
| 8.5 explicitly not proposed | no chain-of-thought, no secrets, no tenant table |

---

## 2. Existing functionality reused

Nothing here replaces an existing system.

| Reused | How |
|---|---|
| `execution_telemetry` | the history backbone; only **indexed** on `test_case_id` (0028), never duplicated |
| `test_cases.executed_run_id/executed_task_id` | the backward half of the link, left exactly as it was |
| `ExecutionPlan.test_case_id` | already existed and was dropped on the floor; now carried into `TaskDef` |
| `quality_nodes` / `quality_relations` | module is a vertex on the existing graph, **not** a new table |
| `upsert_quality_node` / `upsert_quality_relation` | reused verbatim for the `covers` edge |
| `insert_run` | kept its exact signature; `insert_run_for_release` is the new sibling |
| `Store::connect` → `sqlx::migrate!` | migrations applied by the existing mechanism, no new runner |
| `parse_ts` / `parse_ts_opt` / `now_rfc3339` | existing timestamp helpers |
| `LIST_*_MAX` bounding convention | respected (see §6) |

---

## 3. Files changed

### New migrations (both additive, neither destructive)

| File | Change |
|---|---|
| `migrations/0028_qa_memory_linkage.sql` | `ALTER TABLE tasks ADD COLUMN test_case_id TEXT`; 2 indexes |
| `migrations/0029_qa_releases.sql` | `CREATE TABLE qa_releases`; `ALTER TABLE runs ADD COLUMN release_id TEXT`; 3 indexes |

### New source

| File | Purpose |
|---|---|
| `src/qa_memory_tests.rs` | 25 tests — the phase's suite |
| `src/qa_memory_migration_tests.rs` | 1 test — the "existing database" migration case |

### Modified source

| File | Change |
|---|---|
| `src/store.rs` | `ReleaseRow` + `release_from_row`; `create_release`, `list_releases`, `runs_for_release`; `backfill_task_test_case_links`; `table_ddl`, `table_exists`; `insert_run_for_release`; `upsert_module_node`, `link_test_case_to_module`; tasks INSERT/SELECT carry `test_case_id`; runs INSERT carries `release_id` |
| `src/domain.rs` | `TaskDef.test_case_id`; `TaskState.test_case_id` |
| `src/scheduler.rs` | carries `test_case_id` into the in-memory task |
| `src/intelligence.rs` | passes the real case id into `ExecutionTelemetry::from_task_state` (was hardcoded `None`) |
| `src/api.rs` | handoff sets `test_case_id`; crawl task explicitly `None` |
| `src/execution_plan.rs` | `materialize_task_def` forwards `plan.test_case_id` (both branches) |
| `src/plan_validator.rs` | simulated / hand-declared tasks explicitly `None` |
| `src/lib.rs` | registers the two new test modules |

---

## 4. Database / schema changes

```
0028  ALTER TABLE tasks ADD COLUMN test_case_id TEXT
      CREATE INDEX idx_tasks_test_case       ON tasks(test_case_id, run_id)
      CREATE INDEX idx_telemetry_test_case   ON execution_telemetry(project_id, test_case_id)

0029  CREATE TABLE qa_releases (
          id TEXT PRIMARY KEY NOT NULL,
          project_id TEXT NOT NULL,
          label TEXT, version TEXT, notes TEXT,
          created_by TEXT, created_at TEXT NOT NULL, released_at TEXT)
      CREATE UNIQUE INDEX idx_releases_project_label  ON qa_releases(project_id, label)
      CREATE INDEX idx_releases_project_created      ON qa_releases(project_id, created_at DESC)
      ALTER TABLE runs ADD COLUMN release_id TEXT
      CREATE INDEX idx_runs_release                  ON runs(release_id)
```

**Destructive changes: none.** No `DROP`, no `DELETE`, no data rewrite. Verified against
a copy of the live database (§7).

---

## 5. APIs changed/added

**No HTTP endpoint was added in Phase 2.** That is deliberate: the master plan puts
retrieval surfaces in Phase 3. What Phase 2 establishes is the store-level capability
those endpoints will read.

Store surface added:

| Method | Purpose |
|---|---|
| `create_release(project_id, label, version, notes, created_by)` | record a release |
| `list_releases(project_id)` | newest first, project-scoped |
| `runs_for_release(release_id)` | every run of one release |
| `backfill_task_test_case_links(project_id)` | idempotent, project-scoped, refuses to guess |
| `insert_run_for_release(…, release_id)` | `insert_run` plus an optional release; cross-project refused |
| `upsert_module_node(project_id, module_ref, name)` | module vertex on the existing graph |
| `link_test_case_to_module(…)` | `covers` edge, idempotent |
| `table_ddl(table)` / `table_exists(table)` | bound introspection for migration tests |

---

## 6. Tests

### Added — 26

| Test | Covers |
|---|---|
| `the_test_case_link_migration_applies_to_a_fresh_database` | 0028 on empty DB |
| `the_release_migration_applies_to_a_fresh_database` | 0029 on empty DB |
| `migrations_preserve_rows_written_before_them` | rows survive, new column NULL |
| `a_task_built_from_a_test_case_carries_that_case_on_the_task_row` | forward link write |
| `a_task_that_is_not_a_case_execution_is_left_unlinked` | no false attribution |
| `a_malformed_case_id_is_stored_verbatim_but_never_reaches_telemetry_as_a_uuid` | row keeps input; telemetry refuses to link |
| `the_backfill_recovers_an_unambiguous_historical_link_and_is_idempotent` | recovery + idempotence |
| `the_backfill_maps_each_task_to_its_own_case_when_the_plan_says_so` | multi-task mapping |
| `the_backfill_refuses_a_plan_naming_a_case_id_that_is_not_a_uuid` | no garbage links |
| `a_pre_phase2_plan_naming_no_case_recovers_nothing` | the real historical shape |
| `the_backfill_skips_a_plan_where_one_task_has_no_case_id` | no partial filling |
| `the_backfill_does_not_cross_project_boundaries` | tenant isolation |
| `the_backfill_ignores_an_unreadable_plan` | garbage plan skipped |
| `a_release_round_trips_and_is_listed_newest_first` | CRUD + ordering |
| `a_release_label_is_unique_within_a_project_but_not_across_projects` | scoped uniqueness |
| `two_unlabelled_releases_coexist` | NULL uniqueness semantics |
| `a_run_can_be_associated_with_a_release_and_a_release_less_run_is_fine` | association + absence |
| `associating_a_run_with_another_projects_release_is_refused` | tenant isolation |
| `releases_are_listed_only_within_their_own_project` | tenant isolation |
| `a_partial_release_is_legitimate_not_an_error` | NULL-tolerance |
| `runs_for_an_unknown_release_are_empty_not_an_error` | empty-history honesty |
| `a_module_is_a_quality_graph_vertex_not_a_table` | no duplicate identity system |
| `linking_a_case_to_a_module_is_idempotent` | no duplicate vertices/edges |
| `modules_are_scoped_to_their_project` | tenant isolation |
| `a_blank_module_reference_is_refused` | no unnamed vertices |
| `an_existing_database_gains_the_new_schema_and_keeps_every_row` | the "existing database" case |

### Executed

```
cd services/orchestrator
cargo test                 → 1742 passed / 0 failed / 2 ignored   (exit 0)
cargo test --lib qa_memory → 25 passed / 0 failed
cargo test --lib qa_memory_migration → 1 passed
cargo clippy --all-targets → 0 errors
cargo fmt --check → clean for every file this phase created or edited
```

The migration test was run three ways: against a **fresh copy** of the live database
(passes), against an **already-migrated** copy (passes), and with **no copy supplied**
(skips cleanly with a message rather than passing vacuously).

---

## 7. Bugs found and fixed in this phase

| # | Defect | Root cause | Fix | Test proving it |
|---|---|---|---|---|
| B1 | `the_backfill_never_guesses_when_the_plan_is_ambiguous` FAILED | **the test's premise was wrong, not the code.** A plan in which every task carries its own `test_case_id` is fully *determined*; treating it as ambiguous would have discarded real recoverable history | Rewrote as `the_backfill_maps_each_task_to_its_own_case_when_the_plan_says_so`, and added the two genuinely unresolvable cases (no case id anywhere; a non-UUID case id) | 3 tests replacing 1 |
| B2 | migration test failed on a fresh copy, passed on a migrated one | `_sqlx_migrations` legitimately grows 27→29, and the row-count comparison included it — an order-dependent false failure | `_sqlx_migrations` excluded from the comparison, with a comment saying why | verified against a fresh copy |
| B3 | `Store::connect(&path.to_string_lossy())` in my own tests | `connect` takes `&Path` | tests corrected | build |
| B4 | `create_release` returned `sqlx::Error` where `anyhow::Error` was expected | tail expression returned the query result directly | `Ok(release_from_row(..))` | `a_release_round_trips…` |
| B5 | sqlx refused runtime-built SQL in `table_columns` | `pragma_table_info` needs a literal | replaced with `table_ddl` / `table_exists`, both fully bound — and a **stronger** migration assertion, since it reads the statement the migration actually ran | both migration tests |
| B6 | `list_quality_nodes`/`list_quality_relations_for_project` take `&str`, not `Option<&str>` | my tests assumed the wrong signature | tests corrected | module tests |
| B7 | 7 `TaskDef`/`TaskState` initializers missing the new field | added field | 4 given a **real** value (handoff, both `materialize_task_def` branches), 3 given an explicit reasoned `None` (crawl, simulated, hand-declared executor) | full suite green |

**No existing test was weakened, skipped, or disabled.** B1 in particular was fixed by
correcting a factually wrong assertion, and the phase gained test coverage as a result.

---

## 8. Regression result

```
before Phase 2:  1716 passed / 0 failed / 2 ignored
after  Phase 2:  1742 passed / 0 failed / 2 ignored
```

All pre-existing tests still pass. Specifically verified unaffected: run creation,
engine scheduling, execution, regression, adaptive intelligence, coverage, failure
intelligence, healing, RBAC probing, retention, and audit logging.

---

## 9. Data integrity

- No migration drops, rewrites, or moves a row.
- Verified against a **copy** of the live database (27 migrations, 40 tables,
  1036 audit rows, 19 telemetry rows): after migration, **every table's row count is
  unchanged**, `tasks.test_case_id` and `runs.release_id` exist, and
  `execution_telemetry.test_case_id` is still NULL in every historical row — the
  migration invents no links.
- The live database was **not** migrated in place by any test.

---

## 10. Project / tenant isolation

- `execution_telemetry` index leads with `project_id`.
- `create_release`, `list_releases`, `backfill_task_test_case_links` all take
  `project_id` and scope every statement by it.
- `insert_run_for_release` **refuses** a release belonging to another project.
- Module vertices are created through `upsert_quality_node`, whose identity is
  `(project_id, node_type, external_ref)`.
- Five tests assert cross-project behaviour explicitly.

---

## 11. Known limitations

1. **No endpoint exposes releases yet.** The store capability exists; Phase 3/5 wire
   the API. Nothing user-visible changed in this phase.
2. **Historical telemetry still has no case link** and cannot get one: pre-Phase-2
   plans never carried `test_case_id`, so `backfill_task_test_case_links` correctly
   recovers nothing from them. Only new executions are linked. This is honest, not a
   defect, but Phase 3 must not claim per-case history for pre-Phase-2 runs.
3. **`environment` is still the literal `'test'`.** Phase 2 did not change it; the audit
   §11.4 carries it into Phase 3.
4. **`execution_telemetry.evidence_refs` is still always `[]`**, and
   `assertion_result`/`http_status`/`selector`/`verification_outcome` are still never
   written. Phase 1 §4 listed these; Phase 2 deliberately did not expand into them.
5. `list_releases` is unbounded. The list is per-project releases, which is small by
   nature, but it has no `LIST_*_MAX` cap unlike the run and relation listings.
6. Two `store.rs` rustfmt diffs at lines 84 and 1012+ predate this phase (verified
   against `HEAD`); this phase did not add fmt debt to any file it touched.

---

## 12. Architecture impact

**Additive only.** Three new things exist that did not before:

1. one nullable column + two indexes (`tasks.test_case_id`)
2. one nullable column + one index (`runs.release_id`)
3. one new table + three indexes (`qa_releases`)

Plus one new **node type** on the existing quality graph (`module`) and one new
**edge type** (`covers`) — deliberately not a table.

The one behavioural change to existing code: `execution_telemetry.test_case_id` is now
populated for test-case executions, where it was previously always `None`. That is the
phase's entire point.

---

## 13. Phase 2 PASS gate

| Criterion | Status |
|---|---|
| Memory foundation works | yes — 25 store tests |
| Migrations pass (fresh) | yes — every `temp_store()` |
| Migrations pass (existing DB) | yes — against a copy of the live file |
| Existing functionality still works | yes — 1742/0/2 |
| Historical records persist correctly | yes — round-trip + live-copy migration |
| Tests pass | yes — 1742 passed |
| Regression passes | yes — no pre-existing test changed |
| No duplicate architecture | yes — `a_module_is_a_quality_graph_vertex_not_a_table` asserts it |
| Tenant isolation preserved | yes — 5 explicit tests |
| Data integrity verified | yes — row counts unchanged |
| Documentation updated | yes — this report |

**Phase 2: PASS.**