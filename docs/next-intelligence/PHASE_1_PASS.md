# Phase 1 — PASS

**Phase 1 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`: Trace Graph Completion.**
Full evidence in [`PHASE_1_TRACE_GRAPH_REPORT.md`](./PHASE_1_TRACE_GRAPH_REPORT.md).

---

## Result

**PASS.** Every gate criterion below is met by evidence that can be re-run.

| Criterion | Status | Where proven |
|---|---|---|
| Actual relation rows are persisted | PASS | `requirement_edge_persists_to_the_canonical_node_and_is_readable`, `module_edge_persists_and_joins_the_same_vertex_as_the_requirement_edge`, `failure_edge_uses_the_stable_case_id_when_the_telemetry_row_carries_one`, `phase1_trace_endpoint_returns_the_chain_actually_persisted` (asserts the row in SQLite, not just in the response) |
| Stable IDs are used | PASS | `test_cases.id`, `execution_telemetry.test_case_id`, `tasks.test_case_id`, `failure_clusters.id`, `runs.release_id`, `healing_proposals.id`. `task_name` survives only as a `TASK_NODE_TYPE` key, behind `unambiguous_task_ref` |
| Trace retrieval works | PASS | `the_walk_returns_the_chain_that_is_actually_persisted`; `GET /api/v1/projects/:project_id/trace` |
| No fabricated links | PASS | 8 negative tests — see §"Negative tests" below |
| Project isolation passes | PASS | `a_project_cannot_trace_another_projects_case`, `a_task_resolves_its_case_id_and_respects_the_project`, the HTTP e2e, and the §7.2 leak that isolation testing caught |
| All new tests pass | PASS | 22 / 22 |
| Relevant existing regression passes | PASS | 1831 passed / 0 failed / 2 ignored (baseline 1809 / 0 / 2) |
| No duplicate subsystem introduced | PASS | no table, no store, no engine. One reader over the existing graph; one new lookup (`get_failure_cluster_by_fingerprint`); one new getter (`get_task_test_case_id`) |
| Migration safety | PASS (N/A) | no migration created; `phase_one_introduces_no_new_migration` pins the count at 30 and asserts all 7 required columns exist |

---

## What the phase actually fixed

`quality_relations` held **0 rows** in the live database. Not one cause — three
independent code defects that had to be true simultaneously, each verified by reading
every writer, reader and call site:

1. **Two `node_type` values for one concept.** `validates` wrote `"test_case"`;
   `fails_with` wrote `"test"`. `quality_nodes` identity is
   `(project_id, node_type, external_ref)`, so a case's requirement edge and its
   execution's failure edge could never join. `qa_memory.rs:527` already documented the
   intended design — `"test"` keyed on the case id — so the writer was contradicting its
   own module's documented intent.
2. **A name used as an identity**, in four writers.
3. **`covers` had no production caller** — the function name appeared exactly once in
   non-test code: its own declaration.

Ruled out and recorded so they are not re-investigated: not a migration problem, not a
transaction problem, not a foreign-key problem, and **not** "the code path was never
reached" — the `validates` writer *was* reached and returned early, correctly, because
both live test cases carry no requirement reference.

---

## Negative tests — the ones that matter

A suite that only exercises happy paths cannot detect fabricated traceability, which is
this phase's exact failure mode. These assert absence:

| Test | Asserts |
|---|---|
| `a_case_with_no_requirement_writes_no_edge_at_all` | no requirement ⇒ no requirement vertex, no edge |
| `a_passing_execution_writes_no_failure_edge` | a pass has no failure, and none is invented |
| `failure_edge_without_a_case_id_uses_the_task_namespace` | no case id ⇒ `TASK_NODE_TYPE`, never a case vertex |
| `a_task_named_after_a_uuid_cannot_claim_that_case_vertex` | a UUID-shaped name cannot occupy the id namespace |
| `a_fingerprint_with_no_cluster_row_is_reported_as_incomplete` | a fingerprint without a cluster row names the absence |
| `a_case_with_no_history_names_every_missing_hop` | all 8 hops named missing, never a short chain presented as complete |
| `an_empty_project_reports_an_empty_index` | no placeholder summary |
| `the_index_lists_real_cases_and_their_gaps` | a case with no graph vertex is not fabricated into the index |
| `test_28` (rewritten) | the dispatcher must persist its own vertex, and no case vertex from a task name |

---

## Bugs found

**4.** Two are pre-existing production defects, one was introduced by this phase and
caught by its own test before it could ship, and one was a wrong test premise that failed
loudly instead of passing quietly.

1. **Panic in the failure-graph writer** (pre-existing). `&fingerprint[..12]` is a *byte*
   slice; any fingerprint under 12 bytes panicked the entire telemetry ingest. Never
   observed because every produced fingerprint is a 64-char SHA-256 digest. Fixed with
   `chars().take(12)`; guarded across 2/10/21/24-byte and multi-byte cases. The guard
   reproduces the original panic message before the fix.
2. **Cross-tenant leak in the new trace reader** (introduced here, caught here).
   `Store::get_test_case(id)` takes no tenant argument, so the walk returned another
   tenant's `requirement_reference`. Fixed with `TraceReader::case_in_project`.
3. **A test asserting against a vertex it created itself** (pre-existing).
   `test_28` built the node under test with its own `upsert` call.
4. **Wrong test premise** — `runs.release_id` asserted on the wrong table. It failed
   rather than passing silently, which was the correct outcome.

---

## Constraints honoured

| Constraint | Status |
|---|---|
| No new QA memory database | honoured — no table added |
| No second bug / risk / regression / trace system | honoured — one reader over the existing graph |
| Reuse existing IDs, tables, models | honoured — the walk reads 10 pre-existing tables |
| No destructive migration | honoured — **no** migration |
| No invented relationships | honoured — see the 9 negative tests |
| No heuristic guessing to populate links | honoured — the reason `covers` stays unwritten (§below) |
| Project/tenant isolation preserved | honoured, and one leak fixed |
| Action AST, executor, scheduler, healing, regression, adaptive and failure intelligence architecture preserved | honoured — no change to any of them beyond the graph vertex each already wrote |
| Fixtures never presented as production history | honoured — §Limitations |

---

## Limitations carried forward

**Not closed by this phase. Stated, not asserted away.**

1. **`covers` is never written in production — deliberately.** Nothing in the domain
   model names a module: `TestCaseRow` has `source_document` (a document), `TaskDef` has
   no module field, `RunPlan` has no path, and the only `source_file` in the codebase
   belongs to custom-skill import. `link_case_to_module` is now correct, stable-id-keyed
   and tested, but adding a caller would mean inventing a document→module mapping — a
   heuristic link wearing the costume of traceability. The walk reports `module` in
   `incomplete` instead. **Closing it needs a real module identity, which is a schema
   decision for you.**
2. **Risk/value history is stranded** by the identity change for case-backed tasks.
   Nothing deleted, no score wrong; the history simply is not carried across. Fixing it
   needs a name→case backfill, and the old mapping does not exist.
3. **`update_test_intelligence` still scores history by task name.** Project-scoped, so
   no cross-tenant leak, but same-named tasks within one tenant share a failure rate.
   Changing it alters risk scores — that is risk-phase work.
4. **Legacy `"test_case"` edges** are read but remain a separate vertex class, so a case
   with only legacy edges has two vertices. The alternative is a destructive migration.
5. **No frontend surface.** Backend-only endpoint; no UI or TypeScript, so the 1605
   frontend tests / 79 files are untouched and unaffected.
6. **No E2E CI job** — pre-existing; `ci.yml` has 3 jobs, none E2E.
7. **The live database is unchanged and still empty.** Never migrated in place: still 27
   of 30 migrations, `quality_relations = 0`. Its 2 test cases carry no requirement
   reference and its 19 telemetry rows carry no `test_case_id`, so **migrating alone
   would not produce a single edge** from existing data. Only new activity writes: a case
   created with a requirement reference, or a failure whose telemetry row carries a case
   id.

---

## The distinction the plan required

Proven in a **fresh migrated database driven through real production writers** — not in
the live database, and not by a mock:

| Claim | Status |
|---|---|
| Capability exists in code | **true** — verified by build, clippy, and reading every call site |
| Capability is exercised by real persisted data in a migrated database | **true** — 22 tests asserting rows in SQLite, one over real HTTP |
| Capability is exercised by production history | **false, and not claimed** — the live DB is empty and unchanged; see limitation 7 |

Nothing in this phase converts a fixture into a claim about production history.

---

## Discipline

* No test was weakened, skipped or deleted to reach green.
* Every failure encountered was fixed at the root cause. Two of the four failures during
  this phase were wrong premises in tests I had just written; both were corrected rather
  than accommodated.
* The Phase 0 count correction went **against the numbers in the instruction** — the
  audit's own per-feature sections give `EXISTS 2 / MISSING 7`, not `4 / 5` — because
  writing the instructed numbers would have left the table disagreeing with the sections
  it summarises. Reasoning in report §11. **No application logic was changed for it.**
* Formatting churn from `cargo fmt --all` across 16 unrelated files was reverted.

---

**Phase 2 not started. Nothing committed. Nothing pushed.**