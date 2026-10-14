# PHASE 6.1 — Remediation Report

**Phase:** PHASE 6.1 REMEDIATION — Open-Finding Audit and Risk Correctness
**Scope:** fix the confirmed findings only. Phase 6.2 and later phases not started.
**Verdict:** **PASS**. All of R1, R1b, F1 and F2 are closed. The two flaky **F2** fixture tests found
during R1b verification are now fixed deterministically — **§14**, test-only, with 100/100 on each
repaired test and 20/20 suite runs including under CPU load.

Decisions **D2, D4, D5** are settled by this work; **D3 and D6 remain open** and need a product call
(§11, reconciled table).

**Full backend suite: 1984 passed / 0 failed / 5 ignored** (baseline 1953; +31 new tests, none
weakened, none skipped, none newly ignored).
**Clippy:** clean. **Formatting:** clean except one pre-existing line at `HEAD` (§10).
**Integrity suite:** **20/20 consecutive runs**, and 20/20 again under CPU load after the §14 fix.

---

## 1. Initial repository state

Recorded before any edit, per Step 1.

| | |
|---|---|
| Remote | `https://github.com/abhisummer22-design/AIQA.git` (fetch + push) — the correct repository |
| Branch | `main` |
| HEAD | `7930507785d203f3ff9cb3e575413bd95f04aabb` |
| Subject | `fix: correct risk signal measurement handling` |
| Upstream | `origin/main`, **0 ahead / 0 behind** — in sync |
| Working tree | Two untracked files, both pre-existing and **preserved untouched**: `AIQA_NEXT_ROADMAP_PHASED_PLAN.md`, `docs/next-intelligence/PHASE_6_1_OPEN_FINDINGS_AUDIT.md` |

Nothing was reset, cleaned, reverted or discarded. The two untracked files are still untracked and
byte-identical.

### Baseline verification

The audit's values were re-measured rather than assumed, and **one of them needed correcting**.

| Check | Audit said | Measured | Verdict |
|---|---|---|---|
| HEAD | `7930507…` | `7930507…` | confirmed |
| Branch / sync | `main`, clean | `main`, 0/0 | confirmed |
| Full suite | 1953 / 0 / 5 | **1953 / 0 / 5** | confirmed |
| F2 flakiness | intermittent | **2 of 20 runs failed** | confirmed, rate now measured |

**A measurement error of my own, corrected.** My first attempt to count flaky runs used
PowerShell `-match 'FAILED'`, which is **case-insensitive** and therefore matched `0 failed` in
every passing line — reporting 10/10 failures. Re-run with `-cmatch` (case-sensitive), the true
baseline is **2/20**. Every run count in this report uses case-sensitive detection. The baseline
failure was the same assertion each time:

```
integrity_tests::a_genuine_status_mismatch_is_detected_and_can_establish_a_defect
panicked at src\integrity_tests.rs:443:5
assertion `left == right` failed
  left: 2
  right: 1
```

---

## 2. Summary of findings and outcomes

| ID | Finding | Root cause | Outcome |
|---|---|---|---|
| **F2** | Flaky integrity tests; duration comparison unreliable | Relative-only tolerance, no absolute floor | **FIXED** — 20/20 |
| **R1** | Silent storage-error fallbacks | `Result` from a store read discarded by `unwrap_or_default()` | **FIXED** on the flagged paths; **R1b residual reported** |
| **F1** | Test violates the documented `set_task_status` contract | `(true, true)` took only the first branch | **FIXED** (test only) |
| **R2** | `change_frequency` measured but unused | Correct measurement, deliberately zero weight | **DOCUMENTED + TESTED** — no code change |
| **R3** | No source for two risk signals | Correct: `NULL` is the honest value | **PRESERVED + TESTED** — no code change |
| **L4** | Diverging risk formulas | Three distinct metrics; one engine dormant | **DOCUMENTED + TESTED** — no code change |

---

## 3. F2 — duration comparison and test flakiness

### Root cause, confirmed from source

`integrity.rs`, `task_duration_consistency`. One comparison site; there is no second duration path
sharing the logic (verified by searching every `TOLERANCE`, `spread` and `NumericConsistency`
occurrence).

```rust
const TOLERANCE: f64 = 0.10;                     // before, line 729
…
let spread = (a - b).abs() / a.max(b).max(1.0);  // before, line 786
if spread <= TOLERANCE {                         // before, line 787
```

The two durations are **independently measured**:

- **backend** — `(tasks.finished_at - tasks.started_at)`, written by `store::set_task_status`
  stamping each end at the moment it performs that write;
- **API** — `execution_telemetry.duration_ms`, derived by `ExecutionTelemetry::from_task_state`
  (`adaptive_intelligence.rs`) from the `TaskState` timestamps.

Both are **integer milliseconds** (`num_milliseconds()`; a `u64` column), so **1 ms is the quantum**.
Under a relative-only tolerance a 0 ms span is treated as infinitely sensitive: any non-zero
reading on the other side is a 100 % divergence. The verdict therefore depended on whether two
writes landed in the same millisecond.

The fixture made this reachable: `integrity_tests`' `TaskSpec::api_duration_ms` defaults to
`None`, which sets the `TaskState` timestamps to `(None, None)`, so the API side is `0` while the
backend's is real elapsed time.

### The tolerance now in force

`effective = max(10 ms, 10% of the larger measurement)`, compared against the absolute difference.
Boundary is **inclusive** — a difference equal to the tolerance agrees.

**Why 10 ms**, verified against the actual units and precision rather than assumed:

| Consideration | Evidence |
|---|---|
| Quantum | both sides integer ms → smallest non-zero difference is **1 ms** |
| Margin over quantum | 10 ms is **10×** the quantum |
| Won't mask real disagreements | the existing genuine-disagreement case is 0 ms vs **5 000 ms** — a **500×** margin |
| Chosen by | policy constant, not fitted to any dataset |

Expressed in `integrity.rs` as two documented constants plus pure functions
`duration_tolerance_ms` / `compare_durations`, so the rule is testable without a database.

**Behaviour before / after** for the flaky case (backend 0–2 ms vs API 0):

| | Before | After |
|---|---|---|
| Verdict | `Inconsistent` when the span was ≥ 1 ms | `Consistent` always |
| `defect_established` | `true` — a **false product defect** | `false` |
| `defects_established` | 1 or 2, at random | 1, deterministically |

Staleness precedence is preserved: the tolerance is applied only after `decide_pair` has already
returned `Inconsistent`, and `decide_pair` returns `Unanswered` for a pair recorded beyond the
comparison window. An `Unanswered` outcome never reaches the tolerance branch, so a staleness
answer is never rescued into agreement.

### Tests added

Eight boundary tests for the rule as pure arithmetic — the two cases that could previously only be
reached through the whole fixture are now exact:

| Test | Case |
|---|---|
| `the_effective_tolerance_is_the_larger_of_the_absolute_floor_and_the_relative_allowance` | floor governs small, relative governs large, crossover at 100 ms |
| `two_zero_durations_are_within_tolerance` | both zero |
| `a_zero_against_a_small_non_zero_duration_is_measurement_noise` | **the exact former flake**: 0 vs 1 ms, plus symmetric and edge cases |
| `a_difference_exactly_at_the_tolerance_is_agreeing` | 0 vs 10 ms (absolute), 9 000 vs 10 000 ms (relative) |
| `a_difference_beyond_the_tolerance_is_a_disagreement` | 10.001 ms, and 1 010 vs 1 001 ms relative |
| `a_relative_difference_on_a_long_duration_is_still_detected` | 5 000 vs 20 000 ms still detected — the floor is not blind |
| `the_tolerance_rule_does_not_depend_on_which_layer_supplied_which_number` | symmetry |

No sleeps, no scheduler-dependent fixtures, no ignored tests. Two existing tests were **rewritten
because they were themselves weak or timing-dependent**, not to obtain a pass:

- `two_clocks_within_the_tolerance_are_consistent` previously used 0 ms against 0 ms, which
  `decide_pair` settles as `Consistent` **before the tolerance is consulted** — it proved nothing.
  It now records a 5 ms API duration so the tolerance branch actually runs, and asserts the caveat
  says why.
- Added `two_identical_zero_durations_agree_without_consulting_the_tolerance`, which pins the
  other, genuinely different, code path.

The existing genuine-disagreement test (`0 ms` vs `5 000 ms` → `Inconsistent` +
`establishes_defect`) passes **unmodified**, which is the proof the floor did not blunt detection.

---

## 4. R1 — silent storage-error fallbacks

### Root cause

A `Result` from a store read was discarded by `.unwrap_or_default()`, converting a storage failure
into an empty collection — which then read as a real, complete answer.

### 4.1 `GET /intelligence/risk` (`api.rs`)

`list_quality_relations_for_project(...).await.unwrap_or_default()` → `dependency_count = 0` →
declared `Factor::measured(.., 0.0, .., evidence: 0)`. Worse than "reports zero dependency risk":
the endpoint asserted it had **measured** dependency risk and found none, when it had never
looked, and `measured_weight` was inflated by a 0.10 nothing backed.

**Fix:** `.map_err(ApiError::internal)?`.

**This is not a new response shape.** The same handler already propagates its three other reads
with `ApiError::internal` (`list_execution_telemetry`, `list_flakiness_metrics`,
`list_healing_strategy_metrics`). The endpoint's failure modes already include a storage error;
this brings the fourth read into line rather than inventing a contract.

The audit offered a second option — keep `200` and declare the factor `unmeasured`. Rejected: a
partial risk answer that reports `risk_score`, `factors` and `measured_weight` still reads as a
complete assessment. The instruction was that a successful response must not imply a complete
assessment when a read failed; failing is the only representation the existing contract has that
says so unambiguously.

### 4.2 Change-impact and regression selection (`change_impact.rs`)

The audit named two sites (`:512`, `:592`). **Source inspection found twelve** across the same
reader — every read on the selection path:

| Site | Function | Masked read |
|---|---|---|
| 1 | `resolve` | `list_quality_nodes` |
| 2–4 | `impacted_from_node` | `list_quality_nodes`, `list_quality_relations_for_project`, `list_execution_telemetry` |
| 5 | `impacted_from_node` | `list_test_cases` |
| 6 | `safety_floor` | `list_execution_telemetry` |
| 7 | `safety_floor` | `list_bug_drafts` |
| 8 | `safety_floor` | `list_healing_proposals` cluster lookup (`.ok().flatten()`) |
| 9 | `safety_floor` | second `list_execution_telemetry` |
| 10–11 | `risk_scores` | `list_quality_nodes`, `list_test_intelligence` |
| 12 | `analyse` | `list_test_cases` (the `not_selected` list) |
| — | `test_node_id` | `list_quality_nodes` |

**Fix:** `resolve`, `impacted_from_node`, `safety_floor`, `risk_scores`, `analyse` and
`test_node_id` now return `anyhow::Result` and propagate with `?`. `analyse` returning
`Result<ChangeImpact>` is a signature change; `api.rs`'s handler maps it to `ApiError::internal`,
so the HTTP status codes are unchanged (still `201`/`200` on success).

The two the audit called most serious were the worst cases: `safety_floor` is the tier that exists
so a case with recorded failure evidence is *never dropped for speed*, and `risk_scores` feeds the
`HighRisk` tier. Both made a storage fault select **fewer** tests, silently.

### 4.3 Prioritisation (`qa_memory_risk.rs`)

The two flagged reads (`list_quality_nodes`, `list_change_impacts` in the `change_ref` branch) now
propagate, and `prioritise_tests` returns `anyhow::Result<Retrieved<TestPrioritisation>>`.
`api.rs` maps to `ApiError::internal`; `agents/exploration.rs` uses `.ok()?`, which is correct for
an `Option`-returning optional signal.

**This alone did not achieve the objective, and the tests proved it.** Upstream reads masked first,
so the change-path fix was unreachable on that path. R1b is now closed — see §11.

### Tests added

Five, all using `store.pool.close().await` — the real-failure technique already used in
`engine_tests.rs`, not a mock and not fabricated state:

| Test | Asserts |
|---|---|
| `change_impact_tests::a_storage_failure_is_reported_rather_than_returning_an_empty_selection` | healthy read still succeeds; closed pool → `Err`, never an empty `ChangeImpact` |
| `change_impact_tests::a_storage_failure_cannot_silently_empty_the_safety_floor` | the floor errors rather than reading as "nothing ever failed" |
| `change_impact_tests::a_storage_failure_cannot_silently_empty_the_high_risk_tier` | the risk read errors rather than selecting nothing |
| `api_tests::a_storage_failure_is_reported_rather_than_a_risk_answer` | `GET /risk` returns a 5xx, not `200`; the body carries **no** `risk_score`, `factors` or `measured_weight` |
| `api_tests::a_healthy_risk_read_is_unaffected_by_removing_the_silent_fallback` | control — a healthy read still returns `200`, `INSUFFICIENT_DATA` and a `factors` block |

Each storage-failure test asserts the healthy path **first**, so a failure caused by anything else
cannot make it pass.

One test was **rewritten because it asserted the wrong thing**: my first
`prioritise_tests` failure test claimed an error, and failed — because the upstream masked read
short-circuits first. See §11.

---

## 5. F1 — `set_task_status` contract

### Verified from source

The contract is **documented and intentional**:

- `store.rs:5885-5888` — *"the two writes have to be separate, because `set_task_status` treats
  `set_started`/`set_finished` as mutually exclusive and a combined call would never set
  `finished_at`, which history reads filter on."*
- `store_tests.rs:253-256` — says the same.

All three production callers comply, using exactly one flag each:

| Call site | Flags |
|---|---|
| `engine.rs:838` (task starts) | `(true, false)` |
| `engine.rs:508` (task terminal) | `(false, true)` |
| `engine.rs:654-660` (orphan → `InfraError`) | `(false, true)` |

So the problematic state **remains unreachable in production**, and the audit's
`NOT_REPRODUCIBLE` classification stands. **No production code was changed** — the instruction
was explicit that production logic must not be modified to accommodate an invalid test, and the
inspection did not show the contract itself to be wrong.

### The violation, fixed

`engine_tests.rs`' `seed_run` helper called `set_task_status(.., true, true)`, taking only the
start branch and leaving `finished_at` NULL. Consequence: `task_history_by_name` filters on
`finished_at IS NOT NULL` (`store.rs:3356-3359`, `:3372-3374`), so a seeded run was invisible to
the reader most of the intelligence layer depends on. Nothing failed because no caller asserted on
those runs' history — the seed silently stopped meaning what its name said.

**Fix:** two calls, one flag each, with a comment recording why.

### Tests added

`store_tests::the_documented_one_flag_at_a_time_contract_is_what_the_store_does` pins both
directions:

1. **Valid sequence** — start records `started_at` and *not* `finished_at`; finish records
   `finished_at`, leaves `started_at` untouched, stores the reason and status; and the task then
   **appears** in `task_history_by_name`.
2. **Invalid sequence** — `(true, true)` records only the start, leaves `finished_at` NULL, and
   the task stays **invisible** to history. Recorded as the current, intended-by-signature
   behaviour, with a `KNOWN LIMITATION` message, so anyone changing the signature knows the test
   asserted something real.

The signature keeps two booleans. Converting to an enum would remove the trap permanently, but it
changes a public method and is a design decision, not a correctness fix.

---

## 6. R2 — `change_frequency`

**Decision applied: keep it informational, add no scoring weight.** No code change.

### Data flow verified

| Stage | Location |
|---|---|
| Source | `change_impacts` (migration `0031`), written by `Store::insert_change_impact` |
| Reader | `Store::count_distinct_changes_for_node`, one caller: `intelligence.rs` |
| Persisted | `risk_assessments.change_frequency` and `test_value_assessments.change_relevance` |
| Consumed by | Phase 6's `fabricated_constant_check`, which is told it *has* a source |
| **Risk formulas** | **none** — verified by searching every weighted expression in `intelligence.rs`, `adaptive_intelligence.rs` and `qa_memory_risk.rs` |

Availability states confirmed: **available** when the table can be read; **measured zero** when no
declared change touched a node; **unavailable** only when the read itself fails — which now
propagates rather than defaulting.

### Why no weight, recorded in code

`change_impacts` already reaches test selection twice on its own:

1. Phase 3's `DirectlyImpacted` / `RelatedFailure` tiers;
2. Phase 4's `PriorityBand::Changed`.

A third influence through `risk_assessments.score` would be a third observation of the same
underlying fact. Selection de-duplicates, so no test would be selected twice — but the **signal**
would be counted three times, which is the double-counting the plan forbids.

### Tests added — `risk_semantics_tests.rs`

- `r2_change_frequency_is_a_real_measurement_including_at_zero` — 0 before any declared change, 2
  after two, and **still 2** when one change is recorded twice (distinct `change_ref`, not rows).
  Proves the measurement is real rather than merely un-nulled.
- `r2_change_frequency_carries_no_weight_in_either_risk_formula` — asserts `change_frequency` is
  absent from `FACTOR_WEIGHTS`, with the double-counting reasoning in the failure message, so a
  future change fails here first and deliberately.
- `r2_the_only_consumer_of_change_frequency_is_the_integrity_check` — pins the consumer map.

---

## 7. R3 — unsourced signals

**Decision applied: keep `security_sensitivity` and `business_criticality` unmeasured.** No code
change, no migration.

### Verified

Exhaustive search of all 32 migrations and every production struct: the only occurrences are the
nullable `0002` columns, `RiskFactors.security_sensitivity`, and `requirements.kind`'s CHECK
constraint — a *category*. Nothing new exists.

`None` / `NULL` / `Factor::unmeasured` is retained everywhere. No value is inferred from
`requirements.kind`, `test_cases.priority`, `.severity`, `.category` or `tags`.

### What an authoritative source would require

Documented in `risk_semantics_tests.rs`. Any future source must be one of:

1. **A declared classification** — a stored, human-entered value, recorded as a *declaration*
   distinct from a measurement, with who asserted it and when.
2. **A derived measure from a signal that genuinely measures it** — e.g. security sensitivity
   computed from an actual static-analysis or authz-configuration result already stored in this
   schema.

Not acceptable, and pinned by test: a category, a priority, a severity, a tag, a name, a default,
or any heuristic. Those record *what kind of thing a test is*, not *how much it matters*. Wiring
any of them would convert an absence into a fabricated measurement — the exact defect the Phase 4
risk-signal remediation removed.

### Tests added

- `r3_neither_signal_is_inferred_from_any_proxy_in_the_schema` — a case with
  `category: "security"`, `severity: "critical"`, `priority: "P0"` and security tags still yields
  `security_sensitivity = NULL` and `business_criticality = NULL`.
- `r3_measured_zero_unmeasured_and_unavailable_stay_distinct` — `change_frequency = Some(0.0)`
  (measured zero) alongside `security_sensitivity = NULL` (unmeasured), with the unbacked 0.10
  weight quantified and the signal named in `unmeasured_factors`.

---

## 8. L4 — risk calculation clarity

**No function merged, no weight, threshold, stored score or classification changed.** Verified by
inspecting the diff: **no risk-formula weight line was added or removed.**

### What each calculation is

| Calculation | Location | Inputs | Weights | Normalisation | Output |
|---|---|---|---|---|---|
| `calibrate_risk` | `adaptive_intelligence.rs` | failure rate, flakiness, severity, dependency, healing regression, security sensitivity | 0.35 / 0.20 / 0.15 / 0.10 / 0.10 / 0.10 = **1.00** | none | score **+ band** |
| `risk_score` | `intelligence.rs` | historical failure rate, flakiness, dependency, execution cost | 0.40 / 0.20 / 0.15 / 0.15 = **0.90** | ÷ `measured_weight` | score, **bandless** |
| `value_score` | `intelligence.rs` | failure rate, prior `risk_score`, **inverted** flakiness, **inverted** cost | 0.40 / 0.30 / 0.20 / 0.10 = 1.00 | none | score, **inverted** |
| `PriorityBand` | `qa_memory_risk.rs` | floor, change, risk, recurrence, flakiness | n/a — ordered bands | n/a | ordering |

These answer three different questions — *how risky*, *how valuable*, *what to run first*. Only
the first and third carry bands, and `value_score` is deliberately inverted, so it must never be
compared against or renormalised against a risk score.

### Dormancy, verified

`calibrate_risk` has exactly **one** production caller: `qa_memory_risk::calibrate_or_none` at
`qa_memory_risk.rs:970` (`api.rs:9227` is a comment). `calibrate_or_none` refuses unless **all six**
factors measure; `security_sensitivity` has no source and `dependency_count` is structurally
unavailable for a module, release or test case. Therefore **`calibrate_risk` is dormant**: correct,
tested, and unreachable through any live path. `GET /intelligence/risk` reaches a band through
`calibrate_partial` instead.

Recorded, not fixed. Reviving `calibrate_risk` would change every score that endpoint returns —
decision **D3**, out of scope here. `calibrate_risk` is untouched.

### Tests added — `risk_semantics_tests.rs`

| Test | Pins |
|---|---|
| `l4_the_three_calculations_are_distinct_and_are_not_merged` | six factors totalling 1.00; a maximal fully-measured set reads 1.0 / `CRITICAL` |
| `l4_one_owner_of_the_level_bands` | four bands, boundaries at 0.25 / 0.50 / 0.70, so `calibrate_risk` and `calibrate_partial` cannot disagree |
| `l4_calibrate_risk_is_dormant_because_calibrate_or_none_cannot_succeed` | the dormancy mechanism, with the "recorded, not fixed" reasoning |
| `l4_a_fully_measured_model_is_unchanged_by_normalisation` | control — the normalisation is inert when it should be |
| `l4_an_unmeasured_factor_never_becomes_a_zero_inside_the_engines_own_types` | `Factor::unmeasured` carries no value, contributes nothing, states why, has zero evidence |

---

## 9. Test results

### Integrity suite — 20 consecutive runs (required gate)

Baseline **18/20**, post-remediation **20/20**. All 20 runs executed; no run aborted, no run
filtered.

| Run | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|
| | pass | pass | pass | pass | pass | pass | pass | pass | pass | pass |

| Run | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 |
|---|---|---|---|---|---|---|---|---|---|---|
| | pass | pass | pass | pass | pass | pass | pass | pass | pass | pass |

**20/20 passed, 0 failed.**

### Targeted suites

| Suite | Baseline | Final |
|---|---|---|
| `integrity_tests` | 26 (+1 ignored) | **34 passed, 0 failed, 1 ignored** |
| `change_impact_tests` | 16 | **19 passed** |
| `risk_prioritisation` | 18 (+1 ignored) | **20 passed, 0 failed, 1 ignored** |
| `risk_semantics_tests` | — (new) | **10 passed** |
| `risk_signal_tests` | 18 | **18 passed** |
| `store_tests` | 35 | **36 passed** |
| `api_tests` | 313 | **315 passed** |
| `engine_tests` | 33 | **33 passed** |
| `exploration_tests` | 29 (+1 ignored) | **29 passed, 0 failed, 1 ignored** |
| `qa_memory_risk_tests` | 18 | **18 passed** |
| `adaptive_intelligence_tests` | 38 | **38 passed** |

Ignored count is unchanged at **5**. No test was skipped, weakened, removed or newly ignored.

### Full backend suite

```
test result: ok. 1979 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 21.05s
```

1953 → 1979 = **+26**: 8 integrity (F2), 5 storage-failure (R1), 1 contract (F1), 10 semantics
(R2/R3/L4), 2 elsewhere.

### Formatting and Clippy

| Check | Command | Result |
|---|---|---|
| Formatting | `cargo fmt --check` | **exit 1** — one pre-existing `Diff`, §10 |
| Clippy (repository's existing options) | `cargo clippy --all-targets` | **exit 0** |

The repository has no `clippy.toml` and no `[lints]` section, so its existing option set is the
default invocation. For completeness I also ran `cargo clippy --all-targets -- -D warnings`,
which reports **58 lints across 24 pre-existing files** — including unused imports in
`autopilot_api.rs`, `openapi_api.rs` and `regression_api.rs`, none of which this remediation
touches. That flag is stricter than the repository's bar and was not in force at baseline. Of the
58, exactly one was introduced by this work (a doc-comment placement in `store_tests.rs`); it is
fixed — see §10.

---

## 10. A mistake I made, and the diff audit

While adding the F1 contract test I inserted it by searching for a comment string, which placed it
**inside the doc comment of an unrelated test**, orphaning that comment from its function. Caught
by `clippy -- -D warnings` reporting `empty line after outer attribute` at `store_tests.rs:322`.
The block was lifted out and re-appended at the end of the file; the B14 doc comment is contiguous
again and the lint is gone.

Final diff audit:

| Check | Result |
|---|---|
| Files changed | **14** — all intentional |
| `.sql` migrations | **0** |
| New/changed HTTP routes | **0** (the `.get(` matches in the diff are `body.get()` on JSON in tests) |
| Risk-formula weight lines added/removed | **0** |
| `RISK_LEVEL_THRESHOLDS` values changed | **0** |
| Stored score semantics changed | **0** |
| Unrelated files in the diff | **0** — an incidental 2-line whitespace change to `auth.rs` was detected and reverted |

**Pre-existing formatting failure, not mine.** `cargo fmt --check` reports one `Diff` at
`auth.rs:317` — a `concat!` inside `#[doc = …]` in the `role_gate!` macro, where rustfmt is not
idempotent. Proven pre-existing: `git show HEAD:...auth.rs` written to a scratch file and checked
with `rustfmt --check` produces the identical `Diff`, so `HEAD` itself is not rustfmt-clean at that
line. `auth.rs` is untouched by this remediation.

---

## 11. Remaining risks and unresolved questions

## 11. R1b — closed

### 11.1 Verdict

**R1b = PASS.** `QaMemoryReader::telemetry_for` and `test_evidence` return `anyhow::Result`, and no
production caller converts a required storage failure into an empty or default value.

> **Correction to the R1b section this replaces.** The text below previously read *"ripples through
> **all 14 of its callers**"*. That count was inflated: it included the function definition, a doc
> comment, and an unrelated test-local helper also named `telemetry_for`
> (`qa_memory_retrieval_tests.rs:163`). The real production surface is **10 `telemetry_for` call
> sites across 11 public views, plus 3 `test_evidence` callers** — 20 production call sites in
> total, all enumerated below. The gap was real; the stated size of it was not.

### 11.2 Root cause

`telemetry_for` and `test_evidence` read through `unwrap_or_default()`. A closed pool therefore
produced an **empty** evidence set, and the `evidence.all.is_empty()` guard that yields
`NO_EXECUTIONS` was consulted against evidence that had failed to load. A project with recorded
executions was reported as having none — a plausible-looking wrong answer, and the exact opposite
of what `insufficient_data` is supposed to mean.

### 11.3 What was changed

`Result` propagation was completed across the whole R1b call graph, and the search surfaced further
masked reads on the same paths that a signature change alone would have left cosmetic.

| Function | Masked reads replaced with `?` |
|---|---|
| `telemetry_for` (`qa_memory.rs`) | 1 — the defining read |
| `test_evidence` (`qa_memory_risk.rs`) | 4 `unwrap_or_default()` + `.ok()` |
| `bug_history` | **3** — `list_bug_drafts` (`else { return Ok(vec![]) }`), `list_quality_nodes`, `list_quality_relations` |
| `test_stability`, `module_risk_history`, `release_history`, `requirement_history`, `project_memory_summary`, `planning_signals` | 7 across the set |
| `module_risk` | 1 — a `match` whose wildcard arm returned `NoMatchingHistory` for *any* failure |
| `release_risk` | 2 — `list_releases`, `runs_for_release` |
| `release_risk_overview` | 1 — `list_releases` |
| `generate_planning_feedback` (`adaptive_intelligence.rs`) | `planning_signals` |
| `api.rs` `get_test_risk_scoped` | `test_risk` → `ApiError::internal` |
| `api.rs` `generate_test_plan_scoped` | `generate_planning_feedback` (`.ok()`) and `get_feedback_for_planning` (`unwrap_or_default()`) |

**Callers found by search that the original report did not name**, and which were masked:

- `release_risk_overview` — not in the original list; had its own `unwrap_or_default()`.
- `generate_test_plan_scoped` — fetched both grounding inputs fail-soft, so a `200` plan could be
  built against signals never loaded.
- `bug_history` — already returned `Result` for its telemetry read, but three sibling reads ignored
  the error channel that was already there.

The two patterns worth recording, because both survived a `Result` signature and would have made it
decorative:

- `let Ok(x) = read().await else { continue }` — a failure silently drops a release from a trend
  that exists to be computed.
- `match (read_a, read_b) { (Ok(a), Ok(b)) => .., _ => NO_MATCHING_HISTORY }` — both failure and
  genuine empty collapse into one answer.

### 11.4 The three states, preserved

| State | Result |
|---|---|
| healthy store, genuinely no history | `Ok(NO_EXECUTIONS)` — **unchanged and asserted** |
| healthy store, with history | `Ok(view)` |
| **closed store** | **`Err`** |

Genuine `NO_EXECUTIONS` is preserved: where a `let Ok(x) = … else` arm was replaced, the genuine
empty case is still answered — either by an explicit `is_empty()` check or by the pre-existing
fall-through that already produced the right reason. Risk weights, thresholds, migrations and routes
are untouched, and unmeasured factors still report their reason rather than a fabricated zero.

### 11.5 Tests, and which ones actually prove it

`store.pool.close().await` is the technique throughout — a real read failure, no mock, no sleep.

| Test | Proves |
|---|---|
| `qa_memory_risk_tests::r1b_a_failed_read_is_never_reported_as_no_executions` | the three states, incl. preserved `NO_EXECUTIONS` |
| `qa_memory_risk_tests::r1b_no_affected_view_reports_a_failed_read_as_an_empty_answer` | **all 11 views**, each asserted healthy *and* failed |
| `qa_memory_risk_tests::r1b_unmeasured_factors_stay_unmeasured_through_the_new_error_path` | no unmeasured factor became a measured zero |
| `risk_prioritisation_tests::a_storage_failure_is_reported_rather_than_no_executions` | replaced the `KNOWN GAP R1b` characterisation test |

**The characterisation test was inverted, not deleted.** The old
`the_change_path_reads_propagate_and_the_healthy_path_is_unaffected` asserted the *bug*
(`Ok(NO_EXECUTIONS)` after close) and **failed** once R1b landed. It is now
`a_storage_failure_is_reported_rather_than_no_executions`, exactly as its own comment said it should
become. It asserts all three states, healthy first so the failure cannot pass for the wrong reason.

**Discrimination was verified by mutation, not assumed.** Reverting `bug_history`'s drafts read to
fail-soft makes the all-views test fail with `bug_history converted a failed read into an empty
answer`. That is observed, not predicted.

### 11.6 Two tests I wrote that do *not* prove what they first claimed

Recorded because they would otherwise read as coverage they do not provide.

- `api_tests::a_storage_failure_is_not_reported_as_a_grounded_test_plan`
- `adaptive_intelligence_tests::r1b_planning_feedback_reports_a_failed_read_rather_than_no_signals`

Both were checked by mutation. Reverting `qa_feedback` to `.ok()`, and reverting the
`planning_signals` read to `unwrap_or_default()`, **leaves both tests passing** — because a closed
pool cannot single out one read among several: `generate_planning_feedback` reads
`list_execution_telemetry` first and fails there before `planning_signals` is reached, and in the
handler `generate_test_plan` fails later on the same closed pool and returns `5xx` regardless.

One closed pool cannot isolate one read. Distinguishing them needs a seam the store does not offer.
Both tests now document this in their doc comments. They are retained as **endpoint-contract
guards** — a closed store must never yield a `200` plan — which is worth pinning regardless of which
read fails first. The per-read propagation is pinned at the reader level by the all-views test.

### 11.7 The two flaky F2 tests — **now FIXED**, see §14

Found during R1b verification, **not** R1b defects. Both were fixture non-determinism: the F2 tests'
preconditions ("two identical zero durations") were not actually guaranteed. They were reported here
rather than repaired, because repairing them means changing F2 test design, which was outside R1b.

**Repaired in the F2 test-stability follow-up — see §14 for the root cause, the fix, and the
100-run evidence.** They are no longer a residual.

### 11.8 Results

### 11.8 Results

- **Full backend suite: 1984 passed / 0 failed / 5 ignored** — three consecutive runs, identical.
  Baseline was 1979; the delta is the 5 tests added here.
- **Targeted suites:** `qa_memory_risk_tests` 21, `qa_memory_retrieval_tests` 21,
  `qa_memory_planning_tests` 14, `qa_memory_perf_tests` 7, `qa_memory_validation_tests` 7,
  `adaptive_intelligence_tests` 39, `risk_prioritisation` 20 (+1 ignored), `risk_semantics_tests` 10,
  `risk_signal_tests` 18, `api_tests` 316, `integrity_tests` 34 (+1 ignored), `exploration_tests` 29
  (+1 ignored), `change_impact_tests` 19, `store_tests` 36, `engine_tests` 33, `governed_healing` 53 —
  all passing.
- **`cargo fmt --check`:** one diff, `auth.rs:317` — **pre-existing**, proven at HEAD
  (`git show HEAD:…auth.rs` → `rustfmt --check`); rustfmt is non-idempotent on `concat!` inside
  `#[doc = …]` within `role_gate!`. Not introduced here, not fixed here.
- **`cargo clippy --all-targets`:** exit 0 — the repo's bar.
- **New lints:** none. Under `-- -D warnings` (which surfaces ~58 pre-existing lints across 24
  untouched files), the 9 sites in files I touched were each verified to exist at HEAD — 117
  `Extension(ctx)` bindings in HEAD's `api.rs`, `from_str` at exactly `adaptive_intelligence.rs:69`.
  One lint *was* mine (a needless `?` inside `Ok(...)` at `qa_memory.rs:334`) and is fixed.

### 11.9 Audit

A brace-counted scan of every `Result`-returning view reports **0** remaining masked store reads
(`unwrap_or_default`, `.ok()`, `.ok()?`, `let Ok(…) else`, `if let Ok(…)` on a store read). All
**20** production call sites handle the `Result` explicitly.

Seven `.unwrap_or(false)` / `.unwrap_or_default()` sites were checked individually and **left alone**:
they are `Option` operations on in-memory rows (`r.test_case_id.map(…)` for a row with no case, a
node with no `external_ref`) — legitimate, and converting them would be wrong.

### 11.10 Not done

Phase 6.2 and Phase 7 were not started. Nothing was committed or pushed. All pre-existing user
changes and the four untracked files are preserved intact.

### D2–D6 — reconciled

The heading here previously read *"Open decisions (unchanged from the audit, none taken here)"*,
which **contradicted §§6 and 7 of this same report**. Those sections record D2 and D4 as
*"**Decision applied**: …"*, and D5 was implemented as `DURATION_ABSOLUTE_FLOOR_MS = 10.0`. Calling
all five "unanswered" was wrong. Reconciled, with no new answers invented and no formula touched:

| | Decision | Exact question | Status |
|---|---|---|---|
| **D2** | `change_frequency` weight | Should `change_frequency` ever carry weight in a risk formula? | **APPLIED — informational, no weight.** §6 added no weight; `FACTOR_WEIGHTS` unchanged. Recommendation on record: "no, unless the two existing change paths are re-designed together". The *weight-bearing* question is settled by inaction; reopening it is a product call. |
| **D3** | Canonical risk formula | Which formula — `calibrate_risk` or `priority_score` — is *the* risk score? | **OPEN.** Genuinely unanswered. §6/§8 document both without ranking them. Recommendation on record: "keep both, document each", the only option that changes no score. Not decided here. |
| **D4** | Source for the two unsourced signals | Is there an authoritative source for `security_sensitivity` and `business_criticality`? | **APPLIED — keep unmeasured.** §7 records the answer as no source exists; both stay `measured: None` with a reason. Recommendation on record: "no; keep unmeasured". Satisfied without a code change. |
| **D5** | The 10 ms clock-noise floor | Is 10 ms the right floor for two independently recorded durations? | **APPLIED as a stated constant, not endorsed as correct.** `DURATION_ABSOLUTE_FLOOR_MS = 10.0` shipped in §3 with the justification and both tolerance components reported per comparison. The *policy* is in force; the value is provisional, per its own doc comment: "a stated policy constant, not a value fitted to any observed dataset". Recommendation on record: "adopted; revisit if real durations cluster near it". |
| **D6** | `set_task_status` signature | Should `set_task_status` take an enum instead of two `bool` flags? | **OPEN.** Not answered, not attempted. F1 recorded the two-flag contract and the single-call hazard as a documented limitation; redesign is a separate task. Recommendation on record: "optional design improvement, not a correctness fix". |

**So: three are settled by the work (D2, D4, D5 — each settled by leaving a score unchanged rather
than by inventing a value), and two remain genuinely open (D3, D6).** Nothing in this follow-up
changed a formula, a weight, or a threshold.

### Other known limitations, carried forward

- `prioritise_tests` returning `Result` widens its public signature; 24 call sites gained
  `.expect("prioritise_tests")`. **No assertion changed.**
- `ChangeImpactReader::analyse` and four other methods now return `Result`; 20 test call sites
  gained `.expect("analyse")`. **No assertion changed.**
- `GET /intelligence/risk` now returns **5xx** where it previously returned `200` on a relation-read
  failure. This is the one user-visible behaviour change, and it aligns the endpoint with the three
  sibling reads it already failed on.
- `test_value_assessments.change_relevance` still stores a value no formula reads (pre-existing).

---

## 12. Files changed

| File | Change | Why |
|---|---|---|
| `integrity.rs` | +130 / − | absolute tolerance floor, documented constants, pure `compare_durations`, boundary handling | F2 |
| `integrity_tests.rs` | +234 / − | 8 boundary tests; 2 rewritten because weak or timing-dependent | F2 |
| `change_impact.rs` | +123 / − | 12 masked reads propagated; 6 methods now return `Result` | R1 |
| `change_impact_tests.rs` | +151 / − | 3 storage-failure tests; 15 call sites gained `.expect` | R1 |
| `qa_memory_risk.rs` | +19 / − | 2 masked reads propagated; `prioritise_tests` returns `Result` | R1 |
| `api.rs` | +20 / − | 2 read failures propagated as `ApiError::internal` | R1 |
| `agents/exploration.rs` | +5 / − | 1 read failure handled | R1 |
| `exploration_tests.rs` | +20 / − | 4 call sites gained `.expect` | R1 |
| `engine_tests.rs` | +18 / − | `seed_run` now uses the documented two-call contract | F1 |
| `store_tests.rs` | +154 / − | contract test added (re-placed after my own placement error) | F1 |
| `risk_semantics_tests.rs` | **new**, 10 tests | R2 / R3 / L4 contracts | R2/R3/L4 |
| `risk_prioritisation_tests.rs` | +213 / − | storage-failure characterisation test; call sites | R1 / R2 |
| `api_tests.rs` | +111 / − | 2 storage-failure tests | R1 |
| `risk_signal_tests.rs` | +5 / − | reverted a misplaced append | — |
| `lib.rs` | +6 / − | registers `risk_semantics_tests` | — |

Preserved untouched: `AIQA_NEXT_ROADMAP_PHASED_PLAN.md`,
`docs/next-intelligence/PHASE_6_1_OPEN_FINDINGS_AUDIT.md`, and all prior phase reports.

---

## 13. Completion gate

| Requirement | Result |
|---|---|
| Every in-scope confirmed issue fixed and tested, or explicitly justified as needing no code change | **Yes** — F2, R1, R1b, F1 fixed; R2, R3, L4 justified with tests |
| Integrity suite passes 20 consecutive runs | **Yes — 20/20**, and 20/20 again under CPU load after the §14 fix |
| Complete backend suite passes | **Yes — 1984 / 0 / 5**, three consecutive runs |
| Formatting and Clippy pass | **Yes** — Clippy exit 0; fmt clean except one line proven pre-existing at `HEAD` |
| No regression test weakened or removed | **Yes** — 1 characterisation test **inverted** into a real assertion, 0 assertions removed |
| Risk factors not fabricated | **Yes** — R3 pinned by test; `change_frequency` weight deliberately not added; unmeasured factors still report a reason through the new error path |
| Storage failures cannot silently produce misleading risk or regression-selection results | **Yes** — 0 masked store reads remain across every `Result`-returning view; 20/20 production call sites handle the `Result` explicitly |
| Every claimed test proven to exercise a real failure | **Yes** — verified by mutation, including two tests whose limits are documented in §11.6 rather than papered over |
| Final diff contains only intentional changes | **Yes** — 0 migrations, 0 routes, 0 weight or threshold changes |

**PHASE 6.1 REMEDIATION = PASS.** R1b is closed. The two flaky F2 fixture tests are closed (§14).
Decisions D2, D4 and D5 are settled by the work; **D3 and D6 remain open** and need a product call —
they were not guessed (§11, D2–D6 table).

Phase 6.2 has not been started. Nothing was committed or pushed.

---

## 14. F2 test-stability follow-up

**Verdict: PASS.** Both flaky tests are deterministic. No production code changed.

### 14.1 Verified root cause

`record_run` writes the backend's clock with **two separate awaited calls** to
`store::set_task_status`, and `set_task_status` stamps `now_rfc3339()` independently at each end. So
the backend duration the verifier reads is `(finished_at - started_at).num_milliseconds()` of real
scheduling latency — two different instants, never the same one.

The fixture's own comment at `record_run`'s duration test said so outright: *"The backend's clock
spans ~0 ms (set_task_status stamps both ends now)"*. Every duration assertion in the suite read the
backend side as **zero**, so all of them were really asserting whether two writes landed in the same
millisecond.

**Measured, not assumed.** 300 sequential fixture builds produced `0 ms` 300/300 — which is why the
flakes were rare and load-dependent. Under parallel load, 600 builds gave:

| backend duration | samples |
|---|---|
| 0 ms | 393 |
| 1 ms | 163 |
| 2–7 ms | 44 |
| 21 ms, 62 ms | 2 |

That single distribution explains both flakes:

| Flaky test | Trigger | Why it failed |
|---|---|---|
| `two_identical_zero_durations_agree_without_consulting_the_tolerance` | backend read 1 ms | values then *differ*, so the tolerance branch rewrites `detail` to cite the tolerance — exactly what the test asserts never happens |
| `a_genuine_status_mismatch_is_detected_and_can_establish_a_defect` | backend read > 10 ms | the duration pair genuinely disagreed, establishing a **second** defect: `defects_established` 2, not 1 |

A **third** test was latently flaky from the same cause and is fixed by the same change:
`two_clocks_within_the_tolerance_are_consistent` (backend ~0 vs API 5 ms) would have failed had the
backend read ≥ 16 ms.

### 14.2 The fix — test-only

`integrity_tests.rs` gains one fixture helper, `pin_backend_clock`, called from `record_run` after the
`set_task_status` pair:

```sql
UPDATE tasks SET finished_at = started_at
 WHERE run_id = ? AND id = ? AND started_at IS NOT NULL AND finished_at IS NOT NULL
```

The backend duration is now **exactly 0 ms by construction**, so the suite reads what it always meant
to read. The API layer's own duration is unchanged — it still comes from `TaskState` timestamps.

### 14.3 Why this, and not a bigger tolerance

Raising `DURATION_ABSOLUTE_FLOOR_MS` would have fixed only the *second* flake. The first test's
assertion is that the tolerance is **not cited when two values are equal** — no tolerance size can
satisfy that, since any non-zero backend reading makes the values unequal. The production comparison
was therefore the wrong thing to touch: the verifier is right that two clocks recorded a millisecond
apart can disagree. **The defect was the fixture asking a timing question it could not control.**

`DURATION_ABSOLUTE_FLOOR_MS` is unchanged at 10.0, as is `DURATION_RELATIVE_TOLERANCE`,
`duration_tolerance_ms` and `compare_durations`. No assertion was removed, weakened, or reworded; no
test was ignored; no sleep was added.

### 14.4 An honest limit on the NULL guard

The `finished_at IS NOT NULL` guard exists so a missing pair stays missing rather than being
manufactured — which would hide finding F1. **I could not prove the guard load-bearing and am not
claiming it is.** I simulated the F1 regression in the fixture (leaving `finished_at` NULL) both with
and without the guard, and the suite passed either way, because `record_run` always makes two
correctly-flagged calls and no current test drives a NULL pair through it. It is protection against a
future fixture change, not coverage that exists today. F1 itself is pinned independently by
`store_tests::the_documented_one_flag_at_a_time_contract_is_what_the_store_does`.

### 14.5 Files changed

| File | Nature |
|---|---|
| `services/orchestrator/src/integrity_tests.rs` | **the only file changed** — one fixture helper, one call site, documentation |
| `docs/next-intelligence/PHASE_6_1_REMEDIATION_REPORT.md` | this section; §11.7 and §13 corrected |

**Production code changed: none.** `integrity.rs` and `store.rs` are byte-identical to their
pre-follow-up state — the 130-line `integrity.rs` diff against `HEAD` is the *prior* phase's F2 work
already reported in §3, not this follow-up.

### 14.6 Results

- **Each repaired test ×100 consecutive runs:**

  | Test | Pass | Fail |
  |---|---|---|
  | `two_identical_zero_durations_agree_without_consulting_the_tolerance` | **100/100** | 0 |
  | `a_genuine_status_mismatch_is_detected_and_can_establish_a_defect` | **100/100** | 0 |

- **Integrity suite ×20 consecutive runs: 20/20 passed, 0 failed.**
- **Integrity suite ×20 under CPU load** (12 parallel burners): **20/20 passed** — the condition under
  which the original 1/20 rate appeared. Plus **100/100** for the zero-duration test under load.
- **Full backend suite: 1984 passed / 0 failed / 5 ignored** — unchanged from before this follow-up.
- **`cargo fmt --check`:** one diff, `auth.rs:317`, pre-existing and proven at HEAD.
- **Clippy `cargo clippy --all-targets`: exit 0.** Zero lint sites in `integrity_tests.rs` under
  `-- -D warnings`.
- **Diff audit:** this follow-up's code is one `UPDATE` statement and one `.await`. The large diff in
  `integrity_tests.rs` against `HEAD` is the prior phase's added boundary tests; the only *removed*
  lines in the whole file are a doc comment and an import line, both re-added in changed form.

Phase 6.2 has not been started. Nothing was committed or pushed.
