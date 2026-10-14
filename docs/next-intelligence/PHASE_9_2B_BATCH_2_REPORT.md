# Phase 9.2B — Stage A, Batch 2

**Fix `exploration_tests::harness` SQLite fixture cleanup**

**Date:** 2026-10-10
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — unchanged, nothing committed
**Verdict:** **PASS**
**Scope delivered:** 1 fixture helper; **19 call sites**; 6 new lifecycle tests
**Production code changed:** none

---

## 0. Scope statement

Phase 9.2 remains **incomplete**. One helper was fixed. The repository still leaks through at
least **four untouched helpers**, spot-measured at **+97 directories per combined run** (§13).

Overall Phase 9.2 is closed only when **all required fixture families are migrated and the complete
backend suite is verified not to accumulate new artifacts**. Neither condition is met. **No
full-suite artifact measurement was taken in this batch**, by instruction.

---

## 1. Initial repository state

Verified at the start of this batch, not assumed from the previous report.

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 25 / 28 / **0** / 0 |
| `.tmp*` artifacts | **9,227** |
| `C:` free | **46.31 GB** |

Matches the state reported at the end of Batch 1 exactly.

### 1.1 Out-of-scope files pinned by SHA before any edit

Recorded so "production code untouched" is verifiable rather than asserted:

| file | SHA-256 (first 16) | status at end of batch |
|---|---|---|
| `src/store.rs` | `97E79B8F876D7219` | **UNCHANGED** |
| `src/integrity.rs` | `EAEA60CFA367BCB6` | **UNCHANGED** |
| `src/integrity_tests.rs` | `CC1205A7FBB688A6` | **UNCHANGED** |
| `src/phase_8_quality_tests.rs` | `B5D3CA6E616187F1` | **UNCHANGED** |
| `src/phase_9_readiness_tests.rs` | `99D402A29E57EDCF` | **UNCHANGED** |

The last two carry recent-looking modification timestamps, but those are from **Batch 1**. Their
content SHAs are identical to the values pinned at the start of this batch.

---

## 2. Actual call-site count

The historical estimate was 18. **Counted from source, the true number is 19.**

| | value |
|---|---:|
| `harness()` call sites | **19** |
| tests using `harness()` | **19** (1:1 — no test creates two fixtures) |
| binding shapes | `let (store, _d, evidence) = harness().await` ×17<br>`let (store, _d, _evidence) = harness().await` ×2 |
| tests in file | 30 (29 passed + 1 ignored) |
| `pool.close()` before this batch | **0** |
| `impl Drop` in file | **0** |

### 2.1 Baseline measurement (taken before any edit)

```
cargo test exploration_tests -> ok. 29 passed; 0 failed; 1 ignored
.tmp delta = 18
```

**18 leaked directories per run** — the historical estimate was correct for the leak, but the
fixture actually has **19** call sites. One of them does not leak; §6.2 measures that precisely.

---

## 3. Verified root cause

Unchanged from Phase 9.2A and Batch 1, and re-confirmed against the pinned dependencies this
session:

1. `Store::connect` sets **WAL**, so `*.sqlite3-wal` / `*.sqlite3-shm` sidecars are held open for the
   life of the connection.
2. On Windows, `remove_dir_all` **fails** while those handles are open.
3. `tempfile` **swallows** that error, so the failure is silent and the directory leaks.

```rust
// sqlx-core-0.9.0, src/pool/mod.rs:441
pub fn close(&self) -> impl Future<Output = ()> + '_
```

`close()` is **not** an `async fn`; it returns a `Future` the caller must await. An ordinary `Drop`
impl would construct that `Future` and drop it **unpolled**, releasing nothing — a silent no-op that
compiles without warning.

---

## 4. Files changed

| file | change |
|---|---|
| `services/orchestrator/src/exploration_tests.rs` | `harness()` delegates to new `harness_in(root)`; **19** × `store.pool.close().await;`; 6 lifecycle tests appended |
| `docs/next-intelligence/PHASE_9_2B_BATCH_2_REPORT.md` | this report (new) |

**Nothing else.** `store.rs`, `integrity.rs`, `integrity_tests.rs`, and both previously-fixed
fixture files are SHA-verified unchanged.

### 4.1 This file has pre-existing uncommitted work

`exploration_tests.rs` is **tracked**, with **16 insertions / 4 deletions** of uncommitted work
already present: reflowed `reader.prioritise_tests(...)` calls that gained `.expect("prioritise_tests")`.
That work was preserved exactly (§11.2).

---

## 5. Awaited teardown

### 5.1 The change at each of the 19 call sites

```rust
        store.pool.close().await;
```

placed as the **last statement** of each test body, after every existing assertion.

### 5.2 Ownership analysis

| object | holds the pool? | note |
|---|---|---|
| `Store` | **yes** — `pub struct Store { pub pool: SqlitePool }` | the only pool, `pub` field |
| `LocalEvidenceStore` | **no** — `pub struct LocalEvidenceStore { root: PathBuf }` | filesystem only |
| `store.clone()` handles | same pool | `SqlitePool` is `Clone`; all clones share one pool |

The file contains **38 `store.clone()` call sites**, passed into `ExplorationService`,
`AgentContext`, `RunPipelineProbeRunner` and `QaMemoryReader`. These are the Step 2 concern, so
they were checked explicitly rather than assumed:

| concurrency primitive | occurrences in file |
|---|---:|
| `tokio::spawn` | **0** |
| `spawn_blocking` | **0** |
| `tokio::task` | **0** |
| `JoinHandle` | **0** |
| `futures::join` / `tokio::join` | **0** |

**No task of any kind is spawned in this file.** Every clone is a plain local that drops at the end
of its test body. Therefore no legitimate task can be using the pool when the close runs, and the
close cannot cut a live operation short. (Separately, `spawn_maintenance_tasks` uses
`Arc::downgrade`, so even where background tasks exist elsewhere they do not pin the pool.)

### 5.3 Why closing before the clones drop is safe

`Pool::close()` releases connections for the **shared** pool, so the clones held by
`ExplorationService` and friends are covered. sqlx returns connections to the pool after each
query, so nothing is checked out between statements and `close()` completes rather than blocking.

### 5.4 The `harness_in` refactor

`harness()` was made to delegate so lifecycle tests can root the fixture in a directory they own:

```rust
async fn harness() -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    harness_in(&std::env::temp_dir()).await
}

async fn harness_in(root: &std::path::Path) -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    let dir = tempfile::tempdir_in(root).expect("tempdir");
    ...
}
```

The claim "`tempfile::tempdir()` is exactly `tempdir_in(env::temp_dir())`" was **verified in the
vendored source**, not assumed. `tempfile-3.27.0`:

```rust
pub fn tempdir() -> io::Result<TempDir> { TempDir::new() }
pub fn new()     -> io::Result<TempDir> { Builder::new().tempdir() }
pub fn tempdir(&self) -> io::Result<TempDir> { self.tempdir_in(env::temp_dir()) }
```

So the 19 production call sites keep their existing directory location and naming.

---

## 6. Targeted results across all 10 runs

### 6.1 Per-run record

| run | exit code | result | new `.tmp` | teardown errors |
|---:|---:|---|---:|---|
| 1 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 2 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 3 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 4 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 5 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 6 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 7 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 8 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 9 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |
| 10 | 0 | `ok. 35 passed; 0 failed; 1 ignored` | **0** | none |

**10/10 exit 0, 10/10 zero new artifacts, zero teardown errors.** Baseline was **+18**.

Failure detection used `-cmatch 'test result: FAILED'` — case-sensitive. (`-match` is
case-insensitive and matches the literal `0 failed`; that trap was hit in an earlier phase and is
avoided here.)

### 6.2 Why the leak was 18 and not 19

The mutation experiment (§8) removed **all 19** awaited closes and the leak returned at exactly
**18**, matching the pre-edit baseline. So one of the 19 harness-using tests never opens a SQLite
handle and therefore never leaked — the same pattern found in `integrity_tests` in Phase 9.2A,
where 28 of 29 sites leaked. The fix's benefit is **18 directories per run**, and the mutation
confirms this independently.

---

## 7. Lifecycle tests added (6)

| test | Properties covered |
|---|---|
| `p92b2_fixture_is_usable_and_fully_migrated` | **A, B** — 5 core tables migrated; a real exploration session round-trips through the production writers; the evidence directory is real |
| `p92b2_exploration_state_behaviour_is_unchanged` | **C, I** — two sessions listed back; `finish_exploration_session` status is recorded verbatim; an untouched session does **not** inherit another's status; evidence store still writable |
| `p92b2_awaited_close_removes_the_directory_despite_wal_and_shm` | **D, E, F** — asserts WAL/SHM sidecars **exist while live**, then the directory is **gone** after the awaited close |
| `p92b2_repeated_fixtures_accumulate_nothing` | **G** — 10 cycles leave **0** directories |
| `p92b2_concurrent_fixtures_do_not_interfere` | **H** — 8 spawned tasks; each asserts its database existed first, then removes exactly its own |
| `p92b2_teardown_failure_is_observable` | **J** — uses fallible `TempDir::close()` and asserts `is_ok()` |

**Property I** ("existing exploration and telemetry assertions remain intact") is additionally
evidenced structurally: the original **29** tests still exist and still pass, unchanged — §11.2.

### 7.1 Isolated measurement root

```rust
fn p92b2_root(test_name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join("aiqa_p92b2_artifacts").join(test_name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create isolated artifact root");
    root
}
```

Fixtures are created with `harness_in(&root)`. **Global `.tmp*` is never used as an assertion.**
This is not pedantry: in Phase 9.2A a version that counted globally failed 2 of 6 tests purely
because sibling tests ran concurrently. Test parallelism was **not** disabled — the concurrency
test spawns 8 tasks onto the runtime and awaits them together.

### 7.2 One of my own tests failed, and it was my test's fault

First run of the new block: **34 passed, 1 failed**.

```
---- exploration_tests::p92b2_awaited_close_removes_the_directory_despite_wal_and_shm ----
panicked at src\exploration_tests.rs:2345
after an awaited `pool.close()` the directory must be gone. If it remains, an open
SQLite handle -- not the fixture -- is what blocked removal.
```

**Full output preserved** at `%TEMP%\p92b2_fail1.txt` before any rerun.

Cause: the test asserted `!path.exists()` while still holding `dir` in scope. Nothing had dropped
it, so the directory correctly still existed. The fixture was never at fault. Fixed by adding the
missing `drop(dir);` immediately after the awaited close — which is also the correct expression of
the required ordering. **35/35 pass afterwards.** The second failure file (`p92b2_fail2.txt`) was
never written because no second failure occurred.

---

## 8. Mutation validation

### 8.1 The mutation, and proof it applied

**Goal:** remove the awaited close from the 19 production call sites only, leaving the lifecycle
block intact so it could detect the regression.

```
pre-mutation sha (recorded to a sidecar .sha file): d03e5e821b672509
post-mutation sha                                : 36431240569c2b3e   -> differs
occurrences replaced                              : 19
residual 'store.pool.close().await;' after        : 6   (the lifecycle tests only)
'let _ = &store;' inserted                        : 19
cargo build --tests -> "Compiling aiqa-orchestrator v0.1.0"
test binary mtime 22:33:56 newer than mutated source: True
```

This addresses the two ways a mutation experiment can silently lie: the write must land (sha
changed) and the binary must be rebuilt (verified by a fresh `Compiling` line and an mtime
comparison, not assumed).

### 8.2 Result

```
cargo test exploration_tests -> ok. 35 passed; 0 failed; 1 ignored
.tmp delta = 18
```

**The leak returned, at exactly the pre-edit baseline of 18.**

Note what this also proves: **the suite stayed completely green with the fix removed.** A passing
exit code is not evidence that cleanup succeeded — only the artifact delta is.

### 8.3 Restoration

| check | result |
|---|---|
| Expected sha (from pre-mutation sidecar) | `d03e5e821b672509` |
| Restored sha | `d03e5e821b672509` |
| sha match | **True** |
| Byte-identical to pre-mutation backup | **True** |
| Awaited closes restored | **25** (19 production + 6 lifecycle) |
| Mutation residue (`let _ = &store;`) | **0** |
| Backup + sha sidecar deleted | **True** |
| Suite after restore | `ok. 35 passed`, **delta 0** |

The backup used a **unique name** (`c2_B2_MUT_PREMUT.rs`) with a separate `.sha` sidecar, and the
sha was recorded **before** the mutation. This is the direct lesson from Batch 1, where a stale
backup filename rolled a real fix back.

No mutated source or binary remains in the final state.

---

## 9. Regression results

| suite | result | `.tmp` delta |
|---|---|---:|
| `exploration_tests` | ok. **35 passed; 0 failed; 1 ignored** | **0** |
| `agent_tests` (exploration agent) | ok. **20 passed; 0 failed** | **0** |
| `agent_execution_tests` | ok. **33 passed; 0 failed** | +15 *(untouched helper — expected)* |
| `phase_8_quality_tests` | ok. **20 passed; 0 failed** | **0** |
| `phase_9_readiness_tests` | ok. **21 passed; 0 failed** | **0** |
| `phase_9_2a_lifecycle_tests` | ok. **6 passed; 0 failed** | **0** |
| `integrity_tests` (9.2A) | ok. **34 passed; 0 failed; 1 ignored** | **0** |

**No full backend suite was run**, by instruction; that regression is deferred until the staged
migration completes.

Phase 8/9 suites were run because they are the previous batches' output and act as a canary; they
were **not** required, as `harness_in` is private to `exploration_tests` and no shared helper
changed.

---

## 10. Formatting and Clippy

| check | result |
|---|---|
| `cargo fmt --check` | **1 diff: `src/auth.rs:317` only** |
| `cargo clippy --all-targets` | **0 errors** |
| clippy lints in `exploration_tests.rs` | **0** |

### 10.1 No write-mode rustfmt was run against the file

`cargo fmt --check` initially reported 3 diffs in `exploration_tests.rs` (lines 60, 2159, 2331),
all inside this batch's own additions. Following the Batch 1 method, the file was copied to a
temporary path, formatted there, and written back only after the change was verified as confined.

**The first containment check produced a false pass and was discarded.** Its regex for pre-existing
hunk ranges matched nothing, so it reported "safe" while having verified nothing. It was replaced
with a real check: a line-by-line diff of the current file against the pre-edit backup (§11.2), which
enumerates every changed region.

### 10.2 Pre-existing formatting differences

| difference | status |
|---|---|
| `src/auth.rs:317` | **Pre-existing.** rustfmt is non-idempotent on a `concat!` inside `role_gate!`, proven at HEAD in an earlier phase. Deliberately **not** fixed — out of scope. |
| `integrity_tests.rs` reflow (Phase 9.2A) | **Pre-existing and untouched.** SHA unchanged. |

No formatting difference in this report is attributable to this batch.

---

## 11. Diff verification

### 11.1 Every changed region

Diffing the current file against the pre-edit backup gives **21 changed regions**:

| count | kind | content |
|---:|---|---|
| **19** | one-line insert | `store.pool.close().await;` — the 19 call sites |
| **1** | deliberate refactor | `harness()` now delegates to `harness_in(&std::env::temp_dir())` |
| **1** | append | the 6 lifecycle tests, helpers and comments |

### 11.2 The one replacing region, shown in full

```rust
// BEFORE
async fn harness() -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase5-exploration.sqlite3"))
        .await
        .expect("connect");
    let evidence =
        Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence root"));
    (store, dir, evidence)
}

// AFTER
async fn harness() -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    harness_in(&std::env::temp_dir()).await
}

/// ... doc comment ...
async fn harness_in(root: &std::path::Path) -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    let dir = tempfile::tempdir_in(root).expect("tempdir");
    let store = Store::connect(&dir.path().join("phase5-exploration.sqlite3"))
        .await
        .expect("connect");
    let evidence =
        Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence root"));
    (store, dir, evidence)
}
```

This is the **only** region that removed a pre-existing line (`let dir = tempfile::tempdir()...`),
and it is the intended refactor. `Store::connect`, migrations, seeding helpers and every assertion
are untouched.

### 11.3 Pre-existing user work preserved

| pattern | pre-edit | current |
|---|---:|---:|
| `prioritise_tests` | 8 | **8** |
| `.expect("prioritise_tests")` | 4 | **4** |
| multi-line `reader\n        .prioritise_tests` | 4 | **4** |

### 11.4 Placement of the 19 closes

| check | result |
|---|---|
| closes that are **not** the last statement of their test body | **0** |
| distinct indentation forms | 1 (`'    store.pool.close().await;'`) |
| total inserted | **19** |
| early `return` statements in file | **0** |
| `?` operator lines in file | **0** |
| non-unit test signatures | 0 (3 are the non-test helpers `harness`, `project`, `open_session`) |

With no early-exit path, an end-of-body close cannot be skipped.

---

## 12. Production code and artifacts untouched

| check | result |
|---|---|
| `store.rs` | SHA **unchanged** |
| `integrity.rs` | SHA **unchanged** |
| `integrity_tests.rs` | SHA **unchanged**; never opened for writing |
| `phase_8_quality_tests.rs` / `phase_9_readiness_tests.rs` | SHA **unchanged** |
| Only `exploration_tests.rs` written in `src/` this batch | **yes** |
| Migrations or routes added | **none** |
| Commits / pushes | **none**; HEAD still `7930507` |
| Repo state | 25 modified / 28→29 untracked (+1 = this report) / **0 deleted** / 0 staged |

### 12.1 Historical artifacts preserved

| point | `.tmp*` dirs | `C:` free |
|---|---:|---:|
| Batch start | 9,227 | 46.31 GB |
| Batch end | **9,375** | **46.11 GB** |

The +148 comes entirely from the **unpatched control suites** run for measurement
(`store_tests` +36 ×2, `qa_memory_tests` +25 ×2, `qa_memory_retrieval_tests` +21 ×2,
`agent_execution_tests` +15). `exploration_tests` contributed **0** across every run recorded here.

**No `Remove-Item`, `rmdir` or sweep was issued against any historical `.tmp*` directory.** Counts
only ever increased. The only paths removed are
`%TEMP%\aiqa_p92b2_artifacts\<test_name>\`, namespaces created by this batch's own tests.

---

## 13. Remaining leaking helpers

Re-measured at the end of this batch:

| suite | helper | `.tmp` delta | status |
|---|---|---:|---|
| `integrity_tests` | `harness` | **0** | fixed in 9.2A |
| `phase_8_quality_tests` | `Db::new` | **0** | fixed in Batch 1 |
| `phase_9_readiness_tests` | `Db::new` | **0** | fixed in Batch 1 |
| `exploration_tests` | `harness` | **0** | **fixed in this batch** |
| `store_tests` | `temp_store` (34 sites) | **+36** | untouched |
| `qa_memory_tests` | `temp_store` (23 sites) | **+25** | untouched |
| `qa_memory_retrieval_tests` | `temp_store` (20 sites) | **+21** | untouched |
| `agent_execution_tests` | `setup_harness` (15 sites) | **+15** | untouched |

**Spot-measured only — not a repository total.** Not re-measured in this batch:

| call sites | helper | file |
|---:|---|---|
| 116 | `test_state` | `api_tests.rs` (3 already close) |
| 66 | `test_state_with_compiled_api` | `api_tests.rs` |
| 42 | `setup_healing_harness` | `governed_healing_tests.rs` |
| 20 | `temp_store` | `autopilot_store.rs` |
| 18 | `temp_store` | `trace_tests.rs` |
| 16 | `temp_store` | `change_impact_tests.rs` |
| 9 | `setup` | `retention_and_metrics_tests.rs` |
| 32 | `temp_store` (4 already fixed) | `engine_tests.rs` |

### 13.1 Production API blockers

**None encountered.** No silently deferred helper.

The §5.2 finding generalises usefully: **`exploration_tests` spawned no tasks at all**, so the
"task outlives the fixture" risk did not materialise. For Batch 3, `agent_execution_tests`
(`setup_harness`, 15 sites) is structurally similar to what was just fixed and should be low risk.
`api_tests.rs` remains the highest-volume and highest-complexity case — `test_state` returns an
`AppState` rather than a bare `Store`, so its handle graph is richer than any helper fixed so far.
**Still unverified.**

---

## 14. Verdict

# Phase 9.2B Batch 2: **PASS**

| claim | evidence |
|---|---|
| 19 call sites fixed (not the estimated 18) | counted from source; 19 regions in §11.1 |
| Leak eliminated | **+18 → 0**, 10/10 runs, plus every regression run |
| Awaited, never discarded | `.await` on all 19; `impl Drop` count 0 |
| Mutation actually applied | sha changed, 19 replacements, fresh `Compiling`, binary newer than source |
| Mutation proves the fix is load-bearing | leak returns at exactly **18** — while the suite stays **green** |
| Restoration exact | sha match, byte-identical, 0 residue, backup deleted |
| Production code untouched | 5 SHA comparisons, all unchanged |
| Pre-existing uncommitted work preserved | 3 pattern counts identical; 1 intended refactor shown in full |
| No formatting blow-up | 3 diffs, all inside this batch; containment re-verified after a discarded false pass |
| No artifact deleted | no sweep; counts only increased |
| No regressions | 6 suites green, 169 tests |

**Qualification:** 1 helper of ~39; 19 call sites of ~700. **Phase 9.2 is not complete.**

---

## 15. Recommendation for Batch 3

**Recommended: `agent_execution_tests::setup_harness` (15 sites, measured +15).**

Rationale:

* It is **structurally the closest remaining analogue** to what was just fixed — same file-scoped
  fixture, returns a `Store` plus a `TempDir`, and `agent_execution_tests` is a plain module with
  no shared helper. §5.2 suggests the spawn risk is low here.
* **Already measured** at +15, so before/after numbers are known rather than estimated.
* Only 15 sites — the smallest remaining batch, which keeps the per-batch verification loop
  (10 runs + mutation + restore) cheap while the remaining volume is still large.

Suggested Batch 3 scope, in order:

1. `agent_execution_tests::setup_harness` — 15 sites, +15.
2. `qa_memory_retrieval_tests::temp_store` — 20 sites, +21.
3. `store_tests::temp_store` — 34 sites, +36 (largest single-file win).

Hold `api_tests.rs` (116 + 66 sites) for its **own** batch. It is the largest and the only helper
family whose shape has not been proven; its `AppState` return type means the ownership analysis in
§5.2 must be redone rather than assumed.

### 15.1 Process rules for Batch 3

* **Never run write-mode `rustfmt` on a whole test file.** Format a copy and prove containment — and
  if the containment check itself cannot parse its input, **discard it and use a different method**
  (a line diff against the pre-edit backup). A check that silently verifies nothing is worse than no
  check.
* **Unique backup name per experiment, with the sha written to a sidecar *before* mutating.**
* **Use `-cmatch`, not `-match`, for `test result: FAILED`.**
* **Do not trust a green exit code.** §8.2 shows the suite passed green with the entire fix removed.
* Expect **N−1**, not N, when a suite contains a test that never opens a handle.
* Write lifecycle assertions that check the directory is gone **only after dropping or closing the
  `TempDir`** (§7.2).

---

## 16. Unchanged open items

Carried forward, untouched by this batch:

* **D3** — two `failure_severity` implementations; **not** claimed equivalent; neither consumed by
  Phase 8/9.
* **D5** — the 10 ms duration floor is provisional; empirical inter-write timing still unmeasured.
* **D6** — `set_task_status` enum redesign deferred.
* **Staleness criterion** for readiness test #6 — undefined in the roadmap.
* **Roadmap deliverables still owed:** `PHASE_7_PASS.md`, `PHASE_8_PASS.md`, `PHASE_9_PASS.md`,
  `PHASE_10_QA_COPILOT_REPORT.md`.
* **Phase 9 qualification:** still recommended as `PASS WITH DEFERRED ACCEPTANCE CRITERIA` (6 of 13
  roadmap-required tests unimplemented).
* **Production `Store::close()`** remains **unauthorized** and unimplemented. Measured cost so far:
  **4 helpers, 84 call sites, 0 production changes**.

**Batch 3 was not started. Phase 10 was not started.**