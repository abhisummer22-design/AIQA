# Phase 3 — PASS

**Phase number:** 3 of 6
**Implementation:** 2026-10-07 · **Report finalised:** 2026-10-08
**Status:** PASS

## Summary

Deterministic retrieval over stored history, delivered in `src/qa_memory.rs`
(1,326 lines) with 21 tests.

The phase added the three retrievals that genuinely did not exist — **module risk
history**, **requirement history**, **release history** — and rebuilt **test stability**
on the stable case id Phase 2 introduced instead of the free-text `task_name` it had
been keyed on. **Failure history and bug history** were extended rather than rebuilt;
`FailureInvestigation` and `/intelligence/failures` already served them.

The master plan's six retrieval rules are enforced by test rather than documented:
project-scoped, deterministic (byte-identical repeat reads), evidence-backed,
time-aware, explainable, and bounded.

## Files changed

| File | Kind |
|---|---|
| `services/orchestrator/src/qa_memory.rs` | new — 1,326 lines; 6 retrieval functions, `InsufficientData`, `Signal`, `Window` |
| `services/orchestrator/src/qa_memory_retrieval_tests.rs` | new — 21 tests, 1,013 lines |
| `services/orchestrator/src/lib.rs` | modified — registers `qa_memory` and its test module |

## Tests run

```text
cd services/orchestrator
cargo test --lib qa_memory_retrieval    # this phase
cargo test                             # full suite
cargo clippy --all-targets
cargo fmt --check
```

## Tests passed

| | |
|---|---|
| Phase suite | **21 passed / 0 failed** |
| Full backend suite | **1763 passed / 0 failed / 2 ignored** (exit 0) |
| Baseline before this phase | 1742 passed / 0 failed / 2 ignored |
| clippy | 0 errors; 5 warnings in `qa_memory.rs` found and cleared during this phase |
| rustfmt | clean for both new files |

## Bugs found

**Three, all in tests — no production defect in this phase.**

| # | Failure | Why the test was wrong |
|---|---|---|
| T1 | empty-project test expected `NO_EXECUTIONS` for an unknown release | `release_history` checks existence before history; `UNKNOWN_SUBJECT` is the more precise truth — it says the release is not here, where `NO_EXECUTIONS` would imply it is |
| T2 | "module whose cases never ran" expected `NO_MATCHING_HISTORY`, got `NO_EXECUTIONS` | the fixture created **no history at all**, so `NO_EXECUTIONS` was correct; the test name promised something the fixture did not set up |
| T3 | cross-project release read expected `NO_EXECUTIONS`, got `UNKNOWN_SUBJECT` | same ordering issue as T1, viewed from the isolation test |

**T3 deserves recording plainly:** its original assertion would have **passed while the
very leak it exists to catch was happening** — it asserted the *other* project had
results. Correcting it made the test stricter. Net coverage went up, not down.

## Bugs fixed

All three, by correcting a false premise rather than changing behaviour. No production
code was altered to make a test pass, and no existing test was weakened.

## Regression result

Green throughout. `1742 → 1763 passed`, 0 failed. Re-verified unaffected: run creation,
engine scheduling, execution, regression, adaptive intelligence, coverage, failure
intelligence, healing, RBAC probing, retention, audit logging.

## Known limitations

1. **Pre-Phase-2 telemetry can never gain a case link** — those plans never carried
   `test_case_id`, so per-case history must not be claimed for those runs.
2. `environment` is still the literal `'test'` in live data.
3. **`bug_history` shipped broken in this phase** — it compared a human-text cluster
   signature against a SHA-256 telemetry fingerprint, so every defect reported touching
   no tests. Found by Phase 4's tests, fixed under Phase 6's migration 0030.
4. `evidence_refs` is always `[]`, so no retrieval reports evidence rows.

## Next phase prerequisites

Phase 4 (Memory → Test Planning Intelligence) must read before changing code:

1. `docs/qa-intelligence-memory/PHASE_1_AUDIT.md` — §7 duplication risks, §4 missing data
2. `docs/qa-intelligence-memory/PHASE_3_REPORT.md` — §4 the six rules, §11 handoff notes
3. `docs/qa-intelligence-memory/PHASE_3_PASS.md` — this file
4. `services/orchestrator/src/qa_memory.rs` — the `Signal` shape to build on
5. `services/orchestrator/src/adaptive_intelligence.rs` — `QaIntelligenceFeedback`,
   `generate_planning_feedback` (already wired into the planning agent)
6. `services/orchestrator/src/agents/planning.rs` — `qa_risk_signals: Vec<String>`, which
   carries no metric, evidence, confidence or window

Binding rules for Phase 4: no second planning agent; extend
`QaIntelligenceFeedback` rather than replacing it; every signal must carry a historical
signal, a metric, an evidence reference, a confidence and a time window; and no evidence
must mean **no signal**, not a neutral-sounding one.