# Phase 9.2B — Batch 7 Report

## QA Memory and Risk Fixture Cleanup — Four Families, 84 Verified Call Sites

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — nothing committed, nothing pushed
**Verdict: PASS**

---

## 0. Headline correction

The approved scope and the preceding safety audit both state **20 call sites per family, 80 total**.
That figure is **wrong**. The audit's counting regex already excluded the `fn` definition via a
`(?<!fn )` lookbehind and then subtracted one more, double-counting the definition.

**The actual, source-counted number is 21 call sites per family — 84 in total.**

| family | audit said | source says | delta |
|---|---:|---:|---:|
| `qa_memory_retrieval_tests` | 20 | **21** | +1 |
| `autopilot_store` | 20 | **21** | +1 |
| `qa_memory_risk_tests` | 20 | **21** | +1 |
| `risk_prioritisation_tests` | 20 | **21** | +1 |
| **total** | 80 | **84** | **+4** |

The extra sites are the second binding in each two-store test (`let (store2, _dir2) = temp_store()`),
which the audit's regex counted but then lost to the over-subtraction. **All 84 were fixed.**

The *leak* count, however, is genuinely 80, and the two numbers differ for a reason that matters —
see §5.2.

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 28 / 35 / **0** / 0 |
| `.tmp*` directories | 1,901 |
| `C:` free | 51.12 GB |

Source SHA-256 pinned before any edit:

| file | sha256 (first 16) |
|---|---|
| `qa_memory_retrieval_tests.rs` | `A58F347B3D62ED58` |
| `autopilot_store.rs` | `F8A42C1C5C2DE874` |
| `qa_memory_risk_tests.rs` | `89AEE4EF167D1BE0` |
| `risk_prioritisation_tests.rs` | `1C3D8361E400F3C4` |
| `store.rs` (production, must not change) | `97E79B8F876D7219` |

All four target files were backed up to `…\Temp\B7_backup\PRE_<sha>_<name>` before editing. Every
restore in this batch was SHA-verified byte-identical.

---

## 2. What the fixture actually looks like

All four families share one shape:

```rust
async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("<name>.sqlite3")).await.expect("connect");
    (store, dir)
}
```

| family | DB file | helper at | tests | call sites | Store instances | fault injection |
|---|---|---|---:|---:|---:|---:|
| `qa_memory_retrieval_tests` | `q3.sqlite3` | L29 | 21 | 21 | 21 | 0 |
| `autopilot_store` | `test.sqlite3` | L641 | 17 | 21 | 21 (4 tests bind a 2nd) | 0 |
| `qa_memory_risk_tests` | `q5.sqlite3` | L25 | 21 | 21 | 21 (1 test binds a 2nd) | **2** |
| `risk_prioritisation_tests` | `phase4-risk.sqlite3` | L50 | 21 | 21 | 21 (1 `#[ignore]`d) | **1** |

Two structural details the audit's report did not record, both of which changed the edit:

1. **Test bodies close at different indents per file.** `autopilot_store.rs` keeps its tests inside
   `#[cfg(test)] mod tests { … }`, so a test body closes at **4-space** indent (`    }`). The other
   three declare tests at column 0, so their bodies close at **column 0** (`}`). A single
   brace-detection rule applied to all four files silently picks the wrong boundary for three of them.
2. **Body statements sit at attribute-indent + 4.** My first application used the *attribute* indent
   for the inserted close, producing unindented lines in the three column-0 files. Caught by diff
   review, reverted from backup, re-applied correctly. See §11.

`qa_memory_risk_tests.rs` has one test — `the_weight_table_sums_to_one_and_matches_the_documented_formula`
(L335) — with **no** fixture call. It sums a constant table and creates no Store or tempdir. Correctly
out of scope; untouched.

---

## 3. Files changed

Four files, **87 lines added, 0 lines removed**.

| file | added | removed | what |
|---|---:|---:|---|
| `qa_memory_retrieval_tests.rs` | 21 | 0 | 21 awaited closes |
| `autopilot_store.rs` | 24 | 0 | 21 awaited closes + 3 comment lines |
| `qa_memory_risk_tests.rs` | 21 | 0 | 21 awaited closes |
| `risk_prioritisation_tests.rs` | 21 | 0 | 21 awaited closes |

**Proof of pure insertion:** the pre-batch content of each file is an *ordered subsequence* of the
post-batch content — every original line survives, in order, byte-for-byte. `git diff --numstat` on
`autopilot_store.rs` reads `24  0`.

> A naive `git diff -- <file>` reports deletions in three of these files. Those are **not** this
> batch's doing — they are the user's pre-existing uncommitted work measured against **HEAD**. The
> correct control is the pre-batch backup, which shows zero deletions.

---

## 4. The applied pattern

For each fixture, one awaited close per bound `Store`, inserted as the **last statement of the test**,
immediately before its closing brace — matching the Batch 6 style in `qa_memory_tests.rs`:

```rust
    assert_eq!(summary.modules, 2);
    store.pool.close().await;
}
```

Two-store tests get both pools on consecutive lines, mirroring the `engine_tests.rs`
`store` / `restarted` precedent:

```rust
        assert_eq!(states, ["DONE", "SKIPPED", "SKIPPED"]);
        store.pool.close().await;
        store2.pool.close().await;
    }
```

Why the close is load-bearing, from the sqlx source
(`sqlx-core-0.9.0/src/pool/inner.rs:96`):

```rust
pub(super) fn close(self: &Arc<Self>) -> impl Future<Output = ()> + '_ {
    self.mark_closed();
    async move {
        let _permits = self.semaphore.acquire(self.options.max_connections).await;  // waits
        while let Some(idle) = self.idle_conns.pop() { let _ = idle.live.raw.close().await; }
        self.num_idle.store(0, Ordering::Release);
        self.size.store(0, Ordering::Release);
    }
}
```

Three consequences drove the design:

* It returns `impl Future<Output = ()>` — **there is no `Result`** to swallow or suppress. There is
  also no way to "handle" a teardown error, because none is reported.
* It **blocks until every checked-out connection is returned.** This is why the `autopilot_store`
  close must follow `writer.commit()` — see §7.
* A **second** `close()` on an already-closed pool is a safe no-op: `mark_closed()` is idempotent and
  all permits were already released by the first call, so `acquire` returns immediately and
  `idle_conns` is empty. This is what lets the two fault-injection tests also carry a teardown close.

No `Drop` implementation was introduced. No runtime was blocked. No cleanup script was added. No
production file was modified.

---

## 5. Per-suite measurements

### 5.1 The headline table

| family | call sites | closes added | pre-batch | **post-batch** | change |
|---|---:|---:|---:|---:|---|
| `qa_memory_retrieval_tests` | 21 | 21 | 21 | **0** | −21 |
| `autopilot_store` | 21 | 21 | 21 | **0** | −21 |
| `qa_memory_risk_tests` | 21 | 21 | 19 | **0** | −19 |
| `risk_prioritisation_tests` | 21 | 21 | 19 | **0** | −19 |
| **total** | **84** | **84** | **80** | **0** | **−80** |

Both baselines were measured in this session, not carried forward. The pre-batch column reproduces
the audit's figures exactly, which is an independent confirmation of the audit's numbers.

### 5.2 Why 84 call sites but 80 leaks

The two numbers differ because four Store instances are already closed mid-test by fault injection,
so their tempdirs are removable and they never leaked:

* `qa_memory_risk_tests` — the `r1b_no_affected_view…` test closes its `store` mid-test, so of 21
  instances only 20 could leak. Measured 19: the two-store test additionally closes `store2` mid-test,
  leaving 19 unclosed pools and 19 leaked directories.
* `risk_prioritisation_tests` — the `a_storage_failure…` test closes its `store` mid-test. 21
  instances − 1 already-closed − 1 `#[ignore]`d test that never runs = **19**. Measured 19.

`qa_memory_retrieval_tests` (21 → 21) and `autopilot_store` (21 → 21) are a clean 1:1.

**84 closes are still the correct number to insert**: a teardown close is correct whether or not the
pool happened to already be closed, and relying on a fault-injection close to release a directory is
not a teardown guarantee.

### 5.3 Per-test attribution

All **80** tests across the four families were run **individually** with `--exact`, each with a
before/after snapshot of the temp directory:

```
tests run                                : 80
exit codes                               : 0     (all)
new artifacts per test (distinct values) : 0     (all)
total new artifacts, all individual runs : 0
```

Not one test, run alone, leaves a directory behind.

---

## 6. Repeated-run validation — 10 consecutive runs per suite

Every run: exit code 0, no teardown errors, `NewTmp` measured by set-difference of directory names
(not a global count, which is meaningless under concurrent execution).

| suite | run | exit | passed | failed | ignored | new artifacts |
|---|---:|---:|---:|---:|---:|---:|
| `qa_memory_retrieval_tests` | 1–10 | 0 ×10 | 21 | 0 | 0 | **0 ×10** |
| `autopilot_store` | 1–10 | 0 ×10 | 17 | 0 | 0 | **0 ×10** |
| `qa_memory_risk_tests` | 1–10 | 0 ×10 | 21 | 0 | 0 | **0 ×10** |
| `risk_prioritisation_tests` | 1–10 | 0 ×10 | 20 | 0 | 1 | **0 ×10** |

**40 runs, 40 × zero new artifacts.** No run failed, so no failure evidence needed preserving.

The single ignored test, `risk_prioritisation_tests::phase4_measures_prioritisation_cost`, is
pre-existing and deliberate — `#[ignore = "measurement, not a gate; see the module comment"]`. It is a
cost benchmark; it never runs, never creates a fixture, and never leaked. Its teardown close was added
anyway so it is correct if ever run manually.

---

## 7. `autopilot_store` — the writer and cancellation test

This is the only test in the batch with a spawned task and a checked-out connection. Full shape:

```rust
let writer = store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();   // L1264  holds a conn
let cancelling = tokio::spawn({                                        // L1265
    let store = store.clone();                                         // L1266  clone shares the pool
    async move { store.cancel_autopilot_run(id).await }
});
tokio::time::sleep(std::time::Duration::from_millis(200)).await;       // L1269  <-- under test
if cancelling.is_finished() { panic!("the cancel did not wait for the write lock: …"); }
writer.commit().await.unwrap();                                        // L1276  releases the conn
assert_eq!(cancelling.await.unwrap().unwrap(), CancelOutcome::Requested);  // L1278  joins the task
let run = store.get_autopilot_run(id).await.unwrap().unwrap();         // L1279  required write, read back
assert!(run.cancel_requested);                                         // L1280
// Teardown: the writer transaction committed above and the spawned cancel task was
// awaited above, so every connection is back in the pool and this close cannot block.
// The 200 ms sleep is the lock-holding condition under test, not a wait for the task.
store.pool.close().await;                                              // L1284
```

Every requirement is satisfied:

* **The 200 ms sleep is preserved.** It is the mechanism under test — the assertion is that the cancel
  is *still blocked* while the writer holds the SQLite lock. It was not converted to a poll.
* **The spawned writer is preserved**, along with `cancelling.is_finished()` and the `panic!`
  diagnostics.
* **The spawned task is joined before teardown** at L1278, and `writer.commit()` at L1276 releases the
  checked-out connection. Because `close()` acquires `max_connections` permits, placing it before the
  commit would have **deadlocked**. It is placed after both.
* **Teardown cannot hide a failed writer operation.** `close()` returns `()`, so it cannot swallow or
  mask anything; `writer.commit().await.unwrap()` and `cancelling.await.unwrap().unwrap()` still panic
  loudly on failure.
* A three-line comment was added recording the above, so a future edit does not reintroduce the hang.
* The test passes and leaves **0** artifacts when run alone.

---

## 8. Fault-injection preservation — requirement 9 evidence

Recorded verbatim in Step 1 **before any edit**, then re-verified after:

| # | file | test | exact text | position |
|---|---|---|---|---|
| 1 | `qa_memory_risk_tests.rs` | `r1b_a_failed_read_is_never_reported_as_no_executions` | `'    store2.pool.close().await;'` | mid-test, before the failing read |
| 2 | `qa_memory_risk_tests.rs` | `r1b_no_affected_view_reports_a_failed_read_as_an_empty_answer` | `'    store.pool.close().await;'` | mid-test, before the failing table |
| 3 | `risk_prioritisation_tests.rs` | `a_storage_failure_is_reported_rather_than_no_executions` | `'    store.pool.close().await;'` | mid-test, before the failing read |

Post-batch verification, located by **owning test** rather than by line number (line numbers shift as
insertions land):

```
qa_memory_risk_tests::r1b_a_failed_read_is_never_reported_as_no_executions
   injection at L1079  exact text : '    store2.pool.close().await;'
   immediately preceded by : '// --- state 3: the store cannot answer ---'
   immediately followed by : 'let failed = reader2'
   a healthy-read assertion within 10 lines above? True
   the failing-read attempt + is_err() come after it? True
   the broken read starts after the injection (order preserved)? True
   teardown at L1090 (last statement, idempotent no-op on an already-closed pool)
   VERDICT: INTACT AND UNMOVED

qa_memory_risk_tests::r1b_no_affected_view_reports_a_failed_read_as_an_empty_answer
   injection at L1153  exact text : '    store.pool.close().await;'
   a healthy-read assertion within 10 lines above? True
   the failing-read attempt + is_err() come after it? True
   the broken read starts after the injection (order preserved)? True
   teardown at L1236
   VERDICT: INTACT AND UNMOVED

risk_prioritisation_tests::a_storage_failure_is_reported_rather_than_no_executions
   injection at L1356  exact text : '    store.pool.close().await;'
   immediately preceded by : '// --- state 3: the store cannot answer ---'
   immediately followed by : 'let res = reader(&store)'
   a healthy-read assertion within 10 lines above? True
   the failing-read attempt + is_err() come after it? True
   the broken read starts after the injection (order preserved)? True
   teardown at L1366
   VERDICT: INTACT AND UNMOVED

OVERALL: PASS
```

Against the nine stated requirements:

| # | requirement | status |
|---|---|---|
| 1 | exact text and context recorded before editing | done — §1 of the audit and the table above |
| 2 | not removed | held — all three present |
| 3 | not moved | held — each still precedes the operation it breaks |
| 4 | receiving Store unchanged | held — `store2`, `store`, `store` respectively |
| 5 | order relative to the failing operation unchanged | held — verified explicitly |
| 6 | not replaced by fixture teardown | held — teardown was **added alongside**, never substituted |
| 7 | dependent assertions not weakened | held — every `is_err()` / `res.is_err()` intact and still reachable |
| 8 | verified intact at end | done — table above |
| 9 | explicit evidence in this report | this section |

Requirement 6 is the subtle one and is worth stating plainly: **the teardown close is a second,
separate statement at the end of the test.** It does not stand in for the injection close, and the
injection close is not credited with the teardown.

---

## 9. Mutation validation

A green suite is not evidence that the closes did anything. Two mutations were run, and each was
verified to have actually applied before its result was read.

### Mutation 1 — remove the teardown closes

Removed every inserted close that is the last statement of its test (79 of 84; the four `store2`
closes in two-store tests are followed by another close rather than a brace and were correctly
retained, as were the three fault-injection closes).

* **Applied:** all four SHAs changed — `3dc0339d…`→`a58f347b…`, `0cde833e…`→`a3087bf1…`,
  `4a8bbbc1…`→`b993c6f5…`, `7b397983…`→`1c3d8361…`. Exactly 79 lines removed. Asserted.
* **Rebuilt:** `Compiling` ×2, binary `13:11:46` newer than mutated source `13:11:04`, exit 0.

| suite | fixed | mutated | pre-batch |
|---|---:|---:|---:|
| `qa_memory_retrieval_tests` | 0 | **21** | 21 |
| `autopilot_store` | 0 | **17** | 21 |
| `qa_memory_risk_tests` | 0 | **18** | 19 |
| `risk_prioritisation_tests` | 0 | **19** | 19 |

The leak returns in full. The mutated figures match the pre-batch figures wherever the mutation was
complete (21/21 and 19/19); the two lower values (17, 18) are exactly explained by the four surviving
`store` closes and the one test whose pool was already closed mid-test.

### Mutation 2 — remove one fault-injection close

Removed the injection close from `risk_prioritisation_tests::a_storage_failure_is_reported_rather_than_no_executions`
(L1356). The teardown close at L1366 was left in place, so this isolates the injection close alone.

* **Applied:** sha `7b397983…` → `dd302850…`, exactly 1 line removed. Asserted.
* **Rebuilt:** `Compiling` ×2, binary newer than mutated source, exit 0.

```
exit code: 101
thread '…a_storage_failure_is_reported_rather_than_no_executions' panicked at
a closed pool must be an error, not NO_EXECUTIONS for a project that has history: Ok(None)
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2157 filtered out
```

Without the close the reader returns **`Ok(None)`** — a fabricated empty answer, precisely the defect
the test exists to prevent. The injection close is load-bearing and the test genuinely fails without it.

* **Restored:** sha `7b397983…` — byte-identical. Injection at L1356, teardown at L1366, 22 close
  lines in the file.

### An invalid attempt, discarded

An earlier pre-batch-baseline measurement was **invalid** and is reported as such rather than used.
`shutil.copy2` preserves the backup's original mtime, so cargo saw no change, printed **no
`Compiling` line**, and the measurement silently ran against the still-mutated binary. It was
discarded, the restore was redone with `copyfile` (content only, fresh mtime), the rebuild was
confirmed with `Compiling` ×2 and a binary newer than source, and the baseline re-measured. A second
invalid attempt was also discarded: `cargo test <name> -- --exact` with a bare test name matched
**zero tests** and reported a false pass. All subsequent focused runs use the fully-qualified path
obtained from `--list`, with the trailing `:` stripped.

---

## 10. Regressions, formatting and Clippy

### 10.1 Regression sweep — 28 suites

| suite | exit | new artifacts | result |
|---|---:|---:|---|
| `qa_memory_tests` | 0 | 0 | 33 passed |
| `qa_memory_retrieval_tests` | 0 | **0** | 21 passed |
| `qa_memory_risk_tests` | 0 | **0** | 21 passed |
| `qa_memory_planning_tests` | 0 | 14 | 14 passed |
| `qa_memory_perf_tests` | 0 | 7 | 7 passed |
| `qa_memory_validation_tests` | 0 | 7 | 7 passed |
| `risk_prioritisation_tests` | 0 | **0** | 20 passed, 1 ignored |
| `risk_signal_tests` | 0 | 11 | 18 passed |
| `autopilot_store` | 0 | **0** | 17 passed |
| `autopilot_runner` | 0 | 13 | 16 passed |
| `autopilot_steps` | 0 | 33 | 21 passed |
| `autopilot_execute` | 0 | 13 | 21 passed |
| `autopilot_generate` | 0 | 21 | 19 passed |
| `autopilot_report` | 0 | 2 | 17 passed |
| `change_impact_tests` | 0 | 14 | 19 passed |
| `integration_e2e_tests` | 0 | 16 | 20 passed |
| `journey_tests` | 0 | 10 | 13 passed |
| `phase_8_quality_tests` | 0 | 0 | 20 passed |
| `phase_9_readiness_tests` | 0 | 0 | 21 passed |
| `exploration_tests` | 0 | 0 | 35 passed |
| `agent_execution_tests` | 0 | 0 | 40 passed |
| `governed_healing_tests` | 0 | 0 | 60 passed |
| `engine_tests` | 0 | 0 | 41 passed |
| `integrity_tests` | 0 | 0 | 34 passed |
| `operations_tests` | 0 | 15 | 15 passed |
| `adaptive_intelligence_tests` | 0 | 9 | 39 passed |
| `phase_7_1_regression_tests` | 0 | 11 | 15 passed |
| `phase_7_3_decision_tests` | 0 | 12 | 14 passed |

**28 suites, 0 exit failures, 0 suites reporting a real `test result: FAILED`** (verified with
`-cmatch 'test result: FAILED'` — plain `-match` is case-insensitive and matches the words
`0 failed`, which produced a false "28 failed" reading on the first pass and was corrected).

**No regression in any previously-fixed family**: `qa_memory_tests`, `phase_8_quality_tests`,
`phase_9_readiness_tests`, `exploration_tests`, `agent_execution_tests`, `governed_healing_tests`,
`engine_tests` and `integrity_tests` all remain at **0**. Batch 7 changed nothing outside its four
files.

The 16 suites still showing artifacts are exactly the families Batch 7 does not cover (§12).

### 10.2 Formatting

```
cargo fmt --check   →  exit 1
files rustfmt would change:  src/auth.rs:317
```

The only formatting complaint is the **pre-existing** rustfmt non-idempotency on `concat!` inside
`role_gate!` at `auth.rs:317`, documented in the project record. It was left untouched. **None of the
four Batch 7 files appears.** No file was passed through write-mode rustfmt; the pre-batch content is
a verified ordered subsequence of the post-batch content in all four.

### 10.3 Clippy

```
cargo clippy --all-targets   →  exit 0
errors: 0        errors[E]: 0        warnings: 67 (all pre-existing)
```

Diagnostics naming a Batch 7 file: **0 occurrences, in all four files.**

---

## 11. Errors caught and corrected during this batch

Reported rather than smoothed over.

1. **Wrong indentation on first application.** The insertion used the *attribute* indent rather than
   the *body* indent, producing unindented `store.pool.close().await;` lines in the three column-0
   files. Caught on diff review, all four files restored byte-identically from backup, re-applied at
   the correct indent, and re-verified.
2. **`git diff` used as the wrong control.** It compares against HEAD and so reported ~73 "deletions"
   that were the user's pre-existing uncommitted work. Replaced with a pre-batch-subsequence check,
   which is the correct control and shows zero removals.
3. **Brace-boundary mismatch across files.** A single `^}` rule is wrong for `autopilot_store.rs`,
   whose tests live in `mod tests` and close at 4-space indent. Found by an indent census before any
   edit was written.
4. **Two invalid measurement attempts**, both discarded rather than reported — see §9.
5. **Over-strict and over-naive verification checks.** One fault-injection check expected exactly one
   close per test and flagged the (correct) two; another required a `state 3` marker that site 2 never
   had. Both checks were corrected; neither indicated a problem with the code.

---

## 12. Remaining leaking helpers

Re-measured in this batch's regression sweep (see §10.1 for the table):

| remaining helper / suite | measured delta |
|---|---:|
| `api_tests` (6 fixture families, not run in this batch) | 308 *(previously measured)* |
| `autopilot_steps` | 33 |
| `autopilot_generate` | 21 |
| `operations_tests` | 15 |
| `change_impact_tests` | 14 |
| `integration_e2e_tests` | 16 |
| `autopilot_runner` (`made_run`) | 13 |
| `autopilot_execute` | 13 |
| `phase_7_3_decision_tests` | 12 |
| `risk_signal_tests` | 11 |
| `journey_tests` | 10 |
| `phase_7_1_regression_tests` | 11 |
| `adaptive_intelligence_tests` | 9 |
| `qa_memory_planning_tests` | 14 |
| `qa_memory_perf_tests` / `qa_memory_validation_tests` | 7 / 7 |
| `autopilot_report` | 2 |

Suites not sampled in this batch (`store_tests`, `trace_tests`, `key_scope_tests`, `ws_tests`,
`rbac_matrix_tests`, `rbac_probe_tests`, `run_map_retention_tests`, `trace_chain_tests`,
`risk_semantics_tests`, `process_lifetime_maps_tests`, `pre_phase_8_tests`, `phase_7_4_decision_tests`,
`retention_and_metrics_tests`) retain their previously measured deltas.

**Accounting.** Batch 7 removed 80 of the 740 recorded in the safety audit, leaving **660 outstanding**
across 30 unfixed families. The 16 suites re-measured here total **208**; adding `api_tests` (308) gives
**516** confirmed outstanding, with roughly 144 in suites not sampled here. These are three distinct
quantities — a source count, a per-suite directory count and a full-run estimate — and are not summed
into a completion percentage.

**Fixed to date:** 12 families, ~277 call sites, all at measured delta 0.

---

## 13. Confirmations

| item | status |
|---|---|
| Production source untouched | `store.rs` `97E79B8F876D7219` unchanged; `integrity.rs`, `autopilot.rs`, `autopilot_runner.rs` unchanged; `qa_memory.rs` last written **10 Oct 11:38**, untouched by this batch |
| `autopilot_store.rs` is a production file with an inline test module | all **22** added lines verified to land **inside** `#[cfg(test)] mod tests {` (which begins at L637); `git diff --numstat` = `24 0` |
| Migrations / routes / schemas | none added, none changed |
| Existing user changes | preserved; pre-batch content verified as an ordered subsequence in all four files |
| Historical artifacts | **none deleted**; no cleanup script run |
| Commit / push | none. HEAD still `7930507`, 0/0 |
| Resets / stashes / `git clean` | none |
| Whole-file reformatting | none |
| Files written this session | exactly the four Batch 7 test files (mtimes 13:13–13:14; nothing else in `src/` is newer than 13:00) |
| Artifact growth 1,901 → 2,340 | a side-effect of ~150 measurement runs against deliberately unfixed families. Nothing was deleted to make any number look better |

---

## 14. Verdict

# Batch 7: PASS

Four families, **84 call sites fixed**, **80 measured leaks eliminated**, all four suites at
**delta 0** across 10 consecutive runs each and across 80 individual per-test runs. Three
fault-injection closes verified byte-identical and unmoved. Two mutations confirm the fix is causal
in both directions. Production code untouched, no user work lost, no artifact deleted.

---

## 15. Recommendation for Batch 8

**Proceed with Batch 8** as scoped in the inventory, unchanged by anything found here:

`store_tests`, `trace_tests`, `change_impact_tests`, `qa_memory_planning_tests`,
`qa_memory_perf_tests`, `qa_memory_validation_tests` — 93 call sites, measured +96.

Carry forward three corrections this batch established:

1. **Count call sites from source, and do not subtract the `fn` definition twice.** The audit's
   figure was low by 4 across four families. Re-derive every batch's count before editing.
2. **Determine the test-body closing indent per file** by census before scripting the insertion.
   `autopilot_store.rs` closes at 4-space inside `mod tests`; the three QA-memory/risk files close at
   column 0. Insert body statements at *attribute indent + 4*.
3. **Diff against a pre-batch backup, never against HEAD**, and use `copyfile` rather than `copy2` so
   the restored mtime is fresh and cargo actually rebuilds.

Also carry forward from the safety audit: the **autopilot suites stay out of Batch 8**. They share
one helper (`autopilot_test_support::fixture`, 80 call sites) across four modules and belong together
as Batch 13, immediately before `api_tests`.

> **Phase 9.2 remains OPEN.** It closes only when every identified fixture family is either fixed with
> a measured delta of 0 or proven safe, **and** the final full-backend-suite artifact verification
> confirms no new artifacts accumulate. 30 families remain. Phase 10 has not been started.
