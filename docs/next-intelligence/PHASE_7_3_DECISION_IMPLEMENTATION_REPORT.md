# PHASE 7.3 — Decision Implementation Report

**Verdict: PASS**, with **two subtasks BLOCKED for approval** (§15). Both blockers are recorded, not
worked around.

**Scope honoured:** Q1–Q7 as approved. **D3, D5 and D6 remain OPEN** and are documented as such. No
migration, no route, no weight or threshold changed, nothing committed or pushed.

---

## 1. Initial repository state

Verified before any edit; reported values treated as historical.

| | |
|---|---|
| Branch / HEAD | `main` / `7930507` — *fix: correct risk signal measurement handling* |
| Upstream | `origin/main`, **0 ahead / 0 behind** |
| Modified tracked | **22** |
| Untracked | **11** |

### Baseline re-measured in this run

| Check | Result |
|---|---|
| **Full backend suite** | **2032 passed / 0 failed / 5 ignored** |
| `cargo fmt --check` | 1 diff — `auth.rs:317`, pre-existing |
| `cargo clippy --all-targets` | exit 0 |

---

## 2. The inventory, re-scanned and verified

Phase 7.2 reported 15 sites. I re-derived the list from scratch by scanning every rate-like
division in non-test production code, and got **17 divisions resolving to 16 quantities**.

**The reconciliation:** Phase 7.2's 15 *sites* counted **field and signal assignments separately**
(S2 signal L718 vs S2 field L772, and the same for S4 and S5). The 17 divisions are the *code
sites* behind those 15 metrics plus two that Phase 7.2 described but did not number — `api.rs` L9196
(S11) and `intelligence.rs` L531 (S12) — because its scan pattern required `Self::rate(` or a
`/ rows.len()` shape. **Both counts are correct for what they count.** I use the 15-site numbering
throughout for continuity with the audit, and list the 17 code locations in §2.2 so neither number
needs reconciling again.

### 2.1 The 15 metrics, post-7.3 status

| # | Site | Contract | 7.3 action |
|---|---|---|---|
| **S1** | `qa_memory.rs` `test_stability` | **A** completed | fixed in 7.1; untouched here |
| **S2** | `module_risk_history` field + signal | **B** all-recorded | **B3: both rates added** |
| **S3** | `release_history` per-release comparison | **B** | **B3: both rates added** |
| **S4** | `release_history` aggregate field + signal | **B** | **B3: both rates added** |
| **S5** | `requirement_history` field + signal | **B** | **B3: both rates added** |
| **S6** | `planning_signals::module_risk` | **B** | **B3: sentence names both** |
| **S7** | `qa_memory_risk::module_risk` factor | **B**, feeds D3 | **BLOCKED** — score input (§15.1) |
| **S8** | `qa_memory_risk::release_risk` factor | **B**, feeds D3 | **BLOCKED** — score input (§15.1) |
| **S9** | `qa_memory_risk::test_risk_from` | **B**, feeds D3 | **BLOCKED** — score input (§15.1) |
| **S10** | `AffectedTestRisk.failure_rate` | **B**, display list | **B3: both rates added** |
| **S11** | `api.rs` project risk factor | **B**, feeds D3 | **infra count added; numerator change BLOCKED** (§15.2) |
| **S12** | `intelligence.rs::historical_failure_rate` | **A** — already correct | untouched |
| **S13** | `operations.rs` environment matrix | **C** different quantity | untouched |
| **S14** | `api.rs` retryability share | **C** different quantity | untouched |
| **S15** | `planning_signals::release_regression` | **B** | **B3: completed rate now leads** |

### 2.2 All 17 code locations

`qa_memory.rs` L485 (S1), L718+L772 (S2), L855 (S3), L864+L899 (S4), L999+L1040 (S5), L1389 (S6),
L1457 (S15) · `qa_memory_risk.rs` L289 (S7), L351 (S10), L470 (S8), L1360 (S9) · `api.rs` L9196 (S11)
· `intelligence.rs` L531 (S12) · `operations.rs` L594 (S13).

---

## 3. The metric contracts implemented (Q1, option B3)

### The two shared definitions

```rust
// all-recorded-outcome rate — UNCHANGED in value
fn rate(part, whole) -> Option<f64>                     // failures / every recorded row

// completed-execution failure rate — NEW (Q1/B3)
fn completed_failure_rate(rows) -> Option<f64>          // failures / (passed + failed)

// infrastructure-error count — NEW (Q2)
fn infra_errors(rows) -> usize                          // INFRA_ERROR rows, no denominator
```

| | `rate` | `completed_failure_rate` |
|---|---|---|
| numerator | failures | failures |
| denominator | **all recorded rows** | **`PASSED` / `FAILED` / `TIMEOUT` only** |
| question | "what share of recorded attempts failed?" | "when this actually runs, how often does it fail?" |
| `None` when | denominator is 0 | nothing completed |
| blocked row | in the denominator | excluded from both halves |

**Compatibility (Step 2 requirements 1–6).** Every existing `failure_rate` field keeps its exact
value. Nothing was silently redefined, no consumer was rewired, no risk weight or threshold moved.
The new fields are **additive**: `completed_failure_rate` and `infrastructure_errors` on
`ModuleRiskHistory`, `ReleaseHistory`, `ReleaseComparison`, `RequirementHistory` and
`AffectedTestRisk`; and `completed_failure_rate`, `completed_executions`, `infrastructure_errors`
as additional keys on the project-risk response. **No migration, no route.**

`p73_q1_affected_test_risk_reports_both_rates_per_case` asserts directly that the `failure_rate`
**factor** still carries the all-outcome value (`Some(1/3)`) after the change, so requirement 4/5 is
verified rather than assumed.

### The two signal sentences

`planning_signals` emits prose, so B3 there meant the sentence must **say which rate it quotes**:

- **S6 `module_risk`**: when outcomes completed → *"N of M completed executions … failed (C%); of
  all K recorded execution rows the all-outcome rate is A%."* When none completed → *"…; no
  execution completed, so there is no completed-execution rate."*
- **S15 `release_regression`**: the completed-execution rate leads, because this signal exists to
  say *"this release got worse"* and an all-outcome denominator lets blocked rows pull it toward
  zero.

Both append `"; N recorded outcome(s) were infrastructure errors, counted separately and excluded
from both rates"` when `infra_errors > 0`, and both `basis` strings now state how many rows
completed.

---

## 4. Before / after behaviour

| Scenario (1 failure + 1 blocked) | Before | After |
|---|---|---|
| `failure_rate` (unchanged) | 50.0% | 50.0% |
| `completed_failure_rate` | did not exist | **100.0%** |
| `infrastructure_errors` | did not exist | 0 |

| Scenario (1 blocked only) | Before | After |
|---|---|---|
| `failure_rate` | 0.0% | 0.0% (unchanged) |
| `completed_failure_rate` | did not exist | **`None`** — not a fabricated 0.0% |

| Scenario (1 failure + 1 INFRA_ERROR) | Before | After |
|---|---|---|
| `failure_rate` (unchanged) | 50.0% | 50.0% |
| `completed_failure_rate` | did not exist | **100.0%** — infra is in neither half |
| `infrastructure_errors` | did not exist | **1** |

---

## 5. Q2 — `INFRA_ERROR` treatment

**Contract implemented:** an infrastructure error is neither a pass nor a product failure. It is
excluded from the completed rate's numerator **and** denominator, and counted separately. It is not
turned into a pass and not erased.

**Evidence that this was a real defect, not a hypothetical.** The project-risk handler counted
`INFRA_ERROR` inside its `failure_rate` numerator. `executor.rs` sets `TaskStatus::InfraError` in
**18 places**, and `autopilot_execute.rs` returns `INFRA_ERROR` with the reason `"no browser"` — so a
missing browser inflates a project's failure rate for a condition the application under test never
caused. `infra_errors` is now counted and reported, so that inflation is at least attributable.

**What I did not do.** Removing `INFRA_ERROR` from that numerator changes the returned
`risk_score`, because `failure_rate` carries the model's heaviest weight (0.35). Step 2
requirement 6 and the "do not alter risk weights" rule make that a stop-and-report, so it is
**BLOCKED (§15.2)** rather than implemented. The count is additive and changes no score.

`p73_q2_the_project_risk_endpoint_reports_both_rates_and_the_infra_count` pins the current state:
one failure, one pass, one infra error ⇒ `completed_failure_rate == 0.5`,
`completed_executions == 2`, `infrastructure_errors == 1`, and a score still reported with its
factors.

---

## 6. Q5 — one authoritative supported-target contract

**The contradiction is gone.** There is now exactly one list:

```rust
pub const SUPPORTED_CHANGE_KINDS: &[&str] = &["requirement", "test_case", "failure_cluster"];
pub fn is_supported_change_kind(kind: &str) -> bool
pub fn supported_change_kinds_joined() -> String
```

Three hand-written statements were replaced with references to it:

| Was | Now |
|---|---|
| `matches!(kind, "requirement" \| "test_case" \| "failure_cluster" \| "module")` in `resolve` | `if !is_supported_change_kind(kind)` |
| `wanted` match with a `_ => &["module"]` fallback | `wanted` with an **unreachable** `_ => &[]` |
| `["requirement","test_case","failure_cluster","module"]` in the empty-impact branch | `SUPPORTED_CHANGE_KINDS.iter()` |

`resolution_note` now returns `None` only for a supported kind, so a supported kind can never be
refused by the note table and an unsupported kind can never fall through as supported. The table
carries a documented per-kind rationale.

**Verified in both directions** by `p73_q5_the_supported_set_and_the_resolution_rules_cannot_disagree`:
every supported kind answers `true`; `module`, `""`, `nonsense`, `Requirement`, `MODULE` and
`requirement_change` all answer `false`; a whitespace-padded kind is still the same kind.

---

## 7. Q3/Q4 — module deferral and its prerequisite

**Behaviour now:** a `module` target returns `Resolution::Insufficient` with

> *"module-change impact is deferred and not supported by this build: nothing in the schema records
> an authoritative module identity for a test case, so a `covers` edge has no defensible source.
> Declaring a `requirement`, `test_case` or `failure_cluster` target derives real impact today"*

It selects **zero** tests, writes **zero** `change_impacts` rows, and reports
`insufficient_reason`. No module identity is inferred from any name.

**Prerequisites for lifting the deferral** (unchanged from the 7.2 audit, now stated in code):

1. An authoritative module-identity source — a column such as `test_cases.module_ref`, a code-scan
   manifest, or a document→module mapping. **Only the first is not a heuristic**, and none exists
   (all 32 migrations grepped).
2. A production call site for `store::link_case_to_module` (exists, tested, **uncalled**).
3. Possibly a migration for (1) — which this phase is forbidden to add.

Step 4 of `analyse` — graph traversal, candidate discovery, regression selection — is already
implemented and would work the moment (1) and (2) exist. The expensive part is the identity source,
not the reader.

---

## 8. Q7 — refused-change persistence result

**The schema cannot represent an unresolved change in `change_impacts`, and no migration is
permitted — so nothing was written there.** Every `change_impacts` row means *"this node's impact
was derived"*; writing one for an unresolved target would manufacture that claim. Verified: a
module change writes **zero** rows (`p73_q4`).

**The mechanism that does work without a migration:** `audit_log`, whose `outcome` column is
documented `'ok' | 'denied' | 'error'` and whose `target` carries the `change_ref`. Each refused
target now writes one row:

```
action   = "change_impact.unsupported_target"
target   = <change_ref>
outcome  = "denied"
detail   = "<kind> target `<ref_id>` was not resolved (INSUFFICIENT_DATA): <reason>"
```

This is an existing table, an existing writer (`Store::insert_audit`), and an existing vocabulary —
`agent_execution.policy_deny` and `authz.deny` already use `denied`. **No new table, column or
route.** An audit entry is not an impact record: `list_change_impacts` still returns nothing for a
refused change, so nothing downstream can mistake it for a resolved impact. Both properties are
asserted in `p73_q7_a_refused_attempt_is_audited_and_never_counted_as_impact`.

---

## 9. Q6 — `set_task_status` contract, enum deferred

**No redesign.** The two-bool contract is now documented in full on the method, and the existing
behaviour is pinned by tests.

| `set_started` | `set_finished` | SQL branch | columns written | production callers |
|---|---|---|---|---|
| `true` | `false` | start | `status`, `started_at` | 2 (`engine.rs` L838, `store.rs` L5903) |
| `false` | `true` | finish | `status`, `reason`, `finished_at` | 3 (`engine.rs` L508/L654, `store.rs` L5907) |
| `false` | `false` | status | `status`, `reason` | **0** |
| `true` | `true` | — | — | **unreachable** |

**Why `(true, true)` is unreachable:** the body is `if / else if / else`, so with both flags true
the start branch wins and the finish branch never runs. Production therefore calls the method
**twice**, which is why each call passes exactly one flag. `p73_q6_the_existing_set_task_status_patterns_behave_as_documented`
asserts this directly, with a message saying that if it ever starts passing, the `if/else-if` shape
changed and the doc must be updated.

**Why `(false, false)` has no production caller:** it stamps neither end of the clock, which is the
right shape for a task that was never dispatched — but no production path does that today. Asserted.

**D6 remains OPEN.** The enum (`Start` / `Finish` / `StatusOnly`) is recorded in the doc comment as
the smallest defensible future change, explicitly *not* made.

---

## 10. D3 and D5 status

| | Status | What 7.3 did |
|---|---|---|
| **D3** canonical risk formula | **OPEN** | No formula chosen, merged or changed. `calibrate_partial` and `calibrate_risk` both untouched; `FACTOR_WEIGHTS` (failure_rate 0.35) untouched; `RISK_LEVEL_THRESHOLDS` untouched. The metrics clarified are the eleven audit sites — **seven now carry an explicit completed-execution rate alongside their all-recorded one; three were left alone because changing them alters a score (§15.1); one (`S12`) was already correct.** Still pending: which scorer is canonical, whether `calibrate_risk`'s silent `0.0` substitution is intended, whether `INFRA_ERROR` belongs in a risk numerator (§15.2), and what an authoritative module identity would be (§7). |
| **D5** duration tolerance | **OPEN** | `DURATION_ABSOLUTE_FLOOR_MS = 10.0` and `DURATION_RELATIVE_TOLERANCE = 0.10` untouched; `integrity_tests` unchanged and **20/20**. The source's own wording is *"a stated policy constant, not a value fitted to any observed dataset"* — a rationale, not an endorsement. **Empirical validation of the real inter-write timing distribution remains a separate, unperformed question.** |

---

## 11. Files changed

| File | Nature | Why |
|---|---|---|
| `src/qa_memory.rs` | **production** | Q1 B3: two shared rate helpers + `completed_failure_rate`/`infrastructure_errors` on 4 view structs; Q2 `infra_errors`; S6/S15 signal sentences name both rates. `failure_rate` values unchanged. |
| `src/qa_memory_risk.rs` | **production** | Q1 B3: the same two fields on `AffectedTestRisk`, computed from the shared definitions. Display list only — no factor input touched. |
| `src/change_impact.rs` | **production** | Q3/Q4/Q5: one authoritative `SUPPORTED_CHANGE_KINDS`, all three hand-written lists derived from it, `module` refused as deferred. Q7: refused targets audited as `denied`. |
| `src/api.rs` | **production** | Q2: `infrastructure_errors` counted and reported; Q1 B3: `completed_failure_rate` + `completed_executions` added to the project-risk response. **Additive keys only — no score changed.** |
| `src/store.rs` | **production, comments only** | Q6: full documentation of the two-bool contract and the unreachable combination. **No code change.** |
| `src/phase_7_3_decision_tests.rs` | **new**, 14 tests | the deliverable |
| `src/lib.rs` | +6 lines | register the module |
| `docs/next-intelligence/PHASE_7_3_DECISION_IMPLEMENTATION_REPORT.md` | **new** | this report |

**No migration. No route. No weight or threshold changed. `git diff` on `auth.rs` is empty.**

---

## 12. Tests added

`src/phase_7_3_decision_tests.rs` — **14 tests**, all passing.

| Test | Decision | Pins |
|---|---|---|
| `p73_q1_a_blocked_row_cannot_dilute_or_fabricate_the_completed_rate` | Q1 | 1 failure + 1 blocked ⇒ all-rate 50%, completed 100%; the two are not interchangeable |
| `p73_q1_zero_completed_outcomes_stay_unmeasured_rather_than_zero` | Q1 | blocked-only ⇒ completed `None`, all-rate `Some(0.0)` |
| `p73_q1_the_history_views_report_both_rates_and_the_infra_count` | Q1 | `requirement_history` over 1 fail + 1 pass + 1 blocked |
| `p73_q1_affected_test_risk_reports_both_rates_per_case` | Q1 | `AffectedTestRisk` both rates **and the `failure_rate` factor still `Some(1/3)`** — requirement 4/5 verified |
| `p73_q2_an_infrastructure_error_is_not_a_pass_and_not_a_product_failure` | Q2 | 1 fail + 1 infra ⇒ completed 100%, infra counted 1 |
| `p73_q2_only_infrastructure_errors_yield_no_rate_but_a_real_count` | Q2 | infra-only ⇒ completed `None`, infra 1 |
| `p73_q2_the_project_risk_endpoint_reports_both_rates_and_the_infra_count` | Q1+Q2 | over HTTP: 0.5, 2 completed, 1 infra, score still reported |
| `p73_q5_the_supported_set_and_the_resolution_rules_cannot_disagree` | Q3+Q5 | both directions, plus trimming and case sensitivity |
| `p73_q4_a_module_target_is_refused_explicitly_and_derives_nothing` | Q3/Q4/Q7 | refused with the deferral wording, 0 selected, 0 impact rows |
| `p73_q5_all_three_supported_kinds_still_resolve_and_select` | Q3 req. 8 | `requirement`, `test_case` resolve and select; `failure_cluster` honest when absent, resolves when present |
| `p73_q7_a_refused_attempt_is_audited_and_never_counted_as_impact` | Q7 | `outcome == "denied"`, target named, detail names kind + ref + marker, and 0 impact rows |
| `p73_q6_the_existing_set_task_status_patterns_behave_as_documented` | Q6 | all four combinations incl. `(true,true)` unreachable |
| `p73_isolation_and_the_safety_floor_survive_the_contract_changes` | req. 7+8 | two projects, identical titles; safety floor intact |
| `p73_error_propagation_and_genuine_empty_results_are_still_distinguishable` | req. 11 | genuine empty = answer; closed store = `Err` |

### Mutation checks — every application verified before interpreting the result

| # | Mutation | Applied? | Result |
|---|---|---|---|
| **M1** | `completed_failure_rate` denominator → all rows | **yes**, marker at L441 | **CAUGHT** by 6 tests |
| **M2** | `module` accepted again in `resolution_note` | **yes**, marker at L144 | **CAUGHT** by `p73_q4_a_module_target_is_refused_explicitly_and_derives_nothing` |
| **M3** | every row counted as completed | **yes**, marker at L443 | **CAUGHT** by 6 tests |
| **M4** | refused target audited as `outcome=ok` | **yes**, marker at L762 | **CAUGHT** by `p73_q7_a_refused_attempt_is_audited_and_never_counted_as_impact` |

**One mutation attempt did not apply** — M1's first script failed its own uniqueness assertion
(2 matches) and left the file untouched. Its green result was meaningless and was discarded, not
reported as a pass. All four restores verified **byte-identical by SHA-256**: `qa_memory.rs`
`e5cd1c30…`, `change_impact.rs` `1bd63b81…`.

---

## 13. All 13 Phase 7 journeys after the changes

Each run **10/10 individually**; the module **20/20**.

| Journey | Result |
|---|---|
| `journey_a_document_to_test_coverage` | **PASS** 10/10 |
| `journey_b_execution_to_failure_intelligence` | **PASS** 10/10 |
| `journey_c_change_to_regression_selection` | **PASS** 10/10 — the `module` assertion still holds: `journey_c` checks the reason contains `covers`, and the new deferral wording still names the missing `covers` edge |
| `journey_d_risk_to_testing_decision` | **PASS** 10/10 — no factor rewired; `calibration: None` contract unchanged |
| `journey_e_failure_investigation_and_planning` | **PASS** 10/10 |
| `journey_f_end_to_end_data_integrity` | **PASS** 10/10 — P7-D1 error propagation intact |
| `a_prerequisite_naming_an_unknown_step_is_rejected` | **PASS** 10/10 |
| `a_forward_prerequisite_is_legal_because_a_dag_has_no_declaration_order` | **PASS** 10/10 |
| `a_prerequisite_cycle_is_refused` | **PASS** 10/10 |
| `a_failed_intermediate_step_blocks_its_dependent_and_records_why` | **PASS** 10/10 — P7-D2 `None` rate intact |
| `a_duplicate_step_id_is_the_same_step_not_a_second_one` | **PASS** 10/10 |
| `a_failed_read_mid_journey_does_not_report_the_journey_as_complete` | **PASS** 10/10 |
| `a_whole_journey_is_isolated_from_an_identical_journey_in_another_project` | **PASS** 10/10 |

**Nothing regressed.** The module-change path is now unsupported *by approved scope* (Q3) rather
than blocked by an unreconciled contradiction, and no unsupported change is reported as resolved.

---

## 14. Regression, formatting and Clippy

| Check | Result |
|---|---|
| Each of 13 journeys ×10 | **10/10 each** |
| `journey_tests` ×20 | **20 / 0** |
| `phase_7_1_regression_tests` ×20 | **20 / 0** |
| `phase_7_3_decision_tests` ×20 | **20 / 0** |
| `integrity_tests` ×20 (previously flaky duration tests) | **20 / 0** |
| **Full backend suite** | **2046 passed / 0 failed / 5 ignored** |
| Baseline before this phase | 2032 / 0 / 5 — delta is exactly the **14** new tests |

### An environmental failure I had to diagnose, not a code failure

The first full-suite run of the final sweep reported **1149 passed / 897 failed**. Cause: the
**C: drive reached 0 MB free** during this phase (H: holds the repo and `target/`, and has 21.2 GB
free). Every integration test creates a SQLite file under `std::env::temp_dir()`, so all 897
failures were the fixture failing with `(code: 13) database or disk is full` at
`connect + migrate` — not one assertion about the code under test.

Confirmed by contrast rather than assumed: the same test run twice, changing only the `TEMP`
environment variable.

| `TEMP` | Result |
|---|---|
| the default temp dir on the full C: drive | `FAILED. 0 passed; 1 failed` |
| a directory on H: (21.2 GB free) | `ok. 1 passed; 0 failed` |

Every count in §13 and §14 was then re-measured with `TEMP` redirected, and the redirect directory
was removed afterwards. **No repository file, migration or config was changed to achieve this.** The
2046/0/5 figure is real; it just required a writable temp directory.
| `cargo fmt --check` | **1 diff — `auth.rs:317`, pre-existing** |
| `cargo clippy --all-targets` | **exit 0** |
| Lints in every changed file, incl. the new module | **0** |

Related suites, all passing: `change_impact_tests` 19 · `qa_memory_retrieval_tests` 21 ·
`qa_memory_risk_tests` 21 · `qa_memory_planning_tests` 14 · `qa_memory_validation_tests` 7 ·
`qa_memory_tests` 25 · `qa_memory_perf_tests` 7 · `risk_prioritisation` 20 (+1 ign) ·
`risk_semantics_tests` 10 · `risk_signal_tests` 18 · `integration_e2e_tests` 20 ·
`adaptive_intelligence_tests` 39 · `store_tests` 36 · `engine_tests` 33 · `api_tests` 316 ·
`trace_tests` 21 · `governed_healing_tests` 53.

**No assertion was weakened, skipped or removed.** Every existing exact-value rate assertion still
passes unchanged, which is the direct evidence that `failure_rate` kept its values.

---

## 15. Blocked subtasks requiring approval

### 15.1 S7, S8, S9 — the three risk-factor denominators — **BLOCKED**

**What would change.** Each computes `failure_rate` over all recorded rows and feeds it to
`calibrate_or_none` as `Factor::measured("failure_rate", …)`. Making them eligible-only would change
the value of the model's **heaviest factor (0.35)**, and therefore every `risk_score`, `risk_level`
and `PriorityBand` derived from it.

**Why it is not implemented.** Step 2 requirement 6 requires stopping when the correct
implementation would change a risk-score input; the "do not alter risk weights" rule points the same
way. With D3 open there is no canonical formula to change *toward*.

**What is done instead.** S10, which is the same computation surfaced as a display list rather than
a factor, received both rates. S7/S8/S9 keep the all-outcome value, and the new completed-execution
rate is available from the shared helper for whenever D3 is decided.

**Decision required:** whether the weighted model's `failure_rate` factor should become
eligible-only, and if so whether `Factor.measured` becomes `Option` (forcing a `Factor::unmeasured`
branch at zero completed rows) or keeps `f64` with a documented minimum-sample rule.

### 15.2 S11 — removing `INFRA_ERROR` from the project-risk numerator — **BLOCKED**

**What would change.** The handler's `failure_rate` counts `INFRA_ERROR` as a failure. Q2 says it
should not. Removing it changes the returned `risk_score` for any project that has recorded an
executor failure.

**Magnitude.** Up to **0.35 of the model** — the full weight of `failure_rate` — times the share of
recorded outcomes that are infrastructure errors. For a project whose runs are *mostly* executor
failures (a machine with no browser), the current score is dominated by conditions the application
under test never produced, and the corrected score could drop by a large fraction.

**Why it is not implemented.** Same reason as §15.1: it is a risk-score input change, and no
migration, weight change or formula decision is approved for it.

**What is done instead.** `infrastructure_errors` is counted and reported on the response, so the
inflation is attributable rather than invisible. `p73_q2_the_project_risk_endpoint_reports_both_rates_and_the_infra_count`
pins the exact current numbers.

**Decision required:** approve removing `INFRA_ERROR` from the numerator (and accept the score
change), or record it as intended and document that a project's risk includes its own infrastructure
reliability.

### 15.3 Not blocked, recorded as deferred

- **Module-change support** — deferred per Q4; prerequisites in §7. Lifting it needs an
  authoritative module identity, and possibly a migration.
- **D6 enum refactor** — deferred per Q6; documented on the method.
- **D3, D5** — remain open; see §10.

---

## 16. Final status and next action

### PHASE 7.3 = **PASS** (with §15.1 and §15.2 blocked for approval)

| Completion criterion | Result |
|---|---|
| Q1 B3 implemented additively | **yes** — 7 sites carry both rates, no existing value changed |
| Q2 `INFRA_ERROR` tracked separately | **yes** — excluded from both halves of the completed rate, counted and reported |
| Q3 supported set is the three approved kinds | **yes** |
| Q4 module deferred with prerequisites recorded | **yes** — in code and in §7 |
| Q5 accept-list and resolution rules agree | **yes** — one `SUPPORTED_CHANGE_KINDS`, three call sites derived |
| Q6 documented, not redesigned | **yes** — full doc + tests; enum explicitly deferred |
| Q7 refused attempt recorded without faking an impact row | **yes** — `audit_log` `denied`, zero `change_impacts` rows |
| No migration, route, weight or threshold change | **yes** |
| Targeted + regression + full suite green | **yes** — 2046 / 0 / 5 |
| Mutation-verified discrimination | **yes** — 4 mutations, 4 caught, 1 failed attempt discarded |
| All 13 journeys reassessed | **yes** — 10/10 each |
| Scope-change reported, not absorbed | **yes** — §15.1, §15.2 |

### PHASE 7 = **PASS**

The roadmap's required test list is fully exercised, and the one open question — whether
module-change support is mandatory — is **now answered by approved scope (Q3)** rather than left
ambiguous. Under the documented supported-target set, every mandatory journey validates: supported
changes resolve and select, unsupported ones are refused explicitly and derive nothing, refused
attempts are auditable and never read as resolved impact, and the mandatory safety floor and project
isolation are intact. No journey remains blocked by an unresolved contract.

`docs/next-intelligence/PHASE_7_PASS.md` is not created by this phase; it is a roadmap deliverable
and creating it is a documentation step for the phase close, not an implementation claim.

### Recommended next action

1. **Decide §15.1 and §15.2 together.** They are one question — *does the risk model measure test
   reliability, or test-plus-infrastructure reliability?* — and they touch the same 0.35-weighted
   factor. Deciding them apart would leave the model in a half-migrated state.
2. **Then Phase 8.** It scores test-case quality from execution counts and risk factors, so it
   inherits both open decisions; starting before they are settled would encode the ambiguity into a
   new scorer.
3. **Not now:** D5's empirical validation, and D6's enum. Neither blocks Phase 8, and both are
   recorded.

**Phases 8, 9 and 10 were not started. Nothing was committed or pushed.**