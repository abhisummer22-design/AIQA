# Phase 4 — Risk Signal Correctness Review / Remediation: PASS

**Verdict: PASS.** The finding is remediated, every claim is traceable to repository evidence, and
the full suite passes with no test weakened or disabled.

**Full suite: 1953 passed / 0 failed / 5 ignored.** Baseline 1934. Clippy clean under `-D warnings`.
Report: [`PHASE_4_RISK_SIGNAL_REMEDIATION.md`](./PHASE_4_RISK_SIGNAL_REMEDIATION.md).

---

## Gate checklist

| Requirement | Result |
|---|---|
| Determine from the repository what the three values were — no guessing | **Done.** All three were placeholders, identified from the writer's own comments and from the schema |
| Are they measurements / declarations / defaults / placeholders? | **Placeholders.** Not declarations (nothing asserted them), not defaults (nothing downstream read them as neutral) |
| Which calculations consume them? | **Two, both wrong:** `intelligence.rs` risk formula (`security_sensitivity` @ 0.10) and `GET /projects/:id/intelligence/risk` (`calibrate_risk`). Phase 4's `qa_memory_risk` consumes neither and was already correct |
| Can the system distinguish `0.0` from unmeasured? | **It could not.** Schema was nullable; the writer took `f64`. Root cause identified and fixed at the type level |
| Wire the existing source if one exists | **`change_frequency` — done.** Phase 3's `change_impacts` is a real, production-written, project-scoped source. Its old "not wired up yet" comment was true when written and is now false |
| Otherwise represent as UNMEASURED, do not manufacture `0.0` | **Done.** `security_sensitivity` and `business_criticality` have no source and are now `NULL`, with no proxy substituted |
| Do NOT invent business criticality or security sensitivity | **Honoured and pinned by test.** `neither_signal_is_invented_from_a_proxy` fails if a security category, P0 priority or security tag is ever wired in |
| Do NOT create a second risk engine | **No second engine.** Weights from `FACTOR_WEIGHTS`, bands from `RISK_LEVEL_THRESHOLDS` (extracted from `calibrate_risk`'s body so there is one owner), plus only the normalisation. `calibrate_risk` and `calibrate_or_none` untouched |
| Reuse existing `qa_memory_risk` architecture and `calibrate_or_none` behaviour | **Reused and extended.** `calibrate_partial` is the missing half of the same refusal — refuse for reports, normalise for callers that have real factors |
| Test: measured zero distinguishable from unmeasured | **Proven** — `Some(0.0)` and `NULL` reachable at once, in the row, in `inputs`, and through the store's reader |
| Test: unavailable signals do not silently become zero | **Proven** — plus two further claimed measurements in the endpoint found and fixed while doing it |
| Test: risk score not understated by fake measurements | **Proven** — 0.35 → 0.44 on the shared path; stored score recomputed from its own recorded coverage |
| Test: existing measured signals continue working | **Proven** — failure rate, flakiness, dependency count, execution cost unchanged; Phase 4's refusal intact |
| Test: project isolation intact | **Proven** — change-frequency source and the constants reader are both tenant-scoped |
| Test: Phase 4 and Phase 6 tests still pass | **Proven** — `risk_prioritisation` (18) **unmodified**; `integrity_tests` 26 green with one test rewritten and one added |
| Root cause → fix → retest → regression on any failure | **Done.** Four test failures, each diagnosed to its cause before fixing; all recorded in §11 of the report |
| Do not weaken or disable tests | **Honoured.** Zero assertions relaxed or removed. The one obsolete test was **split**, and its lost coverage restored via a new test driving the real production writer |
| Inspect and report `set_task_status`, do not fix unless required | **Reported, not fixed** (§9). Not required for this remediation. Confirmed **latent, not active** — `engine.rs` always makes two calls |
| Preserve historical accuracy; do not rewrite the Phase 4 report | **Honoured.** Phase 4's and Phase 6's reports are **byte-unchanged**. The correction to Phase 4's "no column and no table anywhere" is recorded here **by addition**, dated later |
| Do not commit / push / start Phase 7 | **Honoured** |

---

## Root causes fixed

1. **A writer type that could not say "I don't know."** `upsert_risk_assessment` and
   `upsert_test_value` took `f64` for three columns whose only defensible value was `NULL`. Now
   `Option<f64>`. The schema was already nullable — **no migration was needed, and adding one would
   have been theatre.**

2. **A reader that would have undone the fix.** Phase 6's `list_risk_assessment_constants` read
   `r.get::<f64, _>(…)` with `.unwrap_or_default()`. Against the new `NULL`s that silently returns
   the `0.0`s this work removes. Now `Option<f64>`.

3. **A weighted sum with no weight accounting.** Weights totalling 1.00, evidence for 0.90, and a
   fabricated zero closing the gap — capping every score at 0.90 and shifting every level band. Now
   normalised over measured weight, with the neutrality assumption recorded in the row.

4. **The same defect on a user-visible endpoint.** `GET /risk` built
   `RiskFactors { security_sensitivity: 0.0 }` and emitted a real `CRITICAL/HIGH/MEDIUM/LOW` level
   from it. Now scored via `calibrate_partial` with coverage reported.

5. **A detector that could not tell "measured zero" from "unmeasured."** My first version flagged
   any constant, which flagged a legitimate measured `change_frequency = 0.0`. Corrected with a
   `!has_source` guard — caught by a test rather than shipped.

---

## Stated consequences, not buried

- **Every `risk_assessments.score` rises by exactly ×1.1111** (`1 / 0.90`). Exact, not approximate.
- **Phase 3's `HighRisk` regression tier will select more tests** — nodes scoring in the old range
  **[0.63, 0.70)** now cross `HIGH_RISK_THRESHOLD`. The tier was under-selecting because it read
  understated scores; this is a correction, and it is in the **safe** direction. `change_impact_tests`
  pass unmodified because they write `score` directly.
- **`GET /risk` scores rise** by the same factor and the response **gains four fields**
  (`measured_weight`, `unmeasured_weight`, `weighted_contribution`, `factors_detail`). **No existing
  field changed**, and `api_tests` (313) pass unmodified.
- **The Phase 4 L4 divergence between the two risk formulas is untouched** — `intelligence.rs`'s
  weights are not `FACTOR_WEIGHTS`, so it cannot call `calibrate_partial`. Recorded, not resolved;
  merging the two is outside this remediation.

---

## Honest notes on the work itself

- One remediation belief was wrong: `calibrate_risk`'s weights were thought to sum to 0.90. They sum
  to **1.00**. Caught by an assertion on the sum, which is now a permanent guard. **No code change
  resulted from the wrong belief.**
- Five tests were wrong on first draft — a mis-assumed constant, a mis-designed fixture, a
  mismatched engine comparison, and two wrong expectations. Each was diagnosed to its cause before
  fixing; each is documented in the test file with what the first draft got wrong.
- **`cargo fmt --check` fails on one pre-existing line** — `auth.rs:317`, a `concat!` inside
  `#[doc = …]` in `role_gate!`, where rustfmt is not idempotent. Present at `HEAD`, from an earlier
  phase's fmt run, unrelated to this work. Not touched. Every file in this remediation is fmt-clean.

---

## Open items requiring a decision (not fixed here)

1. **R1** — `GET /risk` uses `.unwrap_or_default()` on the relation read, so a store read failure
   becomes a *measured* "zero dependency risk". Propagating it turns a `200` into a `500`; that is a
   product decision.
2. **R2** — `change_frequency` is now a real measurement but enters no formula.
3. **R3** — `security_sensitivity` and `business_criticality` still have no source. Making them
   real is a schema decision.
4. **F1** — `store::set_task_status` branches `if set_started { … } else if set_finished { … }`, so
   one call with both flags silently skips `finished_at` / `reason`. Latent, not active.
5. **Phase 4 L4** — merge the two diverging risk formulas.

---

**Nothing was committed. Nothing was pushed. Phase 7 has not been started.**
