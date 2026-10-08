# Phase 4 — PASS

**Phase 4 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`: AI Risk-Based Testing + Test
Prioritization.** Full evidence in
[`PHASE_4_RISK_BASED_TESTING_REPORT.md`](./PHASE_4_RISK_BASED_TESTING_REPORT.md).

Predecessors: [`PHASE_3_PASS.md`](./PHASE_3_PASS.md) — Phase 1–3 not restarted or rewritten.

---

## Result

**PASS.**

| Criterion | Status | Where proven |
|---|---|---|
| Risk prioritization uses real evidence | PASS | every factor carries a basis and an evidence count; 18 tests over real writers |
| Existing intelligence reused | PASS | `calibrate_risk`, `Factor`, `calibrate_or_none`, `failure_severity`, `average_flakiness`, Phase 3 change impact — **0 existing risk lines modified** |
| No duplicate risk engine | PASS | no new formula, table or engine; one added scope to the module that owns risk |
| Project isolation passes | PASS | 2 tests, one proving byte-identical fingerprints across tenants do not bleed |
| Mandatory tests protected | PASS | 3 tests; ordering never excludes, floor reported, `restricted_to` enforces |
| Missing data handled honestly | PASS | 3 `INSUFFICIENT_DATA` reasons, 2 unmeasured factors with reasons, no score invented |
| Tests pass | PASS | 18/18 |
| Regression passes | PASS | 1880 / 0 failed / 3 ignored |
| Documentation complete | PASS | report |

---

## The audit finding

**A risk engine already existed, was well designed, and was simply being ignored for test
cases.** `qa_memory_risk.rs` had `module_risk` and `release_risk` and no test scope.

So Phase 4 added a third **scope** to the module that already owns risk — reusing
`calibrate_risk`, `Factor`, `calibrate_or_none`, `failure_severity` and
`average_flakiness` unchanged. **`qa_memory_risk.rs` is +734/−2 lines; every one of the 2
deletions is in `prioritise_tests`, in the two lines the shared read replaced.**

## The central result: no risk score, and that is correct

`calibrate_or_none` refuses to emit a score unless **every** factor measures. For a test
case, two have no column anywhere in this schema:

* `dependency_count` — no case-to-case dependency graph; `TaskDef.prerequisites` is per *run*
* `security_sensitivity` — no column or table at all

**Phase 4 did not invent a score to fill the gap.** It reports `weighted_contribution`,
`measured_weight`, `unmeasured_weight`, and each factor with its basis and evidence count —
and asserts `measured_weight + unmeasured_weight == 1.0`.

## Signals unavailable, and left that way

Requirement criticality (`requirements` has **no** criticality column — only `kind`, a
category, not an importance), module risk (no `covers` edge), coverage gaps (coverage is not
persisted), per-test release trends. Four of the plan's eight named signals. Each reported
as unavailable rather than approximated.

## "Why is this test high priority?" — answered with facts

Every applicable condition is listed with its own line, so the band is reconstructible. The
band comes from a **documented total order** over named conditions — deliberately *not* a new
weighted formula, which would have become a third risk model.

## Safety

Three guarantees, each tested: prioritisation **orders and never excludes** (all 6 cases
listed, band counts add up); the floor is **reported** as `mandatory_floor` so it can be
audited; and **enforcement stays in Phase 3's `restricted_to`**, whose `mandatory` set is
added to the selection and never intersected with it. A test asserts Phase 3 and Phase 4
derive the **same floor set**.

---

## Bugs found

1. **A performance defect I introduced, caught by the audit's own instruction.** My first
   version called `test_risk` per case, and `telemetry_for` is a *fresh bounded read per
   call* — so 200 cases meant 200 telemetry scans, 200 proposal reads, 200 draft reads and a
   cluster lookup per draft. Fixed with a shared `TestEvidence`.
2. **A regression I introduced in that refactor** — extracting `test_risk_from` collapsed
   `UnknownSubject` and `NoMatchingHistory` into one. My own test caught it; the UUID is now
   parsed before the evidence read.
3. **A dead guard in the tests.** `Retrieved` has three fields, so every
   `let … { view: Some(x), .. } = … else { panic!() }` was unreachable — clippy was right.
4. **A regex edit that silently deleted call arguments**, caught by the compiler. The file was
   rewritten cleanly rather than patched again.

I also found and recorded, without merging: **`intelligence.rs:557` computes a different
five-factor formula inline**, so `risk_assessments.score` is not `calibrate_risk`'s number.
Merging them would change an existing feature's output — out of scope, recorded as L4.

---

## Performance — measured, not pinned

200 cases × 5 executions = 1,000 telemetry rows, built through the real `ingest_run` path:

| | |
|---|---|
| One whole-project prioritisation | **62.6 ms** |
| 200 single-case reads (the replaced implementation) | **5.66 s** |
| **Speedup from the shared read** | **90.4×** |

`telemetry_for` is deliberately *not* cached — Phase 6's stance is one bounded read per
question. The fix was therefore not a cache but **one read per question**, shared across the
cases inside it, which also keeps every case's factors derived from the same row set.

No timing threshold is pinned in CI; the measurement test is `#[ignore]`d so the number can
be reproduced rather than asserted on someone else's hardware.

---

## Limitations carried forward

1. **`covers` / module-targeted risk unavailable** — inherited from Phase 2.
2. **Requirement criticality unused entirely** — no column; `kind` is not importance.
3. **The safety floor is derived, not declared** — no mandatory flag exists in the schema.
4. **Two disagreeing risk formulas exist** (`risk_assessments.score` vs `calibrate_risk`) —
   recorded, not merged.
5. **No test-to-test dependency signal** — `dependency_count` stays unmeasured.
6. **`measured_ratio` assumes the unmeasured 30% was neutral** — an assumption, not a
   measurement; the raw contribution is always reported and `calibration` stays `None`.
7. **No frontend surface** — backend-only; 1605 frontend tests untouched.
8. **The live database is unchanged** — no figure in the report is a production claim.
9. **No E2E CI job** — pre-existing.

---

## Discipline

* No test weakened, skipped or deleted. Zero regressions.
* No existing risk function modified; the diff is additive except two intentional lines.
* No score invented to fill a gap the data cannot fill.
* Performance measured rather than asserted, and the measurement reproducible.
* Both bugs I introduced were caught by tests written before the bug, not by inspection.

---

**Phase 5 not started. Nothing committed. Nothing pushed.**