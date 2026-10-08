# Phase 6 — Full Validation & Production Hardening: Report

**Date:** 2026-10-07
**Phase:** 6 of 6
**Status:** PASS
**Baseline before the plan:** 1716 passed / 0 failed / 2 ignored
**After all six phases:** 1809 passed / 0 failed / 2 ignored (+93)
**Frontend:** 1605 passed / 79 files, `tsc --noEmit` clean (unchanged by this plan)

---

## 1. Objective

Validate the complete QA Intelligence Memory system end to end, with test evidence,
and produce `FINAL_VALIDATION_REPORT.md`.

This phase added **no new capability**. It drove the whole chain through real code,
measured performance rather than assuming it, and — in doing so — found and fixed two
real defects that every earlier phase had missed.

---

## 2. The end-to-end chain, validated

Master plan §8 requires this flow to be validated:

```text
Requirements -> Requirement Intelligence -> Test Planning -> Test Execution
  -> Evidence -> Failure Intelligence -> Historical Memory -> Retrieval
  -> Risk Intelligence -> Future Test Planning
```

`the_whole_chain_carries_one_failure_from_a_requirement_to_the_next_plan` drives all
ten steps through production entry points — `upsert_requirement_document`,
`insert_test_case`, `link_test_case_to_module`, `insert_run_for_release`,
`insert_evidence`, `Intelligence::ingest_run`, `insert_execution_telemetry`, the
quality graph, `QaMemoryReader`, and
`AdaptiveIntelligenceService::generate_planning_feedback`.

One failure travels the whole way and each step is asserted:

| Step | Assertion |
|---|---|
| Requirements | the parsed document stores its requirement |
| Test planning | the case is traceable to that requirement key |
| Execution | 3 runs, task carries the case link |
| Evidence | a real `evidence` row per run |
| Failure intelligence | `ingest_run` clusters the repeated failure and drafts a bug |
| Historical memory | 3 telemetry rows, **every one** linked to its case |
| Retrieval | stability, module and requirement history all answer from those rows |
| Risk | the same evidence, risk-shaped, with model coverage reported |
| Future planning | the agent receives a signal carrying `2 of 3` and 3 evidence runs |

`a_chain_where_nothing_failed_still_produces_an_honest_empty_answer` is the mirror:
one clean run yields no defect, a zero failure rate, and a `0.0` contribution. It
exists so "no defects" is proven to be a real state rather than the accident of a code
path that never ran.

---

## 3. Two real bugs found — both mine, both in this plan

### Bug A: a failure cluster could not be joined to the executions behind it

**Found by:** the end-to-end chain test. Symptom: a defect was correctly detected and
its occurrence count was correct, but `related_test_cases` was **empty**.

**Root cause — two identifier schemes for the same failure:**

```text
failure_clusters.signature             = normalize_signature(task.reason)
                                           -> "request post /payment returned #"
execution_telemetry.failure_fingerprint = compute_telemetry_fingerprint(category,
                                           masked_reason, selector)
                                           -> "8a842711a81e..."  (SHA-256)
```

`qa_memory::QaMemoryReader::bug_history` compared these two for equality. They are
never equal, so the correlation silently produced nothing.

**Why it is dangerous, not merely wrong:** the answer it produced is *"this recurring
defect touched no tests"* — presented as fact, while the test that produced it is right
there in the same query. Absence reported as presence-of-nothing is the failure mode
this whole plan exists to prevent.

**Fix — migration `0030_failure_cluster_fingerprint.sql`** (nullable column, additive):

- `failure_clusters.failure_fingerprint` added, plus
  `idx_clusters_fingerprint(project_id, failure_fingerprint)`
- `record_failure` now computes the fingerprint with **exactly** the chain
  `ExecutionTelemetry::from_task_state` uses — `sanitize_telemetry_text` →
  `categorize_failure(_, None)` → `mask_dynamic_tokens` →
  `compute_telemetry_fingerprint` — so the two rows join by equality
- `upsert_failure_cluster` fills it on insert and backfills on update **only when
  still NULL**, so a pre-migration cluster picks it up on its next occurrence instead
  of keeping a value that was never there
- `bug_history` correlates on the fingerprint, falling back to the `fails_with` graph
  for clusters that predate the migration

**Why a migration and not a heuristic:** the cluster node on the quality graph is keyed
by the *fingerprint* while the cluster row is keyed by *normalized text*, and nothing
joined them. Matching on occurrence count or time window would have been a guess, and a
guess here produces exactly the confident-but-wrong answer the plan forbids.

**Tests proving it:** the end-to-end chain (fingerprint round-trip and the correlation
it enables), plus the migration test asserting `failure_fingerprint` exists on an
existing database.

### Bug B: a release appeared twice in its own trend

**Found by:** `a_release_whose_failures_got_worse_is_reported_as_worsening`, which
expected 3 trend points and got 4.

**Root cause:** `release_risk` looped `for other in releases.iter()` without excluding
the target, then appended the target again. Two consequences, both real:

- the point count was inflated
- `trend_from` averaged the newest release against **itself** as one of the "earlier"
  points, so the reported direction of travel was computed against a duplicated input

Phase 3's `release_history` had the correct `.filter(|r| r.id != release_id)`;
Phase 5's `release_risk` did not.

**Fix:** filter the target out of the comparison loop, with a comment saying why.

**Test proving it:** the worsening/improving/steady trend tests, plus the point-count
assertion.

---

## 4. A pre-existing security finding, reported not fixed

### `runs.plan` stores `TaskDef.params` verbatim, including credentials

**Measured, not assumed.** A task given
`params: {"username": "qa", "password": "<secret>"}` has that secret persisted in
`runs.plan` as given.

`adaptive_intelligence::mask_dynamic_tokens` exists and scrubs telemetry text
(UUIDs, timestamps, IPs, ports, hex, digits), but it is applied only to
`execution_telemetry` text — never to the run plan.

**What was checked and passes:** the retrieval layer this plan added reads
`execution_telemetry` and the quality graph, and never reads `params` or echoes
`error_message`. `no_credential_material_reaches_the_memory` puts a secret in a task's
params *and* in an error message that quotes it, then serializes all six retrieval
surfaces and asserts the secret appears in none.

**Why it was not fixed here:** this is run-plan persistence, not QA memory. Changing it
means deciding what the run engine persists for every executor — a real architectural
question, and master plan §14 forbids rewriting existing architecture unnecessarily.
It is therefore reported for a decision rather than changed.

**Recommended, if you want it addressed:** have `insert_run` persist a redacted copy of
`params` while leaving the in-memory `TaskDef` intact for execution, and verify first
that nothing reads `runs.plan` back to drive execution. `rbac_probe` and the audit log
read plans, so this needs a real check, not a guess.

---

## 5. Security verification

| Check | Test | Result |
|---|---|---|
| Tenant/project isolation | `no_cross_project_leakage_anywhere_in_the_chain` | PASS — 9 read surfaces, 4 distinctive values, zero appearances |
| No cross-tenant planning influence | `signals_never_cross_a_project_boundary` | PASS |
| Cross-project release attachment refused | `associating_a_run_with_another_projects_release_is_refused` | PASS |
| No credential in retrieved views | `no_credential_material_reaches_the_memory` | PASS |
| No chain-of-thought | `no_chain_of_thought_is_stored_or_returned_anywhere_in_the_chain` | PASS — 8 forbidden terms × 11 surfaces |

The isolation test is deliberately broad: it serializes every read the retrieval layer
offers, asked about the *other* project, and asserts that none of the owning project's
identifiers, labels, fingerprints or task names appear in any of them — rather than
checking only that a list came back empty.

The chain-of-thought test refuses eight terms that would mean stored reasoning
(`chain_of_thought`, `scratchpad`, `internal_notes`, `monologue`, …) across eleven
serialized surfaces, so a field cannot be slipped in later without failing the suite.

---

## 6. Data integrity

`the_chain_leaves_no_orphaned_or_duplicated_records` checks, against the live schema:

| Property | Assertion |
|---|---|
| No duplicate history | 2 executions → exactly 2 telemetry rows |
| No orphaned telemetry | every row's `run_id` exists in `runs` |
| No orphaned task links | every non-null `tasks.test_case_id` exists in `test_cases` |
| Correct project scope | no `test_case_id` from project A appears in project B's rows |
| Correct timestamps | every `created_at` is RFC3339, none NULL |

`releasing_a_release_never_moves_a_run_out_of_its_project` also asserts that a refused
cross-project insert **leaves no run behind**, not merely that it errors.

### Integrity observation: `tasks.test_case_id` has no foreign key

The column is unconstrained, so any string can be written into it. The system only
writes UUIDs, and every test uses a real inserted case, so nothing is currently wrong —
but the constraint is not enforced by the database.

Adding it would require a table rebuild, since SQLite cannot `ALTER TABLE ADD
CONSTRAINT`, which is a destructive migration and therefore a master plan §10 stop
condition. Recorded here rather than done.

---

## 7. Performance — measured, then asserted

Master plan §8: *"do not prematurely optimize without evidence."* So the numbers were
taken first and the assertions written second, against ceilings rather than against the
measured values.

### 7.1 Index usage, from `EXPLAIN QUERY PLAN`

Every query the retrieval layer issues, with the plan SQLite actually chose:

```text
SEARCH execution_telemetry USING INDEX idx_telemetry_created_at (project_id=?)
SEARCH execution_telemetry USING INDEX idx_telemetry_test_case  (project_id=? AND test_case_id=?)
SEARCH failure_clusters  USING INDEX idx_clusters_fingerprint  (project_id=? AND failure_fingerprint=?)
SEARCH runs              USING INDEX idx_runs_release          (release_id=?)
SEARCH qa_releases       USING INDEX idx_releases_project_created (project_id=?)
```

Every one is a `SEARCH ... USING INDEX`, never a bare `SCAN`. The assertions name the
index, so removing one fails the suite rather than silently degrading.

This is the same discipline migration 0027 used — indexes justified by recorded query
plans, not by a guess that something might be slow.

### 7.2 Retrieval latency

Fixture: 120 test cases × 4 executions = **480 telemetry rows**, 12 modules, 6 releases.
Warmed once so the measurement is of retrieval rather than first-touch page faults.

```text
worst single read    33.0 ms
module_risk_history  17.1 ms
planning_signals     33.0 ms
module_risk          29.0 ms
```

The assertion is a **750 ms ceiling**, deliberately ~20× the measured worst. A timing
test that pins current speed fails on a slower machine for no real reason; a ceiling
catches the thing that actually matters here — an accidental `O(history²)` or an
unbounded scan. Every read is one bounded scan (cap 1000 rows), so anything approaching
the ceiling means a query changed shape.

### 7.3 Planning overhead stays bounded

```text
30 modules + 10 releases -> 20 signals
confidences [0.9 ×4, 0.6 ×16]
```

Two properties asserted, both of which matter more than the count:

1. **Bounded** — the planner cannot grow the prompt without limit as a project grows.
2. **Monotonic in evidence** — signals are ordered best-evidenced first, so truncation
   drops the weakest rather than an arbitrary subset.

### 7.4 Storage growth

```text
50 telemetry rows -> database 663,552 bytes (162 pages × 4096)
telemetry payload      15,962 bytes
per remembered execution   319 bytes
```

Growth is linear in executions. At ~320 bytes per execution, a million remembered
executions is roughly 320 MB of telemetry — worth knowing before retention policy is
ever tuned, and a number no part of the plan had estimated.

---

## 8. Migration safety — verified against a copy of the live database

The live file (`infra/data/orchestrator.sqlite3`, 27 migrations, 40 tables, 1036 audit
rows, 19 telemetry rows) was **copied**, never migrated in place. The test then:

1. snapshots every table's row count with a raw pool, before `Store` is involved
2. lets the real `sqlx` migrator run
3. re-snapshots and compares
4. asserts each of this plan's columns exists **by name** on the pre-existing database
5. asserts historical telemetry gained **no** invented links

```text
migrated a copy of the existing database: 30 migrations applied,
19 telemetry rows preserved, 0 pre-linked
```

`_sqlx_migrations` is excluded from the count comparison — it is the migration ledger
and is *supposed* to grow by one row per migration. Including it made the test fail on a
fresh copy and pass on an already-migrated one, which is precisely backwards for a
regression test.

The applied count is now **read from the ledger** rather than hardcoded. A hardcoded
number would itself go stale the moment a migration was added — the same class of
quietly-wrong assertion this phase exists to catch.

All five migrations are additive:

```text
0028  tasks.test_case_id                        (nullable column + 2 indexes)
0029  qa_releases                               (new table + runs.release_id + 3 indexes)
0030  failure_clusters.failure_fingerprint      (nullable column + 1 index)
```

No `DROP`, no `DELETE`, no data rewrite.

---

## 9. Full regression

```text
backend    1809 passed / 0 failed / 2 ignored   (exit 0)
frontend   1605 passed / 79 files
tsc        clean (exit 0)
clippy     0 errors, 0 warnings in any file this plan created
rustfmt    clean for every file this plan created or edited
```

The 2 ignores are pre-existing and untouched. Two pre-existing `store.rs` rustfmt diffs
(lines 84, 1012+) were verified against `HEAD` and left alone.

---

## 10. Phase 6 PASS gate

| Criterion | Status |
|---|---|
| Full flow validated end to end | yes — all ten steps, real code paths |
| Full regression (unit, integration, API, DB, migration, existing) | yes — 1809 / 1605 / tsc |
| Static checks | yes — clippy 0 errors, rustfmt clean |
| Security: tenant isolation | yes |
| Security: authorization respected | yes — cross-project reads return no data |
| Security: no cross-project leakage | yes — 9 surfaces × 4 values |
| Security: no credentials in memory | yes — retrieval verified; one pre-existing finding reported |
| Security: no chain-of-thought | yes — 8 terms × 11 surfaces |
| Performance: query + index usage | yes — 5 plans recorded, all index searches |
| Performance: large historical dataset | yes — 480 rows, 33 ms worst |
| Performance: planning overhead bounded | yes — 20 signals, evidence-ordered |
| Performance: storage growth | yes — 319 bytes per execution |
| Data integrity: no orphans / duplicates / scope / timestamps / safe migrations | yes |

**Phase 6: PASS. All six phases complete.**

---

## 11. Deliverable

`docs/qa-intelligence-memory/FINAL_VALIDATION_REPORT.md` — the consolidated
architecture, per-phase summary, file and database changes, API surface, test
inventory, regression results, every bug found and fixed, performance and security
observations, known limitations, and the final verdict.

Fifteen documents under `docs/qa-intelligence-memory/` now make up the record:

```text
PHASE_1_AUDIT.md      PHASE_1_REPORT.md     PHASE_1_PASS.md
PHASE_2_REPORT.md     PHASE_2_PASS.md
PHASE_3_REPORT.md     PHASE_3_PASS.md
PHASE_4_REPORT.md     PHASE_4_PASS.md
PHASE_5_REPORT.md     PHASE_5_PASS.md
PHASE_6_REPORT.md     PHASE_6_PASS.md
FINAL_VALIDATION_REPORT.md   PRE_COMMIT_REVIEW.md
```

**Written late, and recorded as such.** The phase reports for phases 3, 4 and 5 and
this phase's own predecessor for phase 1 were produced on 2026-10-08, after the
implementation and its tests were complete, rather than at each phase boundary. The
work and the test evidence were never in question; the written record was late. This was
caught by the release review as finding B1 and cleared by writing them — with no
application logic changed and no test weakened. See `PRE_COMMIT_REVIEW.md` §0.