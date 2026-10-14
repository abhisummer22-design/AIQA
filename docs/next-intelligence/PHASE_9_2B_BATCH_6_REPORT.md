# Phase 9.2B — Stage A, Batch 6

**Fix `qa_memory_tests::temp_store` SQLite fixture cleanup**

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — unchanged, nothing committed
**Verdict:** **PASS**
**Scope delivered:** 1 fixture helper; **24 call sites** + 1 inline fixture with two pools; 8 new lifecycle tests
**Production code changed:** none

---

## 0. Scope statement

| outcome | evidence |
|---|---|
| **1. Successful fixture cleanup** | per-run `.tmp` delta, plus per-test attribution and direct filesystem assertions |
| **2. Background-work completion** | not applicable here — verified by inspection that no background work exists (§6) |
| **3. Successful functional assertions** | the 25 pre-existing tests, unchanged, still passing |

**Phase 9.2 remains open.** Eight helper families are migrated; seven still leak, measured at
**+122** (§15), and this batch **discovered two previously unrecorded leaking suites** (§15.1).
The final complete-backend-suite artifact check has not been run.

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 27 / 32 / **0** / 0 |
| `.tmp*` directories | **990** |
| `C:` free | **55.66 GB** |

Per the brief's historical note, the artifact directories removed between earlier sessions **no longer
exist**; this report makes no claim that they do. Nothing was deleted in this batch — counts only rose
(990 → 1248), and that rise is entirely from the control suites in §15.

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
| `src/engine_tests.rs` | `180C9F31F92A0F63` | **UNCHANGED** |

The target file is **tracked and clean at HEAD** (empty `git diff --numstat`), so `git diff` is
authoritative and there was no pre-existing uncommitted work in it to protect.

---

## 2. Exact helper and call-site counts

| | value |
|---|---:|
| `temp_store()` call sites | **24** |
| tests in file | 25 |
| tests using `temp_store()` | **24** |
| binding shape | `let (store, _d) = temp_store().await` ×**24** — completely uniform |
| `pool.close()` before this batch | **0** |
| `impl Drop` in file | **0** |
| additional fixtures not using `temp_store()` | **1** (see §4) |

The historical figure of 23 was one short; the verified number is **24**. All 24 share one binding
shape, so one insertion pattern applies uniformly.

---

## 3. Verified root cause

Unchanged in mechanism, re-confirmed at the pinned dependency (`sqlx-core-0.9.0`,
`src/pool/mod.rs:441`): `Store::connect` enables WAL; Windows refuses to remove a directory whose
SQLite files are open; `tempfile` swallows that error. Only an **awaited** `pool.close()` releases the
handles — `close()` returns a `Future` a `Drop` impl could not poll.

---

## 4. Files changed

| file | change |
|---|---|
| `services/orchestrator/src/qa_memory_tests.rs` | **+347 / −1** — 24 awaited closes, 2 closes on a second pool, `temp_store_in()` helper, 8 lifecycle tests |
| `docs/next-intelligence/PHASE_9_2B_BATCH_6_REPORT.md` | this report (new) |

**`temp_store()` itself was not modified.** It still calls `tempfile::tempdir()`, so all 24 call sites
keep their exact directory location and naming.

---

## 5. Awaited-close implementation and ownership reasoning

### 5.1 Ownership is minimal

```rust
async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("qa-memory.sqlite3")).await.expect("connect");
    (store, dir)
}
```

| object | holds the pool? |
|---|---|
| `Store` | **yes** — `pub struct Store { pub pool: SqlitePool }` |
| `TempDir` | owns `qa-memory.sqlite3` and its `-wal`/`-shm` sidecars |

No helper objects, no `Arc`, no evidence store, no registry. One pool, one owner.

### 5.2 The change

```rust
    store.pool.close().await;
```

as the last statement of each of the 24 test bodies — **after** every assertion, so no read depends on
a closed pool. The file now contains **35** awaited closes (24 original tests + 1 inner + 1 outer in the
restart fixture + 9 in lifecycle tests), **0** un-awaited `pool.close();`, and **0** `impl Drop`.

---

## 6. Background-work analysis

The previous report expected no background execution here. **That expectation was verified against the
current source rather than assumed**, and it holds:

| primitive | occurrences in the file |
|---|---:|
| `tokio::spawn` | **0** |
| `spawn_blocking` | **0** |
| `JoinHandle` | **0** |
| `tokio::time::sleep` | **0** |
| `tokio::join` | **0** |
| `thread::spawn` | **0** |
| references to `crate::engine::*`, `crate::agents::*`, `spawn_run`, `run_to_completion`, `dispatch*` | **0** |

Every test drives `Store` synchronously. There is no detached task that could still be writing when a
test body returns, so no completion signal is required and no coordination was added. Closing the pool
at the end of each test cannot truncate any work, because there is none in flight.

This was the cheapest batch so far in that respect, and it is the first batch where the
"no background work" claim was checked rather than carried forward.

---

## 7. Results of all 10 repeated runs

| run | exit | passed / failed / ignored | new `.tmp` | panics | teardown errors |
|---:|---:|---|---:|---:|---|
| 1 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 2 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 3 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 4 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 5 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 6 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 7 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 8 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 9 | 0 | 33 / 0 / 0 | **0** | 0 | none |
| 10 | 0 | 33 / 0 / 0 | **0** | 0 | none |

**10/10 exit 0, 10/10 zero artifacts, zero panics, no diagnostic artifacts retained.** Baseline **+25**.

Every required database operation is synchronous, so "evidence that required DB operations completed"
is simply the test assertions themselves, all of which run before the close and all of which pass.

Test count went 25 → 33: the 25 originals all still pass, plus 8 new lifecycle tests.

---

## 8. Per-run artifact measurements

```
run  1: 0     run  6: 0
run  2: 0     run  7: 0
run  3: 0     run  8: 0
run  4: 0     run  9: 0
run  5: 0     run 10: 0
```

### 8.1 Per-test attribution

**Before the fix**, every one of the 25 tests leaked exactly one directory (24 via `temp_store`, 1 via
the inline fixture) — consistent with the +25 suite baseline.

**After the fix**, all 33 tests were run individually:

```
tests discovered: 33
total attributed leak across all 33 tests: +0
```

### 8.2 The last leak was not a `temp_store` call site

After adding the 24 closes the suite still leaked **1**. Per-test attribution named it:

```
LEAK +1  qa_memory_tests::migrations_preserve_rows_written_before_them
```

That test does **not** use `temp_store()`. It builds its own tempdir and opens **two** stores on the
same file:

```rust
let dir = tempfile::tempdir().expect("tempdir");
let path = dir.path().join("pre-existing.sqlite3");

let run_id = {
    let store = Store::connect(&path).await.expect("connect");   // pool #1
    ...
    run_with(&store, &pid, vec![sim_task("pre-task")], None).await
};

let store = Store::connect(&path).await.expect("reconnect");      // pool #2, never closed
```

This is the **same defect class Batch 5 found in `engine_tests`' `d2`/`d3`**: dropping a `Store` does
not close its pool, so pool #1 kept the file open, and pool #2 was never closed at all.

I fixed both pools. This test is in scope because it is in the target file, shares the defect, and
leaving it would have made the suite-level verification in this report meaningless.

The one changed line is a tail expression turned into a binding:

```rust
-    run_with(&store, &pid, vec![sim_task("pre-task")], None).await
+    let id = run_with(&store, &pid, vec![sim_task("pre-task")], None).await;
+    store.pool.close().await;
+    id
```

The returned value is unchanged; no assertion moved.

### 8.3 Two measurement mistakes I made and discarded

1. **A faulty insertion script.** My first attempt matched test bodies by counting `{`/`}`, which
   mis-terminated on string literals containing braces. It added only **12** of 24 closes and put one
   in a syntactically invalid position (`cargo test --no-run` failed with `expected one of ! or ::`).
   I restored from the pre-edit backup (verified byte-identical, 0 closes present, 0 compile errors) and
   redid the insertion by locating each function's **column-0 closing brace**, which is robust to
   braces inside strings. That produced exactly 24 closes and compiled clean.
2. **A broken per-test attribution summary.** My PowerShell accumulator threw
   `Method invocation failed because [System.Object[]] does not contain a method named 'op_Subtraction'`
   and printed a misleading `leaking tests: 0`. The per-test lines it printed were correct and were
   used; the summary line was discarded and the sweep re-run with corrected arithmetic (§8.1).

---

## 9. Mutation results and proof the mutation applied

All five checks the brief requires were performed and recorded:

| # | check | value |
|---|---|---|
| 1 | pre-mutation SHA-256 | `1e77cbcad341dd0cc322a3139a2f16cf0353b57a51dc3764419d399fd5440efa` |
| 2 | mutated SHA-256 | `b002ba8877169c615bf25fcb5b6b00049bc9475bdead899cace9a73440d689c4` |
| 3 | source actually changed | **sha changed: True** |
| 4 | intended lines removed | **25** closes (24 call sites + the restart fixture's); 7 remain in the untouched lifecycle block — `VERIFY applied: True` |
| 5 | binary rebuilt from mutated source | `Compiling aiqa-orchestrator v0.1.0` emitted; binary mtime **later** than the mutated source; **0** compile errors |

**Result:**

```
cargo test qa_memory_tests -> ok. 33 passed; 0 failed; 0 ignored
.tmp delta = 25
```

The leak returned at **exactly the baseline**, while the suite passed 33/33 green. A passing exit code
is not evidence of cleanup.

### 9.1 Restoration

Restore from that mutation's uniquely named backup, then verify:

| check | result |
|---|---|
| expected SHA-256 | `1e77cbcad341dd0…` |
| restored SHA-256 | `1e77cbcad341dd0…` |
| sha match | **True** |
| byte-identical | **True** |
| closes restored | 35 |
| `MUTATION-PROBE` residue | **0** |
| experiment backup + `.sha` deleted | **True** |

No mutated source or binary remains. No stale backup from an earlier batch was used.

---

## 10. QA Memory and relevant regression results

| suite | result | `.tmp` delta |
|---|---|---:|
| `qa_memory_tests` (target) | ok. **33 passed; 0 failed; 0 ignored** | **0** |
| `qa_memory_retrieval_tests` | ok. 21 passed; 0 failed | +21 *(untouched)* |
| `qa_memory_risk_tests` | ok. 21 passed; 0 failed | +19 *(untouched, newly measured)* |
| `risk_prioritisation_tests` | ok. 20 passed; 0 failed; 1 ignored | +19 *(untouched, newly measured)* |
| `engine_tests` | ok. 41 passed; 0 failed | **0** |
| `governed_healing_tests` | ok. 60 passed; 0 failed | **0** |
| `agent_execution_tests` | ok. 40 passed; 0 failed | **0** |
| `exploration_tests` | ok. 35 passed; 0 failed; 1 ignored | **0** |
| `phase_8_quality_tests` | ok. 20 passed; 0 failed | **0** |
| `phase_9_readiness_tests` | ok. 21 passed; 0 failed | **0** |
| `integrity_tests` | ok. 34 passed; 0 failed; 1 ignored | **0** |

**No full backend suite was run**, by instruction. The final artifact verification remains scheduled
after the migration completes.

`temp_store_in()` is private to this file, so **no shared test-only helper changed** and no other
suite is affected by this batch.

### 10.1 One of my own tests failed, and the fix was to check a fact

`p92b6_fixture_is_usable_and_fully_migrated` asserted that five named tables exist and failed:

```
assertion `left == right` failed: the qa-memory fixture must run the real migrations
  left: 4   right: 5
```

My list included `releases`. I did not adjust the number to make it pass — I enumerated
`CREATE TABLE` across all 42 migrations and found **`releases` is not a table**: a release lives on the
`runs` row (`runs.release_id`, migration 0029). The assertion was corrected to five tables that
verifiably exist (`projects`, `runs`, `tasks`, `test_cases`, `execution_telemetry`). Full output
preserved at `%TEMP%\b6_fail1.txt`.

---

## 11. Formatting and Clippy

| check | result |
|---|---|
| `cargo fmt --check` | **1 diff: `src/auth.rs:317` only** |
| `cargo clippy --all-targets` | **0 errors** |
| clippy lints in `qa_memory_tests.rs` | **0** |

No write-mode `rustfmt` was run against the file. A copy was formatted; changed ranges were
`826, 926, 936, 993, 1121–1123` — 5 lines, all inside this batch's additions. The decisive check was a
multiset comparison against the pre-edit backup.

It reported **1** difference:

```
was 1x now 0x : '        run_with(&store, &pid, vec![sim_task("pre-task")], None).await'
```

That is the **intentional** edit in §8.2 (tail expression → binding), not formatting damage. It is
called out rather than quietly accepted.

**`auth.rs:317` is pre-existing** (rustfmt non-idempotent on a `concat!` inside `role_gate!`, proven
at HEAD in an earlier phase) and was left alone.

---

## 12. Production code remained untouched

All nine out-of-scope files are SHA-verified unchanged (§1.1). Specifically:

* no `store.rs`, `integrity.rs`, `integrity_tests.rs` change
* no change to any previously-fixed fixture file
* no production engine change
* no migrations or routes added
* no `unsafe`, no runtime blocking, no global temp-dir cleanup
* no `Drop`-based async shutdown (**0** occurrences)
* no commit, no push; HEAD still `7930507`, 0 ahead / 0 behind

---

## 13. Pre-existing work and historical artifacts not deleted

| check | result |
|---|---|
| target file had pre-existing uncommitted work | **none** (clean at HEAD) |
| pre-edit lines lost or altered across the whole batch | **1**, and it is the intended change in §8.2 |
| lines removed from the file | **1**, the intended one |
| original tests | **25**, all still present and passing |
| assertions removed, weakened or skipped | **none** |
| historical artifacts deleted | **none** — no `Remove-Item`, `rmdir` or sweep against `%TEMP%` at any point |
| counts | 990 → 1248 (increase only, from control suites) |

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
| `engine_tests` | `temp_store` | 0 | fixed Batch 5 |
| `qa_memory_tests` | `temp_store` | **0** | **fixed Batch 6** |
| `qa_memory_retrieval_tests` | `temp_store` | **+21** | untouched |
| `autopilot_store` | `temp_store` | **+21** | untouched |
| `qa_memory_risk_tests` | *see §14.1* | **+19** | untouched |
| `risk_prioritisation_tests` | *see §14.1* | **+19** | untouched |
| `trace_tests` | `temp_store` | **+18** | untouched |
| `change_impact_tests` | `temp_store` | **+14** | untouched |
| `retention_and_metrics_tests` | `setup` | **+10** | untouched |
| `api_tests` | `test_state` / `test_state_with_compiled_api` | not measured | untouched |

**Remaining measured total: +122.**

### 14.1 Two previously unrecorded leaking suites

While running QA Memory regression I measured two suites that earlier inventories had never listed:

* **`qa_memory_risk_tests` — +19**
* **`risk_prioritisation_tests` — +19**

Neither appears in any prior report's inventory. The "remaining measured total" therefore understated
the true remaining leakage by ~38 across an earlier count. I have not analysed their fixtures in this
batch (out of scope), but they must be added to the migration plan. **The prior totals should be
treated as a floor, not a ceiling.**

### 14.2 Production API blockers

**None.** No helper deferred.

---

## 15. Verdict

# Phase 9.2B Batch 6: **PASS**

| claim | evidence |
|---|---|
| 24 call sites fixed (history said 23) | counted from source; uniform binding shape |
| Cleanup | +25 → 0; 10/10 runs; per-test sweep **+0 across all 33 tests** |
| Background work | verified **absent** — 6 concurrency primitives, 0 engine entry points |
| Functional assertions | 25 originals unchanged and passing |
| The last leak found and fixed | §8.2 — two pools on one file, same class as Batch 5's `d2`/`d3` |
| Guard against that class | `p92b6_a_second_pool_on_the_same_file_must_also_be_closed` |
| Mutation verified as applied | sha changed, 25 closes removed, `Compiling` emitted, binary newer |
| Mutation shows the fix is load-bearing | leak returns at exactly **+25**, suite still green |
| Restoration exact | sha match, byte-identical, 0 residue, backups deleted |
| Production untouched | 9 SHA comparisons |
| Two faulty measurements discarded | §8.3 — a broken insertion script and a broken attribution summary |
| No regressions | 11 suites green, 347 tests |

**Qualification:** 8 helper families migrated; ~140 of ~700 call sites remain, concentrated in
`api_tests`. **Phase 9.2 remains open.**

---

## 16. Recommended next step for Batch 7

**Recommended: `qa_memory_retrieval_tests::temp_store` — 20 sites, measured +21.**

Rationale:

* Same fixture family and shape as the one just fixed (`temp_store() -> (Store, TempDir)`), and its
  sibling `qa_memory_tests` is now a proven, directly transferable template.
* Largest single remaining measured delta (+21).
* ~0.6 s per suite run, so the 10-run plus mutation loop stays cheap.

Suggested order after that: `autopilot_store::temp_store` (20, +21), `qa_memory_risk_tests` (+19),
`risk_prioritisation_tests` (+19), `trace_tests::temp_store` (18, +18), `change_impact_tests::temp_store`
(16, +14), `retention_and_metrics_tests::setup` (9, +10).

**Two changes of plan are warranted after this batch:**

1. **Re-survey the remaining inventory before starting Batch 7.** §14.1 shows two suites were missing
   from the tracked list. A fresh sweep of every `temp_store`-shaped helper is cheaper than
   discovering more gaps mid-sequence.
2. **`api_tests` should now be scoped explicitly.** At 116 + 66 sites it is ~40% of the remaining work
   and the only family whose return type (`AppState`) has never been ownership-analysed. It deserves
   its own plan — a dedicated ownership pass, not another template application.

The **final complete-backend-suite artifact check** belongs after `api_tests`, not before.

---

## 17. Process rules for Batch 7

* **Do not locate function boundaries by counting braces.** It breaks on string literals containing
  `{`/`}`. Use the column-0 closing brace, and verify the inserted count against the site count
  (§8.3).
* **Verify the per-test attribution sweep actually runs tests** — print the discovered count and a
  single-test sanity result before trusting a "0 leaks" summary (§8.3).
* **After adding closes, still expect a residual.** A suite-level delta of 1 is a signal to attribute
  per test, not to declare success (§8.2).
* **Look for a second store on the same path** in any test that simulates a restart — this has now
  been the last leak in two consecutive batches.
* **When an assertion about schema or types fails, enumerate the source of truth** rather than
  adjusting the expected value (§10.1).
* **Sanity-check shell arithmetic in loops**; a PowerShell array-subtraction error silently produced a
  false "0 leaking tests" summary.
* **Do not trust a green exit code** — Mutation 1 leaked 25 directories with the suite fully green.

---

## 18. Unchanged open items

* **D3** — two `failure_severity` implementations; **not** claimed equivalent.
* **D5** — the 10 ms duration floor is provisional; empirical inter-write timing unmeasured.
* **D6** — `set_task_status` enum redesign deferred.
* **Staleness criterion** for readiness test #6 — undefined in the roadmap.
* **Roadmap deliverables still owed:** `PHASE_7_PASS.md`, `PHASE_8_PASS.md`, `PHASE_9_PASS.md`,
  `PHASE_10_QA_COPILOT_REPORT.md`.
* **Phase 9 qualification:** still recommended as `PASS WITH DEFERRED ACCEPTANCE CRITERIA`.
* **Production `Store::close()`** remains **unauthorized** and unimplemented. Measured cost so far:
  **8 helpers, 199 call sites, 0 production changes.**
* **The tracked inventory of remaining helpers is incomplete** (§14.1); prior totals are a floor.
* **Final complete-backend-suite artifact verification** has not been run. Phase 9.2 is not closed
  without it.

**Batch 7 was not started. Phase 10 was not started.**