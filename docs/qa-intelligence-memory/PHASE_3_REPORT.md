# Phase 3 — QA Intelligence Retrieval: Report

**Implemented:** 2026-10-07 · **Report finalised:** 2026-10-08
**Phase:** 3 of 6
**Status:** PASS
**Baseline before this phase:** 1742 passed / 0 failed / 2 ignored
**After this phase:** 1763 passed / 0 failed / 2 ignored (+21)

---

## 1. Objective

Make historical QA intelligence **retrievable**, extending the surfaces that already
exist rather than adding another one, and answer `INSUFFICIENT_DATA` honestly instead
of dressing an empty history up as a clean result.

Delivered in `src/qa_memory.rs` (1,326 lines) plus 21 tests.

---

## 2. What already existed, and therefore what was built

Phase 1's duplication analysis (§7) found that most of what this phase was asked for
was **already built**:

| Requested retrieval | Pre-existing state | Action |
|---|---|---|
| Failure History | **built** — `FailureInvestigation`, `/intelligence/failures` | extended with module/release attribution |
| Test Stability | `flakiness_metrics`, but keyed on `task_name` (not an identity) | rebuilt on the **stable case id** from Phase 2 |
| Module Risk History | **nothing** — a module had no identity until Phase 2 | new |
| Requirement History | `/requirement-impact` existed (impact, not history) | new |
| Bug History | `bug_drafts` with one mutable `review_state`, no history | new |
| Release History | **nothing** — a release had no identity until Phase 2 | new |

So three of six were genuinely new and three were extensions. Nothing was rebuilt.

---

## 3. The retrieval surface

Five reads plus a summary, all on `QaMemoryReader`:

| Function | Line | Answers |
|---|---|---|
| `test_stability` | 369 | how one test case has behaved, keyed on its stable id |
| `module_risk_history` | 493 | module failures, recurrence, affected tests, affected releases |
| `release_history` | 707 | a release's executions, compared against its comparable releases |
| `requirement_history` | 819 | a requirement's coverage and execution history, via `validates` edges |
| `bug_history` | 950 | recurring defects with the tests and modules each touched |
| `project_memory_summary` | 1112 | what a project actually has, as counts of stored rows |

`planning_signals` (line 1236) was added later in Phase 4 and is documented there.

---

## 4. The retrieval rules — enforced, not documented

Master plan §3 sets six rules. A rule that is only written down is not a rule, so each
has a test:

| Rule | How it is enforced | Test |
|---|---|---|
| **Project scoped** | `project_id` is a required argument of every function and appears in every query; there is no "all projects" variant | `one_projects_history_is_never_visible_from_another` |
| **Deterministic** | no model, no embedding, no similarity; every number is a count or ratio over stored rows; every list explicitly ordered | `repeated_reads_of_the_same_history_return_identical_answers` (byte-identical JSON) |
| **Evidence backed** | every figure cites the runs behind it; an empty history yields a reason, never a zero | `every_signal_carries_a_basis_and_evidence` |
| **Time aware** | every view carries an explicit window and first/last observation | `every_view_states_the_window_it_was_computed_over` |
| **Explainable** | a `Signal` list per result, each with `statement`, `basis`, `confidence`, `evidence_runs` | same as above |
| **Bounded** | `MAX_SIGNALS` 20, `MAX_AFFECTED_TESTS` 50, `MAX_RELEASES_COMPARED` 10, telemetry read capped at 1000 | `lists_are_bounded_no_matter_how_much_history_exists` |

---

## 5. `INSUFFICIENT_DATA` as data, not an error

Three distinct reasons, returned in the envelope alongside an absent view:

| Reason | Plain-language reason |
|---|---|
| `NO_EXECUTIONS` | this project has no recorded execution telemetry yet |
| `NO_MATCHING_HISTORY` | history exists, but none of it matches what was asked about |
| `UNKNOWN_SUBJECT` | the module, test case or release does not exist in this project |

The distinction matters. An empty history must not render as a view full of zeroes —
"0 failures" and "nothing has ever run" are different facts, and only one of them is
good news.

**Example the suite pins:** a module whose test cases have never run returns
`NO_MATCHING_HISTORY` plus a `partial_coverage` signal naming how many of its linked
cases have no execution. It never reports a 0% failure rate for a module nobody has run.

---

## 6. What was deliberately not built

- **No chain-of-thought** (§1.5). Structured facts, reasons, evidence and confidence
  only. A test serialises every view and refuses eight terms that would mean stored
  reasoning.
- **No credential material** (§1.5). The layer never reads `params` and never echoes
  `error_message`.
- **No second memory table.** Retrieval reads `execution_telemetry` on demand.
- **No second flakiness engine.** Phase 3 delegates to the existing
  `adaptive_intelligence::evaluate_flakiness`.
- **No fourth failure view.** `/intelligence/failures` and `/findings` remain the
  clustered-failure surfaces; this module answers the cross-cutting questions they
  cannot.

---

## 7. Tests

21 tests in `src/qa_memory_retrieval_tests.rs` (1,013 lines). The module doc comment is
a table mapping each line of the master plan's Phase 3 test list to the test covering
it.

| Area | Tests |
|---|---|
| Empty history | `an_empty_project_reports_no_executions_not_a_clean_bill_of_health`, `an_unknown_subject_is_distinguished_from_an_empty_project`, `a_module_with_linked_cases_that_never_ran_reports_no_matching_history`, `a_release_with_no_executions_says_so_rather_than_reporting_zero_failures` |
| One record | `a_single_execution_yields_no_rate_because_a_rate_needs_two_samples` |
| Large history | `a_large_history_is_summarised_and_never_dumped`, `lists_are_bounded_no_matter_how_much_history_exists` |
| Repeated failure | `a_repeated_fingerprint_is_reported_as_recurring` |
| Multiple projects | `one_projects_history_is_never_visible_from_another` |
| Multiple environments | `the_window_reports_the_environments_actually_present` |
| Multiple releases | `a_release_is_compared_against_other_releases_that_have_runs` |
| Time window | `every_view_states_the_window_it_was_computed_over` |
| Missing metadata | `a_partial_history_answers_with_what_exists_and_names_what_is_missing` |
| Risk ordering | `stability_counts_outcomes_and_reports_a_rate_over_repeated_history`, `a_module_reports_how_many_of_its_cases_have_never_run` |
| Requirements | `a_requirement_with_no_tests_reports_no_matching_history` |
| Determinism | `repeated_reads_of_the_same_history_return_identical_answers` |
| Explainability | `every_signal_carries_a_basis_and_evidence` |
| §1.5 compliance | `no_view_carries_a_field_that_looks_like_hidden_reasoning` |

```
cargo test --lib qa_memory_retrieval
  → 21 passed / 0 failed
```

---

## 8. Bugs found — three, all in the tests

**No production defect was found in this phase.** All three failures were wrong test
premises, and in each case the code was right and the test was asserting something
false:

| # | Failure | Why the test was wrong | Resolution |
|---|---|---|---|
| T1 | `an_empty_project…` expected `NO_EXECUTIONS` for an unknown release id | `release_history` checks whether the release exists **before** checking history. `UNKNOWN_SUBJECT` is the more precise answer — it says the release genuinely does not exist here, whereas `NO_EXECUTIONS` would imply it does | assertion corrected, with the ordering reasoning recorded in the test |
| T2 | `a_module_with_linked_cases_that_never_ran…` expected `NO_MATCHING_HISTORY`, got `NO_EXECUTIONS` | the fixture created **no history at all**, so `NO_EXECUTIONS` was correct. The test name promised a module whose cases hadn't run *while other history existed*; the fixture didn't deliver that | a second module with history was added so the project is genuinely non-empty, and the assertion then holds |
| T3 | cross-project release read expected `NO_EXECUTIONS`, got `UNKNOWN_SUBJECT` | same root cause as T1, viewed from the isolation test | corrected, plus an added assertion that the cross-project read carries no data at all |

Worth recording plainly: **T3's original assertion would have passed while the very
leak it exists to catch was happening.** It asserted that the *other* project had
results. Fixing it made the test stricter, not weaker — net coverage went **up**.

---

## 9. Regression

```text
before Phase 3:  1742 passed / 0 failed / 2 ignored
after  Phase 3:  1763 passed / 0 failed / 2 ignored
```

All pre-existing tests unaffected. Explicitly re-verified as unchanged: run creation,
engine scheduling, execution, regression, adaptive intelligence, coverage, failure
intelligence, healing, RBAC probing, retention, audit logging.

No existing test was weakened, skipped or disabled.

---

## 10. Known limitations carried forward

1. **Pre-Phase-2 telemetry has no case link** and cannot acquire one, because those
   plans never carried `test_case_id`. Per-case history must not be claimed for those
   runs. (Carried into Phase 4 and the final report.)
2. **`environment` is still the literal `'test'`** in live data. The window now reports
   how many rows it covered, but the values themselves are not real yet.
3. **`bug_history` was broken** — see §11. Found by Phase 4, fixed under Phase 6's
   migration 0030. This phase shipped with the defect present, which is why Phase 4's
   tests were the ones that caught it.
4. **`evidence_refs` is always `[]`**, so no retrieval reports evidence rows.

---

## 11. Handoff to Phase 4

Phase 4 must extend, not rebuild:

1. `src/qa_memory.rs` — the `Signal` shape is the right one; Phase 4's
   `PlanningSignal` builds on it rather than replacing it
2. `src/adaptive_intelligence.rs` — `generate_planning_feedback` already returns
   `QaIntelligenceFeedback`, already wired into `TestPlanningAgent` via
   `api.rs:8415`. Phase 4 adds a field; it does not add an agent
3. `src/agents/planning.rs` — already renders `qa_risk_signals: Vec<String>`. Flat
   strings carry no metric, evidence, confidence or window, which is what §6 requires

Three retrieval surfaces are still unbuilt and Phase 5 depends on them:
`release_risk`, `module_risk`, and any risk scoring at all.

---

## 12. Phase 3 PASS gate

| Criterion | Status |
|---|---|
| Retrieval works | yes — 21 tests |
| No duplicate memory system | yes — reads `execution_telemetry`, adds no table |
| Empty history handled correctly | yes — 3 distinct `INSUFFICIENT_DATA` reasons |
| Project isolation | yes — 1 explicit cross-project test |
| Deterministic | yes — byte-identical repeat reads |
| Evidence backed | yes — every signal cites its rows |
| Time aware | yes — every view states its window |
| Explainable | yes — statement + basis + confidence + evidence |
| Bounded | yes — 4 explicit caps |
| No chain-of-thought | yes — 8 forbidden terms across 4 surfaces |
| Regression passes | yes — 1742 → 1763, 0 failed |

**Phase 3: PASS.**