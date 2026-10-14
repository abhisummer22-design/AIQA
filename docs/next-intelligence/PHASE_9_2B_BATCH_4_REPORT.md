# Phase 9.2B — Stage A, Batch 4

**Fix `governed_healing_tests::setup_healing_harness` SQLite cleanup**

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — unchanged, nothing committed
**Verdict:** **PASS**
**Scope delivered:** 1 fixture helper; **43 call sites**; 7 new lifecycle tests; 1 pre-existing test race fixed
**Production code changed:** none

---

## 0. Scope statement, and three outcomes kept separate

This report deliberately never conflates three different things:

| outcome | how it is evidenced |
|---|---|
| **1. Successful fixture cleanup** | `.tmp` delta per run, plus direct filesystem assertions |
| **2. Successful background-work completion** | `wait_until_run_settles` + assertions on the detached run's own persisted rows, read **before** the pool closes |
| **3. Successful functional assertions** | the 53 pre-existing tests, unchanged, still passing |

A batch can score 1 and still fail 2 or 3. Batch 4 is the first batch where that distinction was
load-bearing: **a pre-existing race surfaced during this batch and was fixed rather than absorbed.**

**Phase 9.2 remains open.** One helper was fixed. Seven further helpers still leak, freshly measured
at **+140 directories** (§14).

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 26 / 30 / **0** / 0 |

### 1.1 IMPORTANT — `%TEMP%` was wiped externally between sessions

At the start of this batch:

| | end of Batch 3 | start of Batch 4 |
|---|---:|---:|
| `.tmp*` directories in `%TEMP%` | **9,542** | **0** |
| `C:` free | 45.81 GB | **57.07 GB** |

`%TEMP%` now contains only `opencode`, `node-compile-cache`, two fresh `scoped_dir*` (sqlx, created
today 09:34) and 14 loose files. **All 9,542 accumulated artifact directories are gone.**

**I did not delete them.** No `Remove-Item`, `rmdir`, or sweep was issued at any point in this batch
or in Batch 3. The deletion happened between sessions, outside this agent's control. I am reporting
it rather than quietly re-baselining, because every "historical artifacts preserved" claim in the
Batch 1–3 reports is now moot: that history no longer exists on disk.

**Consequence for this batch:** all before/after artifact numbers below are measured on a freshly
emptied `%TEMP%`, which actually makes them cleaner than earlier batches. The historical *rates*
(per-suite deltas) remain valid, because a delta is independent of accumulated history.

### 1.2 Out-of-scope files pinned by SHA before any edit

| file | SHA-256 (first 16) | status at end |
|---|---|---|
| `src/store.rs` | `97E79B8F876D7219` | **UNCHANGED** |
| `src/integrity.rs` | `EAEA60CFA367BCB6` | **UNCHANGED** |
| `src/integrity_tests.rs` | `CC1205A7FBB688A6` | **UNCHANGED** |
| `src/phase_8_quality_tests.rs` | `B5D3CA6E616187F1` | **UNCHANGED** |
| `src/phase_9_readiness_tests.rs` | `99D402A29E57EDCF` | **UNCHANGED** |
| `src/exploration_tests.rs` | `8A8980B655B0C2D5` | **UNCHANGED** |
| `src/agent_execution_tests.rs` | `6CAF99485C2CCE4B` | **UNCHANGED** |

The target file was **clean at HEAD** (empty `git diff --numstat`), so `git diff` is authoritative
for this batch.

---

## 2. Verified helper and call-site count

| | value |
|---|---:|
| `setup_healing_harness()` call sites | **43** |
| tests in file | 53 |
| tests using the fixture | **43** (1:1 — no test builds two) |
| binding `(store, ...)` | 9 |
| binding `(_store, ...)` | 32 |
| `service` / `_service` (tuple element 4) | 41 / 2 |
| `pool.close()` before this batch | **0** |
| `impl Drop` in file | **0** |

The historical count of **42 was one short**; the verified number is **43**.

---

## 3. Reconciliation: the previous +157 versus +183

**The discrepancy was an arithmetic error in the Batch 3 report. There is no overlap and no
different counting scope.**

The Batch 3 report listed eight remaining per-suite deltas:

| suite | delta reported in Batch 3 |
|---|---:|
| `governed_healing_tests` | +43 |
| `engine_tests` | +31 |
| `qa_memory_tests` | +25 |
| `autopilot_store` | +21 |
| `qa_memory_retrieval_tests` | +21 |
| `trace_tests` | +18 |
| `change_impact_tests` | +14 |
| `retention_and_metrics_tests` | +10 |
| **correct sum** | **183** |
| total stated in Batch 3 (report lines 16 and 479) | **157** |

I mis-summed, and then derived a "27% of remaining leakage" figure from the wrong total (43/183 =
23.5%, not 27%). That derived percentage was also wrong.

### 3.1 A real scoping caveat, measured separately

There *is* a genuine scope subtlety, and it is worth recording because it affects future totals: these
figures are **per-suite whole-suite deltas**, not per-helper rates. Several exceed the named
helper's site count — `autopilot_store` +21 against 20 `temp_store` sites, `qa_memory_retrieval_tests`
+21 against 20, `engine_tests` +31 against 32 sites of which 4 already close. So a suite's delta can
include leakage from helpers other than the one named.

This is a **scope caveat, not the explanation** for 157: no amount of overlap makes the arithmetic
come out 26 lower. I have re-measured all seven untouched suites this batch on the emptied `%TEMP%`
and they reproduce exactly (§14): **31, 25, 21, 21, 18, 14, 10 = 140**, plus `governed_healing`'s
43, giving **183**.

---

## 4. Background-task ownership and completion analysis

This is the part that differs most from every previous batch.

### 4.1 The fixture returns a 5-tuple and hands the pool to two services

```rust
async fn setup_healing_harness() -> (
    Store, Arc<ExecutorRegistry>, Arc<LocalEvidenceStore>, GovernedHealingService, tempfile::TempDir,
)
```

Inside, `store.clone()` is given to **both** the dispatcher and the service, and the original is
also returned — so three live handles to one pool exist from the moment the fixture is built.

| object | holds the pool? | evidence |
|---|---|---|
| `Store` | **yes** | `pub struct Store { pub pool: SqlitePool }` |
| `AgentExecutionDispatcher` | **yes**, via `pub store: Store` | given `store.clone()` |
| `GovernedHealingService` | **yes**, via `pub store: Store` | given `store.clone()` |
| `Arc<ExecutorRegistry>` | **no** | `by_kind`/`default` maps of `Arc<dyn Executor>`; no executor holds a pool |
| `Arc<LocalEvidenceStore>` | **no** | `pub struct LocalEvidenceStore { root: PathBuf }` |

Both `service.store` and `service.dispatcher.store` are reachable, so the teardown is expressible
without touching any production code.

### 4.2 The same detached-run hazard as Batch 3 — reached through a second path

`governed_healing_tests.rs` has **zero** spawns, and `governed_healing.rs` has **zero** spawns. But
the service reaches the engine, which does:

```rust
// agent_execution.rs:403, via GovernedHealingService::execute_approved
engine::spawn_run(..., self.store.clone(), ...);

// engine.rs:756 — fire-and-forget: tokio::spawn, no JoinHandle returned
pub fn spawn_run(record: Arc<RunRecord>, registry: Arc<ExecutorRegistry>, concurrency: usize,
                store: Store, ...) {
    tokio::spawn(async move { mark_run_running(&record, &store).await; ... });
}
```

### 4.3 Which tests actually start a run

| category | count | tests |
|---|---:|---|
| use the fixture but start **no** run | **34** | `test_02`…`test_21`, `test_23`, `test_25`…`test_27`, `test_31`, `test_32`, `test_37`…`test_39`, `test_42`…`test_47`, the `a_*` proposal tests, `a_proposal_that_changes_nothing…` |
| start a run but `execute_approved` returns `Err` — **no run created** | **2** | `test_22` (rejected), `test_24` (cancelled) |
| start a run, **already wait via `wait_until_run_settles`** | **1** | `an_approved_proposal_is_executed_on_the_executor_it_was_raised_for` |
| start a run, wait only via a **raw 50 ms sleep** | **4** | `test_28`, `test_29`, `test_30`, `test_33` |
| start a run, **no coordination at all** | **2** | `a_production_proposal_…held_for_approval`, `a_proposal_that_changes_nothing…` (the latter also returns `Err`, so no run) |

### 4.4 The answers the brief asks for

| question | answer |
|---|---|
| **1. What background task is started?** | `engine::spawn_run`'s detached `tokio::spawn`, holding a `Store` clone and writing run/task rows and telemetry |
| **2. What observable signal indicates completion?** | `get_run_summary(run_id).state` leaving `Created`/`Scheduled`/`Running`. **This helper already exists in the file** as `wait_until_run_settles(&Store, Uuid)` |
| **3. Does the test assert on writes by that task?** | **Yes, in 4 tests.** `test_28`/`29`/`30` read run state through `verify_healing`; `test_33` asserts calibration counters. These are the ones that would be corrupted by an early close |
| **4. Can the task outlive the fixture?** | **Yes.** Fire-and-forget, no handle |
| **5. Could closing the pool cause a later write to fail?** | **Yes** — a closed `Pool` will not open new connections |
| **6. How can the test coordinate without touching production code?** | Call the file's own `wait_until_run_settles` before the assertions that read the run's writes, and before the close |

### 4.5 A pre-existing race this analysis uncovered

`test_28`, `test_29`, `test_30` and `test_33` all shared this shape:

```rust
let executed = service.execute_approved(&ctx, approved.id).await.expect("executed");
tokio::time::sleep(Duration::from_millis(50)).await;   // <-- race window
// test_29/30 then write to `tasks` themselves
let verification = service.verify_healing(&ctx, p.id).await...;
```

In `test_29` the test issues `UPDATE tasks SET status = 'FAILED'` and then expects `NotVerified`. If
the detached run has not yet written the revised task's `PASSED` status, the run's write lands
**after** the test's and overwrites it — the test then observes `Verified` and fails. The 50 ms sleep
is an arbitrary guess, not a completion signal.

This is a **pre-existing defect**, not something Batch 4 introduced. It is documented and fixed in §6.

---

## 5. Exact fixture changes and why they are safe

### 5.1 Change 1 — awaited close at all 43 sites

```rust
    service.store.pool.close().await;     // 41 sites
    _service.store.pool.close().await;    //  2 sites (the test binds the service as `_service`)
```

as the last statement of each test body. The two variants differ only in which binding still names
the service; `_service` was left underscore-prefixed rather than renamed, so no existing line is
touched for the sake of style.

**Safety for the 34 no-run tests and the 2 `Err` tests:** no detached task exists, so nothing can be
using the pool.

**Safety for the run-starting tests:** the pool is shared, so `close()` releases the file handles for
every clone. Verified empirically in §7 and §9, not assumed.

### 5.2 Change 2 — settle the run before its writes are asserted

In `test_28`, `test_29`, `test_30`, `test_33`, immediately after the `healing_run_id` binding and
**before** the sleep and any DB manipulation:

```rust
    wait_until_run_settles(&service.store, healing_run_id).await;
```

This is the file's **existing** completion signal, not a new mechanism and not a sleep. It replaces
reliance on an arbitrary delay with a real state check, and it removes the race in §4.5 rather than
papering over it. The pre-existing 50 ms sleep is left in place — it is now redundant but harmless,
and removing it would be an unrelated change.

### 5.3 Change 3 — one capture, so a discarded value can coordinate teardown

`test_33` previously discarded the execution result (`let _ = service.execute_approved(…)`). It now
captures it:

```rust
    let executed = service.execute_approved(&ctx, approved.id).await.expect("executed");
    let healing_run_id = executed.healing_run_id.expect("healing run id");
```

**This is the only line removed from the file.** No assertion changed; a value that was being thrown
away is now used.

### 5.4 Deliberately *not* changed

* `setup_healing_harness()` itself — untouched, still uses `tempfile::tempdir()`, so all 43 existing
  call sites keep their exact directory location. No behavioural equivalence had to be argued.
* No production code. `spawn_run`'s fire-and-forget design was **not** modified — it is a real design
  observation, recorded in §16, not a defect fixed under this batch's authority.

---

## 6. Files changed

| file | change |
|---|---|
| `services/orchestrator/src/governed_healing_tests.rs` | **+486 / −1** |
| `docs/next-intelligence/PHASE_9_2B_BATCH_4_REPORT.md` | this report (new) |

Diff composition against HEAD (authoritative — the file was clean at HEAD):

| content | count |
|---|---:|
| `service.store.pool.close().await;` | 46 (41 existing tests + 5 lifecycle) |
| `_service.store.pool.close().await;` | 2 |
| `wait_until_run_settles(&service.store, healing_run_id).await;` | 5 (4 existing + 1 lifecycle) |
| `setup_healing_harness_in()` helper | 1 |
| `#[tokio::test]` (new lifecycle tests) | 7 |
| **lines removed** | **1** — `let _ = service` → `let executed = service` |

No existing assertion was removed, weakened or skipped. The original **53** tests all still exist and
pass; the file went 53 → 60 tests.

---

## 7. All 10 repeated-run results

### 7.1 Final validation runs (on the shipped file state)

| run | exit | passed / failed / ignored | new `.tmp` | panics |
|---:|---:|---|---:|---:|
| 1 | 0 | 60 / 0 / 0 | **0** | 0 |
| 2 | 0 | 60 / 0 / 0 | **0** | 0 |
| 3 | 0 | 60 / 0 / 0 | **0** | 0 |
| 4 | 0 | 60 / 0 / 0 | **0** | 0 |
| 5 | 0 | 60 / 0 / 0 | **0** | 0 |
| 6 | 0 | 60 / 0 / 0 | **0** | 0 |
| 7 | 0 | 60 / 0 / 0 | **0** | 0 |
| 8 | 0 | 60 / 0 / 0 | **0** | 0 |
| 9 | 0 | 60 / 0 / 0 | **0** | 0 |
| 10 | 0 | 60 / 0 / 0 | **0** | 0 |

**10/10 exit 0, 10/10 zero artifacts, zero panics, zero teardown errors, zero retained failure
artifacts.** Baseline was **+43**.

### 7.2 The flake hunt that ran before those 10

| configuration | runs | failures |
|---|---:|---|
| pre-edit file (baseline) | 15 | **0** |
| fixed file, settle-waits appended at test end | 12 | **1** (`test_29`) |
| fixed file, settle-waits moved before the sleep (shipped) | **25** | **0** |
| shipped file, final validation | 10 | **0** |

---

## 8. Per-run artifact counts

```
run  1: 0     run  6: 0
run  2: 0     run  7: 0
run  3: 0     run  8: 0
run  4: 0     run  9: 0
run  5: 0     run 10: 0
```

Every test in the suite uses this one fixture, so the suite-level delta is attributable to it without
ambiguity. The lifecycle tests additionally assert against private roots under
`%TEMP%\aiqa_p92b4_artifacts\<test_name>\`, so a green global count could not conceal a failure.

Repository totals: `%TEMP%` **0 → 873**, `C:` **57.07 → 55.89 GB** over the batch. The 873 are from
the seven untouched control suites run for §14; `governed_healing_tests` contributed **0**.

---

## 9. Evidence that required database writes were preserved

This is outcome (2), kept separate from cleanup (1).

### 9.1 Two of my own tests failed before this could be established honestly

Both failures are recorded because both changed what the test actually proves.

**(a) Wrong premise.** My central test first asserted that `get_feedback_for_planning` was non-empty
after settling. It failed — the calibration metric is written by `verify_healing`, not by the
detached run, so it was the wrong evidence. Full output: `%TEMP%\b4_fail1.txt`. Rewritten to assert
on the run's own rows.

**(b) Wrong column type.** `SELECT status FROM tasks WHERE run_id = ?` returned nothing because
`tasks.run_id` is **TEXT**, not a UUID column — confirmed by reading
`migrations/0001_initial.sql`, not guessed. Full output: `%TEMP%\b4_fail2.txt`. Fixed to
`.bind(healing_run_id.to_string())`.

Neither failure was a product defect. Both were mine.

### 9.2 The central test: `p92b4_settled_run_writes_persist_before_the_pool_closes`

It runs propose → approve → execute, then:

1. `wait_until_run_settles(&service.store, healing_run_id).await` — **D**
2. asserts `get_run_summary` has left `Created`/`Scheduled`/`Running` — **D**
3. queries `SELECT status FROM tasks WHERE run_id = ?` and asserts the set is **non-empty** and every
   status is terminal — **E**. Those rows are created and updated by the detached run, not by the test
4. calls `verify_healing` and asserts `outcome == Verified` and `revised_task_passed` — **J**. This
   reads back the outcome the detached run persisted
5. **only then** calls `service.store.pool.close().await`

**The ordering is the whole point.** Steps 2–4 read the database while the pool is still open. A
closed pool therefore cannot make a failed background write look like a passing test — if the run's
writes had not landed, step 3 or 4 would fail.

### 9.3 Mutation 2 proves this, not just asserts it

Removing only that settle-wait makes the test **fail**:

```
---- governed_healing_tests::p92b4_settled_run_writes_persist_before_the_pool_closes ----
the run must have settled before teardown, still Scheduled
```

while the cleanup delta stays **0** — correctly isolating *coordination* from *cleanup*.

---

## 10. Mutation results and proof each mutation applied

### 10.1 A no-op mutation that I discarded

An early mutation attempt appeared to give `delta 0` with the closes removed. It was **never
applied** — the PowerShell heredoc that would have written the mutating script failed to parse, so the
file was untouched. I verified this directly (closes still present: 50) before trusting anything.
**That delta was meaningless and is not used as evidence.**

### 10.2 Mutation 1 — delete the 43 awaited closes

| proof | value |
|---|---|
| pre-mutation SHA-256 | `08f014852495130f69e29496b5e3ee5e63f092c85f88d4b9c16ae2da02a8de50` |
| mutated SHA-256 | `c90a95cf74ec0bda52ba701c623c8255bfbafd0cd7f97c5f7a0c57541f55f096` |
| sha changed | **True** |
| closes deleted | **43** (verified present in the mutation output) |
| lifecycle-test closes preserved | 7 |
| compile errors | **0** |
| `Compiling aiqa-orchestrator` emitted | **yes** |
| binary newer than mutated source | **True** |

**Result: `.tmp` delta = +43** — the leak returned, matching the historical measurement exactly — and
**the suite still passed 60/60 green.** A passing exit code is not evidence of cleanup.

### 10.3 Mutation 2 — remove the settle-wait from the central test

| proof | value |
|---|---|
| pre-mutation SHA-256 | `08f014852495130f69e29496b5e3ee5e63f092c85f88d4b9c16ae2da02a8de50` |
| mutated SHA-256 | `afeb27634ccf614799edc74f6998d2e2018a0082434c7981ed7255cb7ebd84b6` |
| sha changed | **True** |
| settle-waits removed | 1 (the 4 in existing tests untouched) |
| compile errors | **0** |
| binary newer than mutated source | **True** |

**Result:** `FAILED. 59 passed; 1 failed` — the central lifecycle test, "still Scheduled". Cleanup
delta **0**.

Both mutations were re-run against the **final shipped file state**, not an intermediate one.

### 10.4 Restoration

After each mutation: restore from that mutation's uniquely named backup, verify **sha match**,
**byte-identical**, **0 residue**, then delete the backup and its `.sha` sidecar. Final state
SHA `08f014852495130f69e29496b5e3ee5e63f092c85f88d4b9c16ae2da02a8de50`. No mutated source or binary
remains.

---

## 11. The pre-existing race: found, measured, fixed

During a run immediately after the formatting pass, one test failed. **I did not capture its name in
that run** — my command printed only the summary line. That is a process failure on my part, and the
run's evidence was lost. I then hunted it properly rather than accepting the subsequent green run:

* 12 more runs → reproduced 1 failure: `test_29_failed_healing_verification`
* preserved output (`%TEMP%\b4_flaky_12.txt`):
  `assertion left == right failed  left: Verified  right: NotVerified`
* ran the **pre-edit** file 15 times → **0 failures**
* ran the fixed file 25 times after relocating the settle-wait → **0 failures**

**Attribution, stated honestly:** I observed 1 failure in 12 runs on the fixed file and 0 in 15 on
the pre-edit file. Those sample sizes do **not** establish causation either way, and I am not
claiming my change caused it. What I can state is the mechanism (§4.5): a fixed 50 ms sleep guards a
read of a detached task's writes, and any added background work makes that window more likely to
miss.

**The fix** is the one the mechanism demands — replace reliance on the sleep with the file's own
completion signal, before the reads — not a retry, sleep increase, or skip. Result: **0 failures in 25
runs.** No assertion was weakened, no test skipped, no timeout lengthened.

**One residual coupling, stated plainly:** `a_production_proposal_…held_for_approval` and
`an_approved_proposal_…` still start detached runs and read state without waiting first. That is
**pre-existing** and unchanged by this batch. Both are safe *for teardown* because the close happens
after all their assertions; their coordination is weak, and is recorded in §16 rather than silently
altered.

---

## 12. Formatting and Clippy

| check | result |
|---|---|
| `cargo fmt --check` | **1 diff: `src/auth.rs:317` only** |
| `cargo clippy --all-targets` | **0 errors** |
| clippy lints in `governed_healing_tests.rs` | **0** |

No write-mode `rustfmt` was run against the file. A copy was formatted; the changed ranges were
`1712`, `1945`, `1958` — all inside this batch's additions — and the decisive check was a multiset
comparison against the pre-edit backup.

That check reported **one pre-edit line "lost"**: `let _ = service`, 2 occurrences → 1. That is
**exactly the intended change in §5.3**, not formatting damage. It is called out here rather than
being quietly accepted, because a multiset check that cannot distinguish "lost" from "changed on
purpose" is only half a check.

**`auth.rs:317` is pre-existing** (rustfmt non-idempotent on a `concat!` inside `role_gate!`, proven
at HEAD in an earlier phase) and was deliberately left alone.

---

## 13. Production code untouched, artifacts preserved

| check | result |
|---|---|
| all 7 out-of-scope files | SHA **unchanged** (§1.2) |
| only `governed_healing_tests.rs` written | **yes** |
| migrations or routes added | **none** |
| `impl Drop` introduced as async shutdown | **none** |
| runtime blocking | **none** |
| global cleanup script | **none** |
| commits / pushes | **none**; HEAD still `7930507` |
| Repo state | 26 → **27 modified** / 30 → 31 untracked (+1 report) / **0 deleted** / 0 staged |

### 13.1 Historical artifacts

I deleted nothing in this batch. But see §1.1: the ~9,542 historical artifact directories that earlier
reports preserved were **already gone before this batch started**, by an external action. The only
paths removed by these tests are `%TEMP%\aiqa_p92b4_artifacts\<test_name>\`, namespaces they create
themselves.

---

## 14. Remaining leaking fixture helpers

All re-measured on the emptied `%TEMP%`.

| suite | helper | sites | delta | status |
|---|---|---:|---:|---|
| `integrity_tests` | `harness` | 29 | **0** | fixed 9.2A |
| `phase_8_quality_tests` | `Db::new` | 15 | **0** | fixed Batch 1 |
| `phase_9_readiness_tests` | `Db::new` | 16 | **0** | fixed Batch 1 |
| `exploration_tests` | `harness` | 19 | **0** | fixed Batch 2 |
| `agent_execution_tests` | `setup_harness` | 15 | **0** | fixed Batch 3 |
| `governed_healing_tests` | `setup_healing_harness` | 43 | **0** | **fixed this batch** |
| `engine_tests` | `temp_store` (4 of 32 already close) | 32 | **+31** | untouched |
| `qa_memory_tests` | `temp_store` | 23 | **+25** | untouched |
| `autopilot_store` | `temp_store` | 20 | **+21** | untouched |
| `qa_memory_retrieval_tests` | `temp_store` | 20 | **+21** | untouched |
| `trace_tests` | `temp_store` | 18 | **+18** | untouched |
| `change_impact_tests` | `temp_store` | 16 | **+14** | untouched |
| `retention_and_metrics_tests` | `setup` | 9 | **+10** | untouched |
| `api_tests` | `test_state` / `test_state_with_compiled_api` | 116 / 66 | not measured | untouched |

**Remaining measured total: +140** (these seven suites reproduce their Batch 3 numbers exactly).

### 14.1 Production API blockers

**None encountered.** No helper was silently deferred.

`api_tests` remains the only unproven family: `test_state` returns an `AppState`, so §4's analysis
would have to be redone from scratch.

---

## 15. Verdict

# Phase 9.2B Batch 4: **PASS**

| claim | evidence |
|---|---|
| 43 call sites fixed (history said 42) | counted from source; `git diff` = 486/1 |
| **1. Cleanup** — leak eliminated | +43 → 0; 10/10 runs; Mutation 1 returns +43 while the suite stays green |
| **2. Background completion** — run settles before teardown | `wait_until_run_settles`; Mutation 2 fails without it |
| **3. Functional assertions** — unchanged | 53 original tests, all pass; 1 line changed, no assertion weakened |
| Detached-run hazard handled | §4: fire-and-forget `spawn_run` reached via two services; writes settled before read |
| Pre-existing race found and fixed | §11; 0 failures in 25 runs afterwards |
| Coordination is load-bearing, not decorative | Mutation 2 |
| No-op mutation discarded | §10.1 |
| Production code untouched | 7 SHA comparisons, all unchanged |
| No formatting blow-up | 3 diffs, all in new code; multiset check isolates one intended change |
| No regressions | 8 suites green, 236 tests |
| Inventory reconciled | §3: the +157 was an arithmetic error; correct total is 183, remaining 140 |

**Qualification:** 1 helper of ~39; 43 call sites of ~700. **Phase 9.2 remains open**, and no
full-suite artifact verification has been performed.

---

## 16. Recommended next step

**Recommended: `engine_tests::temp_store` — 32 sites, measured +31.**

Rationale:

* Largest remaining single-file leak (+31), 22% of the remaining measured leakage.
* Same file-scoped `temp_store` shape as `store_tests`/`qa_memory_tests`, so the pattern transfers
  directly — the ownership question reduces to "does anything hold a `Store` past the test body".
* 28 of its 32 sites lack a close, so the 4 already-fixed sites give a working in-file precedent.
* Its 33 tests run in well under a second, so the 10-run + mutation + restore loop stays cheap.

Suggested Batch 5 scope, in order:

1. `engine_tests::temp_store` — 32 sites, +31.
2. `qa_memory_tests::temp_store` — 23 sites, +25.
3. `autopilot_store::temp_store` — 20 sites, +21.

Hold `api_tests.rs` (116 + 66) for its **own** batch, as in every prior recommendation.

### 16.1 Two design observations recorded, not acted on

Neither is in scope for a test-fixture batch; both are logged so they are not lost:

1. **`engine::spawn_run` is fire-and-forget.** It returns no `JoinHandle`, so no caller *can*
   coordinate with it. Every consumer must poll state instead. That is why `wait_until_run_settles`
   exists as a convention rather than an API. Changing it would be a **production** decision.
2. **`a_production_proposal_…held_for_approval` reads run state without waiting.** Pre-existing, and
   safe for teardown, but it relies on the run reaching `PendingApproval` quickly. Not touched here.

### 16.2 Process rules for Batch 5

* **Capture full output on every run, not just the summary line.** One failure in this batch was lost
  because my command printed only a result line (§11).
* **Verify a mutation actually applied before reading a delta.** A heredoc that failed to write left
  the source untouched and produced a meaningless `delta 0` (§10.1).
* **A multiset check must distinguish "line lost" from "line changed on purpose"** (§12).
* **Do not trust a green exit code.** Mutation 1: 60/60 green, +43 directories leaked.
* **Where a fixture starts detached work, map the ownership graph through the *production* code it
  calls** — `governed_healing_tests.rs` spawns nothing, but it reaches `spawn_run` through a service.
* **Prefer an existing completion signal over a sleep**, and if a fixed sleep is found guarding a
  read of background writes, that is a latent race worth fixing while you are in the file (§11).

---

## 17. Unchanged open items

* **D3** — two `failure_severity` implementations; **not** claimed equivalent.
* **D5** — the 10 ms duration floor is provisional; empirical inter-write timing unmeasured.
* **D6** — `set_task_status` enum redesign deferred.
* **Staleness criterion** for readiness test #6 — undefined in the roadmap.
* **Roadmap deliverables still owed:** `PHASE_7_PASS.md`, `PHASE_8_PASS.md`, `PHASE_9_PASS.md`,
  `PHASE_10_QA_COPILOT_REPORT.md`.
* **Phase 9 qualification:** still recommended as `PASS WITH DEFERRED ACCEPTANCE CRITERIA`.
* **Production `Store::close()`** remains **unauthorized** and unimplemented. Measured cost so far:
  **6 helpers, 142 call sites, 0 production changes**.
* **The Batch 3 report's `+157` total and its derived "27%" are wrong** (§3). The Batch 3 report file
  was **not** edited, to preserve the audit trail; this report carries the correction.

**Batch 5 was not started. Phase 10 was not started.**