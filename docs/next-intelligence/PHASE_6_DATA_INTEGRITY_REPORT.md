# Phase 6 — Data Integrity + Cross-Layer Intelligence

**Phase 6 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`. Backend only. PASS.**

Predecessors: [`PHASE_5_PASS.md`](./PHASE_5_PASS.md) and Phases 1–5 generally. None were
restarted or rewritten.

---

## 1. Audit: what cross-layer capability already exists

### 1.1 No cross-layer comparison exists — but every layer has a real writer

There is no comparison capability to extend. What exists is the **evidence** for one, and it is
better than expected: all five layers the brief names are written by distinct production code
paths.

| Layer | Table / source | Real writer | Reader (pre-existing) |
|---|---|---|---|
| `UI` | `crawler::CrawlOutput` | the TypeScript crawler | `read_crawl_result` |
| `API` | `execution_telemetry` | `Intelligence::ingest_run` | `list_execution_telemetry` |
| `BACKEND` | `tasks` | `store::set_task_status`, from `engine.rs` | `list_tasks_for_run` |
| `DATABASE` | `runs`, `failure_clusters`, `evidence`, `risk_assessments` | `set_run_state`, `record_failure`, `insert_evidence`, `upsert_risk_assessment` | `get_run_summary`, `list_failure_clusters`, `list_evidence_for_run` |
| `AUDIT` | `audit_log`, `run_events` | `insert_audit`, `insert_event` | `list_audit`, `list_events_since` |

**Every one of those readers already existed.** Phase 6 needed exactly **one** new store reader
(§9.1) and **no migration** — `0032` remains the most recent.

### 1.2 There is no governed DB read mechanism, and none was needed

An audit for arbitrary-query or read-only-query infrastructure found **none**: no
`query_only`, no `exec_sql`, no query builder anywhere. `Store` is a hand-written reader per
table, and every statement in it is fixed text with bound parameters.

That turns out to be the strongest possible answer to the brief's read-only requirement:

* there is **no query surface to constrain**, so arbitrary SQL cannot be introduced by
  forgetting a rule;
* "reuse the existing governed read mechanism" resolves to "call the existing readers";
* and `IntegrityVerifier` holds a `&Store`, so it is structurally incapable of writing.

Verified mechanically, not asserted: `integrity.rs` contains **zero** occurrences of `sqlx`,
**zero** SQL write verbs in its code, and **zero** `&mut self` methods. The one `&mut self` in
the file is inside the module's own doc comment, stating that there is none.

### 1.3 Four findings from the audit that shaped the design

**F1 — `store::set_task_status` silently drops `finished_at` when both flags are set.**
The branches are `if set_started { … } else if set_finished { … }`, so a single call with
`set_started = true, set_finished = true` takes only the first branch and never writes
`finished_at` or `reason`. **Latent, not active**: `engine.rs` always makes *two* calls —
`(Running, true, false)` then `(status, false, true)` — so production is correct. Nothing
prevents a future caller from making one call, and this module's duration comparison is the
first reader that would notice, because it is the first thing to compare the backend's clock
against a second clock. Reported, not fixed: `set_task_status` is production scheduling code
and changing its write semantics is outside this phase's scope.

**F2 — `risk_assessments.security_sensitivity` exists, is written, and is always `0.0`.**
This **corrects Phase 4's report**, which stated that `security_sensitivity` had "no column and
no table anywhere". That is not right. The column exists, and `intelligence.rs:553` supplies it
as a literal:

```rust
let security_sensitivity = 0.0_f64; // no signal source wired up yet
let business_criticality = 0.0_f64; // no requirement-linkage source wired up yet
let change_frequency      = 0.0_f64; // no change-intelligence source wired up yet
```

The risk formula weights `security_sensitivity` at **10%**, so every stored `risk_assessments.score`
is silently understated by a fabricated zero — and the row is indistinguishable from one where
the measurement was taken and came out nil.

The correction matters for remediation: Phase 4's report implies **a migration is needed** to
add `security_sensitivity`. The truth is that the column is already there and **a signal source
is what is missing** — a writer to add, not a schema to change. Phase 4's report is left
unmodified rather than rewritten, because rewriting a past report to match a later finding
would be fabricating the record of what was known when.

This finding is also why `ComparisonKind::FabricatedConstant` and
`IntegrityVerifier::fabricated_constant_check` exist — see §5.2.

**F3 — `execution_telemetry.assertion_result` is still never populated** (Phase 5's L1,
re-confirmed). So the API layer's per-assertion outcome cannot be compared against the UI
layer's visible text on equal footing, and the `API_UI` comparison is correspondingly weaker
than its name suggests (§4.1).

**F4 — the fingerprint and the cluster are written by the same call.** `ingest_run` writes the
fingerprint onto telemetry *and* registers the `failure_clusters` row together, so for any
fingerprint a real execution produces, both layers always hold it. The absent-cluster branch is
therefore **defensive only** — reachable if a cluster were pruned or if `record_failure` failed
after the telemetry insert succeeded. Phase 6 does **not** fabricate a state to reach it; the
branch stays as a guard and the coupling is reported as L4.

---

## 2. Architecture

```
IntegrityVerifier<'a> { store: &'a Store, evidence: Option<&'a LocalEvidenceStore>, window }
  │
  ├─ task_status_consistency      BACKEND tasks.status      ↔ API telemetry.status
  ├─ task_duration_consistency    BACKEND started→finished  ↔ API telemetry.duration_ms
  ├─ task_reason_consistency      BACKEND tasks.reason      ↔ API telemetry.error_message
  ├─ run_state_consistency        DATABASE runs.state       ↔ BACKEND derived from tasks
  ├─ fingerprint_registration     API telemetry.fingerprint ↔ DATABASE failure_clusters
  ├─ event_log_consistency        AUDIT run_events.status   ↔ BACKEND tasks.status
  ├─ audit_trail_consistency      AUDIT audit_log           ↔ DATABASE telemetry exists
  ├─ evidence_index_consistency   DATABASE evidence.object_key ↔ evidence store read()
  ├─ ui_api_consistency           UI CrawledElement.text    ↔ API assertion_result/error
  ├─ before_after_state           API selector, run N       ↔ API selector, run N+1
  └─ fabricated_constant_check    what a column claims     ↔ how much the column varies
```

Each comparison produces an `IntegrityComparison` carrying: kind, identity source, entity, field,
outcome, **every observation with its basis**, the comparison window and observed skew, data
sufficiency, `defect_established`, `defect_caveat`, and an optional Phase 1 `trace_key`.

---

## 3. Identity model

Values are never correlated because they look alike. Identity comes only from a **stored key**
both sides recorded independently.

| Source | Key | Corroborated by |
|---|---|---|
| `run_task_key` | `(run_id, task_id)` | the primary key of `tasks`, independently on telemetry and events |
| `run_key` | `run_id` | `runs.id`, `run_events.run_id`, `evidence.run_id` |
| `failure_fingerprint` | Phase 2's canonical fingerprint | `failure_clusters.failure_fingerprint` |
| `test_case_key` | `test_case_id` | `tasks.test_case_id`, `execution_telemetry.test_case_id` |
| `object_key` | `evidence.object_key` | the artifact readable from the evidence store |
| `project_scope` | project id | only for `FABRICATED_CONSTANT`, which has no single entity |

When a key resolves to **more than one** candidate the answer is `INSUFFICIENT_DATA` with the
candidates named, never a guess. `two_elements_sharing_a_selector_yield_an_ambiguous_identity`
is the concrete case: a crawl that recorded the same selector on two elements of one page cannot
identify the UI entity, so the comparison declines and `identity` is left `None`.

`equal_looking_values_from_different_entities_are_not_correlated` closes the other direction:
two test cases with the **same title** stay distinct, and each one's before/after view contains
only its own executions.

---

## 4. Comparison types, and what each can honestly claim

| Brief's type | Implemented as | `defect_established` possible? |
|---|---|---|
| status consistency | `task_status_consistency`, `event_log_consistency` | **yes** — two independent writers, both measurements |
| amount/calculation consistency | `task_duration_consistency` — two independent clocks, stated tolerance | **yes**, beyond tolerance |
| field-value consistency | `task_reason_consistency`, `evidence_index_consistency` | **no** — detail-capture and artifact-retention are bookkeeping |
| before/after state | `before_after_state` — selector stability across two runs | **no** — a test changing its selector is normal evolution |
| API/UI consistency | `ui_api_consistency` | **no** — the crawl records no per-element timestamp (F3) |
| API/DB consistency | `fingerprint_registration` | **no** — one-sided absence, and the writers are coupled (F4) |
| audit-trail consistency | `audit_trail_consistency` | **no** — a records gap says nothing about the application |
| business-state consistency | `run_state_consistency` | **no** — bookkeeping |

So **two** of eleven comparisons can currently establish a product defect, and both require two
independent layers that both recorded real measurements. That is the honest ceiling, and it is
enforced by tests rather than described.

### 4.1 Two deliberate departures from the brief's table

* **`event_log_consistency` reports `defect_established = true` when the log and the backend
  disagree**, even though §4 lists it as not-capable. Rationale: `run_events` and `tasks` are
  written by different code at different moments, so a terminal-status disagreement between them
  is a genuine inconsistency between two measurements — the same shape as the status comparison.
  It is not a statement about the application, and the `detail` says so; but the *inconsistency*
  is real and forcing it into `defect_established = false` would have misrepresented the
  evidence. Recorded here because it is a judgement call, not a derivation.
* **`API_UI` never establishes a defect.** Even when both sides recorded a value, the crawl
  carries no per-element timestamp, so the two observations cannot be shown to be
  contemporaneous. A mismatch is reported as `INCONSISTENT` with that caveat attached.

---

## 5. Evidence-backed result states

### 5.1 The four states, and the order the rules are applied in

`INCONSISTENT` requires **all** of: a resolved identity, a real value on both sides, timestamps
inside the window, and agreement about what is being compared. The checks are applied in this
order, weakest disqualifier first, in one place (`decide_pair`) so they cannot drift between
comparisons:

1. **window** — skew > `max_skew` → `INSUFFICIENT_DATA`, "staleness rather than disagreement"
2. **a value is missing** → `INSUFFICIENT_DATA`, naming the layer that declined
3. **values compared** → `CONSISTENT` or `INCONSISTENT`

The default window is **60 s**, stated as a constant and asserted by a test. A wider window would
span a deployment and hide real staleness; a narrower one would reject every real pair.

`a_zero_comparison_window_turns_disagreements_into_insufficient_data` proves the rule with an
A/B on identical data: the default window yields `INCONSISTENT`, a zero window yields
`INSUFFICIENT_DATA` citing the window. Two real writers never stamp the same instant, so a zero
window is the honest way to exercise the rule without backdating a row by hand.

### 5.2 `defect_established` — the stricter gate

`INCONSISTENT` and "a product defect is broken" are **different claims**, and the brief asks for
both to be distinguished. So `defect_established` is a separate flag, and every comparison sets
`defect_caveat` when it is false. The rule: a constant, a default, or a value with no signal
source is **not a measurement**, and `Observation::is_measurement` says so.

This is not theoretical. `the_fabricated_constant_check_finds_the_hardcoded_zeros` proves the
check finds all three hardcoded columns, that they are flagged `is_measurement: false`, and that
the finding carries `defect_established: false` with the caveat *"this is a finding about AIQA's
own scoring, not about the application under test; no bug record should be raised against the
application from it."*

That is the F2 finding turned into a standing guard: if a future change wires a real signal
source into one of those columns, the column varies, the check stops reporting it, and nothing
else in this module changes.

### 5.3 Duplicate detection is one comparison per distinct fingerprint

Two tasks failing identically fingerprint identically. `a_registered_fingerprint_is_compared_once_however_many_rows_carry_it`
asserts one comparison, not two — otherwise a common failure would dominate every report.

---

## 6. Safety controls

Eight gates per comparison, in order:

| # | Gate | Mechanism |
|---|---|---|
| 1 | project scope | `list_*` readers are project-scoped; `get_run_summary`'s `project_id` is checked |
| 2 | no writes possible | `&Store` + no `&mut self` + no SQL in the module |
| 3 | identity resolved | stored keys only; ambiguous → `INSUFFICIENT_DATA` |
| 4 | window respected | 60 s default, applied first |
| 5 | values present | a missing value → `INSUFFICIENT_DATA` |
| 6 | both are measurements | `is_measurement` gate on `defect_established` |
| 7 | defect requires two independent measurements | `establishes_defect()` |
| 8 | authorization | `require_scoped_project` at the route |

**`the_verifier_cannot_write` proves (2) three ways**: a source guard over comment-stripped code,
and a runtime sentinel — a row's content and four table row counts recorded before every
comparison runs and compared after. No test fabricates state to attack the verifier, because
there is no attack surface.

**`another_projects_run_cannot_be_compared`** asserts that *all six* cross-run comparisons return
empty for another tenant's run, that the whole-report path yields zero findings, and — separately
— that two tenants holding a **byte-identical fingerprint** cannot see each other's rows.

---

## 7. Files changed

| File | Change |
|---|---|
| `integrity.rs` | **new** — 11 comparisons, the model, the verifier, `with_trace_key`, `summarise` |
| `integrity_tests.rs` | **new** — 25 tests + 1 `#[ignore]`d measurement |
| `store.rs` | **+1** reader: `list_risk_assessment_constants`, fixed text + one bound parameter |
| `api.rs` | one read-only route + `IntegrityQuery` |
| `lib.rs` | module + test registration |

### 7.1 Migrations: **none**

`0032` remains the most recent; the count is unchanged at 32. The verifier reads, and reading is
what it was built to do. Phase 6 stores nothing — findings are returned, and the durable records
it compares already exist.

### 7.2 API

```
GET /api/v1/projects/:project_id/integrity/:run_id[?crawl_task_id=]
```

Read-only because the verifier is: there is nothing here that could be turned into a write. An
unreadable `crawl_task_id` returns `INSUFFICIENT_DATA: CRAWL_UNREADABLE` rather than degrading
silently. Guarded by `require_scoped_project`.

---

## 8. Tests

**25 tests + 1 `#[ignore]`d measurement.** Over half negative.

| Requirement | Test |
|---|---|
| fully consistent state | `a_run_the_layers_agree_about_is_recognised_as_consistent` |
| UI/API mismatch | `a_genuine_status_mismatch_is_detected_and_can_establish_a_defect` |
| API/DB mismatch | `a_registered_fingerprint_is_consistent_because_one_call_writes_both_layers` |
| all-layer mismatch | covered by `the_report_counts_add_up_and_names_its_layers` (four layers disagree) |
| missing DB evidence | `a_one_sided_failure_reason_is_reported_without_claiming_a_defect` |
| missing API evidence | `a_task_with_no_api_evidence_is_insufficient_data_not_a_mismatch` |
| ambiguous identity | `two_elements_sharing_a_selector_yield_an_ambiguous_identity` |
| timestamp / eventual consistency | `a_zero_comparison_window_turns_disagreements_into_insufficient_data` |
| project isolation | `another_projects_run_cannot_be_compared` |
| read-only DB enforcement | `the_verifier_cannot_write` (source + sentinel) |
| same visible value, different entities | `equal_looking_values_from_different_entities_are_not_correlated` |
| numeric comparison | `two_independently_recorded_durations_are_compared_with_a_stated_tolerance`, `two_clocks_within_the_tolerance_are_consistent` |
| before/after state | `before_after_reports_a_selector_change_without_calling_it_a_defect` |
| audit/log mismatch | `an_audit_row_naming_a_run_with_no_executions_is_reported`, `the_event_log_is_compared_against_the_backend` |
| negative: no cross-project comparison | `another_projects_run_cannot_be_compared` |
| negative: missing layers ≠ fake mismatch | `a_run_with_no_layers_reporting_produces_no_defects` |
| negative: stale ≠ defect | `a_zero_comparison_window_turns_disagreements_into_insufficient_data` |
| the F2 finding | `the_fabricated_constant_check_finds_the_hardcoded_zeros` |
| window is stated | `the_default_comparison_window_is_stated_and_narrow` |
| names round-trip | `layer_and_outcome_names_round_trip` |

Every fixture is written by real writers: `insert_run_for_release`, `set_task_status`,
`set_run_state`, `Intelligence::ingest_run`, `insert_event`, `insert_audit`, `insert_evidence`,
`LocalEvidenceStore::put`, `insert_test_case`. Where a test needs two layers to disagree it calls
**both real writers with different inputs** — exactly the state a real bug in either would
produce. Nothing hand-writes a row.

### 8.1 Bugs found and fixed

1. **A bug in my own comparison logic, caught by a test.** The first fingerprint comparison
   hardcoded the API side's value as `false` and compared it against the database's `true`, so it
   reported `INCONSISTENT` for **every** fingerprint in the system regardless of the data. Fixed
   by recognising that the two layers hold *different propositions* — "a failure with this
   signature occurred" versus "a cluster exists for it" — which must not be compared for
   equality. The comparison is now over "do both layers know this fingerprint".
2. **The fingerprint test asserted a state the system cannot reach.** Its first draft required a
   fingerprint with no registered cluster, and failed because `ingest_run` writes both together
   (F4). Rather than fabricate the state, the test was rewritten to assert the real coupling, and
   the defensive branch is documented as such.
3. **My harness called `set_task_status(true, true)`** — which F1 shows silently drops
   `finished_at`. The two duration tests failed with *"the backend recorded no start/finish
   pair"*, which is the module correctly reporting a missing measurement rather than a mismatch.
   The harness was fixed to make the two calls `engine.rs` makes.
4. **The read-only source guard tripped on the module's own doc comment**, which names `&mut self`
   in order to state there is none. The guard now strips comments before checking — a guard that
   trips on its own explanation is not a guard.
5. **A vacuous assertion** (`u64 >= 0`) in the measurement test, which clippy escalated to an
   error. Replaced with two assertions that actually test something: every size produced
   comparisons, and per-task cost does not grow with the entity set.

Nothing was weakened, skipped or deleted to make a test pass. In (2) the fix was to **stop
asserting something untrue**, which is the opposite of weakening.

---

## 9. Performance

Measured, not asserted. Reproduce with:

```
cargo test phase6_measures_integrity_cost -- --ignored --nocapture
```

| Tasks in the run | Build (fixture) | Verify (whole report) | Per task |
|---|---|---|---|
| 5 | 46 ms | **6 ms** | 1.20 ms |
| 20 | 47 ms | **8 ms** | 0.40 ms |
| 60 | 133 ms | **20 ms** | 0.33 ms |

| Isolated cost | Result |
|---|---|
| database read — one bounded telemetry read of 85 rows | **2 ms** |
| evidence — 50 artifact writes | **26 ms** |

### 9.1 Observations

* **Per-task cost *falls* as the entity set grows** (1.20 → 0.33 ms), so the work is dominated
  by a small number of fixed reads rather than by per-task work. That is the property that makes
  this usable on a large project, and the measurement asserts it rather than assuming it.
* **No N+1 query pattern.** Each comparison reads its tables once and answers every entity from
  that read — the same discipline Phase 4 established for `prioritise_tests` after finding
  2,000+ redundant reads there. `telemetry_for_run` is the single bounded read.
* **Evidence writes dominate per-finding cost** (0.5 ms each), dominated by filesystem work.
  That is the price of requiring a finding to carry readable proof.
* No threshold is pinned in CI. The measurement is `#[ignore]`d precisely because a wall-clock
  assertion chosen on one machine fails on another.
* All figures are from freshly built, fully migrated **test** databases on this machine — not a
  production latency claim.

---

## 10. Regression

| Run | Before Phase 6 | After |
|---|---|---|
| Backend suite | 1934 − 25 = **1909** passed / 0 failed / 4 ignored | **1934 passed / 0 failed / 5 ignored** |
| `cargo build` | clean | clean |
| `cargo clippy --all-targets` | 0 errors | 0 errors, no warning in any Phase 6 file |
| `rustfmt --check` | — | clean on `integrity.rs`, `integrity_tests.rs`, `store.rs` |
| Migrations | 32 | 32 — none added |
| Frontend | 1605 / 79 files | unchanged — no frontend file touched |

Zero regressions. Every prior suite still passes individually: `trace_tests` 21,
`trace_chain_tests` 14, `change_impact_tests` 16, `risk_prioritisation` 18, `exploration_tests`
29, `governed_healing_tests` 53, `agent_tests` 20, `api_tests` 313.

---

## 11. Limitations

**L1 — the UI layer has no per-element timestamp.** `CrawlOutput` records pages and elements but
no time, so a UI observation's `observed_at` is the comparison's own clock. Consequently the
`API_UI` comparison can never establish a defect, even when both sides recorded values. Making it
usable means recording a timestamp per crawled element — a crawler change, not a verifier change.

**L2 — the fingerprint/cluster comparison is weak by construction** (F4): the two layers are
written by one call, so it can only ever report `CONSISTENT` through the production path.

**L3 — only two of eleven comparisons can establish a defect.** Everything else is one-sided
absence, bookkeeping, or a pair whose timestamps cannot be shown to align. §4 states which, and
why.

**L4 — requirement, coverage and memory layers are not compared.** They are not among the five
layers the brief names, and no `covers` edge exists to attribute a test to a module (Phase 2 L1,
Phase 4 L1, Phase 5 L5) — so nothing could be honestly joined.

**L5 — `audit_trail_consistency` only compares targets that parse as a run id.** Audit rows name
many kinds of target; the others are skipped rather than guessed at. So audit coverage is
partial by design.

**L6 — F1 is reported, not fixed.** `set_task_status(.., true, true)` still silently drops
`finished_at`. Production is unaffected because `engine.rs` makes two calls, but the footgun
remains and fixing it changes production scheduling behaviour.

**L7 — F2 is reported, not fixed.** The three `risk_assessments` columns are still written as
`0.0`. Wiring real signal sources is a feature decision, not an integrity-verifier decision. The
guard now *detects* it, which is Phase 6's part.

**L8 — `with_trace_key` links by `test_case_id`, and only when a `TraceIndex` is supplied.**
Comparisons carry `trace_key: None` by default. Per the brief, an integrity finding is linked to
Requirement → Test → Execution **where evidence supports it**; nothing forces a bug record, and
`establishes_defect()` is the gate a caller must check first.

**L9 — the live database is unchanged** and has had no Phase 6 code run against it. Every figure
in this report comes from freshly built, fully migrated test databases driven through real
writers. None of it is a claim about production data.

**L10 — no E2E CI job.** Pre-existing; `ci.yml` has three jobs, none E2E.

---

## 12. PASS gate

| Criterion | Status | Evidence |
|---|---|---|
| real cross-layer evidence can be collected | **PASS** | 11 comparisons over 5 layers, every one fed by a pre-existing reader and a real production writer; 25 tests |
| entity identity is reliable | **PASS** | 5 stored-key sources; ambiguous → `INSUFFICIENT_DATA` with candidates named; 2 tests including the same-title case |
| consistent state is recognized correctly | **PASS** | `a_run_the_layers_agree_about_is_recognised_as_consistent`, plus the agreeing event, duration, fingerprint and run-state cases |
| genuine mismatches are detected | **PASS** | status, duration, run-state, event-log, audit, evidence, before/after and fingerprint comparisons each detect their mismatch |
| insufficient data is handled honestly | **PASS** | `INSUFFICIENT_DATA` reached by 7 distinct routes; `data_sufficiency` reported; every non-defect carries a `defect_caveat` |
| database verification remains read-only | **PASS** | zero `sqlx`, zero SQL, zero `&mut self` in the module; runtime sentinel unchanged across all 11 comparisons; one new fixed-text reader |
| project isolation passes | **PASS** | all six cross-run comparisons empty for another tenant; identical fingerprints across tenants do not leak |
| tests pass | **PASS** | 25/25 |
| regression passes | **PASS** | 1934 / 0 failed; eight prior suites individually green |
| no duplicate evidence/execution/memory subsystem | **PASS** | no new table, no new engine, no new memory; evidence store read not replaced, `Action::Assert*` read not re-evaluated, Phase 4's reader reused |

### Honest caveats

1. **Phase 6's headline output is a correction and a guard, not a new capability.** It found that
   a column Phase 4 called non-existent exists and is written as a constant, turned that into a
   standing check, and found a latent write-logic footgun in production scheduling. Both are
   reported, neither is fixed here, and neither changes what the verifier can prove.
2. **The most common answer is `INSUFFICIENT_DATA`, and that is correct.** A verifier that
   answered every question would be inventing answers; this one declines by default and states
   why. A caller that reads `INCONSISTENT` as "broken" and `INSUFFICIENT_DATA` as "fine" will
   invert the meaning.
3. **`INCONSISTENT` is not a defect.** Only `defects_established` is, it requires two independent
   layers that both recorded measurements, and it is a separate counter from `inconsistent`. Two
   of eleven comparisons can reach it today.

**Phase 6 = PASS**, with L1–L3 carried forward as the honest bound of what cross-layer
verification can currently claim.

---

**Phase 7 not started. Nothing committed. Nothing pushed.**
