# Phase 5 — Release & Risk Intelligence: Report

**Implemented:** 2026-10-07 · **Report finalised:** 2026-10-08
**Phase:** 5 of 6
**Status:** PASS
**Baseline before this phase:** 1777 passed / 0 failed / 2 ignored
**After this phase:** 1795 passed / 0 failed / 2 ignored (+18)

---

## 1. Objective

Score module and release risk from stored history, reusing the scoring AIQA already
has, and report `INSUFFICIENT_DATA` rather than a fabricated risk conclusion.

Delivered in `src/qa_memory_risk.rs` (808 lines) plus 18 tests.

---

## 2. What already existed — and why this phase is mostly derivation

`adaptive_intelligence::calibrate_risk` already computes a weighted, explainable risk
score from six named factors:

```text
35% failure rate + 20% flakiness + 15% severity
     + 10% dependency + 10% healing regression + 10% security
```

Master plan §7 says "risk score if already supported". So the formula is **not new** —
this phase's job is to feed it real, measured factors for a scope it never covered (a
module and a release). The weights are transcribed into `FACTOR_WEIGHTS` so there is
exactly one place the model's shape is described, and a change to `calibrate_risk`
shows up here as a test failure rather than as a silently different score.

---

## 3. A design problem this phase had to solve

### 3.1 The situation

Three of `calibrate_risk`'s six factors have **nowhere to come from** in this schema:

| Factor | Weight | Why unmeasurable |
|---|---|---|
| `dependency_count` | 0.10 | no column or table records a module's or release's dependencies |
| `healing_regression_rate` | 0.10 | no proposal records a regression against a release |
| `security_sensitivity` | 0.10 | no such field exists anywhere |

That is **0.30 of the model** — `STRUCTURALLY_UNAVAILABLE_WEIGHT`.

### 3.2 The two obvious answers, and why both are wrong

| Option | What it would produce | Why rejected |
|---|---|---|
| Pass `0.0` for the unmeasured factors | max achievable score 0.70 — which is **exactly** the `CRITICAL` threshold. A module failing 100% of the time on the worst category would sit *on* the boundary, not above it | silent fabrication. It reports "no dependency risk, no security risk" for a schema that has **never stored either** |
| Renormalise the weights over the measured factors | a clean 0–1 score | implies the missing 30% **does not matter**, which is a different false claim |

### 3.3 What was done instead

Report the weight **and its evidence coverage**, as numbers, and never emit a score from
partial evidence:

```rust
pub measured_weight: f64,          // 0.70
pub unmeasured_weight: f64,        // 0.30  (STRUCTURALLY_UNAVAILABLE_WEIGHT)
pub weighted_contribution: f64,    // 0.35·rate + 0.20·flaky + 0.15·severity
pub calibration: Option<RiskCalibration>,   // always None here
pub partial: bool,
```

`calibrate_or_none` calls the **real** `calibrate_risk` when every factor is measured,
and returns `None` otherwise. In this schema the reachable branch is the *unreachable*
one — the moment a migration adds one of the three columns, full model output starts
working with no change here.

Pinned by `no_zero_to_one_score_is_invented_from_partial_evidence`, which drives 10/10
severe failures — the worst state the measurable factors can express — and asserts the
score is still absent.

---

## 4. Risk surface

| Function | Line | Answers |
|---|---|---|
| `module_risk` | 213 | one module's risk, from its linked cases' executions |
| `release_risk` | 405 | one release's risk, plus a trend against comparable releases |
| `release_risk_overview` | 603 | a project's releases that have executions, ranked by measured contribution |

Each returns `Retrieved<T>`, so an unknown or unexercised subject yields
`INsufficientData` rather than a zero-risk row.

---

## 5. Trend across releases

`trend_from` compares the newest release's failure rate against the mean of the earlier
ones, with a **5-point band** so noise is not reported as movement:

- `worsening` — more than 5 points above the earlier mean
- `improving` — more than 5 points below
- `steady` — within the band
- `None` — fewer than three comparable releases, because one prior release is not a trend

Two properties the suite pins:

- a release with **no executions is omitted**, never counted as a 0% release — otherwise
  an unexercised release would read as the cleanest one
- `trend_points` are ordered oldest first, so the last point is the newest

---

## 6. Tests

18 tests in `src/qa_memory_risk_tests.rs` (841 lines).

| Master plan §7 case | Test |
|---|---|
| Module Risk with historical failure data | `a_module_with_failing_history_reports_its_measured_factors` |
| Release Risk with regression history | `a_release_with_regression_history_reports_its_own_counts` |
| INSUFFICIENT_DATA when no history | `no_history_means_no_risk_conclusion_not_a_low_one`, `a_module_whose_cases_never_ran_reports_no_matching_history`, `an_unknown_release_id_is_unknown_not_a_zero_result` |
| Risk increases with failures | `a_module_with_more_recent_failures_weighs_more` |
| Risk scores explainable | `every_factor_names_itself_its_weight_and_its_evidence` |
| Comparisons across releases | `releases_are_ranked_by_their_measured_contribution` |
| Additional | `no_zero_to_one_score_is_invented_from_partial_evidence`, `the_weight_table_sums_to_one_and_matches_the_documented_formula`, `a_release_that_got_worse_is_reported_as_worsening`, `a_release_that_cleared_is_reported_as_improving`, `an_unchanged_rate_is_steady`, `a_release_with_no_executions_is_never_counted_as_a_clean_release`, `a_module_that_does_not_exist_is_unknown_not_low_risk`, `a_failure_without_a_category_leaves_severity_unmeasured`, `one_projects_risk_is_never_visible_from_another`, `repeated_risk_reads_return_identical_answers`, `no_risk_view_carries_a_field_that_looks_like_hidden_reasoning` |

```
cargo test --lib qa_memory_risk
  → 18 passed / 0 failed
```

---

## 7. A second real defect, and a corrected test

### 7.1 Production: a release appeared twice in its own trend

`release_risk` looped `for other in releases.iter()` **without excluding the target**,
then appended the target again at the end. Two real consequences:

- the point count was inflated (a test expecting 3 points got 4)
- `trend_from` averaged the newest release against **itself** as one of the "earlier"
  points, so the reported direction of travel was computed against a duplicated input

Phase 3's `release_history` had the correct `.filter(|r| r.id != release_id)`;
Phase 5's `release_risk` did not. Fixed, with a comment saying why.

### 7.2 Test: a marginal assertion, corrected to be exact

`no_zero_to_one_score_is_invented_from_partial_evidence` asserted
`weighted_contribution > 0.5` and failed at exactly 0.50. The arithmetic is right:

```text
failure_rate    1.0 × 0.35 = 0.35   (10 of 10 failed)
flakiness_score 0.0 × 0.20 = 0.00   (consistently failing is NOT flaky)
failure_severity 1.0 × 0.15 = 0.15   (every failure is DATA_FAILURE)
                 = 0.50 of the 0.70 measured weight
```

Corrected to assert the exact value rather than a threshold. That turned the test into a
better one: it now pins the arithmetic and records why flakiness is zero.

### 7.3 A property of the existing model worth flagging

The exact-value assertion makes visible something the plan did not anticipate:

> **A module that fails every single time scores lower on the measurable factors than
> one that fails intermittently**, because the model spends 20% of its weight on
> flakiness and consistent failure is not flaky.

That is `calibrate_risk`'s existing shape, not something introduced here. Changing it
would mean reweighting a scoring function that other surfaces depend on, which is
outside this plan. It is pinned by test and recorded in the final report rather than
silently accepted.

---

## 8. Regression

```text
before Phase 5:  1777 passed / 0 failed / 2 ignored
after  Phase 5:  1795 passed / 0 failed / 2 ignored
```

No existing test weakened. `qa_memory.rs` had seven private helpers widened to
`pub(crate)` so Phase 5 could reuse them rather than reimplementing rate/window/failure
classification — a deliberate reuse, not a duplication.

---

## 9. Known limitations carried forward

1. **No `RiskCalibration` is ever emitted** for a module or release, because 30% of the
   model is structurally unmeasurable. A future migration adding any one of the three
   columns unlocks it with no code change.
2. **The severity weighting table is a stated convention.** `category_weight` assigns
   1.0 to `DataFailure`, 0.95 to auth failures, down to 0.3 to `ClientError`. It is
   documented and testable, but it is a judgement, not a measurement.
3. **`healing_regression_rate` is unmeasured even though healing exists.** The store has
   `healing_strategy_metrics` and `verification_result`, but nothing ties a regression
   to a module or release.
4. **No endpoint exposes any of this.** Store and service capability only.

---

## 10. Phase 5 PASS gate

| Criterion | Status |
|---|---|
| Module risk from history | yes — 18 tests |
| Release risk with regression history | yes |
| INSUFFICIENT_DATA, not a fabricated conclusion | yes — never a zero-risk row |
| No second scoring formula | yes — the existing `calibrate_risk` |
| Model coverage reported honestly | yes — `measured_weight` / `unmeasured_weight` |
| Unmeasured factors are `None`, never 0.0 | yes — asserted per factor |
| Comparisons across releases | yes — ranked, unexercised releases omitted |
| Risk rises with failures | yes |
| Scores explainable | yes — name, weight, basis, evidence per factor |
| Project isolation | yes |
| No chain-of-thought | yes |
| Regression passes | yes — 1777 → 1795 |

**Phase 5: PASS.**