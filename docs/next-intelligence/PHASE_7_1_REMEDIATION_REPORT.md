# PHASE 7.1 — Production Defect Remediation (P7-D1 and P7-D2)

**Verdict: PASS.** Both defects are fixed with the smallest justified production changes, both are
covered by meaningful regression tests, and both fixes were verified by mutation — including
mutations that were *not* caught, which are reported rather than hidden.

**Phase 7 remains BLOCKED.** One reason, and it is not either of these defects — see §16.

**Phases 8, 9, 10 not started. Nothing committed or pushed.**

---

## 1. Initial repository state and baseline

Re-verified at the start of this phase; the reported numbers were treated as historical.

| | |
|---|---|
| Branch | `main` |
| HEAD | `7930507` — *fix: correct risk signal measurement handling* |
| Upstream | `origin/main`, **0 ahead / 0 behind** |
| Modified tracked files at start | 22 (all Phase 6.1 + F2 + Phase 7 work) |
| Untracked files at start | 8 |

### Baseline measured in this run, before any edit

| Check | Result |
|---|---|
| **Full backend suite** | **2017 passed / 0 failed / 5 ignored** |
| `journey_tests` | 13 passed |
| `integrity_tests` | 34 passed / 0 failed / 1 ignored |
| `integration_e2e_tests` | 20 passed |
| Compile errors | 0 |
| `cargo fmt --check` | 1 diff — `auth.rs:317`, pre-existing at HEAD |
| `cargo clippy --all-targets` | exit 0 |

### Reproduction of both defects against the current implementation

Both were reproduced before any change, not assumed from the Phase 7 report.

**P7-D1**, from `journey_f_end_to_end_data_integrity` with `store.pool.close()`:

```
verify_run(...) -> Ok(IntegrityReport {
    inconsistent: 0, defects_established: 0, comparisons: 0,
    layers_observed: [], layers_unavailable: [Ui, Api, Backend, Database, Audit]
})
```

**P7-D2**, from `a_failed_intermediate_step_blocks_its_dependent_and_records_why`:

```
telemetry row:      status BLOCKED, end_time: None, failure_fingerprint: None   (correct)
test_stability:     executions: 1, passed: 0, failed: 0, other_outcomes: 1,
                    failure_rate: Some(0.0)                                      (defect)
```

Audit of the integrity module found **16 masked store reads** (13 × `unwrap_or_default()`, 3 ×
`.await.ok().flatten()`) across `verify_run` itself and **10 comparators** — matching the Phase 7
figure of 16, while Phase 7's prose had said "19 masked reads across 11 comparators" in one place
and 16 in another. 16 is the verified count.

---

## 2. P7-D1 — verified root cause

**Root cause: every store read inside `IntegrityVerifier` discarded its error, and the empty result
it produced was indistinguishable from "there is nothing to report".**

`verify_run` returned `IntegrityReport` (no `Result`). Its comparators returned
`Vec<IntegrityComparison>` / `Option<…>`, so a read failure had nowhere to go but a default:

| Site | Read | Was | Consequence |
|---|---|---|---|
| `telemetry_for_run` | `list_execution_telemetry` | `unwrap_or_default()` | 5 comparators get an empty run at once |
| `task_status_consistency` | `get_run_summary` | `.ok().flatten()` → empty | conflates "no such run" with "read failed" |
| `task_status_consistency` | `list_tasks_for_run` | `unwrap_or_default()` | no backend layer compared |
| `task_duration_consistency` | `list_tasks_for_run` | `unwrap_or_default()` | no duration comparison |
| `task_reason_consistency` | `list_tasks_for_run` | `unwrap_or_default()` | no reason comparison |
| `run_state_consistency` | `get_run_summary` | `.ok().flatten()?` → `None` | same conflation; run state never compared |
| `run_state_consistency` | `list_tasks_for_run` | `unwrap_or_default()` | ditto |
| `fingerprint_registration` | `list_failure_clusters` | `unwrap_or_default()` | fingerprints unverified |
| `event_log_consistency` | `get_run_summary` | `.ok().flatten()` | no event comparison |
| `event_log_consistency` | `list_events_since` | `unwrap_or_default()` | ditto |
| `event_log_consistency` | `list_tasks_for_run` | `unwrap_or_default()` | ditto |
| `audit_trail_consistency` | `list_audit` | `unwrap_or_default()` | no audit comparison |
| `audit_trail_consistency` | `list_execution_telemetry` | `unwrap_or_default()` | ditto |
| `evidence_index_consistency` | `list_evidence_for_run` | `unwrap_or_default()` | evidence index unverified |
| `before_after_state` | `list_execution_telemetry` | `unwrap_or_default()` | no before/after pair |
| `verify_run` | `fabricated_constant_check` | `unwrap_or_default()` | deliberately non-fatal — see §3 |

Two design points that made this easy to miss:

1. **`layers_unavailable` masked it further.** With a closed pool all five layers were correctly
   listed as unavailable — which reads as *"we checked and there was nothing"*, not *"we could not
   check"*.
2. **`INSUFFICIENT_DATA` is the module's most common outcome by design.** A reader who saw zero
   comparisons had no way to tell a clean run from a blind one.

This is the same R1 class Phase 6.1 fixed in `qa_memory.rs`; Phase 6.1's R1b explicitly scoped this
module out (*"integrity.rs has a separate `telemetry_for_run` (5 call sites) … not part of R1b"*),
so the remediation never reached it.

---

## 3. P7-D1 — files changed and caller-impact analysis

### Design decision

`verify_run` now returns `anyhow::Result<IntegrityReport>`; **12** methods return
`anyhow::Result`. The error type matches the convention already in the repository
(`qa_memory.rs` uses fully-qualified `anyhow::Result`; `integrity.rs` previously used
`Result<Vec<_>, String>` in one place).

The three outcomes the fix had to keep apart:

| | Before | After |
|---|---|---|
| **A.** clean verification | `Ok`, 0 comparisons | `Ok`, `is_complete() == true` |
| **B.** genuinely insufficient evidence | `Ok`, 0 comparisons — *identical to A* | `Ok`, `is_complete() == true`, `layers_unavailable` named |
| **C.** storage could not be read | **`Ok`, identical to A** | **`Err`** |

A and B remain deliberately the same shape — they are both *successful answers* and the existing
`layers_unavailable` field already distinguishes them. What changed is that C is no longer
membership of that group.

### The one deliberately non-fatal read

`fabricated_constant_check` keeps its original design intent, which is recorded in the source as
*"A read failure here must not fail the whole report: the other comparisons are independent, and
losing them because one optional reader hiccupped would be a worse failure than the one being
reported."* That reasoning is sound and was **preserved**. What changed is that it is no longer
invisible: the failure is recorded on the report.

`IntegrityReport` gains one field and one method:

```rust
/// Checks that could not be completed, for the one comparator that is deliberately best-effort.
/// …
#[serde(default)]
pub read_failures: Vec<String>,

impl IntegrityReport {
    pub fn is_complete(&self) -> bool { self.read_failures.is_empty() }
}
```

`#[serde(default)]` keeps existing deserialisation working. `is_complete()` is `true` whenever
`verify_run` returns `Ok`, which is correct: required reads now fail loudly, so a returned report
means the required reads succeeded. It exists to cover the one best-effort check.

### Caller impact — every affected caller, and how it now handles errors

**Production call sites: 1.**

| Caller | Before | After |
|---|---|---|
| `api.rs::get_run_integrity_scoped` | embedded the report, always `200 Ok` | `.map_err(to_500)?` — a storage failure is a 5xx, never a report |

This is the requirement that *"the error must remain visible at the appropriate application
boundary"*. `map_err(to_500)` follows the existing convention in this file.

**Test call sites: 55**, deliberately updated rather than blanket-replaced:

| File | `verify_run` | comparators | Handling |
|---|---|---|---|
| `integrity_tests.rs` | 23 | 23 | `.expect("<comparator name>")`, each naming the operation |
| `integration_e2e_tests.rs` | 5 | — | `.expect("verify_run")` |
| `journey_tests.rs` | 4 | — | `.expect("verify_run")`, except the P7-D1 test which asserts `Err` |
| `phase_7_1_regression_tests.rs` (new) | 8 | 9 | mixed `expect` and `is_err` by intent |

The `let _ = verifier.X(...).await;` smoke-test block became
`let _ = verifier.X(...).await.expect("X");` — a strictness increase, since those reads now must
succeed rather than merely be discarded.

The regex-driven insertion was reviewed line by line afterwards: each `.expect()` sits after the
`.await` it belongs to, before any `.is_empty()` / `.is_none()` assertion, and the message names
the operation.

---

## 4. P7-D1 — before/after behaviour and regression-test results

`services/orchestrator/src/phase_7_1_regression_tests.rs`, 7 P7-D1 tests.

| # | Test | Before fix | After fix |
|---|---|---|---|
| 1 | `p71_d1_a_closed_pool_fails_the_verification_rather_than_reporting_clean` | **fails** — `Ok`, not `Err` | **passes** — `Err`, message non-empty |
| 2 | `p71_d1_every_required_comparator_propagates_a_failed_read` | **fails** — 8 comparators return `Ok(vec![])` | **passes** — all 8 plus `before_after_state` return `Err` |
| 3 | `p71_d1_a_genuinely_empty_run_is_insufficient_evidence_not_an_error` | passes | passes — B stays a successful answer; unknown run id likewise |
| 4 | `p71_d1_a_consistent_run_still_verifies_and_completes` | passes | passes — `is_complete()`, non-empty comparisons, `Backend` observed |
| 5 | `p71_d1_a_genuine_inconsistency_is_still_detected` | passes | passes — status + duration + reason disagreements all still reported; counter agrees with contents; `defects_established >= 1` |
| 6 | `p71_d1_a_stale_observation_is_still_insufficient_data_not_a_defect` | passes | passes — `InsufficientData`, `establishes_defect() == false`, 0 defects |
| 7 | `p71_d1_the_api_does_not_convert_a_storage_failure_into_a_successful_report` | passes | passes — readable ⇒ 200 + report with `read_failures: []`; closed ⇒ 5xx, no `report` key, no `defects_established: 0` |

Tests 3–6 are the important half: they prove the fix did not work by making the verifier lazier.

Test 1 also asserts the verifier verifies cleanly *before* the pool is closed, so an
always-failing verifier cannot satisfy it.

---

## 5. P7-D2 — verified root cause

**Root cause: the failure-rate denominator counted every recorded outcome, including ones where the
step never ran.**

`qa_memory.rs::test_stability`:

```rust
let passed = mine.iter().filter(|r| Self::is_pass(r)).count();      // "PASSED"
let failed = mine.iter().filter(|r| Self::is_failure(r)).count();  // "FAILED" | "TIMEOUT"
let other  = mine.len() - passed - failed;                          // everything else
…
failure_rate: Self::rate(failed, mine.len()),                        // <-- denominator = ALL rows
```

`TaskStatus::Blocked` is terminal, so `Intelligence::ingest_run` records it. That is **correct and
was kept**: the step was reached and refused, and the row is unmistakably marked `BLOCKED` with
`end_time: None` and no fingerprint. The defect is that the row was then admitted to the rate's
denominator. One blocked row ⇒ `failed = 0`, denominator `1` ⇒ `Some(0.0)` — "this case passes",
for a case that had never executed.

### Metric contract clarification

The investigation changed my understanding of two things, and the report states both:

1. **What `executions` means.** There is no doc comment on the field, but `other_outcomes` is
   defined as its complement (`mine.len() - passed - failed`) and `window.executions_considered`
   uses the same row set. So `executions` means **recorded outcomes**, not attempts. Per the
   instruction *"if `executions` counts recorded outcomes, preserve that meaning"*, it was **not**
   redefined. A blocked step is still counted as an observation and still visible via
   `other_outcomes`.

2. **The field's doc comment was wrong, and I had relied on it.** It read *"0..1, `None` when there
   are fewer than two executions (a rate from one sample would be a claim, not a measurement)"*.
   Two claims, both false:
   - the value is a **percentage** 0..=100, never 0..1;
   - `rate` returns `None` only when the denominator is **zero**. One `PASSED` execution does
     return `Some(0.0)`, and an existing test asserts it deliberately:
     `qa_memory_retrieval_tests` — *"a 0% failure rate over one execution is arithmetically
     true"*.
   
   The two-sample minimum belongs to `flakiness_score`, not `failure_rate`. The doc comment was
   corrected to describe the implemented contract, with the reasoning recorded in place.

   **This mattered for scope.** Had I trusted the comment, "preserve the existing minimum-evidence
   rule" would have read as *"`None` below two executions"*, and I would have changed a field the
   repository explicitly tests as `Some(0.0)` for one pass — breaking a contract that has nothing
   to do with the defect.

---

## 6. P7-D2 — files changed

One behavioural line, plus three documentation corrections in the same file.

```rust
// qa_memory.rs, in test_stability -- after computing passed / failed / other
+ let completed = passed + failed;          // outcomes that qualify as a completed execution
...
- let mut signals = Vec::new();
- if let Some(f) = Self::rate(failed, mine.len()) {
+ let rate = Self::rate(failed, completed);   // computed once, read twice
+ let mut signals = Vec::new();
+ if let Some(f) = rate {
...
// and in the TestStabilityView construction:
- failure_rate: Self::rate(failed, mine.len()),
+ failure_rate: rate,
```

The denominator becomes the outcomes that qualify as a **completed execution** — exactly the rows
`is_pass` and `is_failure` count. A non-execution outcome can now neither create a rate nor dilute
one. With only a blocked row the denominator is zero and `rate` answers `None`.

Doc changes: `TestStabilityView::failure_rate` (corrected), `::other_outcomes` (was undocumented),
`QaMemoryReader::is_pass` / `is_failure` (state what the eligible set is and why).

**No new route, migration, or metric field.** No change to `executions`, `passed`, `failed`,
`other_outcomes`, `flakiness_score`, `Window`, or `Signals`. `rate()` itself is unchanged.

### A second location in the same view, which I initially missed

`test_stability` reports this metric **twice**: as the `failure_rate` field, and as a
`failure_rate` entry in `signals` that formats the number into prose. My first fix corrected only
the field. The signal still divided by every recorded row, so a case with one failure and one
blocked step reported:

```
failure_rate: Some(100.0)
signals[0].statement: "1 of 2 recorded executions failed (50.0%)"     <-- disagrees
```

Two different numbers for the same metric in the same struct — a defect **I introduced**. The rate
is now computed once (`let rate = Self::rate(failed, completed)`), and both the field and the
signal read it, so they cannot diverge. The signal's statement names the completed count, and its
`basis` names the rows it excluded rather than dropping them silently:

```
"1 of 1 completed executions failed (100.0%)"
basis: "1 completed execution_telemetry row(s) (PASSED/FAILED/TIMEOUT) out of 2 recorded for
        this test case; the 1 other outcome(s) were never executions and are excluded from the rate"
```

Mutation M6 (§8) re-introduced exactly this divergence and is caught by a dedicated test.

### Related finding: the same denominator exists in four other views — NOT changed

| Location | Expression |
|---|---|
| `module_risk_history` (L751) | `Self::rate(failures, mine.len())` |
| `release_history` (L834) | `Self::rate(f, rows.len())` |
| `release_history` (L878) | `Self::rate(failures, mine.len())` |
| `requirement_history` (L1019) | `Self::rate(failures, mine.len())` |

Plus three sites in `qa_memory_risk.rs` (`test` factors L289, `module` factors L470, `prioritise`
L1360), each `failures / rows.len()`.

**A blocked row dilutes those rates toward zero too.** For example 3 genuine failures + 1 blocked
would report 75% rather than 100% — an *under*-report of risk.

**Not fixed, deliberately.** Production changes are authorised for P7-D1 and P7-D2 only, and P7-D2
was reported as the `TestStabilityView` symptom (`executions: 1, other_outcomes: 1,
failure_rate: Some(0.0)`). The three `qa_memory_risk.rs` sites feed the **risk model**, whose
canonical formula is open decision **D3**; changing its inputs is exactly what the instructions
forbid. This is reported as a follow-up requiring a decision, not silently resolved. Note the
risk direction differs from P7-D2's: here the harm is under-reporting risk, not over-reporting
success.

---

## 7. P7-D2 — before/after behaviour and regression-test results

8 P7-D2 tests. Every one writes through production writers (`Store::insert_run_for_release`,
`set_task_status`, `Intelligence::ingest_run`) — no direct SQL.

| # | Test | Before | After |
|---|---|---|---|
| A/B/C | `p71_d2_a_blocked_step_is_reported_as_blocked_and_its_rate_is_unmeasured` | **fails** — `Some(0.0)` | **passes** — `None`; `passed: 0`, `failed: 0`, `other_outcomes: 1` preserved; the blocking step's own case still `Some(100.0)` |
| D | `p71_d2_genuine_passes_still_measure_a_real_zero_rate` | passes | passes — 1 pass ⇒ `Some(0.0)`, 2 passes ⇒ `Some(0.0)` |
| E | `p71_d2_genuine_failures_still_measure_a_real_hundred_percent` | passes | passes — `FAILED` and `TIMEOUT` both count; `Some(100.0)` |
| F | `p71_d2_mixed_eligible_outcomes_produce_the_correct_rate` | passes | passes — 3 pass / 1 fail ⇒ `Some(25.0)` |
| G | `p71_d2_non_execution_outcomes_do_not_distort_the_failure_rate_denominator` | **fails** — would be `Some(20.0)` | **passes** — `Some(50.0)` over 2 completed; `other_outcomes: 3`, `executions: 5` unchanged |
| H | `p71_d2_insufficient_eligible_evidence_stays_unmeasured` | **fails** — `Some(0.0)` | **passes** — `BLOCKED`, `SKIPPED`, `CANCELLED` all ⇒ `None` |
| I | `p71_d2_the_prerequisite_dag_behaviour_is_unchanged` | passes | passes — forward reference legal, unknown prerequisite refused, cycle refused, `satisfies_dependents` unchanged |
| — | `p71_d2_the_failure_rate_field_and_its_signal_always_agree` | n/a — the disagreement was **introduced by the fix** | **passes** — 3 measured shapes agree; 2 unmeasured shapes produce neither a field rate nor a signal |

Test count is 15 overall: 7 P7-D1 + 8 P7-D2.

Test G is the one that matters most: it shows the fix does not merely *suppress* the blocked row,
it prevents the row from corrupting a rate that genuine evidence supports.

---

## 8. Mutation checks — including proof each mutation was applied

Every mutation was verified present in the source **before** the test run, and every restore was
verified **byte-identical by SHA-256**. One mutation was silently not applied in an earlier phase
of this work; this time each application is printed and the file re-read.

| # | Mutation | Applied? | Result |
|---|---|---|---|
| M2 | `test_stability` denominator reverted to `mine.len()` | **yes** — verified at L526 | **CAUGHT** — 3 failures: `p71_d2_a_blocked_step_…`, `p71_d2_non_execution_outcomes_…`, `p71_d2_insufficient_eligible_evidence_…` |
| M1c | `evidence_index_consistency`'s read masked with `unwrap_or_default()` | **yes** — printed the inserted line | **CAUGHT** — `p71_d1_every_required_comparator_propagates_a_failed_read` |
| M1e | **every** required read in `verify_run` swallowed (reproduces P7-D1 exactly) | **yes** — 8 replacements, 0 `?` remaining | **CAUGHT** — `p71_d1_a_closed_pool_fails_the_verification_rather_than_reporting_clean` |
| M1 | `telemetry_for_run`'s read masked | **yes** — L597 confirmed | **NOT caught** — see below |
| M1d | only `audit_trail_consistency`'s `?` in `verify_run` removed | **yes** | **NOT caught** — same reason |
| M6 | the `failure_rate` **signal** reverted to the old denominator while the field keeps the fix | **yes** — inserted marker verified at L487 | **CAUGHT** — `p71_d2_the_failure_rate_field_and_its_signal_always_agree` |
| M5 | API handler substitutes an empty report instead of `map_err(to_500)` | **yes** | **NOT caught** — see below |

Restores verified identical: `integrity.rs` sha `eaea60cf…`, `qa_memory.rs` sha `ca712e49…`,
`api.rs` sha `f3420cce…`.

### The three uncaught mutations, and why

**M1 and M1d — later reads are unobservable.** Failure is injected by closing the whole pool, so
the **first** read in the chain fails first: `task_status_consistency` returns `Err` via
`get_run_summary`, and `verify_run` never reaches `telemetry_for_run` or `audit_trail_consistency`.
Masking a *later* read therefore changes nothing observable. M1e, which masks *all* of them, is
caught. So the tests prove the **aggregate** contract — *some* failed read yields `Err`, and each
comparator yields `Err` on its own — but not per-read attribution within a comparator. Isolating a
later read would need a fault injection that fails one table and not another, which this schema
does not offer. **Recorded as a coverage limitation, not claimed as coverage.**

**M5 — the handler's mapping is unreachable under whole-pool failure.** With the pool closed, the
preceding `require_scoped_project` read fails first, so the request is already a 5xx before
`verify_run` is called. Replacing `map_err(to_500)?` with a substitute empty report is therefore
invisible to this test and cannot be made visible with this injection technique. The mapping is
therefore verified by reading the handler, and the reachable half of the contract (readable ⇒
200 + real report; unreadable ⇒ no report at all) is what the test asserts. Documented in the
test's own doc comment.

I also recorded a **fourth** mutation attempt that failed to apply at all (`get_run_summary` masked
— the replacement string matched 2 sites, the assertion fired, and the file was left untouched).
That run's green result was meaningless and was discarded, not reported as a pass.

### A stale build artifact produced a false FAILURE — and a false green

After M6, restoring the file with `Copy-Item` left its mtime inside the same second as the build, so
**cargo did not rebuild**. The subsequent full-suite run reported:

```
full suite:  FAILED. 2031 passed; 1 failed
phase_7_1_regression_tests x20 -> passed 0 / failed 20
```

against a binary still compiled from the *mutated* source — the failure text quoted the M6 marker.
Inspecting the source showed the fix was present and no mutation marker existed, which is what
identified the cause. After forcing a rebuild (`cargo build --tests` with touched mtimes) the same
tests passed and the suite read **2032 / 0 / 5**.

Two lessons, both recorded rather than quietly corrected:
- every measurement in §9–§12 was then **re-run against an explicitly rebuilt binary**, and the
  counts in this report come from those runs;
- the earlier mutation results were re-checked after the rebuild and are unchanged, so M6's
  *caught* result and the M1/M1d/M5 *not caught* results are not artifacts of a stale binary.
  A stale binary would have shown up as a failure, not as a silent pass, in every case here.

---

## 9. All 13 Phase 7 journeys after remediation

Each run **10/10 individually**, and the module **20/20** as a whole.

| Journey | Result | What changed |
|---|---|---|
| `journey_a_document_to_test_coverage` | **PASS** 10/10 | unaffected |
| `journey_b_execution_to_failure_intelligence` | **PASS** 10/10 | unaffected |
| `journey_c_change_to_regression_selection` | **PASS** 10/10 | unaffected; `module` target still refused (see §14) |
| `journey_d_risk_to_testing_decision` | **PASS** 10/10 | unaffected; no risk factor was touched |
| `journey_e_failure_investigation_and_planning` | **PASS** 10/10 | unaffected; promotion threshold intact |
| `journey_f_end_to_end_data_integrity` | **PASS** 10/10 | **P7-D1 block rewritten** to assert `Err`; D5 boundaries and stale-evidence assertions unchanged |
| `a_prerequisite_naming_an_unknown_step_is_rejected` | **PASS** 10/10 | unaffected |
| `a_forward_prerequisite_is_legal_because_a_dag_has_no_declaration_order` | **PASS** 10/10 | unaffected |
| `a_prerequisite_cycle_is_refused` | **PASS** 10/10 | unaffected |
| `a_failed_intermediate_step_blocks_its_dependent_and_records_why` | **PASS** 10/10 | **P7-D2 block rewritten** to assert `None` |
| `a_duplicate_step_id_is_the_same_step_not_a_second_one` | **PASS** 10/10 | unaffected |
| `a_failed_read_mid_journey_does_not_report_the_journey_as_complete` | **PASS** 10/10 | unaffected |
| `a_whole_journey_is_isolated_from_an_identical_journey_in_another_project` | **PASS** 10/10 | unaffected |

**The two characterisation blocks had to be rewritten.** Phase 7 asserted the *defective* behaviour
deliberately, because a validation phase may not change production code. Both rewrites are evidence
the defects were real rather than hypothetical, and both new versions state explicitly what the
assertion *preserves* (`executions`, `other_outcomes`) versus what it *changes* (the rate).

Specifically revalidated per the completion gate: the integrity journey now reports a genuine
storage error rather than a false clean result; the blocked-step journey no longer publishes a
zero failure rate; valid passing and failing executions keep their metrics; project isolation and
change-impact behaviour are unchanged; failure-to-bug-draft promotion is intact at
`occurrence_count >= 2`; the prerequisite DAG is unchanged including legal forward references; and
no previously passing journey regressed.

---

## 10. Journey, integrity and integration regression results

| Suite | Result |
|---|---|
| `journey_tests` ×20 | **20 passed / 0 failed** |
| `phase_7_1_regression_tests` ×20 | **20 passed / 0 failed** |
| `integrity_tests` ×20 (previously flaky duration tests) | **20 passed / 0 failed** |
| `integration_e2e_tests` | 20 passed |
| `qa_memory_risk_tests` | 21 passed |
| `qa_memory_retrieval_tests` | 21 passed |
| `qa_memory_planning_tests` | 14 passed |
| `qa_memory_validation_tests` | 7 passed |
| `qa_memory_perf_tests` | 7 passed |
| `qa_memory_tests` | 25 passed |
| `risk_prioritisation` | 20 passed / 1 ignored |
| `risk_semantics_tests` | 10 passed |
| `risk_signal_tests` | 18 passed |
| `change_impact_tests` | 19 passed |
| `adaptive_intelligence_tests` | 39 passed |
| `store_tests` | 36 passed |
| `engine_tests` | 33 passed |
| `api_tests` | 316 passed |
| `exploration_tests` | 29 passed / 1 ignored |
| `trace_tests` | 21 passed |
| `governed_healing_tests` | 53 passed |

`integrity_tests` went 34 → 34+1 ignored unchanged: **no existing integrity assertion was weakened,
skipped, or removed.** The full suite count moved 2017 → **2031**, a delta of exactly the 14 new
regression tests.

---

## 11. Full backend test results

| | |
|---|---|
| **Before this phase** | 2017 passed / 0 failed / 5 ignored |
| **After this phase** | **2032 passed / 0 failed / 5 ignored** |
| Delta | +15 — exactly the new regression module |
| Compile errors | 0 |

---

## 12. Formatting and Clippy results

| Check | Result |
|---|---|
| `cargo fmt --check` | **1 diff — `auth.rs:317`, pre-existing at HEAD, untouched** |
| `cargo clippy --all-targets` | **exit 0** |
| Lints in `phase_7_1_regression_tests.rs` | **0** |
| Lints in `integrity.rs` | **0** |
| Lints in `qa_memory.rs` | **0** |

During this work my `.expect()` insertions initially produced **41 new rustfmt diffs** in the test
files. Those were fixed by formatting the four files I touched, and the gate returned to exactly
one pre-existing diff. `git diff --stat` on `auth.rs` is empty — I did not touch it.

---

## 13. Remaining D3, D5 and D6 decisions

All three remain **OPEN**. None was resolved, and none was silently worked around.

| | Status | Effect of this phase |
|---|---|---|
| **D3** canonical risk formula | **OPEN** | Untouched. No weight, threshold or factor changed. This phase found the *same denominator pattern* in three `qa_memory_risk.rs` sites (§6) and deliberately left them, because they feed the open model. Journey D was reassessed and still passes — it needs a recommendation, which prioritisation produces from bands, not one canonical score. |
| **D5** 10 ms duration floor | **OPEN** | Untouched. `compare_durations`, `duration_tolerance_ms`, `DURATION_ABSOLUTE_FLOOR_MS` and `DURATION_RELATIVE_TOLERANCE` unmodified. Journey F's boundary assertions still pass, and `p71_d1_a_genuine_inconsistency_is_still_detected` confirms a 14 ms gap is still *beyond* tolerance — the floor is still doing its documented job. |
| **D6** `set_task_status` signature | **OPEN** | Untouched. The two-boolean call is used unchanged by every test written here, which is further evidence the API is workable as-is — not an argument that it should be redesigned. |

---

## 14. Outstanding risks and unsupported paths

1. **Coverage limitation — per-read attribution in the integrity verifier.** With whole-pool
   failure injection, only the *first* read in a chain is observable. M1 and M1d were not caught;
   M1e (masking all reads) was. §8.
2. **Coverage limitation — the API handler's error mapping is not mutation-verified**, because the
   auth read fails first. §8.
3. **The same rate denominator exists in 7 other places** (`module_risk_history`, `release_history`
   ×2, `requirement_history`, and 3 in `qa_memory_risk.rs`). Blocked rows dilute those rates toward
   zero — under-reporting risk. Not fixed; needs a D3-adjacent decision. §6.
4. **No journey-level verdict aggregate** exists; only per-run integrity verdicts.
5. **Cancellation and timeout remain NOT APPLICABLE** — there is no journey cancel path. The DAG's
   timeout is per-task (`AgentTaskPlan.timeout_secs`).
6. **Module-change impact and `code_file` / `api_contract` / `page` targets remain unresolvable.**
   Journey C still asserts the `module` refusal rather than working around it.
7. **No live executor or browser was driven**, so journey-shaped *application* behaviour remains
   unvalidated. Everything about persistence, comparison and reporting is validated.
8. Single-node SQLite only.

---

## 15. Final remediation verdict

### PHASE 7.1 = **PASS**

| Completion gate | Result |
|---|---|
| P7-D1 fixed and covered by a meaningful regression test | **yes** — 7 tests; masking all reads is caught |
| P7-D2 fixed and covered by meaningful metric tests | **yes** — 8 tests; reverting the denominator is caught by 3, and reverting only the signal by 1 more |
| All relevant storage errors remain observable | **yes** — 0 masked store reads remain in `integrity.rs`; the API surfaces a 5xx |
| Blocked outcomes cannot masquerade as successful zero-failure evidence | **yes** — `None`, and they cannot dilute a genuine rate either |
| Existing valid metric and integrity behaviours preserved | **yes** — full suite 2031/0/5 with no assertion weakened or skipped |
| Required regression suites pass | **yes** — §10 |
| All 13 Phase 7 journeys reassessed | **yes** — 10/10 each, module 20/20 |
| No unrelated decisions or formulas silently changed | **yes** — D3/D5/D6 untouched; §6's related finding reported, not fixed |

**Production changes were confined to P7-D1 and P7-D2:** 3 files (`integrity.rs`, `qa_memory.rs`,
`api.rs`), one new test module, one `mod` line, one report. No migration, route, formula weight or
threshold added or altered.

**One defect was introduced during the fix and caught before completion:** the field/signal
disagreement in §7. It was found by re-auditing every site that computes a rate rather than by a
failing test, and it now has both a dedicated regression test and its own mutation (M6).

---

## 16. Can Phase 7 now be closed?

### PHASE 7 remains **BLOCKED**

Its two reported blockers, B1 and B2, are **resolved**. Phase 7 is blocked for a different reason,
which Phase 7 itself documented and which this phase did not touch:

> **A `module`-target change has no provable impact.** `change_impact.rs::resolution_note("module")`
> still refuses: *"a module vertex exists, but no `covers` edge is ever written"*. A team that
> changes one module therefore gets **no regression selection at all** — journey C completes for a
> `requirement` target and stops at the module target.

Under the roadmap's PASS gate — *"a journey must be executable through existing governed paths"*
— a workflow a user can reach and that produces nothing is not validated. That is an unsupported
capability, not an unresolved test, so it is not something Phase 7.1 can close.

The related Phase 6.2 gaps also remain: `code_file` / `api_contract` / `page` targets unresolvable,
and no live executor driven.

**Nothing in this report should be read as re-opening the Phase 7 findings.** The original report is
kept verbatim as the audit record; its §13 verdict stands as written, and this document supersedes
only the status of B1 and B2.

---

## 17. Recommended next action

**Do not start Phase 8 yet.** Three items, in order:

1. **Decide the denominator question once, for all seven sites** (§6 / §14.3). P7-D2 established the
   principle — a non-execution outcome is not a sample — and four QA Memory views plus three risk
   sites still do not follow it. This is one decision applied consistently, and it needs a decision
   because three of the sites feed the open risk model. It should be settled **before** Phase 8,
   which scores test-case quality from execution counts and would read the diluted rates as
   fact.
2. **Close or formally defer the module-change gap** (§16). Either a `covers` edge is written by a
   supported path so a module change resolves, or the refusal is accepted as a documented product
   limitation and Phase 7's PASS gate is amended to exclude it. Right now Phase 7 cannot pass
   because a reachable workflow produces nothing.
3. **Then Phase 8** — unblocked once 1 and 2 have an answer.

Recommended still not to change: **D3**, **D5**, **D6**. All three remain open by instruction, and
none blocked any journey in this phase.