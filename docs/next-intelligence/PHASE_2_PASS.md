# Phase 2 — PASS

**Phase 2 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`: End-to-End QA Traceability.**
Full evidence in [`PHASE_2_TRACEABILITY_REPORT.md`](./PHASE_2_TRACEABILITY_REPORT.md).

Predecessors: [`PHASE_1_PASS.md`](./PHASE_1_PASS.md) — Phase 1 not redone.

---

## Result

**PASS.** Every criterion below is met by evidence that can be re-run.

| Criterion | Status | Where proven |
|---|---|---|
| Supported chain is persisted | PASS | `the_whole_chain_is_persisted_and_retrievable_end_to_end` — all 7 links, each hop's ids checked against SQLite |
| Retrievable | PASS | same test via `TraceReader`; the unchanged Phase 1 HTTP test confirms the endpoint returns persisted `steps` |
| Project-scoped | PASS | `the_chain_does_not_cross_projects`, `a_proposal_from_another_project_never_joins_our_failure` |
| Evidence-backed | PASS | every hop carries `TraceEvidence { via_node, via_relation, rows, missing }` |
| Regression-safe | PASS | 1845 / 0 failed; both fixes proven caught by reverting them |
| Deterministic | PASS | one shared fingerprint computation |
| No fabricated links | PASS | 6 negative tests |
| No duplicate subsystem | PASS | no table, no store, no engine, no route, no relation type |
| Fresh migration works | PASS | 30 migrations, 9 tables, 5 join columns, from scratch |
| Module handled per instruction | PASS | kept MISSING; no heuristic; no new table |

---

## The finding

**The middle of the chain was not missing — it was impossible.**

The codebase had two functions both called "failure fingerprint" hashing **different
inputs**:

| Call site | Hashed | Column |
|---|---|---|
| `from_task_state` | category + masked message + **selector** | `execution_telemetry.failure_fingerprint` |
| `record_failure` | category + masked message — **selector omitted** | `failure_clusters.failure_fingerprint` |
| `propose_healing` | **`run_id` + `task_id`** + selector + reason | `healing_proposals.failure_fingerprint` |

Because the fingerprint is the join key, each divergence returned **empty rather than
erroring** — and empty reads as "this failure has no fix".

1. **`Failure → Bug` broken for every selector-bearing failure.** `from_task_state` passed
   the selector into the hash; `record_failure` passed `None`. Different digests, so no
   cluster↔telemetry join, so every defect reported "touched no tests".
   `record_failure`'s comment claimed it computed *"exactly the chain `from_task_state`
   uses"* — false.
2. **`Bug → Fix` impossible for any failure.** `run_id` is not an input to the telemetry
   fingerprint, so the two could never be equal. `list_healing_proposals_for_cluster`
   matches on that equality, so its fingerprint branch was dead code.
3. **A fingerprint that could not group.** Hashing `run_id` made each value unique *per
   run*, so the same defect across two runs never grouped. It was an idempotency key
   wearing a fingerprint's name.

`qa_memory.rs` already documents this exact failure mode for the earlier
`signature`-vs-fingerprint split, and says migration 0030 fixed it **at the column**. It
was never fixed **at the computation**.

**Fixed** with one shared `canonical_failure_fingerprint` that all three call sites now use,
so the schemes cannot drift apart again. Idempotency is unaffected — the key already
carried `run_id` and `task_id`.

---

## Why Phase 1's tests passed anyway

They inserted a healing proposal with a **hand-written** fingerprint equal to the
telemetry row's — a state the real system cannot produce. They validated the reader, not
the join.

Phase 2's suite therefore uses one rule: **every row comes from a real production
writer.** `Intelligence::ingest_run` writes telemetry, cluster and bug draft;
`GovernedHealingService::propose_healing` writes the proposal. No test writes a
fingerprint to make a join succeed.

---

## The guards are proven, not assumed

Each fix was reverted and the suite re-run:

| Reverted | Result |
|---|---|
| `governed_healing` → divergent fingerprint | **9 of 14 failed** |
| `record_failure` → selector dropped | **3 of 14 failed** |

Both restored. A test that cannot fail is not evidence.

---

## Also fixed

* **Retest detection by substring.** `verification_result.contains("pass")` both accepts
  prose containing "pass" and *rejects* a genuine `VERIFIED` outcome. Now deserialized into
  `HealingVerification` and read field by field.
* **Releases ignored the retest run.** A fix verified in release B left the trace pointing
  at release A. Releases now resolve from execution runs **∪** retest runs.

---

## Two of my own test premises were wrong, and both failed loudly

* Asserting an absent hop "exists and is empty" — the reader inserts a hop only when it
  has items. Corrected to assert absence plus the `incomplete` entry.
* Asserting our fix's fingerprint differs from another tenant's. **It must not** — a
  fingerprint identifies a failure *pattern* and deliberately carries no tenant. Isolation
  comes from the project-scoped reads. Rewritten to assert the real property (our hop names
  *our* proposal), and the equal-fingerprint fact is now asserted on purpose so that
  fingerprints acquiring a tenant — which would *fake* isolation — fails the build.

One Phase 1 assertion was also widened from exact-match to prefix-match, because Phase 2
made missing-hop labels carry their reason. The assertion that guards against fabrication
(`steps[hop].is_null()`) is unchanged.

---

## Module mapping — limitation, not feature

**`covers` is still never written.** Verified: `TestCaseRow` has `source_document` (a
document), `TaskDef` has no module field, `RunPlan` has no path, and the only
`source_file` in the codebase is for importing a `SKILL.md`. No URL, title or name
heuristic was introduced, and no mapping table was created. The end-to-end test asserts
`module` stays absent.

**The schema decision, for you:** add a nullable `test_cases.module_ref` populated by
whatever process actually knows the mapping. A `module` **table** is unnecessary —
`quality_nodes` already models a module as a vertex.

---

## Constraints honoured

| Constraint | Status |
|---|---|
| Reuse existing `quality_relations`, QA Memory, bug intelligence, release data, telemetry | honoured — every hop is a stored key; no parallel edges written |
| No another memory / bug / trace / regression system | honoured — no table, no store, no engine, no route, no relation type |
| No new migration | honoured — count pinned at 30 by test |
| Missing links stay missing | honoured — 6 negative tests |
| Project/tenant isolation | honoured, and isolation semantics made explicit |
| No commit, no push | honoured |

---

## Limitations carried forward

1. **`covers` / Test Case → Module unavailable** — needs the schema decision above.
2. **Fixes apply only to new rows.** No backfill, and none is possible for proposals: the
   old value embedded a run id, so it cannot be recomputed. Historical selector-bearing
   clusters and all historical proposals stay unreachable — reported, not guessed.
3. **A healed run's task carries no `test_case_id`** (the `AgentTaskPlan` has no such
   field), so a retest is attributed through its proposal. Sound, but not findable by
   "executions after the fix".
4. **No per-aspect views** — requirement coverage, failure-to-bug, bug-to-test, retest
   history. Deliberate: one coherent retrieval path, per the instruction, rather than four
   surfaces that could drift apart. The data is reachable through the existing reader.
5. **No frontend surface** — backend-only; 1605 frontend tests untouched.
6. **The live database is unchanged and still empty** — 27 of 30 migrations,
   `quality_relations` / `failure_clusters` / `bug_drafts` / `healing_proposals` all 0.
   The fixes cannot populate it retroactively; the chain needs new activity end to end.
7. **No E2E CI job** — pre-existing.

---

## Discipline

* No test weakened, skipped or deleted.
* Every failure fixed at root cause. Both fixes proven by reversion.
* Two of my own wrong premises were corrected rather than accommodated.
* `rustfmt` on `intelligence.rs` is now cleaner than HEAD.
* Phase 1 not redone; only its missing-hop label contract widened, with the
  fabrication guard untouched.

---

**Phase 3 not started. Nothing committed. Nothing pushed.**