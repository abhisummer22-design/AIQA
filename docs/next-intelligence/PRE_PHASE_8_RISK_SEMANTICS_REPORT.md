# PRE-PHASE 8 FOLLOW-UP — RISK SEMANTICS REPORT

**`failure_severity` and `INFRA_ERROR`; `completed_failure_rate` unit consistency.**

Repository: `https://github.com/abhisummer22-design/AIQA.git`
Branch: `main` · HEAD: `7930507` · 0 ahead / 0 behind `origin/main`
Predecessor: `PHASE_7_4_RISK_METRIC_IMPLEMENTATION_REPORT.md`

## Verdict: **PASS** — with one open D3 decision recorded, not resolved

Both objectives are implemented, tested, and mutation-verified. `INFRA_ERROR` no longer produces a
product-defect severity, every genuine-failure severity is unchanged, and no name carries two units.

**One thing this follow-up deliberately did not do**, because it is a D3 decision rather than a
defect: the two `failure_severity` implementations in this codebase compute genuinely different
things under the same factor name. See §2.4 and §12.

---

## 1. Initial repository state

Re-verified, not read from the previous report.

| | |
|---|---|
| branch / HEAD | `main` @ `7930507` |
| ahead / behind `origin/main` | 0 / 0 |
| modified tracked | 23 |
| untracked | 15 |
| deleted / staged | 0 / 0 |
| `C:` free at start | 47.3 GB |

### Baseline, re-measured before any edit

| run | result |
|---|---|
| baseline 1 | **2057 passed / 0 failed / 5 ignored** |
| baseline 2 | 2057 passed / 0 failed / 5 ignored |

Matches the historically reported 2057. No prior count is reused anywhere in this report.

---

## 2. `failure_severity`: implementation and callers

### 2.1 There are two, not one

| | `qa_memory_risk::failure_severity` | `api.rs::get_project_risk_handler` |
|---|---|---|
| algorithm | mean of `category_weight(failure_category)` over failures | two-bucket heuristic: **0.8 / 0.4 / 0.0** |
| reads `failure_category`? | **yes** | **no — never** |
| filters on | `is_failure` → `FAILED \| TIMEOUT` | `matches!("FAILED" \| "TIMEOUT" \| "INFRA_ERROR")` |
| result type | `Option<f64>` → `None` when unmeasurable | plain `f64` |
| output range | 0.3 – 1.0 from the weight table | 0.0 / 0.4 / 0.8 |

Both feed the `failure_severity` **risk `Factor`**, weight **0.15**.

### 2.2 Every production caller

| caller | file | factor → |
|---|---|---|
| `module_risk` | `qa_memory_risk.rs:317` | `calibrate_or_none` |
| `release_risk` | `qa_memory_risk.rs:526` | `calibrate_or_none` |
| trend loop → `release_factors` | `qa_memory_risk.rs:615` | `calibrate_or_none` |
| `test_risk_from` | `qa_memory_risk.rs:1483` | `calibrate_or_none` |
| `get_project_risk_handler` | `api.rs:9240` | `calibrate_partial` |

### 2.3 Downstream reach

`failure_severity` reaches **risk assessment** and, through `test_risk`, **test prioritisation**
(`test_prioritisation` consumes `TestRisk`). It does **not** reach failure clustering: that is
`adaptive_intelligence.rs:448`, a separate categorisation path which is unchanged.

### 2.4 The defect, and the narrower finding underneath it

**The defect (fixed).** The handler counted `INFRA_ERROR` in the count that gates the factor, and
counted it again in the condition that selects the 0.8 bucket. Measured before the change:

> a project whose **only** outcome was infrastructure errors reported
> `failure_severity = 0.8` — its highest product-defect value — at full 0.15 weight, with the
> infrastructure-error count as its evidence.

The shared implementation already answered `None` for identical evidence. So the two halves of one
model disagreed about the same data.

**The narrower finding (NOT fixed — this is the D3 decision).** The handler's `failure_severity`
ignores `failure_category` entirely, yet its basis string claimed *"failure categories of this
project's failures, weighted by share"* — which it never computed. Two algorithms, one factor name,
meanings from `0.3`–`1.0` (category-weighted) versus `{0.0, 0.4, 0.8}` (bucketed).

Making them agree means choosing which severity model is canonical. **That is D3**, so it was not
done. The basis string was corrected to describe what the handler actually does, and the
disagreement is recorded in §12.

### 2.5 Is infrastructure error already separately represented?

Yes, since Phase 7.3, and unchanged: `infrastructure_errors` on `ModuleRiskHistory`,
`ReleaseHistory`, `ReleaseComparison`, `RequirementHistory`, `AffectedTestRisk`, and the API
response key.

---

## 3. `INFRA_ERROR` behaviour, before and after

Measured through the real HTTP handler, on identical fixtures.

| fixture | severity before | severity after | evidence | `risk_score` | level |
|---|---|---|---|---|---|
| `INFRA_ERROR` only | **0.8** | **unmeasured** | 1 → 0 | 0.31 → **0.07** | MEDIUM → **LOW** |
| 2× `INFRA_ERROR` | **0.8** | **unmeasured** | 2 → 0 | 0.36 → **0.13** | MEDIUM → **LOW** |
| `FAILED` only | 0.4 | **0.4** *(unchanged)* | 1 → 1 | 0.54 → 0.54 | HIGH |
| `FAILED` + `INFRA_ERROR` | **0.8** | **0.4** | 2 → 1 | 0.64 → **0.56** | HIGH |
| `FAILED` + `TIMEOUT` | 0.8 | **0.8** *(unchanged)* | 2 → 2 | 0.64 → 0.64 | HIGH |
| `TIMEOUT` only | 0.8 | **0.8** *(unchanged)* | 1 → 1 | 0.61 → 0.61 | HIGH |
| `PASSED` only | unmeasured | **unmeasured** | 0 → 0 | 0.0 → 0.0 | LOW |

Read the table as: **every row whose severity derives from a genuine product failure is
byte-identical before and after.** The only rows that move are the ones where an infrastructure
error was doing the work.

### The change, and why it is the narrowest safe one

```rust
// gate: genuine product failures only
let product_failures = telemetry_refs.iter()
    .filter(|t| QaMemoryReader::is_failure(t))      // FAILED | TIMEOUT
    .count();

// bucket: 0.8 on a genuine failure that timed out
let timeouts = telemetry.iter().filter(|t| t.status == "TIMEOUT").count();
let failure_severity = if product_failures > 0 {
    if timeouts > 0 { 0.8 } else { 0.4 }
} else { 0.0 };
```

* The gate now uses the **existing** `is_failure` contract rather than a third inline status list.
* `TIMEOUT` keeps its 0.8 treatment — it is a completed execution that did not pass, which is the
  established contract and is not what this change is about.
* Both bucket values, `0.4` and `0.8`, are unchanged.
* `0.0` is still produced for the no-failure branch but never reaches a consumer: the factor is
  gated on `product_failures > 0` and reports `unmeasured` otherwise, so a fabricated measured zero
  is still impossible.

### Against the recommended semantics

| requirement | status |
|---|---|
| an infrastructure error is not a confirmed product-test failure | ✅ it is in neither the gate nor the bucket |
| infrastructure errors stay observable | ✅ `infrastructure_errors` unchanged and asserted |
| not silently treated as successful | ✅ `failure_rate` is `null`, not `0.0`; asserted |
| no new risk-score weight | ✅ six factors, all weights asserted; no infra-named factor |
| no change to genuine-failure severity | ✅ three fixtures assert identical before/after |

### The shared implementation was not disturbed

It already filtered on `is_failure`. Measured before and after:

| fixture | shared severity |
|---|---|
| `INFRA_ERROR` only | unmeasured *(unchanged)* |
| 2× `INFRA_ERROR` | unmeasured *(unchanged)* |
| `FAILED` only | 0.4 |
| `FAILED` + `INFRA_ERROR` | 0.4 *(unchanged — infra does not move it)* |

---

## 4. Risk-score impact of the severity change

Only two fixtures move, both because infrastructure evidence stopped counting as product-defect
evidence.

| fixture | `weighted_contribution` | `measured_weight` | score | band |
|---|---|---|---|---|
| `INFRA_ERROR` only | 0.1245 → **0.021** | 0.45 → **0.30** | 0.31 → **0.07** | MEDIUM → **LOW** |
| `FAILED` + `INFRA_ERROR` | 0.5133 → **0.4475** | 0.80 (unchanged) | 0.64 → **0.56** | HIGH (unchanged) |

The first row's `measured_weight` falls from 0.45 to 0.30 because `failure_severity` (0.15) leaves
the measured set and joins the unmeasured one. That is the correct consequence: the model no longer
claims to have measured product-defect severity for a project that has never had a product defect
recorded.

No weight and no threshold changed — §5 asserts both, structurally.

---

## 5. Fraction / percentage contract, before and after

### Before

One name, two units:

| surface | kind | unit |
|---|---|---|
| API key `completed_failure_rate` | public JSON | **0..1 fraction** |
| `ModuleRiskHistory.completed_failure_rate` | `Option<f64>` | **0..100** |
| `ReleaseHistory.completed_failure_rate` | `Option<f64>` | **0..100** |
| `ReleaseComparison.completed_failure_rate` | `Option<f64>` | **0..100** |
| `RequirementHistory.completed_failure_rate` | `Option<f64>` | **0..100** |
| `AffectedTestRisk.completed_failure_rate` | `Option<f64>` | **0..100** |

### After

| surface | unit |
|---|---|
| API key `completed_failure_rate` | 0..1 fraction — **unchanged name, unchanged value** |
| API key `completed_failure_rate_fraction` | 0..1 fraction — **new, additive** |
| API key `completed_failure_rate_percent` | 0..100 — **new, additive** |
| `*.completed_failure_rate_percent` (5 view structs) | 0..100 — **renamed** |

The shared helpers keep the names Phase 7.4 gave them: `completed_failure_rate` → 0..100,
`completed_failure_rate_fraction` → 0..1.

### Why the view rename is not a breaking change

Verified exhaustively before renaming, not assumed:

* **0** production reads of `.completed_failure_rate` on those structs — only 3 test assertions;
* **0** handlers return any of the five types (`Json<…>` / `-> Json<…>` scan across the crate);
* **0** call sites serialise them (`into_response`, `to_value`, `to_string` scan);
* **0** hits in the only OpenAPI spec (`executors/schemathesis/openapi.yaml` does not declare this
  endpoint);
* **0** frontend or script consumers in the repository.

They derive `Serialize`, so the renamed field does change the JSON key they *would* emit — but
nothing emits them.

---

## 6. API compatibility decisions

| rule | decision |
|---|---|
| 1 — do not silently change a public field's meaning | `completed_failure_rate` keeps **both** its name and its 0..1 value |
| 2 — no breaking change without demonstrated necessity | none made |
| 3 — preserve, plus a clearly named additive field | added `_fraction` and `_percent` |
| 4 — rename an internal ambiguous field | the 5 view fields, safe per §5 |
| 5 — serialization uses intended names | the three JSON keys are inserted explicitly and asserted by name |
| 6 — every consumer knows its unit | the unit is stated in the doc on all 5 fields and at the response |
| 7 — no double scaling | guarded by `pp8_o2_conversion_is_applied_exactly_once`, mutation-verified |
| 8 — no weight/threshold compensation | none changed |
| 9 — the fraction stays the risk input | `completed_failure_rate_fraction` unchanged; asserted and mutation-verified |
| 10 — all-recorded rate preserved | `rate`, `is_pass`, `is_failure`, `completed_failure_rate` untouched |

No key was removed, renamed, or re-valued on the response.

---

## 7. Files changed

Exactly seven, confirmed by modification time (everything else predates this phase):

| file | change |
|---|---|
| `api.rs` | `product_failures` gate via `is_failure`; `INFRA_ERROR` out of the 0.8 bucket; severity factor gated on genuine failures with a corrected basis; two additive response keys + the unit contract |
| `qa_memory.rs` | 4 fields `completed_failure_rate` → `completed_failure_rate_percent`, each with a doc stating 0..100 and why |
| `qa_memory_risk.rs` | 1 field renamed the same way; its write site kept producing the 0..100 value |
| `lib.rs` | `mod pre_phase_8_tests;` (+ doc) |
| `phase_7_3_decision_tests.rs` | 2 assertions follow the rename |
| `phase_7_4_decision_tests.rs` | 1 assertion follows the rename |
| `pre_phase_8_tests.rs` | **new**, 13 tests |

**Not touched:** `integrity.rs` (the 10 ms floor), `store.rs`, `domain.rs`, `engine.rs`,
`executor.rs`, `autopilot_execute.rs`, `intelligence.rs`, `adaptive_intelligence.rs`,
`change_impact.rs`. Verified: **0** migrations, **0** route registrations, **0** changes to
`set_task_status`, and `DURATION_ABSOLUTE_FLOOR_MS = 10.0` untouched.

A temporary probe file (`pre_phase_8_probe.rs`) was used to record the before/after tables and then
**deleted**; it is not part of the change.

---

## 8. Tests added — and proof they detect the defects

`pre_phase_8_tests.rs`, 13 tests, all driving production aggregation, the real scoring code, or the
real HTTP router.

**Objective 1**

| test | pins |
|---|---|
| `pp8_o1_shared_severity_already_excluded_infrastructure_errors` | documents the shared path's existing correct behaviour |
| `pp8_o1_project_risk_severity_is_unmeasured_when_only_infrastructure_failed` | the defect itself |
| `pp8_o1_genuine_failure_severity_is_unchanged` | 0.4 / 0.8 / 0.8 preserved |
| `pp8_o1_infrastructure_errors_do_not_inflate_a_real_failure_severity` | isolation proof |
| `pp8_o1_no_new_factor_or_weight_for_infrastructure_errors` | six factors, all weights, all thresholds |
| `pp8_o1_shared_severity_is_unchanged_for_genuine_failures` | the shared path was not disturbed |

**Objective 2**

| test | pins |
|---|---|
| `pp8_o2_response_exposes_explicitly_named_units` | three keys, two units, correct values |
| `pp8_o2_conversion_is_applied_exactly_once` | no double scaling, ranges 0..1 and 0..100 |
| `pp8_o2_risk_factor_consumes_the_fraction_and_not_the_percentage` | the decisive one |
| `pp8_o2_unmeasured_is_null_in_every_unit` | all three keys null together |
| `pp8_o2_view_fields_are_percentages` | renamed fields really are percentages |
| `pp8_o2_no_view_field_is_still_called_the_bare_ambiguous_name` | compile-time rename guard |
| `pp8_o2_shared_helpers_agree_on_one_measurement` | the two helpers are one measurement |

### Mutation testing

A passing suite is not proof of semantic correctness. Four defects were re-introduced into the
finished code, each **verified applied** before its result was interpreted, then reverted.

| # | mutation | caught by |
|---|---|---|
| A | `INFRA_ERROR` back into the severity gate **and** the 0.8 bucket | `pp8_o1_project_risk_severity_is_unmeasured…`, `pp8_o1_infrastructure_errors_do_not_inflate…` |
| B | `completed_failure_rate_percent` scaled ×100 twice | `pp8_o2_conversion_is_applied_exactly_once`, `pp8_o2_response_exposes_explicitly_named_units` |
| C | the risk factor fed the **percentage** instead of the fraction | `pp8_o2_risk_factor_consumes_the_fraction…`, plus the two above |
| D | the `_percent` field filled with the fraction | `pp8_o2_view_fields_are_percentages` |

All four caught. All reverted; 0 `MUTATION` markers remain in any production file; no backup files
remain.

**One process note.** While reverting mutation A I overwrote the backup before restoring it, so
mutation A's logic survived into the next measurement and briefly inflated that result. It was
caught by re-reading the file rather than the output, reverted explicitly, and mutation B was then
re-run in isolation. The table above reports only isolated, verified runs.

---

## 9. Test results

### Targeted

| suite | result |
|---|---|
| `pre_phase_8_tests` | **13 passed / 0 failed** |
| `phase_7_4_decision_tests` | 11 passed / 0 failed |
| `phase_7_3_decision_tests` | 14 passed / 0 failed |
| `phase_7_1_regression_tests` | 15 passed / 0 failed |
| `qa_memory_risk_tests` | 21 passed / 0 failed |
| `risk_semantics_tests` | 10 passed / 0 failed |
| `risk_signal_tests` | 18 passed / 0 failed |
| `risk_prioritisation_tests` | 20 passed / 0 failed / 1 ignored |
| `qa_memory_planning_tests` (incl. the `release_regression` signal) | 14 passed / 0 failed |
| `qa_memory_retrieval_tests` | 21 passed / 0 failed |
| `qa_memory_validation_tests` | 7 passed / 0 failed |
| `integration_e2e_tests` (Phase 6.2) | 20 passed / 0 failed |
| `api_tests` | 316 passed / 0 failed |

### Journeys — 13, ten repetitions each

| | |
|---|---|
| runs | 10 |
| per run | 13 passed / 0 failed |
| total assertions | **130** |
| failures | **0** |

Reassessed, not merely re-run. The journeys that traverse the affected surfaces are
`journey_b_execution_to_failure_intelligence` (severity and failure-rate factors),
`journey_c_change_to_regression_selection`, and the module/release journeys. None asserts a numeric
risk score, so none required re-baselining; the fixtures that contain infrastructure errors now
report an unmeasured severity instead of a fabricated one, which no journey asserted against. No
journey was skipped or reclassified.

### Integrity — 20 consecutive runs

| | |
|---|---|
| runs | 20 / 20 clean |
| per run | 34 passed / 0 failed / 1 ignored |
| failures | **0** |

### Full backend suite

| run | result |
|---|---|
| 1 | 2070 passed / 0 failed / 5 ignored |
| 2 | 2070 passed / 0 failed / 5 ignored |
| 3 | 2070 passed / 0 failed / 5 ignored |

`2057 + 13 = 2070`. No existing test was removed, skipped, or weakened.

> **One transient failure, reported rather than hidden.** During this phase a single full-suite run
> reported `2069 passed; 1 failed`. My follow-up command re-ran the suite instead of inspecting the
> failing run's saved output, so **the failing test's name was not captured** — a mistake in my
> method, not in the code. It did not recur in **12 subsequent full-suite runs** (4 before that
> check, 8 after), nor in any targeted suite run. `C:` free space fell from 47.3 GB to 24 GB during
> the session, and every integration test places its SQLite file in `std::env::temp_dir()` on `C:`,
> which is a known pressure source in this repository — but I did **not** verify that as the cause
> and am not asserting it. The honest statement is: one unidentified transient failure, not
> reproducible in 12 further runs, cause unknown.

---

## 10. Formatting and Clippy

| check | result |
|---|---|
| `cargo fmt --check` | **1 diff** — `auth.rs:317`, pre-existing, **not fixed** (rustfmt is non-idempotent on the `concat!` inside `role_gate!`; proven at HEAD in Phase 7.1) |
| `cargo clippy --all-targets` | **exit 0**, **0 errors**, **66 warnings**, identical across two settled passes |

Per-file lint counts, extracted from clippy's `-->` lines:

| file | lints |
|---|---|
| `qa_memory.rs` | 0 |
| `qa_memory_risk.rs` | 0 |
| `pre_phase_8_tests.rs` | 0 |
| `lib.rs` | 0 |
| `api.rs` | 6 — all pre-existing `unused variable: ctx` / one `contains()` style hint, at lines 797, 816, 3602, 8348, 8405, 8425; **none** in the 9150–9550 range this phase edited |

The new test file initially carried one lint (`field dir is never read`); it was fixed by renaming
to `_dir`, and the count above is after the fix.

---

## 11. Files changed — final diff review

Confirmed: **0** migrations, **0** route registrations, **0** `set_task_status` changes,
`DURATION_ABSOLUTE_FLOOR_MS = 10.0` untouched, `RISK_LEVEL_THRESHOLDS` untouched, all six
`FACTOR_WEIGHTS` untouched, `calibrate_partial` / `calibrate_risk` / `calibrate_or_none` untouched.

Every change is one of: a status filter, a factor gate, a response key, a field name, a doc comment,
or a test.

---

## 12. Remaining decisions

### D3 — canonical risk formula: **OPEN**, and now sharper

Two findings block it, both recorded rather than resolved:

1. **Two `failure_severity` algorithms under one factor name.** The shared path averages
   `failure_category` weights (0.3–1.0); the project-risk handler uses buckets (0.0/0.4/0.8) and
   ignores categories entirely. Same factor, weight 0.15, different meanings. **Decide which is
   canonical, or state that the endpoint deliberately answers a different question.**
2. **Three of six factors have no column** in any of the 32 migrations. `calibrate_partial`
   normalises over what is measured and states its neutrality assumption; `calibrate_or_none`
   refuses a score.

Nothing here merged functions, invented a formula, or moved a weight.

### D5 — duration tolerance: **OPEN**, untouched. The 10 ms floor is unchanged.

### D6 — `set_task_status`: **OPEN**, untouched. No enum, no redesign.

### Also open

* **Module-change support (Q4)** — deferred, pending an authoritative module-identity source.
* **The two `failure_severity` implementations will keep drifting** until D3 is answered. The
  corrected basis string now makes the handler's actual behaviour visible, so the drift is
  discoverable rather than silent — but it is not prevented.

---

## 13. Blockers

**None.** Both objectives are complete and verified. The one thing that could not be completed
without a scoring decision — unifying the two severity algorithms — is the D3 item above, and the
narrower subtask it would have blocked was completed instead.

---

## 14. Recommended Phase 8 readiness status

**READY.**

Phase 8 reads exactly these factors. Before this follow-up it would have read:

* a severity that said "severe product defect" about a missing browser;
* a rate whose name meant two different numbers depending on which surface you asked.

Both are now correct and pinned. The inputs Phase 8 will build on are trustworthy.

**Two things to expect when Phase 8 starts:**

1. **Two `failure_severity` behaviours, still.** Modules, releases and tests report a
   category-weighted severity; the project-risk endpoint reports a bucketed one. Phase 8 must not
   assume they agree — or D3 should be answered first, which would be cheaper now than after Phase 8
   builds on top of it.
2. **`completed_failure_rate` now exists in three forms** on the API response. Prefer
   `completed_failure_rate_fraction` or `completed_failure_rate_percent`; the bare name is retained
   only for compatibility and will eventually be deprecated.

Also outstanding, unrelated: `docs/next-intelligence/PHASE_7_PASS.md`, the roadmap deliverable
closing Phase 7. Not created here — this is a follow-up report, not a phase verdict. Phase 7 stands
at PASS under the documented supported-target scope.

---

## Appendix — reproducing the key numbers

```powershell
cd services/orchestrator
cargo test                          # 2070 passed / 0 failed / 5 ignored
cargo test pre_phase_8_tests        # 13 passed
cargo fmt --check                   # 1 diff, auth.rs:317 (pre-existing)
cargo clippy --all-targets          # exit 0
```

Baseline for comparison: **2057** passed before this follow-up; `2057 + 13 = 2070`.