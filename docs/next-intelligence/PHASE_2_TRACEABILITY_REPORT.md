# Phase 2 — End-to-End QA Traceability

**Phase 2 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`. Backend only. PASS.**

Predecessor: [`PHASE_1_TRACE_GRAPH_REPORT.md`](./PHASE_1_TRACE_GRAPH_REPORT.md),
[`PHASE_1_PASS.md`](./PHASE_1_PASS.md). Phase 1 was not redone; one Phase 1 contract was
widened and that is the only Phase 1 change (§4.4).

---

## 1. Headline: the middle of the chain was structurally impossible

Phase 1 could walk `Requirement → Test → Execution → Failure → Bug`. It could **not**
reach `Fix`, `Retest`, or `Release` — and the reason was not missing code.

**The codebase contained two different functions both called "failure fingerprint", and
they hashed different inputs:**

| Call site | Hashed | Column written |
|---|---|---|
| `ExecutionTelemetry::from_task_state` | category + masked message + **selector** + endpoint | `execution_telemetry.failure_fingerprint` |
| `Intelligence::record_failure` | category + masked message — **selector omitted** | `failure_clusters.failure_fingerprint` |
| `governed_healing::propose_healing` | **`run_id` + `task_id`** + selector + reason | `healing_proposals.failure_fingerprint` |

Because the fingerprint is the join key for the whole chain, each divergence produced an
**empty result rather than an error** — and an empty result here reads as *"this failure
has no fix"*, which is the most misleading answer available.

Three consequences, in increasing severity:

1. **`Failure → Bug` was broken for ordinary Playwright failures.** `compute_telemetry_fingerprint`
   mixes the selector into the hash. `from_task_state` passed the selector; `record_failure`
   passed `None`. For any failure carrying a selector the two values differed, so the
   cluster↔telemetry join found nothing and every defect reported an empty
   `related_test_cases`. `record_failure`'s own comment claimed it computed *"exactly the
   chain `from_task_state` uses"* — that claim was false.
2. **`Bug → Fix` could never succeed, for any failure.** `run_id` is not an input to
   `compute_telemetry_fingerprint`, so a proposal's fingerprint could never equal the
   cluster's. `Store::list_healing_proposals_for_cluster` matches on exactly that equality,
   so its fingerprint branch was dead and only its `task_ids` fallback ever fired.
3. **A failure fingerprint could not group a recurring defect.** Hashing `run_id` made the
   value unique *per run*, so the same defect in two runs produced two different
   fingerprints. It was an idempotency key wearing a fingerprint's name.

`qa_memory.rs:984-1006` already documents this exact failure mode for the earlier
`signature`-vs-fingerprint split, and states that migration 0030 fixed it **at the
column**. Phase 2 found it was never fixed **at the computation**.

### 1.1 Why Phase 1's tests passed anyway

Phase 1's tests inserted a healing proposal carrying a hand-written fingerprint equal to
the telemetry row's. That is a state the real system cannot produce, so the tests
validated the *reader* and never the *join*.

Phase 2's suite therefore adopts one rule: **every row is produced by a real production
writer.** `Intelligence::ingest_run` writes telemetry, the cluster and the bug draft;
`GovernedHealingService::propose_healing` writes the proposal. No test writes a
fingerprint to make a join succeed. §6 proves those tests fail when the fix is reverted.

---

## 2. Completed links

| Link | Before Phase 2 | After | Evidence |
|---|---|---|---|
| Requirement → Test Case | **worked** (Phase 1) | unchanged | `requirement_edge_persists_to_the_canonical_node_and_is_readable` |
| Test Case → Execution | **worked** (Phase 1) | unchanged | `execution_telemetry.test_case_id` |
| Execution → Failure | **broken for selector failures** | **fixed** | `a_cluster_and_its_telemetry_agree_even_with_a_failing_selector` |
| Failure → Bug | **worked**, but only for failures whose cluster and telemetry agreed — so broken wherever §1.1 applied | **fixed** (inherits the above) | same test, plus `a_real_healing_proposal_joins_a_real_bug_through_the_cluster` |
| Bug → Fix | **impossible** | **fixed** | `a_real_healing_proposal_joins_a_real_bug_through_the_cluster` |
| Fix → Retest | **unreachable**, and mis-detected when reached | **fixed** | `the_whole_chain_is_persisted_and_retrievable_end_to_end` |
| Retest → Release | **not implemented** — releases came only from execution runs | **fixed** | same test; retest run id resolves to the release |
| Test Case → Module | **no module identity exists** | **still absent, deliberately** | §7 |

### 2.1 `Fix → Retest` is read structurally, not sniffed

Phase 1 detected a retest with `verification_result.to_lowercase().contains("pass")`.
That is wrong in both directions, and both are now tested:

* `"NOT_VERIFIED — 3 regressions, no pass"` contains **no** "pass" as a standalone token
  but contains it inside "passed"-adjacent prose in other formats; a substring test
  accepts any future message containing the letters.
* A genuine pass recorded as `Verified` with `revised_task_passed: true` would be
  **rejected** by a `"pass"` substring check against `VERIFIED`.

`verification_result` is now deserialized into
`governed_healing::HealingVerification` and read field by field. A hop counts as passed
only when `outcome == Verified` **and** `revised_task_passed` **and**
`original_failure_resolved` — so a fix that improved the task while leaving the defect
open is reported as not passed
(`a_regression_introduced_by_a_fix_is_surfaced_as_not_passed`).

A retest hop is emitted whenever a verification **exists**, including a failed one. A fix
that was tried and did not work is real evidence; hiding it would report a clean success
for an unresolved defect.

### 2.2 `Retest → Release` is real evidence, not a re-run

A retest run is a run *created to prove the fix worked*. It is distinct from the run the
original failure happened in, and it is often in a different release. Phase 1 derived
releases only from execution runs, so a bug fixed and verified in release B while the
trace still pointed at release A. Phase 2 unions both: `execution_runs ∪ retest_runs`.

### 2.3 Why these links are derived, not new graph edges

`Bug → Fix`, `Fix → Retest` and `Retest → Release` are walked through **stored keys**, not
through new `quality_relations` rows:

```
bug_drafts.failure_cluster_id
  → failure_clusters.failure_fingerprint          (migration 0030)
  → healing_proposals.failure_fingerprint         (now the same value)
  → healing_proposals.verification_result.healing_run_id
  → runs.release_id
  → qa_releases.id
```

Every hop is a real column that already holds the relationship. Writing parallel graph
edges would be a second copy of data the tables already store, which is the duplication
the plan forbids. The relation **types** `derived_from`, `fixed_by` and `retests` named in
the Phase 1 brief are therefore **not** introduced — the evidence supports the walk, not a
new edge vocabulary. `quality_relations` keeps its existing three types.

---

## 3. Unresolved links

| Link | State | Why |
|---|---|---|
| Test Case → Module (`covers`) | **NOT_AVAILABLE** | §7 |
| Execution → Bug directly | not modelled | the domain has no such link; a bug is reached through its cluster, which is the only recorded relationship |

---

## 4. Files changed

10 modified, 1 added. Phase 2 touched no file it did not need to.

| File | Change |
|---|---|
| `adaptive_intelligence.rs` | **`canonical_failure_fingerprint`** — one shared computation; `from_task_state` routes through it |
| `intelligence.rs` | `record_failure` receives the failing selector and uses the shared computation; the selector is now read once, before both consumers |
| `governed_healing.rs` | `propose_healing` uses the shared computation; `compute_failure_fingerprint` retired with its reason |
| `trace.rs` | `failure_side_of_trace` returns fingerprints + run ids; `healing_side_of_trace` → `fix_and_retest_side_of_trace` with structural verification parsing; new `releases_for_runs`; richer missing-hop reasons |
| `trace_chain_tests.rs` | **new** — 14 tests |
| `trace_tests.rs` | missing-hop assertions made reason-tolerant; shared `temp_store_for_phase2` |
| `api_tests.rs` | the HTTP missing-hop assertion made reason-tolerant (Phase 1 test, §4.4) |
| `lib.rs` | `mod trace_chain_tests` |
| `store.rs`, `agent_execution.rs`, `agent_execution_tests.rs`, `qa_memory.rs`, `api.rs` | unchanged by Phase 2 (Phase 1 work, still on the branch) |

### 4.1 Migrations: **none**

Phase 2 is a behavioural fix. No DDL was added or altered — the columns the chain needs
already existed (`failure_clusters.failure_fingerprint` from 0030,
`healing_proposals.failure_fingerprint`, `verification_result`, `runs.release_id` from
0029). `trace_chain_tests::a_fresh_migration_supplies_every_table_the_chain_traverses`
builds a fresh database, pins the migration count at **30**, and asserts all 9 tables and
5 join columns exist.

### 4.2 APIs: **none added or changed**

The trace endpoint from Phase 1 (`GET /api/v1/projects/:project_id/trace`) is the single
retrieval path and its shape is unchanged. Phase 2 adds hops *inside* the existing
response — `steps.fix`, `steps.retest`, and retest-derived `steps.release` — and makes
`incomplete` more specific. No new route, no new subsystem, one coherent path.

### 4.3 Why no "views"

The plan's Phase 2 capability list also names a requirement-coverage view, a
failure-to-bug view, a bug-to-test view and retest history. Phase 2's operative
instruction asks for **"one coherent traceability retrieval path"**, so these are **not
built** — four extra read surfaces over the same tables would be four places for the
chain's answers to drift. The data for all four is already reachable:
`index` + `walk_test_case` (test → execution → failure → bug → fix → retest → release),
`list_test_cases_for_requirement` (requirement → test), and `store::runs_for_release`
(release → runs). Recorded as limitation L4.

### 4.4 The one Phase 1 test changed, and why

Phase 2 widened missing-hop labels from `"fix"` to
`"fix (no healing proposal recorded against this failure)"`. Two Phase 1 assertions
compared for exact equality and failed.

They were changed to a **prefix** match. This is not a weakening: the assertion still
proves the hop is named as missing, and it is now robust to the message becoming more
informative. Pinning exact wording would have failed a test for a documentation
improvement, which is the wrong reason for a test to fail. The `steps[hop].is_null()`
assertion — the one that actually guards against fabrication — is unchanged.

---

## 5. Bugs found

### 5.1 `Failure → Bug` silently broken for selector-bearing failures — **fixed**

`record_failure` passed `None` where `from_task_state` passed the selector. Same bug
class as the `signature`-vs-fingerprint split that `qa_memory.rs` documents, one layer
down. Its doc comment asserted equivalence that did not hold.

### 5.2 `Bug → Fix` structurally impossible — **fixed**

`healing_proposals.failure_fingerprint` hashed `run_id` and `task_id`. Provably unequal to
the telemetry fingerprint, and unique per run so it could not group recurring defects.
`list_healing_proposals_for_cluster`'s fingerprint branch was dead code.

### 5.3 Retest detection by substring — **fixed**

`verification_result.to_lowercase().contains("pass")` both accepts prose that happens to
contain "pass" and rejects a real `VERIFIED` outcome. Replaced with structural
deserialization.

### 5.4 Releases ignored the retest run — **fixed**

See §2.2.

### 5.5 Two wrong test premises in Phase 2's own suite — corrected, not accommodated

Recorded because both failed *loudly*, which is the correct outcome:

* **`a_fix_without_a_bug_draft_...`** asserted `steps["bug"]` exists-and-is-empty. The
  reader inserts a hop only when it has items, so an absent hop is absent from `steps`
  entirely. Corrected to assert absence plus the `incomplete` entry.
* **`a_proposal_from_another_project_never_joins_our_failure`** asserted our fix's
  fingerprint differs from theirs. It does not, and **it must not**: a failure
  fingerprint identifies a failure *pattern*, deliberately carrying no tenant, run or
  task — two projects hitting the same defect must produce the same value or patterns
  would not be comparable. Isolation comes from the project-scoped reads
  (`list_healing_proposals` is `WHERE project_id = ?`), never from the value. The test was
  rewritten to assert the real property: our hop names *our* proposal. The equal-fingerprint
  fact is now asserted on purpose, so that fingerprints acquiring a tenant — which would
  fake isolation — fails the build.

### 5.6 A compile error masked as a test result

During Phase 2 a delimiter mistake in `api_tests.rs` made `cargo test` exit 101 with no
`test result:` line. A log-parsing check reported "no failures" from an empty match set.
Caught by verifying the exit code and the log tail rather than trusting the grep. Noted
because "no output matched" and "no failures" look identical and are not.

---

## 6. Tests

**14** new, in `trace_chain_tests.rs`.

| Test | Proves |
|---|---|
| `the_retired_fingerprint_scheme_could_never_join_the_telemetry_one` | the retired scheme was unique per run and never produced the join value |
| `the_canonical_fingerprint_groups_a_defect_across_runs` | stable across calls, sensitive to the selector, and masks dynamic ids |
| `a_cluster_and_its_telemetry_agree_even_with_a_failing_selector` | **5.1** fixed |
| `a_real_healing_proposal_joins_a_real_bug_through_the_cluster` | **5.2** fixed; the fix names the bug it addresses |
| `the_whole_chain_is_persisted_and_retrievable_end_to_end` | **all 7 links**, each asserted against the database |
| `a_fix_that_was_never_verified_has_no_retest` | absence stays absent, with the reason stated |
| `a_failed_retest_is_reported_rather_than_hidden` | a failed verification appears with `passed: false` |
| `a_regression_introduced_by_a_fix_is_surfaced_as_not_passed` | a passing revised task does not mask an unresolved defect |
| `a_corrupt_verification_record_is_reported_not_dropped` | an unreadable record surfaces, never reads as "no retest" |
| `a_fix_without_a_bug_draft_is_still_reported_with_the_gap_named` | a single-occurrence fix is real and reported |
| `the_chain_does_not_cross_projects` | another tenant's cluster, bug, proposal and release never appear |
| `a_proposal_from_another_project_never_joins_our_failure` | isolation via scoped reads, not via the fingerprint |
| `repeating_the_chain_does_not_duplicate_relations` | re-proposing is idempotent; re-ingesting does not duplicate edges |
| `a_fresh_migration_supplies_every_table_the_chain_traverses` | fresh DB, count pinned at 30, 9 tables + 5 join columns |

### 6.1 The tests are proven guards, not decoration

Each fix was **reverted and the suite re-run** to confirm the tests actually detect the
regression:

| Reverted fix | Result |
|---|---|
| `governed_healing` → divergent fingerprint | **9 of 14 failed** |
| `record_failure` → selector dropped | **3 of 14 failed**, including `a_cluster_and_its_telemetry_agree_even_with_a_failing_selector` |

Both reverts were then restored. A test that cannot fail is not evidence, and these were
verified rather than assumed.

### 6.2 Regression

| Run | Before Phase 2 | After |
|---|---|---|
| Backend suite | 1831 passed / 0 failed / 2 ignored | **1845 passed / 0 failed / 2 ignored** (+14) |
| `cargo build` | clean | clean |
| `cargo clippy --all-targets` | 0 errors | 0 errors, no warning in any new file |
| `rustfmt --check` | — | clean on all 7 Phase 2 files (`intelligence.rs` is now cleaner than HEAD) |
| `qa_memory` (the consumer of this data) | 93 | 93 |
| `governed_healing` (the writer changed) | 53 | 53 |
| `api_tests` (HTTP surface) | 312 | 312 |
| Frontend | 1605 / 79 files | unchanged — no frontend file touched |

Zero regressions. No pre-existing test was weakened, skipped or deleted.

---

## 7. Module mapping — the limitation, restated

**`covers` is still never written, and that is the correct outcome.** The Phase 2
instruction required this explicitly, and it was verified rather than assumed:

* `TestCaseRow` has `source_document` — a document, not a module
* `TaskDef` has no module field; `RunPlan` has no path
* the only `source_file` in the codebase is `CreateCustomSkillBody.source_file`, for
  importing a `SKILL.md`

So the walk continues to report `module` as **MISSING**, and the end-to-end test asserts
it stays absent (`the_whole_chain_is_persisted_and_retrievable_end_to_end`). No URL, title
or name heuristic was introduced. No module mapping table was created.

**The schema decision that would close it** — for the user, not for this phase:

> Add a real module identity to the test-case domain. The minimal honest option is a
> nullable `test_cases.module_ref` (plus optionally `module_name`), populated by whatever
> process actually knows the mapping — for a code-linked test, the file or package the
> step exercises; for a generated test, the source module it was generated from. A
> `module` **table** is not needed: `quality_nodes` already models a module as a vertex,
> which was the deliberate Phase 2 choice over a fourth parallel identity system.

Until that column exists, any `covers` edge would be a guess presented as traceability.

---

## 8. Remaining limitations

**L1 — `covers` / Test Case → Module unavailable.** §7. Needs the schema decision.

**L2 — the cluster↔telemetry and proposal fixes apply only to rows written from now on.**
No backfill was performed, and none is possible for the proposal side: the old value
embedded a run id, so there is no way to recompute the new fingerprint for an existing
proposal. Historical `failure_clusters` whose failure carried a selector keep a
fingerprint that no telemetry row matches, and historical proposals are unreachable. The
walk reports those gaps rather than guessing. Existing rows are untouched — nothing is
deleted and no value is falsified.

**L3 — a healed run's task carries no `test_case_id`.** The healing dispatcher builds an
`AgentTaskPlan`, which has no such field, so the retest is attributed through the proposal
rather than directly. That is sound (the proposal is already keyed to the failure) but it
means a retest cannot be found by querying "executions of this case after the fix" — only
through the chain.

**L4 — no per-aspect views.** §4.3. The data is reachable; four extra surfaces are not
built, to avoid four places for the answers to drift.

**L5 — no frontend surface.** Backend-only; the frontend's 1605 tests are untouched.

**L6 — the live database is unchanged and still empty.** Never migrated in place: 27 of 30
migrations, `quality_relations = 0`, `failure_clusters = 0`, `bug_drafts = 0`,
`healing_proposals = 0`. **The fixes cannot retroactively populate it** — the chain needs
new activity end to end. A live dataset would have to accumulate: a case with a
requirement reference, two identical failing executions, a healing proposal, a
verification, and a release.

**L7 — no E2E CI job.** Pre-existing: `ci.yml` has 3 jobs, none E2E.

---

## 9. PASS gate

| Criterion | Status | Evidence |
|---|---|---|
| The supported chain is persisted | **PASS** | `the_whole_chain_is_persisted_and_retrievable_end_to_end` asserts all 7 links, each hop's ids checked against SQLite |
| Retrievable | **PASS** | same test, via `TraceReader`; the unchanged Phase 1 HTTP test confirms the endpoint returns persisted `steps` |
| Project-scoped | **PASS** | `the_chain_does_not_cross_projects`, `a_proposal_from_another_project_never_joins_our_failure` |
| Evidence-backed | **PASS** | every hop carries `TraceEvidence { via_node, via_relation, rows, missing }`; each join names the stored key it used |
| Regression-safe | **PASS** | 1845 / 0 failed; no test weakened; both fixes proven to be caught by reverting them (§6.1) |
| Deterministic | **PASS** | one shared fingerprint computation; `the_canonical_fingerprint_groups_a_defect_across_runs` |
| No fabricated links | **PASS** | 6 negative tests: unverified fix, failed retest, regression, corrupt record, no bug draft, and module staying absent |
| No duplicate subsystem | **PASS** | no table, no store, no engine, no route, no new relation type. One reader, one fingerprint function |
| Fresh migration works | **PASS** | 30 migrations, 9 tables, 5 join columns, from scratch |
| Module handled per instruction | **PASS** | kept MISSING; no heuristic; no new table; schema decision documented (§7) |

### Honest caveats

1. **`covers` is absent by design**, not delivered. The gate item "the supported chain" is
   met for the 7 links the domain can evidence; for module the correct answer today is that
   it cannot be written, and the walk says so. §7.
2. **Persistence is proven in fresh migrated databases driven through real writers**, not
   in the live database, which is untouched and empty and cannot be populated
   retroactively (L2, L6). No claim about production history is made.
3. **The `Fix → Retest` and `Retest → Release` links require a governed healing execution
   to have happened.** They are unreachable for projects with no healing proposals — the
   correct answer, reported per hop.

**Phase 2 = PASS**, with L1, L2 and L6 carried forward as known gaps.

---

**Phase 3 not started. Nothing committed. Nothing pushed.**