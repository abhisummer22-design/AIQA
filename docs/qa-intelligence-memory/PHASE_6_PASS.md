# Phase 6 — PASS

**Phase number:** 6 of 6
**Completion date:** 2026-10-07
**Status:** PASS — **all six phases complete**

## Summary

Full validation of the QA Intelligence Memory system, with evidence rather than
assertion.

The end-to-end chain — requirements → planning → execution → evidence → failure
intelligence → historical memory → retrieval → risk → next plan — is driven through
production entry points in a single test, and a failure travels all the way with each
step asserted. Its mirror test proves "no defects" is a real state, not an accident of
a code path that never ran.

Performance was measured before anything was optimized, as the plan requires: five
`EXPLAIN QUERY PLAN` plans (all `SEARCH ... USING INDEX`), retrieval latency over 480
telemetry rows (33 ms worst), planning input bounded and evidence-ordered, and storage
at 319 bytes per remembered execution.

## Files changed

| File | Kind |
|---|---|
| `services/orchestrator/migrations/0030_failure_cluster_fingerprint.sql` | new — bug fix B |
| `services/orchestrator/src/qa_memory_validation_tests.rs` | new — 7 end-to-end/security/integrity tests |
| `services/orchestrator/src/qa_memory_perf_tests.rs` | new — 7 measurement tests |
| `services/orchestrator/src/qa_memory.rs` | modified — `bug_history` correlates by fingerprint |
| `services/orchestrator/src/intelligence.rs` | modified — `record_failure` writes the fingerprint |
| `services/orchestrator/src/store.rs` | modified — `FailureClusterRow.failure_fingerprint`, `upsert_failure_cluster` gains the argument |
| `services/orchestrator/src/operations.rs` | modified — the synthesized cluster row carries its fingerprint |
| `services/orchestrator/src/qa_memory_risk.rs` | modified — the target is excluded from its own trend (bug C) |
| `services/orchestrator/src/lib.rs` | modified — registers the two new test modules |
| `services/orchestrator/src/{intelligence,store_tests,operations_tests,api_tests}.rs` | modified — existing call sites pass `None`; assertions unchanged |
| `docs/qa-intelligence-memory/PHASE_6_REPORT.md` | new |
| `docs/qa-intelligence-memory/FINAL_VALIDATION_REPORT.md` | new |
| `docs/qa-intelligence-memory/PHASE_6_PASS.md` | new — this file |

## Tests run

```text
cd services/orchestrator
cargo test                             # 1809 passed / 0 failed / 2 ignored
cargo test --lib qa_memory_perf -- --nocapture    # with plan + timing + storage output
cargo test --lib qa_memory_migration -- --nocapture  # against a copy of the live DB
cargo clippy --all-targets
cargo fmt --check
cd forentend
npx tsc --noEmit                       # clean
npm test                               # 1605 passed / 79 files
```

## Tests passed

| | |
|---|---|
| Backend | **1809 passed / 0 failed / 2 ignored** (exit 0) |
| Frontend | **1605 passed / 79 files** |
| `tsc --noEmit` | clean |
| clippy | 0 errors, 0 warnings in any file this plan created |
| rustfmt | clean for every file created or edited |
| Migration on a copy of the live DB | 30 migrations applied, 19 telemetry rows preserved, 0 pre-linked |

## Bugs found

Two real code defects, plus one pre-existing security finding.

**A — a cluster could not be joined to the executions behind it.** `bug_history`
compared `cluster.signature` (normalized human text) against
`telemetry.failure_fingerprint` (SHA-256). Never equal, so every defect reported
*"touched no tests"* — absence reported as fact while the test sat in the same query.

**B — the same correlation failed a second, independent way.** The lookup passed a
cluster id where the query wanted a signature, so the result was empty for every
project.

**C — a release appeared twice in its own trend**, so its direction of travel was
computed against a duplicated point.

## Bugs fixed

**A** — migration `0030_failure_cluster_fingerprint.sql`: a nullable column plus
`record_failure` computing the fingerprint with exactly the chain telemetry uses, so the
two rows join by equality. Old clusters stay NULL and fall back to the `fails_with`
graph. A heuristic (matching on occurrence count or time window) was considered and
rejected: a guess here produces exactly the confident-but-wrong answer the plan forbids.

**B** — `get_failure_cluster_by_id_scoped`.

**C** — filter the target out of the comparison loop.

**The security finding was reported, not fixed** — see below.

## Regression result

Green throughout: `1777 → 1809 passed`. No pre-existing test was weakened, skipped or
disabled. The existing `upsert_failure_cluster` call sites pass `None`; their assertions
are unchanged.

## Security finding — reported for your decision, not changed unilaterally

`runs.plan` stores `TaskDef.params` verbatim, **including credentials**.
`adaptive_intelligence::mask_dynamic_tokens` scrubs telemetry text but is never applied
to the run plan.

The retrieval layer added by this plan is clean: it never reads `params` or echoes
`error_message`, verified by a test that seeds a secret into both and serializes all six
surfaces.

Not fixed here because it is run-plan persistence, not QA memory, and changing it means
deciding what the run engine persists for every executor. If you want it addressed, the
suggested approach is to persist a redacted copy of `params` while leaving the in-memory
`TaskDef` intact — after verifying that nothing reads `runs.plan` back to drive
execution, since `rbac_probe` and the audit log read plans.

## Known limitations

Seven are recorded in `FINAL_VALIDATION_REPORT.md` §11. The two that constrain future
work:

1. **Pre-Phase-2 telemetry can never gain a case link** — those plans never carried
   one, and mining them for a guess would fabricate history.
2. **A full `RiskCalibration` is never emitted for a module or release** — three of six
   factors have nowhere to come from, so a score would assert "no dependency risk, no
   security risk" for a schema that stores neither. Model coverage is reported as a
   number (`measured_weight` 0.70 / `unmeasured_weight` 0.30) instead.

## Next steps

No phase remains. The plan is complete. Open items that need your decision, in the
order they would pay off, are in `FINAL_VALIDATION_REPORT.md` §12 — the `runs.plan`
credential scrub and the `tasks.test_case_id` foreign key are the two that need a
judgment call rather than more work.