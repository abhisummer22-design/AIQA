# Phase 4 — AI Risk-Based Testing + Test Prioritization

**Phase 4 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`. Backend only. PASS.**

Predecessors: [`PHASE_3_CHANGE_IMPACT_REPORT.md`](./PHASE_3_CHANGE_IMPACT_REPORT.md),
[`PHASE_3_PASS.md`](./PHASE_3_PASS.md). Phase 1–3 functionality was not restarted or
rewritten.

---

## 1. Audit: what risk intelligence already exists

The decisive finding is that **a risk engine already exists, is well designed, and was
being ignored for test cases.**

| Existing component | What it does | Reused? |
|---|---|---|
| `adaptive_intelligence::calibrate_risk` | 6-factor weighted score → `risk_score`, `risk_level`, prose `explanation` | **yes, called unchanged** |
| `qa_memory_risk::module_risk` | factors per module → `Option<RiskCalibration>`, `Factor[]`, `measured_weight` | **yes, read, never modified** |
| `qa_memory_risk::release_risk` / `release_risk_overview` | same per release, plus trend | **yes, read, never modified** |
| `Factor` / `calibrate_or_none` / `failure_severity` / `average_flakiness` / `per_case_counts` | the measurement machinery | **yes, all reused** |
| `qa_memory::test_stability` | per-case executions, failure rate, flakiness | superseded by the new per-case surface, which builds on the same primitives |
| `adaptive_intelligence::record_failure` / `upsert_failure_cluster` | fingerprinting, clustering, bug drafts | **yes** |
| `change_impact` (Phase 3) | `change_impacts` records | **yes** — `Changed` band |
| `regression_runner::Scope::restricted_to` (Phase 3) | narrowing a run, floor enforcement | **yes** — the safety guarantee |
| `intelligence::update_test_intelligence` | writes `risk_assessments.score` | read only; see §1.1 |

**`qa_memory_risk.rs` had module and release scopes and no test scope.** That is the gap
Phase 4 fills — by adding a third scope to the module that already owns risk, not by
creating anything.

### 1.1 A second, disagreeing risk formula exists — recorded, not merged

`intelligence.rs:557` computes a **different** five-factor formula inline:

| Formula | Weights | Used by |
|---|---|---|
| `calibrate_risk` | 35% failure_rate, 20% flakiness, 15% **failure_severity**, 10% dependency, 10% **healing_regression**, 10% **security** | `qa_memory_risk`, `api.rs:9008` |
| inline in `update_test_intelligence` | 40% failure_rate, 20% flakiness, 15% dependency, 15% **execution_cost**, 10% security | writes `risk_assessments.score`, which `GET /intelligence/tests` and Phase 3's `risk_scores()` read |

So `risk_assessments.score` is **not** `calibrate_risk`'s number. Phase 4 does **not** read
that column and does **not** merge the two formulas — that would be a behavioural change to
an existing feature outside this phase's scope. Recorded as limitation L4, because it is a
real inconsistency a reader of `/intelligence/tests` should know about.

---

## 2. Signals used, and signals unavailable

### 2.1 Used — all measured from stored, project-scoped rows

| Signal | Source | Weight in the existing model |
|---|---|---|
| `failure_rate` | `execution_telemetry` by `test_case_id` | 0.35 |
| `flakiness_score` | `evaluate_flakiness` over the case's own outcomes | 0.20 |
| `failure_severity` | failure categories of this case's failures, weighted by share | 0.15 |
| `healing_regression_rate` | `healing_proposals` on this case's fingerprint → `verification_result` | 0.10 |
| recurring defects | `bug_drafts` → cluster → fingerprint → this case's rows | reported, not scored |
| recent change | `change_impacts` (Phase 3), matched on the named `change_ref` | `Changed` band |

### 2.2 Unavailable — reported as unavailable, never guessed

| Signal | Why it cannot be measured |
|---|---|
| `dependency_count` | **no case-to-case dependency graph.** `TaskDef.prerequisites` is per *run*, not per case |
| `security_sensitivity` | **no column and no table anywhere** — already recorded in `qa_memory_risk`'s own header for modules and releases |
| requirement criticality | **`requirements` has no criticality column.** Fields are `req_key`, `ordinal`, `section`, `kind`, `text`. `kind` is a category (functional/security/…), not an importance, and folding it into a score would be a judgement the data does not make. It is therefore **not** used at all |
| module risk | no `covers` edge is ever written (Phase 2 L1), so no test has a module |
| coverage gaps | **coverage is not persisted** — no coverage table exists; `coverage.rs` recomputes per request |
| release trends | exists per *release* (`release_risk_overview`) but has no per-test dimension |

### 2.3 The consequence: **no risk score is produced**

`calibrate_or_none` refuses to emit a `RiskCalibration` unless **every** factor measures,
because a partial weighted sum silently claims the missing weight was zero. With
`dependency_count` and `security_sensitivity` unavailable, `calibration` is `None` for
every test case — today and structurally.

**Phase 4 does not invent a score to fill the gap.** It reports `weighted_contribution`,
`measured_weight`, `unmeasured_weight` and every `Factor` with its basis and evidence
count. `no_risk_score_is_invented_for_the_factors_that_have_no_column` asserts exactly this,
and asserts that `measured_weight + unmeasured_weight == 1.0`.

---

## 3. Risk output shape

`TestRisk`: `calibration` (always `None`, honestly), `factors[]`, `partial`,
`measured_weight` / `unmeasured_weight` / `weighted_contribution`, `measured_ratio`
(with the neutral-fill caveat), per-case counts, `recurring_defects`, healing counts,
`window`, and `signals[]` each carrying `kind`, `statement`, `evidence_runs`, `confidence`,
`basis`.

`PrioritisedTest`: `priority`, **`applies[]`** (every band that holds), `reasons[]` (one
line per condition), the full `risk`, and `change_ref`.

`TestPrioritisation`: `tests[]`, `band_counts`, **`mandatory_floor`** (reported separately
so a reader can audit what will not be dropped), `window`, `signals`.

### 3.1 "Why is this test high priority?" — answered with facts

Every condition that applies is listed with its own line, so the band is reconstructible:

```
priority : critical_risk
applies  : [mandatory, critical_risk]
reasons  : ["3 recorded failure(s); on the safety floor",
            "measured share of the risk model is 0.71 (>= 0.70)",
            "the measured share of the existing risk model is at or above 0.70"]
risk     : { measured_weight: 0.70, unmeasured_weight: 0.30,
             calibration: null, factors: [ …each with basis and evidence… ],
             measured_ratio: 0.71 }
```

### 3.2 Prioritization is an order, not a blend

A band from a **documented total order** over named conditions — not a new weighted
formula that would become a *third* risk model beside `calibrate_risk` and the inline one:

| Band | Rank | Enters by |
|---|---|---|
| `mandatory` | 0 | has recorded failure evidence (the safety floor) |
| `changed` | 1 | inside a declared change's `change_impacts` set |
| `critical_risk` | 2 | `measured_ratio >= 0.70` |
| `high_risk` | 3 | `measured_ratio >= 0.50` |
| `recurring_defect` | 4 | a bug draft against this case's fingerprint |
| `flaky` | 5 | `flakiness_score >= FLAKY_THRESHOLD` (0.34, stated) |
| `routine` | 6 | nothing applies — **a position, not an exclusion** |

Ties break on case id, so the order is total and reproducible.

---

## 4. Safety: mandatory tests are never removed

Three separate guarantees, each tested:

1. **Prioritisation orders; it never excludes.** Every case with recorded executions is
   returned (`prioritisation_lists_every_case_and_excludes_nothing`).
2. **The floor is reported**, not merely respected — `mandatory_floor` is in the response so
   a caller can audit it (`the_safety_floor_is_reported_and_cannot_be_suppressed`).
3. **Enforcement stays where it belongs.** Narrowing an actual run is Phase 3's
   `Scope::restricted_to`, whose `mandatory` set is *added to* the selection and never
   intersected with it. `narrowing_a_regression_scope_to_a_selection_still_keeps_the_floor`
   drives that end to end with the worst possible selection.

`phase_four_and_phase_three_agree_on_the_safety_floor` asserts the two features derive the
**same set** — two components that both claim to protect mandatory coverage must not
disagree about which cases that is.

The floor is **derived** ("has recorded failure evidence"), not declared: there is **no
mandatory-test flag anywhere in the schema** (Phase 3 verified this; Phase 4 re-checked).
That is stated as a derivation, not presented as policy. See L3.

---

## 5. Files changed

| File | Change |
|---|---|
| `qa_memory_risk.rs` | **+734 / −2** — `TestRisk`, `TestEvidence`, `PriorityBand`, `PrioritisedTest`, `TestPrioritisation`, `FLAKY_THRESHOLD`, `test_risk`, `test_evidence`, `test_risk_from`, `prioritise_tests`. The 2 deletions are the two lines in `prioritise_tests` the shared read replaced |
| `risk_prioritisation_tests.rs` | **new** — 19 tests (18 + 1 `#[ignore]`d measurement) |
| `api.rs` | one route + `get_test_risk_scoped` + `TestRiskQuery` |
| `lib.rs` | module registration |

**No existing risk function was modified.** `calibrate_risk`, `Factor`,
`calibrate_or_none`, `failure_severity`, `average_flakiness`, `module_risk` and
`release_risk` are byte-identical to HEAD.

### 5.1 Migrations: **none**

Phase 4 reads only. No table, no column, no index. Nothing to migrate, and the Phase 1–3
migration-count guards (pinned at 31) did not fire — which is the correct outcome and worth
noting, since they fired for Phase 3's migration.

### 5.2 APIs

One route, shaped like the Phase 1 trace endpoint:

```
GET /api/v1/projects/:project_id/test-risk[?change_ref=&test_case_id=]
```

* omit both → the project's whole order, with `mandatory_floor` and `band_counts`
* `test_case_id` → that case's factors and reasons
* `change_ref` → order relative to one declared change

Guarded by `require_scoped_project`; every read inside is project-scoped.

---

## 6. Tests

**18 tests + 1 `#[ignore]`d measurement.** Over half are negative.

| Scenario | Test |
|---|---|
| no score invented | `no_risk_score_is_invented_for_the_factors_that_have_no_column` |
| empty history → not fake risk | `an_empty_project_reports_insufficient_data_not_low_risk` |
| missing history for one case | `a_case_with_no_executions_is_not_low_risk` |
| malformed id | `a_malformed_case_id_is_unknown_subject` |
| ordering + stated reason | `a_failing_case_outranks_a_clean_one_with_a_stated_reason` |
| nothing excluded | `prioritisation_lists_every_case_and_excludes_nothing` |
| flaky | `an_alternating_case_is_reported_flaky` |
| recurring bug | `a_recurring_failure_is_reported_as_a_recurring_defect` |
| **high-coverage stable area** | `a_stable_passing_case_is_not_reported_risky` |
| missing requirement criticality | `no_risk_score_is_invented_…` + §2.2 |
| missing module information | same — `dependency_count` reported unmeasured, no module claim anywhere |
| changed high-risk area | `a_named_change_promotes_its_cases_without_demoting_others` |
| multiple projects | `an_empty_neighbour_project_has_no_floor_and_no_risk` |
| **same fingerprint across projects** | `the_same_fingerprint_in_two_projects_does_not_bleed_risk` |
| conflicting risk signals | `conflicting_signals_rank_by_evidence_rather_than_by_alarm` |
| mandatory protection | 3 tests (§4) |
| shape / arithmetic | `the_risk_shape_states_its_own_data_sufficiency` |
| healing regression measurable | `a_healing_regression_makes_its_factor_measurable` |

Every row is produced by a real writer — `insert_test_case`,
`Intelligence::ingest_run`, `insert_healing_proposal`, or Phase 3's real
`ChangeImpactReader::analyse`. No test hand-writes a fingerprint or a risk score.

### 6.1 Bugs found and fixed

1. **A performance defect I introduced, caught by the audit's own instruction.** The first
   implementation called `test_risk` per case, and `telemetry_for` is a **fresh bounded
   read per call** — so prioritising 200 cases did 200 telemetry scans, 200 proposal
   reads, 200 draft reads and a cluster lookup per draft. Fixed by `TestEvidence`: one
   read of each table, every case answered from it. §7.
2. **A regression I introduced in that refactor.** Extracting `test_risk_from` collapsed the
   `UnknownSubject` and `NoMatchingHistory` cases into one, and
   `a_malformed_case_id_is_unknown_subject` caught it. Restored: the UUID is parsed before
   the evidence read, so a malformed id is a caller error rather than "nothing found".
3. **A dead guard in the test suite.** `Retrieved` has exactly three fields, so every
   `let Retrieved { view: Some(x), .. } = … else { panic!() }` was **unreachable** — clippy
   was right and the guard did nothing. Replaced with two helpers, `present` / `absent`.
4. **A regex edit that silently deleted call arguments** in the test module, caught by the
   compiler. The file was rewritten cleanly rather than patched again.

---

## 7. Performance

Measured, not asserted — reproduce with:

```
cargo test phase4_measures_prioritisation_cost -- --ignored --nocapture
```

Dataset: **200 test cases × 5 executions = 1,000 telemetry rows**, built through the real
`ingest_run` path.

| Measurement | Result |
|---|---|
| One whole-project prioritisation | **62.6 ms** |
| Per case | 0.31 ms |
| Per telemetry row | 0.063 ms |
| **200 single-case `test_risk` calls (the replaced implementation)** | **5.66 s** |
| **Speedup from the shared read** | **90.4×** |

### 7.1 On the repeated-scan question

`telemetry_for` is **not** cached — it is a fresh bounded read per call, which is Phase 6's
deliberate stance ("one bounded read per question rather than a pile of joins"). Phase 4's
bug was calling that per case. The fix is not a cache (which would make two questions about
different data disagree) but **one read per question**, shared across the cases inside it.
That keeps every case's factors derived from the same row set, so a trace cannot show two
inconsistent views of the same history.

No timing threshold is pinned in CI. The measurement test is `#[ignore]`d precisely because
a wall-clock assertion chosen on one machine fails on another; the numbers live in this
report and the test exists so they can be reproduced.

---

## 8. Regression

| Run | Before Phase 4 | After |
|---|---|---|
| Backend suite | 1862 passed / 0 failed / 2 ignored | **1880 passed / 0 failed / 3 ignored** (+18; the 3rd ignore is the measurement test) |
| `cargo build` | clean | clean |
| `cargo clippy --all-targets` | 0 errors | 0 errors, no warning in any Phase 4 file |
| `rustfmt --check` | — | clean on both Phase 4 files |
| Frontend | 1605 / 79 files | unchanged — no frontend file touched |

Zero regressions. No pre-existing test weakened, skipped or deleted. Phase 1–3 suites
(`trace_tests` 21, `trace_chain_tests` 14, `change_impact_tests` 16) all still pass.

---

## 9. Limitations

**L1 — `covers` / module-targeted risk unavailable.** No `covers` edge is ever written, so
no test has a module and module risk has nothing to join to. Inherited from Phase 2.

**L2 — requirement criticality is not used at all.** `requirements` stores `kind`
(functional | security | performance | …) but no importance. Treating `kind` as a proxy for
importance would be a judgement the data does not make, so it is excluded entirely rather
than approximated.

**L3 — the safety floor is derived, not declared.** No mandatory-test flag exists in the
schema and `test_cases.tags` has no writer convention. The floor is "has recorded failure
evidence" — the strongest *derivable* proxy, stated as a derivation. A real flag would be a
schema decision (`test_cases.mandatory`, or a reserved tag).

**L4 — two disagreeing risk formulas exist.** `risk_assessments.score` comes from the inline
five-factor formula, not `calibrate_risk` (§1.1). Phase 4 does not read that column and does
not merge them — that would change an existing feature's output. A reader comparing
`/intelligence/tests` against a Phase 4 `measured_ratio` must know they answer differently.

**L5 — no test-to-test dependency signal.** Only one hop of dependency exists (shared
failure fingerprint, from Phase 2). `dependency_count` will stay unmeasured until a real
case-level dependency graph exists.

**L6 — `measured_ratio` assumes the unmeasured weight was neutral.** That is an assumption,
not a measurement — which is exactly why `calibration` stays `None` and `band_basis` states
it. The raw `weighted_contribution` is always reported alongside.

**L7 — no frontend surface.** Backend-only; 1605 frontend tests untouched.

**L8 — the live database is unchanged.** 27 of 30 migrations; Phase 3's 0031 and Phase 4's
code have not run against it. No risk figure in this report is a claim about production
data — all measurements are from freshly built, migrated test databases driven through
real writers.

**L9 — no E2E CI job.** Pre-existing.

---

## 10. PASS gate

| Criterion | Status | Evidence |
|---|---|---|
| Risk prioritization uses real evidence | **PASS** | every factor carries a basis and an evidence count; 18 tests over real writers |
| Existing intelligence is reused | **PASS** | `calibrate_risk`, `Factor`, `calibrate_or_none`, `failure_severity`, `average_flakiness`, `test_stability` primitives, Phase 3 change impact — **0 existing risk lines modified** |
| No duplicate risk engine exists | **PASS** | no new formula, no new table, no new engine; one added scope to the module that owns risk |
| Project isolation passes | **PASS** | 2 isolation tests, one proving byte-identical fingerprints across tenants do not bleed |
| Mandatory tests remain protected | **PASS** | 3 tests; ordering never excludes, floor is reported, `restricted_to` enforces |
| Missing data handled honestly | **PASS** | `INSUFFICIENT_DATA` × 3 reasons, 2 factors reported unmeasured with reasons, no score invented, 5 signals listed as unavailable |
| Tests pass | **PASS** | 18/18 |
| Regression passes | **PASS** | 1880 / 0 failed |
| Documentation complete | **PASS** | this report |

### Honest caveats

1. **`critical_risk` / `high_risk` bands rest on `measured_ratio`**, which assumes the
   unmeasured 30% was neutral (L6). The bands are stated with their thresholds, and the raw
   contribution is always reported, but they are weaker than a full-model score and the
   response says so.
2. **Four of the plan's eight named signals are unavailable** (requirement criticality,
   module risk, coverage gaps, per-test release trend) — §2.2. That is a capability gap,
   each reported as unavailable rather than approximated.
3. **The measurement is from a freshly built test database** (200 cases, 1,000 rows), not
   the live one, which is 19 telemetry rows. The 90.4× figure is a like-for-like comparison
   of two implementations on the same data, not a production latency claim.

**Phase 4 = PASS**, with L1–L4 carried forward as known gaps.

---

**Phase 5 not started. Nothing committed. Nothing pushed.**