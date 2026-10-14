# PHASE 7.4 — RISK-METRIC IMPLEMENTATION REPORT

**Completed failure rate as the risk factor's input, with infrastructure errors tracked separately.**

Repository: `https://github.com/abhisummer22-design/AIQA.git`
Branch: `main` · HEAD: `7930507` · 0 ahead / 0 behind `origin/main`
Predecessors: `PHASE_7_1_REMEDIATION_REPORT.md`, `PHASE_7_2_REMAINING_GAPS_AUDIT.md`,
`PHASE_7_3_DECISION_IMPLEMENTATION_REPORT.md`

## Verdict: **PASS**

All in-scope risk consumers use the completed-execution rate. `INFRA_ERROR` is excluded from
both halves of it and remains independently visible. The all-recorded rate is unchanged
everywhere it is reported. Missing measurements stay unmeasured. No weight or threshold was
introduced or moved.

One in-scope consumer was **re-targeted rather than left passing**: a Phase 7.3 assertion that
deliberately pinned the *old* behaviour is now retargeted, and its reasoning is in §11.

---

## 1. Initial repository state and baseline

Measured at the start of this phase, not taken from the previous report.

| | |
|---|---|
| branch / HEAD | `main` @ `7930507` |
| ahead / behind `origin/main` | 0 / 0 |
| modified tracked files | 23 |
| untracked files | 13 |
| deleted | 0 |
| `C:` free | 67 GB (the disk exhaustion of the previous phase had cleared) |
| `H:` free | 21.2 GB |

### Baseline test results, re-measured before any change

The first full-suite run reported `2045 passed; 1 failed` — `journey_tests::journey_b_execution_to_failure_intelligence`.
That was **not** accepted and **not** worked around. It was investigated: the test passed in
isolation (1/1), passed as a whole module (13/13), and passed on the next full-suite run. Three
further consecutive full-suite runs were then taken, all clean. The earlier disk-full condition
on `C:` (every integration test puts its SQLite file in `std::env::temp_dir()`, which defaults to
`C:`) is the plausible cause, and it was verified by repetition rather than assumed.

| run | result |
|---|---|
| baseline 1 | 2045 passed / **1 failed** (`journey_b`) |
| baseline 2 | 2046 passed / 0 failed |
| baseline 3 | 2046 passed / 0 failed |
| baseline 4 | 2046 passed / 0 failed |
| `journey_b` in isolation | 1 passed |
| `journey_tests` as a module | 13 passed |

**Stable baseline: 2046 passed / 0 failed / 5 ignored.** Every number in this report is from a
run in this phase; none is reused from an earlier report.

---

## 2. Exact risk-scoring consumers changed

Five production sites construct the `failure_rate` **risk `Factor`**. All five were changed.

| # | site | file / line | reaches |
|---|---|---|---|
| 1 | `module_risk` | `qa_memory_risk.rs:322` | `calibrate_or_none` (`:373`) |
| 2 | `release_risk` | `qa_memory_risk.rs:531` | `calibrate_or_none` (`:600`) |
| 3 | `release_factors` (trend loop) | `qa_memory_risk.rs:894`, fed from `:612` | `calibrate_or_none` (`:632`) |
| 4 | `test_risk_from` | `qa_memory_risk.rs:1546` | `calibrate_or_none` (`:1614`) |
| 5 | `get_project_risk_handler` | `api.rs:9315` | `calibrate_partial` (`:9418`, `:9428`) |

Site 3 is the one a narrower search would miss: `release_factors` is a private helper that took
a precomputed `f64`, and it is called from the **trend loop** that scores every *other* release.
Before the change, that one variable (`orate`) fed both the reported trend point and the risk
factor. Missing it would have left the target release correct and every historical point wrong —
and no single-assertion test on the target would have seen it. §5.C is the test that covers it.

### The audit Step 2 asked for

For each path, before the change:

| path | numerator | denominator | `INFRA_ERROR` affected it? | missing evidence represented as |
|---|---|---|---|---|
| `module_risk` | `FAILED`\|`TIMEOUT` | **all recorded rows** | denominator only | measured `0.0` |
| `release_risk` | `FAILED`\|`TIMEOUT` | **all recorded rows** | denominator only | measured `0.0` |
| trend loop → `release_factors` | `FAILED`\|`TIMEOUT` | **all recorded rows** | denominator only | measured `0.0` (factor hard-typed `f64`) |
| `test_risk_from` | `FAILED`\|`TIMEOUT` | **all recorded rows** | denominator only | `Some(0.0)` |
| `api.rs` project risk | `FAILED`\|`TIMEOUT`\|**`INFRA_ERROR`** | **all recorded rows** | numerator **and** denominator | measured `0.0` |

Two different defects, which is why Decision 1 and Decision 2 are separate:

* the `qa_memory_risk` paths put `INFRA_ERROR` in the **denominator**, so it *diluted* the factor;
* the `api.rs` path put it in the **numerator**, so it *inflated* the factor.

Note also that the guard in `api.rs` was `total_runs > 0`, which a project whose executions were
*all* infrastructure-errored satisfies — it reported a measured `0.0` for a project that has never
had a passing test. That guard is now on `failure_rate` being `Some`.

### Consumers deliberately **not** changed

Found by an exhaustive scan of every `INFRA_ERROR` / `failure_rate` occurrence in production code,
then classified by contract:

| site | contract | why left alone |
|---|---|---|
| `adaptive_intelligence.rs:448` | failure categorisation / fingerprinting | an `INFRA_ERROR` has a real error message worth categorising; changing it alters the failure-cluster contract (Q3-adjacent, not authorised) |
| `adaptive_intelligence.rs:977` | failure clustering (`fail_counts`, `selector_failures`, `cluster_counts`) | same family |
| `operations.rs:593`, `:619` | `build_environment_matrix` — verified to feed **no** `Factor`, **no** `calibrate_*`, **no** `risk_score` | an operational diagnostic with its own denominator |
| `openapi_api.rs:429` | evidence display ("what a case that did not pass got back") | an `INFRA_ERROR` did not pass; showing its logs is correct |
| `api.rs:5809` | `retry-failed` selection | Step 4: "retryability metrics" keep their existing contract. This is where infrastructure *retries* belong. |
| `qa_memory.rs::TestStabilityView::failure_rate` | **already** completed-based | corrected in Phase 7.1 (P7-D2). Left untouched — see §5 note. |

---

## 3. Before / after source of the failure-rate factor

**Before.** Four of the five sites wrote `failures as f64 / rows.len() as f64` (two of them
`.clamp(0.0, 1.0)`), and the fifth, `api.rs`, wrote `failed_runs / total_runs` with
`failed_runs` matching `FAILED | TIMEOUT | INFRA_ERROR`. Four separate copies of the same
arithmetic, one of which disagreed with the other three about what a failure is.

**After.** One definition, called by all five:

```rust
// qa_memory.rs
pub(crate) fn completed_failure_rate_fraction(rows: &[&ExecutionTelemetryRow]) -> Option<f64> {
    let completed = Self::completed_count(rows);
    let failures = rows.iter().filter(|r| Self::is_failure(r)).count();
    Self::rate(failures, completed).map(|pct| pct / 100.0)
}
```

### Why a second function rather than reusing `completed_failure_rate` directly

`completed_failure_rate` returns **0..100** (it shares `rate()`, which multiplies by 100). The risk
model's factors are **0..1** (`Factor::contribution` is `weight * measured`, feeding a 0..1 score).
Feeding 50.0 where 0.5 belongs would have been a silent 100× overstatement — so the scale
conversion had to exist somewhere, and putting it in one named function means it cannot be
forgotten at a sixth call site later. Two functions, one denominator.

`completed_count(rows)` was extracted for the same reason: a `Factor` must state how many
observations its value was taken over, and every consumer quotes the same eligibility count rather
than re-deriving it.

**Denominator logic is defined once.** `is_pass`, `is_failure`, `completed_count`,
`completed_failure_rate`, `completed_failure_rate_fraction` and `infra_errors` are all in
`qa_memory.rs`; none of the five risk consumers contains an eligibility predicate.

---

## 4. The `completed_failure_rate` contract and timeout treatment

Established in Phase 7.3 and **preserved exactly**; not redefined, not reclassified.

```
numerator   = rows matching is_failure  →  FAILED | TIMEOUT
denominator = rows matching is_pass || is_failure  →  PASSED | FAILED | TIMEOUT
result      = numerator / denominator * 100        (None when denominator == 0)
```

### How `TIMEOUT` contributes — stated explicitly, as Step 5 C requires

`is_failure` matches `"FAILED" | "TIMEOUT"`, so a timeout is in the **numerator**.
`is_pass` matches `"PASSED"` only, and the denominator is `is_pass || is_failure`, so a timeout is
in the **denominator** too.

**A `TIMEOUT` counts as a completed execution that did not pass.** It stays in both halves. The
rationale in the code: the step ran and did not succeed, which is what a failure rate is about.
This is the pre-existing behaviour and was not altered.

`INFRA_ERROR` is neither a pass nor a failure, so it is in **neither** half — which is exactly the
distinction Decision 2 draws: a timeout is a verdict, an infrastructure error is the absence of one.

Verified numerically in `p74_c_mixed_outcomes_follow_the_the_established_contract` with a fixture
containing every outcome type (see §5.C).

---

## 5. INFRA_ERROR handling, before and after

### Creation → persistence

Audited and found **already correct**; no executor change was needed or made. Step 4 says to avoid
changing executor behaviour when the outcome already carries the right classification, and it does:

* `executor.rs` — 13 sites produce `TaskStatus::InfraError` (simulated infra outcome, missing
  browser, missing display, dead subprocess, unavailable executor, name-resolution failure,
  restart-recovery sweep, timeout escalation).
* `autopilot_execute.rs:635` — the no-browser path yields `"INFRA_ERROR"`, never a synthetic
  `FAILED`.
* `domain.rs:52` — `TaskStatus::InfraError` serialises to `"INFRA_ERROR"`; parsing is the inverse.
* `engine.rs:478` — a run containing an infra outcome is `Partial`, never `Failed`.

So the classification is right at every origin and is persisted unchanged; only the **downstream
aggregation** was wrong.

### Aggregation → risk consumption

| | before | after |
|---|---|---|
| `qa_memory_risk` paths | in the denominator (diluting) | in neither half |
| `api.rs` path | in numerator *and* denominator (inflating) | in neither half |
| separately counted? | yes (`infra_errors`, added in 7.3) | yes — unchanged, still reported |
| risk weight for infrastructure | none | **none** |
| ever silently becomes a pass? | no | no — asserted directly (§5.B) |
| ever erased from reporting? | no | no — asserted directly (§5.B) |

`api.rs`'s handler previously re-derived the infrastructure count with its own `filter`; it now
calls `QaMemoryReader::infra_errors`, the same helper every view uses.

---

## 6. Deterministic before / after comparisons

All values measured by running the real aggregation and scoring code, not computed by hand.

### 6.A `test_risk` — the factor and its weighted contribution

Captured before the change by a temporary probe against the real `test_risk` path, then again
after, on identical fixtures:

| fixture | factor before | factor after | weighted contribution before → after | measured weight before → after |
|---|---|---|---|---|
| 1× `FAILED` | 1.000000 | 1.000000 | 0.417500 → 0.417500 | 0.500 → 0.500 |
| `FAILED`, `PASSED` | 0.500000 | 0.500000 | 0.442500 → 0.442500 | 0.700 → 0.700 |
| `FAILED`, `PASSED`, `BLOCKED` | 0.333333 | **0.500000** | 0.384167 → **0.442500** | 0.700 → 0.700 |
| `FAILED`, `PASSED`, 2× `BLOCKED` | 0.250000 | **0.500000** | 0.355000 → **0.442500** | 0.700 → 0.700 |
| 2× `FAILED`, `INFRA_ERROR` | 0.666667 | **1.000000** | 0.300833 → **0.417500** | 0.700 → 0.700 |
| `FAILED`, `TIMEOUT`, `BLOCKED` | 0.666667 | **1.000000** | 0.304583 → **0.421250** | 0.700 → 0.700 |
| `BLOCKED` only | 0.000000 *(measured)* | **unmeasured** | 0.000000 → 0.000000 | 0.350 → **0.000** |
| `INFRA_ERROR` only | 0.000000 *(measured)* | **unmeasured** | 0.000000 → 0.000000 | 0.350 → **0.000** |

The first two rows are controls: every row completed, so nothing moves. That is what makes the
other six interpretable.

The last two rows are the most important. A case whose executions were all blocked or all
infrastructure-errored reported a **measured 0.0** failure rate — indistinguishable from a test
that passes every time — and claimed 0.35 of the model as measured on the strength of it. It is now
`unmeasured`, with the reason in `Factor::basis` and the weight moved to the unmeasured column.

### 6.B The project-risk endpoint — `risk_score`

Measured by running the real handler over HTTP before and after, on two fixtures:

| fixture | rows | factor before | factor after | `risk_score` before → after | level before → after |
|---|---|---|---|---|---|
| `diluted` | `FAILED`, `PASSED`, `BLOCKED`, `BLOCKED` | 0.25 | **0.5** | 0.46 → **0.57** | MEDIUM → **HIGH** |
| `inflated` | `FAILED`, `FAILED`, `PASSED`, `INFRA_ERROR` | 0.75 | **0.6667** | 0.68 → **0.64** | HIGH → HIGH |

Both numbers come from the endpoint's own response, captured with the old input in place and again
with the new one. `measured_weight` is 0.80 in all four responses and `weighted_contribution`
moves 0.3675 → 0.455 (`diluted`) and 0.5425 → 0.5133 (`inflated`).

Note the **level changed** on `diluted`, MEDIUM → HIGH. That is the point of the correction: a
project whose failures were being diluted by steps that never ran was being reported a band too
low. Banding is unchanged — §8.

---

## 7. Explanation of every observed risk-score change

Every change traces to the input measurement and nothing else.

1. **Blocked / skipped / cancelled steps** previously sat in the denominator while contributing
   nothing to the numerator, pulling the factor toward zero. Removing them from the denominator
   raises the factor. (`diluted`: 0.25 → 0.5, score 0.46 → 0.57.)
2. **`INFRA_ERROR`** previously sat in the `api.rs` numerator as if it were a product-test failure,
   pulling the factor toward one. Removing it lowers the factor. (`inflated`: 0.75 → 0.667, score
   0.68 → 0.64.)
3. **`INFRA_ERROR`** in the `qa_memory_risk` paths sat only in the denominator, pulling the factor
   down. Removing it raises the factor.
4. **No completed observation** previously produced a measured `0.0`, which made a test that never
   ran look perfect. It is now `unmeasured`, so the factor contributes nothing and
   `measured_weight` drops by that factor's weight.

Nothing else moved. §5.E asserts this directly: two fixtures with identical *completed* evidence
and different blocked / infrastructure noise must agree on all five non-`failure_rate` factors,
and the test fails if any of them differs.

---

## 8. Weights and thresholds unchanged

Decision 4. Asserted structurally in `p74_e_weights_and_thresholds_are_unchanged`, so a future
edit that moves one fails the suite rather than passing quietly:

| factor | weight |
|---|---|
| `failure_rate` | **0.35** |
| `flakiness_score` | 0.20 |
| `failure_severity` | 0.15 |
| `dependency_count` | 0.10 |
| `healing_regression_rate` | 0.10 |
| `security_sensitivity` | 0.10 |

`RISK_LEVEL_THRESHOLDS` — `CRITICAL 0.70`, `HIGH 0.50`, `MEDIUM 0.25`, `LOW 0.0` — asserted
unchanged.

`Factor::unmeasured("failure_rate", …)` still carries weight `0.35`: the model must know how much
weight that factor *would* have had, which is what makes `measured_weight` a truthful statement.

`calibrate_partial`, `calibrate_risk`, `calibrate_or_none` and `FACTOR_WEIGHTS` were **not**
modified. No new weight was introduced for infrastructure reliability — asserted in §5.B by
checking the model still has exactly six factors and none is named after infrastructure.

---

## 9. The original rate metric is unchanged

Decision 3. `QaMemoryReader::rate`, `is_pass`, `is_failure` and `completed_failure_rate` are
untouched. `api.rs`'s local re-derivation was **removed**, so there is one fewer copy of the
arithmetic rather than one more.

Fields that keep their all-recorded meaning, each asserted in this phase's tests:

| field | contract | asserted by |
|---|---|---|
| `ModuleRiskHistory::failure_rate` | all recorded | `p74_a` (via `QaMemoryReader::rate`) |
| `AffectedTestRisk::failure_rate` | all recorded | `p74_e_module_risk` — 25.0 where the factor is 50.0 |
| `ReleaseRisk::failure_rate` | all recorded | `p74_e_release_risk` — 33.33 where the factor is 50.0 |
| `ReleaseTrendPoint::failure_rate` | all recorded | `p74_e_release_risk` — every point, so the trend stays internally comparable |
| `Signal` statements (`module_failure_rate`, `release_failure_rate`) | all recorded | re-expressed via `QaMemoryReader::rate`; wording unchanged |
| `api.rs` `completed_failure_rate`, `completed_executions`, `infrastructure_errors` | unchanged in name and value | `p74_f`, over HTTP |

**One pre-existing inconsistency, recorded rather than fixed.** The API response key
`completed_failure_rate` is a **0..1 fraction**; the identically-named field on the QA-memory view
structs is a **0..100 percentage**, because it shares `rate()`. This dates from Phase 7.3 and is
visible in the response above (`"completed_failure_rate":0.6666666666666665`). Unifying them would
be a breaking response change and is not authorised here. Both are now computed from one
definition, so the difference is purely the scale.

---

## 10. Files changed, and why

| file | change |
|---|---|
| `qa_memory.rs` | **+2 functions.** `completed_failure_rate_fraction` (0..1, for risk factors) and `completed_count` (the shared eligibility count). No existing function's behaviour altered. |
| `qa_memory_risk.rs` | All 4 risk factors now take the shared completed rate; `None` → `Factor::unmeasured`. `release_factors` takes `Option<f64>`. Four reported figures re-expressed through `QaMemoryReader::rate` so they keep the all-recorded contract. `TestRisk::failure_rate` now tracks its own factor. |
| `api.rs` | The project-risk factor takes the shared completed rate; `None` → `unmeasured` (the guard was `total_runs > 0`, satisfied by a project where everything was infrastructure-errored). The local re-derivation of the infrastructure count and the completed rate was deleted in favour of the shared helpers. |
| `lib.rs` | `mod phase_7_4_decision_tests;` (+ doc comment). |
| `phase_7_3_decision_tests.rs` | **One assertion re-targeted.** See §11. |
| `phase_7_4_decision_tests.rs` | **New**, 11 tests. |

No migration. No route. No change to `store.rs`, `domain.rs`, `engine.rs`, `executor.rs`,
`autopilot_execute.rs`, `intelligence.rs`, `adaptive_intelligence.rs`, or the 10 ms duration floor.

---

## 11. The one re-targeted existing assertion

`phase_7_3_decision_tests::p73_q1_affected_test_risk_reports_both_rates_per_case` failed with
`left: Some(0.5), right: Some(0.3333333333333333}`.

It asserted:

```rust
assert_eq!(fr.measured, Some(1.0 / 3.0),
    "requirement 5: and it still carries the ALL-RECORDED value (1 failure of 3 rows), not the \
     completed-execution one. The weighted model's inputs are unchanged in this phase.");
```

**This assertion existed to prove Phase 7.3 had *not* rewired the model's inputs.** Decision 1
subsequently authorised exactly that rewiring. The value it was built to freeze is the value the
approved change was required to move. The fixture is one `FAILED`, one `PASSED`, one `BLOCKED` —
the blocked step counted in the denominator and diluted the factor by a third.

It now asserts `Some(0.5)`, plus three **additional** assertions Phase 7.3 had no reason to make:

* `fr.evidence == 2` — the evidence count is the completed denominator, not the 3 recorded rows;
* `fr.basis` contains `"completed executions"` — the basis names its denominator;
* `fr.weight == 0.35` — Decision 4.

Every other assertion in that test was kept: the factor must still exist, must still be measured,
and `mv.unmeasured_weight > 0.0` (D3 still open). The assertion was **strengthened, not relaxed**.

---

## 12. Tests added

`services/orchestrator/src/phase_7_4_decision_tests.rs` — 11 tests, all driving real
application aggregation and scoring code (`test_risk`, `module_risk`, `release_risk`, the real HTTP
router). No test re-implements the arithmetic it checks; a copied formula would pass while the
shipped one is wrong, which is the defect class this phase exists to close.

| Step 5 item | test |
|---|---|
| **A** blocked | `p74_a_blocked_outcomes_do_not_dilute_the_completed_rate` |
| **B** infra (all 5 bullets) | `p74_b_infra_error_is_counted_separately_and_in_neither_half` |
| **C** mixed | `p74_c_mixed_outcomes_follow_the_established_contract` |
| **D** unmeasured | `p74_d_nothing_completed_stays_unmeasured_and_is_not_a_fabricated_zero` |
| **E** impact | `p74_e_test_risk_factor_is_the_completed_rate_with_recorded_before_and_after` (8 fixtures, §6.A) |
| **E** module | `p74_e_module_risk_factor_is_the_completed_rate` |
| **E** release + **trend loop** | `p74_e_release_risk_and_trend_factors_are_the_completed_rate` |
| **E** weights/thresholds | `p74_e_weights_and_thresholds_are_unchanged` |
| **E** no leakage | `p74_e_only_the_failure_rate_factor_is_affected_by_this_change` |
| **F** compat | `p74_f_project_risk_response_keeps_both_rates_with_their_own_scales` (over HTTP) |
| **F** compat | `p74_f_all_recorded_reporting_still_counts_every_row` |

### How the tests prove the paths *consume* the metric

* Each asserts the factor equals the value `completed_failure_rate_fraction` returns for that
  evidence — so it proves the factor and the shared definition cannot diverge.
* `p74_b` is the sharpest: it compares a fixture **with** an `INFRA_ERROR` against an otherwise
  identical one **without**, and requires the factors to be **equal**. Equality proves the infra
  row is in neither the numerator (which would raise it) nor the denominator (which would lower
  it), and since `0.5 ≠ 0.0` it cannot have become a pass.
* `p74_e_release_risk` proves the **trend loop** — the one site with no visible factor — by reading
  the `risk_score` it produced, using a categorised failure so `calibrate_or_none` does not refuse.
  The expected score is computed from the documented formula with the *other* factors read off the
  target's own identical rows, and the test asserts the observed score matches the completed-rate
  variant **and does not match** the all-recorded variant.

### Step 5 G — regression guards

Not re-implemented; these are existing suites, run rather than duplicated. Results in §13.

---

## 13. Regression and sensitivity results

### Targeted

| suite | result |
|---|---|
| `phase_7_4_decision_tests` | **11 passed / 0 failed** |
| `phase_7_3_decision_tests` | **14 passed / 0 failed** |
| `phase_7_1_regression_tests` | **15 passed / 0 failed** |
| `qa_memory_risk_tests` | **21 passed / 0 failed** |
| `qa_memory_planning_tests` (incl. the `release_regression` signal) | **14 passed / 0 failed** |
| `qa_memory_validation_tests` | 7 passed / 0 failed |
| `qa_memory_perf_tests` | 7 passed / 0 failed |
| `risk_prioritisation_tests` | 20 passed / 0 failed / 1 ignored |
| `trace_chain_tests` | 14 passed / 0 failed |
| `rbac_matrix_tests` | 15 passed / 0 failed |
| `api_tests` | 316 passed / 0 failed |

### Phase 6.2 integration

`integration_e2e_tests`: **20 passed / 0 failed**.

### Journeys — 13, ten repetitions each

| | |
|---|---|
| runs | 10 |
| per run | 13 passed / 0 failed |
| total journey assertions | **130** |
| failures | **0** |

Every journey is exercised by `journey_tests` against real persistence; none was skipped, none was
re-classified as "not applicable", and none is claimed to pass merely because the suite passed —
each was reassessed against the change, and the journeys that traverse risk scoring
(`journey_b_execution_to_failure_intelligence`, `journey_c_change_to_regression_selection`,
`journey_f_*`) were checked specifically for the failure-rate input change. No journey asserts a
numeric risk score, so none required re-baselining; where a journey's evidence includes
blocked or infrastructure-errored steps, the completed rate it now reports is the correct one.

### Integrity — 20 consecutive runs

| | |
|---|---|
| runs | 20 / 20 clean |
| per run | 34 passed / 0 failed / 1 ignored |
| total failures | **0** |

### Full backend suite

| run | result |
|---|---|
| 1 | **2057 passed / 0 failed / 5 ignored** |
| 2 | 2057 passed / 0 failed / 5 ignored |
| 3 | 2057 passed / 0 failed / 5 ignored |

Baseline was 2046; `2057 − 2046 = 11`, exactly the 11 tests added. No pre-existing test was
removed or skipped.

### Formatting and Clippy

| check | result |
|---|---|
| `cargo fmt --check` | **1 diff** — `auth.rs:317`, pre-existing, **unchanged and not fixed** (rustfmt is non-idempotent on the `concat!` inside `role_gate!`; proven at HEAD in an earlier phase) |
| `cargo clippy --all-targets` | **exit 0**; 0 errors; 67 pre-existing warnings |
| lints in the 6 changed files | `qa_memory.rs` 0 · `qa_memory_risk.rs` 0 · `api.rs` 0 · `lib.rs` 0 · `phase_7_3_decision_tests.rs` 0 · `phase_7_4_decision_tests.rs` 0 |

---

## 14. Mutation testing — the tests were checked against the defect

A passing suite is not proof of semantic correctness, so four defects were re-introduced into the
finished code and the new tests re-run. **Each mutation was verified applied before its result was
interpreted.**

| # | mutation | caught by |
|---|---|---|
| 1 | `completed_failure_rate_fraction` denominator reverted to `rows.len()` (all recorded) | **9 of 11** tests |
| 2 | `is_failure` extended to include `INFRA_ERROR` | **8 of 11** |
| 3 | `None` mapped to `Some(0.0)` (fabricate a measured zero) | **2** — `p74_d`, `p74_e` table |
| 4 | `api.rs` factor reverted to `failed_runs / total_runs` | **1** — `p74_f` (after the fix below) |

**Mutation 4 was initially NOT caught, and that was a real defect in the test.**

`p74_f_project_risk_response_…` used a single fixture of `FAILED + PASSED + BLOCKED + INFRA_ERROR`.
That fixture reads **0.5** under the correct completed contract *and* **0.5** under the old one —
the `INFRA_ERROR` inflating the numerator exactly cancels the `BLOCKED` diluting the denominator.
The test passed with the fix removed. Two error directions masking each other is precisely the
failure mode a mutation check exists to surface.

The test was rebuilt around **two** fixtures that separate the directions:

| fixture | rows | completed | old all-recorded |
|---|---|---|---|
| `diluted` | F, P, BLOCKED, BLOCKED | 1/2 = 0.5 | 1/4 = 0.25 |
| `inflated` | F, F, P, INFRA_ERROR | 2/3 = 0.667 | 3/4 = 0.75 |

Re-run against mutation 4, it fails. It was then re-verified against the final code. This was
caught by the mutation exercise, not by review.

**Restoration.** All four mutations were reverted from SHA-verified backups. `api.rs` restored to
`bb0f62dc44b222c2` and `qa_memory.rs` to `fe2f34732d3d82e4`, both byte-identical to their
pre-mutation state. Zero `MUTATION` markers remain in the six changed files; the 28 remaining
hits in the tree are pre-existing prose in other files. No `.p74bak` files remain.

---

## 15. Remaining open questions

**D3 — canonical risk formula: OPEN, untouched.** No scoring function was merged. Three of the six
factors (0.30 of the model) have no column anywhere in the 32 migrations and are structurally
unmeasured. `calibrate_partial` normalises over what *is* measured and states its neutrality
assumption in `explanation`; `calibrate_or_none` refuses a score entirely. This phase changed one
factor's **input** and did **not** settle which formula is canonical. No claim is made that the risk
formulas are now canonical.

**D5 — duration tolerance: OPEN, untouched.** The 10 ms floor is unchanged; no duration metric was
reinterpreted.

**D6 — `set_task_status` API: OPEN, untouched.** No enum, no redesign. The existing boolean calling
patterns still behave as before — the fixtures in this phase call it the same way the existing
suites do.

**Also still open, and not resolved here:**

* **`failure_severity` still sees `INFRA_ERROR`.** `api.rs:9243` raises severity to `0.8` when an
  `INFRA_ERROR` or `TIMEOUT` is present. Decision 1 authorises changing the **failure-rate** factor's
  input; it does not authorise recomputing a different factor, and Decision 4 forbids unrelated
  scoring changes. Left as-is and reported rather than silently altered. If the intent is that the
  risk model measures *test* reliability only, this is the next site to decide on — and it should be
  decided together with D3, since it is the same underlying question.
* **`TestStabilityView::failure_rate` was already completed-based** (Phase 7.1). Worth stating
  because it means the codebase had two contradictory answers for "failure rate" for three phases:
  the view said completed, the risk factor said all-recorded. This phase ends that split — but a
  reader auditing earlier reports should know the disagreement predates Phase 7.4.
* **Module-change support (Q4)** remains deferred pending an authoritative module-identity source.

---

## 16. Consumers that remain blocked or unmodified

No in-scope consumer is blocked. Every one of the five uses the completed-execution rate.

Unmodified, by design, with contracts that are not the failure-rate factor: the six sites in §2's
table, plus the 28-file scan confirming no other `Factor::measured("failure_rate", …)` exists in
production code.

One verification worth recording: a scan for any remaining `failure_rate` computed over
`rows.len()` or `total_runs` found exactly one hit, `operations.rs:593` — confirmed by reading its
enclosing function (`build_environment_matrix`) to feed no `Factor`, no `calibrate_*` call and no
`risk_score`. Not a risk-scoring consumer.

---

## 17. Completion gate

| condition | status |
|---|---|
| all in-scope risk consumers use the correct `completed_failure_rate` input | ✅ 5 of 5, each with a test that fails if the input is reverted |
| `INFRA_ERROR` excluded from product completed-failure calculations | ✅ neither half |
| …and remains independently visible | ✅ `infra_errors` unchanged, asserted over HTTP and per-view |
| existing all-recorded rate remains compatible | ✅ 5 reported fields + both API keys, each asserted |
| missing measurements remain unmeasured | ✅ `None` → `Factor::unmeasured`; `p74_d` |
| no new weight or threshold introduced | ✅ all six weights + all four thresholds asserted |
| numeric and integration regression tests pass | ✅ 11 new; full suite 2057/0/5 |
| all 13 journeys reassessed | ✅ 10 repetitions each, 130 assertions, 0 failures |
| full backend suite, formatting, Clippy pass | ✅ 2057/0/5 · 1 pre-existing fmt diff · clippy exit 0 |
| final diff contains only intentional changes | ✅ §10; mutations restored SHA-byte-identical |

**PASS.**

---

## 18. Recommended next action

**Start Phase 8 (test-case quality intelligence).** It reads exactly the factors this phase
corrected, so its inputs are now trustworthy: `failure_rate` measures how often a test fails *when
it runs*, rather than how often it fails or fails to run — a distinction Phase 8's quality scoring
would otherwise have inherited silently.

Two things to settle **before or during** Phase 8, both cheap now and expensive later:

1. **`failure_severity` and `INFRA_ERROR`** (§15). One decision, same question as D3. Phase 8 will
   read that factor too, so it should be answered before Phase 8 interprets severity.
2. **The `completed_failure_rate` scale inconsistency** between the API response (0..1) and the
   QA-memory view structs (0..100). Phase 8 will surface both. Unifying is a breaking response
   change and needs its own authorisation.

Not recommended now, neither blocking Phase 8: D5's empirical validation of real inter-write
timing, and D6's `set_task_status` enum refactor.

**Also outstanding, unrelated to the next phase:** `docs/next-intelligence/PHASE_7_PASS.md` — the
roadmap deliverable closing Phase 7. It was deliberately not created by this phase, which is an
implementation report rather than a phase verdict. Phase 7 stands at PASS under the documented
supported-target scope (Q3: `requirement` / `test_case` / `failure_cluster`).

---

## Appendix — how to reproduce the key numbers

```powershell
cd services/orchestrator
cargo test                        # 2057 passed / 0 failed / 5 ignored
cargo test phase_7_4_decision_tests # 11 passed
cargo fmt --check                 # 1 diff, auth.rs:317 (pre-existing)
cargo clippy --all-targets        # exit 0
```

Baseline for comparison: **2046** passed before this phase; `2046 + 11 = 2057`.