# PHASE 8 — TEST CASE QUALITY INTELLIGENCE REPORT

Repository: `https://github.com/abhisummer22-design/AIQA.git`
Branch: `main` · HEAD: `7930507` · 0 ahead / 0 behind `origin/main`
Roadmap: `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §"PHASE 8" (L248–297)
Predecessors: `PHASE_7_USER_JOURNEYS_REPORT.md`, `PHASE_7_3_DECISION_IMPLEMENTATION_REPORT.md`,
`PHASE_7_4_RISK_METRIC_IMPLEMENTATION_REPORT.md`, `PRE_PHASE_8_RISK_SEMANTICS_REPORT.md`

## Verdict: **PASS**

All eight roadmap dimensions are implemented or explicitly reported unanswerable, with the reason
named. No mandatory capability is blocked. The PASS gate is met: the assessment is explainable,
deterministic where it claims to be, honest about missing data, project-scoped, and changes no
approval or execution behaviour.

One roadmap-named deliverable is **not** produced, and that is not a scope reduction — see §17.

---

## 1. Initial repository state and baseline

Re-verified, not read from a previous report.

| | |
|---|---|
| branch / HEAD | `main` @ `7930507` |
| ahead / behind `origin/main` | 0 / 0 |
| modified / untracked / deleted / staged | 23 / 17 / 0 / 0 |
| `C:` free at start | 22.96 GB |

### Baseline, measured before any edit

```
cargo test  →  ok. 2070 passed; 0 failed; 5 ignored   (exit 0)
```
Full output saved to `p8_baseline_1.txt` (400,096 bytes) so it survives any later run.

This matches the historically reported 2070. Every figure below is from a run in this phase.

### On the unresolved earlier failure

The brief asked me not to assume the unexplained `2069/1 failed` run from the pre-Phase-8 work was
fixed or environmental. I did not assume either way:

* It **did not recur** in any run in this phase — the baseline, 3 full-suite runs before the
  mutation exercise, 3 after, and the 3 final runs: **10 consecutive clean full-suite runs**.
* Disk conditions were **checked, not assumed**: every integration test places its SQLite file in
  `std::env::temp_dir()`, which resolves to `C:` here. `C:` free space fell from 22.96 GB to under
  22 GB across the phase — nowhere near exhaustion, so the specific disk-full mechanism that
  produced `(code: 13) database or disk is full` in an earlier phase **did not apply**.
* That rules the disk-full cause *out*. It does **not** establish what the original failure was,
  because its output was never preserved. The honest statement remains: **one unidentified
  transient failure, not reproduced in 10 further full-suite runs, cause still unknown.** I am not
  claiming it was fixed.

---

## 2. Exact Phase 8 scope from the roadmap

Read from `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` L248–297 before any design work.

**Objective (L252):** measure whether generated and saved test cases are useful, traceable,
unambiguous, non-duplicative, and executable.

**Constraint (L256):** *"Only score dimensions that can be measured honestly."*

**Eight proposed dimensions (L258–264):** requirement linkage/coverage; one clear condition per
case; concrete, reproducible steps; observable expected result; negative/boundary coverage where the
requirement supports it; duplicate similarity; automation feasibility against available executor
capabilities; relevance to recorded risk/change signals.

**Eight scoring rules (L268–275):** deterministic rules preferred; AI heuristics labelled with
evidence and confidence; explain every score and deduction; distinguish `SCORE_AVAILABLE` from
`INSUFFICIENT_DATA`; missing criticality is not low criticality; never auto-delete/reject/approve;
no second test-generation or validation engine; reuse the existing validator, coverage techniques,
trace links, memory and risk signals.

**PASS gate (L292):** explainable, reproducible where deterministic, honest about missing data,
project-scoped, and **does not silently change approval/execution behaviour**.

---

## 3. Source audit — what quality can honestly be measured from

Every candidate signal was checked against a real source before use. This is an investigation
result, not a shopping list.

| # | candidate | authoritative source | exists? | measurable or declared? | what it can establish | what it cannot |
|---|---|---|---|---|---|---|
| 1 | requirement linkage | `test_cases.requirement_reference` / `source_requirement`; `quality_relations` `covers`; `requirements` table | **yes** | measured, persisted | whether the case traces to a real requirement row | whether the requirement is well-written |
| 2 | one clear condition | `title`, `description`, `expected_result` | yes | **declared only** | — | "one clear condition" is a semantic judgement with no field carrying it |
| 3 | concrete, reproducible steps | `test_cases.steps` JSON `[{step_number, action, expected_result}]` | **yes** | measured, via the **existing** `execution_plan::compile_deterministic` | whether the existing compiler can build a real plan without guessing | whether the *plan* would pass at runtime |
| 4 | observable expected result | `test_cases.expected_result`, `assertions`; enforced at creation by `api::validate_test_case_request` | **yes** | measured | whether an oracle is declared at all | whether the oracle is *good* — vagueness ("works", "ok") needs a heuristic |
| 5 | negative / boundary coverage | `requirements.text` (prose) | **no field** | **declared intent only** | — | "where the requirement supports it" requires reading prose; nothing records it |
| 6 | duplicate similarity | `test_case_embeddings` + existing cosine | **yes** | measured | whether two APPROVED cases are near-duplicates | anything about cases that were never approved (no row exists) |
| 7 | automation feasibility | `ExecutorRegistry::registered_kinds()` | **yes** | measured | whether any executor is configured for this deployment | whether a *specific* executor fits a specific case — no mapping exists |
| 8 | relevance to change / risk signals | `change_impacts` via `Store::count_distinct_changes_for_node` | **yes** | measured | whether declared changes reached this case | relevance where nothing has been declared |
| 9 | execution history | `execution_telemetry` via `QaMemoryReader` (Phase 7.4 helpers) | **yes** | measured | product-outcome reliability | anything about cases never executed |

**Two existing systems found and deliberately not duplicated** (global rule 4, roadmap rule 8):

* `test_value_assessments` (migration 0002) with `value_score`, `risk_coverage`,
  `historical_defect_detection`, `business_criticality`, `change_relevance`, `execution_cost`,
  `flakiness`. Written by exactly one production caller — `Intelligence::update_test_intelligence`.
  It scores a **quality_nodes vertex** from **execution history**, and only exists once a case has
  run. Phase 8 assesses the **case definition** plus traceability and duplicates, and works with no
  execution history at all. Complementary, not overlapping.
* `execution_plan::compile_deterministic` and `ai_generation::cosine_similarity` — reused directly
  rather than reimplemented (§7).

---

## 4. The quality assessment contract

### No aggregate score — a decision, not an omission

The roadmap defines eight dimensions and eight rules but defines **no weights and no aggregation
formula**. Producing a single 0–100 number would mean inventing weights with no authority behind
them, which is exactly what the brief forbids and what rule 3 ("explain every score and deduction")
cannot satisfy for a made-up weighted mean.

So `TestCaseQuality` has **no aggregate field**, and says so on the object itself
(`aggregate_basis`). `p8_contract_there_is_no_aggregate_quality_score` asserts that the serialized
form contains no `score`/`grade`/`rating`/`quality_percentage` key under any name.

`overall()` returns a **status**, not a number, and is conservative:

```
any dimension Weak                → Weak
every dimension Adequate          → Adequate
anything unmeasured or unassessed → InsufficientData
```

"Nothing was found wrong" is not "everything was checked and is fine".

### Four verdicts, two of which are about *not knowing*

| verdict | meaning |
|---|---|
| `Adequate` | measured, meets the rule |
| `Weak` | measured, does not meet the rule — **only ever used when a real source was read** |
| `InsufficientData` | the source exists; this case has not produced enough of it |
| `NotAssessed` | **no authoritative source exists in this schema** |

The last two are deliberately distinct, because they are different facts. A project with no
embeddings genuinely cannot be assessed for duplication (`InsufficientData`). Negative/boundary
coverage has no source at all (`NotAssessed`), and the evidence string names what is missing.

### Missing evidence (brief rules 9 and 10)

* A case with **no execution history** reads `InsufficientData` — not a defect, not a perfect score.
* A case whose executions were **all infrastructure errors or blocked** reads `InsufficientData`:
  execution never reached a verdict, so nothing is known about product reliability.
* A case with **no embedding row** cannot be judged for duplication.
* A **store read failure** reads `InsufficientData` with `store read failed` in the evidence —
  never a clean or empty assessment.

---

## 5. Each dimension: source and limitation

| key | roadmap dimension | source | limitation stated in the output |
|---|---|---|---|
| `requirement_linkage` | requirement linkage / coverage | `quality_relations` `covers`; `requirements.req_key` resolved against the project's real documents | distinguishes *missing* linkage (Weak) from *broken* linkage (declared but unresolvable) |
| `expected_result_present` | observable expected result | the same rule `api::validate_test_case_request` enforces at creation | cannot judge whether the oracle is *good*; only whether one is declared |
| `concrete_steps` | concrete, reproducible steps | `execution_plan::compile_deterministic` | `None` is reported `InsufficientData`, **not** Weak: the engine falls back to AI compilation in that state, so it is not evidence of vagueness |
| `negative_boundary_coverage` | negative / boundary coverage | **none exists** | `NotAssessed`; evidence names the missing source |
| `duplicate_similarity` | duplicate similarity | `test_case_embeddings` + existing cosine | only APPROVED-and-embedded cases have rows |
| `automation_feasibility` | automation feasibility | `ExecutorRegistry::registered_kinds()` | establishment-level, not per-case executor matching |
| `change_relevance` | relevance to change / risk signals | `count_distinct_changes_for_node` | zero declared changes is `InsufficientData`, never "irrelevant" |
| `execution_reliability` | execution reliability | `QaMemoryReader` Phase 7.4 helpers | product outcomes only; infrastructure reported separately |

`one clear condition per case` (roadmap dimension 2) is **folded into** `concrete_steps` and
`expected_result_present` rather than scored separately. A separate verdict would require a
semantic judgement the schema cannot support, and inventing a "how many conditions does this text
imply" rule would be the fabrication the brief forbids. That is recorded rather than hidden.

---

## 6. Test quality vs product outcomes vs infrastructure outcomes

The three are separated at every point:

* **Test quality** — properties of the case definition, its traceability, and its duplicates.
  Read from `test_cases`, `quality_relations`, `requirements`, `test_case_embeddings`.
* **Product outcomes** — `PASSED` / `FAILED` / `TIMEOUT`, read only through
  `QaMemoryReader::completed_failure_rate_fraction` and `completed_count`.
* **Infrastructure outcomes** — `INFRA_ERROR`, read only through `QaMemoryReader::infra_errors` and
  reported separately in the same evidence string.

`INFRA_ERROR` never enters a product numerator or denominator. Mutation 2 (§11) proves a test
detects it if it ever did.

**No AI judgement is shipped.** Rule 2 requires any heuristic to be labelled with evidence and
confidence; `QualityDimension.deterministic` is the field where one would be labelled, and
`p8_contract_every_dimension_explains_itself` asserts it is `true` for every dimension. If a future
phase adds a heuristic, that assertion is the tripwire.

---

## 7. Files changed, and why

Exactly **5** files, confirmed by modification time (everything else predates this phase):

| file | change | why |
|---|---|---|
| `test_quality.rs` | **new**, ~700 lines | the assessment itself |
| `phase_8_quality_tests.rs` | **new**, 15 tests | Step 7 scenarios + Step 8 negatives |
| `lib.rs` | `pub mod test_quality;` + `#[cfg(test)] mod phase_8_quality_tests;` | wiring |
| `execution_plan.rs` | `compile_deterministic`: `fn` → `pub(crate) fn` + 3 doc lines | **reuse, not reimplementation** (rule 8) |
| `ai_generation.rs` | `cosine_similarity`: `fn` → `pub(crate) fn` + 2 doc lines | **reuse, not reimplementation** (rule 8) |

The two visibility changes are the *only* edits to those files — verified by diff. No behaviour
changed in either.

**Not touched:** `integrity.rs` (the 10 ms floor), `store.rs` (`set_task_status`),
`domain.rs`, `engine.rs`, `executor.rs`, `intelligence.rs`, `adaptive_intelligence.rs`,
`api.rs`, `qa_memory.rs`, `qa_memory_risk.rs`. Confirmed: **0** migrations, **0** routes, no risk
weight, threshold or duration constant changed.

---

## 8. Targeted test results

`phase_8_quality_tests`: **15 passed / 0 failed**.

| Step 7 scenario | test |
|---|---|
| complete case with linked requirements | `p8_quality_a_complete_case_is_measured_not_assumed` |
| missing steps / expected results | `p8_quality_an_incomplete_case_is_weak_not_unmeasured` |
| insufficient execution history | `p8_quality_no_execution_history_is_unmeasured_not_perfect_and_not_defective` |
| genuine repeated failures | `p8_infra_genuine_repeated_failures_are_measured` |
| repeated infrastructure errors | `p8_infra_repeated_infrastructure_errors_are_not_a_product_defect` |
| only blocked executions | `p8_infra_blocked_only_executions_are_unmeasured` |
| duplicate / near-duplicate cases | `p8_dup_near_duplicate_cases_are_flagged_and_distinct_ones_are_not` |
| a case with no embedding row | `p8_dup_a_case_with_no_embedding_is_unmeasured` |
| two projects, identical case text | `p8_scope_two_projects_with_identical_case_text_do_not_share_evidence` |
| project sweep containment | `p8_scope_assess_project_returns_only_that_projects_cases` |
| scores reproducible / deterministic | `p8_contract_every_dimension_explains_itself` |
| evidence genuinely unavailable | `p8_contract_there_is_no_aggregate_quality_score` |
| storage-read failure | `p8_contract_a_storage_read_failure_is_not_disguised_as_an_empty_assessment` |
| Phase 7.4 contracts preserved | `p8_contract_the_phase_7_4_metric_contracts_are_preserved` |
| D3 independence | `p8_contract_phase8_does_not_depend_on_either_severity_implementation` |

Affected existing suites: `pre_phase_8_tests` 13, `phase_7_4` 11, `phase_7_3` 14, `phase_7_1` 15,
`qa_memory_risk_tests` 21, `qa_memory_retrieval_tests` 21, `integration_e2e_tests` 20,
`execution_plan` 48, `ai_generation` 90, `api_tests` 316, `risk_signal_tests` 18,
`risk_prioritisation_tests` 20, `qa_memory_planning_tests` 14 — **all 0 failed**.

---

## 9. D3 — severity-contract dependency

**Traced, and the dependency is nil.**

`failure_severity` is produced in two places and consumed only by the `failure_severity` **risk
factor** (weight 0.15) in `calibrate_risk` / `calibrate_partial`. `test_quality.rs` contains **no
reference to `failure_severity`** and no reference to `category_weight`.

This was not luck; it is the design. Quality is a property of the case *definition* plus its
traceability and duplicates. Product-failure severity is a property of an execution *outcome*.
Importing it would have meant Phase 8 silently depending on an unresolved divergence and would have
made D3 a blocker for no benefit.

The execution dimension uses `QaMemoryReader`'s Phase 7.4 helpers, which are independent of
severity. `p8_contract_phase8_does_not_depend_on_either_severity_implementation` asserts the
serialized assessment contains neither string.

**No Phase 8 subtask is blocked by D3.**

---

## 10. Phase 7.4 contracts preserved

| contract | status |
|---|---|
| `completed_failure_rate_fraction` = 0..1, `PASSED`/`FAILED`/`TIMEOUT` | used unchanged; asserted |
| all-recorded `rate` retains its meaning | untouched |
| `INFRA_ERROR` excluded from both halves, tracked separately | asserted; mutation-verified |
| missing evidence stays unmeasured | asserted; mutation-verified |
| no weight / threshold / duration change | verified by diff |

`p8_contract_the_phase_7_4_metric_contracts_are_preserved` builds a case with `FAILED`, `PASSED`,
`INFRA_ERROR` and `BLOCKED` and asserts `completed_count == 2`, `infra_errors == 1`, and
`completed_failure_rate_fraction == 0.5`.

---

## 11. Mutation results — and the test gap it found

Four defects were re-introduced, each **verified applied** before its result was interpreted, then
restored and confirmed byte-identical.

| # | mutation | caught by |
|---|---|---|
| 1 | unmeasured evidence reported as `Adequate` | **5 tests** |
| 2 | a locally re-derived rate that counts `INFRA_ERROR` as a product failure | **2 tests** |
| 3 | telemetry tenant scope replaced with a wildcard | **5 tests** |
| 4 | a telemetry read failure reported as a clean `Adequate` | **1 test** — *only after the test was fixed* |

**Mutation 4 was initially NOT caught, and that was a real defect in the test.**

`p8_contract_a_storage_read_failure_is_not_disguised_as_an_empty_assessment` loaded the case row
*after* closing the pool. `list_test_cases` therefore failed, the row was `None`, and the function
returned early — never reaching the failure path it claimed to test. It passed while the
implementation was mutated to report a read failure as a clean pass.

The test was rewritten to load the row **first** and assert the post-fault behaviour directly. It
now fails against mutation 4. The rewrite is documented in the test's own doc comment so the reason
survives.

Restoration: `test_quality.rs` restored from a SHA-verified backup — expected `01de8660422c196a`,
actual `01de8660422c196a`, **byte-identical**. Zero `MUTATION` markers remain; no backup files
remain.

*Process note:* while running these, I once overwrote a backup before restoring from it, which let
one mutation's logic survive into the next measurement. It was caught by re-reading the source
rather than trusting the output. Every row above reports isolated, verified runs.

---

## 12. The 13 existing user journeys

| | |
|---|---|
| runs | 10 |
| per run | 13 passed / 0 failed |
| total assertions | **130** |
| failures | **0** |

Reassessed, not merely re-run. Phase 8 **adds** a module and **widens two visibility keywords**; it
changes no existing behaviour. The journeys most exposed — `journey_b_execution_to_failure_intelligence`,
`journey_c_change_to_regression_selection`, and the module/release journeys — do not call the
quality assessment, and none asserts a value this phase changes. No journey was skipped or
reclassified.

---

## 13. The 20-run integrity suite

| | |
|---|---|
| runs | 20 / 20 clean |
| per run | 34 passed / 0 failed / 1 ignored |
| failures | **0** |

---

## 14. Full backend, formatting and Clippy

| check | result |
|---|---|
| full suite, 3 consecutive runs | **2085 passed / 0 failed / 5 ignored** each |
| final 3 runs | 2085 / 0 / 5 each |
| `cargo fmt --check` | **1 diff** — `auth.rs:317`, pre-existing, **not fixed** |
| `cargo clippy --all-targets` | **exit 0**, 0 errors, 66 warnings |

Baseline was 2070; `2070 + 15 = 2085`. No existing test was removed, skipped or weakened.

Per-file lints: `test_quality.rs` **0**, `phase_8_quality_tests.rs` **0**, `execution_plan.rs` **0**,
`lib.rs` **0**. `ai_generation.rs` has 2 — both pre-existing (`contains()` style hint, too-many-
arguments), untouched by the visibility change.

---

## 15. Remaining gaps and open decisions

* **D3** — OPEN, unchanged. Two `failure_severity` implementations, same factor name, weight 0.15.
  Phase 8 does not consume either, so it is unaffected. It must still be answered before anything
  reads that factor as one quantity.
* **D5, D6** — OPEN, untouched.
* **`one clear condition per case`** — not separately scored; folded into the two dimensions that
  can be measured (§5). Honest gap, not a silent omission.
* **Negative/boundary coverage** — `NotAssessed`; needs either a new source column or an
  explicitly-labelled heuristic. Both are decisions, not implementation work.
* **`automation_feasibility` is establishment-level** — it says executors exist, not that one fits
  this case. A per-case mapping would need a new authoritative source.
* **Duplicate coverage reaches only APPROVED cases** — `test_case_embeddings` has no row before
  approval, so a project of mostly-GENERATED cases cannot be assessed for duplication at all.
* **No API route exposes the assessment.** Phase 8 implements and verifies the contract; exposing
  it would require a route, which this phase's rules forbid. A consumer-facing endpoint is the
  natural next step.

---

## 16. Final verdict

**PASS.**

Every mandatory roadmap capability is implemented or explicitly reported unanswerable with the
reason named; all required regression checks pass; no capability is blocked by a missing decision.
The PASS gate is met: explainable (§5, rule 3), deterministic where claimed (§6, rule 1), honest
about missing data (§4, rule 4), project-scoped (§8, mutation 3), and it changes no approval or
execution behaviour (§7 — no route, no lifecycle write, no status transition).

Marking PASS does **not** rest on green unit tests alone: §11 records four deliberate mutations and
the one real test defect they exposed.

---

## 17. Recommended next action

**Decide how the quality assessment is consumed.** The contract is implemented, tested and
mutation-verified, but nothing calls it yet. The natural next step is a read-only endpoint
(`GET /intelligence/tests/{id}/quality`) so a UI or the Phase 10 copilot can read it. That needs
authorisation for a route, which this phase did not have.

Before that, the cheapest high-value decision is **D3**. Phase 8 avoided it by design, but any
future consumer of a single "risk" number will not be able to.

Two lower-priority items, both needing a decision rather than code:

1. **`one clear condition` and negative/boundary coverage** — both need either a new source column
   or an explicitly-labelled heuristic. Neither should be guessed.
2. **Embedding coverage for non-approved cases** — duplicate detection currently cannot see a
   `GENERATED` case, which is exactly where duplicates are most likely.

Phase 9 (release readiness) was **not** started and should not be until the above is settled.

---

## Appendix — reproduce

```powershell
cd services/orchestrator
cargo test                          # 2085 passed / 0 failed / 5 ignored
cargo test phase_8_quality_tests    # 15 passed
cargo fmt --check                   # 1 diff, auth.rs:317 (pre-existing)
cargo clippy --all-targets          # exit 0
```

Baseline: **2070** before this phase; `2070 + 15 = 2085`.