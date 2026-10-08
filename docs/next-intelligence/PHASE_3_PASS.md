# Phase 3 — PASS

**Phase 3 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`: Change Impact Intelligence + Smart
Regression.** Full evidence in
[`PHASE_3_CHANGE_IMPACT_REPORT.md`](./PHASE_3_CHANGE_IMPACT_REPORT.md).

Predecessors: [`PHASE_2_PASS.md`](./PHASE_2_PASS.md) — Phase 1/2 not restarted or rewritten.

---

## Result

**PASS.**

| Criterion | Status | Where proven |
|---|---|---|
| Real change-impact records can be produced | PASS | `impact_records_are_written_once_per_change`; the HTTP test asserts the row in SQLite |
| Impacted tests are explainable | PASS | every hop carries `tier`, `priority` and `via` naming the stored edge |
| Uses existing regression infrastructure | PASS | `Scope::restricted_to` is the entire addition; `scope`/`replay_of` untouched |
| No duplicate intelligence engine | PASS | no new table, engine, graph vocabulary or relation type |
| No false links without evidence | PASS | 7 unresolvable kinds refused; 4 near-miss keys rejected |
| Project isolation passes | PASS | `change_impact_does_not_cross_projects` + cross-project history read |
| Tests pass | PASS | 17/17 |
| Regression passes | PASS | 1862 / 0 failed / 2 ignored |

---

## The audit finding

`change_impacts` had **0 writers, 0 readers, 0 tests, no module, no API** — and exactly one
occurrence in the entire backend: a comment at `store.rs:1831` listing cascade tables.

But **"unfinished feature" was the wrong diagnosis.** Writing to the table would not have
produced a single *true* record, because **there was no source of change data at all**:
the backend has no `git2`, no commit/branch concept, no diff parser, no manifest reader.
Of the plan's five change sources, only *requirements* exist as a stored entity.

So a change is now **declared through an API and resolved against the graph**. The
declaration is the evidence a change occurred; the impact is derived, never guessed.

### The table was not sufficient — verified field by field

Two real defects, both hidden by the table having zero rows:

1. **No `project_id`.** Tenant scope was reachable only by joining through `quality_nodes`,
   making this the one table where a forgotten `WHERE` is a **cross-tenant leak** rather
   than an empty result.
2. **No unique key.** Re-computing impact would insert a duplicate every time, so repeated
   analysis would inflate a single change's record instead of confirming it.

The 1:1:1 shape is **not** a defect — read as *"change X affects node Y, and test Z was
selected because of it"*, the triple is coherent. It was not redesigned.

**Migration 0031** adds `project_id`, an index, and a unique index over
`(project_id, change_ref, affected_node_id, COALESCE(selected_test_node_id,''))`.
**Additive only** — `ADD COLUMN` plus two indexes, nothing dropped or rewritten. The
`COALESCE` is required because SQLite treats NULLs as distinct, and "affects a node, selects
nothing" is a legitimate record.

---

## What the schema can actually ground

| Target kind | Resolvable | Basis |
|---|---|---|
| `requirement` | **yes** | `validates` edges, case → requirement |
| `test_case` | **yes** | the case vertex is keyed on `test_cases.id` |
| `failure_cluster` | **yes** | `fails_with` edges, case → cluster |
| `module` | vertex yes, **impact no** | no `covers` edge is ever written |
| `api_contract`, `page`, `code_file`, `configuration`, `dependency` | **no** | no vertex of any such type exists |

Each refusal carries `marker: "INSUFFICIENT_DATA"` and a **stated reason** — e.g. *"the
backend has no git integration, no diff parser and no file-to-test mapping"*. Matching is
**exact** on `external_ref`; four near-miss requirement keys are proven not to match.

---

## Smart regression

The **entire** Phase 3 addition to the regression engine is
`Scope::restricted_to(selected, mandatory)`. Everything deciding *what can be replayed and
how* — `scope()`'s status queries, `replay_of`, the plan lookup, the same-task rule — is
untouched.

Two properties matter more than the tiering:

* **The floor cannot be subtracted.** `mandatory` is *added to* the selection, never
  intersected with it. Proven: reverting the floor guarantee fails
  `a_failing_case_is_retained_even_when_the_change_does_not_touch_it`.
* **Nothing disappears silently.** Every dropped case moves into the engine's own
  `SkippedCase` list with a reason. A run executing 12 of 60 cases can account for the
  other 48.

Tiers are a total order, not a score: `directly_impacted` → `related_failure` →
`safety_floor` → `high_risk` (threshold 0.7, stated so it can be audited).

---

## Module decision

**`test_cases.module_ref` was NOT added.** Re-inspected per the instruction:
`test_cases.source_document` is a document; `TaskDef` has no module field; `RunPlan` has no
path; the only `source_file` in the codebase belongs to custom-skill import.

Module-targeted impact returns `INSUFFICIENT_DATA` with the reason, and impact works through
requirement, test-case and failure-cluster targets. No heuristic was introduced.

**To close it later:** add nullable `test_cases.module_ref`, link it to the existing
`quality_nodes(type='module')`, and **do not create a `modules` table** — `quality_nodes` is
already the module identity.

---

## Bugs found

**4.** Two were mine, and both are worth naming:

* **The Phase 1 and Phase 2 migration-count guards fired** when 0031 landed, because both
  pinned 30 and declared "Phase N added no migration". That is exactly what they exist for.
  Updated to 31 with a comment recording that the count moved once, for one additive
  migration. Recorded because a failing guard is easy to mistake for a problem to silence.
* **I corrupted `store.rs` with a scripted edit** — a block landed inside another struct and
  the follow-up deletions removed part of `HealingProposalRow`. Caught by the compiler, fixed
  by reverting to HEAD and re-applying all seven edits with **asserted anchors**
  (`count(anchor) == 1`) — the guard that was missing the first time. Green at 1845 before
  any Phase 3 code was written.
* **Two wrong test premises**, corrected rather than accommodated: I asserted `201` for an
  ungrounded change (it correctly returns `200` — nothing resolved, so nothing written; a
  refused target must not manufacture a row just to look productive), and I decoded
  `quality_nodes.id` as `Uuid` when the column is TEXT.
* **A dead loop in `safety_floor`** whose doc comment claimed behaviour the code discarded
  (`let _ = d;`). Implemented properly: draft → cluster → fingerprint → the cases that
  failed with it.

---

## Discipline

* No test weakened, skipped or deleted.
* The safety-floor guarantee proven by reversion, not by assertion.
* Every persisted row in the tests comes from a real writer — no hand-written
  `quality_relations` row or fingerprint, because a fixture-shaped graph would let the
  resolver pass on links production cannot make.
* `regression_runner.rs` was fmt-clean at HEAD and was kept that way.
* Phase 1/2 not restarted or rewritten.

---

## Limitations carried forward

1. **`covers` / module-targeted impact unavailable** — needs the schema decision above.
2. **A change is declared, not ingested.** Four of five change sources are unimplemented.
   A capability gap, not a silent failure — each reports `INSUFFICIENT_DATA` with a reason.
   Closing it needs VCS ingestion, with its own trust and security questions.
3. **The safety floor is derived, not declared.** There is **no mandatory-test concept
   anywhere in the codebase** (verified; `test_cases.tags` has no writer convention). The
   floor is "has recorded failure evidence" — the strongest derivable proxy. A real
   mandatory flag is a schema decision.
4. **Impact resolution is one hop of dependency.** There is no case-to-case dependency
   graph, so "depends on" is not derivable today.
5. **The 0031 fixes apply only to new rows** — pre-existing rows would have
   `project_id = NULL` and fall outside the unique key. There are none; recorded because it
   is the one case where the additive migration changes what NULL means.
6. **No frontend surface** — backend-only; 1605 frontend tests untouched.
7. **The live database is unchanged** — 27 of 30 migrations, `change_impacts` 0 rows.
8. **No E2E CI job** — pre-existing.
9. **Risk scores come from the existing heuristic**, whose own `inputs` records *"heuristic,
   unweighted by any human-override feedback loop yet"*. The `high_risk` tier inherits its
   accuracy.

---

**Phase 4 not started. Nothing committed. Nothing pushed.**