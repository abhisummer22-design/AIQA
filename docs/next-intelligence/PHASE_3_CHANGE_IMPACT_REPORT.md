# Phase 3 — Change Impact Intelligence + Smart Regression

**Phase 3 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`. Backend only. PASS.**

Predecessors: [`PHASE_2_TRACEABILITY_REPORT.md`](./PHASE_2_TRACEABILITY_REPORT.md),
[`PHASE_2_PASS.md`](./PHASE_2_PASS.md). Phase 1/2 functionality was not restarted or
rewritten; the only changes to their files are recorded in §6.

---

## 1. Audit: why `change_impacts` had zero writers

Confirmed by searching all 81 source files. The honest answer is that **"unfinished
feature" was the wrong diagnosis**, and writing to the table would not have produced a
single *true* record.

| Question | Finding |
|---|---|
| Writers | **0.** No `INSERT INTO change_impacts`, no `UPDATE` anywhere |
| Readers | **0.** Exactly one occurrence in the whole backend — `store.rs:1831`, a comment listing tables a project delete cascades |
| Tests | **0** |
| Module / API | none |
| Git integration | **none.** `git2`, `commit_sha`, `rev-parse`, `diff` as a VCS concept: **0 occurrences** in non-test code |
| Requirement-change detection | none |
| API/schema change detection | none |
| Regression selection | **none** — `regression.rs` replays a whole `suite_id` with no selection layer |

**Root cause.** `change_impacts` is a schema stub from migration 0002, and it was never
more than that. But the deeper cause is that **there was no source of change data at all**.
The plan lists five input sources (git, API contract, requirement, configuration,
dependency changes); this backend implements *one* of them (requirements exist as a
first-class stored entity). No amount of writing to the table would have produced a
truthful impact record, because there was nothing truthful to record. Phase 3 therefore
declares changes through an API and resolves them against the graph.

### 1.1 The table was **not** sufficient — verified field by field

The instruction was explicit: *"Do not assume the table is sufficient. Verify its fields,
keys, project scope, and intended lifecycle."*

Original schema (migration 0002):

```
id                     TEXT PRIMARY KEY
change_ref             TEXT NOT NULL
change_type            TEXT NOT NULL
affected_node_id       TEXT NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE
selected_test_node_id  TEXT REFERENCES quality_nodes(id)
rationale              TEXT
computed_at            TEXT NOT NULL
```

Two real defects, both hidden by the table having zero rows:

1. **No `project_id`.** Tenant scope was reachable only by joining through
   `quality_nodes`. That works, but it made this the one table where a forgotten `WHERE` is
   a **cross-tenant leak** rather than an empty result.
2. **No unique key.** Re-computing impact for one change would insert a duplicate row every
   time, so repeated analysis would inflate the record of a single change instead of
   confirming it.

The 1:1:1 shape (`one affected_node_id`, `one selected_test_node_id`) is **not** a defect.
Read as "change X affects node Y, and test Z was selected because of it", the triple is
coherent — and that reading is why the unique key is the triple, not the pair. It was
**not** redesigned.

## 2. Migration: 0031 — additive, not destructive

```sql
ALTER TABLE change_impacts ADD COLUMN project_id TEXT;
CREATE INDEX        idx_change_impacts_project ON change_impacts(project_id, computed_at DESC);
CREATE UNIQUE INDEX uq_change_impacts_record ON change_impacts(
    project_id, change_ref, affected_node_id, COALESCE(selected_test_node_id, ''));
```

* **`ADD COLUMN`** leaves existing rows intact (NULL). There are none, but NULL is tolerated
  deliberately so a pre-migration row stays distinguishable, and the single writer never
  produces one.
* **`COALESCE` in the unique index** because SQLite treats NULLs as distinct — a plain
  index on the raw column would let unlimited NULL-bearing duplicates through, and
  "affects a node, selects nothing" is a legitimate record.
* Nothing is dropped or rewritten. **Not a destructive migration**, so the plan's stop
  condition is not tripped.
* `phase_three_added_one_additive_migration` pins the count at 31 and asserts the columns
  and the index exist.

---

## 3. Implementation

### 3.1 Where a change comes from

A change is **declared by a caller** (`change_ref` + typed targets) and *resolved* against
the graph. The declaration is the evidence a change occurred; the impact is derived.

### 3.2 What the schema can actually ground

| Target kind | Resolvable | Basis |
|---|---|---|
| `requirement` | **yes** | `requirement` vertex keyed on the requirement key; `validates` edges run case → requirement |
| `test_case` | **yes** | the case vertex is keyed on `test_cases.id` |
| `failure_cluster` | **yes** | cluster vertex keyed on the telemetry fingerprint; `fails_with` edges run case → cluster |
| `module` | vertex yes, **impact no** | no `covers` edge is ever written (Phase 2 limitation) → `INSUFFICIENT_DATA` |
| `api_contract`, `page`, `code_file`, `configuration`, `dependency` | **no** | no vertex of any such type exists anywhere in the schema |
| anything else | **no** | refused by name |

Each refusal carries `marker: "INSUFFICIENT_DATA"` and a **stated reason** — e.g. *"the
backend has no git integration, no diff parser and no file-to-test mapping, so a code file
cannot be resolved to anything"*. Nothing is matched by substring.

### 3.3 Impact algorithm — stored relations only

From a resolved vertex, in this order:

1. **The vertex itself** if it is a case vertex.
2. **Reverse edges** into it — `validates`, `fails_with`, `covers`. These are the three
   relation types the schema already has, all pointing *from* a case, so reading them as
   reverse edges uses the stored direction rather than inventing one.
3. **One hop of recorded dependency** — cases that are `fails_with`-linked to a cluster a
   directly-reached case also failed with. Both clusters and both edges are real rows.

Matching is **exact** on `external_ref` within the project. A requirement key that merely
resembles a real one does not match — pinned by `a_near_miss_requirement_key_does_not_match`.

### 3.4 Smart regression — the existing engine, narrowed

The only thing Phase 3 added to the regression engine is
`regression_runner::Scope::restricted_to(selected, mandatory)`. Everything about deciding
*what can be replayed and how* — `scope()`'s two status queries, `replay_of`, the plan
lookup, the "another case was last run as the same task" rule — is untouched and still owns
that decision. **No second regression engine.**

Selection tiers, a total order rather than a score, so the same inputs always produce the
same selection:

| Tier | Enters by |
|---|---|
| `directly_impacted` | the change names the case, or a requirement/cluster/module it is linked to by a stored relation |
| `related_failure` | shares a recorded failure signature with a directly impacted case |
| `safety_floor` | **has recorded failure evidence** — retained regardless of the change |
| `high_risk` | recorded risk ≥ `HIGH_RISK_THRESHOLD` (0.7, a stated number, not a relative judgement) |

Two properties that matter more than the tiering:

* **Nothing disappears silently.** Every case selection drops is moved into the engine's
  own `SkippedCase` list with a reason. A run that executes 12 of 60 cases can account for
  the other 48.
* **The floor cannot be subtracted.** `restricted_to` treats `mandatory` as *added to* the
  selection and never intersected with it. `an_empty_selection_still_retains_the_mandatory_set`
  asserts an empty impact selection cannot empty a regression run.

---

## 4. Module decision

**`test_cases.module_ref` was NOT added.** The instruction required re-inspecting whether
the repository now has a reliable module source. It does not:

| Candidate | Finding |
|---|---|
| `test_cases` columns | `source_document` — a document, not a module. No module field |
| `TaskDef` | no module field (verified against the struct, not assumed) |
| `RunPlan` | no path, no module |
| `TaskDef.params` | free-form JSON; no writer populates a module key |
| the only `source_file` in the codebase | `CreateCustomSkillBody.source_file`, for importing a `SKILL.md` — unrelated |

So module-targeted change impact returns `INSUFFICIENT_DATA` with the reason *"a module
vertex exists, but no `covers` edge is ever written"*, and impact analysis works through
requirement, test-case and failure-cluster targets instead. No heuristic was introduced.

**If you later want it closed**, the decision is: add a nullable `test_cases.module_ref`,
populated by whichever process actually knows the mapping (for a code-linked test, the file
or package its step exercises; for a generated test, the source module). Link it to the
existing `quality_nodes(type='module')` vertex. **Do not create a `modules` table** —
`quality_nodes` is already the module identity, which was the deliberate Phase 2 choice over
a fourth parallel identity system.

---

## 5. APIs

One capability, two verbs:

```
POST /api/v1/projects/:project_id/change-impact   declare + resolve + persist
GET  /api/v1/projects/:project_id/change-impact   the project's impact history
```

`POST` returns **201** when it wrote records, **200** when it only confirmed what was
already recorded — so a caller can tell a new finding from a re-run. An ungrounded target
returns **200 with `recorded: 0`**, because a refused target must not manufacture a row just
to look productive.

Guarded by `auth::require_role(Operator)` *and* `require_scoped_project` — declaring a
change writes durable records, so a viewer must not be able to do it.

---

## 6. Files changed

| File | Change |
|---|---|
| `migrations/0031_change_impacts_scope.sql` | **new** — `project_id` + index + unique key |
| `change_impact.rs` | **new** — resolution, impact, selection, persistence |
| `change_impact_tests.rs` | **new** — 16 tests |
| `store.rs` | `ChangeImpactRow`; `insert_change_impact`, `list_change_impacts`, `count_change_impacts` |
| `regression_runner.rs` | `Scope::restricted_to` — the entire Phase 3 addition to the engine |
| `api.rs` | 2 routes + handlers + body type |
| `lib.rs` | module registration |
| `trace_tests.rs`, `trace_chain_tests.rs` | migration-count guard 30 → 31 (**the guards working**, §7.1) |

### 6.1 Re-applied Phase 1 store edits — recorded honestly

`store.rs` was corrupted by a scripted edit during this phase and was reverted to HEAD and
rebuilt from verified anchors. **All seven earlier edits were re-applied and the full suite
returned to green (1845) before any Phase 3 code was written.** The Phase 1/2 behaviour is
byte-equivalent; the two `list_test_cases_for_requirement` edits had to be re-applied
against the current text because the committed baseline has those SQL line-continuation
backslashes missing (SQL tolerates their absence, which is why 1845 tests passed both
before and after).

---

## 7. Bugs found and fixed

### 7.1 The Phase 1 and Phase 2 migration-count guards fired — as designed

Both pinned 30 and declared "Phase N added no migration". Migration 0031 made **both fail**,
which is exactly what they exist for. They were updated to 31 with a comment recording that
the count moved exactly once, for one additive migration. Recorded because it is easy to
mistake a failing guard for a problem to silence.

### 7.2 `store.rs` corruption from scripted editing — self-inflicted, fixed

A Python insert placed a block inside another struct, and the follow-up deletions removed
part of `HealingProposalRow` and `get_failure_cluster_by_fingerprint`. Caught by the
compiler, fixed by reverting to HEAD and re-applying all edits with **asserted anchors**
(`count(anchor) == 1`), which is the guard that was missing the first time. `store.rs` was
green at 1845 before Phase 3 code was written.

### 7.3 Two wrong test premises in Phase 3's own suite — corrected, not accommodated

* **HTTP status.** Asserted `201` for an ungrounded change. It returns `200`, correctly:
  nothing resolved, so nothing was written. Writing a record just to return `201` would be
  manufacturing a result.
* **`quality_nodes.id` decoded as `Uuid`.** The column is TEXT; the test fetched it with a
  `Uuid` extractor.

Both failed loudly. Neither was weakened to pass.

### 7.4 A dead loop left in `safety_floor`

The doc comment claimed *"a case whose failure produced a bug draft is not droppable"* while
the code looped over drafts and discarded them (`let _ = d;`). Implemented properly:
draft → cluster → fingerprint → the cases that failed with it, all stored keys.

---

## 8. Tests

**17 new**: 16 in `change_impact_tests.rs` + 1 HTTP end-to-end in `api_tests.rs`.

| Concern | Test |
|---|---|
| direct impact | `a_changed_requirement_reaches_its_linked_cases`, `a_changed_test_case_reaches_itself` |
| indirect impact | `a_case_sharing_a_recorded_failure_is_reached_indirectly` |
| unrelated change | `an_unrelated_change_yields_no_impact` |
| empty history | `an_empty_project_yields_an_empty_selection` |
| missing links | `every_unresolvable_change_kind_is_refused_with_a_reason`, `a_malformed_test_case_target_is_refused` |
| **no false links from heuristics** | `a_near_miss_requirement_key_does_not_match` — 4 near-misses must not match |
| multiple projects | `change_impact_does_not_cross_projects` |
| duplicate records | `impact_records_are_written_once_per_change` — 2 rows, re-run adds 0 |
| negative results recorded | `a_resolved_node_with_no_linked_test_is_recorded_as_a_negative_result` |
| **mandatory retention** | `a_failing_case_is_retained_even_when_the_change_does_not_touch_it`, `a_high_risk_case_is_retained_on_its_own_risk`, `an_empty_selection_still_retains_the_mandatory_set` |
| existing engine reused | `restricted_to_narrows_the_existing_scope_without_losing_accounting` |
| migration | `phase_three_added_one_additive_migration` |
| HTTP e2e | `phase3_change_impact_endpoint_persists_records_and_refuses_guesses` |

Every row is produced by a real writer: `insert_test_case`, `link_test_case_to_requirement`,
`ingest_run`, `upsert_risk_assessment`, `upsert_quality_node`. **No test hand-writes a
`quality_relations` row or a fingerprint** — a fixture-shaped graph would let the resolver
pass on links production cannot make.

### 8.1 The safety-floor guard is proven, not asserted

Reverted the floor guarantee (making selection shrink to impact alone) and re-ran:
**`a_failing_case_is_retained_even_when_the_change_does_not_touch_it` failed**, with the
fragile case absent from the selection. Restored.

### 8.2 Regression

| Run | Before Phase 3 | After |
|---|---|---|
| Backend suite | 1845 passed / 0 failed / 2 ignored | **1862 passed / 0 failed / 2 ignored** (+17) |
| `cargo build` | clean | clean |
| `cargo clippy --all-targets` | 0 errors | 0 errors, no warning in any new file |
| `rustfmt --check` on the 3 Phase 3 files | — | clean (`regression_runner.rs` was fmt-clean at HEAD and was kept that way) |
| Frontend | 1605 / 79 files | unchanged — no frontend file touched |

Zero regressions. No pre-existing test weakened, skipped or deleted.

---

## 9. Limitations

**L1 — `covers` / module-targeted change impact unavailable.** §4. Needs the schema
decision.

**L2 — a change is declared, not ingested.** No git, no contract diff, no config watcher.
Code-file, API-contract, page, configuration and dependency changes cannot be analysed at
all. This is a **capability gap, not a silent failure**: each returns `INSUFFICIENT_DATA`
with a stated reason. Closing it needs VCS ingestion, which is a separate piece of work
with its own trust and security questions.

**L3 — the safety floor is derived, not declared.** There is **no mandatory-test concept
anywhere in the codebase** (verified: `mandatory` / `safety_test` / `smoke` hits are all
unrelated AI-generation prompts; `test_cases.tags` has no writer convention). So the floor is
"has recorded failure evidence", which is the strongest *derivable* proxy. A real mandatory
flag would be a schema decision — most plausibly a `test_cases.mandatory` boolean or a
reserved tag.

**L4 — impact resolution is one hop of dependency.** `related_failure` covers cases sharing
a failure signature. There is no dependency graph between test cases (`TaskDef.prerequisites`
exists per *run*, not per case), so "depends on" is not derivable today.

**L5 — the 0031 fixes apply only to new rows.** Pre-existing `change_impacts` rows would
have `project_id = NULL` and therefore fall outside the unique key. There are none in any
deployed database, so this is theoretical; it is recorded because it is the one case where
the additive migration changes what a NULL means.

**L6 — no frontend surface.** Backend-only; the frontend's 1605 tests are untouched.

**L7 — the live database is unchanged.** Never migrated in place: 27 of 30 migrations,
`change_impacts` 0 rows. The Phase 3 migration will run on first start.

**L8 — no E2E CI job.** Pre-existing: `ci.yml` has 3 jobs, none E2E.

**L9 — risk scores come from the existing heuristic.** `list_test_intelligence`'s `score`
column, whose own `inputs` JSON records *"heuristic, unweighted by any human-override
feedback loop yet"*. Phase 3 reads it as-is and does not improve it; the `high_risk` tier
inherits its accuracy. Threshold is stated (0.7) so the tier can be audited.

---

## 10. PASS gate

| Criterion | Status | Evidence |
|---|---|---|
| Real change-impact records can be produced | **PASS** | `impact_records_are_written_once_per_change`, `phase3_change_impact_endpoint_persists_records_and_refuses_guesses` (asserts the row in SQLite) |
| Impacted tests are explainable | **PASS** | every `ImpactedTest` carries `tier`, `priority` and `via` naming the stored edge; asserted in the direct-impact and HTTP tests |
| Uses existing regression infrastructure | **PASS** | `Scope::restricted_to` is the entire addition; `scope`/`replay_of`/`RegressionRunRow` untouched |
| No duplicate intelligence engine | **PASS** | no new table, engine, graph vocabulary or relation type; 1 additive migration adding a column and two indexes |
| No false links without evidence | **PASS** | 7 unresolvable kinds refused with reasons; 4 near-miss keys rejected; `unrelated` and `empty` cases yield nothing |
| Project isolation passes | **PASS** | `change_impact_does_not_cross_projects` + the HTTP test's cross-project history read |
| Tests pass | **PASS** | 17/17 |
| Regression passes | **PASS** | 1862 / 0 failed |

### Honest caveats

1. **`covers` is absent by design** (L1), so module-targeted changes return
   `INSUFFICIENT_DATA`. Impact works through requirement, test-case and failure-cluster
   targets.
2. **Four of the plan's five change sources are unimplemented** (L2). The gate item "real
   change-impact records can be produced" is met for the sources the repository can actually
   ground; the rest report insufficient data rather than guessing.
3. **The safety floor is derived** (L3). The guarantee is structural — the floor is added to
   the selection and never intersected with it — but what counts as "mandatory" is a
   derivation, not a declared fact.
4. **Persistence is proven in fresh migrated databases driven through real writers**, not in
   the live database, which is untouched (L7).

**Phase 3 = PASS**, with L1–L3 carried forward as known gaps.

---

**Phase 4 not started. Nothing committed. Nothing pushed.**