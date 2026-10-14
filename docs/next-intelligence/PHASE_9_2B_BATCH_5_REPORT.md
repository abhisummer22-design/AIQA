# Phase 9.2B — Stage A, Batch 5

**Fix `engine_tests::temp_store` SQLite fixture cleanup**

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — unchanged, nothing committed
**Verdict:** **PASS**
**Scope delivered:** 1 fixture helper; **33 call sites**; 8 new lifecycle tests; 1 pre-existing misclassification corrected
**Production code changed:** none

---

## 0. Three outcomes kept separate

| outcome | how it is evidenced |
|---|---|
| **1. Successful fixture cleanup** | per-run `.tmp` delta, plus direct filesystem assertions |
| **2. Successful background completion** | `run_to_completion`'s `is_finished()` polling, plus assertions on the run's persisted rows read **before** the pool closes |
| **3. Successful functional assertions** | the 33 pre-existing tests, unchanged, still passing |

**Phase 9.2 remains open.** Seven helper families are migrated; six still leak, freshly measured at
**+109** (§15). No full-backend-suite artifact check has been performed.

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 27 / 31 / **0** / 0 |
| `.tmp*` directories | **873** |
| `C:` free | **55.88 GB** |

Per the brief's historical note, the artifact directories wiped between earlier sessions **no longer
exist**, and this report makes no claim that they do. Nothing was deleted in this batch; counts only
increased (873 → 990), and that increase is from the control suites run in §15.

### 1.1 Out-of-scope files pinned by SHA before any edit

| file | SHA-256 (first 16) | status at end |
|---|---|---|
| `src/store.rs` | `97E79B8F876D7219` | **UNCHANGED** |
| `src/integrity.rs` | `EAEA60CFA367BCB6` | **UNCHANGED** |
| `src/integrity_tests.rs` | `CC1205A7FBB688A6` | **UNCHANGED** |
| `src/phase_8_quality_tests.rs` | `B5D3CA6E616187F1` | **UNCHANGED** |
| `src/phase_9_readiness_tests.rs` | `99D402A29E57EDCF` | **UNCHANGED** |
| `src/exploration_tests.rs` | `8A8980B655B0C2D5` | **UNCHANGED** |
| `src/agent_execution_tests.rs` | `6CAF99485C2CCE4B` | **UNCHANGED** |
| `src/governed_healing_tests.rs` | `08F014852495130F` | **UNCHANGED** |

### 1.2 The target file carries pre-existing uncommitted work

`engine_tests.rs` is tracked with **+16 / −2** already in the working tree — a Phase 7.3 fix splitting
a combined `set_task_status(run_id, &task_id, status, None, true, true)` into two calls
(`set_started`, then `set_finished`), because `task_history_by_name` filters on
`finished_at IS NOT NULL`. Preserved exactly (§14.3).

---

## 2. Exact helper and call-site counts

| | value |
|---|---:|
| `temp_store()` call sites | **33** |
| tests in file | 33 |
| tests using the fixture | **33** (1:1 — no test builds two) |
| binding `(store, dir)` | 21 |
| binding `(store, _dir)` | 12 |
| `pool.close()` before this batch | **4** |
| `impl Drop` in file | 1 — `impl Drop for SeededCrawl`, which only removes a result file; unrelated to pools |

The historical figure of 32 call sites was **one short**; the verified number is **33**. The
historical "28 of 32 lacked closure" is consistent with 33 sites and 5 existing closes — of which
**2 are not cleanup at all** (§4.3).

### 2.1 Baseline

```
cargo test engine_tests -> ok. 33 passed; 0 failed; 0 ignored
.tmp delta = 31
```

Matching the historical +31.

---

## 3. Root cause and ownership analysis

Unchanged in mechanism from earlier phases and re-confirmed at the pinned dependency
(`sqlx-core-0.9.0`, `src/pool/mod.rs:441`): `Store::connect` sets WAL, Windows refuses to remove a
directory whose SQLite files are open, and `tempfile` swallows that error. Only an **awaited**
`pool.close()` releases the handles; `close()` returns a `Future` that a `Drop` impl could not poll.

### 3.1 Ownership is the simplest of any batch so far

```rust
async fn temp_store() -> (Store, tempfile::TempDir) { ... }
```

| object | holds the pool? |
|---|---|
| `Store` | **yes** — `pub struct Store { pub pool: SqlitePool }` |
| `Arc<LocalEvidenceStore>` (via `evidence_store(&dir)`) | **no** — `root: PathBuf` |
| `Arc<ExecutorRegistry>` | **no** — maps of `Arc<dyn Executor>` |
| `TempDir` | owns the directory holding `engine.sqlite3` |

`spawn_run` receives `store.clone()`, but a `Pool` clone shares one pool, so one close covers all of
them. No test binds the store as `_store`, and clippy reports **no unused-variable warning** in this
file — so **all 33 sites genuinely touch the database**, and every one needed a close. No site was
skipped as "does not access a database".

---

## 4. Background-execution paths and completion signals

### 4.1 `spawn_run` really is reached here

Unlike `governed_healing_tests.rs`, this file **calls `engine::spawn_run` directly in 10 tests**, plus
once inside the `run_to_completion` helper. The `tokio::spawn` at L2264 is only inside a **doc
comment**, not code. The single `impl Drop` is `SeededCrawl`, unrelated.

### 4.2 The completion signal already exists and is already good

`run_to_completion` does not sleep a fixed amount:

```rust
spawn_run(record.clone(), registry, concurrency, store.clone(), evidence, ...);
// Poll until terminal rather than sleeping a fixed amount.
for _ in 0..300 {
    if record.engine.read().await.is_finished() {
        tokio::time::sleep(Duration::from_millis(50)).await;   // finalisation tail
        break;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
}
```

The tests that call `spawn_run` directly use the **same primitive** — they keep `record.clone()` and
poll `record.engine.read().await.is_finished()` before asserting (verified in `d4_*`, `d5_*`,
`be13_*`). `poll_healing_proposals` is a second bounded polling helper used by the Phase 10 tests.

**Per-test classification:**

| category | count | coordination |
|---|---:|---|
| `run_to_completion` users | 2 | polling on `is_finished()` |
| direct `spawn_run` users | 10 | same `is_finished()` poll (2 of them already closed) |
| Phase 10 users | 14 | `poll_healing_proposals` and/or `run_task_repeatedly` → `run_to_completion` |
| no execution started | ~7 | n/a |

### 4.3 A correction: 2 of the 4 "existing closes" are fault injection, not cleanup

This is the most important finding of the batch, and it corrects an assumption carried in from Batch 4.

`d1_approval_persistence_failure_…` and `d1_rejection_persistence_failure_…` each contain:

```rust
// Induce a genuine persistence failure by closing the store's connection pool.
store.pool.close().await;
```

Those closes exist to make the *next* write fail on purpose. They are the mechanism under test, not
teardown. Two consequences:

1. **They already prevented those two tests from leaking**, which is why the baseline was **31 and
   not 33**. My first pass therefore looked like it had fixed 29 of 31 sites and was still leaving 2.
2. Any mutation that removes *all* closes in this file must expect those two tests to fail — see §10.

### 4.4 The real last 2 leaks: a second pool on the same file

`d2_persistence_failure_…` and `d3_orphaned_task_…` simulate an orchestrator restart:

```rust
store.pool.close().await;
let restarted_store = Store::connect(&db_path).await.expect("reconnect to db");
```

`restarted_store` opens a **second pool on the same database file** and was never closed. Its handle
keeps the file open when the `TempDir` drops. These two tests looked correct — they each had a close
— but the close was on the *wrong store*.

---

## 5. Files changed

| file | change |
|---|---|
| `services/orchestrator/src/engine_tests.rs` | +29 awaited closes, +2 `restarted_store` closes, `temp_store_in()` helper, 8 lifecycle tests |
| `docs/next-intelligence/PHASE_9_2B_BATCH_5_REPORT.md` | this report (new) |

**`temp_store()` itself was not modified.** It still calls `tempfile::tempdir()`, so all 33 call sites
keep their exact directory location and naming.

---

## 6. Description of the awaited-close changes

Three distinct edits, because the site shapes are not uniform:

| # | change | count | why |
|---|---|---:|---|
| 1 | `store.pool.close().await;` as the last statement | **29** | the ordinary case: one pool, no background work outstanding |
| 2 | `restarted_store.pool.close().await;` at the end of `d2` / `d3` | **2** | a second pool on the same file, invisible to a `store`-only teardown |
| 3 | nothing, in `d1_approval` / `d1_rejection` | 0 | their close is fault injection and must stay where it is |

In total the file now carries **43** `store.pool.close().await;` (33 original tests + 8 lifecycle
tests + 2 helper uses) and **2** `restarted_store.pool.close().await;`. There are **0** un-awaited
`pool.close();` statements and no `Drop`-based shutdown.

Every close is placed **after** the test's final assertion, so no read depends on a closed pool.

---

## 7. Ten repeated-run results

| run | exit | passed / failed / ignored | new `.tmp` | panics | teardown errors |
|---:|---:|---|---:|---:|---|
| 1 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 2 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 3 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 4 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 5 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 6 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 7 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 8 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 9 | 0 | 41 / 0 / 0 | **0** | 0 | none |
| 10 | 0 | 41 / 0 / 0 | **0** | 0 | none |

**10/10 exit 0, 10/10 zero artifacts, zero panics, no retained diagnostic artifacts.** Baseline +31.

The count went 33 → 41 tests: the 33 originals all still pass, plus 8 new lifecycle tests.

---

## 8. Per-run artifact counts

```
run  1: 0     run  6: 0
run  2: 0     run  7: 0
run  3: 0     run  8: 0
run  4: 0     run  9: 0
run  5: 0     run 10: 0
```

### 8.1 Per-test attribution (this is how the last 2 leaks were found)

Every test was run individually with `.tmp` counted before and after:

```
tests still leaking: 0    total attributed: +0
```

after the `restarted_store` fix. Before that fix, the same procedure reported exactly:

```
LEAK +1  engine_tests::d2_persistence_failure_reason_is_durable_and_survives_restart
LEAK +1  engine_tests::d3_orphaned_task_under_terminal_run_is_repaired_by_recovery
```

That per-test attribution is what located §4.4; a suite-level number alone would only have shown
"2 remaining".

### 8.2 Two attribution attempts I discarded

Recorded because both produced convincing-looking numbers that were wrong:

1. **A vacuous attribution loop.** Names taken from `cargo test -- --list` carry a trailing `:`, so
   `cargo test engine_tests::foo: -- --exact` matched nothing. It reported **0 leaking tests** while
   the suite was in fact leaking 2. Caught by printing the discovered test count and a single-test
   sanity run before trusting the sweep.
2. **A no-op mutation.** The first mutation split the file at the lifecycle banner, which sits near
   the **top** of this file rather than the end, so `head` contained no closes. It reported
   `sha changed: False` and `VERIFY applied: False` — correctly, but only because the script checked
   rather than assumed. No result from that run is used as evidence.

---

## 9. Mutation evidence, with proof each mutation applied

### 9.1 Mutation 1 — remove every awaited close from the 33 original tests

| proof | value |
|---|---|
| pre-mutation SHA-256 | `5ed0f6793c13eda0da98ef54dc5555721fed9c693b3489bde8cd3a12ae96e092` |
| mutated SHA-256 | `dce87f96f3dd0b15b356c3285ab2a7aa32e3267210563fe3d9422bcac4824c2c` |
| sha changed | **True** |
| `store.pool.close()` removed | **33** |
| `restarted_store.pool.close()` removed | **2** |
| total removed | **35** = 29 added + 4 pre-existing + 2 restart |
| compile errors | **0** |
| `Compiling aiqa-orchestrator` emitted | **yes** |
| binary newer than mutated source | **True** |

**Result: `.tmp` delta = +33** (baseline 31; 33 now because the fault-injection closes are gone
too). The leak returned, decisively.

**Two tests also failed** — `d1_approval_persistence_failure_…` and
`d1_rejection_persistence_failure_…`. I did not assume a cause: I read the failing test and found
L907, *"Induce a genuine persistence failure by closing the store's connection pool."* The mutation
removed the very statement those tests use to inject their fault. **Expected, and it corroborates
§4.3.** It is not a regression introduced by this batch.

### 9.2 Mutation 2 — remove the completion polling from `run_to_completion`

| proof | value |
|---|---|
| pre-mutation SHA-256 | `5ed0f6793c13eda0da98ef54dc5555721fed9c693b3489bde8cd3a12ae96e092` |
| mutated SHA-256 | `15a353037de3f31ec9d66deca5d5ffaabc9ebc9c9d8c0dab2adfe8c72968f80c` |
| sha changed | **True** |
| compile errors | **0** |
| binary newer than mutated source | **True** |

**Result: `FAILED. 39 passed; 2 failed`** — `b4_started_at_reflects_real_execution_start_under_concurrency`
and `b5_hung_executor_times_out_releases_permit_and_run_continues`, the two `run_to_completion`
consumers. Cleanup delta 2 (only the failed tests' directories).

This proves outcome (2) is load-bearing: without the terminal-state poll, the tests that depend on the
run's writes fail.

**Honest limitation:** the two failures are **pre-existing** tests, not my new lifecycle test. My
`p92b5_spawned_run_writes_persist_before_the_pool_closes` still passed under this mutation, because
the mutated helper retains a 50 ms sleep that is enough for its assertions. So the new test is a
useful guard but is **not** the sole detector of lost completion.

### 9.3 Restoration

After each mutation: restore from that mutation's uniquely named backup, verify **sha match** and
**byte-identical**, confirm **0** `MUTATION-PROBE` residue, then delete the backup and its `.sha`
sidecar. Final shipped SHA: `180c9f31f92a0f63…` (post-format). No mutated source or binary remains.

---

## 10. Evidence that required background writes completed

### 10.1 One of my own tests failed before this could be established

`p92b5_spawned_run_writes_persist_before_the_pool_closes` initially asserted that a `RunCompleted`
event existed. It failed — and the failure output showed the events **were** all there:

```
the run's completion event must be persisted before teardown;
got [("RUN_SCHEDULED",), ("TASK_STARTED",), ("TASK_STARTED",),
     ("TASK_PASSED",), ("TASK_FAILED",), ("RUN_COMPLETED",)]
```

My assertion used `RunCompleted`; the stored form is `RUN_COMPLETED`. Full output preserved at
`%TEMP%\b5_fail2.txt`. **The failure was itself the evidence that the background writes landed** —
which is exactly what the test exists to prove.

### 10.2 What the central test proves, and in what order

It drives a real run through the suite's own `run_to_completion`, then, **while the pool is still
open**:

1. asserts `get_run_summary(run_id).state == RunState::Failed` — the run-level write landed;
2. asserts both task rows exist **and** carry per-task outcomes (`Passed` and `Failed`) — the
   per-task writes landed, not merely the run row;
3. asserts a `RUN_COMPLETED` event is present in `run_events` — the audit trail landed;

and only then calls `store.pool.close().await`.

Because every read happens before the close, **a closed pool cannot make a failed background write
look like a passing cleanup test.**

### 10.3 A test for the defect this batch actually found

`p92b5_a_second_pool_on_the_same_file_must_also_be_closed` reproduces the `d2`/`d3` shape — close one
pool, reconnect to the same path, use it, and assert the directory still disappears. It fails if a
second pool is left open, which is precisely how the last two leaks survived a fix that looked
complete.

---

## 11. Formatting and Clippy

| check | result |
|---|---|
| `cargo fmt --check` | **1 diff: `src/auth.rs:317` only** |
| `cargo clippy --all-targets` | **0 errors** |
| clippy lints in `engine_tests.rs` | **0** |

No write-mode `rustfmt` was run against the file. A copy was formatted; the changed ranges were
`224–226, 231–235, 274, 287, 454` — 11 lines, all inside this batch's additions — and the decisive
check was a multiset comparison against the pre-edit backup:

**`PRE-EDIT lines lost or altered by the format pass: 0`**

So the pre-existing `set_task_status` work survived the whole batch, including the format pass.

**`auth.rs:317` is pre-existing** (rustfmt non-idempotent on a `concat!` inside `role_gate!`, proven
at HEAD in an earlier phase) and was left alone.

---

## 12. Production code was not changed

All eight out-of-scope files are SHA-verified unchanged (§1.2). Specifically:

* no `store.rs`, `integrity.rs`, `integrity_tests.rs` change
* no change to the Phase 8/9, `exploration`, `agent_execution` or `governed_healing` fixtures
* **no production engine change.** `spawn_run`'s fire-and-forget design was observed and left alone;
  it is a production decision, outside this batch's authority (§17.1)
* no migrations or routes added
* no `unsafe`, no runtime blocking, no global cleanup script
* no `Drop`-based async shutdown (0 occurrences)
* no commit, no push; HEAD still `7930507`, 0 ahead / 0 behind

---

## 13. Pre-existing work remained intact

| check | result |
|---|---|
| `.expect("start task")` (new `set_task_status` split) | **1** — present |
| `.expect("finish task")` | **1** — present |
| pre-edit lines lost or altered across the whole batch | **0** |
| lines removed from the file | **0** — the diff is pure insertion |
| original tests | **33**, all still present and passing |
| assertions removed, weakened or skipped | **none** |
| stale backups from earlier batches used for restoration | **none** |

---

## 14. Remaining leaking fixture helpers

All re-measured this batch.

| suite | helper | delta | status |
|---|---|---:|---|
| `integrity_tests` | `harness` | 0 | fixed 9.2A |
| `phase_8_quality_tests` | `Db::new` | 0 | fixed Batch 1 |
| `phase_9_readiness_tests` | `Db::new` | 0 | fixed Batch 1 |
| `exploration_tests` | `harness` | 0 | fixed Batch 2 |
| `agent_execution_tests` | `setup_harness` | 0 | fixed Batch 3 |
| `governed_healing_tests` | `setup_healing_harness` | 0 | fixed Batch 4 |
| `engine_tests` | `temp_store` | **0** | **fixed Batch 5** |
| `qa_memory_tests` | `temp_store` | **+25** | untouched |
| `autopilot_store` | `temp_store` | **+21** | untouched |
| `qa_memory_retrieval_tests` | `temp_store` | **+21** | untouched |
| `trace_tests` | `temp_store` | **+18** | untouched |
| `change_impact_tests` | `temp_store` | **+14** | untouched |
| `retention_and_metrics_tests` | `setup` | **+10** | untouched |
| `api_tests` | `test_state` / `test_state_with_compiled_api` | not measured | untouched |

**Remaining measured total: +109** (down from +140 at the start of this batch). `api_tests` remains
unmeasured.

### 14.1 Production API blockers

**None.** No helper deferred.

---

## 15. Verdict

# Phase 9.2B Batch 5: **PASS**

| claim | evidence |
|---|---|
| 33 call sites fixed (history said 32) | counted from source; per-test attribution sweep |
| **1. Cleanup** | +31 → 0; 10/10 runs; per-test sweep: 0 leaking |
| **2. Background completion** | `run_to_completion` polls `is_finished()`; Mutation 2 fails 2 tests without it |
| **3. Functional assertions** | 33 originals unchanged and passing; 0 lines removed |
| The last 2 leaks found and fixed | §4.4 — a second pool on the same file; guarded by a dedicated test |
| A carried assumption corrected | §4.3 — 2 "existing closes" are fault injection, not cleanup |
| Awaited, never discarded | 43 awaited `store` closes + 2 `restarted_store`; **0** un-awaited; **0** `impl Drop` |
| Mutations verified as applied | sha changed, close counts 33+2=35, `Compiling` emitted, binary newer |
| Production untouched | 8 SHA comparisons |
| Pre-existing work intact | 0 pre-edit lines lost; 0 lines removed |
| No regressions | 9 suites green, 277 tests |

**Qualification:** 7 helper families migrated; ~140 of ~700 call sites remain in `api_tests` plus the
six untouched helpers. **Phase 9.2 remains open.**

---

## 16. Recommended next step for Batch 6

**Recommended: `qa_memory_tests::temp_store` — 23 sites, measured +25.**

Rationale:

* Largest single remaining file-level leak (+25, 23% of what remains).
* Identical fixture shape to the one just fixed (`temp_store() -> (Store, TempDir)`), so the Batch 5
  pattern transfers directly.
* No background execution in that helper — it is `qa_memory` reads/writes, so no completion
  coordination is needed, which keeps the batch cheap and low-risk.
* At ~0.5 s per suite run, the 10-run plus mutation loop stays inexpensive.

Suggested order after that: `autopilot_store::temp_store` (20, +21), `qa_memory_retrieval_tests::temp_store`
(20, +21), `trace_tests::temp_store` (18, +18), `change_impact_tests::temp_store` (16, +14),
`retention_and_metrics_tests::setup` (9, +10).

**After those six, `api_tests` should be its own batch**, and that is where the **final full-backend-suite
artifact check** belongs — it should be run only once those are migrated.

---

## 17. Observations recorded, not acted on

Neither is in scope for a test-fixture batch; both are logged so they are not lost.

1. **`engine::spawn_run` returns no `JoinHandle`.** No caller *can* await it directly; every consumer
   must poll `record.engine.is_finished()`. This is why the suite has `run_to_completion` and why 10
   tests carry their own polling loops. Changing it is a **production** decision.
2. **`d1_approval` / `d1_rejection` close their pool mid-test as fault injection.** Correct and
   deliberate, but it means those two tests' teardown is *not* the one that frees their files — they
   rely on that early close happening before the `TempDir` drops. Any future refactor that moved the
   close would silently reintroduce two leaks.

### 17.1 Process rules for Batch 6

* **Sanity-check any bulk script before trusting its output.** A `--list` trailing colon made an
  attribution sweep silently match zero tests and report "0 leaks" while the suite leaked 2 (§8.2).
* **Verify a mutation applied before reading a delta** — check the sha, the removal count, a fresh
  `Compiling`, and a binary-newer-than-source comparison.
* **Attribute leaks per test, not just per suite.** The last two leaks were only findable that way.
* **Check whether an existing close is cleanup or fault injection** before assuming a test does not
  leak.
* **Look for a second store on the same path** in any test that simulates a restart; a `store`-only
  teardown will miss it.
* **Do not trust a green exit code** — Mutation 1 leaked 33 directories while the suite was otherwise
  healthy.
* **Multiset-check formatting against a pre-edit snapshot**, and treat a non-zero count as "investigate",
  not "accept" (Batch 4).

---

## 18. Unchanged open items

* **D3** — two `failure_severity` implementations; **not** claimed equivalent.
* **D5** — the 10 ms duration floor is provisional; empirical inter-write timing unmeasured.
* **D6** — `set_task_status` enum redesign deferred. (The pre-existing `set_task_status` seeding fix in
  this file is a *different*, already-landed change and was preserved untouched.)
* **Staleness criterion** for readiness test #6 — undefined in the roadmap.
* **Roadmap deliverables still owed:** `PHASE_7_PASS.md`, `PHASE_8_PASS.md`, `PHASE_9_PASS.md`,
  `PHASE_10_QA_COPILOT_REPORT.md`.
* **Phase 9 qualification:** still recommended as `PASS WITH DEFERRED ACCEPTANCE CRITERIA`.
* **Production `Store::close()`** remains **unauthorized** and unimplemented. Measured cost so far:
  **7 helpers, 175 call sites, 0 production changes.**
* **Final full-backend-suite artifact verification** has not been run. Phase 9.2 is not closed without it.

**Batch 6 was not started. Phase 10 was not started.**