# Phase 8 — PASS

**Phase 8 of `AIQA_NEXT_ROADMAP_PHASED_PLAN.md`: AI Test Case Quality Intelligence.**

> **Roadmap objective (L252):** "Measure whether generated and saved test cases are useful,
> traceable, unambiguous, non-duplicative, and executable."
>
> **Roadmap constraint (L256):** "Only score dimensions that can be measured honestly."
>
> **Roadmap PASS gate (L292):** "Quality scoring is explainable, reproducible where deterministic,
> honest about missing data, project-scoped, and does not silently change approval/execution
> behavior."
>
> **Roadmap deliverables (L296–297):** `PHASE_8_TEST_QUALITY_REPORT.md`, `PHASE_8_PASS.md`.

Predecessor: [`PHASE_7_PASS.md`](./PHASE_7_PASS.md) — Phases 1–7 not restarted or rewritten.

This file is the second roadmap deliverable. It records **no new implementation**. Every claim below
is quoted or derived from
[`PHASE_8_TEST_CASE_QUALITY_REPORT.md`](./PHASE_8_TEST_CASE_QUALITY_REPORT.md) and its predecessor
[`PRE_PHASE_8_RISK_SEMANTICS_REPORT.md`](./PRE_PHASE_8_RISK_SEMANTICS_REPORT.md), both of which
predate it.

**Naming note.** The roadmap names the report `PHASE_8_TEST_QUALITY_REPORT.md` (L296). The repository
contains `PHASE_8_TEST_CASE_QUALITY_REPORT.md`. The report itself flags this at its §17 and L16:
"One roadmap-named deliverable is **not** produced, and that is not a scope reduction." The content
required by the roadmap is present under a slightly different filename; the discrepancy is recorded
here rather than papered over, and no file was renamed as part of this documentation step.

---

## Result

**PASS.**

| Criterion (roadmap gate) | Status | Where proven |
|---|---|---|
| explainable | PASS | every dimension explains itself; rule 3 satisfied — §4, §5, `p8_contract_every_dimension_explains_itself` |
| reproducible where deterministic | PASS | §6; mutation-verified across 4 defects |
| honest about missing data | PASS | `InsufficientData` is a first-class verdict, not a fallback — §4 |
| project-scoped | PASS | mutation 3 (tenant wildcard) caught by 5 tests; two-projects tests |
| does not silently change approval/execution behaviour | PASS | 0 routes, 0 migrations, no lifecycle write, no status transition — §7 |
| all eight dimensions implemented or reported unanswerable | PASS | §2, §5 — each unanswerable dimension named with its reason |
| full suite green | PASS | 2085 / 0 / 5, six consecutive runs |

The report's own verdict (L9–L14):

> "All eight roadmap dimensions are implemented or explicitly reported unanswerable, with the reason
> named. No mandatory capability is blocked. The PASS gate is met…"

---

## Objectives and scope as executed

The report re-read the roadmap section before any design work (§2: "Read from
`AIQA_NEXT_ROADMAP_PHASED_PLAN.md` L248–297 before any design work") and recorded eight dimensions
and eight scoring rules.

### The central design decision — no aggregate score

The roadmap's worked example is `Test Quality = 91/100`, but it defines **no weights and no
aggregation formula**. The report therefore did not produce a number:

> "Producing a single 0–100 number would mean inventing weights with no authority behind them, which
> is exactly what the brief forbids and what rule 3 … cannot satisfy for a made-up weighted mean. So
> `TestCaseQuality` has **no aggregate field**, and says so on the object itself." — §4

`overall()` returns a **status**, not a number, and is deliberately conservative:

```
any dimension Weak                → Weak
every dimension Adequate          → Adequate
anything unmeasured or unassessed → InsufficientData
```

> "Nothing was found wrong" is not "everything was checked and is fine".

`p8_contract_there_is_no_aggregate_quality_score` asserts the serialised form contains no
`score`/`grade`/`rating`/`quality_percentage` key under any name — the absence is a pinned contract,
not an accident.

This is a documented, defended departure from the roadmap's illustrative example, taken under the
roadmap's own "only score what can be measured honestly" constraint. It is recorded here as a
decision a reviewer should be aware of, not smoothed over.

### Four verdicts, two of which are about *not knowing*

`Adequate` (measured, meets the rule), `Weak` (measured, fails it), `Unassessed` (cannot be judged on
the evidence available), `InsufficientData` (the source exists; this case has not produced enough of
it). Missing criticality is never treated as low criticality — rule 5, asserted.

---

## Verified test-quality results

### Targeted — `phase_8_quality_tests`: 15 passed / 0 failed

| scenario | test |
|---|---|
| complete case with linked requirements | `p8_quality_a_complete_case_is_measured_not_assumed` |
| missing steps / expected results | `p8_quality_an_incomplete_case_is_weak_not_unmeasured` |
| insufficient execution history | `p8_quality_no_execution_history_is_unmeasured_not_perfect_and_not_defective` |
| genuine repeated failures | `p8_infra_genuine_repeated_failures_are_measured` |
| repeated infrastructure errors | `p8_infra_repeated_infrastructure_errors_are_not_a_product_defect` |
| only blocked executions | `p8_infra_blocked_only_executions_are_unmeasured` |
| duplicate / near-duplicate cases | `p8_dup_near_duplicate_cases_are_flagged_and_distinct_ones_are_not` |
| a case with no embedding row | `p8_dup_a_case_with_no_embedding_is_unmeasured` |
| two projects, identical case text | `p8_scope_two_projects_with_identical_case_text_do_not_share_evidence` |
| project sweep containment | `p8_scope_assess_project_returns_only_that_projects_cases` |
| scores explain themselves / deterministic | `p8_contract_every_dimension_explains_itself` |
| no aggregate score exists | `p8_contract_there_is_no_aggregate_quality_score` |
| storage-read failure not disguised | `p8_contract_a_storage_read_failure_is_not_disguised_as_an_empty_assessment` |
| Phase 7.4 contracts preserved | `p8_contract_the_phase_7_4_metric_contracts_are_preserved` |
| D3 independence | `p8_contract_phase8_does_not_depend_on_on_either_severity_implementation` |

**Honest note on roadmap coverage.** The report labels this table against *Step 7 scenarios / Step 8
negatives* of the task prompt — **not** against the roadmap's separate 10-item "Required tests" list
(roadmap L279–288), and it contains no explicit roadmap-test-to-test mapping. Comparing the two
lists, two roadmap items have **no dedicated test** in this table:

* *"unavailable executor"* — consistent with §15's statement that `automation_feasibility` is
  establishment-level (it says executors exist, not that one fits this case).
* *"AI heuristic result marked as such"* — consistent with §15: no AI heuristic is implemented;
  negative/boundary coverage is `NotAssessed` and would need one.

Neither gap is claimed as satisfied by this summary. Both are already recorded as limitations in the
report's §15.

### Affected existing suites — all 0 failed

`pre_phase_8_tests` 13 · `phase_7_4` 11 · `phase_7_3` 14 · `phase_7_1` 15 · `qa_memory_risk_tests` 21 ·
`qa_memory_retrieval_tests` 21 · `integration_e2e_tests` 20 · `execution_plan` 48 · `ai_generation` 90
· `api_tests` 316 · `risk_signal_tests` 18 · `risk_prioritisation_tests` 20 ·
`qa_memory_planning_tests` 14.

### 20-run integrity suite

20 / 20 clean; per run **34 passed / 0 failed / 1 ignored**; **0** failures.

### Full backend suite

| check | result |
|---|---|
| full suite, 3 consecutive runs | **2085 passed / 0 failed / 5 ignored** each |
| final 3 runs | 2085 / 0 / 5 each |
| `cargo fmt --check` | 1 diff — `auth.rs:317`, pre-existing, **not fixed** |
| `cargo clippy --all-targets` | exit 0, 0 errors, 66 warnings |

Baseline 2070; `2070 + 15 = 2085` — exactly the 15 new tests. No existing test was removed, skipped
or weakened. Per-file lints: `test_quality.rs` 0, `phase_8_quality_tests.rs` 0, `execution_plan.rs` 0,
`lib.rs` 0. `ai_generation.rs` has 2, both pre-existing.

---

## Mutation evidence — and the test gap it found

Four defects were re-introduced, each **verified applied** before its result was interpreted, then
restored and confirmed byte-identical (§11):

| # | mutation | caught by |
|---|---|---|
| 1 | unmeasured evidence reported as `Adequate` | **5 tests** |
| 2 | a locally re-derived rate counting `INFRA_ERROR` as a product failure | **2 tests** |
| 3 | telemetry tenant scope replaced with a wildcard | **5 tests** |
| 4 | a telemetry read failure reported as a clean `Adequate` | **1 test** — *only after the test was fixed* |

**Mutation 4 was initially NOT caught, and that was a real defect in the test.** The test loaded the
case row *after* closing the pool, so `list_test_cases` failed, the row was `None`, and the function
returned early — never reaching the failure path it claimed to test. It passed against a mutated
implementation. The test was rewritten to load the row first; the reason is documented in the test's
own doc comment.

Restoration: `test_quality.rs` restored from a SHA-verified backup — expected
`01de8660422c196a`, actual `01de8660422c196a`, **byte-identical**; zero `MUTATION` markers remain.

The report's §16 notes that PASS does not rest on green unit tests alone: "§11 records four
deliberate mutations and the one real test defect they exposed."

---

## Design boundaries respected

* **0 migrations, 0 routes.** No risk weight, threshold or duration constant changed.
* **Not touched:** `integrity.rs` (the 10 ms floor), `store.rs` (`set_task_status`), `domain.rs`,
  `engine.rs`, `executor.rs`, `intelligence.rs`, `adaptive_intelligence.rs`, `api.rs`,
  `qa_memory.rs`, `qa_memory_risk.rs` — all confirmed by §7.
* Exactly **5 files changed**: `test_quality.rs` (new, ~700 lines),
  `phase_8_quality_tests.rs` (new, 15 tests), `lib.rs` (two module lines), and two one-word
  visibility widenings in `execution_plan.rs` (`fn` → `pub(crate) fn`) and `ai_generation.rs`
  (`cosine_similarity`) for **reuse, no behaviour change** — verified by diff.
* **D3 independence was designed, not lucky.** `test_quality.rs` contains no reference to
  `failure_severity` or `category_weight`. The report is explicit that this "was not luck; it is the
  design": quality is a property of the case *definition*, while failure severity is a property of an
  execution *outcome*. Importing it would have made D3 a blocker for no benefit.
  `p8_contract_phase8_does_not_depend_on_on_either_severity_implementation` asserts the serialised
  assessment contains neither string.

---

## Documented limitations and open decisions

From the report's §15 and §17 — recorded, not resolved:

1. **D3 — OPEN, unchanged.** Two `failure_severity` implementations share one factor name, weight
   0.15. Phase 8 consumes neither, so it is unaffected — but it must be answered before anything
   reads that factor as a single quantity.
2. **D5, D6 — OPEN, untouched.**
3. **`one clear condition per case` is not separately scored** — folded into the two dimensions that
   can be measured. Honest gap, not a silent omission.
4. **Negative/boundary coverage is `NotAssessed`** — needs either a new source column or an
   explicitly-labelled heuristic. Both are decisions, not implementation work.
5. **`automation_feasibility` is establishment-level** — it says executors exist, not that one fits
   this case. A per-case mapping needs a new authoritative source.
6. **Duplicate coverage reaches only APPROVED cases** — `test_case_embeddings` has no row before
   approval, so a project of mostly-GENERATED cases cannot be assessed for duplication at all.
7. **No API route exposes the assessment.** The contract is implemented and verified; exposing it
   needs a route, which this phase's rules forbade. A consumer-facing endpoint
   (`GET /intelligence/tests/{id}/quality`) is the natural next step and needs authorisation.
8. **Filename divergence from the roadmap**, as recorded in the header.

---

## Discipline

* No existing test removed, skipped or weakened. `2070 + 15 = 2085` is stated as an arithmetic check.
* Zero migrations, zero routes, no approval or execution behaviour changed.
* A dimension that could not be honestly measured is reported as `Unassessed` or `InsufficientData`
  rather than scored.
* The roadmap's illustrative `91/100` aggregate was **declined on stated authority**, with the
  reasoning recorded in the report, rather than quietly produced or quietly ignored.
* A test that passed against a mutated implementation was found, fixed, and the reason documented in
  the test itself.
* A process error — overwriting a backup before restoring from it, letting one mutation's logic
  survive into the next measurement — was caught by re-reading the source rather than trusting the
  output, and recorded (§11).
* The `auth.rs:317` formatting diff was left untouched.

---

## Repository state

Nothing committed. Nothing pushed. HEAD `7930507`, 0 ahead / 0 behind `origin/main` at the time the
report was written.

Phases 1–7 not restarted or rewritten. Phase 7's own report is kept verbatim; see
[`PHASE_7_PASS.md`](./PHASE_7_PASS.md) for its closing gate.

**Phase 9 has since been started; Phase 9.2 remains open. Phase 10 has not been started.**
