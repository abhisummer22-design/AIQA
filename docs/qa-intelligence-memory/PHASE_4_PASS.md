# Phase 4 — PASS

**Phase number:** 4 of 6
**Implementation:** 2026-10-07 · **Report finalised:** 2026-10-08
**Status:** PASS

## Summary

Historical intelligence now reaches the planning agent in an auditable form.

The wiring already existed — `TestPlanningAgent` already accepted
`QaIntelligenceFeedback`, already fed by `generate_planning_feedback`. What did not
exist was any way to check *why* a plan said something. Every field was one flat string
with no metric, no evidence, no confidence and no window.

`PlanningSignal` supplies all five, plus `kind` and `subject`, and is added **alongside**
the existing string fields rather than replacing them — the existing renderer and its
tests were left untouched.

A 90-day window (`PLANNING_SIGNAL_WINDOW_DAYS`) keeps old history from dominating, and
an empty project produces **no signals at all** rather than neutral-sounding ones.

## Files changed

| File | Kind |
|---|---|
| `services/orchestrator/src/qa_memory.rs` | modified — `PlanningSignal`, `PLANNING_SIGNAL_WINDOW_DAYS`, `planning_signals()` |
| `services/orchestrator/src/adaptive_intelligence.rs` | modified — `QaIntelligenceFeedback.planning_signals`, populated in `generate_planning_feedback` |
| `services/orchestrator/src/qa_memory_planning_tests.rs` | new — 14 tests, 581 lines |
| `services/orchestrator/src/agents/tests.rs` | modified — one added field on a pre-existing fixture; assertions unchanged |
| `services/orchestrator/src/lib.rs` | modified — registers the test module |

## Tests run

```text
cd services/orchestrator
cargo test --lib qa_memory_planning    # this phase
cargo test                             # full suite
cargo clippy --all-targets
cargo fmt --check
```

## Tests passed

| | |
|---|---|
| Phase suite | **14 passed / 0 failed** |
| Full backend suite | **1777 passed / 0 failed / 2 ignored** (exit 0) |
| Baseline before this phase | 1763 passed / 0 failed / 2 ignored |
| clippy | 0 errors; 2 warnings in the new files found and cleared |
| rustfmt | clean for the files this phase created or edited |

## Bugs found

Two, from different sources:

**P1 — the test, not the code.** `signals_never_cross_a_project_boundary` asserted that
the *other* project was non-empty. That assertion would have **passed while the very
leak it exists to catch was happening.** Removed; the owning project being non-empty is
now asserted instead, which is the real property.

**P2 — production code, inherited from Phase 3.** `QaMemoryReader::bug_history` called
`get_failure_cluster_by_signature(project_id, failure_cluster_id)` — passing a cluster
**id** where the query wanted a **signature**. It never matched, so `bug_history`
returned an empty list for every project, i.e. *"there are no recurring defects
anywhere"* — absence reported as fact while the data was there.

Phase 3's own tests missed it because only the **empty** case of `bug_history` was ever
asserted. A function that always returned empty passed. That is a genuine gap in
Phase 3's suite; Phase 6's end-to-end chain test is the guard that closes it.

## Bugs fixed

P1 by correcting a false premise. P2 by calling
`get_failure_cluster_by_id_scoped` — the method that already existed and was meant to be
used. No existing test was weakened, skipped or disabled, and no production code was
changed to make a test pass.

## Regression result

Green throughout. `1763 → 1777 passed`, 0 failed.

## Pattern across the plan so far

Three of the five test failures so far were wrong premises, and **two of them would
have passed while a real leak shipped**. Recording this because it is the honest
characterisation, not because it reflects well on the work: the fix in each case was to
make the assertion *stricter*.

## Known limitations

1. A defect signal carries no evidence runs — the cluster-to-run link was still broken
   at this point (P2). Phase 6's migration 0030 fixes the link; the signal's
   `evidence_runs` stays empty because recurrence is evidence on its own.
2. The window is a single flat 90 days — no per-signal decay, so a module failing
   steadily for 89 days and one failing steadily for 1 day score identically.
3. Three signal kinds only; requirement-gap and unstable-test signals were not built
   though the retrieval layer supports both.
4. Confidence is a stated convention, not a statistical model — deliberate and testable,
   but not a probability.

## Next phase prerequisites

Phase 5 (Release & Risk Intelligence) must read:

1. `docs/qa-intelligence-memory/PHASE_3_REPORT.md` — §11, that module/release risk was
   still unbuilt and Phase 5 depends on it
2. `docs/qa-intelligence-memory/PHASE_4_REPORT.md` — §6, the `bug_history` defect this
   phase inherited and fixed
3. `docs/qa-intelligence-memory/PHASE_4_PASS.md` — this file
4. `services/orchestrator/src/adaptive_intelligence.rs` — `calibrate_risk`,
   `RiskFactors`, `RiskCalibration`. Master plan §7 says "risk score if already
   supported", so Phase 5 must feed this rather than invent a second formula
5. `services/orchestrator/src/qa_memory.rs` — the retrieval views Phase 5 builds on

Binding rules for Phase 5: `INSUFFICIENT_DATA` is preferable to a fabricated risk
conclusion; a factor that cannot be measured must be `None` and excluded, never filled
with 0.0; and project/tenant isolation must hold on every new read.