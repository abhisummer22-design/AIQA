# Phase 9.2B — Pre-Batch 7 Safety Audit

**AUDIT ONLY. No code, test, schema or artifact was changed.**

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — unchanged, nothing committed

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 28 / 34 / **0** / 0 |
| `.tmp*` directories | **1,800** |
| `C:` free | **52.94 GB** |

Untracked rose from 33 to 34 because of the previous audit's report. Nothing else in the tree
changed.

### 1.1 Confirmation that nothing was modified

This task is read-only. No source file was opened for writing. No `pool.close()` was added, moved or
removed. No artifact directory was deleted. No cleanup script was run. The full backend suite was not
run.

Artifacts rose 1,800 → 2,240 during this audit **purely as a measurement side-effect** of running the
six autopilot suites in Part B (free disk 52.94 → 51.33 GB). No cleanup was issued.

---

## Part A — Classification of all unclassified `pool.close()` sites

### A.1 Scope correction

The inventory listed **6** files as unclassified. Source enumeration found **8** files containing
**17** sites. The two extra files (`change_impact_tests.rs`, `adaptive_intelligence_tests.rs`) were
present in the inventory's own scan output but not carried into its ⚠ table.

A full enumeration of the crate finds **247** pool-closure statements across **18** files. Of those,
230 are cleanup added by Phases 9.2A–9.2B Batch 6. The remaining **17** are below.

### A.2 Classification

**All 17 sites are `INTENTIONAL_FAULT_INJECTION`. None is teardown. None may be moved or removed.**

| # | file | line | owning test | receiver | operation immediately before | asserted after |
|---|---|---|---|---|---|---|
| 1 | `journey_tests.rs` | 1964 | `journey_f_end_to_end_data_integrity` | `store` | a healthy `verify_run` reported `is_complete()` | verifier must **refuse** rather than return an empty clean report |
| 2 | `journey_tests.rs` | 2400 | `a_failed_read_mid_journey_does_not_report_the_journey_as_complete` | `store` | `ingest_run` succeeded ("the journey is half done. Now the store fails.") | every subsequent read errors; journey must not be "complete" |
| 3 | `integration_e2e_tests.rs` | 1540 | `s6c_a_storage_failure_is_never_a_measured_zero_or_an_empty_history` | `store` | `test_risk` returned `Ok` and a view | `test_risk` must error, not report zero/empty history |
| 4 | `integration_e2e_tests.rs` | 2172 | `s9a_a_closed_store_fails_loudly_on_every_dependent_consumer` | `store` | asserted "the store must be healthy before it is closed" | every dependent consumer must fail loudly |
| 5 | `integration_e2e_tests.rs` | 2253 | `s9b_a_write_failure_is_reported_and_leaves_prior_state_intact` | `store` | `list_execution_telemetry` returned exactly 1 row | a further execution must fail rather than half-write |
| 6 | `integration_e2e_tests.rs` | 2264 | same test (repeat) | `store` | the write failure already induced | second close, commented "already closed; idempotent" — same purpose |
| 7 | `phase_7_1_regression_tests.rs` | 295 | `p71_d1_a_closed_pool_fails_the_verification_rather_than_reporting_clean` | `store` | a report from readable storage was complete | `verify_run` must report read failures, not a clean report |
| 8 | `phase_7_1_regression_tests.rs` | 329 | `p71_d1_every_required_comparator_propagates_a_failed_read` | `store` | verifier built with an evidence store | every comparator must propagate the failure |
| 9 | `phase_7_1_regression_tests.rs` | 710 | `p71_d1_the_api_does_not_convert_a_storage_failure_into_a_successful_report` | `state.store` | HTTP 200 with `read_failures` empty | the endpoint must not answer 200 |
| 10 | `change_impact_tests.rs` | 1025 | `a_storage_failure_is_reported_rather_than_returning_an_empty_selection` | `store` | "a healthy store must still produce an analysis" | `analyse` must error, not return an empty selection |
| 11 | `change_impact_tests.rs` | 1063 | `a_storage_failure_cannot_silently_empty_the_safety_floor` | `store` | floor empty "for a real reason" | `safety_floor` must error, not empty map |
| 12 | `change_impact_tests.rs` | 1089 | `a_storage_failure_cannot_silently_empty_the_high_risk_tier` | `store` | "no scored cases yet" | `risk_scores` must error, not empty map |
| 13 | `phase_7_3_decision_tests.rs` | 1269 | `p73_error_propagation_and_genuine_empty_results_are_still_distinguishable` | `store` | an absent module returned `insufficient_data` | a *failed read* must be an error, distinct from empty |
| 14 | `qa_memory_risk_tests.rs` | 1062 | `r1b_a_failed_read_is_never_reported_as_no_executions` | **`store2`** | "with recorded executions a real view must be produced" | `test_stability` must not report "no executions" |
| 15 | `qa_memory_risk_tests.rs` | 1134 | `r1b_no_affected_view_reports_a_failed_read_as_an_empty_answer` | `store` | `release_risk_overview` succeeded | every affected view must error, not answer empty |
| 16 | `risk_prioritisation_tests.rs` | 1337 | `a_storage_failure_is_reported_rather_than_no_executions` | `store` | an empty project correctly reported `NO_EXECUTIONS` | `prioritise_tests` must error, not report no executions |
| 17 | `adaptive_intelligence_tests.rs` | 825 | `r1b_planning_feedback_reports_a_failed_read_rather_than_no_signals` | `store` | "no history must still produce an honest empty signal list" | `generate_planning_feedback` must error |

### A.3 Evidence used (not inferred from comments alone)

Each site shares a **three-part structural signature**, verified in the source:

1. a **healthy** read/write asserted `Ok` immediately before the close (so a later failure is
   attributable to the close alone);
2. the close itself;
3. an assertion immediately after that the operation **fails** — `is_err()`, `status.is_server_error()`,
   or a non-empty `read_failures` — with a message naming the distinction the test exists to protect
   ("an unreadable risk read must be an error, not an empty map that selects nothing").

Several also carry a phase comment (`// --- state 2: the store cannot answer ---`,
`// --- state 3: the store cannot answer ---`), and `integration_e2e_tests.rs:2171` states the
technique outright: *"`pool.close()` is the same real-failure technique `engine_tests` uses: no mock,
no sleep."*

### A.4 Answering the brief's per-site questions

| question | answer, uniformly across all 17 |
|---|---|
| Which pool is closed | the test's own `store`, except site 14 which closes `store2` — a **second, independent** `temp_store()` (its own tempdir, its own `q5.sqlite3`), not a second pool on one file |
| Is it teardown | **No** |
| Is it fault injection | **Yes** |
| Does the test assert a subsequent read/write fails | **Yes**, in every case |
| Would adding another close invalidate it | **No** — `close()` is idempotent (site 6 relies on this) |
| Would moving/removing it invalidate it | **Yes, fatally.** Removing the close lets the following read succeed, and the test's central assertion fails. This was empirically demonstrated for the `engine_tests` pair in Batch 5: with the closes removed, `d1_approval_…` and `d1_rejection_…` failed with *"approve_and_spawn must return Err when DB update fails"*. |
| Expected behaviour if removed | The "genuinely empty" / "no failures yet" / "healthy" result and the "failed read" result become indistinguishable — exactly the masking defect Phase 7.1 and these suites were written to prevent |
| Verdict | **Already correct — must remain unchanged** |

### A.5 Unresolved sites

**None.** All 17 classified with source-level evidence. No site requires an additional experiment.

---

## Part B — Autopilot fixture ownership

### B.1 `autopilot_test_support::fixture` — **confirmed leaking, and far larger than the inventory recorded**

The inventory recorded this as "0 call sites, ownership unclear". **That was wrong** — it searched
for the qualified path `autopilot_test_support::fixture`, but consumers import the symbol and call the
bare name. Searching the bare call finds **80 call sites**:

| consumer module | `fixture(` calls |
|---|---:|
| `autopilot_steps.rs` | 33 |
| `autopilot_generate.rs` | 28 |
| `autopilot_execute.rs` | 15 |
| `autopilot_report.rs` | 2 |
| `execution_plan.rs` | 1 |
| `integrity_tests.rs` | 1 (a local, unrelated helper) |
| **total** | **80** |

Construction (`autopilot_test_support.rs:39-88`):

```rust
pub(crate) async fn fixture(step: Step, config: AutopilotConfig, script: …) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::connect(&dir.path().join("t.sqlite3")).await.unwrap();
    …  store.insert_autopilot_run / start_autopilot_run / begin_autopilot_step …
    let ctx = StepCtx { run_id, project_id, step, config, detail, store, api, options };
    Fixture { ctx, api, _dir: dir }
}
```

Ownership: one `TempDir`, one `Store`, moved into `StepCtx`. `Store` is `Clone`, and `StepCtx` is
handed to step runners, so **clones can escape the fixture**. No teardown exists: `Fixture` has no
`Drop` and there is no `pool.close()` anywhere in these four modules.

**Measured (this audit):**

| suite | tests | delta |
|---|---:|---:|
| `autopilot_steps` | 21 | **31** |
| `autopilot_generate` | 19 | **20** |
| `autopilot_execute` | 21 | **14** |
| `autopilot_report` | 17 | **2** |
| **subtotal** | | **+67** |

**Verdict: CONFIRMED LEAKING, +67 per run, previously unrecorded.**

Background-work check for these four: **no `tokio::spawn`, `spawn_blocking`, `JoinHandle`, `engine::`
or `axum::serve` in any of them.** Steps are driven synchronously through `StepCtx`.

### B.2 `autopilot_runner::new` — **the inventory misidentified this**

The inventory listed `autopilot_runner::new` (L458) as an unmeasured fixture with 18 sites. Reading
the source:

```rust
// L441  #[cfg(test)]
// L442  mod tests {
// L457      impl Scripted {
// L458          fn new() -> Arc<Self> { Arc::new(Self::default()) }   // a MOCK, not a fixture
```

`Scripted::new()` builds a scripted test double and touches no database. It is not a fixture and was
never a leak candidate.

The **real** fixture in that file is `made_run(config: &Value) -> World` (L564), of which there are
**2** call sites:

```rust
struct World { store: Store, run: Uuid, _dir: tempfile::TempDir }
```

Ownership is the simple shape: one store, one tempdir, no service bundle.

**Measured: `autopilot_runner` delta 13.**

**Verdict: CONFIRMED LEAKING, +13 per run** — under the name `made_run`, not `new`.

### B.3 Background work in `autopilot_runner` — the significant risk

`autopilot_runner.rs` **production** code contains genuinely detached work:

```rust
pub fn spawn_run(state: &AppState, run: &AutopilotRunRow) -> bool {          // L405
    let worker = Worker { store: state.store.clone(), … };
    tokio::spawn(async move { drive_guarded(&worker, run_id).await });       // L416 — detached
    true
}
```

and `resume_runs` (L425) calls it. A task spawned here holds a `Store` clone past the caller's
control.

**However, the tests do not reach it.** Inside `mod tests`:
* `spawn_run` calls: **0**
* `resume_runs` calls: **0**
* `drive_guarded` calls: 2 — and `drive_guarded` **awaits** its own handle (`task.await`, L396)
* `drive(` calls: 15 — synchronous

So the detached path is unreachable from the test suite today. **Safe today.** This is flagged because
it is the one place in the non-api inventory where a future test that calls `spawn_run` would
silently acquire a detached pool handle.

---

## Part C — Background-work analysis for the unverified suites

### C.1 A discrepancy in the brief and the inventory

The brief refers to "the four unverified suites". The inventory's §8 table actually lists **five**:
`store_tests`, `autopilot_store`, `autopilot_runner`, `integration_e2e_tests`, `journey_tests`.
All five are analysed here, plus the four newly surfaced autopilot suites.

### C.2 Results

| suite | background work in the test module | in production code it calls | can a pool handle outlive the fixture? | teardown strategy |
|---|---|---|---|---|
| `store_tests` | **none** | none | **no** | simple awaited close |
| `integration_e2e_tests` | **none** | `integrity.rs`, `qa_memory.rs` — none | **no** | simple awaited close |
| `journey_tests` | **none** | `integrity.rs`, `qa_memory.rs` — none | **no** | simple awaited close |
| `autopilot_store` | 1 × `tokio::spawn` at L1250, **inside `cfg(test)`**, capturing `store.clone()` | — | **no** — the handle is `await`ed at L1263 | simple awaited close |
| `autopilot_runner` | none reaching production spawns | `spawn_run` (L405) is detached with `state.store.clone()` | **no** — tests call `drive`/`drive_guarded`, both awaited | simple awaited close |
| `autopilot_steps` / `generate` / `execute` / `report` | none | none found | **no** | simple awaited close |

### C.3 The one case that deserves a written note

`autopilot_store.rs:1242` `cancelling_a_run_waits_for_another_writer_instead_of_failing_with_database_is_locked`:

```rust
let writer = store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
let cancelling = tokio::spawn({ let store = store.clone(); async move { store.cancel_autopilot_run(id).await } });
tokio::time::sleep(Duration::from_millis(200)).await;
if cancelling.is_finished() { panic!("the cancel did not wait for the write lock: …"); }
writer.commit().await.unwrap();
assert_eq!(cancelling.await.unwrap().unwrap(), CancelOutcome::Requested);
```

Three things follow for a future Batch:

1. The spawned task **is awaited** (L1263) — it does not outlive the test.
2. The 200 ms sleep is **the mechanism under test**, not a substitute for a completion signal: the
   assertion is that the task is *still running* while the writer holds the SQLite lock. Replacing it
   with a poll would destroy the test. It should be left exactly as it is.
3. A `begin_with` transaction holds a **checked-out connection** until `commit()` at L1261. The awaited
   `pool.close()` must be appended **after** that commit, i.e. as the last statement of the test —
   the same rule as every other family.

**No test in this group relies on an arbitrary sleep standing in for completion**, and no test-only
correction is needed: all five are safe to fix with a plain appended awaited close.

---

## Part D — Batch 7 implementation plan

### D.1 Recount from current source

| file | helper | call sites | tests | existing closes |
|---|---|---:|---:|---:|
| `qa_memory_retrieval_tests.rs` | `temp_store` | **20** | 21 | 0 |
| `autopilot_store.rs` | `temp_store` | **20** | 17 | 0 |
| `qa_memory_risk_tests.rs` | `temp_store` | **20** | 21 | **2 (fault injection)** |
| `risk_prioritisation_tests.rs` | `temp_store` | **20** | 21 | **1 (fault injection)** |
| **total** | | **80** | | **3 (must be preserved)** |

The inventory's "~80 call sites" estimate is **confirmed exactly**.

### D.2 Per-family plan

All four share the proven shape: `temp_store() -> (Store, tempfile::TempDir)`, one pool, one tempdir,
uniform binding `let (store, _d) = temp_store().await;`.

#### 1. `qa_memory_retrieval_tests::temp_store` — 20 sites, measured +21

* DB file: one per fixture. **Store instances per fixture: 1.**
* Fault injection: **none**. Background work: **none**.
* Teardown coverage today: **none** — 0 closes in the file.
* Smallest change: append `store.pool.close().await;` as the last statement of each of the 20 tests.
* Focused tests: fresh DB + migrations; close removes dir despite WAL/SHM; repeated ×10 leaves 0;
  parallel isolation; fallible `TempDir::close()` observable; a second-pool guard is **not** needed
  (single store).
* Regression: `qa_memory_tests` (Batch 6 sibling), `phase_8_quality_tests`, `phase_9_readiness_tests`.

#### 2. `autopilot_store::temp_store` — 20 sites, measured +21

* DB file: one per fixture. **Store instances per fixture: 1.**
* Fault injection: **none**.
* Background work: one `cfg(test)` `tokio::spawn` in a **single** test (L1250), which **is awaited**.
  See §C.3 — the close must come after `writer.commit()`.
* Teardown coverage: **none**.
* Smallest change: append the close to all 20; add a comment at L1266 documenting that the awaited
  close must follow the commit.
* Focused tests: as above, plus one asserting the spawned cancel task is awaited before teardown.
* Regression: `autopilot_runner`, `autopilot_steps`, `autopilot_generate`, `autopilot_execute`.

#### 3. `qa_memory_risk_tests::temp_store` — 20 sites, measured +19

* DB file: one per fixture (`q5.sqlite3`). **Store instances per fixture: 1 — except one test
  (L1036) that binds a *second independent* `temp_store()`** producing its own tempdir and its own
  `q5.sqlite3`. Not a restart pattern; simply two fixtures, each needing its own close.
* Fault injection: **2 sites, at L1062 (`store2`) and L1134 (`store`) — both classified
  `INTENTIONAL_FAULT_INJECTION` in §A.2. These two tests must gain a *final* close while keeping the
  mid-test one in place.**
* Background work: **none**.
* Smallest change: append a close to all 20 tests. For the two fault-injection tests, append a
  **second** close after the final assertion (the mid-test close stays).
* Regression: `qa_memory_tests`, `qa_memory_retrieval_tests`, `risk_prioritisation_tests`.

#### 4. `risk_prioritisation_tests::temp_store` — 20 sites, measured +19

* DB file: one per fixture (`phase4-risk.sqlite3`). **Store instances per fixture: 1.**
* Fault injection: **1 site, L1337 — classified `INTENTIONAL_FAULT_INJECTION`.** Same treatment: keep
  it, append a final close.
* Background work: **none**.
* Regression: `qa_memory_risk_tests`, `qa_memory_tests`.

### D.3 Shared validation plan

For each family, exactly the protocol proven in Batches 5 and 6:

1. Baseline `.tmp` delta, then **per-test attribution** so a residual is attributable.
2. Append awaited closes; compile; 10 consecutive runs recording exit code, counts, delta, panics.
3. Mutation: delete the appended closes; **verify it applied** (sha changed, correct removal count,
   fresh `Compiling`, binary newer than source); confirm the leak returns; restore byte-identically.
4. A dedicated lifecycle test proving required writes land **before** the close (per §C.3 property
   E of every batch since 9.2A).
5. `cargo fmt --check` and Clippy, with the multiset check against a pre-batch snapshot.
6. Confirm the fault-injection tests still pass **for their original reason** — not merely green.

### D.4 Can the four be fixed together?

**Yes — with one caveat.**

* All four share the identical fixture shape, identical binding, single store, no service bundle, no
  restart pattern. The Batch 6 template applies unchanged.
* Only `qa_memory_risk_tests` and `risk_prioritisation_tests` contain fault-injection closes, and
  those are handled by *adding* a close rather than touching the existing one. That is a
  two-line-per-test difference, not a design difference.
* `autopilot_store` needs one extra comment at the lock-contention test (§C.3).

**Recommendation: fix all four in one batch**, with an explicit step in the batch checklist —
"confirm the 3 fault-injection sites are unmodified and their tests still fail a read for the
original reason". Splitting them would cost two extra mutation/restore cycles for no safety gain.

---

## Part E — Reconciled inventory

### E.1 What changed since the inventory report

| change | cause |
|---|---|
| `autopilot_test_support::fixture`: **0 → 80 sites, +67 measured** | inventory searched a qualified path; consumers import the bare symbol |
| `autopilot_runner::new` → **`made_run`, 2 sites, +13 measured** | inventory misidentified `Scripted::new()` (a mock) as a fixture |
| **17 sites classified** as `INTENTIONAL_FAULT_INJECTION` | Part A |
| Remaining total **+660 → +740** | the two corrections above |

### E.2 Reconciled counts — kept as distinct quantities

| quantity | value |
|---|---|
| Unique fixture helpers already fixed | **8** (verified delta 0) |
| Call sites already fixed | **193** |
| Unique fixture helpers remaining | **34** |
| Call sites remaining (source-counted) | **~610** (up from ~530) |
| Tests using those call sites | not independently counted; suite test totals are in the inventory |
| **Measured remaining leakage per full run** | **+740** |
| Measured suites confirmed not leaking | 1 (`qa_memory_migration_tests`) |
| Intentional fault-injection closures | **21** (4 already known: 2 `engine_tests`, 2 `api_tests`; **17 newly classified**) |
| Unmeasured families | 0 remaining — every remaining family now has a measurement |

**These three numbers must not be conflated.** `+740` is a per-run directory count, not a call-site
count. ~610 is a source count, not a leakage estimate. **No completion percentage is computed from
these**, because the denominator for "all fixture families" is not independently established.

### E.3 Overlap in per-suite deltas

Every figure in §E.2 is a **per-suite whole-suite delta**. Two overlaps exist and are not netted out:

* `api_tests` — 6 fixture families and 4 inline fixtures share **one** delta of **+308**. It cannot
  be decomposed per family without per-test attribution across 316 tests.
* `autopilot_*` — `autopilot_store` (21) and `autopilot_runner` (13) are distinct suites with distinct
  deltas; `autopilot_steps`/`generate`/`execute`/`report` share `autopilot_test_support::fixture` but
  are separate suites, so their deltas (31/20/14/2) are **not** double-counted. Summing them is valid.

### E.4 Corrected batch sequence

The inventory's sequence stands, with the four autopilot suites inserted and `autopilot_runner`
renamed.

| batch | content | sites | measured |
|---|---|---:|---:|
| **7** | `qa_memory_retrieval_tests`, `autopilot_store`, `qa_memory_risk_tests`, `risk_prioritisation_tests` | 80 | 80 |
| **8** | `store_tests`, `trace_tests`, `change_impact_tests`, `qa_memory_planning/perf/validation_tests` | 93 | 96 |
| **9** | `process_lifetime_maps_tests`, `key_scope_tests`, `ws_tests`, `rbac_matrix_tests`, `run_map_retention_tests` | 35 | 42 |
| **10** | `trace_chain_tests`, `risk_signal_tests`, `risk_semantics_tests`, `rbac_probe_tests` | 16 | 30 |
| **11** | `phase_7_1_regression_tests`, `phase_7_3_decision_tests`, `pre_phase_8_tests`, `phase_7_4_decision_tests` | 77 | 45 |
| **12** | `integration_e2e_tests`, `journey_tests`, `operations_tests`, `adaptive_intelligence_tests`, `retention_and_metrics_tests` | 91 | 59 |
| **13 (new)** | `autopilot_steps`, `autopilot_generate`, `autopilot_execute`, `autopilot_report` (shared `autopilot_test_support::fixture`), `autopilot_runner` (`made_run`) | 82 | 80 |
| **14** | `api_tests` — dedicated AppState ownership batch | 311 | 308 |

Batches 7–13 total **485 sites / ~477 measured**, leaving `api_tests` last so the closing full-suite
artifact check runs against a repository with no known leak.

### E.5 Remaining API-test ownership risks (unchanged, restated)

1. **7 `reloaded` stores** on one `api.sqlite3`, none closed (L11020, 11064, 11127, 11308, 11478,
   11621, 21740) plus scoped-block stores at L11868/11888/11955.
2. **2 fault-injection closes** to preserve (L12413, L12862).
3. **6 fixture families, 311 sites** — the earlier inventory's 182 counted only three of them.
4. `AppState.runs` holds `Arc<RunRecord>`; safe today because no api test starts a run, but it is the
   property to re-check if one ever does.
5. The `/health` server thread holds no store and is not a teardown risk.

---

## 10. Unresolved safety blockers

| # | blocker | status |
|---|---|---|
| 1 | `api_tests` second pools and 6-family teardown | **open** — needs the Batch 14 ownership plan |
| 2 | Per-family attribution inside `api_tests`' single +308 | **open** — would require attributing 316 tests |
| 3 | `autopilot_test_support::fixture` clones escaping via `StepCtx` | **noted, not blocking** — no background work in any consumer; a close at test end still covers the pool |
| 4 | Batch 7 fault-injection preservation | **resolved** — 3 sites classified; procedure defined in §D.4 |
| 5 | Background work in the previously "unverified" suites | **resolved** — none of the five can strand a pool handle |
| 6 | Any site that cannot be safely fixed test-only | **none found** |

---

## 11. Exact recommendation for the next batch

**Proceed with Batch 7 as scoped in Part D** — four families, 80 call sites, all single-store, no
background work, three fault-injection sites to preserve.

Batch 7's checklist must open with:

1. **Record that the 3 fault-injection closes at `qa_memory_risk_tests.rs:1062`, `:1134` and
   `risk_prioritisation_tests.rs:1337` must be byte-identical at the end of the batch.** They are the
   single most likely thing to damage, because a bulk "add a close everywhere" edit touches the same
   lines.
2. Per-test attribution before and after, not just suite totals — three of the four suites have
   site/delta mismatches (20 sites vs 21/21/19/19).
3. At `autopilot_store.rs:1266`, append the close **after** `writer.commit()` and add a comment
   recording why the 200 ms sleep is the mechanism under test rather than a completion substitute.

Do **not** extend Batch 7 to the autopilot suites. They share one helper across four modules and
should be Batch 13.

---

*Prepared by audit only. No production code, test, fixture, migration or route was modified. No
`pool.close()` was added, moved or removed. No artifact directory was deleted. No cleanup script was
run. The full backend suite was not run. Nothing was committed or pushed.*