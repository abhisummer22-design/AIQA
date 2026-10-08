# Phase 5 — PASS

**Phase number:** 5 of 6
**Implementation:** 2026-10-07 · **Report finalised:** 2026-10-08
**Status:** PASS

## Summary

Module and release risk, computed from stored history and scored by the **existing**
`adaptive_intelligence::calibrate_risk` rather than a second formula — which is what
master plan §7's "risk score if already supported" asks for.

The phase's real work was a design problem rather than code volume. Three of
`calibrate_risk`'s six factors have nowhere to come from in this schema — no column
records a dependency count, a healing regression per release, or a security sensitivity.
That is **0.30 of the model**.

Passing `0.0` for them would have capped the achievable score at exactly 0.70, which is
the `CRITICAL` threshold, so a module failing 100% of the time on the worst category
would sit *on* the boundary rather than above it — a silent claim of "no dependency
risk, no security risk" for a schema that has never stored either. Renormalising the
weights would have implied the missing 30% does not matter, which is a different false
claim.

Both were rejected. Instead the weight **and its evidence coverage** are reported as
numbers (`measured_weight` 0.70 / `unmeasured_weight` 0.30), no 0–1 score is emitted
from partial evidence, and the real `calibrate_risk` is called the moment a migration
makes all six factors measurable — which needs no code change here.

## Files changed

| File | Kind |
|---|---|
| `services/orchestrator/src/qa_memory_risk.rs` | new — 808 lines; `module_risk`, `release_risk`, `release_risk_overview`, `FACTOR_WEIGHTS`, `STRUCTURALLY_UNAVAILABLE_WEIGHT` |
| `services/orchestrator/src/qa_memory_risk_tests.rs` | new — 18 tests, 841 lines |
| `services/orchestrator/src/qa_memory.rs` | modified — seven private helpers widened to `pub(crate)` so Phase 5 reuses rate/window/failure classification instead of reimplementing it |
| `services/orchestrator/src/lib.rs` | modified — registers both modules |

## Tests run

```text
cd services/orchestrator
cargo test --lib qa_memory_risk         # this phase
cargo test                              # full suite
cargo clippy --all-targets
cargo fmt --check
```

## Tests passed

| | |
|---|---|
| Phase suite | **18 passed / 0 failed** |
| Full backend suite | **1795 passed / 0 failed / 2 ignored** (exit 0) |
| Baseline before this phase | 1777 passed / 0 failed / 2 ignored |
| clippy | 0 errors; 4 warnings in the new files found and cleared during this phase |
| rustfmt | clean for both new files |

## Bugs found

**Production.** `release_risk` looped every release **without excluding the target**,
then appended the target again. Two real consequences: the trend point count was
inflated (a test expecting 3 got 4), and `trend_from` averaged the newest release
against **itself** as one of the "earlier" points, so the direction of travel was
computed against a duplicated input. Phase 3's `release_history` had the correct
filter; this phase's function did not.

**Test.** `no_zero_to_one_score_is_invented_from_partial_evidence` asserted
`weighted_contribution > 0.5` and failed at exactly 0.50.

## Bugs fixed

The trend filter was fixed by excluding the target from the comparison loop, with a
comment explaining why.

The test was corrected to assert the **exact** value rather than a threshold, which made
it a better test: it now pins the arithmetic

```text
failure_rate    1.0 × 0.35 = 0.35   (10 of 10 failed)
flakiness_score 0.0 × 0.20 = 0.00   (consistently failing is NOT flaky)
failure_severity 1.0 × 0.15 = 0.15   (every failure is DATA_FAILURE)
```

and records why flakiness is zero.

## A property of the existing model, flagged not changed

Making that assertion exact exposed something the plan did not anticipate:

> **A module that fails every single time scores lower on the measurable factors than
> one that fails intermittently**, because the model spends 20% of its weight on
> flakiness, and consistent failure is not flaky.

That is `calibrate_risk`'s existing shape. Reweighting a scoring function other surfaces
depend on is outside this plan, so it is pinned by test and carried into the final
report rather than quietly accepted.

## Regression result

Green throughout. `1777 → 1795 passed`, 0 failed. No existing test weakened, skipped or
disabled.

## Known limitations

1. **No `RiskCalibration` is ever emitted** for a module or release while 30% of the
   model is unmeasurable. Adding any one of the three missing columns unlocks it.
2. **`category_weight` is a stated convention, not a measurement** — 1.0 for
   `DataFailure`, 0.95 for auth, down to 0.3 for `ClientError`. Documented and testable,
   but a judgement.
3. **`healing_regression_rate` is unmeasured even though healing exists** — the store has
   `healing_strategy_metrics` and `verification_result`, but nothing ties a regression to
   a module or release.
4. **No endpoint exposes any of this.** Store and service capability only.

## Next phase prerequisites

Phase 6 (Full Validation & Production Hardening) must read:

1. `docs/qa-intelligence-memory/PHASE_5_REPORT.md` — §3 the model-coverage decision and
   §7 the trend defect
2. `docs/qa-intelligence-memory/PHASE_5_PASS.md` — this file
3. `docs/qa-intelligence-memory/PHASE_1_AUDIT.md` — §9 migration requirements
4. `services/orchestrator/src/qa_memory_risk.rs` — `calibrate_or_none`, `FACTOR_WEIGHTS`

Binding rules for Phase 6: validate the whole chain end to end through real code paths;
measure performance rather than optimising without evidence; verify tenant isolation,
data integrity, no credentials and no chain-of-thought; verify migrations against a
**copy** of the live database; and produce
`docs/qa-intelligence-memory/FINAL_VALIDATION_REPORT.md`.