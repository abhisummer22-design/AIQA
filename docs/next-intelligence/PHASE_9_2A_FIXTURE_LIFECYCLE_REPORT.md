# Phase 9.2A — High-Volume Test Fixture Lifecycle Remediation

**Date:** 2026-10-10
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind)
**Verdict:** **PASS (partial remediation — one helper of ~39, 34 of ~700 call sites)**
**Production code changed:** none. `store.rs` and `migrations/` untouched by this phase.

---

## 0. Scope statement — read this first

This phase does **not** close the overall leak. One fixture helper (`integrity_tests::harness`)
was remediated and proven fixed by direct filesystem measurement. The repository still leaks
**917 temporary directories per full-suite run**, measured in §7. That remaining accumulation
comes from ~38 other unmodified helpers.

No statement here should be read as "the artifact leak is resolved." It is resolved **for one
helper**, and that partial result is the evidence that the mechanism is understood.

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch | `main` |
| HEAD | `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified (tracked) | 25 → **25** (unchanged count; see §5 note on `integrity_tests.rs`) |
| Untracked | 25 → **26** (+1: this phase's test file) |
| Deleted | **0** — nothing deleted at any point |
| Staged | 0 |
| Commits / pushes | **none** |

Baseline artifact accumulation **before** any edit, measured in `%TEMP%`:

* `.tmp*` directories: **7,940**
* `C:` free: **56.73 GB**

All historical artifacts were preserved. **No `Remove-Item`, `rmdir`, or cleanup sweep was ever
issued against `%TEMP%`.** The only directory removed anywhere in this phase is
`%TEMP%\aiqa_p92a_artifacts\<test_name>\`, a namespace created by this phase's own tests (§6).

---

## 2. Correction to the Phase 9.1 audit

`PHASE_9_1_CONSUMER_AND_GAPS_AUDIT.md` §14 and §15 were **withdrawn and superseded**. The original
text is preserved verbatim; a clearly-marked **"CORRECTION NOTICE"** section was inserted after §15
containing sections C.1–C.7. Nothing was rewritten to erase the original investigation.

### 2.1 The error that was made

§15 preferred `impl Drop for Store { pool.close() }` (option a) and offered reordering local
bindings (option b) as the alternative. Both assume `Pool::close()` releases file handles by itself.

### 2.2 Why it cannot work

Re-verified against the pinned dependency this session — `sqlx-core-0.9.0`, `src/pool/mod.rs:441`:

```rust
pub fn close(&self) -> impl Future<Output = ()> + '_
```

`close` is **not** an `async fn`. It **returns a `Future` the caller must await.** `Drop` cannot
await, so `impl Drop for Store { self.pool.close(); }` constructs a `Future` and immediately drops
it **unpolled**. It compiles cleanly, emits no warning, and releases nothing — a silent no-op. The
sqlx doc comment claiming a dropped pool releases connections (docs L436–438) does not hold for
SQLite on Windows.

### 2.3 Measurements that disproved it

| variant | directories leaked |
|---|---|
| Current fixture order (`store` bound first, `_dir` dropped first) | **5 / 5** |
| Corrected drop order only (`dir` bound before `store`) | **5 / 5** |
| Corrected drop order **+ awaited** `pool.close().await` | **0 / 5** |

**Drop order alone does not fix it.** Only an awaited close does.

### 2.4 Corrected root cause

The lifetime of **open SQLite pool connections** when the temporary directory is removed — not the
declaration order of two locals. Both halves are necessary: the pool must be **closed** (only an
awaited close releases the file handles) **and** the close must **complete before** the `TempDir`
drops. `tempfile` swallows the Windows removal error, so a missing close is silent — which is why
this survived review and was caught only by measurement.

---

## 3. Selected fixture helpers and measured usage

### 3.1 How the highest-volume helpers were identified

An initial global count was **discarded as invalid**: it conflated identically-named helpers across
different files (`temp_store` exists in 6 files) and counted non-callsite matches. Call sites were
re-counted **per definition, within each helper's own module**, which is how these helpers are
actually reused.

### 3.2 Surveyed helpers that do **not** close their pool

| call sites | helper | file |
|---:|---|---|
| 116 | `test_state` | `api_tests.rs` |
| 66 | `test_state_with_compiled_api` | `api_tests.rs` |
| 42 | `setup_healing_harness` | `governed_healing_tests.rs` |
| **34** | **`temp_store`** | **`store_tests.rs`** |
| **29** | **`harness`** | **`integrity_tests.rs`** |
| 23 | `temp_store` | `qa_memory_tests.rs` |
| 20 | `temp_store` | `autopilot_store.rs` |
| 20 | `temp_store` | `qa_memory_retrieval_tests.rs` |
| 18 | `harness` | `exploration_tests.rs` |
| 18 | `temp_store` | `trace_tests.rs` |
| 16 | `temp_store` | `change_impact_tests.rs` |

(Of `api_tests.rs`'s 116 `test_state` sites, 3 already close their pool.)

### 3.3 The selected helper

| property | `integrity_tests::harness` |
|---|---|
| Source | `services/orchestrator/src/integrity_tests.rs` |
| Call sites | **29** (`let (store, _d, _e) = harness().await;`, plus `store2`/`evidence` variants) |
| Return type | `(Store, tempfile::TempDir, Arc<LocalEvidenceStore>)` — **3-tuple** |
| Database ownership | `Store` holds the only `Pool<Sqlite>` for `phase6-integrity.sqlite3` |
| Other pool handles | `Arc<LocalEvidenceStore>` is **filesystem-only** — it holds no pool and cannot retain the SQLite handle |
| Async teardown supported before this phase | **No** — no close anywhere in the file |
| Can the test safely await a close? | **Yes** — `store` is unused after its last statement in every site |
| Directory owned | the `TempDir` returned as tuple element 2 |
| Also covers | the second most common helper shape (3-tuple), complementary to the dominant 2-tuple `temp_store` |

### 3.4 Why this is the highest-value first step

1. **`integrity_tests` is a required Step 7 regression suite.** The fix is therefore re-verified on
   every future full-suite run, permanently, not just once.
2. It is the **only** high-volume helper confined to a single file, so it is a complete, auditable
   unit.
3. It exercises the **3-tuple** shape, so the Stage A migration can rely on one verified pattern
   covering both tuple shapes.
4. It demonstrates the pattern **test-only**, with no production API change — which is what makes
   Stage A low-risk.

### 3.5 An accidental finding, reported because it was measured

The Phase 8 and Phase 9 suites created earlier in this work also leak and are **also unmodified**:

| suite | `.tmp` delta per run |
|---|---:|
| `phase_8_quality_tests` | **+14** |
| `phase_9_readiness_tests` | **+15** |

These are the 4th- and 5th-cheapest helpers to fix and should be prioritised in Stage A. They were
outside this phase's authorised scope and were not touched.

---

## 4. Exact files changed

| file | change |
|---|---|
| `services/orchestrator/src/integrity_tests.rs` | **+537 / −41** — of which **102 are this phase** (34 sites × 3 lines); the remaining 435 added / 41 removed is an **unintended `rustfmt` reflow** — see §5 |
| `services/orchestrator/src/phase_9_2a_lifecycle_tests.rs` | **new**, 253 lines, 6 tests |
| `services/orchestrator/src/lib.rs` | +4 lines: the `#[cfg(test)] mod phase_9_2a_lifecycle_tests;` declaration (the file already carried earlier phases' declarations) |
| `docs/next-intelligence/PHASE_9_1_CONSUMER_AND_GAPS_AUDIT.md` | Correction Notice C.1–C.7 appended after §15; original §14/§15 preserved |
| `docs/next-intelligence/PHASE_9_2A_FIXTURE_LIFECYCLE_REPORT.md` | this report (new) |

**Explicitly NOT changed:** `store.rs`, `migrations/**`, `api.rs`, `qa_memory*.rs`,
`release_readiness.rs`, `test_quality.rs`, risk formulas, weights, thresholds, duration tolerance.
`store.rs` still contains **0** `pool.close()` calls and no `Drop` impl.

### 4.1 `store.rs` diff is pre-existing, not mine

`git diff` shows `store.rs` with +44 lines. This is **Phase 7.3 documentation work** (the D6
deferral note on `set_task_status`), with mtime `10:03:04` — before this phase began. It was not
touched here.

---

## 5. Disclosed side effect — `rustfmt` reflowed pre-existing uncommitted code

`HEAD`'s `integrity_tests.rs` **is** rustfmt-clean (verified). The *uncommitted earlier-phase*
additions in it were **not**. Running `rustfmt` on that file to keep my inserted teardown lines
conformant therefore reflowed earlier-phase code: **435 lines added, 41 removed**, spread across 66
hunks.

* **Semantically neutral** — this is formatter output, not a logic change.
* **Verified** — 34/34 tests pass; `.tmp` delta 0 across 5 runs.
* **Not recoverable without hand reconstruction** — the pre-phase file was overwritten, so the
  reflow cannot be mechanically reverted. Reverting to `HEAD` was rejected because `HEAD` is *not*
  the pre-phase content (`HEAD` sha `4e80e67c4b70d71c` ≠ pre-phase sha `417650ce6d765b8e`), so
  doing so would have **destroyed earlier-phase work**.

This is reported rather than buried: the file's diff is now 5× larger than the change it is
supposed to contain, and a reviewer should read the 102 teardown lines, not the 537.

**Recommendation for the user:** accept the reflow (it makes the file `cargo fmt`-clean), or
reconstruct the file by hand. I did not attempt the latter — guessing at 435 lines of someone
else's formatting would be riskier than leaving it.

---

## 6. Teardown change and why it works

### 6.1 The change

```rust
async fn harness() -> (Store, tempfile::TempDir, Arc<LocalEvidenceStore>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("phase6-integrity.sqlite3")).await.expect("connect");
    let evidence = Arc::new(LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence root"));
    (store, dir, evidence)
}
```

At the end of each of the 34 test functions that consume this helper:

```rust
        // Phase 9.2A: release the SQLite pool before the
        // TempDir above is dropped, or the directory leaks on Windows.
        store.pool.close().await;
```

`pool.close()` is **awaited**, per `src/pool/mod.rs:441`. The `Future` cannot be discarded, because
the `.await` is enforced by the compiler — this satisfies Step 4.4 (teardown cannot silently
discard the close `Future`) structurally rather than by convention.

### 6.2 Against each Step 4 requirement

| # | requirement | how it is met |
|---|---|---|
| 1 | Use the actual supported SQLx API | `sqlx-core-0.9.0::Pool::close`, re-verified this session |
| 2 | Confirm `pool.close()` is awaited | `.await` on all 34 sites |
| 3 | Identify all pool clones retaining access | Only `Store` holds the pool; `Arc<LocalEvidenceStore>` is filesystem-only. `spawn_maintenance_tasks` uses `Arc::downgrade`, so background tasks do not pin the pool |
| 4 | Teardown cannot silently discard the close `Future` | `.await` is compiler-enforced; there is no `Drop` path |
| 5 | No change to functional behaviour or DB setup | Only statements added at function end, after the last use of `store` |
| 6 | Parallel-test isolation preserved | Each fixture owns a private `tempdir`; no shared path, no global state |
| 7 | Never delete another instance's directories | No directory is enumerated or removed by name; removal is `TempDir`'s own, for its own path only |
| 8 | No global cleanup script | None written |
| 9 | No `Drop` as an async shutdown mechanism | None added; §2.2 explains why it would be a silent no-op |
| 10 | No production `Store::close()`, `store.rs` untouched | Confirmed — `store.rs` unmodified by this phase |
| 11 | No `unsafe`, no blocking the runtime | None introduced; `close()` is genuinely awaited |
| 12 | No weakened assertions, no suppressed errors | Assertions untouched; added code is unconditional |

### 6.3 Reuse of the established pattern

`engine_tests.rs` already contained 4 explicit `store.pool.close().await;` statements. That pattern
was reused verbatim — it is genuinely equivalent, being the same `Store` type, the same `Pool`
field, and the same sqlx API. It is, however, applied to only 4 of that file's 32 sites, confirming
the repository is in a partial state rather than uniformly fixed.

---

## 7. Focused lifecycle-test results

`services/orchestrator/src/phase_9_2a_lifecycle_tests.rs` — 6 tests, mapping to Step 5 items A–G.

| test | item | what it proves |
|---|---|---|
| `p92a_fixture_creates_and_uses_a_real_database` | **A** | A real write is read back; the fixture is a working DB, not just a directory |
| `p92a_migrations_run_on_every_fixture` | **B** | All 5 core tables present in 2 consecutive fixtures |
| `p92a_pool_is_closed_before_the_directory_is_removed` | **C + D** | WAL/SHM sidecars are asserted to **exist while live**, then the directory is asserted **gone** after an awaited close |
| `p92a_repeated_use_does_not_accumulate_directories` | **E** | 10 cycles leave **0** directories |
| `p92a_concurrent_fixtures_do_not_interfere` | **F** | 8 interleaved tasks; each removes exactly its own directory; 0 remain |
| `p92a_teardown_errors_are_not_suppressed` | **G** | Uses fallible `TempDir::close()` and asserts `is_ok()` — a failing removal surfaces |

**Result: `ok. 6 passed; 0 failed`.**

### 7.1 Isolated artifact directory (Step 5 requirement)

Each test measures only inside a root it owns:

```
%TEMP%\aiqa_p92a_artifacts\<test_name>\
```

Fixtures are created with `tempfile::tempdir_in(&root)`. This was **not** a stylistic choice. An
earlier version counted `.tmp*` **globally** and **failed 2 of 6** (full output preserved at
`%TEMP%\p92a_fail_run1.txt`):

```
---- p92a_concurrent_fixtures_do_not_interfere stdout ----
assertion `left == right` failed: eight concurrent fixtures must not delete or leak one another's directories
  left: 8024
 right: 8027
```

The cause was **my test design, not the fix**: `cargo test` runs these tests in parallel, so any
other concurrently running fixture perturbs a global total. A global count is not a valid
assertion here. The isolated root removes the coupling, satisfies the brief's isolation requirement,
and touches nothing outside its own namespace.

### 7.2 Filesystem state verified, not just the exit code

Every test above reads the directory listing directly. Test C captures `dir.path()` and asserts
`!path.exists()` after teardown — a green exit code alone would not detect a silently-swallowed
removal failure, which is precisely the original defect.

---

## 8. Before/after artifact counts

### 8.1 The remediated helper

Measured with **unpatched control suites interleaved**, so scheduling order cannot explain the
result:

| suite | helper | `.tmp` delta per run |
|---|---|---:|
| `integrity_tests` | `harness` (**patched**) | **0** — 5 consecutive runs |
| `qa_memory_retrieval_tests` | `temp_store` (control) | +21 |
| `qa_memory_tests` | `temp_store` (control) | +25 |
| `exploration_tests` | `harness` (control) | +18 |

Interleaved sequence (patched / control / patched): `0`, `+21`, `0`.

**Before** this phase `integrity_tests` would have leaked ~28–29 directories per run. **After: 0.**

### 8.2 Why the number is 28, not 29

Neutering **all 34** patched closes returns the leak at exactly **+28**, not +29. The remaining test,
`a_run_the_layers_agree_about_is_recognised_as_consistent`, performs **0 database operations** — it
never opens a SQLite handle, so it never leaked even before this phase. Confirmed by neutering that
one close alone and measuring delta 0.

So: **28 of 29 harness-using tests genuinely leaked; 1 never did.** The fix's benefit is 28
directories per run, not 29.

### 8.3 Repository totals

| point | `.tmp*` dirs | `C:` free |
|---|---:|---:|
| Phase 9.2A start | 7,940 | 56.73 GB |
| Full-suite regression run | 9,059 | 55.46 GB |
| Now | **9,059** | 55.47 GB |

**One full-suite run still leaks 917 directories (~1.02 GB)** — from the ~38 unmodified helpers.
This is the direct evidence that the overall leak is **not** closed.

### 8.4 No blanket cleanup was performed

No historical directory was deleted. Current counts include every artifact from before this phase.

---

## 9. Mutation testing — proof the tests are not vacuous

Green tests are not evidence. Each mutation was applied, measured, then **restored from a
SHA-verified backup and confirmed byte-identical**.

| # | mutation | result |
|---|---|---|
| 1 | Remove `store.pool.close().await` from lifecycle test C | **CAUGHT** — `5 passed; 1 failed`, and the *only* failure was `p92a_pool_is_closed_before_the_directory_is_removed`, with the intended message. No collateral damage |
| 2 | Replace one `close()` in `integrity_tests` with `.pool.close().UNUSED;` | **INVALID — discarded.** This is a syntax error, so nothing compiled and no tests ran. The resulting delta of 0 was meaningless and was **not** reported as a result |
| 2′ | Retry as a compiling mutation (`store.pool.close().await;` → `let _ = &store;`) | delta **0** — revealed the 0-database-op test of §8.2. A genuine finding, not a pass |
| 3 | Neuter **all 34** closes in `integrity_tests` | leak returns at **+28** — confirms the closes are collectively what fixes it |

Mutation 2 being invalid is recorded because the failed first attempt would otherwise look like
evidence. It was not; it was a broken build.

**Restoration audit:** after mutation 3 the file was restored and verified —
`sha cc1205a7fbb688a6` on both sides, `byte-identical: True`, 34 `close()` statements, **0**
mutation residue (`let _ = &store`, `.UNUSED`, `MUTATION-PROBE`), then re-measured at delta 0 twice.

---

## 10. Regression results

| suite | result | `.tmp` delta |
|---|---|---:|
| `phase_9_2a_lifecycle_tests` | ok. **6 passed; 0 failed** | **0** |
| `integrity_tests` | ok. **34 passed; 0 failed; 1 ignored** | **0** |
| `phase_8_quality_tests` | ok. **15 passed; 0 failed** | +14 *(unmodified — pre-existing)* |
| `phase_9_readiness_tests` | ok. **16 passed; 0 failed** | +15 *(unmodified — pre-existing)* |
| `store_tests` | ok. **36 passed; 0 failed** | +36 *(unmodified)* |
| `qa_memory_tests` | ok. **25 passed; 0 failed** | +25 *(unmodified)* |
| **Full suite** | **ok. 2107 passed; 0 failed; 5 ignored** | +917 |

### 10.1 The full-suite count is exactly consistent

Baseline was **2101 passed / 0 failed / 5 ignored**. This phase adds **6** tests.
2101 + 6 = **2107**. Zero regressions, and no test was removed or weakened to achieve it.

### 10.2 On the earlier unexplained full-suite failure

A prior session recorded one unexplained failure (2069 passed / 1 failed) whose test name was never
captured. This run passed fully. **This does not explain that failure**, and no such claim is made.
The disk-consumption magnitude (~1.0–1.1 GB per full run) is measured; its causal link to that
specific failure is **unproven**. Per the brief, a later passing run is not evidence for the cause
of an earlier one.

---

## 11. Formatting and Clippy

| check | result |
|---|---|
| `cargo fmt --check` | **1 diff: `src/auth.rs:317` only** |
| `rustfmt --check` on both files this phase wrote | **clean** |
| `cargo clippy --all-targets` | **0 errors** |

**`auth.rs:317` is a pre-existing, proven-at-HEAD condition** (rustfmt non-idempotent on a `concat!`
inside `role_gate!`). It was **deliberately not fixed** — it is outside this phase's scope and
changing it would add unrelated noise.

Clippy lints in files this phase touched:

| file | lints |
|---|---:|
| `phase_9_2a_lifecycle_tests.rs` | **0** |
| `integrity_tests.rs` | **0** |
| `lib.rs` | **0** |

One clippy lint that *did* fire in the new test file (`needless_borrow` on
`path.join(&format!(...))`) was fixed rather than suppressed.

---

## 12. Remaining unmodified, still-leaking helpers

All measured except where marked; per-suite deltas are spot-measured, **not** a repository total.

| call sites | helper | file | measured delta |
|---:|---|---|---:|
| 116 | `test_state` | `api_tests.rs` | not measured (3 of 116 already close) |
| 66 | `test_state_with_compiled_api` | `api_tests.rs` | not measured |
| 42 | `setup_healing_harness` | `governed_healing_tests.rs` | not measured |
| 34 | `temp_store` | `store_tests.rs` | **+36** (whole suite) |
| 23 | `temp_store` | `qa_memory_tests.rs` | **+25** |
| 20 | `temp_store` | `autopilot_store.rs` | not measured |
| 20 | `temp_store` | `qa_memory_retrieval_tests.rs` | **+21** |
| 18 | `harness` | `exploration_tests.rs` | **+18** |
| 18 | `temp_store` | `trace_tests.rs` | not measured |
| 16 | `temp_store` | `change_impact_tests.rs` | not measured |
| 15 | `setup_harness` | `agent_execution_tests.rs` | not measured |
| 9 | `setup` | `retention_and_metrics_tests.rs` | not measured |
| 14 | *(Phase 8 tests)* | `phase_8_quality_tests.rs` | **+14** |
| 15 | *(Phase 9 tests)* | `phase_9_readiness_tests.rs` | **+15** |

`engine_tests.rs` remains at 4 of 32 sites fixed.

### 12.1 Production API blockers

**None encountered in this phase.** The fix was achievable test-only, so no helper had to be
stopped and no silent switch to Option B occurred.

For Stage A, the one genuine blocker to record: a helper whose tests **never release `store` by
value** — for example tests that move `store` into a spawned task that outlives the test body — cannot
close the pool before `TempDir` drops without either restructuring the test or a production
`Store::close()`. No such helper was found in `integrity_tests`. Whether any of the ~38 remaining
helpers has this shape is **unverified**.

---

## 13. Verdict

# Phase 9.2A: **PASS** (partial)

| claim | evidence |
|---|---|
| Phase 9.1 §14/§15 corrected, original preserved | Correction Notice C.1–C.7 appended, §14/§15 untouched |
| Root cause correctly identified | §2.4; 5/5 → 5/5 → 0/5 |
| Selected helper fully remediated | `integrity_tests::harness`, 34 awaited closes |
| Leak eliminated for the selected helper | delta **0** across 5 runs, interleaved with controls |
| Tests are not vacuous | 2 valid mutations, both caught; 1 invalid mutation discarded, not reported as a pass |
| No production change | `store.rs`/`migrations` untouched; 0 `pool.close()` in `store.rs` |
| No historical artifact deleted | no sweep issued; counts only ever increased |
| No regressions | 2107 = 2101 + 6 |

**The one qualification:** this phase modified **34 of ~700 call sites (1 helper of ~39)**. It is
**not** a fix for the repository-wide leak. One full suite still leaves **917** new directories.

---

## 14. Recommended next action — Stage A

Continue the staged, test-only migration. **Recommended order, cheapest-value first:**

1. **`phase_8_quality_tests.rs` (+14) and `phase_9_readiness_tests.rs` (+15)** — smallest total
   effort (29 sites), both already mine, both single-file, and both verified by required regression
   suites. Fixing them also stops this project's *own* recent work from leaking.
2. **`exploration_tests::harness` (+18)** — the other `harness` helper; completes the pattern across
   both tuple shapes.
3. **`store_tests::temp_store` (+36)** and **`qa_memory_tests::temp_store` (+25)** — largest
   measured single-file wins.
4. **Then the `api_tests.rs` pair** (`test_state` 116, `test_state_with_compiled_api` 66). Largest
   volume but the most call sites and the most `state` handles to audit for stray pool clones.

### 14.1 Preconditions for Stage A

* **Record a `.tmp` baseline before and after each helper**, interleaved with an unpatched control —
  a green suite is not evidence.
* **Mutation-check each batch**: neuter the added closes and confirm the delta returns. Batch-level
  deltas, not just "tests pass".
* **Watch for the blocker in §12.1**: a helper whose tests move `store` into an outliving task.
  If found, **stop that helper and report it**; do not silently add a production `Store::close()`.
* **Do not `rustfmt` whole test files.** §5 shows the cost: 435 lines of unrelated reflow. Format
  only newly written code.
* **Re-run the full suite sparingly** — each run costs ~1.0 GB. One confirming run per stage, not one
  per helper.

### 14.2 The production-API alternative remains open and undecided

If Stage A's per-call-site cost proves unacceptable across ~700 sites, a production
`Store::close()` becomes the better trade. It was **not authorised** in this phase (D-2) and was not
implemented. That decision should be revisited with Stage A's measured effort data in hand — this
report supplies the first real cost figure: **34 sites ≈ 3 of 39 helpers done, 0 production changes.**

---

## 15. Unchanged open items

Carried forward, still unresolved and untouched here:

* **D3** — two `failure_severity` implementations; **not** claimed equivalent; neither consumed by
  Phase 8/9.
* **D5** — the 10 ms duration floor is provisional; empirical inter-write timing still unmeasured.
* **D6** — `set_task_status` enum redesign deferred.
* **Staleness criterion** for readiness test #6 — undefined in the roadmap.
* **Roadmap deliverables still owed:** `PHASE_7_PASS.md`, `PHASE_8_PASS.md`, `PHASE_9_PASS.md`,
  `PHASE_10_QA_COPILOT_REPORT.md`.
* **Phase 9 qualification:** recommended as `PASS WITH DEFERRED ACCEPTANCE CRITERIA` (6 of 13
  roadmap-required tests unimplemented). Unchanged by this phase.

**Phase 10 was not started.**