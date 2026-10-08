# Phase 4 — Risk Signal Correctness Review / Remediation

**Status:** remediation complete, all tests pass.
**Nature:** a **later finding** against Phase 4. This is not a continuation of Phase 4 and not a
correction of what Phase 4 concluded at the time. See §8 for the historical record.
**Baseline:** 1934 → **1953 passed / 0 failed / 5 ignored**. Clippy clean (`-D warnings`).

---

## 1. The finding

Phase 6 (Data Integrity) reported finding **F2**:

> `intelligence.rs:553-555` writes `risk_assessments.security_sensitivity`,
> `.business_criticality`, `.change_frequency` as the literal `0.0`, while the risk formula
> weights `security_sensitivity` at 10% — every stored score is silently understated by a
> fabricated zero.

This review was asked to determine, from the repository and without guessing, what those three
values actually were, and to make them honest.

---

## 2. Answers to the six questions

### Are these values real measurements?

**No. All three were placeholders.** The distinguishing test used here is the comment the writer
carried, read against the schema:

| Column | Verbatim source comment | Verdict |
|---|---|---|
| `security_sensitivity` | `// no signal source wired up yet` | **placeholder** |
| `business_criticality` | `// no requirement-linkage source wired up yet` | **placeholder** |
| `change_frequency` | `// no change-intelligence source wired up yet (see README)` | **placeholder** — *but see §3* |

### Explicit declarations, defaults, or placeholders?

**Placeholders.** The distinction matters and was checked against the code, not inferred:

- A **declaration** is a value someone asserts. Nothing asserts these.
- A **default** is a documented zero-when-absent, chosen so that absent reads as neutral.
  Neither had that, and nothing downstream treated it as neutral.
- A **placeholder** is a number written because no value was available, shaped so that it looks
  like the real thing. That is what these were — and the cost was that `0.0` is
  indistinguishable, at every consumer, from a measurement that legitimately came out nil.

### Which risk calculations consume them?

Three consumers, found by exhaustive search of every non-test file. One was already correct.

| # | Consumer | Consumed these? | Verdict |
|---|---|---|---|
| 1 | `intelligence.rs` `update_test_intelligence` → `risk_assessments.score` | `security_sensitivity` **at weight 0.10**; the other two stored only | **Wrong** — writes `0.0` *and* multiplies it in |
| 2 | `api.rs` `GET /projects/:id/intelligence/risk` → `calibrate_risk` | `security_sensitivity` **at weight 0.10**; the other two not present | **Wrong** — same defect, on a user-visible endpoint |
| 3 | `qa_memory_risk.rs` (Phase 4) | neither column | **Already correct** — see §4 |

`business_criticality` and `change_frequency` are consumed by **no formula anywhere**. They were
written and never read. `test_value_assessments.value_score` does not use them either.

### Can the system distinguish `0.0` from unmeasured?

**At the schema level, yes. In Rust, no. This was the root cause.**

The migration already declared all three nullable:

```
risk_assessments:
    business_criticality    REAL,      -- no NOT NULL
    change_frequency        REAL,      -- no NOT NULL
    security_sensitivity    REAL,      -- no NOT NULL
```

and likewise `test_value_assessments.business_criticality` and `.change_relevance`. **"Unmeasured"
was always representable as SQL `NULL`.** The mechanism existed and the writer simply refused to
use it, taking `f64` where `Option<f64>` was the honest type.

So the defect was **not** a missing migration. It was a type that could not say "I don't know".

---

## 3. Which signals have a real source

Each signal was decided independently. Nothing was invented, and nothing was inferred from a
proxy.

### `security_sensitivity` — **no source exists**

Searched for any column, field, tag convention or derived value anywhere in the schema. There is
none. Phase 4 had already considered and **rejected** deriving it from `requirements.kind`,
because `kind` is a *category* and a category is not an *importance*. That reasoning is unchanged
and is honoured here.

`neither_signal_is_invented_from_a_proxy` pins this: a test case with `category: "security"`,
`severity: "critical"`, `priority: "P0"` and `tags: ["security", "critical"]` still yields
`security_sensitivity = NULL`. Anyone wiring a proxy later will fail that test.

### `business_criticality` — **no source exists**

`requirements` stores `kind`, `section` and `ordinal`. No importance, weight or criticality field.
`test_cases.priority` is an authoring choice, not a measurement of business importance, so using
it would be inventing.

### `change_frequency` — **a source exists, and it now exists because of Phase 3**

The original comment — *"no change-intelligence source wired up yet (see README)"* — was **true
when written and is now false.** Phase 3 built `change_impacts`: real, production-written,
project-scoped records of which declared changes affected which node.

The instruction was: *if a real signal source already exists, wire it correctly.* One does, so it
is wired:

```rust
let change_frequency: Option<f64> = Some(
    self.store
        .count_distinct_changes_for_node(project_id, Some(&node_id))
        .await? as f64,
);
```

Two deliberate choices in that measurement:

- **Distinct `change_ref`, not impact rows.** One change that touches a node five times is one
  change; counting rows would measure the analysis's verbosity rather than change.
- **Tenant-scoped.** `WHERE project_id = ? AND affected_node_id = ?`. A `None` node id returns `0`
  rather than a project-wide total, so a caller cannot get the wrong number by passing nothing.

This is the **measured zero** case that makes the distinction possible: a node no declared change
has ever touched reads `Some(0.0)` — a real observation — while `security_sensitivity` reads
`NULL` — no observation.

---

## 4. Phase 4 itself was already correct

`qa_memory_risk::calibrate_or_none` had this to say:

> `calibrate_risk` takes six `f64`s. Passing 0.0 for a factor that simply has no evidence would
> make it report "no dependency risk" and "no security risk" …

and it **refuses to call `calibrate_risk` at all** unless every factor measures, returning
`None`. Phase 4 reports `security_sensitivity` as `Factor::unmeasured` with a reason. That
architecture was right and is reused, not replaced. `phase_fours_surface_is_unchanged` asserts
its refusal and its measured factors still behave.

**Correction to Phase 4's report.** That report described `security_sensitivity` as having *"no
column and no table anywhere."* That is not correct: the column exists, is nullable, and is
written. What was missing was a **signal source** — a different problem with a different fix.
**The Phase 4 report has not been rewritten.** It records what was known at the time; this
correction is recorded here, by addition. See §8.

---

## 5. What changed

### 5.1 `Option<f64>` where "no source" is the honest answer

`Store::upsert_risk_assessment` and `Store::upsert_test_value` now take `Option<f64>` for
`business_criticality`, `change_frequency` / `change_relevance`, and `security_sensitivity`. `Some(0.0)`
means *measured, and it is zero*; `None` means *no source exists*.

`Store::list_risk_assessment_constants` (Phase 6's reader) reads `Option<f64>` rather than
`f64`. This mattered: reading a `NULL` through `r.get::<f64, _>(…)` errors, and Phase 6's reader
used `.unwrap_or_default()` — which would have silently converted the new `NULL`s back into the
`0.0`s this remediation exists to remove. A reader that cannot say "unmeasured" cannot be part of
fixing that.

**No migration.** All six columns were already nullable. Adding one would have been theatre.

### 5.2 The score is no longer understated

`intelligence.rs` previously summed weights that total 1.00 but only had evidence for 0.90:

```rust
// before
let risk_score = (0.40 * historical_failure_rate
    + 0.20 * flakiness_score
    + 0.15 * dep_norm
    + 0.15 * cost_norm
    + 0.10 * security_sensitivity)   // security_sensitivity == 0.0
    .clamp(0.0, 1.0);
```

Dropping the fabricated term without compensating caps every score at 0.90 and **shifts the level
bands**: a node whose four real factors put it at 0.55 true risk was stored as 0.495, below the
0.50 `HIGH` boundary it actually crosses. So the sum is now normalised over the weight the data
backs:

```rust
let measured_weight: f64 = 0.40 + 0.20 + 0.15 + 0.15;
let contribution = 0.40 * historical_failure_rate
    + 0.20 * flakiness_score
    + 0.15 * dep_norm
    + 0.15 * cost_norm;
let risk_score = (contribution / measured_weight).clamp(0.0, 1.0);
```

**Every stored score therefore rises by exactly ×1.1111.** This is the single most consequential
behavioural effect of the remediation and is called out in §7.

The neutrality assumption — that the unmeasured 0.10 would have contributed nothing — is an
**assumption, not a measurement**, and is written into the row rather than left implicit:

```json
"measured_weight": 0.9,
"unmeasured_weight": 0.1,
"unmeasured_factors": ["security_sensitivity", "business_criticality"],
"security_sensitivity": null,
"score_normalisation": "sum(weight x measured) / measured_weight, with measured_weight=0.9; this
                        assumes the unmeasured weight would have been neutral, which is an
                        assumption and not a measurement"
```

`null` in that blob is now a different fact from `0.0`, which is the entire point.

### 5.3 The normalisation extended, not duplicated: `calibrate_partial`

The same defect existed in `GET /projects/:id/intelligence/risk`, which built
`RiskFactors { … security_sensitivity: 0.0 }` and called `calibrate_risk` directly — producing a
real `CRITICAL/HIGH/MEDIUM/LOW` level from a formula with 10% of its weight fabricated.

An existing test (`api_tests::test_project_risk_intelligence_endpoint`) pins that a project **with**
telemetry gets a non-`INSUFFICIENT_DATA` level, so simply refusing to score would have broken an
existing feature. The remediation therefore adds the **missing half of `calibrate_or_none`**:

```rust
pub fn calibrate_partial(subject: &str, factors: &[Factor]) -> (RiskCalibration, WeightCoverage)
```

`calibrate_or_none` refuses; `calibrate_partial` scores over the measured weight and records the
assumption. Both are kept, for different callers.

It is **not** a second risk engine:

- weights come from the existing `qa_memory_risk::FACTOR_WEIGHTS`, not re-declared;
- the level comes from the existing `adaptive_intelligence::RISK_LEVEL_THRESHOLDS`, which was
  **extracted out of `calibrate_risk`'s body** so both paths read one owner;
- it adds only the normalisation.

`calibrate_risk` itself is **unchanged**, because `qa_memory_risk::calibrate_or_none` calls it and
Phase 4's tests pin its output. Altering its scale would have changed an existing feature's
results without being asked to.

Two supporting visibility changes: `Factor::measured` / `Factor::unmeasured` and
`WeightCoverage`'s fields became `pub`, because an external caller must now be able to say
"no source" — otherwise a caller facing an unmeasurable factor has no option but to pass `0.0`,
which is the defect being fixed.

### 5.4 The endpoint no longer claims measurements it doesn't have

While wiring §5.3, the handler's other factors were checked too. Two more zeros were being claimed
on the strength of no evidence:

- `failure_rate = 0.0` when `total_runs == 0` — a rate over zero observations is not a rate of zero;
- `flakiness_score = 0.0` when no flakiness rows exist.

Both are now `Factor::unmeasured` with the reason. This is the same defect one level up, found
while fixing it. `failure_severity` (gated on `failed_runs > 0`) and `healing_regression_rate`
(gated on `total_attempts > 0`) are gated on their denominators in the same way, because a
severity drawn from zero failures is equally unfounded.

The endpoint's response gains `measured_weight`, `unmeasured_weight`, `weighted_contribution` and
`factors_detail`. Its existing fields — `risk_level`, `risk_score`, `factors.*`, `explanation` —
are unchanged, and `api_tests` passes unmodified.

---

## 6. Phase 6's detector, corrected

`integrity.rs`'s `fabricated_constant_check` was written to catch exactly this bug. Post-remediation
it must report the *new* honest state rather than a condition that no longer exists. It now
distinguishes two things that look alike and mean opposite things:

| Observed | Outcome | Meaning |
|---|---|---|
| every row `NULL`, and **no** source exists | `INSUFFICIENT_DATA` | a stated gap, with the unbacked weight quantified — not a defect |
| every row `NULL`, but a source **does** exist | `INCONSISTENT` | the writer is broken; a measurement that should exist is missing |
| one distinct value, and **no** source exists | `INCONSISTENT` | a number was supplied rather than measured — fabrication |

The `!has_source` guard on the third row is not decoration. My first attempt flagged *any* constant,
which flagged `change_frequency = Some(0.0)` — a legitimately measured zero. That is precisely the
conflation this remediation exists to remove, and it was caught by a test rather than shipped.

`fabricated_constant_check` now returns `Result<…, String>`; `verify_run` tolerates a reader failure
there with `unwrap_or_default()` rather than losing the other ten comparisons to one hiccup.

**No test was weakened.** `the_fabricated_constant_check_finds_the_hardcoded_zeros` asserted the
defect that this work removed. Rather than delete it, it was split:

- `the_constant_check_reports_unsourced_signals_as_unmeasured_not_fabricated` asserts the remediated
  state, and additionally asserts `change_frequency` is **not** reported;
- `a_constant_zero_supplied_through_the_real_writer_is_still_reported_as_fabricated` drives the
  fabricated branch through the **real production writer** with `Some(0.0)` — a value any caller of
  `upsert_risk_assessment` can supply — so a recurrence is still caught.

Deleting the original assertion without replacing it would have quietly removed the coverage it
provided.

---

## 7. Behavioural consequences, stated plainly

**1. `risk_assessments.score` rises by exactly ×1.1111.** Exact, not approximate:
`1 / 0.90`.

**2. Phase 3's `HighRisk` regression tier will select more tests.** `change_impact.rs` treats a
case as high-risk when `risk_assessments.score >= HIGH_RISK_THRESHOLD` (0.7). A node whose old
score fell in **[0.63, 0.70)** now crosses the threshold and is selected. Phase 3's tier was
under-selecting: it was reading scores that had been understated by a fabricated zero. This is a
correction, and it is in the **safe** direction — more coverage, never less — but it is a real
change to a Phase 3 feature's output. `change_impact_tests` (16) pass, and they were unaffected
because they write `score` directly rather than deriving it.

**3. `GET /risk` scores rise and gain coverage fields.** Its five measured factors cover weight
0.90, so the same ×1.1111 applies. A project that read `MEDIUM` may now read `HIGH`. The response
gains four fields and changes no existing one.

**4. The Phase 4 / Phase 6 / Phase 3 divergence is untouched.** Phase 4's `L4` note recorded that
`risk_assessments.score` comes from a *different* five-factor formula than `calibrate_risk`. That
divergence remains. `intelligence.rs` cannot call `calibrate_partial`, because its weights are not
`FACTOR_WEIGHTS` — merging the two is a Phase 4 L4 item and outside this remediation. It is
recorded, not resolved.

---

## 8. Historical accuracy

- **`PHASE_4_RISK_BASED_TESTING_REPORT.md` has not been edited.** Its claim that
  `security_sensitivity` had "no column and no table anywhere" was wrong, and remains as written.
  It records what was known when Phase 4 ran.
- **This document is the correction**, recorded by addition, dated after the fact.
- **`PHASE_6_DATA_INTEGRITY_REPORT.md` has not been edited.** F2 was correctly identified there.
- The remediation's own reasoning contained one error worth recording: it was initially believed
  that `calibrate_risk`'s weights sum to 0.90 and that a fully measured model therefore also needed
  normalising. **They sum to 1.00.** The belief was caught by
  `the_partial_path_and_calibrate_risk_share_one_owner_of_the_bands`, which now asserts the sum is
  1.0 and fires if a future change breaks it. No code change resulted from the wrong belief.

---

## 9. F1 — the latent `set_task_status` issue, reported not fixed

Reported as instructed and **not fixed**, because it is not required for this remediation.

`store.rs`, `set_task_status`:

```rust
if set_started {
    // …
} else if set_finished {
    // …
} else {
    // …
}
```

A single call with `set_started == true` **and** `set_finished == true` takes only the first
branch and silently leaves `finished_at` and `reason` unset.

**Latent, not active.** `engine.rs` always makes two calls — `(Running, true, false)` then
`(status, false, true)` — so no production path hits it today. It is a trap for the next caller
who assumes the flags are independent, not an active defect.

Not fixed here, for two reasons: it is production scheduling code and not required for the
risk-signal work, and it is in every other respect unrelated to risk scoring.

**Recommended fix:** make the branches independent rather than exclusive, i.e. a caller that
supplies both flags gets both effects. That matches what every current caller already does by
hand. Requires a decision, since it changes a store method's semantics.

---

## 10. Residual findings — reported, not fixed

**R1. `GET /risk` masks a read failure as "no dependencies".**
`api.rs` reads the relation count with `.unwrap_or_default()`, so a store read error yields
`dependency_count == 0` and the endpoint reports *measured* "zero dependency risk". Same class of
defect as the fabricated zeros — an error rendered as a measurement — but a different root cause.
Not fixed: propagating it changes a `200` into a `500` on a read failure, which is a product
decision, not a correctness fix I should make unilaterally. **Needs a decision.**

**R2. `change_frequency` is now real but still consumed by nothing.** It is stored and read by
Phase 6's detector, but no formula uses it. Adding it to the risk model would change the weight
budget and therefore every score — out of scope here. Recorded.

**R3. No source exists for `security_sensitivity` or `business_criticality`.** This remediation
made their absence *visible and correctly represented*. It did not create a source, because that
would mean inventing the signal. A real source needs a schema decision: a security-sensitivity
field, a requirement criticality field, or an explicit human-asserted classification.

**R4. `cargo fmt --check` fails on one pre-existing line.** `auth.rs:317`, a `concat!` inside
`#[doc = …]` in `role_gate!`, where rustfmt is not idempotent — it reformats the line and then
wants it changed back. Present at `HEAD`, introduced by an earlier phase's fmt run, unrelated to
this work. Not touched. All files in this remediation are fmt-clean; build, clippy (`-D warnings`)
and the full suite are clean.

---

## 11. Test evidence

**Full suite: 1953 passed / 0 failed / 5 ignored.** Baseline was 1934; +19 new, no test weakened,
none disabled.

| Suite | Count | Note |
|---|---|---|
| `risk_signal_tests` | **18** | new — this remediation |
| `integrity_tests` | 26 (+1 ignored) | Phase 6; 1 test rewritten, 1 added |
| `risk_prioritisation` | 18 (+1 ignored) | Phase 4; **unmodified** |
| `change_impact_tests` | 16 | Phase 3; **unmodified** |
| `exploration_tests` | 29 (+1 ignored) | Phase 5; **unmodified** |
| `trace_tests` / `trace_chain_tests` | 21 / 14 | Phases 1–2; **unmodified** |
| `api_tests` | 313 | **unmodified**, incl. `test_project_risk_intelligence_endpoint` |

Five existing test call sites passed `0.0` into `upsert_risk_assessment`. They were fixtures
supplying placeholder zeros for factors the tests do not exercise; they now pass `None`, which is
the correct expression of the same intent. **No assertion was changed, relaxed or removed.**

### The six required properties

| Required | Test |
|---|---|
| measured zero distinguishable from unmeasured | `a_measured_zero_is_distinguishable_from_an_unmeasured_signal` — `Some(0.0)` and `NULL` reachable simultaneously, in the row, in `inputs`, and through the store reader |
| unavailable signals do not silently become zero | `unavailable_signals_are_persisted_as_null_not_zero`, `neither_signal_is_invented_from_a_proxy` |
| score not understated by fake measurements | `a_partial_model_no_longer_reads_below_the_band_its_factors_justify` (0.35 → 0.44), `the_stored_score_agrees_with_its_own_recorded_coverage` |
| existing measured signals keep working | `the_signals_that_were_already_real_are_unaffected`, `phase_fours_surface_is_unchanged`, `a_fully_measured_model_is_unaffected_by_the_normalisation` |
| project isolation intact | `change_frequency_is_project_scoped`, `risk_signals_remain_project_scoped` |
| Phase 4 + Phase 6 tests still pass | `phase_six_detector_sees_the_remediated_state_and_still_catches_a_recurrence`, plus those suites unmodified and green |

Plus: `change_frequency_reads_from_the_real_change_impact_records` (0 → 3 distinct changes, proving
the source is *wired* and not merely un-`NULL`ed), `a_model_with_nothing_measured_yields_no_score_rather_than_zero`,
`the_endpoints_factor_list_covers_the_engine_and_nothing_else`, and
`a_project_with_no_history_reports_no_measured_factors`.

### Five tests that were wrong, and what corrected them

Each was a defect in the test, found by the test failing. All are recorded in the file.

1. **`FACTOR_WEIGHTS` was believed to sum to 0.90.** It sums to **1.00**. Caught by an assertion
   on the sum. The code was right; the reasoning was wrong.
2. **`change_frequency` was flagged as fabricated at `0.0`.** A legitimately measured zero. Caught
   by a test asserting the column is *not* reported. Led to the `!has_source` guard.
3. **Fixtures used a fresh task name per run**, so `historical_failure_rate` and `flakiness_score`
   — both read per task *name* — stayed at their trivial values. Caught by an assertion of
   `contribution > 0.0` added specifically so a zero-on-both-sides comparison could not pass.
4. **`calibrate_partial` and `calibrate_risk` were compared with mismatched inputs.**
   `calibrate_risk` takes a raw count and normalises internally (`count / 5`); `calibrate_partial`
   takes an already-normalised value. `dependency_count: 0` gives `dep_norm = 0.0`, not `0.5`.
   The asymmetry is now documented in the test. `GET /risk` performs the same normalisation.
5. **The isolation test asserted a row count of 1** where the fixture creates four nodes in the
   other tenant. Now asserts 4 and 1 separately, which is a stronger statement.

---

## 12. Reuse declaration

Nothing new was built. No second risk engine, memory DB, trace DB, or scoring system.

| Need | Reused |
|---|---|
| risk formula and weights | `adaptive_intelligence::calibrate_risk`, `qa_memory_risk::FACTOR_WEIGHTS` |
| level bands | `adaptive_intelligence::RISK_LEVEL_THRESHOLDS` / `risk_level_for` (extracted, not re-declared) |
| unmeasured representation | `qa_memory_risk::Factor`, `WeightCoverage`, `calibrate_or_none` |
| persistence | `Store::upsert_risk_assessment`, `upsert_test_value` |
| change-intelligence source | `change_impacts`, written in Phase 3 |
| change-frequency reader | one new reader, `count_distinct_changes_for_node`, following existing conventions |
| defect detection | `integrity.rs::fabricated_constant_check` (extended) |

Schema changes: **none.** All affected columns were already nullable.

---

## 13. Open items for a decision

1. **R1** — should `GET /risk` propagate a failed relation read instead of reporting zero
   dependencies? A `200`-to-`500` behaviour change.
2. **R2** — should `change_frequency` enter the risk model? Changes every score.
3. **R3** — add a real `security_sensitivity` or requirement `criticality` source? Schema decision.
4. **F1** — make `set_task_status`'s branches independent? Changes a store method's semantics.
5. **Phase 4 L4** — merge the two diverging risk formulas?
