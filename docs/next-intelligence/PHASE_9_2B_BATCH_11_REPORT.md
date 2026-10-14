# Phase 9.2B — Batch 11 Report

## Pre-Phase-8 and Phase 7.1 / 7.3 / 7.4 Regression Fixtures

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — nothing committed, nothing pushed
**Verdict: PASS**

---

## 0. Headline numbers

| | |
|---|---:|
| Unique fixture helpers fixed | **7** across 4 files |
| Call sites fixed (teardown statements) | **49** |
| Tests owning a fixture | **49** of 53 |
| Measured leaks eliminated | **45** |
| Post-fix delta, all four suites | **0** |
| Fault-injection closes preserved | **4 / 4** |
| Repeated suite runs | **40** (10 per suite), all exit 0, all delta 0 |
| Individual per-test runs | **53**, all exit 0, all delta 0 |
| Files changed | 4 test files, **58 lines added, 0 removed, 0 changed** |
| Production files touched | **0** |

**49 ≠ 45** because four of those 49 tests close their pool mid-test as deliberate fault injection;
their directories were already removable and never leaked. §5.2 reconciles this exactly.

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 37 / 39 / **0** / 0 |
| `.tmp*` directories | 3,692 |
| `C:` free | 39.47 GB |

### 1.1 All four targets are untracked user files

| file | lines | pre-batch SHA-256 | git status |
|---|---:|---|---|
| `pre_phase_8_tests.rs` | 979 | `CC90B9493F4F9602` | **UNTRACKED** |
| `phase_7_4_decision_tests.rs` | 1,494 | `E40A638865240F43` | **UNTRACKED** |
| `phase_7_1_regression_tests.rs` | 1,180 | `3185060528658E86` | **UNTRACKED** |
| `phase_7_3_decision_tests.rs` | 1,294 | `DA2B2622A9C111CF` | **UNTRACKED** |

None of the four exists in HEAD. **A HEAD restore is therefore not a recovery option for any of
them** — if a file were lost, the work could not be recovered from git at all. Every restore in this
batch went through `%TEMP%\B11_backup\PRE_<sha>_<name>`, and every restore was SHA-verified.

`pre_phase_8_tests.rs` was previously misidentified by a residue detector as a stray generated file.
It is confirmed here to be a legitimate user test file: 979 lines, 13 `#[tokio::test]` tests, a
`Db` harness and a `pp8_api::Harness`, all consistent with the rest of the suite. Its name matches
nothing in the residue patterns, and the earlier flag was a false positive from a `PRE_` pattern
matching the `pre_` prefix.

---

## 2. Exact counts per suite

| suite | tests | owning a fixture | teardown statements added | fault-injection closes | pre-batch | post-batch |
|---|---:|---:|---:|---:|---:|---:|
| `pre_phase_8_tests` | 13 | 12 | **12** | 0 | 12 | **0** |
| `phase_7_4_decision_tests` | 11 | 10 | **10** | 0 | 10 | **0** |
| `phase_7_1_regression_tests` | 15 | 14 | **14** | **3** | 11 | **0** |
| `phase_7_3_decision_tests` | 14 | 13 | **13** | **1** | 12 | **0** |
| **total** | **53** | **49** | **49** | **4** | **45** | **0** |

Four tests own no fixture and were left untouched: `pp8_o2_no_view_field_is_still_called_the_bar…`,
`p74_e_weights_and_thresholds_are_unchanged`, `p71_d2_the_prerequisite_dag_behaviour_is_unch…`,
`p73_q5_the_supported_set_and_the_resolution_…`.

### 2.1 Three distinct fixture shapes, not one shared helper

| shape | where | fixture call | close expression |
|---|---|---|---|
| `struct Db { store, _dir }` | all four files | `Db::new()` — 4 / 9 / 13 / 12 | `db.store.pool.close().await;` |
| `struct Harness { state, store, _dir }` inside `mod pp8_api` | `pre_phase_8_tests` | `pp8_api::Harness::new()` — 8 | `app.close().await;` (see §4.1) |
| `AppState` built from a caller-owned tempdir | `phase_7_1`, `phase_7_3` | `app(&dir)` — 1 / 1 | `state.store.pool.close().await;` |
| inline `tempdir()` + `Store::connect` | `phase_7_4_decision_tests` | 1 | `store.pool.close().await;` |

DB names: `pp8.sqlite3`, `pp8api.sqlite3`, `phase74.sqlite3`, `phase74api.sqlite3`,
`phase71.sqlite3`, `phase71api.sqlite3`, `phase73.sqlite3`, `phase73api.sqlite3`.

---

## 3. Pre-batch baseline and its exact reconciliation

Measured before any edit, with a set-difference of temp-directory names:

| suite | pre-batch delta |
|---|---:|
| `pre_phase_8_tests` | 12 |
| `phase_7_4_decision_tests` | 10 |
| `phase_7_1_regression_tests` | 11 |
| `phase_7_3_decision_tests` | 12 |
| **total** | **45** |

This matches the fixture inventory **exactly**, and the reconciliation is arithmetic rather than
coincidental:

| suite | fixture-owning tests | already closed mid-test (fault injection) | predicted leak | measured |
|---|---:|---:|---:|---:|
| `pre_phase_8_tests` | 12 | 0 | 12 | **12** |
| `phase_7_4_decision_tests` | 10 | 0 | 10 | **10** |
| `phase_7_1_regression_tests` | 14 | −3 | 11 | **11** |
| `phase_7_3_decision_tests` | 13 | −1 | 12 | **12** |
| **total** | **49** | **−4** | **45** | **45** |

The four fault-injection tests never leaked because their pools are already closed mid-test, so the
`TempDir` drop succeeds. That is also why a close is still added to them (§5.2): correctness of
teardown should not depend on a fault-injection close happening to release the directory.

---

## 4. Ownership, teardown, and one encapsulation constraint

### 4.1 `pp8_api::Harness` has two private fields

The eight `Harness::new()` sites could not be closed by field access. The first attempt produced
`error[E0616]: field 'store' of struct 'pp8_api::Harness' is private`, and switching to the other
field produced `field 'state' … is private` — **both fields are private by design**. The struct
deliberately exposes accessor *methods* (`key_and_project`, `record`, `get`), not its internals.

Rather than break that encapsulation by making fields `pub`, a teardown method was added — test-only,
inside the same test file, consistent with the harness's existing shape:

```rust
/// Teardown. `self.store` and `self.state.store` are the same pool (one is a
/// clone of the other), so closing it here releases every handle this harness
/// handed out, and lets `_dir` drop a directory with no live SQLite/WAL/SHM
/// files in it. The fields are private on purpose -- tests reach the pool
/// through this method rather than by reaching into the struct.
pub async fn close(&self) {
    self.store.pool.close().await;
}
```

`AppState::new` received `store.clone()` (L914), so both handles are the same pool; closing either
releases both. The eight call sites use `app.close().await;`.

### 4.2 Teardown correctness across the other shapes

* `Db` is moved into tests as `let db = Db::new().await;` — never moved further — so
  `db.store.pool` is always reachable at the end of the body.
* `phase_7_1` uses `let store = &db.store;` (a borrow) in several tests. The close deliberately
  targets `db.store.pool`, not the borrow, so it does not depend on which binding a test happens to
  keep.
* The `app(&dir)` shape owns the `TempDir` in the test body and the `Store` inside `AppState`, so the
  close targets `state.store.pool`.

### 4.3 Background work

None of the four files contains `tokio::spawn`, `spawn_blocking`, `JoinHandle`, `thread::spawn`,
`spawn_run`, `axum::serve`, `Instant::now`, or any sleep. The API-shaped tests
(`pre_phase_8_tests::pp8_api`, `phase_7_1::p71_d1_the_api_does_not_convert…`,
`phase_7_3::p73_q2_the_project_risk_endpoint…`, `phase_7_4::p74_f_…`) drive
`crate::api::router(state).oneshot(...)` in-process via `tower::ServiceExt` — **not** a bound
listener, unlike `ws_tests`. No detached server can hold a pool past the test body.

---

## 5. Pool-close classification (Step 4)

### 5.1 Pre-existing closures: exactly four, all `INTENTIONAL_FAULT_INJECTION`

Recorded verbatim **before any edit**, with the healthy read above and the failure assertion below:

| # | file | line (pre-batch) | exact text | healthy read above | failure assertion below |
|---|---|---:|---|---|---|
| 1 | `phase_7_1_regression_tests.rs` | 295 | `'    store.pool.close().await;'` | `assert!(ok.is_complete(), "a report from readable storage is complete")` after `.expect("a readable store verifies")` | `failed.expect_err("P7-D1: an unreadable store must not produce a successful report…")` |
| 2 | `phase_7_1_regression_tests.rs` | 329 | `'    store.pool.close().await;'` | evidence store attached so the verifier actually reads | `results` table asserting every comparator propagates the failure |
| 3 | `phase_7_1_regression_tests.rs` | 710 | `'    state.store.pool.close().await;'` | `assert_eq!(status, StatusCode::OK)` and `read_failures` present **and empty** | `assert!(status.is_server_error(), "an unreadable store must not produce a 200…")` |
| 4 | `phase_7_3_decision_tests.rs` | 1269 | `'    store.pool.close().await;'` | `assert!(empty.insufficient_data.is_some(), "an absent module is insufficient data, not an error")` | `assert!(reader.module_risk_history(…).is_err(), "a closed store must be an error, never an empty module history")` |

**No `UNRESOLVED` sites. No `NORMAL_TEARDOWN`, `RESTART_OR_REPLACEMENT_POOL` or `OTHER_EXPLICIT_PURPOSE`
sites existed pre-batch.** `pre_phase_8_tests` and `phase_7_4_decision_tests` had zero closes.

Two of my automated classification heuristics initially reported `False` for sites 1 and 3. Both were
false negatives in the *checker*, not the code: site 1 asserts failure via `expect_err` (not
`is_err`), and site 3's healthy assertion is 18 lines above the close, outside the 10-line window.
Both were confirmed by reading the source directly. Two earlier batches' checks over-rejected for the
same reason; the code was re-examined in each case rather than the check being trusted.

### 5.2 Post-batch layout: injection first, teardown last

Each of the four tests now carries two closes on the same receiver — the original injection, unmoved
and still ahead of the read it is meant to break, plus a new teardown close as the final statement:

```
phase_7_1_regression_tests::p71_d1_a_closed_pool_fails_the_verification_rather_than_reporting_clean
   closes at [295, 309]   fault-injection positions: [True, False]
phase_7_1_regression_tests::p71_d1_every_required_comparator_propagates_a_failed_read
   closes at [330, 381]   fault-injection positions: [True, False]
phase_7_1_regression_tests::p71_d1_the_api_does_not_convert_a_storage_failure_into_a_successful_report
   closes at [716, 730]   fault-injection positions: [True, False]
phase_7_3_decision_tests::p73_error_propagation_and_genuine_empty_results_are_still_distinguishable
   closes at [1281, 1306] fault-injection positions: [True, False]
```

The teardown close is an **additional** statement. It does not stand in for the injection, and the
injection is not credited with the teardown. §9 proves both halves independently.

---

## 6. Files changed

| file | added | removed | changed | what |
|---|---:|---:|---:|---|
| `pre_phase_8_tests.rs` | 21 | 0 | **0** | 8 × `app.close().await;` + 4 × `db.store.pool…` + 9-line harness `close()` method with doc comment |
| `phase_7_4_decision_tests.rs` | 10 | 0 | **0** | 9 × `db.store.pool…` + 1 × `store.pool…` |
| `phase_7_1_regression_tests.rs` | 14 | 0 | **0** | 13 × `db.store.pool…` + 1 × `state.store.pool…` |
| `phase_7_3_decision_tests.rs` | 13 | 0 | **0** | 12 × `db.store.pool…` + 1 × `state.store.pool…` |
| **total** | **58** | **0** | **0** | |

**All four files are pure insertions** — each file's pre-batch content is a verified ordered
subsequence of its post-batch content, with zero changed lines.

---

## 7. Repeated-run validation — 10 consecutive runs per suite

| suite | run | exit | passed | failed | new artifacts |
|---|---:|---:|---:|---:|---:|
| `pre_phase_8_tests` | 1–10 | 0 ×10 | 13 | 0 | **0 ×10** |
| `phase_7_4_decision_tests` | 1–10 | 0 ×10 | 11 | 0 | **0 ×10** |
| `phase_7_1_regression_tests` | 1–10 | 0 ×10 | 15 | 0 | **0 ×10** |
| `phase_7_3_decision_tests` | 1–10 | 0 ×10 | 14 | 0 | **0 ×10** |

**40 runs, 40 × zero new artifacts, zero teardown errors.** No run failed, so no failure evidence
needed preserving.

### 7.1 Per-test attribution

All **53** tests run **individually** with a fully-qualified `--exact` filter:

```
sanity: snapshot entries = 3737
tests run individually : 53
distinct exit codes    : 0
distinct newTmp values : 0
TOTAL new artifacts    : 0
zero-test filter traps : 0
```

---

## 8. Mutation 1 — remove all 49 teardown statements

* **Applied:** all four SHAs changed. 50 statements removed (49 teardown calls plus the single
  `self.store.pool.close().await` inside the new harness method). The 4 fault-injection closes
  correctly survived — `phase_7_1` reports `fault-injection left=3`, `phase_7_3` reports `1`. Asserted.
* **Rebuilt:** fresh `Compiling`, binary `14:25:12` newer than mutated source `14:24:40`, exit 0.

| suite | fixed | mutated | pre-batch |
|---|---:|---:|---:|
| `pre_phase_8_tests` | 0 | **12** | 12 |
| `phase_7_4_decision_tests` | 0 | **10** | 10 |
| `phase_7_1_regression_tests` | 0 | **11** | 11 |
| `phase_7_3_decision_tests` | 0 | **12** | 12 |
| **total** | **0** | **45** | **45** |

Three of the four mutated SHAs equal their **original pre-batch SHAs exactly**
(`e40a6388…`, `31850605…`, `da2b2622…`), proving the mutation reverted them precisely and removed
nothing else. `pre_phase_8_tests` differs only because the harness `close()` doc comment survives the
line-level removal.

Note `phase_7_1` and `phase_7_3` mutate back to **11** and **12**, not 14 and 13 — the four
fault-injection closes are still in place, so those tests still do not leak. That is independent
confirmation that the preserved closes are doing fault-injection work, not cleanup.

---

## 9. Mutation 2 — remove a fault-injection close

Removed the injection close at `phase_7_1_regression_tests.rs:295`, inside
`p71_d1_a_closed_pool_fails_the_verification_rather_than_reporting_clean`. The teardown close at the
end of the same test was left in place, isolating the injection close alone.

* **Applied:** sha `0a93677a…` → `8c39caa6…`, exactly 1 line removed. Asserted.
* **Rebuilt:** fresh `Compiling`, binary newer than mutated source, exit 0.
* **Run:** fully-qualified `--exact` filter, reporting `0 passed; 1 failed; 2157 filtered out` — the
  test genuinely executed.

```
exit code: 101
panicked at src\phase_7_1_regression_tests.rs:299:26:
P7-D1: an unreadable store must not produce a successful report. Before the fix this returned Ok with comparisons: 0,
test phase_7_1_regression_tests::p71_d1_a_closed_pool_fails_the_verification_… ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2157 filtered out
```

Without the injection close the verifier succeeds and `expect_err` panics. The close is load-bearing
and the test fails loudly for its intended reason.

* **Restored:** sha `0a93677a…` — byte-identical. Control run of the same test: exit 0, 0 artifacts.

---

## 10. Errors made and corrected during this batch

Reported rather than smoothed over. Three were mine.

1. **Wrong close target — private fields.** `error[E0616]` on `Harness.store`, then on
   `Harness.state`. Both fields are private by design. Fixed by adding a `close()` method to the
   harness rather than by making fields `pub`.
2. **Double-applied the edit (the Batch 9 mistake, repeated).** After the `E0616` failure I restored
   only `pre_phase_8_tests.rs` and re-ran the apply script, which re-applied to the **already
   modified** other three, producing 98 closes. Compiling caught it. I then restored all four to
   exact pre-batch SHAs (verified `closes=0/0/3/1`) before a single clean re-apply.
3. **Left four duplicate closes.** My strip step removed only the `Harness`-path lines, leaving four
   `db.store.pool` closes from the failed run, then added four more — duplicated pairs at L245/246,
   L516/517, L776/777, L856/857. Caught by an explicit duplicate check, not by the compiler (a second
   `close()` is idempotent and compiles fine). Restored all four to exact pre-batch state and applied
   once; the duplicate scan now reports `none` in every file.

The third is the most dangerous class of error in this work: **a duplicated teardown close is
functionally harmless and would have passed every test**, so only an explicit structural check catches
it. The second is the same restore-partial-set mistake the Batch 9 report warned about.

Two earlier instrumentation defects were also corrected before any measurement was believed: a
fixture detector that matched generic `new()` methods and produced call-site counts of 28/38/36
(garbage — the real counts are 12/10/14/13), and an `app()` pattern that required empty parentheses
and so missed the two `app(&dir)` sites entirely.

---

## 11. Regression checks

### 11.1 Sweep — 42 suites, exact module filters only

| suite | exit | new artifacts | result |
|---|---:|---:|---|
| `pre_phase_8_tests` | 0 | **0** | 13 passed |
| `phase_7_4_decision_tests` | 0 | **0** | 11 passed |
| `phase_7_1_regression_tests` | 0 | **0** | 15 passed |
| `phase_7_3_decision_tests` | 0 | **0** | 14 passed |
| `integrity_tests` | 0 | 0 | 34 passed, 1 ignored |
| `phase_8_quality_tests` | 0 | 0 | 20 passed |
| `phase_9_readiness_tests` | 0 | 0 | 21 passed |
| `change_impact_tests` | 0 | 0 | 19 passed |
| `risk_signal_tests` | 0 | 0 | 18 passed |
| `risk_semantics_tests` | 0 | 0 | 10 passed |
| `rbac_matrix_tests` | 0 | 0 | 15 passed |
| `rbac_probe_tests` | 0 | 0 | 17 passed |
| `trace_chain_tests` | 0 | 0 | 14 passed |
| `trace_tests` | 0 | 0 | 21 passed |
| `store_tests` | 0 | 0 | 36 passed |
| `qa_memory_risk_tests` | 0 | 0 | 21 passed |
| `risk_prioritisation_tests` | 0 | 0 | 20 passed, 1 ignored |
| `exploration_tests` | 0 | 0 | 35 passed, 1 ignored |
| `agent_execution_tests` | 0 | 0 | 40 passed |
| `governed_healing_tests` | 0 | 0 | 60 passed |
| `engine_tests` | 0 | 0 | 41 passed |
| `qa_memory_tests` | 0 | 0 | 33 passed |
| `qa_memory_retrieval_tests` | 0 | 0 | 21 passed |
| `autopilot_store` | 0 | 0 | 17 passed |
| `qa_memory_planning_tests` | 0 | 0 | 14 passed |
| `qa_memory_perf_tests` | 0 | 0 | 7 passed |
| `qa_memory_validation_tests` | 0 | 0 | 7 passed |
| `key_scope_tests` | 0 | 0 | 9 passed |
| `ws_tests` | 0 | 0 | 9 passed |
| `process_lifetime_maps_tests` | 0 | 0 | 12 passed |
| `run_map_retention_tests` | 0 | 0 | 6 passed |
| `journey_tests` | 0 | 10 | 13 passed |
| `integration_e2e_tests` | 0 | 16 | 20 passed |
| `operations_tests` | 0 | 15 | 15 passed |
| `adaptive_intelligence_tests` | 0 | 9 | 39 passed |
| `autopilot_runner` | 0 | 15 | 16 passed |
| `autopilot_steps` | 0 | 29 | 21 passed |
| `autopilot_execute` | 0 | 14 | 21 passed |
| `autopilot_generate` | 0 | 20 | 19 passed |
| `autopilot_report` | 0 | 2 | 17 passed |
| `store::history_tenant_tests` | 0 | 2 | 2 passed |
| `intelligence::` | 0 | 2 | 24 passed, 1 ignored |

**42 suites, 0 exit failures, 0 reporting a real `test result: FAILED`** (checked with `-cmatch`;
plain `-match` is case-insensitive and matches the words `0 failed`).

**31 suites at zero — all four Batch 11 targets and all 27 previously-fixed families.** Phase 7.1,
7.3, 7.4 and Phase 8 regressions (`integrity_tests`, `phase_8_quality_tests`, `change_impact_tests`)
are unchanged and clean.

Every filter used here is an exact module path. The bare names `auth`, `store`, `trace`,
`change_impact` and `intelligence` have each produced phantom leak figures in earlier batches by
substring-matching unrelated suites, and none is used.

### 11.2 Formatting

```
cargo fmt --check  →  exit 1
files rustfmt would change:  src/auth.rs:317
```

The sole complaint is the **pre-existing** rustfmt non-idempotency on `concat!` inside `role_gate!`,
left untouched by design. **None of the four Batch 11 files appears.** No file was passed through
write-mode rustfmt.

### 11.3 Clippy

```
cargo clippy --all-targets  →  exit 0
errors: 0    error[E]: 0    warnings: 66
```

`phase_7_4_decision_tests.rs` is named in 4 diagnostics and `phase_7_3_decision_tests.rs` in 3.
Both files were re-checked against their pre-batch versions, which are named in the **same** counts
(4 and 3). All seven are pre-existing; this batch introduced none.
`pre_phase_8_tests.rs` and `phase_7_1_regression_tests.rs`: 0 diagnostics.

---

## 12. Confirmations

| item | status |
|---|---|
| Production source untouched | `store.rs` = `97E79B8F876D7219`; `intelligence.rs` = `B8051724D69E2E40`; `api_tests.rs` = `966DF516D9C3A4A0` — all unchanged |
| `store.rs` / `history_tenant_tests` | **not touched** |
| `intelligence.rs` inline tests | **not touched** |
| `api_tests` fixtures | **not touched** |
| Risk formulas, weights, thresholds, severity classification | **none altered** — resource lifecycle only |
| Migrations / routes / schemas | none added, none changed |
| Pre-existing untracked user files | all four **preserved**; pre-batch content verified as an ordered subsequence, zero changed lines |
| Fault-injection behavior | 4 / 4 preserved, byte-identical, unmoved, and proven load-bearing by Mutation 2 |
| Historical artifacts | **none deleted**; no cleanup script run |
| Commit / push | none. HEAD still `7930507`, 0/0 |
| Resets / stashes / `git clean` | none |
| Whole-file reformatting | none |
| Mutation residue | **none**; backups live in `%TEMP%` |
| Artifact growth 3,692 → 3,918 | measurement side-effect only |
| Full backend suite | **not run**, per instruction |

---

## 13. Remaining work

### 13.1 Remaining fixture families (re-measured this batch)

| family | measured |
|---|---:|
| `api_tests` (6 fixture families) | 308 *(previously measured; not run in this batch)* |
| `integration_e2e_tests` | 16 |
| `operations_tests` | 15 |
| `autopilot_runner` | 15 |
| `autopilot_execute` | 14 |
| `autopilot_steps` | 29 |
| `autopilot_generate` | 20 |
| `journey_tests` | 10 |
| `adaptive_intelligence_tests` | 9 |
| `autopilot_report` | 2 |
| **`store::history_tenant_tests`** | **2** *(production-file blocker)* |
| **`intelligence::`** | **2** *(production-file blocker)* |

Not re-measured in this batch, carrying previous figures: `retention_and_metrics_tests`.

The autopilot figures vary by 1–3 between sweeps (concurrent suites and OS-level reclamation timing);
they are re-measured each batch rather than carried forward as exact.

### 13.2 The two production-file blockers — still unresolved

Both require authorization this batch does not have, and both were left untouched:

| blocker | location | fixtures | existing closes | measured |
|---|---|---:|---:|---:|
| `#[cfg(test)] mod history_tenant_tests` | `store.rs:5888` | 2 inline | 0 | **+2** |
| inline `#[cfg(test)]` module | `intelligence.rs:982` (fixtures at L1154, L1260) | 2 inline | 0 | **+2** |

Neither file was reformatted or otherwise written to.

### 13.3 `api_tests` ownership work still outstanding

`api_tests` remains the single largest item at **308**. Its fixture families still need a dedicated
ownership audit:

1. **7 `reloaded` stores** on one `api.sqlite3`, none closed (L11020, 11064, 11127, 11308, 11478,
   11621, 21740), plus scoped-block stores at L11868/11888/11955.
2. **2 fault-injection closes** to preserve (L12413, L12862).
3. **6 fixture families, ~311 call sites** — the earlier inventory counted only three.
4. `AppState.runs` holds `Arc<RunRecord>`; safe today because no api test starts a run, but this is
   the property to re-check if one ever does.

---

## 14. Corrected inventory totals

| component | value |
|---|---:|
| 492 after Batch 10 | 492 |
| − Batch 11 (measured) | −45 |
| **= after Batch 11** | **447** |
| of which `api_tests` | 308 |
| of which two production-file blockers | 4 |
| of which re-measured remaining suites (this sweep) | ~130 |
| of which carried-forward unmeasured (`retention_and_metrics_tests`) | ~5 |

**Fixed to date: 31 families, ~496 call sites, all at measured delta 0.**

These are distinct quantities — call sites, fixture-owning tests, measured directory deltas and family
counts — and are not summed into a completion percentage.

---

## 15. Verdict

# Batch 11: PASS

Four families, **49 teardown statements across 49 fixture-owning tests**, **45 measured leaks
eliminated**, all four suites at **delta 0** across 40 repeated suite runs and 53 individual
per-test runs. Four files changed, **58 lines added, 0 removed, 0 changed** — all pure insertions.
All four fault-injection closes verified byte-identical, unmoved, still ahead of the read each is
meant to break, and proven load-bearing by a paired mutation. No production file touched, no untracked
user file lost, no artifact deleted.

Three errors were made and corrected during this batch (§10), including a repeated partial-restore
and a duplicated-close defect that would have passed every test.

---

## 16. Recommendation for Batch 12

**Batch 12 should be the remaining autopilot family, then `api_tests` last.** Specifically:

1. **Batch 12 — the autopilot cluster** (`autopilot_steps` 29, `autopilot_generate` 20,
   `autopilot_runner` 15, `autopilot_execute` 14, `autopilot_report` 2 — **80 measured**). These five
   share one helper, `autopilot_test_support::fixture` (**80 call sites** across four modules), and
   were deliberately kept together since Batch 7's audit. Fixing the shared helper plus the
   per-module call sites is one coherent batch.
2. **Batch 13 — `api_tests`**, only after the above, so the closing full-suite artifact verification
   runs against a repository with no known leak except the two blocked production-file modules.

Carry forward from this batch:

1. **Restore every mutated file as a set and verify all SHAs before any measurement.** I repeated the
   partial-restore mistake here despite Batch 9's report warning about it.
2. **Add an explicit duplicate-statement check after any scripted insertion.** A duplicated
   `pool.close()` is idempotent, compiles, passes every test and still leaks nothing — only a
   structural check finds it.
3. **Check field visibility before choosing a close target.** A private harness field cost two failed
   builds here; a `close()` method on the harness was the right answer, not `pub` fields.
4. **Never use a bare module name as a test filter.**
5. **These target files are untracked.** There is no git recovery for them. Back up before every
   mutation and verify the restore by SHA, not by `git status`.

> **Phase 9.2 remains OPEN.** It closes only when every identified fixture family is fixed with a
> measured delta of 0 or proven safe — **including the two production-file blockers
> (`store::history_tenant_tests` +2 and `intelligence::` +2), which need an authorization decision** —
> and the final full-backend-suite artifact verification confirms no new artifacts accumulate. 447
> remain. Batch 12 has not been started. Phase 10 has not been started.
