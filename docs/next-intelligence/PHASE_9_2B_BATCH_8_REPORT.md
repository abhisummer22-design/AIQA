# Phase 9.2B — Batch 8 Report

## Store, Trace, Change-Impact and Validation Fixtures — Six Files, 99 Verified Call Sites

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — nothing committed, nothing pushed
**Verdict: PASS** — with one newly discovered blocker recorded in §13

---

## 0. Headline numbers

| | |
|---|---:|
| Call sites fixed | **99** (estimate was 93) |
| Measured leaks eliminated | **96** |
| Post-fix delta, all six suites | **0** |
| Fault-injection closes preserved | **3 / 3** |
| Repeated suite runs | **60** (10 per suite), all exit 0, all delta 0 |
| Individual per-test runs | **104**, all exit 0, all delta 0 |
| Files changed | 6 test files — **no production file** |

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 29 / 36 / **0** / 0 |
| `.tmp*` directories | 2,340 |
| `C:` free | 50.00 GB |
| `store.rs` SHA (production, pinned) | `97E79B8F876D7219` |

Pre-batch SHA-256 pins (all backed up to `…\Temp\B8_backup\PRE_<sha>_<name>` before editing):

| file | sha256 | lines |
|---|---|---:|
| `store_tests.rs` | `AA9899A77CDE8274` | 1,756 |
| `trace_tests.rs` | `DD669FAFBDAAA5B2` | 1,092 |
| `change_impact_tests.rs` | `5D6EB628D82E6EB6` | 1,096 |
| `qa_memory_planning_tests.rs` | `29A5F681EFB0F874` | 691 |
| `qa_memory_perf_tests.rs` | `02F4A9C371958B1A` | 493 |
| `qa_memory_validation_tests.rs` | `30215F88B88EB2FB` | 1,173 |

---

## 2. Exact source-level inventory

The estimate of 93 call sites was low. **The source count is 99.**

| family | file | helper | tests | call sites | pre-batch leak |
|---|---|---|---:|---:|---:|
| `store_tests` | `store_tests.rs` | `temp_store` | 36 | **36** | 36 |
| `trace_tests` | `trace_tests.rs` | `temp_store` | 21 | **18** | 18 |
| `change_impact_tests` | `change_impact_tests.rs` | `temp_store` | 19 | **17** | 14 |
| `qa_memory_planning_tests` | `qa_memory_planning_tests.rs` | `temp_store` | 14 | **14** | 14 |
| `qa_memory_perf_tests` | `qa_memory_perf_tests.rs` | `temp_store` | 7 | **7** | 7 |
| `qa_memory_validation_tests` | `qa_memory_validation_tests.rs` | `temp_store` | 7 | **7** | 7 |
| **total** | | | **104** | **99** | **96** |

All six use the same helper shape:

```rust
async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("<name>.sqlite3")).await.expect("connect");
    (store, dir)
}
```

Per-file DB names: `test.sqlite3`, `phase1-trace.sqlite3`, `phase4-change.sqlite3`,
`q-planning.sqlite3`, `q-perf.sqlite3`, `q-validation.sqlite3`.

**Why 99 sites but 96 leaks.** Three `Store` instances are already closed mid-test by fault injection,
so their directories were always removable. `change_impact_tests` has 17 fixture sites but only 14
could leak: 17 − 3 fault-injection = 14. The 99 closes are still the right number to insert — relying
on a fault-injection close to release a directory is not a teardown guarantee.

**Six tests use no fixture at all** and were correctly left alone:

| file | test |
|---|---|
| `store_tests.rs` | `the_documented_one_flag_at_a_time_contract…` — see below, it *does* use a fixture |
| `trace_tests.rs` | `an_ordinary_task_name_is_its_own_key`, `a_task_named_after_a_uuid_cannot_claim_that_case_vertex`, `the_three_test_related_node_types_are_distinct` — pure constant/array unit tests |
| `change_impact_tests.rs` | `restricted_to_narrows_the_existing_scope_without_losing…`, `an_empty_selection_still_retains_the_mandatory_set` |

### 2.1 The 36th `store_tests` fixture

`store_tests.rs:1627` `the_documented_one_flag_at_a_time_contract_is_what_the_store_does` does **not**
call `temp_store()`. It builds its own fixture inline:

```rust
let dir = tempfile::tempdir().expect("tempdir");
let store = Store::connect(&dir.path().join("task-status-contract.sqlite3")).await.expect("connect");
```

A naive helper-driven scan reports 35 sites and misses this entirely — the 36th. It is now covered.
`integrity_tests.rs:212` only *names* this test in a doc comment; it is not called cross-module.

### 2.2 A cross-module fixture dependency

```rust
// trace_tests.rs:45
pub async fn temp_store_for_phase2() -> (Store, tempfile::TempDir) { temp_store().await }
```

`pub`, and called from **`trace_chain_tests.rs:1060`**:

```rust
let (store, _d) = crate::trace_tests::temp_store_for_phase2().await;
```

Fixing `trace_tests::temp_store` does **not** fix that call site — the wrapper returns a fresh
`(Store, TempDir)` and the caller owns it. `trace_chain_tests` remains fully unfixed (measured +12,
§13) and owns that site. **Out of Batch 8's authorized scope; recorded for Batch 10.**

---

## 3. Existing `pool.close()` classification (Step 2, done before any edit)

Only one of the six files had any existing closes:

| site | classification | evidence |
|---|---|---|
| `change_impact_tests.rs:1025` | **INTENTIONAL_FAULT_INJECTION** | preceded by `assert!(res.is_ok(), "a healthy store must still produce an analysis")`; followed by a second `analyse(...)` asserted to be `Err` |
| `change_impact_tests.rs:1063` | **INTENTIONAL_FAULT_INJECTION** | preceded by an `.expect("healthy read")` emptiness assertion; followed by `assert!(reader.safety_floor(&pid).await.is_err(), "an unreadable floor must be an error, not an empty map…")` |
| `change_impact_tests.rs:1089` | **INTENTIONAL_FAULT_INJECTION** | preceded by `.expect("healthy read")` / `"no scored cases yet"`; followed by `assert!(reader.risk_scores(&pid).await.is_err(), "an unreadable risk read must be an error, not an empty map that selects nothing")` |

All three recorded verbatim before editing. No site was `UNRESOLVED`. The other five files had
**zero** existing closes. **No existing close was removed, moved, replaced or normalised.**

---

## 4. Fixture ownership and the applied fix

Each test binds one `Store`; the `TempDir` is the fixture's other return value. One awaited close per
bound `Store`, inserted as the **last statement of the test**, immediately before its closing brace —
the Batch 7 pattern:

```rust
    assert_eq!(
        passed, N,
        "every concurrent write must be durably persisted"
    );
    // Teardown: all 60 spawned writers were joined above, so this close cannot block.
    store.pool.close().await;
}
```

`Store` is `Clone`, and readers in these suites take `store.clone()`, so all clones share one pool —
a single close releases every handle. The `TempDir` is dropped after the test body returns, i.e. after
the awaited close, so the directory is removable.

Seven comment lines were added in `store_tests.rs` documenting why the close cannot block in the two
concurrency tests (§6). No `Drop` impl, no blocking call, no cleanup script, no production change.

---

## 5. Files changed

| file | added | removed | what |
|---|---:|---:|---|
| `store_tests.rs` | 43 | 0 | 36 closes + 7 comment lines |
| `trace_tests.rs` | 18 | **1** | 18 closes + one deliberate `.clone()` (§11) |
| `change_impact_tests.rs` | 17 | 0 | 17 closes |
| `qa_memory_planning_tests.rs` | 14 | 0 | 14 closes |
| `qa_memory_perf_tests.rs` | 7 | 0 | 7 closes |
| `qa_memory_validation_tests.rs` | 7 | 0 | 7 closes |

**Five of six files are pure insertions** — verified by the pre-batch content being an *ordered
subsequence* of the post-batch content. The sixth has exactly one changed line, characterised at
character level:

```
OLD '    let index = TraceReader::new(store).index(&pid).await;'
NEW '    let index = TraceReader::new(store.clone()).index(&pid).await;'
     delta: at col 38  removed ''  inserted '.clone()'
```

> `git diff -- <file>` against **HEAD** shows deletions in these files. Those are the user's
> pre-existing uncommitted work, not this batch's. The pre-batch backup is the correct control.

---

## 6. Background-work analysis

This batch has more concurrency than Batch 7. Both cases were verified rather than assumed.

### 6.1 `b1_concurrent_writes_do_not_silently_disappear` — 60 spawned writers

```rust
let mut handles = Vec::new();
for i in 0..N {                                  // N = 60
    let s = store.clone();
    handles.push(tokio::spawn(async move { s.set_task_status(...).await }));
}
let mut failures = Vec::new();
for (i, h) in handles.into_iter().enumerate() {
    if let Err(e) = h.await.expect("join") {     // <-- ALL 60 AWAITED
        failures.push(format!("t{i}: {e}"));
    }
}
assert!(failures.is_empty(), ...);
let persisted = store.list_tasks_for_run(run_id).await.expect("list tasks");
assert_eq!(passed, N, "every concurrent write must be durably persisted");
store.pool.close().await;
```

Every spawned task is joined at the loop. No handle survives the test body. The required durability
read happens **before** the close. Teardown cannot hide a failed writer: `h.await.expect("join")`
panics loudly on any error, and `close()` returns `()`, so it cannot swallow anything.

### 6.2 `call_while_the_write_lock_is_held` — the lock-contention helper

```rust
async fn call_while_the_write_lock_is_held<T, Fut>(store: &Store, call: impl FnOnce(Store) -> Fut) -> T {
    let writer = store.pool.begin_with("BEGIN IMMEDIATE").await.expect("take the write lock"); // holds a conn
    let running = tokio::spawn(call(store.clone()));
    tokio::time::sleep(Duration::from_millis(200)).await;   // <-- mechanism under test
    if running.is_finished() { panic!("the call did not wait for the write lock: {:?}", …); }
    writer.commit().await.expect("let the write lock go");  // conn released
    running.await.expect("join").expect("the call, once the write lock was free")  // task joined
}
```

Called by `delete_project_waits_for_another_writer…` (L1570) and
`a_requirement_document_upload_waits_for_another_writer…` (L1603). By the time it returns, the writer
has committed and the spawned call has been joined — **no checked-out connection, no live task.**

The 200 ms sleep is the behaviour under test (the call must still be blocked while the writer holds
the lock). It was **not** converted to a poll or removed. `Pool::close()` acquires
`max_connections` permits, so a close placed *before* the commit would have **deadlocked**; the
placement after both the commit and the join is deliberate and is recorded in a three-line comment at
each of the two call sites.

### 6.3 Everything else

| file | background work | verdict |
|---|---|---|
| `trace_tests.rs` | none | safe |
| `change_impact_tests.rs` | none | safe |
| `qa_memory_planning_tests.rs` | none | safe |
| `qa_memory_perf_tests.rs` | `Instant::now()` ×4 — timing measurement, not a task | safe |
| `qa_memory_validation_tests.rs` | none | safe |

**Timing contracts (§6L).** `qa_memory_perf_tests` has four `Instant::now()`/`elapsed()` blocks
(L324–366). Every close is appended **after** all of them, so teardown cannot influence any measured
duration. All seven perf tests pass with unchanged timings.

No test relies on an arbitrary sleep standing in for completion. The two sleeps present are both
explicitly the lock-holding condition under test.

---

## 7. Per-suite measurements

| family | pre-batch | **post-batch** | change |
|---|---:|---:|---|
| `store_tests` | 36 | **0** | −36 |
| `trace_tests` | 18 | **0** | −18 |
| `change_impact_tests` | 14 | **0** | −14 |
| `qa_memory_planning_tests` | 14 | **0** | −14 |
| `qa_memory_perf_tests` | 7 | **0** | −7 |
| `qa_memory_validation_tests` | 7 | **0** | −7 |
| **total** | **96** | **0** | **−96** |

Both figures measured in this session. The pre-batch column independently reproduces the inventory's
"+96" estimate exactly.

### 7.1 Per-test attribution

All **104** tests run **individually** with `--exact`, each with a before/after temp-directory
snapshot:

```
tests run individually : 104
distinct exit codes    : 0
distinct newTmp values : 0
TOTAL new artifacts    : 0
tests matching zero    : (none — every --exact filter matched a real test)
```

---

## 8. Repeated-run validation — 10 consecutive runs per suite

| suite | run | exit | passed | failed | new artifacts |
|---|---:|---:|---:|---:|---:|
| `store_tests` | 1–10 | 0 ×10 | 36 | 0 | **0 ×10** |
| `trace_tests` | 1–10 | 0 ×10 | 21 | 0 | **0 ×10** |
| `change_impact_tests` | 1–10 | 0 ×10 | 19 | 0 | **0 ×10** |
| `qa_memory_planning_tests` | 1–10 | 0 ×10 | 14 | 0 | **0 ×10** |
| `qa_memory_perf_tests` | 1–10 | 0 ×10 | 7 | 0 | **0 ×10** |
| `qa_memory_validation_tests` | 1–10 | 0 ×10 | 7 | 0 | **0 ×10** |

**60 runs, 60 × zero new artifacts, zero teardown errors.** No run failed, so no failure evidence
needed preserving.

---

## 9. Mutation validation

### Mutation 1 — remove all 99 teardown closes

* **Applied:** all six SHAs changed (`719d9963…`→`30d6a83d…`, `c62c6f0d…`→`9846b371…`,
  `6188a908…`→`5d6eb628…`, `2be00186…`→`29a5f681…`, `4acffe76…`→`02f4a9c3…`,
  `7348a315…`→`30215f88…`). Exactly **99** removed; the 3 fault-injection closes correctly survived
  (`change_impact_tests` shows `closes_left=3`). Asserted in the script.
* **Rebuilt:** fresh `Compiling`, binary `13:30:51` newer than mutated source `13:30:17`, exit 0.

| suite | fixed | mutated | pre-batch |
|---|---:|---:|---:|
| `store_tests` | 0 | **36** | 36 |
| `trace_tests` | 0 | **18** | 18 |
| `change_impact_tests` | 0 | **14** | 14 |
| `qa_memory_planning_tests` | 0 | **14** | 14 |
| `qa_memory_perf_tests` | 0 | **7** | 7 |
| `qa_memory_validation_tests` | 0 | **7** | 7 |
| **total** | **0** | **96** | **96** |

The leak returns **exactly**. `change_impact_tests` still measures 14, not 17, precisely because its
fault-injection closes are still in place — an independent confirmation that those three closes are
doing fault-injection work, not cleanup.

### Mutation 2 — remove one fault-injection close

Removed the injection close from `change_impact_tests::a_storage_failure_is_reported_rather_than_returning_an_empty_selection`.
The teardown close at the end of the same test was left in place, isolating the injection close alone.

* **Applied:** sha `6188a908…` → `29fda23d…`, exactly 1 line removed. Asserted.
* **Rebuilt:** fresh `Compiling`, binary newer than mutated source, exit 0.
* **Focused run used the fully-qualified name from `--list`** (trailing `:` stripped), and the run
  reported `0 passed; 1 failed; 2157 filtered out` — the test genuinely executed.

```
exit code: 101
panicked at: a storage failure must be an error, never an empty ChangeImpact: None
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2157 filtered out
```

Without the injection close the reader returns an **empty `ChangeImpact`** instead of an error — the
fabricated-empty-result defect the test exists to prevent.

* **Restored:** sha `6188a908…` — byte-identical. All three fault-injection tests back to
  `[1039, 1058]`, `[1078, 1085]`, `[1105, 1111]`.

### Invalid attempts, discarded

None this batch. Two traps from Batch 7 were actively avoided: restores used `copyfile` (content only,
fresh mtime) so cargo genuinely rebuilt — every build above shows a fresh `Compiling` line and a
binary newer than its source; and every focused run used a fully-qualified `--exact` name, with the
zero-test check in §7.1 confirming no filter silently matched nothing.

---

## 10. Fault-injection preservation

Located by **owning test** after the edit (line numbers shift as insertions land):

```
change_impact_tests::a_storage_failure_is_reported_rather_than_returning_an_empty_selection
   closes on 'store' at L[1039, 1058]  -> injection L[1039], teardown L[1058]
   exact text : '    store.pool.close().await;'
   healthy-read assertion above?  True     failure assertion after it?  True
   VERDICT: INTACT AND UNMOVED

change_impact_tests::a_storage_failure_cannot_silently_empty_the_safety_floor
   closes at L[1078, 1085]   VERDICT: INTACT AND UNMOVED

change_impact_tests::a_storage_failure_cannot_silently_empty_the_high_risk_tier
   closes at L[1105, 1111]   VERDICT: INTACT AND UNMOVED
```

Each test now carries **two** closes on the same receiver: the original mid-test injection, still
sitting between its healthy-read assertion and the assertion that the read then fails, plus a new
teardown close as the last statement. The teardown was **added alongside** — it does not stand in for
the injection, and the injection is not credited with the teardown. Mutation 2 proves the injection is
still load-bearing, and Mutation 1's `change_impact_tests` figure of 14 proves the teardown is now
doing the cleanup.

---

## 11. Regressions, formatting and Clippy

### 11.1 Regression sweep — 30 suites

| suite | exit | new artifacts | result |
|---|---:|---:|---|
| `store_tests` | 0 | **0** | 36 passed |
| `trace_tests` | 0 | **0** | 21 passed |
| `change_impact_tests` | 0 | **0** | 19 passed |
| `qa_memory_planning_tests` | 0 | **0** | 14 passed |
| `qa_memory_perf_tests` | 0 | **0** | 7 passed |
| `qa_memory_validation_tests` | 0 | **0** | 7 passed |
| `integrity_tests` | 0 | 0 | 34 passed, 1 ignored |
| `phase_8_quality_tests` | 0 | 0 | 20 passed |
| `phase_9_readiness_tests` | 0 | 0 | 21 passed |
| `exploration_tests` | 0 | 0 | 35 passed, 1 ignored |
| `agent_execution_tests` | 0 | 0 | 40 passed |
| `governed_healing_tests` | 0 | 0 | 60 passed |
| `engine_tests` | 0 | 0 | 41 passed |
| `qa_memory_tests` | 0 | 0 | 33 passed |
| `qa_memory_retrieval_tests` | 0 | 0 | 21 passed |
| `autopilot_store` | 0 | 0 | 17 passed |
| `qa_memory_risk_tests` | 0 | 0 | 21 passed |
| `risk_prioritisation_tests` | 0 | 0 | 20 passed, 1 ignored |
| `qa_memory_migration` | 0 | 0 | 1 passed |
| `trace_chain_tests` | 0 | 12 | 14 passed |
| `risk_signal_tests` | 0 | 11 | 18 passed |
| `journey_tests` | 0 | 11 | 13 passed |
| `integration_e2e_tests` | 0 | 16 | 20 passed |
| `operations_tests` | 0 | 15 | 15 passed |
| `adaptive_intelligence_tests` | 0 | 9 | 39 passed |
| `phase_7_1_regression_tests` | 0 | 11 | 15 passed |
| `phase_7_3_decision_tests` | 0 | 12 | 14 passed |
| `autopilot_runner` | 0 | 13 | 16 passed |
| `autopilot_steps` | 0 | 28 | 21 passed |
| `autopilot_execute` | 0 | 14 | 21 passed |
| `autopilot_generate` | 0 | 20 | 19 passed |
| `autopilot_report` | 0 | 2 | 17 passed |
| `store::history_tenant_tests` | 0 | 2 | 2 passed |

**30 suites, 0 exit failures, 0 suites reporting a real `test result: FAILED`** (checked with
`-cmatch 'test result: FAILED'`; plain `-match` is case-insensitive and matches the words
`0 failed`).

**No regression in any previously-fixed family** — all eleven remain at 0.

### 11.2 A measurement error, corrected

An earlier version of the sweep used the broad filters `cargo test store`, `cargo test trace` and
`cargo test change_impact`. These substring-match test *names* across many suites — they selected
**80**, **47** and **21** tests respectively, aggregating unrelated suites and producing phantom
leak figures (`store` +17, `trace` +15, `change_impact` +2). Those numbers are **not findings** and
are not reported as such. The table above uses exact module paths. One real finding did survive the
correction and is recorded in §13.

### 11.3 Formatting

```
cargo fmt --check  →  exit 1
files rustfmt would change:  src/auth.rs:317
```

The sole complaint is the **pre-existing** rustfmt non-idempotency on `concat!` inside `role_gate!`,
documented in the project record and deliberately left untouched. **None of the six Batch 8 files
appears.** No file was passed through write-mode rustfmt.

### 11.4 Clippy

```
cargo clippy --all-targets  →  exit 0
errors: 0    error[E]: 0    warnings: 66 (pre-existing)
diagnostics naming a Batch 8 file: 0 occurrences in each of the six
```

---

## 12. Errors caught and corrected during this batch

1. **Compile failure caught by the compiler, not by inspection.** The first application put
   `store.pool.close().await;` at the end of every fixture test. `trace_tests.rs:856` does
   `TraceReader::new(store)` — a **move** — so `store` no longer existed there. `error[E0382]: borrow
   of moved value: store`. Fixed by adding `.clone()`, matching what 7 of the 8 other
   `TraceReader::new` call sites in the same file already do. The move was an oversight in the
   original test, not a design choice.
2. **A move-detector I wrote was far too noisy** — it flagged 162 "moves" that were almost entirely
   `store.clone()`, method-call auto-refs and the `let (store, _dir) =` destructuring itself. The
   compiler is authoritative and found exactly one real move. The noisy tool output was discarded
   rather than acted on.
3. **Brace-boundary edge cases inspected rather than trusted.** Three tests "end" with a nested
   block's `}` or a trailing comment block; all three accept an insertion before the function's real
   closing brace. Verified individually.
4. **Broad test filters produced phantom leak figures** — caught, corrected, and reported as a
   measurement error rather than passed off as findings (§11.2).
5. **A residue check produced a false positive** — my `PRE_` pattern matched the legitimate
   untracked test file `pre_phase_8_tests.rs`. Confirmed as pre-existing user work, not residue.

---

## 13. Newly discovered blocker

**`store.rs:5888` contains an inline `#[cfg(test)] mod history_tenant_tests` with its own leaking
fixture.**

```rust
// store.rs:5888
#[cfg(test)]
mod history_tenant_tests {
    //! The tenant half of `task_history_by_name`'s project predicate.
    //! …
    async fn temp_store() -> (Store, tempfile::TempDir) {      // L5905
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.sqlite3");
        let store = Store::connect(&path).await.expect("connect");
        (store, dir)
    }
```

* **2 tests, 2 fixtures, 0 existing closes, measured +2 artifacts** (confirmed with the exact filter
  `store::history_tenant_tests`).
* It is **not** in `store_tests.rs`; the module comment says it lives inline "because of file
  ownership, not because it belongs to this file more".
* It **cannot be fixed under the current authorization**: `store.rs` is production code and is
  explicitly on the do-not-modify list, with a hard-pinned SHA `97E79B8F876D7219`.

This is a genuine blocker requiring a separate authorization decision, not a defect in this batch.
It is the only such case found.

---

## 14. Remaining fixture families and latest measured leakage

| remaining family | measured |
|---|---:|
| `api_tests` (6 fixture families; not run in this batch) | 308 *(previously measured)* |
| `autopilot_steps` | 28 |
| `integration_e2e_tests` | 16 |
| `operations_tests` | 15 |
| `autopilot_execute` | 14 |
| `autopilot_runner` | 13 |
| `phase_7_3_decision_tests` | 12 |
| `trace_chain_tests` | 12 |
| `risk_signal_tests` | 11 |
| `journey_tests` | 11 |
| `phase_7_1_regression_tests` | 11 |
| `adaptive_intelligence_tests` | 9 |
| `autopilot_generate` | 20 |
| `autopilot_report` | 2 |
| **`store::history_tenant_tests`** | **2** *(newly discovered, blocked — §13)* |

Not re-measured in this batch, carrying previous figures: `risk_semantics_tests`,
`rbac_matrix_tests`, `rbac_probe_tests`, `key_scope_tests`, `ws_tests`,
`process_lifetime_maps_tests`, `run_map_retention_tests`, `pre_phase_8_tests`,
`phase_7_4_decision_tests`, `retention_and_metrics_tests`.

**Accounting.** Batch 8 removed 96 of the 660 outstanding after Batch 7, leaving **564** across the
remaining families, of which 176 was re-measured here and 308 is `api_tests`. These are distinct
quantities — a source count, per-suite directory counts and a full-run estimate — and are not summed
into a completion percentage.

**Fixed to date:** 18 families, ~376 call sites, all at measured delta 0.

---

## 15. Confirmations

| item | status |
|---|---|
| Production source untouched | `store.rs` = `97E79B8F876D7219`, unchanged. Only the six Batch 8 **test** files have mtimes after 13:20. |
| Migrations / routes / schemas | none added, none changed |
| Existing user changes | preserved; five files pure-insertion, one with a single characterised 6-character addition |
| Historical artifacts | **none deleted**; no cleanup script run |
| Commit / push | none. HEAD still `7930507`, 0/0 |
| Resets / stashes / `git clean` | none |
| Whole-file reformatting | none |
| Mutation residue in the tree | **none** (`PRE_<sha>_` backups live in `%TEMP%\B8_backup`, outside the repo) |
| Artifact growth 2,340 → 2,938 | a side-effect of ~230 measurement runs against deliberately unfixed families. Nothing deleted to improve any number. |
| Full backend suite | **not run**, per instruction |

---

## 16. Verdict

# Batch 8: PASS

Six files, **99 call sites fixed**, **96 measured leaks eliminated**, all six suites at **delta 0**
across 60 repeated suite runs and 104 individual per-test runs. Three fault-injection closes verified
byte-identical and unmoved, and independently proven still load-bearing by Mutation 2 and still
*not* doing cleanup work by Mutation 1's `change_impact_tests` figure. No production file touched, no
user work lost, no artifact deleted.

One blocker recorded (§13) that requires authorization this batch does not have.

---

## 17. Recommendation for Batch 9

**Proceed with Batch 9** as scoped in the inventory, unchanged:

`process_lifetime_maps_tests`, `key_scope_tests`, `ws_tests`, `rbac_matrix_tests`,
`run_map_retention_tests` — ~35 call sites, previously measured +42.

Carry forward from this batch:

1. **Enumerate tests first, then fixtures.** The 36th `store_tests` fixture builds its store inline
   and is invisible to a helper-name scan. Count *tests that own a fixture*, not helper call sites.
2. **Check whether the test moves its `Store`.** `TraceReader::new(store)` consumes it. Compile
   after the first group of edits rather than after all of them.
3. **Use exact module paths as test filters.** `cargo test store` matches 80 tests across many
   suites and produces phantom leak figures.
4. **Audit for inline `#[cfg(test)]` modules inside production files.** `store.rs` hid one. Any fix
   needs explicit authorization.

> **Phase 9.2 remains OPEN.** It closes only when every identified fixture family is fixed with a
> measured delta of 0 or proven safe — **including `store::history_tenant_tests`, which needs a
> production-file authorization decision** — and the final full-backend-suite artifact verification
> confirms no new artifacts accumulate. Batch 9 has not been started. Phase 10 has not been started.
